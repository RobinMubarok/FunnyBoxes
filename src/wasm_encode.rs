//! Penulis binary WebAssembly (cakupan MVP): tipe, fungsi, export, dan kode.
//! Tidak ada dependensi. Urutan section mengikuti spesifikasi: type, function, export, code.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValTy {
    I32,
    I64,
    F32,
    F64,
}

impl ValTy {
    pub fn byte(self) -> u8 {
        match self {
            ValTy::I32 => 0x7f,
            ValTy::I64 => 0x7e,
            ValTy::F32 => 0x7d,
            ValTy::F64 => 0x7c,
        }
    }
}

/// Satu fungsi siap tulis. `code` tidak berisi `end` penutup; penulis menambahkannya.
#[derive(Clone, Debug)]
pub struct WasmFunc {
    pub name: String,
    pub params: Vec<ValTy>,
    pub result: Option<ValTy>,
    /// Lokal tambahan selain parameter.
    pub locals: Vec<ValTy>,
    pub code: Vec<u8>,
}

/// Unsigned LEB128.
pub fn uleb(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
        if v == 0 {
            break;
        }
    }
}

/// Signed LEB128 (dipakai untuk konstanta i32 dan i64).
pub fn sleb(out: &mut Vec<u8>, mut v: i64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        let done = (v == 0 && (b & 0x40) == 0) || (v == -1 && (b & 0x40) != 0);
        out.push(if done { b } else { b | 0x80 });
        if done {
            break;
        }
    }
}

fn section(out: &mut Vec<u8>, id: u8, body: &[u8]) {
    out.push(id);
    uleb(out, body.len() as u64);
    out.extend_from_slice(body);
}

/// Mengelompokkan lokal berurutan yang bertipe sama, sesuai format code section.
fn group_locals(locals: &[ValTy]) -> Vec<(u32, ValTy)> {
    let mut runs: Vec<(u32, ValTy)> = Vec::new();
    for &t in locals {
        if let Some(last) = runs.last_mut() {
            if last.1 == t {
                last.0 += 1;
                continue;
            }
        }
        runs.push((1, t));
    }
    runs
}

/// Menghasilkan modul WASM lengkap. `exports` berisi (nama, indeks fungsi).
pub fn encode(funcs: &[WasmFunc], exports: &[(String, usize)]) -> Vec<u8> {
    let mut out: Vec<u8> = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let n = funcs.len();

    // Type section: satu tipe fungsi per fungsi.
    let mut types = Vec::new();
    uleb(&mut types, n as u64);
    for f in funcs {
        types.push(0x60);
        uleb(&mut types, f.params.len() as u64);
        for p in &f.params {
            types.push(p.byte());
        }
        match f.result {
            Some(r) => {
                uleb(&mut types, 1);
                types.push(r.byte());
            }
            None => uleb(&mut types, 0),
        }
    }
    section(&mut out, 1, &types);

    // Function section: fungsi ke-i memakai tipe ke-i.
    let mut fsec = Vec::new();
    uleb(&mut fsec, n as u64);
    for i in 0..n {
        uleb(&mut fsec, i as u64);
    }
    section(&mut out, 3, &fsec);

    // Export section.
    let mut esec = Vec::new();
    uleb(&mut esec, exports.len() as u64);
    for (name, idx) in exports {
        uleb(&mut esec, name.len() as u64);
        esec.extend_from_slice(name.as_bytes());
        esec.push(0x00); // kind: fungsi
        uleb(&mut esec, *idx as u64);
    }
    section(&mut out, 7, &esec);

    // Code section: setiap body = deklarasi lokal + kode + end.
    let mut csec = Vec::new();
    uleb(&mut csec, n as u64);
    for f in funcs {
        let mut body = Vec::new();
        let runs = group_locals(&f.locals);
        uleb(&mut body, runs.len() as u64);
        for (count, t) in runs {
            uleb(&mut body, count as u64);
            body.push(t.byte());
        }
        body.extend_from_slice(&f.code);
        body.push(0x0b);
        uleb(&mut csec, body.len() as u64);
        csec.extend_from_slice(&body);
    }
    section(&mut out, 10, &csec);

    out
}
