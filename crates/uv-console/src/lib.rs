use console::{measure_text_width, style, Key, Term};
use std::{cmp::Ordering, iter};

/// Prompt the user for confirmation in the given [`Term`].
///
/// This is a slimmed-down version of `dialoguer::Confirm`, with the post-confirmation report
/// enabled.
pub fn confirm(message: &str, term: &Term, default: bool) -> std::io::Result<bool> {
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
        let input = term.read_key_raw()?;
        match input {
            Key::Char('y' | 'Y') => break true,
            Key::Char('n' | 'N') => break false,
            Key::Enter => break default,
            Key::CtrlC => {
                let term = Term::stderr();
                term.show_cursor()?;
                term.write_str("\n")?;
                term.flush()?;

                #[allow(clippy::exit, clippy::cast_possible_wrap)]
                std::process::exit(if cfg!(windows) {
                    0xC000_013A_u32 as i32
                } else {
                    130
                });
            }
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

/// Prompt the user for password in the given [`Term`].
///
/// This is a slimmed-down version of `dialoguer::Password`.
pub fn password(prompt: &str, term: &Term) -> std::io::Result<String> {
    term.write_str(prompt)?;
    term.show_cursor()?;
    term.flush()?;

    let input = term.read_secure_line()?;

    term.clear_line()?;

    Ok(input)
}

/// Prompt the user for input text in the given [`Term`].
///
/// This is a slimmed-down version of `dialoguer::Input`.
#[allow(
    // Suppress Clippy lints triggered by `dialoguer::Input`.
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
pub fn input(prompt: &str, term: &Term) -> std::io::Result<String> {
    term.write_str(prompt)?;
    term.show_cursor()?;
    term.flush()?;

    let prompt_len = measure_text_width(prompt);

    let mut chars: Vec<char> = Vec::new();
    let mut position = 0;
    loop {
        match term.read_key()? {
            Key::Backspace if position > 0 => {
                position -= 1;
                chars.remove(position);
                let line_size = term.size().1 as usize;
                // Case we want to delete last char of a line so the cursor is at the beginning of the next line
                if (position + prompt_len) % (line_size - 1) == 0 {
                    term.clear_line()?;
                    term.move_cursor_up(1)?;
                    term.move_cursor_right(line_size + 1)?;
                } else {
                    term.clear_chars(1)?;
                }

                let tail: String = chars[position..].iter().collect();

                if !tail.is_empty() {
                    term.write_str(&tail)?;

                    let total = position + prompt_len + tail.chars().count();
                    let total_line = total / line_size;
                    let line_cursor = (position + prompt_len) / line_size;
                    term.move_cursor_up(total_line - line_cursor)?;

                    term.move_cursor_left(line_size)?;
                    term.move_cursor_right((position + prompt_len) % line_size)?;
                }

                term.flush()?;
            }
            Key::Char(chr) if !chr.is_ascii_control() => {
                chars.insert(position, chr);
                position += 1;
                let tail: String = iter::once(&chr).chain(chars[position..].iter()).collect();
                term.write_str(&tail)?;
                term.move_cursor_left(tail.chars().count() - 1)?;
                term.flush()?;
            }
            Key::ArrowLeft if position > 0 => {
                if (position + prompt_len) % term.size().1 as usize == 0 {
                    term.move_cursor_up(1)?;
                    term.move_cursor_right(term.size().1 as usize)?;
                } else {
                    term.move_cursor_left(1)?;
                }
                position -= 1;
                term.flush()?;
            }
            Key::ArrowRight if position < chars.len() => {
                if (position + prompt_len) % (term.size().1 as usize - 1) == 0 {
                    term.move_cursor_down(1)?;
                    term.move_cursor_left(term.size().1 as usize)?;
                } else {
                    term.move_cursor_right(1)?;
                }
                position += 1;
                term.flush()?;
            }
            Key::UnknownEscSeq(seq) if seq == vec!['b'] => {
                let line_size = term.size().1 as usize;
                let nb_space = chars[..position]
                    .iter()
                    .rev()
                    .take_while(|c| c.is_whitespace())
                    .count();
                let find_last_space = chars[..position - nb_space]
                    .iter()
                    .rposition(|c| c.is_whitespace());

                // If we find a space we set the cursor to the next char else we set it to the beginning of the input
                if let Some(mut last_space) = find_last_space {
                    if last_space < position {
                        last_space += 1;
                        let new_line = (prompt_len + last_space) / line_size;
                        let old_line = (prompt_len + position) / line_size;
                        let diff_line = old_line - new_line;
                        if diff_line != 0 {
                            term.move_cursor_up(old_line - new_line)?;
                        }

                        let new_pos_x = (prompt_len + last_space) % line_size;
                        let old_pos_x = (prompt_len + position) % line_size;
                        let diff_pos_x = new_pos_x as i64 - old_pos_x as i64;
                        if diff_pos_x < 0 {
                            term.move_cursor_left(-diff_pos_x as usize)?;
                        } else {
                            term.move_cursor_right((diff_pos_x) as usize)?;
                        }
                        position = last_space;
                    }
                } else {
                    term.move_cursor_left(position)?;
                    position = 0;
                }

                term.flush()?;
            }
            Key::UnknownEscSeq(seq) if seq == vec!['f'] => {
                let line_size = term.size().1 as usize;
                let find_next_space = chars[position..].iter().position(|c| c.is_whitespace());

                // If we find a space we set the cursor to the next char else we set it to the beginning of the input
                if let Some(mut next_space) = find_next_space {
                    let nb_space = chars[position + next_space..]
                        .iter()
                        .take_while(|c| c.is_whitespace())
                        .count();
                    next_space += nb_space;
                    let new_line = (prompt_len + position + next_space) / line_size;
                    let old_line = (prompt_len + position) / line_size;
                    term.move_cursor_down(new_line - old_line)?;

                    let new_pos_x = (prompt_len + position + next_space) % line_size;
                    let old_pos_x = (prompt_len + position) % line_size;
                    let diff_pos_x = new_pos_x as i64 - old_pos_x as i64;
                    if diff_pos_x < 0 {
                        term.move_cursor_left(-diff_pos_x as usize)?;
                    } else {
                        term.move_cursor_right((diff_pos_x) as usize)?;
                    }
                    position += next_space;
                } else {
                    let new_line = (prompt_len + chars.len()) / line_size;
                    let old_line = (prompt_len + position) / line_size;
                    term.move_cursor_down(new_line - old_line)?;

                    let new_pos_x = (prompt_len + chars.len()) % line_size;
                    let old_pos_x = (prompt_len + position) % line_size;
                    let diff_pos_x = new_pos_x as i64 - old_pos_x as i64;
                    match diff_pos_x.cmp(&0) {
                        Ordering::Less => {
                            term.move_cursor_left((-diff_pos_x - 1) as usize)?;
                        }
                        Ordering::Equal => {}
                        Ordering::Greater => {
                            term.move_cursor_right((diff_pos_x) as usize)?;
                        }
                    }
                    position = chars.len();
                }

                term.flush()?;
            }
            Key::Enter => break,
            _ => (),
        }
    }
    let input = chars.iter().collect::<String>();
    term.write_line("")?;

    Ok(input)
}
