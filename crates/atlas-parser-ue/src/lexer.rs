//! Tokenizer for Dumper-7-style headers.
//!
//! This is **not** a full C++ lexer. We recognize just enough to parse
//! class/struct/enum declarations, fields with their offset/size
//! comments, and virtual function declarations with their vtable-slot
//! comments. Everything else is either consumed as a `Comment` token or
//! skipped via `Token::Unknown`.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TokenKind {
    // Identifier / keyword. We don't pre-classify keywords here; the
    // parser checks `text` against a known list. Keeps the lexer
    // simple and lets us add keywords later without touching tokens.
    Ident,

    // Integer literal (decimal or hex). Stored as a string; the parser
    // converts as needed.
    Number,

    // String literal. Rare in headers but possible (e.g., #pragma message).
    StringLit,

    // Single-character punctuation. The `text` is one byte.
    Punct,

    // Comment — either `// ...` or `/* ... */`. The leading delimiter
    // is stripped, so the text is just the comment body.
    Comment,

    // Anything we couldn't classify. The parser typically skips these.
    Unknown,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ident => write!(f, "ident"),
            Self::Number => write!(f, "number"),
            Self::StringLit => write!(f, "string"),
            Self::Punct => write!(f, "punct"),
            Self::Comment => write!(f, "comment"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

pub(crate) fn tokenize(src: &str) -> Vec<Token> {
    let mut out = Vec::with_capacity(src.len() / 8);
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut line: u32 = 1;

    while i < bytes.len() {
        let c = bytes[i];

        // Newlines
        if c == b'\n' {
            line += 1;
            i += 1;
            continue;
        }

        // Whitespace
        if c == b' ' || c == b'\t' || c == b'\r' {
            i += 1;
            continue;
        }

        // // line comment
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            let start = i + 2;
            let mut j = start;
            while j < bytes.len() && bytes[j] != b'\n' {
                j += 1;
            }
            let text = std::str::from_utf8(&bytes[start..j])
                .unwrap_or("")
                .trim()
                .to_string();
            out.push(Token {
                kind: TokenKind::Comment,
                text,
                line,
            });
            i = j;
            continue;
        }

        // /* block comment */
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            let start = i + 2;
            let mut j = start;
            let start_line = line;
            while j + 1 < bytes.len() && !(bytes[j] == b'*' && bytes[j + 1] == b'/') {
                if bytes[j] == b'\n' {
                    line += 1;
                }
                j += 1;
            }
            let text = std::str::from_utf8(&bytes[start..j.min(bytes.len())])
                .unwrap_or("")
                .trim()
                .to_string();
            out.push(Token {
                kind: TokenKind::Comment,
                text,
                line: start_line,
            });
            i = j + 2;
            continue;
        }

        // Preprocessor lines — skip to end of line. Phase 1 ignores
        // #pragma / #include / #if entirely.
        if c == b'#' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        // String literal
        if c == b'"' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j] != b'"' {
                if bytes[j] == b'\\' && j + 1 < bytes.len() {
                    j += 2;
                    continue;
                }
                if bytes[j] == b'\n' {
                    line += 1;
                }
                j += 1;
            }
            let text = std::str::from_utf8(&bytes[start..j.min(bytes.len())])
                .unwrap_or("")
                .to_string();
            out.push(Token {
                kind: TokenKind::StringLit,
                text,
                line,
            });
            i = j + 1;
            continue;
        }

        // Identifier / keyword: [A-Za-z_][A-Za-z0-9_]*
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let text = std::str::from_utf8(&bytes[start..i])
                .unwrap_or("")
                .to_string();
            out.push(Token {
                kind: TokenKind::Ident,
                text,
                line,
            });
            continue;
        }

        // Number: decimal or 0x hex
        if c.is_ascii_digit() {
            let start = i;
            if c == b'0' && i + 1 < bytes.len() && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X') {
                i += 2;
                while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                    i += 1;
                }
            } else {
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                // optional fraction (we accept float-looking numbers as Number)
                if i < bytes.len() && bytes[i] == b'.' {
                    i += 1;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            // Accept optional integer suffixes (u, l, ull, etc.) — consume but ignore.
            while i < bytes.len()
                && (bytes[i] == b'u'
                    || bytes[i] == b'U'
                    || bytes[i] == b'l'
                    || bytes[i] == b'L'
                    || bytes[i] == b'f'
                    || bytes[i] == b'F')
            {
                i += 1;
            }
            let text = std::str::from_utf8(&bytes[start..i])
                .unwrap_or("")
                .to_string();
            out.push(Token {
                kind: TokenKind::Number,
                text,
                line,
            });
            continue;
        }

        // Punctuation: we treat each byte as its own token. The parser
        // pieces multi-char operators back together when needed.
        if is_punct(c) {
            out.push(Token {
                kind: TokenKind::Punct,
                text: (c as char).to_string(),
                line,
            });
            i += 1;
            continue;
        }

        // Anything else: emit Unknown and move on.
        out.push(Token {
            kind: TokenKind::Unknown,
            text: (c as char).to_string(),
            line,
        });
        i += 1;
    }

    out
}

