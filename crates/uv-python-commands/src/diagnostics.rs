use uv_errors::{Hints, collect_hint};

use crate::install::InvalidUpgradeRequestError;

/// Return this command family's hints for one error in an error chain.
///
/// Callers can combine these hints with shared workflow hints without depending on
/// command-specific error types.
pub fn error_hints(error: &(dyn std::error::Error + 'static)) -> Hints<'static> {
    let mut hints = Hints::none();
    collect_hint::<InvalidUpgradeRequestError>(error, &mut hints);
    hints
}
