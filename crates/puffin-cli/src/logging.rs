use anyhow::Result;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};
use tracing_tree::time::Uptime;

pub(crate) fn setup_logging() -> Result<()> {
    let subscriber = Registry::default().with(
        tracing_tree::HierarchicalLayer::default()
            .with_indent_lines(true)
            .with_indent_amount(2)
            .with_bracketed_fields(true)
            .with_targets(true)
            .with_writer(|| Box::new(std::io::stderr()))
            .with_timer(Uptime::default())
            .with_filter(EnvFilter::from_default_env()),
    );
    tracing::subscriber::set_global_default(subscriber)?;

    Ok(())
}
