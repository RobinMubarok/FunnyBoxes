//! Lexer FunnyBoxes. Bekerja pada byte; posisi selalu berada di batas karakter UTF-8.

use crate::error::{Diag, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Int(i64),
    Float(f64),
    Str(String),
    Op(&'static str),
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

/// Operator diurutkan dari yang terpanjang agar pencocokan greedy benar.
const PUNCT: &[&str] = &[
    "<<=", ">>=", "...", "===", "!==",
    "=>", "->", "++", "--", "&&", "||", "==", "!=", "<=", ">=", "<<", ">>",
    "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=",
    "+", "-", "*", "/", "%", "=", "<", ">", "!", "~", "&", "|", "^",
    "?", ":", ";", ",", ".", "(", ")", "{", "}", "[", "]",
];

pub struct Lexer<'a> {
    pub src: &'a str,
    pub pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Lexer { src, pos: 0 }
    }

    fn bytes(&self) -> &'a [u8] {
        self.src.as_bytes()
    }

    fn skip_trivia(&mut self) -> Result<(), Diag> {
        let b = self.bytes();
        loop {
            match b.get(self.pos).copied() {
                Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n') => self.pos += 1,
                Some(b'/') if b.get(self.pos + 1).copied() == Some(b'/') => {
                    while self.pos < b.len() && b[self.pos] != b'\n' {
                        self.pos += 1;
                    }
                }
                Some(b'/') if b.get(self.pos + 1).copied() == Some(b'*') => {
                    let start = self.pos;
                    match self.src[start + 2..].find("*/") {
                        Some(i) => self.pos = start + 2 + i + 2,
                        None => {
                            return Err(Diag::new(
                                "Komentar /* tidak ditutup",
                                Span::new(start, start + 2),
                            ))
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    /// Mengambil token berikutnya dan memajukan posisi.
    pub fn next_token(&mut self) -> Result<Token, Diag> {
        self.skip_trivia()?;
        let b = self.bytes();
        let start = self.pos;

        if start >= b.len() {
            return Ok(Token { tok: Tok::Eof, span: Span::new(start, start) });
        }

        let c = b[start];
        if c.is_ascii_alphabetic() || c == b'_' || c == b'$' {
            let mut i = start + 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$') {
                i += 1;
            }
            self.pos = i;
            return Ok(Token {
                tok: Tok::Ident(self.src[start..i].to_string()),
                span: Span::new(start, i),
            });
        }

        let next_is_digit = b.get(start + 1).is_some_and(|d| d.is_ascii_digit());
        if c.is_ascii_digit() || (c == b'.' && next_is_digit) {
            return self.number(start);
        }

        if c == b'"' || c == b'\'' {
            return self.string(start, c);
        }

        for p in PUNCT {
            if b[start..].starts_with(p.as_bytes()) {
                self.pos = start + p.len();
                return Ok(Token { tok: Tok::Op(*p), span: Span::new(start, self.pos) });
            }
        }

        let ch = self.src[start..].chars().next().unwrap_or('?');
        Err(Diag::new(
            format!("Karakter tidak dikenal '{}'", ch),
            Span::new(start, start + ch.len_utf8()),
        ))
    }

    fn number(&mut self, start: usize) -> Result<Token, Diag> {
        let b = self.bytes();
        let span_to = |end: usize| Span::new(start, end);

        // Heksadesimal: 0x... dengan '_' sebagai pemisah opsional.
        let after_zero = b.get(start + 1).copied();
        if b[start] == b'0' && (after_zero == Some(b'x') || after_zero == Some(b'X')) {
            let digits_start = start + 2;
            let mut i = digits_start;
            while i < b.len() && (b[i].is_ascii_hexdigit() || b[i] == b'_') {
                i += 1;
            }
            let digits: String = self.src[digits_start..i].chars().filter(|c| *c != '_').collect();
            if digits.is_empty() {
                return Err(Diag::new("Digit heksadesimal diharapkan", span_to(i)));
            }
            let value = i64::from_str_radix(&digits, 16)
                .map_err(|_| Diag::new("Literal heksadesimal terlalu besar", span_to(i)))?;
            self.pos = i;
            return Ok(Token { tok: Tok::Int(value), span: span_to(i) });
        }

        let mut i = start;
        while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'_') {
            i += 1;
        }

        let mut is_float = false;
        if i + 1 < b.len() && b[i] == b'.' && b[i + 1].is_ascii_digit() {
            is_float = true;
            i += 1;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'_') {
                i += 1;
            }
        }

        if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
            let mut j = i + 1;
            if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
                j += 1;
            }
            if j < b.len() && b[j].is_ascii_digit() {
                is_float = true;
                i = j;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
        }

        let text: String = self.src[start..i].chars().filter(|c| *c != '_').collect();
        self.pos = i;
        let span = span_to(i);

        if is_float {
            // Rust tidak menerima ".5"; tambahkan "0" di depan.
            let normalized = if text.starts_with('.') { format!("0{}", text) } else { text };
            let value: f64 = normalized
                .parse()
                .map_err(|_| Diag::new("Literal float tidak valid", span))?;
            Ok(Token { tok: Tok::Float(value), span })
        } else {
            let value: i64 = text
                .parse()
                .map_err(|_| Diag::new("Literal bilangan terlalu besar", span))?;
            Ok(Token { tok: Tok::Int(value), span })
        }
    }

    fn string(&mut self, start: usize, quote: u8) -> Result<Token, Diag> {
        let b = self.bytes();
        let mut out = String::new();
        let mut seg = start + 1; // awal potongan teks yang belum disalin
        let mut i = start + 1;

        loop {
            if i >= b.len() {
                return Err(Diag::new("String tidak ditutup", Span::new(start, start + 1)));
            }
            let c = b[i];
            if c == quote {
                out.push_str(&self.src[seg..i]);
                i += 1;
                break;
            }
            if c == b'\n' {
                return Err(Diag::new(
                    "String tidak ditutup sebelum akhir baris",
                    Span::new(start, i),
                ));
            }
            if c == b'\\' {
                out.push_str(&self.src[seg..i]);
                let esc = b.get(i + 1).copied().unwrap_or(0);
                let ch = match esc {
                    b'n' => '\n',
                    b't' => '\t',
                    b'r' => '\r',
                    b'0' => '\0',
                    b'\\' => '\\',
                    b'"' => '"',
                    b'\'' => '\'',
                    _ => {
                        return Err(Diag::new(
                            format!("Escape '\\{}' tidak dikenal", esc as char),
                            Span::new(i, i + 2),
                        ))
                    }
                };
                out.push(ch);
                i += 2;
                seg = i;
                continue;
            }
            i += 1;
        }

        self.pos = i;
        Ok(Token { tok: Tok::Str(out), span: Span::new(start, i) })
    }
}
