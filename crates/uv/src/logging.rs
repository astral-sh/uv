use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Context;
use jiff::Timestamp;
use owo_colors::OwoColorize;
use tracing::{Event, Subscriber};
#[cfg(feature = "tracing-durations-export")]
use tracing_durations_export::{
    plot::PlotConfig, DurationsLayer, DurationsLayerBuilder, DurationsLayerDropGuard,
};
use tracing_subscriber::filter::Directive;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};
use tracing_tree::time::Uptime;
use tracing_tree::HierarchicalLayer;

use uv_cli::ColorChoice;
use uv_static::EnvVars;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    #[default]
    Off,
    DebugUv,
    TraceUv,
    TraceAll,
}

/// Enum to set the log level for the file logs
// Discuss if we need to separate trace or debug and the hierarchical layer or not into different args (based on what the use cases are)
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileLogLevel {
    /// Write debug messages to the log file.
    #[default]
    DebugUv,
    TraceUv,
    TraceAll,
}
struct UvFormat {
    display_timestamp: bool,
    display_level: bool,
    show_spans: bool,
}

/// See <https://docs.rs/tracing-subscriber/0.3.18/src/tracing_subscriber/fmt/format/mod.rs.html#1026-1156>
impl<S, N> FormatEvent<S, N> for UvFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        let ansi = writer.has_ansi_escapes();

        if self.display_timestamp {
            if ansi {
                write!(writer, "{} ", Timestamp::now().dimmed())?;
            } else {
                write!(writer, "{} ", Timestamp::now())?;
            }
        }

        if self.display_level {
            let level = meta.level();
            // Same colors as tracing
            if ansi {
                match *level {
                    tracing::Level::TRACE => write!(writer, "{} ", level.purple())?,
                    tracing::Level::DEBUG => write!(writer, "{} ", level.blue())?,
                    tracing::Level::INFO => write!(writer, "{} ", level.green())?,
                    tracing::Level::WARN => write!(writer, "{} ", level.yellow())?,
                    tracing::Level::ERROR => write!(writer, "{} ", level.red())?,
                }
            } else {
                write!(writer, "{level} ")?;
            }
        }

        if self.show_spans {
            let span = event.parent();
            let mut seen = false;

            let span = span
                .and_then(|id| ctx.span(id))
                .or_else(|| ctx.lookup_current());

            let scope = span.into_iter().flat_map(|span| span.scope().from_root());

            for span in scope {
                seen = true;
                if ansi {
                    write!(writer, "{}:", span.metadata().name().bold())?;
                } else {
                    write!(writer, "{}:", span.metadata().name())?;
                }
            }

            if seen {
                writer.write_char(' ')?;
            }
        }

        ctx.field_format().format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}

/// Configure `tracing` based on the given [`Level`], taking into account the `RUST_LOG` environment
/// variable.
///
/// The [`Level`] is used to dictate the default filters (which can be overridden by the `RUST_LOG`
/// environment variable) along with the formatting of the output. For example, [`Level::Verbose`]
/// includes targets and timestamps, along with all `uv=debug` messages by default.
pub(crate) fn setup_logging(
    level: Level,
    durations: impl Layer<Registry> + Send + Sync,
    color: ColorChoice,
    log_path: Option<&PathBuf>,
    file_log_level: FileLogLevel,
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

    // Only record our own spans.
    let durations_layer = durations.with_filter(
        tracing_subscriber::filter::Targets::new()
            .with_target("", tracing::level_filters::LevelFilter::INFO),
    );

    let subscriber = tracing_subscriber::registry().with(durations_layer);

    // Building the layers for logging sort of like a builder pattern
    let mut layers = Vec::new();

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

    let detailed_logging = std::env::var(EnvVars::UV_LOG_CONTEXT).is_ok();

    if detailed_logging {
        // Regardless of the tracing level, include the uptime and target for each message.
        layers.push(
            HierarchicalLayer::default()
                .with_targets(true)
                .with_timer(Uptime::default())
                .with_writer(writer)
                .with_ansi(ansi)
                .with_filter(filter)
                .boxed(),
        );
    } else {
        // Regardless of the tracing level, show messages without any adornment.
        let format = UvFormat {
            display_timestamp: false,
            display_level: true,
            show_spans: false,
        };
        layers.push(
            tracing_subscriber::fmt::layer()
                .event_format(format)
                .with_writer(writer)
                .with_ansi(ansi)
                .with_filter(filter)
                .boxed(),
        );
    }

    // If log path is provided the setup for persistent file logging is done
    if let Some(path) = log_path {
        // file_filter sets the level of logs by default debug logs are written to the file
        let file_directive = match file_log_level {
            FileLogLevel::DebugUv => "uv=debug",
            FileLogLevel::TraceUv => "uv=trace",
            FileLogLevel::TraceAll => "trace",
        };
        let file_filter =
            EnvFilter::try_new(file_directive).unwrap_or_else(|_| EnvFilter::new("uv=debug"));

        let mut new_path = path.clone();
        new_path.set_extension("log");

        // Discuss if previous content should be overwritten or appended.
        // Should it panic or gracefully exit just without logging in case of failure to open or create the file.
        let log_file = fs_err::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&new_path)
            .with_context(|| format!("{} {}", uv_static::LOG_FILE_ERROR, new_path.display()))?;

        // Forcing no anstream in file logs, I don't like the idea of using the same env variable NO_COLOR or cli flag to control both console and file logs
        // If there is a case to introduce color for file logs we can introduce a new env variable or cli flag for it.
        // fs_err doesn't implement RawStream so we Box it and then cast it to the trait std::io::Write, and Send is needed to be explicitly specified as Mutex needs to be shared between threads.
        let file_writer = std::sync::Mutex::new(anstream::AutoStream::new(
            Box::new(log_file) as Box<dyn std::io::Write + Send>,
            anstream::ColorChoice::Never,
        ));

        let detailed_file_logging = std::env::var(EnvVars::UV_FILE_LOG_CONTEXT).is_ok();

        // Depending on the log level, different layers are added to the subscriber.
        // An equivalent of `RUST_LOG` for file logs might be needed to be implemented.
        if detailed_file_logging {
            layers.push(
                HierarchicalLayer::default()
                    .with_targets(true)
                    .with_timer(Uptime::default())
                    .with_writer(file_writer)
                    .with_filter(file_filter)
                    .boxed(),
            );
        } else {
            let file_format = UvFormat {
                // Setting timestamp display as false as to mimic the behavior of the console logs
                // however wanted to discuss: the case where user might want to know when they wrote the logs
                display_timestamp: false,
                display_level: true,
                show_spans: false,
            };
            layers.push(
                tracing_subscriber::fmt::layer()
                    .event_format(file_format)
                    .with_writer(file_writer)
                    .with_filter(file_filter)
                    .boxed(),
            );
        }
    };

    subscriber.with(layers).init();

    Ok(())
}

/// Setup the `TRACING_DURATIONS_FILE` environment variable to enable tracing durations.
#[cfg(feature = "tracing-durations-export")]
pub(crate) fn setup_duration() -> anyhow::Result<(
    Option<DurationsLayer<Registry>>,
    Option<DurationsLayerDropGuard>,
)> {
    if let Ok(location) = std::env::var(EnvVars::TRACING_DURATIONS_FILE) {
        let location = std::path::PathBuf::from(location);
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
            .durations_file(&location)
            .plot_file(location.with_extension("svg"))
            .plot_config(plot_config)
            .build()
            .context("Couldn't create TRACING_DURATIONS_FILE files")?;
        Ok((Some(layer), Some(guard)))
    } else {
        Ok((None, None))
    }
}
