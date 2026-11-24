//! Backend FunnyBoxes: AST menjadi JavaScript (app.js).
//!
//! Batasan tahap ini, dicatat supaya tidak mengejutkan:
//! - Pemeriksaan tipe penuh belum ada. Tipe `boxNM` hanya dipakai untuk pembagian
//!   integer dan konversi. Overflow integer belum dibungkus.
//! - UI dihasilkan sebagai pemanggilan DOM. Setiap perubahan state atau event handler
//!   membangun ulang UI dari fungsi entry. Sederhana dan benar; optimasi granular menyusul.
//! - State bersifat singleton per fungsi komponen, belum per instance.
//! - Builtin yang belum didukung (Memory, Storage, Network, Router, Animation, Browser,
//!   dan Graphics.WASM) menghasilkan error. Tidak diam-diam diabaikan.
//! - WebAssembly belum dihasilkan. Itu tahap berikutnya.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::error::{Diag, Span};

const PRELUDE: &str = include_str!("runtime/prelude.js");

pub struct CodegenOptions {
    pub defines: Vec<String>,
    pub debug: bool,
    /// Fungsi yang dikompilasi ke WASM; di JS hanya berupa pembungkus ke `__app`.
    pub wasm_funcs: Vec<String>,
}

#[derive(Clone)]
struct FnInfo {
    params: Vec<String>,
    int_ret: bool,
}

#[derive(Clone)]
struct Sym {
    /// Tipe integer (`box04`, `box08`, `box14`, `box18`).
    int: bool,
    ty: Option<TypeExpr>,
    is_array: bool,
    func: Option<FnInfo>,
    /// Nama objek JS jika simbol ini adalah state (`__st_Komponen`).
    state_obj: Option<String>,
    /// Dideklarasikan lewat hoisting `var`: deklarasi berikutnya cukup assignment.
    hoisted: bool,
}

impl Sym {
    fn var(int: bool, ty: Option<TypeExpr>, is_array: bool) -> Sym {
        Sym { int, ty, is_array, func: None, state_obj: None, hoisted: false }
    }
}

/// Alias keyword HolyC yang belum punya padanan di spesifikasi.
/// U0, U32, U64, I32, I64, F32, F64 tidak perlu alias: spesifikasi sudah punya
/// box00, box04, box08, box14, box18, box24, dan box28. Bool tidak perlu: hasil
/// perbandingan disimpan di tipe box (lihat spesifikasi bagian 3H).
/// Untuk U8, U16, I8, I16 belum ada padanan di spesifikasi; pemetaan ke box01,
/// box02, box11, box12 menunggu persetujuan.
pub(crate) fn keyword_alias(name: &str) -> Option<&'static str> {
    Some(match name {
        "U8" => "box01",
        "U16" => "box02",
        "I8" => "box11",
        "I16" => "box12",
        _ => return None,
    })
}

