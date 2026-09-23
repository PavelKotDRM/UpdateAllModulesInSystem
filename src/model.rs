//! Типы доменной модели для описания модулей, статусов и доступных обновлений.

/// Категория модуля, определяющая источник обновлений.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ModuleKind {
    /// Системный менеджер пакетов.
    System,
    /// Инструмент или среда разработки.
    Tool,
    /// Менеджер Python-пакетов.
    Python,
}

/// Описание одного доступного обновления пакета.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PackageUpdate {
    /// Логическое имя пакета в формате, зависящем от менеджера.
    pub name: String,
    /// Текущая установленная версия.
    pub current_version: String,
    /// Версия, доступная для установки.
    pub available_version: String,
    /// Необязательная область установки, например профиль редактора.
    pub scope: Option<String>,
    /// Признак, выбран ли пакет пользователем для обновления.
    pub selected: bool,
}

impl PackageUpdate {
    /// Создаёт выбранное по умолчанию обновление без области установки.
    pub fn new(
        name: impl Into<String>,
        current_version: impl Into<String>,
        available_version: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            current_version: current_version.into(),
            available_version: available_version.into(),
            scope: None,
            selected: true,
        }
    }

    /// Добавляет область установки к записи обновления.
    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }

    /// Возвращает имя обновления с областью установки для интерфейса.
    pub fn display_name(&self) -> String {
        let language = crate::localization::current_language();
        self.scope
            .as_ref()
            .map(|scope| {
                let scope = if scope == "самообновление" {
                    crate::tr!(language, model, scope_node_self_update)
                } else {
                    scope
                };
                format!("[{scope}] {}", self.name)
            })
            .unwrap_or_else(|| self.name.clone())
    }

    /// Возвращает стабильный ключ выбора с учетом области установки.
    pub fn selection_key(&self) -> String {
        self.scope
            .as_ref()
            .map(|scope| format!("scope::{scope}::{}", self.name))
            .unwrap_or_else(|| self.name.clone())
    }
}

impl ModuleKind {
    /// Returns the translated user-facing label for this module category.
    pub fn label(self) -> &'static str {
        let language = crate::localization::current_language();
        match self {
            Self::System => crate::tr!(language, model, kind_system),
            Self::Tool => crate::tr!(language, model, kind_tool),
            Self::Python => crate::tr!(language, model, kind_python),
        }
    }
}

/// Состояние модуля после проверки обновлений.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ModuleStatus {
    /// Менеджер или инструмент не обнаружен в системе.
    NotFound,
    /// Обновления отсутствуют.
    UpToDate,
    /// Доступно указанное число обновлений.
    UpdatesAvailable(usize),
    /// Ошибка при проверке или выполнении операции.
    Error(String),
}

impl ModuleStatus {
    /// Возвращает локализованную метку для CLI и GUI.
    pub fn label(&self) -> String {
        let language = crate::localization::current_language();
        match self {
            Self::NotFound => crate::tr!(language, model, status_not_found).to_owned(),
            Self::UpToDate => crate::tr!(language, model, status_up_to_date).to_owned(),
            Self::UpdatesAvailable(count) => {
                crate::tr!(language, model, status_updates_available, count = count)
            }
            Self::Error(message) => crate::tr!(language, model, status_error, message = message),
        }
    }

    /// Проверяет, содержит ли состояние доступные обновления.
    pub fn has_updates(&self) -> bool {
        matches!(self, Self::UpdatesAvailable(_))
    }
}

/// Снимок состояния одного обновляемого модуля.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModuleSnapshot {
    /// Уникальное имя модуля.
    pub name: String,
    /// Категория модуля.
    pub kind: ModuleKind,
    /// Установлен ли модуль в системе.
    pub installed: bool,
    /// Выбран ли модуль пользователем для запуска обновления.
    pub selected: bool,
    /// Требуются ли повышенные привилегии для обновления.
    pub requires_elevation: bool,
    /// Поддерживает ли модуль безопасный выбор отдельных обновлений.
    #[serde(default = "package_selection_supported_by_default")]
    pub supports_package_selection: bool,
    /// Текущее состояние проверки.
    pub status: ModuleStatus,
    /// Список доступных обновлений внутри модуля.
    pub updates: Vec<PackageUpdate>,
}

impl ModuleSnapshot {
    /// Создаёт выбранный снимок со статусом [`ModuleStatus::NotFound`].
    pub fn new(name: impl Into<String>, kind: ModuleKind, requires_elevation: bool) -> Self {
        Self {
            name: name.into(),
            kind,
            installed: false,
            selected: true,
            requires_elevation,
            supports_package_selection: true,
            status: ModuleStatus::NotFound,
            updates: Vec::new(),
        }
    }

