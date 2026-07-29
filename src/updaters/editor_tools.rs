//! Обновление расширений VS Code-совместимых редакторов через их CLI.

use crate::model::PackageUpdate;
use crate::updater::{CommandOutput, UpdaterError, capture_command, find_command};
use crate::updaters::common::stream_checked;
use reqwest::blocking::Client;
use semver::Version;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

const MARKETPLACE_QUERY_URL: &str =
    "https://marketplace.visualstudio.com/_apis/public/gallery/extensionquery";
const OPEN_VSX_API_URL: &str = "https://open-vsx.org/api";
const MARKETPLACE_BATCH_SIZE: usize = 8;
const MARKETPLACE_LATEST_FLAGS: u32 = 1 | 16 | 512;
const MARKETPLACE_HISTORY_FLAGS: u32 = 1 | 16;

#[derive(Debug, PartialEq)]
struct InstalledExtension {
    id: String,
    version: String,
    profile: Option<String>,
}

fn editor_installed(program: &str) -> bool {
    find_command(program).is_some()
}

fn check_editor_extensions(program: &str) -> Result<Vec<PackageUpdate>, UpdaterError> {
    let command = editor_command(program)?;
    let mut installed = Vec::new();
    for profile in editor_profiles(program) {
        let mut args = vec!["--list-extensions".to_owned(), "--show-versions".to_owned()];
        if let Some(profile) = &profile {
            args.extend(["--profile".to_owned(), profile.clone()]);
        }

        let output = capture_command(&command, &args)?;
        ensure_success(&command, &output)?;
        installed.extend(parse_editor_extensions(&output.stdout).into_iter().map(
            |mut extension| {
                extension.profile = profile.clone();
                extension
            },
        ));
    }

    let available_versions = if matches!(program, "code" | "code-insiders") {
        fetch_marketplace_versions(&installed)?
    } else {
        fetch_open_vsx_versions(&installed)?
    };

    let mut updates = collect_extension_updates(installed, &available_versions);
    exclude_builtin_extension_updates(&command, &mut updates);
    Ok(updates)
}

fn fetch_marketplace_versions(
    installed: &[InstalledExtension],
) -> Result<HashMap<String, String>, UpdaterError> {
    if installed.is_empty() {
        return Ok(HashMap::new());
    }

    let mut extension_ids = installed
        .iter()
        .map(|extension| extension.id.to_ascii_lowercase())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    extension_ids.sort_unstable();

    let client = Client::builder().http1_only().build()?;
    let mut versions = HashMap::new();
    for batch in extension_ids.chunks(MARKETPLACE_BATCH_SIZE) {
        versions.extend(fetch_marketplace_batch(
            &client,
            batch,
            MARKETPLACE_LATEST_FLAGS,
        )?);
    }

    let missing_ids = extension_ids
        .iter()
        .filter(|extension_id| !versions.contains_key(*extension_id))
        .cloned()
        .collect::<Vec<_>>();
    for extension_id in &missing_ids {
        versions.extend(fetch_marketplace_batch(
            &client,
            std::slice::from_ref(extension_id),
            MARKETPLACE_HISTORY_FLAGS,
        )?);
    }
    Ok(versions)
}

fn fetch_marketplace_batch(
    client: &Client,
    extension_ids: &[String],
    flags: u32,
) -> Result<HashMap<String, String>, UpdaterError> {
    let filters: Vec<Value> = extension_ids
        .iter()
        .map(|extension_id| {
            json!({
                "criteria": [{ "filterType": 7, "value": extension_id }],
                "pageNumber": 1,
                "pageSize": 1,
                "sortBy": 0,
                "sortOrder": 0
            })
        })
        .collect();
    let response = client
        .post(MARKETPLACE_QUERY_URL)
        .header("Accept", "application/json;api-version=7.2-preview.1")
        .json(&json!({
            "filters": filters,
            "assetTypes": [],
            "flags": flags
        }))
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
    parse_marketplace_versions_for_platform(response, marketplace_target_platform())
}

fn parse_marketplace_versions_for_platform(
    response: &Value,
    target_platform: &str,
) -> HashMap<String, String> {
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
            let versions = extension.get("versions")?.as_array()?;
            let has_target_version = versions.iter().any(|version| {
                version.get("targetPlatform").and_then(Value::as_str) == Some(target_platform)
            });
            let version = versions
                .iter()
                .find(|version| {
                    !is_pre_release(version)
                        && version.get("targetPlatform").and_then(Value::as_str)
                            == Some(target_platform)
                })
                .or_else(|| {
                    (!has_target_version)
                        .then(|| {
                            versions.iter().find(|version| {
                                !is_pre_release(version) && version.get("targetPlatform").is_none()
                            })
                        })
                        .flatten()
                })?
                .get("version")?
                .as_str()?;
            Some((
                format!("{publisher}.{name}").to_ascii_lowercase(),
                version.to_owned(),
            ))
        })
        .collect()
}

