use std::path::PathBuf;

use clap::Parser;
use tracing::info;

use uv_cache::{Cache, CacheArgs};
use uv_configuration::Concurrency;
use uv_python::{
    EnvironmentPreference, Interpreter, PythonEnvironment, PythonPreference, PythonRequest,
};

#[derive(Parser)]
pub(crate) struct CompileArgs {
    /// Compile all `.py` in this or any subdirectory to bytecode
    root: PathBuf,
    python: Option<PathBuf>,
    #[command(flatten)]
    cache_args: CacheArgs,
}

pub(crate) async fn compile(args: CompileArgs) -> anyhow::Result<()> {
    let cache = Cache::try_from(args.cache_args)?.init().await?;

    let interpreter = if let Some(python) = args.python {
        Interpreter::query(python, &cache)?
    } else {
        PythonEnvironment::find(
            &PythonRequest::default(),
            EnvironmentPreference::OnlyVirtual,
            PythonPreference::default(),
            None,
            &cache,
        )?
        .into_interpreter()
    };

    let files = uv_installer::compile_tree(
        &fs_err::canonicalize(args.root)?,
        &interpreter,
        &Concurrency::default(),
        cache.root(),
    )
    .await?;
    info!("Compiled {files} files");
    Ok(())
}
