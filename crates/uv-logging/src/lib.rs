use std::fmt;

use jiff::Timestamp;
use owo_colors::OwoColorize;
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

/// The style of a uv logging line.
pub struct UvFormat {
    pub display_timestamp: bool,
    pub display_level: bool,
    pub show_spans: bool,
}

impl Default for UvFormat {
    /// Regardless of the tracing level, show messages without any adornment.
    fn default() -> Self {
        Self {
            display_timestamp: false,
            display_level: true,
            show_spans: false,
        }
    }
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
