use crate::model::{ModuleKind, PackageUpdate};
use crate::system;
use crate::updater::{capture_command, command_exists, heuristic_parse_updates, stream_command, Updater, UpdaterError};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

pub struct UpdaterDescriptor {
    pub updater: Box<dyn Updater>,
    pub kind: ModuleKind,
    pub requires_elevation: bool,
}

macro_rules! define_simple_updater {
    ($struct_name:ident, $display_name:expr, $kind:expr, $requires_elevation:expr, $installed_fn:path, $check_fn:path, $apply_fn:path) => {
        pub struct $struct_name;

        impl Updater for $struct_name {
            fn name(&self) -> &'static str {
                $display_name
            }

            fn is_installed(&self) -> bool {
                $installed_fn()
            }

            fn check_updates(&self) -> Result<Vec<PackageUpdate>, UpdaterError> {
                $check_fn()
            }

            fn apply_updates(
                &self,
                force_yes: bool,
                selected_updates: &[PackageUpdate],
                log_sender: &Sender<String>,
            ) -> Result<(), UpdaterError> {
                $apply_fn(force_yes, selected_updates, log_sender)
            }
        }

        impl $struct_name {
            pub fn descriptor() -> UpdaterDescriptor {
                UpdaterDescriptor {
                    updater: Box::new(Self),
                    kind: $kind,
                    requires_elevation: $requires_elevation,
                }
            }
        }
    };
}

pub fn registry() -> Vec<UpdaterDescriptor> {
    vec![
        WindowsUpdateUpdater::descriptor(),
        WingetUpdater::descriptor(),
        ChocolateyUpdater::descriptor(),
        AptUpdater::descriptor(),
        AptGetUpdater::descriptor(),
        DnfUpdater::descriptor(),
        YumUpdater::descriptor(),
        ZypperUpdater::descriptor(),
        PacmanUpdater::descriptor(),
        ApkUpdater::descriptor(),
        XbpsUpdater::descriptor(),
        EmergeUpdater::descriptor(),
        FlatpakUpdater::descriptor(),
        SnapUpdater::descriptor(),
        PkconUpdater::descriptor(),
        BrewUpdater::descriptor(),
        RustupUpdater::descriptor(),
        Msys2Updater::descriptor(),
        PipUpdater::descriptor(),
        UvUpdater::descriptor(),
    ]
}

pub fn lookup_updater(name: &str) -> Option<UpdaterDescriptor> {
    registry().into_iter().find(|descriptor| descriptor.updater.name() == name)
}

fn python_invocation() -> Option<(String, Vec<String>)> {
    if command_exists("python") {
        Some(("python".to_owned(), Vec::new()))
    } else if command_exists("python3") {
        Some(("python3".to_owned(), Vec::new()))
    } else if command_exists("py") {
        Some(("py".to_owned(), vec!["-3".to_owned()]))
    } else {
        None
    }
}

fn run_python_module(args: &[String]) -> Result<crate::updater::CommandOutput, UpdaterError> {
    let (program, mut prefix) = python_invocation().ok_or_else(|| UpdaterError::Message("python не найден".to_owned()))?;
    prefix.push("-m".to_owned());
    prefix.push("pip".to_owned());
    prefix.extend(args.iter().cloned());
    capture_command(&program, &prefix)
}

