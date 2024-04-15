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

use distribution_types::IndexLocations;
use uv_cache::{Cache, Refresh};
use uv_client::Connectivity;
use uv_configuration::NoBinary;
use uv_configuration::{ConfigSettings, NoBuild, Reinstall, SetupPyStrategy, Upgrade};
use uv_requirements::{ExtrasSpecification, RequirementsSource};
use uv_resolver::{DependencyMode, PreReleaseMode};

use crate::cli::{CacheCommand, CacheNamespace, Cli, Commands, Maybe, PipCommand, PipNamespace};
#[cfg(feature = "self-update")]
use crate::cli::{SelfCommand, SelfNamespace};
use crate::commands::ExitStatus;
use crate::compat::CompatArgs;

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

    // Configure the `tracing` crate, which controls internal logging.
    #[cfg(feature = "tracing-durations-export")]
    let (duration_layer, _duration_guard) = logging::setup_duration()?;
    #[cfg(not(feature = "tracing-durations-export"))]
    let duration_layer = None::<tracing_subscriber::layer::Identity>;
    logging::setup_logging(
        match cli.verbose {
            0 => logging::Level::Default,
            1 => logging::Level::Verbose,
            2.. => logging::Level::ExtraVerbose,
        },
        duration_layer,
    )?;

    // Configure the `Printer`, which controls user-facing output in the CLI.
    let printer = if cli.quiet {
        printer::Printer::Quiet
    } else if cli.verbose > 0 {
        printer::Printer::Verbose
    } else {
        printer::Printer::Default
    };

    // Configure the `warn!` macros, which control user-facing warnings in the CLI.
    if !cli.quiet {
        uv_warnings::enable();
    }

    if cli.no_color {
        anstream::ColorChoice::write_global(anstream::ColorChoice::Never);
    } else {
        anstream::ColorChoice::write_global(cli.color.into());
    }

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

    let cache = Cache::try_from(cli.cache_args)?;

    match cli.command {
        Commands::Pip(PipNamespace {
            command: PipCommand::Compile(args),
        }) => {
            args.compat_args.validate()?;

            let cache = cache.with_refresh(Refresh::from_args(args.refresh, args.refresh_package));
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
            let index_urls = IndexLocations::new(
                args.index_url.and_then(Maybe::into_option),
                args.extra_index_url
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect(),
                args.find_links,
                args.no_index,
            );
            let extras = if args.all_extras {
                ExtrasSpecification::All
            } else if args.extra.is_empty() {
                ExtrasSpecification::None
            } else {
                ExtrasSpecification::Some(&args.extra)
            };
            let upgrade = Upgrade::from_args(args.upgrade, args.upgrade_package);
            let no_build = NoBuild::from_args(args.only_binary, args.no_build);
            let dependency_mode = if args.no_deps {
                DependencyMode::Direct
            } else {
                DependencyMode::Transitive
            };
            let prerelease = if args.pre {
                PreReleaseMode::Allow
            } else {
                args.prerelease
            };
            let setup_py = if args.legacy_setup_py {
                SetupPyStrategy::Setuptools
            } else {
                SetupPyStrategy::Pep517
            };
            let config_settings = args.config_setting.into_iter().collect::<ConfigSettings>();
            commands::pip_compile(
                &requirements,
                &constraints,
                &overrides,
                extras,
                args.output_file.as_deref(),
                args.resolution,
                prerelease,
                dependency_mode,
                upgrade,
                args.generate_hashes,
                args.no_emit_package,
                args.no_strip_extras,
                !args.no_annotate,
                !args.no_header,
                args.custom_compile_command,
                args.emit_index_url,
                args.emit_find_links,
                args.emit_marker_expression,
                args.emit_index_annotation,
                index_urls,
                args.index_strategy,
                args.keyring_provider,
                setup_py,
                config_settings,
                if args.offline {
                    Connectivity::Offline
                } else {
                    Connectivity::Online
                },
                args.no_build_isolation,
                no_build,
                args.python_version,
                args.exclude_newer,
                args.annotation_style,
                cli.native_tls,
                cli.quiet,
                args.link_mode,
                cache,
                printer,
            )
            .await
        }
        Commands::Pip(PipNamespace {
            command: PipCommand::Sync(args),
        }) => {
            args.compat_args.validate()?;

            let cache = cache.with_refresh(Refresh::from_args(args.refresh, args.refresh_package));
            let index_urls = IndexLocations::new(
                args.index_url.and_then(Maybe::into_option),
                args.extra_index_url
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect(),
                args.find_links,
                args.no_index,
            );
            let sources = args
                .src_file
                .into_iter()
                .map(RequirementsSource::from_requirements_file)
                .collect::<Vec<_>>();
            let reinstall = Reinstall::from_args(args.reinstall, args.reinstall_package);
            let no_binary = NoBinary::from_args(args.no_binary);
            let no_build = NoBuild::from_args(args.only_binary, args.no_build);
            let setup_py = if args.legacy_setup_py {
                SetupPyStrategy::Setuptools
            } else {
                SetupPyStrategy::Pep517
            };
            let config_settings = args.config_setting.into_iter().collect::<ConfigSettings>();

            commands::pip_sync(
                &sources,
                &reinstall,
                args.link_mode,
                args.compile,
                args.require_hashes,
                index_urls,
                args.index_strategy,
                args.keyring_provider,
                setup_py,
                if args.offline {
                    Connectivity::Offline
                } else {
                    Connectivity::Online
                },
                &config_settings,
                args.no_build_isolation,
                no_build,
                no_binary,
                args.strict,
                args.python,
                args.system,
                args.break_system_packages,
                cli.native_tls,
                cache,
                printer,
            )
            .await
        }
        Commands::Pip(PipNamespace {
            command: PipCommand::Install(args),
        }) => {
            let cache = cache.with_refresh(Refresh::from_args(args.refresh, args.refresh_package));
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
            let index_urls = IndexLocations::new(
                args.index_url.and_then(Maybe::into_option),
                args.extra_index_url
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect(),
                args.find_links,
                args.no_index,
            );
            let extras = if args.all_extras {
                ExtrasSpecification::All
            } else if args.extra.is_empty() {
                ExtrasSpecification::None
            } else {
                ExtrasSpecification::Some(&args.extra)
            };
            let reinstall = Reinstall::from_args(args.reinstall, args.reinstall_package);
            let upgrade = Upgrade::from_args(args.upgrade, args.upgrade_package);
            let no_binary = NoBinary::from_args(args.no_binary);
            let no_build = NoBuild::from_args(args.only_binary, args.no_build);
            let dependency_mode = if args.no_deps {
                DependencyMode::Direct
            } else {
                DependencyMode::Transitive
            };
            let prerelease = if args.pre {
                PreReleaseMode::Allow
            } else {
                args.prerelease
            };
            let setup_py = if args.legacy_setup_py {
                SetupPyStrategy::Setuptools
            } else {
                SetupPyStrategy::Pep517
            };
            let config_settings = args.config_setting.into_iter().collect::<ConfigSettings>();

            commands::pip_install(
                &requirements,
                &constraints,
                &overrides,
                &extras,
                args.resolution,
                prerelease,
                dependency_mode,
                upgrade,
                index_urls,
                args.index_strategy,
                args.keyring_provider,
                reinstall,
                args.link_mode,
                args.compile,
                args.require_hashes,
                setup_py,
                if args.offline {
                    Connectivity::Offline
                } else {
                    Connectivity::Online
                },
                &config_settings,
                args.no_build_isolation,
                no_build,
                no_binary,
                args.strict,
                args.exclude_newer,
                args.python,
                args.system,
                args.break_system_packages,
                cli.native_tls,
                cache,
                args.dry_run,
                printer,
            )
            .await
        }
        Commands::Pip(PipNamespace {
            command: PipCommand::Uninstall(args),
        }) => {
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
                args.python,
                args.system,
                args.break_system_packages,
                cache,
                if args.offline {
                    Connectivity::Offline
                } else {
                    Connectivity::Online
                },
                cli.native_tls,
                args.keyring_provider,
                printer,
            )
            .await
        }
        Commands::Pip(PipNamespace {
            command: PipCommand::Freeze(args),
        }) => commands::pip_freeze(
            args.exclude_editable,
            args.strict,
            args.python.as_deref(),
            args.system,
            &cache,
            printer,
        ),
        Commands::Pip(PipNamespace {
            command: PipCommand::List(args),
        }) => commands::pip_list(
            args.editable,
            args.exclude_editable,
            &args.exclude,
            &args.format,
            args.strict,
            args.python.as_deref(),
            args.system,
            &cache,
            printer,
        ),
        Commands::Pip(PipNamespace {
            command: PipCommand::Show(args),
        }) => commands::pip_show(
            args.package,
            args.strict,
            args.python.as_deref(),
            args.system,
            &cache,
            printer,
        ),
        Commands::Pip(PipNamespace {
            command: PipCommand::Check(args),
        }) => commands::pip_check(args.python.as_deref(), args.system, &cache, printer),
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

            let index_locations = IndexLocations::new(
                args.index_url.and_then(Maybe::into_option),
                args.extra_index_url
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect(),
                // No find links for the venv subcommand, to keep things simple
                Vec::new(),
                args.no_index,
            );

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
                args.python.as_deref(),
                args.link_mode,
                &index_locations,
                args.index_strategy,
                args.keyring_provider,
                uv_virtualenv::Prompt::from_args(prompt),
                args.system_site_packages,
                if args.offline {
                    Connectivity::Offline
                } else {
                    Connectivity::Online
                },
                args.seed,
                args.exclude_newer,
                cli.native_tls,
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
