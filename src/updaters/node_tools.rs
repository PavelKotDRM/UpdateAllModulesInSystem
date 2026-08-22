//! Обновляторы для Node.js-экосистемы (`npm`, `pnpm`) и проверка версии Node.js.

use crate::model::PackageUpdate;
use crate::system;
use crate::updater::{CommandOutput, UpdaterError, capture_command, find_command, stream_command};
use crate::updaters::common::{ensure_success, stream_checked};
use crate::updaters::http_client::http_client;
use serde_json::Value;
use std::fs::{self, File};
use std::io;
use std::path::Path;
use std::sync::mpsc::Sender;

const SELF_UPDATE_SCOPE: &str = "самообновление";
const PNPM_EXECUTABLE_PACKAGE: &str = "@pnpm/exe";
const NODE_RELEASE_INDEX_URL: &str = "https://nodejs.org/dist/index.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeInstallMethod {
    Nvm,
    Fnm,
    Winget(&'static str),
    Homebrew,
    OfficialMsi,
    Unknown,
}

pub(super) fn npm_installed() -> bool {
    system::command_available("npm")
}

pub(super) fn npm_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let mut updates = check_cli_version("npm", "npm", "npm")?;
    let output = capture_command(
        "npm",
        &[
            "outdated".to_owned(),
            "--global".to_owned(),
            "--json".to_owned(),
        ],
    )?;
    updates.extend(
        parse_outdated_packages("npm", &output)?
            .into_iter()
            .filter(|update| update.name != "npm"),
    );
    Ok(updates)
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
    let mut updates = check_cli_version("pnpm", "pnpm", "pnpm")?;
    let output = capture_command(
        "pnpm",
        &[
            "outdated".to_owned(),
            "--global".to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    )?;
    updates.extend(
        parse_outdated_packages("pnpm", &output)?
            .into_iter()
            .filter(|update| !is_pnpm_self_package(&update.name)),
    );
    Ok(updates)
}

fn is_pnpm_self_package(package_name: &str) -> bool {
    matches!(package_name, "pnpm" | PNPM_EXECUTABLE_PACKAGE)
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

    let (self_updates, global_updates): (Vec<_>, Vec<_>) = updates
        .into_iter()
        .partition(|update| update.scope.as_deref() == Some(SELF_UPDATE_SCOPE));
    for update in self_updates {
        stream_checked("pnpm", &pnpm_self_update_args(&update), log_sender)?;
    }

    if global_updates.is_empty() {
        return Ok(());
    }

    let mut args = vec![
        "update".to_owned(),
        "--global".to_owned(),
        "--latest".to_owned(),
    ];
    args.extend(global_updates.into_iter().map(|update| update.name));
    stream_checked("pnpm", &args, log_sender)
}

fn pnpm_self_update_args(update: &PackageUpdate) -> Vec<String> {
    vec!["self-update".to_owned(), update.available_version.clone()]
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

    let releases: Value = http_client()?
        .get(NODE_RELEASE_INDEX_URL)
        .send()?
        .error_for_status()?
        .json()?;
    let latest = latest_node_version(&releases).ok_or_else(|| {
        UpdaterError::Message("node: официальный сайт не вернул последнюю версию".to_owned())
    })?;

    if current == latest {
        Ok(Vec::new())
    } else {
        Ok(vec![PackageUpdate::new("node", current, latest)])
    }
}

fn latest_node_version(releases: &Value) -> Option<String> {
    releases
        .as_array()
        .and_then(|items| items.first())
        .and_then(|release| release.get("version"))
        .and_then(Value::as_str)
        .and_then(version_from_output)
}

