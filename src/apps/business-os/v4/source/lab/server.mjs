// Labor-Server Quelltext-Modus (nur statische Dateien, keine Daten-Endpunkte).
//   /                      → v4/source/lab/index.html
//   /business-os/<pfad>    → src/apps/business-os/<pfad>   (lesend; Shell, code-editor, vendor/monaco, shared)
//   /stub/<datei>          → eingefrorener Kern-Stub, Pfad aus CTOX_APP_API_STUB (Default: Pin 9fa9948), inkl. testkit/
//   /~a/shell@<sv>[~lib:…]/<pfad>, /~a/lib:<name>@<M.m>[~lib:…]/<pfad>
//                          → Instanz-URLs wie im echten Ladeweg (Shell-Quellbaum bzw. Bibliotheksordner), Umschreibung
//                            ctox:shared/ und ctox:lib/ durch die Funktionen von v4/sw.js (v4/lab/sw-lab.mjs, MONACO.md)
import http from 'node:http';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import os from 'node:os';
import { resolveInstance, instanceBody } from '../../lab/sw-lab.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const BOS = path.resolve(HERE, '../../..');
export const STUB = process.env.CTOX_APP_API_STUB || path.join(os.homedir(), '.local/state/shell-v4/stub-pins/68b83a3');
const TYPES = { '.woff2': 'font/woff2', '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html; charset=utf-8', '.json': 'application/json', '.svg': 'image/svg+xml', '.ttf': 'font/ttf' };

function resolve(urlPath) {
  let m;
  if (urlPath === '/' || urlPath === '/index.html') return { file: path.join(HERE, 'index.html'), base: HERE };
  if ((m = /^\/stub\/((?:testkit\/)?[\w.-]+)$/.exec(urlPath))) return { file: path.join(STUB, m[1]), base: STUB };
  if ((m = /^\/business-os\/(.+)$/.exec(urlPath))) return { file: path.join(BOS, m[1]), base: BOS };
  return null;
}

export function startServer({ port = 0 } = {}) {
  const server = http.createServer(async (req, res) => {
    const urlPath = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    const inst = resolveInstance(urlPath, BOS);
    if (inst) {
      const body = instanceBody(inst, await readFile(inst.file));
      res.writeHead(200, { 'content-type': TYPES[path.extname(inst.file)] || 'application/octet-stream', 'cache-control': 'no-store' });
      res.end(req.method === 'HEAD' ? undefined : body);
      return;
    }
    if (urlPath.startsWith('/~a/')) { res.writeHead(404); res.end(); return; }
    const r = resolve(urlPath);
    if (!r || !path.resolve(r.file).startsWith(r.base)) { res.writeHead(404); res.end(); return; }
    try {
      const body = await readFile(r.file);
      res.writeHead(200, { 'content-type': TYPES[path.extname(r.file)] || 'application/octet-stream', 'cache-control': 'no-store' });
      res.end(req.method === 'HEAD' ? undefined : body);
    } catch { res.writeHead(404); res.end(); }
  });
  return new Promise((ok) => server.listen(port, '127.0.0.1', () => ok({ server, url: `http://127.0.0.1:${server.address().port}` })));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { url } = await startServer({ port: Number(process.env.PORT || 8791) });
  console.log('Labor Quelltext-Modus:', url, 'Stub:', STUB);
}
