//! Generate the available preview features reference from [`uv_preview::PreviewFeature`].

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use itertools::Itertools;

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
                    "failed to read `{filename}`; run `cargo dev generate-preview-features-reference`"
                )
            })?;
            if current != generated {
                bail!("`{filename}` changed; run `cargo dev generate-preview-features-reference`");
            }
            anstream::println!("Up-to-date: {filename}");
        }
        Mode::Write => {
            fs_err::write(&reference_path, generated)
                .with_context(|| format!("failed to write `{}`", reference_path.display()))?;
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
        .join("\n")
}

fn render(feature: PreviewFeature, description: &str) -> String {
    format!("### `{feature}` {{#{feature}}}\n\n{description}\n")
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
            ### `pylock` {#pylock}

            A first paragraph,
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
        ### `add-bounds` {#add-bounds}

        The [`add-bounds`](../reference/settings.md#add-bounds) setting controls the default version
        bounds added by `uv add`. Instead of the default lower bound, dependencies can be
        constrained to a major or minor version range, or pinned to an exact version.

        ### `adjust-ulimit` {#adjust-ulimit}

        On Unix, uv raises the process's soft open-file limit at startup, up to the hard limit. This
        helps avoid "too many open files" errors during concurrent operations, and child processes
        inherit the raised limit.

        ### `artifact-hash-filtering` {#artifact-hash-filtering}

        The `uv pip compile --generate-hashes` command restricts the generated hashes to artifacts
        allowed by the selected binary and build policies. For example, when source builds are
        disabled, hashes for source distributions are omitted.

        ### `audit-command` {#audit-command}

        The `uv audit` and `uv tool audit` commands check project and installed-tool dependencies
        for known vulnerabilities. Tool audits use the lockfiles recorded by the
        [`tool-install-locks`](#tool-install-locks) feature; enabling both features silences the
        preview warning for `uv tool audit`.

        ### `auth-helper` {#auth-helper}

        The [`uv auth helper`](./authentication/cli.md#using-credentials-with-external-tools)
        command lets external tools retrieve HTTP credentials through uv. It currently supports the
        Bazel credential helper protocol, reading a JSON request from standard input and writing a
        JSON response containing authentication headers when credentials are available.

        ### `azure-endpoint` {#azure-endpoint}

        uv can authenticate requests to an Azure Blob Storage endpoint using Azure credentials. Set
        [`UV_AZURE_ENDPOINT_URL`](../reference/environment.md#uv_azure_endpoint_url) to identify the
        endpoint; authentication uses the default Azure credential chain, including Azure CLI
        credentials and workload identity.

        ### `batch-export` {#batch-export}

        The `uv export --batch` option exports multiple dependency selections from a TOML manifest
        containing `[[export]]` entries. Each entry specifies an `output-file` and its own package,
        extra, and dependency group selections; output paths are relative to the manifest.

        ### `build-dependency-check` {#build-dependency-check}

        Before a nonisolated `uv build`, uv checks that the selected environment satisfies declared,
        backend-reported, and transitive build requirements. This reports missing or incompatible
        build dependencies before building; use `--skip-dependency-check` to skip the check.

        ### `build-dependency-hashes` {#build-dependency-hashes}

        The `--require-build-hashes` option requires hashes for every build dependency, including
        transitive dependencies. It applies when build dependencies are downloaded during builds,
        project resolution, and installation. See [project build dependency
        hashes](./projects/build.md#project-build-dependency-hashes) for configuration and exceptions.

        ### `build-lazy-imports` {#build-lazy-imports}

        uv enables lazy imports when invoking build backends on CPython 3.15 and later. This can
        reduce the work performed by imports during a build, but can also change import-time side
        effects in third-party build backends.

        ### `cache-physical-space` {#cache-physical-space}

        Cache cleanup reports the physical disk space reclaimed, accounting for hardlinks and
        copy-on-write clones on macOS and Linux. If an entry's allocated size cannot be measured, uv
        reports a lower bound; other platforms continue to use a coarser estimate. See [clearing the
        cache](./cache.md#clearing-the-cache) for details.

        ### `cache-size` {#cache-size}

        The `uv cache size` command reports the total size of uv's cache. It supports human-readable
        output and a machine-readable byte count; enabling this feature silences the preview
        warning.

        ### `centralized-project-envs` {#centralized-project-envs}

        uv stores default [project virtual
        environments](./projects/layout.md#centralized-project-environments) in its cache and
        attempts to link `.venv` to the cached environment. Switching interpreters selects separate
        cached environments that can be reused later. Explicit environment paths and environments
        selected with `--active` are not centralized.

        ### `check-command` {#check-command}

        The `uv check` command runs Python type checking with ty. It uses the project's environment
        to resolve imports and can synchronize that environment before checking; enabling this
        feature silences the preview warning.

        ### `content-addressed-cache` {#content-addressed-cache}

        uv identifies cached wheel archives by a digest of their contents. Matching archive contents
        can share a cache entry, reducing duplication when the same wheel contents are encountered
        more than once.

        ### `detect-module-conflicts` {#detect-module-conflicts}

        uv warns when multiple packages install conflicting Python modules into the same
        environment. These conflicts can cause imports to depend on installation order, even when
        the packages have different distribution names.

        ### `extra-build-dependencies` {#extra-build-dependencies}

        The [`extra-build-dependencies`](./projects/config.md#augmenting-build-dependencies) setting
        augments a package's declared build dependencies without disabling build isolation. It can
        supply a missing build dependency or, with `match-runtime = true`, ensure that a build
        dependency uses the same version as the project environment.

        ### `format-command` {#format-command}

        The `uv format` command formats Python code with Ruff, downloading the formatter when
        needed. It can also check formatting with `--check` or show proposed changes with `--diff`;
        enabling this feature silences the preview warning.

        ### `frozen-lockfile` {#frozen-lockfile}

        Frozen project commands can use `uv.lock` without the workspace's `pyproject.toml` file.
        Discovery requires a lockfile with revision 5 or later, which records the workspace
        information needed to select dependencies without the manifest.

        ### `gcs-endpoint` {#gcs-endpoint}

        uv can authenticate requests to a Google Cloud Storage endpoint using Google Cloud
        credentials. Set [`UV_GCS_ENDPOINT_URL`](../reference/environment.md#uv_gcs_endpoint_url) to
        identify the endpoint; authentication uses `GOOGLE_APPLICATION_CREDENTIALS` or Application
        Default Credentials.

        ### `index-by-name` {#index-by-name}

        The `--index` and `--default-index` options accept the names of configured package indexes
        as well as URLs. For example, `--index internal` selects the configured index named
        `internal`, so its URL does not need to be repeated on the command line.

        ### `index-exclude-newer` {#index-exclude-newer}

        Each configured package index can set its own
        [`exclude-newer`](./indexes.md#configuring-exclude-newer-for-an-index) cutoff, overriding
        the global cutoff for packages served by that index. Set the value to `false` to disable the
        cutoff for an index; package-specific `exclude-newer-package` values still take precedence.

        ### `index-hash-algorithm` {#index-hash-algorithm}

        Each configured package index can [require a hash
        algorithm](./indexes.md#requiring-a-hash-algorithm) with the `hash-algorithm` setting. uv
        records that algorithm's hash in the lockfile and fails if a distribution does not advertise
        it, instead of selecting another available algorithm.

        ### `init-project-flag` {#init-project-flag}

        The `uv init` command rejects the deprecated `--project` option. To choose where to create a
        project, pass the target directory as a positional argument, such as `uv init my-project`.

        ### `json-output` {#json-output}

        Commands that support `--output-format json`, such as `uv tree`, can produce
        machine-readable output for use by other tools. The JSON schemas are experimental and may
        change without warning; enabling this feature silences the preview warning.

        ### `lock-without-metadata` {#lock-without-metadata}

        uv omits the `package.metadata` tables from `uv.lock`, except for remote URL and Git
        dependencies. Their metadata is retained so uv can check whether those sources are requested
        or stale without network access.

        ### `lockfile-format-check` {#lockfile-format-check}

        Commands using `--locked` or `--check` reject non-canonical lockfile formatting, even if the
        lockfile can be parsed. This makes formatting part of the lockfile check in addition to
        checking whether the resolution is up to date.

        ### `lockfile-normalization` {#lockfile-normalization}

        uv combines equivalent dependency declarations when writing lockfiles. This reduces
        duplicated declarations in the recorded requirements, constraints, overrides, exclusions,
        and dependency groups.

        ### `malware-check` {#malware-check}

        The `uv sync` command and other installation commands can check packages for malware using
        [OSV](https://osv.dev) before installing them. This checks for known malicious packages,
        while [`uv audit`](#audit-command) checks dependencies for known vulnerabilities.

        ### `metadata-json` {#metadata-json}

        The uv build backend includes `METADATA.json` and `WHEEL.json` files in built wheels
        alongside the standard `METADATA` and `WHEEL` files. These additional files expose package
        and wheel metadata in JSON format.

        ### `minimum-libc-version` {#minimum-libc-version}

        The [`minimum-libc-version`](./resolution.md#minimum-libc-version) setting specifies the
        oldest glibc or musl versions that must be supported during universal resolution. Use it
        with `required-environments` to require compatible wheels for the selected Linux
        environments without excluding wheels for newer libc versions.

        ### `missing-exclude-newer-package-lock` {#missing-exclude-newer-package-lock}

        uv omits `exclude-newer-package` entries from the lockfile when the corresponding packages
        are absent from the resolved dependencies. This keeps cutoffs for unrelated packages out of
        `uv.lock`.

        ### `native-auth` {#native-auth}

        The `uv auth` commands store credentials in a [system-native
        location](./authentication/http.md#the-uv-credentials-store), using Keychain Services on
        macOS, Credential Manager on Windows, or the Secret Service API on Linux. uv only retrieves
        credentials that it has stored itself, rather than credentials saved by other applications.

        ### `package-conflicts` {#package-conflicts}

        Workspace members can declare [conflicting
        dependencies](./resolution.md#conflicting-dependencies) at the package level, in addition to
        conflicts between extras or dependency groups. This allows members with incompatible
        requirements to share a lockfile, provided they are not installed together.

        ### `packaged-init` {#packaged-init}

        The `uv init` command creates a packaged application by default, with a `src/` layout, a
        build system, and a script entry point. This gives new applications an installable package
        structure without requiring `--package`.

        ### `project-directory-must-exist` {#project-directory-must-exist}

        The `--project` option rejects invalid paths instead of warning and continuing in the
        current directory. Except for `uv init`, the path must already exist as a directory or point
        to a `pyproject.toml` file. This feature takes effect before configuration is loaded, so it
        must be enabled on the command line or through an environment variable.

        ### `publish-require-normalized` {#publish-require-normalized}

        The `uv publish` command requires distribution filenames to be normalized. Files with
        non-normalized names are skipped when selecting distributions to upload.

        ### `pylock` {#pylock}

        The `uv pip install` and `uv pip sync` commands can install dependencies from `pylock.toml`
        files, the standardized Python lockfile format. For example, use
        `uv pip install -r pylock.toml` or `uv pip sync pylock.toml`; enabling this feature silences
        the preview warning.

        ### `python-install-default` {#python-install-default}

        The `uv python install --default` option installs `python` and `python3` executables
        alongside the versioned executable, making a uv-managed Python available without specifying
        its minor version. When no version is requested and no `.python-version` file is found,
        enabling this feature also makes `uv python install` create these executables by default.
        See [installing Python executables](./python-versions.md#installing-python-executables) for
        details about the installation location and handling of existing executables.

        ### `relocatable-envs-default` {#relocatable-envs-default}

        uv creates relocatable virtual environments by default, using relative paths in their entry
        point and activation scripts. This allows the environment to be moved without invalidating
        those scripts, although arbitrary binaries and nonstandard scripts are not guaranteed to be
        relocatable. Use `uv venv --no-relocatable` to opt out.

        ### `resolution-inputs` {#resolution-inputs}

        uv omits redundant runtime constraints and unused overrides, exclusions, dependency
        metadata, and package-specific upload cutoffs from the lockfile. It records which settings
        were consulted during resolution, including backtracking, so settings that affected
        resolution can be retained even when their packages are absent from the final dependency
        graph.

        ### `s3-endpoint` {#s3-endpoint}

        uv can authenticate requests to an S3-compatible storage endpoint using AWS Signature
        Version 4. Set [`UV_S3_ENDPOINT_URL`](../reference/environment.md#uv_s3_endpoint_url) to
        identify the endpoint; credentials are obtained from the configured AWS credential sources.

        ### `sbom-export` {#sbom-export}

        The `uv export --format cyclonedx1.5` command exports a software bill of materials in
        CycloneDX 1.5 JSON format. This describes the locked dependencies in a format that can be
        consumed by software inventory and security tools.

        ### `special-conda-env-names` {#special-conda-env-names}

        Conda environments named `base` or `root` are classified using their paths, like other named
        environments. The name alone does not cause uv to treat a user-created environment as the
        base Conda installation.

        ### `target-workspace-discovery` {#target-workspace-discovery}

        For a local `uv run` target, uv starts project and workspace discovery from the directory
        containing the target instead of the current working directory. This feature takes effect
        before configuration is loaded, so it must be enabled on the command line or through an
        environment variable.

        ### `toml-backwards-compatibility` {#toml-backwards-compatibility}

        When building source distributions, the uv build backend rewrites `pyproject.toml` as TOML
        1.0 for compatibility with older build tools. The original file is included as
        `pyproject.toml.orig` in the source distribution.

        ### `tool-install-locks` {#tool-install-locks}

        uv stores a `uv.lock` file alongside each installed tool and reuses it for subsequent
        installations and upgrades. The lockfile records the tool's resolved dependencies and also
        provides the dependency information used by `uv tool audit`.

        ### `venv-safe-clear` {#venv-safe-clear}

        The `uv venv --clear` option refuses to clear a directory that does not contain a
        `pyvenv.cfg` file. This helps avoid removing unrelated files when the target is not a
        virtual environment; use `--force` to explicitly allow clearing such a directory.

        ### `workspace-dir` {#workspace-dir}

        The `uv workspace dir` command prints the path to the workspace root. Use `--package` to
        print the path to a specific workspace member, for example, with `--package my-package`.

        ### `workspace-list` {#workspace-list}

        The `uv workspace list` command lists the names of workspace members, with one name per
        line. Use `--paths` to display their paths instead, for example, when passing workspace
        directories to another tool.

        ### `workspace-list-scripts` {#workspace-list-scripts}

        The `uv workspace list --scripts` command lists standalone Python scripts with inline
        metadata under the workspace root. It prints script paths relative to the workspace root,
        allowing tools to discover scripts separately from workspace members.

        ### `workspace-metadata` {#workspace-metadata}

        The `uv workspace metadata` command exposes structured information about a workspace and its
        resolved dependencies for use by other tools. Its output is experimental and may change; use
        `--sync` to include a mapping from importable modules to the packages that provide them.
        "##);
    }
}
