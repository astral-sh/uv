use std::collections::VecDeque;
use std::str::Chars;

pub(crate) struct Lex<'a> {
    pub(crate) lineno: u32,
    instream: Chars<'a>,
    pushback: VecDeque<String>,
}

impl<'a> Lex<'a> {
    pub(crate) fn new(content: &'a str) -> Self {
        Lex {
            lineno: 1,
            instream: content.chars(),
            pushback: VecDeque::new(),
        }
    }

    fn read_char(&mut self) -> Option<char> {
        let ch = self.instream.next();
        if ch == Some('\n') {
            self.lineno += 1;
        }
        ch
    }

    pub(crate) fn read_line(&mut self) -> String {
        let mut s = String::new();
        for ch in &mut self.instream {
            if ch == '\n' {
                return s;
            }
            s.push(ch);
        }
        s
    }

    pub(crate) fn get_token(&mut self) -> String {
        let p = self.pushback.pop_front();
        if let Some(x) = p {
            return x;
        }
        let mut token = String::new();

        while let Some(ch) = self.read_char() {
            match ch {
                '\n' | '\t' | '\r' | ' ' => {}
                '"' => {
                    while let Some(ch) = self.read_char() {
                        match ch {
                            '"' => {
                                return token;
                            }
                            '\\' => {
                                token.push(self.read_char().unwrap_or(' '));
                            }
                            _ => {
                                token.push(ch);
                            }
                        }
                    }
                }
                _ => {
                    let c = if ch == '\\' {
                        self.read_char().unwrap_or(' ')
                    } else {
                        ch
                    };
                    token.push(c);
                    while let Some(ch) = self.read_char() {
                        let c = match ch {
                            '\n' | '\t' | '\r' | ' ' => {
                                return token;
                            }
                            '\\' => self.read_char().unwrap_or(' '),
                            _ => ch,
                        };
                        token.push(c);
                    }
                }
            }
        }
        token
    }

    pub(crate) fn push_token(&mut self, token: &str) {
        self.pushback.push_back(token.to_owned());
    }
}
