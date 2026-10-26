/// The source to use for project author information.
#[derive(Debug, Default, Copy, Clone)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum AuthorFrom {
    /// Fetch the author information from some sources (e.g., Git) automatically.
    #[default]
    Auto,
    /// Fetch the author information from Git configuration only.
    Git,
    /// Do not infer the author information.
    None,
}

/// The kind of entity to initialize (either a PEP 723 script or a Python project).
#[derive(Debug, Copy, Clone)]
pub enum InitKind {
    /// Initialize a Python project.
    Project(InitProjectKind),
    /// Initialize a PEP 723 script.
    Script,
}

/// The kind of Python project to initialize (either an application or a library).
#[derive(Debug, Copy, Clone, Default)]
pub enum InitProjectKind {
    /// A python package with a `main` function in a `__init__.py` and a script entrypoint pointing
    /// to that.
    #[default]
    ApplicationWithLibrary,
    /// A flat application with a `main.py`.
    Application,
    /// A python package, no entrypoint.
    Library,
    /// Initialize only a `pyproject.toml`
    Bare,
    /// Initialize only a `pyproject.toml` with `[build-system]` table (but without associated
    /// source files).
    BareWithBuildSystem,
}
