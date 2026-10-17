// Statischer Laborserver (127.0.0.1, zufaelliger Port). Liefert NUR die Positivliste:
// Laborseite, Loader-Attrappe (lab.js, kernel-attrappe.js, blob-loader.js) und /sw.js.
// Alles unter /~a/ muss aus dem Service Worker kommen: der Server antwortet dort 410 und zaehlt
// die Treffer, damit jeder Netz-Rueckgriff auffaellt.
import http from 'node:http';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const v4 = path.resolve(here, '../..');
const ALLOW = {
  '/': [path.join(here, 'index.html'), 'text/html; charset=utf-8'],
  '/lab.js': [path.join(here, 'lab.js'), 'text/javascript; charset=utf-8'],
  '/kernel-attrappe.js': [path.join(here, 'kernel-attrappe.js'), 'text/javascript; charset=utf-8'],
  '/blob-loader.js': [path.join(v4, 'blob-loader.js'), 'text/javascript; charset=utf-8'],
  '/sw.js': [path.join(v4, 'sw.js'), 'text/javascript; charset=utf-8'],
  '/monaco.html': [path.join(here, 'monaco.html'), 'text/html; charset=utf-8'],
  '/monaco-lab.js': [path.join(here, 'monaco-lab.js'), 'text/javascript; charset=utf-8'],
};
// Monaco-Labor (monaco.mjs): /~stub/<datei> = Kern-Stub (createStubKernel, artifacts.bind, source); /__spur4/<pfad> =
// Ersatz fuer CTOX Sync Spur 4: der Labor-Kernel holt Shell-Dateien und einzelne Bibliotheksdateien hierueber und legt
// sie in seine IDB. Der SW sieht diese Pfade nie (nur /~a/ ist seiner), App-/Lib-Code kommt weiter nur aus IDB.
const BOS = path.resolve(v4, '..');
const STUB = path.resolve(process.env.CTOX_APP_API_STUB || path.join(process.env.HOME || '', '.local/state/shell-v4/stub-pins/8489b4e'));
const SPUR4_ROOTS = ['vendor/monaco/', 'v4/manifests/', 'v4/source/', 'v4/shell/', 'v4/frame/', 'shared/'];
const TYPES = { '.js': 'text/javascript; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8', '.json': 'application/json', '.ttf': 'font/ttf' };
function prefixFile(p) {
  let m;
  if ((m = /^\/~stub\/((?:testkit\/)?[\w.-]+)$/.exec(p))) return path.join(STUB, m[1]);
  if ((m = /^\/__spur4\/(.+)$/.exec(p))) {
    const rel = decodeURIComponent(m[1]);
    const f = path.resolve(BOS, rel);
    if (!f.startsWith(BOS + path.sep) || !SPUR4_ROOTS.some((r) => rel.startsWith(r))) return null;
    return f;
  }
  return null;
}

export async function startServer() {
  const stats = { artifactHits: 0, artifactPaths: [], otherMisses: 0, served: 0 };
  const server = http.createServer(async (req, res) => {
    const p = new URL(req.url, 'http://x').pathname;
    if (p.startsWith('/~a/')) { stats.artifactHits++; stats.artifactPaths.push(p); res.writeHead(410).end(); return; }
    const pf = prefixFile(p);
    if (pf) {
      try {
        const body = await readFile(pf);
        stats.spur4 = (stats.spur4 || 0) + 1;
        res.writeHead(200, { 'Content-Type': TYPES[path.extname(pf)] || 'application/octet-stream', 'Cache-Control': 'no-store' });
        res.end(body);
      } catch { stats.otherMisses++; res.writeHead(404).end(); }
      return;
    }
    const hit = ALLOW[p];
    if (!hit) { stats.otherMisses++; res.writeHead(404).end(); return; }
    stats.served++;
    res.writeHead(200, { 'Content-Type': hit[1], 'Cache-Control': 'no-store', 'Service-Worker-Allowed': '/' });
    res.end(await readFile(hit[0]));
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const { port } = server.address();
  return {
    url: `http://127.0.0.1:${port}`,
    stats,
    async stop() { server.closeAllConnections?.(); await new Promise((r) => server.close(r)); },
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const s = await startServer();
  console.log(s.url);
}