fn stream_python_module(args: &[String], log_sender: &Sender<String>) -> Result<(), UpdaterError> {
    let (program, mut prefix) = python_invocation().ok_or_else(|| UpdaterError::Message("python не найден".to_owned()))?;
    prefix.push("-m".to_owned());
    prefix.push("pip".to_owned());
    prefix.extend(args.iter().cloned());
    let output = stream_command(&program, &prefix, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program,
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn parse_pip_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    #[derive(Debug, Deserialize)]
    struct PipPackage {
        name: String,
        version: String,
        latest_version: String,
    }

    let output = run_python_module(&["list".to_owned(), "--outdated".to_owned(), "--format=json".to_owned()])?;
    let packages: Vec<PipPackage> = serde_json::from_str(&output.stdout)?;
    Ok(packages
        .into_iter()
        .map(|package| PackageUpdate::new(package.name, package.version, package.latest_version))
        .collect())
}

fn apply_pip_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let updates = if selected_updates.is_empty() {
        match parse_pip_updates() {
            Ok(values) => values,
            Err(error) if uv_installed() => {
                let _ = log_sender.send(format!(
                    "pip: не удалось получить список через python -m pip ({error}); пробую uv"
                ));
                parse_uv_updates()?
            }
            Err(error) => return Err(error),
        }
    } else {
        selected_updates.to_vec()
    };
    if updates.is_empty() {
        let _ = log_sender.send("pip: обновления не найдены".to_owned());
        return Ok(());
    }

    let mut args = vec!["install".to_owned(), "--upgrade".to_owned()];
    if force_yes {
        args.push("--no-input".to_owned());
    }
    args.extend(updates.into_iter().map(|update| update.name));
    match stream_python_module(&args, log_sender) {
        Ok(()) => Ok(()),
        Err(error) if uv_installed() => {
            let _ = log_sender.send(format!(
                "pip: обновление через python -m pip завершилось ошибкой ({error}); пробую uv"
            ));
            run_uv_pip_stream(&args, log_sender)
        }
        Err(error) => Err(error),
    }
}

fn installed_pip() -> bool {
    python_invocation().is_some()
}

fn check_pip_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    match parse_pip_updates() {
        Ok(updates) => Ok(updates),
        Err(_) if uv_installed() => parse_uv_updates(),
        Err(error) => Err(error),
    }
}

fn uv_installed() -> bool {
    system::command_available("uv")
}

fn run_uv_pip_capture(args: &[String]) -> Result<crate::updater::CommandOutput, UpdaterError> {
    let mut command_args = vec!["pip".to_owned()];
    command_args.extend(args.iter().cloned());
    capture_command("uv", &command_args)
}

fn run_uv_pip_stream(args: &[String], log_sender: &Sender<String>) -> Result<(), UpdaterError> {
    let mut command_args = vec!["pip".to_owned()];
    command_args.extend(args.iter().cloned());
    let output = stream_command("uv", &command_args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "uv".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn parse_uv_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    #[derive(Debug, Deserialize)]
    struct UvPackage {
        name: String,
        version: String,
        latest_version: String,
    }

    let output = run_uv_pip_capture(&[
        "list".to_owned(),
        "--outdated".to_owned(),
        "--format=json".to_owned(),
    ])?;
    let packages: Vec<UvPackage> = serde_json::from_str(&output.stdout)?;
    Ok(packages
        .into_iter()
        .map(|package| PackageUpdate::new(package.name, package.version, package.latest_version))
        .collect())
}

fn check_uv_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    parse_uv_updates()
}

fn apply_uv_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let updates = if selected_updates.is_empty() {
        parse_uv_updates()?
    } else {
        selected_updates.to_vec()
    };
    if updates.is_empty() {
        let _ = log_sender.send("uv: обновления не найдены".to_owned());
        return Ok(());
    }

    let mut args = vec!["install".to_owned(), "--upgrade".to_owned()];
    if force_yes {
        args.push("--no-input".to_owned());
    }
    args.extend(updates.into_iter().map(|update| update.name));
    run_uv_pip_stream(&args, log_sender)
}

fn apply_rustup_updates(
    _force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let args = vec!["update".to_owned()];
    let output = stream_command("rustup", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "rustup".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn check_rustup_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    if !system::command_available("rustup") {
        return Ok(Vec::new());
    }
    let output = capture_command("rustup", &vec!["check".to_owned()])?;
    Ok(heuristic_parse_updates("rustup", &output.merged_text()))
}

fn installed_rustup() -> bool {
    system::command_available("rustup")
}

fn winget_installed() -> bool {
    cfg!(target_os = "windows") && system::command_available("winget")
}

fn windows_update_installed() -> bool {
    cfg!(target_os = "windows")
        && (system::command_available("powershell") || system::command_available("pwsh"))
}

fn powershell_program() -> Option<&'static str> {
    if system::command_available("powershell") {
        Some("powershell")
    } else if system::command_available("pwsh") {
        Some("pwsh")
    } else {
        None
    }
}

fn run_powershell_capture(script: &str) -> Result<crate::updater::CommandOutput, UpdaterError> {
    let program = powershell_program()
        .ok_or_else(|| UpdaterError::Message("powershell не найден".to_owned()))?;
    let args = vec![
        "-NoProfile".to_owned(),
        "-NonInteractive".to_owned(),
        "-ExecutionPolicy".to_owned(),
        "Bypass".to_owned(),
        "-Command".to_owned(),
        script.to_owned(),
    ];
    capture_command(program, &args)
}

