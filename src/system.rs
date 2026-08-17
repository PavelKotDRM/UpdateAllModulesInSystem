//! Системные утилиты: проверка прав, доступности команд и поведение консоли в GUI-режиме.

use crate::updater::command_exists;
use anyhow::{Context, Result, bail};
use std::path::Path;

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

/// Повторно запускает текущий процесс с повышенными правами, ограничивая
/// повторное сканирование переданными модулями.
///
/// # Errors
/// Возвращает ошибку, если исполняемый файл не найден, средство повышения прав
/// недоступно или новый процесс не удалось запустить.
pub fn restart_elevated(module_names: &[String], elevation_state_file: &Path) -> Result<()> {
    if is_admin() {
        bail!("приложение уже запущено с повышенными правами");
    }

    if module_names.is_empty() {
        bail!("не указаны модули для повторного сканирования");
    }

    restart_elevated_impl(module_names, elevation_state_file)
}

#[cfg(target_os = "windows")]
fn restart_elevated_impl(module_names: &[String], elevation_state_file: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let executable =
        std::env::current_exe().context("не удалось определить путь к исполняемому файлу")?;
    let executable = executable
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let operation = "runas\0".encode_utf16().collect::<Vec<_>>();
    let parameters = elevated_arguments(module_names, elevation_state_file)
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
            "Windows отклонила запуск с правами администратора (код {})",
            result as isize
        );
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn elevated_arguments(module_names: &[String], elevation_state_file: &Path) -> String {
    let mut raw_arguments = std::env::args_os().skip(1).peekable();
    let mut arguments = Vec::new();

    while let Some(argument) = raw_arguments.next() {
        let argument = argument.to_string_lossy();
        if argument == "--only" {
            raw_arguments.next();
            continue;
        }
        if argument.starts_with("--only=") {
            continue;
        }
        arguments.push(quote_windows_argument(&argument));
    }

    if arguments.is_empty() {
        arguments.push("--gui".to_owned());
    }
    for module_name in module_names {
        arguments.push("--only".to_owned());
        arguments.push(quote_windows_argument(module_name));
    }
    arguments.push("--elevation-state-file".to_owned());
    arguments.push(quote_windows_argument(&elevation_state_file.to_string_lossy()));
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
fn restart_elevated_impl(module_names: &[String], elevation_state_file: &Path) -> Result<()> {
    use std::fs;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    if !command_available("pkexec") {
        bail!("не найден pkexec; установите PolicyKit для запуска GUI с правами root");
    }

    let executable =
        std::env::current_exe().context("не удалось определить путь к исполняемому файлу")?;
    let marker_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("не удалось определить системное время")?
        .as_nanos();
    let ready_file = std::env::temp_dir().join(format!(
        "update_all_modules_elevated_{}_{}",
        std::process::id(),
        marker_id
    ));
    fs::write(&ready_file, []).context("не удалось создать маркер запуска GUI")?;

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
    let mut raw_arguments = std::env::args_os().skip(1).peekable();
    let mut arguments = Vec::new();
    while let Some(argument) = raw_arguments.next() {
        if argument == "--only" {
            raw_arguments.next();
            continue;
        }
        if argument.to_string_lossy().starts_with("--only=") {
            continue;
        }
        arguments.push(argument);
    }
    if arguments.is_empty() {
        command.arg("--gui");
    } else {
        command.args(arguments);
    }
    for module_name in module_names {
        command.arg("--only").arg(module_name);
    }
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
            return Err(error).context("не удалось запустить pkexec");
        }
    };

    loop {
        if fs::metadata(&ready_file).is_ok_and(|metadata| metadata.len() > 0) {
            let _ = fs::remove_file(&ready_file);
            return Ok(());
        }

        if let Some(status) = child
            .try_wait()
            .context("не удалось проверить запуск приложения с правами root")?
        {
            let _ = fs::remove_file(&ready_file);
            bail!("новое окно с правами root не запустилось (код {status})");
        }

        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn restart_elevated_impl(_module_names: &[String], _elevation_state_file: &Path) -> Result<()> {
    bail!("перезапуск с повышенными правами поддерживается только в Windows и Linux")
}

/// Проверяет наличие исполняемой команды в `PATH`.
pub fn command_available(program: &str) -> bool {
    command_exists(program)
}

/// Скрывает текущее окно консоли Windows при запуске GUI.
///
/// На остальных платформах функция ничего не делает.
pub fn hide_windows_console_if_needed(gui_mode: bool) {
    if !cfg!(target_os = "windows") || !gui_mode {
        return;
    }

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

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::quote_windows_argument;

    #[test]
    fn quotes_windows_arguments_for_shell_execute() {
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
}
