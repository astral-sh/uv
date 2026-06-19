use uv_static::EnvVars;

/// Checks if line wrapping should be enabled.
///
/// Returns `false` if `UV_NO_WRAP` is set.
fn should_wrap_lines() -> bool {
    std::env::var_os(EnvVars::UV_NO_WRAP).is_none()
}

/// Gets the terminal width for wrapping.
///
/// Uses `width_override`, then the `COLUMNS` environment variable, and finally attempts to detect
/// the width from the terminal. Returns `None` if no width is available.
pub(crate) fn get_wrap_width(width_override: Option<usize>) -> Option<usize> {
    if !should_wrap_lines() {
        return None;
    }

    if let Some(width) = width_override {
        return Some(width);
    }

    if let Ok(cols) = std::env::var(EnvVars::COLUMNS) {
        if let Ok(width) = cols.parse::<usize>() {
            return Some(width);
        }
    }

    if let Some((terminal_size::Width(width), _)) = terminal_size::terminal_size() {
        return Some(width as usize);
    }

    None
}

/// Wraps text at word boundaries with proper indentation.
///
/// Based on miette's `wrap()` implementation from:
/// <https://github.com/zkat/miette/blob/v7.2.0/src/handlers/graphical.rs#L876-L909>
pub(crate) fn wrap_text(
    text: &str,
    width: Option<usize>,
    initial_indent: &str,
    subsequent_indent: &str,
) -> String {
    let Some(width) = width else {
        return format!("{initial_indent}{text}");
    };

    let options = textwrap::Options::new(width)
        .initial_indent(initial_indent)
        .subsequent_indent(subsequent_indent)
        .break_words(false)
        .word_separator(textwrap::WordSeparator::AsciiSpace)
        .word_splitter(textwrap::WordSplitter::NoHyphenation);

    let mut wrapped = String::with_capacity(text.len());

    for (index, line) in text.split_terminator('\n').enumerate() {
        if index > 0 {
            wrapped.push('\n');
        }

        if line.is_empty() {
            continue;
        }

        // Preserve authored line breaks and wrap each line independently. Hanging indentation is
        // only for continuation lines created by wrapping, not for lines already in the message.
        wrapped.push_str(&textwrap::fill(
            line,
            if index == 0 {
                options.clone()
            } else {
                options.clone().initial_indent("")
            },
        ));
    }

    wrapped
}
