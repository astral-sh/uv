use std::str::FromStr;

use bench::criterion::black_box;
use bench::criterion::{criterion_group, criterion_main, measurement::WallTime, Criterion};
use distribution_types::Requirement;
use uv_cache::Cache;
use uv_client::RegistryClientBuilder;
use uv_resolver::Manifest;

fn resolve_warm_jupyter(c: &mut Criterion<WallTime>) {
    let runtime = &tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let cache = &Cache::from_path(".cache").unwrap().init().unwrap();
    let manifest = &Manifest::simple(vec![Requirement::from_pep508(
        pep508_rs::Requirement::from_str("jupyter").unwrap(),
    )
    .unwrap()]);
    let client = &RegistryClientBuilder::new(cache.clone()).build();

    let run = || {
        runtime
            .block_on(resolver::resolve(
                black_box(manifest.clone()),
                black_box(cache.clone()),
                black_box(client),
            ))
            .unwrap();
    };

    c.bench_function("resolve_warm_jupyter", |b| b.iter(run));
}

criterion_group!(uv, resolve_warm_jupyter);
criterion_main!(uv);

mod resolver {
    use std::path::{Path, PathBuf};

    use anyhow::Result;
    use once_cell::sync::Lazy;

    use distribution_types::{IndexLocations, Requirement, Resolution, SourceDist};
    use pep508_rs::{MarkerEnvironment, MarkerEnvironmentBuilder};
    use platform_tags::{Arch, Os, Platform, Tags};
    use uv_cache::Cache;
    use uv_client::RegistryClient;
    use uv_configuration::{BuildKind, Concurrency, NoBinary, NoBuild, SetupPyStrategy};
    use uv_distribution::DistributionDatabase;
    use uv_interpreter::{Interpreter, PythonEnvironment};
    use uv_resolver::{
        FlatIndex, InMemoryIndex, Manifest, Options, PythonRequirement, ResolutionGraph, Resolver,
    };
    use uv_types::{
        BuildContext, BuildIsolation, EmptyInstalledPackages, HashStrategy, SourceBuildTrait,
    };

    static MARKERS: Lazy<MarkerEnvironment> = Lazy::new(|| {
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

    static TAGS: Lazy<Tags> =
        Lazy::new(|| Tags::from_env(&PLATFORM, (3, 11), "cpython", (3, 11), false).unwrap());

    pub(crate) async fn resolve(
        manifest: Manifest,
        cache: Cache,
        client: &RegistryClient,
    ) -> Result<ResolutionGraph> {
        let flat_index = FlatIndex::default();
        let index = InMemoryIndex::default();
        let interpreter = Interpreter::artificial(PLATFORM.clone(), MARKERS.clone());
        let build_context = Context::new(cache, interpreter.clone());
        let hashes = HashStrategy::None;
        let installed_packages = EmptyInstalledPackages;
        let python_requirement = PythonRequirement::from_marker_environment(&interpreter, &MARKERS);
        let concurrency = Concurrency::default();

        let resolver = Resolver::new(
            manifest,
            Options::default(),
            &python_requirement,
            Some(&MARKERS),
            &TAGS,
            &flat_index,
            &index,
            &hashes,
            &build_context,
            installed_packages,
            DistributionDatabase::new(client, &build_context, concurrency.downloads),
        )?;

        Ok(resolver.resolve().await?)
    }

    struct Context {
        cache: Cache,
        interpreter: Interpreter,
        index_locations: IndexLocations,
    }

    impl Context {
        fn new(cache: Cache, interpreter: Interpreter) -> Self {
            Self {
                cache,
                interpreter,
                index_locations: IndexLocations::default(),
            }
        }
    }

    impl BuildContext for Context {
        type SourceDistBuilder = DummyBuilder;

        fn cache(&self) -> &Cache {
            &self.cache
        }

        fn interpreter(&self) -> &Interpreter {
            &self.interpreter
        }

        fn build_isolation(&self) -> BuildIsolation {
            BuildIsolation::Isolated
        }

        fn no_build(&self) -> &NoBuild {
            &NoBuild::None
        }

        fn no_binary(&self) -> &NoBinary {
            &NoBinary::None
        }

        fn setup_py_strategy(&self) -> SetupPyStrategy {
            SetupPyStrategy::default()
        }

        fn index_locations(&self) -> &IndexLocations {
            &self.index_locations
        }

        async fn resolve<'a>(&'a self, _: &'a [Requirement]) -> Result<Resolution> {
            panic!("benchmarks should not build source distributions")
        }

        async fn install<'a>(&'a self, _: &'a Resolution, _: &'a PythonEnvironment) -> Result<()> {
            panic!("benchmarks should not build source distributions")
        }

        async fn setup_build<'a>(
            &'a self,
            _: &'a Path,
            _: Option<&'a Path>,
            _: &'a str,
            _: Option<&'a SourceDist>,
            _: BuildKind,
        ) -> Result<Self::SourceDistBuilder> {
            Ok(DummyBuilder)
        }
    }

    struct DummyBuilder;

    impl SourceBuildTrait for DummyBuilder {
        async fn metadata(&mut self) -> Result<Option<PathBuf>> {
            panic!("benchmarks should not build source distributions")
        }

        async fn wheel<'a>(&'a self, _: &'a Path) -> Result<String> {
            panic!("benchmarks should not build source distributions")
        }
    }
}
