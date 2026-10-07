//! Commands for discovering and managing Python installations.

pub mod dir;
pub mod find;
pub mod install;
pub mod list;
pub mod pin;
pub mod uninstall;
pub mod update_shell;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum ChangeEventKind {
    /// The Python version was uninstalled.
    Removed,
    /// The Python version was installed.
    Added,
    /// The Python version was reinstalled.
    Reinstalled,
}

#[derive(Debug)]
pub(crate) struct ChangeEvent {
    key: uv_python_types::PythonInstallationKey,
    kind: ChangeEventKind,
}
