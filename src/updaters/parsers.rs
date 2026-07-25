//! Парсеры текстового и JSON-вывода внешних менеджеров пакетов.

use crate::model::PackageUpdate;
use crate::updater::UpdaterError;
use serde::Deserialize;

pub(super) fn parse_windows_update_items(text: &str) -> Result<Vec<PackageUpdate>, UpdaterError> {
    #[derive(Debug, Deserialize)]
    struct WindowsUpdateItem {
        title: String,
    }

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    if trimmed == "[]" {
        return Ok(Vec::new());
    }

    let items = if trimmed.starts_with('[') {
        serde_json::from_str::<Vec<WindowsUpdateItem>>(trimmed)?
    } else {
        vec![serde_json::from_str::<WindowsUpdateItem>(trimmed)?]
    };

    Ok(items
        .into_iter()
        .map(|item| {
            PackageUpdate::new(
                format!("windows-update:{}", item.title),
                "installed",
                "available",
            )
        })
        .collect())
}

pub(super) fn parse_winget_updates(text: &str) -> Vec<PackageUpdate> {
    let mut lines = text.lines();
    let Some(header) = lines.find(|line| winget_column_positions(line).is_some()) else {
        return Vec::new();
    };
    let Some(columns) = winget_column_positions(header) else {
        return Vec::new();
    };

    lines
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with('-') && !line.starts_with("----"))
        .filter(|line| !line.to_ascii_lowercase().contains("upgrades available"))
        .filter_map(|line| {
            let name = slice_chars(line, columns.name, columns.id).trim();
            let package_id = slice_chars(line, columns.id, columns.version).trim();
            let current = slice_chars(line, columns.version, columns.available).trim();
            let available = slice_chars(
                line,
                columns.available,
                columns.source.unwrap_or_else(|| line.chars().count()),
            )
            .trim();
            if name.is_empty() || package_id.is_empty() || current.is_empty() || available.is_empty() {
                return None;
            }

            Some(PackageUpdate::new(
                format!("winget:{name} | {package_id}"),
                current,
                available,
            ))
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct WingetColumnPositions {
    name: usize,
    id: usize,
    version: usize,
    available: usize,
    source: Option<usize>,
}

fn winget_column_positions(header: &str) -> Option<WingetColumnPositions> {
    let name = find_column_start(header, &["Name", "Имя"])?;
    let id = find_column_start(header, &["Id", "ИД"])?;
    let version = find_column_start(header, &["Version", "Версия"])?;
    let available = find_column_start(header, &["Available", "Доступна"])?;
    let source = find_column_start(header, &["Source", "Источник"]);

    (name < id && id < version && version < available).then_some(WingetColumnPositions {
        name,
        id,
        version,
        available,
        source,
    })
}

fn find_column_start(header: &str, labels: &[&str]) -> Option<usize> {
    labels.iter().find_map(|label| {
        header
            .find(label)
            .map(|byte_index| header[..byte_index].chars().count())
    })
}

fn slice_chars(text: &str, start: usize, end: usize) -> &str {
    let start_byte = text
        .char_indices()
        .nth(start)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    let end_byte = text
        .char_indices()
        .nth(end)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    &text[start_byte..end_byte]
}

pub(super) fn parse_choco_updates(text: &str) -> Vec<PackageUpdate> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("Chocolatey") && !line.starts_with("Outdated Packages"))
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() < 3 {
                return None;
            }

            let name = parts[0].trim();
            let current = parts[1].trim();
            let available = parts[2].trim();
            if name.is_empty() || current.is_empty() || available.is_empty() {
                return None;
            }

            Some(PackageUpdate::new(
                format!("choco:{name}"),
                current,
                available,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_choco_updates_reads_limit_output_lines() {
        let text = "\
git|2.45.0|2.46.0|false\n\
python|3.12.0|3.12.4|false\n\
";

        let updates = parse_choco_updates(text);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].name, "choco:git");
        assert_eq!(updates[0].current_version, "2.45.0");
        assert_eq!(updates[0].available_version, "2.46.0");
        assert_eq!(updates[1].name, "choco:python");
    }

    #[test]
    fn parse_winget_updates_reads_table_lines() {
        let text = "\
Name             Id                    Version     Available\n\
----------------------------------------------------------------\n\
Git              Git.Git               2.45.0      2.46.0\n\
Python 3.12      Python.Python.3.12    3.12.0      3.12.4\n\
";

        let updates = parse_winget_updates(text);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].name, "winget:Git | Git.Git");
        assert_eq!(updates[0].current_version, "2.45.0");
        assert_eq!(updates[0].available_version, "2.46.0");
        assert_eq!(updates[1].name, "winget:Python 3.12 | Python.Python.3.12");
    }

    #[test]
    fn parse_winget_updates_reads_source_column_without_version_shift() {
        let text = "\
Name                                      Id                                Version      Available    Source\n\
-----------------------------------------------------------------------------------------------------------\n\
Epic Online Services                      EpicGames.EOS                     4.3.1        4.3.2        winget\n\
Microsoft Visual C++ 2013 Redistribut...  Microsoft.VCRedist.2013.x64       12.0.30501   12.0.40664.0 winget\n\
";

        let updates = parse_winget_updates(text);
        assert_eq!(updates.len(), 2);
        assert_eq!(
            updates[0].name,
            "winget:Epic Online Services | EpicGames.EOS"
        );
        assert_eq!(updates[0].current_version, "4.3.1");
        assert_eq!(updates[0].available_version, "4.3.2");
        assert_eq!(
            updates[1].name,
            "winget:Microsoft Visual C++ 2013 Redistribut... | Microsoft.VCRedist.2013.x64"
        );
        assert_eq!(updates[1].current_version, "12.0.30501");
        assert_eq!(updates[1].available_version, "12.0.40664.0");
    }

    #[test]
    fn parse_winget_updates_uses_header_boundaries_for_redirected_output() {
        let text = format!(
            "{:<61}{:<29}{:<19}{:<21}Source\n{}\n{:<61}{:<29}{:<19}{:<21}winget\n{:<61}{:<29}{:<19}{:<21}winget\n2 upgrades available.\n",
            "Name",
            "Id",
            "Version",
            "Available",
            "-".repeat(136),
            "Epic Online Services",
            "EpicGames.EpicOnlineServices",
            "4.2.1",
            "4.3.1",
            "Microsoft Visual C++ 2013 Redistributable (x86) - 12.0.30501",
            "Microsoft.VCRedist.2013.x86",
            "12.0.30501.0",
            "12.0.40664.0",
        );

        let updates = parse_winget_updates(&text);
        assert_eq!(updates.len(), 2);
        assert_eq!(
            updates[0].name,
            "winget:Epic Online Services | EpicGames.EpicOnlineServices"
        );
        assert_eq!(updates[0].current_version, "4.2.1");
        assert_eq!(updates[0].available_version, "4.3.1");
        assert_eq!(
            updates[1].name,
            "winget:Microsoft Visual C++ 2013 Redistributable (x86) - 12.0.30501 | Microsoft.VCRedist.2013.x86"
        );
        assert_eq!(updates[1].current_version, "12.0.30501.0");
        assert_eq!(updates[1].available_version, "12.0.40664.0");
    }

    #[test]
    fn parse_windows_update_items_handles_array_and_single_object() {
        let array = r#"[{"title":"Cumulative Update for Windows 11"}]"#;
        let updates = parse_windows_update_items(array).expect("array json should parse");
        assert_eq!(updates.len(), 1);
        assert_eq!(
            updates[0].name,
            "windows-update:Cumulative Update for Windows 11"
        );

        let object = r#"{"title":"Security Update KB5000001"}"#;
        let updates = parse_windows_update_items(object).expect("object json should parse");
        assert_eq!(updates.len(), 1);
        assert_eq!(
            updates[0].name,
            "windows-update:Security Update KB5000001"
        );
    }
}
