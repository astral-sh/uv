//! Generate the available preview features reference from [`uv_preview::PreviewFeature`].

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use uv_preview::PreviewFeature;

use crate::ROOT_DIR;
use crate::generate_all::Mode;

#[derive(clap::Args)]
pub(crate) struct Args {
    #[arg(long, default_value_t, value_enum)]
    pub(crate) mode: Mode,
}

pub(crate) fn main(args: &Args) -> Result<()> {
    let filename = ".preview-features.md";
    let reference_path = PathBuf::from(ROOT_DIR)
        .join("docs")
        .join("reference")
        .join(filename);
    let generated = generate();

    match args.mode {
        Mode::DryRun => anstream::println!("{generated}"),
        Mode::Check => {
            let current = fs_err::read_to_string(&reference_path).with_context(|| {
                format!(
                    "failed to read {filename}; run `cargo dev generate-preview-features-reference`"
                )
            })?;
            if current != generated {
                bail!("{filename} changed; run `cargo dev generate-preview-features-reference`");
            }
            anstream::println!("Up-to-date: {filename}");
        }
        Mode::Write => {
            fs_err::write(&reference_path, generated)
                .with_context(|| format!("failed to write {}", reference_path.display()))?;
            anstream::println!("Updating: {filename}");
        }
    }

    Ok(())
}

fn generate() -> String {
    let mut features = PreviewFeature::metadata().to_vec();
    features.sort_unstable_by_key(|(feature, _, _)| feature.to_string());

    let mut output = String::new();

    for (feature, description, _) in features {
        output.push_str("- `");
        output.push_str(&feature.to_string());
        output.push_str("`: ");

        for (index, line) in description.lines().enumerate() {
            if index > 0 {
                output.push(' ');
            }
            output.push_str(line.trim());
        }

        output.push('\n');
    }

    output
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use super::generate;

    #[test]
    fn generates_preview_feature_reference() {
        assert_snapshot!(generate(), @r"
        - `add-bounds`: Allows configuring the [default bounds for `uv add`](../reference/settings.md#add-bounds) invocations.
        - `adjust-ulimit`: On Unix, raises the process's soft open-file limit at startup, up to the hard limit.
        - `audit-command`: Allows using `uv audit`.
        - `auth-helper`: Allows using `uv auth helper` as a credential helper for external tools.
        - `azure-endpoint`: Allows signing requests to Azure Blob Storage endpoints with Azure credentials.
        - `cache-size`: Allows using `uv cache size`.
        - `centralized-project-envs`: Stores [project virtual environments](./projects/layout.md#centralized-project-environments) in the uv cache.
        - `check-command`: Allows using `uv check`.
        - `detect-module-conflicts`: Warns when multiple packages would install conflicting Python modules into the same environment.
        - `direct-publish`: Allows publishing directly to a package index.
        - `extra-build-dependencies`: Allows specifying additional dependencies for package builds.
        - `format-command`: Allows using `uv format`.
        - `gcs-endpoint`: Allows signing requests to configured Google Cloud Storage endpoints.
        - `index-exclude-newer`: Allows setting `exclude-newer` on configured package indexes.
        - `index-hash-algorithm`: Allows requiring a hash algorithm for configured package indexes.
        - `init-project-flag`: Rejects the deprecated `--project` option in `uv init`.
        - `json-output`: Allows `--output-format json` for various uv commands.
        - `lockfile-format-check`: Rejects non-canonical lockfile formatting when using `--locked` or `--check`.
        - `malware-check`: Allows `uv sync` and other commands to check for malware using [OSV](https://osv.dev) before installing packages.
        - `metadata-json`: Includes JSON metadata files in built wheels.
        - `native-auth`: Enables storage of credentials in a [system-native location](../concepts/authentication/http.md#the-uv-credentials-store).
        - `no-distutils-patch`: Stops installing the `_virtualenv.py` / `_virtualenv.pth` distutils configuration monkeypatch in virtual environments for Python 3.10 and later.
        - `package-conflicts`: Allows defining workspace conflicts at the package level.
        - `packaged-init`: Makes `uv init` create a packaged application with a `src/` layout, build system, and script entry point by default.
        - `project-directory-must-exist`: Rejects an invalid `--project` path instead of warning and continuing. Except for `uv init`, the path must already exist as a directory or point to a `pyproject.toml` file. This feature takes effect before configuration is loaded.
        - `publish-require-normalized`: Requires normalized distribution filenames when publishing, skipping files whose names are not normalized.
        - `pylock`: Allows installing from `pylock.toml` files.
        - `python-install-default`: Allows [installing `python` and `python3` executables](./python-versions.md#installing-python-executables).
        - `relocatable-envs-default`: Creates relocatable virtual environments by default.
        - `s3-endpoint`: Allows signing requests to configured S3-compatible endpoints.
        - `sbom-export`: Allows using `uv export --format=cyclonedx1.5`.
        - `special-conda-env-names`: Stops treating Conda environments named `base` or `root` as special.
        - `target-workspace-discovery`: Uses the directory containing a local `uv run` target, rather than the current working directory, as the starting point for project and workspace discovery. This feature takes effect before configuration is loaded.
        - `toml-backwards-compatibility`: Rewrites `pyproject.toml` as TOML 1.0 when building source distributions, preserving the original as `pyproject.toml.orig` to ensure compatibility with older build tools.
        - `tool-install-locks`: Stores a `uv.lock` alongside each installed tool and reuses it for reproducible installations and upgrades.
        - `venv-safe-clear`: Prevents `uv venv --clear` from clearing a directory that does not contain a `pyvenv.cfg` file unless `--force` is provided.
        - `workspace-dir`: Allows using `uv workspace dir`.
        - `workspace-list`: Allows using `uv workspace list`.
        - `workspace-list-scripts`: Allows using `uv workspace list --scripts`.
        - `workspace-metadata`: Allows using `uv workspace metadata`.
        ");
    }
}
