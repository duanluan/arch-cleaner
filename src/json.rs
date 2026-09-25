use std::fmt::Write;

use crate::executor::{CommandStatus, ExecutionMode, ExecutionResult, commands_for_target};
use crate::i18n::Language;
use crate::model::{CleanerOptions, CleanupCommand, CleanupTarget, ScanReport, ScanStatus};

pub fn targets(targets: &[CleanupTarget], options: &CleanerOptions, language: Language) -> String {
    let mut json = String::new();
    json.push_str("{\"format\":\"arch-cleaner.targets.v1\",");
    push_string_field(&mut json, "language", language.code());
    json.push_str(",\"targets\":");
    push_targets(&mut json, targets, None, options, language);
    json.push('}');
    json
}

pub fn scan_reports(
    reports: &[ScanReport],
    options: &CleanerOptions,
    language: Language,
) -> String {
    let mut json = String::new();
    json.push_str("{\"format\":\"arch-cleaner.scan.v1\",");
    push_string_field(&mut json, "language", language.code());
    json.push_str(",\"summary\":");
    push_scan_summary(&mut json, reports);
    json.push_str(",\"reports\":[");

    for (index, report) in reports.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }

        json.push('{');
        push_string_field(&mut json, "target_id", &report.target_id);
        json.push(',');
        push_string_field(&mut json, "title", report.title.get(language));
        json.push(',');
        push_string_field(&mut json, "group", report.group.as_str());
        json.push(',');
        push_string_field(
            &mut json,
            "threshold",
            &(report.threshold_summary)(options, language),
        );
        json.push(',');
        push_string_field(&mut json, "status", &report.status.to_string());
        json.push_str(",\"estimated_bytes\":");
        push_optional_u64(&mut json, report.estimated_bytes);
        json.push_str(",\"estimated_items\":");
        push_optional_usize(&mut json, report.estimated_items);
        json.push_str(",\"details\":");
        push_string_array(&mut json, &report.details);
        json.push_str(",\"warnings\":");
        push_string_array(&mut json, &report.warnings);
        json.push('}');
    }

    json.push_str("]}");
    json
}

pub fn clean_result(
    targets: &[CleanupTarget],
    mode: ExecutionMode,
    executed: bool,
    results: &[ExecutionResult],
    options: &CleanerOptions,
    language: Language,
) -> String {
    let mut json = String::new();
    json.push_str("{\"format\":\"arch-cleaner.clean.v1\",");
    push_string_field(&mut json, "language", language.code());
    json.push_str(",\"mode\":");
    push_string(&mut json, mode.as_str());
    json.push_str(",\"executed\":");
    json.push_str(if executed { "true" } else { "false" });
    json.push_str(",\"summary\":");
    push_clean_summary(&mut json, results);
    json.push_str(",\"targets\":");
    push_targets(&mut json, targets, Some(mode), options, language);
    json.push_str(",\"results\":[");

    for (index, result) in results.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }

        json.push('{');
        push_string_field(&mut json, "target_id", &result.target_id);
        json.push(',');
        push_string_field(&mut json, "command", &result.command);
        json.push_str(",\"status\":");
        push_status(&mut json, &result.status);
        json.push_str(",\"stdout\":");
        push_string(&mut json, &result.stdout);
        json.push_str(",\"stderr\":");
        push_string(&mut json, &result.stderr);
        json.push('}');
    }

    json.push_str("]}");
    json
}

fn push_clean_summary(json: &mut String, results: &[ExecutionResult]) {
    let planned = results
        .iter()
        .filter(|result| matches!(result.status, CommandStatus::Planned))
        .count();
    let succeeded = results
        .iter()
        .filter(|result| matches!(result.status, CommandStatus::Success(_)))
        .count();
    let failed = results
        .iter()
        .filter(|result| matches!(result.status, CommandStatus::Failed(_)))
        .count();
    let could_not_start = results
        .iter()
        .filter(|result| matches!(result.status, CommandStatus::CouldNotStart(_)))
        .count();

    json.push('{');
    write!(
        json,
        "\"planned\":{planned},\"success\":{succeeded},\"failed\":{failed},\"could_not_start\":{could_not_start}"
    )
    .expect("write to string cannot fail");
    json.push('}');
}

