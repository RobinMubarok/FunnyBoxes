//! Tree-shaking runtime. Membuang deklarasi `function`, `const`, dan `let` dari prelude yang
//! tidak dirujuk kode program. Pernyataan lain (efek samping) selalu dipertahankan.
//! Teks pernyataan yang dipertahankan tidak diubah.
//!
//! Penelusuran bersifat konservatif: nama yang muncul di string atau komentar ikut dihitung
//! sebagai rujukan. Akibatnya hanya lebih banyak kode yang dipertahankan, tidak pernah kurang.

use std::collections::HashMap;

struct Piece<'a> {
    text: &'a str,
    name: Option<&'a str>,
}

fn skip_string(b: &[u8], mut i: usize) -> usize {
    let q = b[i];
    i += 1;
    while i < b.len() {
        if b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i] == q {
            return i + 1;
        }
        i += 1;
    }
    b.len()
}

/// Teks setelah komentar dan spasi di depan.
fn head_of(text: &str) -> &str {
    let mut t = text;
    loop {
        t = t.trim_start();
        if t.starts_with("//") {
            t = match t.find('\n') {
                Some(p) => &t[p..],
                None => "",
            };
        } else if t.starts_with("/*") {
            t = match t.find("*/") {
                Some(p) => &t[p + 2..],
                None => "",
            };
        } else {
            return t;
        }
    }
}

/// Nama yang dideklarasikan pernyataan ini (`function`, `const`, atau `let`), jika ada.
fn decl_name(text: &str) -> Option<&str> {
    let mut h = head_of(text);
    if let Some(r) = h.strip_prefix("async ") {
        h = r;
    }
    let rest = if let Some(r) = h.strip_prefix("function ") {
        r
    } else if let Some(r) = h.strip_prefix("const ") {
        r
    } else if let Some(r) = h.strip_prefix("let ") {
        r
    } else {
        return None;
    };
    let rest = rest.trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
        .unwrap_or(rest.len());
    if end == 0 || rest.as_bytes()[0].is_ascii_digit() {
        None
    } else {
        Some(&rest[..end])
    }
}

fn is_fn(text: &str) -> bool {
    let h = head_of(text);
    h.starts_with("function ") || h.starts_with("async function ")
}

fn push_piece<'a>(out: &mut Vec<Piece<'a>>, src: &'a str, a: usize, e: usize) {
    let t = &src[a..e];
    out.push(Piece {
        text: t,
        name: decl_name(t),
    });
}

/// Memecah prelude menjadi pernyataan tingkat atas.
fn split(src: &str) -> Vec<Piece<'_>> {
    let b = src.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut start: Option<usize> = None;
    let mut depth: i32 = 0;

    while i < n {
        let c = b[i];
        if c == b'/' && i + 1 < n && b[i + 1] == b'/' {
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && i + 1 < n && b[i + 1] == b'*' {
            if start.is_none() {
                start = Some(i);
            }
            i += 2;
            while i + 1 < n && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if start.is_none() {
            start = Some(i);
        }
        match c {
            b'"' | b'\'' | b'`' => {
                i = skip_string(b, i);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' => depth -= 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start {
                        if is_fn(&src[s..i + 1]) {
                            push_piece(&mut out, src, s, i + 1);
                            start = None;
                        }
                    }
                }
            }
            b';' if depth == 0 => {
                if let Some(s) = start {
                    push_piece(&mut out, src, s, i + 1);
                    start = None;
                }
            }
            _ => {}
        }
        i += 1;
    }
    if let Some(s) = start {
        push_piece(&mut out, src, s, n);
    }
    out
}

/// Semua token berbentuk identifier di dalam teks.
fn idents(text: &str) -> Vec<&str> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_alphabetic() || c == b'_' || c == b'$' {
            let s = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$') {
                i += 1;
            }
            out.push(&text[s..i]);
        } else {
            i += 1;
        }
    }
    out
}

/// Mengembalikan prelude yang sudah dipangkas untuk kode program `app`.
pub fn shake(prelude: &str, app: &str) -> String {
    let pieces = split(prelude);

    let mut decl: HashMap<&str, usize> = HashMap::new();
    for (k, p) in pieces.iter().enumerate() {
        if let Some(n) = p.name {
            decl.insert(n, k);
        }
    }

    let mut keep = vec![false; pieces.len()];
    let mut work: Vec<&str> = Vec::new();
    for p in &pieces {
        if p.name.is_none() {
            work.extend(idents(p.text));
        }
    }
    work.extend(idents(app));

    while let Some(name) = work.pop() {
        if let Some(&k) = decl.get(name) {
            if !keep[k] {
                keep[k] = true;
                work.extend(idents(pieces[k].text));
            }
        }
    }
    for (k, p) in pieces.iter().enumerate() {
        if p.name.is_none() {
            keep[k] = true;
        }
    }

    pieces
        .iter()
        .enumerate()
        .filter(|(k, _)| keep[*k])
        .map(|(_, p)| p.text)
        .collect::<Vec<_>>()
        .join("\n")
}
