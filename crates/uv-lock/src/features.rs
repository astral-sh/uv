use uv_preview::{Preview, PreviewFeature};

bitflags::bitflags! {
    /// Features enabled when serializing a lockfile.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct LockFeatures: u8 {
        /// Write name-only dependencies as strings instead of inline tables.
        const DEPENDENCY_SHORTHAND = 1 << 0;
    }
}

impl From<Preview> for LockFeatures {
    fn from(preview: Preview) -> Self {
        let mut features = Self::empty();
        features.set(
            Self::DEPENDENCY_SHORTHAND,
            preview.is_enabled(PreviewFeature::LockDependencyShorthand),
        );
        features
    }
}
