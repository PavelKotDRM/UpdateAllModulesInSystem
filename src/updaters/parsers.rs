//! Парсеры текстового и JSON-вывода внешних менеджеров пакетов.

use crate::model::PackageUpdate;
use crate::updater::UpdaterError;
use serde::Deserialize;
use std::cmp::Ordering;

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

pub(super) fn parse_dnf_updates(text: &str, manager: &str) -> Vec<PackageUpdate> {
    text.lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line
                    .to_ascii_lowercase()
                    .starts_with("last metadata expiration")
                && !line.to_ascii_lowercase().starts_with("available upgrades")
                && !line.to_ascii_lowercase().starts_with("obsoleting packages")
        })
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?;
            let available = fields.next()?;
            let _repository = fields.next()?;
            let name_lower = name.to_ascii_lowercase();
            if matches!(
                name_lower.as_str(),
                "name" | "package" | "last" | "error:" | "warning:"
            ) || available.eq_ignore_ascii_case("version")
                || !available
                    .chars()
                    .any(|character| character.is_ascii_digit())
            {
                return None;
            }

            Some(PackageUpdate::new(
                format!("{manager}:{name}"),
                "installed",
                available,
            ))
        })
        .collect()
}

pub(super) fn parse_zypper_updates(text: &str) -> Vec<PackageUpdate> {
    text.lines()
        .filter_map(|line| {
            let fields = line.split('|').map(str::trim).collect::<Vec<_>>();
            if fields.len() < 5 {
                return None;
            }
            let offset = usize::from(fields.len() >= 6 && fields[0].chars().count() <= 1);
            let repository = *fields.get(offset)?;
            let name = *fields.get(offset + 1)?;
            let current = *fields.get(offset + 2)?;
            let available = *fields.get(offset + 3)?;

            (!name.is_empty()
                && !repository.eq_ignore_ascii_case("repository")
                && !name.eq_ignore_ascii_case("name")
                && !current.is_empty()
                && !available.is_empty()
                && !current.eq_ignore_ascii_case("current version"))
            .then(|| PackageUpdate::new(format!("zypper:{name}"), current, available))
        })
        .collect()
}

pub(super) fn parse_snap_updates(text: &str) -> Vec<PackageUpdate> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let lower = line.to_ascii_lowercase();
            if lower.starts_with("all snaps")
                || lower.starts_with("error")
                || lower.starts_with("warning")
            {
                return None;
            }
            let mut fields = line.split_whitespace();
            let name = fields.next()?;
            let available = fields.next()?;

            if name.eq_ignore_ascii_case("name") || available.eq_ignore_ascii_case("version") {
                return None;
            }

            Some(PackageUpdate::new(
                format!("snap:{name}"),
                "installed",
                available,
            ))
        })
        .collect()
}

pub(super) fn parse_rustup_updates(text: &str) -> Vec<PackageUpdate> {
    text.lines()
        .filter_map(|line| {
            let (toolchain, details) = line.split_once(" - ")?;
            let (status, versions) = details.split_once(':')?;
            if !status.trim().eq_ignore_ascii_case("update available") {
                return None;
            }
            let (current, available) = versions.split_once("->")?;
            let current = current.split_whitespace().next()?;
            let available = available.split_whitespace().next()?;

            (!toolchain.trim().is_empty() && !current.is_empty() && !available.is_empty()).then(
                || PackageUpdate::new(format!("rustup:{}", toolchain.trim()), current, available),
            )
        })
        .collect()
}

