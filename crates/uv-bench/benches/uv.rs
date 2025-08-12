use std::str::FromStr;

use std::hint::black_box;
use uv_bench::criterion::{Criterion, criterion_group, criterion_main, measurement::WallTime};
use uv_cache::Cache;
use uv_client::RegistryClientBuilder;
use uv_distribution_types::Requirement;
use uv_python::PythonEnvironment;
use uv_resolver::Manifest;

fn resolve_warm_jupyter(c: &mut Criterion<WallTime>) {
    let run = setup(Manifest::simple(vec![Requirement::from(
        uv_pep508::Requirement::from_str("jupyter==1.0.0").unwrap(),
    )]));
    c.bench_function("resolve_warm_jupyter", |b| b.iter(|| run(false)));
}

fn resolve_warm_jupyter_universal(c: &mut Criterion<WallTime>) {
    let run = setup(Manifest::simple(vec![Requirement::from(
        uv_pep508::Requirement::from_str("jupyter==1.0.0").unwrap(),
    )]));
    c.bench_function("resolve_warm_jupyter_universal", |b| b.iter(|| run(true)));
}

fn resolve_warm_airflow(c: &mut Criterion<WallTime>) {
    let run = setup(Manifest::simple(vec![
        Requirement::from(uv_pep508::Requirement::from_str("apache-airflow[all]==2.9.3").unwrap()),
        Requirement::from(
            uv_pep508::Requirement::from_str("apache-airflow-providers-apache-beam>3.0.0").unwrap(),
        ),
    ]));
    c.bench_function("resolve_warm_airflow", |b| b.iter(|| run(false)));
}

// This takes >5m to run in CodSpeed.
// fn resolve_warm_airflow_universal(c: &mut Criterion<WallTime>) {
//     let run = setup(Manifest::simple(vec![
//         Requirement::from(uv_pep508::Requirement::from_str("apache-airflow[all]").unwrap()),
//         Requirement::from(
//             uv_pep508::Requirement::from_str("apache-airflow-providers-apache-beam>3.0.0").unwrap(),
//         ),
//     ]));
//     c.bench_function("resolve_warm_airflow_universal", |b| b.iter(|| run(true)));
// }

criterion_group!(
    uv,
    resolve_warm_jupyter,
    resolve_warm_jupyter_universal,
    resolve_warm_airflow
);
criterion_main!(uv);

fn setup(manifest: Manifest) -> impl Fn(bool) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        // CodSpeed limits the total number of threads to 500
        .max_blocking_threads(256)
        .enable_all()
        .build()
        .unwrap();

    let cache = Cache::from_path("../../.cache").init().unwrap();
    let interpreter = PythonEnvironment::from_root("../../.venv", &cache)
        .unwrap()
        .into_interpreter();
    let client = RegistryClientBuilder::new(cache.clone()).build();

    move |universal| {
        runtime
            .block_on(resolver::resolve(
                black_box(manifest.clone()),
                black_box(cache.clone()),
                black_box(&client),
                &interpreter,
                universal,
            ))
            .unwrap();
    }
}

mod resolver {
    use std::sync::LazyLock;

    use anyhow::Result;

    use uv_cache::Cache;
    use uv_client::RegistryClient;
    use uv_configuration::{
        BuildOptions, Concurrency, ConfigSettings, Constraints, IndexStrategy,
        PackageConfigSettings, Preview, SourceStrategy,
    };
    use uv_dispatch::{BuildDispatch, SharedState};
    use uv_distribution::DistributionDatabase;
    use uv_distribution_types::{
        DependencyMetadata, ExtraBuildRequires, IndexLocations, RequiresPython,
    };
    use uv_install_wheel::LinkMode;
    use uv_pep440::Version;
    use uv_pep508::{MarkerEnvironment, MarkerEnvironmentBuilder};
    use uv_platform_tags::{Arch, Os, Platform, Tags};
    use uv_pypi_types::{Conflicts, ResolverMarkerEnvironment};
    use uv_python::Interpreter;
    use uv_resolver::{
        ExcludeNewer, FlatIndex, InMemoryIndex, Manifest, OptionsBuilder, PythonRequirement,
        Resolver, ResolverEnvironment, ResolverOutput,
    };
    use uv_types::{BuildIsolation, EmptyInstalledPackages, HashStrategy};
    use uv_workspace::WorkspaceCache;

