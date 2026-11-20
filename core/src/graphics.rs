//! Graphics 2D: software rasterizer di atas framebuffer RGBA dalam linear memory.
//! Alpha selalu 255. Fungsi gambar di sini bekerja pada slice biasa supaya bisa diuji natif.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::memory::{alloc_raw, free_raw};

static FB_PTR: AtomicUsize = AtomicUsize::new(0);
static FB_W: AtomicUsize = AtomicUsize::new(0);
static FB_H: AtomicUsize = AtomicUsize::new(0);

/// Menulis satu piksel. Di luar batas diabaikan (clipping).
pub fn put(buf: &mut [u8], w: usize, h: usize, x: i32, y: i32, c: u32) {
    if x < 0 || y < 0 || (x as usize) >= w || (y as usize) >= h {
        return;
    }
    let i = (y as usize * w + x as usize) * 4;
    buf[i] = (c >> 16) as u8;
    buf[i + 1] = (c >> 8) as u8;
    buf[i + 2] = c as u8;
    buf[i + 3] = 255;
}

pub fn clear(buf: &mut [u8], w: usize, h: usize, c: u32) {
    for y in 0..h {
        for x in 0..w {
            put(buf, w, h, x as i32, y as i32, c);
        }
    }
}

pub fn rect(buf: &mut [u8], w: usize, h: usize, x: i32, y: i32, rw: i32, rh: i32, c: u32) {
    for dy in 0..rh.max(0) {
        for dx in 0..rw.max(0) {
            put(buf, w, h, x + dx, y + dy, c);
        }
    }
}

/// Garis Bresenham dari (x0, y0) ke (x1, y1), termasuk kedua ujungnya.
pub fn line(buf: &mut [u8], w: usize, h: usize, x0: i32, y0: i32, x1: i32, y1: i32, c: u32) {
    let (mut x, mut y) = (x0, y0);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        put(buf, w, h, x, y, c);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Lingkaran berisi.
pub fn circle(buf: &mut [u8], w: usize, h: usize, cx: i32, cy: i32, r: i32, c: u32) {
    if r < 0 {
        return;
    }
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                put(buf, w, h, cx + dx, cy + dy, c);
            }
        }
    }
}

fn edge(ax: i64, ay: i64, bx: i64, by: i64, px: i64, py: i64) -> i64 {
    (bx - ax) * (py - ay) - (by - ay) * (px - ax)
}

/// Segitiga berisi dengan warna datar. Tidak peduli urutan titik (searah jarum jam atau tidak).
#[allow(clippy::too_many_arguments)]
pub fn triangle(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    c: u32,
) {
    let (x0, y0, x1, y1, x2, y2) = (x0 as i64, y0 as i64, x1 as i64, y1 as i64, x2 as i64, y2 as i64);
    let area = edge(x0, y0, x1, y1, x2, y2);
    if area == 0 {
        return;
    }
    let min_x = x0.min(x1).min(x2).max(0);
    let max_x = x0.max(x1).max(x2).min(w as i64 - 1);
    let min_y = y0.min(y1).min(y2).max(0);
    let max_y = y0.max(y1).max(y2).min(h as i64 - 1);

    for py in min_y..=max_y {
        for px in min_x..=max_x {
            let w0 = edge(x1, y1, x2, y2, px, py);
            let w1 = edge(x2, y2, x0, y0, px, py);
            let w2 = edge(x0, y0, x1, y1, px, py);
            let inside = if area > 0 {
                w0 >= 0 && w1 >= 0 && w2 >= 0
            } else {
                w0 <= 0 && w1 <= 0 && w2 <= 0
            };
            if inside {
                put(buf, w, h, px as i32, py as i32, c);
            }
        }
    }
}

// ---------- Framebuffer di linear memory ----------

/// Menjalankan `f` dengan framebuffer aktif. Tidak melakukan apa-apa jika belum di-init.
fn with_fb(f: impl FnOnce(&mut [u8], usize, usize)) {
    let ptr = FB_PTR.load(Ordering::Relaxed);
    if ptr == 0 {
        return;
    }
    let w = FB_W.load(Ordering::Relaxed);
    let h = FB_H.load(Ordering::Relaxed);
    let buf = unsafe { std::slice::from_raw_parts_mut(ptr as *mut u8, w * h * 4) };
    f(buf, w, h);
}

