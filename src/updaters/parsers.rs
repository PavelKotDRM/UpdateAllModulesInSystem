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
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("Name") && !line.starts_with('-') && !line.starts_with("----"))
        .filter(|line| !line.to_ascii_lowercase().contains("upgrades available"))
        .filter_map(|line| {
            let cols = split_columns_by_wide_spaces(line);
            if cols.len() < 4 {
                return None;
            }

            let name = cols.first()?.trim();
            let current = cols.get(2)?.trim();
            let available = cols.get(3)?.trim();
            if name.is_empty() || current.is_empty() || available.is_empty() {
                return None;
            }

            Some(PackageUpdate::new(
                format!("winget:{name}"),
                current,
                available,
            ))
        })
        .collect()
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

pub(super) fn split_columns_by_wide_spaces(line: &str) -> Vec<String> {
    let mut columns = Vec::new();
    let mut current = String::new();
    let mut spaces = 0usize;

    for ch in line.chars() {
        if ch == ' ' {
            spaces += 1;
            if spaces >= 2 {
                if !current.trim().is_empty() {
                    columns.push(current.trim().to_owned());
                    current.clear();
                }
                continue;
            }
        } else {
            spaces = 0;
        }
        current.push(ch);
    }

    if !current.trim().is_empty() {
        columns.push(current.trim().to_owned());
    }

    columns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_columns_by_wide_spaces_splits_expected_columns() {
        let line = "Git.Git         Git   2.45.0     2.46.0";
        let columns = split_columns_by_wide_spaces(line);
        assert_eq!(columns, vec!["Git.Git", "Git", "2.45.0", "2.46.0"]);
    }

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
        assert_eq!(updates[0].name, "winget:Git");
        assert_eq!(updates[0].current_version, "2.45.0");
        assert_eq!(updates[0].available_version, "2.46.0");
        assert_eq!(updates[1].name, "winget:Python 3.12");
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
