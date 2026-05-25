#!/usr/bin/env node
/*
 * Browser/Rust RxDB WebRTC smoke test for CTOX Business OS.
 *
 * Defaults to the isolated smoke page and browser-to-rust mode:
 *   node src/core/rxdb/tools/browser_rust_smoke.js
 *
 * Useful variants:
 *   SMOKE_MODE=rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-update-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-large-materialize-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-large-file-viewer-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-large-file-viewer-restart-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=command-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=command-burst-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=command-restart-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=command-midflight-restart-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=rollover-native-peer-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=tab-freeze-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=network-flap-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=restart-signaling-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=signaling-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=checkpoint-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=schema-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=replication-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-update-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-large-materialize-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-large-file-viewer-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-large-file-viewer-restart-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=command-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=command-burst-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=command-restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=command-midflight-restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=rollover-native-peer-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=tab-freeze-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=network-flap-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=restart-signaling-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=signaling-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=checkpoint-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=schema-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=replication-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 */
const net = require('net');
const path = require('path');
const crypto = require('crypto');
const fs = require('fs');
const os = require('os');
const { spawn, spawnSync } = require('child_process');

const root = path.resolve(__dirname, '../../../..');
const runtimeRoot = process.env.CTOX_SMOKE_ROOT || fs.mkdtempSync(path.join(os.tmpdir(), 'ctox-rxdb-smoke-'));
prepareSmokeRoot(runtimeRoot);
const playwrightModule =
  process.env.PLAYWRIGHT_MODULE_PATH ||
  (() => {
    const candidates = [
      'playwright',
      '/tmp/ctox-pw-smoke/node_modules/playwright',
    ];
    for (const candidate of candidates) {
      try {
        return require.resolve(candidate);
      } catch {
        // Try the next known browser automation runtime.
      }
    }
    throw new Error(
      'No Playwright runtime found. Install playwright in this checkout or set PLAYWRIGHT_MODULE_PATH.'
    );
  })();
const { chromium } = require(playwrightModule);

function existingChromeExecutable() {
  if (process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE) return process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE;
  const candidates = [
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/usr/bin/google-chrome',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
  ];
  return candidates.find((candidate) => fs.existsSync(candidate));
}

function chromiumLaunchOptions() {
  const executablePath = existingChromeExecutable();
  return executablePath
    ? { headless: true, executablePath }
    : { headless: true };
}

const ctoxBin = process.env.CTOX_BIN || path.join(root, 'runtime/build/core-rxdb-integration-target/debug/ctox');
const businessPort = parsePositiveIntegerEnv('BUSINESS_PORT', process.env.BUSINESS_PORT || '8877', { max: 65535 });
const signalingPort = parsePositiveIntegerEnv('SIGNALING_PORT', process.env.SIGNALING_PORT || '18876', { max: 65535 });
const signalingUrl = `ws://127.0.0.1:${signalingPort}`;
const signalingDebug = process.env.SIGNALING_DEBUG === '1';
const sqlitePath = process.env.CTOX_SQLITE || path.join(runtimeRoot, 'runtime/business-os-rxdb.sqlite3');
const pagePath = process.env.SMOKE_PAGE_PATH || '/__rxdb_smoke__.html';
const smokeMode = process.env.SMOKE_MODE || 'browser-to-rust';
const useAppDb = process.env.SMOKE_USE_APP_DB === '1' || /^\/index\.html(?:[?#]|$)/.test(pagePath);
const hasOwn = (object, key) => Object.prototype.hasOwnProperty.call(object, key);

if (![
  'browser-to-rust',
  'rust-to-browser',
  'workspace-rust-to-browser',
  'workspace-update-rust-to-browser',
  'workspace-large-materialize-rust-to-browser',
  'workspace-large-file-viewer-rust-to-browser',
  'workspace-large-file-viewer-restart-rust-to-browser',
  'command-browser-to-rust',
  'command-burst-browser-to-rust',
  'command-restart-browser-to-rust',
  'command-midflight-restart-browser-to-rust',
  'rollover-native-peer-browser-to-rust',
  'tab-freeze-browser-to-rust',
  'network-flap-browser-to-rust',
  'restart-browser-to-rust',
  'restart-signaling-browser-to-rust',
  'signaling-error-browser-status',
  'checkpoint-error-browser-status',
  'schema-error-browser-status',
  'replication-error-browser-status',
].includes(smokeMode)) {
  throw new Error(`Unsupported SMOKE_MODE=${smokeMode}`);
}
if (['signaling-error-browser-status', 'checkpoint-error-browser-status', 'schema-error-browser-status', 'replication-error-browser-status'].includes(smokeMode) && !useAppDb) {
  throw new Error(`SMOKE_MODE=${smokeMode} requires SMOKE_PAGE_PATH=/index.html`);
}

function parsePositiveIntegerEnv(name, value, options = {}) {
  const parsed = Number(value);
  const min = options.min ?? 1;
  const max = options.max ?? Number.MAX_SAFE_INTEGER;
  if (!Number.isInteger(parsed) || parsed < min || parsed > max) {
    throw new Error(`${name} must be an integer between ${min} and ${max}; got ${JSON.stringify(String(value))}`);
  }
  return parsed;
}

function token(len = 12) {
  const alphabet = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';
  let out = '';
  for (let i = 0; i < len; i++) out += alphabet[Math.floor(Math.random() * alphabet.length)];
  return out;
}

function prepareSmokeRoot(targetRoot) {
  fs.mkdirSync(path.join(targetRoot, 'runtime'), { recursive: true });
  for (const entry of ['Cargo.toml', 'src', 'contracts']) {
    const target = path.join(targetRoot, entry);
    if (fs.existsSync(target)) continue;
    fs.symlinkSync(path.join(root, entry), target, entry === 'Cargo.toml' ? 'file' : 'dir');
  }
}

function encodeFrame(text) {
  const payload = Buffer.from(text);
  let header;
  if (payload.length < 126) {
    header = Buffer.from([0x81, payload.length]);
  } else if (payload.length < 65536) {
    header = Buffer.alloc(4);
    header[0] = 0x81;
    header[1] = 126;
    header.writeUInt16BE(payload.length, 2);
  } else {
    header = Buffer.alloc(10);
    header[0] = 0x81;
    header[1] = 127;
    header.writeBigUInt64BE(BigInt(payload.length), 2);
  }
  return Buffer.concat([header, payload]);
}

function tryDecodeFrame(buffer) {
  if (buffer.length < 2) return null;
  const opcode = buffer[0] & 0x0f;
  let len = buffer[1] & 0x7f;
  let offset = 2;
  if (len === 126) {
    if (buffer.length < 4) return null;
    len = buffer.readUInt16BE(2);
    offset = 4;
  } else if (len === 127) {
    if (buffer.length < 10) return null;
    const big = buffer.readBigUInt64BE(2);
    if (big > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error('frame too large');
    len = Number(big);
    offset = 10;
  }
  const masked = (buffer[1] & 0x80) !== 0;
  const maskOffset = offset;
  if (masked) offset += 4;
  if (buffer.length < offset + len) return null;
  let payload = buffer.subarray(offset, offset + len);
  if (masked) {
    const mask = buffer.subarray(maskOffset, maskOffset + 4);
    const unmasked = Buffer.alloc(len);
    for (let i = 0; i < len; i++) unmasked[i] = payload[i] ^ mask[i % 4];
    payload = unmasked;
  }
  return { opcode, text: payload.toString('utf8'), rest: buffer.subarray(offset + len) };
}

function startSignalingServer() {
  const peers = new Map();
  const rooms = new Map();
  const sockets = new Set();
  const server = net.createServer((socket) => {
    sockets.add(socket);
    let handshake = false;
    let buffer = Buffer.alloc(0);
    let peer = null;

    function send(message) {
      if (!socket.destroyed) socket.write(encodeFrame(JSON.stringify(message)));
    }

    function joined(roomId) {
      const room = rooms.get(roomId) || new Set();
      if (signalingDebug) console.error(`[smoke-signaling] joined room=${roomId} peers=${room.size}`);
      for (const id of room) {
        peers.get(id)?.send({ type: 'joined', otherPeerIds: Array.from(room) });
      }
    }

    function disconnect() {
      if (!peer) return;
      for (const roomId of peer.rooms) {
        const room = rooms.get(roomId);
        room?.delete(peer.id);
        if (room && room.size === 0) rooms.delete(roomId);
        else joined(roomId);
      }
      peers.delete(peer.id);
      peer = null;
    }

    socket.on('data', (chunk) => {
      buffer = Buffer.concat([buffer, chunk]);
      if (!handshake) {
        const headerEnd = buffer.indexOf('\r\n\r\n');
        if (headerEnd === -1) return;
        const header = buffer.subarray(0, headerEnd).toString('utf8');
        const key = /^Sec-WebSocket-Key: (.+)$/im.exec(header)?.[1]?.trim();
        if (!key) return socket.destroy();
        const accept = crypto
          .createHash('sha1')
          .update(key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11')
          .digest('base64');
        socket.write([
          'HTTP/1.1 101 Switching Protocols',
          'Upgrade: websocket',
          'Connection: Upgrade',
          `Sec-WebSocket-Accept: ${accept}`,
          '\r\n',
        ].join('\r\n'));
        handshake = true;
        buffer = buffer.subarray(headerEnd + 4);
        peer = { id: token(), rooms: new Set(), send, injectedControlPlaneError: false };
        peers.set(peer.id, peer);
        if (signalingDebug) console.error(`[smoke-signaling] open peer=${peer.id}`);
        send({ type: 'init', yourPeerId: peer.id });
      }

      while (true) {
        const decoded = tryDecodeFrame(buffer);
        if (!decoded) break;
        buffer = decoded.rest;
        if (decoded.opcode === 8) {
          socket.end();
          break;
        }
        if (decoded.opcode !== 1) continue;
        let msg;
        try {
          msg = JSON.parse(decoded.text);
        } catch {
          socket.destroy();
          break;
        }
        if (msg.type === 'join') {
          if (typeof msg.room !== 'string' || msg.room.length <= 5 || msg.room.length >= 512) {
            socket.destroy();
            break;
          }
          peer.rooms.add(msg.room);
          if (!rooms.has(msg.room)) rooms.set(msg.room, new Set());
          rooms.get(msg.room).add(peer.id);
          if (signalingDebug) console.error(`[smoke-signaling] join peer=${peer.id} room=${msg.room} size=${rooms.get(msg.room).size}`);
          if (smokeMode === 'signaling-error-browser-status' && !peer.injectedControlPlaneError) {
            peer.injectedControlPlaneError = true;
            send({
              type: 'ctoxError',
              scope: 'control-plane',
              code: 'instance_mismatch',
              reason: 'smoke injected control-plane instance mismatch',
            });
          }
          joined(msg.room);
        } else if (msg.type === 'signal') {
          if (msg.senderPeerId !== peer.id) {
            socket.destroy();
            break;
          }
          if (signalingDebug) console.error(`[smoke-signaling] signal from=${peer.id} to=${msg.receiverPeerId} receiver=${peers.has(msg.receiverPeerId) ? 'yes' : 'no'} room=${msg.room}`);
          peers.get(msg.receiverPeerId)?.send(msg);
        } else if (msg.type !== 'ping') {
          socket.destroy();
          break;
        }
      }
    });
    socket.on('close', () => {
      sockets.delete(socket);
      disconnect();
    });
    socket.on('error', () => {
      sockets.delete(socket);
      disconnect();
    });
  });
  server.closeAllSockets = () => {
    for (const socket of sockets) socket.destroy();
    sockets.clear();
  };

  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(signalingPort, '127.0.0.1', () => resolve(server));
  });
}

async function stopSignalingServer(server) {
  if (!server) return;
  server.closeAllSockets?.();
  await withHostTimeout(new Promise((resolve) => server.close(() => resolve())), 5000);
}

function withHostTimeout(promise, ms) {
  return Promise.race([
    Promise.resolve(promise),
    new Promise((resolve) => setTimeout(resolve, ms)),
  ]);
}

async function waitForHttp(url, ms = 20000) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    if (globalThis.__ctoxProcess && globalThis.__ctoxProcess.exitCode !== null) {
      throw new Error(`ctox exited before ${url}: code=${globalThis.__ctoxProcess.exitCode}`);
    }
    try {
      const res = await fetch(url);
      if (res.ok) return await res.json();
    } catch {
      // Retry until deadline.
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`timeout waiting for ${url}`);
}

function sqlite(statement) {
  const result = spawnSync('/usr/bin/sqlite3', [sqlitePath, statement], { encoding: 'utf8' });
  if (result.status !== 0) {
    throw new Error(`sqlite failed: ${result.stderr || result.stdout}`);
  }
  return result.stdout;
}

async function waitForSqliteTables(tableNames, ms = 30000) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    try {
      const rows = sqlite("SELECT name FROM sqlite_master WHERE type='table';")
        .split(/\r?\n/)
        .map((name) => name.trim())
        .filter(Boolean);
      if (tableNames.every((name) => rows.includes(name))) return;
    } catch {
      // The native peer may still be creating the SQLite database.
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`timeout waiting for sqlite tables: ${tableNames.join(', ')}`);
}

