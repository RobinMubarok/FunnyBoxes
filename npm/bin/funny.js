#!/usr/bin/env node
"use strict";

/*
 * Pembungkus npm untuk binary Rust `funny`. Urutan pencarian binary:
 *   1. FUNNY_BIN (kalau diset dan file-nya ada)
 *   2. native/<platform>-<arch>/funny[.exe] di dalam paket
 *   3. target/release/funny[.exe] di folder proyek (hasil `cargo build --release`)
 */

const { spawnSync } = require("child_process");
const fs = require("fs");
const path = require("path");

function candidates() {
  const ext = process.platform === "win32" ? ".exe" : "";
  const list = [];
  if (process.env.FUNNY_BIN) list.push(process.env.FUNNY_BIN);
  list.push(path.join(__dirname, "..", "native", `${process.platform}-${process.arch}`, `funny${ext}`));
  list.push(path.join(__dirname, "..", "..", "target", "release", `funny${ext}`));
  return list;
}

function findBinary() {
  const list = candidates();
  for (const c of list) {
    if (fs.existsSync(c)) return c;
  }
  console.error("funny: binary tidak ditemukan. Path yang dicoba:");
  for (const c of list) console.error("  - " + c);
  console.error("Jalankan 'cargo build --release' di folder funny, atau set FUNNY_BIN.");
  process.exit(1);
}

const result = spawnSync(findBinary(), process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(`funny: gagal menjalankan binary: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
