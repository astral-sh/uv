#![allow(clippy::print_stderr, clippy::print_stdout)]

use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::hint::black_box;
use std::io::{self, BufRead, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use cpu_time::ProcessTime;
use uv_python_bytecode::{CompiledModule, compile};
use walkdir::WalkDir;

const EXPECTED_VERSION: &str = "3.14.5";
const EXPECTED_MAGIC: &str = "2b0e0d0a";
const BOOTSTRAP_SAMPLES: usize = 10_000;
const BOOTSTRAP_SEED: u64 = 0x75_76_2d_62_79_74_65;

const PYTHON_HELPER: &str = r#"
import importlib.util
import marshal
import sys
import time
import warnings

EXPECTED_VERSION = (3, 14, 5)
EXPECTED_MAGIC = "2b0e0d0a"

version = sys.version_info[:3]
magic = importlib.util.MAGIC_NUMBER.hex()
if version != EXPECTED_VERSION or magic != EXPECTED_MAGIC:
    print(
        f"ERROR\texpected CPython 3.14.5 magic {EXPECTED_MAGIC}, "
        f"got {'.'.join(map(str, version))} magic {magic}",
        flush=True,
    )
    raise SystemExit(2)

print(f"READY\t{'.'.join(map(str, version))}\t{magic}", flush=True)
warnings.simplefilter("ignore", SyntaxWarning)
stream = sys.stdin.buffer
sources = []
precompiled = []

def read_exact(length):
    value = stream.read(length)
    if len(value) != length:
        raise EOFError(f"expected {length} bytes, got {len(value)}")
    return value

while True:
    command = stream.readline()
    if not command:
        raise SystemExit(0)
    fields = command.decode("ascii").rstrip("\n").split("\t")
    if fields[0] == "INIT":
        count = int(fields[1])
        accepted = []
        rejected = 0
        accepted_bytes = 0
        for index in range(count):
            header = stream.readline().decode("ascii").rstrip("\n").split("\t")
            if len(header) != 3 or header[0] != "FILE":
                raise RuntimeError(f"invalid FILE header: {header!r}")
            filename = read_exact(int(header[1])).decode("utf-8")
            source = read_exact(int(header[2])).decode("utf-8")
            try:
                code = compile(
                    source,
                    filename,
                    "exec",
                    dont_inherit=True,
                    optimize=0,
                )
            except (SyntaxError, UnicodeError, ValueError):
                rejected += 1
                continue
            accepted.append((filename, source))
            precompiled.append(code)
            accepted_bytes += len(source.encode("utf-8"))
            print(f"ACCEPT\t{index}")
        sources = accepted
        print(
            f"MANIFEST\t{len(accepted)}\t{rejected}\t{accepted_bytes}",
            flush=True,
        )
    elif fields[0] == "RUN":
        phase = fields[1]
        if phase == "compiler_core":
            wall_started = time.perf_counter_ns()
            process_cpu_started = time.process_time_ns()
            outputs = [
                compile(
                    source,
                    filename,
                    "exec",
                    dont_inherit=True,
                    optimize=0,
                )
                for filename, source in sources
            ]
            process_cpu_ns = time.process_time_ns() - process_cpu_started
            wall_elapsed_ns = time.perf_counter_ns() - wall_started
            observed = sum(len(output.co_code) for output in outputs)
        elif phase == "compile_marshal":
            wall_started = time.perf_counter_ns()
            process_cpu_started = time.process_time_ns()
            outputs = [
                marshal.dumps(
                    compile(
                        source,
                        filename,
                        "exec",
                        dont_inherit=True,
                        optimize=0,
                    )
                )
                for filename, source in sources
            ]
            process_cpu_ns = time.process_time_ns() - process_cpu_started
            wall_elapsed_ns = time.perf_counter_ns() - wall_started
            observed = sum(len(output) for output in outputs)
        elif phase == "marshal_only":
            wall_started = time.perf_counter_ns()
            process_cpu_started = time.process_time_ns()
            outputs = [marshal.dumps(output) for output in precompiled]
            process_cpu_ns = time.process_time_ns() - process_cpu_started
            wall_elapsed_ns = time.perf_counter_ns() - wall_started
            observed = sum(len(output) for output in outputs)
        else:
            raise RuntimeError(f"unknown phase: {phase}")
        output_count = len(outputs)
        del outputs
        print(
            f"RESULT\t{wall_elapsed_ns}\t{process_cpu_ns}"
            f"\t{output_count}\t{observed}",
            flush=True,
        )
    elif fields[0] == "EXIT":
        raise SystemExit(0)
    else:
        raise RuntimeError(f"unknown command: {fields!r}")
"#;

#[derive(Clone, Debug, Eq, PartialEq)]
struct Options {
    python: String,
    phase: Phase,
    warmups: usize,
    samples: usize,
    cooldown: Duration,
    output: PathBuf,
    roots: Vec<PathBuf>,
    limit: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    CompilerCore,
    CompileMarshal,
    MarshalOnly,
}

impl Phase {
    const fn label(self) -> &'static str {
        match self {
            Self::CompilerCore => "compiler_core",
            Self::CompileMarshal => "compile_marshal",
            Self::MarshalOnly => "marshal_only",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "compiler_core" => Ok(Self::CompilerCore),
            "compile_marshal" => Ok(Self::CompileMarshal),
            "marshal_only" => Ok(Self::MarshalOnly),
            _ => Err(format!(
                "--phase must be one of compiler_core, compile_marshal, or marshal_only; got {value}"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Engine {
    Rust,
    Cpython,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Metric {
    Wall,
    ProcessCpu,
}

impl Metric {
    const ALL: [Self; 2] = [Self::Wall, Self::ProcessCpu];

    const fn label(self) -> &'static str {
        match self {
            Self::Wall => "wall",
            Self::ProcessCpu => "process_cpu",
        }
    }

    const fn value(self, row: &RawRow) -> u64 {
        match self {
            Self::Wall => row.wall_elapsed_ns,
            Self::ProcessCpu => row.process_cpu_ns,
        }
    }
}

impl Engine {
    const fn label(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Cpython => "cpython",
        }
    }
}

#[derive(Debug)]
struct SourceFile {
    path: PathBuf,
    filename: String,
    source: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawRow {
    phase: &'static str,
    block: usize,
    position: usize,
    order: &'static str,
    kind: &'static str,
    first_engine: &'static str,
    engine: &'static str,
    cooldown_ms: u128,
    wall_elapsed_ns: u64,
    process_cpu_ns: u64,
    file_count: usize,
    source_bytes: usize,
    included: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Timings {
    wall_elapsed_ns: u64,
    process_cpu_ns: u64,
}

impl Timings {
    fn new(wall_elapsed_ns: u64, process_cpu_ns: u64) -> Result<Self, String> {
        if wall_elapsed_ns == 0 {
            return Err("wall-clock sample duration is zero".to_string());
        }
        if process_cpu_ns == 0 {
            return Err("process CPU sample duration is zero".to_string());
        }
        Ok(Self {
            wall_elapsed_ns,
            process_cpu_ns,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SampleResult {
    timings: Timings,
    count: usize,
    observed: usize,
}

#[derive(Debug)]
struct PythonRunner {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

#[derive(Debug)]
struct PythonManifest {
    accepted_indices: Vec<usize>,
    rejected: usize,
    source_bytes: usize,
}

#[derive(Clone, Copy, Debug)]
struct Summary {
    median: f64,
    p95: f64,
    mad: f64,
    relative_mad: f64,
    half_delta: f64,
}

#[derive(Clone, Copy, Debug)]
struct BlockSample {
    block: usize,
    order: &'static str,
    rust_mean_ns: f64,
    cpython_mean_ns: f64,
    ratio: f64,
}

#[derive(Clone, Copy, Debug)]
struct RatioStatistics {
    summary: Summary,
    bootstrap_low: f64,
    bootstrap_high: f64,
    abba_median: Option<f64>,
    baab_median: Option<f64>,
    order_delta: Option<f64>,
    stable: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let options = parse_options_from(env::args().skip(1))?;
    let paths = discover(&options.roots, options.limit);
    if paths.is_empty() {
        return Err("no Python files discovered".to_string());
    }
    let sources = load_sources(paths)?;
    let discovered_source_bytes = sources
        .iter()
        .map(|source| source.source.len())
        .sum::<usize>();

    let mut python = PythonRunner::start(&options.python)?;
    let manifest = python.initialize(&sources)?;
    if manifest.accepted_indices.len() + manifest.rejected != sources.len() {
        return Err(format!(
            "CPython manifest is incomplete: {} accepted + {} rejected != {} discovered",
            manifest.accepted_indices.len(),
            manifest.rejected,
            sources.len()
        ));
    }
    let accepted = manifest
        .accepted_indices
        .iter()
        .map(|&index| {
            sources
                .get(index)
                .ok_or_else(|| format!("CPython returned invalid accepted index {index}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let accepted_source_bytes = accepted
        .iter()
        .map(|source| source.source.len())
        .sum::<usize>();
    if accepted_source_bytes != manifest.source_bytes {
        return Err(format!(
            "source-byte disagreement: Rust={accepted_source_bytes}, CPython={}",
            manifest.source_bytes
        ));
    }

    let precompiled = accepted
        .iter()
        .map(|source| {
            compile(&source.source, &source.filename).map_err(|error| {
                format!(
                    "Rust failed to compile CPython-accepted file {}: {error}",
                    source.path.display()
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let expected_rows = expected_row_count(&options)?;
    let mut rows = Vec::with_capacity(expected_rows);
    run_blocks(
        options.phase,
        "warmup",
        options.warmups,
        options.cooldown,
        false,
        &accepted,
        &precompiled,
        &mut python,
        &mut rows,
    )?;
    run_blocks(
        options.phase,
        "measured",
        options.samples,
        options.cooldown,
        true,
        &accepted,
        &precompiled,
        &mut python,
        &mut rows,
    )?;
    if rows.len() != expected_rows {
        return Err(format!(
            "phase {} produced {} observations, expected {expected_rows}",
            options.phase.label(),
            rows.len(),
        ));
    }
    python.finish()?;

    let metadata = metadata(
        &options,
        &sources,
        discovered_source_bytes,
        &manifest,
        accepted_source_bytes,
    )?;
    write_raw(&options.output, &metadata, &accepted, &rows)?;
    print_summaries(&rows, options.phase)?;
    Ok(())
}

fn parse_options_from<I, S>(arguments: I) -> Result<Options, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut python = None;
    let mut phase = None;
    let mut warmups = None;
    let mut samples = None;
    let mut cooldown = None;
    let mut output = None;
    let mut limit = None;
    let mut roots = Vec::new();
    let mut arguments = arguments.into_iter().map(Into::into);

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--python" => {
                python = Some(
                    arguments
                        .next()
                        .ok_or_else(|| "--python requires an executable".to_string())?,
                );
            }
            "--phase" => {
                phase = Some(Phase::parse(
                    &arguments
                        .next()
                        .ok_or_else(|| "--phase requires a value".to_string())?,
                )?);
            }
            "--warmups" => {
                warmups = Some(parse_positive(
                    &arguments
                        .next()
                        .ok_or_else(|| "--warmups requires a number".to_string())?,
                    "--warmups",
                )?);
            }
            "--samples" => {
                samples = Some(parse_positive(
                    &arguments
                        .next()
                        .ok_or_else(|| "--samples requires a number".to_string())?,
                    "--samples",
                )?);
            }
            "--cooldown-ms" => {
                cooldown =
                    Some(parse_duration(&arguments.next().ok_or_else(|| {
                        "--cooldown-ms requires a number".to_string()
                    })?)?);
            }
            "--output" => {
                output = Some(PathBuf::from(
                    arguments
                        .next()
                        .ok_or_else(|| "--output requires a path".to_string())?,
                ));
            }
            "--limit" => {
                limit = Some(parse_positive(
                    &arguments
                        .next()
                        .ok_or_else(|| "--limit requires a number".to_string())?,
                    "--limit",
                )?);
            }
            "--help" | "-h" => return Err(usage()),
            _ if argument.starts_with('-') => return Err(format!("unknown option: {argument}")),
            _ => roots.push(PathBuf::from(argument)),
        }
    }

    if roots.is_empty() {
        return Err(format!("at least one ROOT is required\n{}", usage()));
    }
    Ok(Options {
        python: python.ok_or_else(|| "--python is required".to_string())?,
        phase: phase.ok_or_else(|| "--phase is required".to_string())?,
        warmups: warmups.ok_or_else(|| "--warmups is required".to_string())?,
        samples: samples.ok_or_else(|| "--samples is required".to_string())?,
        cooldown: cooldown.ok_or_else(|| "--cooldown-ms is required".to_string())?,
        output: output.ok_or_else(|| "--output is required".to_string())?,
        roots,
        limit,
    })
}

fn usage() -> String {
    "usage: benchmark_cpython_corpus --python PATH --phase {compiler_core|compile_marshal|marshal_only} --warmups N --samples N --cooldown-ms N --output PATH [--limit N] ROOT ...".to_string()
}

fn expected_row_count(options: &Options) -> Result<usize, String> {
    options
        .warmups
        .checked_add(options.samples)
        .and_then(|blocks| blocks.checked_mul(4))
        .ok_or_else(|| "warmup and sample counts produce too many observations".to_string())
}

fn parse_positive(value: &str, option: &str) -> Result<usize, String> {
    let value = value
        .parse::<usize>()
        .map_err(|_| format!("{option} requires a positive number"))?;
    if value == 0 {
        return Err(format!("{option} requires a positive number"));
    }
    Ok(value)
}

fn parse_duration(value: &str) -> Result<Duration, String> {
    value
        .parse::<u64>()
        .map(Duration::from_millis)
        .map_err(|_| "--cooldown-ms requires a non-negative integer".to_string())
}

fn discover(roots: &[PathBuf], limit: Option<usize>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for root in roots {
        for entry in WalkDir::new(root).follow_links(false) {
            let Ok(entry) = entry else {
                continue;
            };
            if entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "py")
            {
                paths.push(entry.into_path());
            }
        }
    }
    paths.sort();
    paths.dedup();
    if let Some(limit) = limit {
        paths.truncate(limit);
    }
    paths
}

fn load_sources(paths: Vec<PathBuf>) -> Result<Vec<SourceFile>, String> {
    paths
        .into_iter()
        .map(|path| {
            let bytes = fs_err::read(&path)
                .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
            let source = std::str::from_utf8(&bytes)
                .map_err(|error| format!("{} is not UTF-8: {error}", path.display()))?;
            let normalized;
            let source = if source.contains('\r') {
                normalized = source.replace("\r\n", "\n").replace('\r', "\n");
                normalized.as_str()
            } else {
                source
            };
            let source = source
                .strip_prefix('\u{feff}')
                .unwrap_or(source)
                .to_string();
            let absolute = path
                .canonicalize()
                .map_err(|error| format!("failed to canonicalize {}: {error}", path.display()))?;
            let filename = absolute
                .to_str()
                .ok_or_else(|| format!("path is not UTF-8: {}", absolute.display()))?
                .to_string();
            Ok(SourceFile {
                path: absolute,
                filename,
                source,
            })
        })
        .collect()
}

impl PythonRunner {
    fn start(python: &str) -> Result<Self, String> {
        let mut child = Command::new(python)
            .args(["-u", "-c", PYTHON_HELPER])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("failed to start {python}: {error}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "failed to open CPython stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "failed to open CPython stdout".to_string())?;
        let mut runner = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        };
        let ready = runner.read_line()?;
        let expected = format!("READY\t{EXPECTED_VERSION}\t{EXPECTED_MAGIC}");
        if ready != expected {
            return Err(format!("invalid CPython identity: {ready}"));
        }
        Ok(runner)
    }

    fn initialize(&mut self, sources: &[SourceFile]) -> Result<PythonManifest, String> {
        writeln!(self.stdin, "INIT\t{}", sources.len())
            .map_err(|error| format!("failed to initialize CPython: {error}"))?;
        for source in sources {
            writeln!(
                self.stdin,
                "FILE\t{}\t{}",
                source.filename.len(),
                source.source.len()
            )
            .and_then(|()| self.stdin.write_all(source.filename.as_bytes()))
            .and_then(|()| self.stdin.write_all(source.source.as_bytes()))
            .map_err(|error| format!("failed to send {}: {error}", source.path.display()))?;
        }
        self.stdin
            .flush()
            .map_err(|error| format!("failed to flush CPython input: {error}"))?;

        let mut accepted_indices = Vec::new();
        loop {
            let line = self.read_line()?;
            let fields = line.split('\t').collect::<Vec<_>>();
            match fields.as_slice() {
                ["ACCEPT", index] => accepted_indices.push(
                    index
                        .parse::<usize>()
                        .map_err(|_| format!("invalid accepted index: {line}"))?,
                ),
                ["MANIFEST", accepted, rejected, source_bytes] => {
                    let accepted = accepted
                        .parse::<usize>()
                        .map_err(|_| format!("invalid manifest: {line}"))?;
                    if accepted != accepted_indices.len() {
                        return Err(format!(
                            "CPython reported {accepted} accepted files but sent {} indices",
                            accepted_indices.len()
                        ));
                    }
                    return Ok(PythonManifest {
                        accepted_indices,
                        rejected: rejected
                            .parse::<usize>()
                            .map_err(|_| format!("invalid manifest: {line}"))?,
                        source_bytes: source_bytes
                            .parse::<usize>()
                            .map_err(|_| format!("invalid manifest: {line}"))?,
                    });
                }
                _ => return Err(format!("invalid CPython manifest response: {line}")),
            }
        }
    }

    fn run_phase(&mut self, phase: Phase) -> Result<SampleResult, String> {
        writeln!(self.stdin, "RUN\t{}", phase.label())
            .and_then(|()| self.stdin.flush())
            .map_err(|error| format!("failed to request CPython sample: {error}"))?;
        let line = self.read_line()?;
        parse_sample_response(&line)
    }

    fn read_line(&mut self) -> Result<String, String> {
        let mut line = String::new();
        let bytes = self
            .stdout
            .read_line(&mut line)
            .map_err(|error| format!("failed to read CPython response: {error}"))?;
        if bytes == 0 {
            return Err("CPython helper closed its output unexpectedly".to_string());
        }
        Ok(line.trim_end_matches(['\r', '\n']).to_string())
    }

    fn finish(mut self) -> Result<(), String> {
        writeln!(self.stdin, "EXIT")
            .and_then(|()| self.stdin.flush())
            .map_err(|error| format!("failed to stop CPython helper: {error}"))?;
        drop(self.stdin);
        let status = self
            .child
            .wait()
            .map_err(|error| format!("failed to wait for CPython helper: {error}"))?;
        if !status.success() {
            return Err(format!("CPython helper exited with {status}"));
        }
        Ok(())
    }
}

fn parse_sample_response(line: &str) -> Result<SampleResult, String> {
    let fields = line.split('\t').collect::<Vec<_>>();
    let ["RESULT", wall_elapsed_ns, process_cpu_ns, count, observed] = fields.as_slice() else {
        return Err(format!("invalid CPython sample response: {line}"));
    };
    Ok(SampleResult {
        timings: Timings::new(
            wall_elapsed_ns
                .parse::<u64>()
                .map_err(|_| format!("invalid CPython wall elapsed time: {line}"))?,
            process_cpu_ns
                .parse::<u64>()
                .map_err(|_| format!("invalid CPython process CPU time: {line}"))?,
        )?,
        count: count
            .parse::<usize>()
            .map_err(|_| format!("invalid CPython output count: {line}"))?,
        observed: observed
            .parse::<usize>()
            .map_err(|_| format!("invalid CPython observed bytes: {line}"))?,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_blocks(
    phase: Phase,
    kind: &'static str,
    blocks: usize,
    cooldown: Duration,
    included: bool,
    accepted: &[&SourceFile],
    precompiled: &[CompiledModule],
    python: &mut PythonRunner,
    rows: &mut Vec<RawRow>,
) -> Result<(), String> {
    let source_bytes = accepted.iter().map(|source| source.source.len()).sum();
    for block in 1..=blocks {
        let order = block_order(block);
        for (position, engine) in order.into_iter().enumerate() {
            thread::sleep(cooldown);
            let sample = match engine {
                Engine::Rust => run_rust_phase(phase, accepted, precompiled)?,
                Engine::Cpython => {
                    let sample = python.run_phase(phase)?;
                    if sample.count != accepted.len() {
                        return Err(format!(
                            "CPython returned {} outputs for {} accepted files",
                            sample.count,
                            accepted.len()
                        ));
                    }
                    black_box(sample.observed);
                    sample.timings
                }
            };
            rows.push(RawRow {
                phase: phase.label(),
                block,
                position: position + 1,
                order: block_order_label(block),
                kind,
                first_engine: order[0].label(),
                engine: engine.label(),
                cooldown_ms: cooldown.as_millis(),
                wall_elapsed_ns: sample.wall_elapsed_ns,
                process_cpu_ns: sample.process_cpu_ns,
                file_count: accepted.len(),
                source_bytes,
                included,
            });
        }
    }
    Ok(())
}

fn block_order(block: usize) -> [Engine; 4] {
    if block % 2 == 1 {
        [Engine::Rust, Engine::Cpython, Engine::Cpython, Engine::Rust]
    } else {
        [Engine::Cpython, Engine::Rust, Engine::Rust, Engine::Cpython]
    }
}

fn block_order_label(block: usize) -> &'static str {
    if block % 2 == 1 { "ABBA" } else { "BAAB" }
}

fn run_rust_phase(
    phase: Phase,
    accepted: &[&SourceFile],
    precompiled: &[CompiledModule],
) -> Result<Timings, String> {
    let wall_started = Instant::now();
    let process_cpu_started = process_timer_result(ProcessTime::try_now(), "start")?;
    match phase {
        Phase::CompilerCore => {
            let outputs = accepted
                .iter()
                .map(|source| compile(&source.source, &source.filename))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("Rust compiler failed during sample: {error}"))?;
            let timings = finish_timings(wall_started, process_cpu_started)?;
            black_box(&outputs);
            Ok(timings)
        }
        Phase::CompileMarshal => {
            let outputs = accepted
                .iter()
                .map(|source| {
                    compile(&source.source, &source.filename).map(|module| module.marshal())
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("Rust compiler failed during sample: {error}"))?;
            let timings = finish_timings(wall_started, process_cpu_started)?;
            black_box(&outputs);
            Ok(timings)
        }
        Phase::MarshalOnly => {
            let outputs = precompiled
                .iter()
                .map(CompiledModule::marshal)
                .collect::<Vec<_>>();
            let timings = finish_timings(wall_started, process_cpu_started)?;
            black_box(&outputs);
            Ok(timings)
        }
    }
}

fn finish_timings(
    wall_started: Instant,
    process_cpu_started: ProcessTime,
) -> Result<Timings, String> {
    let process_cpu = process_timer_result(process_cpu_started.try_elapsed(), "stop")?;
    let wall = wall_started.elapsed();
    Timings::new(
        duration_ns(wall, "wall-clock")?,
        duration_ns(process_cpu, "process CPU")?,
    )
}

fn process_timer_result<T>(timer: io::Result<T>, action: &str) -> Result<T, String> {
    timer.map_err(|error| format!("failed to {action} process CPU timer: {error}"))
}

fn duration_ns(duration: Duration, clock: &str) -> Result<u64, String> {
    u64::try_from(duration.as_nanos())
        .map_err(|_| format!("{clock} sample duration exceeds u64 nanoseconds"))
}

fn metadata(
    options: &Options,
    sources: &[SourceFile],
    discovered_source_bytes: usize,
    manifest: &PythonManifest,
    accepted_source_bytes: usize,
) -> Result<Vec<(String, String)>, String> {
    let command_output = |program: &str, arguments: &[&str]| -> Result<String, String> {
        let output = Command::new(program)
            .args(arguments)
            .output()
            .map_err(|error| format!("failed to run {program}: {error}"))?;
        if !output.status.success() {
            return Err(format!("{program} exited with {}", output.status));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    let mut metadata = vec![
        ("python_requested".to_string(), options.python.clone()),
        (
            "python_realpath".to_string(),
            command_output(
                &options.python,
                &[
                    "-c",
                    "import os, sys; print(os.path.realpath(sys.executable))",
                ],
            )?,
        ),
        ("python_version".to_string(), EXPECTED_VERSION.to_string()),
        ("python_magic".to_string(), EXPECTED_MAGIC.to_string()),
        ("phase".to_string(), options.phase.label().to_string()),
        ("warmups".to_string(), options.warmups.to_string()),
        ("samples".to_string(), options.samples.to_string()),
        cooldown_metadata(options),
        ("schedule".to_string(), "ABBA_BAAB".to_string()),
        (
            "primary_attribution_metric".to_string(),
            "process_cpu_ns".to_string(),
        ),
        (
            "process_cpu_semantics".to_string(),
            "whole-process user-plus-system CPU time".to_string(),
        ),
        (
            "wall_time_role".to_string(),
            "diagnostic".to_string(),
        ),
        ("observations_per_block".to_string(), "4".to_string()),
        (
            "observations_per_engine_per_block".to_string(),
            "2".to_string(),
        ),
        (
            "ratio_contract".to_string(),
            "process_cpu_ns mean_rust_ns/mean_cpython_ns; relative_mad<=0.03; half_delta<=0.03; order_delta<=0.03".to_string(),
        ),
        ("discovered_files".to_string(), sources.len().to_string()),
        (
            "discovered_source_bytes".to_string(),
            discovered_source_bytes.to_string(),
        ),
        (
            "accepted_files".to_string(),
            manifest.accepted_indices.len().to_string(),
        ),
        (
            "cpython_rejected".to_string(),
            manifest.rejected.to_string(),
        ),
        (
            "accepted_source_bytes".to_string(),
            accepted_source_bytes.to_string(),
        ),
        (
            "rustc".to_string(),
            command_output("rustc", &["-Vv"])?.replace('\n', " | "),
        ),
        ("cargo".to_string(), command_output("cargo", &["-V"])?),
        ("os".to_string(), env::consts::OS.to_string()),
        ("arch".to_string(), env::consts::ARCH.to_string()),
        ("profile".to_string(), "release".to_string()),
        (
            "target_dir".to_string(),
            env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string()),
        ),
    ];
    for (index, root) in options.roots.iter().enumerate() {
        metadata.push((format!("root_{index}"), root.display().to_string()));
    }
    if let Some(limit) = options.limit {
        metadata.push(("limit".to_string(), limit.to_string()));
    }
    Ok(metadata)
}

fn cooldown_metadata(options: &Options) -> (String, String) {
    (
        "cooldown_ms".to_string(),
        options.cooldown.as_millis().to_string(),
    )
}

fn write_raw(
    output: &Path,
    metadata: &[(String, String)],
    accepted: &[&SourceFile],
    rows: &[RawRow],
) -> Result<(), String> {
    let mut text = String::new();
    for (key, value) in metadata {
        writeln!(&mut text, "# metadata\t{key}\t{value}")
            .map_err(|error| format!("failed to format metadata: {error}"))?;
    }
    for source in accepted {
        writeln!(&mut text, "# accepted\t{}", source.filename)
            .map_err(|error| format!("failed to format manifest: {error}"))?;
    }
    text.push_str(
        "phase\tblock\tposition\torder\tkind\tfirst_engine\tengine\tcooldown_ms\twall_elapsed_ns\tprocess_cpu_ns\tfile_count\tsource_bytes\tincluded\n",
    );
    for row in rows {
        writeln!(
            &mut text,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            row.phase,
            row.block,
            row.position,
            row.order,
            row.kind,
            row.first_engine,
            row.engine,
            row.cooldown_ms,
            row.wall_elapsed_ns,
            row.process_cpu_ns,
            row.file_count,
            row.source_bytes,
            row.included,
        )
        .map_err(|error| format!("failed to format sample: {error}"))?;
    }
    fs_err::write(output, text)
        .map_err(|error| format!("failed to write {}: {error}", output.display()))
}

fn print_summaries(rows: &[RawRow], phase: Phase) -> Result<(), String> {
    for metric in Metric::ALL {
        let blocks = block_samples(rows, phase.label(), metric)?;
        let rust = blocks
            .iter()
            .map(|block| block.rust_mean_ns)
            .collect::<Vec<_>>();
        let cpython = blocks
            .iter()
            .map(|block| block.cpython_mean_ns)
            .collect::<Vec<_>>();
        let rust_summary = summarize(&rust)?;
        let cpython_summary = summarize(&cpython)?;
        let ratio_statistics = ratio_statistics(&blocks)?;
        println!(
            "metric={} phase={} engine=rust median_ns={:.0} p95_ns={} mad_ns={:.0} relative_mad={:.6} half_delta={:.6}",
            metric.label(),
            phase.label(),
            rust_summary.median,
            rust_summary.p95.round(),
            rust_summary.mad,
            rust_summary.relative_mad,
            rust_summary.half_delta,
        );
        println!(
            "metric={} phase={} engine=cpython median_ns={:.0} p95_ns={} mad_ns={:.0} relative_mad={:.6} half_delta={:.6}",
            metric.label(),
            phase.label(),
            cpython_summary.median,
            cpython_summary.p95.round(),
            cpython_summary.mad,
            cpython_summary.relative_mad,
            cpython_summary.half_delta,
        );
        println!(
            "metric={} phase={} block_ratio_median={:.6} block_ratio_p95={:.6} block_ratio_mad={:.6} block_ratio_relative_mad={:.6} block_ratio_half_delta={:.6} bootstrap95_low={:.6} bootstrap95_high={:.6} abba_median={} baab_median={} order_delta={} stable={}",
            metric.label(),
            phase.label(),
            ratio_statistics.summary.median,
            ratio_statistics.summary.p95,
            ratio_statistics.summary.mad,
            ratio_statistics.summary.relative_mad,
            ratio_statistics.summary.half_delta,
            ratio_statistics.bootstrap_low,
            ratio_statistics.bootstrap_high,
            format_optional(ratio_statistics.abba_median),
            format_optional(ratio_statistics.baab_median),
            format_optional(ratio_statistics.order_delta),
            ratio_statistics.stable,
        );
    }
    Ok(())
}

fn block_samples(rows: &[RawRow], phase: &str, metric: Metric) -> Result<Vec<BlockSample>, String> {
    let mut grouped = BTreeMap::<usize, Vec<&RawRow>>::new();
    for row in rows.iter().filter(|row| row.included && row.phase == phase) {
        grouped.entry(row.block).or_default().push(row);
    }
    if grouped.is_empty() {
        return Err(format!("no measured blocks for {phase}"));
    }

    grouped
        .into_iter()
        .map(|(block, mut rows)| {
            rows.sort_by_key(|row| row.position);
            let expected_order = block_order(block);
            if rows.len() != expected_order.len() {
                return Err(format!(
                    "{phase} block {block} has {} observations, expected {}",
                    rows.len(),
                    expected_order.len()
                ));
            }
            let expected_label = block_order_label(block);
            for (index, (row, expected_engine)) in rows.iter().zip(expected_order).enumerate() {
                if row.position != index + 1
                    || row.order != expected_label
                    || row.engine != expected_engine.label()
                {
                    return Err(format!(
                        "{phase} block {block} has malformed position {}",
                        index + 1
                    ));
                }
            }
            let rust = rows
                .iter()
                .filter(|row| row.engine == Engine::Rust.label())
                .map(|row| metric.value(row) as f64)
                .collect::<Vec<_>>();
            let cpython = rows
                .iter()
                .filter(|row| row.engine == Engine::Cpython.label())
                .map(|row| metric.value(row) as f64)
                .collect::<Vec<_>>();
            if rust.len() != 2 || cpython.len() != 2 {
                return Err(format!(
                    "{phase} block {block} must contain two observations per engine"
                ));
            }
            let rust_mean_ns = rust.iter().sum::<f64>() / 2.0;
            let cpython_mean_ns = cpython.iter().sum::<f64>() / 2.0;
            Ok(BlockSample {
                block,
                order: expected_label,
                rust_mean_ns,
                cpython_mean_ns,
                ratio: rust_mean_ns / cpython_mean_ns,
            })
        })
        .collect()
}

fn ratio_statistics(blocks: &[BlockSample]) -> Result<RatioStatistics, String> {
    for (index, block) in blocks.iter().enumerate() {
        if block.block != index + 1 {
            return Err(format!(
                "measured blocks must be contiguous: expected {}, got {}",
                index + 1,
                block.block
            ));
        }
    }
    let ratios = blocks.iter().map(|block| block.ratio).collect::<Vec<_>>();
    let summary = summarize(&ratios)?;
    let (bootstrap_low, bootstrap_high) =
        bootstrap_median_ci(&ratios, BOOTSTRAP_SAMPLES, BOOTSTRAP_SEED)?;
    let abba = blocks
        .iter()
        .filter(|block| block.order == "ABBA")
        .map(|block| block.ratio)
        .collect::<Vec<_>>();
    let baab = blocks
        .iter()
        .filter(|block| block.order == "BAAB")
        .map(|block| block.ratio)
        .collect::<Vec<_>>();
    let abba_median = (!abba.is_empty()).then(|| median_f64(&abba)).transpose()?;
    let baab_median = (!baab.is_empty()).then(|| median_f64(&baab)).transpose()?;
    let order_delta = abba_median
        .zip(baab_median)
        .map(|(abba, baab)| relative_difference(abba, baab));
    let stable = summary.relative_mad <= 0.03
        && summary.half_delta <= 0.03
        && order_delta.is_some_and(|delta| delta <= 0.03);
    Ok(RatioStatistics {
        summary,
        bootstrap_low,
        bootstrap_high,
        abba_median,
        baab_median,
        order_delta,
        stable,
    })
}

fn summarize(values: &[f64]) -> Result<Summary, String> {
    if values.is_empty() {
        return Err("cannot summarize an empty sample".to_string());
    }
    let median = median_f64(values)?;
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let p95_index = ((95 * sorted.len()).div_ceil(100)).saturating_sub(1);
    let p95 = sorted[p95_index];
    let deviations = values
        .iter()
        .map(|value| (*value - median).abs())
        .collect::<Vec<_>>();
    let mad = median_f64(&deviations)?;
    let (first, second) = if values.len() == 1 {
        let value = values[0];
        (value, value)
    } else {
        let middle = values.len().div_ceil(2);
        (
            median_f64(&values[..middle])?,
            median_f64(&values[middle..])?,
        )
    };
    let half_delta = relative_difference(first, second);
    Ok(Summary {
        median,
        p95,
        mad,
        relative_mad: mad / median,
        half_delta,
    })
}

fn relative_difference(left: f64, right: f64) -> f64 {
    if left == 0.0 || right == 0.0 {
        0.0
    } else {
        left.max(right) / left.min(right) - 1.0
    }
}

fn format_optional(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_string(), |value| format!("{value:.6}"))
}

fn median_f64(values: &[f64]) -> Result<f64, String> {
    if values.is_empty() {
        return Err("cannot compute median of an empty sample".to_string());
    }
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        Ok(f64::midpoint(values[middle - 1], values[middle]))
    } else {
        Ok(values[middle])
    }
}

fn bootstrap_median_ci(
    ratios: &[f64],
    repetitions: usize,
    seed: u64,
) -> Result<(f64, f64), String> {
    if ratios.is_empty() || repetitions == 0 {
        return Err("bootstrap requires ratios and repetitions".to_string());
    }
    let mut state = seed;
    let mut medians = Vec::with_capacity(repetitions);
    let mut sample = Vec::with_capacity(ratios.len());
    for _ in 0..repetitions {
        sample.clear();
        for _ in 0..ratios.len() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            sample.push(ratios[state as usize % ratios.len()]);
        }
        medians.push(median_f64(&sample)?);
    }
    medians.sort_by(f64::total_cmp);
    let low = ((repetitions * 25).div_ceil(1000)).saturating_sub(1);
    let high = ((repetitions * 975).div_ceil(1000)).saturating_sub(1);
    Ok((medians[low], medians[high]))
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::PathBuf;
    use std::time::Duration;

    use super::{
        BOOTSTRAP_SEED, BlockSample, Engine, Metric, Options, Phase, RawRow, SourceFile, Timings,
        block_order, block_order_label, block_samples, bootstrap_median_ci, cooldown_metadata,
        expected_row_count, parse_options_from, parse_sample_response, process_timer_result,
        ratio_statistics, summarize, write_raw,
    };

    fn raw_row(
        block: usize,
        position: usize,
        engine: Engine,
        wall_elapsed_ns: u64,
        process_cpu_ns: u64,
    ) -> RawRow {
        RawRow {
            phase: "compiler_core",
            block,
            position,
            order: block_order_label(block),
            kind: "measured",
            first_engine: block_order(block)[0].label(),
            engine: engine.label(),
            cooldown_ms: 0,
            wall_elapsed_ns,
            process_cpu_ns,
            file_count: 2,
            source_bytes: 20,
            included: true,
        }
    }

    #[test]
    fn parses_required_options_and_limit() {
        let options = parse_options_from([
            "--python",
            "/opt/homebrew/bin/python3",
            "--phase",
            "compiler_core",
            "--warmups",
            "5",
            "--samples",
            "21",
            "--cooldown-ms",
            "5000",
            "--output",
            "samples.tsv",
            "--limit",
            "10",
            "corpus",
        ])
        .expect("valid options");
        assert_eq!(options.phase, Phase::CompilerCore);
        assert_eq!(options.warmups, 5);
        assert_eq!(options.samples, 21);
        assert_eq!(options.cooldown, Duration::from_millis(5000));
        assert_eq!(options.limit, Some(10));
        assert_eq!(options.roots.len(), 1);
    }

    #[test]
    fn parses_each_supported_phase() {
        for (label, phase) in [
            ("compiler_core", Phase::CompilerCore),
            ("compile_marshal", Phase::CompileMarshal),
            ("marshal_only", Phase::MarshalOnly),
        ] {
            let options = parse_options_from([
                "--python",
                "python3",
                "--phase",
                label,
                "--warmups",
                "1",
                "--samples",
                "1",
                "--cooldown-ms",
                "0",
                "--output",
                "samples.tsv",
                "corpus",
            ])
            .expect("supported phase");
            assert_eq!(options.phase, phase);
        }
    }

    #[test]
    fn requires_a_supported_phase() {
        let missing = parse_options_from([
            "--python",
            "python3",
            "--warmups",
            "1",
            "--samples",
            "1",
            "--cooldown-ms",
            "0",
            "--output",
            "samples.tsv",
            "corpus",
        ])
        .expect_err("phase is required");
        assert_eq!(missing, "--phase is required");

        let unsupported = parse_options_from([
            "--python",
            "python3",
            "--phase",
            "all",
            "--warmups",
            "1",
            "--samples",
            "1",
            "--cooldown-ms",
            "0",
            "--output",
            "samples.tsv",
            "corpus",
        ])
        .expect_err("unsupported phase must fail");
        assert!(unsupported.contains("compiler_core, compile_marshal, or marshal_only"));
    }

    #[test]
    fn rejects_zero_samples() {
        let error = parse_options_from([
            "--python",
            "python3",
            "--phase",
            "compiler_core",
            "--warmups",
            "1",
            "--samples",
            "0",
            "--cooldown-ms",
            "0",
            "--output",
            "samples.tsv",
            "corpus",
        ])
        .expect_err("zero samples must fail");
        assert!(error.contains("positive number"));
    }

    #[test]
    fn requires_cooldown() {
        let error = parse_options_from([
            "--python",
            "python3",
            "--phase",
            "compiler_core",
            "--warmups",
            "1",
            "--samples",
            "1",
            "--output",
            "samples.tsv",
            "corpus",
        ])
        .expect_err("cooldown is required");
        assert!(error.contains("--cooldown-ms is required"));
    }

    #[test]
    fn records_cooldown_metadata() {
        let options = Options {
            python: "python3".to_string(),
            phase: Phase::CompilerCore,
            warmups: 1,
            samples: 1,
            cooldown: Duration::from_millis(5000),
            output: PathBuf::from("samples.tsv"),
            roots: vec![PathBuf::from("corpus")],
            limit: None,
        };
        assert_eq!(
            cooldown_metadata(&options),
            ("cooldown_ms".to_string(), "5000".to_string())
        );
    }

    #[test]
    fn counts_rows_for_exactly_one_phase() {
        let options = Options {
            python: "python3".to_string(),
            phase: Phase::MarshalOnly,
            warmups: 5,
            samples: 21,
            cooldown: Duration::ZERO,
            output: PathBuf::from("samples.tsv"),
            roots: vec![PathBuf::from("corpus")],
            limit: None,
        };
        assert_eq!(expected_row_count(&options), Ok(104));
    }

    #[test]
    fn parses_dual_clock_response() {
        let sample =
            parse_sample_response("RESULT\t120\t90\t2\t42").expect("valid dual-clock response");
        assert_eq!(
            sample.timings,
            Timings {
                wall_elapsed_ns: 120,
                process_cpu_ns: 90,
            }
        );
        assert_eq!(sample.count, 2);
        assert_eq!(sample.observed, 42);

        let error = parse_sample_response("RESULT\t120\t2\t42")
            .expect_err("response without process CPU time must fail");
        assert!(error.contains("invalid CPython sample response"));
    }

    #[test]
    fn propagates_process_timer_errors() {
        let error =
            process_timer_result::<Duration>(Err(io::Error::other("clock unavailable")), "stop")
                .expect_err("timer error must propagate");
        assert_eq!(error, "failed to stop process CPU timer: clock unavailable");
    }

    #[test]
    fn summarizes_nearest_rank_p95_and_half_drift() {
        let summary = summarize(&[
            100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0, 200.0, 201.0, 202.0,
        ])
        .expect("non-empty sample");
        assert_eq!(summary.median, 105.0);
        assert_eq!(summary.p95, 202.0);
        assert!(summary.mad > 0.0);
        assert!(summary.half_delta > 0.5);
    }

    #[test]
    fn alternates_abba_and_baab_blocks() {
        assert_eq!(
            block_order(1),
            [Engine::Rust, Engine::Cpython, Engine::Cpython, Engine::Rust]
        );
        assert_eq!(
            block_order(2),
            [Engine::Cpython, Engine::Rust, Engine::Rust, Engine::Cpython]
        );
    }

    #[test]
    fn aggregates_two_observations_per_engine() {
        let rows = vec![
            raw_row(1, 1, Engine::Rust, 100, 70),
            raw_row(1, 2, Engine::Cpython, 80, 60),
            raw_row(1, 3, Engine::Cpython, 100, 70),
            raw_row(1, 4, Engine::Rust, 120, 90),
        ];
        let wall = block_samples(&rows, "compiler_core", Metric::Wall).expect("valid block");
        assert_eq!(wall.len(), 1);
        assert_eq!(wall[0].rust_mean_ns, 110.0);
        assert_eq!(wall[0].cpython_mean_ns, 90.0);
        assert_eq!(wall[0].ratio, 110.0 / 90.0);

        let process_cpu =
            block_samples(&rows, "compiler_core", Metric::ProcessCpu).expect("valid block");
        assert_eq!(process_cpu[0].rust_mean_ns, 80.0);
        assert_eq!(process_cpu[0].cpython_mean_ns, 65.0);
        assert_eq!(process_cpu[0].ratio, 80.0 / 65.0);
    }

    #[test]
    fn selects_each_clock_from_raw_rows() {
        let row = raw_row(1, 1, Engine::Rust, 120, 90);
        assert_eq!(Metric::Wall.value(&row), 120);
        assert_eq!(Metric::ProcessCpu.value(&row), 90);
    }

    #[test]
    fn rejects_a_malformed_block() {
        let rows = vec![
            raw_row(1, 1, Engine::Rust, 100, 90),
            raw_row(1, 2, Engine::Cpython, 80, 70),
            raw_row(1, 3, Engine::Cpython, 100, 90),
        ];
        let error = block_samples(&rows, "compiler_core", Metric::ProcessCpu)
            .expect_err("block is incomplete");
        assert!(error.contains("3 observations, expected 4"));
    }

    #[test]
    fn checks_ratio_drift_and_order_strata() {
        let stable = (1..=21)
            .map(|block| {
                let ratio = if block % 2 == 1 { 1.001 } else { 0.999 };
                BlockSample {
                    block,
                    order: block_order_label(block),
                    rust_mean_ns: ratio * 100.0,
                    cpython_mean_ns: 100.0,
                    ratio,
                }
            })
            .collect::<Vec<_>>();
        let statistics = ratio_statistics(&stable).expect("valid ratio statistics");
        assert!(statistics.stable);
        assert!(statistics.summary.relative_mad <= 0.03);
        assert!(statistics.summary.half_delta <= 0.03);
        assert!(statistics.order_delta.is_some_and(|delta| delta <= 0.03));

        let unstable = (1..=21)
            .map(|block| {
                let ratio = if block % 2 == 1 { 1.0 } else { 1.1 };
                BlockSample {
                    block,
                    order: block_order_label(block),
                    rust_mean_ns: ratio * 100.0,
                    cpython_mean_ns: 100.0,
                    ratio,
                }
            })
            .collect::<Vec<_>>();
        assert!(
            !ratio_statistics(&unstable)
                .expect("valid ratio statistics")
                .stable
        );
    }

    #[test]
    fn bootstrap_is_deterministic() {
        let ratios = [0.8, 0.9, 1.0, 1.1, 1.2];
        let first =
            bootstrap_median_ci(&ratios, 1_000, BOOTSTRAP_SEED).expect("valid bootstrap sample");
        let second =
            bootstrap_median_ci(&ratios, 1_000, BOOTSTRAP_SEED).expect("valid bootstrap sample");
        assert_eq!(first, second);
        assert!(first.0 <= 1.0 && first.1 >= 1.0);
    }

    #[test]
    fn serializes_both_clocks() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let output = directory.path().join("samples.tsv");
        let source = SourceFile {
            path: PathBuf::from("source.py"),
            filename: "source.py".to_string(),
            source: "pass\n".to_string(),
        };
        write_raw(
            &output,
            &[(
                "primary_attribution_metric".to_string(),
                "process_cpu_ns".to_string(),
            )],
            &[&source],
            &[raw_row(1, 1, Engine::Rust, 120, 90)],
        )
        .expect("serialize raw observations");
        let text = fs_err::read_to_string(output).expect("read raw observations");
        assert_eq!(
            text,
            concat!(
                "# metadata\tprimary_attribution_metric\tprocess_cpu_ns\n",
                "# accepted\tsource.py\n",
                "phase\tblock\tposition\torder\tkind\tfirst_engine\tengine\t",
                "cooldown_ms\twall_elapsed_ns\tprocess_cpu_ns\tfile_count\t",
                "source_bytes\tincluded\n",
                "compiler_core\t1\t1\tABBA\tmeasured\trust\trust\t0\t120\t90\t",
                "2\t20\ttrue\n",
            )
        );
    }
}
