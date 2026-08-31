use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::model::{
    CleanerOptions, CleanupCommand, CleanupTarget, RiskLevel, ScanReport, ScanStatus,
};
use crate::platform::{
    command_exists, count_non_empty_lines, dir_size, format_bytes, home_dir, output_text,
    path_display, run_capture,
};

pub fn all_targets(options: &CleanerOptions) -> Vec<CleanupTarget> {
    vec![
        pacman_cache(options),
        orphan_packages(),
        systemd_journal(options),
        user_cache(options),
        temporary_files(options),
        thumbnail_cache(),
        crash_dumps(),
    ]
}

pub fn scan_all(targets: &[CleanupTarget]) -> Vec<ScanReport> {
    targets.iter().map(scan_target).collect()
}

pub fn scan_target(target: &CleanupTarget) -> ScanReport {
    match target.id {
        "pacman-cache" => scan_pacman_cache(target),
        "orphan-packages" => scan_orphan_packages(target),
        "systemd-journal" => scan_systemd_journal(target),
        "user-cache" => scan_user_cache(target),
        "temp-files" => scan_temp_files(target),
        "thumbnail-cache" => scan_thumbnail_cache(target),
        "crash-dumps" => scan_crash_dumps(target),
        _ => {
            let mut report = ScanReport::new(target);
            report.status = ScanStatus::Unavailable;
            report.warnings.push("Unknown cleanup target.".to_string());
            report
        }
    }
}

pub fn select_targets(
    targets: &[CleanupTarget],
    requested: Option<&str>,
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
        return Err("--targets must include at least one target id".to_string());
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
        return Err(format!(
            "unknown target(s): {}. Valid targets: {}",
            unknown_ids.join(", "),
            valid_ids.join(", ")
        ));
    }

    Ok(targets
        .iter()
        .filter(|target| requested_ids.contains(&target.id))
        .cloned()
        .collect())
}

fn pacman_cache(options: &CleanerOptions) -> CleanupTarget {
    let keep = options.keep_package_versions.to_string();

    CleanupTarget {
        id: "pacman-cache",
        title: "Pacman package cache",
        description: "Remove old package archives while keeping recent versions for rollback.",
        risk: RiskLevel::Low,
        requires_sudo: true,
        dry_run_commands: vec![CleanupCommand::new(
            format!("paccache -d -r -k {keep}"),
            "paccache",
            ["-d", "-r", "-k", keep.as_str()],
            false,
        )],
        apply_commands: vec![CleanupCommand::new(
            format!("sudo paccache -r -k {keep}"),
            "sudo",
            ["paccache", "-r", "-k", keep.as_str()],
            true,
        )],
    }
}

fn orphan_packages() -> CleanupTarget {
    let dry_run_script = "orphans=$(pacman -Qtdq 2>/dev/null); status=$?; if [ \"$status\" -eq 0 ] && [ -n \"$orphans\" ]; then printf '%s\\n' \"$orphans\"; elif [ \"$status\" -eq 1 ]; then printf '%s\\n' 'No orphan packages found.'; else exit \"$status\"; fi";
    let apply_script = "orphans=$(pacman -Qtdq 2>/dev/null); status=$?; if [ \"$status\" -eq 0 ] && [ -n \"$orphans\" ]; then sudo pacman -Rns $orphans; elif [ \"$status\" -eq 1 ]; then printf '%s\\n' 'No orphan packages found.'; else exit \"$status\"; fi";

    CleanupTarget {
        id: "orphan-packages",
        title: "Orphan packages",
        description: "Remove packages installed as dependencies that are no longer required.",
        risk: RiskLevel::Medium,
        requires_sudo: true,
        dry_run_commands: vec![CleanupCommand::shell(
            "pacman -Qtdq",
            dry_run_script,
            false,
        )],
        apply_commands: vec![CleanupCommand::shell(
            "orphans=$(pacman -Qtdq); sudo pacman -Rns $orphans",
            apply_script,
            true,
        )],
    }
}

