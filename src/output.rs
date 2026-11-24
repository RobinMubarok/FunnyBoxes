//! Penyusunan keluaran: SATU file HTML berisi CSS, JavaScript, dan WASM tertanam (base64).
//!
//! Aturan:
//! - Input: file entry `.fny`, atau folder proyek (dicari `src/main.fny` lalu `main.fny`).
//! - Semua `.fny` di folder entry digabung menjadi satu program, entry diproses terakhir.
//! - WASM hanya dibuat untuk fungsi skalar yang benar-benar dipakai. Kalau tidak ada, tidak ada WASM.
//! - core.wasm hanya dimuat kalau program memakai Memory, Math, atau Graphics.
//! - Aset (gambar, font) hanya ditanam kalau dirujuk dari kode.

use std::path::{Path, PathBuf};

use crate::base64;
use crate::codegen::{self, CodegenOptions};
use crate::error::{Diag, SourceFile, Span};
use crate::wasm_emit;

pub struct Output {
    /// Isi seluruh file HTML.
    pub html: String,
    /// Apakah core.wasm ikut tertanam.
    pub core_wasm: bool,
    /// Fungsi skalar yang tertanam sebagai WASM (kosong berarti tidak ada app WASM).
    pub wasm_funcs: Vec<String>,
}

const STYLE_CSS: &str = "*,*::before,*::after{box-sizing:border-box}html,body{margin:0;font-family:system-ui,-apple-system,\"Segoe UI\",sans-serif}.fb-window{min-height:100vh}";

/// Menentukan file entry dari argumen: file `.fny`, atau folder proyek.
pub fn resolve_entry(input: &Path) -> Result<PathBuf, String> {
    if input.is_dir() {
        for cand in [input.join("src").join("main.fny"), input.join("main.fny")] {
            if cand.is_file() {
                return Ok(cand);
            }
        }
        return Err(format!(
            "folder '{}' tidak punya src/main.fny atau main.fny",
            input.display()
        ));
    }
    if input.is_file() {
        return Ok(input.to_path_buf());
    }
    Err(format!("'{}' tidak ditemukan", input.display()))
}

/// Entry berada di folder `src` berarti ini proyek: sibling ikut digabung dan aset dicari di induknya.
fn in_src(entry: &Path) -> bool {
    entry
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map_or(false, |n| n == "src")
}

/// Folder aset: induk dari `src` untuk proyek, atau folder entry untuk file tunggal.
fn project_root(entry: &Path) -> PathBuf {
    let dir = entry.parent().unwrap_or(Path::new(""));
    let base = if in_src(entry) {
        dir.parent().unwrap_or(Path::new(""))
    } else {
        dir
    };
    if base.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        base.to_path_buf()
    }
}

/// Membuat proyek baru: `<nama>/src/main.fny` dan folder `<nama>/images`.
pub fn init_project(name: &Path) -> Result<(), String> {
    if name.exists() {
        return Err(format!("'{}' sudah ada", name.display()));
    }
    let src = name.join("src");
    std::fs::create_dir_all(&src).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(name.join("images")).map_err(|e| e.to_string())?;
    std::fs::write(src.join("main.fny"), TEMPLATE_MAIN).map_err(|e| e.to_string())?;
    Ok(())
}

const TEMPLATE_MAIN: &str = r#"play UI;

// Komponen dengan state: perubahan state memperbarui tampilan.
fun Penghitung() {
    state jumlah = 0;
    return (
        <Container className="card">
            <p>Klik: {jumlah}</p>
            <button onClick={() => jumlah++}>Tambah</button>
        </Container>
    );
}

export default fun Here() {
    return (
        <Window title="Halo">
            <Penghitung />
        </Window>
    );
}
"#;

