use super::GuiApp;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

impl GuiApp {
    pub(super) fn append_log(&mut self, message: String) {
        let (module, text) = split_module_log(&message);
        self.logs.entry(module).or_default().push(text);
    }

    pub(super) fn log_count(&self) -> usize {
        self.logs.values().map(Vec::len).sum()
    }

    pub(super) fn export_logs(&mut self) {
        match export_logs_to_file(&self.logs) {
            Ok(path) => {
                let message = format!("Логи экспортированы: {}", path.display());
                self.status_line = message.clone();
                self.append_log(format!("[system] {message}"));
            }
            Err(error) => {
                let message = format!("Не удалось экспортировать логи: {error}");
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

fn export_logs_to_file(logs: &BTreeMap<String, Vec<String>>) -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()
        .map_err(|error| anyhow::anyhow!("не удалось определить рабочую директорию: {error}"))?;

    let logs_dir = cwd.join("logs");
    fs::create_dir_all(&logs_dir)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| anyhow::anyhow!("ошибка времени системы: {error}"))?
        .as_secs();

    let path = logs_dir.join(format!("update_all_modules_logs_{now}.txt"));
    let content = if logs.is_empty() {
        "Логи отсутствуют\n".to_owned()
    } else {
        let mut text = String::new();
        for (module, lines) in logs {
            text.push_str(&format!("[{module}]\n"));
            for line in lines {
                text.push_str(line);
                text.push('\n');
            }
            text.push('\n');
        }
        text
    };

    fs::write(&path, content)?;
    Ok(path)
}
