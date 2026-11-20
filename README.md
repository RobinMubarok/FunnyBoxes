# funny 🧃

> 🇬🇧 **English** · 🇮🇩 [Bahasa Indonesia](#-bahasa-indonesia)

---

# 🇬🇧 English

> *Welcome to FunnyBoxes!* A language for building neat, strict, and slightly
> mischievous web apps. You write the code once, and `funny` turns it into
> `index.html`, `style.css`, and `app.js` that open in any browser.

If you've ever wondered why `box14` isn't just a number, don't worry. Everything
is explained here, and if you still get confused, `funny` will politely tell you
where it hurts (with a `^` pointing at the exact spot).

---

## Table of Contents

- [What is funny?](#what-is-funny)
- [Get started in 3 minutes](#get-started-in-3-minutes)
- [Syntax in ten minutes](#syntax-in-ten-minutes)
- [Commands](#commands)
- [One folder, one project](#one-folder-one-project)
- [What doesn't work yet](#what-doesnt-work-yet-honest-ones)
- [When something breaks](#when-something-breaks)

---

## What is funny?

FunnyBoxes has three main traits:

1. **Clear types.** `box14` is a 32-bit integer, `box24` is a decimal. No silent surprises halfway through.
2. **Declarative UI.** You describe what should be shown, not how to draw each pixel.
3. **Simple output.** No secret, heavy runtime. The result is plain HTML, CSS, and JavaScript you can read with your own eyes.

The compiler is written in Rust, so it's fast and avoids unnecessary drama.

---

## Get started in 3 minutes

You need [Rust](https://rustup.rs). Think of it as the entry ticket. Then:

```powershell
cd funny
cargo run -- examples\counter\src\main.fny -o dist
```

Open `dist\index.html` in your browser. If the number goes up when you click the
button, congratulations! You just ran your first FunnyBoxes app. Celebrate with
something sweet.

> 💡 **Tip:** `cargo run --` is the bridge between Cargo and `funny`. Anything after
> `--` is passed straight to `funny`.

---

## Syntax in ten minutes

The smallest program that's still entertaining:

```c
play UI;                          // "I'd like to use the UI components, please."

fun Hello(name) {                 // a plain function, no types, relaxed vibes
    return <h2>Hello, {name}!</h2>;
}

fun Counter() {
    state clicks = 0;             // state: a value that can change and makes the UI update
    return (
        <Container className="card">
            <p>Clicked {clicks} times</p>
            <button onClick={() => clicks++}>Click me</button>
        </Container>
    );
}

export default fun Here() {       // the entry point. Required, exactly one.
    return (
        <Window title="Practice">
            <Hello name="Robin" />
            <Counter />
        </Window>
    );
}
```

Things to remember:

- **`export default fun Here()`** is the front door. Without it, `funny` will sadly ask, "so where do I start?"
- **`play UI;`** enables components like `Container`, `Button`, and `Window`. Other modules (`Math`, `Graphics`, `Input`) also need a `play` first.
- **`state`** is for values that change. Global variables can also be changed from events, and the UI refreshes after the event finishes.
- **Attributes become parameters.** `<Hello name="Robin" />` fills the `name` parameter.
- **Numeric types are serious.** For `box14`, `7 / 2` gives `3`, not `3.5`. Want decimals? Use `box24` or `Cast(...)`.

---

## Commands

| Command | What it does |
| :--- | :--- |
| `funny file.fny -o dist` | Build a website from the project. Default output is `dist/`. |
| `funny build file.fny` | Same as above, the more explicit version. |
| `funny check file.fny` | Check the code without writing anything. Highly recommended before a build. |
| `funny ast file.fny` | Show the code's syntax tree. For those who like to dissect things. |
| `funny --debug ...` | Enable `debug { }` blocks. Handy when hunting bugs. |
| `funny --version` | Know your version, so you can brag to friends. |

---

## One folder, one project

`funny` reads **all `.fny` files in the same folder** as the entry file and combines
them like one big family. So you can split your code like this:

```
src/
├── main.fny       ← export default fun Here() lives here
├── products.fny
└── cart.fny
```

Just run `funny src/main.fny -o dist`. The others come along automatically.

Small note: subfolders aren't read yet. If you put a file in
`src/components/card.fny`, `funny` won't know it exists. That's a feature we're
working on, not an oversight.

---

## What doesn't work yet (honest ones)

We'd rather tell you upfront than leave you confused for hours:

- ⏳ **WebAssembly (`core.wasm`)** isn't available yet. The plan is to compile heavy parts like Graphics and Math to WASM.
- 🚫 **Unsupported modules:** `Storage`, `Router`, `Network`, `Animation`, `Browser`, `Memory`, and `Graphics.WASM`. If you use them, you'll get an "unsupported" message, not a surprise in the browser.
- 🧍 **State is single per component.** One component function = one state. If it's called twice, both share the same value. Think of them as twins who always agree.
- 🔄 **The UI is rebuilt on every change.** Safe and correct, but an input field you're typing in may lose focus. If that bugs you, let us know.
- 🧮 **Type checking is incomplete.** Integer overflow isn't wrapped yet.

---

## When something breaks

Don't panic. `funny` errors always point to the file, line, and column:

```
mysite/src/main.fny:4:13: error: 'name' is not defined. Add 'play <Module>;' ...
   4 |     return <h2>{name}</h2>;
     |             ^
```

Usually the cause is one of these:

- Forgot `play UI;` at the top.
- Forgot `export default fun Here()`.
- Typo in a variable name. `funny` cares a lot about uppercase and lowercase.
- Used a module that isn't supported yet (see the list above).

Still stuck? Run `funny check`, then send us the first error message. If it turns
out to be a compiler bug, we'll happily admit it.

---

## Contributing

Got an idea, a bug, or a complaint? Open an issue or just send a message. Clean
code and honest comments are very welcome.

License: MIT. Use it, modify it, share it. Please don't use it to bake a cake
that makes your `box14` overflow. 🎂


---

# 🇮🇩 Bahasa Indonesia

> sedikit usil. Kamu menulis kode sekali, lalu `funny` menyulapnya jadi
> `index.html`, `style.css`, dan `app.js` yang bisa dibuka di browser mana pun.

Kalau kamu pernah bingung kenapa `box14` itu bukan sekadar angka, tenang. Semuanya
dijelaskan di sini, dan kalau masih bingung, `funny` akan memberitahumu dengan
sopan (lengkap dengan panah `^` yang menunjuk lokasi salahnya).

---

## Daftar Isi

- [Apa sih funny itu?](#apa-sih-funny-itu)
- [Mulai dalam 3 menit](#mulai-dalam-3-menit)
- [Sintaks dalam sepuluh menit](#sintaks-dalam-sepuluh-menit)
- [Perintah-perintah sakti](#perintah-perintah-sakti)
- [Satu folder, satu proyek](#satu-folder-satu-proyek)
- [Yang belum bisa (jujur aja ya)](#yang-belum-bisa-jujur-aja-ya)
- [Kalau ada yang error](#kalau-ada-yang-error)

---

## Apa sih funny itu?

FunnyBoxes punya tiga sifat utama:

1. **Tipe yang jelas.** `box14` itu integer 32-bit, `box24` itu pecahan. Tidak ada kejutan diam-diam di tengah jalan.
2. **UI yang deklaratif.** Kamu tulis apa yang mau ditampilkan, bukan cara menggambarnya satu per satu.
3. **Output yang sederhana.** Bukan runtime rahasia yang tebal. Hasilnya HTML, CSS, dan JavaScript biasa yang bisa kamu baca langsung.

Compiler-nya ditulis dengan Rust, supaya cepat dan tidak banyak drama.

---

## Mulai dalam 3 menit

Kamu butuh [Rust](https://rustup.rs). Anggap saja sebagai tiket masuk. Setelah itu:

```powershell
cd funny
cargo run -- examples\counter\src\main.fny -o dist
```

Buka `dist\index.html` di browser. Kalau angkanya naik waktu tombol diklik, selamat!
Kamu baru saja menjalankan aplikasi FunnyBoxes pertamamu. Rayakan dengan sesuatu yang manis.

> 💡 **Tips:** `cargo run --` itu jembatan antara Cargo dan `funny`. Apa pun yang
> ada setelah `--` akan diteruskan ke `funny`.

---

## Sintaks dalam sepuluh menit

Program paling kecil yang masih bisa menghibur:

```c
play UI;                          // "Saya mau pakai komponen UI, ya."

fun Halo(nama) {                  // fungsi biasa, tanpa tipe, santai saja
    return <h2>Halo, {nama}!</h2>;
}

fun Penghitung() {
    state jumlah = 0;             // state: nilai yang boleh berubah dan bikin UI ikut berubah
    return (
        <Container className="card">
            <p>Sudah diklik {jumlah} kali</p>
            <button onClick={() => jumlah++}>Klik aku</button>
        </Container>
    );
}

export default fun Here() {       // titik masuk aplikasi. Wajib ada, tepat satu.
    return (
        <Window title="Latihan">
            <Halo nama="Robin" />
            <Penghitung />
        </Window>
    );
}
```

Hal-hal yang perlu diingat:

- **`export default fun Here()`** adalah pintu masuk. Tanpa ini, `funny` akan bertanya dengan sedih, "terus aku mulai dari mana?"
- **`play UI;`** mengaktifkan komponen seperti `Container`, `Button`, dan `Window`. Modul lain (`Math`, `Graphics`, `Input`) juga perlu `play` dulu.
- **`state`** untuk nilai yang berubah-ubah. Variabel global juga boleh diubah dari event, dan UI akan ikut diperbarui setelah event selesai.
- **Atribut jadi parameter.** `<Halo nama="Robin" />` mengisi parameter `nama`.
- **Tipe angka itu serius.** Untuk `box14`, `7 / 2` hasilnya `3`, bukan `3.5`. Mau pecahan? Pakai `box24` atau `Cast(...)`.

---

## Perintah-perintah sakti

| Perintah | Gunanya |
| :--- | :--- |
| `funny file.fny -o dist` | Bikin website dari proyek. Default outputnya `dist/`. |
| `funny build file.fny` | Sama seperti di atas, versi yang lebih eksplisit. |
| `funny check file.fny` | Cek kode saja, tanpa menulis apa-apa. Sangat disarankan sebelum build. |
| `funny ast file.fny` | Lihat struktur pohon kode. Untuk yang suka membedah. |
| `funny --debug ...` | Aktifkan blok `debug { }`. Berguna saat cari bug. |
| `funny --version` | Tahu versi, supaya bisa pamer ke teman. |

---

## Satu folder, satu proyek

`funny` membaca **semua file `.fny` yang ada di folder yang sama** dengan file entry,
lalu menggabungkannya seperti satu keluarga besar. Jadi kamu bisa memecah kode
seperti ini:

```
src/
├── main.fny       ← export default fun Here() ada di sini
├── products.fny
└── cart.fny
```

Cukup jalankan `funny src/main.fny -o dist`. Yang lain ikut otomatis.

Catatan kecil: subfolder belum ikut dibaca. Kalau kamu menaruh file di
`src/komponen/kartu.fny`, `funny` tidak akan tahu keberadaannya. Itu fitur yang
sedang kami siapkan, bukan kelalaian.

---

## Yang belum bisa (jujur aja ya)

Kami lebih suka bilang terus terang daripada bikin kamu bingung berjam-jam:

- ⏳ **WebAssembly (`core.wasm`)** belum ada. Rencananya, bagian berat seperti Graphics dan Math akan dikompilasi ke WASM.
- 🚫 **Modul yang belum didukung:** `Storage`, `Router`, `Network`, `Animation`, `Browser`, `Memory`, dan `Graphics.WASM`. Kalau dipakai, kamu akan dapat pesan "belum didukung", bukan kejutan diam-diam di browser.
- 🧍 **State bersifat tunggal per komponen.** Satu fungsi komponen = satu state. Kalau dipanggil dua kali, keduanya berbagi nilai yang sama. Anggap saja mereka kembar yang selalu sepakat.
- 🔄 **UI dibangun ulang setiap ada perubahan.** Aman dan benar, tapi input yang sedang difokus bisa kehilangan fokus. Kalau mengganggu, kabari kami.
- 🧮 **Pemeriksaan tipe belum lengkap.** Overflow integer juga belum dibungkus.

---

## Kalau ada yang error

Jangan panik. Error `funny` selalu menunjuk file, baris, dan kolomnya:

```
mysite/src/main.fny:4:13: error: 'nama' tidak dikenal. Tambahkan 'play <Modul>;' ...
   4 |     return <h2>{nama}</h2>;
     |             ^
```

Biasanya penyebabnya salah satu dari ini:

- Lupa `play UI;` di paling atas.
- Lupa `export default fun Here()`.
- Nama variabel salah ketik. `funny` sangat peduli huruf besar-kecil.
- Memakai modul yang belum didukung (lihat daftar di atas).

Masih buntu? Jalankan `funny check`, lalu kirim pesan error pertamanya. Kalau ternyata
bug di compiler, kami dengan senang hati mengakuinya.

---

## Kontribusi

Ada ide, bug, atau keluhan? Buka issue atau cukup kirim pesan. Kode yang rapi dan
komentar yang jujur sangat dihargai.

Lisensi: MIT. Silakan dipakai, dimodifikasi, dan dibagikan. Mohon jangan dipakai
untuk memanggang kue yang bisa membuat nilai `box14`-mu overflow. 🎂
