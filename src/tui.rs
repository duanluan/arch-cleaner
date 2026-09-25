use std::collections::HashSet;
use std::io::{self, Write};

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::queue;
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::cli::{parse_u8, parse_u16, print_plan, print_results, print_scan_reports};
use crate::executor::{ExecutionMode, ExecutionOptions, execute_targets};
use crate::i18n::{self, Language};
use crate::model::{CleanerOptions, CleanupTarget};
use crate::rules::{all_targets, is_valid_journal_size, scan_all};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Page {
    Targets,
    Settings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SettingField {
    KeepPackages,
    JournalDays,
    JournalSize,
    TempDays,
    UserCacheDays,
    AiAgentDays,
}

impl SettingField {
    const ALL: [SettingField; 6] = [
        Self::KeepPackages,
        Self::JournalDays,
        Self::JournalSize,
        Self::TempDays,
        Self::UserCacheDays,
        Self::AiAgentDays,
    ];

    fn label(self, language: Language) -> &'static str {
        match self {
            Self::KeepPackages => i18n::tr(language, "pacman 包保留数", "pacman keep count"),
            Self::JournalDays => i18n::tr(language, "journal 天数", "journal days"),
            Self::JournalSize => i18n::tr(language, "journal 大小", "journal size"),
            Self::TempDays => i18n::tr(language, "临时文件天数", "temp file days"),
            Self::UserCacheDays => i18n::tr(language, "用户缓存天数", "user cache days"),
            Self::AiAgentDays => i18n::tr(language, "AI agent 天数", "AI agent days"),
        }
    }

    fn description(self, language: Language) -> &'static str {
        match self {
            Self::KeepPackages => i18n::tr(
                language,
                "用于 pacman 包缓存清理，值越大，保留的回滚包版本越多。",
                "Used by pacman cache cleanup. Higher values keep more rollback versions.",
            ),
            Self::JournalDays => i18n::tr(
                language,
                "用于 journalctl --vacuum-time 的天数限制。",
                "Used by journalctl --vacuum-time.",
            ),
            Self::JournalSize => i18n::tr(
                language,
                "用于 journalctl --vacuum-size，例如 1G。",
                "Used by journalctl --vacuum-size, for example 1G.",
            ),
            Self::TempDays => i18n::tr(
                language,
                "用于 /tmp 和 /var/tmp 的最小保留天数。",
                "Minimum age for /tmp and /var/tmp entries.",
            ),
            Self::UserCacheDays => i18n::tr(
                language,
                "用于 $HOME/.cache 顶层目录的最小保留天数。",
                "Minimum age for top-level $HOME/.cache directories.",
            ),
            Self::AiAgentDays => i18n::tr(
                language,
                "用于已知 AI agent 缓存、日志、附件和生成产物的最小保留天数。",
                "Minimum age for known AI agent caches, logs, attachments, and generated artifacts.",
            ),
        }
    }

    fn affected_target(self, language: Language) -> &'static str {
        match self {
            Self::KeepPackages => i18n::tr(language, "Pacman 包缓存", "Pacman package cache"),
            Self::JournalDays | Self::JournalSize => {
                i18n::tr(language, "systemd 日志", "Systemd journal")
            }
            Self::TempDays => i18n::tr(language, "临时文件", "Temporary files"),
            Self::UserCacheDays => i18n::tr(language, "用户缓存", "User cache"),
            Self::AiAgentDays => i18n::tr(language, "AI agent 缓存", "AI agent caches"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

impl Rect {
    fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    fn inset(self, x: u16, y: u16) -> Self {
        Self {
            x: self.x.saturating_add(x),
            y: self.y.saturating_add(y),
            width: self.width.saturating_sub(x.saturating_mul(2)),
            height: self.height.saturating_sub(y.saturating_mul(2)),
        }
    }
}

#[derive(Clone, Copy)]
struct Paint {
    color: Option<Color>,
    bold: bool,
}

impl Paint {
    const NORMAL: Self = Self {
        color: None,
        bold: false,
    };

    const MUTED: Self = Self {
        color: Some(Color::DarkGrey),
        bold: false,
    };

    const ACCENT: Self = Self {
        color: Some(Color::Cyan),
        bold: true,
    };

    const FOCUS: Self = Self {
        color: Some(Color::Yellow),
        bold: true,
    };

    const WARNING: Self = Self {
        color: Some(Color::Yellow),
        bold: false,
    };
}

pub fn run(language: Language) -> Result<i32, String> {
    let mut session = Session::new(language);
    session.run()
}

struct Session {
    page: Page,
    language: Language,
    options: CleanerOptions,
    targets: Vec<CleanupTarget>,
    selected: Vec<bool>,
    target_cursor: usize,
    setting_cursor: usize,
}

impl Session {
    fn new(language: Language) -> Self {
        let options = CleanerOptions::default();
        let targets = all_targets(&options);
        let selected = vec![true; targets.len()];

        Self {
            page: Page::Targets,
            language,
            options,
            targets,
            selected,
            target_cursor: 0,
            setting_cursor: 0,
        }
    }

    fn run(&mut self) -> Result<i32, String> {
        let _guard = TerminalGuard::enter()?;

        loop {
            self.draw()?;

            let event =
                event::read().map_err(|error| format!("could not read key event: {error}"))?;
            if let Event::Key(key_event) = event {
                if key_event.kind != KeyEventKind::Press {
                    continue;
                }

                if self.handle_key(key_event.code, key_event.modifiers)? {
                    break;
                }
            }
        }

        Ok(0)
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Result<bool, String> {
        if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c')) {
            // Quit on Ctrl+C instead of falling through to the `c` cleanup
            // shortcut on the targets page.
            return Ok(true);
        }

        if matches!(code, KeyCode::Char('\u{0c}'))
            || (modifiers.contains(KeyModifiers::CONTROL)
                && matches!(code, KeyCode::Char('l') | KeyCode::Char('L')))
        {
            self.language = self.language.toggle();
            return Ok(false);
        }

        match self.page {
            Page::Targets => self.handle_targets_key(code),
            Page::Settings => self.handle_settings_key(code),
        }
    }

    fn handle_targets_key(&mut self, code: KeyCode) -> Result<bool, String> {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(true),
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('g') => self.page = Page::Settings,
            KeyCode::Char('a') => self.selected.fill(true),
            KeyCode::Char('n') => self.selected.fill(false),
            KeyCode::Char(' ') => self.toggle_target_selection(),
            KeyCode::Up | KeyCode::Char('k') => self.move_target_cursor(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_target_cursor(1),
            KeyCode::Home => self.target_cursor = 0,
            KeyCode::End => self.target_cursor = self.targets.len().saturating_sub(1),
            KeyCode::Char('s') | KeyCode::Enter => self.scan_selected()?,
            KeyCode::Char('c') => self.apply_selected()?,
            KeyCode::Char(character) if character.is_ascii_digit() => {
                if let Some(index) = character
                    .to_digit(10)
                    .map(|digit| digit as usize)
                    .and_then(|digit| digit.checked_sub(1))
                    .filter(|index| *index < self.selected.len())
                {
                    self.selected[index] = !self.selected[index];
                    self.target_cursor = index;
                }
            }
            _ => {}
        }

        Ok(false)
    }

    fn handle_settings_key(&mut self, code: KeyCode) -> Result<bool, String> {
        match code {
            KeyCode::Esc | KeyCode::Tab | KeyCode::Left | KeyCode::Char('g') => {
                self.page = Page::Targets;
            }
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Up | KeyCode::Char('k') => self.move_setting_cursor(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_setting_cursor(1),
            KeyCode::Home => self.setting_cursor = 0,
            KeyCode::End => self.setting_cursor = SettingField::ALL.len().saturating_sub(1),
            KeyCode::Char('r') => self.reset_settings(),
            KeyCode::Char('+') | KeyCode::Char('=') => self.bump_current_setting(1),
            KeyCode::Char('-') => self.bump_current_setting(-1),
            KeyCode::Enter => self.edit_current_setting()?,
            _ => {}
        }

        Ok(false)
    }

    fn draw(&self) -> Result<(), String> {
        let (width, height) =
            terminal::size().map_err(|error| format!("could not read terminal size: {error}"))?;
        let mut stdout = io::stdout();
        execute!(stdout, Clear(ClearType::All), MoveTo(0, 0))
            .map_err(|error| format!("could not clear screen: {error}"))?;

        if width < 48 || height < 14 {
            write_at(
                &mut stdout,
                0,
                0,
                width,
                i18n::tr(
                    self.language,
                    "终端窗口太小，请放大后继续。",
                    "Terminal is too small. Resize it to continue.",
                ),
                Paint::WARNING,
            )?;
            stdout
                .flush()
                .map_err(|error| format!("could not flush stdout: {error}"))?;
            return Ok(());
        }

        self.draw_header(&mut stdout, width)?;
        match self.page {
            Page::Targets => self.draw_targets(&mut stdout, width, height)?,
            Page::Settings => self.draw_settings(&mut stdout, width, height)?,
        }
        self.draw_footer(&mut stdout, width, height)?;

        stdout
            .flush()
            .map_err(|error| format!("could not flush stdout: {error}"))
    }

    fn draw_header(&self, stdout: &mut io::Stdout, width: u16) -> Result<(), String> {
        let selected_count = self.selected.iter().filter(|selected| **selected).count();
        let page_label = match self.page {
            Page::Targets => i18n::targets_page_title(self.language),
            Page::Settings => i18n::settings_page_title(self.language),
        };
        let header = format!(
            "arch-cleaner {}  |  {}  |  {} {}/{}  |  {}: {}",
            env!("CARGO_PKG_VERSION"),
            page_label,
            i18n::tr(self.language, "已选", "selected"),
            selected_count,
            self.targets.len(),
            i18n::current_language_label(self.language),
            self.language.label()
        );
        write_at(stdout, 0, 0, width, header, Paint::ACCENT)?;

        let tabs = format!(
            "{}   {}",
            tab_label(
                i18n::targets_page_title(self.language),
                self.page == Page::Targets
            ),
            tab_label(
                i18n::settings_page_title(self.language),
                self.page == Page::Settings,
            )
        );
        write_at(stdout, 0, 1, width, tabs, Paint::NORMAL)?;
        draw_rule(stdout, 0, 2, width)
    }

    fn draw_targets(&self, stdout: &mut io::Stdout, width: u16, height: u16) -> Result<(), String> {
        let content = content_rect(width, height);
        if content.is_empty() {
            return Ok(());
        }

        if width >= 96 && content.height >= 12 {
            let left_width = (width / 3).clamp(34, 46);
            let left = Rect {
                x: content.x,
                y: content.y,
                width: left_width,
                height: content.height,
            };
            let right = Rect {
                x: content.x + left_width + 2,
                y: content.y,
                width: content.width.saturating_sub(left_width + 2),
                height: content.height,
            };
            self.draw_target_list(stdout, left)?;
            draw_vertical_rule(stdout, left.x + left.width, left.y, left.height)?;
            self.draw_target_detail(stdout, right)
        } else {
            self.draw_targets_compact(stdout, content)
        }
    }

    fn draw_target_list(&self, stdout: &mut io::Stdout, rect: Rect) -> Result<(), String> {
        draw_panel_title(
            stdout,
            rect,
            i18n::tr(self.language, "清理目标", "Cleanup targets"),
        )?;
        let list = rect.inset(0, 1);
        let capacity = (list.height as usize / 2).max(1);
        let start = scroll_start(self.target_cursor, self.targets.len(), capacity);
        let end = (start + capacity).min(self.targets.len());

        for (visible, index) in (start..end).enumerate() {
            let target = &self.targets[index];
            let y = list.y + visible as u16 * 2;
            let focused = index == self.target_cursor;
            let mark = if self.selected[index] { "x" } else { " " };
            let title = target.title.get(self.language);
            let line = format!(
                "{} [{}] {:>2}. {}",
                if focused { ">" } else { " " },
                mark,
                index + 1,
                title
            );
            let paint = if focused {
                Paint::FOCUS
            } else if self.selected[index] {
                Paint::NORMAL
            } else {
                Paint::MUTED
            };
            write_at(stdout, list.x, y, list.width, line, paint)?;
            write_at(
                stdout,
                list.x,
                y + 1,
                list.width,
                self.target_meta_line(target),
                Paint::MUTED,
            )?;
        }

        Ok(())
    }

    fn draw_target_detail(&self, stdout: &mut io::Stdout, rect: Rect) -> Result<(), String> {
        draw_panel_title(stdout, rect, i18n::tr(self.language, "详情", "Details"))?;
        let Some(target) = self.targets.get(self.target_cursor) else {
            return Ok(());
        };

        let body = rect.inset(0, 1);
        let mut row = 0u16;
        panel_line(
            stdout,
            body,
            &mut row,
            target.title.get(self.language),
            Paint::ACCENT,
        )?;
        panel_line(
            stdout,
            body,
            &mut row,
            self.target_meta_line(target),
            Paint::NORMAL,
        )?;
        panel_line(stdout, body, &mut row, "", Paint::NORMAL)?;

        for line in wrap_to_width(
            target.description.get(self.language),
            body.width as usize,
            3,
        ) {
            panel_line(stdout, body, &mut row, line, Paint::NORMAL)?;
        }
        panel_line(stdout, body, &mut row, "", Paint::NORMAL)?;

        panel_line(
            stdout,
            body,
            &mut row,
            i18n::tr(self.language, "预览命令", "Dry-run commands"),
            Paint::MUTED,
        )?;
        for command in &target.dry_run_commands {
            panel_line(
                stdout,
                body,
                &mut row,
                format!("  {}", command.display),
                Paint::NORMAL,
            )?;
        }
        panel_line(stdout, body, &mut row, "", Paint::NORMAL)?;

        panel_line(
            stdout,
            body,
            &mut row,
            i18n::tr(self.language, "执行命令", "Apply commands"),
            Paint::MUTED,
        )?;
        for command in &target.apply_commands {
            panel_line(
                stdout,
                body,
                &mut row,
                format!("  {}", command.display),
                Paint::NORMAL,
            )?;
        }
        Ok(())
    }

    fn draw_targets_compact(&self, stdout: &mut io::Stdout, rect: Rect) -> Result<(), String> {
        draw_panel_title(
            stdout,
            rect,
            i18n::tr(self.language, "清理目标", "Cleanup targets"),
        )?;
        let mut row = 1u16;
        let capacity = ((rect.height.saturating_sub(7)) as usize / 2).max(1);
        let start = scroll_start(self.target_cursor, self.targets.len(), capacity);
        let end = (start + capacity).min(self.targets.len());

        for index in start..end {
            let target = &self.targets[index];
            let focused = index == self.target_cursor;
            let mark = if self.selected[index] { "x" } else { " " };
            let title = target.title.get(self.language);
            let paint = if focused { Paint::FOCUS } else { Paint::NORMAL };
            write_at(
                stdout,
                rect.x,
                rect.y + row,
                rect.width,
                format!(
                    "{} [{}] {:>2}. {}",
                    if focused { ">" } else { " " },
                    mark,
                    index + 1,
                    title
                ),
                paint,
            )?;
            row += 1;
            write_at(
                stdout,
                rect.x,
                rect.y + row,
                rect.width,
                self.target_meta_line(target),
                Paint::MUTED,
            )?;
            row += 1;
        }

        if let Some(target) = self.targets.get(self.target_cursor) {
            row = row.saturating_add(1);
            draw_rule(stdout, rect.x, rect.y + row, rect.width)?;
            row = row.saturating_add(1);
            let description = target.description.get(self.language);
            for line in wrap_to_width(description, rect.width as usize, 2) {
                if row >= rect.height {
                    break;
                }
                write_at(
                    stdout,
                    rect.x,
                    rect.y + row,
                    rect.width,
                    line,
                    Paint::NORMAL,
                )?;
                row += 1;
            }
        }

        Ok(())
    }

    fn draw_settings(
        &self,
        stdout: &mut io::Stdout,
        width: u16,
        height: u16,
    ) -> Result<(), String> {
        let content = content_rect(width, height);
        if content.is_empty() {
            return Ok(());
        }

        if width >= 92 && content.height >= 10 {
            let left_width = (width / 2).clamp(38, 54);
            let left = Rect {
                x: content.x,
                y: content.y,
                width: left_width,
                height: content.height,
            };
            let right = Rect {
                x: content.x + left_width + 2,
                y: content.y,
                width: content.width.saturating_sub(left_width + 2),
                height: content.height,
            };
            self.draw_settings_list(stdout, left)?;
            draw_vertical_rule(stdout, left.x + left.width, left.y, left.height)?;
            self.draw_setting_detail(stdout, right)
        } else {
            self.draw_settings_compact(stdout, content)
        }
    }

    fn draw_settings_list(&self, stdout: &mut io::Stdout, rect: Rect) -> Result<(), String> {
        draw_panel_title(stdout, rect, i18n::settings_page_title(self.language))?;
        for (index, field) in SettingField::ALL.iter().enumerate() {
            let y = rect.y + 1 + index as u16;
            if y >= rect.y + rect.height {
                break;
            }
            let marker = if index == self.setting_cursor {
                ">"
            } else {
                " "
            };
            let paint = if index == self.setting_cursor {
                Paint::FOCUS
            } else {
                Paint::NORMAL
            };
            write_at(
                stdout,
                rect.x,
                y,
                rect.width,
                format!("{marker} {}", self.setting_line(*field)),
                paint,
            )?;
        }
        Ok(())
    }

    fn draw_setting_detail(&self, stdout: &mut io::Stdout, rect: Rect) -> Result<(), String> {
        draw_panel_title(
            stdout,
            rect,
            i18n::tr(self.language, "设置说明", "Setting details"),
        )?;
        let Some(field) = SettingField::ALL.get(self.setting_cursor).copied() else {
            return Ok(());
        };

        let body = rect.inset(0, 1);
        let mut row = 0u16;
        panel_line(
            stdout,
            body,
            &mut row,
            field.label(self.language),
            Paint::ACCENT,
        )?;
        panel_line(
            stdout,
            body,
            &mut row,
            format!(
                "{}: {}",
                i18n::tr(self.language, "当前值", "Current value"),
                self.current_setting_value(field)
            ),
            Paint::NORMAL,
        )?;
        panel_line(
            stdout,
            body,
            &mut row,
            format!(
                "{}: {}",
                i18n::tr(self.language, "影响目标", "Affects"),
                field.affected_target(self.language)
            ),
            Paint::NORMAL,
        )?;
        panel_line(stdout, body, &mut row, "", Paint::NORMAL)?;
        for line in wrap_to_width(field.description(self.language), body.width as usize, 4) {
            panel_line(stdout, body, &mut row, line, Paint::NORMAL)?;
        }
        panel_line(stdout, body, &mut row, "", Paint::NORMAL)?;
        panel_line(
            stdout,
            body,
            &mut row,
            i18n::settings_page_hint(self.language),
            Paint::MUTED,
        )
    }

    fn draw_settings_compact(&self, stdout: &mut io::Stdout, rect: Rect) -> Result<(), String> {
        draw_panel_title(stdout, rect, i18n::settings_page_title(self.language))?;
        let mut row = 1u16;
        for (index, field) in SettingField::ALL.iter().enumerate() {
            if row >= rect.height {
                break;
            }
            let marker = if index == self.setting_cursor {
                ">"
            } else {
                " "
            };
            let paint = if index == self.setting_cursor {
                Paint::FOCUS
            } else {
                Paint::NORMAL
            };
            write_at(
                stdout,
                rect.x,
                rect.y + row,
                rect.width,
                format!("{marker} {}", self.setting_line(*field)),
                paint,
            )?;
            row += 1;
        }

        row += 1;
        if row < rect.height {
            write_at(
                stdout,
                rect.x,
                rect.y + row,
                rect.width,
                i18n::settings_page_hint(self.language),
                Paint::MUTED,
            )?;
        }
        Ok(())
    }

    fn draw_footer(&self, stdout: &mut io::Stdout, width: u16, height: u16) -> Result<(), String> {
        let footer_top = height.saturating_sub(3);
        draw_rule(stdout, 0, footer_top, width)?;
        let help = match self.page {
            Page::Targets => i18n::help_line(self.language),
            Page::Settings => i18n::settings_page_hint(self.language),
        };
        write_at(stdout, 0, footer_top + 1, width, help, Paint::MUTED)?;
        write_at(
            stdout,
            0,
            footer_top + 2,
            width,
            i18n::tr(
                self.language,
                "Ctrl+L 切换语言 | q 退出",
                "Ctrl+L language | q quit",
            ),
            Paint::MUTED,
        )
    }

    fn target_meta_line(&self, target: &CleanupTarget) -> String {
        let group = i18n::target_group_label(self.language, target.group);
        let risk = i18n::risk_label(self.language, target.risk);
        let scope = i18n::scope_label(self.language, target.requires_sudo);
        let threshold = fit_to_width(
            &(target.threshold_summary)(&self.options, self.language),
            18,
        )
        .trim_end()
        .to_string();
        format!("{} | {} | {} | {}", group, risk, scope, threshold)
    }

    fn setting_line(&self, field: SettingField) -> String {
        format!(
            "{:<18} {}",
            field.label(self.language),
            self.current_setting_value(field)
        )
    }

    fn selected_targets(&self) -> Vec<CleanupTarget> {
        self.targets
            .iter()
            .zip(self.selected.iter())
            .filter_map(|(target, is_selected)| is_selected.then_some(target.clone()))
            .collect()
    }

    fn toggle_target_selection(&mut self) {
        if self.target_cursor < self.selected.len() {
            self.selected[self.target_cursor] = !self.selected[self.target_cursor];
        }
    }

    fn move_target_cursor(&mut self, delta: isize) {
        self.target_cursor = move_index(self.target_cursor, delta, self.targets.len());
    }

    fn move_setting_cursor(&mut self, delta: isize) {
        self.setting_cursor = move_index(self.setting_cursor, delta, SettingField::ALL.len());
    }

    fn scan_selected(&mut self) -> Result<(), String> {
        let selected_targets = self.selected_targets();
        show_output_screen()?;

        if selected_targets.is_empty() {
            return wait_for_enter_then_resume(
                i18n::no_targets_selected(self.language),
                self.language,
            );
        }

        println!("{}\n", i18n::scan_header(self.language));
        let reports = scan_all(&selected_targets, &self.options, self.language);

        print_scan_reports(&reports, self.language);

        wait_for_enter_then_resume(i18n::scan_finished(self.language), self.language)
    }

    fn apply_selected(&mut self) -> Result<(), String> {
        let selected_targets = self.selected_targets();
        show_output_screen()?;

        if selected_targets.is_empty() {
            return wait_for_enter_then_resume(
                i18n::no_targets_selected(self.language),
                self.language,
            );
        }

        print_plan(
            &selected_targets,
            ExecutionMode::Apply,
            &self.options,
            self.language,
        );

        if !confirm_apply_cooked(self.language)? {
            return wait_for_enter_then_resume(i18n::aborted(self.language), self.language);
        }

        let results = execute_targets(
            &selected_targets,
            ExecutionOptions {
                mode: ExecutionMode::Apply,
                run_readonly_checks: true,
            },
        );
        print_results(&results, self.language);
        wait_for_enter_then_resume(i18n::cleanup_finished(self.language), self.language)
    }

    fn edit_current_setting(&mut self) -> Result<(), String> {
        let Some(field) = SettingField::ALL.get(self.setting_cursor).copied() else {
            return Ok(());
        };

        let current = self.current_setting_value(field);
        let prompt = i18n::setting_prompt(self.language, field.label(self.language), &current);
        let value = prompt_line(&prompt)?;
        let value = value.trim();

        if value.is_empty() {
            return Ok(());
        }

        let result = self.update_setting(field, value);
        match result {
            Ok(()) => Ok(()),
            Err(error) => show_message(&error, self.language),
        }
    }

    fn update_setting(&mut self, field: SettingField, value: &str) -> Result<(), String> {
        match field {
            SettingField::KeepPackages => {
                self.options.keep_package_versions =
                    parse_u8(field.label(self.language), value, self.language)?;
            }
            SettingField::JournalDays => {
                self.options.journal_days =
                    parse_u16(field.label(self.language), value, self.language)?;
            }
            SettingField::JournalSize => {
                if !is_valid_journal_size(value) {
                    return Err(i18n::invalid_size_value(
                        self.language,
                        field.label(self.language),
                        value,
                    ));
                }
                self.options.journal_size = value.trim().to_string();
            }
            SettingField::TempDays => {
                self.options.temp_min_age_days =
                    parse_u16(field.label(self.language), value, self.language)?;
            }
            SettingField::UserCacheDays => {
                self.options.user_cache_min_age_days =
                    parse_u16(field.label(self.language), value, self.language)?;
            }
            SettingField::AiAgentDays => {
                self.options.ai_agent_min_age_days =
                    parse_u16(field.label(self.language), value, self.language)?;
            }
        }

        self.refresh_targets();
        Ok(())
    }

    fn bump_current_setting(&mut self, delta: i16) {
        let Some(field) = SettingField::ALL.get(self.setting_cursor).copied() else {
            return;
        };

        match field {
            SettingField::KeepPackages => {
                let value = self.options.keep_package_versions as i16 + delta;
                self.options.keep_package_versions = value.clamp(1, 99) as u8;
            }
            SettingField::JournalDays => {
                let value = self.options.journal_days as i32 + delta as i32;
                self.options.journal_days = value.clamp(1, 3650) as u16;
            }
            SettingField::JournalSize => {
                if delta >= 0 {
                    self.options.journal_size = bump_size_up(&self.options.journal_size);
                } else {
                    self.options.journal_size = bump_size_down(&self.options.journal_size);
                }
            }
            SettingField::TempDays => {
                let value = self.options.temp_min_age_days as i32 + delta as i32;
                self.options.temp_min_age_days = value.clamp(1, 3650) as u16;
            }
            SettingField::UserCacheDays => {
                let value = self.options.user_cache_min_age_days as i32 + delta as i32;
                self.options.user_cache_min_age_days = value.clamp(1, 3650) as u16;
            }
            SettingField::AiAgentDays => {
                let value = self.options.ai_agent_min_age_days as i32 + delta as i32;
                self.options.ai_agent_min_age_days = value.clamp(1, 3650) as u16;
            }
        }

        self.refresh_targets();
    }

    fn reset_settings(&mut self) {
        self.options = CleanerOptions::default();
        self.refresh_targets();
    }

    fn current_setting_value(&self, field: SettingField) -> String {
        match field {
            SettingField::KeepPackages => self.options.keep_package_versions.to_string(),
            SettingField::JournalDays => self.options.journal_days.to_string(),
            SettingField::JournalSize => self.options.journal_size.clone(),
            SettingField::TempDays => self.options.temp_min_age_days.to_string(),
            SettingField::UserCacheDays => self.options.user_cache_min_age_days.to_string(),
            SettingField::AiAgentDays => self.options.ai_agent_min_age_days.to_string(),
        }
    }

    fn refresh_targets(&mut self) {
        let selected_ids: HashSet<&'static str> = self
            .targets
            .iter()
            .zip(self.selected.iter())
            .filter_map(|(target, selected)| selected.then_some(target.id))
            .collect();

        self.targets = all_targets(&self.options);
        self.selected = self
            .targets
            .iter()
            .map(|target| selected_ids.contains(&target.id))
            .collect();

        if self.target_cursor >= self.targets.len() && !self.targets.is_empty() {
            self.target_cursor = self.targets.len() - 1;
        }

        if self.setting_cursor >= SettingField::ALL.len() {
            self.setting_cursor = SettingField::ALL.len().saturating_sub(1);
        }
    }
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self, String> {
        terminal::enable_raw_mode()
            .map_err(|error| format!("could not enable raw mode: {error}"))?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Hide)
            .map_err(|error| format!("could not enter alternate screen: {error}"))?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, Show, LeaveAlternateScreen);
    }
}

fn content_rect(width: u16, height: u16) -> Rect {
    Rect {
        x: 0,
        y: 3,
        width,
        height: height.saturating_sub(6),
    }
}

fn tab_label(label: &str, active: bool) -> String {
    if active {
        format!("[{label}]")
    } else {
        format!(" {label} ")
    }
}

fn write_at(
    stdout: &mut io::Stdout,
    x: u16,
    y: u16,
    width: u16,
    text: impl AsRef<str>,
    paint: Paint,
) -> Result<(), String> {
    queue!(stdout, MoveTo(x, y)).map_err(|error| format!("could not move cursor: {error}"))?;
    if let Some(color) = paint.color {
        queue!(stdout, SetForegroundColor(color))
            .map_err(|error| format!("could not set color: {error}"))?;
    }
    if paint.bold {
        queue!(stdout, SetAttribute(Attribute::Bold))
            .map_err(|error| format!("could not set style: {error}"))?;
    }
    queue!(stdout, Print(fit_to_width(text.as_ref(), width as usize)))
        .map_err(|error| format!("could not write text: {error}"))?;
    queue!(stdout, ResetColor, SetAttribute(Attribute::Reset))
        .map_err(|error| format!("could not reset style: {error}"))
}

fn draw_rule(stdout: &mut io::Stdout, x: u16, y: u16, width: u16) -> Result<(), String> {
    write_at(
        stdout,
        x,
        y,
        width,
        "-".repeat(width as usize),
        Paint::MUTED,
    )
}

fn draw_vertical_rule(stdout: &mut io::Stdout, x: u16, y: u16, height: u16) -> Result<(), String> {
    for row in 0..height {
        write_at(stdout, x, y + row, 1, "|", Paint::MUTED)?;
    }
    Ok(())
}

fn draw_panel_title(stdout: &mut io::Stdout, rect: Rect, title: &str) -> Result<(), String> {
    write_at(stdout, rect.x, rect.y, rect.width, title, Paint::ACCENT)
}

fn panel_line(
    stdout: &mut io::Stdout,
    rect: Rect,
    row: &mut u16,
    text: impl AsRef<str>,
    paint: Paint,
) -> Result<(), String> {
    if *row >= rect.height {
        return Ok(());
    }
    write_at(stdout, rect.x, rect.y + *row, rect.width, text, paint)?;
    *row += 1;
    Ok(())
}

fn scroll_start(cursor: usize, total: usize, capacity: usize) -> usize {
    if total <= capacity {
        return 0;
    }

    let half = capacity / 2;
    let start = cursor.saturating_sub(half);
    start.min(total.saturating_sub(capacity))
}

fn move_index(current: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }

    if delta < 0 {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        (current + delta as usize).min(len - 1)
    }
}

