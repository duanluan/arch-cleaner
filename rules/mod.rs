use std::collections::HashSet;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::i18n::{self, Language};
use crate::model::{
    CleanerOptions, CleanupCommand, CleanupTarget, LocalizedText, RiskLevel, ScanReport,
    ScanStatus, TargetGroup,
};
use crate::platform::{
    command_exists, count_non_empty_lines, dir_size, format_bytes, home_dir, is_ignorable_io_error,
    output_text, parse_human_bytes, path_display, run_capture, run_capture_with_env,
};

pub fn all_targets(options: &CleanerOptions) -> Vec<CleanupTarget> {
    vec![
        pacman_cache(options),
        orphan_packages(),
        systemd_journal(options),
        user_cache(options),
        ai_agent_caches(options),
        temporary_files(options),
        thumbnail_cache(),
        crash_dumps(),
    ]
}

pub fn scan_all(
    targets: &[CleanupTarget],
    options: &CleanerOptions,
    language: Language,
) -> Vec<ScanReport> {
    targets
        .iter()
        .map(|target| scan_target(target, options, language))
        .collect()
}

pub fn scan_target(
    target: &CleanupTarget,
    options: &CleanerOptions,
    language: Language,
) -> ScanReport {
    (target.scan)(target, options, language)
}

fn pacman_cache(options: &CleanerOptions) -> CleanupTarget {
    let keep = options.keep_package_versions.to_string();

    CleanupTarget {
        id: "pacman-cache",
        title: LocalizedText {
            zh_cn: "Pacman 包缓存",
            en: "Pacman package cache",
        },
        description: LocalizedText {
            zh_cn: "删除旧的包归档，保留最近版本以便回滚。",
            en: "Remove old package archives while keeping recent versions for rollback.",
        },
        group: TargetGroup::Packages,
        risk: RiskLevel::Low,
        requires_sudo: true,
        threshold_summary: |options: &CleanerOptions, language: Language| {
            i18n::tr_owned(
                language,
                format!("保留 {} 个版本", options.keep_package_versions),
                format!("Keep {} versions", options.keep_package_versions),
            )
        },
        scan: scan_pacman_cache,
        dry_run_commands: vec![CleanupCommand::new(
            format!("paccache -d -k {keep}"),
            "paccache",
            ["-d", "-k", keep.as_str()],
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
    // Keep stderr visible and only report "no orphans" when pacman exited 1
    // with empty output, matching the scan logic: a real pacman failure must
    // not be silently turned into a successful no-op.
    // Apply feeds the package list to pacman via stdin (`pacman -Rns -`), so
    // the script never word-splits unquoted shell variables.
    let dry_run_script = "orphans=$(pacman -Qtdq); status=$?; if [ \"$status\" -eq 0 ] && [ -n \"$orphans\" ]; then printf '%s\\n' \"$orphans\"; elif [ \"$status\" -eq 1 ] && [ -z \"$orphans\" ]; then printf '%s\\n' 'No orphan packages found.'; else exit \"$status\"; fi";
    let apply_script = "orphans=$(pacman -Qtdq); status=$?; if [ \"$status\" -eq 0 ] && [ -n \"$orphans\" ]; then printf '%s\\n' \"$orphans\" | sudo pacman -Rns -; elif [ \"$status\" -eq 1 ] && [ -z \"$orphans\" ]; then printf '%s\\n' 'No orphan packages found.'; else exit \"$status\"; fi";

    CleanupTarget {
        id: "orphan-packages",
        title: LocalizedText {
            zh_cn: "孤儿包",
            en: "Orphan packages",
        },
        description: LocalizedText {
            zh_cn: "移除作为依赖安装、现在已经不再需要的包。",
            en: "Remove packages installed as dependencies that are no longer required.",
        },
        group: TargetGroup::Packages,
        risk: RiskLevel::Medium,
        requires_sudo: true,
        threshold_summary: |_options: &CleanerOptions, language: Language| {
            i18n::tr(
                language,
                "无固定阈值，按当前系统状态动态查询。",
                "No fixed threshold; queried from the current system state.",
            )
            .to_string()
        },
        scan: scan_orphan_packages,
        dry_run_commands: vec![CleanupCommand::shell("pacman -Qtdq", dry_run_script, false)],
        apply_commands: vec![CleanupCommand::shell(
            "orphans=$(pacman -Qtdq); printf '%s\\n' \"$orphans\" | sudo pacman -Rns -",
            apply_script,
            true,
        )],
    }
}

/// journalctl `--vacuum-size` 接受 `500M`、`1G` 这类大小（无后缀按字节）。
/// CLI 和 TUI 在入口处用它统一校验，坏值在拼进命令前就报错，
/// 而不是延迟到 journalctl 运行时才失败。
pub fn is_valid_journal_size(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() || !value.starts_with(|character: char| character.is_ascii_digit()) {
        return false;
    }

    let split_index = value
        .char_indices()
        .find(|(_, character)| !(character.is_ascii_digit() || *character == '.'))
        .map(|(index, _)| index)
        .unwrap_or(value.len());
    let number = &value[..split_index];
    if number.matches('.').count() > 1 || number.ends_with('.') {
        return false;
    }

    let unit = value[split_index..].trim().to_ascii_uppercase();
    matches!(
        unit.as_str(),
        "" | "B"
            | "K"
            | "KB"
            | "KIB"
            | "M"
            | "MB"
            | "MIB"
            | "G"
            | "GB"
            | "GIB"
            | "T"
            | "TB"
            | "TIB"
            | "P"
            | "PB"
            | "PIB"
            | "E"
            | "EB"
            | "EIB"
    )
}

fn systemd_journal(options: &CleanerOptions) -> CleanupTarget {
    let vacuum_time = format!("--vacuum-time={}d", options.journal_days);
    let vacuum_size = format!("--vacuum-size={}", options.journal_size);

    CleanupTarget {
        id: "systemd-journal",
        title: LocalizedText {
            zh_cn: "systemd 日志",
            en: "Systemd journal",
        },
        description: LocalizedText {
            zh_cn: "按时间和总占用清理归档日志。",
            en: "Vacuum archived journal logs by age and total disk use.",
        },
        group: TargetGroup::System,
        risk: RiskLevel::Low,
        requires_sudo: true,
        threshold_summary: |options: &CleanerOptions, language: Language| {
            i18n::tr_owned(
                language,
                format!(
                    "按 {} 天 / {} 清理",
                    options.journal_days, options.journal_size
                ),
                format!(
                    "Vacuum over {} days / {}",
                    options.journal_days, options.journal_size
                ),
            )
        },
        scan: scan_systemd_journal,
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
        title: LocalizedText {
            zh_cn: "用户缓存",
            en: "User cache",
        },
        description: LocalizedText {
            zh_cn: "清理较旧的顶层用户缓存目录，排除已有单独目标的缓存。",
            en: "Remove old top-level user cache directories, excluding caches handled by dedicated targets.",
        },
        group: TargetGroup::User,
        risk: RiskLevel::Medium,
        requires_sudo: false,
        threshold_summary: |options: &CleanerOptions, language: Language| {
            i18n::tr_owned(
                language,
                format!("未变化 {} 天以上", options.user_cache_min_age_days),
                format!("Unchanged for {}+ days", options.user_cache_min_age_days),
            )
        },
        scan: scan_user_cache,
        dry_run_commands: vec![user_cache_find_command(
            &cache_dir_arg,
            &display_path,
            &min_age,
            false,
        )],
        apply_commands: vec![user_cache_find_command(
            &cache_dir_arg,
            &display_path,
            &min_age,
            true,
        )],
    }
}

fn user_cache_find_command(
    cache_dir_arg: &str,
    display_path: &str,
    min_age: &str,
    apply: bool,
) -> CleanupCommand {
    let mut display =
        format!("find {display_path} -xdev -mindepth 1 -maxdepth 1 -type d -mtime {min_age}");
    let mut args = vec![
        cache_dir_arg.to_string(),
        "-xdev".to_string(),
        "-mindepth".to_string(),
        "1".to_string(),
        "-maxdepth".to_string(),
        "1".to_string(),
        "-type".to_string(),
        "d".to_string(),
        "-mtime".to_string(),
        min_age.to_string(),
    ];

    for name in user_cache_excluded_names() {
        display.push_str(&format!(" -not -iname '{name}'"));
        args.push("-not".to_string());
        args.push("-iname".to_string());
        args.push((*name).to_string());
    }

    if apply {
        display.push_str(" -exec rm -rf -- {} +");
        args.extend(
            ["-exec", "rm", "-rf", "--", "{}", "+"]
                .into_iter()
                .map(String::from),
        );
    } else {
        display.push_str(" -print");
        args.push("-print".to_string());
    }

    CleanupCommand::new(display, "find", args, false)
}

fn user_cache_excluded_names() -> &'static [&'static str] {
    &[
        "thumbnails",
        "claude",
        "codex",
        "opencode",
        "cursor",
        "windsurf",
        "gemini",
    ]
}

// `find -iname` semantics: excluded cache names match case-insensitively so
// capitalized variants that exist in practice (e.g. `~/.cache/Codex`) are left
// alone instead of being removed wholesale by the generic user-cache rule.
fn matches_user_cache_excluded_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    user_cache_excluded_names()
        .iter()
        .any(|excluded| lower == *excluded)
}

fn ai_agent_caches(options: &CleanerOptions) -> CleanupTarget {
    let min_age = format!("+{}", options.ai_agent_min_age_days);
    let candidate_dirs = ai_agent_cleanup_dirs();

    CleanupTarget {
        id: "ai-agent-caches",
        title: LocalizedText {
            zh_cn: "AI agent 缓存",
            en: "AI agent caches",
        },
        description: LocalizedText {
            zh_cn: "清理已知 AI 编程 agent 的旧缓存、日志、附件、生成产物和 temp-scratch 条目。",
            en: "Remove old cache, log, attachment, generated artifact, and temp-scratch entries from known AI coding agents.",
        },
        group: TargetGroup::Developer,
        risk: RiskLevel::Medium,
        requires_sudo: false,
        threshold_summary: |options: &CleanerOptions, language: Language| {
            i18n::tr_owned(
                language,
                format!("未变化 {} 天以上", options.ai_agent_min_age_days),
                format!("Older than {} days", options.ai_agent_min_age_days),
            )
        },
        scan: scan_ai_agent_caches,
        // 展示用逐字脚本（与 sh -c 实际执行的完全一致）：这个目标没有
        // 单行等价命令，审计以脚本原文为准，而不是描述性占位。
        dry_run_commands: vec![{
            let script = ai_agent_find_script(&candidate_dirs, &min_age, false);
            CleanupCommand::shell(script.clone(), script, false)
        }],
        apply_commands: vec![{
            let script = ai_agent_find_script(&candidate_dirs, &min_age, true);
            CleanupCommand::shell(script.clone(), script, false)
        }],
    }
}

fn temporary_files(options: &CleanerOptions) -> CleanupTarget {
    let min_age = format!("+{}", options.temp_min_age_days);
    let excluded_prefixes = temp_file_excluded_prefixes();

    CleanupTarget {
        id: "temp-files",
        title: LocalizedText {
            zh_cn: "临时文件",
            en: "Temporary files",
        },
        description: LocalizedText {
            zh_cn: "清理 /var/tmp 和 /tmp 中较旧的顶层条目，跳过 AI agent 临时目录和常见运行时目录。",
            en: "Remove old top-level entries from /var/tmp and /tmp, excluding AI agent scratch and common runtime directories.",
        },
        group: TargetGroup::User,
        risk: RiskLevel::Medium,
        requires_sudo: true,
        threshold_summary: |options: &CleanerOptions, language: Language| {
            i18n::tr_owned(
                language,
                format!("未变化 {} 天以上", options.temp_min_age_days),
                format!("Older than {} days", options.temp_min_age_days),
            )
        },
        scan: scan_temp_files,
        dry_run_commands: vec![
            find_print_command("/var/tmp", &min_age, false, &excluded_prefixes),
            find_print_command("/tmp", &min_age, false, &excluded_prefixes),
        ],
        apply_commands: vec![
            sudo_find_remove_command("/var/tmp", &min_age, &excluded_prefixes),
            sudo_find_remove_command("/tmp", &min_age, &excluded_prefixes),
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
        title: LocalizedText {
            zh_cn: "缩略图缓存",
            en: "Thumbnail cache",
        },
        description: LocalizedText {
            zh_cn: "清理文件管理器生成的缩略图预览。",
            en: "Clear freedesktop thumbnail previews generated by file managers.",
        },
        group: TargetGroup::User,
        risk: RiskLevel::Low,
        requires_sudo: false,
        threshold_summary: |_options: &CleanerOptions, language: Language| {
            i18n::tr(language, "无固定阈值", "No fixed threshold").to_string()
        },
        scan: scan_thumbnail_cache,
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
        title: LocalizedText {
            zh_cn: "系统崩溃转储",
            en: "System crash dumps",
        },
        description: LocalizedText {
            zh_cn: "删除 systemd 保存的 coredump 文件。",
            en: "Remove saved systemd coredump files from /var/lib/systemd/coredump.",
        },
        group: TargetGroup::System,
        risk: RiskLevel::Medium,
        requires_sudo: true,
        threshold_summary: |_options: &CleanerOptions, language: Language| {
            i18n::tr(language, "无固定阈值", "No fixed threshold").to_string()
        },
        scan: scan_crash_dumps,
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

fn ai_agent_cleanup_dirs() -> Vec<PathBuf> {
    ai_agent_cleanup_dirs_for(home_dir().as_deref())
}

/// Pure so tests can build the whitelist from a fixed fake home and stay
/// independent of the environment's `HOME`.
fn ai_agent_cleanup_dirs_for(home: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();

    if let Some(home) = home {
        for relative in [
            ".claude/cache",
            ".cache/claude",
            ".config/Claude/Cache",
            ".config/Claude/Code Cache",
            ".config/Claude/GPUCache",
            ".config/Claude/DawnGraphiteCache",
            ".config/Claude/DawnWebGPUCache",
            ".config/Claude/Crashpad",
            ".config/Claude/logs",
            ".config/Claude/Shared Dictionary/cache",
            ".codex/tmp",
            ".codex/attachments",
            ".codex/ambient-suggestions",
            ".codex/shell_snapshots",
            ".codex/visualizations",
            ".cache/codex",
            ".gemini/tmp",
            ".gemini/antigravity/html_artifacts",
            ".config/Cursor/Cache",
            ".config/Cursor/CachedData",
            ".config/Cursor/CachedProfilesData",
            ".config/Cursor/Code Cache",
            ".config/Cursor/GPUCache",
            ".config/Cursor/DawnGraphiteCache",
            ".config/Cursor/DawnWebGPUCache",
            ".config/Cursor/Crashpad",
            ".config/Cursor/logs",
            ".config/Cursor/sentry",
            ".config/Cursor/Shared Dictionary/cache",
            ".config/Windsurf/Cache",
            ".config/Windsurf/CachedData",
            ".config/Windsurf/CachedProfilesData",
            ".config/Windsurf/Code Cache",
            ".config/Windsurf/GPUCache",
            ".config/Windsurf/DawnGraphiteCache",
            ".config/Windsurf/DawnWebGPUCache",
            ".config/Windsurf/Crashpad",
            ".config/Windsurf/logs",
            ".config/Windsurf/Shared Dictionary/cache",
            ".local/share/opencode/log",
            ".cache/opencode",
            ".local/share/rtk/tee",
            ".pi/agent/analytics",
        ] {
            push_unique_path(&mut paths, &mut seen, home.join(relative));
        }
    }

    for path in ai_agent_temp_paths() {
        push_unique_path(&mut paths, &mut seen, path);
    }

    paths
}

fn ai_agent_temp_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    for root in [Path::new("/var/tmp"), Path::new("/tmp")] {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };

        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            if !file_type.is_file() && !file_type.is_dir() {
                continue;
            }

            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };

            if matches_ai_agent_temp_entry(name) {
                paths.push(entry.path());
            }
        }
    }

    paths
}

