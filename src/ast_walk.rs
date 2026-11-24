//! Penelusuran AST: mengumpulkan semua nama yang dirujuk. Dipakai untuk menentukan
//! fungsi WASM mana yang benar-benar dipanggil, sehingga yang tidak dipakai tidak dibuat.

use crate::ast::*;

pub fn stmts_refs(list: &[Stmt], out: &mut Vec<String>) {
    for s in list {
        stmt_refs(s, out);
    }
}

pub fn stmt_refs(s: &Stmt, out: &mut Vec<String>) {
    match &s.kind {
        StmtKind::Block(l) | StmtKind::Debug(l) => stmts_refs(l, out),
        StmtKind::Expr(e) | StmtKind::Throw(e) => expr_refs(e, out),
        StmtKind::Var(v) => var_refs(v, out),
        StmtKind::Func(f) => stmts_refs(&f.body, out),
        StmtKind::Struct(sd) => {
            for f in &sd.fields {
                if let Some(d) = &f.dims {
                    expr_refs(d, out);
                }
            }
        }
        StmtKind::Alias(_)
        | StmtKind::Play(_)
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::Asm(_)
        | StmtKind::Goto(_)
        | StmtKind::Extern(_) => {}
        StmtKind::If { test, cons, alt } => {
            expr_refs(test, out);
            stmt_refs(cons, out);
            if let Some(a) = alt {
                stmt_refs(a, out);
            }
        }
        StmtKind::While { test, body } | StmtKind::DoWhile { body, test } => {
            expr_refs(test, out);
            stmt_refs(body, out);
        }
        StmtKind::For { init, test, update, body } => {
            match init {
                Some(ForInit::Decls(ds)) => {
                    for d in ds {
                        var_refs(d, out);
                    }
                }
                Some(ForInit::Expr(e)) => expr_refs(e, out),
                None => {}
            }
            if let Some(t) = test {
                expr_refs(t, out);
            }
            if let Some(u) = update {
                expr_refs(u, out);
            }
            stmt_refs(body, out);
        }
        StmtKind::Switch { disc, cases } => {
            expr_refs(disc, out);
            for c in cases {
                if let Some(t) = &c.test {
                    expr_refs(t, out);
                }
                stmts_refs(&c.body, out);
            }
        }
        StmtKind::Return(a) => {
            if let Some(e) = a {
                expr_refs(e, out);
            }
        }
        StmtKind::StaticIf { cond, then, alt } => {
            expr_refs(cond, out);
            stmts_refs(then, out);
            stmts_refs(alt, out);
        }
        StmtKind::Try { body, catch_body } => {
            stmts_refs(body, out);
            if let Some(c) = catch_body {
                stmts_refs(c, out);
            }
        }
        StmtKind::Labeled { body, .. } => stmts_refs(body, out),
    }
}

fn var_refs(v: &VarDecl, out: &mut Vec<String>) {
    if let Some(d) = &v.dims {
        expr_refs(d, out);
    }
    if let Some(i) = &v.init {
        expr_refs(i, out);
    }
}

pub fn expr_refs(e: &Expr, out: &mut Vec<String>) {
    match &e.kind {
        ExprKind::Ident(n) => out.push(n.clone()),
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Str(_) | ExprKind::Bool(_) | ExprKind::Null => {}
        ExprKind::Binary { l, r, .. } => {
            expr_refs(l, out);
            expr_refs(r, out);
        }
        ExprKind::Unary { arg, .. } | ExprKind::Update { arg, .. } | ExprKind::Await(arg) => {
            expr_refs(arg, out)
        }
        ExprKind::Assign { target, value, .. } => {
            expr_refs(target, out);
            expr_refs(value, out);
        }
        ExprKind::Cond { test, cons, alt } => {
            expr_refs(test, out);
            expr_refs(cons, out);
            expr_refs(alt, out);
        }
        ExprKind::Call { callee, args } => {
            expr_refs(callee, out);
            for a in args {
                expr_refs(a, out);
            }
        }
        ExprKind::Member { obj, .. } => expr_refs(obj, out),
        ExprKind::Index { obj, index } => {
            expr_refs(obj, out);
            expr_refs(index, out);
        }
        ExprKind::Arrow { body, .. } => match body {
            ArrowBody::Expr(x) => expr_refs(x, out),
            ArrowBody::Block(l) => stmts_refs(l, out),
        },
        ExprKind::Brace(elems) => {
            for x in elems {
                expr_refs(x, out);
            }
        }
        ExprKind::ObjectLit(props) => {
            for (_, v) in props {
                expr_refs(v, out);
            }
        }
        ExprKind::Offset { ty, .. } => out.push(ty.clone()),
        ExprKind::Jsx(el) => jsx_refs(el, out),
    }
}

fn jsx_refs(el: &JsxElement, out: &mut Vec<String>) {
    if let Some(t) = &el.tag {
        if t.chars().next().map_or(false, |c| c.is_ascii_uppercase()) {
            out.push(t.clone());
        }
    }
    for a in &el.attrs {
        match a {
            JsxAttr::Named { value, .. } => {
                if let Some(v) = value {
                    expr_refs(v, out);
                }
            }
            JsxAttr::Spread(e) => expr_refs(e, out),
        }
    }
    for c in &el.children {
        match c {
            JsxChild::Text(_) => {}
            JsxChild::Expr(e) => expr_refs(e, out),
            JsxChild::Element(x) => jsx_refs(x, out),
        }
    }
}