fn fit_to_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    if UnicodeWidthStr::width(text) <= width {
        return pad_to_width(text.to_string(), width);
    }

    if width <= 3 {
        return ".".repeat(width);
    }

    let limit = width - 3;
    let mut output = String::new();
    let mut used = 0usize;

    for character in text.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > limit {
            break;
        }
        output.push(character);
        used += character_width;
    }

    output.push_str("...");
    pad_to_width(output, width)
}

fn pad_to_width(mut text: String, width: usize) -> String {
    let used = UnicodeWidthStr::width(text.as_str());
    if used < width {
        text.push_str(&" ".repeat(width - used));
    }
    text
}

fn wrap_to_width(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    if width == 0 || max_lines == 0 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for character in text.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if current_width + character_width > width && !current.is_empty() {
            lines.push(current);
            if lines.len() == max_lines {
                return lines;
            }
            current = String::new();
            current_width = 0;
        }

        current.push(character);
        current_width += character_width;
    }

    if !current.is_empty() && lines.len() < max_lines {
        lines.push(current);
    }

    lines
}

fn show_output_screen() -> Result<(), String> {
    terminal::disable_raw_mode().map_err(|error| format!("could not disable raw mode: {error}"))?;
    let mut stdout = io::stdout();
    execute!(stdout, Show, Clear(ClearType::All), MoveTo(0, 0))
        .map_err(|error| format!("could not prepare output screen: {error}"))
}

