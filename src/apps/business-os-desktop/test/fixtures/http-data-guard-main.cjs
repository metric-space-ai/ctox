"use strict";

const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");
const { app, BrowserView, BrowserWindow } = require("electron");
const {
  createInstanceBrowserView,
} = require("../../src/main/session-view.cjs");
const {
  isAllowedBusinessOsNavigation,
  isForbiddenBusinessOsHttpDataRequest,
  scrubCtoxConfigFromWebContents,
} = require("../../src/main/url-safety.cjs");

const outputPath = process.argv[2];
const userDataPath = process.argv[3];

if (!outputPath || !userDataPath) {
  throw new Error("usage: electron http-data-guard-main.cjs <outputPath> <userDataPath>");
}

fs.mkdirSync(userDataPath, { recursive: true });
app.setPath("userData", userDataPath);
app.commandLine.appendSwitch("disable-gpu");

app.whenReady().then(async () => {
  const serverState = { requests: [] };
  const server = await startServer(serverState);
  let window;
  let exitCode = 0;
  try {
    const baseUrl = `http://127.0.0.1:${server.address().port}`;
    const launch = {
      launchUrl: `${baseUrl}/business-os/?ctox_config=bootstrap-secret`,
      ctoxConfig: {
        transport: "webrtc",
        http_bridge_available: false,
      },
    };
    window = new BrowserWindow({
      show: false,
      width: 900,
      height: 700,
      webPreferences: {
        contextIsolation: true,
        nodeIntegration: false,
      },
    });
    const view = createInstanceBrowserView({
      BrowserView,
      instance: {
        id: "local-http-guard",
        source: "local_daemon",
        displayName: "HTTP Guard",
        sessionPartition: "persist:ctox-http-guard-smoke",
      },
      launch,
      shell: { openExternal: async () => undefined },
      scrubCtoxConfigFromWebContents,
      isAllowedBusinessOsNavigation,
      isForbiddenBusinessOsHttpDataRequest,
    });
    window.addBrowserView(view);
    view.setBounds({ x: 0, y: 0, width: 900, height: 700 });
    await view.webContents.loadURL(launch.launchUrl);
    const result = await waitForResult(view);
    const blockedPaths = ["/api/business-os/records", "/rxdb/pull", "/commands"];
    const resultOk = result.status.ok
      && result.status.status === 200
      && result["/rxdb/dist/ctox-rxdb-js.mjs"].ok === true
      && blockedPaths.every((entry) => result[entry].ok === false)
      && serverState.requests.includes("/api/business-os/status")
      && serverState.requests.includes("/rxdb/dist/ctox-rxdb-js.mjs")
      && blockedPaths.every((entry) => !serverState.requests.includes(entry))
      && !view.webContents.getURL().includes("ctox_config=");
    writeResult({
      ok: resultOk,
      result,
      requests: serverState.requests,
      finalUrl: view.webContents.getURL(),
    });
    exitCode = resultOk ? 0 : 2;
  } catch (error) {
    writeResult({
      ok: false,
      error: error instanceof Error ? error.stack || error.message : String(error),
      requests: serverState.requests,
    });
    exitCode = 1;
  } finally {
    if (window && !window.isDestroyed()) window.destroy();
    await closeServer(server);
    process.exit(exitCode);
  }
});

function startServer(state) {
  const server = http.createServer((request, response) => {
    const url = new URL(request.url, "http://127.0.0.1");
    state.requests.push(url.pathname);
    if (url.pathname === "/business-os/") {
      response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      response.end(`<!doctype html>
<title>CTOX HTTP Data Guard Smoke</title>
<script>
async function attempt(path, options) {
  try {
    const response = await fetch(path, options || {});
    return { ok: true, status: response.status, text: await response.text() };
  } catch (error) {
    return { ok: false, error: String(error && error.message || error) };
  }
}
(async () => {
  window.__ctoxHttpDataGuardResult = {
    status: await attempt("/api/business-os/status"),
    "/rxdb/dist/ctox-rxdb-js.mjs": await attempt("/rxdb/dist/ctox-rxdb-js.mjs"),
    "/api/business-os/records": await attempt("/api/business-os/records"),
    "/rxdb/pull": await attempt("/rxdb/pull"),
    "/commands": await attempt("/commands", { method: "POST", body: "{}" }),
  };
})();
</script>`);
      return;
    }
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ ok: true, path: url.pathname }));
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve(server));
  });
}

function closeServer(server) {
  return new Promise((resolve, reject) => {
    if (typeof server.closeAllConnections === "function") {
      server.closeAllConnections();
    }
    const timeout = setTimeout(resolve, 1000);
    server.close((error) => {
      clearTimeout(timeout);
      if (error) {
        reject(error);
        return;
      }
      resolve();
    });
  });
}

function waitForResult(view, timeoutMs = 10000) {
  const startedAt = Date.now();
  return new Promise((resolve, reject) => {
    async function poll() {
      try {
        const result = await view.webContents.executeJavaScript("window.__ctoxHttpDataGuardResult || null", true);
        if (result) {
          resolve(result);
          return;
        }
      } catch (error) {
        reject(error);
        return;
      }
      if (Date.now() - startedAt > timeoutMs) {
        reject(new Error("HTTP data guard result timed out"));
        return;
      }
      setTimeout(poll, 100);
    }
    poll();
  });
}

function writeResult(result) {
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, `${JSON.stringify(result, null, 2)}\n`);
}
