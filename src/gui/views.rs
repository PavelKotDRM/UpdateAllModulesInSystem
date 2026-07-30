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
                .add_enabled(!self.busy, egui::Button::new("Проверить обновления"))
                .clicked()
            {
                self.started_scan = false;
                self.start_scan();
            }

            if ui
                .add_enabled(!self.busy, egui::Button::new("Обновить выбранное"))
                .clicked()
            {
                self.start_update();
            }

            if ui
                .add_enabled(!self.busy, egui::Button::new("Обновить все"))
                .clicked()
            {
                self.start_update_all();
            }

            let can_cancel = self
                .update_cancellation
                .as_ref()
                .is_some_and(|cancellation| !cancellation.is_cancelled());
            if can_cancel && ui.button("Отменить").clicked() {
                self.cancel_update();
            }

            if !system::is_admin() {
                let elevate_response = ui
                    .add_enabled(!self.elevation_pending, egui::Button::new("Повысить права"))
                    .on_hover_text("Перезапустить приложение с правами администратора");
                if elevate_response.clicked() {
                    self.start_elevated();
                }
            }

            ui.add_enabled_ui(!self.busy, |ui| {
                self.show_selection_menu(ui);
            });

            let auto_yes_response = ui.checkbox(&mut self.auto_yes, "Автоматическое согласие (-y)");
            if auto_yes_response.changed() {
                self.persist_state();
            }
        });

        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.active_tab, GuiTab::Overview, "Обзор");
            ui.selectable_value(&mut self.active_tab, GuiTab::Modules, "Модули");
            ui.selectable_value(&mut self.active_tab, GuiTab::Logs, "Логи");
            ui.selectable_value(&mut self.active_tab, GuiTab::Settings, "Настройки");
        });

        ui.separator();
        ui.label(&self.status_line);
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "Выбрано модулей: {}/{}",
                self.selected_count(),
                self.visible_modules_count()
            ));
            ui.separator();
            ui.label(format!(
                "С обновлениями: {}/{}",
                self.updates_count(),
                self.visible_modules_count()
            ));
        });
    }

    pub(super) fn show_overview_tab(&self, ui: &mut egui::Ui) {
        ui.heading("Сводка");
        ui.add_space(4.0);
        if let Some(warning) = self.elevation_warning_text() {
            ui.colored_label(egui::Color32::YELLOW, warning);
            ui.add_space(4.0);
        }

        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Найдено модулей: {}", self.visible_modules_count()));
            ui.separator();
            ui.label(format!("Выбрано: {}", self.selected_count()));
            ui.separator();
            ui.label(format!("Требуют обновления: {}", self.updates_count()));
            ui.separator();
            ui.label(format!("Логов накоплено: {}", self.log_count()));
        });

        ui.add_space(8.0);
        ui.label("Используйте вкладку 'Модули' для выбора пакетов, 'Логи' для просмотра прогресса и ошибок.");
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
                    ui.label("Список модулей пуст. Нажмите 'Проверить обновления'.");
                    return;
                }

                let available_width = ui.available_width();
                let columns = ((available_width / 420.0).floor() as usize).clamp(1, 3);
                let mut selection_changed = false;
                let visible_count = self.visible_modules_count();
                let module_progress = &self.module_progress;

                if visible_count == 0 {
                    ui.label("Нет отображаемых модулей. Включите показ не найденных в настройках или выполните сканирование.");
                    return;
                }

                if columns == 1 {
                    for module in &mut self.modules {
                        if !self.show_not_found && !module.installed {
                            continue;
                        }
                        let progress = module_progress.get(&module.name);
                        Self::render_module_card(ui, module, progress, &mut selection_changed);
                    }
                } else {
                    ui.columns(columns, |columns_ui| {
                        let mut visible_index = 0usize;
                        for module in &mut self.modules {
                            if !self.show_not_found && !module.installed {
                                continue;
                            }
                            let column = &mut columns_ui[visible_index % columns];
                            let progress = module_progress.get(&module.name);
                            Self::render_module_card(column, module, progress, &mut selection_changed);
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
        ui.heading("Console Log");
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Экспорт логов").clicked() {
                self.export_logs();
            }
            ui.label(format!("Модулей: {}", self.logs.len()));
            ui.separator();
            ui.label(format!("Записей: {}", self.log_count()));
        });
        ui.add_space(4.0);

        egui::ScrollArea::vertical()
            .id_salt("logs_scroll")
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.logs.is_empty() {
                    ui.label("Логи пока пусты");
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
        ui.heading("Настройки");
        ui.add_space(4.0);

        let auto_yes_response = ui.checkbox(
            &mut self.auto_yes,
            "Автоматическое согласие на обновление (-y)",
        );
        if auto_yes_response.changed() {
            self.persist_state();
        }

        let show_not_found_response =
            ui.checkbox(&mut self.show_not_found, "Показывать не найденные модули");
        if show_not_found_response.changed() {
            self.persist_state();
        }

        ui.add_space(8.0);
        ui.label(
            "Состояние выбранных модулей и настройки отображения сохраняются между запусками.",
        );

        ui.add_space(8.0);
        ui.separator();
        ui.small(format!("Версия: {}", crate::build_info::VERSION));
        ui.small(format!("Коммит: {}", crate::build_info::GIT_HASH));
        ui.small(format!(
            "Описание сборки: {}",
            crate::build_info::DESCRIPTION
        ));

        egui::CollapsingHeader::new("Подробная информация о сборке")
            .default_open(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        for (section, entries) in crate::build_info::DETAILS {
                            ui.strong(*section);
                            for (label, value) in *entries {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(format!("{label}:"));
                                    ui.monospace(*value);
                                });
                            }
                            ui.add_space(6.0);
                        }
                    });
            });
    }

    fn elevation_warning_text(&self) -> Option<&'static str> {
        if system::should_warn_about_elevation() {
            Some("Запуск без прав администратора: системные менеджеры могут быть недоступны")
        } else {
            None
        }
    }
}
