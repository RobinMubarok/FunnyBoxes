/* FunnyBoxes runtime. Disisipkan ke app.js oleh compiler `funny`. Jangan diedit manual. */
"use strict";

const __root = document.getElementById("root");
let __renderFn = null;
let __core = null; /* ekspor WebAssembly dari core.wasm, diisi oleh __start */
let __app = null;  /* ekspor WebAssembly dari fungsi skalar program, diisi oleh __start */

/* Mengompilasi modul WASM yang tertanam sebagai base64. Browser mendekode base64 secara native
 * lewat data: URL. Fallback: atob. Kedua jalur tidak memerlukan server. */
async function __loadWasm(b64) {
  try {
    const { instance } = await WebAssembly.instantiateStreaming(
      fetch("data:application/wasm;base64," + b64),
      {}
    );
    return instance.exports;
  } catch (_) {
    const bin = atob(b64);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    const { instance } = await WebAssembly.instantiate(bytes, {});
    return instance.exports;
  }
}

/* Titik masuk program: muat WASM yang dibutuhkan, lalu jalankan kode pengguna. */
async function __start(needCore, needApp, body) {
  try {
    if (needCore) __core = await __loadWasm(__CORE_B64);
    if (needApp) __app = await __loadWasm(__APP_B64);
  } catch (e) {
    document.body.textContent = "Gagal memuat WebAssembly: " + e.message;
    return;
  }
  body();
}

/* Membangun ulang seluruh UI dari fungsi entry. Dipanggil setelah state atau event berubah. */
function __rerender() {
  if (__renderFn === null) return;
  __root.replaceChildren(__toNode(__renderFn()));
}

function __mount(render) {
  __renderFn = render;
  __rerender();
}

/* Penugasan ke state: menulis lalu memicu render ulang. */
function __set(obj, key, value) {
  obj[key] = value;
  __rerender();
  return value;
}

/* ++ dan -- pada state. */
function __inc(obj, key, delta, prefix) {
  const old = obj[key];
  obj[key] = old + delta;
  __rerender();
  return prefix ? obj[key] : old;
}

/* Array dengan panjang tetap: sisa slot diisi make(). */
function __arr(items, n, make) {
  const a = items.slice();
  while (a.length < n) a.push(make());
  return a;
}

function __toNode(v) {
  const frag = document.createDocumentFragment();
  __appendTo(frag, v);
  return frag;
}

function __appendTo(parent, v) {
  if (v === null || v === undefined || typeof v === "boolean" || typeof v === "function") return;
  if (Array.isArray(v)) {
    for (const x of v) __appendTo(parent, x);
    return;
  }
  if (v instanceof Node) {
    parent.appendChild(v);
    return;
  }
  parent.appendChild(document.createTextNode(String(v)));
}

const __UNITLESS = new Set(["opacity", "zIndex", "flex", "flexGrow", "flexShrink", "fontWeight", "lineHeight", "order", "zoom"]);

function __style(el, style) {
  for (const k of Object.keys(style)) {
    const v = style[k];
    el.style[k] = typeof v === "number" && v !== 0 && !__UNITLESS.has(k) ? v + "px" : String(v);
  }
}

/* Membuat elemen HTML. Handler on* dibungkus agar UI dirender ulang setelah dipanggil. */
function __h(tag, props, kids) {
  const el = document.createElement(tag);
  const refs = [];
  if (props) {
    for (const k of Object.keys(props)) {
      const v = props[k];
      if (v === null || v === undefined || v === false) continue;
      /* props.children dipakai komponen, bukan atribut. key tidak dipakai: UI dibangun ulang penuh. */
      if (k === "children" || k === "key") continue;
      if (k === "ref") {
        if (typeof v === "function") refs.push(v);
        continue;
      }
      if (k === "className") {
        el.setAttribute("class", String(v));
      } else if (k === "style" && typeof v === "object") {
        __style(el, v);
      } else if (/^on[A-Z]/.test(k) && typeof v === "function") {
        el.addEventListener(k.slice(2).toLowerCase(), (e) => {
          v(e);
          __rerender();
        });
      } else if (k === "value") {
        el.value = String(v);
      } else if (k === "checked") {
        el.checked = Boolean(v);
      } else {
        el.setAttribute(k, v === true ? "" : String(v));
      }
    }
  }
  __appendTo(el, kids);
  for (const r of refs) r(el);
  return el;
}

/* Komponen UI bawaan: kelas dasar FunnyBoxes ditambahkan ke className. */
function __comp(tag, base) {
  return (props, kids) => {
    const p = Object.assign({}, props);
    p.className = [base, p.className].filter(Boolean).join(" ");
    return __h(tag, p, kids);
  };
}