pub(super) fn node_apply_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let updates = selected_or_detected(selected_updates, node_check_updates)?;
    let Some(update) = updates.first() else {
        let _ = log_sender.send("node: обновления не найдены".to_owned());
        return Ok(());
    };

    let method = detect_node_install_method();
    let _ = log_sender.send(format!(
        "node: определен способ установки: {}",
        node_install_method_label(method)
    ));
    match method {
        NodeInstallMethod::Nvm => {
            stream_checked("nvm", &["install".to_owned(), update.available_version.clone()], log_sender)?;
            stream_checked("nvm", &["use".to_owned(), update.available_version.clone()], log_sender)
        }
        NodeInstallMethod::Fnm => stream_checked(
            "fnm",
            &[
                "install".to_owned(),
                update.available_version.clone(),
                "--use".to_owned(),
            ],
            log_sender,
        ),
        NodeInstallMethod::Winget(package_id) => {
            let mut args = vec![
                "upgrade".to_owned(),
                "--id".to_owned(),
                package_id.to_owned(),
                "--exact".to_owned(),
                "--accept-source-agreements".to_owned(),
                "--accept-package-agreements".to_owned(),
            ];
            if force_yes {
                args.push("--silent".to_owned());
            }
            stream_checked("winget", &args, log_sender)
        }
        NodeInstallMethod::Homebrew => {
            stream_checked("brew", &["upgrade".to_owned(), "node".to_owned()], log_sender)
        }
        NodeInstallMethod::OfficialMsi => {
            install_official_node_msi(&update.available_version, log_sender)
        }
        NodeInstallMethod::Unknown => Err(UpdaterError::Message(
            "node: не удалось определить способ установки; обновите Node.js вручную через исходный менеджер или официальный установщик".to_owned(),
        )),
    }
}

fn detect_node_install_method() -> NodeInstallMethod {
    let node_path = find_command("node").unwrap_or_default();
    let lower_path = node_path.to_ascii_lowercase();
    if find_command("nvm").is_some() || lower_path.contains("nvm") {
        return NodeInstallMethod::Nvm;
    }
    if find_command("fnm").is_some() || lower_path.contains("fnm") {
        return NodeInstallMethod::Fnm;
    }
    if find_command("brew").is_some() && command_succeeds("brew", &["list", "node"]) {
        return NodeInstallMethod::Homebrew;
    }
    if cfg!(target_os = "windows")
        && (windows_node_msi_registered() || is_official_windows_node_path(&node_path))
    {
        return NodeInstallMethod::OfficialMsi;
    }
    if let Some(package_id) = winget_node_package() {
        return NodeInstallMethod::Winget(package_id);
    }
    NodeInstallMethod::Unknown
}

fn winget_node_package() -> Option<&'static str> {
    find_command("winget")?;
    ["OpenJS.NodeJS", "OpenJS.NodeJS.LTS"]
        .into_iter()
        .find(|package_id| {
            command_succeeds(
                "winget",
                &[
                    "list",
                    "--id",
                    package_id,
                    "--exact",
                    "--accept-source-agreements",
                ],
            )
        })
}

fn command_succeeds(program: &str, args: &[&str]) -> bool {
    capture_command(
        program,
        &args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
    )
    .is_ok_and(|output| output.success)
}

fn windows_node_msi_registered() -> bool {
    if !cfg!(target_os = "windows") || find_command("reg").is_none() {
        return false;
    }
    [
        r"HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall",
        r"HKLM\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall",
    ]
    .into_iter()
    .any(|key| {
        capture_command(
            "reg",
            &[
                "query".to_owned(),
                key.to_owned(),
                "/s".to_owned(),
                "/f".to_owned(),
                "Node.js".to_owned(),
            ],
        )
        .is_ok_and(|output| output.success && output.merged_text().contains("Node.js"))
    })
}