fn systemd_journal(options: &CleanerOptions) -> CleanupTarget {
    let vacuum_time = format!("--vacuum-time={}d", options.journal_days);
    let vacuum_size = format!("--vacuum-size={}", options.journal_size);

    CleanupTarget {
        id: "systemd-journal",
        title: "Systemd journal",
        description: "Vacuum archived journal logs by age and total disk use.",
        risk: RiskLevel::Low,
        requires_sudo: true,
        dry_run_commands: vec![CleanupCommand::new(
            "journalctl --disk-usage",
            "journalctl",
            ["--disk-usage"],
            false,
        )],
        apply_commands: vec![CleanupCommand::new(
            format!("sudo journalctl {vacuum_time} {vacuum_size}"),
            "sudo",
            ["journalctl", vacuum_time.as_str(), vacuum_size.as_str()],
            true,
        )],
    }
}

fn user_cache(options: &CleanerOptions) -> CleanupTarget {
    let cache_dir = home_dir()
        .map(|path| path.join(".cache"))
        .unwrap_or_else(|| PathBuf::from("$HOME/.cache"));
    let cache_dir_arg = cache_dir.to_string_lossy().to_string();
    let min_age = format!("+{}", options.user_cache_min_age_days);
    let display_path = path_display(&cache_dir);

    CleanupTarget {
        id: "user-cache",
        title: "User cache",
        description: "Remove top-level user cache directories that have not changed recently.",
        risk: RiskLevel::Medium,
        requires_sudo: false,
        dry_run_commands: vec![CleanupCommand::new(
            format!(
                "find {display_path} -xdev -mindepth 1 -maxdepth 1 -type d -mtime {min_age} -print"
            ),
            "find",
            [
                cache_dir_arg.as_str(),
                "-xdev",
                "-mindepth",
                "1",
                "-maxdepth",
                "1",
                "-type",
                "d",
                "-mtime",
                min_age.as_str(),
                "-print",
            ],
            false,
        )],
        apply_commands: vec![CleanupCommand::new(
            format!(
                "find {display_path} -xdev -mindepth 1 -maxdepth 1 -type d -mtime {min_age} -exec rm -rf -- {{}} +"
            ),
            "find",
            [
                cache_dir_arg.as_str(),
                "-xdev",
                "-mindepth",
                "1",
                "-maxdepth",
                "1",
                "-type",
                "d",
                "-mtime",
                min_age.as_str(),
                "-exec",
                "rm",
                "-rf",
                "--",
                "{}",
                "+",
            ],
            false,
        )],
    }
}

fn temporary_files(options: &CleanerOptions) -> CleanupTarget {
    let min_age = format!("+{}", options.temp_min_age_days);

    CleanupTarget {
        id: "temp-files",
        title: "Temporary files",
        description: "Remove old entries from /var/tmp and /tmp without crossing mount points.",
        risk: RiskLevel::Medium,
        requires_sudo: true,
        dry_run_commands: vec![
            find_print_command("/var/tmp", &min_age, false),
            find_print_command("/tmp", &min_age, false),
        ],
        apply_commands: vec![
            sudo_find_remove_command("/var/tmp", &min_age),
            sudo_find_remove_command("/tmp", &min_age),
        ],
    }
}

fn thumbnail_cache() -> CleanupTarget {
    let thumbnails_dir = home_dir()
        .map(|path| path.join(".cache/thumbnails"))
        .unwrap_or_else(|| PathBuf::from("$HOME/.cache/thumbnails"));
    let thumbnails_arg = thumbnails_dir.to_string_lossy().to_string();
    let display_path = path_display(&thumbnails_dir);

    CleanupTarget {
        id: "thumbnail-cache",
        title: "Thumbnail cache",
        description: "Clear freedesktop thumbnail previews generated by file managers.",
        risk: RiskLevel::Low,
        requires_sudo: false,
        dry_run_commands: vec![CleanupCommand::new(
            format!("find {display_path} -mindepth 1 -maxdepth 1 -print"),
            "find",
            [
                thumbnails_arg.as_str(),
                "-mindepth",
                "1",
                "-maxdepth",
                "1",
                "-print",
            ],
            false,
        )],
        apply_commands: vec![CleanupCommand::new(
            format!("find {display_path} -mindepth 1 -maxdepth 1 -exec rm -rf -- {{}} +"),
            "find",
            [
                thumbnails_arg.as_str(),
                "-mindepth",
                "1",
                "-maxdepth",
                "1",
                "-exec",
                "rm",
                "-rf",
                "--",
                "{}",
                "+",
            ],
            false,
        )],
    }
}

