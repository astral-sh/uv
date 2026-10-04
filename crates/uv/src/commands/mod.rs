pub(crate) use auth::dir::dir as auth_dir;
pub(crate) use auth::helper::helper as auth_helper;
pub(crate) use auth::login::login as auth_login;
pub(crate) use auth::logout::logout as auth_logout;
pub(crate) use auth::token::token as auth_token;
pub(crate) use build_frontend::build_frontend;
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
pub(crate) use project::ProjectError;
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
pub(crate) use publish::publish;
pub(crate) use python::dir::dir as python_dir;
pub(crate) use python::find::find as python_find;
pub(crate) use python::find::find_script as python_find_script;
pub(crate) use python::install::install as python_install;
pub(crate) use python::install::{PythonUpgrade, PythonUpgradeSource};
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
pub(crate) use tool::run::ToolRunCommand;
pub(crate) use tool::run::run as tool_run;
pub(crate) use tool::uninstall::uninstall as tool_uninstall;
pub(crate) use tool::update_shell::update_shell as tool_update_shell;
pub(crate) use tool::upgrade::upgrade as tool_upgrade;
pub(crate) use uv_console::human_readable_bytes;
pub(crate) use venv::venv;
pub(crate) use version::self_version;
pub(crate) use workspace::dir::dir;
pub(crate) use workspace::list::list;
pub(crate) use workspace::metadata::metadata;

mod auth;
pub(crate) mod build_backend;
mod build_frontend;
mod cache_clean;
mod cache_dir;
mod cache_prune;
mod cache_size;
pub(crate) mod diagnostics;
mod help;
pub(crate) use uv_pip_commands as pip;
pub(crate) use uv_project_commands::project;
mod publish;
pub(crate) use uv_python_commands as python;
pub(crate) mod reporters;
#[cfg(feature = "self-update")]
mod self_update;
pub(crate) use uv_tool_commands as tool;
mod venv;
mod version;
pub(crate) use uv_workspace_commands as workspace;

pub use uv_command_support::ExitStatus;
pub(crate) use uv_command_support::UvError;
pub(crate) use uv_project_commands::ScriptPath;

#[cfg(test)]
mod error_tests {
    use std::io::{Error, ErrorKind};

    use anyhow::bail;
    use insta::{allow_duplicates, assert_snapshot};

    use uv_lock_operations::LockError;
    use uv_settings::{LockedFlag, LockedSource};

    use super::{UvError, project};

    #[test]
    fn contextual_operations_keep_their_classification_and_cause() -> anyhow::Result<()> {
        let conversions: [fn(uv_resolve_operations::Error) -> UvError; 6] = [
            UvError::from,
            |error| UvError::from(uv_environment_operations::OperationsError::from(error)),
            |error| UvError::from(uv_environment_operations::EnvironmentError::from(error)),
            |error| UvError::from(LockError::from(error)),
            |error| UvError::from(project::ProjectError::from(LockError::from(error))),
            |error| {
                UvError::from(project::ProjectError::from(
                    uv_environment_operations::EnvironmentError::from(error),
                ))
            },
        ];
        for convert in conversions {
            for (kind, user_failure) in [
                (ErrorKind::NotFound, true),
                (ErrorKind::PermissionDenied, false),
            ] {
                let error = uv_resolve_operations::Error::Requirements(uv_requirements::Error::Io(
                    Error::new(kind, "requirements failure"),
                ));
                let error = convert(
                    error
                        .with_resolution_context("script")
                        .with_resolution_context("tool"),
                );
                let ((UvError::User(error), true) | (UvError::Unexpected(error), false)) =
                    (error, user_failure)
                else {
                    bail!("operation classification changed with context");
                };
                allow_duplicates! {
                    assert_snapshot!(format!("{error:#}"), @"Failed to resolve tool requirement: requirements failure");
                }
                assert!(
                    error
                        .chain()
                        .any(<dyn std::error::Error>::is::<uv_requirements::Error>)
                );
            }
        }
        Ok(())
    }

    #[test]
    fn resolution_context_leaves_other_errors_unchanged() -> anyhow::Result<()> {
        let error = uv_resolve_operations::Error::Io(Error::new(
            ErrorKind::PermissionDenied,
            "cache write failed",
        ));
        let UvError::Unexpected(error) = UvError::from(error.with_resolution_context("tool"))
        else {
            bail!("operation classification changed with context");
        };
        assert_snapshot!(format!("{error:#}"), @"cache write failed");
        assert!(
            error
                .downcast_ref::<uv_resolve_operations::Error>()
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn project_errors_use_shared_operation_classification() -> anyhow::Result<()> {
        let error = uv_environment_operations::EnvironmentError::Requirements(
            uv_requirements::Error::Io(Error::new(ErrorKind::NotFound, "requirements failure")),
        );
        assert!(matches!(
            UvError::from(project::ProjectError::from(error)),
            UvError::User(_)
        ));

        let conversions: [fn(LockError) -> UvError; 2] = [UvError::from, |error| {
            UvError::from(project::ProjectError::from(error))
        }];
        for convert in conversions {
            let error =
                LockError::LockFormat("uv.lock".into(), 3, LockedSource::Cli(LockedFlag::Check));
            let UvError::User(error) = convert(error) else {
                bail!("lock policy errors must be classified as user failures");
            };
            allow_duplicates! {
                assert_snapshot!(format!("{error:#}"), @"The lockfile at `uv.lock` has non-canonical formatting at line 3, but `--check` was provided.");
            }
            assert!(error.downcast_ref::<LockError>().is_some());
        }
        Ok(())
    }
}
