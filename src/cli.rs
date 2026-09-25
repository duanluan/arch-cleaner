use std::collections::HashSet;
use std::io::{self, Write};

use crate::executor::{
    CommandStatus, ExecutionMode, ExecutionOptions, any_failed, commands_for_target,
    execute_targets, execute_targets_captured,
};
use crate::i18n::{self, Language};
use crate::json as json_output;
use crate::model::{CleanerOptions, CleanupTarget, ScanReport, ScanStatus, TargetGroup};
use crate::platform::format_bytes;
use crate::rules::{all_targets, is_valid_journal_size, scan_all};
use crate::tui;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct RunOptions {
    cleaner: CleanerOptions,
    targets: Option<String>,
    apply: bool,
    yes: bool,
    run_readonly_checks: bool,
    json: bool,
}

pub fn run_from_env() -> Result<i32, String> {
    run(std::env::args().skip(1))
}

pub fn run<I, S>(args: I) -> Result<i32, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    let (language, args) = extract_language(args)?;

    if args.is_empty() {
        return tui::run(language);
    }

    match args[0].as_str() {
        "-h" | "--help" | "help" => {
            print_help(language);
            Ok(0)
        }
        "-V" | "--version" | "version" => {
            println!("arch-cleaner {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        "tui" => tui::run(language),
        "list-targets" => {
            let options = parse_run_options(&args[1..], language)?;
            let targets = all_targets(&options.cleaner);
            if options.json {
                println!(
                    "{}",
                    json_output::targets(&targets, &options.cleaner, language)
                );
            } else {
                print_targets(&targets, &options.cleaner, language);
            }
            Ok(0)
        }
        "scan" => {
            let options = parse_run_options(&args[1..], language)?;
            let targets = all_targets(&options.cleaner);
            let selected = select_targets(&targets, options.targets.as_deref(), language)?;
            let reports = scan_all(&selected, &options.cleaner, language);

            if options.json {
                println!(
                    "{}",
                    json_output::scan_reports(&reports, &options.cleaner, language)
                );
            } else {
                print_scan_reports(&reports, language);
            }

            Ok(0)
        }
        "clean" => {
            let options = parse_run_options(&args[1..], language)?;
            let targets = all_targets(&options.cleaner);
            let selected = select_targets(&targets, options.targets.as_deref(), language)?;
            let mode = if options.apply {
                ExecutionMode::Apply
            } else {
                ExecutionMode::DryRun
            };

            if options.json {
                return run_clean_json(&selected, mode, &options, language);
            }

            print_plan(&selected, mode, &options.cleaner, language);

            if !options.apply {
                println!("\n{}", i18n::dry_run_notice(language));

                if options.run_readonly_checks {
                    println!("\n{}\n", i18n::running_readonly_checks(language));
                    let results = execute_targets(
                        &selected,
                        ExecutionOptions {
                            mode,
                            run_readonly_checks: true,
                        },
                    );
                    print_results(&results, language);
                    return Ok(if any_failed(&results) { 1 } else { 0 });
                }

                return Ok(0);
            }

            if !options.yes && !confirm_apply(language)? {
                println!("{}", i18n::aborted(language));
                return Ok(1);
            }

            println!("\n{}\n", i18n::executing_cleanup_plan(language));
            let results = execute_targets(
                &selected,
                ExecutionOptions {
                    mode,
                    run_readonly_checks: true,
                },
            );
            print_results(&results, language);

            Ok(if any_failed(&results) { 1 } else { 0 })
        }
        unknown => Err(i18n::unknown_command(language, unknown)),
    }
}

/// 把 `--targets` 这类 `--flag=value` 写法拆成两个参数，主循环就只需要
/// 维护一份 `--flag value` 形式的清单。
fn split_inline_values(args: &[String]) -> Vec<String> {
    const VALUE_FLAGS: [&str; 7] = [
        "--targets",
        "--keep-packages",
        "--journal-days",
        "--journal-size",
        "--temp-days",
        "--user-cache-days",
        "--ai-agent-days",
    ];

    let mut expanded = Vec::with_capacity(args.len() + 2);
    for arg in args {
        // 空值（`--targets=`）也拆开，让主循环按"缺值/坏值"报错，
        // 而不是把 `--targets=` 整个当成未知选项。
        if let Some((flag, value)) = arg.split_once('=')
            && VALUE_FLAGS.contains(&flag)
        {
            expanded.push(flag.to_string());
            expanded.push(value.to_string());
            continue;
        }
        expanded.push(arg.clone());
    }
    expanded
}

/// 按 `--targets` 的取值筛选清理目标；未知目标直接报错并列出可用项。
/// 这是 CLI 输入解析，所以放在 cli 而不是 rules。
fn select_targets(
    targets: &[CleanupTarget],
    requested: Option<&str>,
    language: Language,
) -> Result<Vec<CleanupTarget>, String> {
    let Some(requested) = requested else {
        return Ok(targets.to_vec());
    };

    if requested.trim().eq_ignore_ascii_case("all") {
        return Ok(targets.to_vec());
    }

    let requested_ids: Vec<&str> = requested
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect();

    if requested_ids.is_empty() {
        return Err(i18n::select_targets_missing(language).to_string());
    }

    let known_ids: HashSet<&str> = targets.iter().map(|target| target.id).collect();
    let unknown_ids: Vec<&str> = requested_ids
        .iter()
        .copied()
        .filter(|id| !known_ids.contains(id))
        .collect();

    if !unknown_ids.is_empty() {
        let mut valid_ids: Vec<&str> = known_ids.into_iter().collect();
        valid_ids.sort_unstable();
        return Err(i18n::select_targets_unknown(
            language,
            &unknown_ids.join(", "),
            &valid_ids.join(", "),
        ));
    }

    Ok(targets
        .iter()
        .filter(|target| requested_ids.contains(&target.id))
        .cloned()
        .collect())
}

pub fn print_targets(
    targets: &[CleanupTarget],
    options: &crate::model::CleanerOptions,
    language: Language,
) {
    for group in TargetGroup::ALL {
        let group_targets: Vec<&CleanupTarget> = targets
            .iter()
            .filter(|target| target.group == group)
            .collect();
        if group_targets.is_empty() {
            continue;
        }

        println!("{}", i18n::target_group_label(language, group));
        for target in group_targets {
            let title = target.title.get(language);
            let description = target.description.get(language);
            let risk = i18n::risk_label(language, target.risk);
            let scope = i18n::scope_label(language, target.requires_sudo);
            let threshold = (target.threshold_summary)(options, language);
            println!(
                "  {:<17} {:<24} {}: {:<6} {}: {}",
                target.id,
                title,
                if matches!(language, Language::ZhCn) {
                    "风险"
                } else {
                    "risk"
                },
                risk,
                if matches!(language, Language::ZhCn) {
                    "范围"
                } else {
                    "scope"
                },
                scope
            );
            println!(
                "    {}: {}",
                i18n::target_threshold_label(language),
                threshold
            );
            println!("    {}", description);
        }
    }
}

pub fn print_scan_reports(reports: &[ScanReport], language: Language) {
    for report in reports {
        print_scan_report(report, language);
    }

    if !reports.is_empty() {
        print_scan_summary(reports, language);
    }
}

pub fn print_scan_report(report: &ScanReport, language: Language) {
    let title = report.title.get(language);
    println!(
        "{} ({})",
        title,
        i18n::scan_status_label(language, report.status)
    );

    if let Some(bytes) = report.estimated_bytes {
        println!(
            "  {}: {}",
            i18n::tr(language, "预估可清理大小", "Estimated cleanable size"),
            format_bytes(bytes)
        );
    }

    for detail in &report.details {
        println!("  - {detail}");
    }

    for warning in &report.warnings {
        println!("  ! {warning}");
    }

    println!();
}

fn print_scan_summary(reports: &[ScanReport], language: Language) {
    let total_bytes = reports
        .iter()
        .filter_map(|report| report.estimated_bytes)
        .fold(0u64, |total, bytes| total.saturating_add(bytes));
    let has_bytes = reports
        .iter()
        .any(|report| report.estimated_bytes.is_some());
    let total_items = reports
        .iter()
        .filter_map(|report| report.estimated_items)
        .fold(0usize, |total, items| total.saturating_add(items));
    let has_items = reports
        .iter()
        .any(|report| report.estimated_items.is_some());
    let ready = reports
        .iter()
        .filter(|report| report.status == ScanStatus::Ready)
        .count();
    let missing = reports
        .iter()
        .filter(|report| report.status == ScanStatus::MissingTool)
        .count();
    let unavailable = reports
        .iter()
        .filter(|report| report.status == ScanStatus::Unavailable)
        .count();

    println!("{}", i18n::tr(language, "扫描汇总", "Scan summary"));
    if has_bytes {
        println!(
            "  {}: {}",
            i18n::tr(
                language,
                "预估可清理总大小",
                "Estimated total cleanable size"
            ),
            format_bytes(total_bytes)
        );
    }
    if has_items {
        println!(
            "  {}: {}",
            i18n::tr(language, "已知候选项", "Known eligible entries"),
            total_items
        );
    }
    println!(
        "  {}: {} {}, {} {}, {} {}",
        i18n::tr(language, "状态", "Status"),
        ready,
        i18n::scan_status_label(language, ScanStatus::Ready),
        missing,
        i18n::scan_status_label(language, ScanStatus::MissingTool),
        unavailable,
        i18n::scan_status_label(language, ScanStatus::Unavailable)
    );
    println!();
}

pub fn print_plan(
    targets: &[CleanupTarget],
    mode: ExecutionMode,
    options: &crate::model::CleanerOptions,
    language: Language,
) {
    println!("{}:", i18n::execution_mode_label(language, mode));

    for target in targets {
        let title = target.title.get(language);
        let risk = i18n::risk_label(language, target.risk);
        let threshold = (target.threshold_summary)(options, language);
        println!(
            "\n{} [{}: {}, {}: {}]",
            title,
            if matches!(language, Language::ZhCn) {
                "风险"
            } else {
                "risk"
            },
            risk,
            i18n::target_threshold_label(language),
            threshold
        );
        for command in commands_for_target(target, mode) {
            println!("  {}", command.display);
        }
    }
}

pub fn print_results(results: &[crate::executor::ExecutionResult], language: Language) {
    for result in results {
        match &result.status {
            CommandStatus::Planned => println!(
                "{}: {}",
                i18n::tr(language, "已计划", "planned"),
                result.command
            ),
            CommandStatus::Success(code) => println!(
                "{}({code}): {}",
                i18n::tr(language, "成功", "ok"),
                result.command
            ),
            CommandStatus::Failed(code) => println!(
                "{}({code}): {}",
                i18n::tr(language, "失败", "failed"),
                result.command
            ),
            CommandStatus::CouldNotStart(error) => println!(
                "{}: {} ({error})",
                i18n::tr(language, "无法启动", "could not start"),
                result.command
            ),
        }
    }

    if !results.is_empty() {
        print_result_summary(results, language);
    }
}

fn print_result_summary(results: &[crate::executor::ExecutionResult], language: Language) {
    let planned = results
        .iter()
        .filter(|result| matches!(result.status, CommandStatus::Planned))
        .count();
    let succeeded = results
        .iter()
        .filter(|result| matches!(result.status, CommandStatus::Success(_)))
        .count();
    let failed = results
        .iter()
        .filter(|result| matches!(result.status, CommandStatus::Failed(_)))
        .count();
    let could_not_start = results
        .iter()
        .filter(|result| matches!(result.status, CommandStatus::CouldNotStart(_)))
        .count();

    println!(
        "\n{}: {} {}, {} {}, {} {}, {} {}",
        i18n::tr(language, "结果汇总", "Result summary"),
        planned,
        i18n::tr(language, "已计划", "planned"),
        succeeded,
        i18n::tr(language, "成功", "ok"),
        failed,
        i18n::tr(language, "失败", "failed"),
        could_not_start,
        i18n::tr(language, "无法启动", "could not start")
    );
}

fn run_clean_json(
    selected: &[CleanupTarget],
    mode: ExecutionMode,
    options: &RunOptions,
    language: Language,
) -> Result<i32, String> {
    if mode == ExecutionMode::Apply && !options.yes {
        return Err(i18n::tr(
            language,
            "clean --json --apply 需要同时传 --yes，才能保持 stdout 只输出一段 JSON。",
            "clean --json --apply requires --yes to keep stdout valid JSON.",
        )
        .to_string());
    }

    let results = execute_targets_captured(
        selected,
        ExecutionOptions {
            mode,
            run_readonly_checks: options.run_readonly_checks,
        },
    );
    let executed = mode == ExecutionMode::Apply || options.run_readonly_checks;

    println!(
        "{}",
        json_output::clean_result(
            selected,
            mode,
            executed,
            &results,
            &options.cleaner,
            language
        )
    );

    Ok(if any_failed(&results) { 1 } else { 0 })
}

fn parse_run_options(raw_args: &[String], language: Language) -> Result<RunOptions, String> {
    let args = split_inline_values(raw_args);
    let mut options = RunOptions::default();
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();

        match arg {
            "--targets" => {
                index += 1;
                options.targets =
                    Some(require_value(&args, index, "--targets", language)?.to_string());
            }
            "--keep-packages" => {
                index += 1;
                options.cleaner.keep_package_versions = parse_u8(
                    "--keep-packages",
                    require_value(&args, index, "--keep-packages", language)?,
                    language,
                )?;
            }
            "--journal-days" => {
                index += 1;
                options.cleaner.journal_days = parse_u16(
                    "--journal-days",
                    require_value(&args, index, "--journal-days", language)?,
                    language,
                )?;
            }
            "--journal-size" => {
                index += 1;
                options.cleaner.journal_size = parse_journal_size(
                    "--journal-size",
                    require_value(&args, index, "--journal-size", language)?,
                    language,
                )?;
            }
            "--temp-days" => {
                index += 1;
                options.cleaner.temp_min_age_days = parse_u16(
                    "--temp-days",
                    require_value(&args, index, "--temp-days", language)?,
                    language,
                )?;
            }
            "--user-cache-days" => {
                index += 1;
                options.cleaner.user_cache_min_age_days = parse_u16(
                    "--user-cache-days",
                    require_value(&args, index, "--user-cache-days", language)?,
                    language,
                )?;
            }
            "--ai-agent-days" => {
                index += 1;
                options.cleaner.ai_agent_min_age_days = parse_u16(
                    "--ai-agent-days",
                    require_value(&args, index, "--ai-agent-days", language)?,
                    language,
                )?;
            }
            "--apply" => options.apply = true,
            "--yes" | "-y" => options.yes = true,
            "--run-readonly-checks" => options.run_readonly_checks = true,
            "--json" => options.json = true,
            "-h" | "--help" => {
                print_help(language);
                std::process::exit(0);
            }
            unknown => return Err(i18n::unknown_option(language, unknown)),
        }

        index += 1;
    }

    Ok(options)
}

