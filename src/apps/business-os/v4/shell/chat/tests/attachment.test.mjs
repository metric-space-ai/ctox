// CHATATT: Anhang über die Büroklammer der V2-Chatleiste im v4-Rahmen, Kern-Stub (Pin b6624eb), echte Maus/Tastatur.
// Lauf (aus src/apps/business-os): node v4/shell/chat/tests/attachment.test.mjs [counter]
//   counter = Gegenprobe ohne Adapter: index.js und raw-db.js werden mit dem Stand von HEAD (vor CHATATT) ausgeliefert;
//   (a) und (b) müssen rot sein.
//   noref = zweite Gegenprobe (E30): Adapter aktiv, aber ohne BlobRef im business_chats-Dokument; dann muss genau
//   „files.get als App ctox“ (und der Dokument-Check) rot sein – die Lesbarkeit kommt aus dem Chat-Dokument.
// Fälle: (a) Datei über die Büroklammer (echter Dateidialog, setFiles) → Vorschau im Chat, keine Fehlermeldung;
// (b) Senden → genau ein Befehl, Nutzlast trägt den BlobRef, Bytes über files.get als App ctox byte-gleich (E30 über
// business_chats), eine App ohne Leserecht auf business_chats bekommt forbidden.
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import crypto from 'node:crypto';
import zlib from 'node:zlib';
import path from 'node:path';
import fs from 'node:fs';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { SEED, openDock } from './drive.mjs';
import { realManifestRoutes } from '../../../tools/port/real-manifest.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
process.env.CTOX_APP_API_STUB ||= path.join(os.homedir(), '.local/state/shell-v4/stub-pins/4b0d0ac');
const { startServer } = await import('../../../frame/lab/serve.mjs');
const require = createRequire(path.join(os.homedir(), 'Documents/ctox/src/apps/business-os/package.json'));
const { chromium } = require('playwright');
const exe = path.join(os.homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');
const COUNTER = process.argv[2] === 'counter';
const NOREF = process.argv[2] === 'noref';
const MODE = COUNTER ? 'att-counter' : NOREF ? 'att-noref' : 'att';
const shots = path.join(here, 'shots');
fs.mkdirSync(shots, { recursive: true });
const shot = (name) => path.join(shots, `${MODE}-${name}.png`);
const TEXT = 'Bitte die Rechnung im Anhang prüfen.';

// Binärdatei: gültiges PNG 32×32 (Verlauf), damit die Vorschau ein Bild zeigt und files.get Bytes (kein Text) liefert.
function pngBytes() {
  const crcTable = Array.from({ length: 256 }, (_, n) => { let c = n; for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
  const crc = (buf) => { let c = 0xffffffff; for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; };
  const chunk = (type, data) => {
    const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(type, 'ascii'), data]);
    const c = Buffer.alloc(4); c.writeUInt32BE(crc(td));
    return Buffer.concat([len, td, c]);
  };
  const w = 32, h = 32;
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = 2;
  const rows = [];
  for (let y = 0; y < h; y += 1) { rows.push(0); for (let x = 0; x < w; x += 1) rows.push(x * 8, y * 8, 160); }
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(Buffer.from(rows))), chunk('IEND', Buffer.alloc(0))]);
}
const FILE = { name: 'rechnung-scan.png', mimeType: 'image/png', buffer: pngBytes() };
const FILE_SHA = crypto.createHash('sha256').update(FILE.buffer).digest('hex');

