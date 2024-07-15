use std::path::PathBuf;

use clap::Parser;
use tracing::info;

use uv_cache::{Cache, CacheArgs};
use uv_python::{EnvironmentPreference, PythonEnvironment, PythonRequest};

#[derive(Parser)]
pub(crate) struct CompileArgs {
    /// Compile all `.py` in this or any subdirectory to bytecode
    root: PathBuf,
    python: Option<PathBuf>,
    #[command(flatten)]
    cache_args: CacheArgs,
}

pub(crate) async fn compile(args: CompileArgs) -> anyhow::Result<()> {
    let cache = Cache::try_from(args.cache_args)?.init()?;

    let interpreter = if let Some(python) = args.python {
        python
    } else {
        let interpreter = PythonEnvironment::find(
            &PythonRequest::default(),
            EnvironmentPreference::OnlyVirtual,
            &cache,
        )?
        .into_interpreter();
        interpreter.sys_executable().to_path_buf()
    };

    let files = uv_installer::compile_tree(
        &fs_err::canonicalize(args.root)?,
        &interpreter,
        cache.root(),
    )
    .await?;
    info!("Compiled {files} files");
    Ok(())
}
