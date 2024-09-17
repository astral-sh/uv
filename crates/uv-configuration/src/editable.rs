#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum EditableMode {
    #[default]
    Editable,
    NonEditable,
}

impl EditableMode {
    /// Determine the editable mode based on the command-line arguments.
    pub fn from_args(no_editable: bool) -> Self {
        if no_editable {
            Self::NonEditable
        } else {
            Self::Editable
        }
    }
}
