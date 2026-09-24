mod en;
mod ru;

use std::cell::Cell;

/// Language used for application messages.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    clap::ValueEnum,
    serde::Serialize,
    serde::Deserialize,
)]
pub(crate) enum Language {
    #[default]
    #[serde(rename = "en")]
    #[value(name = "en")]
    English,
    #[serde(rename = "ru")]
    #[value(name = "ru")]
    Russian,
}

impl Language {
    pub(crate) const ALL: [Self; 2] = [Self::English, Self::Russian];

    pub(crate) const fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Russian => "Русский",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Catalog {
    pub(crate) gui: GuiText,
    pub(crate) model: ModelText,
    pub(crate) app: AppText,
    pub(crate) updater: UpdaterText,
    pub(crate) system: SystemText,
    pub(crate) cli: CliText,
    pub(crate) build_info: BuildInfoText,
}

#[derive(Clone, Copy)]
pub(crate) struct BuildInfoText {
    pub(crate) application: &'static str,
    pub(crate) version: &'static str,
    pub(crate) profile: &'static str,
    pub(crate) git: &'static str,
    pub(crate) branch: &'static str,
    pub(crate) commit_time: &'static str,
    pub(crate) dirty: &'static str,
    pub(crate) compiler: &'static str,
    pub(crate) rust_version: &'static str,
    pub(crate) optimization: &'static str,
    pub(crate) debug_info: &'static str,
    pub(crate) target_platform: &'static str,
    pub(crate) target: &'static str,
    pub(crate) host: &'static str,
}

#[derive(Clone, Copy)]
pub(crate) struct GuiText {
    pub(crate) check_updates: &'static str,
    pub(crate) update_selected: &'static str,
    pub(crate) update_all: &'static str,
    pub(crate) cancel: &'static str,
    pub(crate) elevate: &'static str,
    pub(crate) wait_operation: &'static str,
    pub(crate) restart_as_admin: &'static str,
    pub(crate) auto_yes_toolbar: &'static str,
    pub(crate) tab_overview: &'static str,
    pub(crate) tab_modules: &'static str,
    pub(crate) tab_logs: &'static str,
    pub(crate) tab_settings: &'static str,
    pub(crate) selected_modules_counter: &'static str,
    pub(crate) modules_with_updates_counter: &'static str,
    pub(crate) overview_title: &'static str,
    pub(crate) found_modules: &'static str,
    pub(crate) selected_count: &'static str,
    pub(crate) need_updates_count: &'static str,
    pub(crate) logs_accumulated: &'static str,
    pub(crate) overview_instructions: &'static str,
    pub(crate) empty_modules: &'static str,
    pub(crate) empty_visible_modules: &'static str,
    pub(crate) export_logs: &'static str,
    pub(crate) module_count: &'static str,
    pub(crate) record_count: &'static str,
    pub(crate) logs_empty: &'static str,
    pub(crate) settings_title: &'static str,
    pub(crate) auto_yes_setting: &'static str,
    pub(crate) show_not_found: &'static str,
    pub(crate) show_up_to_date: &'static str,
    pub(crate) settings_saved: &'static str,
    pub(crate) language_label: &'static str,
    pub(crate) language_changed: &'static str,
    pub(crate) version: &'static str,
    pub(crate) commit: &'static str,
    pub(crate) dirty: &'static str,
    pub(crate) build_details: &'static str,
    pub(crate) elevation_warning: &'static str,
    pub(crate) selection_menu: &'static str,
    pub(crate) select_visible: &'static str,
    pub(crate) deselect_visible: &'static str,
    pub(crate) invert_selection: &'static str,
    pub(crate) select_only_updates: &'static str,
    pub(crate) status_waiting: &'static str,
    pub(crate) status_scanning: &'static str,
    pub(crate) log_scan_started: &'static str,
    pub(crate) status_updating: &'static str,
    pub(crate) summary_update_cancelled: &'static str,
    pub(crate) summary_no_selected_updates: &'static str,
    pub(crate) summary_update_success: &'static str,
    pub(crate) summary_update_errors: &'static str,
    pub(crate) status_cancel_requested: &'static str,
    pub(crate) log_cancel_requested: &'static str,
    pub(crate) status_cancel_incomplete: &'static str,
    pub(crate) log_cancel_error: &'static str,
    pub(crate) status_no_elevated_modules: &'static str,
    pub(crate) error_save_scan_results: &'static str,
    pub(crate) status_request_elevation: &'static str,
    pub(crate) status_scan_progress: &'static str,
    pub(crate) status_scan_complete: &'static str,
    pub(crate) status_elevated_success: &'static str,
    pub(crate) error_elevate: &'static str,
    pub(crate) error_prefix: &'static str,
    pub(crate) status_update_prefix: &'static str,
    pub(crate) selected_packages_count: &'static str,
    pub(crate) update_details: &'static str,
    pub(crate) full_system_update_notice: &'static str,
    pub(crate) logs_copied: &'static str,
    pub(crate) error_export_logs: &'static str,
    pub(crate) error_save_state: &'static str,
    pub(crate) error_config_directory: &'static str,
}

#[derive(Clone, Copy)]
pub(crate) struct ModelText {
    pub(crate) kind_system: &'static str,
    pub(crate) kind_tool: &'static str,
    pub(crate) kind_python: &'static str,
    pub(crate) status_not_found: &'static str,
    pub(crate) status_up_to_date: &'static str,
    pub(crate) status_updates_available: &'static str,
    pub(crate) status_error: &'static str,
    pub(crate) status_windows_updates: &'static str,
    pub(crate) status_empty_updates: &'static str,
    pub(crate) scope_node_self_update: &'static str,
}

#[derive(Clone, Copy)]
pub(crate) struct AppText {
    pub(crate) phase_queued: &'static str,
    pub(crate) phase_running: &'static str,
    pub(crate) phase_completed: &'static str,
    pub(crate) phase_failed: &'static str,
    pub(crate) phase_cancelled: &'static str,
    pub(crate) table_tool: &'static str,
    pub(crate) table_installed: &'static str,
    pub(crate) table_status: &'static str,
    pub(crate) table_updates: &'static str,
    pub(crate) yes: &'static str,
    pub(crate) no: &'static str,
    pub(crate) full_update: &'static str,
    pub(crate) elevation_warning: &'static str,
    pub(crate) skip_prefix: &'static str,
    pub(crate) updater_not_found: &'static str,
    pub(crate) stage_error: &'static str,
    pub(crate) stage_start: &'static str,
    pub(crate) stage_cancelled: &'static str,
    pub(crate) stage_completed: &'static str,
    pub(crate) stage_cancelled_before_start: &'static str,
    pub(crate) update_not_verified: &'static str,
    pub(crate) skip_not_selected: &'static str,
    pub(crate) skip_not_installed: &'static str,
    pub(crate) skip_no_updates: &'static str,
    pub(crate) skip_all_unselected: &'static str,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub(crate) struct UpdaterText {
    pub(crate) invalid_package_name: &'static str,
    pub(crate) command_spawn_error: &'static str,
    pub(crate) command_stream_error: &'static str,
    pub(crate) command_failed: &'static str,
    pub(crate) json_error: &'static str,
    pub(crate) http_error: &'static str,
    pub(crate) create_http_client_error: &'static str,
    pub(crate) command_stdout_unavailable: &'static str,
    pub(crate) command_stderr_unavailable: &'static str,
    pub(crate) command_stdout_thread_failed: &'static str,
    pub(crate) command_stderr_thread_failed: &'static str,
    pub(crate) process_tree_termination_failed: &'static str,
    pub(crate) taskkill_failed: &'static str,
    pub(crate) process_termination_unsupported: &'static str,
    pub(crate) powershell_missing: &'static str,
    pub(crate) windows_update_summary: &'static str,
    pub(crate) winget_package_update_failed: &'static str,
    pub(crate) winget_packages_update_failed: &'static str,
    pub(crate) winget_download_timeout: &'static str,
    pub(crate) winget_install_method_changed: &'static str,
    pub(crate) command_exit_code_with_output: &'static str,
    pub(crate) command_exit_code: &'static str,
    pub(crate) pacman_no_details: &'static str,
    pub(crate) apt_get_refreshing: &'static str,
    pub(crate) selected_update_mode: &'static str,
    pub(crate) updating_selected_packages: &'static str,
    pub(crate) python_missing: &'static str,
    pub(crate) pip_list_failed_fallback: &'static str,
    pub(crate) pip_update_failed_fallback: &'static str,
    pub(crate) pip_updates_not_found: &'static str,
    pub(crate) uv_updates_not_found: &'static str,
    pub(crate) uv_current_version_unknown: &'static str,
    pub(crate) npm_updates_not_found: &'static str,
    pub(crate) pnpm_updates_not_found: &'static str,
    pub(crate) node_updates_not_found: &'static str,
    pub(crate) node_release_index_invalid_json: &'static str,
    pub(crate) node_official_version_missing: &'static str,
    pub(crate) node_nvm_version_invalid: &'static str,
    pub(crate) node_current_version_unknown: &'static str,
    pub(crate) node_install_method: &'static str,
    pub(crate) node_install_method_unknown: &'static str,
    pub(crate) node_nvm_command_missing: &'static str,
    pub(crate) node_nvm_script_missing: &'static str,
    pub(crate) node_nvm_script_invalid: &'static str,
    pub(crate) node_bash_required: &'static str,
    pub(crate) node_official_msi: &'static str,
    pub(crate) unknown: &'static str,
    pub(crate) node_msi_unsupported_arch: &'static str,
    pub(crate) node_downloading_installer: &'static str,
    pub(crate) node_create_file_failed: &'static str,
    pub(crate) node_save_file_failed: &'static str,
    pub(crate) package_manager_version_unknown: &'static str,
    pub(crate) npm_registry_latest_missing: &'static str,
    pub(crate) editor_extension_update_failed: &'static str,
    pub(crate) editor_extension_install_retrying: &'static str,
    pub(crate) editor_extensions_update_failed: &'static str,
    pub(crate) editor_command_missing: &'static str,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub(crate) struct SystemText {
    pub(crate) temp_file_missing_parent: &'static str,
    pub(crate) temp_directory_unknown: &'static str,
    pub(crate) temp_directory_check_failed: &'static str,
    pub(crate) temp_file_outside_directory: &'static str,
    pub(crate) temp_file_name_invalid: &'static str,
    pub(crate) temp_file_name_unexpected: &'static str,
    pub(crate) temp_file_identity_invalid: &'static str,
    pub(crate) temp_file_pid_invalid: &'static str,
    pub(crate) temp_file_nonce_invalid: &'static str,
    pub(crate) temp_file_missing: &'static str,
    pub(crate) temp_file_not_regular: &'static str,
    pub(crate) temp_file_already_used: &'static str,
    pub(crate) already_elevated: &'static str,
    pub(crate) no_modules_to_rescan: &'static str,
    pub(crate) executable_path_unknown: &'static str,
    pub(crate) windows_elevation_denied: &'static str,
    pub(crate) pkexec_missing: &'static str,
    pub(crate) system_time_unknown: &'static str,
    pub(crate) ready_marker_create_failed: &'static str,
    pub(crate) pkexec_start_failed: &'static str,
    pub(crate) elevated_launch_check_failed: &'static str,
    pub(crate) elevated_window_failed: &'static str,
    pub(crate) elevation_unsupported: &'static str,
}

#[derive(Clone, Copy)]
pub(crate) struct CliText {
    pub(crate) unknown_modules: &'static str,
    pub(crate) update_worker_panicked: &'static str,
}

thread_local! {
    static CURRENT_LANGUAGE: Cell<Language> = const { Cell::new(Language::English) };
}

struct LanguageGuard(Language);

impl Drop for LanguageGuard {
    fn drop(&mut self) {
        CURRENT_LANGUAGE.with(|current| current.set(self.0));
    }
}

pub(crate) fn current_language() -> Language {
    CURRENT_LANGUAGE.with(Cell::get)
}

pub(crate) fn with_language<T>(language: Language, operation: impl FnOnce() -> T) -> T {
    let previous = CURRENT_LANGUAGE.with(|current| current.replace(language));
    let _guard = LanguageGuard(previous);
    operation()
}

pub(crate) fn catalog(language: Language) -> &'static Catalog {
    match language {
        Language::English => &en::CATALOG,
        Language::Russian => &ru::CATALOG,
    }
}

