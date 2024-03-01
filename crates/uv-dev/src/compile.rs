use std::path::PathBuf;

use clap::Parser;
use platform_host::Platform;
use tracing::info;
use uv_cache::{Cache, CacheArgs};
use uv_interpreter::PythonEnvironment;

#[derive(Parser)]
pub(crate) struct CompileArgs {
    /// Compile all `.py` in this or any subdirectory to bytecode
    root: PathBuf,
    python: Option<PathBuf>,
    #[command(flatten)]
    cache_args: CacheArgs,
}

pub(crate) async fn compile(args: CompileArgs) -> anyhow::Result<()> {
    let interpreter = if let Some(python) = args.python {
        python
    } else {
        let cache = Cache::try_from(args.cache_args)?;
        let platform = Platform::current()?;
        let venv = PythonEnvironment::from_virtualenv(platform, &cache)?;
        venv.python_executable().to_path_buf()
    };

    let files = uv_installer::compile_tree(&fs_err::canonicalize(args.root)?, &interpreter).await?;
    info!("Compiled {files} files");
    Ok(())
}
