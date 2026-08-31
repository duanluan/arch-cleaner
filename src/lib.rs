pub mod cli;
pub mod executor;
pub mod model;
pub mod platform;
pub mod targets;
pub mod tui;

pub use model::{CleanerOptions, CleanupCommand, CleanupTarget, RiskLevel, ScanReport, ScanStatus};
pub use targets::{all_targets, scan_all, scan_target};
