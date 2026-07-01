import test from 'node:test';
import assert from 'node:assert/strict';

import {
  applyWorkspaceBranding,
  normalizeBrandingImportPayload,
  workspaceBrandingStyleText,
} from './branding.js';

test('workspace branding style generation emits light and dark token overrides', () => {
  const css = workspaceBrandingStyleText({
    custom: true,
    name: 'Acme',
    light: {
      bg: '#ffffff',
      surface: '#f8fafc',
      text: '#111827',
      accent: '#005a9c',
    },
    dark: {
      bg: '#030712',
      surface: '#111827',
      text: '#f9fafb',
      accent: '#7dd3fc',
    },
  });

  assert.match(css, /:root\[data-workspace-branding="custom"\]/);
  assert.match(css, /--bg: #ffffff;/);
  assert.match(css, /--accent: #005a9c;/);
  assert.match(css, /\[data-theme="dark"\]/);
  assert.match(css, /--accent: #7dd3fc;/);
});

test('workspace branding import rejects unknown tokens and unsafe css values', () => {
  assert.throws(() => normalizeBrandingImportPayload({
    name: 'Bad',
    light: { bg: '#fff', unknown: '#000' },
    dark: { bg: '#000' },
  }), /Unbekannter Branding Token/);

  assert.throws(() => normalizeBrandingImportPayload({
    name: 'Bad',
    light: { bg: 'url(https://example.test/pixel)' },
    dark: { bg: '#000' },
  }), /Unsicherer Branding Wert/);

  assert.throws(() => normalizeBrandingImportPayload({
    name: 'Bad',
    light: { bg: '#fff' },
  }), /dark Objekt/);
});

test('applyWorkspaceBranding installs one style tag and reset removes it', () => {
  const elements = new Map();
  globalThis.document = {
    documentElement: {
      dataset: {},
      removeAttribute(name) {
        delete this.dataset[name.replace(/^data-/, '').replace(/-([a-z])/g, (_, ch) => ch.toUpperCase())];
      },
    },
    head: {
      appendChild(el) {
        elements.set(el.id, el);
      },
    },
    getElementById(id) {
      return elements.get(id) || null;
    },
    createElement(tagName) {
      return {
        tagName,
        id: '',
        dataset: {},
        textContent: '',
        remove() {
          elements.delete(this.id);
        },
      };
    },
  };

  const branding = applyWorkspaceBranding({
    custom: true,
    name: 'Acme',
    light: { bg: '#ffffff', text: '#111827' },
    dark: { bg: '#030712', text: '#f9fafb' },
  });

  assert.equal(branding.custom, true);
  assert.equal(document.documentElement.dataset.workspaceBranding, 'custom');
  const style = document.getElementById('ctox-workspace-branding-style');
  assert.ok(style);
  assert.match(style.textContent, /--bg: #ffffff;/);

  const reset = applyWorkspaceBranding(null);
  assert.equal(reset.custom, false);
  assert.equal(document.getElementById('ctox-workspace-branding-style'), null);
  assert.equal(document.documentElement.dataset.workspaceBranding, undefined);
});
