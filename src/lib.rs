pub mod cli;
pub mod executor;
pub mod i18n;
pub mod json;
pub mod model;
pub mod platform;
pub mod tui;

// 清理规则本体在仓库根的 rules/ 目录（见 rules/README.md），通过 #[path]
// 挂进 crate，让模块名直接体现归属，而不是伪装成 src 的一部分。
#[path = "../rules/mod.rs"]
pub mod rules;

pub use model::{CleanerOptions, CleanupCommand, CleanupTarget, RiskLevel, ScanReport, ScanStatus};
pub use rules::{all_targets, scan_all, scan_target};
