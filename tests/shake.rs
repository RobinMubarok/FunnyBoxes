use funny::shake::shake;

#[test]
fn keeps_referenced_and_drops_unused() {
    let p = "function a(){ return 1; }\nfunction b(){ return a(); }\nconst c = 1;\nwindow.x = 2;\n";
    let out = shake(p, "b();");
    assert!(out.contains("function a"));
    assert!(out.contains("function b"));
    assert!(!out.contains("const c"));
    assert!(out.contains("window.x"));
}

#[test]
fn effects_keep_their_dependencies() {
    let p = "let s = 0;\nfunction f(){ return s; }\nwindow.addEventListener(\"x\", () => f());";
    let out = shake(p, "");
    assert!(out.contains("let s"));
    assert!(out.contains("function f"));
}

#[test]
fn comments_before_declarations_stay_with_them() {
    let p = "/* penjelasan */\nfunction keep(){ return 1; }\n/* tidak dipakai */\nfunction drop(){ return 2; }";
    let out = shake(p, "keep();");
    assert!(out.contains("penjelasan"));
    assert!(out.contains("function keep"));
    assert!(!out.contains("function drop"));
}