// Both matchers stay case-sensitive on purpose: the find commands exclude
// entries with `-path '<root>/<prefix>*'` globs, and find glob matching is
// case-sensitive. Keeping the scan-side matchers identical to the globs keeps
// scan estimates and apply behavior aligned.
fn matches_ai_agent_temp_entry(name: &str) -> bool {
    ai_agent_temp_prefixes()
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

fn ai_agent_temp_prefixes() -> &'static [&'static str] {
    &[
        "codex-",
        "codex_",
        "codex.",
        "openai-codex",
        "pi-agent",
        "pi-research",
        "pi-bash",
        "glm-coding",
        "claude",
        "cursor",
        "windsurf",
        "gemini",
        "opencode",
    ]
}

fn temp_file_excluded_prefixes() -> Vec<&'static str> {
    let mut prefixes = ai_agent_temp_prefixes().to_vec();
    prefixes.extend(temp_runtime_prefixes());
    prefixes
}

fn temp_runtime_prefixes() -> &'static [&'static str] {
    &[
        ".mount_",
        ".x11-unix",
        ".ice-unix",
        ".font-unix",
        ".xim-unix",
        ".test-unix",
        "systemd-private-",
        "snap-private-tmp",
        "ssh-",
        "gpg-",
        "keyring-",
        "pulse-",
        "wayland-",
    ]
}