const fn is_punct(c: u8) -> bool {
    matches!(
        c,
        b'{' | b'}'
            | b'('
            | b')'
            | b'['
            | b']'
            | b'<'
            | b'>'
            | b':'
            | b';'
            | b','
            | b'.'
            | b'='
            | b'*'
            | b'&'
            | b'!'
            | b'?'
            | b'+'
            | b'-'
            | b'/'
            | b'%'
            | b'^'
            | b'|'
            | b'~'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_simple_class_header() {
        let src = "class APlayer : public APawn { int32_t Score; };";
        let tokens = tokenize(src);
        let texts: Vec<_> = tokens.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "class", "APlayer", ":", "public", "APawn", "{", "int32_t", "Score", ";", "}", ";"
            ]
        );
    }

    #[test]
    fn lexes_hex_and_decimal_numbers() {
        let src = "0x0040 256";
        let tokens: Vec<_> = tokenize(src)
            .into_iter()
            .filter(|t| t.kind == TokenKind::Number)
            .collect();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text, "0x0040");
        assert_eq!(tokens[1].text, "256");
    }

    #[test]
    fn lexes_line_comment_strips_slashes() {
        let src = "int x; // 0x0000(0x0004)\nint y;";
        let tokens = tokenize(src);
        let comment = tokens
            .iter()
            .find(|t| t.kind == TokenKind::Comment)
            .expect("comment");
        assert_eq!(comment.text, "0x0000(0x0004)");
    }

    #[test]
    fn lexes_block_comment_strips_delimiters() {
        let src = "/* hello\nworld */ x";
        let tokens = tokenize(src);
        let comment = tokens
            .iter()
            .find(|t| t.kind == TokenKind::Comment)
            .expect("comment");
        assert!(comment.text.contains("hello"));
        assert!(comment.text.contains("world"));
    }

    #[test]
    fn tracks_line_numbers_across_newlines() {
        let src = "a\nb\nc";
        let tokens = tokenize(src);
        assert_eq!(tokens[0].line, 1);
        assert_eq!(tokens[1].line, 2);
        assert_eq!(tokens[2].line, 3);
    }

    #[test]
    fn skips_preprocessor_lines() {
        let src = "#pragma once\nclass A {};";
        let tokens = tokenize(src);
        let texts: Vec<_> = tokens.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, vec!["class", "A", "{", "}", ";"]);
    }

    #[test]
    fn handles_unknown_chars_as_unknown_tokens() {
        let src = "x @ y";
        let tokens = tokenize(src);
        // Should produce: Ident(x), Unknown(@), Ident(y)
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[1].kind, TokenKind::Unknown);
        assert_eq!(tokens[1].text, "@");
    }
}