    /// Возвращает пользовательскую метку статуса с особыми инструкциями для
    /// Центра обновления Windows.
    pub fn status_label(&self) -> String {
        let language = crate::localization::current_language();
        if self.name == "windows-update"
            && let ModuleStatus::UpdatesAvailable(count) = self.status
        {
            return crate::tr!(language, model, status_windows_updates, count = count);
        }

        self.status.label()
    }

    /// Формирует строки `name: current -> available` либо одну строку-заглушку.
    pub fn detail_lines(&self) -> Vec<String> {
        if self.updates.is_empty() {
            return vec![
                crate::tr!(
                    crate::localization::current_language(),
                    model,
                    status_empty_updates
                )
                .to_owned(),
            ];
        }

        self.updates
            .iter()
            .map(|update| {
                format!(
                    "{}: {} -> {}",
                    update.display_name(),
                    update.current_version,
                    update.available_version
                )
            })
            .collect()
    }
}

fn package_selection_supported_by_default() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::localization::{Language, with_language};

    #[test]
    fn status_labels_and_has_updates_are_consistent() {
        assert_eq!(ModuleStatus::NotFound.label(), "Not installed");
        assert_eq!(ModuleStatus::UpToDate.label(), "Up to date");
        assert_eq!(
            ModuleStatus::UpdatesAvailable(3).label(),
            "Updates available: 3"
        );
        assert!(ModuleStatus::UpdatesAvailable(1).has_updates());
        assert!(!ModuleStatus::UpToDate.has_updates());
    }

    #[test]
    fn detail_lines_returns_placeholder_when_no_updates() {
        let module = ModuleSnapshot::new("pip", ModuleKind::Python, false);
        let details = module.detail_lines();
        assert_eq!(details, vec!["No individual updates to display".to_owned()]);
    }

    #[test]
    fn detail_lines_formats_updates() {
        let mut module = ModuleSnapshot::new("pip", ModuleKind::Python, false);
        module.updates = vec![
            PackageUpdate::new("requests", "2.31.0", "2.32.0"),
            PackageUpdate::new("urllib3", "2.0.0", "2.1.0"),
        ];

        let details = module.detail_lines();
        assert_eq!(details.len(), 2);
        assert_eq!(details[0], "requests: 2.31.0 -> 2.32.0");
        assert_eq!(details[1], "urllib3: 2.0.0 -> 2.1.0");
    }

    #[test]
    fn detail_lines_include_update_scope() {
        let mut module = ModuleSnapshot::new("vscode-extensions", ModuleKind::Tool, false);
        module.updates =
            vec![PackageUpdate::new("ms-python.python", "1.0.0", "2.0.0").with_scope("Python")];

        assert_eq!(
            module.detail_lines(),
            vec!["[Python] ms-python.python: 1.0.0 -> 2.0.0".to_owned()]
        );
        assert_eq!(
            module.updates[0].selection_key(),
            "scope::Python::ms-python.python"
        );
    }

    #[test]
    fn node_self_update_scope_is_displayed_in_the_selected_language() {
        let update = PackageUpdate::new("npm", "1.0", "2.0").with_scope("самообновление");

        assert_eq!(update.display_name(), "[self-update] npm");
        with_language(Language::Russian, || {
            assert_eq!(update.display_name(), "[самообновление] npm");
        });
    }

    #[test]
    fn status_labels_follow_the_selected_language() {
        with_language(Language::Russian, || {
            assert_eq!(ModuleStatus::NotFound.label(), "Не установлен");
            assert_eq!(ModuleStatus::UpToDate.label(), "Актуален");
            assert_eq!(
                ModuleStatus::UpdatesAvailable(3).label(),
                "Доступно обновлений: 3"
            );

            let mut module = ModuleSnapshot::new("windows-update", ModuleKind::System, true);
            module.status = ModuleStatus::UpdatesAvailable(2);
            assert_eq!(
                module.status_label(),
                "Доступно обновлений: 2. Установите их через Центр обновления Windows"
            );
        });
    }

    #[test]
    fn windows_update_status_recommends_update_center() {
        let mut module = ModuleSnapshot::new("windows-update", ModuleKind::System, true);
        module.status = ModuleStatus::UpdatesAvailable(2);

        assert_eq!(
            module.status_label(),
            "Available updates: 2. Install them through Windows Update"
        );
    }
}
