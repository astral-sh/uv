use uv_bench::criterion::{
    criterion_group, criterion_main, measurement::WallTime, BenchmarkId, Criterion, Throughput,
};
use uv_distribution_filename::WheelFilename;
use uv_platform_tags::Tags;

/// A set of platform tags extracted from burntsushi's Archlinux workstation.
/// We could just re-create these via `Tags::from_env`, but those might differ
/// depending on the platform. This way, we always use the same data. It also
/// lets us assert tag compatibility regardless of where the benchmarks run.
const PLATFORM_TAGS: &[(&str, &str, &str)] = include!("../inputs/platform_tags.rs");

/// A set of wheel names used in the benchmarks below. We pick short and long
/// names, as well as compatible and not-compatibles (with `PLATFORM_TAGS`)
/// names.
///
/// The tuple is (name, filename, compatible) where `name` is a descriptive
/// name for humans used in the benchmark definition. And `filename` is the
/// actual wheel filename we want to benchmark operation on. And `compatible`
/// indicates whether the tags in the wheel filename are expected to be
/// compatible with the tags in `PLATFORM_TAGS`.
const WHEEL_NAMES: &[(&str, &str, bool)] = &[
    // This tests a case with a very short name that is *not* compatible
    // with PLATFORM_TAGS. It only uses one tag for each component (one
    // Python version, one ABI and one platform).
    (
        "flyte-short-incompatible",
        "hypothesis-4.24.5-py2-none-any.whl",
        false,
    ),
    // This tests a case with a very short name that *is* compatible with
    // PLATFORM_TAGS. It only uses one tag for each component (one Python
    // version, one ABI and one platform).
    (
        "flyte-short-compatible",
        "ipython-2.1.0-py3-none-any.whl",
        true,
    ),
    // This tests a case with a long name that is *not* compatible. That
    // is, all platform tags need to be checked against the tags in the
    // wheel filename. This is essentially the worst possible practical
    // case.
    (
        "flyte-long-incompatible",
        "protobuf-3.5.2.post1-cp36-cp36m-macosx_10_6_intel.macosx_10_9_intel.macosx_10_9_x86_64.macosx_10_10_intel.macosx_10_10_x86_64.whl",
        false,
    ),
    // This tests a case with a long name that *is* compatible. We
    // expect this to be (on average) quicker because the compatibility
    // check stops as soon as a positive match is found. (Where as the
    // incompatible case needs to check all tags.)
    (
        "flyte-long-compatible",
        "coverage-6.6.0b1-cp311-cp311-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
        true,
    ),
];

/// A list of names that are candidates for wheel filenames but will ultimately
/// fail to parse.
const INVALID_WHEEL_NAMES: &[(&str, &str)] = &[
    ("flyte-short-extension", "mock-5.1.0.tar.gz"),
    (
        "flyte-long-extension",
        "Pillow-5.4.0.dev0-py3.7-macosx-10.13-x86_64.egg",
    ),
];

/// Benchmarks the construction of platform tags.
///
/// This only happens ~once per program startup. Originally, construction was
/// trivial. But to speed up `WheelFilename::is_compatible`, we added some
/// extra processing. We thus expect construction to become slower, but we
/// write a benchmark to ensure it is still "reasonable."
fn benchmark_build_platform_tags(c: &mut Criterion<WallTime>) {
    let tags: Vec<(String, String, String)> = PLATFORM_TAGS
        .iter()
        .map(|&(py, abi, plat)| (py.to_string(), abi.to_string(), plat.to_string()))
        .collect();

    let mut group = c.benchmark_group("build_platform_tags");
    group.bench_function(BenchmarkId::from_parameter("burntsushi-archlinux"), |b| {
        b.iter(|| std::hint::black_box(Tags::new(tags.clone())));
    });
    group.finish();
}

/// Benchmarks `WheelFilename::from_str`. This has been observed to take some
/// non-trivial time in profiling (although, at time of writing, not as much
/// as tag compatibility). In the process of optimizing tag compatibility,
/// we tweaked wheel filename parsing. This benchmark was therefore added to
/// ensure we didn't regress here.
fn benchmark_wheelname_parsing(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("wheelname_parsing");
    for (name, filename, _) in WHEEL_NAMES.iter().copied() {
        let len = u64::try_from(filename.len()).expect("length fits in u64");
        group.throughput(Throughput::Bytes(len));
        group.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter(|| {
                filename
                    .parse::<WheelFilename>()
                    .expect("valid wheel filename");
            });
        });
    }
    group.finish();
}

/// Benchmarks `WheelFilename::from_str` when it fails. This routine is called
/// on every filename in a package's metadata. A non-trivial portion of which
/// are not wheel filenames. Ensuring that the error path is fast is thus
/// probably a good idea.
fn benchmark_wheelname_parsing_failure(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("wheelname_parsing_failure");
    for (name, filename) in INVALID_WHEEL_NAMES.iter().copied() {
        let len = u64::try_from(filename.len()).expect("length fits in u64");
        group.throughput(Throughput::Bytes(len));
        group.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter(|| {
                filename
                    .parse::<WheelFilename>()
                    .expect_err("invalid wheel filename");
            });
        });
    }
    group.finish();
}

/// Benchmarks the `WheelFilename::is_compatible` routine. This was revealed
/// to be the #1 bottleneck in the resolver. The main issue was that the
/// set of platform tags (generated once) is quite large, and the original
/// implementation did an exhaustive search over each of them for each tag in
/// the wheel filename.
fn benchmark_wheelname_tag_compatibility(c: &mut Criterion<WallTime>) {
    let tags: Vec<(String, String, String)> = PLATFORM_TAGS
        .iter()
        .map(|&(py, abi, plat)| (py.to_string(), abi.to_string(), plat.to_string()))
        .collect();
    let tags = Tags::new(tags);

    let mut group = c.benchmark_group("wheelname_tag_compatibility");
    for (name, filename, expected) in WHEEL_NAMES.iter().copied() {
        let wheelname: WheelFilename = filename.parse().expect("valid wheel filename");
        let len = u64::try_from(filename.len()).expect("length fits in u64");
        group.throughput(Throughput::Bytes(len));
        group.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter(|| {
                assert_eq!(expected, wheelname.is_compatible(&tags));
            });
        });
    }
    group.finish();
}

criterion_group!(
    uv_distribution_filename,
    benchmark_build_platform_tags,
    benchmark_wheelname_parsing,
    benchmark_wheelname_parsing_failure,
    benchmark_wheelname_tag_compatibility,
);
criterion_main!(uv_distribution_filename);