fn run_powershell_stream(
    script: &str,
    log_sender: &Sender<String>,
) -> Result<crate::updater::CommandOutput, UpdaterError> {
    let program = powershell_program()
        .ok_or_else(|| UpdaterError::Message("powershell не найден".to_owned()))?;
    let args = vec![
        "-NoProfile".to_owned(),
        "-NonInteractive".to_owned(),
        "-ExecutionPolicy".to_owned(),
        "Bypass".to_owned(),
        "-Command".to_owned(),
        script.to_owned(),
    ];
    stream_command(program, &args, log_sender)
}

fn windows_update_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    if !windows_update_installed() {
        return Ok(Vec::new());
    }

    let script = r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
$ErrorActionPreference = 'Stop'
$session = New-Object -ComObject Microsoft.Update.Session
$searcher = $session.CreateUpdateSearcher()
$result = $searcher.Search("IsInstalled=0 and IsHidden=0 and Type='Software'")
if ($result.Updates.Count -eq 0) { Write-Output '[]'; exit 0 }
$items = @()
for ($i = 0; $i -lt $result.Updates.Count; $i++) {
    $u = $result.Updates.Item($i)
    $items += [PSCustomObject]@{ title = $u.Title }
}
$items | ConvertTo-Json -Compress
"#;

    let output = run_powershell_capture(script)?;
    if !output.success {
        let program = powershell_program().unwrap_or("powershell");
        return Err(UpdaterError::CommandFailed {
            program: program.to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        });
    }

    parse_windows_update_items(&output.stdout)
}