fn matches_temp_file_excluded_entry(name: &str) -> bool {
    ai_agent_temp_prefixes()
        .iter()
        .chain(temp_runtime_prefixes())
        .any(|prefix| name.starts_with(prefix))
}

fn push_unique_path(paths: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, path: PathBuf) {
    if seen.insert(path.clone()) {
        paths.push(path);
    }
}

fn ai_agent_find_script(candidate_paths: &[PathBuf], min_age: &str, apply: bool) -> String {
    let mut script = String::from("set -efu\n");

    for path in candidate_paths {
        script.push_str("path=");
        script.push_str(&path_display(path));
        script.push('\n');
        script.push_str("if [ -d \"$path\" ]; then\n");
        if apply {
            script.push_str("  find \"$path\" -xdev -mindepth 1 -type f -mtime ");
            script.push_str(min_age);
            script.push_str(" -delete\n");
            script.push_str("  find \"$path\" -xdev -depth -mindepth 1 -type d -empty -delete\n");
        } else {
            script.push_str("  find \"$path\" -xdev -mindepth 1 -type f -mtime ");
            script.push_str(min_age);
            script.push_str(" -print\n");
            script.push_str("  find \"$path\" -xdev -depth -mindepth 1 -type d -empty -print\n");
        }
        script.push_str("elif [ -f \"$path\" ]; then\n");
        if apply {
            script.push_str("  find \"$path\" -xdev -maxdepth 0 -mtime ");
            script.push_str(min_age);
            script.push_str(" -delete\n");
        } else {
            script.push_str("  find \"$path\" -xdev -maxdepth 0 -mtime ");
            script.push_str(min_age);
            script.push_str(" -print\n");
        }
        script.push_str("fi\n");
    }

    script
}

