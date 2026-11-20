//! Memori: alokator sederhana di atas linear memory WebAssembly.
//!
//! Setiap blok punya header 8 byte yang menyimpan ukuran data. Nilai yang
//! dikembalikan adalah alamat data (setelah header). 0 berarti gagal.

use std::alloc::{alloc, dealloc, Layout};

const HEADER: usize = 8;

fn layout_for(n: usize) -> Option<Layout> {
    Layout::from_size_align(HEADER + n.max(1), 8).ok()
}

pub fn alloc_raw(n: usize) -> usize {
    let layout = match layout_for(n) {
        Some(l) => l,
        None => return 0,
    };
    unsafe {
        let base = alloc(layout);
        if base.is_null() {
            return 0;
        }
        (base as *mut usize).write_unaligned(n);
        base as usize + HEADER
    }
}

fn block_size(ptr: usize) -> usize {
    unsafe { ((ptr - HEADER) as *const usize).read_unaligned() }
}

pub fn free_raw(ptr: usize) {
    if ptr == 0 {
        return;
    }
    let n = block_size(ptr);
    if let Some(layout) = layout_for(n) {
        unsafe { dealloc((ptr - HEADER) as *mut u8, layout) };
    }
}

pub fn realloc_raw(ptr: usize, n: usize) -> usize {
    if ptr == 0 {
        return alloc_raw(n);
    }
    let old = block_size(ptr);
    let fresh = alloc_raw(n);
    if fresh == 0 {
        return 0;
    }
    copy_raw(fresh, ptr, old.min(n));
    free_raw(ptr);
    fresh
}

/// Menyalin n byte. Aman untuk area yang saling tumpang tindih.
pub fn copy_raw(dst: usize, src: usize, n: usize) {
    if n == 0 {
        return;
    }
    unsafe { std::ptr::copy(src as *const u8, dst as *mut u8, n) };
}

pub fn set_raw(dst: usize, value: u8, n: usize) {
    if n == 0 {
        return;
    }
    unsafe { std::ptr::write_bytes(dst as *mut u8, value, n) };
}

/// Membandingkan n byte. Mengembalikan -1, 0, atau 1 seperti memcmp.
pub fn cmp_raw(a: usize, b: usize, n: usize) -> i32 {
    for i in 0..n {
        let (x, y) = unsafe { (*((a + i) as *const u8), *((b + i) as *const u8)) };
        if x != y {
            return if x < y { -1 } else { 1 };
        }
    }
    0
}

// ---------- Ekspor ke JavaScript ----------

#[no_mangle]
pub extern "C" fn fb_alloc(n: u32) -> u32 {
    alloc_raw(n as usize) as u32
}

#[no_mangle]
pub extern "C" fn fb_realloc(p: u32, n: u32) -> u32 {
    realloc_raw(p as usize, n as usize) as u32
}

#[no_mangle]
pub extern "C" fn fb_free(p: u32) {
    free_raw(p as usize)
}

#[no_mangle]
pub extern "C" fn fb_memcopy(dst: u32, src: u32, n: u32) {
    copy_raw(dst as usize, src as usize, n as usize)
}

#[no_mangle]
pub extern "C" fn fb_memset(dst: u32, value: u32, n: u32) {
    set_raw(dst as usize, value as u8, n as usize)
}

#[no_mangle]
pub extern "C" fn fb_memcmp(a: u32, b: u32, n: u32) -> i32 {
    cmp_raw(a as usize, b as usize, n as usize)
}

#[no_mangle]
pub extern "C" fn fb_peek_u8(p: u32) -> u32 {
    unsafe { *(p as usize as *const u8) as u32 }
}

#[no_mangle]
pub extern "C" fn fb_poke_u8(p: u32, v: u32) {
    unsafe { *(p as usize as *mut u8) = v as u8 }
}

#[no_mangle]
pub extern "C" fn fb_peek_i32(p: u32) -> i32 {
    unsafe { (p as usize as *const i32).read_unaligned() }
}

#[no_mangle]
pub extern "C" fn fb_poke_i32(p: u32, v: i32) {
    unsafe { (p as usize as *mut i32).write_unaligned(v) }
}

