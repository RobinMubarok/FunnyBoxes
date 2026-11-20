//! Encoder base64 tanpa dependensi dan tanpa alokasi per byte.
//!
//! Dua trik untuk kecepatan:
//! - Tabel 4096 entri (12 bit -> 2 karakter) dihitung saat compile, jadi setiap 3 byte
//!   hanya butuh dua lookup dan satu penulisan 4 byte.
//! - Hasil ditulis langsung ke buffer tujuan, tidak lewat String perantara.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

const fn build_pairs() -> [[u8; 2]; 4096] {
    let mut t = [[0u8; 2]; 4096];
    let mut i = 0;
    while i < 4096 {
        t[i] = [ALPHABET[i >> 6], ALPHABET[i & 63]];
        i += 1;
    }
    t
}

static PAIRS: [[u8; 2]; 4096] = build_pairs();

/// Menulis base64 dari `input` ke akhir `out`. Ukuran tambahan dihitung sekali di awal.
pub fn encode_into(input: &[u8], out: &mut Vec<u8>) {
    out.reserve(input.len().div_ceil(3) * 4);
    let mut chunks = input.chunks_exact(3);
    for c in &mut chunks {
        let n = ((c[0] as usize) << 16) | ((c[1] as usize) << 8) | (c[2] as usize);
        let hi = PAIRS[n >> 12];
        let lo = PAIRS[n & 4095];
        out.extend_from_slice(&[hi[0], hi[1], lo[0], lo[1]]);
    }
    match chunks.remainder() {
        [a] => {
            let hi = PAIRS[(*a as usize) << 4];
            out.extend_from_slice(&[hi[0], hi[1], b'=', b'=']);
        }
        [a, b] => {
            let n = ((*a as usize) << 10) | ((*b as usize) << 2);
            let hi = PAIRS[n >> 6];
            out.extend_from_slice(&[hi[0], hi[1], ALPHABET[n & 63], b'=']);
        }
        _ => {}
    }
}

/// Versi kenyamanan yang mengembalikan String.
pub fn encode(input: &[u8]) -> String {
    let mut out = Vec::with_capacity(input.len().div_ceil(3) * 4);
    encode_into(input, &mut out);
    String::from_utf8(out).expect("base64 selalu ASCII")
}

#[cfg(test)]
mod tests {
    use super::{encode, encode_into};

    #[test]
    fn known_vectors() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"Man"), "TWFu");
        assert_eq!(encode(b"hello world"), "aGVsbG8gd29ybGQ=");
    }

    #[test]
    fn binary_bytes() {
        assert_eq!(encode(&[0x00, 0x61, 0x73, 0x6d]), "AGFzbQ==");
        assert_eq!(encode(&[0xff, 0xff, 0xff]), "////");
    }

    #[test]
    fn appends_to_existing_buffer() {
        let mut out = b"x=".to_vec();
        encode_into(b"Man", &mut out);
        assert_eq!(out, b"x=TWFu");
    }
}
