//! Резервный парсер неструктурированного вывода менеджеров пакетов.
//!
//! Эвристика предназначена для простых строковых таблиц. Форматы JSON и таблицы
//! с известной схемой разбираются специализированными функциями в `updaters`.

use crate::model::PackageUpdate;

/// Эвристически извлекает обновления из строк `name current available`.
///
/// Для форматов с устойчивой структурой следует использовать специализированный
/// парсер: эта функция намеренно пропускает шум, но не валидирует версии.
pub fn heuristic_parse_updates(manager: &'static str, text: &str) -> Vec<PackageUpdate> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !looks_like_noise(line))
        .filter_map(|line| parse_update_line(manager, line))
        .collect()
}

fn looks_like_noise(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("no packages")
        || lower.contains("no updates")
        || lower.contains("nothing to do")
        || lower.contains("up to date")
        || lower.starts_with("warning")
        || lower.starts_with("error")
        || lower.starts_with("name")
        || lower.starts_with("version")
        || lower.starts_with("package")
        || line
            .chars()
            .all(|ch| ch == '-' || ch == '=' || ch.is_whitespace())
}

fn parse_update_line(manager: &'static str, line: &str) -> Option<PackageUpdate> {
    let tokens: Vec<&str> = line
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect();

    if tokens.is_empty() {
        return None;
    }

    let name_token = tokens[0].trim_matches(|ch: char| ch == ':' || ch == '|' || ch == ',');
    if name_token.is_empty() {
        return None;
    }

    let current = tokens.get(1).copied().unwrap_or("?");
    let available = if tokens.get(2) == Some(&"->") {
        tokens.get(3).copied().unwrap_or("?")
    } else {
        tokens.get(2).copied().unwrap_or("?")
    };
    Some(PackageUpdate::new(
        format!("{manager}:{name_token}"),
        current,
        available,
    ))
}
