//! Emitter WebAssembly untuk fungsi bertipe skalar (tipe box, U/I/F, dan Bool).
//!
//! Cakupan v1: literal, variabel lokal, parameter, aritmetika, perbandingan, logika,
//! `if`, `while`, `for`, `break`, `continue`, `return`, pemanggilan fungsi skalar lain,
//! dan `Cast` antar tipe skalar.
//!
//! Di luar cakupan (struct, pointer, array, fungsi dinamis, pemanggilan JS): error yang jelas.
//! Fungsi yang tanda tangannya tidak skalar dilewati dan tidak dihasilkan.

use std::collections::HashMap;

use crate::ast::*;
use crate::codegen::keyword_alias;
use crate::error::{Diag, Span};
use crate::wasm_encode::{encode, uleb, ValTy, WasmFunc};

/// Tipe skalar di sisi WASM: tipe nilai, signedness, dan lebar bit logis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Num {
    pub val: ValTy,
    pub signed: bool,
    pub bits: u8,
}

const BOOL: Num = Num { val: ValTy::I32, signed: false, bits: 1 };
const NUM_I32: Num = Num { val: ValTy::I32, signed: true, bits: 32 };

/// Pemetaan nama tipe ke tipe WASM. Keyword HolyC (U8, I32, F64, ...) lewat `keyword_alias`.
pub fn num_of(name: &str) -> Option<Num> {
    let canon = keyword_alias(name).unwrap_or(name);
    let n = |val: ValTy, signed: bool, bits: u8| Some(Num { val, signed, bits });
    match canon {
        "bool" => Some(BOOL),
        "box01" => n(ValTy::I32, false, 8),
        "box11" => n(ValTy::I32, true, 8),
        "box02" => n(ValTy::I32, false, 16),
        "box12" => n(ValTy::I32, true, 16),
        "box04" => n(ValTy::I32, false, 32),
        "box14" => n(ValTy::I32, true, 32),
        "box08" => n(ValTy::I64, false, 64),
        "box18" => n(ValTy::I64, true, 64),
        "box24" => n(ValTy::F32, true, 32),
        "box28" => n(ValTy::F64, true, 64),
        _ => None,
    }
}

#[derive(Clone, Debug)]
struct Sig {
    params: Vec<Num>,
    ret: Option<Num>,
    index: usize,
}

fn type_name(ty: &TypeExpr) -> Option<&str> {
    match ty {
        TypeExpr::Name { name, .. } => Some(name.as_str()),
        _ => None,
    }
}

/// Tanda tangan skalar sebuah fungsi, atau None jika ada bagian yang bukan skalar.
fn scalar_sig(f: &FuncDecl) -> Option<(Vec<Num>, Option<Num>)> {
    if f.dynamic {
        return None;
    }
    let mut params = Vec::new();
    for p in &f.params {
        params.push(num_of(type_name(p.ty.as_ref()?)?)?);
    }
    let ret = match type_name(f.ret.as_ref()?)? {
        "box00" | "U0" => None,
        other => Some(num_of(other)?),
    };
    Some((params, ret))
}

/// Hasil untuk `funny build`: binary modul dan nama fungsi yang berhasil ke WASM.
pub struct ScalarModule {
    pub bytes: Vec<u8>,
    pub names: Vec<String>,
}

