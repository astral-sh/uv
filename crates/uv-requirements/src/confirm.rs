use anyhow::Result;
use console::{style, Key, Term};

/// Prompt the user for confirmation in the given [`Term`].
///
/// This is a slimmed-down version of `dialoguer::Confirm`, with the post-confirmation report
/// enabled.
pub(crate) fn confirm(message: &str, term: &Term, default: bool) -> Result<bool> {
    // Set the Ctrl-C handler to exit the process.
    let result = ctrlc::set_handler(move || {
        let term = Term::stderr();
        term.show_cursor().ok();
        term.flush().ok();

        #[allow(clippy::exit, clippy::cast_possible_wrap)]
        std::process::exit(if cfg!(windows) {
            0xC000_013A_u32 as i32
        } else {
            130
        });
    });

    match result {
        Ok(()) => {}
        Err(ctrlc::Error::MultipleHandlers) => {
            // If multiple handlers were set, we assume that the existing handler is our
            // confirmation handler, and continue.
        }
        Err(e) => return Err(e.into()),
    }

    let prompt = format!(
        "{} {} {} {} {}",
        style("?".to_string()).for_stderr().yellow(),
        style(message).for_stderr().bold(),
        style("[y/n]").for_stderr().black().bright(),
        style("›").for_stderr().black().bright(),
        style(if default { "yes" } else { "no" })
            .for_stderr()
            .cyan(),
    );

    term.write_str(&prompt)?;
    term.hide_cursor()?;
    term.flush()?;

    // Match continuously on every keystroke, and do not wait for user to hit the
    // `Enter` key.
    let response = loop {
        let input = term.read_key()?;
        match input {
            Key::Char('y' | 'Y') => break true,
            Key::Char('n' | 'N') => break false,
            Key::Enter => break default,
            _ => {}
        };
    };

    let report = format!(
        "{} {} {} {}",
        style("✔".to_string()).for_stderr().green(),
        style(message).for_stderr().bold(),
        style("·").for_stderr().black().bright(),
        style(if response { "yes" } else { "no" })
            .for_stderr()
            .cyan(),
    );

    term.clear_line()?;
    term.write_line(&report)?;
    term.show_cursor()?;
    term.flush()?;

    Ok(response)
}
