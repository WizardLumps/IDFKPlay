//! Minimal text VDF (Valve KeyValues) parser.
//!
//! Handles quoted and unquoted tokens, `\\ \" \n \t` escapes, `//` comments,
//! `[$PLATFORM]` conditionals (ignored), and a leading BOM. Key lookups are
//! case-insensitive because Steam's own files are inconsistent about casing.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Obj(Obj),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            Value::Obj(_) => None,
        }
    }

    pub fn as_obj(&self) -> Option<&Obj> {
        match self {
            Value::Obj(o) => Some(o),
            Value::Str(_) => None,
        }
    }
}

/// An ordered list of key/value pairs. Duplicate keys are kept.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Obj {
    pub entries: Vec<(String, Value)>,
}

impl Obj {
    /// First value whose key matches `key`, ignoring ASCII case.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }

    pub fn get_obj(&self, key: &str) -> Option<&Obj> {
        self.get(key).and_then(Value::as_obj)
    }

    /// Follows a path of nested objects, e.g. `["Software", "Valve", "Steam"]`.
    pub fn get_path(&self, path: &[&str]) -> Option<&Obj> {
        path.iter().try_fold(self, |obj, key| obj.get_obj(key))
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, PartialEq)]
enum Token {
    Open,
    Close,
    Str(String),
    Conditional,
}

struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    line: usize,
}

impl<'a> Lexer<'a> {
    fn new(text: &'a str) -> Self {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        Lexer { chars: text.chars().peekable(), line: 1 }
    }

    fn err(&self, message: impl Into<String>) -> ParseError {
        ParseError { line: self.line, message: message.into() }
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.next();
        if c == Some('\n') {
            self.line += 1;
        }
        c
    }

    fn skip_trivia(&mut self) {
        loop {
            match self.chars.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('/') => {
                    // Only `//` starts a comment; a lone `/` is part of an unquoted token.
                    let mut ahead = self.chars.clone();
                    ahead.next();
                    if ahead.peek() != Some(&'/') {
                        return;
                    }
                    while let Some(c) = self.chars.peek() {
                        if *c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                _ => return,
            }
        }
    }

    fn next_token(&mut self) -> Result<Option<Token>, ParseError> {
        self.skip_trivia();
        let Some(&c) = self.chars.peek() else {
            return Ok(None);
        };
        match c {
            '{' => {
                self.bump();
                Ok(Some(Token::Open))
            }
            '}' => {
                self.bump();
                Ok(Some(Token::Close))
            }
            '"' => {
                self.bump();
                self.quoted().map(|s| Some(Token::Str(s)))
            }
            '[' => {
                while let Some(c) = self.bump() {
                    if c == ']' {
                        return Ok(Some(Token::Conditional));
                    }
                }
                Err(self.err("unterminated [conditional]"))
            }
            _ => {
                let mut s = String::new();
                while let Some(&c) = self.chars.peek() {
                    if c.is_whitespace() || matches!(c, '{' | '}' | '"') {
                        break;
                    }
                    s.push(c);
                    self.bump();
                }
                Ok(Some(Token::Str(s)))
            }
        }
    }

    fn quoted(&mut self) -> Result<String, ParseError> {
        let start_line = self.line;
        let mut s = String::new();
        loop {
            match self.bump() {
                None => {
                    return Err(ParseError {
                        line: start_line,
                        message: "unterminated string".into(),
                    })
                }
                Some('"') => return Ok(s),
                Some('\\') => match self.bump() {
                    Some('\\') => s.push('\\'),
                    Some('"') => s.push('"'),
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    // Unknown escape: keep it literally.
                    Some(other) => {
                        s.push('\\');
                        s.push(other);
                    }
                    None => return Err(self.err("unterminated string")),
                },
                Some(c) => s.push(c),
            }
        }
    }
}

/// Parses a VDF document into its top-level object (usually one root key).
pub fn parse(text: &str) -> Result<Obj, ParseError> {
    let mut lexer = Lexer::new(text);
    parse_obj(&mut lexer, false)
}

fn parse_obj(lexer: &mut Lexer, nested: bool) -> Result<Obj, ParseError> {
    let mut obj = Obj::default();
    loop {
        let key = match lexer.next_token()? {
            None if nested => return Err(lexer.err("missing closing '}'")),
            None => return Ok(obj),
            Some(Token::Close) if nested => return Ok(obj),
            Some(Token::Close) => return Err(lexer.err("unexpected '}'")),
            Some(Token::Open) => return Err(lexer.err("expected a key, found '{'")),
            Some(Token::Conditional) => continue,
            Some(Token::Str(k)) => k,
        };
        let value = match lexer.next_token()? {
            Some(Token::Open) => Value::Obj(parse_obj(lexer, true)?),
            Some(Token::Str(v)) => Value::Str(v),
            Some(Token::Conditional) => {
                return Err(lexer.err(format!("expected a value for \"{}\"", shown(&key))))
            }
            Some(Token::Close) | None => {
                return Err(lexer.err(format!("missing value for \"{}\"", shown(&key))))
            }
        };
        obj.entries.push((key, value));
    }
}

/// Keeps error messages readable when a key is huge or full of control characters.
fn shown(key: &str) -> String {
    let clean: String = key.chars().map(|c| if c.is_control() { '?' } else { c }).collect();
    match clean.char_indices().nth(40) {
        Some((i, _)) => format!("{}…", &clean[..i]),
        None => clean,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_and_case_insensitive() {
        let doc = parse(
            r#"
            "Root"
            {
                "Name"   "Half-Life"   // trailing comment
                "Nested" { "Deep" "1" }
            }
            "#,
        )
        .unwrap();
        let root = doc.get_obj("root").unwrap();
        assert_eq!(root.get_str("NAME"), Some("Half-Life"));
        assert_eq!(root.get_path(&["nested"]).unwrap().get_str("deep"), Some("1"));
    }

    #[test]
    fn handles_escapes_unquoted_bom_and_conditionals() {
        let doc = parse("\u{feff}root { path \"C:\\\\Games\\\\Steam\" key val [$WIN32] q \"a\\\"b\" }").unwrap();
        let root = doc.get_obj("root").unwrap();
        assert_eq!(root.get_str("path"), Some("C:\\Games\\Steam"));
        assert_eq!(root.get_str("key"), Some("val"));
        assert_eq!(root.get_str("q"), Some("a\"b"));
    }

    #[test]
    fn reports_errors_with_line() {
        let err = parse("\"a\"\n{\n\"b\" \"c\"\n").unwrap_err();
        assert!(err.message.contains("closing"));
        assert!(parse("\"a\" \"unterminated").is_err());
        assert!(parse("}").is_err());
    }
}