/// Mengompilasi fungsi skalar yang bisa ke WASM. Fungsi yang gagal dibuang dari modul
/// (dan fungsi yang memanggilnya ikut gagal), lalu diulang sampai stabil. Fungsi yang
/// gagal tetap dihasilkan sebagai JavaScript oleh codegen, jadi tidak ada error di sini.
pub fn emit_scalar(program: &Program) -> ScalarModule {
    let mut cand: Vec<(&FuncDecl, Vec<Num>, Option<Num>)> = Vec::new();
    for s in &program.body {
        if let StmtKind::Func(f) = &s.kind {
            if let Some((p, r)) = scalar_sig(f) {
                cand.push((f, p, r));
            }
        }
    }
    loop {
        let mut sigs: HashMap<String, Sig> = HashMap::new();
        for (i, (f, p, r)) in cand.iter().enumerate() {
            sigs.insert(f.name.clone(), Sig { params: p.clone(), ret: *r, index: i });
        }
        let mut compiled = Vec::new();
        let mut failed: Vec<String> = Vec::new();
        for (f, p, r) in &cand {
            match compile_fn(f, p, *r, &sigs) {
                Ok(w) => compiled.push(w),
                Err(_) => failed.push(f.name.clone()),
            }
        }
        if failed.is_empty() {
            let exports: Vec<(String, usize)> = cand
                .iter()
                .enumerate()
                .map(|(i, (f, _, _))| (f.name.clone(), i))
                .collect();
            let names = exports.iter().map(|(n, _)| n.clone()).collect();
            return ScalarModule { bytes: encode(&compiled, &exports), names };
        }
        cand.retain(|(f, _, _)| !failed.contains(&f.name));
    }
}

