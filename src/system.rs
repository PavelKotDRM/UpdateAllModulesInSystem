use crate::updater::command_exists;

pub fn is_admin() -> bool {
    is_elevated::is_elevated()
}

pub fn should_warn_about_elevation() -> bool {
    !is_admin()
}

pub fn command_available(program: &str) -> bool {
    command_exists(program)
}

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