fn extract_language(args: Vec<String>) -> Result<(Language, Vec<String>), String> {
    let mut language = Language::default();
    let mut filtered = Vec::with_capacity(args.len());
    let mut index = 0usize;

    while index < args.len() {
        let arg = &args[index];

        if let Some(value) = arg.strip_prefix("--lang=") {
            language =
                i18n::parse_language(value).map_err(|_| i18n::language_option_error(value))?;
        } else if matches!(arg.as_str(), "--lang" | "-l") {
            index += 1;
            let value = require_language_value(&args, index)?;
            language =
                i18n::parse_language(value).map_err(|_| i18n::language_option_error(value))?;
        } else {
            filtered.push(arg.clone());
        }

        index += 1;
    }

    Ok((language, filtered))
}

fn require_language_value(args: &[String], index: usize) -> Result<&str, String> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| i18n::missing_value(Language::default(), "--lang"))
}

fn require_value<'a>(
    args: &'a [String],
    index: usize,
    flag: &str,
    language: Language,
) -> Result<&'a str, String> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| i18n::missing_value(language, flag))
}

pub(crate) fn parse_u8(flag: &str, value: &str, language: Language) -> Result<u8, String> {
    value
        .parse::<u8>()
        .map_err(|_| i18n::invalid_integer(language, flag, value))
}