fn find_print_command(
    path: &'static str,
    min_age: &str,
    needs_sudo: bool,
    excluded_name_prefixes: &[&str],
) -> CleanupCommand {
    let display = temp_find_display(path, min_age, excluded_name_prefixes, "-print");
    let mut args = temp_find_args(path, min_age, excluded_name_prefixes);
    args.push("-print".to_string());

    CleanupCommand::new(display, "find", args, needs_sudo)
}

fn sudo_find_remove_command(
    path: &'static str,
    min_age: &str,
    excluded_name_prefixes: &[&str],
) -> CleanupCommand {
    let display = format!(
        "sudo {}",
        temp_find_display(
            path,
            min_age,
            excluded_name_prefixes,
            "-exec rm -rf -- {} +",
        )
    );
    let mut args = vec!["find".to_string()];
    args.extend(temp_find_args(path, min_age, excluded_name_prefixes));
    args.extend(
        ["-exec", "rm", "-rf", "--", "{}", "+"]
            .into_iter()
            .map(String::from),
    );

    CleanupCommand::new(display, "sudo", args, true)
}

fn temp_find_args(
    path: &'static str,
    min_age: &str,
    excluded_name_prefixes: &[&str],
) -> Vec<String> {
    let mut args = vec![path.to_string(), "-xdev".to_string()];

    if !excluded_name_prefixes.is_empty() {
        args.push("(".to_string());
        for (index, prefix) in excluded_name_prefixes.iter().enumerate() {
            if index > 0 {
                args.push("-o".to_string());
            }
            args.push("-path".to_string());
            args.push(format!("{path}/{prefix}*"));
        }
        args.extend([")", "-prune", "-o"].into_iter().map(String::from));
    }

    args.extend([
        "-mindepth".to_string(),
        "1".to_string(),
        "-maxdepth".to_string(),
        "1".to_string(),
        "-mtime".to_string(),
        min_age.to_string(),
    ]);
    args
}

fn temp_find_display(
    path: &'static str,
    min_age: &str,
    excluded_name_prefixes: &[&str],
    action: &str,
) -> String {
    let mut display = format!("find {path} -xdev");

    if !excluded_name_prefixes.is_empty() {
        display.push_str(" \\( ");
        for (index, prefix) in excluded_name_prefixes.iter().enumerate() {
            if index > 0 {
                display.push_str(" -o ");
            }
            display.push_str(&format!("-path '{path}/{prefix}*'"));
        }
        display.push_str(" \\) -prune -o");
    }

    display.push_str(&format!(
        " -mindepth 1 -maxdepth 1 -mtime {min_age} {action}"
    ));
    display
}

fn scan_pacman_cache(
    target: &CleanupTarget,
    _options: &CleanerOptions,
    language: Language,
) -> ScanReport {
    let mut report = ScanReport::new(target);

    if !command_exists("paccache") {
        report.status = ScanStatus::MissingTool;
        report.warnings.push(
            i18n::tr(
                language,
                "未找到 paccache；安装 pacman-contrib 后才能清理缓存。",
                "paccache was not found; install pacman-contrib to enable cache cleanup.",
            )
            .to_string(),
        );
        return report;
    }

    // Scan reuses the exact argv of the dry-run command shown in the plan, so
    // the estimate always describes the command the user sees (only the locale
    // is pinned so the summary line parses stably).
    let Some(dry_run) = target.dry_run_commands.first() else {
        report.status = ScanStatus::Unavailable;
        return report;
    };
    let args: Vec<&str> = dry_run.args.iter().map(String::as_str).collect();
    match run_capture_with_env(
        &dry_run.program,
        &args,
        &[("LC_ALL", "C"), ("LANG", "C"), ("LANGUAGE", "C")],
    ) {
        Ok(output) => {
            let text = output_text(&output);
            if !output.status.success() {
                report.status = ScanStatus::Unavailable;
                report.warnings.push(format!(
                    "{}: {text}",
                    i18n::tr(
                        language,
                        "无法查询 pacman 缓存候选项",
                        "Could not query pacman cache candidates"
                    )
                ));
                return report;
            }

            let parsed = parse_paccache_dry_run(&text);
            if parsed.candidates.is_none() && parsed.bytes.is_none() {
                report.status = ScanStatus::Unavailable;
                report.warnings.push(
                    i18n::tr(
                        language,
                        "无法解析 pacman 缓存候选项摘要。",
                        "Could not parse pacman cache candidate summary.",
                    )
                    .to_string(),
                );
            }
            report.estimated_items = parsed.candidates;
            report.estimated_bytes = parsed.bytes;
            if let Some(candidates) = parsed.candidates {
                report.details.push(format!(
                    "{}: {candidates}",
                    i18n::tr(language, "候选包", "Candidate packages")
                ));
            }
            if !text.is_empty() {
                report.details.push(text);
            }
        }
        Err(error) => {
            report.status = ScanStatus::Unavailable;
            report.warnings.push(format!(
                "{}: {error}",
                i18n::tr(
                    language,
                    "无法查询 pacman 缓存候选项",
                    "Could not query pacman cache candidates"
                )
            ));
        }
    }

    report
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct PaccacheDryRunSummary {
    candidates: Option<usize>,
    bytes: Option<u64>,
}

fn parse_paccache_dry_run(text: &str) -> PaccacheDryRunSummary {
    if text.contains("no candidate packages found") {
        return PaccacheDryRunSummary {
            candidates: Some(0),
            bytes: Some(0),
        };
    }

    let mut summary = PaccacheDryRunSummary::default();
    for line in text.lines() {
        if let Some(after_marker) = line.split("finished dry run:").nth(1) {
            summary.candidates = after_marker
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<usize>().ok());
        }
        if let Some(after_marker) = line.split("disk space saved:").nth(1) {
            let value = after_marker.trim().trim_end_matches(')').trim();
            summary.bytes = parse_human_bytes(value);
        }
    }

    summary
}

