use std::io::Read;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use fs_err::File;
use rustc_hash::FxHashMap;
use tokio::process::Command;
use tokio::sync::{Mutex, OnceCell};
use tracing::debug;

use uv_configuration::{
    BuildOptions, Constraints, HashCheckingMode, IndexStrategy, NoBinary, NoBuild, NoSources,
};
use uv_distribution_types::{
    BuiltDist, DependencyMetadata, Dist, Index, IndexLocations, IndexUrl, PluginVariantFeature,
    Requirement, ResolvedDist, VariantProviderOptIn, valid_plugin_endpoint,
};
use uv_pep508::{Requirement as Pep508Requirement, VersionOrUrl};
use uv_pypi_types::VerbatimParsedUrl;
use uv_resolver::{ExcludeNewer, FlatIndex};
use uv_static::EnvVars;
use uv_types::{BuildContext, BuildStack, HashStrategy, HashVerification};
use uv_virtualenv::{OnExisting, Prompt, Seed, UpgradePolicy};

use crate::BuildDispatch;

const MAX_REQUIREMENTS: usize = 32;
const MAX_REQUIREMENT_LENGTH: usize = 4_096;
const MAX_MARKER_COMPLEXITY: usize = 128;
const MAX_OUTPUT_BYTES: u64 = 1_048_576;

#[derive(Debug, Hash, PartialEq, Eq)]
struct ProviderKey {
    requires: Vec<String>,
    endpoint: String,
    index: IndexUrl,
}

#[derive(Clone, Default)]
pub(crate) struct ProviderCache(Arc<Mutex<FxHashMap<ProviderKey, ProviderResult>>>);

type ProviderResult = Arc<OnceCell<Vec<PluginVariantFeature>>>;

pub(crate) async fn query(
    dispatch: &BuildDispatch<'_>,
    requires: &[String],
    plugin_api: Option<&str>,
    opt_in: &VariantProviderOptIn,
    require_hashes: bool,
) -> Result<Option<Vec<PluginVariantFeature>>> {
    ensure!(
        !requires.is_empty() && requires.len() <= MAX_REQUIREMENTS,
        "wheel variant provider must declare between 1 and {MAX_REQUIREMENTS} requirements"
    );
    let requirements = requires
        .iter()
        .map(|requirement| {
            ensure!(
                requirement.len() <= MAX_REQUIREMENT_LENGTH,
                "provider requirement is too long"
            );
            // Index metadata controls this input. Bound both nested expressions and long chains
            // of boolean operators before handing the marker to the recursive PEP 508 parser.
            if let Some((_, marker)) = requirement.split_once(';') {
                let boolean_operators = marker
                    .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                    .filter(|token| *token == "and" || *token == "or")
                    .count();
                ensure!(
                    marker.bytes().filter(|byte| *byte == b'(').count() <= MAX_MARKER_COMPLEXITY
                        && boolean_operators <= MAX_MARKER_COMPLEXITY,
                    "provider requirement marker is too complex"
                );
            }
            let requirement = requirement
                .parse::<Pep508Requirement<VerbatimParsedUrl>>()
                .context("invalid provider requirement")?;
            ensure!(
                requirement.extras.is_empty(),
                "provider requirements cannot include extras"
            );
            match &requirement.version_or_url {
                Some(VersionOrUrl::Url(_)) => {
                    bail!("provider requirements cannot use direct URLs");
                }
                Some(VersionOrUrl::VersionSpecifier(_)) | None => {}
            }
            Ok(Requirement::from(requirement))
        })
        .collect::<Result<Vec<_>>>()?;
    let first = requirements
        .first()
        .context("missing provider requirement")?;
    let endpoint = plugin_api
        .map(str::to_owned)
        .unwrap_or_else(|| first.name.as_dist_info_name().into_owned());
    ensure!(
        valid_plugin_endpoint(&endpoint),
        "invalid provider plugin API endpoint `{endpoint}`"
    );
    let packages = requirements.iter().map(|requirement| &requirement.name);
    if !opt_in.authorizes(packages, &endpoint) {
        return Ok(None);
    }
    let index = dispatch
        .variant_policy
        .and_then(|policy| policy.provider_index())
        .context("external wheel variant providers require an authorized provider index")?;
    let required_verification = match dispatch.base_hasher.verification() {
        HashVerification::Required(_) => true,
        HashVerification::None | HashVerification::IfPresent(_) => false,
    };
    // The package wheel's hash does not authenticate the provider or its dependencies.
    // TODO(konsti): Record provider wheel hashes so hash checking can cover this environment.
    ensure!(
        !require_hashes && !required_verification,
        "wheel variant provider packages cannot be installed when hashes are required; provide static target properties"
    );
    let active = requirements
        .iter()
        .filter(|requirement| {
            requirement.evaluate_markers(Some(dispatch.interpreter.markers()), &[])
        })
        .cloned()
        .collect::<Vec<_>>();
    let first_active = active
        .first()
        .context("provider has no active requirements")?;
    ensure!(
        plugin_api.is_some() || first_active.name == first.name,
        "inferred provider endpoint must retain the first requirement after marker filtering"
    );

    let key = ProviderKey {
        requires: requires.to_vec(),
        endpoint: endpoint.clone(),
        index: index.clone(),
    };
    let cell = dispatch
        .shared_state
        .variant_providers
        .0
        .lock()
        .await
        .entry(key)
        .or_default()
        .clone();
    let features = cell
        .get_or_try_init(|| run(dispatch, &active, &endpoint, index))
        .await?;
    Ok(Some(features.clone()))
}