function pollSqliteFileAndChunk(id, ms = 30000) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const fileRow = sqlite(`SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id='${sqlString(id)}' LIMIT 1;`).trim();
    const chunkRow = sqlite(`SELECT data FROM ctox_business_os__desktop_file_chunks__v0 WHERE id='${sqlString(`${id}_0`)}' LIMIT 1;`).trim();
    if (fileRow && chunkRow) {
      const file = JSON.parse(fileRow);
      const chunk = JSON.parse(chunkRow);
      const payload = Buffer.from(String(chunk.data || ''), 'base64').toString('utf8');
      return { file, chunk, payload };
    }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
  }
  throw new Error(`sqlite file/chunk rows not replicated for ${id}`);
}

function sqlString(value) {
  return String(value).replaceAll("'", "''");
}

function assertHealthyAdvancedStatusContract(status) {
  const problems = [];
  if (!status || typeof status !== 'object') {
    throw new Error(`Business OS advanced status missing: ${JSON.stringify(status)}`);
  }
  if (status.version !== 'business-os-advanced-status-v1') {
    problems.push(`unexpected version ${JSON.stringify(status.version)}`);
  }
  if (!status.ok) problems.push('status.ok is false');
  if (status.sync?.mode !== 'webrtc') problems.push(`sync.mode is ${JSON.stringify(status.sync?.mode)}`);
  if (status.sync?.protocol !== 'ctox-rxdb-protocol-v1') {
    problems.push(`sync.protocol is ${JSON.stringify(status.sync?.protocol)}`);
  }
  if (!Array.isArray(status.sync?.capabilities) || !status.sync.capabilities.includes('ctox-peer-session-v1')) {
    problems.push('sync.capabilities is missing ctox-peer-session-v1');
  }
  if (!Array.isArray(status.sync?.peerSessions)) {
    problems.push('sync.peerSessions is not an array');
  } else if (status.sync.peerSessions.length === 0) {
    problems.push('sync.peerSessions is empty');
  } else if (status.sync.peerSessions.some((session) => !Number.isFinite(Number(session?.generation)) || Number(session.generation) < 1)) {
    problems.push(`sync.peerSessions contains invalid generation: ${JSON.stringify(status.sync.peerSessions)}`);
  } else if (status.sync.peerSessions.some((session) => !session?.checkpoint?.epoch || session.checkpoint.state !== 'advertised')) {
    problems.push(`sync.peerSessions missing checkpoint epoch evidence: ${JSON.stringify(status.sync.peerSessions)}`);
  }
  if (!Array.isArray(status.sync?.collectionErrors)) {
    problems.push('sync.collectionErrors is not an array');
  } else if (status.sync.collectionErrors.length > 0) {
    problems.push(`sync.collectionErrors is not empty: ${JSON.stringify(status.sync.collectionErrors)}`);
  }
  if (!Array.isArray(status.sync?.checkpointErrors)) {
    problems.push('sync.checkpointErrors is not an array');
  } else if (status.sync.checkpointErrors.length > 0) {
    problems.push(`sync.checkpointErrors is not empty: ${JSON.stringify(status.sync.checkpointErrors)}`);
  }
  if (!Array.isArray(status.sync?.failedCollections)) {
    problems.push('sync.failedCollections is not an array');
  } else if (status.sync.failedCollections.length > 0) {
    problems.push(`sync.failedCollections is not empty: ${JSON.stringify(status.sync.failedCollections)}`);
  }
  if (!Array.isArray(status.sync?.missingRequiredCollections)) {
    problems.push('sync.missingRequiredCollections is not an array');
  } else if (status.sync.missingRequiredCollections.length > 0) {
    problems.push(`sync.missingRequiredCollections is not empty: ${JSON.stringify(status.sync.missingRequiredCollections)}`);
  }
  if (!status.sync?.initialSync || typeof status.sync.initialSync !== 'object') {
    problems.push('sync.initialSync is missing');
  } else {
    const initialSync = status.sync.initialSync;
    if (!Array.isArray(initialSync.missingInitialReplication)) {
      problems.push('sync.initialSync.missingInitialReplication is not an array');
    } else if (initialSync.missingInitialReplication.length > 0) {
      problems.push(`sync.initialSync.missingInitialReplication is not empty: ${JSON.stringify(initialSync.missingInitialReplication)}`);
    }
    if (!Array.isArray(initialSync.entries) || initialSync.entries.length === 0) {
      problems.push('sync.initialSync.entries is empty');
    } else if (initialSync.entries.some((entry) => entry?.state !== 'complete' || !entry?.initialReplicationAt)) {
      problems.push(`sync.initialSync.entries contains incomplete collection: ${JSON.stringify(initialSync.entries)}`);
    }
  }
  if (problems.length) {
    throw new Error(`Business OS advanced status contract failed: ${problems.join('; ')}\n${JSON.stringify(status, null, 2)}`);
  }
}