fn push_scan_summary(json: &mut String, reports: &[ScanReport]) {
    let total_bytes = reports
        .iter()
        .filter_map(|report| report.estimated_bytes)
        .fold(0u64, |total, bytes| total.saturating_add(bytes));
    let total_items = reports
        .iter()
        .filter_map(|report| report.estimated_items)
        .fold(0usize, |total, items| total.saturating_add(items));
    let has_bytes = reports
        .iter()
        .any(|report| report.estimated_bytes.is_some());
    let has_items = reports
        .iter()
        .any(|report| report.estimated_items.is_some());
    let ready = reports
        .iter()
        .filter(|report| report.status == ScanStatus::Ready)
        .count();
    let missing_tool = reports
        .iter()
        .filter(|report| report.status == ScanStatus::MissingTool)
        .count();
    let unavailable = reports
        .iter()
        .filter(|report| report.status == ScanStatus::Unavailable)
        .count();

    json.push('{');
    json.push_str("\"estimated_bytes\":");
    if has_bytes {
        write!(json, "{total_bytes}").expect("write to string cannot fail");
    } else {
        json.push_str("null");
    }
    json.push_str(",\"estimated_items\":");
    if has_items {
        write!(json, "{total_items}").expect("write to string cannot fail");
    } else {
        json.push_str("null");
    }
    json.push_str(",\"status_counts\":{");
    write!(
        json,
        "\"ready\":{ready},\"missing_tool\":{missing_tool},\"unavailable\":{unavailable}"
    )
    .expect("write to string cannot fail");
    json.push_str("}}");
}

fn push_targets(
    json: &mut String,
    targets: &[CleanupTarget],
    mode: Option<ExecutionMode>,
    options: &CleanerOptions,
    language: Language,
) {
    json.push('[');

    for (index, target) in targets.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }

        json.push('{');
        push_string_field(json, "id", target.id);
        json.push(',');
        push_string_field(json, "title", target.title.get(language));
        json.push(',');
        push_string_field(json, "description", target.description.get(language));
        json.push(',');
        push_string_field(json, "group", target.group.as_str());
        json.push(',');
        push_string_field(
            json,
            "threshold",
            &(target.threshold_summary)(options, language),
        );
        json.push(',');
        push_string_field(json, "risk", &target.risk.to_string());
        json.push_str(",\"requires_sudo\":");
        json.push_str(if target.requires_sudo {
            "true"
        } else {
            "false"
        });

        if let Some(mode) = mode {
            json.push_str(",\"commands\":");
            push_commands(json, commands_for_target(target, mode));
        }

        json.push('}');
    }

    json.push(']');
}

fn push_commands(json: &mut String, commands: &[CleanupCommand]) {
    json.push('[');

    for (index, command) in commands.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }

        json.push('{');
        push_string_field(json, "display", &command.display);
        json.push(',');
        push_string_field(json, "program", &command.program);
        json.push_str(",\"args\":");
        push_string_array(json, &command.args);
        json.push_str(",\"needs_sudo\":");
        json.push_str(if command.needs_sudo { "true" } else { "false" });
        json.push('}');
    }

    json.push(']');
}

fn push_status(json: &mut String, status: &CommandStatus) {
    json.push('{');

    match status {
        CommandStatus::Planned => {
            push_string_field(json, "kind", "planned");
            json.push_str(",\"code\":null,\"error\":null");
        }
        CommandStatus::Success(code) => {
            push_string_field(json, "kind", "success");
            write!(json, ",\"code\":{code},\"error\":null").expect("write to string cannot fail");
        }
        CommandStatus::Failed(code) => {
            push_string_field(json, "kind", "failed");
            write!(json, ",\"code\":{code},\"error\":null").expect("write to string cannot fail");
        }
        CommandStatus::CouldNotStart(error) => {
            push_string_field(json, "kind", "could-not-start");
            json.push_str(",\"code\":null,\"error\":");
            push_string(json, error);
        }
    }

    json.push('}');
}

fn push_string_field(json: &mut String, key: &str, value: &str) {
    push_string(json, key);
    json.push(':');
    push_string(json, value);
}

