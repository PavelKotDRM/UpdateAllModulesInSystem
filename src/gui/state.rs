use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

const GUI_STATE_FILE: &str = ".update_all_modules_gui_state.json";

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct GuiState {
    pub(super) selected_modules: Vec<String>,
    pub(super) auto_yes: Option<bool>,
    pub(super) show_not_found: Option<bool>,
    pub(super) selected_updates: BTreeMap<String, Vec<String>>,
}

fn gui_state_path() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .map(|cwd| cwd.join(GUI_STATE_FILE))
}

pub(super) fn load_gui_state() -> GuiState {
    let Some(path) = gui_state_path() else {
        return GuiState::default();
    };

    let Ok(text) = fs::read_to_string(path) else {
        return GuiState::default();
    };

    serde_json::from_str::<GuiState>(&text).unwrap_or_default()
}

pub(super) fn save_gui_state(
    selection: &BTreeSet<String>,
    selected_updates: &BTreeMap<String, BTreeSet<String>>,
    auto_yes: bool,
    show_not_found: bool,
) -> anyhow::Result<()> {
    let path = gui_state_path()
        .ok_or_else(|| anyhow::anyhow!("не удалось определить рабочую директорию"))?;
    let state = GuiState {
        selected_modules: selection.iter().cloned().collect(),
        auto_yes: Some(auto_yes),
        show_not_found: Some(show_not_found),
        selected_updates: selected_updates
            .iter()
            .map(|(module, updates)| (module.clone(), updates.iter().cloned().collect()))
            .collect(),
    };
    let text = serde_json::to_string_pretty(&state)?;
    fs::write(path, text)?;
    Ok(())
}
