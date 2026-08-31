use std::io;
use std::process::{Command, Stdio};

use crate::model::{CleanupCommand, CleanupTarget};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionMode {
    DryRun,
    Apply,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionOptions {
    pub mode: ExecutionMode,
    pub yes: bool,
    pub run_readonly_checks: bool,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            mode: ExecutionMode::DryRun,
            yes: false,
            run_readonly_checks: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionResult {
    pub target_id: String,
    pub command: String,
    pub status: CommandStatus,
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
    let mut results = Vec::new();

    for target in targets {
        for command in commands_for_target(target, options.mode) {
            if options.mode == ExecutionMode::DryRun && !options.run_readonly_checks {
                results.push(ExecutionResult {
                    target_id: target.id.to_string(),
                    command: command.display.clone(),
                    status: CommandStatus::Planned,
                });
                continue;
            }

            results.push(run_command(target.id, command));
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

pub fn confirm(prompt: &str) -> io::Result<bool> {
    use std::io::Write;

    print!("{prompt} [y/N] ");
    io::stdout().flush()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;

    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "YES" | "Yes"))
}
