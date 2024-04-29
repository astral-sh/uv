use std::str::FromStr;

use criterion::black_box;
use criterion::{criterion_group, criterion_main, measurement::WallTime, Criterion};

use pep508_rs::Requirement;
use uv_cache::Cache;
use uv_resolver::Manifest;

fn resolve_warm_black(c: &mut Criterion<WallTime>) {
    let cache = Cache::from_path(".cache").unwrap();

    let cache = &cache;
    let run = || async move {
        let manifest = Manifest::simple(vec![Requirement::from_str("black").unwrap()]);
        resolver::resolve(black_box(manifest), black_box(cache.clone())).await
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    c.bench_function("resolve_warm_black", |b| {
        b.to_async(&runtime).iter_with_large_drop(run);
    });
}

fn resolve_warm_jupyter(c: &mut Criterion<WallTime>) {
    let cache = Cache::from_path(".cache").unwrap();

    let cache = &cache;
    let run = || async move {
        let manifest = Manifest::simple(vec![Requirement::from_str("jupyter").unwrap()]);
        resolver::resolve(black_box(manifest), black_box(cache.clone())).await
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    c.bench_function("resolve_warm_jupyter", |b| {
        b.to_async(&runtime).iter_with_large_drop(run);
    });
}

criterion_group!(uv, resolve_warm_black, resolve_warm_jupyter);
criterion_main!(uv);

mod resolver {
    use anyhow::Result;
    use once_cell::sync::Lazy;
    use std::path::{Path, PathBuf};
    use std::str::FromStr;

    use distribution_types::{IndexLocations, Resolution, SourceDist};
    use pep508_rs::{MarkerEnvironment, Requirement, StringVersion};
    use platform_tags::{Arch, Os, Platform, Tags};
    use uv_cache::Cache;
    use uv_client::RegistryClientBuilder;
    use uv_configuration::{BuildKind, NoBinary, NoBuild, SetupPyStrategy};
    use uv_interpreter::{Interpreter, PythonEnvironment};
    use uv_resolver::{FlatIndex, InMemoryIndex, Manifest, Options, ResolutionGraph, Resolver};
    use uv_types::{
        BuildContext, BuildIsolation, EmptyInstalledPackages, HashStrategy, SourceBuildTrait,
    };

    static MARKERS: Lazy<MarkerEnvironment> = Lazy::new(|| {
        MarkerEnvironment {
            implementation_name: "cpython".to_string(),
            implementation_version: StringVersion::from_str("3.11.5").unwrap(),
            os_name: "posix".to_string(),
            platform_machine: "arm64".to_string(),
            platform_python_implementation: "CPython".to_string(),
            platform_release: "21.6.0".to_string(),
            platform_system: "Darwin".to_string(),
            platform_version: "Darwin Kernel Version 21.6.0: Mon Aug 22 20:19:52 PDT 2022; root:xnu-8020.140.49~2/RELEASE_ARM64_T6000".to_string(),
            python_full_version: StringVersion::from_str("3.11.5").unwrap(),
            python_version: StringVersion::from_str("3.11").unwrap(),
            sys_platform: "darwin".to_string(),
        }
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

    pub(crate) async fn resolve(manifest: Manifest, cache: Cache) -> Result<ResolutionGraph> {
        let client = RegistryClientBuilder::new(cache.clone()).build();
        let flat_index = FlatIndex::default();
        let index = InMemoryIndex::default();
        let interpreter = Interpreter::artificial(PLATFORM.clone(), MARKERS.clone());
        let build_context = Context::new(cache, interpreter.clone());
        let hashes = HashStrategy::None;
        let installed_packages = EmptyInstalledPackages;

        let resolver = Resolver::new(
            manifest,
            Options::default(),
            &MARKERS,
            &interpreter,
            &TAGS,
            &client,
            &flat_index,
            &index,
            &hashes,
            &build_context,
            &installed_packages,
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
