use std::str::FromStr;

use anyhow::Context;
#[cfg(feature = "tracing-durations-export")]
use tracing_durations_export::{
    DurationsLayer, DurationsLayerBuilder, DurationsLayerDropGuard, plot::PlotConfig,
};
use tracing_subscriber::filter::Directive;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};
use tracing_tree::HierarchicalLayer;
use tracing_tree::time::Uptime;

use uv_cli::ColorChoice;
use uv_logging::UvFormat;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    #[default]
    Off,
    DebugUv,
    TraceUv,
    TraceAll,
}

/// Configure `tracing` based on the given [`Level`], taking into account the `RUST_LOG` environment
/// variable.
///
/// The [`Level`] is used to dictate the default filters (which can be overridden by the `RUST_LOG`
/// environment variable) along with the formatting of the output. For example, [`Level::Verbose`]
/// includes targets and timestamps, along with all `uv=debug` messages by default.
pub(crate) fn setup_logging(
    level: Level,
    durations_layer: Option<impl Layer<Registry> + Send + Sync>,
    color: ColorChoice,
    detailed_logging: bool,
) -> anyhow::Result<()> {
    // We use directives here to ensure `RUST_LOG` can override them
    let default_directive = match level {
        Level::Off => {
            // Show nothing
            tracing::level_filters::LevelFilter::OFF.into()
        }
        Level::DebugUv => {
            // Show `DEBUG` messages from the CLI crate (and ERROR/WARN/INFO)
            Directive::from_str("uv=debug").unwrap()
        }
        Level::TraceUv => {
            // Show `TRACE` messages from the CLI crate (and ERROR/WARN/INFO/DEBUG)
            Directive::from_str("uv=trace").unwrap()
        }
        Level::TraceAll => {
            // Show all `TRACE` messages (and ERROR/WARN/INFO/DEBUG)
            Directive::from_str("trace").unwrap()
        }
    };

    // Avoid setting the default log level to INFO
    let durations_layer = durations_layer.map(|durations_layer| {
        durations_layer.with_filter(
            // Only record our own spans
            tracing_subscriber::filter::Targets::new()
                .with_target("", tracing::level_filters::LevelFilter::INFO),
        )
    });
    let filter = EnvFilter::builder()
        .with_default_directive(default_directive)
        .from_env()
        .context("Invalid RUST_LOG directives")?;

    // Determine our final color settings and create an anstream wrapper based on it.
    //
    // The tracing `with_ansi` function on affects color tracing adds *on top of* the
    // log messages. This means that if we `debug!("{}", "hello".green())`,
    // (a thing we absolutely do throughout uv), then there will still be color
    // in the logs, which is undesirable.
    //
    // So we tell tracing to print to an anstream wrapper around stderr that force-strips ansi.
    // Given we do this, using `with_ansi` at all is arguably pointless, but it feels morally
    // correct to still do it? I don't know what would break if we didn't... but why find out?
    let (ansi, color_choice) =
        match color.and_colorchoice(anstream::Stderr::choice(&std::io::stderr())) {
            ColorChoice::Always => (true, anstream::ColorChoice::Always),
            ColorChoice::Never => (false, anstream::ColorChoice::Never),
            ColorChoice::Auto => unreachable!("anstream can't return auto as choice"),
        };
    let writer = std::sync::Mutex::new(anstream::AutoStream::new(std::io::stderr(), color_choice));

    if detailed_logging {
        // Regardless of the tracing level, include the uptime and target for each message.
        tracing_subscriber::registry()
            .with(durations_layer)
            .with(
                HierarchicalLayer::default()
                    .with_targets(true)
                    .with_timer(Uptime::default())
                    .with_writer(writer)
                    .with_ansi(ansi)
                    .with_filter(filter),
            )
            .init();
    } else {
        tracing_subscriber::registry()
            .with(durations_layer)
            .with(
                tracing_subscriber::fmt::layer()
                    .event_format(UvFormat::default())
                    .with_writer(writer)
                    .with_ansi(ansi)
                    .with_filter(filter),
            )
            .init();
    }

    Ok(())
}

/// Setup the `TRACING_DURATIONS_FILE` environment variable to enable tracing durations.
#[cfg(feature = "tracing-durations-export")]
pub(crate) fn setup_durations(
    tracing_durations_file: Option<&std::path::PathBuf>,
) -> anyhow::Result<(
    Option<DurationsLayer<Registry>>,
    Option<DurationsLayerDropGuard>,
)> {
    if let Some(location) = tracing_durations_file {
        if let Some(parent) = location.parent() {
            fs_err::create_dir_all(parent)
                .context("Failed to create parent of TRACING_DURATIONS_FILE")?;
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
            .durations_file(location)
            .plot_file(location.with_extension("svg"))
            .plot_config(plot_config)
            .build()
            .context("Couldn't create TRACING_DURATIONS_FILE files")?;
        Ok((Some(layer), Some(guard)))
    } else {
        Ok((None, None))
    }
}