fn parse_journal_disk_usage(text: &str) -> Option<u64> {
    let after_marker = text.split("take up").nth(1)?;
    let size = after_marker.split("in the file system").next()?.trim();
    parse_human_bytes(size)
}

fn scan_orphan_packages(
    target: &CleanupTarget,
    _options: &CleanerOptions,
    language: Language,
) -> ScanReport {
    let mut report = ScanReport::new(target);

    if !command_exists("pacman") {
        report.status = ScanStatus::MissingTool;
        report.warnings.push(
            i18n::tr(
                language,
                "未在 PATH 中找到 pacman。",
                "pacman was not found on PATH.",
            )
            .to_string(),
        );
        return report;
    }

    match run_capture("pacman", &["-Qtdq"]) {
        Ok(output) => {
            let text = output_text(&output);
            if output.status.success() {
                let count = count_non_empty_lines(&text);
                report.estimated_items = Some(count);
                report.details.push(format!(
                    "{}: {count}",
                    i18n::tr(language, "孤儿包数量", "Orphan packages")
                ));

                if count > 0 {
                    let preview = text
                        .lines()
                        .filter(|line| !line.trim().is_empty())
                        .take(8)
                        .collect::<Vec<&str>>()
                        .join(", ");
                    report.details.push(format!(
                        "{}: {preview}",
                        i18n::tr(language, "预览", "Preview")
                    ));
                }
            } else if output.status.code() == Some(1) && text.is_empty() {
                // `pacman -Qtdq` exits 1 and prints nothing when there are no
                // orphans; anything else is a real query failure.
                report.estimated_items = Some(0);
                report.details.push(format!(
                    "{}: 0",
                    i18n::tr(language, "孤儿包数量", "Orphan packages")
                ));
            } else {
                report.status = ScanStatus::Unavailable;
                report.warnings.push(format!(
                    "{}: {text}",
                    i18n::tr(
                        language,
                        "无法查询孤儿包",
                        "Could not query orphan packages",
                    )
                ));
            }
        }
        Err(error) => {
            report.status = ScanStatus::Unavailable;
            report.warnings.push(format!(
                "{}: {error}",
                i18n::tr(
                    language,
                    "无法查询孤儿包",
                    "Could not query orphan packages",
                )
            ));
        }
    }

    report
}

fn scan_systemd_journal(
    target: &CleanupTarget,
    _options: &CleanerOptions,
    language: Language,
) -> ScanReport {
    let mut report = ScanReport::new(target);

    if !command_exists("journalctl") {
        report.status = ScanStatus::MissingTool;
        report.warnings.push(
            i18n::tr(
                language,
                "未在 PATH 中找到 journalctl。",
                "journalctl was not found on PATH.",
            )
            .to_string(),
        );
        return report;
    }

    match run_capture_with_env(
        "journalctl",
        &["--disk-usage"],
        &[("LC_ALL", "C"), ("LANG", "C"), ("LANGUAGE", "C")],
    ) {
        Ok(output) => {
            let text = output_text(&output);
            if text.is_empty() {
                report.details.push(
                    i18n::tr(language, "日志占用为空。", "Journal disk usage was empty.")
                        .to_string(),
                );
                return report;
            }

            if let Some(bytes) = parse_journal_disk_usage(&text) {
                report.estimated_bytes = Some(bytes);
            } else {
                report.status = ScanStatus::Unavailable;
                report.warnings.push(
                    i18n::tr(
                        language,
                        "无法解析日志占用。",
                        "Could not parse journal disk usage.",
                    )
                    .to_string(),
                );
            }
            report.details.push(text);
        }
        Err(error) => {
            report.status = ScanStatus::Unavailable;
            report.warnings.push(format!(
                "{}: {error}",
                i18n::tr(
                    language,
                    "无法查询日志占用",
                    "Could not query journal disk usage",
                )
            ));
        }
    }

    report
}

fn scan_user_cache(
    target: &CleanupTarget,
    options: &CleanerOptions,
    language: Language,
) -> ScanReport {
    let mut report = ScanReport::new(target);
    let Some(cache_dir) = home_dir().map(|path| path.join(".cache")) else {
        report.status = ScanStatus::Unavailable;
        report
            .warnings
            .push(i18n::tr(language, "未设置 HOME。", "HOME is not set.").to_string());
        return report;
    };

    match top_level_cleanable_entries(
        &cache_dir,
        options.user_cache_min_age_days,
        true,
        matches_user_cache_excluded_name,
    ) {
        Ok(buckets) => add_cleanable_entry_details(&mut report, buckets, language),
        Err(error) => push_inspect_warning(&mut report, error, language),
    }

    report.details.push(
        i18n::tr(
            language,
            "只统计符合当前天数设置的一级缓存目录；缩略图和 AI agent 缓存由单独目标处理。",
            "Only top-level cache directories matching the current age setting are counted; thumbnails and AI agent caches are handled by dedicated targets.",
        )
        .to_string(),
    );
    report
}

