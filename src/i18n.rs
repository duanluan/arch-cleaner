use crate::executor::ExecutionMode;
use crate::model::{CleanerOptions, RiskLevel, ScanStatus, TargetGroup};
use crate::rules::all_targets;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Language {
    #[default]
    ZhCn,
    En,
}

impl Language {
    pub fn toggle(self) -> Self {
        match self {
            Self::ZhCn => Self::En,
            Self::En => Self::ZhCn,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN",
            Self::En => "en",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ZhCn => "中文",
            Self::En => "English",
        }
    }
}

pub fn parse_language(value: &str) -> Result<Language, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "zh" | "zh-cn" | "zh_cn" | "cn" => Ok(Language::ZhCn),
        "en" | "en-us" | "en_us" => Ok(Language::En),
        _ => Err(format!("unknown language: {value}. Supported: zh, en")),
    }
}

pub fn tr<'a>(language: Language, zh: &'a str, en: &'a str) -> &'a str {
    match language {
        Language::ZhCn => zh,
        Language::En => en,
    }
}

/// tr 的格式化版本：两侧都要 format! 时用这个，避免重写 match language。
pub fn tr_owned(language: Language, zh: String, en: String) -> String {
    match language {
        Language::ZhCn => zh,
        Language::En => en,
    }
}

fn target_id_list() -> String {
    all_targets(&CleanerOptions::default())
        .iter()
        .map(|target| target.id)
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn help_text(language: Language, version: &str) -> String {
    let targets = target_id_list();
    match language {
        Language::ZhCn => format!(
            "arch-cleaner {version}\n\n用法:\n    arch-cleaner                  启动交互式 TUI 菜单\n    arch-cleaner tui              启动交互式 TUI 菜单\n    arch-cleaner list-targets     显示清理目标\n    arch-cleaner scan [OPTIONS]   检查选中的目标\n    arch-cleaner clean [OPTIONS]  显示或执行清理计划\n\n选项:\n    -V, --version                 显示版本号\n    --lang, -l <zh|en>            界面语言 [默认: zh]\n    --targets <ids>               以逗号分隔的目标 ID，或 all\n    --apply                       执行清理命令\n    --yes, -y                     跳过 --apply 的确认提示\n    --run-readonly-checks         在 dry-run 模式下运行只读命令\n    --json                        输出机器可读 JSON\n    --keep-packages <n>           Pacman 包版本保留数量 [默认: 3]\n    --journal-days <n>            日志清理天数阈值 [默认: 14]\n    --journal-size <size>         日志清理大小阈值 [默认: 1G]\n    --temp-days <n>               临时文件保留天数 [默认: 7]\n    --user-cache-days <n>         用户缓存保留天数 [默认: 30]\n    --ai-agent-days <n>           AI agent 缓存保留天数 [默认: 30]\n\n说明:\n    在 TUI 中按 Tab 进入设置页，按 Ctrl+L 切换语言。\n\n目标:\n    {targets}"
        ),
        Language::En => format!(
            "arch-cleaner {version}\n\nUSAGE:\n    arch-cleaner                  Start the interactive TUI menu\n    arch-cleaner tui              Start the interactive TUI menu\n    arch-cleaner list-targets     Show cleanup targets\n    arch-cleaner scan [OPTIONS]   Inspect selected targets\n    arch-cleaner clean [OPTIONS]  Show or execute a cleanup plan\n\nOPTIONS:\n    -V, --version                 Print version\n    --lang, -l <zh|en>            UI language [default: zh]\n    --targets <ids>               Comma-separated target ids, or all\n    --apply                       Execute cleanup commands\n    --yes, -y                     Skip confirmation prompts for --apply\n    --run-readonly-checks         In dry-run mode, run read-only commands\n    --json                        Print machine-readable JSON\n    --keep-packages <n>           Pacman package versions to keep [default: 3]\n    --journal-days <n>            Journal age vacuum threshold [default: 14]\n    --journal-size <size>         Journal size vacuum threshold [default: 1G]\n    --temp-days <n>               Temp file age threshold [default: 7]\n    --user-cache-days <n>         User cache age threshold [default: 30]\n    --ai-agent-days <n>           AI agent cache age threshold [default: 30]\n\nNOTES:\n    Press Tab in the TUI to open settings and Ctrl+L to switch languages.\n\nTARGETS:\n    {targets}"
        ),
    }
}

pub fn target_group_label(language: Language, group: TargetGroup) -> &'static str {
    match group {
        TargetGroup::Packages => tr(language, "软件包", "Packages"),
        TargetGroup::System => tr(language, "系统", "System"),
        TargetGroup::User => tr(language, "用户", "User"),
        TargetGroup::Developer => tr(language, "开发", "Developer"),
    }
}