/// Mengumpulkan (path, isi) untuk entry dan saudara-saudaranya. Entry selalu di urutan terakhir.
fn collect_sources(entry: &Path) -> Result<Vec<(String, String)>, String> {
    let dir: PathBuf = match entry.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let read = std::fs::read_dir(&dir)
        .map_err(|e| format!("tidak bisa membaca folder '{}': {}", dir.display(), e))?;

    let mut others: Vec<PathBuf> = Vec::new();
    if in_src(entry) {
        for entry_res in read {
            let path = entry_res.map_err(|e| e.to_string())?.path();
            let is_fny = path.extension().and_then(|e| e.to_str()) == Some("fny");
            if is_fny && path != entry && path.is_file() {
                others.push(path);
            }
        }
    }
    others.sort();
    others.push(entry.to_path_buf());

    let mut parts = Vec::new();
    for p in others {
        let src = std::fs::read_to_string(&p)
            .map_err(|e| format!("tidak bisa membaca '{}': {}", p.display(), e))?;
        parts.push((p.display().to_string(), src));
    }
    Ok(parts)
}

fn concat(parts: &[(String, String)]) -> (String, Vec<usize>) {
    let mut combined = String::new();
    let mut starts = Vec::new();
    for (_, src) in parts {
        starts.push(combined.len());
        combined.push_str(src);
        combined.push('\n');
    }
    (combined, starts)
}

/// Diagnostik dengan path dan baris dari file asalnya, bukan dari gabungan.
fn render_at(parts: &[(String, String)], starts: &[usize], d: &Diag) -> String {
    let idx = starts.iter().rposition(|&st| st <= d.span.start).unwrap_or(0);
    let (path, src) = &parts[idx];
    let local = d.span.start.saturating_sub(starts[idx]);
    let local_diag = Diag::new(d.msg.clone(), Span::new(local, local));
    SourceFile::new(path.clone(), src.clone()).render(&local_diag)
}

/// core.wasm dari --core, lalu variabel lingkungan FUNNY_CORE_WASM.
fn read_core(explicit: Option<&Path>) -> Result<Vec<u8>, String> {
    let from_env = std::env::var_os("FUNNY_CORE_WASM").map(PathBuf::from);
    let path = explicit
        .map(Path::to_path_buf)
        .or(from_env)
        .ok_or_else(|| {
            "program memakai Memory/Math/Graphics, tetapi core.wasm belum ditentukan. \
             Beri --core <file.wasm> atau set FUNNY_CORE_WASM (lihat core/README.md)."
                .to_string()
        })?;
    std::fs::read(&path).map_err(|e| format!("tidak bisa membaca core.wasm '{}': {}", path.display(), e))
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let rel = format!("{}/{}", prefix, e.file_name().to_string_lossy());
        let path = e.path();
        if e.file_type()?.is_dir() {
            walk(&path, &rel, out)?;
        } else {
            out.push((rel, path));
        }
    }
    Ok(())
}

fn mime_of(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "woff2" => Some("font/woff2"),
        "woff" => Some("font/woff"),
        "ttf" => Some("font/ttf"),
        _ => None,
    }
}

/// Menanam aset yang dirujuk dari kode sebagai data URI. Aset yang tidak dirujuk dilewati.
fn embed_assets(mut js: String, entry: &Path) -> Result<String, String> {
    let root = project_root(entry);
    for dir_name in ["images", "public", "assets"] {
        let dir = root.join(dir_name);
        if !dir.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        walk(&dir, dir_name, &mut files).map_err(|e| e.to_string())?;
        for (rel, path) in files {
            let quoted = format!("\"{}\"", rel);
            if !js.contains(&quoted) {
                continue;
            }
            let Some(mime) = mime_of(&path) else { continue };
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            let mut enc: Vec<u8> = format!("\"data:{};base64,", mime).into_bytes();
            base64::encode_into(&bytes, &mut enc);
            enc.push(b'"');
            let data = String::from_utf8(enc).map_err(|e| e.to_string())?;
            js = js.replace(&quoted, &data);
        }
    }
    Ok(js)
}

