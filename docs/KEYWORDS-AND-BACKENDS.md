# Keyword FunnyBoxes: status implementasi

## 1. Aturan tipe boxNM (dari pemilik proyek)

`boxNM`: N = jenis (0 unsigned, 1 signed, 2 float), M = lebar dalam satuan 8 bit.
Jadi jumlah bit = M * 8. Contoh: `box14` = signed 32 bit, `box08` = unsigned 64 bit,
`box01` = unsigned 8 bit, `box12` = signed 16 bit, `box24` = float 32 bit.

Konsekuensi: `U8`, `U16`, `I8`, `I16` berpadanan dengan `box01`, `box02`, `box11`, `box12`.
Keempatnya sekarang sah, bukan karangan.

## 2. Keyword HolyC yang disetujui

| Keyword | Di JS | Di WASM | Status |
| :--- | :--- | :--- | :--- |
| `union` | struct dengan field; JS tidak meniru aliasing byte | belum (tidak dikompilasi ke WASM) | Parser dan JS selesai; layout WASM menyusul |
| `try` / `catch` / `throw` | `try` / `catch` / `throw` | tidak didukung (fungsi jatuh ke JS) | Selesai di JS |
| `extern` | `const $nama = globalThis["nama"]` | tidak didukung | Selesai di JS |
| `offset(Tipe, field)` | konstanta byte, dihitung compile-time | belum | Selesai di JS |
| `asm { ... }` | error kecuali di fungsi WASM | byte mentah masuk body | Selesai di WASM |
| `reg`, `noreg` | diabaikan (hint) | diabaikan | Selesai |
| `interrupt` | diabaikan (penanda) | diabaikan | Selesai sebagai modifier |
| `lock` | diabaikan (belum ada atomik di JS) | belum atomik | Parser selesai; semantik atomik menyusul |
| `no_warn` | tidak menghasilkan kode | tidak menghasilkan kode | Selesai |
| `start` / `end` | badan `switch` seperti `{` `}` | sama (lewat codegen) | Selesai |
| `goto` | `goto nama;` menjadi `break nama;`, hanya ke label yang melingkupi | ditolak (fungsi jatuh ke JS) | Selesai, dengan batasan bagian 4 |

## 3. Keyword JSX yang disetujui

| Keyword | Perilaku | Status |
| :--- | :--- | :--- |
| `key` | diterima, tidak dipakai (UI dibangun ulang penuh) | Selesai |
| `ref` | callback yang menerima elemen setelah dibuat | Selesai |

## 4. Goto: bentuk yang didukung

Sintaks:

```c
selesai: {
    if (gagal) { goto selesai; }
    Proses();
}
```

- `nama: { ... }` adalah blok berlabel. `goto nama;` hanya valid di dalam blok itu.
- Di JS menjadi `nama: { ... }` dan `break nama;`.
- Tidak boleh melintasi batas fungsi. Goto ke label di luar fungsi ditolak.
- Goto ke label yang tidak melingkupi ditolak dengan pesan yang jelas.
- Tidak ada lompat mundur ke label yang sudah lewat, karena bentuk itu tidak punya padanan di JS maupun di `br` WASM.

## 5. Yang masih menunggu

- Layout `union` di WASM. Ini bergantung pada struct di WASM, yang belum ada. Emitter saat ini hanya menangani tipe skalar, tanpa struct, pointer, atau memori.
- Semantik `lock` sebagai operasi atomik. Ini butuh shared memory dan `i32.atomic.*`, dan juga bergantung pada akses memori di WASM.
- Goto di WASM. Blok berlabel belum dikompilasi ke WASM, jadi fungsi yang memakainya tetap berjalan sebagai JS.
