use std::convert;
use std::sync::Arc;

use anyhow::{Context, Error, Result, ensure};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rustc_hash::FxHashMap;
use tokio::sync::oneshot;
use tracing::{instrument, warn};

use uv_cache::Cache;
use uv_configuration::initialize_rayon_once;
use uv_distribution::DistributionDatabase;
use uv_distribution_types::{CachedDist, Name, Resolution, VariantsJsonFilename};
use uv_install_wheel::{Layout, LinkMode};
use uv_normalize::PackageName;
use uv_pep508::MarkerEnvironment;
use uv_preview::Preview;
use uv_python::PythonEnvironment;
use uv_types::BuildContext;
use uv_variants::variants_json::VariantsJsonContent;

pub struct Installer<'a> {
    venv: &'a PythonEnvironment,
    link_mode: LinkMode,
    cache: Option<&'a Cache>,
    reporter: Option<Arc<dyn Reporter>>,
    /// The name of the [`Installer`].
    name: Option<String>,
    /// The metadata associated with the [`Installer`].
    metadata: bool,
    /// The selected properties used to evaluate each installed wheel's dependency markers.
    variants: FxHashMap<PackageName, serde_json::Value>,
    /// Preview settings for the installer.
    preview: Preview,
}

impl<'a> Installer<'a> {
    /// Initialize a new installer.
    pub fn new(venv: &'a PythonEnvironment, preview: Preview) -> Self {
        Self {
            venv,
            link_mode: LinkMode::default(),
            cache: None,
            reporter: None,
            name: Some("uv".to_string()),
            metadata: true,
            variants: FxHashMap::default(),
            preview,
        }
    }

    /// Set the [`LinkMode`][`uv_install_wheel::LinkMode`] to use for this installer.
    #[must_use]
    pub fn with_link_mode(self, link_mode: LinkMode) -> Self {
        Self { link_mode, ..self }
    }

    /// Set the [`Cache`] to use for this installer.
    #[must_use]
    pub fn with_cache(self, cache: &'a Cache) -> Self {
        Self {
            cache: Some(cache),
            ..self
        }
    }

    /// Set the [`Reporter`] to use for this installer.
    #[must_use]
    pub fn with_reporter(self, reporter: Arc<dyn Reporter>) -> Self {
        Self {
            reporter: Some(reporter),
            ..self
        }
    }

    /// Set the `installer_name` to something other than `"uv"`.
    #[must_use]
    pub fn with_installer_name(self, installer_name: Option<String>) -> Self {
        Self {
            name: installer_name,
            ..self
        }
    }

    /// Set whether to install uv-specifier files in the dist-info directory.
    #[must_use]
    pub fn with_installer_metadata(self, installer_metadata: bool) -> Self {
        Self {
            metadata: installer_metadata,
            ..self
        }
    }

    /// Resolve the installed marker context before replacing any existing packages.
    ///
    /// A wheel can list several alternative property values. Save only the values supported
    /// by the installation target so subsequent inspection does not need to execute providers.
    pub async fn with_variant_contexts(
        mut self,
        wheels: &[CachedDist],
        resolution: &Resolution,
        database: &DistributionDatabase<'_, impl BuildContext>,
        markers: &MarkerEnvironment,
    ) -> Result<Self> {
        for wheel in wheels {
            let Some(label) = wheel.filename().variant() else {
                continue;
            };
            let installed_path = uv_install_wheel::installed_dist_info_path(
                &self.venv.interpreter().layout(),
                wheel.path(),
            )?;
            let dist_info = installed_path
                .file_name()
                .context("Missing wheel dist-info directory")?;
            let metadata_path = wheel.path().join(dist_info).join("variant.json");
            let metadata: VariantsJsonContent =
                serde_json::from_slice(&fs_err::read(&metadata_path)?)?;
            let filename = VariantsJsonFilename {
                name: wheel.name().clone(),
                version: wheel.filename().version.clone(),
            };
            metadata.validate_wheel(label)?;
            let variant = if let Some(context) = resolution.variant_context(wheel.filename()) {
                // The target was already determined during selection. Validate the retained
                // properties against the wheel without repeating provider discovery or policy.
                ensure!(
                    context.label.as_ref() == Some(label),
                    "Selected wheel variant label changed"
                );
                context
                    .validate_properties(metadata, label)
                    .context("Selected wheel variant properties changed")?
            } else {
                database
                    .query_wheel_variants(metadata, label, markers, &filename)
                    .await?
            };
            self.variants
                .insert(wheel.name().clone(), serde_json::to_value(variant)?);
        }
        Ok(self)
    }

