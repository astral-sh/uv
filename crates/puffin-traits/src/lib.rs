use gourgeist::Venv;
use pep508_rs::Requirement;
use puffin_interpreter::PythonExecutable;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

// TODO(konstin): Proper error types
pub trait PuffinCtx {
    // TODO(konstin): Add a cache abstraction
    fn cache(&self) -> Option<&Path>;
    fn python(&self) -> &PythonExecutable;

    fn resolve<'a>(
        &'a self,
        requirements: &'a [Requirement],
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<Requirement>>> + 'a>>;
    fn install<'a>(
        &'a self,
        requirements: &'a [Requirement],
        venv: &'a Venv,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + 'a>>;
    /// Returns the filename of the built wheel
    fn build_source_distribution<'a>(
        &'a self,
        sdist: &'a Path,
        wheel_dir: &'a Path,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + 'a>>;
}