fn is_official_windows_node_path(path: &str) -> bool {
    let normalized = path.replace('/', "\\").to_ascii_lowercase();
    normalized.ends_with(r"\nodejs\node.exe")
        && (normalized.contains(r"\program files\")
            || normalized.contains(r"\program files (x86)\"))
}

fn node_install_method_label(method: NodeInstallMethod) -> &'static str {
    match method {
        NodeInstallMethod::Nvm => "nvm",
        NodeInstallMethod::Fnm => "fnm",
        NodeInstallMethod::Winget(_) => "winget",
        NodeInstallMethod::Homebrew => "Homebrew",
        NodeInstallMethod::OfficialMsi => "официальный MSI",
        NodeInstallMethod::Unknown => "неизвестен",
    }
}

fn install_official_node_msi(
    version: &str,
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let architecture = node_msi_architecture().ok_or_else(|| {
        UpdaterError::Message(format!(
            "node: официальный MSI не поддерживает архитектуру {}",
            std::env::consts::ARCH
        ))
    })?;
    let url = node_msi_url(version, architecture);
    let path = std::env::temp_dir().join(format!("node-v{version}-{architecture}.msi"));
    let _ = log_sender.send(format!("node: загрузка официального установщика {url}"));
    download_file(&url, &path)?;

    let args = vec![
        "/i".to_owned(),
        path.to_string_lossy().into_owned(),
        "/passive".to_owned(),
        "/norestart".to_owned(),
    ];
    let result = stream_command("msiexec", &args, log_sender).and_then(|output| {
        if output.success || output.exit_code == Some(3010) {
            Ok(())
        } else {
            ensure_success("msiexec", &output)
        }
    });
    let _ = fs::remove_file(path);
    result
}

fn node_msi_architecture() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Some("x64"),
        "aarch64" => Some("arm64"),
        "x86" => Some("x86"),
        _ => None,
    }
}

fn node_msi_url(version: &str, architecture: &str) -> String {
    format!("https://nodejs.org/dist/v{version}/node-v{version}-{architecture}.msi")
}

