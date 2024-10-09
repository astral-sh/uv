use uv_normalize::PackageName;

pub trait Diagnostic {
    /// Convert the diagnostic into a user-facing message.
    fn message(&self) -> String;

    /// Returns `true` if the [`PackageName`] is involved in this diagnostic.
    fn includes(&self, name: &PackageName) -> bool;
}
