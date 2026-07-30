//! Парсеры текстового и JSON-вывода внешних менеджеров пакетов.

use crate::model::PackageUpdate;
use crate::updater::UpdaterError;
use serde::Deserialize;

/// Разбирает как `apt list --upgradable`, так и симуляцию APT-RPM.
///
/// Для строк `Inst name [current] (candidate repository)` версии извлекаются
/// по структурным скобкам, поэтому RPM epoch и суффиксы ALT сохраняются целиком.
pub(super) fn parse_apt_updates(text: &str, manager: &str) -> Vec<PackageUpdate> {
    text.lines()
        .map(str::trim)
        .filter_map(|line| parse_apt_update_line(line, manager))
        .collect()
}

pub(super) fn parse_flatpak_updates(text: &str) -> Vec<PackageUpdate> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let columns = line.split('\t').map(str::trim).collect::<Vec<_>>();
            let (application, available) = match columns.as_slice() {
                [application, available, ..] => (*application, *available),
                _ => {
                    let mut fields = line.split_whitespace();
                    (fields.next()?, fields.next()?)
                }
            };

            (!application.eq_ignore_ascii_case("application")
                && !application.is_empty()
                && !available.is_empty())
            .then(|| PackageUpdate::new(format!("flatpak:{application}"), "installed", available))
        })
        .collect()
}

pub(super) fn parse_pkcon_updates(text: &str) -> Vec<PackageUpdate> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.contains("[===="))
        .filter_map(|line| {
            let package_id = line
                .split_whitespace()
                .find(|field| field.matches(';').count() >= 3)?;
            let mut fields = package_id.split(';');
            let name = fields.next()?;
            let available = fields.next()?;

            (!name.is_empty() && !available.is_empty())
                .then(|| PackageUpdate::new(format!("pkcon:{name}"), "installed", available))
        })
        .collect()
}

fn parse_apt_update_line(line: &str, manager: &str) -> Option<PackageUpdate> {
    if let Some(rest) = line.strip_prefix("Inst ") {
        return parse_apt_get_install_line(rest, manager);
    }

    parse_apt_list_update_line(line, manager)
}

fn parse_apt_get_install_line(rest: &str, manager: &str) -> Option<PackageUpdate> {
    let (name, version_info) = rest.split_once(char::is_whitespace)?;
    let version_info = version_info.trim_start();
    let (current, candidate_info) = if let Some(old_version) = version_info.strip_prefix('[') {
        let (current, candidate_info) = old_version.split_once(']')?;
        (current.trim(), candidate_info.trim_start())
    } else {
        ("not installed", version_info)
    };
    let candidate_info = candidate_info.strip_prefix('(')?;
    let (candidate, _) = candidate_info.split_once(')')?;
    let available = candidate.split_whitespace().next()?;

    (!name.is_empty() && !current.is_empty() && !available.is_empty())
        .then(|| PackageUpdate::new(format!("{manager}:{name}"), current, available))
}

fn parse_apt_list_update_line(line: &str, manager: &str) -> Option<PackageUpdate> {
    let mut fields = line.split_whitespace();
    let package = fields.next()?;
    let available = fields.next()?;
    let architecture = fields.next()?;
    let details = fields.collect::<Vec<_>>().join(" ");
    let old_version_info = details.strip_prefix('[')?.strip_suffix(']')?;
    let current = old_version_info
        .split_whitespace()
        .last()?
        .trim_start_matches([':', '=']);
    let name = package.split_once('/').map_or(package, |(name, _)| name);

    (package.contains('/')
        && !available.is_empty()
        && !architecture.is_empty()
        && !current.is_empty())
    .then(|| PackageUpdate::new(format!("{manager}:{name}"), current, available))
}

/// Разбирает один объект или массив объектов Windows Update с полем `title`.
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