fn scan_ai_agent_caches(
    target: &CleanupTarget,
    options: &CleanerOptions,
    language: Language,
) -> ScanReport {
    let mut report = ScanReport::new(target);

    if home_dir().is_none() {
        report.warnings.push(
            i18n::tr(
                language,
                "未设置 HOME，跳过 home 目录下的 AI agent 缓存。",
                "HOME is not set; home-relative AI agent caches are skipped.",
            )
            .to_string(),
        );
    }

    let mut buckets = Vec::new();
    let candidate_paths = ai_agent_cleanup_dirs();
    for path in candidate_paths {
        if !path.exists() {
            continue;
        }

        match cleanable_files_size(&path, options.ai_agent_min_age_days) {
            Ok(bucket) if bucket.bytes > 0 || bucket.entries > 0 => buckets.push(bucket),
            Ok(_) => {}
            Err(error) => report.warnings.push(format!(
                "{}: {error}",
                i18n::tr(language, "无法检查", "Could not inspect")
            )),
        }
    }

    add_cleanable_entry_details(&mut report, buckets, language);
    report.details.push(
        i18n::tr(
            language,
            "只统计白名单目录中符合当前天数设置的文件；空目录会在执行时删除。",
            "Only files matching the current age setting inside whitelisted paths are counted; empty directories are removed during apply.",
        )
        .to_string(),
    );
    report.details.push(
        i18n::tr(
            language,
            "不会删除配置、密钥、会话历史或插件。",
            "Config, secrets, session history, and plugins are left intact.",
        )
        .to_string(),
    );
    report
}

fn scan_temp_files(
    target: &CleanupTarget,
    options: &CleanerOptions,
    language: Language,
) -> ScanReport {
    let mut report = ScanReport::new(target);
    let mut buckets = Vec::new();

    for root in [Path::new("/var/tmp"), Path::new("/tmp")] {
        match top_level_cleanable_entries(root, options.temp_min_age_days, false, |name| {
            matches_temp_file_excluded_entry(name)
        }) {
            Ok(mut root_buckets) => buckets.append(&mut root_buckets),
            Err(error) => push_inspect_warning(&mut report, error, language),
        }
    }

    add_cleanable_entry_details(&mut report, buckets, language);
    report.details.push(
        i18n::tr(
            language,
            "只统计符合当前天数设置的顶层条目；已知 AI agent 临时目录和常见运行时目录由本目标跳过。",
            "Only top-level entries matching the current age setting are counted; known AI agent temp directories and common runtime directories are skipped.",
        )
        .to_string(),
    );
    report
}

fn scan_thumbnail_cache(
    target: &CleanupTarget,
    _options: &CleanerOptions,
    language: Language,
) -> ScanReport {
    let mut report = ScanReport::new(target);
    let Some(thumbnails_dir) = home_dir().map(|path| path.join(".cache/thumbnails")) else {
        report.status = ScanStatus::Unavailable;
        report
            .warnings
            .push(i18n::tr(language, "未设置 HOME。", "HOME is not set.").to_string());
        return report;
    };

    add_dir_size(
        &mut report,
        &thumbnails_dir,
        i18n::tr(language, "缩略图缓存", "Thumbnail cache"),
        language,
    );
    report
}

fn scan_crash_dumps(
    target: &CleanupTarget,
    _options: &CleanerOptions,
    language: Language,
) -> ScanReport {
    let mut report = ScanReport::new(target);
    add_dir_size(
        &mut report,
        Path::new("/var/lib/systemd/coredump"),
        i18n::tr(language, "coredump 存储", "Coredump storage"),
        language,
    );
    report
}

#[derive(Clone, Debug)]
struct CleanableEntry {
    path: PathBuf,
    bytes: u64,
    entries: usize,
}

fn top_level_cleanable_entries<F>(
    root: &Path,
    min_age_days: u16,
    dirs_only: bool,
    is_excluded: F,
) -> io::Result<Vec<CleanableEntry>>
where
    F: Fn(&str) -> bool,
{
    if !root.exists() {
        return Ok(Vec::new());
    }

    let root_dev = fs::symlink_metadata(root)?.dev();
    let mut buckets = Vec::new();
    for entry in fs::read_dir(root)?.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        // find(1) lstats entries: `-type d` never matches symlinks, and with
        // no -type restriction symlinks, sockets, and fifos are removed too.
        if dirs_only && !file_type.is_dir() {
            continue;
        }

        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if is_excluded(name) {
            continue;
        }

        // lstat, like find -P: for symlinks the age of the link itself counts.
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        // The apply commands run `find -xdev`, which skips entries on other
        // mounts; scan estimates must skip them as well.
        if metadata.dev() != root_dev {
            continue;
        }
        if !is_older_than(&metadata, min_age_days) {
            continue;
        }

        let bytes = if metadata.is_file() {
            metadata.len()
        } else {
            dir_size(&path)?
        };
        buckets.push(CleanableEntry {
            path,
            bytes,
            entries: 1,
        });
    }

    Ok(buckets)
}

fn cleanable_files_size(path: &Path, min_age_days: u16) -> io::Result<CleanableEntry> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(CleanableEntry {
            path: path.to_path_buf(),
            bytes: 0,
            entries: 0,
        });
    }

    if metadata.is_file() {
        let cleanable = is_older_than(&metadata, min_age_days);
        return Ok(CleanableEntry {
            path: path.to_path_buf(),
            bytes: if cleanable { metadata.len() } else { 0 },
            entries: usize::from(cleanable),
        });
    }

    let mut total = 0u64;
    let mut entries = 0usize;
    let mut stack = vec![path.to_path_buf()];

    while let Some(current) = stack.pop() {
        let children = match fs::read_dir(&current) {
            Ok(children) => children,
            Err(error) if is_ignorable_io_error(&error) => continue,
            Err(error) => return Err(error),
        };

        for entry in children.flatten() {
            let child_path = entry.path();
            let metadata = match fs::symlink_metadata(&child_path) {
                Ok(metadata) => metadata,
                Err(error) if is_ignorable_io_error(&error) => continue,
                Err(error) => return Err(error),
            };

            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                stack.push(child_path);
                continue;
            }
            if metadata.is_file() && is_older_than(&metadata, min_age_days) {
                total = total.saturating_add(metadata.len());
                entries = entries.saturating_add(1);
            }
        }
    }

    Ok(CleanableEntry {
        path: path.to_path_buf(),
        bytes: total,
        entries,
    })
}

