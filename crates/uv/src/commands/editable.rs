use std::sync::Arc;

use uv_configuration::EditableMode;
use uv_distribution_types::{DirectorySourceDist, Dist, Resolution, ResolvedDist, SourceDist};

/// If necessary, convert editable distributions to non-editable.
pub(crate) fn apply_editable_mode(
    resolution: Resolution,
    editable: Option<EditableMode>,
) -> Resolution {
    match editable {
        // No modifications are necessary for editable mode; retain any editable distributions.
        None => resolution,

        // Filter out any non-editable distributions.
        Some(EditableMode::Editable) => resolution.map(|dist| {
            let ResolvedDist::Installable { dist, version } = dist else {
                return None;
            };
            let Dist::Source(SourceDist::Directory(DirectorySourceDist {
                name,
                install_path,
                editable: None | Some(false),
                r#virtual,
                url,
            })) = dist.as_ref()
            else {
                return None;
            };

            Some(ResolvedDist::Installable {
                dist: Arc::new(Dist::Source(SourceDist::Directory(DirectorySourceDist {
                    name: name.clone(),
                    install_path: install_path.clone(),
                    editable: Some(true),
                    r#virtual: *r#virtual,
                    url: url.clone(),
                }))),
                version: version.clone(),
            })
        }),

        // If a package is editable, map it to a non-editable distribution.
        Some(EditableMode::NonEditable) => resolution.map(|dist| {
            let ResolvedDist::Installable { dist, version } = dist else {
                return None;
            };
            let Dist::Source(SourceDist::Directory(DirectorySourceDist {
                name,
                install_path,
                editable: None | Some(true),
                r#virtual,
                url,
            })) = dist.as_ref()
            else {
                return None;
            };

            Some(ResolvedDist::Installable {
                dist: Arc::new(Dist::Source(SourceDist::Directory(DirectorySourceDist {
                    name: name.clone(),
                    install_path: install_path.clone(),
                    editable: Some(false),
                    r#virtual: *r#virtual,
                    url: url.clone(),
                }))),
                version: version.clone(),
            })
        }),
    }
}
