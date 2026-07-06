use crate::model::{ModuleKind, PackageUpdate};
use crate::updater::{Updater, UpdaterError};
use std::sync::mpsc::Sender;

pub type CheckFn = fn() -> Result<Vec<PackageUpdate>, UpdaterError>;
pub type InstalledFn = fn() -> bool;
pub type ApplyFn = fn(bool, &[PackageUpdate], &Sender<String>) -> Result<(), UpdaterError>;

pub struct UpdaterDescriptor {
    pub updater: Box<dyn Updater>,
    pub kind: ModuleKind,
    pub requires_elevation: bool,
}

struct FunctionUpdater {
    name: &'static str,
    installed: InstalledFn,
    check: CheckFn,
    apply: ApplyFn,
}

#[derive(Clone, Copy)]
pub struct UpdaterSpec {
    pub name: &'static str,
    pub kind: ModuleKind,
    pub requires_elevation: bool,
    pub installed: InstalledFn,
    pub check: CheckFn,
    pub apply: ApplyFn,
}

impl Updater for FunctionUpdater {
    fn name(&self) -> &'static str {
        self.name
    }

    fn is_installed(&self) -> bool {
        (self.installed)()
    }

    fn check_updates(&self) -> Result<Vec<PackageUpdate>, UpdaterError> {
        (self.check)()
    }

    fn apply_updates(
        &self,
        force_yes: bool,
        selected_updates: &[PackageUpdate],
        log_sender: &Sender<String>,
    ) -> Result<(), UpdaterError> {
        (self.apply)(force_yes, selected_updates, log_sender)
    }
}

impl UpdaterSpec {
    pub fn into_descriptor(self) -> UpdaterDescriptor {
        UpdaterDescriptor {
            updater: Box::new(FunctionUpdater {
                name: self.name,
                installed: self.installed,
                check: self.check,
                apply: self.apply,
            }),
            kind: self.kind,
            requires_elevation: self.requires_elevation,
        }
    }
}
