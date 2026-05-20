#!/usr/bin/env node
/*
 * Browser/Rust RxDB WebRTC smoke test for CTOX Business OS.
 *
 * Defaults to the isolated smoke page and browser-to-rust mode:
 *   node src/core/rxdb/tools/browser_rust_smoke.js
 *
 * Useful variants:
 *   SMOKE_MODE=rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=command-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=command-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 */
const net = require('net');
const path = require('path');
const crypto = require('crypto');
const { spawn, spawnSync } = require('child_process');

const root = path.resolve(__dirname, '../../../..');
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
const ctoxBin = process.env.CTOX_BIN || path.join(root, 'runtime/build/core-rxdb-integration-target/debug/ctox');
const businessPort = Number(process.env.BUSINESS_PORT || 8877);
const signalingPort = Number(process.env.SIGNALING_PORT || 18876);
const signalingUrl = `ws://127.0.0.1:${signalingPort}`;
const sqlitePath = process.env.CTOX_SQLITE || path.join(root, 'runtime/ctox.sqlite3');
const pagePath = process.env.SMOKE_PAGE_PATH || '/__rxdb_smoke__.html';
const smokeMode = process.env.SMOKE_MODE || 'browser-to-rust';
const useAppDb = process.env.SMOKE_USE_APP_DB === '1' || /^\/index\.html(?:[?#]|$)/.test(pagePath);
const hasOwn = (object, key) => Object.prototype.hasOwnProperty.call(object, key);

if (!['browser-to-rust', 'rust-to-browser', 'command-browser-to-rust'].includes(smokeMode)) {
  throw new Error(`Unsupported SMOKE_MODE=${smokeMode}`);
}

function token(len = 12) {
  const alphabet = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';
  let out = '';
  for (let i = 0; i < len; i++) out += alphabet[Math.floor(Math.random() * alphabet.length)];
  return out;
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
  const server = net.createServer((socket) => {
    let handshake = false;
    let buffer = Buffer.alloc(0);
    let peer = null;

    function send(message) {
      if (!socket.destroyed) socket.write(encodeFrame(JSON.stringify(message)));
    }

    function joined(roomId) {
      const room = rooms.get(roomId) || new Set();
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
        peer = { id: token(), rooms: new Set(), send };
        peers.set(peer.id, peer);
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
          if (typeof msg.room !== 'string' || msg.room.length <= 5 || msg.room.length >= 100) {
            socket.destroy();
            break;
          }
          peer.rooms.add(msg.room);
          if (!rooms.has(msg.room)) rooms.set(msg.room, new Set());
          rooms.get(msg.room).add(peer.id);
          joined(msg.room);
        } else if (msg.type === 'signal') {
          if (msg.senderPeerId !== peer.id) {
            socket.destroy();
            break;
          }
          peers.get(msg.receiverPeerId)?.send(msg);
        } else if (msg.type !== 'ping') {
          socket.destroy();
          break;
        }
      }
    });
    socket.on('close', disconnect);
    socket.on('error', disconnect);
  });

  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(signalingPort, '127.0.0.1', () => resolve(server));
  });
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
  const id = `${source}_${now}_${token(5)}`;
  const content = hasOwn(process.env, 'SMOKE_RUST_FILE_CONTENT')
    ? process.env.SMOKE_RUST_FILE_CONTENT
    : `hello from ${source} ${now}`;
  const encoded = Buffer.from(content, 'utf8').toString('base64');
  const lwt = now + 0.1;
  const file = {
    id,
    path: `/${source}/smoke/${id}.txt`,
    name: `${id}.txt`,
    kind: 'file',
    mime_type: 'text/plain',
    extension: 'txt',
    size_bytes: Buffer.byteLength(content),
    owner_id: 'ctox',
    source,
    content_ref: id,
    sort_index: now,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
    _deleted: false,
    _attachments: {},
    _meta: { lwt },
    _rev: '1-rustsmoke',
  };
  const chunk = {
    id: `${id}_0`,
    file_id: id,
    idx: 0,
    total: 1,
    encoding: 'base64',
    data: encoded,
    size_bytes: encoded.length,
    created_at_ms: now,
    _deleted: false,
    _attachments: {},
    _meta: { lwt: lwt + 0.1 },
    _rev: '1-rustsmoke',
  };
  sqlite([
    `INSERT OR REPLACE INTO ctox_business_os__desktop_files__v0 (id, revision, deleted, lastWriteTime, data) VALUES ('${id}', '1-rustsmoke', 0, ${lwt}, '${sqlString(JSON.stringify(file))}');`,
    `INSERT OR REPLACE INTO ctox_business_os__desktop_file_chunks__v0 (id, revision, deleted, lastWriteTime, data) VALUES ('${id}_0', '1-rustsmoke', 0, ${lwt + 0.1}, '${sqlString(JSON.stringify(chunk))}');`,
  ].join('\n'));
  return { id, content };
}