fn marketplace_target_platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "win32-x64",
        ("windows", "aarch64") => "win32-arm64",
        ("windows", "x86") => "win32-ia32",
        ("macos", "x86_64") => "darwin-x64",
        ("macos", "aarch64") => "darwin-arm64",
        ("linux", "x86_64") if cfg!(target_env = "musl") => "alpine-x64",
        ("linux", "aarch64") if cfg!(target_env = "musl") => "alpine-arm64",
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("linux", "arm") => "linux-armhf",
        _ => "universal",
    }
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
            is_newer_version(available, &extension.version).then(|| {
                let update = PackageUpdate::new(extension.id, extension.version, available.clone());
                match extension.profile {
                    Some(profile) => update.with_scope(profile),
                    None => update,
                }
            })
        })
        .collect()
}

fn is_newer_version(available: &str, installed: &str) -> bool {
    match (Version::parse(available), Version::parse(installed)) {
        (Ok(available), Ok(installed)) => available > installed,
        _ => false,
    }
}

fn exclude_builtin_extension_updates(command: &str, updates: &mut Vec<PackageUpdate>) {
    let extension_ids = updates
        .iter()
        .map(|update| update.name.clone())
        .collect::<HashSet<_>>();
    let builtin_ids = extension_ids
        .into_iter()
        .filter(|extension_id| {
            let args = vec!["--locate-extension".to_owned(), extension_id.clone()];
            capture_command(command, &args)
                .ok()
                .filter(|output| output.success)
                .is_some_and(|output| is_builtin_extension_location(&output.stdout))
        })
        .collect::<HashSet<_>>();

    updates.retain(|update| !builtin_ids.contains(&update.name));
}

fn is_builtin_extension_location(location: &str) -> bool {
    location
        .replace('\\', "/")
        .to_ascii_lowercase()
        .contains("/resources/app/extensions/")
}

fn apply_editor_extensions(
    program: &str,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let command = editor_command(program)?;
    if selected_updates.is_empty() {
        for profile in editor_profiles(program) {
            let mut args = vec!["--update-extensions".to_owned()];
            if let Some(profile) = profile {
                args.extend(["--profile".to_owned(), profile]);
            }
            stream_checked(&command, &args, log_sender)?;
        }
        return Ok(());
    }

    for update in selected_updates {
        let args = extension_install_args(update);
        stream_checked(&command, &args, log_sender)?;
    }

    Ok(())
}

fn extension_install_args(update: &PackageUpdate) -> Vec<String> {
    let mut args = vec![
        "--install-extension".to_owned(),
        update.name.clone(),
        "--force".to_owned(),
    ];
    if let Some(profile) = &update.scope {
        args.extend(["--profile".to_owned(), profile.clone()]);
    }
    args
}

fn editor_command(program: &str) -> Result<String, UpdaterError> {
    find_command(program)
        .ok_or_else(|| UpdaterError::Message(format!("{program}: команда редактора не найдена")))
}

fn editor_profiles(program: &str) -> Vec<Option<String>> {
    let mut profiles = vec![None];
    let Some(user_dir) = editor_user_dir(program) else {
        return profiles;
    };
    let profile_root = user_dir.join("profiles");
    let Ok(entries) = fs::read_dir(&profile_root) else {
        return profiles;
    };
    let existing_ids = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|id| id != "builtin")
        .collect::<HashSet<_>>();
    if existing_ids.is_empty() {
        return profiles;
    }

    let registry_path = user_dir.join("sync/profiles/lastSyncprofiles.json");
    let Ok(registry) = fs::read_to_string(registry_path) else {
        return profiles;
    };
    let mut names = parse_profile_names(&registry, &existing_ids);
    names.sort_unstable_by_key(|name| name.to_ascii_lowercase());
    profiles.extend(names.into_iter().map(Some));
    profiles
}

fn editor_user_dir(program: &str) -> Option<PathBuf> {
    let product_dir = match program {
        "code" => "Code",
        "code-insiders" => "Code - Insiders",
        _ => return None,
    };

    #[cfg(target_os = "windows")]
    {
        return std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|path| path.join(product_dir).join("User"));
    }

    #[cfg(target_os = "macos")]
    {
        return std::env::var_os("HOME").map(PathBuf::from).map(|path| {
            path.join("Library/Application Support")
                .join(product_dir)
                .join("User")
        });
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let config_dir = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|path| path.join(".config"))
            })?;
        Some(config_dir.join(product_dir).join("User"))
    }
}

