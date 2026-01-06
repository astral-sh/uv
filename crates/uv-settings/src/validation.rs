use std::path::{Path, PathBuf};

use uv_fs::Simplified;

use crate::{GlobalOptions, Options};

pub(crate) trait Validator<Context = (), Err = ValidationError> {
    fn validate(&self, ctx: &Context) -> Result<(), Err>;
}

pub(crate) struct Context<'p> {
    pub path: &'p Path,
}

impl<'path> Validator<Context<'path>> for Options {
    /// Validate that an [`Options`] struct has correct values.
    fn validate(&self, ctx: &Context<'path>) -> Result<(), ValidationError> {
        let Self { globals, .. } = self;
        globals.validate(ctx)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    #[error(transparent)]
    GlobalOptions(#[from] GlobalOptionsError),
}

impl<'path> Validator<Context<'path>> for GlobalOptions {
    /// Validate that a [`GlobalOptions`] struct has correct values.
    fn validate(&self, ctx: &Context<'path>) -> Result<(), ValidationError> {
        let Self {
            preview,
            preview_features,
            ..
        } = self;
        if preview.is_some() && preview_features.is_some() {
            return Err(GlobalOptionsError::PreviewFeatures(ctx.path.to_path_buf()).into());
        }
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum GlobalOptionsError {
    #[error("Failed to parse: `{}`. Cannot specify both `preview` and `preview-features`.", _0.user_display())]
    PreviewFeatures(PathBuf),
}