pub(crate) fn format_template(template: &str, arguments: &[(&str, String)]) -> String {
    arguments
        .iter()
        .fold(template.to_owned(), |message, (name, value)| {
            message.replace(&format!("{{{name}}}"), value)
        })
}

/// Translates a catalog message and formats any named placeholders.
#[macro_export]
macro_rules! tr {
    ($language:expr, $section:ident, $key:ident) => {
        $crate::localization::catalog($language).$section.$key
    };
    ($language:expr, $section:ident, $key:ident, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::localization::format_template(
            $crate::localization::catalog($language).$section.$key,
            &[$((stringify!($name), ($value).to_string())),+],
        )
    };
}

#[cfg(test)]
mod tests {
    use super::{Language, catalog, current_language, format_template, with_language};
    use std::collections::BTreeSet;

    #[test]
    fn english_is_the_default_language() {
        assert_eq!(Language::default(), Language::English);
        assert_eq!(Language::ALL, [Language::English, Language::Russian]);
        assert_eq!(serde_json::to_string(&Language::English).unwrap(), "\"en\"");
        assert_eq!(serde_json::to_string(&Language::Russian).unwrap(), "\"ru\"");
    }

    #[test]
    fn named_template_arguments_are_replaced_without_changing_other_text() {
        assert_eq!(
            format_template(
                "{name}: {count} updates; keep {{literal}}",
                &[("name", "npm".to_owned()), ("count", "2".to_owned())],
            ),
            "npm: 2 updates; keep {{literal}}"
        );
    }