fn resume_menu_screen() -> Result<(), String> {
    terminal::enable_raw_mode()
        .map_err(|error| format!("could not re-enable raw mode: {error}"))?;
    let mut stdout = io::stdout();
    execute!(stdout, Hide).map_err(|error| format!("could not hide cursor: {error}"))
}

fn prompt_line(prompt: &str) -> Result<String, String> {
    terminal::disable_raw_mode().map_err(|error| format!("could not disable raw mode: {error}"))?;
    let mut stdout = io::stdout();
    execute!(stdout, Show).map_err(|error| format!("could not show cursor: {error}"))?;
    print!("\r\n{prompt}");
    io::stdout()
        .flush()
        .map_err(|error| format!("could not flush stdout: {error}"))?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("could not read input: {error}"))?;

    resume_menu_screen()?;
    Ok(input)
}

fn read_line_cooked(prompt: &str) -> Result<String, String> {
    print!("{prompt}");
    io::stdout()
        .flush()
        .map_err(|error| format!("could not flush stdout: {error}"))?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("could not read input: {error}"))?;

    Ok(input)
}

fn confirm_apply_cooked(language: Language) -> Result<bool, String> {
    let answer = read_line_cooked(i18n::confirm_apply_prompt(language))?;
    Ok(answer.trim() == "APPLY")
}

