use std::fmt;

use owo_colors::OwoColorize;
use tracing::{Event, Subscriber, field::Field};
use tracing_subscriber::field::MakeExt;
use tracing_subscriber::fmt::format::{self, Writer};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

/// The style of a uv logging line.
#[derive(Default)]
pub struct UvFormat;

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

        ctx.field_format().format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}

/// Return the field formatter for uv logging.
///
/// The event formatter is responsible for uv's own log colors, such as the level prefix. Field
/// values can come from arbitrary `Display` or `Debug` implementations, so strip any ANSI escape
/// sequences there before writing them to the log line.
pub fn uv_fields() -> impl for<'writer> FormatFields<'writer> {
    format::debug_fn(format_field)
        .display_messages()
        .delimited(" ")
}

fn format_field(writer: &mut Writer<'_>, field: &Field, value: &dyn fmt::Debug) -> fmt::Result {
    // NOTE: The various cases in this function match tracing-subscriber's default field formatting.
    // See: https://docs.rs/tracing-subscriber/0.3.23/src/tracing_subscriber/fmt/format/mod.rs.html#1303-1338

    let field = field.name();
    if field.starts_with("log.") {
        return Ok(());
    }

    let value = format!("{value:?}");
    let value = anstream::adapter::strip_str(&value);

    if field == "message" {
        write!(writer, "{value}")
    } else {
        write!(
            writer,
            "{}={value}",
            // Render `type=...` instead of `r#type=...`.
            field.strip_prefix("r#").unwrap_or(field)
        )
    }
}

#[cfg(test)]
mod tests {
    use tracing::{Callsite, Event, Level, field::Value, metadata::Kind};
    use tracing_subscriber::fmt::FormatFields;
    use tracing_subscriber::fmt::format::Writer;

    use super::uv_fields;

    #[test]
    fn strips_ansi_from_message_fields() {
        let callsite = tracing::callsite! {
            name: "event",
            kind: Kind::EVENT,
            level: Level::TRACE,
            fields: message
        };
        let metadata = callsite.metadata();
        let message = format_args!("Error trace: {}", "\x1b[36m\x1b[1mhint\x1b[0m");
        let values = [Some(&message as &dyn Value)];
        let fields = metadata.fields().value_set_all(&values);
        let event = Event::new(metadata, &fields);
        let mut output = String::new();

        uv_fields()
            .format_fields(Writer::new(&mut output), event)
            .expect("field formatting should succeed");

        assert_eq!(output, "Error trace: hint");
    }
}
