"use strict";

const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");
const { app, BrowserWindow } = require("electron");
const { sessionPartitionFor } = require("../../src/common/instance-model.cjs");

const outputPath = process.argv[2];
const userDataPath = process.argv[3];

if (!outputPath || !userDataPath) {
  throw new Error("usage: electron session-isolation-main.cjs <outputPath> <userDataPath>");
}

fs.mkdirSync(userDataPath, { recursive: true });
app.setPath("userData", userDataPath);
app.commandLine.appendSwitch("disable-gpu");

app.whenReady().then(async () => {
  const server = await startServer();
  const windows = [];
  let exitCode = 0;
  try {
    const baseUrl = `http://127.0.0.1:${server.address().port}/`;
    const alpha = {
      id: "local-alpha",
      source: "local_daemon",
      displayName: "Alpha",
    };
    const beta = {
      id: "local-beta",
      source: "local_daemon",
      displayName: "Beta",
    };
    const alphaWindow = await withTimeout(loadWindow(baseUrl, sessionPartitionFor(alpha)), "load alpha window");
    const betaWindow = await withTimeout(loadWindow(baseUrl, sessionPartitionFor(beta)), "load beta window");
    windows.push(alphaWindow, betaWindow);
    const alphaWrite = await withTimeout(writeAndRead(alphaWindow.webContents, "alpha"), "write alpha state");
    const betaWrite = await withTimeout(writeAndRead(betaWindow.webContents, "beta"), "write beta state");
    const alphaRead = await withTimeout(readState(alphaWindow.webContents, "alpha"), "read alpha state");
    const betaRead = await withTimeout(readState(betaWindow.webContents, "beta"), "read beta state");
    const result = {
      ok: alphaRead.localStorage === "alpha"
        && betaRead.localStorage === "beta"
        && alphaRead.indexedDb === "alpha"
        && betaRead.indexedDb === "beta"
        && alphaRead.cookie.includes("ctoxSmoke=alpha")
        && betaRead.cookie.includes("ctoxSmoke=beta"),
      partitions: [sessionPartitionFor(alpha), sessionPartitionFor(beta)],
      alphaWrite,
      betaWrite,
      alphaRead,
      betaRead,
    };
    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.writeFileSync(outputPath, `${JSON.stringify(result, null, 2)}\n`);
    exitCode = result.ok ? 0 : 2;
  } catch (error) {
    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.writeFileSync(outputPath, JSON.stringify({
      ok: false,
      error: error instanceof Error ? error.stack || error.message : String(error),
    }, null, 2));
    exitCode = 1;
  } finally {
    for (const window of windows) {
      if (!window.isDestroyed()) window.destroy();
    }
    await closeServer(server);
    process.exit(exitCode);
  }
});

function startServer() {
  const server = http.createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end("<!doctype html><title>CTOX Session Isolation Smoke</title><main>ready</main>");
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

async function loadWindow(url, partition) {
  const window = new BrowserWindow({
    show: false,
    width: 400,
    height: 300,
    webPreferences: {
      partition,
      contextIsolation: true,
      nodeIntegration: false,
    },
  });
  await window.loadURL(url);
  return window;
}

function writeAndRead(webContents, value) {
  return webContents.executeJavaScript(`
    (async () => {
      const value = ${JSON.stringify(value)};
      document.cookie = "ctoxSmoke=" + value + "; path=/; SameSite=Lax";
      localStorage.setItem("ctoxSmoke", value);
      await new Promise((resolve, reject) => {
        const request = indexedDB.open("ctoxSmokeDb", 1);
        request.onupgradeneeded = () => request.result.createObjectStore("kv");
        request.onerror = () => reject(request.error);
        request.onsuccess = () => {
          const db = request.result;
          const tx = db.transaction("kv", "readwrite");
          tx.objectStore("kv").put(value, "value");
          tx.oncomplete = () => {
            db.close();
            resolve();
          };
          tx.onerror = () => reject(tx.error);
        };
      });
      return (${readStateScript})(value);
    })();
  `, true);
}

function readState(webContents, expected = "") {
  return webContents.executeJavaScript(`(${readStateScript})(${JSON.stringify(expected)});`, true);
}

function withTimeout(promise, label, timeoutMs = 5000) {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error(`${label} timed out`)), timeoutMs);
    Promise.resolve(promise).then(
      (value) => {
        clearTimeout(timeout);
        resolve(value);
      },
      (error) => {
        clearTimeout(timeout);
        reject(error);
      },
    );
  });
}

const readStateScript = `async (expected = "") => {
  const readOnce = async () => {
    const indexedDb = await new Promise((resolve, reject) => {
      const request = indexedDB.open("ctoxSmokeDb", 1);
      request.onupgradeneeded = () => request.result.createObjectStore("kv");
      request.onerror = () => reject(request.error);
      request.onsuccess = () => {
        const db = request.result;
        const tx = db.transaction("kv", "readonly");
        const get = tx.objectStore("kv").get("value");
        get.onsuccess = () => {
          db.close();
          resolve(get.result || "");
        };
        get.onerror = () => reject(get.error);
      };
    });
    return {
      cookie: document.cookie,
      localStorage: localStorage.getItem("ctoxSmoke") || "",
      indexedDb,
    };
  };
  const ready = (state) => !expected || (
    state.cookie.includes("ctoxSmoke=" + expected)
    && state.localStorage === expected
    && state.indexedDb === expected
  );
  const deadline = Date.now() + 5000;
  let latest = await readOnce();
  while (!ready(latest) && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 50));
    latest = await readOnce();
  }
  return latest;
}`;
