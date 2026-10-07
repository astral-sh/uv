use std::fmt::{Display, Formatter};

/// The kind of Python distribution being built.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum BuildKind {
    /// A PEP 517 wheel build.
    #[default]
    Wheel,
    /// A PEP 517 source distribution build.
    Sdist,
    /// A PEP 660 editable installation wheel build.
    Editable,
}

impl Display for BuildKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wheel => f.write_str("wheel"),
            Self::Sdist => f.write_str("sdist"),
            Self::Editable => f.write_str("editable"),
        }
    }
}