/// Menghasilkan binary WASM untuk semua fungsi bertipe skalar; error jika ada yang gagal.
pub fn emit_module(program: &Program) -> Result<Vec<u8>, Vec<Diag>> {
    let mut funcs: Vec<(&FuncDecl, Vec<Num>, Option<Num>)> = Vec::new();
    for s in &program.body {
        if let StmtKind::Func(f) = &s.kind {
            if let Some((p, r)) = scalar_sig(f) {
                funcs.push((f, p, r));
            }
        }
    }

    let mut sigs: HashMap<String, Sig> = HashMap::new();
    for (i, (f, p, r)) in funcs.iter().enumerate() {
        sigs.insert(f.name.clone(), Sig { params: p.clone(), ret: *r, index: i });
    }

    let mut compiled = Vec::new();
    let mut errors = Vec::new();
    for (f, p, r) in &funcs {
        match compile_fn(f, p, *r, &sigs) {
            Ok(w) => compiled.push(w),
            Err(d) => errors.push(d),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let exports: Vec<(String, usize)> = funcs
        .iter()
        .enumerate()
        .map(|(i, (f, _, _))| (f.name.clone(), i))
        .collect();
    Ok(encode(&compiled, &exports))
}

#[derive(Clone, Copy)]
struct Label {
    /// Target `break` (blok di luar loop).
    brk: bool,
    /// Target `continue` (blok di dalam loop, sebelum update).
    cont: bool,
}

struct Ctx<'a> {
    sigs: &'a HashMap<String, Sig>,
    /// Variabel lokal yang terlihat: (nama, tipe, indeks lokal WASM). Dipotong saat blok selesai.
    scope: Vec<(String, Num, u32)>,
    nparams: u32,
    /// Lokal tambahan (di luar parameter).
    extra: Vec<ValTy>,
    code: Vec<u8>,
    labels: Vec<Label>,
    ret: Option<Num>,
}

fn compile_fn(
    f: &FuncDecl,
    params: &[Num],
    ret: Option<Num>,
    sigs: &HashMap<String, Sig>,
) -> Result<WasmFunc, Diag> {
    let mut cx = Ctx {
        sigs,
        scope: Vec::new(),
        nparams: params.len() as u32,
        extra: Vec::new(),
        code: Vec::new(),
        labels: Vec::new(),
        ret,
    };
    for (i, (p, n)) in f.params.iter().zip(params.iter()).enumerate() {
        cx.scope.push((p.name.clone(), *n, i as u32));
    }
    cx.body(&f.body, true)?;

    // Badan yang tidak berakhir dengan return bernilai harus tetap valid untuk validator WASM.
    let ends_with_value_return = matches!(
        f.body.last().map(|s| &s.kind),
        Some(StmtKind::Return(Some(_)))
    );
    if ret.is_some() && !ends_with_value_return {
        cx.code.push(0x00); // unreachable
    }

    Ok(WasmFunc {
        name: f.name.clone(),
        params: params.iter().map(|n| n.val).collect(),
        result: ret.map(|n| n.val),
        locals: cx.extra,
        code: cx.code,
    })
}

fn binop(op: &str, n: Num) -> Option<(u8, Num)> {
    use ValTy::*;
    let cmp = matches!(op, "==" | "!=" | "<" | ">" | "<=" | ">=");
    let arith = matches!(op, "+" | "-" | "*" | "/" | "%" | "&" | "|" | "^" | "<<" | ">>");
    if !cmp && !arith {
        return None;
    }
    let s = n.signed;
    let code: u8 = match (n.val, op) {
        (I32, "+") => 0x6a,
        (I32, "-") => 0x6b,
        (I32, "*") => 0x6c,
        (I32, "/") => if s { 0x6d } else { 0x6e },
        (I32, "%") => if s { 0x6f } else { 0x70 },
        (I32, "&") => 0x71,
        (I32, "|") => 0x72,
        (I32, "^") => 0x73,
        (I32, "<<") => 0x74,
        (I32, ">>") => if s { 0x75 } else { 0x76 },
        (I64, "+") => 0x7c,
        (I64, "-") => 0x7d,
        (I64, "*") => 0x7e,
        (I64, "/") => if s { 0x7f } else { 0x80 },
        (I64, "%") => if s { 0x81 } else { 0x82 },
        (I64, "&") => 0x83,
        (I64, "|") => 0x84,
        (I64, "^") => 0x85,
        (I64, "<<") => 0x86,
        (I64, ">>") => if s { 0x87 } else { 0x88 },
        (F32, "+") => 0x92,
        (F32, "-") => 0x93,
        (F32, "*") => 0x94,
        (F32, "/") => 0x95,
        (F64, "+") => 0xa0,
        (F64, "-") => 0xa1,
        (F64, "*") => 0xa2,
        (F64, "/") => 0xa3,
        (I32, "==") => 0x46,
        (I32, "!=") => 0x47,
        (I32, "<") => if s { 0x48 } else { 0x49 },
        (I32, ">") => if s { 0x4a } else { 0x4b },
        (I32, "<=") => if s { 0x4c } else { 0x4d },
        (I32, ">=") => if s { 0x4e } else { 0x4f },
        (I64, "==") => 0x51,
        (I64, "!=") => 0x52,
        (I64, "<") => if s { 0x53 } else { 0x54 },
        (I64, ">") => if s { 0x55 } else { 0x56 },
        (I64, "<=") => if s { 0x57 } else { 0x58 },
        (I64, ">=") => if s { 0x59 } else { 0x5a },
        (F32, "==") => 0x5b,
        (F32, "!=") => 0x5c,
        (F32, "<") => 0x5d,
        (F32, ">") => 0x5e,
        (F32, "<=") => 0x5f,
        (F32, ">=") => 0x60,
        (F64, "==") => 0x61,
        (F64, "!=") => 0x62,
        (F64, "<") => 0x63,
        (F64, ">") => 0x64,
        (F64, "<=") => 0x65,
        (F64, ">=") => 0x66,
        _ => return None,
    };
    let result = if cmp { BOOL } else { n };
    Some((code, result))
}

impl<'a> Ctx<'a> {
    fn op(&mut self, b: u8) {
        self.code.push(b);
    }

    fn uleb_op(&mut self, v: u64) {
        uleb(&mut self.code, v);
    }

    fn sleb_op(&mut self, v: i64) {
        crate::wasm_encode::sleb(&mut self.code, v);
    }

    fn err(&self, span: Span, msg: impl Into<String>) -> Diag {
        Diag::new(msg, span)
    }

    fn lookup(&self, name: &str) -> Option<(Num, u32)> {
        self.scope
            .iter()
            .rev()
            .find(|(n, _, _)| n == name)
            .map(|(_, t, i)| (*t, *i))
    }

    fn new_local(&mut self, name: String, n: Num) -> u32 {
        let idx = self.nparams + self.extra.len() as u32;
        self.extra.push(n.val);
        self.scope.push((name, n, idx));
        idx
    }

    /// Konstanta sesuai tipe `n`.
    fn konst(&mut self, n: Num, v: f64) {
        match n.val {
            ValTy::I32 => {
                self.op(0x41);
                self.sleb_op(v as i32 as i64);
            }
            ValTy::I64 => {
                self.op(0x42);
                self.sleb_op(v as i64);
            }
            ValTy::F32 => {
                self.op(0x43);
                self.code.extend_from_slice(&(v as f32).to_le_bytes());
            }
            ValTy::F64 => {
                self.op(0x44);
                self.code.extend_from_slice(&v.to_le_bytes());
            }
        }
    }

    /// Membungkus integer ke lebar sempit (8 atau 16 bit), sama seperti tipe sungguhan.
    fn wrap(&mut self, n: Num) {
        if n.val != ValTy::I32 || n.bits >= 32 || n.bits == 1 {
            return;
        }
        match (n.bits, n.signed) {
            (8, false) => {
                self.op(0x41);
                self.sleb_op(0xff);
                self.op(0x71);
            }
            (16, false) => {
                self.op(0x41);
                self.sleb_op(0xffff);
                self.op(0x71);
            }
            (8, true) => {
                self.op(0x41);
                self.sleb_op(24);
                self.op(0x74);
                self.op(0x41);
                self.sleb_op(24);
                self.op(0x75);
            }
            (16, true) => {
                self.op(0x41);
                self.sleb_op(16);
                self.op(0x74);
                self.op(0x41);
                self.sleb_op(16);
                self.op(0x75);
            }
            _ => {}
        }
    }

    /// Konversi eksplisit antar tipe skalar (dipakai oleh Cast).
    fn convert(&mut self, from: Num, to: Num, span: Span) -> Result<(), Diag> {
        if from == to {
            return Ok(());
        }
        match (from.val, to.val) {
            (ValTy::I32, ValTy::I64) => self.op(if from.signed { 0xac } else { 0xad }),
            (ValTy::I64, ValTy::I32) => self.op(0xa7),
            (ValTy::I32, ValTy::F64) => self.op(if from.signed { 0xb7 } else { 0xb8 }),
            (ValTy::I32, ValTy::F32) => self.op(if from.signed { 0xb2 } else { 0xb3 }),
            (ValTy::F64, ValTy::I32) => self.op(if to.signed { 0xaa } else { 0xab }),
            (ValTy::F32, ValTy::I32) => self.op(if to.signed { 0xa8 } else { 0xa9 }),
            (ValTy::F32, ValTy::F64) => self.op(0xbb),
            (ValTy::F64, ValTy::F32) => self.op(0xb6),
            (a, b) if a == b => {}
            _ => return Err(self.err(span, "konversi antar tipe ini belum didukung di backend WASM")),
        }
        self.wrap(to);
        Ok(())
    }

    /// Tipe harus persis sama. Tidak ada konversi diam-diam (sesuai aturan boxNM).
    fn expect(&self, got: Num, want: Num, span: Span) -> Result<(), Diag> {
        if got == want {
            Ok(())
        } else {
            Err(self.err(
                span,
                format!("tipe tidak cocok: diharapkan {:?}, ditemukan {:?}. Gunakan Cast(...)", want, got),
            ))
        }
    }

    fn value(&mut self, e: &Expr, want: Option<Num>, what: &str) -> Result<Num, Diag> {
        match self.expr(e, want)? {
            Some(n) => Ok(n),
            None => Err(self.err(e.span, format!("{} tidak menghasilkan nilai", what))),
        }
    }

    fn cond(&mut self, e: &Expr) -> Result<(), Diag> {
        let n = self.value(e, Some(BOOL), "kondisi")?;
        if n.val != ValTy::I32 {
            return Err(self.err(e.span, "kondisi harus bertipe bool atau integer 32-bit"));
        }
        Ok(())
    }

    // ---------- Ekspresi ----------

    fn expr(&mut self, e: &Expr, want: Option<Num>) -> Result<Option<Num>, Diag> {
        match &e.kind {
            ExprKind::Int(v) => {
                let n = match want {
                    Some(w) if matches!(w.val, ValTy::F32 | ValTy::F64 | ValTy::I32 | ValTy::I64) => w,
                    _ => NUM_I32,
                };
                self.konst(n, *v as f64);
                self.wrap(n);
                Ok(Some(n))
            }
            ExprKind::Float(v) => {
                let n = match want {
                    Some(w) if matches!(w.val, ValTy::F32 | ValTy::F64) => w,
                    _ => Num { val: ValTy::F64, signed: true, bits: 64 },
                };
                self.konst(n, *v);
                Ok(Some(n))
            }
            ExprKind::Bool(b) => {
                self.op(0x41);
                self.sleb_op(if *b { 1 } else { 0 });
                Ok(Some(BOOL))
            }
            ExprKind::Ident(name) => match self.lookup(name) {
                Some((n, idx)) => {
                    self.op(0x20);
                    self.uleb_op(idx as u64);
                    Ok(Some(n))
                }
                None => Err(self.err(
                    e.span,
                    format!("'{}' belum didukung di backend WASM (hanya variabel lokal dan parameter)", name),
                )),
            },
            ExprKind::Binary { op, l, r } => self.binary(op, l, r, want, e.span),
            ExprKind::Unary { op, arg } => self.unary(op, arg, want, e.span),
            ExprKind::Call { callee, args } => self.call(callee, args, e.span),
            _ => Err(self.err(e.span, "ekspresi ini belum didukung di backend WASM")),
        }
    }

    fn binary(
        &mut self,
        op: &str,
        l: &Expr,
        r: &Expr,
        want: Option<Num>,
        span: Span,
    ) -> Result<Option<Num>, Diag> {
        if op == "&&" || op == "||" {
            self.cond(l)?;
            self.op(0x04); // if (result i32)
            self.op(0x7f);
            if op == "&&" {
                self.cond(r)?;
                self.op(0x05);
                self.op(0x41);
                self.sleb_op(0);
            } else {
                self.op(0x41);
                self.sleb_op(1);
                self.op(0x05);
                self.cond(r)?;
            }
            self.op(0x0b);
            return Ok(Some(BOOL));
        }

        let lt = self.value(l, want, "operand kiri")?;
        let rt = self.value(r, Some(lt), "operand kanan")?;
        self.expect(rt, lt, r.span)?;
        let (code, result) = binop(op, lt)
            .ok_or_else(|| self.err(span, format!("operator '{}' tidak didukung untuk tipe {:?}", op, lt)))?;
        self.op(code);
        if result == lt {
            self.wrap(lt);
        }
        Ok(Some(result))
    }

    fn unary(&mut self, op: &str, arg: &Expr, want: Option<Num>, span: Span) -> Result<Option<Num>, Diag> {
        match op {
            "+" => self.value(arg, want, "operand").map(Some),
            "-" => {
                let n = self.value(arg, want, "operand")?;
                match n.val {
                    ValTy::I32 => {
                        self.konst(n, -1.0);
                        self.op(0x6c);
                    }
                    ValTy::I64 => {
                        self.konst(n, -1.0);
                        self.op(0x7e);
                    }
                    ValTy::F32 => self.op(0x8c),
                    ValTy::F64 => self.op(0x9a),
                }
                self.wrap(n);
                Ok(Some(n))
            }
            "!" => {
                let n = self.value(arg, Some(BOOL), "operand")?;
                if n.val != ValTy::I32 {
                    return Err(self.err(span, "'!' hanya untuk bool atau integer 32-bit"));
                }
                self.op(0x45); // i32.eqz
                Ok(Some(BOOL))
            }
            "~" => {
                let n = self.value(arg, want, "operand")?;
                match n.val {
                    ValTy::I32 => {
                        self.konst(n, -1.0);
                        self.op(0x73);
                    }
                    ValTy::I64 => {
                        self.konst(n, -1.0);
                        self.op(0x85);
                    }
                    _ => return Err(self.err(span, "'~' hanya untuk integer")),
                }
                self.wrap(n);
                Ok(Some(n))
            }
            _ => Err(self.err(span, format!("operator unary '{}' belum didukung di backend WASM", op))),
        }
    }

    fn call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Result<Option<Num>, Diag> {
        let name = match &callee.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return Err(self.err(span, "pemanggilan ini belum didukung di backend WASM")),
        };
        if name == "Cast" {
            return self.cast(args, span);
        }
        let sig = match self.sigs.get(&name).cloned() {
            Some(s) => s,
            None => {
                return Err(self.err(
                    span,
                    format!("'{}' bukan fungsi bertipe skalar; belum bisa dipanggil dari WASM", name),
                ))
            }
        };
        if args.len() != sig.params.len() {
            return Err(self.err(
                span,
                format!("fungsi '{}' membutuhkan {} argumen, diberikan {}", name, sig.params.len(), args.len()),
            ));
        }
        for (a, p) in args.iter().zip(sig.params.iter()) {
            let got = self.value(a, Some(*p), "argumen")?;
            self.expect(got, *p, a.span)?;
        }
        self.op(0x10); // call
        self.uleb_op(sig.index as u64);
        Ok(sig.ret)
    }

    fn cast(&mut self, args: &[Expr], span: Span) -> Result<Option<Num>, Diag> {
        let (value, tname) = match args {
            [v, Expr { kind: ExprKind::Ident(t), .. }] => (v, t.clone()),
            _ => return Err(self.err(span, "pemakaian: Cast(nilai, TipeTujuan)")),
        };
        let to = num_of(&tname)
            .ok_or_else(|| self.err(span, format!("Cast ke tipe '{}' belum didukung di backend WASM", tname)))?;
        let got = self.value(value, None, "nilai Cast")?;
        self.convert(got, to, span)?;
        Ok(Some(to))
    }

    // ---------- Statement ----------

    fn body(&mut self, list: &[Stmt], tail: bool) -> Result<(), Diag> {
        let n = list.len();
        for (i, s) in list.iter().enumerate() {
            if tail && i + 1 == n {
                if let StmtKind::Return(arg) = &s.kind {
                    return self.ret_value(arg.as_ref(), s.span, true);
                }
            }
            self.stmt(s)?;
        }
        Ok(())
    }

    fn stmt(&mut self, s: &Stmt) -> Result<(), Diag> {
        match &s.kind {
            StmtKind::Var(v) => self.var_decl(v),
            StmtKind::Expr(e) => self.expr_stmt(e),
            StmtKind::Block(list) => {
                let mark = self.scope.len();
                self.body(list, false)?;
                self.scope.truncate(mark);
                Ok(())
            }
            StmtKind::If { test, cons, alt } => self.if_stmt(test, cons, alt.as_deref()),
            StmtKind::While { test, body } => self.while_stmt(test, body),
            StmtKind::For { init, test, update, body } => {
                self.for_stmt(init.as_ref(), test.as_ref(), update.as_ref(), body)
            }
            StmtKind::Return(arg) => self.ret_value(arg.as_ref(), s.span, false),
            StmtKind::Break => self.jump(s.span, true),
            StmtKind::Continue => self.jump(s.span, false),
            // `asm`: byte mentah langsung ke body fungsi. Tanggung jawab pemanggil menjaga tumpukan.
            StmtKind::Asm(bytes) => {
                self.code.extend_from_slice(bytes);
                Ok(())
            }
            _ => Err(self.err(s.span, "statement ini belum didukung di backend WASM")),
        }
    }

    fn var_decl(&mut self, v: &VarDecl) -> Result<(), Diag> {
        if v.kind != VarKind::Typed {
            return Err(self.err(
                v.span,
                "variabel 'var', 'let', dan 'state' belum didukung di backend WASM; pakai tipe box",
            ));
        }
        if v.dims.is_some() {
            return Err(self.err(v.span, "array belum didukung di backend WASM"));
        }
        let tname = v
            .ty
            .as_ref()
            .and_then(|t| type_name(t))
            .ok_or_else(|| self.err(v.span, "tipe harus berupa tipe skalar"))?;
        let n = num_of(tname)
            .ok_or_else(|| self.err(v.span, format!("tipe '{}' belum didukung di backend WASM", tname)))?;
        match &v.init {
            Some(init) => {
                let got = self.value(init, Some(n), "inisialisasi")?;
                self.expect(got, n, init.span)?;
            }
            None => self.konst(n, 0.0),
        }
        let idx = self.new_local(v.name.clone(), n);
        self.op(0x21); // local.set
        self.uleb_op(idx as u64);
        Ok(())
    }

    fn expr_stmt(&mut self, e: &Expr) -> Result<(), Diag> {
        match &e.kind {
            ExprKind::Assign { op, target, value } => self.assign(op, target, value, e.span),
            ExprKind::Update { op, arg, .. } => self.update(op, arg, e.span),
            _ => {
                if self.expr(e, None)?.is_some() {
                    self.op(0x1a); // drop
                }
                Ok(())
            }
        }
    }

    fn local_target(&self, target: &Expr) -> Result<(Num, u32), Diag> {
        match &target.kind {
            ExprKind::Ident(name) => self
                .lookup(name)
                .ok_or_else(|| self.err(target.span, format!("'{}' bukan variabel lokal", name))),
            _ => Err(self.err(target.span, "target penugasan harus variabel lokal di backend WASM")),
        }
    }

    fn assign(&mut self, op: &str, target: &Expr, value: &Expr, span: Span) -> Result<(), Diag> {
        let (n, idx) = self.local_target(target)?;
        if op == "=" {
            let got = self.value(value, Some(n), "nilai penugasan")?;
            self.expect(got, n, value.span)?;
        } else {
            let bop = &op[..op.len() - 1];
            self.op(0x20);
            self.uleb_op(idx as u64);
            let got = self.value(value, Some(n), "nilai penugasan")?;
            self.expect(got, n, value.span)?;
            let (code, _) = binop(bop, n)
                .ok_or_else(|| self.err(span, format!("operator '{}' tidak didukung untuk tipe {:?}", op, n)))?;
            self.op(code);
            self.wrap(n);
        }
        self.op(0x21);
        self.uleb_op(idx as u64);
        Ok(())
    }

    fn update(&mut self, op: &str, arg: &Expr, span: Span) -> Result<(), Diag> {
        let (n, idx) = self.local_target(arg)?;
        self.op(0x20);
        self.uleb_op(idx as u64);
        self.konst(n, 1.0);
        let code = match (op, n.val) {
            ("++", ValTy::I32) => 0x6a,
            ("--", ValTy::I32) => 0x6b,
            ("++", ValTy::I64) => 0x7c,
            ("--", ValTy::I64) => 0x7d,
            ("++", ValTy::F32) => 0x92,
            ("--", ValTy::F32) => 0x93,
            ("++", ValTy::F64) => 0xa0,
            ("--", ValTy::F64) => 0xa1,
            _ => return Err(self.err(span, format!("operator '{}' tidak didukung", op))),
        };
        self.op(code);
        self.wrap(n);
        self.op(0x21);
        self.uleb_op(idx as u64);
        Ok(())
    }

    fn if_stmt(&mut self, test: &Expr, cons: &Stmt, alt: Option<&Stmt>) -> Result<(), Diag> {
        self.cond(test)?;
        self.op(0x04); // if
        self.op(0x40); // tanpa nilai
        self.labels.push(Label { brk: false, cont: false });
        self.stmt(cons)?;
        if let Some(a) = alt {
            self.op(0x05); // else
            self.stmt(a)?;
        }
        self.op(0x0b);
        self.labels.pop();
        Ok(())
    }

    /// Struktur while:
    /// block(brk) loop { test; i32.eqz; br_if 1; block(cont) { body } ; br 0 }
    fn while_stmt(&mut self, test: &Expr, body: &Stmt) -> Result<(), Diag> {
        self.op(0x02);
        self.op(0x40);
        self.labels.push(Label { brk: true, cont: false });
        self.op(0x03);
        self.op(0x40);
        self.labels.push(Label { brk: false, cont: false });
        self.cond(test)?;
        self.op(0x45);
        self.op(0x0d);
        self.uleb_op(1);
        self.op(0x02);
        self.op(0x40);
        self.labels.push(Label { brk: false, cont: true });
        self.stmt(body)?;
        self.labels.pop();
        self.op(0x0b);
        self.op(0x0c);
        self.uleb_op(0);
        self.op(0x0b);
        self.labels.pop();
        self.op(0x0b);
        self.labels.pop();
        Ok(())
    }

    /// Struktur for sama seperti while, tetapi update dijalankan setelah blok continue.
    fn for_stmt(
        &mut self,
        init: Option<&ForInit>,
        test: Option<&Expr>,
        update: Option<&Expr>,
        body: &Stmt,
    ) -> Result<(), Diag> {
        let mark = self.scope.len();
        match init {
            Some(ForInit::Decls(decls)) => {
                for d in decls {
                    self.var_decl(d)?;
                }
            }
            Some(ForInit::Expr(e)) => self.expr_stmt(e)?,
            None => {}
        }
        self.op(0x02);
        self.op(0x40);
        self.labels.push(Label { brk: true, cont: false });
        self.op(0x03);
        self.op(0x40);
        self.labels.push(Label { brk: false, cont: false });
        if let Some(t) = test {
            self.cond(t)?;
            self.op(0x45);
            self.op(0x0d);
            self.uleb_op(1);
        }
        self.op(0x02);
        self.op(0x40);
        self.labels.push(Label { brk: false, cont: true });
        self.stmt(body)?;
        self.labels.pop();
        self.op(0x0b);
        if let Some(u) = update {
            self.expr_stmt(u)?;
        }
        self.op(0x0c);
        self.uleb_op(0);
        self.op(0x0b);
        self.labels.pop();
        self.op(0x0b);
        self.labels.pop();
        self.scope.truncate(mark);
        Ok(())
    }

    fn jump(&mut self, span: Span, brk: bool) -> Result<(), Diag> {
        let target = self
            .labels
            .iter()
            .rposition(|l| if brk { l.brk } else { l.cont });
        match target {
            Some(idx) => {
                let depth = self.labels.len() - 1 - idx;
                self.op(0x0c);
                self.uleb_op(depth as u64);
                Ok(())
            }
            None => Err(self.err(
                span,
                if brk { "'break' hanya boleh di dalam loop" } else { "'continue' hanya boleh di dalam loop" },
            )),
        }
    }

    fn ret_value(&mut self, arg: Option<&Expr>, span: Span, tail: bool) -> Result<(), Diag> {
        match (arg, self.ret) {
            (Some(e), Some(r)) => {
                let got = self.value(e, Some(r), "nilai return")?;
                self.expect(got, r, e.span)?;
            }
            (None, None) => {}
            (Some(_), None) => return Err(self.err(span, "fungsi bertipe U0 tidak boleh mengembalikan nilai")),
            (None, Some(_)) => return Err(self.err(span, "fungsi harus mengembalikan nilai")),
        }
        if !tail {
            self.op(0x0f); // return
        }
        Ok(())
    }
}
