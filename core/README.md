# funny-core

Library low-level FunnyBoxes: memori, math, dan graphics 2D. Ditulis dengan Rust,
dikompilasi ke WebAssembly. Compiler `funny` menanamkan hasilnya ke `app.js` sebagai base64,
jadi `index.html` bisa langsung dibuka di browser tanpa server.

## Membangun core.wasm

```powershell
rustup target add wasm32-unknown-unknown     # sekali saja
cd core
cargo test                                   # uji logika di mesin sendiri
cargo build --release --target wasm32-unknown-unknown
```

Hasilnya: `core\target\wasm32-unknown-unknown\release\funny_core.wasm`.

## Memakainya

```powershell
cd ..
cargo run -- examples\counter\src\main.fny -o dist --core core\target\wasm32-unknown-unknown\release\funny_core.wasm
```

Atau set sekali lewat variabel lingkungan:

```powershell
$env:FUNNY_CORE_WASM = "D:\funnyboxes-v0.1.0\funny\core\target\wasm32-unknown-unknown\release\funny_core.wasm"
```

Program yang memakai `play Memory;`, `play Math;`, atau `play Graphics;` memerlukan core.
Program lain tidak.

Lalu buka `dist\index.html` langsung di browser.

## Cara WASM dimuat

`app.js` berisi `const __CORE_B64 = "..."`. Saat start, runtime memanggil
`WebAssembly.instantiateStreaming(fetch("data:application/wasm;base64,..."))`.
Browser mendekode base64 secara native dan langsung mengompilasi dari stream, jadi
tidak ada loop decode di JavaScript. Kalau jalur itu gagal, runtime memakai `atob` sebagai fallback.

Ukuran: base64 menambah sekitar 33 persen dari ukuran `.wasm`. Untuk memperkecil `.wasm`,
profil release memakai `opt-level = "s"`, LTO, `panic = "abort"`, dan `strip = true`.
Jika `wasm-opt` (dari Binaryen) tersedia, menjalankannya setelah build bisa memangkas ukuran lagi.

## Isi library (nama sesuai spesifikasi)

| Modul | Nama di .fny | Ekspor WASM |
| :--- | :--- | :--- |
| Memory | `Alloc`, `Realloc`, `Free`, `MemoryCopy`, `MemorySet`, `MemoryCompare` | `fb_alloc`, `fb_realloc`, `fb_free`, `fb_memcopy`, `fb_memset`, `fb_memcmp` |
| Memory | `PeekBox01`/`PokeBox01` (u8), `PeekBox11`/`PokeBox11` (i8), `PeekBox02`/`PokeBox02` (u16), `PeekBox12`/`PokeBox12` (i16), `PeekBox04`/`PokeBox04` (u32), `PeekBox14`/`PokeBox14` (i32), `PeekBox24`/`PokeBox24` (f32), `PeekBox28`/`PokeBox28` (f64) | `fb_peek_*`, `fb_poke_*` |
| Math | `sqrt`, `pow`, `sin`, `cos`, `clamp`, `lerp`, `random`, `randomInt` | `fb_sqrt`, `fb_pow`, `fb_sin`, `fb_cos`, `fb_clamp`, `fb_lerp`, `fb_random`, `fb_random_int` |
| Graphics | `gfxClear`, `gfxPixel`, `gfxLine`, `gfxRect`, `gfxCircle` | `fb_clear`, `fb_pixel`, `fb_line`, `fb_rect`, `fb_circle` |
| Graphics | `gfxBegin`, `gfxColor`, `gfxVertex`, `gfxEnd`, `GFX_TRIANGLES` | `fb_triangle` (dipanggil per segitiga) |
| Graphics | `GraphicsCanvas`, `gfxPresent` | framebuffer disalin ke `<canvas>` |

Catatan:

- `abs`, `min`, `max`, `floor`, `ceil`, dan `round` tetap memakai `Math` JavaScript.
  Itu operasi satu instruksi, jadi memanggil WASM hanya menambah biaya.
- Pointer adalah alamat numerik. Akses data lewat `PeekBoxNM` dan `PokeBoxNM`.
- Pola nama mengikuti tipe box: digit pertama adalah jenis (0 unsigned, 1 signed, 2 float),
  dua digit berikutnya adalah jumlah byte. Jadi `box12` = signed 2 byte, dan `PeekBox12` membaca tipe itu.
- Tipe `box01`, `box11`, `box02`, dan `box12` ditambahkan ke bahasa. Spesifikasi hanya menulis
  tipe 4 dan 8 byte, jadi tipe 1 dan 2 byte perlu ditambahkan ke dokumen spesifikasi.
- `box08`, `box18`, dan `box28` (8 byte) belum punya `PeekBox`/`PokeBox`. Nilai 64-bit tidak muat
  presisi di number JavaScript, jadi perlu `BigInt`. Itu tahap berikutnya.
- Gaya vertex (`gfxBegin`/`gfxVertex`/`gfxEnd`) mengisi segitiga dengan warna vertex pertama (flat shading).
- Belum ada: `Vector2`, `Vector3`, `Matrix2`, `Matrix4`, `Quaternion`, shader, texture, dan buffer.