pub fn risk_label(language: Language, risk: RiskLevel) -> &'static str {
    match risk {
        RiskLevel::Low => tr(language, "低", "low"),
        RiskLevel::Medium => tr(language, "中", "medium"),
    }
}

pub fn scope_label(language: Language, requires_sudo: bool) -> &'static str {
    if requires_sudo {
        tr(language, "系统", "sudo")
    } else {
        tr(language, "用户", "user")
    }
}

pub fn execution_mode_label(language: Language, mode: ExecutionMode) -> &'static str {
    match mode {
        ExecutionMode::DryRun => tr(language, "预览计划", "Dry-run plan"),
        ExecutionMode::Apply => tr(language, "执行计划", "Apply plan"),
    }
}

pub fn scan_status_label(language: Language, status: ScanStatus) -> &'static str {
    match status {
        ScanStatus::Ready => tr(language, "就绪", "ready"),
        ScanStatus::MissingTool => tr(language, "缺少工具", "missing-tool"),
        ScanStatus::Unavailable => tr(language, "不可用", "unavailable"),
    }
}

pub fn language_note(language: Language) -> &'static str {
    match language {
        Language::ZhCn => "Ctrl+L 切换语言",
        Language::En => "Ctrl+L switches languages",
    }
}

pub fn language_changed_message(language: Language) -> &'static str {
    match language {
        Language::ZhCn => "语言已切换。",
        Language::En => "Language switched.",
    }
}

pub fn no_targets_selected(language: Language) -> &'static str {
    tr(language, "没有选中的目标。", "No targets selected.")
}

pub fn scan_skipped(language: Language) -> &'static str {
    tr(language, "已跳过扫描。", "Scan skipped.")
}

pub fn scan_finished(language: Language) -> &'static str {
    tr(language, "扫描完成。", "Scan finished.")
}

pub fn cleanup_finished(language: Language) -> &'static str {
    tr(language, "清理完成。", "Cleanup finished.")
}

pub fn unknown_action(language: Language) -> &'static str {
    tr(language, "未知操作。", "Unknown action.")
}

pub fn aborted(language: Language) -> &'static str {
    tr(
        language,
        "已取消，没有执行任何更改。",
        "Aborted. No changes made.",
    )
}

pub fn confirm_apply_prompt(language: Language) -> &'static str {
    tr(
        language,
        "输入 APPLY 以执行所选清理命令：",
        "Type APPLY to execute selected cleanup commands: ",
    )
}

pub fn press_enter_prompt(language: Language) -> &'static str {
    tr(language, "按 Enter 继续...", "Press Enter to continue...")
}

pub fn select_action_prompt(language: Language) -> &'static str {
    tr(language, "请选择操作：", "Select an action: ")
}

pub fn cli_prompt_action(language: Language) -> &'static str {
    tr(language, "操作：", "Actions: ")
}

pub fn help_line(language: Language) -> &'static str {
    tr(
        language,
        "方向键移动 | 空格勾选 | Enter/s 扫描 | c 清理 | Tab 设置 | a 全选 | n 全不选",
        "Arrows move | Space toggle | Enter/s scan | c clean | Tab settings | a all | n none",
    )
}

pub fn list_header(language: Language) -> &'static str {
    tr(language, "目标列表", "Target list")
}

pub fn scan_header(language: Language) -> &'static str {
    tr(language, "扫描结果", "Scan results")
}

pub fn dry_run_notice(language: Language) -> &'static str {
    tr(
        language,
        "没有执行更改。重新运行并加上 --apply 才会执行计划。",
        "No changes made. Re-run with --apply to execute the plan.",
    )
}

pub fn running_readonly_checks(language: Language) -> &'static str {
    tr(language, "运行只读检查：", "Running read-only checks:")
}

pub fn executing_cleanup_plan(language: Language) -> &'static str {
    tr(language, "正在执行清理计划：", "Executing cleanup plan:")
}

pub fn unknown_command(language: Language, command: &str) -> String {
    tr_owned(
        language,
        format!("未知命令：{command}\n\n运行 'arch-cleaner --help'。"),
        format!("unknown command: {command}\n\nRun 'arch-cleaner --help'."),
    )
}

pub fn unknown_option(language: Language, option: &str) -> String {
    tr_owned(
        language,
        format!("未知选项：{option}"),
        format!("unknown option: {option}"),
    )
}