async function stopChild(child) {
  if (!child || child.exitCode !== null) return;
  child.kill('SIGINT');
  await new Promise((resolve) => {
    const timer = setTimeout(() => {
      if (child.exitCode === null) child.kill('SIGKILL');
      resolve();
    }, 5000);
    child.once('exit', () => {
      clearTimeout(timer);
      resolve();
    });
  });
}

(async () => {
  const signaling = await startSignalingServer();
  console.log(`signaling=${signalingUrl}`);
  const ctox = spawn(ctoxBin, ['business-os', 'serve', '--addr', `127.0.0.1:${businessPort}`], {
    cwd: root,
    env: {
      ...process.env,
      CTOX_BUSINESS_OS_SIGNALING_URLS: signalingUrl,
      CARGO_TARGET_DIR: path.join(root, 'runtime/build/core-rxdb-integration-target'),
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  ctox.stdout.on('data', (d) => process.stdout.write(`[ctox] ${d}`));
  ctox.stderr.on('data', (d) => process.stderr.write(`[ctox:err] ${d}`));
  globalThis.__ctoxProcess = ctox;
  ctox.on('exit', (code, signal) => console.log(`[ctox:exit] code=${code} signal=${signal}`));

  let browser;
  try {
    const config = await waitForHttp(`http://127.0.0.1:${businessPort}/api/business-os/sync/config`);
    if (!config.native_rxdb_peer_available) {
      throw new Error(`native peer unavailable: ${JSON.stringify(config)}`);
    }
    await waitForSqliteTables([
      'ctox_business_os__desktop_files__v0',
      'ctox_business_os__desktop_file_chunks__v0',
    ]);

    const rustSeed = seedRustSideFile(smokeMode === 'rust-to-browser' ? 'rust_smoke' : 'rust_ready');
    browser = await chromium.launch({ headless: true });
    const page = await browser.newPage();
    page.on('console', (msg) => console.log(`[browser:${msg.type()}] ${msg.text()}`));
    page.on('pageerror', (err) => console.error(`[browser:error] ${err.stack || err.message}`));
    const browserPath = useAppDb ? addQueryParam(pagePath, 'rxdbSmoke', '1') : pagePath;
    await page.goto(`http://127.0.0.1:${businessPort}${browserPath}`, { waitUntil: 'commit', timeout: 10000 });
    if (useAppDb) {
      await page.waitForFunction(() => Boolean(globalThis.ctoxBusinessOsSmoke), null, { timeout: 60000 });
    }

    const browserPayload = hasOwn(process.env, 'SMOKE_BROWSER_FILE_CONTENT')
      ? process.env.SMOKE_BROWSER_FILE_CONTENT
      : 'hello';
    const result = await page.evaluate(async ({ signalingUrl, smokeMode, rustSeed, useAppDb, browserPayload }) => {
      if (!globalThis.process) globalThis.process = {};
      if (typeof globalThis.process.nextTick !== 'function') {
        globalThis.process.nextTick = (callback, ...args) => Promise.resolve().then(() => callback(...args));
      }
      const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const bounded = (promise, ms) => promise
        ? Promise.race([promise.catch?.(() => undefined) || promise, delay(ms)])
        : delay(0);

      let db;
      let appFileReplicationState = null;
      let appCommandReplicationState = null;
      let appQueueReplicationState = null;
      let ownsDb = false;
      const replicationStates = [];
      const commandMode = smokeMode === 'command-browser-to-rust';

      if (useAppDb) {
        const deadline = Date.now() + 30000;
        let appState = null;
        while (Date.now() < deadline) {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const raw = state?.db?.raw;
          const hasCollections = commandMode
            ? raw?.business_commands && raw?.ctox_queue_tasks
            : raw?.desktop_files && raw?.desktop_file_chunks;
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
        if (commandMode) {
          const commandBridge = await appState.sync.startCollection('business_commands');
          const queueBridge = await appState.sync.startCollection('ctox_queue_tasks');
          commandBridge?.state?.error$?.subscribe?.((error) => console.error('app business_commands replication error', error));
          queueBridge?.state?.error$?.subscribe?.((error) => console.error('app ctox_queue_tasks replication error', error));
          appCommandReplicationState = commandBridge?.state || null;
          appQueueReplicationState = queueBridge?.state || null;
          await bounded(appCommandReplicationState?.awaitInitialReplication?.(), 15000);
          await bounded(appQueueReplicationState?.awaitInitialReplication?.(), 15000);
          await bounded(appCommandReplicationState?.awaitInSync?.(), 15000);
          await bounded(appQueueReplicationState?.awaitInSync?.(), 15000);
        } else {
          const fileBridge = await appState.sync.startCollection('desktop_files');
          const chunkBridge = await appState.sync.startCollection('desktop_file_chunks');
          fileBridge?.state?.error$?.subscribe?.((error) => console.error('app desktop_files replication error', error));
          chunkBridge?.state?.error$?.subscribe?.((error) => console.error('app desktop_file_chunks replication error', error));
          appFileReplicationState = fileBridge?.state || null;
          await bounded(appFileReplicationState?.awaitInitialReplication?.(), 15000);
          await bounded(appFileReplicationState?.awaitInSync?.(), 15000);
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
        const collections = commandMode
          ? {
              business_commands: { schema: ctoxSchemaMod.collections.business_commands },
              ctox_queue_tasks: { schema: ctoxSchemaMod.collections.ctox_queue_tasks },
            }
          : {
              desktop_files: { schema: desktopSchemaMod.collections.desktop_files },
              desktop_file_chunks: { schema: desktopSchemaMod.collections.desktop_file_chunks },
            };
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
          replicationState.error$?.subscribe?.((error) => console.error(`${collectionName} replication error`, error));
          replicationStates.push(replicationState);
        }

        if (commandMode) {
          await startReplication('business_commands');
          await startReplication('ctox_queue_tasks');
        } else {
          await startReplication('desktop_files');
          await startReplication('desktop_file_chunks');
        }
      }

      async function waitForFile(id, ms = 30000) {
        const deadline = Date.now() + ms;
        while (Date.now() < deadline) {
          const fileDoc = await db.desktop_files.findOne(id).exec();
          const chunkDocs = await db.desktop_file_chunks.find({
            selector: { file_id: id },
            sort: [{ idx: 'asc' }],
          }).exec();
          if (fileDoc && chunkDocs.length > 0) {
            return atob(chunkDocs.map((doc) => doc.toJSON().data).join(''));
          }
          await new Promise((resolve) => setTimeout(resolve, 500));
        }
        throw new Error(`browser did not receive rust-side file ${id}`);
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

      if (commandMode) {
        const now = Date.now();
        const id = `command_smoke_${now}`;
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
              await Promise.all(replicationStates.map((state) => state.cancel?.()));
              if (ownsDb) await db.close();
              return {
                mode: smokeMode,
                id,
                status: command.status,
                taskId,
                taskStatus: command.task_status || task.status || '',
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

      const payload = await waitForFile(rustSeed.id);
      if (smokeMode === 'rust-to-browser') {
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return { mode: smokeMode, id: rustSeed.id, payload };
      }

      const now = Date.now();
      const id = `browser_smoke_${now}`;
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
      await delay(25000);
      await Promise.all(replicationStates.map((state) => state.cancel?.()));
      if (ownsDb) await db.close();
      return { mode: smokeMode, id, readinessPayload: payload, browserPayload };
    }, { signalingUrl, smokeMode, rustSeed, useAppDb, browserPayload });

    if (result.mode === 'rust-to-browser') {
      if (result.payload !== rustSeed.content) throw new Error(`browser payload mismatch: ${result.payload}`);
      console.log(`replicated_id=${result.id}`);
      console.log(result.payload);
    } else if (result.mode === 'command-browser-to-rust') {
      console.log(`command_id=${result.id}`);
      console.log(`task_id=${result.taskId}`);
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
      console.log(`readiness_payload=${result.readinessPayload}`);
      console.log(`replicated_id=${result.id}`);
      console.log(JSON.stringify({
        file: replicated.file.id,
        chunk: replicated.chunk.id,
        payload: replicated.payload,
      }));
    }
  } finally {
    if (browser) await browser.close().catch(() => {});
    await stopChild(ctox);
    signaling.close();
  }
})().catch((error) => {
  console.error(error.stack || error.message || error);
  process.exit(1);
});
