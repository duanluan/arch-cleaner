use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanerOptions {
    pub keep_package_versions: u8,
    pub journal_days: u16,
    pub journal_size: String,
    pub temp_min_age_days: u16,
    pub user_cache_min_age_days: u16,
}

impl Default for CleanerOptions {
    fn default() -> Self {
        Self {
            keep_package_versions: 3,
            journal_days: 14,
            journal_size: "1G".to_string(),
            temp_min_age_days: 7,
            user_cache_min_age_days: 30,
        }
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupTarget {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub risk: RiskLevel,
    pub requires_sudo: bool,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanReport {
    pub target_id: String,
    pub title: String,
    pub status: ScanStatus,
    pub estimated_bytes: Option<u64>,
    pub details: Vec<String>,
    pub warnings: Vec<String>,
}

impl ScanReport {
    pub fn new(target: &CleanupTarget) -> Self {
        Self {
            target_id: target.id.to_string(),
            title: target.title.to_string(),
            status: ScanStatus::Ready,
            estimated_bytes: None,
            details: Vec::new(),
            warnings: Vec::new(),
        }
    }
}
