//! Integration tests for uv Python commands.

#[path = "it/python_dir.rs"]
mod python_dir;

#[cfg(feature = "test-python")]
#[path = "it/python_find.rs"]
mod python_find;

#[cfg(feature = "test-python-managed")]
#[path = "it/python_install.rs"]
mod python_install;

#[cfg(feature = "test-python")]
#[path = "it/python_list.rs"]
mod python_list;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/python_module.rs"]
mod python_module;

#[cfg(feature = "test-python")]
#[path = "it/python_pin.rs"]
mod python_pin;

#[cfg(feature = "test-python-managed")]
#[path = "it/python_upgrade.rs"]
mod python_upgrade;

#[cfg(feature = "test-python")]
#[path = "it/venv.rs"]
mod venv;
