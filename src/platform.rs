use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn command_exists(command: &str) -> bool {
    if command.contains('/') {
        return Path::new(command).exists();
    }

    let Some(paths) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&paths).any(|path| path.join(command).is_file())
}

pub fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

pub fn dir_size(path: &Path) -> io::Result<u64> {
    if !path.exists() {
        return Ok(0);
    }

    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];

    while let Some(current) = stack.pop() {
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if is_ignorable_io_error(&error) => continue,
            Err(error) => return Err(error),
        };

        if metadata.file_type().is_symlink() {
            continue;
        }

        if metadata.is_file() {
            total = total.saturating_add(metadata.len());
            continue;
        }

        if metadata.is_dir() {
            let entries = match fs::read_dir(&current) {
                Ok(entries) => entries,
                Err(error) if is_ignorable_io_error(&error) => continue,
                Err(error) => return Err(error),
            };

            for entry in entries.flatten() {
                stack.push(entry.path());
            }
        }
    }

    Ok(total)
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];

    if bytes < 1024 {
        return format!("{bytes} B");
    }

    let mut size = bytes as f64;
    let mut unit = 0usize;

    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    format!("{size:.1} {}", UNITS[unit])
}

pub fn run_capture(program: &str, args: &[&str]) -> io::Result<Output> {
    Command::new(program).args(args).output()
}

pub fn run_capture_with_env(
    program: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> io::Result<Output> {
    let mut command = Command::new(program);
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output()
}

pub fn output_text(output: &Output) -> String {
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text.trim().to_string()
}

/// 解析外部工具输出里的大小（如 `1.5 GiB`、`4G`、`1,024.0 MiB`）。
/// 供规则解析 paccache / journalctl 的摘要行使用。
pub fn parse_human_bytes(value: &str) -> Option<u64> {
    let value = value.trim().trim_end_matches('.').replace(',', "");
    if value.is_empty() {
        return None;
    }

    let mut number_end = 0usize;
    for (index, character) in value.char_indices() {
        if character.is_ascii_digit() || character == '.' {
            number_end = index + character.len_utf8();
        } else {
            break;
        }
    }

    let number = value[..number_end].trim();
    let unit = value[number_end..]
        .trim()
        .replace(' ', "")
        .to_ascii_uppercase();
    let amount = number.parse::<f64>().ok()?;

    let multiplier = match unit.as_str() {
        "" | "B" => 1f64,
        "K" | "KB" | "KIB" => 1024f64,
        "M" | "MB" | "MIB" => 1024f64.powi(2),
        "G" | "GB" | "GIB" => 1024f64.powi(3),
        "T" | "TB" | "TIB" => 1024f64.powi(4),
        "P" | "PB" | "PIB" => 1024f64.powi(5),
        "E" | "EB" | "EIB" => 1024f64.powi(6),
        _ => return None,
    };

    Some((amount * multiplier).round() as u64)
}

pub fn count_non_empty_lines(text: &str) -> usize {
    text.lines().filter(|line| !line.trim().is_empty()).count()
}

/// 读取当前所有进程的进程名（/proc/<pid>/comm，小写）。
/// 与清理脚本里 `pgrep -x <name>` 匹配的是同一个东西，扫描端的
/// “应用正在运行”判定用它保持两边行为一致。
pub fn running_process_names() -> Vec<String> {
    let mut names = Vec::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return names;
    };

    for entry in entries.flatten() {
        if !is_proc_pid_entry(&entry.file_name()) {
            continue;
        }
        let comm_path = Path::new("/proc").join(entry.file_name()).join("comm");
        let Ok(raw) = fs::read_to_string(&comm_path) else {
            continue;
        };
        if let Some(name) = raw.lines().next() {
            let name = name.trim().to_ascii_lowercase();
            if !name.is_empty() {
                names.push(name);
            }
        }
    }

    names
}

/// 读取当前所有进程的完整命令行（/proc/<pid>/cmdline，NUL 分隔转空格）。
/// 进程名（comm）不够用的场合使用，例如识别 Gradle 守护进程。
pub fn running_process_cmdlines() -> Vec<String> {
    let mut cmdlines = Vec::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return cmdlines;
    };

    for entry in entries.flatten() {
        if !is_proc_pid_entry(&entry.file_name()) {
            continue;
        }
        let cmdline_path = Path::new("/proc").join(entry.file_name()).join("cmdline");
        let Ok(raw) = fs::read(&cmdline_path) else {
            continue;
        };
        let line = raw
            .split(|byte| *byte == 0)
            .map(|argument| String::from_utf8_lossy(argument).into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        if !line.trim().is_empty() {
            cmdlines.push(line);
        }
    }

    cmdlines
}

fn is_proc_pid_entry(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_digit())
}

pub fn path_display(path: &Path) -> String {
    shell_quote(path.as_os_str())
}

fn shell_quote(value: &OsStr) -> String {
    let value = value.to_string_lossy();
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// 扫描/遍历时可安全跳过的 IO 错误（目录消失、无权限等）；
/// rules 与 platform 的遍历共用同一份判定。
pub fn is_ignorable_io_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound
            | io::ErrorKind::PermissionDenied
            | io::ErrorKind::InvalidInput
            | io::ErrorKind::BrokenPipe
    )
}

#[cfg(test)]
mod tests {
    use super::{count_non_empty_lines, format_bytes, parse_human_bytes};

    #[test]
    fn formats_bytes_with_binary_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MiB");
    }

    #[test]
    fn counts_only_non_empty_lines() {
        assert_eq!(count_non_empty_lines("a\n\n b \n"), 2);
    }

    #[test]
    fn parses_compact_human_bytes() {
        assert_eq!(parse_human_bytes("1.5 GiB"), Some(1_610_612_736));
        assert_eq!(parse_human_bytes("4G"), Some(4_294_967_296));
        assert_eq!(parse_human_bytes("1,024.0 MiB"), Some(1_073_741_824));
        assert_eq!(parse_human_bytes("wat"), None);
    }
}
