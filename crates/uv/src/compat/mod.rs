use anyhow::{anyhow, Result};
use clap::{Args, ValueEnum};

use uv_warnings::warn_user;

pub(crate) trait CompatArgs {
    fn validate(&self) -> Result<()>;
}

/// Arguments for `pip-compile` compatibility.
///
/// These represent a subset of the `pip-compile` interface that uv supports by default.
/// For example, users often pass `--allow-unsafe`, which is unnecessary with uv. But it's a
/// nice user experience to warn, rather than fail, when users pass `--allow-unsafe`.
#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PipCompileCompatArgs {
    #[clap(long, hide = true)]
    allow_unsafe: bool,

    #[clap(long, hide = true)]
    no_allow_unsafe: bool,

    #[clap(long, hide = true)]
    reuse_hashes: bool,

    #[clap(long, hide = true)]
    no_reuse_hashes: bool,

    #[clap(long, hide = true)]
    build_isolation: bool,

    #[clap(long, hide = true)]
    no_build_isolation: bool,

    #[clap(long, hide = true)]
    resolver: Option<Resolver>,

    #[clap(long, hide = true)]
    annotation_style: Option<AnnotationStyle>,

    #[clap(long, hide = true)]
    max_rounds: Option<usize>,

    #[clap(long, hide = true)]
    cert: Option<String>,

    #[clap(long, hide = true)]
    client_cert: Option<String>,

    #[clap(long, hide = true)]
    trusted_host: Option<String>,

    #[clap(long, hide = true)]
    emit_trusted_host: bool,

    #[clap(long, hide = true)]
    no_emit_trusted_host: bool,

    #[clap(long, hide = true)]
    unsafe_package: Vec<String>,

    #[clap(long, hide = true)]
    config: Option<String>,

    #[clap(long, hide = true)]
    no_config: bool,

    #[clap(long, hide = true)]
    no_emit_index_url: bool,

    #[clap(long, hide = true)]
    no_emit_find_links: bool,

    #[clap(long, hide = true)]
    emit_options: bool,

    #[clap(long, hide = true)]
    no_emit_options: bool,

    #[clap(long, hide = true)]
    strip_extras: bool,

    #[clap(long, hide = true)]
    no_strip_extras: bool,

    #[clap(long, hide = true)]
    pip_args: Option<String>,
}

impl CompatArgs for PipCompileCompatArgs {
    /// Validate the arguments passed for `pip-compile` compatibility.
    ///
    /// This method will warn when an argument is passed that has no effect but matches uv's
    /// behavior. If an argument is passed that does _not_ match uv's behavior (e.g.,
    /// `--no-build-isolation`), this method will return an error.
    fn validate(&self) -> Result<()> {
        if self.allow_unsafe {
            warn_user!(
                "pip-compile's `--allow-unsafe` has no effect (uv can safely pin `pip` and other packages)."
            );
        }

        if self.no_allow_unsafe {
            warn_user!("pip-compile's `--no-allow-unsafe` has no effect (uv can safely pin `pip` and other packages).");
        }

        if self.reuse_hashes {
            return Err(anyhow!(
                "pip-compile's `--reuse-hashes` is unsupported (uv doesn't reuse hashes)."
            ));
        }

        if self.no_reuse_hashes {
            warn_user!(
                "pip-compile's `--no-reuse-hashes` has no effect (uv doesn't reuse hashes)."
            );
        }

        if self.build_isolation {
            warn_user!(
                "pip-compile's `--build-isolation` has no effect (uv always uses build isolation)."
            );
        }

        if self.no_build_isolation {
            return Err(anyhow!(
                "pip-compile's `--no-build-isolation` is unsupported (uv always uses build isolation)."
            ));
        }

        if let Some(resolver) = self.resolver {
            match resolver {
                Resolver::Backtracking => {
                    warn_user!(
                        "pip-compile's `--resolver=backtracking` has no effect (uv always backtracks)."
                    );
                }
                Resolver::Legacy => {
                    return Err(anyhow!(
                        "pip-compile's `--resolver=legacy` is unsupported (uv always backtracks)."
                    ));
                }
            }
        }

        if let Some(annotation_style) = self.annotation_style {
            match annotation_style {
                AnnotationStyle::Split => {
                    warn_user!(
                        "pip-compile's `--annotation-style=split` has no effect (uv always emits split annotations)."
                    );
                }
                AnnotationStyle::Line => {
                    return Err(anyhow!(
                        "pip-compile's `--annotation-style=line` is unsupported (uv always emits split annotations)."
                    ));
                }
            }
        }

        if self.max_rounds.is_some() {
            return Err(anyhow!(
                "pip-compile's `--max-rounds` is unsupported (uv always resolves until convergence)."
            ));
        }

        if self.client_cert.is_some() {
            return Err(anyhow!(
                "pip-compile's `--client-cert` is unsupported (uv doesn't support dedicated client certificates)."
            ));
        }

        if self.trusted_host.is_some() {
            return Err(anyhow!(
                "pip-compile's `--trusted-host` is unsupported (uv always requires HTTPS)."
            ));
        }

        if self.emit_trusted_host {
            return Err(anyhow!(
                "pip-compile's `--emit-trusted-host` is unsupported (uv always requires HTTPS)."
            ));
        }

        if self.no_emit_trusted_host {
            warn_user!(
                "pip-compile's `--no-emit-trusted-host` has no effect (uv never emits trusted hosts)."
            );
        }

        if !self.unsafe_package.is_empty() {
            return Err(anyhow!(
                "pip-compile's `--unsafe-package` is not supported."
            ));
        }

        if self.config.is_some() {
            return Err(anyhow!(
                "pip-compile's `--config` is unsupported (uv does not use a configuration file)."
            ));
        }

        if self.no_config {
            warn_user!(
                "pip-compile's `--no-config` has no effect (uv does not use a configuration file)."
            );
        }

        if self.no_emit_index_url {
            warn_user!(
                "pip-compile's `--no-emit-index-url` has no effect (uv excludes index URLs by default)."
            );
        }

        if self.no_emit_find_links {
            warn_user!(
                "pip-compile's `--no-emit-find-links` has no effect (uv excludes `--find-links` URLs by default)."
            );
        }

        if self.emit_options {
            return Err(anyhow!(
                "pip-compile's `--emit-options` is unsupported (uv never emits options)."
            ));
        }

        if self.no_emit_options {
            warn_user!("pip-compile's `--no-emit-options` has no effect (uv never emits options).");
        }

        if self.strip_extras {
            warn_user!("pip-compile's `--strip-extras` has no effect (uv always strips extras).");
        }

        if self.no_strip_extras {
            return Err(anyhow!(
                "pip-compile's `--no-strip-extras` is unsupported (uv always strips extras)."
            ));
        }

        if self.pip_args.is_some() {
            return Err(anyhow!(
                "pip-compile's `--pip-args` is unsupported (try passing arguments to uv directly)."
            ));
        }

        Ok(())
    }
}