async fn run(
    dispatch: &BuildDispatch<'_>,
    requirements: &[Requirement],
    endpoint: &str,
    index: &IndexUrl,
) -> Result<Vec<PluginVariantFeature>> {
    let build_options = BuildOptions::new(NoBinary::None, NoBuild::All);
    let index_locations =
        IndexLocations::new(vec![Index::from_index_url(index.clone())], vec![], false);
    let client = dispatch
        .client
        .clone()
        .with_index_locations(index_locations.clone());
    let constraints = Constraints::default();
    let dependency_metadata = DependencyMetadata::default();
    let flat_index = FlatIndex::default();
    let hasher = HashStrategy::default();
    // Provider code runs during resolution, so its packages must be selected independently of
    // project-controlled sources, constraints, and dependency metadata.
    let provider_dispatch = BuildDispatch {
        client: &client,
        constraints: &constraints,
        dependency_metadata: &dependency_metadata,
        index_locations: &index_locations,
        index_strategy: IndexStrategy::default(),
        flat_index: &flat_index,
        base_hasher: &hasher,
        build_hash_checking: HashCheckingMode::Verify,
        exclude_newer: ExcludeNewer::default(),
        sources: NoSources::All,
        build_options: &build_options,
        variant_policy: None,
        shared_state: dispatch.shared_state.fork(),
        ..dispatch.clone()
    };
    let stack = BuildStack::for_variant_provider();
    let resolved = provider_dispatch
        .resolve(requirements, None, &stack)
        .await
        .context("failed to resolve provider requirements")?;
    for distribution in resolved.resolution().distributions() {
        let authorized_wheel = match distribution {
            ResolvedDist::Installable { dist, .. } => match dist.as_ref() {
                Dist::Built(BuiltDist::Registry(wheels)) => {
                    let wheel = wheels.best_wheel();
                    wheel.filename.variant().is_none() && &wheel.index == index
                }
                Dist::Built(
                    BuiltDist::DirectUrl(_) | BuiltDist::Path(_) | BuiltDist::GitPath(_),
                ) => false,
                Dist::Source(_) => false,
            },
            ResolvedDist::Installed { .. } => false,
        };
        ensure!(
            authorized_wheel,
            "provider dependency `{distribution}` is not a non-variant wheel from the \
             authorized provider index"
        );
    }

    let temporary = dispatch.cache.venv_dir()?;
    let environment = uv_virtualenv::create_venv(
        &temporary.path().join("venv"),
        dispatch.interpreter.clone(),
        Prompt::None,
        false,
        OnExisting::Fail,
        false,
        Seed::Disabled,
        UpgradePolicy::Fixed,
    )?;
    provider_dispatch
        .install(&resolved, &environment, &stack)
        .await
        .context("failed to install provider requirements")?;

    let output = temporary.path().join("output.json");
    let script = r"import importlib, json, sys
module, separator, object_path = sys.argv[1].partition(':')
plugin = importlib.import_module(module)
if separator:
    for component in object_path.split('.'):
        plugin = getattr(plugin, component)
configs = plugin.get_supported_configs()
result = [
    {'name': config.name, 'values': config.values, 'multi_value': config.multi_value}
    for config in configs
]
with open(sys.argv[2], 'w', encoding='utf-8') as stream:
    json.dump(result, stream)
";
    debug!("Querying wheel variant provider `{endpoint}`");
    let status = Command::new(environment.python_executable())
        .args(["-I", "-c", script, endpoint])
        .arg(&output)
        .current_dir(temporary.path())
        .env_remove(EnvVars::PYX_API_KEY)
        .env_remove(EnvVars::UV_API_KEY)
        .env_remove(EnvVars::PYX_AUTH_TOKEN)
        .env_remove(EnvVars::UV_AUTH_TOKEN)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .status()
        .await
        .context("failed to execute provider plugin")?;
    if !status.success() {
        bail!("provider plugin exited with {status}");
    }
    let mut contents = Vec::new();
    File::open(&output)?
        .take(MAX_OUTPUT_BYTES + 1)
        .read_to_end(&mut contents)?;
    ensure!(
        contents.len() as u64 <= MAX_OUTPUT_BYTES,
        "provider output exceeded {MAX_OUTPUT_BYTES} bytes"
    );
    serde_json::from_slice(&contents).context("invalid provider plugin output")
}
