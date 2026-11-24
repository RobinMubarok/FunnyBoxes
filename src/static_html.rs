//! Mode HTML statis: kalau seluruh tampilan bisa dihitung saat compile, keluarannya
//! HTML (dan CSS bila perlu) saja, tanpa JavaScript dan tanpa WASM.
//!
//! Syarat (kalau salah satu tidak terpenuhi, mode statis ditolak dan dipakai mode JS):
//! - Hanya deklarasi `play`, `struct`, `alias`, `fun`, dan `var/let` dengan nilai konstan.
//! - Entry `export default fun Nama()` tanpa parameter, tubuhnya `return <JSX>;`.
//! - Tidak ada `state`, event (`on*`), `ref`, `{...spread}`, pemanggilan fungsi, ternary,
//!   atau ekspresi lain yang butuh runtime.
//! - Komponen buatan sendiri hanya kalau tubuhnya satu `return <JSX>;`.

use std::collections::HashMap;

use crate::ast::*;

/// Nilai konstan yang bisa dihitung saat compile.
#[derive(Clone)]
enum Val {
    Str(String),
    Num(f64),
    Bool(bool),
    Obj(Vec<(String, Val)>),
    /// HTML yang sudah dirender (dipakai untuk `children`).
    Html(String),
}

pub struct Static {
    pub title: Option<String>,
    /// Elemen untuk <head> (dari komponen Head): link, meta, style.
    pub head: String,
    pub body: String,
    /// Apakah basis CSS (kelas fb-*) perlu disertakan.
    pub css: bool,
}

const UNITLESS: &[&str] = &[
    "opacity", "zIndex", "flex", "flexGrow", "flexShrink", "fontWeight", "lineHeight", "order", "zoom",
];

type Env = Vec<HashMap<String, Val>>;

fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            c => o.push(c),
        }
    }
    o
}

/// camelCase ke kebab-case untuk properti CSS.
fn kebab(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        if c.is_ascii_uppercase() {
            o.push('-');
            o.push(c.to_ascii_lowercase());
        } else {
            o.push(c);
        }
    }
    o
}

