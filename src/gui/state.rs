//! Сохранение пользовательских настроек GUI между запусками.
//!
//! Состояние хранится в JSON-файле системного каталога конфигурации. Чтение
//! выполняется в режиме best effort: отсутствующий или повреждённый файл даёт
//! настройки по умолчанию, не блокируя запуск приложения.

use crate::localization::Language;
use crate::model::ModuleSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const CONFIG_DIRECTORY: &str = "update-all-modules";
const GUI_STATE_FILE: &str = "gui_state.json";
const LEGACY_GUI_STATE_FILE: &str = ".update_all_modules_gui_state.json";

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct GuiState {
    pub(super) language: Option<Language>,
    pub(super) selected_modules: Option<Vec<String>>,
    pub(super) auto_yes: Option<bool>,
    pub(super) show_not_found: Option<bool>,
    pub(super) show_up_to_date: Option<bool>,
    pub(super) selected_updates: BTreeMap<String, Vec<String>>,
}

fn gui_state_path() -> Option<PathBuf> {
    dirs::config_dir().map(|directory| directory.join(CONFIG_DIRECTORY).join(GUI_STATE_FILE))
}

fn legacy_gui_state_path() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .map(|directory| directory.join(LEGACY_GUI_STATE_FILE))
}

/// Загружает состояние либо возвращает [`GuiState::default`] при любой ошибке.
pub(super) fn load_gui_state() -> GuiState {
    let Some(path) = gui_state_path() else {
        return GuiState::default();
    };

    let text = fs::read_to_string(&path).or_else(|_| {
        legacy_gui_state_path()
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
            .and_then(fs::read_to_string)
    });
    let Ok(text) = text else {
        return GuiState::default();
    };

    serde_json::from_str::<GuiState>(&text).unwrap_or_default()
}

/// Сохраняет выбор модулей, пакетов и параметры интерфейса.
///
/// # Errors
///
/// Возвращает ошибку, если системный каталог конфигурации недоступен, состояние
/// не удалось сериализовать или атомарно записать в файл.
pub(super) fn save_gui_state(
    language: Language,
    selection: Option<&BTreeSet<String>>,
    selected_updates: &BTreeMap<String, BTreeSet<String>>,
    auto_yes: bool,
    show_not_found: bool,
    show_up_to_date: bool,
) -> anyhow::Result<()> {
    let path = gui_state_path().ok_or_else(|| {
        anyhow::anyhow!(crate::tr!(
            crate::localization::current_language(),
            gui,
            error_config_directory
        ))
    })?;
    let state = GuiState {
        language: Some(language),
        selected_modules: selection.map(|selection| selection.iter().cloned().collect()),
        auto_yes: Some(auto_yes),
        show_not_found: Some(show_not_found),
        show_up_to_date: Some(show_up_to_date),
        selected_updates: selected_updates
            .iter()
            .map(|(module, updates)| (module.clone(), updates.iter().cloned().collect()))
            .collect(),
    };
    let text = serde_json::to_string_pretty(&state)?;
    write_atomically(&path, text.as_bytes())?;
    Ok(())
}

fn write_atomically(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    fs::create_dir_all(parent)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(GUI_STATE_FILE);
    let temporary_path = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        timestamp
    ));

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        replace_file(&temporary_path, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

#[cfg(not(target_os = "windows"))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(target_os = "windows")]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Сохраняет результаты сканирования для передачи в elevated-процесс.
pub(super) fn save_elevation_modules(modules: &[ModuleSnapshot]) -> anyhow::Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| anyhow::anyhow!(error))?
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "update_all_modules_scan_{}_{}.json",
        std::process::id(),
        timestamp
    ));
    let text = serde_json::to_string(modules)?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?
        .write_all(text.as_bytes())?;
    Ok(path)
}

/// Загружает и удаляет одноразовый снимок результатов сканирования.
pub(super) fn take_elevation_modules(path: &std::path::Path) -> Option<Vec<ModuleSnapshot>> {
    let text = fs::read_to_string(path).ok()?;
    let _ = fs::remove_file(path);
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::{GuiState, write_atomically};
    use crate::localization::Language;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn atomic_write_creates_parent_and_replaces_existing_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "update-all-modules-state-test-{}-{unique}",
            std::process::id()
        ));
        let path = directory.join("nested/gui_state.json");

        write_atomically(&path, b"old").unwrap();
        write_atomically(&path, b"new").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert!(fs::read_dir(path.parent().unwrap()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn gui_state_distinguishes_empty_selection_from_missing_selection() {
        let explicitly_empty: GuiState =
            serde_json::from_str(r#"{"selected_modules":[],"selected_updates":{}}"#).unwrap();
        let not_saved: GuiState = serde_json::from_str(r#"{"selected_updates":{}}"#).unwrap();

        assert_eq!(explicitly_empty.selected_modules, Some(Vec::new()));
        assert_eq!(not_saved.selected_modules, None);
        assert_eq!(not_saved.language, None);
    }

    #[test]
    fn gui_state_persists_language_using_language_codes() {
        let state: GuiState =
            serde_json::from_str(r#"{"language":"ru","selected_updates":{}}"#).unwrap();

        assert_eq!(state.language, Some(Language::Russian));
    }
}
