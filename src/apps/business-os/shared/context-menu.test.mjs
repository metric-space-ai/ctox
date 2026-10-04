import assert from 'node:assert/strict';
import { createContextMenu } from './context-menu.js';

const listeners = new Map();
class Element {
  constructor(tag) {
    this.tag = tag;
    this.children = [];
    this.style = {};
    this.dataset = {};
    this.attributes = {};
    this.isConnected = true;
    this.classes = new Set();
    this.classList = {
      add: (name) => this.classes.add(name),
      remove: (name) => this.classes.delete(name),
      contains: (name) => this.classes.has(name),
      toggle: (name, enabled) => enabled ? this.classes.add(name) : this.classes.delete(name),
    };
  }
  set className(value) { this.classes = new Set(value.split(' ').filter(Boolean)); }
  set innerHTML(value) {
    this.markup = value;
    this.label = new Element('span');
  }
  setAttribute(name, value) { this.attributes[name] = value; }
  appendChild(child) { this.children.push(child); child.parentElement = this; }
  querySelector(selector) {
    if (selector === '.shell-context-menu-label') return this.label;
    if (selector === '.shell-context-menu-trailing') return null;
    if (selector === '.shell-context-menu-item.is-selected') return this.children.find((item) => item.classes.has('is-selected')) || null;
    return null;
  }
  querySelectorAll(selector) {
    if (selector === '.shell-context-menu-item') return this.children.filter((item) => item.classes.has('shell-context-menu-item'));
    return [];
  }
  getBoundingClientRect() {
    if (this.tag === 'viewport') return { left: 0, top: 0, right: 300, bottom: 200, width: 300, height: 200 };
    return { left: 0, top: 0, right: 200, bottom: Math.min(350, parseInt(this.style.maxHeight || '350', 10)), width: 200, height: Math.min(350, parseInt(this.style.maxHeight || '350', 10)) };
  }
  contains(target) { return this === target || this.children.some((child) => child.contains(target)); }
  focus() { document.activeElement = this; }
  click() { this.onclick?.({ target: { closest: () => null }, stopPropagation() {} }); }
  remove() { this.isConnected = false; }
}

globalThis.HTMLElement = Element;
globalThis.document = {
  body: new Element('body'),
  documentElement: new Element('viewport'),
  createElement: (tag) => new Element(tag),
  activeElement: null,
  addEventListener: (name, callback) => listeners.set(name, callback),
  removeEventListener: (name, callback) => { if (listeners.get(name) === callback) listeners.delete(name); },
};
globalThis.requestAnimationFrame = (callback) => callback();

const origin = new Element('button');
const menu = createContextMenu({});
let disabledCalls = 0;
let enabledCalls = 0;
const items = [
  { label: 'Offline action', disabled: true, disabledReason: 'Sync unavailable', onDisabled: () => { disabledCalls += 1; } },
  { label: 'Ready action', action: () => { enabledCalls += 1; } },
];
const event = { target: origin, clientX: 290, clientY: 190, preventDefault() {}, stopPropagation() {} };
menu.show(event, items);
const opened = document.body.children.at(-1);
assert.equal(opened.style.left, '92px');
assert.equal(opened.style.top, '8px');
assert.equal(opened.style.maxHeight, '184px');
assert.equal(opened.children[0].attributes['aria-description'], 'Sync unavailable');
assert.equal(document.activeElement, opened.children[0]);
await new Promise((resolve) => setTimeout(resolve, 15));
listeners.get('keydown')({ key: 'Enter', preventDefault() {} });
assert.equal(disabledCalls, 1);
assert.equal(enabledCalls, 0);

menu.show(event, items);
await new Promise((resolve) => setTimeout(resolve, 15));
listeners.get('keydown')({ key: 'ArrowDown', preventDefault() {} });
assert.equal(document.activeElement, document.body.children.at(-1).children[1]);
listeners.get('keydown')({ key: 'Enter', preventDefault() {} });
assert.equal(enabledCalls, 1);

menu.show(event, items);
await new Promise((resolve) => setTimeout(resolve, 15));
listeners.get('keydown')({ key: 'Escape', preventDefault() {} });
assert.equal(document.activeElement, origin);
menu.show(event, items);
await new Promise((resolve) => setTimeout(resolve, 15));
listeners.get('mousedown')({ target: new Element('outside') });
assert.equal(listeners.has('keydown'), false);

console.log('context menu keyboard and viewport behavior ok');