fn add_cleanable_entry_details(
    report: &mut ScanReport,
    mut entries: Vec<CleanableEntry>,
    language: Language,
) {
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.bytes));
    let total_bytes = entries
        .iter()
        .fold(0u64, |total, entry| total.saturating_add(entry.bytes));
    let total_entries = entries
        .iter()
        .fold(0usize, |total, entry| total.saturating_add(entry.entries));
    report.estimated_bytes = Some(total_bytes);
    report.estimated_items = Some(total_entries);

    report.details.push(format!(
        "{}: {}",
        i18n::tr(language, "可清理条目", "Eligible entries"),
        total_entries
    ));
    report.details.push(format!(
        "{}: {}",
        i18n::tr(language, "候选位置", "Eligible locations"),
        entries.len()
    ));

    for entry in entries.iter().take(10) {
        report.details.push(format!(
            "{} ({}, {} {})",
            entry.path.display(),
            format_bytes(entry.bytes),
            entry.entries,
            i18n::tr(language, "项", "items")
        ));
    }

    if entries.len() > 10 {
        report.details.push(format!(
            "{}: {}",
            i18n::tr(language, "还有更多位置", "Additional locations"),
            entries.len() - 10
        ));
    }
}

fn push_inspect_warning(report: &mut ScanReport, error: io::Error, language: Language) {
    report.status = ScanStatus::Unavailable;
    report.warnings.push(format!(
        "{}: {error}",
        i18n::tr(language, "无法检查", "Could not inspect")
    ));
}

fn is_older_than(metadata: &fs::Metadata, days: u16) -> bool {
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    let Ok(age) = SystemTime::now().duration_since(modified) else {
        return false;
    };

    matches_find_mtime_plus(age, days)
}

/// Mirror GNU find's `-mtime +N`, which matches once the age counted in whole
/// 24-hour days exceeds N (so `+7` matches files at least 8 days old).
/// Scan estimates must use the same rule as the find commands that run during
/// apply, otherwise entries between N and N+1 days are counted but not removed.
fn matches_find_mtime_plus(age: Duration, days: u16) -> bool {
    age.as_secs() / 86_400 > u64::from(days)
}

