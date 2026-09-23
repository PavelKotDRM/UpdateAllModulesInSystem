//! Информация о версии и сборке приложения.

/// Версия пакета из `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Полный идентификатор Git-коммита.
pub const GIT_HASH: &str = env!("VERGEN_GIT_SHA");

/// Признак наличия незакоммиченных изменений при сборке.
pub const GIT_DIRTY: &str = env!("VERGEN_GIT_DIRTY");

/// Сгруппированная информация о приложении, исходниках и компиляции.
pub fn details(
    language: crate::localization::Language,
) -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    let labels = crate::localization::catalog(language).build_info;
    vec![
        (
            labels.application,
            vec![
                (labels.version, VERSION),
                (labels.profile, env!("BUILD_PROFILE")),
            ],
        ),
        (
            labels.git,
            vec![
                (labels.branch, env!("VERGEN_GIT_BRANCH")),
                ("SHA", GIT_HASH),
                (labels.commit_time, env!("VERGEN_GIT_COMMIT_TIMESTAMP")),
                (labels.dirty, GIT_DIRTY),
            ],
        ),
        (
            labels.compiler,
            vec![
                (labels.rust_version, env!("BUILD_RUSTC_VERSION")),
                (labels.optimization, env!("BUILD_OPT_LEVEL")),
                (labels.debug_info, env!("BUILD_DEBUG")),
            ],
        ),
        (
            labels.target_platform,
            vec![
                (labels.target, env!("BUILD_TARGET")),
                (labels.host, env!("BUILD_HOST")),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::details;
    use crate::localization::Language;

    #[test]
    fn detailed_build_info_covers_source_compiler_and_target() {
        let english_details = details(Language::English);
        let sections = english_details
            .iter()
            .map(|(section, _)| *section)
            .collect::<Vec<_>>();

        assert_eq!(
            sections,
            ["Application", "Git", "Compiler", "Target platform"]
        );
        assert!(english_details.iter().all(|(_, entries)| {
            !entries.is_empty() && entries.iter().all(|(_, value)| !value.trim().is_empty())
        }));

        let russian_sections = details(Language::Russian)
            .into_iter()
            .map(|(section, _)| section)
            .collect::<Vec<_>>();
        assert_eq!(
            russian_sections,
            ["Приложение", "Git", "Компилятор", "Целевая платформа"]
        );
    }
}