fn crash_dumps() -> CleanupTarget {
    CleanupTarget {
        id: "crash-dumps",
        title: "System crash dumps",
        description: "Remove saved systemd coredump files from /var/lib/systemd/coredump.",
        risk: RiskLevel::Medium,
        requires_sudo: true,
        dry_run_commands: vec![CleanupCommand::new(
            "find /var/lib/systemd/coredump -xdev -mindepth 1 -type f -print",
            "find",
            [
                "/var/lib/systemd/coredump",
                "-xdev",
                "-mindepth",
                "1",
                "-type",
                "f",
                "-print",
            ],
            false,
        )],
        apply_commands: vec![CleanupCommand::new(
            "sudo find /var/lib/systemd/coredump -xdev -mindepth 1 -type f -delete",
            "sudo",
            [
                "find",
                "/var/lib/systemd/coredump",
                "-xdev",
                "-mindepth",
                "1",
                "-type",
                "f",
                "-delete",
            ],
            true,
        )],
    }
}

fn find_print_command(path: &'static str, min_age: &str, needs_sudo: bool) -> CleanupCommand {
    CleanupCommand::new(
        format!("find {path} -xdev -mindepth 1 -mtime {min_age} -print"),
        "find",
        [path, "-xdev", "-mindepth", "1", "-mtime", min_age, "-print"],
        needs_sudo,
    )
}

fn sudo_find_remove_command(path: &'static str, min_age: &str) -> CleanupCommand {
    CleanupCommand::new(
        format!("sudo find {path} -xdev -mindepth 1 -mtime {min_age} -exec rm -rf -- {{}} +"),
        "sudo",
        [
            "find",
            path,
            "-xdev",
            "-mindepth",
            "1",
            "-mtime",
            min_age,
            "-exec",
            "rm",
            "-rf",
            "--",
            "{}",
            "+",
        ],
        true,
    )
}

fn scan_pacman_cache(target: &CleanupTarget) -> ScanReport {
    let mut report = ScanReport::new(target);

    if !command_exists("pacman") {
        report.status = ScanStatus::MissingTool;
        report
            .warnings
            .push("pacman was not found on PATH.".to_string());
    }

    if !command_exists("paccache") {
        report.status = ScanStatus::MissingTool;
        report.warnings.push(
            "paccache was not found; install pacman-contrib to enable cache cleanup.".to_string(),
        );
    }

    add_dir_size(
        &mut report,
        Path::new("/var/cache/pacman/pkg"),
        "Package cache",
    );
    report
}

fn scan_orphan_packages(target: &CleanupTarget) -> ScanReport {
    let mut report = ScanReport::new(target);

    if !command_exists("pacman") {
        report.status = ScanStatus::MissingTool;
        report
            .warnings
            .push("pacman was not found on PATH.".to_string());
        return report;
    }

    match run_capture("pacman", &["-Qtdq"]) {
        Ok(output) => {
            let text = output_text(&output);
            let count = if output.status.success() {
                count_non_empty_lines(&text)
            } else {
                0
            };
            report.details.push(format!("Orphan packages: {count}"));

            if count > 0 {
                let preview = text
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .take(8)
                    .collect::<Vec<&str>>()
                    .join(", ");
                report.details.push(format!("Preview: {preview}"));
            }
        }
        Err(error) => {
            report.status = ScanStatus::Unavailable;
            report
                .warnings
                .push(format!("Could not query orphan packages: {error}"));
        }
    }

    report
}

fn scan_systemd_journal(target: &CleanupTarget) -> ScanReport {
    let mut report = ScanReport::new(target);

    if !command_exists("journalctl") {
        report.status = ScanStatus::MissingTool;
        report
            .warnings
            .push("journalctl was not found on PATH.".to_string());
        return report;
    }

    match run_capture("journalctl", &["--disk-usage"]) {
        Ok(output) => {
            let text = output_text(&output);
            if text.is_empty() {
                report
                    .details
                    .push("Journal disk usage was empty.".to_string());
            } else {
                report.details.push(text);
            }
        }
        Err(error) => {
            report.status = ScanStatus::Unavailable;
            report
                .warnings
                .push(format!("Could not query journal disk usage: {error}"));
        }
    }

    report
}

