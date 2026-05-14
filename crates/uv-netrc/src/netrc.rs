//! This parser and the tests are a translation of the official Python netrc library.

use crate::lex::Lex;
use std::collections::HashMap;

#[derive(Debug)]
pub struct ParsingError {
    lineno: u32,
    message: String,
}

impl std::fmt::Display for ParsingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parsing error: {} (line {})", self.message, self.lineno)
    }
}

/// Authenticators for host.
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct Authenticator {
    /// Identify a user on the remote machine.
    pub login: String,

    /// Supply an additional account password.
    pub account: String,

    /// Supply a password
    pub password: String,
}

impl Authenticator {
    #[allow(dead_code)]
    pub fn new(login: &str, account: &str, password: &str) -> Self {
        Self {
            login: login.to_owned(),
            account: account.to_owned(),
            password: password.to_owned(),
        }
    }
}

/// Represents the netrc file.
#[derive(Debug, Default)]
pub struct Netrc {
    /// Dictionary mapping host names to the authenticators.
    pub hosts: HashMap<String, Authenticator>,

    /// Dictionary mapping macro names to string lists.
    pub macros: HashMap<String, Vec<String>>,
}

impl std::fmt::Display for Netrc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (host, attrs) in &self.hosts {
            writeln!(f, "machine {host}")?;
            writeln!(f, "\tlogin {}", attrs.login)?;
            if !attrs.account.is_empty() {
                writeln!(f, "\taccount  {}", attrs.account)?;
            }
            writeln!(f, "\tpassword  {}", attrs.password)?;
        }
        for (macro_, lines) in &self.macros {
            writeln!(f, "macdef {macro_}")?;
            for line in lines {
                writeln!(f, "{line}")?;
            }
        }
        Ok(())
    }
}

impl std::str::FromStr for Netrc {
    type Err = ParsingError;

    fn from_str(s: &str) -> Result<Self, ParsingError> {
        let mut res = Self::default();
        let mut lexer = Lex::new(s);

        loop {
            let saved_lineno = lexer.lineno;
            let tt = lexer.get_token();
            if tt.is_empty() {
                break;
            }
            if tt.chars().nth(0) == Some('#') {
                if lexer.lineno == saved_lineno && tt.len() == 1 {
                    lexer.read_line();
                }
                continue;
            }

            let entryname = match tt.as_str() {
                "" => {
                    break;
                }
                "machine" => lexer.get_token(),
                "default" => String::from("default"),
                "macdef" => {
                    let entryname = lexer.get_token();
                    let mut v = Vec::new();
                    loop {
                        let line = lexer.read_line();
                        if line.trim().is_empty() {
                            break;
                        }
                        v.push(line.trim().to_owned());
                    }
                    res.macros.insert(entryname, v);
                    continue;
                }
                _ => {
                    return Err(ParsingError {
                        lineno: lexer.lineno,
                        message: format!("bad toplevel token '{tt}'"),
                    });
                }
            };
            if entryname.is_empty() {
                return Err(ParsingError {
                    lineno: lexer.lineno,
                    message: format!("missing '{tt}' name"),
                });
            }

            let mut auth = Authenticator::default();

            loop {
                let prev_lineno = lexer.lineno;
                let tt = lexer.get_token();
                if tt.starts_with('#') {
                    if lexer.lineno == prev_lineno {
                        lexer.read_line();
                    }
                    continue;
                }
                match tt.as_str() {
                    "" | "machine" | "default" | "macdef" => {
                        res.hosts.insert(entryname, auth);
                        lexer.push_token(&tt);
                        break;
                    }
                    "login" | "user" => {
                        auth.login = lexer.get_token();
                    }
                    "account" => {
                        auth.account = lexer.get_token();
                    }
                    "password" => {
                        auth.password = lexer.get_token();
                    }
                    _ => {
                        return Err(ParsingError {
                            lineno: lexer.lineno,
                            message: format!("bad follower token '{tt}'"),
                        });
                    }
                }
            }
        }

        Ok(res)
    }
}