pub(crate) fn parse_u16(flag: &str, value: &str, language: Language) -> Result<u16, String> {
    value
        .parse::<u16>()
        .map_err(|_| i18n::invalid_integer(language, flag, value))
}

fn parse_journal_size(flag: &str, value: &str, language: Language) -> Result<String, String> {
    if is_valid_journal_size(value) {
        Ok(value.to_string())
    } else {
        Err(i18n::invalid_size_value(language, flag, value))
    }
}

fn confirm_apply(language: Language) -> Result<bool, String> {
    print!("\n{} ", i18n::confirm_apply_prompt(language));
    io::stdout()
        .flush()
        .map_err(|error| format!("could not flush stdout: {error}"))?;

    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|error| format!("could not read confirmation: {error}"))?;

    Ok(answer.trim() == "APPLY")
}

/// CLI 帮助文案放在 cli 而不是 i18n：目标清单从注册表生成，
/// 帮助内容随命令行选项演进，i18n 不需要反向依赖 rules。
fn help_text(language: Language, version: &str) -> String {
    let targets = all_targets(&CleanerOptions::default())
        .iter()
        .map(|target| target.id)
        .collect::<Vec<_>>()
        .join(", ");
    match language {
        Language::ZhCn => format!(
            "arch-cleaner {version}\n\n用法:\n    arch-cleaner                  启动交互式 TUI 菜单\n    arch-cleaner tui              启动交互式 TUI 菜单\n    arch-cleaner list-targets     显示清理目标\n    arch-cleaner scan [OPTIONS]   检查选中的目标\n    arch-cleaner clean [OPTIONS]  显示或执行清理计划\n\n选项:\n    -V, --version                 显示版本号\n    --lang, -l <zh|en>            界面语言 [默认: zh]\n    --targets <ids>               以逗号分隔的目标 ID，或 all\n    --apply                       执行清理命令\n    --yes, -y                     跳过 --apply 的确认提示\n    --run-readonly-checks         在 dry-run 模式下运行只读命令\n    --json                        输出机器可读 JSON\n    --keep-packages <n>           Pacman 包版本保留数量 [默认: 3]\n    --journal-days <n>            日志清理天数阈值 [默认: 14]\n    --journal-size <size>         日志清理大小阈值 [默认: 1G]\n    --temp-days <n>               临时文件保留天数 [默认: 7]\n    --user-cache-days <n>         用户缓存保留天数 [默认: 30]\n    --ai-agent-days <n>           AI agent 缓存保留天数 [默认: 30]\n\n说明:\n    在 TUI 中按 Tab 进入设置页，按 Ctrl+L 切换语言。\n\n目标:\n    {targets}"
        ),
        Language::En => format!(
            "arch-cleaner {version}\n\nUSAGE:\n    arch-cleaner                  Start the interactive TUI menu\n    arch-cleaner tui              Start the interactive TUI menu\n    arch-cleaner list-targets     Show cleanup targets\n    arch-cleaner scan [OPTIONS]   Inspect selected targets\n    arch-cleaner clean [OPTIONS]  Show or execute a cleanup plan\n\nOPTIONS:\n    -V, --version                 Print version\n    --lang, -l <zh|en>            UI language [default: zh]\n    --targets <ids>               Comma-separated target ids, or all\n    --apply                       Execute cleanup commands\n    --yes, -y                     Skip confirmation prompts for --apply\n    --run-readonly-checks         In dry-run mode, run read-only commands\n    --json                        Print machine-readable JSON\n    --keep-packages <n>           Pacman package versions to keep [default: 3]\n    --journal-days <n>            Journal age vacuum threshold [default: 14]\n    --journal-size <size>         Journal size vacuum threshold [default: 1G]\n    --temp-days <n>               Temp file age threshold [default: 7]\n    --user-cache-days <n>         User cache age threshold [default: 30]\n    --ai-agent-days <n>           AI agent cache age threshold [default: 30]\n\nNOTES:\n    Press Tab in the TUI to open settings and Ctrl+L to switch languages.\n\nTARGETS:\n    {targets}"
        ),
    }
}

