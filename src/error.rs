//! Lokasi sumber dan diagnostik kompilasi.

use std::fmt;

/// Rentang byte dalam file sumber.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }

    /// Rentang yang mencakup kedua span.
    pub fn join(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// Satu error kompilasi beserta lokasinya.
#[derive(Debug, Clone, PartialEq)]
pub struct Diag {
    pub msg: String,
    pub span: Span,
}

impl Diag {
    pub fn new(msg: impl Into<String>, span: Span) -> Self {
        Diag { msg: msg.into(), span }
    }
}

impl fmt::Display for Diag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl std::error::Error for Diag {}

/// File sumber beserta path-nya, dipakai untuk memformat diagnostik.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: String,
    pub src: String,
}

impl SourceFile {
    pub fn new(path: impl Into<String>, src: impl Into<String>) -> Self {
        SourceFile { path: path.into(), src: src.into() }
    }

    /// Mengubah offset byte menjadi (baris, kolom), keduanya mulai dari 1.
    pub fn line_col(&self, pos: usize) -> (usize, usize) {
        let pos = pos.min(self.src.len());
        let before = &self.src.as_bytes()[..pos];
        let line = before.iter().filter(|&&b| b == b'\n').count() + 1;
        let line_start = before.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        let col = self
            .src
            .get(line_start..pos)
            .map_or(pos - line_start, |s| s.chars().count())
            + 1;
        (line, col)
    }

    /// Memformat diagnostik dengan path, lokasi, dan cuplikan baris.
    pub fn render(&self, d: &Diag) -> String {
        let (line, col) = self.line_col(d.span.start);
        let text = self.src.lines().nth(line - 1).unwrap_or("");
        format!(
            "{}:{}:{}: error: {}\n{:>4} | {}\n     | {}^",
            self.path,
            line,
            col,
            d.msg,
            line,
            text,
            " ".repeat(col - 1),
        )
    }
}
