use tracing::level_filters::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;
use tracing_tree::time::Uptime;
use tracing_tree::HierarchicalLayer;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    /// Suppress all tracing output by default (overrideable by `RUST_LOG`).
    #[default]
    Default,
    /// Show debug messages by default (overrideable by `RUST_LOG`).
    Verbose,
}

/// Configure `tracing` based on the given [`Level`], taking into account the `RUST_LOG` environment
/// variable.
///
/// The [`Level`] is used to dictate the default filters (which can be overridden by the `RUST_LOG`
/// environment variable) along with the formatting of the output. For example, [`Level::Verbose`]
/// includes targets and timestamps, along with all `puffin=debug` messages by default.
pub(crate) fn setup_logging(level: Level) {
    match level {
        Level::Default => {
            // Show nothing, but allow `RUST_LOG` to override.
            let filter = EnvFilter::builder()
                .with_default_directive(LevelFilter::OFF.into())
                .from_env_lossy();

            // Regardless of the tracing level, show messages without any adornment.
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    tracing_subscriber::fmt::layer()
                        .without_time()
                        .with_target(false)
                        .with_writer(std::io::sink),
                )
                .init();
        }
        Level::Verbose => {
            // Show `DEBUG` messages from the CLI crate, but allow `RUST_LOG` to override.
            let filter = EnvFilter::try_from_default_env()
                .or_else(|_| EnvFilter::try_new("puffin=debug"))
                .unwrap();

            // Regardless of the tracing level, include the uptime and target for each message.
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    HierarchicalLayer::default()
                        .with_targets(true)
                        .with_timer(Uptime::default())
                        .with_writer(std::io::stderr),
                )
                .init();
        }
    }
}