/// Mengumpulkan nama `var` yang berlingkup fungsi (termasuk yang di dalam blok).
/// Tidak masuk ke fungsi atau arrow bersarang: mereka punya lingkupnya sendiri.
fn collect_vars(list: &[Stmt], out: &mut Vec<String>) {
    for s in list {
        match &s.kind {
            StmtKind::Var(v) if v.kind == VarKind::Dyn => out.push(v.name.clone()),
            StmtKind::Block(b) => collect_vars(b, out),
            StmtKind::If { cons, alt, .. } => {
                collect_vars(std::slice::from_ref(cons.as_ref()), out);
                if let Some(a) = alt {
                    collect_vars(std::slice::from_ref(a.as_ref()), out);
                }
            }
            StmtKind::While { body, .. } | StmtKind::DoWhile { body, .. } => {
                collect_vars(std::slice::from_ref(body.as_ref()), out)
            }
            StmtKind::For { init, body, .. } => {
                if let Some(ForInit::Decls(decls)) = init {
                    for d in decls {
                        if d.kind == VarKind::Dyn {
                            out.push(d.name.clone());
                        }
                    }
                }
                collect_vars(std::slice::from_ref(body.as_ref()), out);
            }
            StmtKind::Switch { cases, .. } => {
                for c in cases {
                    collect_vars(&c.body, out);
                }
            }
            StmtKind::Try { body, catch_body } => {
                collect_vars(body, out);
                if let Some(cb) = catch_body {
                    collect_vars(cb, out);
                }
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy)]
enum Builtin {
    /// Ekspresi JS yang langsung dipakai.
    Js(&'static str),
    /// Komponen UI, dipakai lewat JSX sebagai `__C.Nama`.
    Ui(&'static str),
    Cast,
    SizeOf,
}

const BOX_TYPES: &[&str] = &[
    "box00", "box01", "box02", "box04", "box08", "box11", "box12", "box14", "box18", "box24", "box28",
];
const BOX_INTS: &[&str] = &["box01", "box02", "box04", "box08", "box11", "box12", "box14", "box18"];
const BOX_SIZES: &[(&str, u64)] = &[
    ("box00", 0),
    ("bool", 1),
    ("box01", 1),
    ("box02", 2),
    ("box11", 1),
    ("box12", 2),
    ("box04", 4),
    ("box08", 8),
    ("box14", 4),
    ("box18", 8),
    ("box24", 4),
    ("box28", 8),
];

const CORE: &[(&str, Builtin)] = &[
    ("Print", Builtin::Js("console.log")),
    ("Assert", Builtin::Js("__assert")),
    ("Panic", Builtin::Js("__panic")),
    ("TypeOf", Builtin::Js("__typeof")),
    ("Cast", Builtin::Cast),
    ("SizeOf", Builtin::SizeOf),
];

const MATH: &[(&str, Builtin)] = &[
    ("sqrt", Builtin::Js("__core.fb_sqrt")),
    ("pow", Builtin::Js("__core.fb_pow")),
    ("sin", Builtin::Js("__core.fb_sin")),
    ("cos", Builtin::Js("__core.fb_cos")),
    ("clamp", Builtin::Js("__core.fb_clamp")),
    ("random", Builtin::Js("__core.fb_random")),
    ("randomInt", Builtin::Js("__core.fb_random_int")),
    ("lerp", Builtin::Js("__core.fb_lerp")),
    ("abs", Builtin::Js("Math.abs")),
    ("min", Builtin::Js("Math.min")),
    ("max", Builtin::Js("Math.max")),
    ("floor", Builtin::Js("Math.floor")),
    ("ceil", Builtin::Js("Math.ceil")),
    ("round", Builtin::Js("Math.round")),
];

const MEMORY: &[(&str, Builtin)] = &[
    ("Alloc", Builtin::Js("__core.fb_alloc")),
    ("Realloc", Builtin::Js("__core.fb_realloc")),
    ("Free", Builtin::Js("__core.fb_free")),
    ("MemoryCopy", Builtin::Js("__core.fb_memcopy")),
    ("MemorySet", Builtin::Js("__core.fb_memset")),
    ("MemoryCompare", Builtin::Js("__core.fb_memcmp")),
    // Akses memori bernama sesuai tipe box: PeekBoxNM membaca, PokeBoxNM menulis.
    ("PeekBox01", Builtin::Js("__core.fb_peek_u8")),
    ("PokeBox01", Builtin::Js("__core.fb_poke_u8")),
    ("PeekBox11", Builtin::Js("__core.fb_peek_i8")),
    ("PokeBox11", Builtin::Js("__core.fb_poke_i8")),
    ("PeekBox02", Builtin::Js("__core.fb_peek_u16")),
    ("PokeBox02", Builtin::Js("__core.fb_poke_u16")),
    ("PeekBox12", Builtin::Js("__core.fb_peek_i16")),
    ("PokeBox12", Builtin::Js("__core.fb_poke_i16")),
    ("PeekBox04", Builtin::Js("__peekBox04")),
    ("PokeBox04", Builtin::Js("__core.fb_poke_u32")),
    ("PeekBox14", Builtin::Js("__core.fb_peek_i32")),
    ("PokeBox14", Builtin::Js("__core.fb_poke_i32")),
    ("PeekBox24", Builtin::Js("__core.fb_peek_f32")),
    ("PokeBox24", Builtin::Js("__core.fb_poke_f32")),
    ("PeekBox28", Builtin::Js("__core.fb_peek_f64")),
    ("PokeBox28", Builtin::Js("__core.fb_poke_f64")),
];

const UI: &[(&str, Builtin)] = &[
    ("Window", Builtin::Ui("Window")),
    ("Container", Builtin::Ui("Container")),
    ("Button", Builtin::Ui("Button")),
    ("Input", Builtin::Ui("Input")),
    ("Text", Builtin::Ui("Text")),
    ("Image", Builtin::Ui("Image")),
    ("List", Builtin::Ui("List")),
    ("Grid", Builtin::Ui("Grid")),
    ("Scroll", Builtin::Ui("Scroll")),
    ("Dialog", Builtin::Ui("Dialog")),
    ("Menu", Builtin::Ui("Menu")),
    ("Head", Builtin::Ui("Head")),
];

const GRAPHICS: &[(&str, Builtin)] = &[
    ("GFX_TRIANGLES", Builtin::Js("1")),
    ("gfxClear", Builtin::Js("__gfxClear")),
    ("gfxPixel", Builtin::Js("__gfxPixel")),
    ("gfxLine", Builtin::Js("__gfxLine")),
    ("gfxRect", Builtin::Js("__gfxRect")),
    ("gfxCircle", Builtin::Js("__gfxCircle")),
    ("gfxBegin", Builtin::Js("__gfxBegin")),
    ("gfxColor", Builtin::Js("__gfxColor")),
    ("gfxVertex", Builtin::Js("__gfxVertex")),
    ("gfxEnd", Builtin::Js("__gfxEnd")),
    ("gfxPresent", Builtin::Js("__gfxPresent")),
    ("GraphicsCanvas", Builtin::Ui("GraphicsCanvas")),
];

const INPUT: &[(&str, Builtin)] = &[
    ("OnKeyDown", Builtin::Js("__onKeyDown")),
    ("OnMouseMove", Builtin::Js("__onMouseMove")),
    ("OnPointerDown", Builtin::Js("__onPointerDown")),
    ("KEY_ENTER", Builtin::Js("\"Enter\"")),
    ("KEY_ESCAPE", Builtin::Js("\"Escape\"")),
    ("KEY_SPACE", Builtin::Js("\" \"")),
    ("KEY_TAB", Builtin::Js("\"Tab\"")),
    ("KEY_ARROW_UP", Builtin::Js("\"ArrowUp\"")),
    ("KEY_ARROW_DOWN", Builtin::Js("\"ArrowDown\"")),
    ("KEY_ARROW_LEFT", Builtin::Js("\"ArrowLeft\"")),
    ("KEY_ARROW_RIGHT", Builtin::Js("\"ArrowRight\"")),
];

fn module_table(module: &str) -> Result<&'static [(&'static str, Builtin)], String> {
    match module {
        "Core" => Ok(CORE),
        "Math" => Ok(MATH),
        "Memory" => Ok(MEMORY),
        "UI" => Ok(UI),
        "Graphics" | "Graphics.Browser" => Ok(GRAPHICS),
        "Input" => Ok(INPUT),
        "Storage" => Err("modul 'Storage' tidak ada; pakai API JavaScript langsung: localStorage, sessionStorage".into()),
        "Network" => Err("modul 'Network' tidak ada; pakai API JavaScript langsung: fetch".into()),
        "Router" => Err("modul 'Router' tidak ada; pakai API JavaScript langsung: location, history".into()),
        "Animation" => Err("modul 'Animation' tidak ada; pakai API JavaScript langsung: element.animate()".into()),
        other => Err(format!("modul '{}' belum didukung oleh backend tahap ini", other)),
    }
}

/// Identifier global JavaScript yang diteruskan apa adanya (tanpa prefiks `$`).
const JS_GLOBALS: &[&str] = &[
    "window", "document", "navigator", "location", "history", "localStorage", "sessionStorage",
    "fetch", "console", "JSON", "setTimeout", "clearTimeout", "setInterval", "clearInterval",
    "requestAnimationFrame", "cancelAnimationFrame", "queueMicrotask", "Promise", "Date",
    "Number", "String", "Array", "Object", "Map", "Set", "URL", "Intl", "structuredClone",
    "alert", "confirm", "prompt", "getComputedStyle", "matchMedia",
];

/// Mengubah teks menjadi literal string JavaScript.
fn js_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn indent(d: usize) -> String {
    "  ".repeat(d)
}

fn is_lvalue(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Ident(_) | ExprKind::Member { .. } | ExprKind::Index { .. } => true,
        ExprKind::Unary { op, .. } => *op == "*",
        _ => false,
    }
}

fn named_attr<'a>(attrs: &'a [JsxAttr], name: &str) -> Option<Option<&'a Expr>> {
    attrs.iter().find_map(|a| match a {
        JsxAttr::Named { name: n, value } if n.as_str() == name => Some(value.as_ref()),
        _ => None,
    })
}

struct Gen {
    defines: HashSet<String>,
    debug: bool,
    structs: HashMap<String, Vec<FieldDecl>>,
    /// Nama struct yang didefinisikan sebagai `union`.
    unions: HashSet<String>,
    /// Label blok yang melingkupi posisi saat ini (untuk validasi goto).
    labels: Vec<String>,
    aliases: HashMap<String, TypeExpr>,
    /// scopes[0] berisi deklarasi top-level. Setiap fungsi/blok menambah scope.
    scopes: Vec<HashMap<String, Sym>>,
    builtins: HashMap<String, Builtin>,
    errors: Vec<Diag>,
    /// Program memakai library WASM (core.wasm).
    uses_core: bool,
    wasm_funcs: HashSet<String>,
    /// Baris `import ... from "url";` (sintaks `play` dari URL).
    imports: Vec<String>,
}

/// Hasil codegen: isi app.js dan apakah program membutuhkan core.wasm.
pub struct Generated {
    pub js: String,
    pub uses_core: bool,
}

/// Menghasilkan isi `app.js` (runtime + kode pengguna) dari program yang sudah di-parse.
pub fn generate(program: &Program, opts: &CodegenOptions) -> Result<String, Vec<Diag>> {
    generate_full(program, opts).map(|g| g.js)
}

