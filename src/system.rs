//! Системные утилиты: проверка прав, доступности команд и поведение консоли в GUI-режиме.

use crate::updater::command_exists;

/// Проверяет, запущен ли процесс с правами администратора/root.
///
/// # Arguments
/// Функция не принимает аргументов.
///
/// # Returns
/// `true`, если процесс имеет повышенные привилегии; иначе `false`.
///
/// # Panics
/// Не паникует.
///
/// # Examples
/// ```rust,ignore
/// let elevated = system::is_admin();
/// println!("elevated={elevated}");
/// ```
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

/// Сообщает, требуется ли предупреждение о запуске без повышенных прав.
///
/// # Arguments
/// Функция не принимает аргументов.
///
/// # Returns
/// `true`, если процесс запущен без прав администратора/root.
///
/// # Panics
/// Не паникует.
///
/// # Examples
/// ```rust,ignore
/// if system::should_warn_about_elevation() {
///     eprintln!("ограниченные права");
/// }
/// ```
pub fn should_warn_about_elevation() -> bool {
    !is_admin()
}

/// Проверяет наличие исполняемой команды в `PATH`.
///
/// # Arguments
/// * `program` - Имя исполняемой команды.
///
/// # Returns
/// `true`, если команда найдена; иначе `false`.
///
/// # Panics
/// Не паникует.
///
/// # Examples
/// ```rust,ignore
/// assert!(system::command_available("cargo") || !system::command_available("cargo"));
/// ```
pub fn command_available(program: &str) -> bool {
    command_exists(program)
}

/// Скрывает окно консоли Windows при запуске в GUI-режиме.
///
/// # Arguments
/// * `gui_mode` - Признак запуска в графическом режиме.
///
/// # Returns
/// Ничего не возвращает.
///
/// # Safety
/// На Windows использует WinAPI-вызовы `GetConsoleWindow` и `ShowWindow`
/// внутри `unsafe` блока. Указатели не сохраняются и применяются только
/// для текущего окна консоли.
///
/// # Panics
/// Не паникует.
///
/// # Examples
/// ```rust,ignore
/// system::hide_windows_console_if_needed(true);
/// ```
pub fn hide_windows_console_if_needed(gui_mode: bool) {
    if !cfg!(target_os = "windows") || !gui_mode {
        return;
    }

    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::System::Console::GetConsoleWindow;
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

        let window = GetConsoleWindow();
        if !window.is_null() {
            ShowWindow(window, SW_HIDE);
        }
    }
}