    static MARKERS: LazyLock<MarkerEnvironment> = LazyLock::new(|| {
        MarkerEnvironment::try_from(MarkerEnvironmentBuilder {
            implementation_name: "cpython",
            implementation_version: "3.11.5",
            os_name: "posix",
            platform_machine: "arm64",
            platform_python_implementation: "CPython",
            platform_release: "21.6.0",
            platform_system: "Darwin",
            platform_version: "Darwin Kernel Version 21.6.0: Mon Aug 22 20:19:52 PDT 2022; root:xnu-8020.140.49~2/RELEASE_ARM64_T6000",
            python_full_version: "3.11.5",
            python_version: "3.11",
            sys_platform: "darwin",
        }).unwrap()
    });

    static PLATFORM: Platform = Platform::new(
        Os::Macos {
            major: 21,
            minor: 6,
        },
        Arch::Aarch64,
    );

    static TAGS: LazyLock<Tags> = LazyLock::new(|| {
        Tags::from_env(&PLATFORM, (3, 11), "cpython", (3, 11), false, false).unwrap()
    });

    pub(crate) async fn resolve(
        manifest: Manifest,
        cache: Cache,
        client: &RegistryClient,
        interpreter: &Interpreter,
        universal: bool,
    ) -> Result<ResolverOutput> {
        let build_isolation = BuildIsolation::default();
        let extra_build_requires = ExtraBuildRequires::default();
        let build_options = BuildOptions::default();
        let concurrency = Concurrency::default();
        let config_settings = ConfigSettings::default();
        let config_settings_package = PackageConfigSettings::default();
        let exclude_newer = ExcludeNewer::global(
            jiff::civil::date(2024, 9, 1)
                .to_zoned(jiff::tz::TimeZone::UTC)
                .unwrap()
                .timestamp()
                .into(),
        );
        let build_constraints = Constraints::default();
        let flat_index = FlatIndex::default();
        let hashes = HashStrategy::default();
        let state = SharedState::default();
        let index = InMemoryIndex::default();
        let index_locations = IndexLocations::default();
        let installed_packages = EmptyInstalledPackages;
        let options = OptionsBuilder::new()
            .exclude_newer(exclude_newer.clone())
            .build();
        let sources = SourceStrategy::default();
        let dependency_metadata = DependencyMetadata::default();
        let conflicts = Conflicts::empty();
        let workspace_cache = WorkspaceCache::default();

        let python_requirement = if universal {
            PythonRequirement::from_requires_python(
                interpreter,
                RequiresPython::greater_than_equal_version(&Version::new([3, 11])),
            )
        } else {
            PythonRequirement::from_interpreter(interpreter)
        };

        let build_context = BuildDispatch::new(
            client,
            &cache,
            &build_constraints,
            interpreter,
            &index_locations,
            &flat_index,
            &dependency_metadata,
            state,
            IndexStrategy::default(),
            &config_settings,
            &config_settings_package,
            build_isolation,
            &extra_build_requires,
            LinkMode::default(),
            &build_options,
            &hashes,
            exclude_newer,
            sources,
            workspace_cache,
            concurrency,
            Preview::default(),
        );

        let markers = if universal {
            ResolverEnvironment::universal(vec![])
        } else {
            ResolverEnvironment::specific(ResolverMarkerEnvironment::from(MARKERS.clone()))
        };

        let resolver = Resolver::new(
            manifest,
            options,
            &python_requirement,
            markers,
            interpreter.markers(),
            conflicts,
            Some(&TAGS),
            &flat_index,
            &index,
            &hashes,
            &build_context,
            installed_packages,
            DistributionDatabase::new(client, &build_context, concurrency.downloads),
        )?;

        Ok(resolver.resolve().await?)
    }
}
