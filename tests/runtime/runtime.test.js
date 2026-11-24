'use strict';
/* Tes runtime FunnyBoxes di Node. Routing memakai API JavaScript langsung (location, history).
 * Jalankan dari folder funny:  node --test tests/runtime/runtime.test.js */

const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('vm');
const fs = require('fs');
const path = require('path');
const { createDocument, FakeNode } = require('./fake-dom');

const PRELUDE = fs.readFileSync(path.join(__dirname, '..', '..', 'src', 'runtime', 'prelude.js'), 'utf8');

/** Menjalankan prelude di lingkungan palsu, lalu kode tambahan. */
function boot(code, { protocol = 'http:', pathname = '/', hash = '' } = {}) {
  const { document, root } = createDocument();
  const location = { protocol, pathname, hash };
  const listeners = {};
  const storage = () => ({ getItem() { return null; }, setItem() {}, removeItem() {}, clear() {} });
  const windowObj = {
    addEventListener: (t, fn) => { (listeners[t] ||= []).push(fn); },
    localStorage: storage(),
    sessionStorage: storage(),
  };
  const history = { pushState(_s, _t, url) { location.pathname = url; } };
  const sandbox = {
    document, window: windowObj, location, history,
    Node: FakeNode, console, queueMicrotask, structuredClone,
    ImageData: class {}, Uint8ClampedArray,
  };
  vm.createContext(sandbox);
  vm.runInContext(PRELUDE + '\n' + code, sandbox, { filename: 'app.js' });
  return { sandbox, root, document, location, listeners };
}

test('href "/..." di file:// ditulis ulang menjadi hash', () => {
  const { root } = boot(`__mount(() => __h("a", { href: "/kontak" }, ["k"]));`, { protocol: 'file:' });
  assert.equal(root.childNodes[0].getAttribute('href'), '#/kontak');
});

test('href di server dibiarkan apa adanya', () => {
  const { root } = boot(`__mount(() => __h("a", { href: "/kontak" }, ["k"]));`, { protocol: 'http:' });
  assert.equal(root.childNodes[0].getAttribute('href'), '/kontak');
});

test('kode pengguna membaca location.hash langsung di file://', () => {
  const { root } = boot(`
    __mount(() => location.hash === "#/tentang-kami"
      ? __h("p", null, ["tentang"])
      : __h("p", null, ["beranda"]));
  `, { protocol: 'file:', hash: '#/tentang-kami' });
  assert.equal(root.textContent, 'tentang');
});

test('kode pengguna membaca location.pathname di server', () => {
  const { root } = boot(`
    __mount(() => location.pathname === "/kontak"
      ? __h("p", null, ["kontak"])
      : __h("p", null, ["beranda"]));
  `, { protocol: 'http:', pathname: '/kontak' });
  assert.equal(root.textContent, 'kontak');
});

test('klik + history.pushState merender ulang UI', () => {
  const { root, location } = boot(`
    __mount(() => __h("div", null, [
      location.pathname === "/narasi" ? __h("p", null, ["narasi"]) : __h("p", null, ["beranda"]),
      __h("button", { onClick: () => history.pushState({}, "", "/narasi") }, ["go"])
    ]));
  `, { protocol: 'http:', pathname: '/' });
  assert.match(root.textContent, /beranda/);
  root.find((n) => n.nodeName === 'BUTTON').click();
  assert.equal(location.pathname, '/narasi');
  assert.match(root.textContent, /narasi/);
});

test('Head: <link> dipindah ke <head> dan tidak diduplikasi saat render ulang', () => {
  const { document } = boot(`
    __mount(() => __h("div", null, [
      __C.Head({}, [__h("link", { rel: "stylesheet", href: "https://fonts.example/css?family=X" }, [])]),
      __h("p", null, ["isi"])
    ]));
    __rerender();
    __rerender();
  `);
  const links = document.head.childNodes.filter((n) => n.nodeName === 'LINK');
  assert.equal(links.length, 1);
  assert.equal(links[0].getAttribute('href'), 'https://fonts.example/css?family=X');
});

test('fokus input dipertahankan setelah render ulang', () => {
  const { document, root, sandbox } = boot(`
    __mount(() => __h("div", null, [
      __h("input", { id: "a" }, []),
      __h("input", { id: "b" }, [])
    ]));
  `);
  const before = root.querySelectorAll('input');
  before[1].focus();
  assert.equal(document.activeElement, before[1]);
  vm.runInContext('__rerender();', sandbox);
  const after = root.querySelectorAll('input');
  assert.notEqual(after[1], before[1]);
  assert.equal(document.activeElement, after[1]);
});