/// Arguments for `pip-sync` compatibility.
///
/// These represent a subset of the `pip-sync` interface that uv supports by default.
#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PipSyncCompatArgs {
    #[clap(short, long, hide = true)]
    ask: bool,

    #[clap(long, hide = true)]
    trusted_host: Option<String>,

    #[clap(long, hide = true)]
    python_executable: Option<String>,

    #[clap(long, hide = true)]
    user: bool,

    #[clap(long, hide = true)]
    cert: Option<String>,

    #[clap(long, hide = true)]
    client_cert: Option<String>,

    #[clap(long, hide = true)]
    config: Option<String>,

    #[clap(long, hide = true)]
    no_config: bool,

    #[clap(long, hide = true)]
    pip_args: Option<String>,
}

impl CompatArgs for PipSyncCompatArgs {
    /// Validate the arguments passed for `pip-sync` compatibility.
    ///
    /// This method will warn when an argument is passed that has no effect but matches uv's
    /// behavior. If an argument is passed that does _not_ match uv's behavior, this method will
    /// return an error.
    fn validate(&self) -> Result<()> {
        if self.ask {
            return Err(anyhow!(
                "pip-sync's `--ask` is unsupported (uv never asks for confirmation)."
            ));
        }

        if self.python_executable.is_some() {
            return Err(anyhow!(
                "pip-sync's `--python-executable` is unsupported (to install into a separate Python environment, try setting `VIRTUAL_ENV` instead)."
            ));
        }

        if self.user {
            return Err(anyhow!("pip-sync's `--user` is unsupported."));
        }

        if self.client_cert.is_some() {
            return Err(anyhow!(
                "pip-sync's `--client-cert` is unsupported (uv doesn't support dedicated client certificates)."
            ));
        }

        if self.trusted_host.is_some() {
            return Err(anyhow!(
                "pip-sync's `--trusted-host` is unsupported (uv always requires HTTPS)."
            ));
        }

        if self.config.is_some() {
            return Err(anyhow!(
                "pip-sync's `--config` is unsupported (uv does not use a configuration file)."
            ));
        }

        if self.no_config {
            warn_user!(
                "pip-sync's `--no-config` has no effect (uv does not use a configuration file)."
            );
        }

        if self.pip_args.is_some() {
            return Err(anyhow!(
                "pip-sync's `--pip-args` is unsupported (try passing arguments to uv directly)."
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Copy, Clone, ValueEnum)]
enum Resolver {
    Backtracking,
    Legacy,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
enum AnnotationStyle {
    Line,
    Split,
}

/// Arguments for `venv` compatibility.
///
/// These represent a subset of the `virtualenv` interface that uv supports by default.
#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct VenvCompatArgs {
    #[clap(long, hide = true)]
    clear: bool,

    #[clap(long, hide = true)]
    no_seed: bool,

    #[clap(long, hide = true)]
    no_pip: bool,

    #[clap(long, hide = true)]
    no_setuptools: bool,

    #[clap(long, hide = true)]
    no_wheel: bool,
}

impl CompatArgs for VenvCompatArgs {
    /// Validate the arguments passed for `venv` compatibility.
    ///
    /// This method will warn when an argument is passed that has no effect but matches uv's
    /// behavior. If an argument is passed that does _not_ match uv's behavior, this method will
    /// return an error.
    fn validate(&self) -> Result<()> {
        if self.clear {
            warn_user!(
                "virtualenv's `--clear` has no effect (uv always clears the virtual environment)."
            );
        }

        if self.no_seed {
            warn_user!(
                "virtualenv's `--no-seed` has no effect (uv omits seed packages by default)."
            );
        }

        if self.no_pip {
            warn_user!("virtualenv's `--no-pip` has no effect (uv omits `pip` by default).");
        }

        if self.no_setuptools {
            warn_user!(
                "virtualenv's `--no-setuptools` has no effect (uv omits `setuptools` by default)."
            );
        }

        if self.no_wheel {
            warn_user!("virtualenv's `--no-wheel` has no effect (uv omits `wheel` by default).");
        }

        Ok(())
    }
}
