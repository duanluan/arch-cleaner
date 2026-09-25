use std::process::{Command, Stdio};

use crate::model::{CleanupCommand, CleanupTarget};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionMode {
    DryRun,
    Apply,
}

impl ExecutionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DryRun => "dry-run",
            Self::Apply => "apply",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionOptions {
    pub mode: ExecutionMode,
    pub run_readonly_checks: bool,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            mode: ExecutionMode::DryRun,
            run_readonly_checks: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionResult {
    pub target_id: String,
    pub command: String,
    pub status: CommandStatus,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandStatus {
    Planned,
    Success(i32),
    Failed(i32),
    CouldNotStart(String),
}

pub fn commands_for_target(target: &CleanupTarget, mode: ExecutionMode) -> &[CleanupCommand] {
    match mode {
        ExecutionMode::DryRun => &target.dry_run_commands,
        ExecutionMode::Apply => &target.apply_commands,
    }
}

pub fn execute_targets(
    targets: &[CleanupTarget],
    options: ExecutionOptions,
) -> Vec<ExecutionResult> {
    run_targets(targets, options, run_command)
}

pub fn execute_targets_captured(
    targets: &[CleanupTarget],
    options: ExecutionOptions,
) -> Vec<ExecutionResult> {
    run_targets(targets, options, run_command_captured)
}

fn run_targets(
    targets: &[CleanupTarget],
    options: ExecutionOptions,
    run: fn(&str, &CleanupCommand) -> ExecutionResult,
) -> Vec<ExecutionResult> {
    let mut results = Vec::new();

    for target in targets {
        for command in commands_for_target(target, options.mode) {
            if options.mode == ExecutionMode::DryRun && !options.run_readonly_checks {
                results.push(planned_result(target.id, command));
                continue;
            }

            results.push(run(target.id, command));
        }
    }

    results
}

pub fn run_command(target_id: &str, command: &CleanupCommand) -> ExecutionResult {
    let status = match Command::new(&command.program)
        .args(&command.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
    {
        Ok(status) if status.success() => CommandStatus::Success(status.code().unwrap_or(0)),
        Ok(status) => CommandStatus::Failed(status.code().unwrap_or(1)),
        Err(error) => CommandStatus::CouldNotStart(error.to_string()),
    };

    ExecutionResult {
        target_id: target_id.to_string(),
        command: command.display.clone(),
        status,
        stdout: String::new(),
        stderr: String::new(),
    }
}

pub fn run_command_captured(target_id: &str, command: &CleanupCommand) -> ExecutionResult {
    match Command::new(&command.program)
        .args(&command.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
    {
        Ok(output) => ExecutionResult {
            target_id: target_id.to_string(),
            command: command.display.clone(),
            status: if output.status.success() {
                CommandStatus::Success(output.status.code().unwrap_or(0))
            } else {
                CommandStatus::Failed(output.status.code().unwrap_or(1))
            },
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
        Err(error) => ExecutionResult {
            target_id: target_id.to_string(),
            command: command.display.clone(),
            status: CommandStatus::CouldNotStart(error.to_string()),
            stdout: String::new(),
            stderr: String::new(),
        },
    }
}

fn planned_result(target_id: &str, command: &CleanupCommand) -> ExecutionResult {
    ExecutionResult {
        target_id: target_id.to_string(),
        command: command.display.clone(),
        status: CommandStatus::Planned,
        stdout: String::new(),
        stderr: String::new(),
    }
}

pub fn any_failed(results: &[ExecutionResult]) -> bool {
    results.iter().any(|result| {
        matches!(
            result.status,
            CommandStatus::Failed(_) | CommandStatus::CouldNotStart(_)
        )
    })
}
