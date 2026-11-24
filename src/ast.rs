//! Abstract syntax tree FunnyBoxes.

use crate::error::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub body: Vec<Stmt>,
}

// ---------- Statement ----------

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Block(Vec<Stmt>),
    Expr(Expr),
    Var(VarDecl),
    Func(FuncDecl),
    Struct(StructDecl),
    Alias(AliasDecl),
    Play(PlayDecl),
    If {
        test: Expr,
        cons: Box<Stmt>,
        alt: Option<Box<Stmt>>,
    },
    While {
        test: Expr,
        body: Box<Stmt>,
    },
    DoWhile {
        body: Box<Stmt>,
        test: Expr,
    },
    For {
        init: Option<ForInit>,
        test: Option<Expr>,
        update: Option<Expr>,
        body: Box<Stmt>,
    },
    Switch {
        disc: Expr,
        cases: Vec<SwitchCase>,
    },
    Return(Option<Expr>),
    Break,
    Continue,
    /// Blok `debug { }`; hanya ikut dikompilasi saat mode debug.
    Debug(Vec<Stmt>),
    /// `version (...) { } else ...` dan `static if (...) { } else { }`.
    StaticIf {
        cond: Expr,
        then: Vec<Stmt>,
        alt: Vec<Stmt>,
    },
    /// `try { } catch { }` (HolyC). Di JS menjadi try/catch.
    Try {
        body: Vec<Stmt>,
        catch_body: Option<Vec<Stmt>>,
    },
    /// `throw expr;` (HolyC).
    Throw(Expr),
    /// `asm { 0x41, 0x2a };` (HolyC). Byte mentah WASM, hanya di fungsi WASM.
    Asm(Vec<u8>),
    /// `extern` (HolyC): deklarasi fungsi yang disediakan lingkungan (JS global).
    Extern(ExternDecl),
    /// `nama: { ... }`: blok berlabel. Target `goto nama` dari dalamnya.
    Labeled {
        name: String,
        body: Vec<Stmt>,
    },
    /// `goto nama;`: hanya ke label yang melingkupi (sama seperti break label).
    Goto(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternDecl {
    pub name: String,
    pub name_span: Span,
    pub ret: Option<TypeExpr>,
    pub params: Vec<Param>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ForInit {
    Decls(Vec<VarDecl>),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SwitchCase {
    /// `None` untuk `default:`.
    pub test: Option<Expr>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarKind {
    /// Deklarasi bertipe: `box14 x = 1;`
    Typed,
    /// Deklarasi dinamis berlingkup fungsi, di-hoist seperti JS: `var x = 1;`
    Dyn,
    /// Deklarasi dinamis berlingkup blok, seperti JS: `let x = 1;`
    Let,
    /// Variabel reaktif di dalam komponen: `state x = 0;`
    State,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VarDecl {
    pub name: String,
    pub kind: VarKind,
    pub ty: Option<TypeExpr>,
    /// Panjang array, misalnya `Product products[3]`.
    pub dims: Option<Expr>,
    pub init: Option<Expr>,
    pub span: Span,
}

// ---------- Tipe ----------

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    /// Nama tipe: `box14`, `char`, `Product`, atau alias.
    Name { name: String, span: Span },
    /// Pointer: `T *`.
    Ptr(Box<TypeExpr>),
}

// ---------- Fungsi, struct, modul ----------

#[derive(Debug, Clone, PartialEq)]
pub struct FuncDecl {
    pub name: String,
    pub name_span: Span,
    /// `Some` untuk fungsi bertipe; `None` untuk `fun` dinamis.
    pub ret: Option<TypeExpr>,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub dynamic: bool,
    pub is_async: bool,
    pub exported: bool,
    pub is_default: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<FieldDecl>,
    /// `union` (HolyC): semua field berbagi offset 0.
    pub is_union: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDecl {
    pub name: String,
    pub ty: TypeExpr,
    pub dims: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AliasDecl {
    pub name: String,
    pub target: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayDecl {
    /// Modul bawaan FunnyBoxes, termasuk bagian setelah titik (`Graphics.WASM`).
    /// Kosong untuk impor dari URL.
    pub module: String,
    /// Bentuk lama: `play Math as M;`.
    pub alias: Option<String>,
    /// Bentuk lama: `play Math : sqrt, pow;`.
    pub names: Option<Vec<String>>,
    /// Impor dari URL: `play ... from "url";` (sama seperti `import ... from "url"` di JS).
    pub url: Option<String>,
    /// `play Nama from "url";` (import default).
    pub default_name: Option<String>,
    /// `play * as Nama from "url";` (import namespace).
    pub namespace: Option<String>,
    /// `play { a, b as c } from ...;` (impor bernama). Pasangan (nama asli, nama lokal).
    pub named: Vec<(String, String)>,
}

// ---------- Ekspresi ----------

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
    Ident(String),
    Binary {
        op: &'static str,
        l: Box<Expr>,
        r: Box<Expr>,
    },
    Unary {
        op: &'static str,
        arg: Box<Expr>,
    },
    Update {
        op: &'static str,
        arg: Box<Expr>,
        prefix: bool,
    },
    Assign {
        op: &'static str,
        target: Box<Expr>,
        value: Box<Expr>,
    },
    Cond {
        test: Box<Expr>,
        cons: Box<Expr>,
        alt: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    /// `a.b` atau `a->b`.
    Member {
        obj: Box<Expr>,
        prop: String,
        arrow: bool,
    },
    Index {
        obj: Box<Expr>,
        index: Box<Expr>,
    },
    Arrow {
        params: Vec<String>,
        body: ArrowBody,
        is_async: bool,
    },
    Await(Box<Expr>),
    /// Literal `{a, b}` untuk inisialisasi struct atau array.
    Brace(Vec<Expr>),
    /// Literal objek `{ key: value }`.
    ObjectLit(Vec<(String, Expr)>),
    /// `offset(Tipe, field)` (HolyC): offset byte field dalam struct, dihitung compile-time.
    Offset {
        ty: String,
        field: String,
    },
    Jsx(JsxElement),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArrowBody {
    Expr(Box<Expr>),
    Block(Vec<Stmt>),
}

// ---------- JSX ----------

#[derive(Debug, Clone, PartialEq)]
pub struct JsxElement {
    /// `None` untuk fragment `<>...</>`.
    pub tag: Option<String>,
    pub attrs: Vec<JsxAttr>,
    pub children: Vec<JsxChild>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JsxAttr {
    /// `name="teks"` atau `name={ekspresi}`, atau hanya `name` (nilai `true`).
    Named {
        name: String,
        value: Option<Expr>,
    },
    /// Spread: `{...ekspresi}`.
    Spread(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum JsxChild {
    Text(String),
    Expr(Expr),
    Element(JsxElement),
}