#[cfg(test)]
#[expect(
    clippy::needless_raw_string_hashes,
    reason = "Keep the vendored parser tests close to upstream."
)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_toplevel_non_ordered_tokens() {
        let nrc = Netrc::from_str(
            "\
            machine host.domain.com password pass1 login log1 account acct1
            default login log2 password pass2 account acct2
        ",
        )
        .unwrap();

        assert_eq!(
            nrc.hosts["host.domain.com"],
            Authenticator::new("log1", "acct1", "pass1")
        );
        assert_eq!(
            nrc.hosts["default"],
            Authenticator::new("log2", "acct2", "pass2")
        );
    }

    #[test]
    fn test_toplevel_tokens() {
        let nrc = Netrc::from_str(
            "\
            machine host.domain.com login log1 password pass1 account acct1
            default login log2 password pass2 account acct2
        ",
        )
        .unwrap();
        assert_eq!(
            nrc.hosts["host.domain.com"],
            Authenticator::new("log1", "acct1", "pass1")
        );
        assert_eq!(
            nrc.hosts["default"],
            Authenticator::new("log2", "acct2", "pass2")
        );
    }

    #[test]
    fn test_macros() {
        let nrc = Netrc::from_str(
            "\
            macdef macro1
            line1
            line2

            macdef macro2
            line3
            line4
            ",
        )
        .unwrap();
        assert_eq!(nrc.macros["macro1"], vec!["line1", "line2"]);
        assert_eq!(nrc.macros["macro2"], vec!["line3", "line4"]);
    }

    #[test]
    fn test_optional_tokens_machine() {
        let data = vec![
            "machine host.domain.com",
            "machine host.domain.com login",
            "machine host.domain.com account",
            "machine host.domain.com password",
            "machine host.domain.com login \"\" account",
            "machine host.domain.com login \"\" password",
            "machine host.domain.com account \"\" password",
        ];

        for item in data {
            let nrc = Netrc::from_str(item).unwrap();
            assert_eq!(nrc.hosts["host.domain.com"], Authenticator::new("", "", ""));
        }
    }

    #[test]
    fn test_optional_tokens_default() {
        let data = vec![
            "default",
            "default login",
            "default account",
            "default password",
            "default login \"\" account",
            "default login \"\" password",
            "default account \"\" password",
        ];

        for item in data {
            let nrc = Netrc::from_str(item).unwrap();
            assert_eq!(nrc.hosts["default"], Authenticator::new("", "", ""));
        }
    }

    #[test]
    fn test_invalid_tokens() {
        let data = vec![
            (
                "invalid host.domain.com",
                "parsing error: bad toplevel token 'invalid' (line 1)",
            ),
            (
                "machine host.domain.com invalid",
                "parsing error: bad follower token 'invalid' (line 1)",
            ),
            (
                "machine host.domain.com login log password pass account acct invalid",
                "parsing error: bad follower token 'invalid' (line 1)",
            ),
            (
                "default host.domain.com invalid",
                "parsing error: bad follower token 'host.domain.com' (line 1)",
            ),
            (
                "default host.domain.com login log password pass account acct invalid",
                "parsing error: bad follower token 'host.domain.com' (line 1)",
            ),
        ];

        for (item, msg) in data {
            let nrc = Netrc::from_str(item);
            assert_eq!(nrc.unwrap_err().to_string(), msg);
        }
    }

    fn test_token_x(data: &str, token: &str, value: &str) {
        let nrc = Netrc::from_str(data).unwrap();
        match token {
            "login" => {
                assert_eq!(
                    nrc.hosts["host.domain.com"],
                    Authenticator::new(value, "acct", "pass")
                );
            }
            "account" => {
                assert_eq!(
                    nrc.hosts["host.domain.com"],
                    Authenticator::new("log", value, "pass")
                );
            }
            "password" => {
                assert_eq!(
                    nrc.hosts["host.domain.com"],
                    Authenticator::new("log", "acct", value)
                );
            }
            _ => {}
        }
    }

    #[test]
    fn test_token_value_quotes() {
        test_token_x(
            "\
            machine host.domain.com login \"log\" password pass account acct
            ",
            "login",
            "log",
        );
        test_token_x(
            "\
            machine host.domain.com login log password pass account \"acct\"
            ",
            "account",
            "acct",
        );
        test_token_x(
            "\
            machine host.domain.com login log password \"pass\" account acct
            ",
            "password",
            "pass",
        );
    }

    #[test]
    fn test_token_value_escape() {
        test_token_x(
            r#"machine host.domain.com login \"log password pass account acct"#,
            "login",
            "\"log",
        );
        test_token_x(
            "\
            machine host.domain.com login \"\\\"log\" password pass account acct
            ",
            "login",
            "\"log",
        );
        test_token_x(
            "\
            machine host.domain.com login log password pass account \\\"acct
            ",
            "account",
            "\"acct",
        );
        test_token_x(
            "\
            machine host.domain.com login log password pass account \"\\\"acct\"
            ",
            "account",
            "\"acct",
        );
        test_token_x(
            "\
            machine host.domain.com login log password \\\"pass account acct
            ",
            "password",
            "\"pass",
        );
        test_token_x(
            "\
            machine host.domain.com login log password \"\\\"pass\" account acct
            ",
            "password",
            "\"pass",
        );
    }

    #[test]
    fn test_token_value_whitespace() {
        test_token_x(
            r#"machine host.domain.com login "lo g" password pass account acct"#,
            "login",
            "lo g",
        );
        test_token_x(
            r#"machine host.domain.com login log password "pas s" account acct"#,
            "password",
            "pas s",
        );
        test_token_x(
            r#"machine host.domain.com login log password pass account "acc t""#,
            "account",
            "acc t",
        );
    }

    #[test]
    fn test_token_value_non_ascii() {
        test_token_x(
            r#"machine host.domain.com login ¡¢ password pass account acct"#,
            "login",
            "¡¢",
        );
        test_token_x(
            r#"machine host.domain.com login log password pass account ¡¢"#,
            "account",
            "¡¢",
        );
        test_token_x(
            r#"machine host.domain.com login log password ¡¢ account acct"#,
            "password",
            "¡¢",
        );
    }

    #[test]
    fn test_token_value_leading_hash() {
        test_token_x(
            r#"machine host.domain.com login #log password pass account acct"#,
            "login",
            "#log",
        );
        test_token_x(
            r#"machine host.domain.com login log password pass account #acct"#,
            "account",
            "#acct",
        );
        test_token_x(
            r#"machine host.domain.com login log password #pass account acct"#,
            "password",
            "#pass",
        );
    }

    #[test]
    fn test_token_value_trailing_hash() {
        test_token_x(
            r#"machine host.domain.com login log# password pass account acct"#,
            "login",
            "log#",
        );
        test_token_x(
            r#"machine host.domain.com login log password pass account acct#"#,
            "account",
            "acct#",
        );
        test_token_x(
            r#"machine host.domain.com login log password pass# account acct"#,
            "password",
            "pass#",
        );
    }

    #[test]
    fn test_token_value_internal_hash() {
        test_token_x(
            r#"machine host.domain.com login lo#g password pass account acct"#,
            "login",
            "lo#g",
        );
        test_token_x(
            r#"machine host.domain.com login log password pass account ac#ct"#,
            "account",
            "ac#ct",
        );
        test_token_x(
            r#"machine host.domain.com login log password pa#ss account acct"#,
            "password",
            "pa#ss",
        );
    }

    fn test_comment(data: &str) {
        let nrc = Netrc::from_str(data).unwrap();
        assert_eq!(
            nrc.hosts["foo.domain.com"],
            Authenticator::new("bar", "", "pass")
        );
        assert_eq!(
            nrc.hosts["bar.domain.com"],
            Authenticator::new("foo", "", "pass")
        );
    }

    #[test]
    fn test_comment_before_machine_line() {
        test_comment(
            r#"# comment
            machine foo.domain.com login bar password pass
            machine bar.domain.com login foo password pass
            "#,
        );
    }
    #[test]
    fn test_comment_before_machine_line_no_space() {
        test_comment(
            r#"#comment
            machine foo.domain.com login bar password pass
            machine bar.domain.com login foo password pass
            "#,
        );
    }

    #[test]
    fn test_comment_before_machine_line_hash_only() {
        test_comment(
            r#"#
            machine foo.domain.com login bar password pass
            machine bar.domain.com login foo password pass
            "#,
        );
    }

    #[test]
    fn test_comment_after_machine_line() {
        test_comment(
            r#"machine foo.domain.com login bar password pass
            # comment
            machine bar.domain.com login foo password pass
            "#,
        );
        test_comment(
            r#"machine foo.domain.com login bar password pass
            machine bar.domain.com login foo password pass
            # comment
            "#,
        );
    }

    #[test]
    fn test_comment_after_machine_line_no_space() {
        test_comment(
            r#"machine foo.domain.com login bar password pass
            #comment
            machine bar.domain.com login foo password pass
            "#,
        );
        test_comment(
            r#"machine foo.domain.com login bar password pass
            machine bar.domain.com login foo password pass
            #comment
            "#,
        );
    }

    #[test]
    fn test_comment_after_machine_line_hash_only() {
        test_comment(
            r#"machine foo.domain.com login bar password pass
            #
            machine bar.domain.com login foo password pass
            "#,
        );
        test_comment(
            r#"machine foo.domain.com login bar password pass
            machine bar.domain.com login foo password pass
            #
            "#,
        );
    }

    #[test]
    fn test_comment_at_end_of_machine_line() {
        test_comment(
            r#"machine foo.domain.com login bar password pass # comment
            machine bar.domain.com login foo password pass
            "#,
        );
    }

    #[test]
    fn test_comment_at_end_of_machine_line_no_space() {
        test_comment(
            r#"machine foo.domain.com login bar password pass #comment
            machine bar.domain.com login foo password pass
            "#,
        );
    }

    #[test]
    fn test_comment_at_end_of_machine_line_pass_has_hash() {
        let nrc = Netrc::from_str(
            r#"machine foo.domain.com login bar password #pass #comment
            machine bar.domain.com login foo password pass
        "#,
        )
        .unwrap();
        assert_eq!(
            nrc.hosts["foo.domain.com"],
            Authenticator::new("bar", "", "#pass")
        );
        assert_eq!(
            nrc.hosts["bar.domain.com"],
            Authenticator::new("foo", "", "pass")
        );
    }
}
