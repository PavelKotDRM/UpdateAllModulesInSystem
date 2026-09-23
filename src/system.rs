//! Системные утилиты: проверка прав, доступности команд и поведение консоли в GUI-режиме.

use crate::updater::command_exists;
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;

const ELEVATION_STATE_PREFIX: &str = "update_all_modules_scan_";
const ELEVATION_STATE_SUFFIX: &str = ".json";
const ELEVATION_READY_PREFIX: &str = "update_all_modules_elevated_";

fn forwarded_arguments(
    arguments: impl IntoIterator<Item = std::ffi::OsString>,
) -> Vec<std::ffi::OsString> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    if arguments.is_empty() {
        vec!["--gui".into()]
    } else {
        arguments
    }
}

/// Проверяет, запущен ли процесс с правами администратора или `root`.
pub fn is_admin() -> bool {
    is_admin_impl()
}

#[cfg(target_os = "windows")]
fn is_admin_impl() -> bool {
    is_elevated::is_elevated()
}

#[cfg(unix)]
fn is_admin_impl() -> bool {
    unsafe extern "C" {
        fn geteuid() -> u32;
    }

    unsafe { geteuid() == 0 }
}

#[cfg(not(any(target_os = "windows", unix)))]
fn is_admin_impl() -> bool {
    false
}

/// Сообщает, нужно ли предупредить о запуске без повышенных прав.
pub fn should_warn_about_elevation() -> bool {
    !is_admin()
}

/// Проверяет путь одноразового снимка состояния перед повышением прав.
///
/// # Errors
/// Возвращает ошибку для файла вне системного временного каталога, ссылки,
/// каталога или имени, не соответствующего формату приложения.
pub fn validate_elevation_state_file(path: &Path) -> Result<()> {
    validate_elevation_file(path, ELEVATION_STATE_PREFIX, ELEVATION_STATE_SUFFIX, false)
}

/// Проверяет путь одноразового маркера успешного запуска elevated GUI.
///
/// # Errors
/// Возвращает ошибку для файла вне системного временного каталога, ссылки,
/// каталога, непустого файла или неожиданного имени.
pub fn validate_elevation_ready_file(path: &Path) -> Result<()> {
    validate_elevation_file(path, ELEVATION_READY_PREFIX, "", true)
}

fn validate_elevation_file(
    path: &Path,
    prefix: &str,
    suffix: &str,
    must_be_empty: bool,
) -> Result<()> {
    let parent = path.parent().context(crate::tr!(
        crate::localization::current_language(),
        system,
        temp_file_missing_parent
    ))?;
    let expected_parent = fs::canonicalize(std::env::temp_dir()).context(crate::tr!(
        crate::localization::current_language(),
        system,
        temp_directory_unknown
    ))?;
    let actual_parent = fs::canonicalize(parent).context(crate::tr!(
        crate::localization::current_language(),
        system,
        temp_directory_check_failed
    ))?;
    if actual_parent != expected_parent {
        bail!(
            "{}",
            crate::tr!(
                crate::localization::current_language(),
                system,
                temp_file_outside_directory
            )
        );
    }

    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context(crate::tr!(
            crate::localization::current_language(),
            system,
            temp_file_name_invalid
        ))?;
    let identity = name
        .strip_prefix(prefix)
        .and_then(|name| name.strip_suffix(suffix))
        .context(crate::tr!(
            crate::localization::current_language(),
            system,
            temp_file_name_unexpected
        ))?;
    let (process_id, nonce) = identity
        .split_once('_')
        .filter(|(_, nonce)| !nonce.contains('_'))
        .context(crate::tr!(
            crate::localization::current_language(),
            system,
            temp_file_identity_invalid
        ))?;
    process_id.parse::<u32>().context(crate::tr!(
        crate::localization::current_language(),
        system,
        temp_file_pid_invalid
    ))?;
    nonce.parse::<u128>().context(crate::tr!(
        crate::localization::current_language(),
        system,
        temp_file_nonce_invalid
    ))?;

    let metadata = fs::symlink_metadata(path).context(crate::tr!(
        crate::localization::current_language(),
        system,
        temp_file_missing
    ))?;
    if !metadata.file_type().is_file() {
        bail!(
            "{}",
            crate::tr!(
                crate::localization::current_language(),
                system,
                temp_file_not_regular
            )
        );
    }
    if must_be_empty && metadata.len() != 0 {
        bail!(
            "{}",
            crate::tr!(
                crate::localization::current_language(),
                system,
                temp_file_already_used
            )
        );
    }
    Ok(())
}

