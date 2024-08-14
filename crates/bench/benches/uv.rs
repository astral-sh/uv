use std::str::FromStr;

use bench::criterion::black_box;
use bench::criterion::{criterion_group, criterion_main, measurement::WallTime, Criterion};
use pypi_types::Requirement;
use uv_cache::Cache;
use uv_client::RegistryClientBuilder;
use uv_python::PythonEnvironment;
use uv_resolver::Manifest;

fn resolve_warm_jupyter(c: &mut Criterion<WallTime>) {
    let run = setup(Manifest::simple(vec![Requirement::from(
        pep508_rs::Requirement::from_str("jupyter==1.0.0").unwrap(),
    )]));
    c.bench_function("resolve_warm_jupyter", |b| b.iter(|| run(false)));
}

fn resolve_warm_jupyter_universal(c: &mut Criterion<WallTime>) {
    let run = setup(Manifest::simple(vec![Requirement::from(
        pep508_rs::Requirement::from_str("jupyter==1.0.0").unwrap(),
    )]));
    c.bench_function("resolve_warm_jupyter_universal", |b| b.iter(|| run(true)));
}

fn resolve_warm_airflow(c: &mut Criterion<WallTime>) {
    let run = setup(Manifest::simple(vec![
        Requirement::from(pep508_rs::Requirement::from_str("apache-airflow[all]==2.9.3").unwrap()),
        Requirement::from(
            pep508_rs::Requirement::from_str("apache-airflow-providers-apache-beam>3.0.0").unwrap(),
        ),
    ]));
    c.bench_function("resolve_warm_airflow", |b| b.iter(|| run(false)));
}

// This takes >5m to run in CodSpeed.
// fn resolve_warm_airflow_universal(c: &mut Criterion<WallTime>) {
//     let run = setup(Manifest::simple(vec![
//         Requirement::from(pep508_rs::Requirement::from_str("apache-airflow[all]").unwrap()),
//         Requirement::from(
//             pep508_rs::Requirement::from_str("apache-airflow-providers-apache-beam>3.0.0").unwrap(),
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
    use chrono::NaiveDate;

    use distribution_types::IndexLocations;
    use install_wheel_rs::linker::LinkMode;
    use pep440_rs::Version;
    use pep508_rs::{MarkerEnvironment, MarkerEnvironmentBuilder};
    use platform_tags::{Arch, Os, Platform, Tags};
    use uv_cache::Cache;
    use uv_client::RegistryClient;
    use uv_configuration::{
        BuildOptions, Concurrency, ConfigSettings, IndexStrategy, PreviewMode, SetupPyStrategy,
        SourceStrategy,
    };
    use uv_dispatch::BuildDispatch;
    use uv_distribution::DistributionDatabase;
    use uv_git::GitResolver;
    use uv_python::Interpreter;
    use uv_resolver::{
        FlatIndex, InMemoryIndex, Manifest, OptionsBuilder, PythonRequirement, RequiresPython,
        ResolutionGraph, Resolver, ResolverMarkers,
    };
    use uv_types::{BuildIsolation, EmptyInstalledPackages, HashStrategy, InFlight};

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

    static TAGS: LazyLock<Tags> =
        LazyLock::new(|| Tags::from_env(&PLATFORM, (3, 11), "cpython", (3, 11), false).unwrap());

    pub(crate) async fn resolve(
        manifest: Manifest,
        cache: Cache,
        client: &RegistryClient,
        interpreter: &Interpreter,
        universal: bool,
    ) -> Result<ResolutionGraph> {
        let build_isolation = BuildIsolation::Isolated;
        let build_options = BuildOptions::default();
        let concurrency = Concurrency::default();
        let config_settings = ConfigSettings::default();
        let exclude_newer = Some(
            NaiveDate::from_ymd_opt(2024, 8, 8)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc()
                .into(),
        );
        let flat_index = FlatIndex::default();
        let git = GitResolver::default();
        let hashes = HashStrategy::None;
        let in_flight = InFlight::default();
        let index = InMemoryIndex::default();
        let index_locations = IndexLocations::default();
        let installed_packages = EmptyInstalledPackages;
        let sources = SourceStrategy::default();
        let options = OptionsBuilder::new().exclude_newer(exclude_newer).build();
        let build_constraints = [];

        let python_requirement = if universal {
            PythonRequirement::from_requires_python(
                interpreter,
                &RequiresPython::greater_than_equal_version(&Version::new([3, 11])),
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
            &index,
            &git,
            &in_flight,
            IndexStrategy::default(),
            SetupPyStrategy::default(),
            &config_settings,
            build_isolation,
            LinkMode::default(),
            &build_options,
            exclude_newer,
            sources,
            concurrency,
            PreviewMode::Disabled,
        );

        let markers = if universal {
            ResolverMarkers::universal(vec![])
        } else {
            ResolverMarkers::specific_environment(MARKERS.clone())
        };

        let resolver = Resolver::new(
            manifest,
            options,
            &python_requirement,
            markers,
            Some(&TAGS),
            &flat_index,
            &index,
            &hashes,
            &build_context,
            installed_packages,
            DistributionDatabase::new(
                client,
                &build_context,
                concurrency.downloads,
                PreviewMode::Disabled,
            ),
        )?;

        Ok(resolver.resolve().await?)
    }
}
