use std::env;
use std::fmt;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;
use std::time::Instant;

use owo_colors::AnsiColors;
use tracing::{debug, trace};
use tracing_durations_export::DurationsLayerBuilder;
use tracing_durations_export::plot::PlotConfig;
use tracing_subscriber::filter::Directive;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use uv_dev::run;
use uv_static::EnvVars;

struct Stderr;

impl fmt::Write for Stderr {
    fn write_str(&mut self, output: &str) -> fmt::Result {
        anstream::eprint!("{output}");
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let (duration_layer, _guard) = if let Ok(location) = env::var(EnvVars::TRACING_DURATIONS_FILE) {
        let location = PathBuf::from(location);
        if let Some(parent) = location.parent() {
            fs_err::tokio::create_dir_all(&parent)
                .await
                .expect("Failed to create parent of TRACING_DURATIONS_FILE");
        }
        let plot_config = PlotConfig {
            multi_lane: true,
            min_length: None,
            remove: Some(
                ["get_cached_with_callback".to_string()]
                    .into_iter()
                    .collect(),
            ),
            ..PlotConfig::default()
        };
        let (layer, guard) = DurationsLayerBuilder::default()
            .durations_file(&location)
            .plot_file(location.with_extension("svg"))
            .plot_config(plot_config)
            .build()
            .expect("Couldn't create TRACING_DURATIONS_FILE files");
        (Some(layer), Some(guard))
    } else {
        (None, None)
    };

    // Show `INFO` messages from the uv crate, but allow `RUST_LOG` to override.
    let default_directive = Directive::from_str("uv=info").unwrap();

    let filter = EnvFilter::builder()
        .with_default_directive(default_directive)
        .from_env()
        .expect("Valid RUST_LOG directives");

    tracing_subscriber::registry()
        .with(duration_layer)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(filter),
        )
        .init();

    let start = Instant::now();
    let result = run().await;
    debug!("Took {}ms", start.elapsed().as_millis());
    if let Err(err) = result {
        trace!("Error trace: {err:?}");
        let err = err.context("uv-dev failed");
        uv_errors::write_error_chain(
            err.as_ref(),
            Stderr,
            "error",
            AnsiColors::Red,
            uv_errors::Hints::none(),
            None,
        )
        .expect("writing to stderr should not fail");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
