use toml_parser::decoder::Encoding;
use toml_parser::lexer::Token;
use toml_parser::parser::{EventReceiver, parse_document};
use toml_parser::{ErrorSink, Source, Span};

/// Detect TOML 1.1 specific features in a TOML document.
///
/// Note: This function does _not_ perform any validation.
pub fn has_toml11_features(source: &str) -> bool {
    let tokens: Box<[Token]> = Source::new(source).lex().collect();
    let mut checker = DetectToml11::new(source);
    let mut errors = None;
    parse_document(&tokens, &mut checker, &mut errors);
    checker.is_11()
}

/// Structure state in a TOML document
#[derive(Debug, Copy, Clone)]
enum State {
    /// Regular table (e.g. `[foo]`)
    StdTable,
    /// Array table (e.g. `[[foo]]`)
    ArrayTable,
    /// Inline table (e.g. `{ k = "v" }`
    InlineTable { trailing_sep: bool },
    /// Array (e.g. `[1, 2, 3]`)
    Array,
}

/// Detect TOML 1.1 specific features.
pub struct DetectToml11<'s> {
    /// The underlying TOML source
    source: &'s str,
    /// Current nesting state
    state: Vec<State>,
    /// Set to true when a TOML 1.1 specific feature is seen
    toml11: bool,
}

impl<'s> DetectToml11<'s> {
    fn new(source: &'s str) -> Self {
        Self {
            source,
            state: Vec::new(),
            toml11: false,
        }
    }

    fn raw_at(&self, span: Span) -> &'s str {
        &self.source[span.start()..span.end()]
    }

    fn flag_11(&mut self) {
        self.toml11 = true;
    }

    fn set_sep(&mut self, sep: bool) {
        if let Some(State::InlineTable { trailing_sep }) = self.state.last_mut() {
            *trailing_sep = sep;
        }
    }

    pub fn is_11(&self) -> bool {
        self.toml11
    }
}

impl EventReceiver for DetectToml11<'_> {
    fn std_table_open(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.state.push(State::StdTable);
    }

    fn std_table_close(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.state.pop();
    }

    fn array_table_open(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.state.push(State::ArrayTable);
    }

    fn array_table_close(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.state.pop();
    }

    fn inline_table_open(&mut self, _span: Span, _error: &mut dyn ErrorSink) -> bool {
        self.state.push(State::InlineTable {
            trailing_sep: false,
        });
        true
    }

    fn inline_table_close(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        if matches!(
            self.state.last(),
            Some(State::InlineTable { trailing_sep: true })
        ) {
            // TOML 1.1 introduces trailing commas in inline tables
            self.flag_11();
        }
        self.state.pop();
    }

    fn array_open(&mut self, _span: Span, _error: &mut dyn ErrorSink) -> bool {
        self.state.push(State::Array);
        true
    }

    fn array_close(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.state.pop();
    }

    fn simple_key(&mut self, _span: Span, _kind: Option<Encoding>, _error: &mut dyn ErrorSink) {
        self.set_sep(false);
    }

    fn scalar(&mut self, span: Span, kind: Option<Encoding>, _error: &mut dyn ErrorSink) {
        self.set_sep(false);

        if matches!(kind, Some(Encoding::BasicString | Encoding::MlBasicString)) {
            if has_toml11_escapes(self.raw_at(span)) {
                // TOML 1.1 introduces new escape sequences
                self.flag_11();
            }
        }
    }

    fn value_sep(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.set_sep(true);
    }

    fn newline(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        if matches!(self.state.last(), Some(State::InlineTable { .. })) {
            // TOML 1.1 introduces newlines in inline tables
            self.flag_11();
        }
    }
}

/// Scan the characters of a snippet of TOML representing a basic string for the TOML 1.1 exclusive
/// escape sequences: `\xHH` and `\e`
fn has_toml11_escapes(raw: &str) -> bool {
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(c) = chars.next()
            && matches!(c, 'x' | 'e')
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_plain_string() {
        assert!(!has_toml11_escapes(r#""hello world""#));
    }

    #[test]
    fn escapes_toml10_escape_n() {
        assert!(!has_toml11_escapes(r#""hello\nworld""#));
    }

    #[test]
    fn escapes_toml10_escape_u() {
        assert!(!has_toml11_escapes(r#""r\u00E9sum\u00E9""#));
    }

    #[test]
    fn escapes_toml11_hex() {
        assert!(has_toml11_escapes(r#""val \x41""#));
    }

    #[test]
    fn escapes_toml11_esc() {
        assert!(has_toml11_escapes(r#""val \e""#));
    }

    #[test]
    fn escapes_double_backslash_e() {
        assert!(!has_toml11_escapes(r#""\\e""#));
    }

    #[test]
    fn escapes_double_backslash_x() {
        assert!(!has_toml11_escapes(r#""\\x41""#));
    }

    #[test]
    fn features_plain_toml10() {
        assert!(!has_toml11_features("x = 1\ny = \"hello\"\nz = true\n"));
    }

    #[test]
    fn features_std_table() {
        assert!(!has_toml11_features(
            "[server]\nhost = \"localhost\"\nport = 8080\n"
        ));
    }

    #[test]
    fn features_array_of_tables() {
        assert!(!has_toml11_features(
            "[[items]]\nname = \"a\"\n[[items]]\nname = \"b\"\n"
        ));
    }

    #[test]
    fn features_inline_table_no_trailing_comma() {
        assert!(!has_toml11_features("x = {a = 1, b = 2}\n"));
    }

    #[test]
    fn features_trailing_comma_in_inline_table() {
        assert!(has_toml11_features("x = {a = 1, b = 2,}\n"));
    }

    #[test]
    fn features_multiline_inline_table() {
        assert!(has_toml11_features("x = {\n  a = 1\n}\n"));
    }

    #[test]
    fn features_multiline_inline_table_with_trailing_comma() {
        assert!(has_toml11_features("x = {\n  a = 1,\n}\n"));
    }

    #[test]
    fn features_hex_escape() {
        assert!(has_toml11_features("x = \"val \\x41\"\n"));
    }

    #[test]
    fn features_esc_escape() {
        assert!(has_toml11_features("x = \"val \\e\"\n"));
    }

    #[test]
    fn features_double_backslash_not_escape() {
        assert!(!has_toml11_features("x = \"\\\\e\"\n"));
    }

    #[test]
    fn features_toml10_escape_in_value() {
        assert!(!has_toml11_features("x = \"tab\\there\"\n"));
    }

    #[test]
    fn features_escape_in_nested_structure() {
        assert!(has_toml11_features("[t]\na = {b = \"\\x20\",}\n"));
    }

    #[test]
    fn features_trailing_comma_in_array_is_not_11() {
        assert!(!has_toml11_features("x = [1, 2, 3,]\n"));
    }
}