fn download_file(url: &str, path: &Path) -> Result<(), UpdaterError> {
    let mut response = http_client()?.get(url).send()?.error_for_status()?;
    let mut file = File::create(path).map_err(|error| {
        UpdaterError::Message(format!(
            "node: не удалось создать {}: {error}",
            path.display()
        ))
    })?;
    io::copy(&mut response, &mut file).map_err(|error| {
        UpdaterError::Message(format!(
            "node: не удалось сохранить {}: {error}",
            path.display()
        ))
    })?;
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

fn check_cli_version(
    program: &'static str,
    registry_program: &'static str,
    registry_package: &'static str,
) -> Result<Vec<PackageUpdate>, UpdaterError> {
    let current_output = capture_command(program, &["--version".to_owned()])?;
    ensure_success(program, &current_output)?;
    let current = version_from_output(&current_output.stdout).ok_or_else(|| {
        UpdaterError::Message(format!("{program}: не удалось определить текущую версию"))
    })?;

    let latest_output = capture_command(
        registry_program,
        &[
            "view".to_owned(),
            registry_package.to_owned(),
            "version".to_owned(),
            "--json".to_owned(),
        ],
    )?;
    ensure_success(registry_program, &latest_output)?;
    let latest = version_from_output(&latest_output.stdout).ok_or_else(|| {
        UpdaterError::Message(format!(
            "{program}: npm registry не вернул последнюю версию"
        ))
    })?;

    Ok((current != latest)
        .then(|| PackageUpdate::new(program, current, latest).with_scope(SELF_UPDATE_SCOPE))
        .into_iter()
        .collect())
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
    if manager == "npm" && is_missing_npm_global_directory(&value) {
        return Ok(Vec::new());
    }

    let mut updates = Vec::new();
    collect_outdated_packages(&value, None, &mut updates);
    if updates.is_empty() && !output.success {
        ensure_success(manager, output)?;
    }
    Ok(updates)
}

fn is_missing_npm_global_directory(value: &Value) -> bool {
    let Some(error) = value.get("error") else {
        return false;
    };
    error.get("code").and_then(Value::as_str) == Some("ENOENT")
        && error
            .get("summary")
            .and_then(Value::as_str)
            .is_some_and(|summary| {
                summary.contains("no such file or directory") && summary.contains("lstat")
            })
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
    let value = match serde_json::from_str::<Value>(trimmed) {
        Ok(Value::String(version)) => version,
        Ok(Value::Array(versions)) if versions.len() == 1 => versions.first()?.as_str()?.to_owned(),
        Ok(Value::Array(_)) | Ok(Value::Object(_)) | Ok(Value::Null) => return None,
        Ok(Value::Bool(_)) | Ok(Value::Number(_)) | Err(_) => trimmed.to_owned(),
    };
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
    fn pnpm_global_updates_exclude_the_self_managed_executable() {
        let output = command_output(
            r#"[{"name":"@pnpm/exe","current":"11.21.0","latest":"11.22.0"},{"name":"typescript","current":"5.8.0","latest":"5.9.0"}]"#,
            true,
        );

        let updates = parse_outdated_packages("pnpm", &output)
            .expect("pnpm json should parse")
            .into_iter()
            .filter(|update| !is_pnpm_self_package(&update.name))
            .collect::<Vec<_>>();

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "typescript");
    }

    #[test]
    fn missing_npm_global_directory_means_no_global_packages() {
        let output = command_output(
            r#"{"error":{"code":"ENOENT","summary":"ENOENT: no such file or directory, lstat 'C:\\Users\\admin\\AppData\\Roaming\\npm'","detail":"This is related to npm not being able to find a file."}}"#,
            false,
        );

        let updates = parse_outdated_packages("npm", &output)
            .expect("missing npm global directory should be treated as empty");
        assert!(updates.is_empty());
    }

    #[test]
    fn unrelated_npm_enoent_remains_an_error() {
        let output = command_output(
            r#"{"error":{"code":"ENOENT","summary":"missing package metadata"}}"#,
            false,
        );

        assert!(parse_outdated_packages("npm", &output).is_err());
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
        assert_eq!(
            version_from_output("[\n  \"12.0.1\"\n]\n").as_deref(),
            Some("12.0.1")
        );
        assert_eq!(version_from_output("[]"), None);
        assert_eq!(version_from_output(r#"["12.0.1","12.0.0"]"#), None);
    }

    #[test]
    fn pnpm_self_update_is_distinguished_from_global_packages() {
        let self_update =
            PackageUpdate::new("pnpm", "10.0.0", "11.0.0").with_scope(SELF_UPDATE_SCOPE);
        let global_update = PackageUpdate::new("typescript", "5.0.0", "5.1.0");
        let (self_updates, global_updates): (Vec<_>, Vec<_>) = vec![self_update, global_update]
            .into_iter()
            .partition(|update| update.scope.as_deref() == Some(SELF_UPDATE_SCOPE));

        assert_eq!(self_updates.len(), 1);
        assert_eq!(self_updates[0].name, "pnpm");
        assert_eq!(
            pnpm_self_update_args(&self_updates[0]),
            vec!["self-update", "11.0.0"]
        );
        assert_eq!(global_updates.len(), 1);
        assert_eq!(global_updates[0].name, "typescript");
    }

    #[test]
    fn reads_latest_version_from_official_node_index() {
        let releases = serde_json::json!([
            { "version": "v26.5.0", "lts": false },
            { "version": "v26.4.0", "lts": false }
        ]);

        assert_eq!(latest_node_version(&releases).as_deref(), Some("26.5.0"));
    }

    #[test]
    fn recognizes_official_windows_msi_path_and_url() {
        assert!(is_official_windows_node_path(
            r"C:\Program Files\nodejs\node.exe"
        ));
        assert!(!is_official_windows_node_path(
            r"C:\Users\admin\scoop\apps\nodejs\node.exe"
        ));
        assert_eq!(
            node_msi_url("26.5.0", "x64"),
            "https://nodejs.org/dist/v26.5.0/node-v26.5.0-x64.msi"
        );
    }
}
