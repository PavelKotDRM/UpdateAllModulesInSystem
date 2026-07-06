#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleKind {
    System,
    Tool,
    Python,
}

#[derive(Debug, Clone)]
pub struct PackageUpdate {
    pub name: String,
    pub current_version: String,
    pub available_version: String,
    pub selected: bool,
}

impl PackageUpdate {
    pub fn new(name: impl Into<String>, current_version: impl Into<String>, available_version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            current_version: current_version.into(),
            available_version: available_version.into(),
            selected: true,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ModuleStatus {
    NotFound,
    UpToDate,
    UpdatesAvailable(usize),
    Error(String),
}

impl ModuleStatus {
    pub fn label(&self) -> String {
        match self {
            Self::NotFound => "Не найден в системе".to_owned(),
            Self::UpToDate => "Актуален".to_owned(),
            Self::UpdatesAvailable(count) => format!("Доступны обновления ({count} пакетов)"),
            Self::Error(message) => format!("Ошибка: {message}"),
        }
    }

    pub fn has_updates(&self) -> bool {
        matches!(self, Self::UpdatesAvailable(_))
    }
}

#[derive(Debug, Clone)]
pub struct ModuleSnapshot {
    pub name: String,
    pub kind: ModuleKind,
    pub installed: bool,
    pub selected: bool,
    pub requires_elevation: bool,
    pub status: ModuleStatus,
    pub updates: Vec<PackageUpdate>,
}

impl ModuleSnapshot {
    pub fn new(name: impl Into<String>, kind: ModuleKind, requires_elevation: bool) -> Self {
        Self {
            name: name.into(),
            kind,
            installed: false,
            selected: true,
            requires_elevation,
            status: ModuleStatus::NotFound,
            updates: Vec::new(),
        }
    }

    pub fn status_label(&self) -> String {
        self.status.label()
    }

    pub fn detail_lines(&self) -> Vec<String> {
        if self.updates.is_empty() {
            return vec!["Список конкретных обновлений пуст".to_owned()];
        }

        self.updates
            .iter()
            .map(|update| {
                format!(
                    "{}: {} -> {}",
                    update.name, update.current_version, update.available_version
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_labels_and_has_updates_are_consistent() {
        assert_eq!(ModuleStatus::NotFound.label(), "Не найден в системе");
        assert_eq!(ModuleStatus::UpToDate.label(), "Актуален");
        assert_eq!(
            ModuleStatus::UpdatesAvailable(3).label(),
            "Доступны обновления (3 пакетов)"
        );
        assert!(ModuleStatus::UpdatesAvailable(1).has_updates());
        assert!(!ModuleStatus::UpToDate.has_updates());
    }

    #[test]
    fn detail_lines_returns_placeholder_when_no_updates() {
        let module = ModuleSnapshot::new("pip", ModuleKind::Python, false);
        let details = module.detail_lines();
        assert_eq!(details, vec!["Список конкретных обновлений пуст".to_owned()]);
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
}
