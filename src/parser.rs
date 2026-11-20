//! Parser FunnyBoxes (recursive descent).
//!
//! Teks JSX tidak ditokenisasi sebagai kode. Parser memindai byte mentah dari
//! sumber (`jsx_*`), lalu memanggil parser ekspresi untuk isi `{...}` setelah
//! lexer dipindahkan ke posisi yang tepat.

use std::collections::VecDeque;

use crate::ast::*;
use crate::error::{Diag, Span};
use crate::lexer::{Lexer, Tok, Token};

pub type PResult<T> = Result<T, Diag>;

/// Kata kunci yang tidak boleh dipakai sebagai nama.
pub const KEYWORDS: &[&str] = &[
    "struct", "fun", "export", "default", "play", "alias", "version", "static",
    "if", "else", "for", "while", "do", "return", "break", "continue", "switch",
    "case", "debug", "state", "var", "let", "true", "false", "null", "await",
    // Keyword HolyC yang disetujui:
    "union", "try", "catch", "throw", "extern", "offset", "asm", "reg", "noreg",
    "interrupt", "lock", "no_warn", "start", "end", "goto",
];

const ASSIGN_OPS: &[&str] = &["=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>="];

/// Prioritas operator biner; angka lebih besar mengikat lebih kuat.
fn binary_prec(op: &str) -> Option<u8> {
    Some(match op {
        "||" => 1,
        "&&" => 2,
        "|" => 3,
        "^" => 4,
        "&" => 5,
        "==" | "!=" | "===" | "!==" => 6,
        "<" | ">" | "<=" | ">=" => 7,
        "<<" | ">>" => 8,
        "+" | "-" => 9,
        "*" | "/" | "%" => 10,
        _ => return None,
    })
}

fn is_keyword(word: &str) -> bool {
    KEYWORDS.contains(&word)
}

fn is_box_type(name: &str) -> bool {
    matches!(
        name,
        "box00" | "box04" | "box14" | "box08" | "box18" | "box24" | "box28" | "char"
    )
}

fn describe(tok: &Tok) -> String {
    match tok {
        Tok::Eof => "akhir file".to_string(),
        Tok::Str(s) => format!("string \"{}\"", s),
        Tok::Int(v) => format!("angka {}", v),
        Tok::Float(v) => format!("angka {}", v),
        Tok::Ident(s) => format!("'{}'", s),
        Tok::Op(s) => format!("'{}'", s),
    }
}

fn var_stmts(decls: Vec<VarDecl>) -> Vec<Stmt> {
    decls
        .into_iter()
        .map(|d| Stmt { span: d.span, kind: StmtKind::Var(d) })
        .collect()
}

/// Mencari akhir nama identifier (huruf, angka, `_`, `$`; opsional `-` dan `:` untuk atribut JSX).
/// Mengembalikan `p` jika tidak ada nama di posisi tersebut.
fn scan_name(b: &[u8], p: usize, allow_dash_colon: bool) -> usize {
    if p >= b.len() {
        return p;
    }
    let first = b[p];
    if !(first.is_ascii_alphabetic() || first == b'_' || first == b'$') {
        return p;
    }
    let mut i = p + 1;
    while i < b.len() {
        let c = b[i];
        let ok = c.is_ascii_alphanumeric()
            || c == b'_'
            || c == b'$'
            || (allow_dash_colon && (c == b'-' || c == b':'));
        if !ok {
            break;
        }
        i += 1;
    }
    i
}

/// Normalisasi teks JSX seperti React: baris kosong dibuang, spasi tepi per baris dipangkas.
fn jsx_text(raw: &str) -> String {
    if !raw.contains('\n') {
        return raw.to_string();
    }
    let lines: Vec<&str> = raw.split('\n').collect();
    let last = lines.len() - 1;
    let mut parts: Vec<&str> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let mut x: &str = line.trim_end_matches('\r');
        if i > 0 {
            x = x.trim_start();
        }
        if i < last {
            x = x.trim_end();
        }
        if !x.is_empty() {
            parts.push(x);
        }
    }
    parts.join(" ")
}

pub struct Parser<'a> {
    lex: Lexer<'a>,
    buf: VecDeque<Token>,
    /// Satu entri per fungsi yang sedang diparse: `true` jika memakai `await`.
    fn_stack: Vec<bool>,
}

impl<'a> Parser<'a> {
    pub fn new(src: &'a str) -> Self {
        Parser {
            lex: Lexer::new(src),
            buf: VecDeque::new(),
            fn_stack: Vec::new(),
        }
    }

