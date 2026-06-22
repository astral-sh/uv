use uv_normalize::PackageName;

#[derive(Debug, Clone, PartialEq)]
pub enum EditableMode {
    Editable,
    NonEditable,
    NonEditablePackages(Vec<PackageName>),
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

impl EditableMode {
    /// Determine the editable installation strategy to use for the given arguments.
    pub fn from_args(
        editable: Option<bool>,
        no_editable_package: Vec<PackageName>,
    ) -> Option<Self> {
        match editable {
            Some(editable) => Some(Self::from(editable)),
            None if no_editable_package.is_empty() => None,
            None => Some(Self::NonEditablePackages(no_editable_package)),
        }
    }

    /// Return the editable override for a specific package, if any.
    pub fn for_package(&self, package_name: &PackageName) -> Option<bool> {
        match self {
            Self::Editable => Some(true),
            Self::NonEditable => Some(false),
            Self::NonEditablePackages(packages) if packages.contains(package_name) => Some(false),
            Self::NonEditablePackages(_) => None,
        }
    }
}
