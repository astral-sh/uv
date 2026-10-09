pub(crate) use auth::dir::dir as auth_dir;
pub(crate) use auth::helper::helper as auth_helper;
pub(crate) use auth::login::login as auth_login;
pub(crate) use auth::logout::logout as auth_logout;
pub(crate) use auth::token::token as auth_token;
pub(crate) use cache_clean::cache_clean;
pub(crate) use cache_dir::cache_dir;
pub(crate) use cache_prune::cache_prune;
pub(crate) use cache_size::cache_size;
pub(crate) use help::help;
pub(crate) use pip::check::pip_check;
pub(crate) use pip::compile::pip_compile;
pub(crate) use pip::freeze::pip_freeze;
pub(crate) use pip::install::pip_install;
pub(crate) use pip::list::pip_list;
pub(crate) use pip::show::pip_show;
pub(crate) use pip::sync::pip_sync;
pub(crate) use pip::tree::pip_tree;
pub(crate) use pip::uninstall::pip_uninstall;
pub(crate) use project::add::add;
pub(crate) use project::audit::audit;
pub(crate) use project::check::check;
pub(crate) use project::export::export;
pub(crate) use project::format::format;
pub(crate) use project::init::init;
pub(crate) use project::lock::lock;
pub(crate) use project::remove::remove;
pub(crate) use project::run::{ParsedRunCommand, RunCommand, run};
pub(crate) use project::sync::sync;
pub(crate) use project::tree::tree;
pub(crate) use project::upgrade::upgrade;
pub(crate) use project::version::project_version;
pub(crate) use python::dir::dir as python_dir;
pub(crate) use python::find::find as python_find;
pub(crate) use python::find::find_script as python_find_script;
pub(crate) use python::install::install as python_install;
pub(crate) use python::list::list as python_list;
pub(crate) use python::pin::pin as python_pin;
pub(crate) use python::uninstall::uninstall as python_uninstall;
pub(crate) use python::update_shell::update_shell as python_update_shell;
#[cfg(feature = "self-update")]
pub(crate) use self_update::self_update;
pub(crate) use tool::audit::audit as tool_audit;
pub(crate) use tool::dir::dir as tool_dir;
pub(crate) use tool::install::install as tool_install;
pub(crate) use tool::list::list as tool_list;
pub(crate) use tool::run::run as tool_run;
pub(crate) use tool::uninstall::uninstall as tool_uninstall;
pub(crate) use tool::update_shell::update_shell as tool_update_shell;
pub(crate) use tool::upgrade::upgrade as tool_upgrade;
pub(crate) use uv_build_commands::build_frontend;
pub(crate) use uv_console::human_readable_bytes;
pub(crate) use uv_publish_commands::publish;
pub(crate) use venv::venv;
pub(crate) use version::self_version;
pub(crate) use workspace::dir::dir;
pub(crate) use workspace::list::list;
pub(crate) use workspace::metadata::metadata;

mod auth;
pub(crate) mod build_backend;
mod cache_clean;
mod cache_dir;
mod cache_prune;
mod cache_size;
pub(crate) mod diagnostics;
mod help;
pub(crate) use uv_pip_commands as pip;
pub(crate) use uv_project_commands as project;
pub(crate) use uv_python_commands as python;
pub(crate) mod reporters;
#[cfg(feature = "self-update")]
mod self_update;
pub(crate) use uv_tool_commands as tool;
mod venv;
mod version;
pub(crate) use uv_workspace_commands as workspace;

pub(crate) use uv_project_commands::ScriptPath;

#[cfg(test)]
mod error_tests {
    use std::io::{Error, ErrorKind};

    use anyhow::bail;
    use insta::assert_snapshot;

    use uv_command_support::UvError;
    use uv_environment_operations::EnvironmentError;
    use uv_project_commands::ProjectError;
    use uv_resolve_operations::Error as ResolveError;

    #[test]
    fn resolution_context_missing_requirements() -> anyhow::Result<()> {
        let error = ResolveError::Requirements(uv_requirements::Error::Io(Error::new(
            ErrorKind::NotFound,
            "requirements failure",
        )));

        // Tool requirements replace the script context.
        let error = error
            .with_resolution_context("script")
            .with_resolution_context("tool");

        let UvError::User(error) = UvError::from(error) else {
            bail!("expected a user error");
        };
        assert_snapshot!(format!("{error:#}"), @"Failed to resolve tool requirement: requirements failure");

        Ok(())
    }

    #[test]
    fn resolution_context_unreadable_requirements() -> anyhow::Result<()> {
        let error = ResolveError::Requirements(uv_requirements::Error::Io(Error::new(
            ErrorKind::PermissionDenied,
            "requirements failure",
        )));

        // Tool requirements replace the script context.
        let error = error
            .with_resolution_context("script")
            .with_resolution_context("tool");

        let UvError::Unexpected(error) = UvError::from(error) else {
            bail!("expected an unexpected error");
        };
        assert_snapshot!(format!("{error:#}"), @"Failed to resolve tool requirement: requirements failure");

        Ok(())
    }

    #[test]
    fn resolution_context_leaves_other_errors_unchanged() -> anyhow::Result<()> {
        let error = ResolveError::Io(Error::new(
            ErrorKind::PermissionDenied,
            "cache write failed",
        ));

        // Resolution context only applies to requirements and solver errors.
        let error = error.with_resolution_context("tool");

        let UvError::Unexpected(error) = UvError::from(error) else {
            bail!("expected an unexpected error");
        };
        assert_snapshot!(format!("{error:#}"), @"cache write failed");

        Ok(())
    }

    #[test]
    fn project_requirements_use_operation_classification() -> anyhow::Result<()> {
        let error = EnvironmentError::Requirements(uv_requirements::Error::Io(Error::new(
            ErrorKind::NotFound,
            "requirements failure",
        )));

        // A project wrapper retains the requirements error's classification.
        let UvError::User(_) = UvError::from(ProjectError::from(error)) else {
            bail!("expected a user error");
        };

        Ok(())
    }
}
