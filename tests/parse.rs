use funny::ast::*;
use funny::parse;

fn ok(src: &str) -> Program {
    match parse(src) {
        Ok(p) => p,
        Err(d) => panic!("seharusnya berhasil, tetapi: {} (span {:?})\n{}", d.msg, d.span, src),
    }
}

fn err(src: &str) -> String {
    match parse(src) {
        Ok(_) => panic!("seharusnya gagal:\n{}", src),
        Err(d) => d.msg,
    }
}

/// Mengambil body fungsi bernama `name` dari program.
fn func_body<'p>(p: &'p Program, name: &str) -> &'p [Stmt] {
    for s in &p.body {
        if let StmtKind::Func(f) = &s.kind {
            if f.name == name {
                return &f.body;
            }
        }
    }
    panic!("fungsi '{}' tidak ditemukan", name);
}

/// Mengambil ekspresi dari `return <expr>;` pertama di body.
fn return_expr(body: &[Stmt]) -> &Expr {
    for s in body {
        if let StmtKind::Return(Some(e)) = &s.kind {
            return e;
        }
    }
    panic!("tidak ada return dengan nilai");
}

// ---------- Deklarasi dan tipe ----------

#[test]
fn struct_and_array_initializer() {
    let p = ok(r#"
        struct Product { box14 id; char *name; box24 price; box14 stock; };
        Product products[3] = { {1, "Headphones", 799000, 12}, {2, "Keyboard", 1250000, 8} };
    "#);
    assert_eq!(p.body.len(), 2);
    match &p.body[0].kind {
        StmtKind::Struct(s) => {
            assert_eq!(s.name, "Product");
            assert_eq!(s.fields.len(), 4);
            assert!(matches!(&s.fields[1].ty, TypeExpr::Ptr(_)));
        }
        other => panic!("struct diharapkan, dapat {:?}", other),
    }
    match &p.body[1].kind {
        StmtKind::Var(v) => {
            assert_eq!(v.name, "products");
            assert!(v.dims.is_some());
            assert!(matches!(v.init, Some(Expr { kind: ExprKind::Brace(_), .. })));
        }
        other => panic!("var diharapkan, dapat {:?}", other),
    }
}

#[test]
fn typed_function_with_pointer_param() {
    let p = ok(r#"
        Product *cart[50];
        box14 cartCount = 0;
        box00 AddToCart(Product *product) {
            if (cartCount >= 50)
                return;
            cart[cartCount++] = product;
        }
    "#);
    assert_eq!(p.body.len(), 3);
    match &p.body[2].kind {
        StmtKind::Func(f) => {
            assert!(!f.dynamic);
            assert!(f.ret.is_some());
            assert_eq!(f.params.len(), 1);
            assert!(matches!(f.params[0].ty, Some(TypeExpr::Ptr(_))));
        }
        other => panic!("fungsi diharapkan, dapat {:?}", other),
    }
}

#[test]
fn dynamic_function_is_marked_dynamic() {
    let p = ok("fun Add(a, b) { return a + b; }");
    match &p.body[0].kind {
        StmtKind::Func(f) => {
            assert!(f.dynamic);
            assert!(f.ret.is_none());
            assert!(f.params.iter().all(|p| p.ty.is_none()));
        }
        other => panic!("{:?}", other),
    }
}

#[test]
fn export_default_entry_point() {
    let p = ok("export default fun Here() { return <main>ok</main>; }");
    match &p.body[0].kind {
        StmtKind::Func(f) => {
            assert!(f.exported);
            assert!(f.is_default);
            assert_eq!(f.name, "Here");
        }
        other => panic!("{:?}", other),
    }
}

#[test]
fn export_non_function_is_error() {
    assert!(err("export box14 x = 1;").contains("hanya berlaku untuk fungsi"));
}

#[test]
fn play_forms() {
    let p = ok(r#"
        play UI;
        play Math : sqrt, pow;
        play Graphics as GFX;
        play Graphics.WASM;
    "#);
    let plays: Vec<&PlayDecl> = p
        .body
        .iter()
        .filter_map(|s| match &s.kind {
            StmtKind::Play(d) => Some(d),
            _ => None,
        })
        .collect();
    assert_eq!(plays.len(), 4);
    assert_eq!(plays[0].module, "UI");
    assert_eq!(plays[1].names.as_ref().unwrap(), &vec!["sqrt".to_string(), "pow".to_string()]);
    assert_eq!(plays[2].alias.as_deref(), Some("GFX"));
    assert_eq!(plays[3].module, "Graphics.WASM");
}

#[test]
fn alias_and_static_if_and_debug() {
    let p = ok(r#"
        alias uint = box04;
        version (WebAssembly) { play Graphics.WASM; }
        else version (Browser) { play Graphics.Browser; }
        static if (DEBUG) { box14 x = 1; } else { box14 x = 2; }
        debug { Print("dbg"); }
    "#);
    assert!(matches!(p.body[0].kind, StmtKind::Alias(_)));
    match &p.body[1].kind {
        StmtKind::StaticIf { alt, .. } => assert_eq!(alt.len(), 1, "else version dibungkus StaticIf"),
        other => panic!("{:?}", other),
    }
    assert!(matches!(p.body[2].kind, StmtKind::StaticIf { .. }));
    assert!(matches!(p.body[3].kind, StmtKind::Debug(_)));
}

#[test]
fn state_declaration_inside_function() {
    let p = ok("fun Counter() { state count = 0; return <p>{count}</p>; }");
    let body = func_body(&p, "Counter");
    match &body[0].kind {
        StmtKind::Var(v) => assert_eq!(v.kind, VarKind::State),
        other => panic!("{:?}", other),
    }
}

// ---------- Statement ----------

#[test]
fn control_flow_forms() {
    ok(r#"
        box00 Run() {
            for (box14 i = 0; i < 10; i++) { if (i == 3) continue; if (i == 7) break; }
            box14 k = 0;
            while (k < 2) { k++; }
            do { k++; } while (k < 5);
            switch (k) {
                case 1: Print("satu"); break;
                case 2: Print("dua"); break;
                default: Print("lain");
            }
            if (k > 0) { k = 0; } else if (k < 0) { k = 1; } else k = 2;
        }
    "#);
}

#[test]
fn dynamic_var_and_for_with_var() {
    ok("var count = 10; for (var j = 0; j < 3; j++) { count += j; }");
}

// ---------- Ekspresi ----------

#[test]
fn multiplication_binds_tighter_than_addition() {
    let p = ok("fun f() { return a + b * c; }");
    let e = return_expr(func_body(&p, "f"));
    match &e.kind {
        ExprKind::Binary { op: "+", r, .. } => match &r.kind {
            ExprKind::Binary { op: "*", .. } => {}
            other => panic!("'*' diharapkan di kanan: {:?}", other),
        },
        other => panic!("'+' diharapkan di atas: {:?}", other),
    }
}

#[test]
fn ternary_and_logical_operators() {
    ok("box14 isValid = age >= 18 && active; box14 r = (a + b) * c; fun f() { return x ? 1 : 2; }");
}

#[test]
fn hex_float_and_underscore_literals() {
    let p = ok("box04 flags = 0xFF; box24 price = 12500.50; box24 half = .5; box14 big = 1_000_000;");
    let vals: Vec<String> = p
        .body
        .iter()
        .filter_map(|s| match &s.kind {
            StmtKind::Var(VarDecl { init: Some(e), .. }) => Some(format!("{:?}", e.kind)),
            _ => None,
        })
        .collect();
    assert_eq!(vals[0], "Int(255)");
    assert_eq!(vals[1], "Float(12500.5)");
    assert_eq!(vals[2], "Float(0.5)");
    assert_eq!(vals[3], "Int(1000000)");
}

#[test]
fn member_arrow_and_index_access() {
    ok("box24 t = cart[i]->price; box14 n = list.length; p->x = 1;");
}

#[test]
fn anonymous_function_and_arrows() {
    let p = ok(r#"
        var twice = fun(a) { return a * 2; };
        var inc = x => x + 1;
        var add = (a, b) => a + b;
        var block = () => { gfxClear(0x20242C); };
    "#);
    assert_eq!(p.body.len(), 4);
}

#[test]
fn await_inside_function_sets_async() {
    let p = ok("fun Load() { var response = await Fetch(\"/api/products\"); return response; }");
    match &p.body[0].kind {
        StmtKind::Func(f) => assert!(f.is_async),
        other => panic!("{:?}", other),
    }
}

#[test]
fn object_literal_vs_brace_list() {
    let p = ok("var o = { key: 1, \"name\": 2 }; var l = { 1, 2, 3 };");
    match &p.body[0].kind {
        StmtKind::Var(v) => assert!(matches!(v.init, Some(Expr { kind: ExprKind::ObjectLit(_), .. }))),
        other => panic!("{:?}", other),
    }
    match &p.body[1].kind {
        StmtKind::Var(v) => assert!(matches!(v.init, Some(Expr { kind: ExprKind::Brace(_), .. }))),
        other => panic!("{:?}", other),
    }
}

// ---------- JSX ----------

#[test]
fn jsx_nested_elements_and_expressions() {
    let p = ok(r#"
        fun ProductCard(product) {
            return (
                <article className="product-card">
                    <h2>{product.name}</h2>
                    <p>Rp {product.price}</p>
                    <button onClick={() => AddToCart(&product)}>Add to Cart</button>
                </article>
            );
        }
    "#);
    let e = return_expr(func_body(&p, "ProductCard"));
    match &e.kind {
        ExprKind::Jsx(el) => {
            assert_eq!(el.tag.as_deref(), Some("article"));
            assert_eq!(el.attrs.len(), 1);
            assert_eq!(el.children.len(), 3);
        }
        other => panic!("JSX diharapkan: {:?}", other),
    }
}

#[test]
fn jsx_component_self_closing_and_spread() {
    ok(r#"
        fun Page() {
            return <Window title="ShopBox" {...props}><Counter start={5} /><Footer /></Window>;
        }
    "#);
}

#[test]
fn jsx_fragment() {
    let p = ok("fun F() { return <><p>a</p><p>b</p></>; }");
    match &return_expr(func_body(&p, "F")).kind {
        ExprKind::Jsx(el) => {
            assert!(el.tag.is_none());
            assert_eq!(el.children.len(), 2);
        }
        other => panic!("{:?}", other),
    }
}

#[test]
fn jsx_conditional_and_map() {
    ok(r#"
        fun List(products, cart) {
            return (
                <div className="product-list">
                    {products.map(product => (
                        <ProductCard product={product} />
                    ))}
                    {cart.length == 0
                        ? <p>Your cart is empty.</p>
                        : <CartItems items={cart} />
                    }
                </div>
            );
        }
    "#);
}

#[test]
fn jsx_empty_expression_with_comment_is_allowed() {
    ok("fun F() { return <div>{/* komentar */}<p>x</p></div>; }");
}

#[test]
fn jsx_style_object_and_multiline_text() {
    ok(r##"
        fun Card() {
            return (
                <div style={{
                    background: "#20242C",
                    padding: "20px"
                }}>
                    Halo,
                    dunia
                </div>
            );
        }
    "##);
}

#[test]
fn jsx_apostrophe_in_text_does_not_break_arrow_lookahead() {
    ok(r#"fun F() { return (<p>Don't stop</p>); }"#);
}

#[test]
fn spec_style_element_with_expression_is_rejected() {
    // Bentuk `<style={{...}} />` di spesifikasi lama tidak valid sebagai JSX.
    // Perbaikan: pindahkan ke atribut `style` pada elemen induk.
    // Parser membaca '=' sebagai awal atribut, dan '=' bukan nama atribut yang valid.
    let msg = err(r#"fun F() { return <Window><style={{ color: "red" }} /></Window>; }"#);
    assert!(msg.contains("Nama atribut JSX diharapkan"), "pesan: {}", msg);
}

// ---------- Error sintaks ----------

#[test]
fn error_missing_semicolon_points_at_expectation() {
    assert!(err("box14 x = 1").contains("Diharapkan ';'"));
}

#[test]
fn error_mismatched_closing_tag() {
    assert!(err("fun F() { return <div><p>x</div>; }").contains("tidak cocok"));
}

#[test]
fn error_unterminated_string() {
    assert!(err("char *s = \"halo;\n").contains("String tidak ditutup"));
}

#[test]
fn error_unterminated_block() {
    assert!(err("fun F() { return 1;").contains("sebelum akhir file"));
}

#[test]
fn error_await_outside_function() {
    assert!(err("var x = await Fetch(\"/api\");").contains("'await' hanya boleh di dalam fungsi"));
}

#[test]
fn error_keyword_as_expression() {
    assert!(err("var x = if;").contains("tidak dapat dipakai sebagai ekspresi"));
}

#[test]
fn error_state_is_not_checked_here_but_parses() {
    // Aturan "state hanya di dalam fungsi" diperiksa analyzer (tahap berikutnya).
    ok("state count = 0;");
}

// ---------- Contoh proyek ----------

fn collect_fny(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("folder contoh harus ada") {
        let path = entry.expect("entry valid").path();
        if path.is_dir() {
            collect_fny(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("fny") {
            out.push(path);
        }
    }
}

#[test]
fn example_projects_parse() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut files = Vec::new();
    collect_fny(&root, &mut files);
    assert!(!files.is_empty(), "tidak ada contoh .fny");
    for f in files {
        let src = std::fs::read_to_string(&f).expect("bisa dibaca");
        if let Err(d) = parse(&src) {
            panic!("{}: {}", f.display(), d.msg);
        }
    }
}

#[test]
fn let_and_var_keywords_parse() {
    let p = ok("fun F() { let a = 1; var b = 2; for (let i = 0; i < 2; i++) { } return a; }");
    assert_eq!(p.body.len(), 1);
}

#[test]
fn approved_holyc_and_jsx_keywords_parse() {
    ok(r#"
        union Value { box14 i; box24 f; };
        extern box14 host_add(box14 a, box14 b);
        extern fun log(msg);
        box14 risky(box14 a) {
            try {
                throw a;
            } catch {
                return 0;
            }
        }
        box14 F(box14 n) {
            reg box14 i = 0;
            noreg box14 j = 1;
            lock n = 2;
            no_warn j;
            switch (n) start
                case 1: return offset(Value, f);
                default: return 0;
            end
        }
        export default fun Here() {
            return <input key="a" ref={(el) => el} />;
        }
    "#);
}

#[test]
fn labeled_block_and_goto_parse() {
    ok("fun F() { done: { goto done; } return 1; }");
}

#[test]
fn play_import_forms_parse() {
    ok(r#"
        play Chart from "https://cdn.example/chart.js";
        play { registerables, Chart as C } from "https://cdn.example/chart.js";
        play * as Lib from "https://cdn.example/lib.js";
        play { sqrt as akar } from Math;
        play UI;
    "#);
}