/// Angka seperti `String(n)` di JavaScript untuk nilai yang umum.
fn num_text(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

fn is_event(name: &str) -> bool {
    name.len() > 2 && name.starts_with("on") && name.chars().nth(2).map_or(false, |c| c.is_ascii_uppercase())
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn single_return(body: &[Stmt]) -> Option<&Expr> {
    match body {
        [Stmt { kind: StmtKind::Return(Some(e)), .. }] => Some(e),
        _ => None,
    }
}

fn render_attrs(list: &[(String, Option<String>)]) -> String {
    let mut s = String::new();
    for (n, v) in list {
        s.push(' ');
        s.push_str(n);
        if let Some(v) = v {
            s.push_str("=\"");
            s.push_str(v);
            s.push('"');
        }
    }
    s
}

struct Cx<'a> {
    consts: HashMap<String, Val>,
    fns: HashMap<String, &'a FuncDecl>,
    title: Option<String>,
    head: String,
    depth: usize,
}

impl<'a> Cx<'a> {
    fn lookup(&self, name: &str, env: &Env) -> Option<Val> {
        for f in env.iter().rev() {
            if let Some(v) = f.get(name) {
                return Some(v.clone());
            }
        }
        self.consts.get(name).cloned()
    }

    fn value(&self, e: &Expr, env: &Env) -> Option<Val> {
        match &e.kind {
            ExprKind::Str(s) => Some(Val::Str(s.clone())),
            ExprKind::Int(v) => Some(Val::Num(*v as f64)),
            ExprKind::Float(v) => Some(Val::Num(*v)),
            ExprKind::Bool(b) => Some(Val::Bool(*b)),
            ExprKind::Ident(n) => self.lookup(n, env),
            ExprKind::ObjectLit(props) => {
                let mut out = Vec::new();
                for (k, v) in props {
                    out.push((k.clone(), self.value(v, env)?));
                }
                Some(Val::Obj(out))
            }
            _ => None,
        }
    }

    /// Teks untuk nilai yang tampil sebagai isi elemen (sudah di-escape).
    fn text_of(v: &Val) -> Option<String> {
        match v {
            Val::Str(s) => Some(escape(s)),
            Val::Num(n) => Some(num_text(*n)),
            Val::Bool(true) => Some("true".to_string()),
            Val::Bool(false) => Some(String::new()),
            Val::Html(h) => Some(h.clone()),
            Val::Obj(_) => None,
        }
    }

    /// Nilai `style={{...}}` sebagai teks CSS, sama seperti runtime (angka jadi px kecuali unitless).
    fn style_of(v: &Val) -> Option<String> {
        match v {
            Val::Obj(props) => {
                let mut parts = Vec::new();
                for (k, pv) in props {
                    let text = match pv {
                        Val::Num(n) if *n != 0.0 && !UNITLESS.contains(&k.as_str()) => {
                            format!("{}px", num_text(*n))
                        }
                        Val::Num(n) => num_text(*n),
                        Val::Str(s) => s.clone(),
                        _ => return None,
                    };
                    parts.push(format!("{}:{}", kebab(k), text));
                }
                Some(parts.join(";"))
            }
            _ => None,
        }
    }

    fn collect_attrs(&self, el: &JsxElement, env: &Env) -> Option<Vec<(String, Option<String>)>> {
        let mut out = Vec::new();
        for a in &el.attrs {
            let JsxAttr::Named { name, value } = a else {
                return None;
            };
            if name == "key" {
                continue;
            }
            if name == "ref" || is_event(name) || !valid_name(name) {
                return None;
            }
            let Some(e) = value else {
                out.push((name.clone(), None));
                continue;
            };
            let v = self.value(e, env)?;
            match (name.as_str(), &v) {
                ("className", _) => out.push(("class".to_string(), Some(Self::text_of(&v)?))),
                ("style", _) => out.push(("style".to_string(), Some(escape(&Self::style_of(&v)?)))),
                (_, Val::Bool(false)) => {}
                (_, Val::Bool(true)) => out.push((name.clone(), None)),
                _ => out.push((name.clone(), Some(Self::text_of(&v)?))),
            }
        }
        Some(out)
    }

    fn children(&mut self, kids: &[JsxChild], env: &mut Env) -> Option<String> {
        let mut out = String::new();
        for k in kids {
            match k {
                JsxChild::Text(t) => out.push_str(&escape(t)),
                JsxChild::Element(e) => out.push_str(&self.element(e, env)?),
                JsxChild::Expr(e) => {
                    let v = self.value(e, env)?;
                    out.push_str(&Self::text_of(&v)?);
                }
            }
        }
        Some(out)
    }

    fn element(&mut self, el: &JsxElement, env: &mut Env) -> Option<String> {
        self.depth += 1;
        if self.depth > 64 {
            return None;
        }
        let out = self.element_inner(el, env);
        self.depth -= 1;
        out
    }

    fn element_inner(&mut self, el: &JsxElement, env: &mut Env) -> Option<String> {
        let Some(tag) = &el.tag else {
            return self.children(&el.children, env);
        };

        // Elemen HTML biasa.
        if tag.chars().next().map_or(false, |c| c.is_ascii_lowercase()) {
            if !valid_name(tag) {
                return None;
            }
            let attrs = self.collect_attrs(el, env)?;
            let inner = self.children(&el.children, env)?;
            return Some(format!("<{}{}>{}</{}>", tag, render_attrs(&attrs), inner, tag));
        }

        // Komponen buatan sendiri: tubuhnya harus satu `return <JSX>;`.
        if let Some(f) = self.fns.get(tag).copied() {
            if el.attrs.iter().any(|a| matches!(a, JsxAttr::Spread(_))) {
                return None;
            }
            let body = single_return(&f.body)?;
            let mut frame: HashMap<String, Val> = HashMap::new();
            for p in &f.params {
                if p.name == "children" {
                    let html = self.children(&el.children, env)?;
                    frame.insert(p.name.clone(), Val::Html(html));
                    continue;
                }
                let attr = el.attrs.iter().find_map(|a| match a {
                    JsxAttr::Named { name, value } if name == &p.name => Some(value.clone()),
                    _ => None,
                })?;
                let v = match attr {
                    Some(e) => self.value(&e, env)?,
                    None => Val::Bool(true),
                };
                frame.insert(p.name.clone(), v);
            }
            let ExprKind::Jsx(inner) = &body.kind else {
                return None;
            };
            env.push(frame);
            let out = self.element(inner, env);
            env.pop();
            return out;
        }

        // Komponen bawaan UI (sesuai `play UI`).
        let (base, elem): (&str, &str) = match tag.as_str() {
            "Window" => ("fb-window", "div"),
            "Container" => ("fb-container", "div"),
            "Button" => ("fb-button", "button"),
            "Text" => ("fb-text", "span"),
            "Image" => ("fb-image", "img"),
            "List" => ("fb-list", "ul"),
            "Grid" => ("fb-grid", "div"),
            "Scroll" => ("fb-scroll", "div"),
            "Input" => ("fb-input", "input"),
            "Menu" => ("fb-menu", "nav"),
            "Dialog" => ("fb-dialog", "dialog"),
            "Head" => {
                let html = self.children(&el.children, env)?;
                if html.contains("<script") {
                    return None;
                }
                self.head.push_str(&html);
                return Some(String::new());
            }
            _ => return None,
        };

        if tag == "Window" {
            for a in &el.attrs {
                if let JsxAttr::Named { name, value: Some(e) } = a {
                    if name == "title" {
                        if let Some(Val::Str(s)) = self.value(e, env) {
                            self.title = Some(s);
                        }
                    }
                }
            }
        }

        let mut attrs = self.collect_attrs(el, env)?;
        // Kelas dasar komponen digabung di depan className yang diberikan.
        match attrs.iter().position(|(n, _)| n == "class") {
            Some(i) => {
                let extra = attrs[i].1.take().unwrap_or_default();
                attrs[i].1 = Some(format!("{} {}", base, extra).trim().to_string());
            }
            None => attrs.insert(0, ("class".to_string(), Some(base.to_string()))),
        }

        if elem == "img" || elem == "input" {
            return Some(format!("<{}{}>", elem, render_attrs(&attrs)));
        }
        let inner = self.children(&el.children, env)?;
        Some(format!("<{}{}>{}</{}>", elem, render_attrs(&attrs), inner, elem))
    }
}

/// Mencoba mode HTML statis. `None` berarti program butuh JavaScript.
pub fn render(program: &Program) -> Option<Static> {
    let mut cx = Cx {
        consts: HashMap::new(),
        fns: HashMap::new(),
        title: None,
        head: String::new(),
        depth: 0,
    };

    let mut entry: Option<&FuncDecl> = None;
    for s in &program.body {
        match &s.kind {
            StmtKind::Play(_) | StmtKind::Struct(_) | StmtKind::Alias(_) => {}
            StmtKind::Func(f) => {
                if f.is_default {
                    if entry.is_some() {
                        return None;
                    }
                    entry = Some(f);
                }
                cx.fns.insert(f.name.clone(), f);
            }
            StmtKind::Var(v) if v.kind != VarKind::State => {
                if v.dims.is_some() {
                    return None;
                }
                let init = v.init.as_ref()?;
                let val = cx.value(init, &Vec::new())?;
                cx.consts.insert(v.name.clone(), val);
            }
            _ => return None,
        }
    }

    let f = entry?;
    if !f.dynamic || !f.params.is_empty() {
        return None;
    }
    let e = single_return(&f.body)?;
    let ExprKind::Jsx(el) = &e.kind else {
        return None;
    };
    let mut env: Env = Vec::new();
    let body = cx.element(el, &mut env)?;
    let css = body.contains("fb-");
    Some(Static {
        title: cx.title,
        head: cx.head,
        body,
        css,
    })
}