pub fn generate_full(program: &Program, opts: &CodegenOptions) -> Result<Generated, Vec<Diag>> {
    let mut g = Gen {
        defines: opts.defines.iter().cloned().collect(),
        debug: opts.debug,
        structs: HashMap::new(),
        unions: HashSet::new(),
        labels: Vec::new(),
        aliases: HashMap::new(),
        scopes: vec![HashMap::new()],
        builtins: HashMap::new(),
        errors: Vec::new(),
        uses_core: false,
        wasm_funcs: opts.wasm_funcs.iter().cloned().collect(),
        imports: Vec::new(),
    };
    let body = g.expand(&program.body);

    // Pass 1: struct dan alias.
    for s in &body {
        match &s.kind {
            StmtKind::Struct(sd) => {
                if g.structs.contains_key(&sd.name) {
                    g.err(sd.span, format!("Struct '{}' sudah dideklarasikan", sd.name));
                }
                g.structs.insert(sd.name.clone(), sd.fields.clone());
                if sd.is_union {
                    g.unions.insert(sd.name.clone());
                }
            }
            StmtKind::Alias(a) => {
                g.aliases.insert(a.name.clone(), a.target.clone());
            }
            _ => {}
        }
    }

    // Pass 2: `play` menentukan builtin yang terlihat.
    for s in &body {
        if let StmtKind::Play(p) = &s.kind {
            g.use_play(p, s.span);
        }
    }

    // Pass 3: deklarasi top-level, agar bisa dirujuk sebelum didefinisikan.
    for s in &body {
        match &s.kind {
            StmtKind::Func(f) => {
                let info = FnInfo {
                    params: f.params.iter().map(|p| p.name.clone()).collect(),
                    int_ret: f.ret.as_ref().map_or(false, |t| g.is_int_type(t)),
                };
                let sym = Sym { func: Some(info), ..Sym::var(false, None, false) };
                g.declare(&f.name, sym, f.name_span);
            }
            StmtKind::Extern(ext) => {
                let info = FnInfo {
                    params: ext.params.iter().map(|p| p.name.clone()).collect(),
                    int_ret: ext.ret.as_ref().map_or(false, |t| g.is_int_type(t)),
                };
                let sym = Sym { func: Some(info), ..Sym::var(false, None, false) };
                g.declare(&ext.name, sym, ext.name_span);
            }
            StmtKind::Var(v) if v.kind != VarKind::State => {
                let int = v.ty.as_ref().map_or(false, |t| g.is_int_type(t));
                g.declare(&v.name, Sym::var(int, v.ty.clone(), v.dims.is_some()), v.span);
            }
            StmtKind::Var(v) => {
                g.err(v.span, "'state' hanya boleh di dalam badan fun");
            }
            _ => {}
        }
    }

    // Pass 4: emisi kode, dengan urutan: struct, variabel global, objek state, fungsi, statement lain.
    let mut code = String::new();

    for s in &body {
        if let StmtKind::Struct(sd) = &s.kind {
            let text = g.struct_factory(sd);
            code.push_str(&text);
        }
    }

    for s in &body {
        if let StmtKind::Var(v) = &s.kind {
            if v.kind != VarKind::State {
                let init = g.init_value(v.ty.as_ref(), v.dims.as_ref(), v.init.as_ref());
                let init = g.scalar_init(v.ty.as_ref(), v.dims.is_some(), init);
                let kw = if v.kind == VarKind::Dyn { "var" } else { "let" };
                code.push_str(&format!("{} ${} = {};\n", kw, v.name, init));
            }
        }
    }

    for s in &body {
        if let StmtKind::Func(f) = &s.kind {
            if let Some(line) = g.state_object(f) {
                code.push_str(&line);
                code.push('\n');
            }
        }
    }

    for s in &body {
        if let StmtKind::Func(f) = &s.kind {
            let text = if g.wasm_funcs.contains(&f.name) {
                g.wasm_wrapper(f)
            } else {
                g.function(f, true, 0)
            };
            code.push_str(&text);
        }
    }

    for s in &body {
        match &s.kind {
            StmtKind::Struct(_) | StmtKind::Alias(_) | StmtKind::Play(_) | StmtKind::Func(_) | StmtKind::Var(_) => {}
            _ => g.stmt(s, 0, &mut code),
        }
    }

    // Entry point: tepat satu `export default fun Nama()` tanpa parameter.
    let entries: Vec<&FuncDecl> = body
        .iter()
        .filter_map(|s| match &s.kind {
            StmtKind::Func(f) if f.is_default => Some(f),
            _ => None,
        })
        .collect();
    match entries.len() {
        0 => g.err(
            Span::new(0, 0),
            "Entry point tidak ditemukan. Tambahkan 'export default fun Here() { ... }'",
        ),
        1 => {
            let f = entries[0];
            if !f.dynamic || !f.params.is_empty() {
                g.err(f.name_span, "Entry point harus berupa 'fun' tanpa parameter");
            }
            code.push_str(&format!("__mount(() => ${}());\n", f.name));
        }
        _ => g.err(entries[1].name_span, "Hanya boleh ada satu 'export default fun' per proyek"),
    }

    if !g.errors.is_empty() {
        return Err(g.errors);
    }
    let needs_core = if g.uses_core { "true" } else { "false" };
    let needs_app = if g.wasm_funcs.is_empty() { "false" } else { "true" };
    let wrapped = format!("__start({}, {}, () => {{\n{}}});\n", needs_core, needs_app, code);
    // Import harus di tingkat atas modul, jadi ditaruh paling depan.
    let imports = if g.imports.is_empty() {
        String::new()
    } else {
        format!("{}\n", g.imports.join("\n"))
    };
    Ok(Generated {
        js: format!("{}{}\n{}", imports, crate::shake::shake(PRELUDE, &wrapped), wrapped),
        uses_core: g.uses_core,
    })
}

impl Gen {
    fn err(&mut self, span: Span, msg: impl Into<String>) {
        self.errors.push(Diag::new(msg, span));
    }

    fn lookup(&self, name: &str) -> Option<Sym> {
        for scope in self.scopes.iter().rev() {
            if let Some(sym) = scope.get(name) {
                return Some(sym.clone());
            }
        }
        None
    }

