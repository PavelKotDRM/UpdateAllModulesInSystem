//! Обновление расширений VS Code-совместимых редакторов через их CLI.

use crate::model::PackageUpdate;
use crate::updater::{CommandOutput, UpdaterError, capture_command, find_command};
use crate::updaters::common::stream_checked;
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::mpsc::Sender;

const MARKETPLACE_QUERY_URL: &str =
    "https://marketplace.visualstudio.com/_apis/public/gallery/extensionquery";
const OPEN_VSX_API_URL: &str = "https://open-vsx.org/api";

#[derive(Debug, PartialEq)]
struct InstalledExtension {
    id: String,
    version: String,
}

fn editor_installed(program: &str) -> bool {
    find_command(program).is_some()
}

fn check_editor_extensions(program: &str) -> Result<Vec<PackageUpdate>, UpdaterError> {
    let command = editor_command(program)?;
    let output = capture_command(
        &command,
        &["--list-extensions".to_owned(), "--show-versions".to_owned()],
    )?;
    ensure_success(&command, &output)?;
    let installed = parse_editor_extensions(&output.stdout);
    let available_versions = if matches!(program, "code" | "code-insiders") {
        fetch_marketplace_versions(&installed)?
    } else {
        fetch_open_vsx_versions(&installed)?
    };

    Ok(collect_extension_updates(installed, &available_versions))
}

fn fetch_marketplace_versions(
    installed: &[InstalledExtension],
) -> Result<HashMap<String, String>, UpdaterError> {
    if installed.is_empty() {
        return Ok(HashMap::new());
    }

    let filters: Vec<Value> = installed
        .iter()
        .map(|extension| {
            json!({
                "criteria": [{ "filterType": 7, "value": extension.id }],
                "pageNumber": 1,
                "pageSize": 1,
                "sortBy": 0,
                "sortOrder": 0
            })
        })
        .collect();
    let response = Client::new()
        .post(MARKETPLACE_QUERY_URL)
        .header("Accept", "application/json;api-version=7.2-preview.1")
        .json(&json!({ "filters": filters, "assetTypes": [], "flags": 17 }))
        .send()?
        .error_for_status()?
        .json()?;

    Ok(parse_marketplace_versions(&response))
}

fn fetch_open_vsx_versions(
    installed: &[InstalledExtension],
) -> Result<HashMap<String, String>, UpdaterError> {
    let client = Client::new();
    let mut versions = HashMap::new();

    for extension in installed {
        let Some((publisher, name)) = extension.id.split_once('.') else {
            continue;
        };
        let response = client
            .get(format!("{OPEN_VSX_API_URL}/{publisher}/{name}/latest"))
            .send()?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            continue;
        }
        let value: Value = response.error_for_status()?.json()?;
        if let Some(version) = value.get("version").and_then(Value::as_str) {
            versions.insert(extension.id.to_ascii_lowercase(), version.to_owned());
        }
    }

    Ok(versions)
}

fn parse_marketplace_versions(response: &Value) -> HashMap<String, String> {
    response
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|result| result.get("extensions").and_then(Value::as_array))
        .flatten()
        .filter_map(|extension| {
            let publisher = extension.pointer("/publisher/publisherName")?.as_str()?;
            let name = extension.get("extensionName")?.as_str()?;
            let version = extension
                .get("versions")?
                .as_array()?
                .iter()
                .find(|version| !is_pre_release(version))?
                .get("version")?
                .as_str()?;
            Some((
                format!("{publisher}.{name}").to_ascii_lowercase(),
                version.to_owned(),
            ))
        })
        .collect()
}

fn is_pre_release(version: &Value) -> bool {
    version
        .get("properties")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|property| {
            property.get("key").and_then(Value::as_str)
                == Some("Microsoft.VisualStudio.Code.PreRelease")
                && property.get("value").and_then(Value::as_str) == Some("true")
        })
}

fn collect_extension_updates(
    installed: Vec<InstalledExtension>,
    available_versions: &HashMap<String, String>,
) -> Vec<PackageUpdate> {
    installed
        .into_iter()
        .filter_map(|extension| {
            let available = available_versions.get(&extension.id.to_ascii_lowercase())?;
            (extension.version != *available)
                .then(|| PackageUpdate::new(extension.id, extension.version, available.clone()))
        })
        .collect()
}

