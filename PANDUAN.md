# Panduan FunnyBoxes: dari nol sampai aplikasi jalan

Panduan ini untuk Windows dengan PowerShell. Semua langkah bisa dilakukan berurutan.

## Langkah 1: pasang Rust

1. Buka https://rustup.rs dan jalankan `rustup-init.exe`. Pilih opsi default.
2. Rust di Windows butuh linker MSVC. Kalau saat build muncul error `link.exe not found`,
   pasang **Build Tools for Visual Studio 2022** dan centang **Desktop development with C++**.
   Tutup dan buka lagi PowerShell setelah itu.
3. Pasang target WebAssembly (sekali saja):

```powershell
rustup target add wasm32-unknown-unknown
```

Cek hasilnya:

```powershell
cargo --version
rustup target list --installed
```

Di daftar kedua harus ada `wasm32-unknown-unknown`.

## Langkah 2: ekstrak proyek

Ekstrak `funny-rust-tahap1.zip` ke `D:\funnyboxes-v0.1.0\`. Hasilnya folder:

```
D:\funnyboxes-v0.1.0\funny\
├── src\            compiler (Rust)
├── core\           library low-level (Rust -> WASM)
├── examples\       contoh .fny
├── docs\           dokumen desain
└── PANDUAN.md
```

## Langkah 3: uji compiler dan library

```powershell
cd D:\funnyboxes-v0.1.0\funny
cargo test
cd core
cargo test
```

Kalau semua lolos, lanjut. Kalau ada yang gagal, salin pesan `panicked at` atau
baris `error[...]` pertama, lalu kirimkan.

## Langkah 4: bangun core.wasm

```powershell
cd D:\funnyboxes-v0.1.0\funny\core
cargo build --release --target wasm32-unknown-unknown
```

Hasilnya:
`core\target\wasm32-unknown-unknown\release\funny_core.wasm`

Simpan path itu. Langkah 6 memakainya.

## Langkah 5: bangun compiler `funny`

```powershell
cd D:\funnyboxes-v0.1.0\funny
cargo build --release
```

Hasilnya: `target\release\funny.exe`.

## Langkah 6: beri tahu compiler lokasi core.wasm

Sekali saja per sesi PowerShell:

```powershell
$env:FUNNY_CORE_WASM = "D:\funnyboxes-v0.1.0\funny\core\target\wasm32-unknown-unknown\release\funny_core.wasm"
```

Supaya permanen, pakai `setx FUNNY_CORE_WASM "..."` lalu buka ulang PowerShell.
Kalau tidak diset, pakai opsi `--core <path>` setiap kali build.

## Langkah 7: buat proyek sendiri

```powershell
cd D:\funnyboxes-v0.1.0
mkdir mysite\src
```

Buat file `mysite\src\main.fny` dengan isi berikut:

```c
play UI;

// Fungsi bertipe: masuk ke WebAssembly.
box14 tambah(box14 a, box14 b) {
    return a + b;
}

// Komponen dinamis: masuk ke JavaScript, dengan state.
fun Penghitung() {
    state jumlah = 0;

    return (
        <Container className="card">
            <p>Klik: {jumlah}</p>
            <p>3 + 4 = {tambah(3, 4)}</p>
            <button onClick={() => jumlah++}>Tambah</button>
        </Container>
    );
}

