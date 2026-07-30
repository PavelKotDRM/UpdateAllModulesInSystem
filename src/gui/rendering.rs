use super::{GuiApp, ModuleSelectionState, module_selection_state};
use crate::app::{ModuleUpdateProgress, UpdatePhase};
use crate::model::ModuleSnapshot;
use eframe::egui;

impl GuiApp {
    pub(super) fn render_module_card(
        ui: &mut egui::Ui,
        module: &mut ModuleSnapshot,
        progress: Option<&ModuleUpdateProgress>,
        selection_changed: &mut bool,
    ) {
        let module_id = module.name.clone();
        ui.push_id(module_id, |ui| {
            ui.group(|ui| {
                ui.set_min_width(320.0);
                ui.horizontal_wrapped(|ui| {
                    let response = if module.updates.is_empty() {
                        ui.checkbox(&mut module.selected, "")
                    } else {
                        let selected_count = module
                            .updates
                            .iter()
                            .filter(|update| update.selected)
                            .count();
                        let state = module_selection_state(selected_count, module.updates.len());
                        let mut select_all = state == ModuleSelectionState::All;
                        let response = ui.add(
                            egui::Checkbox::new(&mut select_all, "")
                                .indeterminate(state == ModuleSelectionState::Partial),
                        );

                        if response.changed() {
                            module.selected = select_all;
                            for update in &mut module.updates {
                                update.selected = select_all;
                            }
                        }

                        response
                    };
                    if response.changed() {
                        *selection_changed = true;
                    }
                    ui.heading(format!("{} ({:?})", module.name, module.kind));
                });

                ui.label(module.status_label());

                if let Some(progress) = progress {
                    ui.colored_label(
                        phase_color(progress.phase),
                        format!("Статус обновления: {}", progress.phase.label()),
                    );
                    if let Some(detail) = &progress.detail {
                        let detail = detail
                            .strip_prefix("[stdout] ")
                            .or_else(|| detail.strip_prefix("[stderr] "))
                            .unwrap_or(detail);
                        let mut visible = detail.chars().take(140).collect::<String>();
                        if detail.chars().count() > 140 {
                            visible.push_str("...");
                        }
                        ui.small(visible);
                    }
                }

                if !module.updates.is_empty() {
                    let selected_updates_count = module
                        .updates
                        .iter()
                        .filter(|update| update.selected)
                        .count();
                    let detail_lines = module.detail_lines();
                    ui.small(format!(
                        "Выбрано приложений: {}/{}",
                        selected_updates_count,
                        module.updates.len()
                    ));

                    egui::CollapsingHeader::new(format!("Детали ({})", module.updates.len()))
                        .default_open(false)
                        .show(ui, |ui| {
                            let mut update_selection_changed = false;
                            for (update, detail_line) in module.updates.iter_mut().zip(detail_lines)
                            {
                                ui.horizontal_wrapped(|ui| {
                                    let response = ui.checkbox(&mut update.selected, "");
                                    if response.changed() {
                                        update_selection_changed = true;
                                    }
                                    ui.label(detail_line);
                                });
                            }

                            if update_selection_changed {
                                module.selected =
                                    module.updates.iter().any(|update| update.selected);
                                *selection_changed = true;
                            }
                        });
                }
            });
        });
    }
}

fn phase_color(phase: UpdatePhase) -> egui::Color32 {
    match phase {
        UpdatePhase::Queued => egui::Color32::from_rgb(210, 170, 40),
        UpdatePhase::Running => egui::Color32::from_rgb(70, 140, 240),
        UpdatePhase::Completed => egui::Color32::from_rgb(40, 160, 80),
        UpdatePhase::Failed => egui::Color32::from_rgb(200, 70, 70),
        UpdatePhase::Cancelled => egui::Color32::from_rgb(190, 150, 50),
    }
}
