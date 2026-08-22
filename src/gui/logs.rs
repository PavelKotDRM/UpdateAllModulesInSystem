//! Сбор, группировка и экспорт журналов GUI.
//!
//! Сообщения вида `[модуль] текст` группируются по имени модуля. Сообщения
//! без такого префикса относятся к группе `system`. Экспорт сохраняет все
//! группы в JSON-формате в буфер обмена.

use super::GuiApp;

impl GuiApp {
    pub(super) fn append_log(&mut self, message: String) {
        let (module, text) = split_module_log(&message);
        self.logs.entry(module).or_default().push(text);
    }

    pub(super) fn log_count(&self) -> usize {
        self.logs.values().map(Vec::len).sum()
    }

    pub(super) fn export_logs(&mut self, ctx: &egui::Context) {
        match serde_json::to_string_pretty(&self.logs) {
            Ok(logs_json) => {
                ctx.copy_text(logs_json);
                let message = "Логи скопированы в буфер обмена в формате JSON".to_owned();
                self.status_line = message.clone();
                self.append_log(format!("[system] {message}"));
            }
            Err(error) => {
                let message = format!("Не удалось подготовить логи к экспорту: {error}");
                self.status_line = message.clone();
                self.append_log(format!("[system] Ошибка: {message}"));
            }
        }
    }
}

pub(super) fn split_module_log(message: &str) -> (String, String) {
    if let Some(rest) = message.strip_prefix('[')
        && let Some((module, text)) = rest.split_once("] ")
        && !module.is_empty()
    {
        return (module.to_owned(), text.to_owned());
    }

    ("system".to_owned(), message.to_owned())
}
