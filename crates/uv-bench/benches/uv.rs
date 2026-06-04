// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

use std::fmt::Write;
use std::hint::black_box;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main, measurement::WallTime};
use flate2::write::GzEncoder;
use futures::executor::block_on;
use futures::io::AllowStdIo;
use tokio_util::compat::{FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt};
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity, RegistryClientBuilder};
use uv_distribution_filename::{SourceDistExtension, WheelFilename};
use uv_distribution_types::Requirement;
use uv_install_wheel::{InstallState, Layout, LinkMode};
use uv_preview::Preview;
use uv_pypi_types::Scheme;
use uv_python::PythonEnvironment;
use uv_resolver::Manifest;

const MANY_FILES_WHEEL_FILENAME: &str = "manyfiles-0.0.0-py3-none-any.whl";
const MANY_FILES_WHEEL_FILE_COUNT: usize = 10_000;
const MANY_FILES_SDIST_TOP_LEVEL: &str = "manyfiles-0.0.0";
const MANY_FILES_SDIST_FILE_COUNT: usize = 10_000;

fn create_many_files_wheel() -> tempfile::NamedTempFile {
    let archive = tempfile::NamedTempFile::new().expect("Failed to create temporary archive");
    let mut writer = ZipFileWriter::new(Vec::new());
    let mut record = String::new();
    for index in 0..MANY_FILES_WHEEL_FILE_COUNT {
        let path = format!("manyfiles/{index}.txt");
        write_zip_entry(&mut writer, &path, b"");
        writeln!(record, "{path},,0").expect("Writing to a string cannot fail");
    }
    write_zip_entry(
        &mut writer,
        "manyfiles-0.0.0.dist-info/METADATA",
        b"Metadata-Version: 2.1\nName: manyfiles\nVersion: 0.0.0\n",
    );
    write_zip_entry(
        &mut writer,
        "manyfiles-0.0.0.dist-info/WHEEL",
        b"Wheel-Version: 1.0\nGenerator: uv-bench\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    );
    record.push_str("manyfiles-0.0.0.dist-info/METADATA,,\n");
    record.push_str("manyfiles-0.0.0.dist-info/WHEEL,,\n");
    record.push_str("manyfiles-0.0.0.dist-info/RECORD,,\n");
    write_zip_entry(
        &mut writer,
        "manyfiles-0.0.0.dist-info/RECORD",
        record.as_bytes(),
    );
    fs_err::write(
        archive.path(),
        block_on(writer.close()).expect("Failed to finish ZIP archive"),
    )
    .expect("Failed to write temporary archive");
    archive
}

fn create_many_files_sdist() -> tempfile::NamedTempFile {
    let archive = tempfile::NamedTempFile::new().expect("Failed to create temporary archive");
    let encoder = GzEncoder::new(archive.as_file(), flate2::Compression::default());
    let mut writer =
        tokio_tar::Builder::new_non_terminated(AllowStdIo::new(encoder).compat_write());
    for index in 0..MANY_FILES_SDIST_FILE_COUNT {
        write_tar_entry(
            &mut writer,
            &format!("{MANY_FILES_SDIST_TOP_LEVEL}/manyfiles/{index}.txt"),
            b"",
        );
    }
    write_tar_entry(
        &mut writer,
        &format!("{MANY_FILES_SDIST_TOP_LEVEL}/PKG-INFO"),
        b"Metadata-Version: 2.1\nName: manyfiles\nVersion: 0.0.0\n",
    );
    write_tar_entry(
        &mut writer,
        &format!("{MANY_FILES_SDIST_TOP_LEVEL}/pyproject.toml"),
        b"[project]\nname = \"manyfiles\"\nversion = \"0.0.0\"\n",
    );
    let writer = block_on(writer.into_inner()).expect("Failed to finish tar archive");
    writer
        .into_inner()
        .into_inner()
        .finish()
        .expect("Failed to finish gzip archive");
    archive
}

fn create_sdist_extraction_directory() -> tempfile::TempDir {
    #[cfg(target_os = "linux")]
    if let Ok(directory) = tempfile::tempdir_in("/dev/shm") {
        return directory;
    }

    tempfile::tempdir().expect("Failed to create sdist extraction directory")
}

fn unpack_sdist_many_files(c: &mut Criterion<WallTime>) {
    let archive = create_many_files_sdist();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");

    c.bench_function("unpack_sdist_many_files", |b| {
        b.iter_batched(
            || {
                (
                    runtime
                        .block_on(fs_err::tokio::File::open(archive.path()))
                        .expect("Failed to open temporary archive"),
                    create_sdist_extraction_directory(),
                )
            },
            |(archive, extracted_sdist)| {
                let files = runtime
                    .block_on(uv_extract::stream::archive(
                        archive,
                        SourceDistExtension::TarGz,
                        extracted_sdist.path(),
                    ))
                    .expect("Failed to unpack sdist");
                let source_tree = uv_extract::strip_component(extracted_sdist.path())
                    .expect("Failed to strip top-level sdist directory");
                black_box((files, extracted_sdist, source_tree))
            },
            BatchSize::PerIteration,
        );
    });
}

fn unzip_wheel_many_files(c: &mut Criterion<WallTime>) {
    let archive = create_many_files_wheel();

    c.bench_function("unzip_wheel_many_files", |b| {
        b.iter_batched(
            || {
                (
                    fs_err::File::open(archive.path()).expect("Failed to open temporary archive"),
                    tempfile::tempdir().expect("Failed to create wheel extraction directory"),
                )
            },
            |(archive, extracted_wheel)| {
                let files = uv_extract::unzip(archive, extracted_wheel.path())
                    .expect("Failed to extract wheel");
                black_box((files, extracted_wheel))
            },
            BatchSize::SmallInput,
        );
    });
}

fn prepare_wheel_many_files(c: &mut Criterion<WallTime>) {
    let archive = create_many_files_wheel();
    let filename =
        WheelFilename::from_str(MANY_FILES_WHEEL_FILENAME).expect("Invalid wheel filename");

    c.bench_function("prepare_wheel_many_files", |b| {
        b.iter_batched(
            || {
                (
                    fs_err::File::open(archive.path()).expect("Failed to open temporary archive"),
                    tempfile::tempdir().expect("Failed to create wheel extraction directory"),
                )
            },
            |(archive, extracted_wheel)| {
                let files = prepare_wheel(archive, extracted_wheel.path(), &filename);
                black_box((files, extracted_wheel))
            },
            BatchSize::SmallInput,
        );
    });
}

fn install_wheel_many_files(c: &mut Criterion<WallTime>) {
    let archive = create_many_files_wheel();
    let filename =
        WheelFilename::from_str(MANY_FILES_WHEEL_FILENAME).expect("Invalid wheel filename");
    let extracted_wheel = tempfile::tempdir().expect("Failed to create wheel extraction directory");
    prepare_wheel(
        fs_err::File::open(archive.path()).expect("Failed to open temporary archive"),
        extracted_wheel.path(),
        &filename,
    );

    c.bench_function("install_wheel_many_files", |b| {
        b.iter_batched(
            || {
                let environment =
                    tempfile::tempdir().expect("Failed to create installation directory");
                let layout = layout(environment.path());
                fs_err::create_dir_all(&layout.scheme.purelib)
                    .expect("Failed to create site-packages directory");
                (environment, layout)
            },
            |(environment, layout)| {
                let state = InstallState::new(Preview::default());
                uv_install_wheel::install_wheel(
                    &layout,
                    false,
                    extracted_wheel.path(),
                    &filename,
                    None,
                    None::<&()>,
                    None::<&()>,
                    Some("uv"),
                    true,
                    LinkMode::default(),
                    &state,
                )
                .expect("Failed to install wheel");
                state
                    .warn_package_conflicts()
                    .expect("Failed to check for package conflicts");
                black_box((environment, layout))
            },
            BatchSize::SmallInput,
        );
    });
}

fn prepare_wheel(
    archive: fs_err::File,
    extracted_wheel: &Path,
    filename: &WheelFilename,
) -> Vec<(PathBuf, u64)> {
    let files = uv_extract::unzip(archive, extracted_wheel).expect("Failed to extract wheel");
    uv_install_wheel::validate_and_heal_record(extracted_wheel, files.iter(), filename)
        .expect("Failed to validate wheel");
    files
}

fn write_zip_entry(writer: &mut ZipFileWriter<Vec<u8>>, path: &str, contents: &[u8]) {
    let entry = ZipEntryBuilder::new(path.into(), Compression::Stored);
    block_on(writer.write_entry_whole(entry, contents)).expect("Failed to write ZIP entry");
}

fn write_tar_entry<W: tokio::io::AsyncWrite + Unpin + Send>(
    writer: &mut tokio_tar::Builder<W>,
    path: &str,
    contents: &[u8],
) {
    let mut header = tokio_tar::Header::new_gnu();
    header.set_size(contents.len() as u64);
    header.set_mode(0o644);
    header.set_entry_type(tokio_tar::EntryType::Regular);
    header.set_cksum();
    block_on(writer.append_data(
        &mut header,
        path,
        AllowStdIo::new(Cursor::new(contents)).compat(),
    ))
    .expect("Failed to write tar entry");
}

fn layout(root: &Path) -> Layout {
    let site_packages = root.join("site-packages");
    Layout {
        sys_executable: root.join("bin/python"),
        python_version: (3, 11),
        os_name: "posix".to_string(),
        scheme: Scheme {
            purelib: site_packages.clone(),
            platlib: site_packages,
            scripts: root.join("bin"),
            data: root.to_path_buf(),
            include: root.join("include"),
        },
    }
}

fn resolve_warm_jupyter(c: &mut Criterion<WallTime>) {
    let manifest = Manifest::simple(vec![Requirement::from(
        uv_pep508::Requirement::from_str("jupyter==1.0.0").unwrap(),
    )]);
    let run = setup(manifest, false);
    c.bench_function("resolve_warm_jupyter", |b| b.iter(&run));
}

fn resolve_warm_jupyter_universal(c: &mut Criterion<WallTime>) {
    let manifest = Manifest::simple(vec![Requirement::from(
        uv_pep508::Requirement::from_str("jupyter==1.0.0").unwrap(),
    )]);
    let run = setup(manifest, true);
    c.bench_function("resolve_warm_jupyter_universal", |b| b.iter(&run));
}

fn resolve_warm_airflow(c: &mut Criterion<WallTime>) {
    let manifest = Manifest::simple(vec![
        Requirement::from(uv_pep508::Requirement::from_str("apache-airflow[all]==2.9.3").unwrap()),
        Requirement::from(
            uv_pep508::Requirement::from_str("apache-airflow-providers-apache-beam>3.0.0").unwrap(),
        ),
    ]);
    let run = setup(manifest, false);
    c.bench_function("resolve_warm_airflow", |b| b.iter(&run));
}

// This takes >5m to run in CodSpeed.
// fn resolve_warm_airflow_universal(c: &mut Criterion<WallTime>) {
//     let manifest = Manifest::simple(vec![
//         Requirement::from(uv_pep508::Requirement::from_str("apache-airflow[all]").unwrap()),
//         Requirement::from(
//             uv_pep508::Requirement::from_str("apache-airflow-providers-apache-beam>3.0.0").unwrap(),
//         ),
//     ]);
//     let run = setup(manifest, true);
//     c.bench_function("resolve_warm_airflow_universal", |b| b.iter(&run));
// }

criterion_group!(
    uv,
    unpack_sdist_many_files,
    unzip_wheel_many_files,
    prepare_wheel_many_files,
    install_wheel_many_files,
    resolve_warm_jupyter,
    resolve_warm_jupyter_universal,
    resolve_warm_airflow
);
criterion_main!(uv);

fn setup(manifest: Manifest, universal: bool) -> impl Fn() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        // CodSpeed limits the total number of threads to 500
        .max_blocking_threads(256)
        .enable_all()
        .build()
        .unwrap();

    let cache = Cache::from_path("../../.cache")
        .init_no_wait()
        .expect("No cache contention when running benchmarks")
        .unwrap();
    let interpreter = PythonEnvironment::from_root("../../.venv", &cache)
        .unwrap()
        .into_interpreter();
    let client = RegistryClientBuilder::new(BaseClientBuilder::default(), cache.clone())
        .build()
        .expect("failed to build registry client");

    // Prime the cache: First run for performance the network operation, the second run primes
    // reading from the cache from the first run. If they are already primed, we only lose ~1s for
    // the large airflow benchmark.
    for _ in 0..2 {
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

    // No matter how long the benchmarks run, never do fresh network requests
    let client = RegistryClientBuilder::new(
        BaseClientBuilder::default().connectivity(Connectivity::Offline),
        cache.clone(),
    )
    .build()
    .expect("failed to build registry client");

    move || {
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
    use uv_configuration::{BuildOptions, Concurrency, Constraints, IndexStrategy, NoSources};
    use uv_dispatch::{BuildDispatch, SharedState};
    use uv_distribution::DistributionDatabase;
    use uv_distribution_types::{
        ConfigSettings, DependencyMetadata, ExtraBuildRequires, ExtraBuildVariables,
        IndexLocations, PackageConfigSettings, RequiresPython,
    };
    use uv_install_wheel::LinkMode;
    use uv_pep440::Version;
    use uv_pep508::{MarkerEnvironment, MarkerEnvironmentBuilder};
    use uv_platform_tags::{Arch, Os, Platform, Tags, TagsOptions};
    use uv_preview::Preview;
    use uv_pypi_types::{Conflicts, ResolverMarkerEnvironment};
    use uv_python::Interpreter;
    use uv_resolver::{
        ExcludeNewer, FlatIndex, InMemoryIndex, Manifest, OptionsBuilder, PythonRequirement,
        Resolver, ResolverEnvironment, ResolverOutput,
    };
    use uv_types::{
        BuildIsolation, EmptyInstalledPackages, HashStrategy, SourceTreeEditablePolicy,
    };
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
        Tags::from_env(
            &PLATFORM,
            (3, 11),
            "cpython",
            (3, 11),
            TagsOptions::default(),
        )
        .unwrap()
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
        let extra_build_variables = ExtraBuildVariables::default();
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
        let sources = NoSources::default();
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
            &extra_build_variables,
            LinkMode::default(),
            &build_options,
            &hashes,
            exclude_newer,
            sources,
            SourceTreeEditablePolicy::Project,
            workspace_cache,
            concurrency.clone(),
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
            DistributionDatabase::new(
                client,
                &build_context,
                concurrency.downloads_semaphore.clone(),
            ),
        )?;

        Ok(resolver.resolve().await?)
    }
}
