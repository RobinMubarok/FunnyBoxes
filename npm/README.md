# funnyboxes (npm)

Pembungkus npm untuk compiler `funny`. Paket ini hanya meneruskan argumen ke binary Rust.

```bash
npx funny src/main.fny -o dist
```

Keluaran di `dist/`: `index.html`, `style.css`, `app.js`. WebAssembly (`core.wasm`) belum termasuk.

## Pengembangan lokal

```bash
cd ../
cargo build --release
set FUNNY_BIN=%CD%\target\release\funny.exe      # PowerShell: $env:FUNNY_BIN="..."
cd npm
node bin/funny.js ../examples/counter/src/main.fny -o ../out
```

Binary native untuk rilis ditaruh di `native/<platform>-<arch>/`, misalnya `native/win32-x64/funny.exe`.
