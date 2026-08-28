//! Информация о версии и сборке приложения.

/// Версия пакета из `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Полный идентификатор Git-коммита.
pub const GIT_HASH: &str = env!("VERGEN_GIT_SHA");

/// Признак наличия незакоммиченных изменений при сборке.
pub const GIT_DIRTY: &str = env!("VERGEN_GIT_DIRTY");

/// Сгруппированная информация о приложении, исходниках и компиляции.
pub const DETAILS: &[(&str, &[(&str, &str)])] = &[
    (
        "Приложение",
        &[("Версия", VERSION), ("Профиль", env!("BUILD_PROFILE"))],
    ),
    (
        "Git",
        &[
            ("Ветка", env!("VERGEN_GIT_BRANCH")),
            ("SHA", GIT_HASH),
            ("Время коммита", env!("VERGEN_GIT_COMMIT_TIMESTAMP")),
            ("Есть изменения", GIT_DIRTY),
        ],
    ),
    (
        "Компилятор",
        &[
            ("Версия Rust", env!("BUILD_RUSTC_VERSION")),
            ("Уровень оптимизации", env!("BUILD_OPT_LEVEL")),
            ("Отладочная информация", env!("BUILD_DEBUG")),
        ],
    ),
    (
        "Целевая платформа",
        &[
            ("Target", env!("BUILD_TARGET")),
            ("Host", env!("BUILD_HOST")),
        ],
    ),
];

#[cfg(test)]
mod tests {
    use super::DETAILS;

    #[test]
    fn detailed_build_info_covers_source_compiler_and_target() {
        let sections = DETAILS
            .iter()
            .map(|(section, _)| *section)
            .collect::<Vec<_>>();

        assert_eq!(
            sections,
            ["Приложение", "Git", "Компилятор", "Целевая платформа"]
        );
        assert!(DETAILS.iter().all(|(_, entries)| {
            !entries.is_empty() && entries.iter().all(|(_, value)| !value.trim().is_empty())
        }));
    }
}
