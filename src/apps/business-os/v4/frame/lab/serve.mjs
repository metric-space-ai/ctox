// Statischer Labor-Server. Wurzel = src/apps/business-os (damit ../../shared/ aufloest).
// /~a/<app>@<ver>/<pfad> -> v4/frame/lab/apps/<app>/<ver>/<pfad>  (steht fuer den Service Worker von P1).
// /~a/shell@<sv>[~lib:…]/<pfad> und /~a/lib:<name>@<M.m>[~lib:…]/<pfad> -> Shell-Quellbaum bzw. Bibliotheksordner, mit der
// Umschreibung des SW (ctox:shared/, ctox:lib/ nach der Bindung der URL; v4/lab/sw-lab.mjs, v4/source/MONACO.md).
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolveInstance, instanceBody } from '../../lab/sw-lab.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../../..');
const appsDir = path.join(here, 'apps');
// Kern-Stub (read-only, nicht geaendert) unter /~stub/
// Eingefrorener Stand (Pin 67229d2, §16.19); abweichend per Umgebungsvariable CTOX_APP_API_STUB.
const stubDir = path.resolve(process.env.CTOX_APP_API_STUB || path.join(process.env.HOME || '', '.local/state/shell-v4/stub-pins/68b83a3'));
const types = { '.ttf': 'font/ttf', '.woff2': 'font/woff2', '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.svg': 'image/svg+xml', '.json': 'application/json', '.png': 'image/png' };

export function startServer(port = 0) {
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, 'http://lab');
    const inst = resolveInstance(decodeURIComponent(url.pathname), root);
    if (inst) {
      res.writeHead(200, { 'content-type': types[path.extname(inst.file)] || 'application/octet-stream', 'cache-control': 'no-store' });
      res.end(req.method === 'HEAD' ? undefined : instanceBody(inst));
      return;
    }
    if (/^\/~a\/(?:shell@|lib:)/.test(url.pathname)) { res.writeHead(404); res.end(); return; }
    let file;
    const m = /^\/~a\/([^/@]+)@([^/~]+)(?:~[^/]*)?\/(.+)$/.exec(decodeURIComponent(url.pathname));
    const st = /^\/~stub\/([\w.-]+\.(?:mjs|js))$/.exec(url.pathname);
    if (m) file = path.join(appsDir, m[1], m[2], m[3]);
    else if (st) file = path.join(stubDir, st[1]);
    else file = path.join(root, decodeURIComponent(url.pathname));
    if (!(file.startsWith(root) || (st && file.startsWith(stubDir))) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) { res.writeHead(404); res.end(); return; }
    res.writeHead(200, { 'content-type': types[path.extname(file)] || 'application/octet-stream', 'cache-control': 'no-store' });
    fs.createReadStream(file).pipe(res);
  });
  return new Promise((resolve) => server.listen(port, '127.0.0.1', () => resolve(server)));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const s = await startServer(Number(process.argv[2] || 8790));
  console.log(`http://127.0.0.1:${s.address().port}/v4/frame/lab/index.html`);
}
