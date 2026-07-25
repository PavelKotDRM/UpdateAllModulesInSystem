//! Обновляторы для Node.js-экосистемы (`npm`, `pnpm`) и проверка версии Node.js.

use crate::model::PackageUpdate;
use crate::system;
use crate::updater::{CommandOutput, UpdaterError, capture_command, stream_command};
use serde_json::Value;
use std::sync::mpsc::Sender;

pub(super) fn npm_installed() -> bool {
    system::command_available("npm")
}

pub(super) fn npm_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command(
        "npm",
        &[
            "outdated".to_owned(),
            "--global".to_owned(),
            "--json".to_owned(),
        ],
    )?;
    parse_outdated_packages("npm", &output)
}

pub(super) fn npm_apply_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let updates = selected_or_detected(selected_updates, npm_check_updates)?;
    if updates.is_empty() {
        let _ = log_sender.send("npm: обновления не найдены".to_owned());
        return Ok(());
    }

    let mut args = vec!["install".to_owned(), "--global".to_owned()];
    if force_yes {
        args.push("--yes".to_owned());
    }
    args.extend(
        updates
            .into_iter()
            .map(|update| format!("{}@latest", update.name)),
    );
    stream_checked("npm", &args, log_sender)
}

pub(super) fn pnpm_installed() -> bool {
    system::command_available("pnpm")
}

pub(super) fn pnpm_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command(
        "pnpm",
        &[
            "outdated".to_owned(),
            "--global".to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    )?;
    parse_outdated_packages("pnpm", &output)
}

pub(super) fn pnpm_apply_updates(
    _force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let updates = selected_or_detected(selected_updates, pnpm_check_updates)?;
    if updates.is_empty() {
        let _ = log_sender.send("pnpm: обновления не найдены".to_owned());
        return Ok(());
    }

    let mut args = vec![
        "update".to_owned(),
        "--global".to_owned(),
        "--latest".to_owned(),
    ];
    args.extend(updates.into_iter().map(|update| update.name));
    stream_checked("pnpm", &args, log_sender)
}

pub(super) fn node_installed() -> bool {
    system::command_available("node")
}

pub(super) fn node_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let current_output = capture_command("node", &["--version".to_owned()])?;
    ensure_success("node", &current_output)?;
    let current = version_from_output(&current_output.stdout).ok_or_else(|| {
        UpdaterError::Message("node: не удалось определить текущую версию".to_owned())
    })?;

    if !npm_installed() {
        return Err(UpdaterError::Message(
            "node: для проверки последней версии требуется npm".to_owned(),
        ));
    }

    let latest_output = capture_command(
        "npm",
        &[
            "view".to_owned(),
            "node".to_owned(),
            "version".to_owned(),
            "--json".to_owned(),
        ],
    )?;
    ensure_success("npm", &latest_output)?;
    let latest = version_from_output(&latest_output.stdout)
        .ok_or_else(|| UpdaterError::Message("node: npm не вернул последнюю версию".to_owned()))?;

    if current == latest {
        Ok(Vec::new())
    } else {
        Ok(vec![PackageUpdate::new("node", current, latest)])
    }
}

pub(super) fn node_apply_updates(
    _force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let _ = log_sender.send(
        "node: автоматическая замена системной установки отключена; обновите Node.js через менеджер, которым он был установлен (winget, brew, nvm или fnm)".to_owned(),
    );
    Ok(())
}

fn selected_or_detected(
    selected_updates: &[PackageUpdate],
    check: fn() -> Result<Vec<PackageUpdate>, UpdaterError>,
) -> Result<Vec<PackageUpdate>, UpdaterError> {
    if selected_updates.is_empty() {
        check()
    } else {
        Ok(selected_updates.to_vec())
    }
}

fn stream_checked(
    program: &str,
    args: &[String],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let output = stream_command(program, args, log_sender)?;
    ensure_success(program, &output)
}

fn ensure_success(program: &str, output: &CommandOutput) -> Result<(), UpdaterError> {
    if output.success {
        return Ok(());
    }

    Err(UpdaterError::CommandFailed {
        program: program.to_owned(),
        code: output.exit_code,
        stderr: output.merged_text().trim().to_owned(),
    })
}

fn parse_outdated_packages(
    manager: &'static str,
    output: &CommandOutput,
) -> Result<Vec<PackageUpdate>, UpdaterError> {
    let text = output.stdout.trim();
    if text.is_empty() {
        ensure_success(manager, output)?;
        return Ok(Vec::new());
    }

    let value: Value = serde_json::from_str(text)?;
    let mut updates = Vec::new();
    collect_outdated_packages(&value, None, &mut updates);
    if updates.is_empty() && !output.success {
        ensure_success(manager, output)?;
    }
    Ok(updates)
}

fn collect_outdated_packages(value: &Value, key: Option<&str>, updates: &mut Vec<PackageUpdate>) {
    match value {
        Value::Object(object) => {
            let current = object.get("current").and_then(Value::as_str);
            let latest = object.get("latest").and_then(Value::as_str);
            let name = object.get("name").and_then(Value::as_str).or(key);

            if let (Some(name), Some(current), Some(latest)) = (name, current, latest) {
                if current != latest {
                    updates.push(PackageUpdate::new(name, current, latest));
                }
                return;
            }

            for (child_key, child) in object {
                collect_outdated_packages(child, Some(child_key), updates);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_outdated_packages(item, None, updates);
            }
        }
        _ => {}
    }
}

fn version_from_output(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let value = serde_json::from_str::<String>(trimmed).unwrap_or_else(|_| trimmed.to_owned());
    let version = value.trim().trim_start_matches('v');
    (!version.is_empty()).then(|| version.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command_output(stdout: &str, success: bool) -> CommandOutput {
        CommandOutput {
            stdout: stdout.to_owned(),
            stderr: String::new(),
            exit_code: Some(if success { 0 } else { 1 }),
            success,
        }
    }

    #[test]
    fn parse_outdated_packages_reads_npm_object_on_exit_code_one() {
        let output = command_output(
            r#"{"typescript":{"current":"5.7.2","wanted":"5.8.3","latest":"5.8.3"}}"#,
            false,
        );

        let updates = parse_outdated_packages("npm", &output).expect("npm json should parse");
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "typescript");
        assert_eq!(updates[0].current_version, "5.7.2");
        assert_eq!(updates[0].available_version, "5.8.3");
    }

    #[test]
    fn parse_outdated_packages_reads_array_format() {
        let output = command_output(
            r#"[{"name":"pnpm","current":"9.15.0","latest":"10.0.0"}]"#,
            true,
        );

        let updates = parse_outdated_packages("pnpm", &output).expect("pnpm json should parse");
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "pnpm");
    }

    #[test]
    fn version_from_output_accepts_node_and_json_versions() {
        assert_eq!(
            version_from_output("v22.14.0\n").as_deref(),
            Some("22.14.0")
        );
        assert_eq!(
            version_from_output("\"23.9.0\"\n").as_deref(),
            Some("23.9.0")
        );
    }
}