fn scan_user_cache(target: &CleanupTarget) -> ScanReport {
    let mut report = ScanReport::new(target);
    let Some(cache_dir) = home_dir().map(|path| path.join(".cache")) else {
        report.status = ScanStatus::Unavailable;
        report.warnings.push("HOME is not set.".to_string());
        return report;
    };

    add_dir_size(&mut report, &cache_dir, "User cache total size");
    report.details.push(
        "Cleanup only targets top-level cache directories older than the configured age."
            .to_string(),
    );
    report
}

fn scan_temp_files(target: &CleanupTarget) -> ScanReport {
    let mut report = ScanReport::new(target);
    add_dir_size(&mut report, Path::new("/var/tmp"), "/var/tmp total size");
    add_dir_size(&mut report, Path::new("/tmp"), "/tmp total size");
    report
        .details
        .push("Cleanup only targets entries older than the configured age.".to_string());
    report
}

fn scan_thumbnail_cache(target: &CleanupTarget) -> ScanReport {
    let mut report = ScanReport::new(target);
    let Some(thumbnails_dir) = home_dir().map(|path| path.join(".cache/thumbnails")) else {
        report.status = ScanStatus::Unavailable;
        report.warnings.push("HOME is not set.".to_string());
        return report;
    };

    add_dir_size(&mut report, &thumbnails_dir, "Thumbnail cache");
    report
}

fn scan_crash_dumps(target: &CleanupTarget) -> ScanReport {
    let mut report = ScanReport::new(target);
    add_dir_size(
        &mut report,
        Path::new("/var/lib/systemd/coredump"),
        "Coredump storage",
    );
    report
}

fn add_dir_size(report: &mut ScanReport, path: &Path, label: &str) {
    match dir_size(path) {
        Ok(bytes) => {
            report.estimated_bytes =
                Some(report.estimated_bytes.unwrap_or(0).saturating_add(bytes));
            report.details.push(format!(
                "{label}: {} ({})",
                path.display(),
                format_bytes(bytes)
            ));
        }
        Err(error) => {
            report.status = ScanStatus::Unavailable;
            report
                .warnings
                .push(format!("Could not inspect {}: {error}", path.display()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{all_targets, select_targets};
    use crate::model::CleanerOptions;
    use std::collections::HashSet;

    #[test]
    fn target_ids_are_unique() {
        let options = CleanerOptions::default();
        let targets = all_targets(&options);
        let unique_ids: HashSet<&str> = targets.iter().map(|target| target.id).collect();

        assert_eq!(targets.len(), unique_ids.len());
    }

    #[test]
    fn selects_comma_separated_targets() {
        let options = CleanerOptions::default();
        let targets = all_targets(&options);
        let selected = select_targets(&targets, Some("pacman-cache,systemd-journal")).unwrap();

        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].id, "pacman-cache");
        assert_eq!(selected[1].id, "systemd-journal");
    }

    #[test]
    fn orphan_package_commands_preserve_real_query_failures() {
        let options = CleanerOptions::default();
        let targets = all_targets(&options);
        let target = targets
            .iter()
            .find(|target| target.id == "orphan-packages")
            .unwrap();

        for command in [&target.dry_run_commands[0], &target.apply_commands[0]] {
            assert_eq!(command.program, "sh");
            assert!(command.args.iter().any(|arg| arg.contains("No orphan packages found.")));
            assert!(command.args.iter().any(|arg| arg.contains("exit \"$status\"")));
        }
    }

    #[test]
    fn crash_dump_delete_command_keeps_find_guards() {
        let options = CleanerOptions::default();
        let targets = all_targets(&options);
        let target = targets
            .iter()
            .find(|target| target.id == "crash-dumps")
            .unwrap();
        let command = &target.apply_commands[0];

        assert_eq!(command.program, "sudo");
        assert!(command.args.contains(&"-xdev".to_string()));
        assert!(command.args.contains(&"-mindepth".to_string()));
        assert!(command.args.contains(&"1".to_string()));
        assert!(command.args.contains(&"-delete".to_string()));
    }

    #[test]
    fn rejects_unknown_targets() {
        let options = CleanerOptions::default();
        let targets = all_targets(&options);
        let error = select_targets(&targets, Some("wat")).unwrap_err();

        assert!(error.contains("unknown target"));
    }
}