    /// Install a set of wheels into a Python virtual environment.
    #[instrument(skip_all, fields(num_wheels = %wheels.len()))]
    pub async fn install(self, wheels: Vec<CachedDist>) -> Result<Vec<CachedDist>> {
        let Self {
            venv,
            cache,
            link_mode,
            reporter,
            name: installer_name,
            metadata: installer_metadata,
            variants,
            preview,
        } = self;

        if cache.is_some_and(Cache::is_temporary) {
            if link_mode.is_symlink() {
                return Err(anyhow::anyhow!(
                    "Symlink-based installation is not supported with `--no-cache`. The created environment will be rendered unusable by the removal of the cache."
                ));
            }
        }

        let (tx, rx) = oneshot::channel();

        let layout = venv.interpreter().layout();
        let relocatable = venv.relocatable();
        // Initialize the threadpool with the user settings.
        initialize_rayon_once();
        rayon::spawn(move || {
            let result = install(
                wheels,
                &layout,
                installer_name.as_deref(),
                link_mode,
                reporter.as_ref(),
                relocatable,
                installer_metadata,
                &variants,
                preview,
            );

            // This may fail if the main task was cancelled.
            let _ = tx.send(result);
        });

        rx.await
            .map_err(|_| anyhow::anyhow!("`install_blocking` task panicked"))
            .and_then(convert::identity)
    }

    /// Install a set of wheels into a Python virtual environment synchronously.
    #[instrument(skip_all, fields(num_wheels = %wheels.len()))]
    pub fn install_blocking(self, wheels: Vec<CachedDist>) -> Result<Vec<CachedDist>> {
        if self.cache.is_some_and(Cache::is_temporary) {
            if self.link_mode.is_symlink() {
                return Err(anyhow::anyhow!(
                    "Symlink-based installation is not supported with `--no-cache`. The created environment will be rendered unusable by the removal of the cache."
                ));
            }
        }

        install(
            wheels,
            &self.venv.interpreter().layout(),
            self.name.as_deref(),
            self.link_mode,
            self.reporter.as_ref(),
            self.venv.relocatable(),
            self.metadata,
            &self.variants,
            self.preview,
        )
    }
}

/// Install a set of wheels into a Python virtual environment synchronously.
#[instrument(skip_all, fields(num_wheels = %wheels.len()))]
fn install(
    wheels: Vec<CachedDist>,
    layout: &Layout,
    installer_name: Option<&str>,
    link_mode: LinkMode,
    reporter: Option<&Arc<dyn Reporter>>,
    relocatable: bool,
    installer_metadata: bool,
    variants: &FxHashMap<PackageName, serde_json::Value>,
    preview: Preview,
) -> Result<Vec<CachedDist>> {
    // Initialize the threadpool with the user settings.
    initialize_rayon_once();
    let state = uv_install_wheel::InstallState::new(preview);
    wheels.par_iter().try_for_each(|wheel| {
        uv_install_wheel::install_wheel(
            layout,
            relocatable,
            wheel.path(),
            wheel.filename(),
            wheel
                .parsed_url()
                .map(uv_pypi_types::DirectUrl::from)
                .as_ref(),
            if wheel.cache_info().is_empty() {
                None
            } else {
                Some(wheel.cache_info())
            },
            wheel.build_info(),
            variants.get(wheel.name()),
            installer_name,
            installer_metadata,
            link_mode,
            &state,
        )
        .with_context(|| format!("Failed to install: {} ({wheel})", wheel.filename()))?;

        if let Some(reporter) = reporter.as_ref() {
            reporter.on_install_progress(wheel);
        }

        Ok::<(), Error>(())
    })?;
    if let Err(err) = state.warn_package_conflicts() {
        warn!("Checking for conflicts between packages failed: {err}");
    }

    Ok(wheels)
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a dependency is installed.
    fn on_install_progress(&self, wheel: &CachedDist);

    /// Callback to invoke when the resolution is complete.
    fn on_install_complete(&self);
}

#[cfg(test)]
mod tests {
    use uv_cache::Cache;
    use uv_preview::Preview;
    use uv_python::{EnvironmentPreference, PythonEnvironment, PythonPreference, PythonRequest};

    use super::Installer;

    fn environment() -> PythonEnvironment {
        let _preview = uv_preview::test::with_features(&[]);
        let cache = Cache::temp().expect("cache should be available");
        PythonEnvironment::find(
            &PythonRequest::Any,
            EnvironmentPreference::Any,
            PythonPreference::System,
            &cache,
        )
        .expect("Python environment should be available")
    }

    #[test]
    fn default_installer_name() {
        let environment = environment();

        let installer = Installer::new(&environment, Preview::default());

        assert_eq!(installer.name.as_deref(), Some("uv"));
    }

    #[test]
    fn custom_installer_name() {
        let environment = environment();

        let installer = Installer::new(&environment, Preview::default())
            .with_installer_name(Some("client".to_string()));

        assert_eq!(installer.name.as_deref(), Some("client"));
    }

    #[test]
    fn disabled_installer_name() {
        let environment = environment();

        let installer = Installer::new(&environment, Preview::default()).with_installer_name(None);

        assert_eq!(installer.name, None);
    }
}
