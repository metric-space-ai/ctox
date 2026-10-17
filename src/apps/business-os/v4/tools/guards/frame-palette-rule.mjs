#!/usr/bin/env node
// Shell v4 – Waechter-Regel „frame_palette“ (Owner-Mangel M-17, Strang MAN2).
//
// Die Fensterkanten setzen sich aus den Icon-Pixeln fort; der Rahmen berechnet sie (v4/frame/index.js ignoriert
// layout.frame_palette) und der Manifest-Generator uebernimmt das Feld nicht mehr. Eine haendisch gesetzte Palette
// ist deshalb ein Befund: diese Regel meldet jeden Schluessel `frame_palette` (in beliebiger Tiefe) in
// <app>/module.json und <app>/v4-manifest.json unter modules/, customer-modules/ und desktop-apps/.
//
// Ratchet: frame-palette-baseline.json haelt je Datei die erlaubte Hoechstzahl (heutiger Altbestand, nur senken).
//   node frame-palette-rule.mjs [--root DIR] [--json]
//   node frame-palette-rule.mjs --check            Exit 1, wenn eine Datei ueber der Baseline liegt
//   node frame-palette-rule.mjs --write-baseline   Baseline auf den aktuellen Stand setzen (nur senken; neue Treffer
//                                                  nur mit --force nach Owner-Freigabe)
//
// Keine Abhaengigkeiten ausser Node >= 18.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const DEFAULT_ROOT = path.resolve(HERE, '../../..');
export const DEFAULT_BASELINE = path.join(HERE, 'frame-palette-baseline.json');
export const RULE = 'frame-palette';
export const KEY = 'frame_palette';
export const APP_ROOTS = ['modules', 'customer-modules', 'desktop-apps'];
export const FILES = ['module.json', 'v4-manifest.json'];

// Zaehlt `frame_palette`-Schluessel in einem JSON-Wert; Ergebnis: Liste der JSON-Pfade (z. B. "layout.frame_palette").
export function findKey(value, trail = []) {
  const hits = [];
  if (Array.isArray(value)) value.forEach((v, i) => hits.push(...findKey(v, [...trail, String(i)])));
  else if (value && typeof value === 'object') {
    for (const [k, v] of Object.entries(value)) {
      if (k === KEY) hits.push([...trail, k].join('.'));
      hits.push(...findKey(v, [...trail, k]));
    }
  }
  return hits;
}

// Text einer Datei pruefen. Nicht lesbares JSON zaehlt Textvorkommen (damit ein kaputtes Manifest nichts verdeckt).
export function scanText(text) {
  try {
    return findKey(JSON.parse(text));
  } catch {
    return [...String(text).matchAll(/"frame_palette"\s*:/g)].map(() => '(kein JSON)');
  }
}

export function scan(root = DEFAULT_ROOT) {
  const files = {};
  for (const top of APP_ROOTS) {
    const dir = path.join(root, top);
    if (!fs.existsSync(dir)) continue;
    for (const app of fs.readdirSync(dir).sort()) {
      for (const name of FILES) {
        const abs = path.join(dir, app, name);
        if (!fs.existsSync(abs) || !fs.statSync(abs).isFile()) continue;
        const hits = scanText(fs.readFileSync(abs, 'utf8'));
        if (hits.length) files[`${top}/${app}/${name}`] = hits;
      }
    }
  }
  return { rule: RULE, total: Object.values(files).reduce((s, h) => s + h.length, 0), files };
}

export function loadBaseline(file = DEFAULT_BASELINE) {
  if (!fs.existsSync(file)) return {};
  return JSON.parse(fs.readFileSync(file, 'utf8')).files || {};
}

// raised: Dateien ueber der Baseline (Exit 1); lowered: Dateien darunter (Baseline senken).
export function ratchet(res, baseline) {
  const raised = [];
  const lowered = [];
  for (const [file, hits] of Object.entries(res.files)) {
    const allowed = baseline[file] ?? 0;
    if (hits.length > allowed) raised.push({ file, count: hits.length, allowed, paths: hits });
  }
  for (const [file, allowed] of Object.entries(baseline)) {
    const count = res.files[file]?.length ?? 0;
    if (count < allowed) lowered.push({ file, count, allowed });
  }
  return { raised, lowered };
}

export function writeBaseline(file, res, { force = false, baseline = loadBaseline(file) } = {}) {
  const { raised } = ratchet(res, baseline);
  if (raised.length && !force) throw new Error(`Baseline wuerde steigen (${raised.map((r) => r.file).join(', ')}); nur mit --force nach Owner-Freigabe`);
  const files = Object.fromEntries(Object.entries(res.files).sort(([a], [b]) => a.localeCompare(b)).map(([f, h]) => [f, h.length]));
  fs.writeFileSync(file, `${JSON.stringify({ schema: 'shell-v4-frame-palette-baseline-v1', note: 'Nur senken. Hoechstzahl frame_palette je Datei; neue Dateien 0.', total: res.total, files }, null, 2)}\n`);
  return res.total;
}

function parseArgs(argv) {
  const o = { root: DEFAULT_ROOT, baseline: DEFAULT_BASELINE, json: false, check: false, write: false, force: false };
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === '--root') o.root = path.resolve(argv[++i]);
    else if (a === '--baseline') o.baseline = path.resolve(argv[++i]);
    else if (a === '--json') o.json = true;
    else if (a === '--check') o.check = true;
    else if (a === '--write-baseline') o.write = true;
    else if (a === '--force') o.force = true;
    else throw new Error(`unbekannte Option ${a}`);
  }
  return o;
}

export function main(argv = process.argv.slice(2)) {
  const o = parseArgs(argv);
  const res = scan(o.root);
  if (o.write) {
    const total = writeBaseline(o.baseline, res, { force: o.force });
    process.stdout.write(`Baseline geschrieben: ${o.baseline} (${total} Treffer)\n`);
    return 0;
  }
  const rat = ratchet(res, loadBaseline(o.baseline));
  if (o.json) process.stdout.write(`${JSON.stringify({ ...res, ...rat }, null, 2)}\n`);
  else {
    const out = [`frame_palette (M-17): ${res.total} Treffer in ${Object.keys(res.files).length} Datei(en)`];
    for (const [file, hits] of Object.entries(res.files)) out.push(`  ${file}: ${hits.join(', ')}`);
    if (rat.raised.length) out.push('', `Ueber der Baseline (${rat.raised.length}) – Kantenfarben kommen aus dem Icon, Feld entfernen:`, ...rat.raised.map((r) => `  ${r.file}: ${r.count} > ${r.allowed}`));
    if (rat.lowered.length) out.push('', `Unter der Baseline (${rat.lowered.length}) – mit --write-baseline senken.`);
    process.stdout.write(`${out.join('\n')}\n`);
  }
  return o.check && rat.raised.length ? 1 : 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { process.exitCode = main(); } catch (err) { process.stderr.write(`frame-palette-rule: ${err.message}\n`); process.exitCode = 2; }
}