#[no_mangle]
pub extern "C" fn fb_fb_init(w: u32, h: u32) -> u32 {
    let (w, h) = (w as usize, h as usize);
    free_raw(FB_PTR.swap(0, Ordering::Relaxed));
    let p = alloc_raw(w.saturating_mul(h).saturating_mul(4));
    FB_PTR.store(p, Ordering::Relaxed);
    FB_W.store(w, Ordering::Relaxed);
    FB_H.store(h, Ordering::Relaxed);
    p as u32
}

#[no_mangle]
pub extern "C" fn fb_fb_ptr() -> u32 {
    FB_PTR.load(Ordering::Relaxed) as u32
}

#[no_mangle]
pub extern "C" fn fb_fb_w() -> u32 {
    FB_W.load(Ordering::Relaxed) as u32
}

#[no_mangle]
pub extern "C" fn fb_fb_h() -> u32 {
    FB_H.load(Ordering::Relaxed) as u32
}

#[no_mangle]
pub extern "C" fn fb_clear(c: u32) {
    with_fb(|b, w, h| clear(b, w, h, c))
}

#[no_mangle]
pub extern "C" fn fb_pixel(x: i32, y: i32, c: u32) {
    with_fb(|b, w, h| put(b, w, h, x, y, c))
}

#[no_mangle]
pub extern "C" fn fb_rect(x: i32, y: i32, rw: i32, rh: i32, c: u32) {
    with_fb(|b, w, h| rect(b, w, h, x, y, rw, rh, c))
}

#[no_mangle]
pub extern "C" fn fb_line(x0: i32, y0: i32, x1: i32, y1: i32, c: u32) {
    with_fb(|b, w, h| line(b, w, h, x0, y0, x1, y1, c))
}

#[no_mangle]
pub extern "C" fn fb_circle(cx: i32, cy: i32, r: i32, c: u32) {
    with_fb(|b, w, h| circle(b, w, h, cx, cy, r, c))
}

#[no_mangle]
pub extern "C" fn fb_triangle(x0: i32, y0: i32, x1: i32, y1: i32, x2: i32, y2: i32, c: u32) {
    with_fb(|b, w, h| triangle(b, w, h, x0, y0, x1, y1, x2, y2, c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(b: &[u8], w: usize, x: usize, y: usize) -> [u8; 4] {
        let i = (y * w + x) * 4;
        [b[i], b[i + 1], b[i + 2], b[i + 3]]
    }

    #[test]
    fn rect_fills_expected_area() {
        let (w, h) = (4, 3);
        let mut b = vec![0u8; w * h * 4];
        rect(&mut b, w, h, 1, 1, 2, 1, 0xFF0000);
        assert_eq!(px(&b, w, 1, 1), [0xFF, 0, 0, 255]);
        assert_eq!(px(&b, w, 2, 1), [0xFF, 0, 0, 255]);
        assert_eq!(px(&b, w, 0, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn put_clips_outside_buffer() {
        let (w, h) = (2, 2);
        let mut b = vec![0u8; w * h * 4];
        put(&mut b, w, h, -1, 0, 0xFFFFFF);
        put(&mut b, w, h, 2, 2, 0xFFFFFF);
        assert!(b.iter().all(|&v| v == 0));
    }

    #[test]
    fn triangle_fills_inside_only() {
        let (w, h) = (4, 4);
        let mut b = vec![0u8; w * h * 4];
        triangle(&mut b, w, h, 0, 0, 3, 0, 0, 3, 0x00FF00);
        assert_eq!(px(&b, w, 1, 1)[1], 0xFF);
        assert_eq!(px(&b, w, 3, 3), [0, 0, 0, 0]);
    }

    #[test]
    fn triangle_accepts_either_winding() {
        let (w, h) = (4, 4);
        let mut a = vec![0u8; w * h * 4];
        let mut c = vec![0u8; w * h * 4];
        triangle(&mut a, w, h, 0, 0, 3, 0, 0, 3, 0xFFFFFF);
        triangle(&mut c, w, h, 0, 0, 0, 3, 3, 0, 0xFFFFFF);
        assert_eq!(a, c);
    }

    #[test]
    fn line_includes_both_endpoints() {
        let (w, h) = (5, 5);
        let mut b = vec![0u8; w * h * 4];
        line(&mut b, w, h, 0, 0, 4, 4, 0xFFFFFF);
        assert_eq!(px(&b, w, 0, 0)[3], 255);
        assert_eq!(px(&b, w, 4, 4)[3], 255);
    }
}