fn windows_update_apply_updates(
    _force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    if !windows_update_installed() {
        return Ok(());
    }

    let script = r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
$ErrorActionPreference = 'Stop'
$session = New-Object -ComObject Microsoft.Update.Session
$searcher = $session.CreateUpdateSearcher()
$searchResult = $searcher.Search("IsInstalled=0 and IsHidden=0 and Type='Software'")
if ($searchResult.Updates.Count -eq 0) {
    Write-Output 'Windows Update: обновления не найдены'
    exit 0
}

$selected = @()
if ('__SELECTED_UPDATES_JSON__' -ne '') {
    $selected = '__SELECTED_UPDATES_JSON__' | ConvertFrom-Json
}

$updates = New-Object -ComObject Microsoft.Update.UpdateColl
for ($i = 0; $i -lt $searchResult.Updates.Count; $i++) {
    $candidate = $searchResult.Updates.Item($i)
    if ($selected.Count -eq 0 -or $selected -contains $candidate.Title) {
        [void]$updates.Add($candidate)
    }
}

if ($updates.Count -eq 0) {
    Write-Output 'Windows Update: по текущему выбору обновлений нет'
    exit 0
}

Write-Output ("Windows Update: найдено обновлений: " + $updates.Count)
$downloader = $session.CreateUpdateDownloader()
$downloader.Updates = $updates
[void]$downloader.Download()

$installer = $session.CreateUpdateInstaller()
$installer.Updates = $updates
$installResult = $installer.Install()

$resultCode = [int]$installResult.ResultCode
$resultDescription = switch ($resultCode) {
    0 { 'не начато (NotStarted)' }
    1 { 'в процессе (InProgress)' }
    2 { 'успешно (Succeeded)' }
    3 { 'успешно с ошибками (SucceededWithErrors)' }
    4 { 'ошибка (Failed)' }
    5 { 'прервано (Aborted)' }
    default { 'неизвестный код' }
}
Write-Output ("Windows Update: код результата: " + $resultCode + " (" + $resultDescription + ")")
if ($installResult.RebootRequired) {
    Write-Output 'Windows Update: требуется перезагрузка системы'
}

if ($resultCode -gt 3) {
    [Console]::Error.WriteLine('Windows Update: установка завершилась с ошибкой')
    exit 1
}
exit 0
"#;

    let selected_titles: Vec<String> = selected_updates
        .iter()
        .map(|update| {
            update
                .name
                .strip_prefix("windows-update:")
                .unwrap_or(update.name.as_str())
                .to_owned()
        })
        .collect();
    let selected_json = if selected_titles.is_empty() {
        String::new()
    } else {
        serde_json::to_string(&selected_titles)?
    };
    let script = script.replace("__SELECTED_UPDATES_JSON__", &selected_json.replace('"', "''"));

    let output = run_powershell_stream(&script, log_sender)?;
    if output.success {
        Ok(())
    } else {
        let program = powershell_program().unwrap_or("powershell");
        Err(UpdaterError::CommandFailed {
            program: program.to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn parse_windows_update_items(text: &str) -> Result<Vec<PackageUpdate>, UpdaterError> {
    #[derive(Debug, Deserialize)]
    struct WindowsUpdateItem {
        title: String,
    }

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    if trimmed == "[]" {
        return Ok(Vec::new());
    }

    let items = if trimmed.starts_with('[') {
        serde_json::from_str::<Vec<WindowsUpdateItem>>(trimmed)?
    } else {
        vec![serde_json::from_str::<WindowsUpdateItem>(trimmed)?]
    };

    Ok(items
        .into_iter()
        .map(|item| PackageUpdate::new(format!("windows-update:{}", item.title), "installed", "available"))
        .collect())
}

fn winget_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    if !winget_installed() {
        return Ok(Vec::new());
    }
    let output = capture_command("winget", &vec!["upgrade".to_owned(), "--accept-source-agreements".to_owned()])?;
    Ok(parse_winget_updates(&output.merged_text()))
}

fn winget_apply_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    if selected_updates.is_empty() {
        let mut args = vec!["upgrade".to_owned(), "--all".to_owned(), "--include-unknown".to_owned(), "--accept-source-agreements".to_owned(), "--accept-package-agreements".to_owned()];
        if force_yes {
            args.push("--silent".to_owned());
        }
        let output = stream_command("winget", &args, log_sender)?;
        return if output.success {
            Ok(())
        } else {
            Err(UpdaterError::CommandFailed {
                program: "winget".to_owned(),
                code: output.exit_code,
                stderr: output.stderr,
            })
        };
    }

    for update in selected_updates {
        let name = update
            .name
            .strip_prefix("winget:")
            .unwrap_or(update.name.as_str());
        let mut args = vec![
            "upgrade".to_owned(),
            "--name".to_owned(),
            name.to_owned(),
            "--accept-source-agreements".to_owned(),
            "--accept-package-agreements".to_owned(),
        ];
        if force_yes {
            args.push("--silent".to_owned());
        }
        let output = stream_command("winget", &args, log_sender)?;
        if !output.success {
            return Err(UpdaterError::CommandFailed {
                program: "winget".to_owned(),
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
    }
    Ok(())
}

fn chocolatey_installed() -> bool {
    cfg!(target_os = "windows") && system::command_available("choco")
}

fn chocolatey_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("choco", &vec!["outdated".to_owned(), "--no-color".to_owned(), "--limit-output".to_owned()])?;
    Ok(parse_choco_updates(&output.merged_text()))
}

fn chocolatey_apply_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = if selected_updates.is_empty() {
        vec!["upgrade".to_owned(), "all".to_owned()]
    } else {
        let mut values = vec!["upgrade".to_owned()];
        values.extend(selected_updates.iter().map(|update| {
            update
                .name
                .strip_prefix("choco:")
                .unwrap_or(update.name.as_str())
                .to_owned()
        }));
        values
    };
    if force_yes {
        args.push("--yes".to_owned());
    }
    let output = stream_command("choco", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "choco".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn apt_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("apt")
}

fn apt_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("apt", &vec!["list".to_owned(), "--upgradable".to_owned()])?;
    Ok(heuristic_parse_updates("apt", &output.merged_text()))
}

fn apt_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["upgrade".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    let output = stream_command("apt-get", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "apt-get".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn apt_get_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("apt-get")
}

fn apt_get_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    if system::command_available("apt") {
        let output = capture_command("apt", &vec!["list".to_owned(), "--upgradable".to_owned()])?;
        return Ok(heuristic_parse_updates("apt-get", &output.merged_text()));
    }

    let output = capture_command("apt-get", &vec!["-s".to_owned(), "upgrade".to_owned()])?;
    Ok(heuristic_parse_updates("apt-get", &output.merged_text()))
}

fn apt_get_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["upgrade".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    let output = stream_command("apt-get", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "apt-get".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn dnf_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("dnf")
}

fn dnf_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("dnf", &vec!["check-update".to_owned()])?;
    Ok(heuristic_parse_updates("dnf", &output.merged_text()))
}

fn dnf_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["upgrade".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    let output = stream_command("dnf", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "dnf".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn yum_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("yum")
}

fn yum_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("yum", &vec!["check-update".to_owned()])?;
    Ok(heuristic_parse_updates("yum", &output.merged_text()))
}

fn yum_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["upgrade".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    let output = stream_command("yum", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "yum".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn zypper_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("zypper")
}

fn zypper_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("zypper", &vec!["list-updates".to_owned()])?;
    Ok(heuristic_parse_updates("zypper", &output.merged_text()))
}

fn zypper_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["update".to_owned()];
    if force_yes {
        args.push("--non-interactive".to_owned());
    }
    let output = stream_command("zypper", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "zypper".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn pacman_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("pacman")
}

fn pacman_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("pacman", &vec!["-Qu".to_owned()])?;
    Ok(heuristic_parse_updates("pacman", &output.merged_text()))
}

fn pacman_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["-Syu".to_owned()];
    if force_yes {
        args.push("--noconfirm".to_owned());
    } else {
        args.push("--needed".to_owned());
    }
    let output = stream_command("pacman", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "pacman".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn apk_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("apk")
}

fn apk_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("apk", &vec!["version".to_owned(), "-l".to_owned(), "<".to_owned()])?;
    Ok(heuristic_parse_updates("apk", &output.merged_text()))
}

fn apk_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["upgrade".to_owned()];
    if force_yes {
        args.push("--no-interactive".to_owned());
    }
    let output = stream_command("apk", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "apk".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn xbps_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("xbps-install")
}

fn xbps_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("xbps-install", &vec!["-Mun".to_owned()])?;
    Ok(heuristic_parse_updates("xbps", &output.merged_text()))
}

