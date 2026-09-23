//! Представления вкладок и панели команд GUI.
//!
//! Модуль компонует обзор, список модулей, журнал и настройки из состояния
//! [`GuiApp`], оставляя запуск операций контроллеру.

use super::{GuiApp, GuiTab};
use crate::system;
use eframe::egui;

impl GuiApp {
    pub(super) fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.busy,
                    egui::Button::new(crate::tr!(self.language, gui, check_updates)),
                )
                .clicked()
            {
                self.start_scan();
            }

            if ui
                .add_enabled(
                    !self.busy,
                    egui::Button::new(crate::tr!(self.language, gui, update_selected)),
                )
                .clicked()
            {
                self.start_update();
            }

            if ui
                .add_enabled(
                    !self.busy,
                    egui::Button::new(crate::tr!(self.language, gui, update_all)),
                )
                .clicked()
            {
                self.start_update_all();
            }

            let can_cancel = self
                .update_cancellation
                .as_ref()
                .is_some_and(|cancellation| !cancellation.is_cancelled());
            if can_cancel && ui.button(crate::tr!(self.language, gui, cancel)).clicked() {
                self.cancel_update();
            }

            if !system::is_admin() {
                let elevate_response = ui
                    .add_enabled(
                        !self.busy && !self.elevation_pending,
                        egui::Button::new(crate::tr!(self.language, gui, elevate)),
                    )
                    .on_hover_text(if self.busy {
                        crate::tr!(self.language, gui, wait_operation)
                    } else {
                        crate::tr!(self.language, gui, restart_as_admin)
                    });
                if elevate_response.clicked() {
                    self.start_elevated();
                }
            }

            ui.add_enabled_ui(!self.busy, |ui| {
                self.show_selection_menu(ui);
            });

            let auto_yes_response = ui.checkbox(
                &mut self.auto_yes,
                crate::tr!(self.language, gui, auto_yes_toolbar),
            );
            if auto_yes_response.changed() {
                self.persist_state();
            }
        });

        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(
                &mut self.active_tab,
                GuiTab::Overview,
                crate::tr!(self.language, gui, tab_overview),
            );
            ui.selectable_value(
                &mut self.active_tab,
                GuiTab::Modules,
                crate::tr!(self.language, gui, tab_modules),
            );
            ui.selectable_value(
                &mut self.active_tab,
                GuiTab::Logs,
                crate::tr!(self.language, gui, tab_logs),
            );
            ui.selectable_value(
                &mut self.active_tab,
                GuiTab::Settings,
                crate::tr!(self.language, gui, tab_settings),
            );
        });

        ui.separator();
        ui.label(&self.status_line);
        ui.horizontal_wrapped(|ui| {
            ui.label(crate::tr!(
                self.language,
                gui,
                selected_modules_counter,
                selected = self.selected_count(),
                total = self.visible_modules_count()
            ));
            ui.separator();
            ui.label(crate::tr!(
                self.language,
                gui,
                modules_with_updates_counter,
                updates = self.updates_count(),
                total = self.visible_modules_count()
            ));
        });
    }

    pub(super) fn show_overview_tab(&self, ui: &mut egui::Ui) {
        ui.heading(crate::tr!(self.language, gui, overview_title));
        ui.add_space(4.0);
        if let Some(warning) = self.elevation_warning_text() {
            ui.colored_label(egui::Color32::YELLOW, warning);
            ui.add_space(4.0);
        }

        ui.horizontal_wrapped(|ui| {
            ui.label(crate::tr!(
                self.language,
                gui,
                found_modules,
                count = self.visible_modules_count()
            ));
            ui.separator();
            ui.label(crate::tr!(
                self.language,
                gui,
                selected_count,
                count = self.selected_count()
            ));
            ui.separator();
            ui.label(crate::tr!(
                self.language,
                gui,
                need_updates_count,
                count = self.updates_count()
            ));
            ui.separator();
            ui.label(crate::tr!(
                self.language,
                gui,
                logs_accumulated,
                count = self.log_count()
            ));
        });

        ui.add_space(8.0);
        ui.label(crate::tr!(self.language, gui, overview_instructions));
    }

    pub(super) fn show_modules_tab(&mut self, ui: &mut egui::Ui) {
        if let Some(warning) = self.elevation_warning_text() {
            ui.colored_label(egui::Color32::YELLOW, warning);
            ui.add_space(6.0);
        }

        egui::ScrollArea::vertical()
            .id_salt("modules_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.modules.is_empty() {
                    ui.label(crate::tr!(self.language, gui, empty_modules));
                    return;
                }

                let available_width = ui.available_width();
                let columns = ((available_width / 420.0).floor() as usize).clamp(1, 3);
                let mut selection_changed = false;
                let visible_count = self.visible_modules_count();
                let module_progress = &self.module_progress;
                let show_not_found = self.show_not_found;
                let show_up_to_date = self.show_up_to_date;

                if visible_count == 0 {
                    ui.label(crate::tr!(self.language, gui, empty_visible_modules));
                    return;
                }

                if columns == 1 {
                    for module in &mut self.modules {
                        if (!show_not_found && !module.installed)
                            || (!show_up_to_date
                                && matches!(module.status, crate::model::ModuleStatus::UpToDate))
                        {
                            continue;
                        }
                        let progress = module_progress.get(&module.name);
                        Self::render_module_card(ui, module, progress, &mut selection_changed);
                    }
                } else {
                    ui.columns(columns, |columns_ui| {
                        let mut visible_index = 0usize;
                        for module in &mut self.modules {
                            if (!show_not_found && !module.installed)
                                || (!show_up_to_date
                                    && matches!(
                                        module.status,
                                        crate::model::ModuleStatus::UpToDate
                                    ))
                            {
                                continue;
                            }
                            let column = &mut columns_ui[visible_index % columns];
                            let progress = module_progress.get(&module.name);
                            Self::render_module_card(
                                column,
                                module,
                                progress,
                                &mut selection_changed,
                            );
                            visible_index += 1;
                        }
                    });
                }

                if selection_changed {
                    self.persist_state();
                }
            });
    }

    pub(super) fn show_logs_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(crate::tr!(self.language, gui, tab_logs));
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(crate::tr!(self.language, gui, export_logs))
                .clicked()
            {
                self.export_logs(ui.ctx());
            }
            ui.label(crate::tr!(
                self.language,
                gui,
                module_count,
                count = self.logs.len()
            ));
            ui.separator();
            ui.label(crate::tr!(
                self.language,
                gui,
                record_count,
                count = self.log_count()
            ));
        });
        ui.add_space(4.0);

        egui::ScrollArea::vertical()
            .id_salt("logs_scroll")
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.logs.is_empty() {
                    ui.label(crate::tr!(self.language, gui, logs_empty));
                } else {
                    for (module, lines) in &self.logs {
                        egui::CollapsingHeader::new(format!("{module} ({})", lines.len()))
                            .default_open(true)
                            .show(ui, |ui| {
                                for line in lines {
                                    ui.monospace(line);
                                }
                            });
                    }
                }
            });
    }

    pub(super) fn show_settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(crate::tr!(self.language, gui, settings_title));
        ui.add_space(4.0);

        let auto_yes_response = ui.checkbox(
            &mut self.auto_yes,
            crate::tr!(self.language, gui, auto_yes_setting),
        );
        if auto_yes_response.changed() {
            self.persist_state();
        }

        let show_not_found_response = ui.checkbox(
            &mut self.show_not_found,
            crate::tr!(self.language, gui, show_not_found),
        );
        if show_not_found_response.changed() {
            self.persist_state();
        }

        let show_up_to_date_response = ui.checkbox(
            &mut self.show_up_to_date,
            crate::tr!(self.language, gui, show_up_to_date),
        );
        if show_up_to_date_response.changed() {
            self.persist_state();
        }

        ui.add_space(8.0);
        ui.label(crate::tr!(self.language, gui, settings_saved));

        ui.add_space(8.0);
        let previous_language = self.language;
        egui::ComboBox::from_label(crate::tr!(self.language, gui, language_label))
            .selected_text(self.language.native_name())
            .show_ui(ui, |ui| {
                for language in crate::localization::Language::ALL {
                    ui.selectable_value(&mut self.language, language, language.native_name());
                }
            });
        if previous_language != self.language {
            self.status_line = crate::tr!(
                self.language,
                gui,
                language_changed,
                language = self.language.native_name()
            );
            self.persist_state();
        }

        ui.add_space(8.0);
        ui.separator();
        ui.small(crate::tr!(
            self.language,
            gui,
            version,
            value = crate::build_info::VERSION
        ));
        ui.small(crate::tr!(
            self.language,
            gui,
            commit,
            value = crate::build_info::GIT_HASH
        ));
        ui.small(crate::tr!(
            self.language,
            gui,
            dirty,
            value = crate::build_info::GIT_DIRTY
        ));

        egui::CollapsingHeader::new(crate::tr!(self.language, gui, build_details))
            .default_open(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        for (section, entries) in crate::build_info::details(self.language) {
                            ui.strong(section);
                            for (label, value) in entries {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(format!("{label}:"));
                                    ui.monospace(value);
                                });
                            }
                            ui.add_space(6.0);
                        }
                    });
            });
    }

    fn elevation_warning_text(&self) -> Option<&'static str> {
        if system::should_warn_about_elevation() {
            Some(crate::tr!(self.language, gui, elevation_warning))
        } else {
            None
        }
    }
}