fn add_dir_size(report: &mut ScanReport, path: &Path, label: &str, language: Language) {
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
            report.warnings.push(format!(
                "{}: {error}",
                i18n::tr(language, "无法检查", "Could not inspect",)
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        all_targets, is_valid_journal_size, matches_ai_agent_temp_entry, matches_find_mtime_plus,
        matches_temp_file_excluded_entry, matches_user_cache_excluded_name,
    };
    use crate::i18n::Language;
    use crate::model::CleanerOptions;
    use std::collections::HashSet;
    use std::path::Path;
    use std::time::Duration;

    #[test]
    fn target_ids_are_unique() {
        let options = CleanerOptions::default();
        let targets = all_targets(&options);
        let unique_ids: HashSet<&str> = targets.iter().map(|target| target.id).collect();

        assert_eq!(targets.len(), unique_ids.len());
    }

    #[test]
    fn every_target_is_self_contained() {
        let options = CleanerOptions::default();
        for target in all_targets(&options) {
            assert!(!target.title.get(Language::ZhCn).is_empty());
            assert!(!target.title.get(Language::En).is_empty());
            assert!(!target.description.get(Language::ZhCn).is_empty());
            assert!(!target.description.get(Language::En).is_empty());
            for language in [Language::ZhCn, Language::En] {
                assert!(
                    !(target.threshold_summary)(&options, language).is_empty(),
                    "{} threshold summary missing for {language:?}",
                    target.id
                );
            }
        }
    }

    #[test]
    fn readme_lists_every_target() {
        let readme =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md")).unwrap();

        for target in all_targets(&CleanerOptions::default()) {
            assert!(
                readme.contains(target.id),
                "README.md 缺少目标 {}；新增规则时必须同步 README 表格",
                target.id
            );
        }
    }

    #[test]
    fn journal_size_validation_accepts_systemd_units() {
        for value in ["1G", "500M", "10K", "1024", "1.5G", "2 TiB", "3gb"] {
            assert!(is_valid_journal_size(value), "{value} should be valid");
        }
        for value in ["", "G", "1x", "1..5G", "1.", "-1G", "abc"] {
            assert!(!is_valid_journal_size(value), "{value} should be invalid");
        }
    }

    #[test]
    fn pacman_cache_dry_run_uses_single_paccache_operation() {
        let options = CleanerOptions::default();
        let targets = all_targets(&options);
        let target = targets
            .iter()
            .find(|target| target.id == "pacman-cache")
            .unwrap();
        let command = &target.dry_run_commands[0];

        assert_eq!(command.program, "paccache");
        assert!(command.args.contains(&"-d".to_string()));
        assert!(command.args.contains(&"-k".to_string()));
        assert!(!command.args.contains(&"-r".to_string()));
    }

    #[test]
    fn parses_paccache_dry_run_summary() {
        let summary = super::parse_paccache_dry_run(
            "==> finished dry run: 42 candidates (disk space saved: 1.5 GiB)",
        );

        assert_eq!(summary.candidates, Some(42));
        assert_eq!(summary.bytes, Some(1_610_612_736));
    }

    #[test]
    fn parses_empty_paccache_dry_run_summary() {
        let summary = super::parse_paccache_dry_run(">>> no candidate packages found for pruning");

        assert_eq!(summary.candidates, Some(0));
        assert_eq!(summary.bytes, Some(0));
    }

    #[test]
    fn parses_journal_disk_usage_summary() {
        let text = "Archived and active journals take up 4G in the file system.";
        let bytes = super::parse_journal_disk_usage(text);

        assert_eq!(bytes, Some(4_294_967_296));
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
            assert!(
                command
                    .args
                    .iter()
                    .any(|arg| arg.contains("No orphan packages found."))
            );
            assert!(
                command
                    .args
                    .iter()
                    .any(|arg| arg.contains("exit \"$status\""))
            );
        }

        // The apply script must feed pacman via stdin instead of expanding an
        // unquoted variable, per rules/README.md "avoid broad shell expansion".
        let apply_script = &target.apply_commands[0].args[1];
        assert!(apply_script.contains("sudo pacman -Rns -"));
        assert!(!apply_script.contains("pacman -Rns $orphans"));
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
    fn ai_agent_cleanup_is_separate_and_conservative() {
        let options = CleanerOptions::default();
        let targets = all_targets(&options);
        let target = targets
            .iter()
            .find(|target| target.id == "ai-agent-caches")
            .unwrap();
        assert!(!target.requires_sudo);
        assert!(
            target
                .dry_run_commands
                .iter()
                .chain(target.apply_commands.iter())
                .all(|command| command.program == "sh" && !command.needs_sudo)
        );

        // Build the script from a fixed fake home so the test does not depend
        // on the environment's `HOME` being set.
        let dirs = super::ai_agent_cleanup_dirs_for(Some(Path::new("/home/demo")));
        let script_text = super::ai_agent_find_script(&dirs, "+30", false)
            + &super::ai_agent_find_script(&dirs, "+30", true);

        assert!(script_text.contains(".codex/attachments"));
        assert!(script_text.contains(".config/Claude/Cache"));
        assert!(script_text.contains("-mindepth 1"));
        assert!(script_text.contains("-delete"));
        assert!(!script_text.contains("rm -rf"));
        assert!(!script_text.contains(".codex/sessions"));
        assert!(!script_text.contains(".codex/archived_sessions"));
        assert!(!script_text.contains(".pi/agent/sessions"));
        assert!(!script_text.contains("IndexedDB"));
        assert!(!script_text.contains("Local Storage"));
        assert!(!script_text.contains("workspaceStorage"));
        assert!(!script_text.contains("node_modules"));
    }

    #[test]
    fn user_cache_skips_dedicated_cache_targets() {
        let options = CleanerOptions::default();
        let targets = all_targets(&options);
        let target = targets
            .iter()
            .find(|target| target.id == "user-cache")
            .unwrap();
        let command_text = target
            .dry_run_commands
            .iter()
            .chain(target.apply_commands.iter())
            .flat_map(|command| command.args.iter())
            .cloned()
            .collect::<Vec<String>>()
            .join("\n");

        assert!(command_text.contains("thumbnails"));
        assert!(command_text.contains("codex"));
        assert!(command_text.contains("claude"));
        assert!(command_text.contains("opencode"));
        assert!(command_text.contains("-not"));
        assert!(command_text.contains("-iname"));
    }

    #[test]
    fn user_cache_exclusion_matches_case_insensitively() {
        assert!(matches_user_cache_excluded_name("thumbnails"));
        assert!(matches_user_cache_excluded_name("Codex"));
        assert!(matches_user_cache_excluded_name("CLAUDE"));
        assert!(matches_user_cache_excluded_name("Windsurf"));
        // Exact-name matching only, like find -iname.
        assert!(!matches_user_cache_excluded_name("codex-cli"));
        assert!(!matches_user_cache_excluded_name("paru"));
    }

    #[test]
    fn temp_files_skip_ai_agent_prefixes() {
        let options = CleanerOptions::default();
        let targets = all_targets(&options);
        let target = targets
            .iter()
            .find(|target| target.id == "temp-files")
            .unwrap();
        let command_text = target
            .dry_run_commands
            .iter()
            .chain(target.apply_commands.iter())
            .flat_map(|command| command.args.iter())
            .cloned()
            .collect::<Vec<String>>()
            .join("\n");
        let apply_command = &target.apply_commands[0];

        assert_eq!(apply_command.program, "sudo");
        assert_eq!(apply_command.args[0], "find");
        assert!(apply_command.display.starts_with("sudo find "));
        assert!(command_text.contains("-maxdepth"));
        assert!(command_text.contains("-prune"));
        assert!(command_text.contains("/var/tmp/codex-*"));
        assert!(command_text.contains("/var/tmp/pi-agent*"));
        assert!(command_text.contains("/tmp/openai-codex*"));
        assert!(command_text.contains("/tmp/.mount_*"));
        assert!(!command_text.contains("pi-current"));
    }

    #[test]
    fn temp_file_exclusions_include_runtime_entries() {
        assert!(matches_temp_file_excluded_entry(".mount_multiABCD"));
        assert!(matches_temp_file_excluded_entry("systemd-private-123"));
        assert!(matches_temp_file_excluded_entry("ssh-agent.ABCD"));
        assert!(matches_temp_file_excluded_entry("wayland-0"));
        assert!(matches_temp_file_excluded_entry("codex-radar-dup-t7sNGp"));
        assert!(!matches_temp_file_excluded_entry("paru-build-cache"));
    }

    #[test]
    fn ai_agent_temp_prefixes_are_narrow() {
        assert!(matches_ai_agent_temp_entry("codex-radar-dup-t7sNGp"));
        assert!(matches_ai_agent_temp_entry("pi-agent-research.HTEQua"));
        assert!(matches_ai_agent_temp_entry("pi-research.HTEQua"));
        assert!(matches_ai_agent_temp_entry("pi-bash-26b98648680f6a8a.log"));
        assert!(matches_ai_agent_temp_entry("openai-codex-pricing.Lz9jE9"));
        assert!(!matches_ai_agent_temp_entry("pi-current"));
        assert!(!matches_ai_agent_temp_entry("node-compile-cache"));
        assert!(!matches_ai_agent_temp_entry("pytest-of-njcm"));
    }

    #[test]
    fn age_threshold_matches_find_mtime_plus() {
        let day = 86_400u64;

        // `find -mtime +7` only matches once the age exceeds 7 whole days.
        assert!(!matches_find_mtime_plus(Duration::from_secs(7 * day), 7));
        assert!(!matches_find_mtime_plus(
            Duration::from_secs(8 * day - 1),
            7
        ));
        assert!(matches_find_mtime_plus(Duration::from_secs(8 * day), 7));
        assert!(!matches_find_mtime_plus(Duration::from_secs(0), 0));
        assert!(matches_find_mtime_plus(Duration::from_secs(day), 0));
    }

    #[test]
    fn temp_prefix_matching_is_case_sensitive_like_find_globs() {
        // find -path globs are case-sensitive, so the matchers must agree.
        assert!(matches_temp_file_excluded_entry("codex-scratch"));
        assert!(!matches_temp_file_excluded_entry("Codex-scratch"));
        assert!(matches_ai_agent_temp_entry("pi-agent-x"));
        assert!(!matches_ai_agent_temp_entry("Pi-agent-x"));
    }
}