fn push_string_array(json: &mut String, values: &[String]) {
    json.push('[');

    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }

        push_string(json, value);
    }

    json.push(']');
}

fn push_optional_u64(json: &mut String, value: Option<u64>) {
    match value {
        Some(value) => write!(json, "{value}").expect("write to string cannot fail"),
        None => json.push_str("null"),
    }
}

fn push_optional_usize(json: &mut String, value: Option<usize>) {
    match value {
        Some(value) => write!(json, "{value}").expect("write to string cannot fail"),
        None => json.push_str("null"),
    }
}

fn push_string(json: &mut String, value: &str) {
    json.push('"');

    for character in value.chars() {
        match character {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            '\n' => json.push_str("\\n"),
            '\r' => json.push_str("\\r"),
            '\t' => json.push_str("\\t"),
            '\u{08}' => json.push_str("\\b"),
            '\u{0c}' => json.push_str("\\f"),
            character if character.is_control() => {
                write!(json, "\\u{:04x}", character as u32).expect("write to string cannot fail");
            }
            character => json.push(character),
        }
    }

    json.push('"');
}

#[cfg(test)]
mod tests {
    use super::{clean_result, scan_reports, targets};
    use crate::executor::{CommandStatus, ExecutionMode, ExecutionResult};
    use crate::i18n::Language;
    use crate::model::{CleanerOptions, ScanReport};
    use crate::rules::all_targets;

    #[test]
    fn serializes_targets_with_stable_format_marker() {
        let options = CleanerOptions::default();
        let cleanup_targets = all_targets(&options);
        let json = targets(&cleanup_targets, &options, Language::En);

        assert!(json.starts_with("{\"format\":\"arch-cleaner.targets.v1\","));
        assert!(json.contains("\"language\":\"en\""));
        assert!(json.contains("\"id\":\"pacman-cache\""));
        assert!(json.contains("\"group\":\"packages\""));
        assert!(json.contains("\"requires_sudo\":true"));
    }

    #[test]
    fn serializes_scan_reports_with_null_size_and_escaped_strings() {
        let options = CleanerOptions::default();
        let targets = all_targets(&options);
        let mut report = ScanReport::new(&targets[0]);
        report.details.push("quote: \" newline:\n".to_string());

        let json = scan_reports(&[report], &options, Language::ZhCn);

        assert!(json.contains("\"format\":\"arch-cleaner.scan.v1\""));
        assert!(json.contains("\"language\":\"zh-CN\""));
        assert!(json.contains("\"summary\":{\"estimated_bytes\":null,\"estimated_items\":null"));
        assert!(json.contains("\"group\":\"packages\""));
        assert!(json.contains("\"estimated_bytes\":null"));
        assert!(json.contains("\"estimated_items\":null"));
        assert!(json.contains("quote: \\\" newline:\\n"));
    }

    #[test]
    fn serializes_scan_summary_totals() {
        let options = CleanerOptions::default();
        let targets = all_targets(&options);
        let mut report = ScanReport::new(&targets[0]);
        report.estimated_bytes = Some(1024);
        report.estimated_items = Some(2);
        let json = scan_reports(&[report], &options, Language::En);

        assert!(json.contains("\"summary\":{\"estimated_bytes\":1024,\"estimated_items\":2"));
    }

    #[test]
    fn serializes_clean_plan_and_results() {
        let options = CleanerOptions::default();
        let targets = all_targets(&options);
        let result = ExecutionResult {
            target_id: "pacman-cache".to_string(),
            command: "paccache -d -k 3".to_string(),
            status: CommandStatus::Planned,
            stdout: String::new(),
            stderr: String::new(),
        };

        let json = clean_result(
            &targets[..1],
            ExecutionMode::DryRun,
            false,
            &[result],
            &options,
            Language::En,
        );

        assert!(json.contains("\"format\":\"arch-cleaner.clean.v1\""));
        assert!(json.contains("\"language\":\"en\""));
        assert!(json.contains("\"mode\":\"dry-run\""));
        assert!(json.contains(
            "\"summary\":{\"planned\":1,\"success\":0,\"failed\":0,\"could_not_start\":0}"
        ));
        assert!(json.contains("\"commands\":["));
        assert!(json.contains("\"kind\":\"planned\""));
    }
}
