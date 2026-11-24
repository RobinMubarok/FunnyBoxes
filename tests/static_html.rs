use funny::static_html::render;

fn stat(src: &str) -> Option<funny::static_html::Static> {
    render(&funny::parse(src).expect("sintaks valid"))
}

#[test]
fn plain_jsx_becomes_static_html() {
    let st = stat("export default fun Here() { return <p>Halo</p>; }").expect("harus statis");
    assert_eq!(st.body, "<p>Halo</p>");
    assert!(!st.css);
}

#[test]
fn ui_components_and_style_objects_are_static() {
    let st = stat(r##"
        play UI;
        var gaya = { color: "#780002", fontSize: 18 };
        export default fun Here() {
            return <Window title="Halo"><Container className="card"><p style={gaya}>x</p></Container></Window>;
        }
    "##)
    .expect("harus statis");
    assert_eq!(st.title.as_deref(), Some("Halo"));
    assert!(st.body.contains("<p style=\"color:#780002;font-size:18px\">x</p>"), "{}", st.body);
    assert!(st.body.contains("class=\"fb-container card\""), "{}", st.body);
    assert!(st.css);
}

#[test]
fn user_component_with_only_return_is_inlined() {
    let st = stat(r##"
        fun Kartu(judul) { return <h2>{judul}</h2>; }
        export default fun Here() { return <main><Kartu judul="Selamat" /></main>; }
    "##)
    .expect("harus statis");
    assert_eq!(st.body, "<main><h2>Selamat</h2></main>");
}

#[test]
fn state_needs_javascript() {
    assert!(stat(r##"
        export default fun Here() {
            state n = 0;
            return <p>{n}</p>;
        }
    "##).is_none());
}

#[test]
fn event_handlers_need_javascript() {
    assert!(stat(r##"
        export default fun Here() { return <button onClick={() => 1}>x</button>; }
    "##).is_none());
}

#[test]
fn function_calls_need_javascript() {
    assert!(stat(r##"
        box14 tambah(box14 a, box14 b) { return a + b; }
        export default fun Here() { return <p>{tambah(1, 2)}</p>; }
    "##).is_none());
}

#[test]
fn text_is_escaped() {
    let st = stat(r##"export default fun Here() { return <p>{"<b>&"}</p>; }"##).expect("harus statis");
    assert_eq!(st.body, "<p>&lt;b&gt;&amp;</p>");
}
