use std::sync::Arc;

use uv_configuration::EditableMode;
use uv_distribution_types::{DirectorySourceDist, Dist, Resolution, ResolvedDist, SourceDist};

/// Apply editable installation overrides to local directory distributions.
pub(crate) fn apply_editable_mode(
    resolution: Resolution,
    editable: Option<EditableMode>,
) -> Resolution {
    let Some(editable) = editable else {
        return resolution;
    };

    resolution.map(|dist| {
        let ResolvedDist::Installable { dist, version } = dist else {
            return None;
        };
        let Dist::Source(SourceDist::Directory(DirectorySourceDist {
            name,
            source,
            editable: current_editable,
            r#virtual,
        })) = dist.as_ref()
        else {
            return None;
        };

        let editable = editable.for_package(name)?;
        if *current_editable == Some(editable) {
            return None;
        }

        Some(ResolvedDist::Installable {
            dist: Arc::new(Dist::Source(SourceDist::Directory(DirectorySourceDist {
                name: name.clone(),
                source: source.clone(),
                editable: Some(editable),
                r#virtual: *r#virtual,
            }))),
            version: version.clone(),
        })
    })
}
