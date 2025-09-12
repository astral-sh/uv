#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum EditableMode {
    #[default]
    Editable,
    NonEditable,
}

impl From<bool> for EditableMode {
    fn from(value: bool) -> Self {
        if value {
            Self::Editable
        } else {
            Self::NonEditable
        }
    }
}