    fn declare(&mut self, name: &str, sym: Sym, span: Span) {
        let duplicate = self.scopes.last().map_or(false, |s| s.contains_key(name));
        if duplicate {
            self.err(span, format!("Nama '{}' sudah dideklarasikan di scope ini", name));
            return;
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), sym);
        }
    }

    // ---------- Module, expansion, dan tipe ----------

    /// `play ... from "url";` menjadi import JavaScript. Nama lokal diberi `$` seperti identifier lain.
    fn add_import(&mut self, p: &PlayDecl, url: &str, span: Span) {
        let src = js_str(url);
        if let Some(local) = &p.default_name {
            self.imports.push(format!("import ${} from {};", local, src));
            self.declare(local, Sym::var(false, None, false), span);
        }
        if let Some(local) = &p.namespace {
            self.imports.push(format!("import * as ${} from {};", local, src));
            self.declare(local, Sym::var(false, None, false), span);
        }
        if !p.named.is_empty() {
            let items: Vec<String> = p
                .named
                .iter()
                .map(|(imp, local)| format!("{} as ${}", imp, local))
                .collect();
            self.imports.push(format!("import {{ {} }} from {};", items.join(", "), src));
            for (_, local) in &p.named {
                self.declare(local, Sym::var(false, None, false), span);
            }
        }
    }

    fn use_play(&mut self, p: &PlayDecl, span: Span) {
        if let Some(url) = &p.url {
            self.add_import(p, url, span);
            return;
        }
        // Impor bernama dari modul bawaan: `play { sqrt as akar } from Math;`
        if !p.named.is_empty() {
            let table = match module_table(&p.module) {
                Ok(t) => t,
                Err(msg) => {
                    self.err(span, msg);
                    return;
                }
            };
            if matches!(p.module.as_str(), "Memory" | "Math" | "Graphics" | "Graphics.Browser") {
                self.uses_core = true;
            }
            for (imp, local) in &p.named {
                match table.iter().find(|(tn, _)| *tn == imp.as_str()) {
                    Some((_, b)) => {
                        self.builtins.insert(local.clone(), *b);
                    }
                    None => self.err(span, format!("'{}' tidak ada di modul '{}'", imp, p.module)),
                }
            }
            return;
        }
        if p.alias.is_some() {
            self.err(span, "'play ... as ...' belum didukung oleh backend tahap ini");
            return;
        }
        let table = match module_table(&p.module) {
            Ok(t) => t,
            Err(msg) => {
                self.err(span, msg);
                return;
            }
        };
        if matches!(p.module.as_str(), "Memory" | "Math" | "Graphics" | "Graphics.Browser") {
            self.uses_core = true;
        }
        match &p.names {
            None => {
                for (name, b) in table {
                    self.builtins.insert(name.to_string(), *b);
                }
            }
            Some(names) => {
                for n in names {
                    match table.iter().find(|(tn, _)| *tn == n.as_str()) {
                        Some((_, b)) => {
                            self.builtins.insert(n.clone(), *b);
                        }
                        None => self.err(span, format!("'{}' tidak ada di modul '{}'", n, p.module)),
                    }
                }
            }
        }
    }

    /// Menerapkan static if / version / debug: mengembalikan daftar statement yang berlaku.
    fn expand(&mut self, list: &[Stmt]) -> Vec<Stmt> {
        let mut out = Vec::new();
        for s in list {
            match &s.kind {
                StmtKind::StaticIf { cond, then, alt } => {
                    let v = match self.eval_static(cond) {
                        Some(b) => b,
                        None => {
                            self.err(cond.span, "Kondisi static if/version hanya boleh berisi define, angka, bool, !, &&, ||, ==, dan !=");
                            false
                        }
                    };
                    let chosen: Vec<Stmt> = if v { then.clone() } else { alt.clone() };
                    let expanded = self.expand(&chosen);
                    out.extend(expanded);
                }
                StmtKind::Debug(body) => {
                    if self.debug {
                        let expanded = self.expand(body);
                        out.extend(expanded);
                    }
                }
                _ => out.push(s.clone()),
            }
        }
        out
    }

    fn eval_static(&self, e: &Expr) -> Option<bool> {
        match &e.kind {
            ExprKind::Ident(n) => Some(self.defines.contains(n)),
            ExprKind::Int(v) => Some(*v != 0),
            ExprKind::Bool(b) => Some(*b),
            ExprKind::Unary { op, arg } if *op == "!" => self.eval_static(arg).map(|b| !b),
            ExprKind::Binary { op, l, r } => {
                let a = self.eval_static(l)?;
                let b = self.eval_static(r)?;
                match *op {
                    "&&" => Some(a && b),
                    "||" => Some(a || b),
                    "==" | "===" => Some(a == b),
                    "!=" | "!==" => Some(a != b),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Mengikuti alias sampai tipe dasar (batas 16 langkah untuk mencegah siklus).
    fn resolve(&self, ty: &TypeExpr) -> TypeExpr {
        let mut cur = ty.clone();
        for _ in 0..16 {
            let next = match &cur {
                TypeExpr::Name { name, span } => match keyword_alias(name) {
                    Some(canon) => Some(TypeExpr::Name { name: canon.to_string(), span: *span }),
                    None => self.aliases.get(name).cloned(),
                },
                _ => None,
            };
            match next {
                Some(t) => cur = t,
                None => break,
            }
        }
        cur
    }

    fn name_of(&self, ty: &TypeExpr) -> Option<String> {
        match self.resolve(ty) {
            TypeExpr::Name { name, .. } => Some(name),
            _ => None,
        }
    }

    fn is_int_type(&self, ty: &TypeExpr) -> bool {
        self.name_of(ty).map_or(false, |n| BOX_INTS.contains(&n.as_str()))
    }

    fn struct_name(&self, ty: &TypeExpr) -> Option<String> {
        let n = self.name_of(ty)?;
        if self.structs.contains_key(&n) {
            Some(n)
        } else {
            None
        }
    }

    /// Nama struct yang ditunjuk tipe ini, termasuk lewat pointer.
    fn deref_struct(&self, ty: &TypeExpr) -> Option<String> {
        match self.resolve(ty) {
            TypeExpr::Ptr(inner) => self.struct_name(&inner),
            other => self.struct_name(&other),
        }
    }

    fn default_value(&mut self, ty: &TypeExpr) -> String {
        let t = self.resolve(ty);
        match &t {
            TypeExpr::Ptr(_) => "null".to_string(),
            TypeExpr::Name { name, span } => {
                if name == "char" {
                    "\"\"".to_string()
                } else if name == "box00" {
                    "undefined".to_string()
                } else if name == "bool" {
                    "false".to_string()
                } else if BOX_TYPES.contains(&name.as_str()) {
                    "0".to_string()
                } else if self.structs.contains_key(name) {
                    format!("__new_{}()", name)
                } else {
                    let sp = *span;
                    self.err(sp, format!("Tipe '{}' tidak dikenal", name));
                    "undefined".to_string()
                }
            }
        }
    }

    /// Mendaftarkan nama `var` di scope saat ini sebelum body dijalankan (hoisting JS).
    fn hoist_vars(&mut self, list: &[Stmt]) {
        let mut names = Vec::new();
        collect_vars(list, &mut names);
        for name in names {
            let present = self.scopes.last().map_or(false, |sc| sc.contains_key(&name));
            if present {
                continue;
            }
            let mut sym = Sym::var(false, None, false);
            sym.hoisted = true;
            if let Some(sc) = self.scopes.last_mut() {
                sc.insert(name, sym);
            }
        }
    }

    /// Apakah deklarasi `var` ini nama yang sudah di-hoist (tidak dideklarasikan ulang).
    fn var_hoisted(&self, name: &str, kind: VarKind) -> bool {
        kind == VarKind::Dyn && self.lookup(name).map_or(false, |s| s.hoisted)
    }

    /// Pembungkus JS untuk fungsi yang sudah di WASM. Tipe 64-bit lewat BigInt.
    fn wasm_wrapper(&mut self, f: &FuncDecl) -> String {
        let is_i64 = |n: &str| crate::wasm_emit::num_of(n).map_or(false, |x| x.val == crate::wasm_encode::ValTy::I64);
        let mut params = Vec::new();
        let mut args = Vec::new();
        for p in &f.params {
            let name = format!("${}", p.name);
            let wide = p.ty.as_ref().and_then(|t| self.name_of(t)).map_or(false, |n| is_i64(&n));
            args.push(if wide { format!("BigInt({})", name) } else { name.clone() });
            params.push(name);
        }
        let call = format!("__app.{}({})", f.name, args.join(", "));
        let ret_wide = f.ret.as_ref().and_then(|t| self.name_of(t)).map_or(false, |n| is_i64(&n));
        let body = if ret_wide { format!("Number({})", call) } else { call };
        format!("function ${}({}) {{ return {}; }}\n", f.name, params.join(", "), body)
    }

    /// Membungkus nilai integer ke lebar tipe `name`, seperti tipe sungguhan di C.
    /// Tipe 8 byte dan float tidak dibungkus: JS number sudah cukup untuk mereka.
    fn wrap_int(name: &str, js: String) -> String {
        match name {
            "box01" => format!("(({}) & 255)", js),
            "box11" => format!("((({}) << 24) >> 24)", js),
            "box02" => format!("(({}) & 65535)", js),
            "box12" => format!("((({}) << 16) >> 16)", js),
            "box14" => format!("(({}) | 0)", js),
            "box04" => format!("(({}) >>> 0)", js),
            _ => js,
        }
    }

    /// Inisialisasi variabel bertipe skalar: dibungkus sesuai lebar tipenya.
    fn scalar_init(&mut self, ty: Option<&TypeExpr>, dims: bool, js: String) -> String {
        if dims {
            return js;
        }
        match ty.and_then(|t| self.name_of(t)) {
            Some(n) => Self::wrap_int(&n, js),
            None => js,
        }
    }

    fn field_default(&mut self, f: &FieldDecl) -> String {
        match &f.dims {
            Some(d) => {
                let n = self.expr(d);
                let base = self.default_value(&f.ty);
                format!("__arr([], {}, () => {})", n, base)
            }
            None => self.default_value(&f.ty),
        }
    }

    fn struct_factory(&mut self, sd: &StructDecl) -> String {
        let mut parts = Vec::new();
        for f in &sd.fields {
            let v = self.field_default(f);
            parts.push(format!("{}: {}", js_str(&f.name), v));
        }
        format!("function __new_{}() {{\n  return {{ {} }};\n}}\n", sd.name, parts.join(", "))
    }

    /// Nilai untuk tipe tertentu, termasuk literal `{...}` untuk struct atau array.
    fn value_for(&mut self, ty: Option<&TypeExpr>, e: &Expr) -> String {
        if let ExprKind::Brace(elems) = &e.kind {
            if let Some(t) = ty {
                if let Some(sname) = self.struct_name(t) {
                    return self.struct_literal(&sname, elems, e.span);
                }
            }
            let mut parts = Vec::new();
            for x in elems {
                parts.push(self.value_for(None, x));
            }
            return format!("[{}]", parts.join(", "));
        }

        let js = self.expr(e);
        if let Some(t) = ty {
            let is_place = matches!(e.kind, ExprKind::Ident(_) | ExprKind::Member { .. } | ExprKind::Index { .. });
            if is_place && self.struct_name(t).is_some() {
                // Struct bernilai disalin, bukan dibagi referensinya.
                return format!("structuredClone({})", js);
            }
        }
        js
    }

    fn struct_literal(&mut self, name: &str, elems: &[Expr], span: Span) -> String {
        let fields = match self.structs.get(name) {
            Some(f) => f.clone(),
            None => return "undefined".to_string(),
        };
        if elems.len() > fields.len() {
            self.err(span, format!("Terlalu banyak nilai untuk struct '{}' ({} field)", name, fields.len()));
        }
        let mut parts = Vec::new();
        for (i, f) in fields.iter().enumerate() {
            let v = if i < elems.len() {
                self.value_for(Some(&f.ty), &elems[i])
            } else {
                self.field_default(f)
            };
            parts.push(format!("{}: {}", js_str(&f.name), v));
        }
        format!("{{ {} }}", parts.join(", "))
    }

    /// Inisialisasi variabel, termasuk array dengan dimensi.
    fn init_value(&mut self, ty: Option<&TypeExpr>, dims: Option<&Expr>, init: Option<&Expr>) -> String {
        match dims {
            Some(d) => {
                let n = self.expr(d);
                if let Some(Expr { kind: ExprKind::Brace(elems), .. }) = init {
                    let mut items = Vec::new();
                    for x in elems {
                        items.push(self.value_for(ty, x));
                    }
                    let fill = match ty {
                        Some(t) => self.default_value(t),
                        None => "undefined".to_string(),
                    };
                    return format!("__arr([{}], {}, () => {})", items.join(", "), n, fill);
                }
                if let Some(e) = init {
                    return self.expr(e);
                }
                let fill = match ty {
                    Some(t) => self.default_value(t),
                    None => "undefined".to_string(),
                };
                format!("__arr([], {}, () => {})", n, fill)
            }
            None => match init {
                Some(e) => self.value_for(ty, e),
                None => match ty {
                    Some(t) => self.default_value(t),
                    None => "undefined".to_string(),
                },
            },
        }
    }

    // ---------- State dan fungsi ----------

    /// Objek state untuk satu fungsi komponen, mis. `const __st_Counter = { "count": 0 };`.
    fn state_object(&mut self, f: &FuncDecl) -> Option<String> {
        let body = self.expand(&f.body);
        let mut parts = Vec::new();
        for s in &body {
            if let StmtKind::Var(v) = &s.kind {
                if v.kind == VarKind::State {
                    let init = match &v.init {
                        Some(e) => self.value_for(None, e),
                        None => "0".to_string(),
                    };
                    parts.push(format!("{}: {}", js_str(&v.name), init));
                }
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(format!("const __st_{} = {{ {} }};", f.name, parts.join(", ")))
        }
    }

    fn register_state(&mut self, fname: &str, v: &VarDecl) {
        let sym = Sym {
            int: false,
            ty: None,
            is_array: false,
            func: None,
            state_obj: Some(format!("__st_{}", fname)),
            hoisted: false,
        };
        self.declare(&v.name, sym, v.span);
    }

    /// Fungsi JS. `top` menandai fungsi level atas, satu-satunya tempat `state` boleh dideklarasikan.
    /// `goto` tidak boleh melintasi batas fungsi: label luar tidak terlihat dari dalam.
    fn function(&mut self, f: &FuncDecl, top: bool, d: usize) -> String {
        let saved = std::mem::take(&mut self.labels);
        let text = self.function_inner(f, top, d);
        self.labels = saved;
        text
    }

    fn function_inner(&mut self, f: &FuncDecl, top: bool, d: usize) -> String {
        let mut scope = HashMap::new();
        for p in &f.params {
            let int = p.ty.as_ref().map_or(false, |t| self.is_int_type(t));
            scope.insert(p.name.clone(), Sym::var(int, p.ty.clone(), false));
        }
        self.scopes.push(scope);

        let body = self.expand(&f.body);
        self.hoist_vars(&body);

        // State didaftarkan dulu agar bisa dirujuk dari mana pun di fungsi.
        for s in &body {
            if let StmtKind::Var(v) = &s.kind {
                if v.kind == VarKind::State {
                    if top {
                        self.register_state(&f.name, v);
                    } else {
                        self.err(s.span, "'state' hanya boleh di level atas badan fun");
                    }
                }
            }
        }

        let mut inner = String::new();
        for s in &body {
            if matches!(&s.kind, StmtKind::Var(v) if v.kind == VarKind::State) {
                continue;
            }
            self.stmt(s, d + 1, &mut inner);
        }
        self.scopes.pop();

        let params: Vec<String> = f.params.iter().map(|p| format!("${}", p.name)).collect();
        let pad = indent(d);
        format!(
            "{}{}function ${}({}) {{\n{}{}}}\n",
            pad,
            if f.is_async { "async " } else { "" },
            f.name,
            params.join(", "),
            inner,
            pad
        )
    }

    // ---------- Statement ----------

    fn block_body(&mut self, list: &[Stmt], d: usize, out: &mut String) {
        let expanded = self.expand(list);
        for s in &expanded {
            self.stmt(s, d, out);
        }
    }

    /// Body sebagai teks `{ ... }` tanpa indentasi awal, untuk if/while/for.
    fn body_text(&mut self, s: &Stmt, d: usize) -> String {
        let mut inner = String::new();
        self.scopes.push(HashMap::new());
        match &s.kind {
            StmtKind::Block(list) => self.block_body(list, d + 1, &mut inner),
            _ => self.stmt(s, d + 1, &mut inner),
        }
        self.scopes.pop();
        format!("{{\n{}{}}}", inner, indent(d))
    }

    fn if_text(&mut self, test: &Expr, cons: &Stmt, alt: Option<&Stmt>, d: usize) -> String {
        let t = self.expr(test);
        let mut text = format!("if ({}) {}", t, self.body_text(cons, d));
        if let Some(a) = alt {
            match &a.kind {
                StmtKind::If { test, cons, alt } => {
                    let inner = self.if_text(test, cons, alt.as_deref(), d);
                    text.push_str(" else ");
                    text.push_str(&inner);
                }
                _ => {
                    let b = self.body_text(a, d);
                    text.push_str(" else ");
                    text.push_str(&b);
                }
            }
        }
        text
    }

    /// `let $x = ...` dengan pendaftaran simbol. Dipakai untuk variabel lokal dan header for.
    fn decl_part(&mut self, v: &VarDecl) -> String {
        let int = v.ty.as_ref().map_or(false, |t| self.is_int_type(t));
        let init = self.init_value(v.ty.as_ref(), v.dims.as_ref(), v.init.as_ref());
        let init = self.scalar_init(v.ty.as_ref(), v.dims.is_some(), init);
        if !self.var_hoisted(&v.name, v.kind) {
            self.declare(&v.name, Sym::var(int, v.ty.clone(), v.dims.is_some()), v.span);
        }
        format!("${} = {}", v.name, init)
    }

    fn stmt(&mut self, s: &Stmt, d: usize, out: &mut String) {
        let pad = indent(d);
        match &s.kind {
            StmtKind::Block(_) => {
                let text = self.body_text(s, d);
                out.push_str(&format!("{}{}\n", pad, text));
            }
            StmtKind::Expr(e) => {
                let js = self.expr(e);
                out.push_str(&format!("{}{};\n", pad, js));
            }
            StmtKind::Var(v) => {
                if v.kind == VarKind::State {
                    self.err(v.span, "'state' hanya boleh di level atas badan fun");
                } else {
                    // `var` tetap ditulis di deklarasi meski sudah di-hoist (sama seperti JS).
                    let part = self.decl_part(v);
                    let kw = if v.kind == VarKind::Dyn { "var" } else { "let" };
                    out.push_str(&format!("{}{} {};\n", pad, kw, part));
                }
            }
            StmtKind::Func(f) => {
                let info = FnInfo {
                    params: f.params.iter().map(|p| p.name.clone()).collect(),
                    int_ret: f.ret.as_ref().map_or(false, |t| self.is_int_type(t)),
                };
                let sym = Sym { func: Some(info), ..Sym::var(false, None, false) };
                self.declare(&f.name, sym, f.name_span);
                let text = self.function(f, false, d);
                out.push_str(&text);
            }
            StmtKind::If { test, cons, alt } => {
                let text = self.if_text(test, cons, alt.as_deref(), d);
                out.push_str(&format!("{}{}\n", pad, text));
            }
            StmtKind::While { test, body } => {
                let t = self.expr(test);
                let b = self.body_text(body, d);
                out.push_str(&format!("{}while ({}) {}\n", pad, t, b));
            }
            StmtKind::DoWhile { body, test } => {
                let b = self.body_text(body, d);
                let t = self.expr(test);
                out.push_str(&format!("{}do {} while ({});\n", pad, b, t));
            }
            StmtKind::For { init, test, update, body } => {
                self.scopes.push(HashMap::new());
                let init_js = match init {
                    None => String::new(),
                    Some(ForInit::Expr(e)) => self.expr(e),
                    Some(ForInit::Decls(decls)) => {
                        let kw = match decls.first().map(|v| v.kind) {
                            Some(VarKind::Dyn) => "var ",
                            _ => "let ",
                        };
                        let mut parts = Vec::new();
                        for v in decls {
                            parts.push(self.decl_part(v));
                        }
                        format!("{}{}", kw, parts.join(", "))
                    }
                };
                let test_js = match test {
                    Some(t) => self.expr(t),
                    None => String::new(),
                };
                let update_js = match update {
                    Some(u) => self.expr(u),
                    None => String::new(),
                };
                let b = self.body_text(body, d);
                self.scopes.pop();
                out.push_str(&format!("{}for ({}; {}; {}) {}\n", pad, init_js, test_js, update_js, b));
            }
            StmtKind::Switch { disc, cases } => {
                let dj = self.expr(disc);
                let mut text = format!("{}switch ({}) {{\n", pad, dj);
                for c in cases {
                    match &c.test {
                        Some(t) => {
                            let tj = self.expr(t);
                            text.push_str(&format!("{}case {}:\n", indent(d + 1), tj));
                        }
                        None => text.push_str(&format!("{}default:\n", indent(d + 1))),
                    }
                    self.scopes.push(HashMap::new());
                    self.block_body(&c.body, d + 2, &mut text);
                    self.scopes.pop();
                }
                text.push_str(&format!("{}}}\n", pad));
                out.push_str(&text);
            }
            StmtKind::Return(arg) => match arg {
                Some(e) => {
                    let js = self.value_for(None, e);
                    out.push_str(&format!("{}return {};\n", pad, js));
                }
                None => out.push_str(&format!("{}return;\n", pad)),
            },
            StmtKind::Break => out.push_str(&format!("{}break;\n", pad)),
            StmtKind::Continue => out.push_str(&format!("{}continue;\n", pad)),
            StmtKind::Try { body, catch_body } => {
                out.push_str(&format!("{}try {{\n", pad));
                self.scopes.push(HashMap::new());
                self.block_body(body, d + 1, out);
                self.scopes.pop();
                out.push_str(&format!("{}}} catch (__e) {{\n", pad));
                if let Some(cb) = catch_body {
                    self.scopes.push(HashMap::new());
                    self.block_body(cb, d + 1, out);
                    self.scopes.pop();
                }
                out.push_str(&format!("{}}}\n", pad));
            }
            StmtKind::Throw(e) => {
                let js = self.value_for(None, e);
                out.push_str(&format!("{}throw {};\n", pad, js));
            }
            StmtKind::Asm(_) => {
                self.err(s.span, "asm hanya didukung di fungsi bertipe yang dikompilasi ke WASM");
            }
            StmtKind::Labeled { name, body } => {
                out.push_str(&format!("{}{}: {{\n", pad, name));
                self.labels.push(name.clone());
                self.scopes.push(HashMap::new());
                self.block_body(body, d + 1, out);
                self.scopes.pop();
                self.labels.pop();
                out.push_str(&format!("{}}}\n", pad));
            }
            StmtKind::Goto(name) => {
                if self.labels.iter().any(|l| l == name) {
                    out.push_str(&format!("{}break {};\n", pad, name));
                } else {
                    self.err(
                        s.span,
                        format!("goto '{}' hanya boleh ke label yang melingkupinya", name),
                    );
                }
            }
            StmtKind::Extern(ext) => {
                if self.scopes.len() > 1 {
                    self.err(s.span, "extern hanya boleh di level atas file");
                }
                // Binding ditunda sampai pemanggilan: library dari <script> di <Head> dimuat asinkron.
                out.push_str(&format!(
                    "{}const ${} = (...__a) => globalThis[{}](...__a);\n",
                    pad, ext.name, js_str(&ext.name)
                ));
            }
            StmtKind::Struct(_) | StmtKind::Alias(_) | StmtKind::Play(_) => {
                self.err(s.span, "deklarasi ini hanya boleh di level atas file");
            }
            StmtKind::Debug(_) | StmtKind::StaticIf { .. } => {}
        }
    }

    // ---------- Ekspresi ----------

    fn expr(&mut self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Int(v) => v.to_string(),
            ExprKind::Float(v) => format!("{}", v),
            ExprKind::Str(s) => js_str(s),
            ExprKind::Bool(b) => b.to_string(),
            ExprKind::Null => "null".to_string(),
            ExprKind::Ident(n) => self.ident(n, e.span),
            ExprKind::Binary { op, l, r } => self.binary(op, l, r),
            ExprKind::Unary { op, arg } => {
                let a = self.expr(arg);
                match *op {
                    // Alamat dan dereferensi tidak mengubah objek di JS.
                    "&" | "*" => a,
                    _ => format!("({}{})", op, a),
                }
            }
            ExprKind::Update { op, arg, prefix } => self.update(op, arg, *prefix),
            ExprKind::Assign { op, target, value } => self.assign(op, target, value, e.span),
            ExprKind::Cond { test, cons, alt } => {
                let t = self.expr(test);
                let c = self.expr(cons);
                let a = self.expr(alt);
                format!("({} ? {} : {})", t, c, a)
            }
            ExprKind::Call { callee, args } => self.call(callee, args, e.span),
            ExprKind::Member { obj, prop, .. } => {
                let o = self.expr(obj);
                format!("{}.{}", o, prop)
            }
            ExprKind::Index { obj, index } => {
                let o = self.expr(obj);
                let i = self.expr(index);
                format!("{}[{}]", o, i)
            }
            ExprKind::Arrow { params, body, is_async } => self.arrow(params, body, *is_async),
            ExprKind::Await(arg) => {
                let a = self.expr(arg);
                format!("(await {})", a)
            }
            ExprKind::Brace(_) => self.value_for(None, e),
            ExprKind::ObjectLit(props) => {
                let mut parts = Vec::new();
                for (k, v) in props {
                    let js = self.expr(v);
                    parts.push(format!("{}: {}", js_str(k), js));
                }
                format!("({{ {} }})", parts.join(", "))
            }
            ExprKind::Offset { ty, field } => self.offset_of(ty, field, e.span),
            ExprKind::Jsx(el) => self.jsx(el),
        }
    }

    fn ident(&mut self, name: &str, span: Span) -> String {
        if let Some(sym) = self.lookup(name) {
            return match &sym.state_obj {
                Some(obj) => format!("{}.{}", obj, name),
                None => format!("${}", name),
            };
        }
        match self.builtins.get(name).copied() {
            Some(Builtin::Js(js)) => js.to_string(),
            Some(_) => {
                self.err(span, format!("'{}' hanya bisa dipakai sebagai komponen JSX atau pemanggilan", name));
                "undefined".to_string()
            }
            None if JS_GLOBALS.contains(&name) => name.to_string(),
            None => {
                self.err(
                    span,
                    format!("'{}' tidak dikenal. Tambahkan 'play <Modul>;' jika ini builtin, atau deklarasikan dulu", name),
                );
                "undefined".to_string()
            }
        }
    }

    /// Simbol state dan namanya, jika `e` adalah identifier state.
    fn state_target(&self, e: &Expr) -> Option<(String, String)> {
        if let ExprKind::Ident(n) = &e.kind {
            let sym = self.lookup(n)?;
            let obj = sym.state_obj?;
            return Some((obj, n.clone()));
        }
        None
    }

    fn binary(&mut self, op: &str, l: &Expr, r: &Expr) -> String {
        let int_div = op == "/" && self.is_int(l) && self.is_int(r);
        let unsigned = op == ">>" && self.is_unsigned(l);
        let a = self.expr(l);
        let b = self.expr(r);
        if int_div {
            return format!("Math.trunc({} / {})", a, b);
        }
        let js_op = match op {
            "==" => "===",
            "!=" => "!==",
            ">>" if unsigned => ">>>",
            other => other,
        };
        format!("({} {} {})", a, js_op, b)
    }

    fn update(&mut self, op: &str, arg: &Expr, prefix: bool) -> String {
        if !is_lvalue(arg) {
            self.err(arg.span, "Operand ++ atau -- harus berupa l-value");
        }
        let delta = if op == "++" { "1" } else { "-1" };
        if let Some((obj, key)) = self.state_target(arg) {
            return format!("__inc({}, {}, {}, {})", obj, js_str(&key), delta, prefix);
        }
        let a = self.expr(arg);
        if prefix {
            format!("({}{})", op, a)
        } else {
            format!("({}{})", a, op)
        }
    }

    fn assign(&mut self, op: &str, target: &Expr, value: &Expr, span: Span) -> String {
        if !is_lvalue(target) {
            self.err(span, "Target penugasan harus berupa l-value");
        }
        let rhs = self.value_for(None, value);
        let int_div = op == "/=" && self.is_int(target) && self.is_int(value);

        let combine = |cur: &str| -> String {
            if op == "=" {
                rhs.clone()
            } else if int_div {
                format!("Math.trunc({} / {})", cur, rhs)
            } else {
                let bop = &op[..op.len() - 1];
                format!("({} {} {})", cur, bop, rhs)
            }
        };

        if let Some((obj, key)) = self.state_target(target) {
            let cur = format!("{}.{}", obj, key);
            let v = combine(&cur);
            return format!("__set({}, {}, {})", obj, js_str(&key), v);
        }

        let lhs = self.expr(target);
        let v = combine(&lhs);
        let v = match self.expr_ty(target).and_then(|t| self.name_of(&t)) {
            Some(n) => Self::wrap_int(&n, v),
            None => v,
        };
        format!("({} = {})", lhs, v)
    }

    fn call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> String {
        if let ExprKind::Ident(name) = &callee.kind {
            if let Some(sym) = self.lookup(name) {
                if let Some(info) = sym.func {
                    return self.user_call(name, &info, args, span);
                }
            } else if let Some(b) = self.builtins.get(name).copied() {
                match b {
                    Builtin::Cast => return self.cast(args, span),
                    Builtin::SizeOf => return self.sizeof(args, span),
                    Builtin::Js(js) => {
                        let a = self.args(args);
                        return format!("{}({})", js, a);
                    }
                    Builtin::Ui(_) => {
                        self.err(span, format!("komponen '{}' hanya bisa dipakai di dalam JSX", name));
                        return "undefined".to_string();
                    }
                }
            }
        }
        let c = self.expr(callee);
        let a = self.args(args);
        format!("{}({})", c, a)
    }

    fn user_call(&mut self, name: &str, info: &FnInfo, args: &[Expr], span: Span) -> String {
        if args.len() != info.params.len() {
            self.err(
                span,
                format!("Fungsi '{}' membutuhkan {} argumen, diberikan {}", name, info.params.len(), args.len()),
            );
        }
        let a = self.args(args);
        format!("${}({})", name, a)
    }

    fn args(&mut self, args: &[Expr]) -> String {
        let mut parts = Vec::new();
        for a in args {
            parts.push(self.value_for(None, a));
        }
        parts.join(", ")
    }

    fn cast(&mut self, args: &[Expr], span: Span) -> String {
        let (value, tname) = match args {
            [v, Expr { kind: ExprKind::Ident(t), .. }] => (v, t.clone()),
            _ => {
                self.err(span, "Pemakaian: Cast(nilai, TipeTujuan)");
                return "undefined".to_string();
            }
        };
        let x = self.expr(value);
        let int = format!("Math.trunc(Number({}))", x);
        match tname.as_str() {
            "box01" => format!("({} & 255)", int),
            "box11" => format!("(({} << 24) >> 24)", int),
            "box02" => format!("({} & 65535)", int),
            "box12" => format!("(({} << 16) >> 16)", int),
            "box14" => format!("({} | 0)", int),
            "box04" => format!("({} >>> 0)", int),
            "box08" | "box18" => int,
            "box24" => format!("Math.fround(Number({}))", x),
            "box28" => format!("Number({})", x),
            "char" => format!("String({})", x),
            _ => {
                self.err(span, format!("Cast ke tipe '{}' tidak didukung", tname));
                "undefined".to_string()
            }
        }
    }

    fn sizeof(&mut self, args: &[Expr], span: Span) -> String {
        let name = match args {
            [Expr { kind: ExprKind::Ident(n), .. }] => n.clone(),
            _ => {
                self.err(span, "Pemakaian: SizeOf(TipeTujuan)");
                return "0".to_string();
            }
        };
        let ty = TypeExpr::Name { name, span };
        match self.size_of(&ty) {
            Some(n) => n.to_string(),
            None => {
                self.err(span, "Ukuran tipe tidak dapat dihitung di tahap ini");
                "0".to_string()
            }
        }
    }

    /// Ukuran satu field, termasuk jumlah elemen bila array.
    fn field_size(&self, f: &FieldDecl) -> Option<u64> {
        let elem = self.size_of(&f.ty)?;
        let count: u64 = match &f.dims {
            None => 1,
            Some(Expr { kind: ExprKind::Int(n), .. }) => *n as u64,
            Some(_) => return None,
        };
        Some(elem * count)
    }

    /// `offset(Struct, field)`: offset byte field. Untuk union selalu 0.
    fn offset_of(&mut self, ty: &str, field: &str, span: Span) -> String {
        let fields = match self.structs.get(ty) {
            Some(f) => f.clone(),
            None => {
                self.err(span, format!("struct '{}' tidak dikenal", ty));
                return "0".to_string();
            }
        };
        if self.unions.contains(ty) {
            if fields.iter().any(|f| f.name == field) {
                return "0".to_string();
            }
            self.err(span, format!("field '{}' tidak ada pada union '{}'", field, ty));
            return "0".to_string();
        }
        let mut off: u64 = 0;
        for f in &fields {
            if f.name == field {
                return off.to_string();
            }
            match self.field_size(f) {
                Some(sz) => off += sz,
                None => {
                    self.err(span, "ukuran field tidak dapat dihitung untuk offset");
                    return "0".to_string();
                }
            }
        }
        self.err(span, format!("field '{}' tidak ada pada struct '{}'", field, ty));
        "0".to_string()
    }

    fn size_of(&self, ty: &TypeExpr) -> Option<u64> {
        let t = self.resolve(ty);
        match &t {
            TypeExpr::Ptr(_) => Some(8),
            TypeExpr::Name { name, .. } => {
                if name == "char" {
                    return Some(8);
                }
                if let Some((_, sz)) = BOX_SIZES.iter().find(|(n, _)| *n == name.as_str()) {
                    return Some(*sz);
                }
                let fields = self.structs.get(name)?;
                let mut total = 0u64;
                for f in fields {
                    let elem = self.size_of(&f.ty)?;
                    let count: u64 = match &f.dims {
                        None => 1,
                        Some(Expr { kind: ExprKind::Int(n), .. }) => *n as u64,
                        Some(_) => return None,
                    };
                    total += elem * count;
                }
                Some(total)
            }
        }
    }

    fn arrow(&mut self, params: &[String], body: &ArrowBody, is_async: bool) -> String {
        let saved = std::mem::take(&mut self.labels);
        let text = self.arrow_inner(params, body, is_async);
        self.labels = saved;
        text
    }

    fn arrow_inner(&mut self, params: &[String], body: &ArrowBody, is_async: bool) -> String {
        let mut scope = HashMap::new();
        for p in params {
            scope.insert(p.clone(), Sym::var(false, None, false));
        }
        self.scopes.push(scope);
        let ps: Vec<String> = params.iter().map(|p| format!("${}", p)).collect();
        let head = format!("{}({})", if is_async { "async " } else { "" }, ps.join(", "));
        let text = match body {
            ArrowBody::Expr(e) => {
                let js = self.value_for(None, e);
                format!("({} => ({}))", head, js)
            }
            ArrowBody::Block(list) => {
                let mut inner = String::new();
                self.hoist_vars(list);
                self.block_body(list, 1, &mut inner);
                format!("({} => {{\n{}}})", head, inner)
            }
        };
        self.scopes.pop();
        text
    }

    // ---------- Informasi tipe sederhana ----------

    /// Tipe statis sebuah ekspresi jika diketahui (identifier, indeks array, field, `&x`).
    fn expr_ty(&self, e: &Expr) -> Option<TypeExpr> {
        match &e.kind {
            ExprKind::Ident(n) => self.lookup(n)?.ty,
            ExprKind::Index { obj, .. } => {
                if let ExprKind::Ident(n) = &obj.kind {
                    let sym = self.lookup(n)?;
                    if sym.is_array {
                        return sym.ty;
                    }
                }
                None
            }
            ExprKind::Member { obj, prop, .. } => self.field_ty(obj, prop),
            ExprKind::Unary { op, arg } if *op == "&" => {
                self.expr_ty(arg).map(|t| TypeExpr::Ptr(Box::new(t)))
            }
            _ => None,
        }
    }

    fn field_ty(&self, obj: &Expr, prop: &str) -> Option<TypeExpr> {
        let t = self.expr_ty(obj)?;
        let sname = self.deref_struct(&t)?;
        let fields = self.structs.get(&sname)?;
        fields.iter().find(|f| f.name == prop).map(|f| f.ty.clone())
    }

    fn is_int(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Int(_) => true,
            ExprKind::Ident(n) => self.lookup(n).map_or(false, |s| s.int),
            ExprKind::Binary { op, l, r } => {
                matches!(*op, "+" | "-" | "*" | "/" | "%" | "<<" | ">>" | "&" | "|" | "^")
                    && self.is_int(l)
                    && self.is_int(r)
            }
            ExprKind::Unary { op, arg } => *op == "-" && self.is_int(arg),
            ExprKind::Call { callee, args } => {
                if let ExprKind::Ident(n) = &callee.kind {
                    if let Some(sym) = self.lookup(n) {
                        return sym.func.map_or(false, |f| f.int_ret);
                    }
                    if n == "Cast" {
                        if let Some(Expr { kind: ExprKind::Ident(t), .. }) = args.get(1) {
                            return BOX_INTS.contains(&t.as_str());
                        }
                    }
                }
                false
            }
            _ => self.expr_ty(e).map_or(false, |t| self.is_int_type(&t)),
        }
    }

    fn is_unsigned(&self, e: &Expr) -> bool {
        self.expr_ty(e)
            .and_then(|t| self.name_of(&t))
            .map_or(false, |n| matches!(n.as_str(), "box01" | "box02" | "box04" | "box08"))
    }

    // ---------- JSX ----------

    fn jsx(&mut self, el: &JsxElement) -> String {
        let tag = match &el.tag {
            None => {
                let kids = self.jsx_kids(&el.children);
                return format!("[{}]", kids.join(", "));
            }
            Some(t) => t.clone(),
        };

        if tag.chars().next().map_or(false, |c| c.is_ascii_lowercase()) {
            let props = self.props_js(&el.attrs, None);
            let kids = self.jsx_kids(&el.children);
            return format!("__h({}, {}, [{}])", js_str(&tag), props, kids.join(", "));
        }

        if let Some(sym) = self.lookup(&tag) {
            if let Some(info) = sym.func {
                return self.component_call(&tag, &info, el);
            }
        }

        match self.builtins.get(&tag).copied() {
            Some(Builtin::Ui(name)) => {
                let props = self.props_js(&el.attrs, None);
                let kids = self.jsx_kids(&el.children);
                format!("__C.{}({}, [{}])", name, props, kids.join(", "))
            }
            _ => {
                self.err(
                    el.span,
                    format!(
                        "Komponen '{}' tidak dikenal. Tambahkan 'play UI;' atau definisikan 'fun {}(...)'",
                        tag, tag
                    ),
                );
                "undefined".to_string()
            }
        }
    }

    fn jsx_kids(&mut self, kids: &[JsxChild]) -> Vec<String> {
        let mut out = Vec::new();
        for k in kids {
            match k {
                JsxChild::Text(t) => out.push(js_str(t)),
                JsxChild::Expr(e) => out.push(self.expr(e)),
                JsxChild::Element(el) => out.push(self.jsx(el)),
            }
        }
        out
    }

    /// Pemanggilan komponen fun. Atribut dipetakan ke parameter menurut nama:
    /// `<Card product={p} />` menjadi `$Card(p)` jika parameternya bernama `product`.
    /// Parameter `props` menerima seluruh atribut sebagai objek.
    fn component_call(&mut self, name: &str, info: &FnInfo, el: &JsxElement) -> String {
        let kids = self.jsx_kids(&el.children);
        let kids_arr = format!("[{}]", kids.join(", "));
        let mut args = Vec::new();
        for p in &info.params {
            if p == "props" {
                let with_kids = if el.children.is_empty() { None } else { Some(kids_arr.clone()) };
                args.push(self.props_js(&el.attrs, with_kids));
            } else if p == "children" {
                args.push(kids_arr.clone());
            } else if let Some(value) = named_attr(&el.attrs, p) {
                let js = match value {
                    Some(e) => self.expr(e),
                    None => "true".to_string(),
                };
                args.push(js);
            } else {
                args.push("undefined".to_string());
            }
        }
        format!("${}({})", name, args.join(", "))
    }

    /// Objek props dari atribut JSX. Spread digabung dengan Object.assign.
    fn props_js(&mut self, attrs: &[JsxAttr], kids: Option<String>) -> String {
        let mut has_spread = false;
        let mut segs: Vec<String> = Vec::new();
        let mut cur: Vec<String> = Vec::new();
        for a in attrs {
            match a {
                JsxAttr::Named { name, value } => {
                    let v = match value {
                        Some(e) => self.expr(e),
                        None => "true".to_string(),
                    };
                    cur.push(format!("{}: {}", js_str(name), v));
                }
                JsxAttr::Spread(e) => {
                    has_spread = true;
                    if !cur.is_empty() {
                        segs.push(format!("{{ {} }}", cur.join(", ")));
                        cur.clear();
                    }
                    let js = self.expr(e);
                    segs.push(js);
                }
            }
        }
        if let Some(k) = kids {
            cur.push(format!("\"children\": {}", k));
        }
        if !has_spread {
            return format!("{{ {} }}", cur.join(", "));
        }
        if !cur.is_empty() {
            segs.push(format!("{{ {} }}", cur.join(", ")));
        }
        format!("Object.assign({{}}, {})", segs.join(", "))
    }
}