/// Разбирает таблицу `winget upgrade` по позициям локализованных заголовков.
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
            if name.is_empty()
                || !is_single_token(package_id)
                || !is_single_token(current)
                || !is_single_token(available)
            {
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

fn is_single_token(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_whitespace)
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

/// Разбирает машинный формат Chocolatey `name|current|available|...`.
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

/// Разбирает вывод MSYS2/pacman вида `name current -> available`.
pub(super) fn parse_msys2_updates(text: &str) -> Vec<PackageUpdate> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next()?;
            let current = parts.next()?;
            (parts.next()? == "->").then_some(())?;
            let available = parts.next()?;

            Some(PackageUpdate::new(
                format!("msys2:{name}"),
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
    fn parse_apt_updates_reads_list_versions() {
        let text = "Listing...\ncode/stable 1.102.2-1753187809 amd64 [upgradable from: 1.101.2-1750797935]\n";

        let updates = parse_apt_updates(text, "apt");

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "apt:code");
        assert_eq!(updates[0].current_version, "1.101.2-1750797935");
        assert_eq!(updates[0].available_version, "1.102.2-1753187809");
    }

    #[test]
    fn parse_apt_updates_reads_alt_linux_simulation() {
        let text = "Inst apt [0.5.15lorg2-alt88] (0.5.15lorg2-alt89 Sisyphus:classic [x86_64])\nInst glibc-core:i586 [6:2.38.0.76.e9f05-alt1] (6:2.38.0.76.e9f05-alt2 Sisyphus:classic [i586])\nConf apt (0.5.15lorg2-alt89 Sisyphus:classic [x86_64])\nRemv old-kernel [6.12.1-alt1]\n";

        let updates = parse_apt_updates(text, "apt-get");

        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].name, "apt-get:apt");
        assert_eq!(updates[0].current_version, "0.5.15lorg2-alt88");
        assert_eq!(updates[0].available_version, "0.5.15lorg2-alt89");
        assert_eq!(updates[1].name, "apt-get:glibc-core:i586");
        assert_eq!(updates[1].current_version, "6:2.38.0.76.e9f05-alt1");
        assert_eq!(updates[1].available_version, "6:2.38.0.76.e9f05-alt2");
    }

    #[test]
    fn parse_apt_updates_reads_new_dependencies_from_simulation() {
        let text = "Inst new-dependency (2.4.1-alt3 p11:classic [x86_64])\n";

        let updates = parse_apt_updates(text, "apt-get");

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "apt-get:new-dependency");
        assert_eq!(updates[0].current_version, "not installed");
        assert_eq!(updates[0].available_version, "2.4.1-alt3");
    }

    #[test]
    fn parse_apt_updates_rejects_incomplete_simulation_lines() {
        let text = "Inst missing-candidate [1.0-alt1]\nInst missing-bracket [1.0-alt1 (2.0-alt1 repo [x86_64])\nInst missing-parenthesis [1.0-alt1] (2.0-alt1 repo [x86_64]\n";

        assert!(parse_apt_updates(text, "apt-get").is_empty());
    }

    #[test]
    fn parse_flatpak_updates_reads_tab_separated_columns() {
        let text = "Application\tVersion\norg.mozilla.firefox\t128.0.3\norg.gimp.GIMP\t3.0.4\n";

        let updates = parse_flatpak_updates(text);

        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].name, "flatpak:org.mozilla.firefox");
        assert_eq!(updates[0].current_version, "installed");
        assert_eq!(updates[0].available_version, "128.0.3");
    }

    #[test]
    fn parse_pkcon_updates_ignores_progress_and_reads_package_ids() {
        let text = "Getting updates [=========================]\nNormal firefox;128.0.3-alt1;x86_64;sisyphus Firefox web browser\nBlocked kernel-image;6.12.40-alt1;x86_64;sisyphus Linux kernel\nFinished [=========================]\n";

        let updates = parse_pkcon_updates(text);

        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].name, "pkcon:firefox");
        assert_eq!(updates[0].available_version, "128.0.3-alt1");
        assert_eq!(updates[1].name, "pkcon:kernel-image");
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
    fn parse_msys2_updates_reads_pacman_query_output() {
        let text = "\
pacman 6.1.0-3 -> 7.0.0.r6.g1f38429-1\n\
mingw-w64-ucrt-x86_64-gcc 14.2.0-2 -> 15.1.0-1\n\
";

        let updates = parse_msys2_updates(text);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].name, "msys2:pacman");
        assert_eq!(updates[0].current_version, "6.1.0-3");
        assert_eq!(updates[0].available_version, "7.0.0.r6.g1f38429-1");
        assert_eq!(updates[1].name, "msys2:mingw-w64-ucrt-x86_64-gcc");
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
    fn parse_winget_updates_ignores_unknown_version_footer() {
        let text = "\
Name             Id                    Version     Available\n\
----------------------------------------------------------------\n\
Git              Git.Git               2.45.0      2.46.0\n\
1 package(s) have version numbers that cannot be determined. Use --include-unknown to see all results.\n\
";

        let updates = parse_winget_updates(text);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "winget:Git | Git.Git");
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
        assert_eq!(updates[0].name, "windows-update:Security Update KB5000001");
    }
}
