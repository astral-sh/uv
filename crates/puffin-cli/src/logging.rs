use anyhow::Result;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};
use tracing_tree::time::Uptime;

pub(crate) fn setup_logging() -> Result<()> {
    let targets = Targets::new()
        .with_target("hyper", LevelFilter::WARN)
        .with_target("reqwest", LevelFilter::WARN)
        .with_target("tokio", LevelFilter::WARN)
        .with_target("blocking", LevelFilter::OFF)
        .with_default(LevelFilter::TRACE);

    let subscriber = Registry::default().with(
        tracing_tree::HierarchicalLayer::default()
            .with_targets(true)
            .with_writer(|| Box::new(std::io::stderr()))
            .with_timer(Uptime::default())
            .with_filter(EnvFilter::from_default_env())
            .with_filter(targets),
    );
    tracing::subscriber::set_global_default(subscriber)?;

    Ok(())
}
