use std::fmt;
use std::path::PathBuf;

use crate::i18n::Language;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanerOptions {
    pub keep_package_versions: u8,
    pub journal_days: u16,
    pub journal_size: String,
    pub temp_min_age_days: u16,
    pub user_cache_min_age_days: u16,
    pub ai_agent_min_age_days: u16,
    pub downloads_min_age_days: u16,
    pub large_file_min_size: String,
    pub duplicate_min_size: String,
}

/// 中英双语文案。规则自带全部语言版本，i18n 模块不再按 id 二次维护。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalizedText {
    pub zh_cn: &'static str,
    pub en: &'static str,
}

impl LocalizedText {
    pub fn get(self, language: Language) -> &'static str {
        match language {
            Language::ZhCn => self.zh_cn,
            Language::En => self.en,
        }
    }
}

impl Default for CleanerOptions {
    fn default() -> Self {
        Self {
            keep_package_versions: 3,
            journal_days: 14,
            journal_size: "1G".to_string(),
            temp_min_age_days: 7,
            user_cache_min_age_days: 30,
            ai_agent_min_age_days: 30,
            downloads_min_age_days: 90,
            large_file_min_size: "500M".to_string(),
            duplicate_min_size: "1M".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetGroup {
    Packages,
    System,
    User,
    Developer,
}

impl TargetGroup {
    pub const ALL: [Self; 4] = [Self::Packages, Self::System, Self::User, Self::Developer];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Packages => "packages",
            Self::System => "system",
            Self::User => "user",
            Self::Developer => "developer",
        }
    }
}

impl fmt::Display for TargetGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupCommand {
    pub display: String,
    pub program: String,
    pub args: Vec<String>,
    pub needs_sudo: bool,
}

impl CleanupCommand {
    pub fn new(
        display: impl Into<String>,
        program: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
        needs_sudo: bool,
    ) -> Self {
        Self {
            display: display.into(),
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            needs_sudo,
        }
    }

    pub fn shell(display: impl Into<String>, script: impl Into<String>, needs_sudo: bool) -> Self {
        Self::new(display, "sh", ["-c".to_string(), script.into()], needs_sudo)
    }
}

/// 规则自带的扫描函数：按规则自己的方式估算可清理内容。
pub type ScanFn = fn(&CleanupTarget, &CleanerOptions, Language) -> ScanReport;

/// 规则自带的阈值摘要：用当前选项渲染一句话说明。
pub type ThresholdFn = fn(&CleanerOptions, Language) -> String;

// 不派生 Eq/PartialEq：字段里有函数指针，地址比较没有意义，也无人使用。
#[derive(Clone, Debug)]
pub struct CleanupTarget {
    pub id: &'static str,
    pub title: LocalizedText,
    pub description: LocalizedText,
    pub group: TargetGroup,
    pub risk: RiskLevel,
    pub requires_sudo: bool,
    pub threshold_summary: ThresholdFn,
    pub scan: ScanFn,
    pub dry_run_commands: Vec<CleanupCommand>,
    pub apply_commands: Vec<CleanupCommand>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScanStatus {
    Ready,
    MissingTool,
    Unavailable,
}

impl fmt::Display for ScanStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ready => write!(f, "ready"),
            Self::MissingTool => write!(f, "missing-tool"),
            Self::Unavailable => write!(f, "unavailable"),
        }
    }
}

/// 扫描发现的单个可清理条目（一个顶层目录或文件）。
/// 文件选择器以它为数据源，clean 只删除用户勾选的条目。
#[derive(Clone, Debug)]
pub struct ScanItem {
    pub path: PathBuf,
    pub bytes: u64,
    pub entries: usize,
}

#[derive(Clone, Debug)]
pub struct ScanReport {
    pub target_id: String,
    pub title: LocalizedText,
    pub group: TargetGroup,
    pub threshold_summary: ThresholdFn,
    pub status: ScanStatus,
    pub estimated_bytes: Option<u64>,
    pub estimated_items: Option<usize>,
    pub items: Vec<ScanItem>,
    pub details: Vec<String>,
    pub warnings: Vec<String>,
}

impl ScanReport {
    pub fn new(target: &CleanupTarget) -> Self {
        Self {
            target_id: target.id.to_string(),
            title: target.title,
            group: target.group,
            threshold_summary: target.threshold_summary,
            status: ScanStatus::Ready,
            estimated_bytes: None,
            estimated_items: None,
            items: Vec::new(),
            details: Vec::new(),
            warnings: Vec::new(),
        }
    }
}