fn print_help(language: Language) {
    println!("{}", help_text(language, env!("CARGO_PKG_VERSION")));
}

#[cfg(test)]
mod tests {
    use super::run;
    use crate::i18n::Language;
    use crate::model::CleanerOptions;
    use crate::rules::all_targets;

    #[test]
    fn splits_inline_value_flags() {
        let options =
            super::parse_run_options(&["--journal-size=2G".to_string()], Language::En).unwrap();

        assert_eq!(options.cleaner.journal_size, "2G");
    }

    #[test]
    fn rejects_invalid_journal_size_at_parse_time() {
        let error = super::parse_run_options(
            &["--journal-size".to_string(), "wat".to_string()],
            Language::En,
        )
        .unwrap_err();

        assert!(error.contains("--journal-size"));
    }

    #[test]
    fn selects_comma_separated_targets() {
        let targets = all_targets(&CleanerOptions::default());
        let selected =
            super::select_targets(&targets, Some("pacman-cache,systemd-journal"), Language::En)
                .unwrap();

        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].id, "pacman-cache");
        assert_eq!(selected[1].id, "systemd-journal");
    }

    #[test]
    fn rejects_unknown_targets() {
        let targets = all_targets(&CleanerOptions::default());
        let error = super::select_targets(&targets, Some("wat"), Language::En).unwrap_err();

        assert!(error.contains("unknown target"));
    }

    #[test]
    fn help_lists_every_registered_target() {
        let ids = all_targets(&CleanerOptions::default())
            .iter()
            .map(|target| target.id)
            .collect::<Vec<_>>()
            .join(", ");
        let help = super::help_text(Language::ZhCn, "0.1.0");
        assert!(help.contains(&ids) && help.contains("-V, --version"));

        let help = super::help_text(Language::En, "0.1.0");
        assert!(help.contains(&ids) && help.contains("UI language"));
    }

    #[test]
    fn help_exits_successfully() {
        assert_eq!(run(["--help"]).unwrap(), 0);
    }

    #[test]
    fn parses_json_option() {
        let options = super::parse_run_options(&["--json".to_string()], Language::En).unwrap();

        assert!(options.json);
    }

    #[test]
    fn parses_ai_agent_days_option() {
        let options =
            super::parse_run_options(&["--ai-agent-days=12".to_string()], Language::En).unwrap();

        assert_eq!(options.cleaner.ai_agent_min_age_days, 12);
    }

    #[test]
    fn json_apply_requires_yes() {
        let error = run([
            "-l",
            "en",
            "clean",
            "--json",
            "--apply",
            "--targets",
            "pacman-cache",
        ])
        .unwrap_err();

        assert!(error.contains("--yes"));
    }

    #[test]
    fn clean_json_dry_run_exits_successfully() {
        assert_eq!(
            run(["-l", "en", "clean", "--json", "--targets", "pacman-cache"]).unwrap(),
            0
        );
    }

    #[test]
    fn unknown_command_is_error() {
        let error = run(["-l", "en", "wat"]).unwrap_err();
        assert!(error.contains("unknown command"));
    }
}
