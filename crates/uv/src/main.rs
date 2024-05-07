use std::env;
use std::io::stdout;
use std::path::PathBuf;
use std::process::ExitCode;

use anstream::eprintln;
use anyhow::Result;
use clap::error::{ContextKind, ContextValue};
use clap::{CommandFactory, Parser};
use owo_colors::OwoColorize;
use tracing::instrument;

use uv_cache::Cache;

use uv_requirements::RequirementsSource;

use crate::cli::{CacheCommand, CacheNamespace, Cli, Commands, PipCommand, PipNamespace};
#[cfg(feature = "self-update")]
use crate::cli::{SelfCommand, SelfNamespace};
use crate::commands::ExitStatus;
use crate::compat::CompatArgs;
use crate::settings::{
    CacheSettings, GlobalSettings, PipCheckSettings, PipCompileSettings, PipFreezeSettings,
    PipInstallSettings, PipListSettings, PipShowSettings, PipSyncSettings, PipUninstallSettings,
};

#[cfg(target_os = "windows")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "openbsd"),
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64"
    )
))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod cli;
mod commands;
mod compat;
mod logging;
mod printer;
mod settings;
mod shell;
mod version;

#[instrument]
async fn run() -> Result<ExitStatus> {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(mut err) => {
            if let Some(ContextValue::String(subcommand)) = err.get(ContextKind::InvalidSubcommand)
            {
                match subcommand.as_str() {
                    "compile" | "lock" => {
                        err.insert(
                            ContextKind::SuggestedSubcommand,
                            ContextValue::String("uv pip compile".to_string()),
                        );
                    }
                    "sync" => {
                        err.insert(
                            ContextKind::SuggestedSubcommand,
                            ContextValue::String("uv pip sync".to_string()),
                        );
                    }
                    "install" | "add" => {
                        err.insert(
                            ContextKind::SuggestedSubcommand,
                            ContextValue::String("uv pip install".to_string()),
                        );
                    }
                    "uninstall" | "remove" => {
                        err.insert(
                            ContextKind::SuggestedSubcommand,
                            ContextValue::String("uv pip uninstall".to_string()),
                        );
                    }
                    "freeze" => {
                        err.insert(
                            ContextKind::SuggestedSubcommand,
                            ContextValue::String("uv pip freeze".to_string()),
                        );
                    }
                    "list" => {
                        err.insert(
                            ContextKind::SuggestedSubcommand,
                            ContextValue::String("uv pip list".to_string()),
                        );
                    }
                    "show" => {
                        err.insert(
                            ContextKind::SuggestedSubcommand,
                            ContextValue::String("uv pip show".to_string()),
                        );
                    }
                    _ => {}
                }
            }
            err.exit()
        }
    };

    // Load the workspace settings, prioritizing (in order):
    // 1. The configuration file specified on the command-line.
    // 2. The configuration file in the current directory.
    // 3. The user configuration file.
    let workspace = if let Some(config_file) = cli.config_file.as_ref() {
        Some(uv_workspace::Workspace::from_file(config_file)?)
    } else if cli.isolated {
        None
    } else if let Some(workspace) = uv_workspace::Workspace::find(env::current_dir()?)? {
        Some(workspace)
    } else {
        uv_workspace::Workspace::user()?
    };

    // Resolve the global settings.
    let globals = GlobalSettings::resolve(cli.global_args, workspace.as_ref());

    // Configure the `tracing` crate, which controls internal logging.
    #[cfg(feature = "tracing-durations-export")]
    let (duration_layer, _duration_guard) = logging::setup_duration()?;
    #[cfg(not(feature = "tracing-durations-export"))]
    let duration_layer = None::<tracing_subscriber::layer::Identity>;
    logging::setup_logging(
        match globals.verbose {
            0 => logging::Level::Default,
            1 => logging::Level::Verbose,
            2.. => logging::Level::ExtraVerbose,
        },
        duration_layer,
    )?;

    // Configure the `Printer`, which controls user-facing output in the CLI.
    let printer = if globals.quiet {
        printer::Printer::Quiet
    } else if globals.verbose > 0 {
        printer::Printer::Verbose
    } else {
        printer::Printer::Default
    };

    // Configure the `warn!` macros, which control user-facing warnings in the CLI.
    if !globals.quiet {
        uv_warnings::enable();
    }

    anstream::ColorChoice::write_global(globals.color.into());

    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .break_words(false)
                .word_separator(textwrap::WordSeparator::AsciiSpace)
                .word_splitter(textwrap::WordSplitter::NoHyphenation)
                .wrap_lines(env::var("UV_NO_WRAP").map(|_| false).unwrap_or(true))
                .build(),
        )
    }))?;

    // Resolve the cache settings.
    let cache = CacheSettings::resolve(cli.cache_args, workspace.as_ref());
    let cache = Cache::from_settings(cache.no_cache, cache.cache_dir)?;

    match cli.command {
        Commands::Pip(PipNamespace {
            command: PipCommand::Compile(args),
        }) => {
            args.compat_args.validate()?;

            // Resolve the settings from the command-line arguments and workspace configuration.
            let args = PipCompileSettings::resolve(args, workspace);

            let cache = cache.with_refresh(args.refresh);
            let requirements = args
                .src_file
                .into_iter()
                .map(RequirementsSource::from_requirements_file)
                .collect::<Vec<_>>();
            let constraints = args
                .constraint
                .into_iter()
                .map(RequirementsSource::from_constraints_txt)
                .collect::<Vec<_>>();
            let overrides = args
                .r#override
                .into_iter()
                .map(RequirementsSource::from_overrides_txt)
                .collect::<Vec<_>>();

            commands::pip_compile(
                &requirements,
                &constraints,
                &overrides,
                args.shared.extras,
                args.shared.output_file.as_deref(),
                args.shared.resolution,
                args.shared.prerelease,
                args.shared.dependency_mode,
                args.upgrade,
                args.shared.generate_hashes,
                args.shared.no_emit_package,
                args.shared.no_strip_extras,
                !args.shared.no_annotate,
                !args.shared.no_header,
                args.shared.custom_compile_command,
                args.shared.emit_index_url,
                args.shared.emit_find_links,
                args.shared.emit_marker_expression,
                args.shared.emit_index_annotation,
                args.shared.index_locations,
                args.shared.index_strategy,
                args.shared.keyring_provider,
                args.shared.setup_py,
                args.shared.config_setting,
                args.shared.connectivity,
                args.shared.no_build_isolation,
                args.shared.no_build,
                args.shared.python_version,
                args.shared.python_platform,
                args.shared.exclude_newer,
                args.shared.annotation_style,
                args.shared.link_mode,
                args.shared.python,
                args.shared.system,
                args.uv_lock,
                globals.native_tls,
                globals.quiet,
                globals.preview,
                cache,
                printer,
            )
            .await
        }
        Commands::Pip(PipNamespace {
            command: PipCommand::Sync(args),
        }) => {
            args.compat_args.validate()?;

            // Resolve the settings from the command-line arguments and workspace configuration.
            let args = PipSyncSettings::resolve(args, workspace);

            let cache = cache.with_refresh(args.refresh);
            let sources = args
                .src_file
                .into_iter()
                .map(RequirementsSource::from_requirements_file)
                .collect::<Vec<_>>();

            commands::pip_sync(
                &sources,
                &args.reinstall,
                args.shared.link_mode,
                args.shared.compile_bytecode,
                args.shared.require_hashes,
                args.shared.index_locations,
                args.shared.index_strategy,
                args.shared.keyring_provider,
                args.shared.setup_py,
                args.shared.connectivity,
                &args.shared.config_setting,
                args.shared.no_build_isolation,
                args.shared.no_build,
                args.shared.no_binary,
                args.shared.python_version,
                args.shared.python_platform,
                args.shared.strict,
                args.shared.python,
                args.shared.system,
                args.shared.break_system_packages,
                args.shared.target,
                globals.native_tls,
                globals.preview,
                cache,
                printer,
            )
            .await
        }
        Commands::Pip(PipNamespace {
            command: PipCommand::Install(args),
        }) => {
            args.compat_args.validate()?;
            // Resolve the settings from the command-line arguments and workspace configuration.
            let args = PipInstallSettings::resolve(args, workspace);

            let cache = cache.with_refresh(args.refresh);
            let requirements = args
                .package
                .into_iter()
                .map(RequirementsSource::from_package)
                .chain(args.editable.into_iter().map(RequirementsSource::Editable))
                .chain(
                    args.requirement
                        .into_iter()
                        .map(RequirementsSource::from_requirements_file),
                )
                .collect::<Vec<_>>();
            let constraints = args
                .constraint
                .into_iter()
                .map(RequirementsSource::from_constraints_txt)
                .collect::<Vec<_>>();
            let overrides = args
                .r#override
                .into_iter()
                .map(RequirementsSource::from_overrides_txt)
                .collect::<Vec<_>>();

            commands::pip_install(
                &requirements,
                &constraints,
                &overrides,
                &args.shared.extras,
                args.shared.resolution,
                args.shared.prerelease,
                args.shared.dependency_mode,
                args.upgrade,
                args.shared.index_locations,
                args.shared.index_strategy,
                args.shared.keyring_provider,
                args.reinstall,
                args.shared.link_mode,
                args.shared.compile_bytecode,
                args.shared.require_hashes,
                args.shared.setup_py,
                args.shared.connectivity,
                &args.shared.config_setting,
                args.shared.no_build_isolation,
                args.shared.no_build,
                args.shared.no_binary,
                args.shared.python_version,
                args.shared.python_platform,
                args.shared.strict,
                args.shared.exclude_newer,
                args.shared.python,
                args.shared.system,
                args.shared.break_system_packages,
                args.shared.target,
                args.uv_lock,
                globals.native_tls,
                globals.preview,
                cache,
                args.dry_run,
                printer,
            )
            .await
        }
        Commands::Pip(PipNamespace {
            command: PipCommand::Uninstall(args),
        }) => {
            // Resolve the settings from the command-line arguments and workspace configuration.
            let args = PipUninstallSettings::resolve(args, workspace);

            let sources = args
                .package
                .into_iter()
                .map(RequirementsSource::from_package)
                .chain(
                    args.requirement
                        .into_iter()
                        .map(RequirementsSource::from_requirements_txt),
                )
                .collect::<Vec<_>>();
            commands::pip_uninstall(
                &sources,
                args.shared.python,
                args.shared.system,
                args.shared.break_system_packages,
                args.shared.target,
                cache,
                args.shared.connectivity,
                globals.native_tls,
                globals.preview,
                args.shared.keyring_provider,
                printer,
            )
            .await
        }
        Commands::Pip(PipNamespace {
            command: PipCommand::Freeze(args),
        }) => {
            // Resolve the settings from the command-line arguments and workspace configuration.
            let args = PipFreezeSettings::resolve(args, workspace);

            commands::pip_freeze(
                args.exclude_editable,
                args.shared.strict,
                args.shared.python.as_deref(),
                args.shared.system,
                &cache,
                printer,
            )
        }
        Commands::Pip(PipNamespace {
            command: PipCommand::List(args),
        }) => {
            args.compat_args.validate()?;

            // Resolve the settings from the command-line arguments and workspace configuration.
            let args = PipListSettings::resolve(args, workspace);

            commands::pip_list(
                args.editable,
                args.exclude_editable,
                &args.exclude,
                &args.format,
                args.shared.strict,
                args.shared.python.as_deref(),
                args.shared.system,
                &cache,
                printer,
            )
        }
        Commands::Pip(PipNamespace {
            command: PipCommand::Show(args),
        }) => {
            // Resolve the settings from the command-line arguments and workspace configuration.
            let args = PipShowSettings::resolve(args, workspace);

            commands::pip_show(
                args.package,
                args.shared.strict,
                args.shared.python.as_deref(),
                args.shared.system,
                &cache,
                printer,
            )
        }
        Commands::Pip(PipNamespace {
            command: PipCommand::Check(args),
        }) => {
            // Resolve the settings from the command-line arguments and workspace configuration.
            let args = PipCheckSettings::resolve(args, workspace);

            commands::pip_check(
                args.shared.python.as_deref(),
                args.shared.system,
                &cache,
                printer,
            )
        }
        Commands::Cache(CacheNamespace {
            command: CacheCommand::Clean(args),
        })
        | Commands::Clean(args) => commands::cache_clean(&args.package, &cache, printer),
        Commands::Cache(CacheNamespace {
            command: CacheCommand::Prune,
        }) => commands::cache_prune(&cache, printer),
        Commands::Cache(CacheNamespace {
            command: CacheCommand::Dir,
        }) => {
            commands::cache_dir(&cache);
            Ok(ExitStatus::Success)
        }
        Commands::Venv(args) => {
            args.compat_args.validate()?;

            // Resolve the settings from the command-line arguments and workspace configuration.
            let args = settings::VenvSettings::resolve(args, workspace);

            // Since we use ".venv" as the default name, we use "." as the default prompt.
            let prompt = args.prompt.or_else(|| {
                if args.name == PathBuf::from(".venv") {
                    Some(".".to_string())
                } else {
                    None
                }
            });

            commands::venv(
                &args.name,
                args.shared.python.as_deref(),
                args.shared.link_mode,
                &args.shared.index_locations,
                args.shared.index_strategy,
                args.shared.keyring_provider,
                uv_virtualenv::Prompt::from_args(prompt),
                args.system_site_packages,
                args.shared.connectivity,
                args.seed,
                args.allow_existing,
                args.shared.exclude_newer,
                globals.native_tls,
                &cache,
                printer,
            )
            .await
        }
        Commands::Run(args) => {
            // Resolve the settings from the command-line arguments and workspace configuration.
            let args = settings::RunSettings::resolve(args, workspace);

            let requirements = args
                .with
                .into_iter()
                .map(RequirementsSource::from_package)
                // TODO(zanieb): Consider editable package support. What benefit do these have in an ephemeral
                //               environment?
                // .chain(
                //     args.with_editable
                //         .into_iter()
                //         .map(RequirementsSource::Editable),
                // )
                // TODO(zanieb): Consider requirements file support, this comes with additional complexity due to
                //               to the extensive configuration allowed in requirements files
                // .chain(
                //     args.with_requirements
                //         .into_iter()
                //         .map(RequirementsSource::from_requirements_file),
                // )
                .collect::<Vec<_>>();

            commands::run(
                args.target,
                args.args,
                requirements,
                args.python,
                cli.isolated,
                globals.preview,
                &cache,
                printer,
            )
            .await
        }
        #[cfg(feature = "self-update")]
        Commands::Self_(SelfNamespace {
            command: SelfCommand::Update,
        }) => commands::self_update(printer).await,
        Commands::Version { output_format } => {
            commands::version(output_format, &mut stdout())?;
            Ok(ExitStatus::Success)
        }
        Commands::GenerateShellCompletion { shell } => {
            shell.generate(&mut Cli::command(), &mut stdout());
            Ok(ExitStatus::Success)
        }
    }
}

fn main() -> ExitCode {
    let result = if let Ok(stack_size) = env::var("UV_STACK_SIZE") {
        // Artificially limit the stack size to test for stack overflows. Windows has a default stack size of 1MB,
        // which is lower than the linux and mac default.
        // https://learn.microsoft.com/en-us/cpp/build/reference/stack-stack-allocations?view=msvc-170
        let stack_size = stack_size.parse().expect("Invalid stack size");
        let tokio_main = move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_stack_size(stack_size)
                .build()
                .expect("Failed building the Runtime")
                .block_on(run())
        };
        std::thread::Builder::new()
            .stack_size(stack_size)
            .spawn(tokio_main)
            .expect("Tokio executor failed, was there a panic?")
            .join()
            .expect("Tokio executor failed, was there a panic?")
    } else {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed building the Runtime")
            .block_on(run())
    };

    match result {
        Ok(code) => code.into(),
        Err(err) => {
            let mut causes = err.chain();
            eprintln!("{}: {}", "error".red().bold(), causes.next().unwrap());
            for err in causes {
                eprintln!("  {}: {}", "Caused by".red().bold(), err);
            }
            ExitStatus::Error.into()
        }
    }
}
