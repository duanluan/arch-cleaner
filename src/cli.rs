use std::io::{self, Write};

use crate::executor::{
    CommandStatus, ExecutionMode, ExecutionOptions, any_failed, commands_for_target,
    execute_targets,
};
use crate::model::{CleanerOptions, CleanupTarget, ScanReport};
use crate::platform::format_bytes;
use crate::targets::{all_targets, scan_all, select_targets};
use crate::tui;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct RunOptions {
    cleaner: CleanerOptions,
    targets: Option<String>,
    apply: bool,
    yes: bool,
    run_readonly_checks: bool,
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

    if args.is_empty() {
        return tui::run();
    }

    match args[0].as_str() {
        "-h" | "--help" | "help" => {
            print_help();
            Ok(0)
        }
        "-V" | "--version" | "version" => {
            println!("arch-cleaner {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        "tui" => tui::run(),
        "list-targets" => {
            let options = parse_run_options(&args[1..])?;
            let targets = all_targets(&options.cleaner);
            print_targets(&targets);
            Ok(0)
        }
        "scan" => {
            let options = parse_run_options(&args[1..])?;
            let targets = all_targets(&options.cleaner);
            let selected = select_targets(&targets, options.targets.as_deref())?;
            let reports = scan_all(&selected);

            for report in &reports {
                print_scan_report(report);
            }

            Ok(0)
        }
        "clean" => {
            let options = parse_run_options(&args[1..])?;
            let targets = all_targets(&options.cleaner);
            let selected = select_targets(&targets, options.targets.as_deref())?;
            let mode = if options.apply {
                ExecutionMode::Apply
            } else {
                ExecutionMode::DryRun
            };

            print_plan(&selected, mode);

            if !options.apply {
                println!("\nNo changes made. Re-run with --apply to execute the plan.");

                if options.run_readonly_checks {
                    println!("\nRunning read-only checks:\n");
                    let results = execute_targets(
                        &selected,
                        ExecutionOptions {
                            mode,
                            yes: options.yes,
                            run_readonly_checks: true,
                        },
                    );
                    print_results(&results);
                    return Ok(if any_failed(&results) { 1 } else { 0 });
                }

                return Ok(0);
            }

            if !options.yes && !confirm_apply()? {
                println!("Aborted. No changes made.");
                return Ok(1);
            }

            println!("\nExecuting cleanup plan:\n");
            let results = execute_targets(
                &selected,
                ExecutionOptions {
                    mode,
                    yes: options.yes,
                    run_readonly_checks: true,
                },
            );
            print_results(&results);

            Ok(if any_failed(&results) { 1 } else { 0 })
        }
        unknown => Err(format!(
            "unknown command: {unknown}\n\nRun 'arch-cleaner --help'."
        )),
    }
}

pub fn print_targets(targets: &[CleanupTarget]) {
    for target in targets {
        let sudo = if target.requires_sudo { "sudo" } else { "user" };
        println!(
            "{:<17} {:<24} risk: {:<6} scope: {}",
            target.id, target.title, target.risk, sudo
        );
        println!("  {}", target.description);
    }
}

pub fn print_scan_report(report: &ScanReport) {
    println!("{} ({})", report.title, report.status);

    if let Some(bytes) = report.estimated_bytes {
        println!("  Estimated inspected size: {}", format_bytes(bytes));
    }

    for detail in &report.details {
        println!("  - {detail}");
    }

    for warning in &report.warnings {
        println!("  ! {warning}");
    }

    println!();
}

pub fn print_plan(targets: &[CleanupTarget], mode: ExecutionMode) {
    let label = match mode {
        ExecutionMode::DryRun => "Dry-run plan",
        ExecutionMode::Apply => "Apply plan",
    };

    println!("{label}:");

    for target in targets {
        println!("\n{} [{} risk]", target.title, target.risk);
        for command in commands_for_target(target, mode) {
            println!("  {}", command.display);
        }
    }
}

pub fn print_results(results: &[crate::executor::ExecutionResult]) {
    for result in results {
        match &result.status {
            CommandStatus::Planned => println!("planned: {}", result.command),
            CommandStatus::Success(code) => println!("ok({code}): {}", result.command),
            CommandStatus::Failed(code) => println!("failed({code}): {}", result.command),
            CommandStatus::CouldNotStart(error) => {
                println!("could not start: {} ({error})", result.command)
            }
        }
    }
}

fn parse_run_options(args: &[String]) -> Result<RunOptions, String> {
    let mut options = RunOptions::default();
    let mut index = 0usize;

    while index < args.len() {
        let arg = &args[index];

        if let Some(value) = arg.strip_prefix("--targets=") {
            options.targets = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--keep-packages=") {
            options.cleaner.keep_package_versions = parse_u8("--keep-packages", value)?;
        } else if let Some(value) = arg.strip_prefix("--journal-days=") {
            options.cleaner.journal_days = parse_u16("--journal-days", value)?;
        } else if let Some(value) = arg.strip_prefix("--journal-size=") {
            options.cleaner.journal_size = value.to_string();
        } else if let Some(value) = arg.strip_prefix("--temp-days=") {
            options.cleaner.temp_min_age_days = parse_u16("--temp-days", value)?;
        } else if let Some(value) = arg.strip_prefix("--user-cache-days=") {
            options.cleaner.user_cache_min_age_days = parse_u16("--user-cache-days", value)?;
        } else {
            match arg.as_str() {
                "--targets" => {
                    index += 1;
                    options.targets = Some(require_value(args, index, "--targets")?.to_string());
                }
                "--apply" => options.apply = true,
                "--yes" | "-y" => options.yes = true,
                "--run-readonly-checks" => options.run_readonly_checks = true,
                "--keep-packages" => {
                    index += 1;
                    options.cleaner.keep_package_versions = parse_u8(
                        "--keep-packages",
                        require_value(args, index, "--keep-packages")?,
                    )?;
                }
                "--journal-days" => {
                    index += 1;
                    options.cleaner.journal_days = parse_u16(
                        "--journal-days",
                        require_value(args, index, "--journal-days")?,
                    )?;
                }
                "--journal-size" => {
                    index += 1;
                    options.cleaner.journal_size =
                        require_value(args, index, "--journal-size")?.to_string();
                }
                "--temp-days" => {
                    index += 1;
                    options.cleaner.temp_min_age_days =
                        parse_u16("--temp-days", require_value(args, index, "--temp-days")?)?;
                }
                "--user-cache-days" => {
                    index += 1;
                    options.cleaner.user_cache_min_age_days = parse_u16(
                        "--user-cache-days",
                        require_value(args, index, "--user-cache-days")?,
                    )?;
                }
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                unknown => return Err(format!("unknown option: {unknown}")),
            }
        }

        index += 1;
    }

    Ok(options)
}

fn require_value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_u8(flag: &str, value: &str) -> Result<u8, String> {
    value
        .parse::<u8>()
        .map_err(|_| format!("{flag} expects an integer, got {value}"))
}

fn parse_u16(flag: &str, value: &str) -> Result<u16, String> {
    value
        .parse::<u16>()
        .map_err(|_| format!("{flag} expects an integer, got {value}"))
}

fn confirm_apply() -> Result<bool, String> {
    print!("\nType APPLY to execute these cleanup commands: ");
    io::stdout()
        .flush()
        .map_err(|error| format!("could not flush stdout: {error}"))?;

    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|error| format!("could not read confirmation: {error}"))?;

    Ok(answer.trim() == "APPLY")
}

fn print_help() {
    println!(
        "arch-cleaner {version}\n\nUSAGE:\n    arch-cleaner                  Start the interactive TUI menu\n    arch-cleaner tui              Start the interactive TUI menu\n    arch-cleaner list-targets     Show cleanup targets\n    arch-cleaner scan [OPTIONS]   Inspect selected targets\n    arch-cleaner clean [OPTIONS]  Show or execute a cleanup plan\n\nOPTIONS:\n    --targets <ids>               Comma-separated target ids, or all\n    --apply                       Execute cleanup commands\n    --yes, -y                     Skip confirmation prompts for --apply\n    --run-readonly-checks         In dry-run mode, run read-only commands\n    --keep-packages <n>           Pacman package versions to keep [default: 3]\n    --journal-days <n>            Journal age vacuum threshold [default: 14]\n    --journal-size <size>         Journal size vacuum threshold [default: 1G]\n    --temp-days <n>               Temp file age threshold [default: 7]\n    --user-cache-days <n>         User cache age threshold [default: 30]\n\nTARGETS:\n    pacman-cache, orphan-packages, systemd-journal, user-cache, temp-files, thumbnail-cache, crash-dumps",
        version = env!("CARGO_PKG_VERSION")
    );
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn help_exits_successfully() {
        assert_eq!(run(["--help"]).unwrap(), 0);
    }

    #[test]
    fn unknown_command_is_error() {
        let error = run(["wat"]).unwrap_err();
        assert!(error.contains("unknown command"));
    }
}