    fn src(&self) -> &'a str {
        self.lex.src
    }

    // ---------- Helper token ----------

    fn peek(&mut self, k: usize) -> PResult<Token> {
        while self.buf.len() <= k {
            let t = self.lex.next_token()?;
            self.buf.push_back(t);
        }
        Ok(self.buf[k].clone())
    }

    fn bump(&mut self) -> PResult<Token> {
        match self.buf.pop_front() {
            Some(t) => Ok(t),
            None => self.lex.next_token(),
        }
    }

    fn is_word(tok: &Tok, v: &str) -> bool {
        match tok {
            Tok::Op(s) => *s == v,
            Tok::Ident(s) => s == v,
            _ => false,
        }
    }

    fn is_at(&mut self, k: usize, v: &str) -> PResult<bool> {
        let t = self.peek(k)?;
        Ok(Self::is_word(&t.tok, v))
    }

    fn eat(&mut self, v: &str) -> PResult<bool> {
        if self.is_at(0, v)? {
            self.bump()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn expect(&mut self, v: &str) -> PResult<Token> {
        let t = self.peek(0)?;
        if Self::is_word(&t.tok, v) {
            self.bump()
        } else {
            Err(Diag::new(
                format!("Diharapkan '{}', ditemukan {}", v, describe(&t.tok)),
                t.span,
            ))
        }
    }

    /// Nama identifier yang bukan kata kunci.
    fn ident(&mut self) -> PResult<(String, Span)> {
        let t = self.peek(0)?;
        match &t.tok {
            Tok::Ident(s) if !is_keyword(s) => {
                let name = s.clone();
                self.bump()?;
                Ok((name, t.span))
            }
            _ => Err(Diag::new(
                format!("Diharapkan nama, ditemukan {}", describe(&t.tok)),
                t.span,
            )),
        }
    }

    /// Membuang seluruh lookahead dan memindahkan lexer ke posisi `pos`.
    fn reset_to(&mut self, pos: usize) {
        self.lex.pos = pos;
        self.buf.clear();
    }

    // ---------- Program & statement ----------

    pub fn parse_program(&mut self) -> PResult<Program> {
        let mut body = Vec::new();
        while self.peek(0)?.tok != Tok::Eof {
            body.extend(self.parse_statement()?);
        }
        Ok(Program { body })
    }

    /// Satu pernyataan. Mengembalikan Vec karena satu deklarasi bisa berisi beberapa variabel.
    fn parse_statement(&mut self) -> PResult<Vec<Stmt>> {
        let t = self.peek(0)?;

        if Self::is_word(&t.tok, "{") {
            return Ok(vec![self.parse_block_stmt()?]);
        }
        if Self::is_word(&t.tok, ";") {
            self.bump()?;
            return Ok(vec![]);
        }

        if let Tok::Ident(word) = &t.tok {
            match word.as_str() {
                "export" => return self.parse_export(),
                "play" => return Ok(vec![self.parse_play()?]),
                "alias" => return Ok(vec![self.parse_alias()?]),
                "struct" => {
                    if self.is_at(2, "{")? {
                        return Ok(vec![self.parse_struct(false)?]);
                    }
                }
                "union" => {
                    if self.is_at(2, "{")? {
                        return Ok(vec![self.parse_struct(true)?]);
                    }
                }
                "try" => return Ok(vec![self.parse_try()?]),
                "throw" => return Ok(vec![self.parse_throw()?]),
                "extern" => return Ok(vec![self.parse_extern()?]),
                "asm" => {
                    if self.is_at(1, "{")? {
                        return Ok(vec![self.parse_asm()?]);
                    }
                }
                // Modifier tanpa efek di JS: hint untuk WASM atau penanda. Diparse lalu diabaikan.
                "reg" | "noreg" | "lock" | "interrupt" => {
                    self.bump()?;
                    return self.parse_statement();
                }
                "no_warn" => {
                    self.bump()?;
                    self.ident()?;
                    self.expect(";")?;
                    return Ok(vec![]);
                }
                "goto" => {
                    let kw = self.bump()?;
                    let (label, _) = self.ident()?;
                    self.expect(";")?;
                    return Ok(vec![Stmt { kind: StmtKind::Goto(label), span: kw.span }]);
                }
                "fun" => {
                    // `fun (` di posisi pernyataan adalah fungsi anonim (ekspresi).
                    if !self.is_at(1, "(")? {
                        return Ok(vec![self.parse_dyn_func()?]);
                    }
                }
                "if" => return Ok(vec![self.parse_if()?]),
                "for" => return Ok(vec![self.parse_for()?]),
                "while" => return Ok(vec![self.parse_while()?]),
                "do" => return Ok(vec![self.parse_do_while()?]),
                "switch" => return Ok(vec![self.parse_switch()?]),
                "return" => return Ok(vec![self.parse_return()?]),
                "break" => {
                    let kw = self.bump()?;
                    self.expect(";")?;
                    return Ok(vec![Stmt { kind: StmtKind::Break, span: kw.span }]);
                }
                "continue" => {
                    let kw = self.bump()?;
                    self.expect(";")?;
                    return Ok(vec![Stmt { kind: StmtKind::Continue, span: kw.span }]);
                }
                "debug" => {
                    if self.is_at(1, "{")? {
                        let kw = self.bump()?;
                        let body = self.parse_block_body()?;
                        return Ok(vec![Stmt { kind: StmtKind::Debug(body), span: kw.span }]);
                    }
                }
                "version" => {
                    if self.is_at(1, "(")? {
                        return Ok(vec![self.parse_static_if()?]);
                    }
                }
                "static" => {
                    if self.is_at(1, "if")? {
                        return Ok(vec![self.parse_static_if()?]);
                    }
                }
                "state" => return Ok(vec![self.parse_state()?]),
                "var" | "let" => {
                    let kind = if word == "let" { VarKind::Let } else { VarKind::Dyn };
                    self.bump()?;
                    let decls = self.parse_decl_list(None, kind)?;
                    self.expect(";")?;
                    return Ok(var_stmts(decls));
                }
                _ => {}
            }
        }

        // Blok berlabel: `nama: { ... }`
        if let Tok::Ident(name) = &t.tok {
            if !is_keyword(name) && self.is_at(1, ":")? && self.is_at(2, "{")? {
                let name = name.clone();
                self.bump()?;
                self.bump()?;
                let body = self.parse_block_body()?;
                return Ok(vec![Stmt {
                    kind: StmtKind::Labeled { name, body },
                    span: t.span,
                }]);
            }
        }

        if self.is_decl_start()? {
            return self.parse_typed_decl();
        }

        let expr = self.parse_expr()?;
        self.expect(";")?;
        Ok(vec![Stmt { span: expr.span, kind: StmtKind::Expr(expr) }])
    }

    /// Apakah posisi saat ini dimulai dengan deklarasi bertipe (`box14 x`, `Product *p`).
    fn is_decl_start(&mut self) -> PResult<bool> {
        let t = self.peek(0)?;
        let name = match &t.tok {
            Tok::Ident(s) => s.clone(),
            _ => return Ok(false),
        };

        if name == "struct" {
            let n = self.peek(1)?;
            return Ok(matches!(n.tok, Tok::Ident(_)));
        }
        if is_box_type(&name) {
            return Ok(true);
        }
        if is_keyword(&name) {
            return Ok(false);
        }

        let n = self.peek(1)?;
        match &n.tok {
            Tok::Ident(s) if !is_keyword(s) => return Ok(true),
            Tok::Op("*") => {
                let n2 = self.peek(2)?;
                return Ok(matches!(&n2.tok, Tok::Ident(s) if !is_keyword(s)));
            }
            _ => {}
        }
        Ok(false)
    }

    /// Body blok `{ ... }`, sebagai daftar statement.
    fn parse_block_body(&mut self) -> PResult<Vec<Stmt>> {
        self.expect("{")?;
        let mut body = Vec::new();
        loop {
            let t = self.peek(0)?;
            if Self::is_word(&t.tok, "}") {
                break;
            }
            if t.tok == Tok::Eof {
                return Err(Diag::new("Diharapkan '}' sebelum akhir file", t.span));
            }
            body.extend(self.parse_statement()?);
        }
        self.expect("}")?;
        Ok(body)
    }

    fn parse_block_stmt(&mut self) -> PResult<Stmt> {
        let span = self.peek(0)?.span;
        let body = self.parse_block_body()?;
        Ok(Stmt { kind: StmtKind::Block(body), span })
    }

    /// Body untuk if/for/while: blok atau satu pernyataan, selalu dibungkus Block.
    fn parse_body(&mut self) -> PResult<Stmt> {
        if self.is_at(0, "{")? {
            return self.parse_block_stmt();
        }
        let span = self.peek(0)?.span;
        let body = self.parse_statement()?;
        Ok(Stmt { kind: StmtKind::Block(body), span })
    }

    /// Body fungsi: mencatat apakah ada `await` di dalamnya.
    fn parse_fn_body(&mut self) -> PResult<(Vec<Stmt>, bool)> {
        self.fn_stack.push(false);
        let body = self.parse_block_body();
        let is_async = self.fn_stack.pop().unwrap_or(false);
        Ok((body?, is_async))
    }

    fn parse_export(&mut self) -> PResult<Vec<Stmt>> {
        let kw = self.bump()?;
        let is_default = self.eat("default")?;
        let mut stmts = self.parse_statement()?;
        if stmts.len() != 1 {
            return Err(Diag::new("'export' hanya berlaku untuk fungsi", kw.span));
        }
        match &mut stmts[0].kind {
            StmtKind::Func(f) => {
                f.exported = true;
                f.is_default = is_default;
            }
            _ => return Err(Diag::new("'export' hanya berlaku untuk fungsi", kw.span)),
        }
        Ok(stmts)
    }

    fn parse_play(&mut self) -> PResult<Stmt> {
        let kw = self.bump()?;
        let (mut module, _) = self.ident()?;
        while self.eat(".")? {
            let (part, _) = self.ident()?;
            module.push('.');
            module.push_str(&part);
        }

        let mut alias = None;
        let mut names = None;
        if self.eat("as")? {
            alias = Some(self.ident()?.0);
        } else if self.eat(":")? {
            let mut list = Vec::new();
            loop {
                list.push(self.ident()?.0);
                if !self.eat(",")? {
                    break;
                }
            }
            names = Some(list);
        }

        self.expect(";")?;
        Ok(Stmt {
            kind: StmtKind::Play(PlayDecl { module, alias, names }),
            span: kw.span,
        })
    }

    fn parse_alias(&mut self) -> PResult<Stmt> {
        let kw = self.bump()?;
        let (name, name_span) = self.ident()?;
        self.expect("=")?;
        let target = self.parse_type()?;
        self.expect(";")?;
        Ok(Stmt {
            kind: StmtKind::Alias(AliasDecl { name, target, span: name_span }),
            span: kw.span,
        })
    }

    fn parse_try(&mut self) -> PResult<Stmt> {
        let kw = self.bump()?;
        let body = self.parse_block_body()?;
        if !self.is_at(0, "catch")? {
            return Err(Diag::new("'try' membutuhkan 'catch'", kw.span));
        }
        self.bump()?;
        if self.eat("(")? {
            self.ident()?;
            self.expect(")")?;
        }
        let catch_body = Some(self.parse_block_body()?);
        Ok(Stmt {
            kind: StmtKind::Try { body, catch_body },
            span: kw.span,
        })
    }

    fn parse_throw(&mut self) -> PResult<Stmt> {
        let kw = self.bump()?;
        let e = self.parse_expr()?;
        self.expect(";")?;
        Ok(Stmt { kind: StmtKind::Throw(e), span: kw.span })
    }

    /// `asm { 0x41, 0x2a };`: hanya literal byte.
    fn parse_asm(&mut self) -> PResult<Stmt> {
        let kw = self.bump()?;
        self.expect("{")?;
        let mut bytes = Vec::new();
        while !self.is_at(0, "}")? {
            let t = self.peek(0)?;
            match t.tok {
                Tok::Int(v) if (0..=255).contains(&v) => {
                    self.bump()?;
                    bytes.push(v as u8);
                }
                _ => {
                    return Err(Diag::new(
                        "asm hanya menerima literal byte (0..255)",
                        t.span,
                    ))
                }
            }
            self.eat(",")?;
        }
        self.expect("}")?;
        self.eat(";")?;
        Ok(Stmt { kind: StmtKind::Asm(bytes), span: kw.span })
    }

    /// `extern fun Nama(a, b);` atau `extern box14 Nama(box14 a);`
    fn parse_extern(&mut self) -> PResult<Stmt> {
        let kw = self.bump()?;
        let (ret, name, name_span, params) = if self.eat("fun")? {
            let (n, s) = self.ident()?;
            self.expect("(")?;
            let names = self.parse_param_names()?;
            self.expect(")")?;
            let params = names
                .into_iter()
                .map(|(pn, ps)| Param { name: pn, ty: None, span: ps })
                .collect();
            (None, n, s, params)
        } else {
            let ty = self.parse_type()?;
            let (n, s) = self.ident()?;
            self.expect("(")?;
            let mut params = Vec::new();
            if !self.is_at(0, ")")? {
                loop {
                    let pt = self.parse_type()?;
                    let (pn, ps) = self.ident()?;
                    params.push(Param { name: pn, ty: Some(pt), span: ps });
                    if !self.eat(",")? {
                        break;
                    }
                }
            }
            self.expect(")")?;
            (Some(ty), n, s, params)
        };
        self.expect(";")?;
        Ok(Stmt {
            kind: StmtKind::Extern(ExternDecl { name, name_span, ret, params, span: kw.span }),
            span: kw.span,
        })
    }

    fn parse_struct(&mut self, is_union: bool) -> PResult<Stmt> {
        let kw = self.bump()?;
        let (name, name_span) = self.ident()?;
        self.expect("{")?;

        let mut fields = Vec::new();
        while !self.is_at(0, "}")? {
            let base = self.parse_type()?;
            loop {
                let (fname, fspan) = self.ident()?;
                let dims = self.parse_dims()?;
                fields.push(FieldDecl {
                    name: fname,
                    ty: base.clone(),
                    dims,
                    span: fspan,
                });
                if !self.eat(",")? {
                    break;
                }
            }
            self.expect(";")?;
        }
        self.expect("}")?;
        self.eat(";")?;

        Ok(Stmt {
            kind: StmtKind::Struct(StructDecl { name, fields, is_union, span: name_span }),
            span: kw.span.join(name_span),
        })
    }

    /// `[expr]` opsional setelah nama variabel atau field.
    fn parse_dims(&mut self) -> PResult<Option<Expr>> {
        if self.eat("[")? {
            let e = self.parse_expr()?;
            self.expect("]")?;
            Ok(Some(e))
        } else {
            Ok(None)
        }
    }

    fn parse_if(&mut self) -> PResult<Stmt> {
        let kw = self.bump()?;
        self.expect("(")?;
        let test = self.parse_expr()?;
        self.expect(")")?;
        let cons = self.parse_body()?;

        let alt = if self.eat("else")? {
            if self.is_at(0, "if")? {
                Some(Box::new(self.parse_if()?))
            } else {
                Some(Box::new(self.parse_body()?))
            }
        } else {
            None
        };

        Ok(Stmt {
            kind: StmtKind::If { test, cons: Box::new(cons), alt },
            span: kw.span,
        })
    }

    fn parse_while(&mut self) -> PResult<Stmt> {
        let kw = self.bump()?;
        self.expect("(")?;
        let test = self.parse_expr()?;
        self.expect(")")?;
        let body = self.parse_body()?;
        Ok(Stmt {
            kind: StmtKind::While { test, body: Box::new(body) },
            span: kw.span,
        })
    }

    fn parse_do_while(&mut self) -> PResult<Stmt> {
        let kw = self.bump()?;
        let body = self.parse_body()?;
        self.expect("while")?;
        self.expect("(")?;
        let test = self.parse_expr()?;
        self.expect(")")?;
        self.expect(";")?;
        Ok(Stmt {
            kind: StmtKind::DoWhile { body: Box::new(body), test },
            span: kw.span,
        })
    }

    fn parse_for(&mut self) -> PResult<Stmt> {
        let kw = self.bump()?;
        self.expect("(")?;

        let init = if self.eat("var")? {
            Some(ForInit::Decls(self.parse_decl_list(None, VarKind::Dyn)?))
        } else if self.eat("let")? {
            Some(ForInit::Decls(self.parse_decl_list(None, VarKind::Let)?))
        } else if self.is_at(0, ";")? {
            None
        } else if self.is_decl_start()? {
            let ty = self.parse_type()?;
            Some(ForInit::Decls(self.parse_decl_list(Some(ty), VarKind::Typed)?))
        } else {
            Some(ForInit::Expr(self.parse_expr()?))
        };
        self.expect(";")?;

        let test = if self.is_at(0, ";")? { None } else { Some(self.parse_expr()?) };
        self.expect(";")?;

        let update = if self.is_at(0, ")")? { None } else { Some(self.parse_expr()?) };
        self.expect(")")?;

        let body = self.parse_body()?;
        Ok(Stmt {
            kind: StmtKind::For {
                init,
                test,
                update,
                body: Box::new(body),
            },
            span: kw.span,
        })
    }

    fn parse_switch(&mut self) -> PResult<Stmt> {
        let kw = self.bump()?;
        self.expect("(")?;
        let disc = self.parse_expr()?;
        self.expect(")")?;
        // `start ... end` (gaya HolyC) atau `{ ... }`.
        let closer = if self.eat("start")? {
            "end"
        } else {
            self.expect("{")?;
            "}"
        };

        let mut cases = Vec::new();
        while !self.is_at(0, closer)? {
            let test = if self.eat("case")? {
                Some(self.parse_expr()?)
            } else if self.eat("default")? {
                None
            } else {
                let t = self.peek(0)?;
                return Err(Diag::new(
                    format!("Diharapkan 'case' atau 'default', ditemukan {}", describe(&t.tok)),
                    t.span,
                ));
            };
            self.expect(":")?;

            let mut body = Vec::new();
            while !(self.is_at(0, "case")? || self.is_at(0, "default")? || self.is_at(0, closer)?) {
                body.extend(self.parse_statement()?);
            }
            cases.push(SwitchCase { test, body });
        }
        self.expect(closer)?;

        Ok(Stmt {
            kind: StmtKind::Switch { disc, cases },
            span: kw.span,
        })
    }

    fn parse_return(&mut self) -> PResult<Stmt> {
        let kw = self.bump()?;
        let arg = if self.is_at(0, ";")? { None } else { Some(self.parse_expr()?) };
        self.expect(";")?;
        Ok(Stmt { kind: StmtKind::Return(arg), span: kw.span })
    }

    /// `version (X) { } else version (Y) { } else { }` dan `static if (cond) { } else { }`.
    fn parse_static_if(&mut self) -> PResult<Stmt> {
        let kw = self.peek(0)?;
        let cond = if self.eat("version")? {
            self.expect("(")?;
            let (name, span) = self.ident()?;
            self.expect(")")?;
            Expr { kind: ExprKind::Ident(name), span }
        } else {
            self.expect("static")?;
            self.expect("if")?;
            self.expect("(")?;
            let e = self.parse_expr()?;
            self.expect(")")?;
            e
        };

        let then = self.parse_block_body()?;
        let alt = if self.eat("else")? {
            if self.is_at(0, "version")? || self.is_at(0, "static")? {
                vec![self.parse_static_if()?]
            } else {
                self.parse_block_body()?
            }
        } else {
            Vec::new()
        };

        Ok(Stmt {
            kind: StmtKind::StaticIf { cond, then, alt },
            span: kw.span,
        })
    }

    /// `state nama = nilai;` (hanya valid di dalam fungsi; diperiksa oleh analyzer).
    fn parse_state(&mut self) -> PResult<Stmt> {
        let kw = self.bump()?;
        let (name, name_span) = self.ident()?;
        let init = if self.eat("=")? { Some(self.parse_expr()?) } else { None };
        self.expect(";")?;
        Ok(Stmt {
            kind: StmtKind::Var(VarDecl {
                name,
                kind: VarKind::State,
                ty: None,
                dims: None,
                init,
                span: name_span,
            }),
            span: kw.span,
        })
    }

    // ---------- Deklarasi ----------

    fn parse_type(&mut self) -> PResult<TypeExpr> {
        if self.is_at(0, "struct")? {
            self.bump()?;
        }
        let (name, span) = self.ident()?;
        let mut ty = TypeExpr::Name { name, span };
        while self.eat("*")? {
            ty = TypeExpr::Ptr(Box::new(ty));
        }
        Ok(ty)
    }

    /// Deklarasi bertipe: variabel (`box14 a = 1, b;`) atau fungsi bertipe (`box14 F() {}`).
    fn parse_typed_decl(&mut self) -> PResult<Vec<Stmt>> {
        let ty = self.parse_type()?;
        let (name, name_span) = self.ident()?;

        if self.is_at(0, "(")? {
            return Ok(vec![self.parse_typed_func(ty, name, name_span)?]);
        }

        let mut decls = vec![self.parse_decl_rest(name, name_span, Some(ty.clone()), VarKind::Typed)?];
        while self.eat(",")? {
            let (n, s) = self.ident()?;
            decls.push(self.parse_decl_rest(n, s, Some(ty.clone()), VarKind::Typed)?);
        }
        self.expect(";")?;
        Ok(var_stmts(decls))
    }

    fn parse_decl_list(&mut self, ty: Option<TypeExpr>, kind: VarKind) -> PResult<Vec<VarDecl>> {
        let mut out = Vec::new();
        loop {
            let (name, span) = self.ident()?;
            out.push(self.parse_decl_rest(name, span, ty.clone(), kind)?);
            if !self.eat(",")? {
                break;
            }
        }
        Ok(out)
    }

    /// Sisa deklarasi setelah nama: dimensi array dan inisialisasi, keduanya opsional.
    fn parse_decl_rest(
        &mut self,
        name: String,
        span: Span,
        ty: Option<TypeExpr>,
        kind: VarKind,
    ) -> PResult<VarDecl> {
        let dims = self.parse_dims()?;
        let init = if self.eat("=")? { Some(self.parse_assign()?) } else { None };
        Ok(VarDecl { name, kind, ty, dims, init, span })
    }

    fn parse_typed_func(&mut self, ret: TypeExpr, name: String, name_span: Span) -> PResult<Stmt> {
        self.expect("(")?;
        let mut params = Vec::new();
        if !self.is_at(0, ")")? {
            loop {
                let ty = self.parse_type()?;
                let (pname, pspan) = self.ident()?;
                params.push(Param { name: pname, ty: Some(ty), span: pspan });
                if !self.eat(",")? {
                    break;
                }
            }
        }
        self.expect(")")?;
        let (body, is_async) = self.parse_fn_body()?;

        Ok(Stmt {
            kind: StmtKind::Func(FuncDecl {
                name,
                name_span,
                ret: Some(ret),
                params,
                body,
                dynamic: false,
                is_async,
                exported: false,
                is_default: false,
                span: name_span,
            }),
            span: name_span,
        })
    }

    fn parse_dyn_func(&mut self) -> PResult<Stmt> {
        let kw = self.bump()?;
        let (name, name_span) = self.ident()?;
        self.expect("(")?;
        let names = self.parse_param_names()?;
        self.expect(")")?;
        let (body, is_async) = self.parse_fn_body()?;

        let params = names
            .into_iter()
            .map(|(n, s)| Param { name: n, ty: None, span: s })
            .collect();

        Ok(Stmt {
            kind: StmtKind::Func(FuncDecl {
                name,
                name_span,
                ret: None,
                params,
                body,
                dynamic: true,
                is_async,
                exported: false,
                is_default: false,
                span: kw.span,
            }),
            span: kw.span,
        })
    }

    /// Daftar nama parameter tanpa tipe, tanpa tanda kurung pembuka dan penutup.
    fn parse_param_names(&mut self) -> PResult<Vec<(String, Span)>> {
        let mut out = Vec::new();
        if self.is_at(0, ")")? {
            return Ok(out);
        }
        loop {
            out.push(self.ident()?);
            if !self.eat(",")? {
                break;
            }
        }
        Ok(out)
    }

    // ---------- Ekspresi ----------

    pub fn parse_expr(&mut self) -> PResult<Expr> {
        self.parse_assign()
    }

    fn parse_assign(&mut self) -> PResult<Expr> {
        if let Some(arrow) = self.try_arrow()? {
            return Ok(arrow);
        }

        let left = self.parse_cond()?;
        let t = self.peek(0)?;
        if let Tok::Op(op) = t.tok {
            if ASSIGN_OPS.contains(&op) {
                self.bump()?;
                let value = self.parse_assign()?;
                let span = left.span.join(value.span);
                return Ok(Expr {
                    kind: ExprKind::Assign {
                        op,
                        target: Box::new(left),
                        value: Box::new(value),
                    },
                    span,
                });
            }
        }
        Ok(left)
    }

    /// Mendeteksi `x => ...`, `(a, b) => ...`, dan `fun(...) {...}` tidak termasuk di sini.
    fn try_arrow(&mut self) -> PResult<Option<Expr>> {
        let t = self.peek(0)?;

        if let Tok::Ident(name) = &t.tok {
            if !is_keyword(name) && self.is_at(1, "=>")? {
                let name = name.clone();
                self.bump()?;
                self.bump()?;
                return Ok(Some(self.arrow_body(vec![name], t.span)?));
            }
            return Ok(None);
        }

        if !Self::is_word(&t.tok, "(") {
            return Ok(None);
        }

        // Lookahead: cari ')' yang sesuai lalu cek '=>'. Posisi lexer dipulihkan sesudahnya,
        // sehingga teks JSX yang tidak valid sebagai kode tidak merusak parsing berikutnya.
        let saved_pos = self.lex.pos;
        let saved_buf = self.buf.clone();
        let is_arrow = self.scan_arrow_head().unwrap_or(false);
        self.lex.pos = saved_pos;
        self.buf = saved_buf;

        if !is_arrow {
            return Ok(None);
        }

        self.bump()?; // '('
        let names = self.parse_param_names()?;
        self.expect(")")?;
        self.expect("=>")?;
        let params = names.into_iter().map(|(n, _)| n).collect();
        Ok(Some(self.arrow_body(params, t.span)?))
    }

    /// Dari '(' di posisi 0, cari ')' penutup yang seimbang lalu cek apakah diikuti '=>'.
    fn scan_arrow_head(&mut self) -> PResult<bool> {
        let mut depth: usize = 0;
        let mut k = 0usize;
        loop {
            let t = self.peek(k)?;
            match t.tok {
                Tok::Eof => return Ok(false),
                Tok::Op("(") | Tok::Op("[") | Tok::Op("{") => depth += 1,
                Tok::Op(")") | Tok::Op("]") | Tok::Op("}") => {
                    depth -= 1;
                    if depth == 0 {
                        return self.is_at(k + 1, "=>");
                    }
                }
                _ => {}
            }
            k += 1;
        }
    }

    fn arrow_body(&mut self, params: Vec<String>, span: Span) -> PResult<Expr> {
        let is_block = self.is_at(0, "{")?;
        self.fn_stack.push(false);
        let body = if is_block {
            self.parse_block_body().map(ArrowBody::Block)
        } else {
            self.parse_assign().map(|e| ArrowBody::Expr(Box::new(e)))
        };
        let is_async = self.fn_stack.pop().unwrap_or(false);
        Ok(Expr {
            kind: ExprKind::Arrow {
                params,
                body: body?,
                is_async,
            },
            span,
        })
    }

    /// Fungsi anonim `fun(a, b) { ... }` sebagai ekspresi.
    fn anon_fun(&mut self) -> PResult<Expr> {
        let kw = self.bump()?;
        self.expect("(")?;
        let names = self.parse_param_names()?;
        self.expect(")")?;
        let params = names.into_iter().map(|(n, _)| n).collect();
        let (body, is_async) = self.parse_fn_body()?;
        Ok(Expr {
            kind: ExprKind::Arrow {
                params,
                body: ArrowBody::Block(body),
                is_async,
            },
            span: kw.span,
        })
    }

    fn parse_cond(&mut self) -> PResult<Expr> {
        let test = self.parse_binary(1)?;
        if self.is_at(0, "?")? {
            self.bump()?;
            let cons = self.parse_assign()?;
            self.expect(":")?;
            let alt = self.parse_assign()?;
            let span = test.span.join(alt.span);
            return Ok(Expr {
                kind: ExprKind::Cond {
                    test: Box::new(test),
                    cons: Box::new(cons),
                    alt: Box::new(alt),
                },
                span,
            });
        }
        Ok(test)
    }

    fn parse_binary(&mut self, min_prec: u8) -> PResult<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            let t = self.peek(0)?;
            let op = match t.tok {
                Tok::Op(o) => o,
                _ => break,
            };
            let prec = match binary_prec(op) {
                Some(p) if p >= min_prec => p,
                _ => break,
            };
            self.bump()?;
            let right = self.parse_binary(prec + 1)?;
            let span = left.span.join(right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    op,
                    l: Box::new(left),
                    r: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> PResult<Expr> {
        let t = self.peek(0)?;
        match &t.tok {
            Tok::Op(op) if matches!(*op, "!" | "~" | "-" | "+" | "&" | "*") => {
                let op = *op;
                self.bump()?;
                let arg = self.parse_unary()?;
                let span = t.span.join(arg.span);
                Ok(Expr {
                    kind: ExprKind::Unary { op, arg: Box::new(arg) },
                    span,
                })
            }
            Tok::Op(op) if *op == "++" || *op == "--" => {
                let op = *op;
                self.bump()?;
                let arg = self.parse_unary()?;
                let span = t.span.join(arg.span);
                Ok(Expr {
                    kind: ExprKind::Update { op, arg: Box::new(arg), prefix: true },
                    span,
                })
            }
            Tok::Ident(w) if w.as_str() == "await" => {
                self.bump()?;
                match self.fn_stack.last_mut() {
                    Some(flag) => *flag = true,
                    None => {
                        return Err(Diag::new("'await' hanya boleh di dalam fungsi", t.span));
                    }
                }
                let arg = self.parse_unary()?;
                let span = t.span.join(arg.span);
                Ok(Expr { kind: ExprKind::Await(Box::new(arg)), span })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> PResult<Expr> {
        let e = self.parse_call()?;
        let t = self.peek(0)?;
        if let Tok::Op(op) = t.tok {
            if op == "++" || op == "--" {
                self.bump()?;
                let span = e.span.join(t.span);
                return Ok(Expr {
                    kind: ExprKind::Update { op, arg: Box::new(e), prefix: false },
                    span,
                });
            }
        }
        Ok(e)
    }

    /// Primary diikuti rantai akses: `.prop`, `->prop`, `[index]`, dan `(args)`.
    fn parse_call(&mut self) -> PResult<Expr> {
        let mut e = self.parse_primary()?;
        loop {
            let t = self.peek(0)?;
            let op = match t.tok {
                Tok::Op(o) => o,
                _ => break,
            };
            match op {
                "." | "->" => {
                    self.bump()?;
                    let (prop, pspan) = self.ident()?;
                    let span = e.span.join(pspan);
                    e = Expr {
                        kind: ExprKind::Member {
                            obj: Box::new(e),
                            prop,
                            arrow: op == "->",
                        },
                        span,
                    };
                }
                "[" => {
                    self.bump()?;
                    let index = self.parse_expr()?;
                    let close = self.expect("]")?;
                    let span = e.span.join(close.span);
                    e = Expr {
                        kind: ExprKind::Index {
                            obj: Box::new(e),
                            index: Box::new(index),
                        },
                        span,
                    };
                }
                "(" => {
                    self.bump()?;
                    let mut args = Vec::new();
                    if !self.is_at(0, ")")? {
                        loop {
                            args.push(self.parse_assign()?);
                            if !self.eat(",")? {
                                break;
                            }
                        }
                    }
                    let close = self.expect(")")?;
                    let span = e.span.join(close.span);
                    e = Expr {
                        kind: ExprKind::Call {
                            callee: Box::new(e),
                            args,
                        },
                        span,
                    };
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn parse_primary(&mut self) -> PResult<Expr> {
        let t = self.peek(0)?;
        match &t.tok {
            Tok::Int(v) => {
                self.bump()?;
                Ok(Expr { kind: ExprKind::Int(*v), span: t.span })
            }
            Tok::Float(v) => {
                self.bump()?;
                Ok(Expr { kind: ExprKind::Float(*v), span: t.span })
            }
            Tok::Str(s) => {
                let s = s.clone();
                self.bump()?;
                Ok(Expr { kind: ExprKind::Str(s), span: t.span })
            }
            Tok::Ident(w) => {
                let w = w.clone();
                match w.as_str() {
                    "true" => {
                        self.bump()?;
                        Ok(Expr { kind: ExprKind::Bool(true), span: t.span })
                    }
                    "false" => {
                        self.bump()?;
                        Ok(Expr { kind: ExprKind::Bool(false), span: t.span })
                    }
                    "null" => {
                        self.bump()?;
                        Ok(Expr { kind: ExprKind::Null, span: t.span })
                    }
                    "fun" => {
                        if self.is_at(1, "(")? {
                            return self.anon_fun();
                        }
                        Err(Diag::new(
                            "Kata kunci 'fun' tidak dapat dipakai sebagai ekspresi",
                            t.span,
                        ))
                    }
                    "offset" => {
                        if !self.is_at(1, "(")? {
                            return Err(Diag::new("'offset' harus diikuti '(Tipe, field)'", t.span));
                        }
                        self.bump()?;
                        self.bump()?;
                        let (ty, _) = self.ident()?;
                        self.expect(",")?;
                        let (field, _) = self.ident()?;
                        let close = self.expect(")")?;
                        Ok(Expr {
                            kind: ExprKind::Offset { ty, field },
                            span: t.span.join(close.span),
                        })
                    }
                    _ if is_keyword(&w) => Err(Diag::new(
                        format!("Kata kunci '{}' tidak dapat dipakai sebagai ekspresi", w),
                        t.span,
                    )),
                    _ => {
                        self.bump()?;
                        Ok(Expr { kind: ExprKind::Ident(w), span: t.span })
                    }
                }
            }
            Tok::Op("(") => {
                self.bump()?;
                let e = self.parse_expr()?;
                self.expect(")")?;
                Ok(e)
            }
            Tok::Op("{") => self.parse_brace(),
            Tok::Op("<") => {
                let (el, end) = self.jsx_element(t.span.start)?;
                self.reset_to(end);
                Ok(Expr {
                    kind: ExprKind::Jsx(el),
                    span: Span::new(t.span.start, end),
                })
            }
            _ => Err(Diag::new(
                format!("Ekspresi diharapkan, ditemukan {}", describe(&t.tok)),
                t.span,
            )),
        }
    }

    /// `{}`, `{a, b}` (positional), atau `{ key: value }` (objek).
    fn parse_brace(&mut self) -> PResult<Expr> {
        let open = self.bump()?;
        if self.is_at(0, "}")? {
            let close = self.bump()?;
            return Ok(Expr {
                kind: ExprKind::Brace(Vec::new()),
                span: open.span.join(close.span),
            });
        }

        let t0 = self.peek(0)?;
        let t1 = self.peek(1)?;
        let is_object = matches!(t0.tok, Tok::Ident(_) | Tok::Str(_)) && Self::is_word(&t1.tok, ":");

        if is_object {
            let mut props = Vec::new();
            loop {
                let k = self.bump()?;
                let key = match &k.tok {
                    Tok::Ident(s) | Tok::Str(s) => s.clone(),
                    _ => {
                        return Err(Diag::new(
                            format!("Nama properti diharapkan, ditemukan {}", describe(&k.tok)),
                            k.span,
                        ))
                    }
                };
                self.expect(":")?;
                let value = self.parse_assign()?;
                props.push((key, value));
                if !self.eat(",")? || self.is_at(0, "}")? {
                    break;
                }
            }
            let close = self.expect("}")?;
            return Ok(Expr {
                kind: ExprKind::ObjectLit(props),
                span: open.span.join(close.span),
            });
        }

        let mut elems = Vec::new();
        while !self.is_at(0, "}")? {
            elems.push(self.parse_assign()?);
            if !self.eat(",")? {
                break;
            }
        }
        let close = self.expect("}")?;
        Ok(Expr {
            kind: ExprKind::Brace(elems),
            span: open.span.join(close.span),
        })
    }

    // ---------- JSX (pemindaian mentah) ----------

    fn raw_ws(&self, mut p: usize) -> usize {
        let b = self.src().as_bytes();
        while p < b.len() && b[p].is_ascii_whitespace() {
            p += 1;
        }
        p
    }

    /// Mengurai elemen JSX yang dimulai dari '<' di posisi `start`.
    /// Mengembalikan (elemen, posisi setelah elemen).
    fn jsx_element(&mut self, start: usize) -> PResult<(JsxElement, usize)> {
        let src = self.src();
        let b = src.as_bytes();
        let mut p = self.raw_ws(start + 1);

        // Fragment: <> ... </>
        if b.get(p).copied() == Some(b'>') {
            let (children, q) = self.jsx_children(p + 1, start)?;
            let c = self.raw_ws(q + 2);
            if b.get(c).copied() != Some(b'>') {
                return Err(Diag::new(
                    "Penutup fragment harus '</>'",
                    Span::new(c, (c + 1).min(b.len())),
                ));
            }
            let el = JsxElement {
                tag: None,
                attrs: Vec::new(),
                children,
                span: Span::new(start, c + 1),
            };
            return Ok((el, c + 1));
        }

        let name_end = scan_name(b, p, false);
        if name_end == p {
            return Err(Diag::new(
                "Nama tag JSX diharapkan",
                Span::new(p, (p + 1).min(b.len())),
            ));
        }
        let tag = src[p..name_end].to_string();
        p = self.raw_ws(name_end);

        let (attrs, after) = self.jsx_attrs(p)?;
        p = after;

        // Elemen tanpa anak: <tag ... />
        if src[p..].starts_with("/>") {
            let el = JsxElement {
                tag: Some(tag),
                attrs,
                children: Vec::new(),
                span: Span::new(start, p + 2),
            };
            return Ok((el, p + 2));
        }

        if b.get(p).copied() != Some(b'>') {
            return Err(Diag::new(
                format!("Diharapkan '>' pada tag <{}>", tag),
                Span::new(p, (p + 1).min(b.len())),
            ));
        }

        let (children, close_at) = self.jsx_children(p + 1, start)?;
        let name_start = self.raw_ws(close_at + 2);
        let close_end = scan_name(b, name_start, false);
        let close_name = &src[name_start..close_end];
        if close_name != tag.as_str() {
            return Err(Diag::new(
                format!("Tag penutup </{}> tidak cocok dengan <{}>", close_name, tag),
                Span::new(close_at, (close_at + 2).min(b.len())),
            ));
        }

        let c = self.raw_ws(close_end);
        if b.get(c).copied() != Some(b'>') {
            return Err(Diag::new(
                format!("Diharapkan '>' pada penutup </{}>", tag),
                Span::new(c, (c + 1).min(b.len())),
            ));
        }

        let el = JsxElement {
            tag: Some(tag),
            attrs,
            children,
            span: Span::new(start, c + 1),
        };
        Ok((el, c + 1))
    }

    /// Atribut tag JSX. Mengembalikan (atribut, posisi setelah atribut).
    fn jsx_attrs(&mut self, mut p: usize) -> PResult<(Vec<JsxAttr>, usize)> {
        let src = self.src();
        let b = src.as_bytes();
        let mut attrs = Vec::new();

        loop {
            p = self.raw_ws(p);
            if p >= b.len() {
                return Err(Diag::new("Tag JSX tidak ditutup", Span::new(p, p)));
            }
            if b[p] == b'/' || b[p] == b'>' {
                break;
            }

            // Spread: {...expr}
            if b[p] == b'{' {
                let q = self.raw_ws(p + 1);
                if !src[q..].starts_with("...") {
                    return Err(Diag::new(
                        "Diharapkan '...' pada spread atribut",
                        Span::new(q, (q + 1).min(b.len())),
                    ));
                }
                let (expr, end) = self.expr_at(q + 3)?;
                attrs.push(JsxAttr::Spread(expr));
                p = end;
                continue;
            }

            let name_end = scan_name(b, p, true);
            if name_end == p {
                return Err(Diag::new(
                    "Nama atribut JSX diharapkan",
                    Span::new(p, (p + 1).min(b.len())),
                ));
            }
            let name = src[p..name_end].to_string();
            p = self.raw_ws(name_end);

            let mut value = None;
            if b.get(p).copied() == Some(b'=') {
                p = self.raw_ws(p + 1);
                match b.get(p).copied() {
                    Some(q @ (b'"' | b'\'')) => {
                        let close = match src[p + 1..].find(q as char) {
                            Some(i) => p + 1 + i,
                            None => {
                                return Err(Diag::new(
                                    "Nilai atribut tidak ditutup",
                                    Span::new(p, p + 1),
                                ))
                            }
                        };
                        value = Some(Expr {
                            kind: ExprKind::Str(src[p + 1..close].to_string()),
                            span: Span::new(p, close + 1),
                        });
                        p = close + 1;
                    }
                    Some(b'{') => {
                        let (expr, end) = self.expr_at(p + 1)?;
                        value = Some(expr);
                        p = end;
                    }
                    _ => {
                        return Err(Diag::new(
                            "Nilai atribut harus berupa string atau {ekspresi}",
                            Span::new(p, (p + 1).min(b.len())),
                        ))
                    }
                }
            }
            attrs.push(JsxAttr::Named { name, value });
        }

        Ok((attrs, p))
    }

    /// Anak-anak elemen sampai bertemu "</". Mengembalikan (anak, posisi '<' penutup).
    fn jsx_children(&mut self, mut p: usize, start: usize) -> PResult<(Vec<JsxChild>, usize)> {
        let src = self.src();
        let b = src.as_bytes();
        let mut children = Vec::new();

        loop {
            if p >= b.len() {
                return Err(Diag::new(
                    "Elemen JSX tidak ditutup",
                    Span::new(start, (start + 1).min(b.len())),
                ));
            }

            if b[p] == b'<' && b.get(p + 1).copied() == Some(b'/') {
                return Ok((children, p));
            }

            if b[p] == b'<' {
                let (el, end) = self.jsx_element(p)?;
                children.push(JsxChild::Element(el));
                p = end;
                continue;
            }

            if b[p] == b'{' {
                if let Some(end) = self.skip_empty_expr(p + 1) {
                    p = end;
                    continue;
                }
                let (expr, end) = self.expr_at(p + 1)?;
                children.push(JsxChild::Expr(expr));
                p = end;
                continue;
            }

            let mut q = p;
            while q < b.len() && b[q] != b'<' && b[q] != b'{' {
                q += 1;
            }
            let text = jsx_text(&src[p..q]);
            if !text.is_empty() {
                children.push(JsxChild::Text(text));
            }
            p = q;
        }
    }

    /// `{}` atau `{/* komentar */}` tanpa isi; mengembalikan posisi setelah '}'.
    fn skip_empty_expr(&self, mut q: usize) -> Option<usize> {
        let src = self.src();
        let b = src.as_bytes();
        loop {
            while q < b.len() && b[q].is_ascii_whitespace() {
                q += 1;
            }
            if src[q..].starts_with("/*") {
                match src[q + 2..].find("*/") {
                    Some(i) => q = q + 2 + i + 2,
                    None => return None,
                }
                continue;
            }
            break;
        }
        if q < b.len() && b[q] == b'}' {
            Some(q + 1)
        } else {
            None
        }
    }

    /// Mengurai ekspresi `{...}` yang isinya dimulai di posisi `p`.
    /// Mengembalikan (ekspresi, posisi setelah '}').
    fn expr_at(&mut self, p: usize) -> PResult<(Expr, usize)> {
        self.reset_to(p);
        let expr = self.parse_expr()?;
        let close = self.peek(0)?;
        if !Self::is_word(&close.tok, "}") {
            return Err(Diag::new(
                "'}' diharapkan untuk menutup ekspresi JSX",
                close.span,
            ));
        }
        Ok((expr, close.span.end))
    }
}