fn xbps_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["-Su".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    let output = stream_command("xbps-install", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "xbps-install".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn emerge_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("emerge")
}

fn emerge_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("emerge", &vec!["-puDN".to_owned(), "@world".to_owned()])?;
    Ok(heuristic_parse_updates("emerge", &output.merged_text()))
}

fn emerge_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["-uDN".to_owned(), "@world".to_owned()];
    if force_yes {
        args.push("--ask=n".to_owned());
    }
    let output = stream_command("emerge", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "emerge".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn flatpak_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("flatpak")
}

fn flatpak_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command(
        "flatpak",
        &vec![
            "remote-ls".to_owned(),
            "--updates".to_owned(),
            "--columns=application,installed-version,version".to_owned(),
        ],
    )?;
    Ok(heuristic_parse_updates("flatpak", &output.merged_text()))
}

fn flatpak_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["update".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    let output = stream_command("flatpak", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "flatpak".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn snap_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("snap")
}

fn snap_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("snap", &vec!["refresh".to_owned(), "--list".to_owned()])?;
    Ok(heuristic_parse_updates("snap", &output.merged_text()))
}

fn snap_apply_updates(
    _force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let output = stream_command("snap", &vec!["refresh".to_owned()], log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "snap".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn pkcon_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("pkcon")
}

fn pkcon_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("pkcon", &vec!["get-updates".to_owned()])?;
    Ok(heuristic_parse_updates("pkcon", &output.merged_text()))
}

