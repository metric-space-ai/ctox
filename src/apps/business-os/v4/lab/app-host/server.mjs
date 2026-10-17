// Labor-Server App-Host (nur statische Dateien, keine Daten-Endpunkte).
//   /                          → lab/app-host/index.html
//   /lab/<pfad>                → lab/app-host/<pfad>
//   /~a/shell@<ver>/<pfad>     → src/apps/business-os/<pfad> (Einstieg v4/frame/index.js; alle Versionen = derselbe Quellstand, eigene URL)
//   /~a/<app>@<ver>/<pfad>     → lab/app-host/apps/<app>/v<ver>/<pfad>
// Versionierte Pfade (/~a/…) sind unveränderlich (Cache-Control: immutable), wie im Service-Worker-Ladeweg (§4).
import http from 'node:http';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const HERE = path.dirname(fileURLToPath(import.meta.url));
// Shell-Artefakt = Quellbaum src/apps/business-os (Rahmen v4/frame, Host v4/shell, shared/), je Version eigene URL.
const SHELL = path.resolve(HERE, '../../..');
// Kern-Stub: eingefrorener Stand (Pin 67229d2, §16.19); abweichend per Umgebungsvariable CTOX_APP_API_STUB.
const STUB = path.resolve(process.env.CTOX_APP_API_STUB || path.join(process.env.HOME, '.local/state/shell-v4/stub-pins/68b83a3'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html; charset=utf-8', '.json': 'application/json' };

function resolve(urlPath) {
  let m;
  if (urlPath === '/' || urlPath === '/index.html') return { file: path.join(HERE, 'index.html'), immutable: false };
  // Kern-Stub (nur lesend ausgeliefert, Eigentum „ctox sync v4“): /stub/<datei>
  if ((m = /^\/stub\/([\w.-]+)$/.exec(urlPath))) return { file: path.join(STUB, m[1]), immutable: false, base: STUB };
  if ((m = /^\/lab\/(.+)$/.exec(urlPath))) return { file: path.join(HERE, m[1]), immutable: false, base: HERE };
  if ((m = /^\/~a\/shell@([\w.-]+)\/(.+)$/.exec(urlPath))) return { file: path.join(SHELL, m[2]), immutable: true, base: SHELL };
  if ((m = /^\/~a\/([\w-]+)@([\w.-]+)(?:~[^/]*)?\/(.+)$/.exec(urlPath))) { // ~shell@…/~lib:…-Bindung (artifacts.bind) wird ignoriert
    const base = path.join(HERE, 'apps', m[1], `v${m[2]}`);
    return { file: path.join(base, m[3]), immutable: true, base };
  }
  return null;
}

export function startServer({ port = 0, log = () => {} } = {}) {
  const server = http.createServer(async (req, res) => {
    const urlPath = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    const r = resolve(urlPath);
    log(req.method, urlPath, r ? 'ok' : '404');
    if (!r || (r.base && !path.resolve(r.file).startsWith(r.base))) { res.writeHead(404); res.end(); return; }
    try {
      const body = await readFile(r.file);
      res.writeHead(200, {
        'content-type': TYPES[path.extname(r.file)] || 'application/octet-stream',
        'cache-control': r.immutable ? 'public, max-age=31536000, immutable' : 'no-store',
      });
      res.end(body);
    } catch { res.writeHead(404); res.end(); }
  });
  return new Promise((ok) => server.listen(port, '127.0.0.1', () => ok({ server, url: `http://127.0.0.1:${server.address().port}` })));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { url } = await startServer({ port: Number(process.env.PORT || 8790), log: (...a) => console.log(...a) });
  console.log('Labor App-Host:', url);
}
