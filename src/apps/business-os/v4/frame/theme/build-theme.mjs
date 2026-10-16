#!/usr/bin/env node
// Shell v4 – Theme-Tokens für den Rahmen erzeugen (FR2 Zusatz 1).
//
// Apps lesen Farb-, Abstands- und Typo-Variablen, die bisher app.css (v3-Shell) am :root bereitstellte. Der v4-Rahmen
// lädt app.css nicht. Dieses Skript schreibt v4/frame/theme/theme.css mit GENAU den Variablen, die Apps nachweislich
// nutzen (var(--x) in modules/*/index.css und shared/base.css, transitiv aufgelöst), in Hell (:root) und Dunkel
// (:root[data-theme="dark"]) – dieselben Selektoren wie app.css, also dieselbe Umschaltung über data-theme.
// Quellen: app.css (Token-Blöcke am :root), ui-contract/v1/workjet-ui-contract.css. shared/base.css lädt der Rahmen
// selbst; was base.css schon definiert, steht nicht doppelt hier. Dazu die generischen Kit-Regeln der Pane-Grammatik
// aus app.css (KIT_CLASSES, siehe unten; Strang GEO2), die base.css voraussetzt, aber nicht selbst enthält.
//
//   node v4/frame/theme/build-theme.mjs [--check]     --check: Exit 1, wenn theme.css nicht dem Stand entspricht.
import { readFileSync, writeFileSync, readdirSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = join(HERE, '..', '..', '..');
const OUT = join(HERE, 'theme.css');
const strip = (css) => css.replace(/\/\*[\s\S]*?\*\//g, '');

// Oberste Regeln (ohne @-Blöcke) als [selektor, rumpf].
function topRules(css) {
  const out = [];
  let i = 0;
  const s = strip(css);
  while (i < s.length) {
    const open = s.indexOf('{', i);
    if (open < 0) break;
    const sel = s.slice(i, open).trim();
    let depth = 1, j = open + 1;
    while (j < s.length && depth) { if (s[j] === '{') depth++; else if (s[j] === '}') depth--; j++; }
    if (!sel.startsWith('@')) out.push([sel.replace(/\s+/g, ' '), s.slice(open + 1, j - 1)]);
    i = j;
  }
  return out;
}
function decls(body) {
  const m = new Map();
  for (const d of body.split(';')) {
    const k = d.indexOf(':');
    if (k < 0) continue;
    const name = d.slice(0, k).trim();
    if (name.startsWith('--')) m.set(name, d.slice(k + 1).trim());
  }
  return m;
}
const LIGHT = new Set([':root', ':root, :root[data-theme="light"]', ':root,:root[data-theme="light"]', ':root[data-theme="light"]']);
const DARK = new Set([':root[data-theme="dark"]']);
function tokens(file) {
  const light = new Map(), dark = new Map();
  for (const [sel, body] of topRules(readFileSync(file, 'utf8'))) {
    if (LIGHT.has(sel)) for (const [k, v] of decls(body)) light.set(k, v);
    else if (DARK.has(sel)) for (const [k, v] of decls(body)) dark.set(k, v);
  }
  return { light, dark };
}
const used = (css) => new Set([...strip(css).matchAll(/var\(\s*(--[\w-]+)/g)].map((m) => m[1]));

const sources = [join(BOS, 'ui-contract/v1/workjet-ui-contract.css'), join(BOS, 'app.css')].map(tokens);
const light = new Map(), dark = new Map();
for (const t of sources) { for (const [k, v] of t.light) light.set(k, v); for (const [k, v] of t.dark) dark.set(k, v); }
const base = tokens(join(BOS, 'shared/base.css'));
const baseCss = readFileSync(join(BOS, 'shared/base.css'), 'utf8');

// Kit-Regeln (Strang GEO2): generische Pane-Grammatik, die bisher NUR app.css bereitstellte (base.css verweist für die
// Kopfzeilen-Typografie ausdrücklich auf app.css). Ohne .ctox-pane-title-row (flex) rutschen die Kopf-Icons unter den
// Titel. Übernommen werden genau die obersten app.css-Regeln, deren Selektoren ausschließlich Klassen aus KIT_CLASSES
// tragen; keine App-Sonderfälle, keine Shell-Positionierung (.drawer selbst ist fixed-Shell-Chrome und bleibt draußen),
// keine an `.shell-window[…]` gebundenen v2-Regeln. Jede Klasse muss von mindestens einer App (modules/*/index.js|
// index.css) genutzt werden, sonst bricht der Generator ab.
// v4-Vertrag G4 (Geometrie-Labor): keine abgeschnittenen Texte. Die v3-Typografie kürzt Kicker und Pane-Titel per
// Ellipse (und die v2-Fensterregeln lassen dabei den Kicker zuerst weichen). Im Kit entfallen nowrap/overflow/
// text-overflow und das dafür nötige min-width: 0; KIT_V4 legt fest, wer weicht: der Kicker (kurze Marke) bleibt ganz,
// der Titel bricht an Wortgrenzen um (Mindestbreite = längstes Wort, kein Bruch mitten im Wort).
// Strang VIS4: Köpfe wachsen mit, aber alle Spaltenköpfe eines Fensters stehen gleich hoch. v4/frame/pane-heads.js
// misst je Gruppe nebeneinanderstehender Köpfe die höchste natürliche Höhe und setzt sie als --v4-pane-head-h samt
// [data-v4-pane-row]; die Regel hier macht daraus min-height. !important, weil Apps eigene min-height-Werte für
// ihre Köpfe setzen: die gemessene Höhe enthält diese bereits (sie ist das Maximum der natürlichen Höhen).
const KIT_CLASSES = ['ctox-pane-header', 'ctox-pane-title-row', 'ctox-pane-titles', 'ctox-pane-kicker', 'ctox-pane-title', 'os-scrollbar', 'drawer-body', 'drawer-header-row'];
const KIT_V4 = `.ctox-pane-titles > .ctox-pane-kicker {
  flex: 0 0 auto;
}

.ctox-pane-titles > .ctox-pane-title {
  flex: 0 1 auto;
}

.ctox-pane-header[data-v4-pane-row] {
  box-sizing: border-box;
  min-height: var(--v4-pane-head-h) !important;
}`;
// Strang FR4: gesperrte Kit-Knöpfe lesbar (WCAG ≥ 3:1 Text gegen Knopfgrund, hell und dunkel). base.css dimmt per
// opacity .5/.55 – auf dem Akzent (is-primary, ctox-run-control) blieb die Beschriftung kaum lesbar (~1,9:1).
// Gesperrt heißt hier: neutrale Fläche, gedämpfte Schrift (--muted ≈ 4,3:1 auf --kit-fill), volle Deckkraft. :root vorn,
// weil theme.css vor base.css lädt und dessen Regeln (bis .is-primary:hover, 0-3-0) sonst gewinnen; Ghost bleibt ohne Fläche.
// KIT2: ohne Übergang – ein gesperrter Knopf hat keinen Hover; beim Theme-Wechsel stand er sonst einen Übergang lang mit
// heller Schrift auf dunkler Fläche (FR4-4 maß das nach einem Frame: 2,74 nach der neuen --muted-Spreizung).
const KIT_DISABLED = `:root :is(.ctox-button, .ctox-icon-button, .ctox-run-control):disabled,
:root :is(.ctox-button, .ctox-icon-button, .ctox-run-control):disabled:hover {
  opacity: 1;
  background: var(--kit-fill);
  border-color: var(--line);
  color: var(--muted);
  cursor: not-allowed;
  transition: none;
}

:root .ctox-button--ghost:disabled,
:root .ctox-button--ghost:disabled:hover {
  background: transparent;
  border-color: transparent;
}`;
// Strang KIT2: Inhalts-Palette auf WCAG AA (G10 strikt). Die Rahmen-Tokens aus app.css lagen für App-INHALTE knapp
// darunter: --muted auf der Kit-Mulde .ctox-well 4,43/4,33 und auf der gewählten Zeile (--accent-soft) 3,87/3,78;
// dunkel der Akzent als Text auf --surface 4,06, ebenso der Kit-Primärknopf (--surface auf --accent). Weiß auf einem
// Akzent, der als Text auf #111 ≥ 4,5 erreicht, kann nicht ≥ 4,5 sein; dunkel trägt der Akzent darum die Flächenfarbe
// als Beschriftung (--accent-foreground = --surface, wie der Kit-Primärknopf). Gilt nur im Fensterinhalt
// (.shell-window-content, .v4-app-root): Fensterrahmen (Fokuskante aus --accent, Fensterknöpfe aus --muted),
// Kopfleiste, Startmenü, Desktop und Crew-Leiste behalten ihre Werte (G0/TOP/FR6d-G0). Host- und Workspace-Branding
// (--ctox-host-*, data-workspace-branding="custom") gehen weiter vor.
//   hell  --muted #64646d: --surface 5,86 · .ctox-well 5,37 · --accent-soft 4,69 (Text bleibt #52525b, also abgestuft)
//   hell  --danger #a4453c (statt oklch(0.56 0.13 28) = #b45248, auf --accent-soft 3,97): --accent-soft 4,82 · --surface 6,01
//   dunkel --muted #959595: --surface 6,30 · .ctox-well 5,63 · --accent-soft 4,92
//   dunkel --accent #6690f5: als Text auf --surface 6,18 · .ctox-well 5,52 · --accent-soft 4,83; --surface darauf 6,18
const KIT_CONTENT = `:root:not([data-theme="dark"]):not([data-workspace-branding="custom"]) :where(.shell-window-content, .v4-app-root) {
  --muted: var(--ctox-host-muted, #64646d);
  --danger: #a4453c;
}

:root[data-theme="dark"]:not([data-workspace-branding="custom"]) :where(.shell-window-content, .v4-app-root) {
  --muted: var(--ctox-host-muted, #959595);
  --accent: var(--ctox-host-accent, #6690f5);
  --accent-foreground: var(--ctox-host-accent-foreground, var(--surface));
}`;
const classesOf = (sel) => [...sel.matchAll(/\.([A-Za-z_][\w-]*)/g)].map((m) => m[1]);
function kitRules(css) {
  const out = [];
  for (const [sel, body] of topRules(css)) {
    const parts = sel.split(',').map((p) => p.trim());
    const ok = parts.every((p) => p && !/[[#]/.test(p) && classesOf(p).length && classesOf(p).every((c) => KIT_CLASSES.includes(c)));
    if (!ok) continue;
    let decl = body.split(';').map((d) => d.trim()).filter(Boolean);
    if (decl.some((d) => /^text-overflow\s*:\s*ellipsis$/.test(d))) {
      // min-width: 0 diente nur dem Kürzen; ohne Ellipse ließe es den Text mitten im Wort brechen („OUTBOU|ND“).
      decl = decl.filter((d) => !/^(text-overflow|white-space|overflow)\s*:/.test(d) && !/^min-width\s*:\s*0$/.test(d));
      decl.push('overflow-wrap: break-word');
    }
    out.push(`${parts.join(',\n')} {\n${decl.map((d) => `  ${d};`).join('\n')}\n}`);
  }
  return out;
}
const appCss = readFileSync(join(BOS, 'app.css'), 'utf8');
const KIT = [...kitRules(appCss), KIT_V4, KIT_DISABLED, KIT_CONTENT];
const kitUsers = Object.fromEntries(KIT_CLASSES.map((c) => [c, []]));

const want = new Set(used(baseCss));
for (const r of KIT) for (const v of used(r)) want.add(v);
const modules = join(BOS, 'modules');
const appFiles = [];
for (const id of readdirSync(modules)) {
  const f = join(modules, id, 'index.css');
  if (existsSync(f)) { appFiles.push(id); for (const v of used(readFileSync(f, 'utf8'))) want.add(v); }
  for (const g of [f, join(modules, id, 'index.js')]) {
    if (!existsSync(g)) continue;
    const t = readFileSync(g, 'utf8');
    for (const c of KIT_CLASSES) if (new RegExp(`(^|[^\\w-])${c}(?![\\w-])`).test(t) && !kitUsers[c].includes(id)) kitUsers[c].push(id);
  }
}
const unusedKit = KIT_CLASSES.filter((c) => !kitUsers[c].length);
if (unusedKit.length) { console.error(`Kit-Klassen ohne App-Nutzung: ${unusedKit.join(', ')}`); process.exit(1); }
const missingKit = KIT_CLASSES.filter((c) => !KIT.some((r) => r.includes(`.${c}`)));
if (missingKit.length) { console.error(`Kit-Klassen ohne Regel in app.css: ${missingKit.join(', ')}`); process.exit(1); }
// Transitiv: Tokens, auf die gewollte Tokens verweisen.
const queue = [...want];
while (queue.length) {
  const k = queue.pop();
  for (const v of [light.get(k), dark.get(k), base.light.get(k), base.dark.get(k)]) {
    if (!v) continue;
    for (const d of used(v)) if (!want.has(d)) { want.add(d); queue.push(d); }
  }
}
const pick = (map, baseMap) => [...want].filter((k) => map.has(k) && !baseMap.has(k)).sort().map((k) => `  ${k}: ${map.get(k)};`);
const L = pick(light, base.light);
const D = pick(dark, base.dark);
const css = `/* GENERIERT von v4/frame/theme/build-theme.mjs – nicht von Hand ändern.
   Theme-Tokens der Shell-Schnittstelle: nur Variablen, die Apps nachweislich nutzen (${appFiles.length} App-Stylesheets modules/<id>/index.css +
   shared/base.css, transitiv). Hell am :root, dunkel an :root[data-theme="dark"] (wie app.css). ${L.length} hell, ${D.length} dunkel. */
:root {
  color-scheme: light;
${L.join('\n')}
}
:root[data-theme="dark"] {
  color-scheme: dark;
${D.join('\n')}
}

/* Kit-Regeln der Pane-Grammatik aus app.css (${KIT.length} Regeln; Ellipse durch Umbruch ersetzt, G4; Klassen und nutzende Apps:
${KIT_CLASSES.map((c) => `   ${c}: ${kitUsers[c].length} (${kitUsers[c].slice(0, 6).join(', ')}${kitUsers[c].length > 6 ? ', …' : ''})`).join('\n')}) */
${KIT.join('\n\n')}
`;
if (process.argv.includes('--check')) {
  const cur = existsSync(OUT) ? readFileSync(OUT, 'utf8') : '';
  if (cur !== css) { console.error('theme.css veraltet: node v4/frame/theme/build-theme.mjs'); process.exit(1); }
  console.log(`theme.css aktuell (${L.length} hell, ${D.length} dunkel)`);
} else {
  writeFileSync(OUT, css);
  const missing = [...want].filter((k) => !light.has(k) && !base.light.has(k) && !k.startsWith('--shell-v2-'));
  console.log(`theme.css: ${L.length} hell, ${D.length} dunkel, ${KIT.length} Kit-Regeln; genutzt ${want.size}; ohne Quelle (app-eigen oder Rückfall) ${missing.length}`);
}