fn pkcon_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["update".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    let output = stream_command("pkcon", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "pkcon".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn brew_installed() -> bool {
    system::command_available("brew")
}

fn brew_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("brew", &vec!["outdated".to_owned()])?;
    Ok(heuristic_parse_updates("brew", &output.merged_text()))
}

fn brew_apply_updates(
    _force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let output = stream_command("brew", &vec!["upgrade".to_owned()], log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "brew".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn msys2_installed() -> bool {
    cfg!(target_os = "windows") && (msys2_bash_path().is_some() || system::command_available("pacman"))
}

fn msys2_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = msys2_capture_pacman(&["-Qu"])?;
    Ok(heuristic_parse_updates("msys2", &output.merged_text()))
}

fn msys2_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["-Syu"];
    if force_yes {
        args.push("--noconfirm");
    } else {
        args.push("--needed");
    }
    let output = msys2_stream_pacman(&args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "pacman".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn msys2_root_candidates() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(root) = std::env::var_os("MSYS2_ROOT") {
        roots.push(PathBuf::from(root));
    }

    roots.push(PathBuf::from("C:\\msys64"));
    roots.push(PathBuf::from("C:\\tools\\msys64"));
    roots
}

fn msys2_bash_path() -> Option<String> {
    for root in msys2_root_candidates() {
        let candidate = root.join("usr").join("bin").join("bash.exe");
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

fn msys2_capture_pacman(args: &[&str]) -> Result<crate::updater::CommandOutput, UpdaterError> {
    if let Some(bash_path) = msys2_bash_path() {
        let command = if args.is_empty() {
            "pacman".to_owned()
        } else {
            format!("pacman {}", args.join(" "))
        };
        return capture_command(&bash_path, &["-lc".to_owned(), command]);
    }

    let fallback_args: Vec<String> = args.iter().map(|value| (*value).to_owned()).collect();
    capture_command("pacman", &fallback_args)
}

fn msys2_stream_pacman(
    args: &[&str],
    log_sender: &Sender<String>,
) -> Result<crate::updater::CommandOutput, UpdaterError> {
    if let Some(bash_path) = msys2_bash_path() {
        let command = if args.is_empty() {
            "pacman".to_owned()
        } else {
            format!("pacman {}", args.join(" "))
        };
        return stream_command(&bash_path, &["-lc".to_owned(), command], log_sender);
    }

    let fallback_args: Vec<String> = args.iter().map(|value| (*value).to_owned()).collect();
    stream_command("pacman", &fallback_args, log_sender)
}

define_simple_updater!(WingetUpdater, "winget", ModuleKind::System, true, winget_installed, winget_check_updates, winget_apply_updates);
define_simple_updater!(WindowsUpdateUpdater, "windows-update", ModuleKind::System, true, windows_update_installed, windows_update_check_updates, windows_update_apply_updates);
define_simple_updater!(ChocolateyUpdater, "choco", ModuleKind::System, true, chocolatey_installed, chocolatey_check_updates, chocolatey_apply_updates);
define_simple_updater!(AptUpdater, "apt", ModuleKind::System, true, apt_installed, apt_check_updates, apt_apply_updates);
define_simple_updater!(AptGetUpdater, "apt-get", ModuleKind::System, true, apt_get_installed, apt_get_check_updates, apt_get_apply_updates);
define_simple_updater!(DnfUpdater, "dnf", ModuleKind::System, true, dnf_installed, dnf_check_updates, dnf_apply_updates);
define_simple_updater!(YumUpdater, "yum", ModuleKind::System, true, yum_installed, yum_check_updates, yum_apply_updates);
define_simple_updater!(ZypperUpdater, "zypper", ModuleKind::System, true, zypper_installed, zypper_check_updates, zypper_apply_updates);
define_simple_updater!(PacmanUpdater, "pacman", ModuleKind::System, true, pacman_installed, pacman_check_updates, pacman_apply_updates);
define_simple_updater!(ApkUpdater, "apk", ModuleKind::System, true, apk_installed, apk_check_updates, apk_apply_updates);
define_simple_updater!(XbpsUpdater, "xbps", ModuleKind::System, true, xbps_installed, xbps_check_updates, xbps_apply_updates);
define_simple_updater!(EmergeUpdater, "emerge", ModuleKind::System, true, emerge_installed, emerge_check_updates, emerge_apply_updates);
define_simple_updater!(FlatpakUpdater, "flatpak", ModuleKind::System, true, flatpak_installed, flatpak_check_updates, flatpak_apply_updates);
define_simple_updater!(SnapUpdater, "snap", ModuleKind::System, true, snap_installed, snap_check_updates, snap_apply_updates);
define_simple_updater!(PkconUpdater, "pkcon", ModuleKind::System, true, pkcon_installed, pkcon_check_updates, pkcon_apply_updates);
define_simple_updater!(BrewUpdater, "brew", ModuleKind::System, true, brew_installed, brew_check_updates, brew_apply_updates);
define_simple_updater!(RustupUpdater, "rustup", ModuleKind::Tool, false, installed_rustup, check_rustup_updates, apply_rustup_updates);
define_simple_updater!(Msys2Updater, "msys2", ModuleKind::Tool, true, msys2_installed, msys2_check_updates, msys2_apply_updates);
define_simple_updater!(PipUpdater, "pip", ModuleKind::Python, false, installed_pip, check_pip_updates, apply_pip_updates);
define_simple_updater!(UvUpdater, "uv", ModuleKind::Python, false, uv_installed, check_uv_updates, apply_uv_updates);

fn parse_winget_updates(text: &str) -> Vec<PackageUpdate> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("Name") && !line.starts_with("-") && !line.starts_with("----"))
        .filter(|line| !line.to_ascii_lowercase().contains("upgrades available"))
        .filter_map(|line| {
            let cols = split_columns_by_wide_spaces(line);
            if cols.len() < 4 {
                return None;
            }

            let name = cols.first()?.trim();
            let current = cols.get(2)?.trim();
            let available = cols.get(3)?.trim();
            if name.is_empty() || current.is_empty() || available.is_empty() {
                return None;
            }

            Some(PackageUpdate::new(
                format!("winget:{name}"),
                current,
                available,
            ))
        })
        .collect()
}

fn parse_choco_updates(text: &str) -> Vec<PackageUpdate> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("Chocolatey") && !line.starts_with("Outdated Packages"))
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() < 3 {
                return None;
            }

            let name = parts[0].trim();
            let current = parts[1].trim();
            let available = parts[2].trim();
            if name.is_empty() || current.is_empty() || available.is_empty() {
                return None;
            }

            Some(PackageUpdate::new(
                format!("choco:{name}"),
                current,
                available,
            ))
        })
        .collect()
}

