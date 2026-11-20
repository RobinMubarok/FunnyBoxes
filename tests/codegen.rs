use funny::codegen::{generate, CodegenOptions};
use funny::error::Diag;

fn opts() -> CodegenOptions {
    CodegenOptions {
        defines: vec!["Browser".into(), "JS".into(), "RELEASE".into()],
        debug: false,
        wasm_funcs: Vec::new(),
    }
}

fn gen(src: &str) -> Result<String, Vec<Diag>> {
    let program = funny::parse(src).expect("sintaks harus valid untuk test ini");
    generate(&program, &opts())
}

fn ok(src: &str) -> String {
    match gen(src) {
        Ok(js) => js,
        Err(ds) => panic!("seharusnya berhasil: {:?}", ds.iter().map(|d| d.msg.clone()).collect::<Vec<_>>()),
    }
}

fn err(src: &str) -> Vec<String> {
    match gen(src) {
        Ok(_) => panic!("seharusnya gagal:\n{}", src),
        Err(ds) => ds.into_iter().map(|d| d.msg).collect(),
    }
}

#[test]
fn counter_generates_state_object_and_rerender_handler() {
    let js = ok(std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/counter/src/main.fny"),
    )
    .unwrap()
    .as_str());
    assert!(js.contains("const __st_Counter = { \"count\": 0 };"), "{}", js);
    assert!(js.contains("__inc(__st_Counter, \"count\", 1, false)"), "{}", js);
    assert!(js.contains("__st_Counter.count"), "{}", js);
    assert!(js.contains("__C.Container({ \"className\": \"card\" }"), "{}", js);
    assert!(js.contains("__C.Window({ \"title\": \"Halo\" }, [$Counter()])"), "{}", js);
    assert!(js.contains("__mount(() => $Here());"), "{}", js);
}

#[test]
fn runtime_prelude_is_included() {
    let js = ok("play UI; export default fun Here() { return <Container />; }");
    assert!(js.contains("function __h("));
    assert!(js.contains("function __rerender()"));
}

