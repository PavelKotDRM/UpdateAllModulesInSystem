//! Сохранение пользовательских настроек GUI между запусками.
//!
//! Состояние хранится в JSON-файле рабочей директории. Чтение выполняется в
//! режиме best effort: отсутствующий или повреждённый файл даёт настройки по
//! умолчанию, не блокируя запуск приложения.

use serde::{Deserialize, Serialize};
use crate::model::ModuleSnapshot;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const GUI_STATE_FILE: &str = ".update_all_modules_gui_state.json";

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct GuiState {
    pub(super) selected_modules: Vec<String>,
    pub(super) auto_yes: Option<bool>,
    pub(super) show_not_found: Option<bool>,
    pub(super) show_up_to_date: Option<bool>,
    pub(super) selected_updates: BTreeMap<String, Vec<String>>,
}

fn gui_state_path() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .map(|cwd| cwd.join(GUI_STATE_FILE))
}

/// Загружает состояние либо возвращает [`GuiState::default`] при любой ошибке.
pub(super) fn load_gui_state() -> GuiState {
    let Some(path) = gui_state_path() else {
        return GuiState::default();
    };

    let Ok(text) = fs::read_to_string(path) else {
        return GuiState::default();
    };

    serde_json::from_str::<GuiState>(&text).unwrap_or_default()
}

/// Сохраняет выбор модулей, пакетов и параметры интерфейса.
///
/// # Errors
///
/// Возвращает ошибку, если рабочая директория недоступна, состояние не удалось
/// сериализовать или записать в файл.
pub(super) fn save_gui_state(
    selection: &BTreeSet<String>,
    selected_updates: &BTreeMap<String, BTreeSet<String>>,
    auto_yes: bool,
    show_not_found: bool,
    show_up_to_date: bool,
) -> anyhow::Result<()> {
    let path = gui_state_path()
        .ok_or_else(|| anyhow::anyhow!("не удалось определить рабочую директорию"))?;
    let state = GuiState {
        selected_modules: selection.iter().cloned().collect(),
        auto_yes: Some(auto_yes),
        show_not_found: Some(show_not_found),
        show_up_to_date: Some(show_up_to_date),
        selected_updates: selected_updates
            .iter()
            .map(|(module, updates)| (module.clone(), updates.iter().cloned().collect()))
            .collect(),
    };
    let text = serde_json::to_string_pretty(&state)?;
    fs::write(path, text)?;
    Ok(())
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
    fs::write(&path, text)?;
    Ok(path)
}

/// Загружает и удаляет одноразовый снимок результатов сканирования.
pub(super) fn take_elevation_modules(path: &std::path::Path) -> Option<Vec<ModuleSnapshot>> {
    let text = fs::read_to_string(path).ok()?;
    let _ = fs::remove_file(path);
    serde_json::from_str(&text).ok()
}