/// Разбирает машинный формат `brew outdated --json=v2`.
pub(super) fn parse_brew_updates(text: &str) -> Result<Vec<PackageUpdate>, UpdaterError> {
    #[derive(Deserialize)]
    struct BrewOutdated {
        #[serde(default)]
        formulae: Vec<BrewPackage>,
        #[serde(default)]
        casks: Vec<BrewPackage>,
    }

    #[derive(Deserialize)]
    struct BrewPackage {
        name: String,
        #[serde(default)]
        installed_versions: Vec<String>,
        current_version: String,
    }

    let outdated = serde_json::from_str::<BrewOutdated>(text)?;
    let formulae = outdated
        .formulae
        .into_iter()
        .map(|package| ("formula", package));
    let casks = outdated.casks.into_iter().map(|package| ("cask", package));

    Ok(formulae
        .chain(casks)
        .filter_map(|(kind, package)| {
            let installed = package.installed_versions.last()?.clone();
            (installed != package.current_version).then(|| {
                PackageUpdate::new(
                    format!("brew:{}", package.name),
                    installed,
                    package.current_version,
                )
                .with_scope(kind)
            })
        })
        .collect())
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
                || !is_winget_upgrade(current, available)
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

fn is_winget_upgrade(current: &str, available: &str) -> bool {
    if current.eq_ignore_ascii_case(available) {
        return false;
    }

    match (parse_semver(current), parse_semver(available)) {
        (Some(current), Some(available)) => available > current,
        _ => match (numeric_version_key(current), numeric_version_key(available)) {
            (Some(current_key), Some(available_key))
                if version_pattern(current) == version_pattern(available) =>
            {
                available_key.cmp(&current_key) == Ordering::Greater
            }
            _ => true,
        },
    }
}

fn parse_semver(version: &str) -> Option<semver::Version> {
    semver::Version::parse(version).ok()
}

fn numeric_version_key(version: &str) -> Option<Vec<u64>> {
    let mut key = Vec::new();
    let mut digits = String::new();

    for character in version.chars() {
        if character.is_ascii_digit() {
            digits.push(character);
        } else if !digits.is_empty() {
            key.push(digits.parse().ok()?);
            digits.clear();
        }
    }

    if !digits.is_empty() {
        key.push(digits.parse().ok()?);
    }

    (!key.is_empty()).then_some(key)
}

fn version_pattern(version: &str) -> String {
    let mut pattern = String::new();
    let mut in_digits = false;

    for character in version.chars() {
        if character.is_ascii_digit() {
            if !in_digits {
                pattern.push('#');
                in_digits = true;
            }
        } else {
            in_digits = false;
            pattern.push(character.to_ascii_lowercase());
        }
    }

    pattern
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
            if name.is_empty() || current.is_empty() || available.is_empty() || current == available
            {
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
    fn parse_dnf_updates_reads_package_versions_and_skips_headers() {
        let text = "\
Last metadata expiration check: 0:05:11 ago\n\
Package                 Version       Repository\n\
git.x86_64               2.45.1-1      updates\n\
";

        let updates = parse_dnf_updates(text, "dnf");

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "dnf:git.x86_64");
        assert_eq!(updates[0].current_version, "installed");
        assert_eq!(updates[0].available_version, "2.45.1-1");
    }

    #[test]
    fn parse_zypper_updates_handles_status_and_plain_tables() {
        let text = "\
S | Repository | Name | Current Version | Available Version | Arch\n\
-+------------+------+-----------------+-------------------+------\n\
v | repo-oss   | vim  | 9.1.0-1         | 9.1.1-1           | x86_64\n\
";

        let updates = parse_zypper_updates(text);

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "zypper:vim");
        assert_eq!(updates[0].current_version, "9.1.0-1");
        assert_eq!(updates[0].available_version, "9.1.1-1");
    }

    #[test]
    fn parse_snap_updates_reads_selected_snap_names() {
        let text = "\
Name      Version        Rev  Tracking       Publisher  Notes\n\
firefox   140.0.1-2      6100 latest/stable  mozilla✓   -\n\
";

        let updates = parse_snap_updates(text);

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "snap:firefox");
        assert_eq!(updates[0].available_version, "140.0.1-2");
    }

    #[test]
    fn parse_rustup_updates_reads_toolchain_versions() {
        let text = "\
stable-x86_64-unknown-linux-gnu - Update available : 1.90.0 (hash-old 2026-08-01) -> 1.91.0 (hash-new 2026-09-01)\n\
nightly-x86_64-unknown-linux-gnu - Up to date : 1.92.0-nightly (hash 2026-09-20)\n\
";

        let updates = parse_rustup_updates(text);

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "rustup:stable-x86_64-unknown-linux-gnu");
        assert_eq!(updates[0].current_version, "1.90.0");
        assert_eq!(updates[0].available_version, "1.91.0");
    }

    #[test]
    fn parse_rustup_updates_accepts_lowercase_status() {
        let text = "nightly-x86_64-pc-windows-msvc - update available: 1.100.0-nightly (old) -> 1.100.0-nightly (new)\n";

        let updates = parse_rustup_updates(text);

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "rustup:nightly-x86_64-pc-windows-msvc");
        assert_eq!(updates[0].current_version, "1.100.0-nightly");
        assert_eq!(updates[0].available_version, "1.100.0-nightly");
    }

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
    fn parse_brew_updates_reads_formulae_and_casks_from_json() {
        let text = r#"{
            "formulae": [{
                "name": "git",
                "installed_versions": ["2.45.0"],
                "current_version": "2.46.0"
            }],
            "casks": [{
                "name": "firefox",
                "installed_versions": ["128.0"],
                "current_version": "129.0"
            }]
        }"#;

        let updates = parse_brew_updates(text).unwrap();

        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].name, "brew:git");
        assert_eq!(updates[0].current_version, "2.45.0");
        assert_eq!(updates[0].available_version, "2.46.0");
        assert_eq!(updates[1].name, "brew:firefox");
        assert_eq!(updates[1].available_version, "129.0");
        assert_eq!(updates[0].scope.as_deref(), Some("formula"));
        assert_eq!(updates[1].scope.as_deref(), Some("cask"));
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
    fn parse_choco_updates_ignores_packages_with_matching_versions() {
        let text = "\
chocolatey|2.7.3|2.7.3|false\n\
python|3.14.7|3.14.7|false\n\
git|2.45.0|2.46.0|false\n\
";

        let updates = parse_choco_updates(text);

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "choco:git");
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
    fn parse_winget_updates_ignores_matching_or_older_versions() {
        let text = "\
Name             Id                    Version       Available\n\
----------------------------------------------------------------\n\
Git              Git.Git               2.46.0        2.46.0\n\
Unity 6000.6.2f1 Unity.Unity.6000      6000.6.2f1     6000.6.0f1\n\
Python           Python.Python         3.12.0        3.12.4\n\
";

        let updates = parse_winget_updates(text);

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "winget:Python | Python.Python");
    }

    #[test]
    fn parse_winget_updates_accepts_unity_f_release_versions() {
        let text = "\
Name             Id                    Version       Available\n\
----------------------------------------------------------------\n\
Unity 6000.6.0f1 Unity.Unity.6000      6000.6.0f1     6000.6.2f1\n\
";

        let updates = parse_winget_updates(text);

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].current_version, "6000.6.0f1");
        assert_eq!(updates[0].available_version, "6000.6.2f1");
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
