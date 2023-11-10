use std::sync::Arc;

use anyhow::Result;
use fxhash::FxHashMap;
use resolvo::DefaultSolvableDisplay;
use resolvo::Solver;

use pep508_rs::MarkerEnvironment;
use platform_tags::Tags;
use puffin_client::RegistryClient;
use puffin_traits::BuildContext;

use crate::database::Database;
use crate::resolution::Resolution;
use crate::resolvo::provider::ResolvoDependencyProvider;
use crate::resolvo::ResolvoPackage;
use crate::{Manifest, ResolveError};

/// Resolve a [`Manifest`] into a [`Resolution`].
pub async fn resolve<Context: BuildContext + Send + Sync + 'static>(
    manifest: Manifest,
    markers: MarkerEnvironment,
    tags: Tags,
    client: RegistryClient,
    build_context: Context,
) -> Result<Resolution, ResolveError> {
    let database = Arc::new(Database::new(tags, client, build_context));

    // A channel to fetch package metadata (e.g., given `flask`, fetch all versions) and version
    // metadata (e.g., given `flask==1.0.0`, fetch the metadata for that version).
    let (request_sink, request_stream) = futures::channel::mpsc::unbounded();

    // Run the fetcher.
    let requests_fut = database.listen(request_stream);

    // Construct a provider
    let provider = ResolvoDependencyProvider::new(database.clone(), request_sink, markers);

    // Generate the root requirements.
    let pool = provider.pool();
    let mut root_requirements = Vec::with_capacity(manifest.requirements.len());
    for requirement in &manifest.requirements {
        let package_name =
            pool.intern_package_name(ResolvoPackage::Package(requirement.name.clone()));
        let version_set_id =
            pool.intern_version_set(package_name, requirement.version_or_url.clone().into());
        root_requirements.push(version_set_id);

        for extra in requirement.extras.iter().flatten() {
            let dependency_package_name = pool.intern_package_name(ResolvoPackage::Extra(
                requirement.name.clone(),
                extra.clone(),
            ));
            let version_set_id = pool.intern_version_set(
                dependency_package_name,
                requirement.version_or_url.clone().into(),
            );
            root_requirements.push(version_set_id);
        }
    }

    // Run the solver.
    let resolve_fut = tokio::task::spawn_blocking(move || solve(&provider, root_requirements));

    // The requests stream should terminate before the solver.
    requests_fut.await?;
    let resolution = resolve_fut.await??;

    Ok(resolution)
}

/// Run the Resolvo solver.
fn solve<Context: BuildContext>(
    provider: &ResolvoDependencyProvider<Context>,
    root_requirements: Vec<resolvo::VersionSetId>,
) -> Result<Resolution, ResolveError> {
    // Run the solver itself.
    let mut solver = Solver::new(provider);
    let solvables = match solver.solve(root_requirements) {
        Ok(solvables) => solvables,
        Err(err) => {
            return Err(ResolveError::Resolvo(anyhow::anyhow!(
                "{}",
                err.display_user_friendly(&solver, &DefaultSolvableDisplay)
                    .to_string()
                    .trim()
            )));
        }
    };

    // Convert the solution to a `Resolution`.
    let pool = provider.pool();
    let mut packages = FxHashMap::default();
    for solvable_id in solvables {
        let solvable = pool.resolve_solvable(solvable_id);
        let package = pool.resolve_package_name(solvable.name_id());
        packages.insert(package.name().clone(), provider.dist(solvable_id));
    }

    Ok(Resolution::new(packages))
}
