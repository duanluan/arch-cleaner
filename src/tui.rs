use std::io::{self, Write};

use crate::cli::{print_plan, print_results, print_scan_report, print_targets};
use crate::executor::{ExecutionMode, ExecutionOptions, any_failed, execute_targets};
use crate::model::{CleanerOptions, CleanupTarget};
use crate::targets::{all_targets, scan_all};

pub fn run() -> Result<i32, String> {
    let options = CleanerOptions::default();
    let targets = all_targets(&options);
    let mut selected = vec![true; targets.len()];

    loop {
        draw_menu(&targets, &selected)?;
        let input = read_input("Select an action: ")?;
        let action = input.trim();

        match action {
            "" | "s" | "scan" => scan_selected(&targets, &selected),
            "c" | "clean" => {
                let selected_targets = selected_targets(&targets, &selected);
                if selected_targets.is_empty() {
                    pause("No targets selected.")?;
                    continue;
                }

                print_plan(&selected_targets, ExecutionMode::Apply);

                if !confirm_apply()? {
                    pause("Aborted. No changes made.")?;
                    continue;
                }

                let results = execute_targets(
                    &selected_targets,
                    ExecutionOptions {
                        mode: ExecutionMode::Apply,
                        yes: false,
                        run_readonly_checks: true,
                    },
                );
                print_results(&results);
                let exit_code = if any_failed(&results) { 1 } else { 0 };
                pause("Cleanup finished.")?;
                return Ok(exit_code);
            }
            "l" | "list" => {
                print_targets(&targets);
                pause("Target list shown.")?;
            }
            "a" | "all" => {
                selected.fill(true);
            }
            "n" | "none" => {
                selected.fill(false);
            }
            "q" | "quit" | "exit" => return Ok(0),
            value => match value.parse::<usize>() {
                Ok(number) if (1..=selected.len()).contains(&number) => {
                    let index = number - 1;
                    selected[index] = !selected[index];
                }
                _ => pause("Unknown action.")?,
            },
        }
    }
}

fn draw_menu(targets: &[CleanupTarget], selected: &[bool]) -> Result<(), String> {
    print!("\x1b[2J\x1b[H");
    println!("arch-cleaner {}", env!("CARGO_PKG_VERSION"));
    println!("Interactive Arch Linux cleanup menu\n");

    for (index, target) in targets.iter().enumerate() {
        let mark = if selected[index] { "x" } else { " " };
        let sudo = if target.requires_sudo { "sudo" } else { "user" };
        println!(
            "  {:>2}. [{}] {:<24} risk: {:<6} scope: {}",
            index + 1,
            mark,
            target.title,
            target.risk,
            sudo
        );
        println!("      {}", target.description);
    }

    println!(
        "\nActions: [number] toggle  [s] scan  [c] clean  [a] all  [n] none  [l] list  [q] quit"
    );
    io::stdout()
        .flush()
        .map_err(|error| format!("could not flush stdout: {error}"))
}

fn scan_selected(targets: &[CleanupTarget], selected: &[bool]) {
    let selected_targets = selected_targets(targets, selected);

    if selected_targets.is_empty() {
        println!("No targets selected.");
        let _ = pause("Scan skipped.");
        return;
    }

    println!("Scanning selected targets:\n");
    let reports = scan_all(&selected_targets);

    for report in &reports {
        print_scan_report(report);
    }

    let _ = pause("Scan finished.");
}

fn selected_targets(targets: &[CleanupTarget], selected: &[bool]) -> Vec<CleanupTarget> {
    targets
        .iter()
        .zip(selected.iter())
        .filter_map(|(target, is_selected)| is_selected.then_some(target.clone()))
        .collect()
}

fn read_input(prompt: &str) -> Result<String, String> {
    print!("{prompt}");
    io::stdout()
        .flush()
        .map_err(|error| format!("could not flush stdout: {error}"))?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("could not read input: {error}"))?;

    Ok(input)
}

fn confirm_apply() -> Result<bool, String> {
    let answer = read_input("Type APPLY to execute selected cleanup commands: ")?;
    Ok(answer.trim() == "APPLY")
}

fn pause(message: &str) -> Result<(), String> {
    println!("\n{message}");
    let _ = read_input("Press Enter to continue...")?;
    Ok(())
}
