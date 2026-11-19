#![cfg(feature = "render")]

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use clap::Parser;
use poloto::build;
use resvg::usvg::fontdb;
use serde::Deserialize;
use tagu::prelude::*;

#[derive(Parser)]
pub(crate) struct RenderBenchmarksArgs {
    /// Path to a JSON output from a `hyperfine` benchmark.
    path: PathBuf,
    /// Title of the plot.
    #[clap(long, short)]
    title: Option<String>,
}

pub(crate) fn render_benchmarks(args: &RenderBenchmarksArgs) -> Result<()> {
    let mut results: BenchmarkResults = serde_json::from_slice(&fs_err::read(&args.path)?)?;
    assert_eq!(
        results.schema_version, 2,
        "unsupported hyperfine JSON schema version"
    );

    // Each result measures the same workload (e.g., resolve-warm or install-cold),
    // so skip that and only use the tool name as the plot label.
    for result in &mut results.results {
        let benchmark_name = result.name.as_deref().unwrap_or(&result.command);
        result.command = if benchmark_name.starts_with("uv") {
            "uv"
        } else if benchmark_name.starts_with("pip-compile") {
            "pip-compile"
        } else if benchmark_name.starts_with("pip-sync") {
            "pip-sync"
        } else if benchmark_name.starts_with("poetry") {
            "Poetry"
        } else if benchmark_name.starts_with("pdm") {
            "PDM"
        } else {
            bail!("could not find command name in benchmark '{benchmark_name}'");
        }
        .into();
    }

    let fontdb = load_fonts();

    render_to_png(
        &plot_benchmark(args.title.as_deref().unwrap_or("Benchmark"), &results)?,
        &args.path.with_extension("png"),
        fontdb,
    )?;

    Ok(())
}

/// Render a benchmark to an SVG (as a string).
fn plot_benchmark(heading: &str, results: &BenchmarkResults) -> Result<String> {
    let mut data = Vec::new();
    for result in &results.results {
        assert_eq!(result.summary.time_wall_clock.unit, "second");
        data.push((result.summary.time_wall_clock.mean, &result.command));
    }

    let theme = poloto::render::Theme::light();
    let theme = theme.append(tagu::build::raw(
        ".poloto0.poloto_fill{fill: #6340AC !important;}",
    ));
    let theme = theme.append(tagu::build::raw(
        ".poloto_background{fill: white !important;}",
    ));

    Ok(build::bar::gen_simple("", data, [0.0])
        .label((heading, "Time (s)", ""))
        .append_to(poloto::header().append(theme))
        .render_string()?)
}

/// Render an SVG to a PNG file.
fn render_to_png(data: &str, path: &Path, fontdb: fontdb::Database) -> Result<()> {
    // Create options with the font database for text rendering
    let mut opt = resvg::usvg::Options::default();
    *opt.fontdb_mut() = fontdb;

    let tree = resvg::usvg::Tree::from_str(data, &opt)?;

    // Calculate scale to fit width of 1600 while maintaining aspect ratio
    let target_width = 1600_u32;
    let original_size = tree.size().to_int_size();
    let scaled_size = original_size
        .scale_to_width(target_width)
        .ok_or_else(|| anyhow!("failed to scale to target width"))?;
    #[expect(
        clippy::cast_precision_loss,
        reason = "Acceptable precision loss for render scale calculation"
    )]
    let scale = target_width as f32 / original_size.width() as f32;

    let mut pixmap = resvg::tiny_skia::Pixmap::new(scaled_size.width(), scaled_size.height())
        .ok_or_else(|| anyhow!("failed to create pixmap"))?;

    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    if let Some(parent) = path.parent() {
        fs_err::create_dir_all(parent)?;
    }
    pixmap.save_png(path)?;
    Ok(())
}

/// Load the system fonts and set the default font families.
fn load_fonts() -> fontdb::Database {
    let mut fontdb = fontdb::Database::new();
    fontdb.load_system_fonts();
    fontdb.set_serif_family("Times New Roman");
    fontdb.set_sans_serif_family("Arial");
    fontdb.set_cursive_family("Comic Sans MS");
    fontdb.set_fantasy_family("Impact");
    fontdb.set_monospace_family("Courier New");

    fontdb
}

#[derive(Debug, Deserialize)]
struct BenchmarkResults {
    schema_version: u64,
    results: Vec<BenchmarkResult>,
}

#[derive(Debug, Deserialize)]
struct BenchmarkResult {
    command: String,
    name: Option<String>,
    summary: BenchmarkSummary,
}

#[derive(Debug, Deserialize)]
struct BenchmarkSummary {
    time_wall_clock: MetricSummary,
}

#[derive(Debug, Deserialize)]
struct MetricSummary {
    unit: String,
    mean: f64,
}