    #[test]
    fn language_context_is_restored_after_nested_operations() {
        assert_eq!(current_language(), Language::English);
        with_language(Language::Russian, || {
            assert_eq!(current_language(), Language::Russian);
            with_language(Language::English, || {
                assert_eq!(current_language(), Language::English);
            });
            assert_eq!(current_language(), Language::Russian);
        });
        assert_eq!(current_language(), Language::English);
    }

    #[test]
    fn catalog_templates_use_the_same_named_arguments() {
        let english = catalog(Language::English);
        let russian = catalog(Language::Russian);
        let pairs = [
            (
                english.gui.selected_modules_counter,
                russian.gui.selected_modules_counter,
            ),
            (
                english.gui.modules_with_updates_counter,
                russian.gui.modules_with_updates_counter,
            ),
            (
                english.gui.status_scan_progress,
                russian.gui.status_scan_progress,
            ),
            (
                english.gui.status_scan_complete,
                russian.gui.status_scan_complete,
            ),
            (english.gui.found_modules, russian.gui.found_modules),
            (english.gui.selected_count, russian.gui.selected_count),
            (
                english.gui.need_updates_count,
                russian.gui.need_updates_count,
            ),
            (english.gui.logs_accumulated, russian.gui.logs_accumulated),
            (english.gui.module_count, russian.gui.module_count),
            (english.gui.record_count, russian.gui.record_count),
            (english.gui.language_changed, russian.gui.language_changed),
            (english.gui.version, russian.gui.version),
            (english.gui.commit, russian.gui.commit),
            (english.gui.dirty, russian.gui.dirty),
            (english.gui.log_cancel_error, russian.gui.log_cancel_error),
            (
                english.gui.error_save_scan_results,
                russian.gui.error_save_scan_results,
            ),
            (english.gui.error_elevate, russian.gui.error_elevate),
            (english.gui.error_prefix, russian.gui.error_prefix),
            (
                english.gui.status_update_prefix,
                russian.gui.status_update_prefix,
            ),
            (
                english.gui.selected_packages_count,
                russian.gui.selected_packages_count,
            ),
            (english.gui.update_details, russian.gui.update_details),
            (
                english.gui.full_system_update_notice,
                russian.gui.full_system_update_notice,
            ),
            (english.gui.error_export_logs, russian.gui.error_export_logs),
            (english.gui.error_save_state, russian.gui.error_save_state),
            (
                english.model.status_updates_available,
                russian.model.status_updates_available,
            ),
            (english.model.status_error, russian.model.status_error),
            (
                english.model.status_windows_updates,
                russian.model.status_windows_updates,
            ),
            (english.app.full_update, russian.app.full_update),
            (english.app.elevation_warning, russian.app.elevation_warning),
            (english.app.skip_prefix, russian.app.skip_prefix),
            (english.app.updater_not_found, russian.app.updater_not_found),
            (english.app.stage_error, russian.app.stage_error),
            (
                english.app.update_not_verified,
                russian.app.update_not_verified,
            ),
            (english.app.stage_error, russian.app.stage_error),
            (
                english.updater.invalid_package_name,
                russian.updater.invalid_package_name,
            ),
            (
                english.updater.command_spawn_error,
                russian.updater.command_spawn_error,
            ),
            (
                english.updater.command_stream_error,
                russian.updater.command_stream_error,
            ),
            (
                english.updater.command_failed,
                russian.updater.command_failed,
            ),
            (english.updater.json_error, russian.updater.json_error),
            (english.updater.http_error, russian.updater.http_error),
            (
                english.updater.create_http_client_error,
                russian.updater.create_http_client_error,
            ),
            (
                english.updater.command_stdout_unavailable,
                russian.updater.command_stdout_unavailable,
            ),
            (
                english.updater.command_stderr_unavailable,
                russian.updater.command_stderr_unavailable,
            ),
            (
                english.updater.command_stdout_thread_failed,
                russian.updater.command_stdout_thread_failed,
            ),
            (
                english.updater.command_stderr_thread_failed,
                russian.updater.command_stderr_thread_failed,
            ),
            (
                english.updater.process_tree_termination_failed,
                russian.updater.process_tree_termination_failed,
            ),
            (
                english.updater.taskkill_failed,
                russian.updater.taskkill_failed,
            ),
            (
                english.updater.windows_update_summary,
                russian.updater.windows_update_summary,
            ),
            (
                english.updater.winget_package_update_failed,
                russian.updater.winget_package_update_failed,
            ),
            (
                english.updater.winget_packages_update_failed,
                russian.updater.winget_packages_update_failed,
            ),
            (
                english.updater.winget_download_timeout,
                russian.updater.winget_download_timeout,
            ),
            (
                english.updater.winget_install_method_changed,
                russian.updater.winget_install_method_changed,
            ),
            (
                english.updater.command_exit_code_with_output,
                russian.updater.command_exit_code_with_output,
            ),
            (
                english.updater.command_exit_code,
                russian.updater.command_exit_code,
            ),
            (
                english.updater.selected_update_mode,
                russian.updater.selected_update_mode,
            ),
            (
                english.updater.pip_list_failed_fallback,
                russian.updater.pip_list_failed_fallback,
            ),
            (
                english.updater.pip_update_failed_fallback,
                russian.updater.pip_update_failed_fallback,
            ),
            (
                english.updater.node_official_version_missing,
                russian.updater.node_official_version_missing,
            ),
            (
                english.updater.node_release_index_invalid_json,
                russian.updater.node_release_index_invalid_json,
            ),
            (
                english.updater.node_nvm_version_invalid,
                russian.updater.node_nvm_version_invalid,
            ),
            (
                english.updater.node_install_method,
                russian.updater.node_install_method,
            ),
            (
                english.updater.node_install_method_unknown,
                russian.updater.node_install_method_unknown,
            ),
            (
                english.updater.node_nvm_command_missing,
                russian.updater.node_nvm_command_missing,
            ),
            (
                english.updater.node_nvm_script_missing,
                russian.updater.node_nvm_script_missing,
            ),
            (
                english.updater.node_nvm_script_invalid,
                russian.updater.node_nvm_script_invalid,
            ),
            (
                english.updater.node_bash_required,
                russian.updater.node_bash_required,
            ),
            (
                english.updater.node_msi_unsupported_arch,
                russian.updater.node_msi_unsupported_arch,
            ),
            (
                english.updater.node_downloading_installer,
                russian.updater.node_downloading_installer,
            ),
            (
                english.updater.node_create_file_failed,
                russian.updater.node_create_file_failed,
            ),
            (
                english.updater.node_save_file_failed,
                russian.updater.node_save_file_failed,
            ),
            (
                english.updater.package_manager_version_unknown,
                russian.updater.package_manager_version_unknown,
            ),
            (
                english.updater.editor_extension_update_failed,
                russian.updater.editor_extension_update_failed,
            ),
            (
                english.updater.editor_extension_install_retrying,
                russian.updater.editor_extension_install_retrying,
            ),
            (
                english.updater.editor_extensions_update_failed,
                russian.updater.editor_extensions_update_failed,
            ),
            (
                english.updater.editor_command_missing,
                russian.updater.editor_command_missing,
            ),
            (
                english.updater.winget_package_update_failed,
                russian.updater.winget_package_update_failed,
            ),
            (
                english.updater.winget_packages_update_failed,
                russian.updater.winget_packages_update_failed,
            ),
            (
                english.updater.winget_download_timeout,
                russian.updater.winget_download_timeout,
            ),
            (
                english.updater.winget_install_method_changed,
                russian.updater.winget_install_method_changed,
            ),
            (
                english.updater.command_exit_code_with_output,
                russian.updater.command_exit_code_with_output,
            ),
            (
                english.updater.command_exit_code,
                russian.updater.command_exit_code,
            ),
            (
                english.updater.selected_update_mode,
                russian.updater.selected_update_mode,
            ),
            (
                english.updater.pip_list_failed_fallback,
                russian.updater.pip_list_failed_fallback,
            ),
            (
                english.updater.pip_update_failed_fallback,
                russian.updater.pip_update_failed_fallback,
            ),
            (
                english.updater.node_msi_unsupported_arch,
                russian.updater.node_msi_unsupported_arch,
            ),
            (
                english.updater.node_downloading_installer,
                russian.updater.node_downloading_installer,
            ),
            (
                english.updater.node_create_file_failed,
                russian.updater.node_create_file_failed,
            ),
            (
                english.updater.node_save_file_failed,
                russian.updater.node_save_file_failed,
            ),
            (
                english.updater.package_manager_version_unknown,
                russian.updater.package_manager_version_unknown,
            ),
            (
                english.updater.npm_registry_latest_missing,
                russian.updater.npm_registry_latest_missing,
            ),
            (
                english.system.windows_elevation_denied,
                russian.system.windows_elevation_denied,
            ),
            (
                english.system.elevated_window_failed,
                russian.system.elevated_window_failed,
            ),
            (
                english.system.windows_elevation_denied,
                russian.system.windows_elevation_denied,
            ),
            (english.cli.unknown_modules, russian.cli.unknown_modules),
        ];

        for (english, russian) in pairs {
            assert_eq!(placeholders(english), placeholders(russian));
        }
    }

    fn placeholders(template: &str) -> BTreeSet<&str> {
        let mut placeholders = BTreeSet::new();
        let mut remaining = template;

        while let Some((_, after_open)) = remaining.split_once('{') {
            let Some((placeholder, after_close)) = after_open.split_once('}') else {
                break;
            };
            if !placeholder.is_empty() && !placeholder.starts_with('{') {
                placeholders.insert(placeholder);
            }
            remaining = after_close;
        }

        placeholders
    }
}
