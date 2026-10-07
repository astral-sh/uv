use uv_errors::{Hints, collect_hint};

use crate::common::NoExecutablesError;
use crate::run::ToolRunScriptError;
use crate::run::ToolRunUsageError;

/// Return this command family's hints for one error in an error chain.
///
/// Callers can combine these hints with shared workflow hints without depending on
/// command-specific error types.
pub fn error_hints(error: &(dyn std::error::Error + 'static)) -> Hints<'static> {
    let mut hints = Hints::none();
    collect_hint::<ToolRunUsageError>(error, &mut hints);
    collect_hint::<ToolRunScriptError>(error, &mut hints);
    collect_hint::<NoExecutablesError>(error, &mut hints);
    hints
}
