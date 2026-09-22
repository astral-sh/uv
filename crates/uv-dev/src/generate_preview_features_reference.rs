//! Generate the available preview features reference from [`uv_preview::PreviewFeature`].

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use textwrap::indent;

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

    features
        .into_iter()
        .map(|(feature, description, _)| render(feature, description))
        .collect()
}

fn render(feature: PreviewFeature, description: &str) -> String {
    let mut output =
        format!(r##"- <a id="{feature}" href="#{feature}"><code>{feature}</code></a>: "##);

    if let Some((first, remaining)) = description.split_once('\n') {
        output.push_str(first);
        output.push('\n');
        output.push_str(&indent(remaining, "  "));
    } else {
        output.push_str(description);
    }

    output.push('\n');

    output
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use uv_preview::PreviewFeature;

    use super::{generate, render};

    #[test]
    fn preserves_multiline_preview_feature_documentation() {
        assert_snapshot!(
            render(
                PreviewFeature::Pylock,
                "A first paragraph,\ncontinued on the next line.\n\nA second paragraph:\n\n- A nested list item.\n- Another nested list item.\n\n```toml\nkey = \"value\"\n```",
            ),
            @r##"
            - <a id="pylock" href="#pylock"><code>pylock</code></a>: A first paragraph,
              continued on the next line.

              A second paragraph:

              - A nested list item.
              - Another nested list item.

              ```toml
              key = "value"
              ```
            "##
        );
    }

    #[test]
    fn generates_preview_feature_reference() {
        assert_snapshot!(generate(), @r##"
        - <a id="add-bounds" href="#add-bounds"><code>add-bounds</code></a>: Allows configuring the [default bounds for `uv add`](../reference/settings.md#add-bounds) invocations.
        - <a id="adjust-ulimit" href="#adjust-ulimit"><code>adjust-ulimit</code></a>: On Unix, raises the process's soft open-file limit at startup, up to the hard limit.
        - <a id="artifact-hash-filtering" href="#artifact-hash-filtering"><code>artifact-hash-filtering</code></a>: Restricts generated requirement hashes to artifacts allowed by binary and build policies.
        - <a id="audit-command" href="#audit-command"><code>audit-command</code></a>: Allows using `uv audit` and `uv tool audit`.
        - <a id="auth-helper" href="#auth-helper"><code>auth-helper</code></a>: Allows using `uv auth helper` as a credential helper for external tools.
        - <a id="azure-endpoint" href="#azure-endpoint"><code>azure-endpoint</code></a>: Allows signing requests to Azure Blob Storage endpoints with Azure credentials.
        - <a id="batch-export" href="#batch-export"><code>batch-export</code></a>: Allows using `uv export --batch`.
        - <a id="build-dependency-check" href="#build-dependency-check"><code>build-dependency-check</code></a>: Checks build dependencies before nonisolated builds with `uv build`.
        - <a id="build-lazy-imports" href="#build-lazy-imports"><code>build-lazy-imports</code></a>: Enables lazy imports in build backend invocations on CPython 3.15 and later.
          This can affect import-time side effects in third-party build backends.
        - <a id="cache-physical-space" href="#cache-physical-space"><code>cache-physical-space</code></a>: Reports the physical disk space reclaimed by cache cleanup, accounting for hardlinks and copy-on-write clones.
        - <a id="cache-size" href="#cache-size"><code>cache-size</code></a>: Allows using `uv cache size`.
        - <a id="centralized-project-envs" href="#centralized-project-envs"><code>centralized-project-envs</code></a>: Stores [project virtual environments](./projects/layout.md#centralized-project-environments)
          in the uv cache.
        - <a id="check-command" href="#check-command"><code>check-command</code></a>: Allows using `uv check`.
        - <a id="content-addressed-cache" href="#content-addressed-cache"><code>content-addressed-cache</code></a>: Enables content-addressed wheel archives in the cache.
        - <a id="detect-module-conflicts" href="#detect-module-conflicts"><code>detect-module-conflicts</code></a>: Warns when multiple packages would install conflicting Python modules into the same
          environment.
        - <a id="extra-build-dependencies" href="#extra-build-dependencies"><code>extra-build-dependencies</code></a>: Allows specifying additional dependencies for package builds.
        - <a id="format-command" href="#format-command"><code>format-command</code></a>: Allows using `uv format`.
        - <a id="gcs-endpoint" href="#gcs-endpoint"><code>gcs-endpoint</code></a>: Allows signing requests to configured Google Cloud Storage endpoints.
        - <a id="index-by-name" href="#index-by-name"><code>index-by-name</code></a>: Allows selecting configured package indexes by name with `--index` and `--default-index`.
        - <a id="index-exclude-newer" href="#index-exclude-newer"><code>index-exclude-newer</code></a>: Allows setting `exclude-newer` on configured package indexes.
        - <a id="index-hash-algorithm" href="#index-hash-algorithm"><code>index-hash-algorithm</code></a>: Allows requiring a hash algorithm for configured package indexes.
        - <a id="init-project-flag" href="#init-project-flag"><code>init-project-flag</code></a>: Rejects the deprecated `--project` option in `uv init`.
        - <a id="json-output" href="#json-output"><code>json-output</code></a>: Allows `--output-format json` for various uv commands.
        - <a id="lock-without-metadata" href="#lock-without-metadata"><code>lock-without-metadata</code></a>: Omit `package.metadata` from `uv.lock`, except for remote URL dependencies.
        - <a id="lockfile-format-check" href="#lockfile-format-check"><code>lockfile-format-check</code></a>: Rejects non-canonical lockfile formatting when using `--locked` or `--check`.
        - <a id="malware-check" href="#malware-check"><code>malware-check</code></a>: Allows `uv sync` and other commands to check for malware using [OSV](https://osv.dev) before
          installing packages.
        - <a id="metadata-json" href="#metadata-json"><code>metadata-json</code></a>: Includes JSON metadata files in built wheels.
        - <a id="minimum-libc-version" href="#minimum-libc-version"><code>minimum-libc-version</code></a>: Allows setting minimum libc versions for universal resolutions.
        - <a id="missing-exclude-newer-package-lock" href="#missing-exclude-newer-package-lock"><code>missing-exclude-newer-package-lock</code></a>: Exclude `exclude-newer-package` entries from the lockfile when not included in the
          project's resolved dependencies.
        - <a id="native-auth" href="#native-auth"><code>native-auth</code></a>: Enables storage of credentials in a [system-native location](../concepts/authentication/http.md#the-uv-credentials-store).
        - <a id="no-distutils-patch" href="#no-distutils-patch"><code>no-distutils-patch</code></a>: Stops installing the `_virtualenv.py` / `_virtualenv.pth` distutils configuration monkeypatch
          in virtual environments for Python 3.10 and later.
        - <a id="package-conflicts" href="#package-conflicts"><code>package-conflicts</code></a>: Allows defining workspace conflicts at the package level.
        - <a id="packaged-init" href="#packaged-init"><code>packaged-init</code></a>: Makes `uv init` create a packaged application with a `src/` layout, build system, and script
          entry point by default.
        - <a id="project-directory-must-exist" href="#project-directory-must-exist"><code>project-directory-must-exist</code></a>: Rejects an invalid `--project` path instead of warning and continuing. Except for `uv init`,
          the path must already exist as a directory or point to a `pyproject.toml` file. This feature
          takes effect before configuration is loaded.
        - <a id="publish-require-normalized" href="#publish-require-normalized"><code>publish-require-normalized</code></a>: Requires normalized distribution filenames when publishing, skipping files whose names are
          not normalized.
        - <a id="pylock" href="#pylock"><code>pylock</code></a>: Allows installing from `pylock.toml` files.
        - <a id="python-install-default" href="#python-install-default"><code>python-install-default</code></a>: Allows [installing `python` and `python3` executables](./python-versions.md#installing-python-executables).
        - <a id="relocatable-envs-default" href="#relocatable-envs-default"><code>relocatable-envs-default</code></a>: Creates relocatable virtual environments by default.
        - <a id="resolution-inputs" href="#resolution-inputs"><code>resolution-inputs</code></a>: Records runtime configuration consultations and omits unused constraints, overrides, exclusions,
          dependency metadata, and package-specific upload cutoffs from the lockfile.
        - <a id="s3-endpoint" href="#s3-endpoint"><code>s3-endpoint</code></a>: Allows signing requests to configured S3-compatible endpoints.
        - <a id="sbom-export" href="#sbom-export"><code>sbom-export</code></a>: Allows using `uv export --format=cyclonedx1.5`.
        - <a id="special-conda-env-names" href="#special-conda-env-names"><code>special-conda-env-names</code></a>: Stops treating Conda environments named `base` or `root` as special.
        - <a id="tar-codec" href="#tar-codec"><code>tar-codec</code></a>: Uses the new `tar-codec` encoding/decoding backend, instead of `astral-tokio-tar`.
        - <a id="target-workspace-discovery" href="#target-workspace-discovery"><code>target-workspace-discovery</code></a>: Uses the directory containing a local `uv run` target, rather than the current working
          directory, as the starting point for project and workspace discovery. This feature takes
          effect before configuration is loaded.
        - <a id="toml-backwards-compatibility" href="#toml-backwards-compatibility"><code>toml-backwards-compatibility</code></a>: Rewrites `pyproject.toml` as TOML 1.0 when building source distributions, preserving the
          original as `pyproject.toml.orig` to ensure compatibility with older build tools.
        - <a id="tool-install-locks" href="#tool-install-locks"><code>tool-install-locks</code></a>: Stores a `uv.lock` alongside each installed tool and reuses it for reproducible installations,
          upgrades, and audits.
        - <a id="venv-safe-clear" href="#venv-safe-clear"><code>venv-safe-clear</code></a>: Prevents `uv venv --clear` from clearing a directory that does not contain a `pyvenv.cfg` file
          unless `--force` is provided.
        - <a id="workspace-dir" href="#workspace-dir"><code>workspace-dir</code></a>: Allows using `uv workspace dir`.
        - <a id="workspace-list" href="#workspace-list"><code>workspace-list</code></a>: Allows using `uv workspace list`.
        - <a id="workspace-list-scripts" href="#workspace-list-scripts"><code>workspace-list-scripts</code></a>: Allows using `uv workspace list --scripts`.
        - <a id="workspace-metadata" href="#workspace-metadata"><code>workspace-metadata</code></a>: Allows using `uv workspace metadata`.
        "##);
    }
}