fn wait_for_enter_then_resume(message: &str, language: Language) -> Result<(), String> {
    println!("\n{message}");
    let _ = read_line_cooked(i18n::press_enter_prompt(language))?;
    resume_menu_screen()
}

fn show_message(message: &str, language: Language) -> Result<(), String> {
    terminal::disable_raw_mode().map_err(|error| format!("could not disable raw mode: {error}"))?;
    println!("\r\n{message}");
    let _ = read_line_cooked(i18n::press_enter_prompt(language))?;
    resume_menu_screen()
}

fn bump_size_up(value: &str) -> String {
    let (number, suffix) = parse_size(value);
    format!("{}{}", number.saturating_add(1), suffix)
}

fn bump_size_down(value: &str) -> String {
    let (number, suffix) = parse_size(value);
    format!("{}{}", number.saturating_sub(1).max(1), suffix)
}

fn parse_size(value: &str) -> (u64, String) {
    let trimmed = value.trim();
    let split_index = trimmed
        .char_indices()
        .find(|(_, character)| !character.is_ascii_digit())
        .map(|(index, _)| index)
        .unwrap_or(trimmed.len());

    let number = trimmed[..split_index].parse::<u64>().unwrap_or(1).max(1);
    let suffix = trimmed[split_index..].trim().to_string();
    (number, suffix)
}