#[no_mangle]
pub extern "C" fn fb_peek_f32(p: u32) -> f32 {
    unsafe { (p as usize as *const f32).read_unaligned() }
}

#[no_mangle]
pub extern "C" fn fb_poke_f32(p: u32, v: f32) {
    unsafe { (p as usize as *mut f32).write_unaligned(v) }
}

// Akses 1, 2, dan 4 byte; nama fungsi mengikuti tipe box di FunnyBoxes.
#[no_mangle] pub extern "C" fn fb_peek_i8(p: u32) -> i32 { unsafe { *(p as usize as *const i8) as i32 } }
#[no_mangle] pub extern "C" fn fb_poke_i8(p: u32, v: i32) { unsafe { *(p as usize as *mut i8) = v as i8 } }
#[no_mangle] pub extern "C" fn fb_peek_u16(p: u32) -> u32 { unsafe { (p as usize as *const u16).read_unaligned() as u32 } }
#[no_mangle] pub extern "C" fn fb_poke_u16(p: u32, v: u32) { unsafe { (p as usize as *mut u16).write_unaligned(v as u16) } }
#[no_mangle] pub extern "C" fn fb_peek_i16(p: u32) -> i32 { unsafe { (p as usize as *const i16).read_unaligned() as i32 } }
#[no_mangle] pub extern "C" fn fb_poke_i16(p: u32, v: i32) { unsafe { (p as usize as *mut i16).write_unaligned(v as i16) } }
#[no_mangle] pub extern "C" fn fb_peek_u32(p: u32) -> u32 { unsafe { (p as usize as *const u32).read_unaligned() } }
#[no_mangle] pub extern "C" fn fb_poke_u32(p: u32, v: u32) { unsafe { (p as usize as *mut u32).write_unaligned(v) } }
#[no_mangle] pub extern "C" fn fb_peek_f64(p: u32) -> f64 { unsafe { (p as usize as *const f64).read_unaligned() } }
#[no_mangle] pub extern "C" fn fb_poke_f64(p: u32, v: f64) { unsafe { (p as usize as *mut f64).write_unaligned(v) } }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_copy_compare_free() {
        let a = alloc_raw(16);
        let b = alloc_raw(16);
        assert!(a != 0 && b != 0);
        set_raw(a, 7, 16);
        copy_raw(b, a, 16);
        assert_eq!(cmp_raw(a, b, 16), 0);
        set_raw(b, 9, 1);
        assert_eq!(cmp_raw(a, b, 16), -1);
        free_raw(a);
        free_raw(b);
    }

    /// Ekspor WASM memakai pointer u32, yang hanya valid di wasm32. Di host 64-bit,
    /// alamat heap dipotong ke u32 dan menyebabkan access violation, jadi tes ini hanya untuk wasm32.
    #[cfg(target_arch = "wasm32")]
    #[test]
    fn typed_access_roundtrip() {
        let p = alloc_raw(16);
        fb_poke_u8(p as u32, 255);
        assert_eq!(fb_peek_u8(p as u32), 255);
        fb_poke_i8(p as u32, -1);
        assert_eq!(fb_peek_i8(p as u32), -1);
        fb_poke_u16(p as u32, 65535);
        assert_eq!(fb_peek_u16(p as u32), 65535);
        fb_poke_i16(p as u32, -2);
        assert_eq!(fb_peek_i16(p as u32), -2);
        fb_poke_u32(p as u32, 0xFFFF_FFFF);
        assert_eq!(fb_peek_u32(p as u32), 0xFFFF_FFFF);
        fb_poke_f64(p as u32, 2.5);
        assert_eq!(fb_peek_f64(p as u32), 2.5);
        free_raw(p);
    }

    #[test]
    fn realloc_keeps_prefix() {
        let p = alloc_raw(4);
        set_raw(p, 0xAB, 4);
        let q = realloc_raw(p, 64);
        assert!(q != 0);
        assert_eq!(unsafe { *(q as *const u8) }, 0xAB);
        free_raw(q);
    }
}