/// Повторно запускает текущий процесс с повышенными правами, сохраняя его
/// фильтры модулей.
///
/// # Errors
/// Возвращает ошибку, если исполняемый файл не найден, средство повышения прав
/// недоступно или новый процесс не удалось запустить.
pub fn restart_elevated(module_names: &[String], elevation_state_file: &Path) -> Result<()> {
    if is_admin() {
        bail!(
            "{}",
            crate::tr!(
                crate::localization::current_language(),
                system,
                already_elevated
            )
        );
    }

    if module_names.is_empty() {
        bail!(
            "{}",
            crate::tr!(
                crate::localization::current_language(),
                system,
                no_modules_to_rescan
            )
        );
    }

    validate_elevation_state_file(elevation_state_file)?;

    restart_elevated_impl(module_names, elevation_state_file)
}

#[cfg(target_os = "windows")]
fn restart_elevated_impl(_module_names: &[String], elevation_state_file: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let executable = std::env::current_exe().context(crate::tr!(
        crate::localization::current_language(),
        system,
        executable_path_unknown
    ))?;
    let executable = executable
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let operation = "runas\0".encode_utf16().collect::<Vec<_>>();
    let parameters = elevated_arguments(elevation_state_file)
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();

    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            operation.as_ptr(),
            executable.as_ptr(),
            parameters.as_ptr(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    };

    if result as isize <= 32 {
        bail!(
            "{}",
            crate::tr!(
                crate::localization::current_language(),
                system,
                windows_elevation_denied,
                code = result as isize
            )
        );
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn elevated_arguments(elevation_state_file: &Path) -> String {
    let mut arguments = forwarded_arguments(std::env::args_os().skip(1))
        .iter()
        .map(|argument| quote_windows_argument(&argument.to_string_lossy()))
        .collect::<Vec<_>>();
    arguments.push("--elevation-state-file".to_owned());
    arguments.push(quote_windows_argument(
        &elevation_state_file.to_string_lossy(),
    ));
    arguments.join(" ")
}

#[cfg(target_os = "windows")]
fn quote_windows_argument(argument: &str) -> String {
    let mut quoted = String::from("\"");
    let mut backslashes = 0;

    for character in argument.chars() {
        match character {
            '\\' => backslashes += 1,
            '"' => {
                quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            _ => {
                quoted.push_str(&"\\".repeat(backslashes));
                quoted.push(character);
                backslashes = 0;
            }
        }
    }

    quoted.push_str(&"\\".repeat(backslashes * 2));
    quoted.push('"');
    quoted
}

#[cfg(target_os = "linux")]
fn restart_elevated_impl(_module_names: &[String], elevation_state_file: &Path) -> Result<()> {
    use std::fs::OpenOptions;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    if !command_available("pkexec") {
        bail!(
            "{}",
            crate::tr!(
                crate::localization::current_language(),
                system,
                pkexec_missing
            )
        );
    }

    let executable = std::env::current_exe().context(crate::tr!(
        crate::localization::current_language(),
        system,
        executable_path_unknown
    ))?;
    let marker_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context(crate::tr!(
            crate::localization::current_language(),
            system,
            system_time_unknown
        ))?
        .as_nanos();
    let ready_file = std::env::temp_dir().join(format!(
        "update_all_modules_elevated_{}_{}",
        std::process::id(),
        marker_id
    ));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&ready_file)
        .context(crate::tr!(
            crate::localization::current_language(),
            system,
            ready_marker_create_failed
        ))?;

    let mut command = Command::new("pkexec");
    command.arg("env");
    for variable in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XAUTHORITY",
        "XDG_RUNTIME_DIR",
        "DBUS_SESSION_BUS_ADDRESS",
        "XDG_SESSION_TYPE",
        "GDK_BACKEND",
        "LANG",
        "LC_ALL",
    ] {
        if let Some(value) = std::env::var_os(variable) {
            let mut assignment = std::ffi::OsString::from(variable);
            assignment.push("=");
            assignment.push(value);
            command.arg(assignment);
        }
    }
    command.arg(executable);
    command.args(forwarded_arguments(std::env::args_os().skip(1)));
    command
        .arg("--elevation-state-file")
        .arg(elevation_state_file);
    command.arg("--elevation-ready-file").arg(&ready_file);
    if let Ok(current_dir) = std::env::current_dir() {
        command.current_dir(current_dir);
    }
    command.stdout(Stdio::null()).stderr(Stdio::null());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = fs::remove_file(&ready_file);
            return Err(error).context(crate::tr!(
                crate::localization::current_language(),
                system,
                pkexec_start_failed
            ));
        }
    };

    loop {
        if fs::metadata(&ready_file).is_ok_and(|metadata| metadata.len() > 0) {
            let _ = fs::remove_file(&ready_file);
            return Ok(());
        }

        if let Some(status) = child.try_wait().context(crate::tr!(
            crate::localization::current_language(),
            system,
            elevated_launch_check_failed
        ))? {
            let _ = fs::remove_file(&ready_file);
            bail!(
                "{}",
                crate::tr!(
                    crate::localization::current_language(),
                    system,
                    elevated_window_failed,
                    code = status
                )
            );
        }

        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn restart_elevated_impl(_module_names: &[String], _elevation_state_file: &Path) -> Result<()> {
    bail!(
        "{}",
        crate::tr!(
            crate::localization::current_language(),
            system,
            elevation_unsupported
        )
    )
}

/// Проверяет наличие исполняемой команды в `PATH`.
pub fn command_available(program: &str) -> bool {
    command_exists(program)
}

/// Скрывает текущее окно консоли Windows при запуске GUI.
///
/// На остальных платформах функция ничего не делает.
pub fn hide_windows_console_if_needed(gui_mode: bool) {
    if cfg!(target_os = "windows") && gui_mode {
        #[cfg(target_os = "windows")]
        unsafe {
            use windows_sys::Win32::System::Console::GetConsoleWindow;
            use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};

            let window = GetConsoleWindow();
            if !window.is_null() {
                ShowWindow(window, SW_HIDE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_elevation_ready_file, validate_elevation_state_file};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn elevated_restart_preserves_module_filters() {
        let arguments = super::forwarded_arguments(
            ["--gui", "--only", "apt-get"]
                .into_iter()
                .map(std::ffi::OsString::from),
        );

        assert_eq!(
            arguments,
            ["--gui", "--only", "apt-get"]
                .into_iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn elevated_restart_defaults_to_gui_when_no_arguments_were_provided() {
        assert_eq!(
            super::forwarded_arguments(std::iter::empty::<std::ffi::OsString>()),
            vec![std::ffi::OsString::from("--gui")]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn quotes_windows_arguments_for_shell_execute() {
        use super::quote_windows_argument;

        assert_eq!(quote_windows_argument(""), "\"\"");
        assert_eq!(quote_windows_argument("two words"), "\"two words\"");
        assert_eq!(
            quote_windows_argument(r#"say "hello""#),
            r#""say \"hello\"""#
        );
        assert_eq!(
            quote_windows_argument(r"C:\Program Files\"),
            r#""C:\Program Files\\""#
        );
    }

    #[test]
    fn validates_only_expected_one_time_elevation_files() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp = std::env::temp_dir();
        let state = temp.join(format!(
            "update_all_modules_scan_{}_{nonce}.json",
            std::process::id()
        ));
        let ready = temp.join(format!(
            "update_all_modules_elevated_{}_{nonce}",
            std::process::id()
        ));
        let invalid_name = temp.join(format!("unexpected_{nonce}.json"));
        let nested_directory = temp.join(format!("update-all-modules-path-test-{nonce}"));
        let nested_state = nested_directory.join(format!(
            "update_all_modules_scan_{}_{nonce}.json",
            std::process::id()
        ));

        fs::write(&state, "{}").unwrap();
        fs::write(&ready, []).unwrap();
        fs::write(&invalid_name, "{}").unwrap();
        fs::create_dir(&nested_directory).unwrap();
        fs::write(&nested_state, "{}").unwrap();

        assert!(validate_elevation_state_file(&state).is_ok());
        assert!(validate_elevation_ready_file(&ready).is_ok());
        assert!(validate_elevation_state_file(&invalid_name).is_err());
        assert!(validate_elevation_state_file(&nested_state).is_err());

        fs::write(&ready, "used").unwrap();
        assert!(validate_elevation_ready_file(&ready).is_err());

        fs::remove_file(state).unwrap();
        fs::remove_file(ready).unwrap();
        fs::remove_file(invalid_name).unwrap();
        fs::remove_dir_all(nested_directory).unwrap();
    }
}