fn parse_profile_names(text: &str, existing_ids: &HashSet<String>) -> Vec<String> {
    let Ok(envelope) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let Some(content) = envelope
        .pointer("/syncData/content")
        .and_then(Value::as_str)
    else {
        return Vec::new();
    };
    let Ok(entries) = serde_json::from_str::<Vec<Value>>(content) else {
        return Vec::new();
    };

    entries
        .into_iter()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?;
            let name = entry.get("name")?.as_str()?;
            existing_ids.contains(id).then(|| name.to_owned())
        })
        .collect()
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
                profile: None,
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
    use super::{
        collect_extension_updates, extension_install_args, is_builtin_extension_location,
        parse_editor_extensions, parse_marketplace_versions, parse_marketplace_versions_for_platform,
        parse_profile_names,
    };
    use crate::model::PackageUpdate;
    use serde_json::json;
    use std::collections::{HashMap, HashSet};

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
    fn ignores_marketplace_version_older_than_builtin_extension() {
        let installed = parse_editor_extensions("github.copilot-chat@0.58.0\n");
        let available = HashMap::from([("github.copilot-chat".to_owned(), "0.48.1".to_owned())]);

        let updates = collect_extension_updates(installed, &available);

        assert!(updates.is_empty());
    }

    #[test]
    fn identifies_builtin_extension_locations() {
        assert!(is_builtin_extension_location(
            r"C:\Program Files\Microsoft VS Code\resources\app\extensions\copilot"
        ));
        assert!(is_builtin_extension_location(
            "/usr/share/code/resources/app/extensions/github"
        ));
        assert!(!is_builtin_extension_location(
            r"C:\Users\user\.vscode\extensions\publisher.extension-1.0.0"
        ));
    }

    #[test]
    fn keeps_profile_on_extension_update() {
        let mut installed = parse_editor_extensions("ms-python.python@1.0.0\n");
        installed[0].profile = Some("Python".to_owned());
        let available = HashMap::from([("ms-python.python".to_owned(), "2.0.0".to_owned())]);

        let updates = collect_extension_updates(installed, &available);

        assert_eq!(updates[0].scope.as_deref(), Some("Python"));
    }

    #[test]
    fn installs_extension_into_its_source_profile() {
        let update = PackageUpdate::new("ms-python.python", "1.0.0", "2.0.0").with_scope("Python");

        assert_eq!(
            extension_install_args(&update),
            vec![
                "--install-extension",
                "ms-python.python",
                "--force",
                "--profile",
                "Python"
            ]
        );
    }

    #[test]
    fn parses_only_locally_existing_profile_names() {
        let content = serde_json::to_string(&json!([
            { "id": "python-id", "name": "Python" },
            { "id": "remote-id", "name": "Remote only" }
        ]))
        .unwrap();
        let registry = json!({ "syncData": { "content": content } }).to_string();
        let existing = HashSet::from(["python-id".to_owned()]);

        assert_eq!(parse_profile_names(&registry, &existing), vec!["Python"]);
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

    #[test]
    fn marketplace_prefers_current_platform_over_old_universal_version() {
        let response = json!({
            "results": [{
                "extensions": [{
                    "extensionName": "cpptools",
                    "publisher": { "publisherName": "ms-vscode" },
                    "versions": [
                        { "version": "1.33.4", "targetPlatform": "win32-x64", "properties": [] },
                        { "version": "1.19.7", "targetPlatform": "win32-ia32", "properties": [] },
                        { "version": "1.7.1", "properties": [] }
                    ]
                }]
            }]
        });

        let versions = parse_marketplace_versions_for_platform(&response, "win32-x64");

        assert_eq!(versions["ms-vscode.cpptools"], "1.33.4");
    }

    #[test]
    fn marketplace_does_not_fall_back_when_platform_latest_is_prerelease() {
        let response = json!({
            "results": [{
                "extensions": [{
                    "extensionName": "cpptools",
                    "publisher": { "publisherName": "ms-vscode" },
                    "versions": [
                        {
                            "version": "1.33.4",
                            "targetPlatform": "win32-x64",
                            "properties": [{
                                "key": "Microsoft.VisualStudio.Code.PreRelease",
                                "value": "true"
                            }]
                        },
                        { "version": "1.7.1", "properties": [] }
                    ]
                }]
            }]
        });

        let versions = parse_marketplace_versions_for_platform(&response, "win32-x64");

        assert!(!versions.contains_key("ms-vscode.cpptools"));
    }
}
