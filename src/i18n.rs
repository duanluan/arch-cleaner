use crate::executor::ExecutionMode;
use crate::model::{RiskLevel, ScanStatus, TargetGroup};

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
        RiskLevel::High => tr(language, "高", "high"),
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

pub fn no_items_selected(language: Language) -> &'static str {
    tr(language, "没有勾选任何条目。", "No items selected.")
}

pub fn picker_only_hint(language: Language, ids: &str) -> String {
    tr_owned(
        language,
        format!("以下目标不整批删除，请在扫描结果页逐项勾选后清理：{ids}"),
        format!(
            "These targets have no bulk delete; clean them entry by entry in the scan results page: {ids}"
        ),
    )
}

pub fn results_help_line(language: Language) -> &'static str {
    tr(
        language,
        "方向键移动 | 空格勾选 | c 清理勾选项 | a/n 全选/全不选 | Tab 返回",
        "Arrows move | Space toggle | c clean selected | a/n all/none | Tab back",
    )
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
    use super::{Language, parse_language};

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
}