fn title_of(path: &Path) -> String {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("FunnyBoxes").to_string()
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn html_doc(title: &str, css: &str, js: &str) -> String {
    // Di dalam <script>, "</script" harus diganti agar tidak menutup tag lebih awal.
    let js = js.replace("</script", "<\\/script");
    format!(
        "<!doctype html>\n<html lang=\"id\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{}</title>\n<style>{}</style>\n</head>\n<body>\n<div id=\"root\"></div>\n\
         <script type=\"module\">{}</script>\n</body>\n</html>\n",
        escape_html(title),
        css,
        js
    )
}

/// Mengompilasi proyek atau file entry menjadi satu dokumen HTML.
pub fn build_project(input: &Path, debug: bool, core_path: Option<&Path>) -> Result<Output, Vec<String>> {
    let entry = resolve_entry(input).map_err(|e| vec![format!("funny: {}", e)])?;
    let parts = collect_sources(&entry).map_err(|e| vec![format!("funny: {}", e)])?;
    let (combined, starts) = concat(&parts);

    let program = crate::parse(&combined).map_err(|d| vec![render_at(&parts, &starts, &d)])?;

    // WASM: hanya fungsi skalar yang dipakai.
    let used = wasm_emit::emit_scalar_used(&program);

    let mode = if debug { "DEBUG" } else { "RELEASE" };
    let opts = CodegenOptions {
        defines: vec!["Browser".to_string(), "JS".to_string(), mode.to_string()],
        debug,
        wasm_funcs: used.names.clone(),
    };
    let generated = codegen::generate_full(&program, &opts)
        .map_err(|ds| ds.iter().map(|d| render_at(&parts, &starts, d)).collect::<Vec<_>>())?;

    // Mode HTML statis: tampilan bisa dihitung saat compile, jadi tidak ada JavaScript maupun WASM.
    if let Some(st) = crate::static_html::render(&program) {
        let title = st.title.clone().unwrap_or_else(|| title_of(&entry));
        let mut html = String::with_capacity(st.body.len() + 512);
        html.push_str("<!doctype html>\n<html lang=\"id\">\n<head>\n<meta charset=\"utf-8\">\n");
        html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
        html.push_str(&format!("<title>{}</title>\n", escape_html(&title)));
        if st.css {
            html.push_str(&format!("<style>{}</style>\n", STYLE_CSS));
        }
        html.push_str(&st.head);
        html.push_str("</head>\n<body>\n");
        html.push_str(&st.body);
        html.push_str("\n</body>\n</html>\n");
        let html = embed_assets(html, &entry).map_err(|e| vec![format!("funny: {}", e)])?;
        return Ok(Output {
            html,
            core_wasm: false,
            wasm_funcs: Vec::new(),
        });
    }

    let core = if generated.uses_core {
        Some(read_core(core_path).map_err(|e| vec![format!("funny: {}", e)])?)
    } else {
        None
    };

    // Prefiks WASM tertanam di depan kode program.
    let mut buf: Vec<u8> = Vec::with_capacity(generated.js.len() + 64);
    if let Some(bytes) = &core {
        buf.extend_from_slice(b"const __CORE_B64 = \"");
        base64::encode_into(bytes, &mut buf);
        buf.extend_from_slice(b"\";\n");
    }
    if !used.names.is_empty() {
        buf.extend_from_slice(b"const __APP_B64 = \"");
        base64::encode_into(&used.bytes, &mut buf);
        buf.extend_from_slice(b"\";\n");
    }
    buf.extend_from_slice(generated.js.as_bytes());
    let js = String::from_utf8(buf).map_err(|e| vec![format!("funny: {}", e)])?;

    let js = embed_assets(js, &entry).map_err(|e| vec![format!("funny: {}", e)])?;

    Ok(Output {
        html: html_doc(&title_of(&entry), STYLE_CSS, &js),
        core_wasm: core.is_some(),
        wasm_funcs: used.names,
    })
}

/// Menulis satu file HTML (CSS, JS, dan WASM tertanam).
pub fn write(out: &Output, file: &Path) -> std::io::Result<PathBuf> {
    if let Some(parent) = file.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(file, &out.html)?;
    Ok(file.to_path_buf())
}
