"use strict";

const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");

const MIME_TYPES = new Map([
  [".css", "text/css; charset=utf-8"],
  [".html", "text/html; charset=utf-8"],
  [".ico", "image/x-icon"],
  [".jpeg", "image/jpeg"],
  [".jpg", "image/jpeg"],
  [".js", "text/javascript; charset=utf-8"],
  [".json", "application/json; charset=utf-8"],
  [".mjs", "text/javascript; charset=utf-8"],
  [".png", "image/png"],
  [".svg", "image/svg+xml"],
  [".wasm", "application/wasm"],
  [".woff", "font/woff"],
  [".woff2", "font/woff2"],
]);

function businessOsShellRoot({ isPackaged = false, resourcesPath = "", appDir = "" } = {}) {
  if (isPackaged) return path.join(resourcesPath, "business-os");
  return path.resolve(appDir || __dirname, "../../../business-os");
}

function startBundledBusinessOsShell({ root }) {
  const canonicalRoot = fs.realpathSync(String(root || ""));
  // Managed launch configs include ICE/TURN and native-peer diagnostics and can
  // exceed Node's 16 KiB default request-line/header limit. This server binds
  // loopback only and serves static files, so allow a bounded 64 KiB envelope.
  const server = http.createServer({ maxHeaderSize: 64 * 1024 }, (request, response) => {
    if (!request.url || !["GET", "HEAD"].includes(request.method || "")) {
      response.writeHead(405).end();
      return;
    }
    let pathname;
    try {
      pathname = decodeURIComponent(new URL(request.url, "http://127.0.0.1").pathname);
    } catch (_error) {
      response.writeHead(400).end();
      return;
    }
    const withoutPrefix = pathname.replace(/^\/business-os(?:\/|$)/, "/");
    const relative = withoutPrefix === "/" ? "index.html" : withoutPrefix.replace(/^\/+/, "");
    const candidate = path.resolve(canonicalRoot, relative);
    if (candidate !== canonicalRoot && !candidate.startsWith(`${canonicalRoot}${path.sep}`)) {
      response.writeHead(403).end();
      return;
    }
    fs.stat(candidate, (error, stat) => {
      if (error || !stat.isFile()) {
        response.writeHead(404).end();
        return;
      }
      response.writeHead(200, {
        "cache-control": "no-store",
        "content-length": stat.size,
        "content-type": MIME_TYPES.get(path.extname(candidate).toLowerCase()) || "application/octet-stream",
        "x-content-type-options": "nosniff",
      });
      if (request.method === "HEAD") {
        response.end();
        return;
      }
      fs.createReadStream(candidate).pipe(response);
    });
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      server.removeListener("error", reject);
      const address = server.address();
      resolve({
        root: canonicalRoot,
        url: `http://127.0.0.1:${address.port}/`,
        close: () => new Promise((done) => server.close(() => done())),
      });
    });
  });
}

module.exports = { businessOsShellRoot, startBundledBusinessOsShell };