const results = [];
function check(name, ok, detail = '') {
  results.push({ name, ok: !!ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'} [${MODE}] ${name}${detail ? ` – ${detail}` : ''}`);
}

const server = await startServer(0);
const base = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({ executablePath: exe, headless: true });
const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
const P = await ctx.newPage();
const perr = [];
P.on('pageerror', (e) => perr.push(String(e)));
P.on('console', (m) => { if (m.type() === 'error' && !/404|Failed to load resource/.test(m.text())) perr.push(m.text()); });
if (COUNTER) {
  const head = (rel) => execFileSync('git', ['show', `HEAD:./${rel}`], { cwd: path.resolve(here, '..'), encoding: 'utf8' });
  const bodies = { 'index.js': head('index.js'), 'raw-db.js': head('raw-db.js') };
  await P.route(/\/v4\/shell\/chat\/(index|raw-db)\.js(\?.*)?$/, (route) => {
    const file = /\/(index|raw-db)\.js/.exec(route.request().url())[1] + '.js';
    return route.fulfill({ status: 200, contentType: 'text/javascript', body: bodies[file] });
  });
}
if (NOREF) {
  const RECORD = 'if (row.linked_collection === CHAT_COLLECTION && row.linked_record_id) await recordChatBlob(';
  const body = fs.readFileSync(path.resolve(here, '../raw-db.js'), 'utf8');
  if (!body.includes(RECORD)) throw new Error('noref: Schreibstelle des BlobRef nicht gefunden');
  await P.route(/\/v4\/shell\/chat\/raw-db\.js(\?.*)?$/, (route) => route.fulfill({ status: 200, contentType: 'text/javascript',
    body: body.replace(RECORD, 'if (false) await recordChatBlob(') }));
}

async function clickCenter(handle) {
  const b = await handle?.boundingBox();
  if (!b) return false;
  await P.mouse.click(b.x + b.width / 2, b.y + b.height / 2);
  return true;
}
const W = `[data-ctox-chat-root] [data-chat-id="${SEED.id}"]`;

try {
  await P.goto(`${base}/v4/frame/lab/stub-lab.html?inst=chatatt-${Date.now().toString(36)}`);
  await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
  await P.evaluate(async ([seed, ctoxRoutes]) => {
    const { kernel } = window.__lab;
    window.__attCmds = [];
    kernel.sim.onCommand('business_os.chat.task', (payload, c) => {
      window.__attCmds.push({ payload, app_id: c.app_id, command_id: c.command_id });
      return { ok: true, task_id: `queue-att-${window.__attCmds.length}`, chat_id: payload.chat_id, status: 'queued' };
    });
    const now = Date.now();
    await kernel.sim.push('business_chats', [{
      id: seed.id, owner_user_id: kernel.appApi('shell', 'dev').actor.id, title: seed.title, createdAt: now - 60_000, updated_at_ms: now - 60_000,
      open: true, minimized: true, messages: [{ id: 'msg_seed_1', role: 'user', text: seed.text, createdAt: now - 60_000 }],
    }]);
    const svg = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" fill="#0f766e"/></svg>';
    // E48: Eigentümer der Shell-Fremdlesungen (business_chats, ctox_crew_members, …) mit echtem Manifest (collections +
    // exports), sonst liest die Shell sie nicht.
    await kernel.sim.publishAppVersion('ctox', '1.0.0', { title: 'CTOX', layout: { category: 'Crew', icon_svg: svg }, styles: [], manifestExtra: ctoxRoutes });
    // Gegenprobe für E30: eine App ohne business_chats darf die Datei nicht lesen.
    await kernel.sim.publishAppVersion('notes-x', '1.0.0', { title: 'Notizen', layout: { category: 'Büro', icon_svg: svg }, styles: [], collections: { notes_x_items: {} } });
  }, [SEED, realManifestRoutes('ctox')]);
  await P.waitForSelector('[data-v4-app="ctox"]', { state: 'attached' });
  await P.waitForSelector('[data-ctox-chat-root] [data-chat-dock]', { timeout: 8000 }).catch(() => {});
  await openDock(P);
  if (!(await P.evaluate((sel) => { const r = document.querySelector(sel)?.getBoundingClientRect(); return !!r && r.width > 0; }, `${W} textarea[name="message"]`))) {
    await clickCenter(await P.$(`[data-ctox-chat-root] [data-chat-focus="${SEED.id}"]`));
  }
  await P.waitForSelector(`${W} [data-chat-clip]`, { timeout: 5000 }).catch(() => {});

  // ---------- (a) Büroklammer → Dateidialog → Vorschau ----------
  const chooserP = P.waitForEvent('filechooser', { timeout: 5000 }).catch(() => null);
  const clipped = await clickCenter(await P.$(`${W} [data-chat-clip]`));
  const chooser = await chooserP;
  check('(a) Büroklammer öffnet den Dateidialog (echte Maus)', clipped && !!chooser, JSON.stringify({ clipped, chooser: !!chooser }));
  if (chooser) await chooser.setFiles(FILE);
  await P.waitForSelector(`${W} .ctox-attachment-item`, { timeout: 5000 }).catch(() => {});
  const staged = await P.evaluate((sel) => {
    const items = [...document.querySelectorAll(`${sel} .ctox-attachment-item`)];
    return { count: items.length, names: items.map((i) => i.querySelector('.ctox-attachment-name')?.textContent?.trim() || ''), thumb: !!document.querySelector(`${sel} .ctox-attachment-thumbnail`) };
  }, W);
  check('(a) Anhang steht als Vorschau im Chat', staged.count === 1 && staged.names[0] === FILE.name && staged.thumb, JSON.stringify(staged));
  await P.screenshot({ path: shot('1280-staged') });

  // ---------- (b) tippen, senden ----------
  const ta = await P.$(`${W} textarea[name="message"]`);
  await clickCenter(ta);
  await P.keyboard.type(TEXT, { delay: 5 });
  await clickCenter(await P.$(`${W} [data-chat-send]`));
  await P.waitForFunction(() => (window.__attCmds || []).length > 0, null, { timeout: 8000 }).catch(() => {});
  await P.waitForFunction((sel) => /Aufgabenliste|nicht an die Crew übergeben/.test(document.querySelector(sel)?.textContent || ''), W, { timeout: 8000 }).catch(() => {});
  await P.waitForTimeout(800);
  const after = await P.evaluate((sel) => {
    const t = document.querySelector(sel)?.textContent?.replace(/\s+/g, ' ') || '';
    const err = /nicht an die Crew übergeben|leaseCollection|Dateiablage ist nicht verfügbar|nicht in der Dateiablage/.exec(t)?.[0] || '';
    return { accepted: /Aufgabenliste/.test(t), err, excerpt: err ? t.slice(Math.max(0, t.indexOf(err) - 80), t.indexOf(err) + 160) : '' };
  }, W);
  check('(a) keine Fehlermeldung nach dem Senden, Crew hat angenommen', after.accepted && !after.err, JSON.stringify(after));
  const sent = await P.evaluate(() => window.__attCmds);
  const p = sent[0]?.payload || {};
  const ref = Array.isArray(p.attachments) ? p.attachments[0] : null;
  check('(b) genau ein Befehl business_os.chat.task', sent.length === 1, `count=${sent.length}`);
  check('(b) Nutzlast trägt den BlobRef (kind blob, ref = sha256 der Datei, beide Listen, Speicher files)',
    !!ref && ref.kind === 'blob' && ref.ref === FILE_SHA && ref.content_hash === FILE_SHA && ref.size_bytes === FILE.buffer.length
      && ref.name === FILE.name && ref.mime_type === FILE.mimeType && !('file_id' in ref) && p.attachment_refs?.[0]?.ref === FILE_SHA
      && p.prompt === TEXT,
    JSON.stringify({ ref, refs: p.attachment_refs?.length ?? null, prompt: p.prompt }));
  // BlobRef im Chat-Dokument (Eigentümer ctox), kanonisch.
  await P.waitForTimeout(400);
  const viaChat = await P.evaluate(async ([seedId, sha, bytesLen]) => {
    const { kernel } = window.__lab;
    const row = kernel.appApi('ctox', '1.0.0').get('business_chats', seedId).get();
    const blobs = row?.attachment_blobs || [];
    const out = { blobs: blobs.map((b) => ({ ref: b.ref, name: b.name, size: b.size })), messages: (row?.messages || []).length };
    try {
      const got = await kernel.appApi('ctox', '1.0.0').files.get({ ref: sha });
      const bytes = got.bytes || (got.text != null ? new TextEncoder().encode(got.text) : null);
      out.found = got.found; out.size = bytes?.byteLength ?? null; out.b64 = bytes ? btoa(String.fromCharCode(...bytes)) : null;
    } catch (e) { out.error = `${e.code || ''} ${e.message}`; }
    try { await kernel.appApi('notes-x', '1.0.0').files.get({ ref: sha }); out.foreign = 'readable'; } catch (e) { out.foreign = e.code || String(e); }
    out.len = bytesLen;
    return out;
  }, [SEED.id, FILE_SHA, FILE.buffer.length]);
  check('(b) business_chats-Dokument trägt den BlobRef', viaChat.blobs.length === 1 && viaChat.blobs[0].ref === FILE_SHA, JSON.stringify(viaChat.blobs));
  check('(b) files.get als App ctox: Bytes byte-gleich (E30 über business_chats)',
    viaChat.found === true && viaChat.b64 === FILE.buffer.toString('base64'), JSON.stringify({ found: viaChat.found, size: viaChat.size, error: viaChat.error || null }));
  check('(b) App ohne Leserecht auf business_chats: forbidden', viaChat.foreign === 'forbidden', String(viaChat.foreign));
  const bubble = await P.evaluate((sel) => document.querySelector(sel)?.textContent?.includes('Bitte die Rechnung im Anhang') || false, W);
  check('(b) Verlauf zeigt die gesendete Nachricht', bubble);
  await P.screenshot({ path: shot('1280-sent') });
  check('keine Seitenfehler', perr.length === 0, perr.slice(0, 3).join(' | '));
} finally {
  await browser.close();
  server.close();
}

const failed = results.filter((r) => !r.ok);
console.log(`\n[${MODE}] ${results.length - failed.length}/${results.length} PASS`);
fs.writeFileSync(path.join(shots, `${MODE}-results.json`), JSON.stringify(results, null, 2));
process.exit(COUNTER || NOREF ? 0 : (failed.length ? 1 : 0));
