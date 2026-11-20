//! Menyusun keluaran web: index.html, style.css, dan app.js.
//! WebAssembly (core.wasm) belum termasuk di tahap ini.
//!
//! Satu proyek = file entry beserta semua file `.fny` di folder yang sama.
//! Semua file digabung menjadi satu namespace (seperti spesifikasi v0.1),
//! dan entry file selalu diproses terakhir.

use std::path::{Path, PathBuf};

use crate::base64;
use crate::codegen::{self, CodegenOptions};
use crate::error::{Diag, SourceFile, Span};

pub struct Output {
    pub index_html: String,
    pub style_css: String,
    pub app_js: String,
    /// Isi core.wasm bila program memakai library Rust (Memory, Math, Graphics).
    pub core_wasm: Option<Vec<u8>>,
    /// Fungsi skalar yang dikompilasi ke app.wasm.
    pub wasm_funcs: Vec<String>,
}

const STYLE_CSS: &str = "/* Basis gaya dari funny. Gaya per elemen ditulis langsung di JSX lewat style={{...}}. */
*, *::before, *::after { box-sizing: border-box; }
html, body { margin: 0; font-family: system-ui, -apple-system, \"Segoe UI\", sans-serif; }
.fb-window { min-height: 100vh; }
";

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn index_html(title: &str) -> String {
    format!(
        "<!doctype html>
<html lang=\"id\">
<head>
<meta charset=\"utf-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<title>{}</title>
<link rel=\"stylesheet\" href=\"style.css\">
</head>
<body>
<div id=\"root\"></div>
<script src=\"app.js\"></script>
</body>
</html>
",
        escape_html(title)
    )
}

fn title_of(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("FunnyBoxes")
        .to_string()
}

/// Mengumpulkan (path, isi) untuk entry dan saudara-saudaranya (file .fny di folder yang sama).
/// Entry selalu di urutan terakhir.
fn collect_sources(entry: &Path) -> Result<Vec<(String, String)>, String> {
    let dir: PathBuf = match entry.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };

    let read = std::fs::read_dir(&dir)
        .map_err(|e| format!("tidak bisa membaca folder '{}': {}", dir.display(), e))?;

    let mut others: Vec<PathBuf> = Vec::new();
    for entry_res in read {
        let path = entry_res.map_err(|e| e.to_string())?.path();
        let is_fny = path.extension().and_then(|e| e.to_str()) == Some("fny");
        if is_fny && path != entry && path.is_file() {
            others.push(path);
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

/// Menggabungkan semua sumber menjadi satu string. Mengembalikan offset awal tiap bagian.
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

/// Memformat diagnostik dengan path dan baris dari file asalnya, bukan dari gabungan.
fn render_at(parts: &[(String, String)], starts: &[usize], d: &Diag) -> String {
    let idx = starts
        .iter()
        .rposition(|&st| st <= d.span.start)
        .unwrap_or(0);
    let (path, src) = &parts[idx];
    let local = d.span.start.saturating_sub(starts[idx]);
    let local_diag = Diag::new(d.msg.clone(), Span::new(local, local));
    SourceFile::new(path.clone(), src.clone()).render(&local_diag)
}

/// Lokasi core.wasm: argumen --core, lalu variabel lingkungan FUNNY_CORE_WASM.
fn read_core(explicit: Option<&Path>) -> Result<Vec<u8>, String> {
    let from_env = std::env::var_os("FUNNY_CORE_WASM").map(PathBuf::from);
    let path = explicit
        .map(Path::to_path_buf)
        .or(from_env)
        .ok_or_else(|| {
            "program memakai library WASM (Memory/Math/Graphics), tetapi core.wasm belum ditentukan.\n\
             Beri --core <file.wasm> atau set FUNNY_CORE_WASM. Lihat core/README.md untuk cara membangunnya."
                .to_string()
        })?;
    std::fs::read(&path).map_err(|e| format!("tidak bisa membaca core.wasm '{}': {}", path.display(), e))
}

/// Mengompilasi proyek yang berpusat pada file entry.
pub fn build_project(entry: &Path, debug: bool, core_path: Option<&Path>) -> Result<Output, Vec<String>> {
    let parts = collect_sources(entry).map_err(|e| vec![format!("funny: {}", e)])?;
    let (combined, starts) = concat(&parts);

    let program = crate::parse(&combined).map_err(|d| vec![render_at(&parts, &starts, &d)])?;

    let mode = if debug { "DEBUG" } else { "RELEASE" };
    let scalar = crate::wasm_emit::emit_scalar(&program);
    let opts = CodegenOptions {
        defines: vec!["Browser".to_string(), "JS".to_string(), mode.to_string()],
        debug,
        wasm_funcs: scalar.names.clone(),
    };
    let generated = codegen::generate_full(&program, &opts).map_err(|ds| {
        ds.iter()
            .map(|d| render_at(&parts, &starts, d))
            .collect::<Vec<_>>()
    })?;

    let core_wasm = if generated.uses_core {
        Some(read_core(core_path).map_err(|e| vec![format!("funny: {}", e)])?)
    } else {
        None
    };

    // core.wasm ditanam di app.js sebagai base64, jadi tidak perlu server atau berkas terpisah.
    // Satu buffer, satu alokasi: prefiks base64 untuk core.wasm dan app.wasm, lalu kode program.
    let mut buf: Vec<u8> = Vec::with_capacity(generated.js.len() + 64);
    if let Some(bytes) = &core_wasm {
        buf.extend_from_slice(b"const __CORE_B64 = \"");
        base64::encode_into(bytes, &mut buf);
        buf.extend_from_slice(b"\";\n");
    }
    if !scalar.names.is_empty() {
        buf.extend_from_slice(b"const __APP_B64 = \"");
        base64::encode_into(&scalar.bytes, &mut buf);
        buf.extend_from_slice(b"\";\n");
    }
    buf.extend_from_slice(generated.js.as_bytes());
    let app_js = String::from_utf8(buf).expect("app.js selalu UTF-8");

    Ok(Output {
        index_html: index_html(&title_of(entry)),
        style_css: STYLE_CSS.to_string(),
        app_js,
        core_wasm,
        wasm_funcs: scalar.names,
    })
}

/// Menulis keluaran ke folder tujuan. Folder dibuat jika belum ada.
pub fn write(out: &Output, dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("index.html"), &out.index_html)?;
    std::fs::write(dir.join("style.css"), &out.style_css)?;
    std::fs::write(dir.join("app.js"), &out.app_js)?;
    Ok(())
}