#[test]
fn int_division_truncates() {
    let js = ok(r#"
        export default fun Here() {
            box14 a = 7;
            box14 b = 2;
            return <p>{a / b}</p>;
        }
    "#);
    assert!(js.contains("Math.trunc($a / $b)"), "{}", js);
}

#[test]
fn float_division_stays_plain() {
    let js = ok(r#"
        export default fun Here() {
            box24 a = 7.0;
            return <p>{a / 2.0}</p>;
        }
    "#);
    assert!(js.contains("($a / 2)"), "{}", js);
    assert!(!js.contains("Math.trunc($a"), "{}", js);
}

#[test]
fn struct_literal_and_array_are_generated() {
    let js = ok(r#"
        play UI;
        struct Product { box14 id; char *name; box24 price; };
        Product products[3] = { {1, "Headphones", 799000}, {2, "Keyboard", 1250000} };
        export default fun Here() { return <p>{products.length}</p>; }
    "#);
    assert!(js.contains("function __new_Product()"), "{}", js);
    assert!(js.contains("let $products = __arr([{ \"id\": 1, \"name\": \"Headphones\", \"price\": 799000 }"), "{}", js);
    assert!(js.contains("__arr([{ \"id\": 1"), "{}", js);
}

#[test]
fn typed_function_and_call_are_generated() {
    let js = ok(r#"
        box14 Add(box14 a, box14 b) { return a + b; }
        export default fun Here() { return <p>{Add(1, 2)}</p>; }
    "#);
    assert!(js.contains("function $Add($a, $b)") || js.contains("function $Add($a, $b) {"), "{}", js);
    assert!(js.contains("$Add(1, 2)"), "{}", js);
}

#[test]
fn static_if_selects_browser_branch() {
    let js = ok(r#"
        play UI;
        version (WebAssembly) { box14 target = 1; }
        else version (Browser) { box14 target = 2; }
        export default fun Here() { return <p>{target}</p>; }
    "#);
    assert!(js.contains("let $target = ((2) | 0);"), "{}", js);
    assert!(!js.contains("let $target = 1;"), "{}", js);
}

#[test]
fn debug_block_is_dropped_in_release() {
    let js = ok(r#"
        play Core;
        debug { Print("dbg"); }
        export default fun Here() { return <p>x</p>; }
    "#);
    assert!(!js.contains("dbg"), "{}", js);
}

#[test]
fn error_unknown_name_is_reported() {
    let msgs = err("export default fun Here() { return <p>{nama}</p>; }");
    assert!(msgs.iter().any(|m| m.contains("'nama' tidak dikenal")), "{:?}", msgs);
}

#[test]
fn error_unsupported_module_is_reported() {
    let msgs = err("play Animation; export default fun Here() { return <p>x</p>; }");
    assert!(msgs.iter().any(|m| m.contains("belum didukung")), "{:?}", msgs);
}

#[test]
fn error_missing_entry_point() {
    let msgs = err("box14 x = 1;");
    assert!(msgs.iter().any(|m| m.contains("Entry point tidak ditemukan")), "{:?}", msgs);
}

#[test]
fn error_wrong_arity_is_reported() {
    let msgs = err(r#"
        box14 Add(box14 a, box14 b) { return a + b; }
        export default fun Here() { return <p>{Add(1)}</p>; }
    "#);
    assert!(msgs.iter().any(|m| m.contains("membutuhkan 2 argumen")), "{:?}", msgs);
}

#[test]
fn error_component_not_defined() {
    let msgs = err("play UI; export default fun Here() { return <Kartu />; }");
    assert!(msgs.iter().any(|m| m.contains("Komponen 'Kartu' tidak dikenal")), "{:?}", msgs);
}

#[test]
fn error_state_outside_top_level_of_fun() {
    let msgs = err(r#"
        play UI;
        fun Bad() {
            if (true) { state x = 0; }
            return <p>x</p>;
        }
        export default fun Here() { return <Bad />; }
    "#);
    assert!(msgs.iter().any(|m| m.contains("'state' hanya boleh")), "{:?}", msgs);
}

#[test]
fn memory_builtins_call_wasm_exports() {
    let program = funny::parse(
        r#"
        play Memory;
        export default fun Here() {
            box00 *buf = Alloc(64);
            MemorySet(buf, 0, 64);
            Free(buf);
            return <p>x</p>;
        }
    "#,
    )
    .unwrap();
    let g = funny::codegen::generate_full(&program, &opts()).expect("harus berhasil");
    assert!(g.uses_core, "play Memory harus menandai pemakaian core.wasm");
    assert!(g.js.contains("__core.fb_alloc(64)"), "{}", g.js);
    assert!(g.js.contains("__core.fb_memset("), "{}", g.js);
    assert!(g.js.contains("__core.fb_free("), "{}", g.js);
    assert!(g.js.contains("__start(true, false, () => {"), "{}", g.js);
}

#[test]
fn program_without_low_level_modules_does_not_need_core() {
    let program = funny::parse("play UI; export default fun Here() { return <p>x</p>; }").unwrap();
    let g = funny::codegen::generate_full(&program, &opts()).unwrap();
    assert!(!g.uses_core);
    assert!(g.js.contains("__start(false, false, () => {"), "{}", g.js);
}

#[test]
fn graphics_builtins_go_through_framebuffer() {
    let program = funny::parse(
        r#"
        play Graphics;
        export default fun Here() {
            return <GraphicsCanvas width={100} height={50}>{() => {
                gfxClear(0x20242C);
                gfxBegin(GFX_TRIANGLES);
                gfxColor(0xFF0000);
                gfxVertex(0, 0);
                gfxVertex(10, 0);
                gfxVertex(0, 10);
                gfxEnd();
            }}</GraphicsCanvas>;
        }
    "#,
    )
    .unwrap();
    let g = funny::codegen::generate_full(&program, &opts()).unwrap();
    assert!(g.uses_core);
    assert!(g.js.contains("__gfxBegin(1)"), "{}", g.js);
    assert!(g.js.contains("__gfxColor(16711680)"), "{}", g.js);
    assert!(g.js.contains("__gfxEnd()"), "{}", g.js);
}

#[test]
fn spec_builtin_names_are_available() {
    // Nama-nama ini diambil dari spesifikasi: Memory, Math, dan Graphics.
    let program = funny::parse(
        r#"
        play Memory;
        play Math;
        play Graphics;
        export default fun Here() {
            box00 *m = Alloc(8);
            PokeBox14(m, 42);
            box14 v = PeekBox14(m);
            box24 r = sqrt(16.0) + random() + lerp(0.0, 1.0, 0.5);
            box14 k = randomInt(1, 6) + clamp(v, 0, 10);
            gfxRect(0, 0, 10, 10, 0xFFFFFF);
            gfxCircle(5, 5, 2, 0x00FF00);
            gfxLine(0, 0, 9, 9, 0xFF0000);
            gfxPixel(1, 1, 0x0000FF);
            Free(m);
            return <p>{r}{k}</p>;
        }
    "#,
    )
    .unwrap();
    let g = funny::codegen::generate_full(&program, &opts()).expect("semua nama harus dikenali");
    assert!(g.js.contains("__core.fb_poke_i32(") && g.js.contains("__core.fb_peek_i32("), "{}", g.js);
    assert!(g.js.contains("__core.fb_random()"), "{}", g.js);
    assert!(g.js.contains("__core.fb_lerp("), "{}", g.js);
}

#[test]
fn box_typed_memory_names_map_to_width() {
    let program = funny::parse(
        r#"
        play Core;
        play Memory;
        export default fun Here() {
            box00 *p = Alloc(16);
            PokeBox01(p, 255);
            PokeBox12(p, -2);
            box12 small = Cast(70000, box12);
            box01 byte = Cast(300, box01);
            return <p>{PeekBox01(p)}{PeekBox04(p)}{small}{byte}</p>;
        }
    "#,
    )
    .unwrap();
    let g = funny::codegen::generate_full(&program, &opts()).expect("harus berhasil");
    assert!(g.js.contains("__core.fb_poke_u8("), "{}", g.js);
    assert!(g.js.contains("__core.fb_poke_i16("), "{}", g.js);
    assert!(g.js.contains("__peekBox04("), "{}", g.js);
    assert!(g.js.contains("((Math.trunc(Number(70000)) << 16) >> 16)"), "{}", g.js);
    assert!(g.js.contains("(Math.trunc(Number(300)) & 255)"), "{}", g.js);
}

#[test]
fn narrow_int_types_wrap_in_js() {
    let js = ok(r#"
        export default fun Here() {
            box12 x = 70000;
            box01 b = 300;
            box01 c = 10;
            c = c + 250;
            return <p>{x}{b}{c}</p>;
        }
    "#);
    assert!(js.contains("let $x = (((70000) << 16) >> 16);"), "{}", js);
    assert!(js.contains("let $b = ((300) & 255);"), "{}", js);
    assert!(js.contains("let $c = ((10) & 255);"), "{}", js);
    assert!(js.contains("($c = ((($c + 250)) & 255))"), "{}", js);
}

#[test]
fn let_and_var_follow_js_keywords() {
    let js = ok(r#"
        export default fun Here() {
            let a = 1;
            var b = 2;
            return <p>{a}{b}</p>;
        }
    "#);
    assert!(js.contains("let $a = 1;"), "{}", js);
    assert!(js.contains("var $b = 2;"), "{}", js);
}

#[test]
fn var_is_hoisted_like_js() {
    // Di JS, `var` boleh dipakai sebelum deklarasinya (nilainya undefined), tidak error.
    let js = ok(r#"
        export default fun Here() {
            var a = b;
            var b = 2;
            return <p>{a}</p>;
        }
    "#);
    // Kata kunci `var` tetap ada di setiap deklarasi; hoisting hanya mengizinkan pemakaian lebih awal.
    assert!(js.contains("var $a = $b;"), "{}", js);
    assert!(js.contains("var $b = 2;"), "{}", js);
}

#[test]
fn let_is_not_hoisted() {
    let msgs = err(r#"
        export default fun Here() {
            let a = b;
            let b = 2;
            return <p>{a}</p>;
        }
    "#);
    assert!(msgs.iter().any(|m| m.contains("'b' tidak dikenal")), "{:?}", msgs);
}

#[test]
fn holyc_type_keywords_map_to_box_types() {
    let js = ok(r#"
        export default fun Here() {
            U8 small = 300;
            I16 mid = 70000;
            box28 precise = 1.5;
            return <p>{small}{mid}{precise}</p>;
        }
    "#);
    assert!(js.contains("let $small = ((300) & 255);"), "{}", js);
    assert!(js.contains("let $mid = (((70000) << 16) >> 16);"), "{}", js);
    assert!(js.contains("let $precise = 1.5;"), "{}", js);
}

#[test]
fn wasm_functions_become_js_wrappers() {
    let o = CodegenOptions {
        defines: vec!["RELEASE".into()],
        debug: false,
        wasm_funcs: vec!["add".into(), "big".into()],
    };
    let program = {
        let mut src = String::from("box14 add(box14 a, box14 b) { return a + b; } box18 big(box18 a) { return a; }");
        src.push_str("export default fun Here() { return <p>x</p>; }");
        funny::parse(&src).unwrap()
    };
    let g = funny::codegen::generate_full(&program, &o).unwrap();
    assert!(g.js.contains("function $add($a, $b) { return __app.add($a, $b); }"), "{}", g.js);
    assert!(g.js.contains("function $big($a) { return Number(__app.big(BigInt($a))); }"), "{}", g.js);
    assert!(g.js.contains("__start(false, true, () => {"), "{}", g.js);
}

#[test]
fn try_catch_throw_become_js() {
    let js = ok(r#"
        export default fun Here() {
            try {
                throw "gagal";
            } catch {
                return <p>x</p>;
            }
        }
    "#);
    assert!(js.contains("try {"), "{}", js);
    assert!(js.contains("throw \"gagal\";"), "{}", js);
    assert!(js.contains("} catch (__e) {"), "{}", js);
}

#[test]
fn offset_is_compile_time_field_offset() {
    let js = ok(r#"
        struct P { box14 a; box24 b; box28 c; };
        union U { box14 x; box28 y; };
        export default fun Here() {
            return <p>{offset(P, b)}{offset(P, c)}{offset(U, y)}</p>;
        }
    "#);
    // offset(P, b) = 4 (box14 4 byte), offset(P, c) = 8, offset(U, y) = 0 (union)
    assert!(js.contains("[4, 8, 0]"), "offset harus 4, 8, 0:\n{}", js);
}

#[test]
fn extern_binds_host_global() {
    let js = ok(r#"
        extern fun log(msg);
        export default fun Here() {
            log("halo");
            return <p>x</p>;
        }
    "#);
    assert!(js.contains("const $log = globalThis[\"log\"];"), "{}", js);
}

#[test]
fn asm_outside_wasm_function_is_clear_error() {
    let msgs = err(r#"
        fun Bad() {
            asm { 0x41, 0x2a };
            return <p>x</p>;
        }
        export default fun Here() { return <Bad />; }
    "#);
    assert!(msgs.iter().any(|m| m.contains("asm hanya didukung")), "{:?}", msgs);
}

#[test]
fn goto_to_enclosing_label_becomes_break() {
    let js = ok(r#"
        export default fun Here() {
            done: {
                if (true) { goto done; }
                return <p>tidak</p>;
            }
            return <p>ya</p>;
        }
    "#);
    assert!(js.contains("done: {"), "{}", js);
    assert!(js.contains("break done;"), "{}", js);
}

#[test]
fn goto_outside_enclosing_label_is_error() {
    let msgs = err(r#"
        export default fun Here() {
            goto nope;
            return <p>x</p>;
        }
    "#);
    assert!(msgs.iter().any(|m| m.contains("hanya boleh ke label yang melingkupinya")), "{:?}", msgs);
}

#[test]
fn goto_cannot_cross_function_boundary() {
    let msgs = err(r#"
        export default fun Here() {
            outer: {
                fun inner() { goto outer; return 1; }
                return <p>x</p>;
            }
        }
    "#);
    assert!(msgs.iter().any(|m| m.contains("hanya boleh ke label yang melingkupinya")), "{:?}", msgs);
}
