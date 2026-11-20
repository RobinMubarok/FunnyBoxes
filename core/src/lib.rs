//! funny-core: library low-level FunnyBoxes, dikompilasi ke WebAssembly.
//!
//! Semua fungsi publik yang diekspor ke JavaScript memakai awalan `fb_`.
//! Pointer adalah alamat (u32) di dalam linear memory WebAssembly.
//! Warna dinyatakan sebagai 0xRRGGBB.

pub mod graphics;
pub mod math;
pub mod memory;
