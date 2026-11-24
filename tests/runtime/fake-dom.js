'use strict';
/* DOM palsu untuk tes runtime di Node. Hanya mencakup yang dipakai prelude. */

class FakeNode {
  constructor(nodeType, name) {
    this.nodeType = nodeType;
    this.nodeName = name;
    this.parentNode = null;
    this.childNodes = [];
    this.attributes = {};
    this.style = {};
    this.listeners = {};
    this.data = '';
  }
  get textContent() {
    if (this.nodeType === 3) return this.data;
    return this.childNodes.map((c) => c.textContent).join('');
  }
  get children() { return this.childNodes.filter((c) => c.nodeType === 1); }
  get tagName() { return this.nodeName; }
  appendChild(child) {
    if (child.nodeType === 11) {
      const kids = child.childNodes.slice();
      child.childNodes = [];
      for (const k of kids) this.appendChild(k);
      return child;
    }
    if (child.parentNode) child.parentNode.removeChild(child);
    child.parentNode = this;
    this.childNodes.push(child);
    return child;
  }
  removeChild(child) {
    const i = this.childNodes.indexOf(child);
    if (i >= 0) this.childNodes.splice(i, 1);
    child.parentNode = null;
    return child;
  }
  replaceChildren(...nodes) {
    for (const c of this.childNodes) c.parentNode = null;
    this.childNodes = [];
    for (const n of nodes) this.appendChild(n);
  }
  setAttribute(name, value) { this.attributes[name] = String(value); }
  getAttribute(name) { return this.attributes[name] ?? null; }
  addEventListener(type, fn) { (this.listeners[type] ||= []).push(fn); }
  removeEventListener() {}
  focus() { this.ownerDocument.activeElement = this; }
  querySelectorAll(selector) {
    const names = selector.split(',').map((s) => s.trim().toUpperCase());
    const out = [];
    const walk = (n) => {
      for (const c of n.childNodes) {
        if (c.nodeType === 1 && names.includes(c.nodeName)) out.push(c);
        walk(c);
      }
    };
    walk(this);
    return out;
  }
  click() { for (const fn of this.listeners.click || []) fn({ type: 'click', target: this }); }
  find(predicate) {
    if (predicate(this)) return this;
    for (const c of this.childNodes) {
      const hit = c.find(predicate);
      if (hit) return hit;
    }
    return null;
  }
}

function createDocument() {
  const root = new FakeNode(1, 'DIV');
  const head = new FakeNode(1, 'HEAD');
  const document = {
    title: '',
    root,
    head,
    activeElement: null,
    createElement: (name) => {
      const n = new FakeNode(1, String(name).toUpperCase());
      n.ownerDocument = document;
      return n;
    },
    createTextNode: (text) => {
      const n = new FakeNode(3, '#text');
      n.data = String(text);
      return n;
    },
    createComment: () => new FakeNode(8, '#comment'),
    createDocumentFragment: () => new FakeNode(11, '#document-fragment'),
    getElementById: (id) => (id === 'root' ? root : null),
  };
  root.ownerDocument = document;
  head.ownerDocument = document;
  return { document, root, head, FakeNode };
}

module.exports = { FakeNode, createDocument };