export default fun Here() {
    return (
        <Window title="Latihan">
            <Penghitung />
        </Window>
    );
}
```

Entry point wajib satu: `export default fun Here()` tanpa parameter.
Semua file `.fny` di folder `src\` yang sama ikut digabung, jadi bisa dipecah ke beberapa file.

## Langkah 8: build

```powershell
cd D:\funnyboxes-v0.1.0
..\funny\target\release\funny.exe mysite\src\main.fny -o mysite\dist
```

Kalau berhasil, muncul pesan:

```
Berhasil: mysite\dist/ (index.html, style.css, app.js, core.wasm?, N fungsi WASM)
```

Hasilnya di `mysite\dist\`:

- `index.html`: halaman utama.
- `style.css`: gaya dasar.
- `app.js`: kode JavaScript dan WASM tertanam sebagai base64.

Tidak ada `.wasm` terpisah. Ini disengaja, supaya bisa dibuka langsung tanpa server.

Kalau `FUNNY_CORE_WASM` belum diset dan programmu memakai `play Memory`, `play Math`,
atau `play Graphics`, build akan gagal dengan pesan yang menyebut `--core`.

Untuk cek sintaks dan semantik saja, tanpa menulis output:

```powershell
..\funny\target\release\funny.exe check mysite\src\main.fny
```

## Langkah 9: buka hasilnya

Klik dua kali `mysite\dist\index.html`, atau buka lewat browser:

```
file:///D:/funnyboxes-v0.1.0/mysite/dist/index.html
```

Kamu akan melihat kartu dengan tombol. Setelah diklik, angka "Klik" naik. Teks
"3 + 4 = 7" datang dari fungsi WASM `tambah`.

Kalau halamannya kosong atau muncul pesan "Gagal memuat WebAssembly", buka
Developer Tools (F12), lihat tab Console, lalu kirim pesan error-nya. Membuka lewat
server lokal (`python -m http.server` di folder `dist`) bisa dipakai sebagai pembanding.

## Langkah 10: mengubah dan build ulang

Edit `main.fny`, lalu jalankan langkah 8 lagi. Folder `dist` ditimpa setiap build.

## Batasan saat ini

- Fungsi bertipe yang bisa masuk WASM: parameter dan return berupa tipe skalar
  (`box01`, `box02`, `box04`, `box08`, `box11`, `box12`, `box14`, `box18`, `box24`, `box28`),
  dengan variabel lokal, operator, `if`, `while`, `for`, `break`, `continue`, dan
  pemanggilan fungsi skalar lain. Fungsi yang memakai struct, pointer, array, atau
  JavaScript tetap berjalan sebagai JavaScript.
- `U8`, `U16`, `I8`, `I16` masih berupa alias ke `box01`, `box02`, `box11`, `box12`,
  menunggu keputusan apakah tipe 1 dan 2 byte masuk ke spesifikasi.
- Tipe 64-bit (`box08`, `box18`) dari JavaScript memakai `BigInt` otomatis di pembungkus.
- `key` dan `ref` di JSX belum punya padanan di spesifikasi, jadi diperlakukan sebagai atribut biasa.
- Belum diuji di browser sungguhan: pemuatan WASM lewat data URL. Sudah diuji di Node
  dengan modul WASM kecil, dan hasilnya benar.
- Kode Rust belum pernah dikompilasi di sandbox. Error pertama saat `cargo build` atau
  `cargo test` kemungkinan besar kecil, dan perlu diperbaiki bersama.

## Kalau ada masalah

| Gejala | Penyebab umum | Solusi |
| :--- | :--- | :--- |
| `link.exe not found` | Build Tools belum terpasang | Langkah 1.2 |
| `can't find crate` / target tidak ada | Target WASM belum dipasang | `rustup target add wasm32-unknown-unknown` |
| `core.wasm belum ditentukan` | `FUNNY_CORE_WASM` belum diset | Langkah 6 |
| Error `tidak dikenal` | Nama belum dideklarasikan atau `play` belum ada | Periksa `play UI;` dan deklarasi |
| Halaman kosong | Error runtime | Lihat Console di F12 |


## Keyword baru (HolyC dan JSX)

Contoh yang menggabungkan keyword yang sudah diimplementasikan:

```c
play UI;

box14 bagi(box14 a, box14 b) {
    if (b == 0) {
        return 0;
    }
    return a / b;
}

extern fun catatLog(pesan);   // disediakan oleh halaman HTML/JS

export default fun Here() {
    state hasil = 0;

    return (
        <Container className="card">
            <p>Hasil: {hasil}</p>
            <button
                ref={(el) => el}
                onClick={() => {
                    selesai: {
                        if (hasil > 10) { goto selesai; }
                        hasil = bagi(100, 4);
                    }
                }}>
                Hitung
            </button>
        </Container>
    );
}
```

Batasan keyword yang sudah ada:

- `union`: di JS, field berbagi offset 0 secara logis, tapi tidak meniru aliasing byte.
- `try`, `catch`, `throw`: hanya di JS. Fungsi yang memakainya tidak masuk ke WASM.
- `extern`: mengikat nama ke `globalThis`. Pastikan fungsinya ada di halaman sebelum aplikasi dijalankan.
- `asm { ... }`: hanya boleh di fungsi bertipe yang masuk ke WASM. Gunakan byte WASM mentah, dan pastikan tumpukannya benar.
- `goto`: hanya ke label blok yang melingkupi (`nama: { ... }`). Tidak boleh melintasi fungsi.
- `lock`, `reg`, `noreg`, `interrupt`, `no_warn`: diparse, tapi belum punya efek. `lock` belum atomik.
- `key` dan `ref` di JSX: `key` diterima tapi tidak dipakai, dan `ref` menerima elemen setelah dibuat.

## Yang belum selesai

- Struct, pointer, dan akses memori di WASM. Ini milestone berikutnya, dan menjadi syarat untuk `union` di WASM dan `lock` atomik.
- Goto mundur dan goto yang dikompilasi ke WASM.
