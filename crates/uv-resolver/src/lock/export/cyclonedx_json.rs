use crate::Installable;
use crate::LockError;
use crate::lock::export::ExportableRequirements;
use cyclonedx_bom::prelude::Bom;
use uv_configuration::{
    DependencyGroupsWithDefaults, EditableMode, ExtrasSpecificationWithDefaults, InstallOptions,
};
use uv_normalize::PackageName;

pub fn from_lock<'lock>(
    target: &impl Installable<'lock>,
    prune: &[PackageName],
    extras: &ExtrasSpecificationWithDefaults,
    groups: &DependencyGroupsWithDefaults,
    annotate: bool,
    #[allow(unused_variables)] editable: Option<EditableMode>,
    install_options: &'lock InstallOptions,
) -> Result<Bom, LockError> {
    // Extract the packages from the lock file.
    let ExportableRequirements(_nodes) =
        ExportableRequirements::from_lock(target, prune, extras, groups, annotate, install_options);

    let bom = Bom { ..Bom::default() };

    Ok(bom)
}