fn split_columns_by_wide_spaces(line: &str) -> Vec<String> {
    let mut columns = Vec::new();
    let mut current = String::new();
    let mut spaces = 0usize;

    for ch in line.chars() {
        if ch == ' ' {
            spaces += 1;
            if spaces >= 2 {
                if !current.trim().is_empty() {
                    columns.push(current.trim().to_owned());
                    current.clear();
                }
                continue;
            }
        } else {
            spaces = 0;
        }
        current.push(ch);
    }

    if !current.trim().is_empty() {
        columns.push(current.trim().to_owned());
    }

    columns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_columns_by_wide_spaces_splits_expected_columns() {
        let line = "Git.Git         Git   2.45.0     2.46.0";
        let columns = split_columns_by_wide_spaces(line);
        assert_eq!(columns, vec!["Git.Git", "Git", "2.45.0", "2.46.0"]);
    }

    #[test]
    fn parse_choco_updates_reads_limit_output_lines() {
        let text = "\
git|2.45.0|2.46.0|false\n\
python|3.12.0|3.12.4|false\n\
";

        let updates = parse_choco_updates(text);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].name, "choco:git");
        assert_eq!(updates[0].current_version, "2.45.0");
        assert_eq!(updates[0].available_version, "2.46.0");
        assert_eq!(updates[1].name, "choco:python");
    }

    #[test]
    fn parse_winget_updates_reads_table_lines() {
        let text = "\
Name             Id                    Version     Available\n\
----------------------------------------------------------------\n\
Git              Git.Git               2.45.0      2.46.0\n\
Python 3.12      Python.Python.3.12    3.12.0      3.12.4\n\
";

        let updates = parse_winget_updates(text);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].name, "winget:Git");
        assert_eq!(updates[0].current_version, "2.45.0");
        assert_eq!(updates[0].available_version, "2.46.0");
        assert_eq!(updates[1].name, "winget:Python 3.12");
    }

    #[test]
    fn parse_windows_update_items_handles_array_and_single_object() {
        let array = r#"[{"title":"Cumulative Update for Windows 11"}]"#;
        let updates = parse_windows_update_items(array).expect("array json should parse");
        assert_eq!(updates.len(), 1);
        assert_eq!(
            updates[0].name,
            "windows-update:Cumulative Update for Windows 11"
        );

        let object = r#"{"title":"Security Update KB5000001"}"#;
        let updates = parse_windows_update_items(object).expect("object json should parse");
        assert_eq!(updates.len(), 1);
        assert_eq!(
            updates[0].name,
            "windows-update:Security Update KB5000001"
        );
    }
}