pub fn missing_value(language: Language, flag: &str) -> String {
    tr_owned(
        language,
        format!("{flag} 需要一个值"),
        format!("{flag} requires a value"),
    )
}

pub fn invalid_integer(language: Language, flag: &str, value: &str) -> String {
    tr_owned(
        language,
        format!("{flag} 需要整数，但收到 {value}"),
        format!("{flag} expects an integer, got {value}"),
    )
}

pub fn select_targets_missing(language: Language) -> &'static str {
    tr(
        language,
        "--targets 必须包含至少一个目标 ID",
        "--targets must include at least one target id",
    )
}

pub fn select_targets_unknown(language: Language, unknown: &str, valid: &str) -> String {
    tr_owned(
        language,
        format!("未知目标：{unknown}。可用目标：{valid}"),
        format!("unknown target(s): {unknown}. Valid targets: {valid}"),
    )
}

pub fn language_option_error(value: &str) -> String {
    format!("unknown language: {value}. Supported: zh, en")
}

pub fn current_language_label(language: Language) -> &'static str {
    tr(language, "当前语言", "Current language")
}

pub fn targets_page_title(language: Language) -> &'static str {
    tr(language, "目标页", "Targets")
}

pub fn settings_page_title(language: Language) -> &'static str {
    tr(language, "设置页", "Settings")
}

pub fn settings_page_hint(language: Language) -> &'static str {
    tr(
        language,
        "Tab 返回目标页 | Enter 编辑 | +/- 调整 | r 恢复默认值。",
        "Tab back | Enter edit | +/- adjust | r reset.",
    )
}

pub fn target_threshold_label(language: Language) -> &'static str {
    tr(language, "阈值", "Threshold")
}

pub fn setting_prompt(language: Language, label: &str, current: &str) -> String {
    tr_owned(
        language,
        format!("输入新的 {label}（当前：{current}）："),
        format!("Enter new {label} (current: {current}): "),
    )
}

pub fn setting_saved_message(language: Language) -> &'static str {
    tr(language, "设置已更新。", "Setting updated.")
}

pub fn settings_reset_message(language: Language) -> &'static str {
    tr(
        language,
        "设置已恢复为默认值。",
        "Settings reset to defaults.",
    )
}

pub fn invalid_setting_value(language: Language, label: &str, value: &str) -> String {
    tr_owned(
        language,
        format!("{label} 无法解析：{value}"),
        format!("Could not parse {label}: {value}"),
    )
}

pub fn invalid_size_value(language: Language, label: &str, value: &str) -> String {
    tr_owned(
        language,
        format!("{label} 需要形如 500M / 1G 的大小，但收到 {value}"),
        format!("{label} expects a size like 500M / 1G, got {value}"),
    )
}

pub fn open_settings_hint(language: Language) -> &'static str {
    tr(language, "按 Tab 进入设置页", "Press Tab to open settings")
}

pub fn return_targets_hint(language: Language) -> &'static str {
    tr(
        language,
        "按 Tab 返回目标页",
        "Press Tab to return to targets",
    )
}

pub fn control_l_hint(language: Language) -> &'static str {
    tr(language, "Ctrl+L 切换语言", "Ctrl+L switches languages")
}

#[cfg(test)]
mod tests {
    use super::{Language, help_text, parse_language};
    use crate::model::CleanerOptions;
    use crate::rules::all_targets;

    #[test]
    fn parses_language_codes() {
        assert_eq!(parse_language("zh").unwrap(), Language::ZhCn);
        assert_eq!(parse_language("en").unwrap(), Language::En);
    }

    #[test]
    fn toggles_language() {
        assert_eq!(Language::ZhCn.toggle(), Language::En);
        assert_eq!(Language::En.toggle(), Language::ZhCn);
    }

    #[test]
    fn help_lists_every_registered_target() {
        let ids = all_targets(&CleanerOptions::default())
            .iter()
            .map(|target| target.id)
            .collect::<Vec<_>>()
            .join(", ");

        assert!(help_text(Language::ZhCn, "0.1.0").contains(&ids));
        assert!(help_text(Language::En, "0.1.0").contains(&ids));
    }

    #[test]
    fn builds_localized_help_text() {
        let help = help_text(Language::ZhCn, "0.1.0");
        assert!(help.contains("Ctrl+L 切换语言"));
        assert!(help.contains("-V, --version"));
        let help = help_text(Language::En, "0.1.0");
        assert!(help.contains("UI language"));
        assert!(help.contains("-V, --version"));
    }
}
