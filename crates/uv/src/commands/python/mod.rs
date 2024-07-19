pub(crate) mod dir;
pub(crate) mod find;
pub(crate) mod install;
pub(crate) mod list;
pub(crate) mod pin;
pub(crate) mod uninstall;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum ChangeEventKind {
    /// The Python version was uninstalled.
    Removed,
    /// The Python version was installed.
    Added,
}

#[derive(Debug)]
pub(super) struct ChangeEvent {
    key: uv_python::PythonInstallationKey,
    kind: ChangeEventKind,
}
