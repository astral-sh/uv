/// Reformat a TOML array to use multiline format.
pub(crate) fn format_multiline_array(dependencies: &mut toml_edit::Array) {
    if dependencies.is_empty() {
        dependencies.set_trailing("");
        return;
    }

    for item in dependencies.iter_mut() {
        let decor = item.decor_mut();
        let mut prefix = String::new();
        for comment in find_comments(decor.prefix()).chain(find_comments(decor.suffix())) {
            prefix.push_str("\n    ");
            prefix.push_str(comment);
        }
        prefix.push_str("\n    ");
        decor.set_prefix(prefix);
        decor.set_suffix("");
    }

    dependencies.set_trailing(&{
        let mut comments = find_comments(Some(dependencies.trailing())).peekable();
        let mut value = String::new();
        if comments.peek().is_some() {
            for comment in comments {
                value.push_str("\n    ");
                value.push_str(comment);
            }
        }
        value.push('\n');
        value
    });

    dependencies.set_trailing_comma(true);
}

/// Return an iterator over the comments in a raw string.
fn find_comments(raw_string: Option<&toml_edit::RawString>) -> impl Iterator<Item = &str> {
    raw_string
        .and_then(toml_edit::RawString::as_str)
        .unwrap_or("")
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            line.starts_with('#').then_some(line)
        })
}
