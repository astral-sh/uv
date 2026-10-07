use uv_errors::{Hints, collect_hint};

use crate::project::ProjectError;
use crate::project::add::AddDependencyError;
use crate::project::remove::DependencyNotFoundError;
use crate::project::run::RecursionLimitError;
use crate::project::version::MissingProjectVersionError;

/// Return this command family's hints for one error in an error chain.
///
/// Callers can combine these hints with shared workflow hints without depending on
/// command-specific error types.
pub fn error_hints(error: &(dyn std::error::Error + 'static)) -> Hints<'static> {
    let mut hints = Hints::none();
    collect_hint::<AddDependencyError>(error, &mut hints);
    collect_hint::<RecursionLimitError>(error, &mut hints);
    collect_hint::<DependencyNotFoundError>(error, &mut hints);
    collect_hint::<ProjectError>(error, &mut hints);
    collect_hint::<MissingProjectVersionError>(error, &mut hints);
    hints
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;
    use uv_errors::Hints;
    use uv_lock_operations::LockError;
    use uv_settings::{LockedFlag, LockedSource};

    use super::error_hints;
    use crate::project::ProjectError;

    #[test]
    fn collects_lock_hints_through_project_errors() {
        let error =
            LockError::LockFormat("uv.lock".into(), 3, LockedSource::Cli(LockedFlag::Check));

        // Project and command context retain the lockfile's regeneration hint.
        let error =
            anyhow::Error::new(ProjectError::from(error)).context("Failed to check the lockfile");

        let mut hints = Hints::none();
        for cause in error.chain() {
            hints.extend(error_hints(cause));
        }
        assert_debug_snapshot!(hints.iter().collect::<Vec<_>>(), @r#"
        [
            "To regenerate the lockfile, run `uv lock --refresh --preview-features lockfile-format-check`.",
        ]
        "#);
    }
}