const __C = {
  Window: (props, kids) => {
    if (props && props.title !== undefined) document.title = String(props.title);
    return __comp("div", "fb-window")(props, kids);
  },
  Container: __comp("div", "fb-container"),
  Button: __comp("button", "fb-button"),
  Input: __comp("input", "fb-input"),
  Text: __comp("span", "fb-text"),
  Image: __comp("img", "fb-image"),
  List: __comp("ul", "fb-list"),
  Grid: __comp("div", "fb-grid"),
  Scroll: __comp("div", "fb-scroll"),
  Dialog: __comp("dialog", "fb-dialog"),
  Menu: __comp("nav", "fb-menu"),
  GraphicsCanvas: (props, kids) => {
    const c = __comp("canvas", "fb-canvas")(props, []);
    const draw = (kids || []).find((k) => typeof k === "function");
    if (draw) queueMicrotask(() => __gfxRun(c, draw));
    return c;
  },
};

/* Graphics 2D: primitif digambar ke framebuffer di linear memory WASM,
 * lalu framebuffer disalin ke <canvas> setelah fungsi draw selesai. */
let __gfxActive = false;

function __need() {
  if (!__gfxActive) throw new Error("Fungsi Graphics hanya boleh dipanggil di dalam GraphicsCanvas");
}

function __gfxRun(canvas, draw) {
  const w = canvas.width || 300;
  const h = canvas.height || 150;
  __gfxActive = true;
  try {
    __core.fb_fb_init(w, h);
    draw();
    const ptr = __core.fb_fb_ptr();
    const pixels = new Uint8ClampedArray(__core.memory.buffer, ptr, w * h * 4);
    canvas.getContext("2d").putImageData(new ImageData(pixels, w, h), 0, 0);
  } finally {
    __gfxActive = false;
  }
}

/* Gaya vertex sesuai spesifikasi: gfxBegin, gfxColor, gfxVertex, gfxEnd.
 * Setiap segitiga memakai warna vertex pertamanya (flat shading). */
let __gfxPen = 0xFFFFFF;
let __gfxVerts = [];

function __gfxClear(c) { __need(); __core.fb_clear(c); }
function __gfxPixel(x, y, c) { __need(); __core.fb_pixel(x, y, c); }
function __gfxLine(x1, y1, x2, y2, c) { __need(); __core.fb_line(x1, y1, x2, y2, c); }
function __gfxRect(x, y, w, h, c) { __need(); __core.fb_rect(x, y, w, h, c); }
function __gfxCircle(x, y, r, c) { __need(); __core.fb_circle(x, y, r, c); }
function __gfxBegin(mode) { __need(); __gfxVerts = []; }
function __gfxColor(c) { __need(); __gfxPen = c; }
function __gfxVertex(x, y) { __need(); __gfxVerts.push(x, y, __gfxPen); }
function __gfxEnd() {
  __need();
  const v = __gfxVerts; /* stride 3: x, y, warna */
  for (let i = 0; i + 9 <= v.length; i += 9) {
    __core.fb_triangle(v[i], v[i + 1], v[i + 3], v[i + 4], v[i + 6], v[i + 7], v[i + 2]);
  }
  __gfxVerts = [];
}
function __gfxPresent() {}

/* Input browser. */
function __onKeyDown(key, fn) {
  window.addEventListener("keydown", (e) => {
    if (key === null || key === undefined || e.key === key) {
      fn(e);
      __rerender();
    }
  });
}

function __onMouseMove(fn) {
  window.addEventListener("mousemove", (e) => {
    fn(e);
    __rerender();
  });
}

function __onPointerDown(fn) {
  window.addEventListener("pointerdown", (e) => {
    fn(e);
    __rerender();
  });
}

/* Akses memori: u32 dari WASM bertanda 32-bit, jadi dikembalikan ke rentang tak bertanda. */
function __peekBox04(p) { return __core.fb_peek_u32(p) >>> 0; }

/* Core. */
function __assert(cond, msg) {
  if (!cond) throw new Error("Assertion gagal" + (msg === undefined ? "" : ": " + msg));
}

function __panic(msg) {
  throw new Error("PANIC: " + msg);
}

function __typeof(v) {
  return v === null ? "null" : Array.isArray(v) ? "array" : typeof v;
}

__start(false, false, () => {
function $Here() {
  return __h("body", {  }, ["Hello"]);
}
__mount(() => $Here());
});
