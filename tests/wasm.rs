use funny::wasm_emit::emit_module;

fn module(src: &str) -> Vec<u8> {
    let p = funny::parse(src).expect("sintaks valid");
    match emit_module(&p) {
        Ok(b) => b,
        Err(ds) => panic!("gagal: {:?}", ds.iter().map(|d| d.msg.clone()).collect::<Vec<_>>()),
    }
}

fn err(src: &str) -> Vec<String> {
    let p = funny::parse(src).expect("sintaks valid");
    match emit_module(&p) {
        Ok(_) => panic!("seharusnya gagal:\n{}", src),
        Err(ds) => ds.into_iter().map(|d| d.msg).collect(),
    }
}

#[test]
fn add_matches_verified_bytes() {
    // Byte ini sudah diverifikasi di Node: WebAssembly.instantiate menghasilkan add(2,3) = 5.
    let bytes = module("box14 add(box14 a, box14 b) { return a + b; }");
    let expected: Vec<u8> = vec![
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
        0x01, 0x07, 0x01, 0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7f,
        0x03, 0x02, 0x01, 0x00,
        0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x00,
        0x0a, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b,
    ];
    assert_eq!(bytes, expected);
}

#[test]
fn loops_and_locals_compile() {
    let bytes = module(r#"
        box14 sum(box14 n) {
            box14 s = 0;
            for (box14 i = 0; i < n; i++) {
                if (i == 3) continue;
                s += i;
            }
            return s;
        }
    "#);
    assert_eq!(&bytes[0..4], &[0x00, 0x61, 0x73, 0x6d]);
}

#[test]
fn calls_between_scalar_functions() {
    let bytes = module(r#"
        box14 inc(box14 x) { return x + 1; }
        box14 twice(box14 x) { return inc(inc(x)); }
    "#);
    assert!(bytes.windows(2).any(|w| w == [0x10, 0x00]), "harus ada call 0");
}

#[test]
fn holyc_keywords_map_to_wasm_types() {
    let bytes = module("box18 big(box18 a) { return a * 2; } box28 half(box28 a) { return a / 2.0; }");
    // Dua fungsi, masing-masing dengan tipe i64 dan f64.
    assert!(bytes.contains(&0x7e), "i64 harus muncul");
    assert!(bytes.contains(&0x7c), "f64 harus muncul");
}

#[test]
fn non_scalar_functions_are_skipped() {
    let bytes = module(r#"
        struct P { box14 x; };
        box14 f(P *p) { return 0; }
        box14 g(box14 a) { return a; }
    "#);
    assert!(!bytes.is_empty());
}

#[test]
fn error_unknown_identifier_in_wasm() {
    let msgs = err("box14 f(box14 a) { return b; }");
    assert!(msgs.iter().any(|m| m.contains("'b' belum didukung")), "{:?}", msgs);
}

#[test]
fn error_type_mismatch_is_strict() {
    let msgs = err("box14 f(box14 a, box24 b) { return a + b; }");
    assert!(msgs.iter().any(|m| m.contains("tipe tidak cocok")), "{:?}", msgs);
}

#[test]
fn error_call_to_unknown_function() {
    let msgs = err("box14 f(box14 a) { return g(a); }");
    assert!(msgs.iter().any(|m| m.contains("'g' bukan fungsi bertipe skalar")), "{:?}", msgs);
}

#[test]
fn error_break_outside_loop() {
    let msgs = err("box14 f(box14 a) { break; return a; }");
    assert!(msgs.iter().any(|m| m.contains("'break' hanya boleh")), "{:?}", msgs);
}

#[test]
fn emit_scalar_drops_failing_functions_and_their_callers() {
    let p = funny::parse(r#"
        box14 ok(box14 a) { return a + 1; }
        box14 bad(box14 a) { return b; }
        box14 caller(box14 a) { return bad(a); }
    "#).unwrap();
    let m = funny::wasm_emit::emit_scalar(&p);
    assert_eq!(m.names, vec!["ok".to_string()]);
    assert_eq!(&m.bytes[0..4], &[0x00, 0x61, 0x73, 0x6d]);
}

#[test]
fn asm_bytes_are_emitted_into_wasm_body() {
    let bytes = module("box14 answer() { asm { 0x41, 0x2a }; }");
    assert!(bytes.windows(2).any(|w| w == [0x41, 0x2a]), "byte asm harus ada: {:?}", bytes);
}

#[test]
fn unused_scalar_functions_are_not_generated() {
    let p = funny::parse(r#"
        box14 pakai(box14 a) { return a + 1; }
        box14 tidakPakai(box14 a) { return a * 2; }
        export default fun Here() { return <p>{pakai(1)}</p>; }
    "#).unwrap();
    let m = funny::wasm_emit::emit_scalar_used(&p);
    assert_eq!(m.names, vec!["pakai".to_string()]);
}

#[test]
fn no_wasm_when_nothing_is_used() {
    let p = funny::parse(r#"
        box14 tidakPakai(box14 a) { return a * 2; }
        export default fun Here() { return <p>x</p>; }
    "#).unwrap();
    let m = funny::wasm_emit::emit_scalar_used(&p);
    assert!(m.names.is_empty());
}