fn apply_editor_extensions(
    program: &str,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let command = editor_command(program)?;
    if selected_updates.is_empty() {
        return stream_checked(&command, &["--update-extensions".to_owned()], log_sender);
    }

    for update in selected_updates {
        stream_checked(
            &command,
            &[
                "--install-extension".to_owned(),
                update.name.clone(),
                "--force".to_owned(),
            ],
            log_sender,
        )?;
    }

    Ok(())
}

fn editor_command(program: &str) -> Result<String, UpdaterError> {
    find_command(program)
        .ok_or_else(|| UpdaterError::Message(format!("{program}: команда редактора не найдена")))
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

fn parse_editor_extensions(text: &str) -> Vec<InstalledExtension> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let (extension_id, current_version) = line.rsplit_once('@')?;
            if extension_id.is_empty() || current_version.is_empty() {
                return None;
            }

            Some(InstalledExtension {
                id: extension_id.to_owned(),
                version: current_version.to_owned(),
            })
        })
        .collect()
}

macro_rules! editor_updater {
    ($installed:ident, $check:ident, $apply:ident, $program:literal) => {
        pub(super) fn $installed() -> bool {
            editor_installed($program)
        }

        pub(super) fn $check() -> Result<Vec<PackageUpdate>, UpdaterError> {
            check_editor_extensions($program)
        }

        pub(super) fn $apply(
            _force_yes: bool,
            selected_updates: &[PackageUpdate],
            log_sender: &Sender<String>,
        ) -> Result<(), UpdaterError> {
            apply_editor_extensions($program, selected_updates, log_sender)
        }
    };
}

editor_updater!(
    code_installed,
    code_check_updates,
    code_apply_updates,
    "code"
);
editor_updater!(
    code_insiders_installed,
    code_insiders_check_updates,
    code_insiders_apply_updates,
    "code-insiders"
);
editor_updater!(
    codium_installed,
    codium_check_updates,
    codium_apply_updates,
    "codium"
);
editor_updater!(
    cursor_installed,
    cursor_check_updates,
    cursor_apply_updates,
    "cursor"
);
editor_updater!(
    windsurf_installed,
    windsurf_check_updates,
    windsurf_apply_updates,
    "windsurf"
);
editor_updater!(
    positron_installed,
    positron_check_updates,
    positron_apply_updates,
    "positron"
);

#[cfg(test)]
mod tests {
    use super::{collect_extension_updates, parse_editor_extensions, parse_marketplace_versions};
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn parses_extension_ids_and_versions() {
        let updates = parse_editor_extensions(
            "rust-lang.rust-analyzer@0.3.2500\nms-python.python@2026.10.0\n",
        );

        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].id, "rust-lang.rust-analyzer");
        assert_eq!(updates[0].version, "0.3.2500");
    }

    #[test]
    fn ignores_cli_noise_and_malformed_lines() {
        let updates = parse_editor_extensions("warning: profile unavailable\ninvalid@\n@1.0.0\n");

        assert!(updates.is_empty());
    }

    #[test]
    fn keeps_only_extensions_with_different_available_version() {
        let installed = parse_editor_extensions(
            "bierner.markdown-mermaid@1.32.1\ndocker.docker@0.17.0\nprivate.extension@1.0.0\n",
        );
        let available = HashMap::from([
            ("bierner.markdown-mermaid".to_owned(), "1.32.1".to_owned()),
            ("docker.docker".to_owned(), "0.18.0".to_owned()),
        ]);

        let updates = collect_extension_updates(installed, &available);

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "docker.docker");
        assert_eq!(updates[0].current_version, "0.17.0");
        assert_eq!(updates[0].available_version, "0.18.0");
    }

    #[test]
    fn parses_marketplace_latest_versions() {
        let response = json!({
            "results": [{
                "extensions": [{
                    "extensionName": "markdown-mermaid",
                    "publisher": { "publisherName": "bierner" },
                    "versions": [
                        {
                            "version": "1.33.20260725",
                            "properties": [{
                                "key": "Microsoft.VisualStudio.Code.PreRelease",
                                "value": "true"
                            }]
                        },
                        { "version": "1.32.1", "properties": [] }
                    ]
                }]
            }]
        });

        let versions = parse_marketplace_versions(&response);

        assert_eq!(versions["bierner.markdown-mermaid"], "1.32.1");
    }
}