#[cfg(test)]
mod tests {
    use super::{Page, Session, bump_size_down, bump_size_up, fit_to_width, move_index};
    use crate::i18n::Language;
    use crossterm::event::{KeyCode, KeyModifiers};
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn ctrl_l_toggles_language_without_enter() {
        let mut session = Session::new(Language::ZhCn);

        session
            .handle_key(KeyCode::Char('l'), KeyModifiers::CONTROL)
            .unwrap();

        assert_eq!(session.language, Language::En);
    }

    #[test]
    fn control_character_form_feed_toggles_language() {
        let mut session = Session::new(Language::ZhCn);

        session
            .handle_key(KeyCode::Char('\u{0c}'), KeyModifiers::empty())
            .unwrap();

        assert_eq!(session.language, Language::En);
    }

    #[test]
    fn ctrl_c_quits_from_any_page() {
        let mut session = Session::new(Language::ZhCn);
        assert!(
            session
                .handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL)
                .unwrap()
        );

        session
            .handle_key(KeyCode::Tab, KeyModifiers::empty())
            .unwrap();
        assert!(
            session
                .handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL)
                .unwrap()
        );
    }

    #[test]
    fn tab_opens_settings_page() {
        let mut session = Session::new(Language::ZhCn);

        session
            .handle_key(KeyCode::Tab, KeyModifiers::empty())
            .unwrap();

        assert_eq!(session.page, Page::Settings);
    }

    #[test]
    fn plus_updates_current_setting() {
        let mut session = Session::new(Language::ZhCn);

        session
            .handle_key(KeyCode::Tab, KeyModifiers::empty())
            .unwrap();
        session
            .handle_key(KeyCode::Char('+'), KeyModifiers::empty())
            .unwrap();

        assert_eq!(session.options.keep_package_versions, 4);
        assert_eq!(
            session.targets[0].dry_run_commands[0].display,
            "paccache -d -k 4"
        );
    }

    #[test]
    fn bumps_journal_size_without_losing_suffix() {
        assert_eq!(bump_size_up("1G"), "2G");
        assert_eq!(bump_size_down("2G"), "1G");
    }

    #[test]
    fn moves_cursor_with_bounds() {
        assert_eq!(move_index(0, -1, 7), 0);
        assert_eq!(move_index(3, 1, 7), 4);
        assert_eq!(move_index(6, 1, 7), 6);
    }

    #[test]
    fn fits_cjk_text_to_display_width() {
        assert_eq!(
            UnicodeWidthStr::width(fit_to_width("Pacman 包缓存", 12).as_str()),
            12
        );
        assert_eq!(
            UnicodeWidthStr::width(fit_to_width("Pacman 包缓存", 8).as_str()),
            8
        );
    }
}