async function collectStartupState(page) {
  return page.evaluate(() => ({
    url: location.href,
    title: document.title,
    readyState: document.readyState,
    hasSmoke: Boolean(globalThis.ctoxBusinessOsSmoke),
    search: location.search,
    scriptSrcs: [...document.scripts].map((script) => script.src || '[inline]').slice(0, 20),
    resources: performance.getEntriesByType('resource')
      .filter((entry) => /\/(?:app|shared|modules|vendor)\//.test(entry.name) || entry.name.includes('/app.js'))
      .map((entry) => ({
        name: entry.name,
        initiatorType: entry.initiatorType,
        duration: Math.round(entry.duration),
        transferSize: entry.transferSize,
        decodedBodySize: entry.decodedBodySize,
      }))
      .slice(-30),
    bodyDataset: { ...document.body?.dataset },
    bodyText: (document.body?.innerText || '').slice(0, 800),
  })).catch((evalError) => ({ evaluateError: String(evalError?.message || evalError) }));
}

function isPreHookModuleGraphStall(startupState) {
  if (!startupState || startupState.hasSmoke) return false;
  if (startupState.readyState !== 'interactive') return false;
  const scripts = Array.isArray(startupState.scriptSrcs) ? startupState.scriptSrcs : [];
  return scripts.some((src) => /\/app\.js\?/.test(src));
}

function addQueryParam(urlPath, key, value) {
  const [pathAndQuery, hash = ''] = urlPath.split('#');
  const [pathname, query = ''] = pathAndQuery.split('?');
  const params = new URLSearchParams(query);
  params.set(key, value);
  const nextQuery = params.toString();
  return `${pathname}${nextQuery ? `?${nextQuery}` : ''}${hash ? `#${hash}` : ''}`;
}

function seedRustSideFile(source) {
  const now = Date.now();
  const dir = path.join(runtimeRoot, 'runtime/business-os/notes/rxdb-smoke');
  fs.mkdirSync(dir, { recursive: true });
  const content = hasOwn(process.env, 'SMOKE_RUST_FILE_CONTENT')
    ? process.env.SMOKE_RUST_FILE_CONTENT
    : `hello from ${source} ${now}`;
  const filePath = path.join(dir, `${source}_${now}_${token(5)}.txt`);
  fs.writeFileSync(filePath, content);
  const canonicalPath = fs.realpathSync(filePath);
  const id = `ctox_file_${crypto.createHash('sha256').update(canonicalPath).digest('hex')}`;
  return { id, content, path: canonicalPath, syncMode: 'file' };
}

function seedRustWorkspaceFile(options = {}) {
  const now = Date.now();
  const workspaceName = `workspace_${now}_${token(5)}`;
  const workspacePath = path.join(runtimeRoot, 'runtime/business-os/workspaces/rxdb-smoke', workspaceName);
  const dir = path.join(workspacePath, 'reports');
  fs.mkdirSync(dir, { recursive: true });
  const largeContent = options.large
    ? `${'large workspace smoke block\n'.repeat(45000)}large workspace smoke ${now}\n`
    : null;
  const content = hasOwn(process.env, 'SMOKE_RUST_FILE_CONTENT')
    ? process.env.SMOKE_RUST_FILE_CONTENT
    : (largeContent || `hello from workspace_smoke ${now}`);
  const filePath = path.join(dir, 'brief.md');
  fs.writeFileSync(filePath, content);
  const canonicalPath = fs.realpathSync(filePath);
  const id = `ctox_file_${crypto.createHash('sha256').update(canonicalPath).digest('hex')}`;
  return {
    id,
    content,
    path: canonicalPath,
    syncMode: 'workspace',
    workspacePath: fs.realpathSync(workspacePath),
    expectedVirtualPath: `/CTOX/${workspaceName}/reports/brief.md`,
  };
}

function syncRustSeedFile(seed) {
  const deadline = Date.now() + 60000;
  let lastOutput = '';
  const args = seed.syncMode === 'workspace'
    ? ['business-os', 'files', 'sync-workspace', seed.workspacePath]
    : ['business-os', 'files', 'sync', seed.path];
  while (Date.now() < deadline) {
    const result = spawnSync(ctoxBin, args, {
      cwd: root,
      env: {
        ...process.env,
        CTOX_ROOT: runtimeRoot,
        CARGO_TARGET_DIR: path.join(root, 'runtime/build/core-rxdb-integration-target'),
      },
      encoding: 'utf8',
    });
    if (result.status === 0) return;
    lastOutput = result.stderr || result.stdout || '';
    if (!String(lastOutput).includes('database is locked')) break;
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
  }
  throw new Error(`ctox ${args.join(' ')} failed: ${lastOutput}`);
}

async function stopChild(child) {
  if (!child || child.exitCode !== null) return;
  child.kill('SIGINT');
  await new Promise((resolve) => {
    const timer = setTimeout(() => {
      if (child.exitCode === null) child.kill('SIGKILL');
      resolve();
    }, 15000);
    child.once('exit', () => {
      clearTimeout(timer);
      resolve();
    });
  });
}

function startCtoxServer() {
  const child = spawn(ctoxBin, ['business-os', 'serve', '--addr', `127.0.0.1:${businessPort}`], {
    cwd: root,
    env: {
      ...process.env,
      CTOX_BUSINESS_OS_SIGNALING_URLS: signalingUrl,
      CTOX_BUSINESS_OS_DISABLE_BACKGROUND_FILE_INDEX: '1',
      CTOX_BUSINESS_OS_ENABLE_SMOKE_CONTROLS: '1',
      CTOX_ROOT: runtimeRoot,
      CARGO_TARGET_DIR: path.join(root, 'runtime/build/core-rxdb-integration-target'),
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  child.stdout.on('data', (d) => process.stdout.write(`[ctox] ${d}`));
  child.stderr.on('data', (d) => process.stderr.write(`[ctox:err] ${d}`));
  globalThis.__ctoxProcess = child;
  child.on('exit', (code, signal) => console.log(`[ctox:exit] code=${code} signal=${signal}`));
  return child;
}

function ensureCtoxSmokeBinary() {
  if (process.env.CTOX_BIN || process.env.CTOX_SKIP_SMOKE_BUILD === '1') return;
  const targetDir = path.join(root, 'runtime/build/core-rxdb-integration-target');
  const result = spawnSync('cargo', [
    'build',
    '--locked',
    '--bin',
    'ctox',
    '--target-dir',
    targetDir,
  ], {
    cwd: root,
    env: {
      ...process.env,
      CARGO_TARGET_DIR: targetDir,
    },
    encoding: 'utf8',
  });
  if (result.status !== 0) {
    throw new Error(`ctox smoke binary build failed: ${result.stderr || result.stdout || 'no output'}`);
  }
  if (!fs.existsSync(ctoxBin)) {
    throw new Error(`ctox smoke binary was not produced at ${ctoxBin}`);
  }
}

(async () => {
  ensureCtoxSmokeBinary();
  let signaling = await startSignalingServer();
  console.log(`signaling=${signalingUrl}`);
  const workspaceFileMode = smokeMode === 'workspace-rust-to-browser'
    || smokeMode === 'workspace-update-rust-to-browser'
    || smokeMode === 'workspace-large-materialize-rust-to-browser'
    || smokeMode === 'workspace-large-file-viewer-rust-to-browser'
    || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser';
  const rustSeed = workspaceFileMode
    ? seedRustWorkspaceFile({
        large: smokeMode === 'workspace-large-materialize-rust-to-browser'
          || smokeMode === 'workspace-large-file-viewer-rust-to-browser'
          || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser',
      })
    : seedRustSideFile(smokeMode === 'rust-to-browser' ? 'rust_smoke' : 'rust_ready');
  let ctox = startCtoxServer();

  let browser;
  try {
    const config = await waitForHttp(`http://127.0.0.1:${businessPort}/api/business-os/sync/config`);
    if (!config.native_rxdb_peer_available) {
      throw new Error(`native peer unavailable: ${JSON.stringify(config)}`);
    }
    browser = await chromium.launch(chromiumLaunchOptions());
    const page = await browser.newPage();
    page.on('console', (msg) => console.log(`[browser:${msg.type()}] ${msg.text()}`));
    page.on('pageerror', (err) => console.error(`[browser:error] ${err.stack || err.message}`));
    page.on('requestfailed', (request) => {
      const url = request.url();
      if (url.includes('/app.js') || url.includes('/shared/') || url.includes('/modules/') || url.includes('/vendor/')) {
        console.error(`[browser:requestfailed] ${request.method()} ${url} ${request.failure()?.errorText || ''}`);
      }
    });
    page.on('response', (response) => {
      const url = response.url();
      if (response.status() >= 400 && (url.includes('/app.js') || url.includes('/shared/') || url.includes('/modules/') || url.includes('/vendor/'))) {
        console.error(`[browser:response] ${response.status()} ${url}`);
      }
    });
    await page.exposeFunction('__ctoxSyncRustSeedFile', () => syncRustSeedFile(rustSeed));
    await page.exposeFunction('__ctoxUpdateRustSeedFile', (content) => {
      fs.writeFileSync(rustSeed.path, content);
      rustSeed.content = content;
      syncRustSeedFile(rustSeed);
      return {
        id: rustSeed.id,
        content,
        path: rustSeed.path,
        expectedVirtualPath: rustSeed.expectedVirtualPath || '',
      };
    });
    await page.exposeFunction('__ctoxRestartNativePeer', async () => {
      await stopChild(ctox);
      ctox = startCtoxServer();
      await waitForHttp(`http://127.0.0.1:${businessPort}/api/business-os/sync/config`);
      await waitForSqliteTables([
        'ctox_business_os__desktop_files__v0',
        'ctox_business_os__desktop_file_chunks__v0',
      ]);
      return true;
    });
    await page.exposeFunction('__ctoxRolloverNativePeerInProcess', async () => {
      const res = await fetch(`http://127.0.0.1:${businessPort}/api/business-os/sync/native-peer/restart`, {
        method: 'POST',
      });
      if (!res.ok) {
        throw new Error(`native peer in-process restart failed: ${res.status} ${await res.text()}`);
      }
      const status = await res.json();
      await waitForHttp(`http://127.0.0.1:${businessPort}/api/business-os/sync/config`);
      await waitForSqliteTables([
        'ctox_business_os__desktop_files__v0',
        'ctox_business_os__desktop_file_chunks__v0',
      ]);
      return status;
    });
    await page.exposeFunction('__ctoxRestartSignalingAndNativePeer', async () => {
      await stopChild(ctox);
      await stopSignalingServer(signaling);
      signaling = await startSignalingServer();
      ctox = startCtoxServer();
      await waitForHttp(`http://127.0.0.1:${businessPort}/api/business-os/sync/config`);
      await waitForSqliteTables([
        'ctox_business_os__desktop_files__v0',
        'ctox_business_os__desktop_file_chunks__v0',
      ]);
      return true;
    });
    const browserPath = useAppDb ? addQueryParam(pagePath, 'rxdbSmoke', '1') : pagePath;
    const smokeUrl = `http://127.0.0.1:${businessPort}${browserPath}`;
    await page.goto(smokeUrl, { waitUntil: 'commit', timeout: 10000 });
    let advancedStatusEvidenceVersion = '';
    if (useAppDb) {
      let startupState = null;
      for (let attempt = 0; attempt < 2; attempt += 1) {
        try {
          await page.waitForFunction(() => Boolean(globalThis.ctoxBusinessOsSmoke), null, { timeout: 120000 });
          startupState = null;
          break;
        } catch (error) {
          startupState = await collectStartupState(page);
          if (attempt === 0 && isPreHookModuleGraphStall(startupState)) {
            console.warn(`[smoke] Business OS module graph stalled before smoke hook; reloading once: ${JSON.stringify({
              readyState: startupState.readyState,
              scriptSrcs: startupState.scriptSrcs,
              bodyDataset: startupState.bodyDataset,
            })}`);
            await page.goto('about:blank', { waitUntil: 'commit', timeout: 10000 }).catch(() => {});
            await page.goto(smokeUrl, { waitUntil: 'commit', timeout: 10000 });
            continue;
          }
          break;
        }
      }
      if (startupState) {
        throw new Error(`Business OS smoke hook did not initialize: ${JSON.stringify(startupState, null, 2)}`);
      }
      if (smokeMode === 'signaling-error-browser-status') {
        const errorStatus = await page.evaluate(async () => {
          const deadline = Date.now() + 60000;
          let lastSnapshot = null;
          while (Date.now() < deadline) {
            lastSnapshot = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({ includeCounts: false });
            const collectionErrors = Array.isArray(lastSnapshot?.sync?.collectionErrors)
              ? lastSnapshot.sync.collectionErrors
              : [];
            const match = collectionErrors.find((error) => (
              error?.name === 'CtoxSignalingControlPlaneError' &&
              error?.code === 'instance_mismatch'
            ));
            if (match) return { ok: true, error: match, snapshot: lastSnapshot };
            await new Promise((resolve) => setTimeout(resolve, 250));
          }
          return { ok: false, snapshot: lastSnapshot };
        });
        if (!errorStatus?.ok) {
          throw new Error(`Business OS did not expose injected signaling error in advanced status: ${JSON.stringify(errorStatus, null, 2)}`);
        }
        console.log(`signaling_error_collection=${errorStatus.error.collection}`);
        console.log(`signaling_error_code=${errorStatus.error.code}`);
        console.log(`signaling_error_name=${errorStatus.error.name}`);
        return;
      }
      if (smokeMode === 'checkpoint-error-browser-status') {
        const errorStatus = await page.evaluate(async () => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          if (!state) return { ok: false, reason: 'missing smoke state' };
          state.session = { authenticated: true };
          state.sync = { mode: 'webrtc' };
          state.syncDiagnostics = {
            mode: 'webrtc',
            phase: 'collection-sync',
            protocol: 'ctox-rxdb-protocol-v1',
            capabilities: ['ctox-peer-session-v1', 'ctox-checkpoint-epoch-v1'],
            collections: {
              checkpoint_fixture: {
                collection: 'checkpoint_fixture',
                status: 'error',
                connectionStatus: 'error',
                remoteProtocol: 'ctox-rxdb-protocol-v1',
                remoteCapabilities: ['ctox-peer-session-v1', 'ctox-checkpoint-epoch-v1'],
                remotePeerSession: 'ctox_instance:checkpoint-fixture',
                remoteCheckpoint: null,
                peerGeneration: 1,
                peerSessionSeenAt: new Date().toISOString(),
                lastError: {
                  name: 'CtoxCheckpointProtocolError',
                  code: 'ctox_checkpoint_epoch_missing',
                  phase: 'checkpoint-handshake',
                  severity: 'error',
                  retryable: false,
                  message: 'Remote RxDB peer did not provide advertised checkpoint epoch evidence.',
                },
              },
            },
            lastError: null,
          };
          const snapshot = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
            includeCounts: false,
            requiredCollections: ['checkpoint_fixture'],
          });
          const checkpointErrors = Array.isArray(snapshot?.sync?.checkpointErrors)
            ? snapshot.sync.checkpointErrors
            : [];
          const match = checkpointErrors.find((error) => (
            error?.name === 'CtoxCheckpointProtocolError' &&
            error?.code === 'ctox_checkpoint_epoch_missing' &&
            error?.phase === 'checkpoint-handshake'
          ));
          return { ok: Boolean(match && snapshot?.checks?.noCheckpointProtocolErrors === false), error: match || null, snapshot };
        });
        if (!errorStatus?.ok) {
          throw new Error(`Business OS did not expose checkpoint protocol error in advanced status: ${JSON.stringify(errorStatus, null, 2)}`);
        }
        console.log(`checkpoint_error_collection=${errorStatus.error.collection}`);
        console.log(`checkpoint_error_code=${errorStatus.error.code}`);
        console.log(`checkpoint_error_name=${errorStatus.error.name}`);
        return;
      }
      if (smokeMode === 'schema-error-browser-status') {
        const errorStatus = await page.evaluate(async () => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          if (!state) return { ok: false, reason: 'missing smoke state' };
          state.session = { authenticated: true };
          state.sync = { mode: 'webrtc' };
          state.syncDiagnostics = {
            mode: 'webrtc',
            phase: 'collection-sync',
            protocol: 'ctox-rxdb-protocol-v1',
            capabilities: ['ctox-peer-session-v1', 'ctox-schema-hash-v1'],
            collections: {
              schema_fixture: {
                collection: 'schema_fixture',
                status: 'error',
                connectionStatus: 'error',
                remoteProtocol: 'ctox-rxdb-protocol-v1',
                remoteCapabilities: ['ctox-peer-session-v1', 'ctox-schema-hash-v1'],
                remotePeerSession: 'ctox_instance:schema-fixture',
                remoteCheckpoint: {
                  source: 'ctox-rs',
                  state: 'advertised',
                  collection: 'schema_fixture',
                  schemaHash: 'actual-schema-hash',
                  epoch: 'schema-fixture-epoch',
                },
                peerGeneration: 1,
                peerSessionSeenAt: new Date().toISOString(),
                lastError: {
                  name: 'CtoxSchemaProtocolError',
                  code: 'ctox_schema_hash_mismatch',
                  phase: 'schema-handshake',
                  severity: 'error',
                  retryable: false,
                  expected: 'expected-schema-hash',
                  actual: 'actual-schema-hash',
                  message: 'Remote RxDB peer collection schema hash does not match the Browser schema.',
                },
              },
            },
            lastError: null,
          };
          const snapshot = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
            includeCounts: false,
            requiredCollections: ['schema_fixture'],
          });
          const schemaErrors = Array.isArray(snapshot?.sync?.schemaErrors)
            ? snapshot.sync.schemaErrors
            : [];
          const match = schemaErrors.find((error) => (
            error?.name === 'CtoxSchemaProtocolError' &&
            error?.code === 'ctox_schema_hash_mismatch' &&
            error?.phase === 'schema-handshake' &&
            error?.expected === 'expected-schema-hash' &&
            error?.actual === 'actual-schema-hash'
          ));
          return { ok: Boolean(match && snapshot?.checks?.noSchemaProtocolErrors === false), error: match || null, snapshot };
        });
        if (!errorStatus?.ok) {
          throw new Error(`Business OS did not expose schema protocol error in advanced status: ${JSON.stringify(errorStatus, null, 2)}`);
        }
        console.log(`schema_error_collection=${errorStatus.error.collection}`);
        console.log(`schema_error_code=${errorStatus.error.code}`);
        console.log(`schema_error_name=${errorStatus.error.name}`);
        return;
      }
      if (smokeMode === 'replication-error-browser-status') {
        const errorStatus = await page.evaluate(async () => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          if (!state) return { ok: false, reason: 'missing smoke state' };
          state.session = { authenticated: true };
          state.sync = { mode: 'webrtc' };
          state.syncDiagnostics = {
            mode: 'webrtc',
            phase: 'collection-sync',
            protocol: 'ctox-rxdb-protocol-v1',
            capabilities: ['ctox-peer-session-v1'],
            collections: {
              replication_fixture: {
                collection: 'replication_fixture',
                status: 'error',
                connectionStatus: 'error',
                remoteProtocol: 'ctox-rxdb-protocol-v1',
                remoteCapabilities: ['ctox-peer-session-v1'],
                remotePeerSession: 'ctox_instance:replication-fixture',
                remoteCheckpoint: {
                  source: 'ctox-rs',
                  state: 'advertised',
                  collection: 'replication_fixture',
                  epoch: 'replication-fixture-epoch',
                },
                peerGeneration: 1,
                peerSessionSeenAt: new Date().toISOString(),
                lastError: {
                  name: 'CtoxReplicationIoError',
                  code: 'ctox_replication_pull_failed',
                  phase: 'replication-pull',
                  severity: 'error',
                  retryable: true,
                  direction: 'pull',
                  upstreamCode: 'RC_PULL',
                  batchSize: 20,
                  rowCount: 0,
                  message: 'RxDB WebRTC pull from the remote peer failed.',
                },
              },
            },
            lastError: null,
          };
          const snapshot = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
            includeCounts: false,
            requiredCollections: ['replication_fixture'],
          });
          const replicationErrors = Array.isArray(snapshot?.sync?.replicationErrors)
            ? snapshot.sync.replicationErrors
            : [];
          const match = replicationErrors.find((error) => (
            error?.name === 'CtoxReplicationIoError' &&
            error?.code === 'ctox_replication_pull_failed' &&
            error?.phase === 'replication-pull' &&
            error?.direction === 'pull' &&
            error?.upstreamCode === 'RC_PULL'
          ));
          return { ok: Boolean(match && snapshot?.checks?.noReplicationIoErrors === false), error: match || null, snapshot };
        });
        if (!errorStatus?.ok) {
          throw new Error(`Business OS did not expose replication I/O error in advanced status: ${JSON.stringify(errorStatus, null, 2)}`);
        }
        console.log(`replication_error_collection=${errorStatus.error.collection}`);
        console.log(`replication_error_code=${errorStatus.error.code}`);
        console.log(`replication_error_name=${errorStatus.error.name}`);
        return;
      }
      try {
        await page.waitForFunction(() => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const modulesLoaded = Array.isArray(state?.modules) && state.modules.length > 0;
          const shellOpened = Boolean(document.body?.dataset?.moduleShell);
          const loading = Boolean(document.body?.dataset?.moduleLoading);
          return modulesLoaded && shellOpened && !loading;
        }, null, { timeout: 60000 });
      } catch (error) {
        const waitError = String(error?.message || error);
        const startupState = await page.evaluate(async (waitErrorMessage) => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const status = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({ includeCounts: true }).catch((snapshotError) => ({
            snapshotError: String(snapshotError?.message || snapshotError),
          }));
          return {
            error: waitErrorMessage,
            hasSmoke: Boolean(globalThis.ctoxBusinessOsSmoke),
            authenticated: Boolean(state?.session?.authenticated),
            moduleCount: Array.isArray(state?.modules) ? state.modules.length : null,
            activeModule: state?.activeModule?.id || null,
            syncMode: state?.sync?.mode || null,
            syncDiagnostics: state?.syncDiagnostics || null,
            advancedStatus: status || null,
            bodyDataset: { ...document.body?.dataset },
            statusText: document.querySelector('[data-status]')?.textContent || '',
            visibleText: (document.body?.innerText || '').slice(0, 800),
          };
        }, waitError).catch((evalError) => ({ evaluateError: String(evalError?.message || evalError) }));
        throw new Error(`Business OS shell did not become ready: ${JSON.stringify(startupState, null, 2)}`);
      }
      const advancedStatus = await page.evaluate(() => globalThis.CTOX_BUSINESS_OS_STATUS?.waitForHealthy?.({
        timeoutMs: 60000,
        requiredCollections: [
          'business_module_catalog',
          'ctox_runtime_settings',
          'business_commands',
          'ctox_queue_tasks',
          'desktop_files',
          'desktop_file_chunks',
        ],
      }));
      if (!advancedStatus?.ok) {
        throw new Error(`Business OS advanced status unhealthy after startup: ${JSON.stringify(advancedStatus, null, 2)}`);
      }
      assertHealthyAdvancedStatusContract(advancedStatus);
      advancedStatusEvidenceVersion = advancedStatus.version || '';
      if (smokeMode === 'tab-freeze-browser-to-rust') {
        const cdp = await page.context().newCDPSession(page);
        await cdp.send('Page.setWebLifecycleState', { state: 'frozen' });
        await new Promise((resolve) => setTimeout(resolve, 5000));
        await cdp.send('Page.setWebLifecycleState', { state: 'active' });
        await cdp.detach().catch(() => {});
        const resumedStatus = await page.evaluate(() => globalThis.CTOX_BUSINESS_OS_STATUS?.waitForHealthy?.({
          timeoutMs: 60000,
          requiredCollections: [
            'business_module_catalog',
            'ctox_runtime_settings',
            'business_commands',
            'ctox_queue_tasks',
            'desktop_files',
            'desktop_file_chunks',
          ],
        }));
        if (!resumedStatus?.ok) {
          throw new Error(`Business OS advanced status unhealthy after tab freeze resume: ${JSON.stringify(resumedStatus, null, 2)}`);
        }
        assertHealthyAdvancedStatusContract(resumedStatus);
        advancedStatusEvidenceVersion = resumedStatus.version || advancedStatusEvidenceVersion;
      }
      if (smokeMode === 'network-flap-browser-to-rust') {
        await page.context().setOffline(true);
        await new Promise((resolve) => setTimeout(resolve, 5000));
        await page.context().setOffline(false);
        const resumedStatus = await page.evaluate(() => globalThis.CTOX_BUSINESS_OS_STATUS?.waitForHealthy?.({
          timeoutMs: 90000,
          requiredCollections: [
            'business_module_catalog',
            'ctox_runtime_settings',
            'business_commands',
            'ctox_queue_tasks',
            'desktop_files',
            'desktop_file_chunks',
          ],
        }));
        if (!resumedStatus?.ok) {
          throw new Error(`Business OS advanced status unhealthy after browser network flap: ${JSON.stringify(resumedStatus, null, 2)}`);
        }
        assertHealthyAdvancedStatusContract(resumedStatus);
        advancedStatusEvidenceVersion = resumedStatus.version || advancedStatusEvidenceVersion;
      }
    }

    const browserPayload = hasOwn(process.env, 'SMOKE_BROWSER_FILE_CONTENT')
      ? process.env.SMOKE_BROWSER_FILE_CONTENT
      : 'hello';
    const result = await page.evaluate(async ({ signalingUrl, smokeMode, rustSeed, useAppDb, browserPayload, advancedStatusEvidenceVersion }) => {
      if (!globalThis.process) globalThis.process = {};
      if (typeof globalThis.process.nextTick !== 'function') {
        globalThis.process.nextTick = (callback, ...args) => Promise.resolve().then(() => callback(...args));
      }
      const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const bounded = (promise, ms) => promise
        ? Promise.race([promise.catch?.(() => undefined) || promise, delay(ms)])
        : delay(0);
      const isRecoverablePeerLifecycleError = (error) => {
        const haystack = [
          error?.code,
          error?.parameters?.error?.code,
          error?.message,
          (() => {
            try { return JSON.stringify(error?.parameters || null); } catch { return ''; }
          })(),
        ].filter(Boolean).join('\n');
        return haystack.includes('ERR_CONNECTION_FAILURE')
          || haystack.includes('ERR_SET_LOCAL_DESCRIPTION')
          || haystack.includes('ERR_PC_CONSTRUCTOR')
          || haystack.includes('Cannot create so many PeerConnections')
          || haystack.includes('Still in CONNECTING state');
      };
      const logUnexpectedReplicationError = (label, error) => {
        if (isRecoverablePeerLifecycleError(error)) return;
        console.error(label, error);
      };

      let db;
      let appFileReplicationState = null;
      let appChunkReplicationState = null;
      let appCommandReplicationState = null;
      let appQueueReplicationState = null;
      let ownsDb = false;
      let advancedStatusVersion = advancedStatusEvidenceVersion || '';
      const replicationStates = [];
      const commandSmokeMode = smokeMode === 'command-browser-to-rust'
        || smokeMode === 'command-burst-browser-to-rust'
        || smokeMode === 'command-restart-browser-to-rust'
        || smokeMode === 'command-midflight-restart-browser-to-rust';
      const materializeSmokeMode = smokeMode === 'workspace-large-materialize-rust-to-browser'
        || smokeMode === 'workspace-large-file-viewer-rust-to-browser'
        || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser';
      const needsCommandCollections = commandSmokeMode || materializeSmokeMode;
      const needsFileCollections = !commandSmokeMode || materializeSmokeMode;

      if (useAppDb) {
        const deadline = Date.now() + 30000;
        let appState = null;
        while (Date.now() < deadline) {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const raw = state?.db?.raw;
          const hasCollections = (!needsFileCollections || (raw?.desktop_files && raw?.desktop_file_chunks))
            && (!needsCommandCollections || (raw?.business_commands && raw?.ctox_queue_tasks));
          if (hasCollections && state?.sync) {
            appState = state;
            break;
          }
          await new Promise((resolve) => setTimeout(resolve, 250));
        }
        if (!appState) {
          const smoke = globalThis.ctoxBusinessOsSmoke;
          const raw = smoke?.state?.db?.raw;
          throw new Error(`Business OS app DB did not become available for smoke test: ${JSON.stringify({
            hasSmoke: Boolean(smoke),
            hasDb: Boolean(smoke?.state?.db),
            hasSync: Boolean(smoke?.state?.sync),
            rawCollections: raw ? Object.keys(raw).slice(0, 20) : [],
            status: document.querySelector('[data-status]')?.textContent || '',
            bodyClass: document.body?.className || '',
          })}`);
        }
        if (needsCommandCollections) {
          const commandBridge = await appState.sync.startCollection('business_commands');
          const queueBridge = await appState.sync.startCollection('ctox_queue_tasks');
          commandBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app business_commands replication error', error));
          queueBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app ctox_queue_tasks replication error', error));
          appCommandReplicationState = commandBridge?.state || null;
          appQueueReplicationState = queueBridge?.state || null;
          await bounded(appCommandReplicationState?.awaitInitialReplication?.(), 15000);
          await bounded(appQueueReplicationState?.awaitInitialReplication?.(), 15000);
          await bounded(appCommandReplicationState?.awaitInSync?.(), 15000);
          await bounded(appQueueReplicationState?.awaitInSync?.(), 15000);
        }
        if (needsFileCollections) {
          const fileBridge = await appState.sync.startCollection('desktop_files');
          const chunkBridge = await appState.sync.startCollection('desktop_file_chunks');
          fileBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app desktop_files replication error', error));
          chunkBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app desktop_file_chunks replication error', error));
          appFileReplicationState = fileBridge?.state || null;
          appChunkReplicationState = chunkBridge?.state || null;
          await bounded(appFileReplicationState?.awaitInitialReplication?.(), 15000);
          await bounded(appChunkReplicationState?.awaitInitialReplication?.(), 15000);
          await bounded(appFileReplicationState?.awaitInSync?.(), 15000);
          await bounded(appChunkReplicationState?.awaitInSync?.(), 15000);
        }
        db = appState.db.raw;
      } else {
        const config = await (await fetch('/api/business-os/sync/config')).json();
        const rxdb = await import('/vendor/rxdb-bundle.mjs');
        registerRxdbPlugin(rxdb, rxdb.RxDBMigrationSchemaPlugin || rxdb.RxDBMigrationPlugin);
        const desktopSchemaMod = await import('/modules/desktop/schema.js');
        const ctoxSchemaMod = await import('/modules/ctox/schema.js');
        db = await rxdb.createRxDatabase({
          name: `ctox_smoke_${Date.now()}`,
          storage: rxdb.getRxStorageDexie(),
          multiInstance: false,
          closeDuplicates: true,
        });
        ownsDb = true;
        const collections = {};
        if (needsCommandCollections) {
          collections.business_commands = { schema: ctoxSchemaMod.collections.business_commands };
          collections.ctox_queue_tasks = { schema: ctoxSchemaMod.collections.ctox_queue_tasks };
        }
        if (needsFileCollections) {
          collections.desktop_files = { schema: desktopSchemaMod.collections.desktop_files };
          collections.desktop_file_chunks = { schema: desktopSchemaMod.collections.desktop_file_chunks };
        }
        await db.addCollections(collections);

        async function startReplication(collectionName) {
          const replicationState = await rxdb.replicateWebRTC({
            collection: db[collectionName],
            topic: `${config.sync_room}:${collectionName}`,
            connectionHandlerCreator: rxdb.getConnectionHandlerSimplePeer({ signalingServerUrl: signalingUrl }),
            pull: { batchSize: 10 },
            push: { batchSize: 10 },
            retryTime: 1000,
          });
          replicationState.error$?.subscribe?.((error) => logUnexpectedReplicationError(`${collectionName} replication error`, error));
          replicationStates.push(replicationState);
        }

        if (needsCommandCollections) {
          await startReplication('business_commands');
          await startReplication('ctox_queue_tasks');
        }
        if (needsFileCollections) {
          await startReplication('desktop_files');
          await startReplication('desktop_file_chunks');
        }
        await Promise.all(replicationStates.map((state) => bounded(state?.awaitInitialReplication?.(), 15000)));
        await Promise.all(replicationStates.map((state) => bounded(state?.awaitInSync?.(), 15000)));
      }

      function selectActiveFileChunks(chunks, contentGenerationId) {
        const candidates = Array.isArray(chunks) ? chunks : [];
        let selected = [];
        if (contentGenerationId) {
          selected = candidates.filter((chunk) => chunk.generation_id === contentGenerationId);
        }
        if (selected.length === 0) {
          const newestCreatedAt = candidates.reduce((max, chunk) => Math.max(max, Number(chunk.created_at_ms || 0)), 0);
          selected = candidates.filter((chunk) => Number(chunk.created_at_ms || 0) === newestCreatedAt);
        }
        selected.sort((left, right) => Number(left.idx || 0) - Number(right.idx || 0));
        const expectedTotal = selected.length > 0 ? Number(selected[0].total || selected.length) : 0;
        return {
          chunks: selected,
          expectedTotal,
          complete: selected.length > 0 && selected.length === expectedTotal,
        };
      }

      async function waitForFile(id, ms = 30000, expectedPayload = null) {
        const deadline = Date.now() + ms;
        let lastSeen = null;
        while (Date.now() < deadline) {
          const fileDoc = await db.desktop_files.findOne(id).exec();
          const file = fileDoc?.toJSON?.() || fileDoc;
          const allChunks = (await db.desktop_file_chunks.find().exec())
            .map((doc) => doc.toJSON?.() || doc)
            .filter((doc) => doc.file_id === id);
          const active = selectActiveFileChunks(allChunks, file?.content_generation_id || '');
          if (file && active.complete) {
            const payload = atob(active.chunks.map((doc) => doc.data).join(''));
            lastSeen = {
              file,
              chunks: active.chunks,
              payload,
              generationId: file.content_generation_id || active.chunks[0]?.generation_id || '',
              allChunkCount: allChunks.length,
            };
            if (expectedPayload === null || payload === expectedPayload) return lastSeen;
          }
          await new Promise((resolve) => setTimeout(resolve, 500));
        }
        const fileDoc = await db.desktop_files.findOne(id).exec();
        const allChunkDocs = await db.desktop_file_chunks.find().exec();
        const chunkDocs = allChunkDocs
          .map((doc) => doc.toJSON?.() || doc)
          .filter((doc) => doc.file_id === id);
        throw new Error(`browser did not receive rust-side file ${id}: ${JSON.stringify({
          hasFile: Boolean(fileDoc),
          chunkCount: chunkDocs.length,
          totalChunkCount: allChunkDocs.length,
          expectedPayload: expectedPayload === null ? null : {
            length: expectedPayload.length,
            prefix: expectedPayload.slice(0, 80),
          },
          lastSeen,
          syncMode: globalThis.ctoxBusinessOsSmoke?.state?.sync?.mode || '',
          syncConfig: globalThis.ctoxBusinessOsSmoke?.state?.sync?.config || null,
        })}`);
      }

      async function waitForFileMetadata(id, ms = 30000) {
        const deadline = Date.now() + ms;
        while (Date.now() < deadline) {
          const fileDoc = await db.desktop_files.findOne(id).exec();
          const file = fileDoc?.toJSON?.() || fileDoc;
          if (file) {
            const chunks = (await db.desktop_file_chunks.find().exec())
              .map((doc) => doc.toJSON?.() || doc)
              .filter((doc) => doc.file_id === id);
            return { file, chunks };
          }
          await delay(500);
        }
        throw new Error(`browser did not receive rust-side file metadata ${id}`);
      }

      function registerRxdbPlugin(target, plugin) {
        const add = target?.addRxPlugin;
        if (typeof add !== 'function' || !plugin) return;
        try {
          add(plugin);
        } catch (error) {
          const message = String(error?.message || error || '');
          if (!message.toLowerCase().includes('already')) throw error;
        }
      }

      async function repairAppFileAndCommandReplicationAfterNativeRestart() {
        await globalThis.__ctoxRestartNativePeer?.();
        const repairedState = globalThis.ctoxBusinessOsSmoke?.state;
        if (!repairedState?.db?.raw?.desktop_files || !repairedState?.db?.raw?.desktop_file_chunks) {
          throw new Error('Business OS file collections were not available after native peer restart');
        }
        db = repairedState.db.raw;
        const criticalCollections = [
          'business_module_catalog',
          'ctox_runtime_settings',
          'business_commands',
          'ctox_queue_tasks',
          'desktop_files',
          'desktop_file_chunks',
        ];
        if (typeof repairedState.sync?.restartCollections === 'function') {
          const repairedBridges = await repairedState.sync.restartCollections(criticalCollections);
          const bridgeByCollection = Object.fromEntries(criticalCollections.map((collection, index) => [collection, repairedBridges[index]]));
          appCommandReplicationState = bridgeByCollection.business_commands?.state || appCommandReplicationState;
          appQueueReplicationState = bridgeByCollection.ctox_queue_tasks?.state || appQueueReplicationState;
          appFileReplicationState = bridgeByCollection.desktop_files?.state || appFileReplicationState;
          appChunkReplicationState = bridgeByCollection.desktop_file_chunks?.state || appChunkReplicationState;
          await bounded(appCommandReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appQueueReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appFileReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appChunkReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appCommandReplicationState?.awaitInSync?.(), 30000);
          await bounded(appQueueReplicationState?.awaitInSync?.(), 30000);
          await bounded(appFileReplicationState?.awaitInSync?.(), 30000);
          await bounded(appChunkReplicationState?.awaitInSync?.(), 30000);
          return repairedState;
        }
        const startFresh = async (collection) => {
          if (typeof repairedState.sync?.restartCollection === 'function') {
            return repairedState.sync.restartCollection(collection);
          }
          return repairedState.sync?.startCollection?.(collection);
        };
        const repairedModuleBridge = await startFresh('business_module_catalog');
        const repairedRuntimeBridge = await startFresh('ctox_runtime_settings');
        const repairedCommandBridge = await startFresh('business_commands');
        const repairedQueueBridge = await startFresh('ctox_queue_tasks');
        const repairedFileBridge = await startFresh('desktop_files');
        const repairedChunkBridge = await startFresh('desktop_file_chunks');
        appCommandReplicationState = repairedCommandBridge?.state || appCommandReplicationState;
        appQueueReplicationState = repairedQueueBridge?.state || appQueueReplicationState;
        appFileReplicationState = repairedFileBridge?.state || appFileReplicationState;
        appChunkReplicationState = repairedChunkBridge?.state || appChunkReplicationState;
        await bounded(repairedModuleBridge?.state?.awaitInitialReplication?.(), 20000);
        await bounded(repairedRuntimeBridge?.state?.awaitInitialReplication?.(), 20000);
        await bounded(appCommandReplicationState?.awaitInitialReplication?.(), 20000);
        await bounded(appQueueReplicationState?.awaitInitialReplication?.(), 20000);
        await bounded(appFileReplicationState?.awaitInitialReplication?.(), 20000);
        await bounded(appChunkReplicationState?.awaitInitialReplication?.(), 20000);
        await bounded(repairedModuleBridge?.state?.awaitInSync?.(), 30000);
        await bounded(repairedRuntimeBridge?.state?.awaitInSync?.(), 30000);
        await bounded(appCommandReplicationState?.awaitInSync?.(), 30000);
        await bounded(appQueueReplicationState?.awaitInSync?.(), 30000);
        await bounded(appFileReplicationState?.awaitInSync?.(), 30000);
        await bounded(appChunkReplicationState?.awaitInSync?.(), 30000);
        return repairedState;
      }

      async function waitForAppSyncCollections(collections, ms = 90000) {
        const deadline = Date.now() + ms;
        let lastDiagnostics = null;
        while (Date.now() < deadline) {
          lastDiagnostics = globalThis.ctoxBusinessOsSmoke?.state?.syncDiagnostics || null;
          const collectionDiagnostics = lastDiagnostics?.collections || {};
          const ready = collections.every((collection) => {
            const entry = collectionDiagnostics[collection] || {};
            return entry.connectionStatus === 'connected';
          });
          if (ready) return;
          await delay(500);
        }
        throw new Error(`Business OS sync collections did not reconnect: ${JSON.stringify({
          collections,
          diagnostics: lastDiagnostics,
        })}`);
      }

      if (commandSmokeMode) {
        if (smokeMode === 'command-burst-browser-to-rust') {
          const now = Date.now();
          const commandCount = Math.max(2, Number(globalThis.__ctoxCommandBurstCount || 5));
          const ids = Array.from({ length: commandCount }, (_, index) => `command_burst_smoke_${now}_${index}`);
          await Promise.all(ids.map((id, index) => db.business_commands.insert({
            id,
            command_id: id,
            module: 'ctox',
            command_type: 'business_os.smoke',
            record_id: '',
            status: 'pending_sync',
            inbound_channel: 'ctox',
            payload: { title: `WebRTC command burst smoke ${index + 1}`, instruction: 'smoke test only' },
            client_context: { source: 'rxdb-smoke', burst: true, index },
            updated_at_ms: now + index,
          })));
          await bounded(appCommandReplicationState?.awaitInSync?.(), 25000);
          await bounded(appQueueReplicationState?.awaitInSync?.(), 25000);
          const deadline = Date.now() + 60000;
          const accepted = new Map();
          while (Date.now() < deadline) {
            for (const id of ids) {
              if (accepted.has(id)) continue;
              const commandDoc = await db.business_commands.findOne(id).exec();
              const command = commandDoc?.toJSON?.();
              const taskId = command?.task_id || '';
              if (!command || command.status === 'pending_sync' || !taskId) continue;
              const taskDoc = await db.ctox_queue_tasks.findOne(taskId).exec();
              const task = taskDoc?.toJSON?.();
              if (!task) continue;
              const queueTasksForCommand = (await db.ctox_queue_tasks.find().exec())
                .map((doc) => doc.toJSON?.() || doc)
                .filter((doc) => doc.command_id === id);
              if (queueTasksForCommand.length !== 1) {
                throw new Error(`command ${id} produced ${queueTasksForCommand.length} queue tasks: ${JSON.stringify(queueTasksForCommand)}`);
              }
              accepted.set(id, { taskId, status: command.status, taskStatus: command.task_status || task.status || '' });
            }
            if (accepted.size === ids.length) {
              await Promise.all(replicationStates.map((state) => state.cancel?.()));
              if (ownsDb) await db.close();
              return {
                mode: smokeMode,
                commandCount: ids.length,
                taskCountForCommands: accepted.size,
                ids,
                taskIds: [...accepted.values()].map((item) => item.taskId),
              };
            }
            await delay(500);
          }
          const commandDocs = await Promise.all(ids.map(async (id) => (await db.business_commands.findOne(id).exec())?.toJSON?.() || null));
          const queueDocs = (await db.ctox_queue_tasks.find().exec()).map((doc) => doc.toJSON?.() || doc);
          throw new Error(`command burst was not accepted via RxDB/WebRTC: ${JSON.stringify({
            commandCount: ids.length,
            acceptedCount: accepted.size,
            commands: commandDocs,
            queueCount: queueDocs.length,
          })}`);
        }
        const now = Date.now();
        const id = `command_smoke_${now}`;
        if (smokeMode === 'command-restart-browser-to-rust') {
          await globalThis.__ctoxRestartNativePeer?.();
          if (useAppDb) {
            const repairedState = globalThis.ctoxBusinessOsSmoke?.state;
            if (!repairedState?.db?.raw?.business_commands || !repairedState?.db?.raw?.ctox_queue_tasks) {
              throw new Error('Business OS command collections were not available after native peer restart');
            }
            db = repairedState.db.raw;
            const repairedCommandBridge = await repairedState.sync.startCollection('business_commands');
            const repairedQueueBridge = await repairedState.sync.startCollection('ctox_queue_tasks');
            appCommandReplicationState = repairedCommandBridge?.state || null;
            appQueueReplicationState = repairedQueueBridge?.state || null;
          }
          await bounded(appCommandReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appQueueReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appCommandReplicationState?.awaitInSync?.(), 30000);
          await bounded(appQueueReplicationState?.awaitInSync?.(), 30000);
        }
        if (smokeMode === 'command-midflight-restart-browser-to-rust') {
          const commandBus = globalThis.ctoxBusinessOsSmoke?.state?.commandBus;
          if (!commandBus?.dispatch) throw new Error('Business OS command bus is not available for mid-flight restart smoke');
          const restartPromise = globalThis.__ctoxRestartNativePeer?.();
          await delay(50);
          await commandBus.dispatch({
            id,
            module: 'ctox',
            type: 'business_os.smoke',
            record_id: '',
            inbound_channel: 'ctox',
            payload: { title: 'WebRTC command restart smoke', instruction: 'smoke test only' },
            client_context: { source: 'rxdb-smoke', restart: 'midflight' },
          });
          await restartPromise;
          if (useAppDb) {
            const repairedState = globalThis.ctoxBusinessOsSmoke?.state;
            db = repairedState?.db?.raw || db;
            const repairedCommandBridge = await repairedState?.sync?.startCollection?.('business_commands');
            const repairedQueueBridge = await repairedState?.sync?.startCollection?.('ctox_queue_tasks');
            appCommandReplicationState = repairedCommandBridge?.state || appCommandReplicationState;
            appQueueReplicationState = repairedQueueBridge?.state || appQueueReplicationState;
          }
          await bounded(appCommandReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appQueueReplicationState?.awaitInitialReplication?.(), 20000);
        } else {
          await db.business_commands.insert({
            id,
            command_id: id,
            module: 'ctox',
            command_type: 'business_os.smoke',
            record_id: '',
            status: 'pending_sync',
            inbound_channel: 'ctox',
            payload: { title: 'WebRTC command smoke', instruction: 'smoke test only' },
            client_context: { source: 'rxdb-smoke' },
            updated_at_ms: now,
          });
        }
        await bounded(appCommandReplicationState?.awaitInSync?.(), 25000);
        await bounded(appQueueReplicationState?.awaitInSync?.(), 25000);
        const deadline = Date.now() + 45000;
        while (Date.now() < deadline) {
          const commandDoc = await db.business_commands.findOne(id).exec();
          const command = commandDoc?.toJSON?.();
          const taskId = command?.task_id || '';
          if (command && command.status !== 'pending_sync' && taskId) {
            const taskDoc = await db.ctox_queue_tasks.findOne(taskId).exec();
            const task = taskDoc?.toJSON?.();
            if (task) {
              const queueTasksForCommand = (await db.ctox_queue_tasks.find().exec())
                .map((doc) => doc.toJSON?.() || doc)
                .filter((doc) => doc.command_id === id);
              if (queueTasksForCommand.length !== 1) {
                throw new Error(`command ${id} produced ${queueTasksForCommand.length} queue tasks: ${JSON.stringify(queueTasksForCommand)}`);
              }
              await Promise.all(replicationStates.map((state) => state.cancel?.()));
              if (ownsDb) await db.close();
              return {
                mode: smokeMode,
                id,
                status: command.status,
                taskId,
                taskStatus: command.task_status || task.status || '',
                taskCountForCommand: queueTasksForCommand.length,
              };
            }
          }
          await delay(500);
        }
        const commandDoc = await db.business_commands.findOne(id).exec();
        const queueDocs = await db.ctox_queue_tasks.find({ limit: 5 }).exec();
        throw new Error(`command ${id} was not accepted via RxDB/WebRTC: ${JSON.stringify({
          command: commandDoc?.toJSON?.() || null,
          queueCount: queueDocs.length,
          syncMode: globalThis.ctoxBusinessOsSmoke?.state?.sync?.mode || '',
          syncConfig: globalThis.ctoxBusinessOsSmoke?.state?.sync?.config || null,
        })}`);
      }

      const received = smokeMode === 'restart-browser-to-rust'
        || smokeMode === 'restart-signaling-browser-to-rust'
        || smokeMode === 'rollover-native-peer-browser-to-rust'
        ? { payload: rustSeed.content, file: {} }
        : await (async () => {
            await globalThis.__ctoxSyncRustSeedFile?.();
            if (smokeMode === 'workspace-large-materialize-rust-to-browser'
              || smokeMode === 'workspace-large-file-viewer-rust-to-browser'
              || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser') {
              return waitForFileMetadata(rustSeed.id, 60000);
            }
            return waitForFile(rustSeed.id);
          })();
      if ((smokeMode === 'workspace-rust-to-browser'
        || smokeMode === 'workspace-update-rust-to-browser'
        || smokeMode === 'workspace-large-materialize-rust-to-browser'
        || smokeMode === 'workspace-large-file-viewer-rust-to-browser'
        || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser')
        && rustSeed.expectedVirtualPath) {
        const actualPath = received.file?.virtual_path || received.file?.path || '';
        if (actualPath !== rustSeed.expectedVirtualPath) {
          throw new Error(`workspace file virtual path mismatch: ${JSON.stringify({
            expected: rustSeed.expectedVirtualPath,
            actual: actualPath,
            file: received.file,
          })}`);
        }
      }
      if (smokeMode === 'workspace-large-materialize-rust-to-browser') {
        if (received.file?.content_state !== 'lazy') {
          throw new Error(`large workspace file was not indexed lazily: ${JSON.stringify(received.file)}`);
        }
        if (received.chunks.length !== 0) {
          throw new Error(`large workspace file wrote eager chunks before materialize: ${received.chunks.length}`);
        }
        const commandId = `materialize_smoke_${Date.now()}`;
        await db.business_commands.insert({
          id: commandId,
          command_id: commandId,
          module: 'ctox',
          command_type: 'ctox.file.materialize',
          record_id: rustSeed.id,
          status: 'pending_sync',
          inbound_channel: 'ctox',
          payload: {
            file_id: rustSeed.id,
            path: received.file?.local_path || received.file?.path || rustSeed.path,
          },
          client_context: { source: 'rxdb-smoke', materialize: true },
          updated_at_ms: Date.now(),
        });
        await bounded(appCommandReplicationState?.awaitInSync?.(), 25000);
        await bounded(appFileReplicationState?.awaitInSync?.(), 25000);
        await bounded(appChunkReplicationState?.awaitInSync?.(), 25000);
        const materialized = await waitForFile(rustSeed.id, 90000, rustSeed.content);
        if (materialized.file?.content_state !== 'available') {
          throw new Error(`large workspace file did not become available after materialize: ${JSON.stringify(materialized.file)}`);
        }
        const commandDoc = await db.business_commands.findOne(commandId).exec();
        const command = commandDoc?.toJSON?.() || null;
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          id: rustSeed.id,
          commandId,
          commandStatus: command?.status || '',
          payloadLength: materialized.payload.length,
          chunkCount: materialized.chunks.length,
          generationId: materialized.generationId || '',
          virtualPath: materialized.file?.virtual_path || materialized.file?.path || '',
        };
      }
      if (smokeMode === 'workspace-large-file-viewer-rust-to-browser'
        || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser') {
        if (received.file?.content_state !== 'lazy') {
          throw new Error(`large workspace file was not indexed lazily for file viewer: ${JSON.stringify(received.file)}`);
        }
        if (received.chunks.length !== 0) {
          throw new Error(`large workspace file wrote eager chunks before file viewer materialize: ${received.chunks.length}`);
        }
        if (smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser') {
          await repairAppFileAndCommandReplicationAfterNativeRestart();
        }
        const mount = document.createElement('section');
        mount.setAttribute('data-file-viewer-smoke', 'true');
        mount.style.cssText = 'position:fixed;left:0;top:0;width:760px;height:560px;z-index:99999;background:#0b1117;';
        document.body.append(mount);
        const viewer = await import(`/desktop-apps/file-viewer/app.js?v=file-viewer-smoke-${Date.now()}`);
        const smokeState = globalThis.ctoxBusinessOsSmoke?.state;
        const teardown = await viewer.mount(mount, {
          db: smokeState?.db || db,
          sync: smokeState?.sync,
          commandBus: smokeState?.commandBus,
          session: smokeState?.session,
          setTitle: () => {},
          args: {
            fileId: rustSeed.id,
            name: rustSeed.name || 'brief.md',
            mimeType: received.file?.mime_type || 'text/markdown',
            sizeBytes: received.file?.size_bytes || rustSeed.content.length,
            path: received.file?.local_path || received.file?.path || rustSeed.path,
            contentState: received.file?.content_state || '',
            contentGenerationId: received.file?.content_generation_id || '',
          },
        });
        const deadline = Date.now() + 120000;
        let text = '';
        while (Date.now() < deadline) {
          const pre = mount.querySelector('[data-file-text]');
          text = pre?.textContent || '';
          if (text === rustSeed.content) break;
          const errorText = mount.querySelector('.is-error')?.textContent || '';
          if (errorText) throw new Error(`file viewer failed to materialize large file: ${errorText}`);
          await delay(500);
        }
        try { teardown?.(); } catch {}
        mount.remove();
        if (text !== rustSeed.content) {
          throw new Error(`file viewer did not render materialized payload: ${JSON.stringify({
            expectedLength: rustSeed.content.length,
            actualLength: text.length,
            prefix: text.slice(0, 80),
          })}`);
        }
        const materialized = await waitForFile(rustSeed.id, 30000, rustSeed.content);
        const advancedStatus = smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser'
          ? await globalThis.CTOX_BUSINESS_OS_STATUS?.waitForHealthy?.({
              timeoutMs: 90000,
              requiredCollections: [
                'business_module_catalog',
                'ctox_runtime_settings',
                'business_commands',
                'ctox_queue_tasks',
                'desktop_files',
                'desktop_file_chunks',
              ],
            })
          : null;
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          id: rustSeed.id,
          payloadLength: text.length,
          chunkCount: materialized.chunks.length,
          generationId: materialized.generationId || '',
          virtualPath: materialized.file?.virtual_path || materialized.file?.path || '',
          restarted: smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser',
          advancedStatus,
        };
      }
      if (smokeMode === 'workspace-update-rust-to-browser') {
        const updatedContent = `${rustSeed.content}\nupdated via workspace smoke ${Date.now()}`;
        const update = await globalThis.__ctoxUpdateRustSeedFile?.(updatedContent);
        const updated = await waitForFile(rustSeed.id, 60000, updatedContent);
        const updatedPath = updated.file?.virtual_path || updated.file?.path || '';
        if (rustSeed.expectedVirtualPath && updatedPath !== rustSeed.expectedVirtualPath) {
          throw new Error(`workspace updated file virtual path mismatch: ${JSON.stringify({
            expected: rustSeed.expectedVirtualPath,
            actual: updatedPath,
            file: updated.file,
          })}`);
        }
        if (received.generationId && updated.generationId && received.generationId === updated.generationId) {
          throw new Error(`workspace updated file reused content generation: ${JSON.stringify({
            previousGeneration: received.generationId,
            updatedGeneration: updated.generationId,
            update,
          })}`);
        }
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          id: rustSeed.id,
          previousPayload: received.payload,
          updatedPayload: updated.payload,
          previousGenerationId: received.generationId || '',
          updatedGenerationId: updated.generationId || '',
          virtualPath: updatedPath,
        };
      }
      if (smokeMode === 'rust-to-browser' || smokeMode === 'workspace-rust-to-browser') {
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          id: rustSeed.id,
          payload: received.payload,
          virtualPath: received.file?.virtual_path || received.file?.path || '',
        };
      }

      if (smokeMode === 'restart-browser-to-rust'
        || smokeMode === 'restart-signaling-browser-to-rust'
        || smokeMode === 'rollover-native-peer-browser-to-rust') {
        const peerSessionsBeforeRestart = useAppDb
          ? (await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({ includeCounts: false }))?.sync?.peerSessions || []
          : [];
        if (smokeMode === 'rollover-native-peer-browser-to-rust') {
          await globalThis.__ctoxRolloverNativePeerInProcess?.();
        } else if (smokeMode === 'restart-signaling-browser-to-rust') {
          await globalThis.__ctoxRestartSignalingAndNativePeer?.();
        } else {
          await globalThis.__ctoxRestartNativePeer?.();
        }
        if (useAppDb) {
          const repairedState = globalThis.ctoxBusinessOsSmoke?.state;
          const repairedDb = repairedState?.db?.raw;
          if (!repairedDb?.desktop_files || !repairedDb?.desktop_file_chunks) {
            throw new Error('Business OS app DB was not available after reconnect repair');
          }
          db = repairedDb;
          const repairedBridges = typeof repairedState.sync.restartCollections === 'function'
            ? await repairedState.sync.restartCollections(['desktop_files', 'desktop_file_chunks'])
            : [
                await repairedState.sync.restartCollection('desktop_files'),
                await repairedState.sync.restartCollection('desktop_file_chunks'),
              ];
          const repairedFileBridge = repairedBridges[0];
          const repairedChunkBridge = repairedBridges[1];
          appFileReplicationState = repairedFileBridge?.state || null;
          appChunkReplicationState = repairedChunkBridge?.state || null;
          await bounded(appFileReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appChunkReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appFileReplicationState?.awaitInSync?.(), 30000);
          await bounded(appChunkReplicationState?.awaitInSync?.(), 30000);
          const advancedStatusAfterRepair = await globalThis.CTOX_BUSINESS_OS_STATUS?.waitForHealthy?.({
            timeoutMs: 60000,
            requiredCollections: [
              'business_module_catalog',
              'ctox_runtime_settings',
              'business_commands',
              'ctox_queue_tasks',
              'desktop_files',
              'desktop_file_chunks',
            ],
          });
          const peerSessionsAfterRepair = advancedStatusAfterRepair?.sync?.peerSessions || [];
          const beforeByCollection = new Map(peerSessionsBeforeRestart.map((session) => [session.collection, session]));
          const generationChanged = peerSessionsAfterRepair.some((session) => {
            const before = beforeByCollection.get(session.collection);
            return before
              && before.peerSession
              && session.peerSession
              && before.peerSession !== session.peerSession
              && Number(session.generation || 0) > Number(before.generation || 0);
          });
          if (!generationChanged) {
            throw new Error(`Peer generation did not advance after native peer restart: ${JSON.stringify({
              before: peerSessionsBeforeRestart,
              after: peerSessionsAfterRepair,
              advancedStatusAfterRepair,
              mode: smokeMode,
            }, null, 2)}`);
          }
          advancedStatusVersion = advancedStatusAfterRepair?.version || advancedStatusVersion;
        }
      }

      const now = Date.now();
      const idPrefixByMode = {
        'restart-signaling-browser-to-rust': 'browser_signaling_restart_smoke',
        'rollover-native-peer-browser-to-rust': 'browser_rollover_smoke',
        'tab-freeze-browser-to-rust': 'browser_tab_freeze_smoke',
        'network-flap-browser-to-rust': 'browser_network_flap_smoke',
        'restart-browser-to-rust': 'browser_restart_smoke',
      };
      const id = `${idPrefixByMode[smokeMode] || 'browser_smoke'}_${now}`;
      const encoded = btoa(browserPayload);
      await db.desktop_files.insert({
        id,
        path: `/browser/smoke/${id}.txt`,
        name: `${id}.txt`,
        kind: 'file',
        mime_type: 'text/plain',
        extension: 'txt',
        size_bytes: browserPayload.length,
        owner_id: 'browser-smoke',
        source: 'browser-webrtc-smoke',
        content_ref: id,
        sort_index: now,
        is_deleted: false,
        created_at_ms: now,
        updated_at_ms: now,
      });
      await db.desktop_file_chunks.insert({
        id: `${id}_0`,
        file_id: id,
        idx: 0,
        total: 1,
        encoding: 'base64',
        data: encoded,
        size_bytes: encoded.length,
        created_at_ms: now,
      });
      await bounded(appFileReplicationState?.awaitInSync?.(), 25000);
      await bounded(appChunkReplicationState?.awaitInSync?.(), 25000);
      await delay(25000);
      await Promise.all(replicationStates.map((state) => state.cancel?.()));
      if (ownsDb) await db.close();
      return { mode: smokeMode, id, readinessPayload: received.payload, browserPayload, advancedStatusVersion };
    }, { signalingUrl, smokeMode, rustSeed, useAppDb, browserPayload, advancedStatusEvidenceVersion });

    if (result.mode === 'rust-to-browser' || result.mode === 'workspace-rust-to-browser') {
      if (result.payload !== rustSeed.content) throw new Error(`browser payload mismatch: ${result.payload}`);
      console.log(`replicated_id=${result.id}`);
      if (result.virtualPath) console.log(`virtual_path=${result.virtualPath}`);
      console.log(result.payload);
    } else if (result.mode === 'workspace-update-rust-to-browser') {
      if (result.updatedPayload !== rustSeed.content) {
        throw new Error(`browser updated payload mismatch: ${result.updatedPayload}`);
      }
      console.log(`replicated_id=${result.id}`);
      if (result.virtualPath) console.log(`virtual_path=${result.virtualPath}`);
      console.log(`previous_generation=${result.previousGenerationId}`);
      console.log(`updated_generation=${result.updatedGenerationId}`);
      console.log(result.updatedPayload);
    } else if (result.mode === 'workspace-large-materialize-rust-to-browser'
      || result.mode === 'workspace-large-file-viewer-rust-to-browser'
      || result.mode === 'workspace-large-file-viewer-restart-rust-to-browser') {
      if (result.payloadLength !== rustSeed.content.length) {
        throw new Error(`browser materialized payload length mismatch: ${result.payloadLength} !== ${rustSeed.content.length}`);
      }
      if (result.advancedStatus) {
        assertHealthyAdvancedStatusContract(result.advancedStatus);
        console.log(`advanced_status=${result.advancedStatus.version}`);
      }
      console.log(`replicated_id=${result.id}`);
      if (result.commandId) console.log(`command_id=${result.commandId}`);
      if (result.commandStatus) console.log(`command_status=${result.commandStatus}`);
      if (result.virtualPath) console.log(`virtual_path=${result.virtualPath}`);
      console.log(`generation=${result.generationId}`);
      console.log(`chunk_count=${result.chunkCount}`);
      console.log(`payload_length=${result.payloadLength}`);
    } else if (result.mode === 'command-burst-browser-to-rust') {
      console.log(`command_count=${result.commandCount}`);
      console.log(`task_count_for_commands=${result.taskCountForCommands}`);
      console.log(`command_ids=${result.ids.join(',')}`);
      console.log(`task_ids=${result.taskIds.join(',')}`);
    } else if (result.mode === 'command-browser-to-rust'
      || result.mode === 'command-restart-browser-to-rust'
      || result.mode === 'command-midflight-restart-browser-to-rust') {
      console.log(`command_id=${result.id}`);
      console.log(`task_id=${result.taskId}`);
      console.log(`task_count_for_command=${result.taskCountForCommand}`);
      console.log(`status=${result.status}`);
      console.log(`task_status=${result.taskStatus}`);
    } else {
      if (result.readinessPayload !== rustSeed.content) {
        throw new Error(`browser readiness payload mismatch: ${result.readinessPayload}`);
      }
      const replicated = pollSqliteFileAndChunk(result.id);
      if (replicated.payload !== result.browserPayload) {
        throw new Error(`sqlite payload mismatch: ${replicated.payload}`);
      }
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      console.log(`readiness_payload=${result.readinessPayload}`);
      console.log(`replicated_id=${result.id}`);
      console.log(JSON.stringify({
        file: replicated.file.id,
        chunk: replicated.chunk.id,
        payload: replicated.payload,
      }));
    }
  } finally {
    if (browser) await withHostTimeout(browser.close(), 5000).catch(() => {});
    await stopChild(ctox);
    await stopSignalingServer(signaling);
  }
})().catch((error) => {
  console.error(error.stack || error.message || error);
  process.exit(1);
});
