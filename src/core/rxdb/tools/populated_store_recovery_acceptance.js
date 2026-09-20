#!/usr/bin/env node
'use strict';

const { spawn, spawnSync } = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const root = path.resolve(__dirname, '../../../..');
const ctoxBin = process.env.CTOX_BIN || path.join(root, 'target/debug/ctox');
const POST_CUTOVER_COMMAND_ID = 'cmd-post-cutover-001';
const ATTACHMENT_SHA256 = '70b24f43af4f6268646054f87b87b2509adfd9c7e4f7198bd03bf6c281fb01b0';
const ATTACHMENT_BYTES = 39;
const CTOX_COMMAND_TIMEOUT_MS = 30000;
const SQLITE_COMMAND_TIMEOUT_MS = 15000;
const RESTORE_COMMAND_TIMEOUT_MS = 60000;
const GIT_COMMAND_TIMEOUT_MS = 15000;

function populatedStoreScratchBase() {
  if (process.platform === 'darwin') {
    if (!fs.existsSync('/Volumes/tmp')) {
      throw new Error(
        'macOS populated-store acceptance requires mounted /Volumes/tmp; refusing os.tmpdir() fallback'
      );
    }
    return '/Volumes/tmp/dev-artifacts/ctox/populated-store-recovery';
  }
  return fs.existsSync('/Volumes/tmp/dev-artifacts')
    ? '/Volumes/tmp/dev-artifacts/ctox/populated-store-recovery'
    : os.tmpdir();
}

const scratchBase = populatedStoreScratchBase();
const evidenceDir = process.env.POPULATED_STORE_EVIDENCE_DIR
  || path.join(scratchBase, 'evidence');
const workRoot = process.env.POPULATED_STORE_ROOT
  || fs.mkdtempSync(path.join(mkdirp(scratchBase), 'ctox-populated-store-'));
const skipBrowser = process.env.POPULATED_STORE_SKIP_BROWSER === '1';
const businessPort = Number(process.env.BUSINESS_PORT || '8891');
const startedAt = Date.now();

function mkdirp(dir) {
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function childHasExited(child) {
  return Boolean(child) && (child.exitCode !== null || child.signalCode !== null);
}

async function waitForChildClose(child, timeoutMs, label) {
  if (!child || childHasExited(child)) return;
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      cleanup();
      if (childHasExited(child)) {
        resolve();
        return;
      }
      reject(new Error(
        `${label} drain deadline expired after ${timeoutMs}ms (exitCode=${child.exitCode}, signalCode=${child.signalCode})`
      ));
    }, timeoutMs);
    const onClose = () => {
      cleanup();
      resolve();
    };
    const cleanup = () => {
      clearTimeout(timer);
      child.off('close', onClose);
    };
    child.once('close', onClose);
    if (childHasExited(child)) {
      cleanup();
      resolve();
    }
  });
}

function timedSpawnSync(command, args, options = {}) {
  const timeout = options.timeout ?? CTOX_COMMAND_TIMEOUT_MS;
  const result = spawnSync(command, args, {
    maxBuffer: 16 * 1024 * 1024,
    killSignal: 'SIGKILL',
    ...options,
    timeout,
  });
  if (result.error) {
    const timedOut = result.error.code === 'ETIMEDOUT';
    const err = new Error(
      `${command} ${args.join(' ')} ${timedOut ? `timed out after ${timeout}ms` : `failed: ${result.error.message}`}`
    );
    err.result = result;
    throw err;
  }
  return result;
}

function sha256File(filePath) {
  return `${crypto.createHash('sha256').update(fs.readFileSync(filePath)).digest('hex')}  ${filePath}`;
}

function prepareWorkRoot(targetRoot) {
  mkdirp(path.join(targetRoot, 'runtime'));
  for (const entry of ['Cargo.toml', 'contracts']) {
    const target = path.join(targetRoot, entry);
    if (fs.existsSync(target)) continue;
    fs.symlinkSync(path.join(root, entry), target, entry === 'Cargo.toml' ? 'file' : 'dir');
  }
  const sourceRoot = path.join(root, 'src');
  const targetSourceRoot = path.join(targetRoot, 'src');
  mkdirp(path.join(targetSourceRoot, 'apps'));
  for (const entry of fs.readdirSync(sourceRoot)) {
    if (entry === 'apps') continue;
    const target = path.join(targetSourceRoot, entry);
    if (fs.existsSync(target)) continue;
    fs.symlinkSync(path.join(sourceRoot, entry), target, 'dir');
  }
  const sourceAppsRoot = path.join(sourceRoot, 'apps');
  const targetAppsRoot = path.join(targetSourceRoot, 'apps');
  for (const entry of fs.readdirSync(sourceAppsRoot)) {
    const target = path.join(targetAppsRoot, entry);
    if (fs.existsSync(target)) continue;
    const type = fs.statSync(path.join(sourceAppsRoot, entry)).isDirectory() ? 'dir' : 'file';
    fs.symlinkSync(path.join(sourceAppsRoot, entry), target, type);
  }
}

function runCtox(args, options = {}) {
  const result = timedSpawnSync(ctoxBin, args, {
    cwd: root,
    encoding: 'utf8',
    env: { ...process.env, CTOX_ROOT: workRoot, ...options.env },
    timeout: options.timeout ?? CTOX_COMMAND_TIMEOUT_MS,
  });
  if (result.status !== 0) {
    const err = new Error(`ctox ${args.join(' ')} failed: ${result.stderr || result.stdout}`);
    err.result = result;
    throw err;
  }
  const stdout = String(result.stdout || '').trim();
  return stdout ? JSON.parse(stdout) : {};
}

function sqlite(statement, dbPath = path.join(workRoot, 'runtime/business-os-rxdb.sqlite3')) {
  const result = timedSpawnSync('sqlite3', ['-cmd', '.timeout 10000', dbPath], {
    encoding: 'utf8',
    input: statement,
    timeout: SQLITE_COMMAND_TIMEOUT_MS,
  });
  if (result.status !== 0) {
    throw new Error(`sqlite failed: ${result.stderr || result.stdout}`);
  }
  return result.stdout;
}

function sqliteRows(statement, dbPath) {
  const raw = sqlite(statement, dbPath).trim();
  if (!raw) return [];
  return raw.split('\n').map((line) => JSON.parse(line));
}

function startServe() {
  const logPath = path.join(workRoot, 'serve.log');
  const log = fs.openSync(logPath, 'w');
  const child = spawn(ctoxBin, ['business-os', 'serve', '--addr', `127.0.0.1:${businessPort}`], {
    cwd: root,
    env: {
      ...process.env,
      CTOX_ROOT: workRoot,
      CTOX_BUSINESS_OS_ENABLE_SMOKE_CONTROLS: '1',
      CTOX_BUSINESS_OS_DISABLE_BACKGROUND_FILE_INDEX: '1',
    },
    stdio: ['ignore', log, log],
  });
  child.__logPath = logPath;
  return child;
}

async function stopServe(child) {
  if (!child) return;
  if (!childHasExited(child)) {
    child.kill('SIGTERM');
  }
  try {
    await waitForChildClose(child, 15000, 'ctox serve SIGTERM');
    return;
  } catch (termError) {
    if (!childHasExited(child)) {
      child.kill('SIGKILL');
    }
    try {
      await waitForChildClose(child, 5000, 'ctox serve SIGKILL');
    } catch (killError) {
      throw new Error(
        `ctox serve drain deadline expired after SIGTERM (${termError.message}) and SIGKILL (${killError.message})`
      );
    }
  }
}

async function waitForServe(child, timeoutMs = 90000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (childHasExited(child)) {
      throw new Error(`ctox serve exited: ${fs.readFileSync(child.__logPath, 'utf8').slice(-8000)}`);
    }
    const log = fs.existsSync(child.__logPath) ? fs.readFileSync(child.__logPath, 'utf8') : '';
    if (log.includes('CTOX Business OS listening')) return;
    await delay(250);
  }
  throw new Error(`ctox serve did not listen: ${fs.readFileSync(child.__logPath, 'utf8').slice(-8000)}`);
}

async function waitForCutover(timeoutMs = 90000) {
  const deadline = Date.now() + timeoutMs;
  let lastError = '';
  while (Date.now() < deadline) {
    try {
      const receipt = runCtox(['business-os', 'rxdb', 'cutover-receipt']);
      const stale = sqlite(`SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='${table('business_commands', 0)}';`).trim();
      const phase = receipt.receipt && receipt.receipt.phase;
      if (receipt.present && phase !== 'in_progress' && stale === '0') return receipt;
      lastError = `present=${receipt.present} stale_v0=${stale}`;
    } catch (error) {
      lastError = String(error.message || error);
    }
    await delay(250);
  }
  throw new Error(`production cutover did not finish: ${lastError}`);
}

async function waitForAcceptedWrites(timeoutMs = 60000) {
  const deadline = Date.now() + timeoutMs;
  let dispatched = false;
  let lastError = '';
  while (Date.now() < deadline) {
    try {
      if (!dispatched) {
        runCtox(['business-os', 'commands', 'dispatch', '--json', JSON.stringify({
          id: POST_CUTOVER_COMMAND_ID,
          command_id: POST_CUTOVER_COMMAND_ID,
          module: 'ctox',
          command_type: 'business_os.test',
          status: 'accepted',
          inbound_channel: 'populated-store-recovery',
          payload: { title: 'post-cutover accepted write' },
          client_context: { source: 'populated-store-recovery' },
          updated_at_ms: Date.now(),
        })]);
        dispatched = true;
      }
      const commands = sqliteRows(`SELECT json_object(
        'id', id,
        'deleted', deleted,
        'status', json_extract(data, '$.status'),
        'command_id', COALESCE(NULLIF(json_extract(data, '$.command_id'), ''), id)
      ) FROM ${table('business_commands', 2)}
       WHERE id='${POST_CUTOVER_COMMAND_ID}'
          OR json_extract(data, '$.command_id')='${POST_CUTOVER_COMMAND_ID}';`);
      const command = commands.find((row) => (
        (row.id === POST_CUTOVER_COMMAND_ID || row.command_id === POST_CUTOVER_COMMAND_ID)
        && Number(row.deleted) === 0
        && row.status === 'accepted'
      ));
      const tasks = sqliteRows(`SELECT json_object(
        'id', id,
        'deleted', deleted,
        'status', json_extract(data, '$.status'),
        'command_id', json_extract(data, '$.command_id')
      ) FROM ${table('ctox_queue_tasks', 3)}
       WHERE json_extract(data, '$.command_id')='${POST_CUTOVER_COMMAND_ID}'
          OR json_extract(data, '$.business_os_command_id')='${POST_CUTOVER_COMMAND_ID}';`);
      const task = tasks.find((row) => (
        Number(row.deleted) === 0
        && row.command_id === POST_CUTOVER_COMMAND_ID
      ));
      if (command && task) {
        return { command, task };
      }
      lastError = `command=${JSON.stringify(command || null)} task=${JSON.stringify(task || null)}`;
    } catch (error) {
      lastError = String(error.message || error);
    }
    await delay(500);
  }
  throw new Error(
    `serve did not persist accepted command/queue identity for ${POST_CUTOVER_COMMAND_ID}: ${lastError}`
  );
}

function table(collection, version) {
  return `ctox_business_os__${collection}__v${version}`;
}

function acceptedWriteSnapshot(acceptedWrite) {
  return [
    ['business_commands', 2, acceptedWrite.command.id],
    ['ctox_queue_tasks', 3, acceptedWrite.task.id],
  ].map(([collection, version, id]) => {
    if (typeof id !== 'string' || !id) throw new Error(`missing ${collection} identity`);
    const escapedId = id.replace(/'/g, "''");
    const rows = sqliteRows(`SELECT json_object(
      'id', id, 'revision', revision, 'deleted', deleted, 'data', data
    ) FROM ${table(collection, version)} WHERE id='${escapedId}';`);
    if (rows.length !== 1 || Number(rows[0].deleted) !== 0) {
      throw new Error(`accepted ${collection} record was lost: ${id}`);
    }
    return { collection, ...rows[0] };
  });
}

function verifyAttachmentBytes() {
  const fileRows = sqliteRows(`SELECT json_object(
    'id', id,
    'content_hash', json_extract(data, '$.content_hash'),
    'size_bytes', json_extract(data, '$.size_bytes')
  ) FROM ${table('desktop_files', 0)} WHERE id='file-attachment-001';`);
  if (fileRows.length !== 1) {
    throw new Error(`expected one desktop file attachment, got ${fileRows.length}`);
  }
  const chunks = sqliteRows(`SELECT json_object(
    'idx', json_extract(data, '$.idx'),
    'data', json_extract(data, '$.data'),
    'chunk_hash', json_extract(data, '$.chunk_hash'),
    'size_bytes', json_extract(data, '$.size_bytes')
  ) FROM ${table('desktop_file_chunks', 0)}
   WHERE json_extract(data, '$.file_id')='file-attachment-001'
   ORDER BY json_extract(data, '$.idx');`);
  if (!chunks.length) {
    throw new Error('attachment chunks missing after cutover');
  }
  const parts = [];
  for (const chunk of chunks) {
    const bytes = Buffer.from(chunk.data, 'base64');
    const chunkHash = crypto.createHash('sha256').update(bytes).digest('hex');
    if (chunkHash !== chunk.chunk_hash) {
      throw new Error(`chunk hash does not match decoded bytes: ${chunkHash} vs ${chunk.chunk_hash}`);
    }
    parts.push(bytes);
  }
  const assembled = Buffer.concat(parts);
  const actual = crypto.createHash('sha256').update(assembled).digest('hex');
  if (actual !== fileRows[0].content_hash) {
    throw new Error(`reassembled attachment bytes do not match content_hash: ${actual} vs ${fileRows[0].content_hash}`);
  }
  if (actual !== ATTACHMENT_SHA256) {
    throw new Error(`attachment hash lost: ${actual}`);
  }
  if (assembled.length !== ATTACHMENT_BYTES) {
    throw new Error(`attachment byte length lost: ${assembled.length}`);
  }
  return { hash: actual, bytes: assembled.length };
}

async function assertDuplicatePeerRejected() {
  const logPath = path.join(workRoot, 'duplicate-serve.log');
  const log = fs.openSync(logPath, 'w');
  const child = spawn(ctoxBin, ['business-os', 'serve', '--addr', `127.0.0.1:${businessPort + 1}`], {
    cwd: root,
    env: { ...process.env, CTOX_ROOT: workRoot },
    stdio: ['ignore', log, log],
  });
  const deadline = Date.now() + 15000;
  let found = false;
  let drainError = null;
  try {
    while (Date.now() < deadline) {
      const text = fs.existsSync(logPath) ? fs.readFileSync(logPath, 'utf8') : '';
      if (/native rxdb peer already runs in another process|lock held by another process/i.test(text)) {
        found = true;
        break;
      }
      if (childHasExited(child)) break;
      await delay(200);
    }
  } finally {
    try {
      if (!childHasExited(child)) {
        child.kill('SIGTERM');
        try {
          await waitForChildClose(child, 5000, 'duplicate serve SIGTERM');
        } catch {
          child.kill('SIGKILL');
          await waitForChildClose(child, 5000, 'duplicate serve SIGKILL');
        }
      } else {
        await waitForChildClose(child, 5000, 'duplicate serve close');
      }
    } catch (error) {
      drainError = error;
    }
  }
  if (drainError) {
    throw new Error(`duplicate peer drain deadline expired: ${drainError.message}`);
  }
  if (!found) {
    throw new Error(`stale writer was not rejected: ${fs.readFileSync(logPath, 'utf8').slice(-4000)}`);
  }
}

function restoreClosed(fromPath, extraArgs = []) {
  return timedSpawnSync(ctoxBin, [
    'business-os', 'rxdb', 'restore-immutable-backup', '--confirm-restore', '--from', fromPath, ...extraArgs,
  ], {
    cwd: root,
    encoding: 'utf8',
    env: { ...process.env, CTOX_ROOT: workRoot },
    timeout: RESTORE_COMMAND_TIMEOUT_MS,
  });
}

async function main() {
  mkdirp(evidenceDir);
  prepareWorkRoot(workRoot);
  const sourceSha = timedSpawnSync('git', ['rev-parse', 'HEAD'], {
    cwd: root,
    encoding: 'utf8',
    timeout: GIT_COMMAND_TIMEOUT_MS,
  }).stdout.trim();
  const binarySha = sha256File(ctoxBin);
  const fixturePath = path.join(root, 'tests/fixtures/populated-store-recovery/supported-historical-inventory.json');
  const fixtureSha = sha256File(fixturePath);

  const supported = runCtox(['business-os', 'rxdb', 'supported-historical-versions']);
  const materialized = runCtox(['business-os', 'rxdb', 'materialize-historical-fixture', '--confirm-synthetic-fixture']);
  const before = runCtox(['business-os', 'rxdb', 'inventory']);
  const backup = runCtox(['business-os', 'rxdb', 'backup-immutable']);
  const backupPath = backup.backup_path;

  const matchingCutover = runCtox(['business-os', 'rxdb', 'run-populated-store-cutover', '--confirm-synthetic-fixture']);
  const matchingRestore = runCtox([
    'business-os', 'rxdb', 'restore-immutable-backup', '--confirm-restore', '--from', backupPath,
  ]);

  const serveStarted = Date.now();
  const serve = startServe();
  let runError;
  try {
    await waitForServe(serve);
    const liveReceipt = await waitForCutover();
    const serveMs = Date.now() - serveStarted;
    const after = runCtox(['business-os', 'rxdb', 'inventory']);
    const inbound = sqlite(`SELECT json_extract(data, '$.inbound_channel') FROM ${table('business_commands', 2)} WHERE id='cmd-history-001';`).trim();
    if (inbound !== 'ctox') throw new Error(`inbound_channel not migrated: ${inbound}`);
    const deleted = sqlite(`SELECT deleted FROM ${table('business_commands', 2)} WHERE id='cmd-deleted-001';`).trim();
    if (deleted !== '1') throw new Error(`deletion marker lost: ${deleted}`);
    const attachment = verifyAttachmentBytes();
    const pendingTasks = Number(sqlite(`SELECT COUNT(*) FROM ${table('ctox_queue_tasks', 3)} WHERE json_extract(data, '$.command_id')='cmd-pending-001';`).trim());
    if (pendingTasks > 1) throw new Error(`pending command duplicated: ${pendingTasks} tasks`);

    await assertDuplicatePeerRejected();

    const restoreWhileRunning = restoreClosed(backupPath);
    const restoreWhileRunningClosed = restoreWhileRunning.status !== 0
      && /active native peer holds/i.test(`${restoreWhileRunning.stderr}${restoreWhileRunning.stdout}`);
    if (!restoreWhileRunningClosed) {
      throw new Error(`unsafe rollback while peer held lock was not fail-closed: ${restoreWhileRunning.stderr || restoreWhileRunning.stdout}`);
    }

    const acceptedWrite = await waitForAcceptedWrites();
    await stopServe(serve);
    await delay(500);

    const beforeDeniedRestore = acceptedWriteSnapshot(acceptedWrite);
    const restoreAfterStop = restoreClosed(backupPath);
    const restoreAfterStopClosed = restoreAfterStop.status !== 0
      && /post-cutover writes were accepted/i.test(`${restoreAfterStop.stderr}${restoreAfterStop.stdout}`);
    if (!restoreAfterStopClosed) {
      throw new Error(`unsafe rollback after stop was not fail-closed: ${restoreAfterStop.stderr || restoreAfterStop.stdout}`);
    }
    const afterDeniedRestore = acceptedWriteSnapshot(acceptedWrite);
    if (JSON.stringify(afterDeniedRestore) !== JSON.stringify(beforeDeniedRestore)) {
      throw new Error('denied restore changed the accepted command or queue task');
    }

    let browser = { skipped: skipBrowser };
    if (!skipBrowser) {
      const smokeRoot = fs.mkdtempSync(path.join(mkdirp(scratchBase), 'ctox-populated-store-smoke-'));
      fs.cpSync(workRoot, smokeRoot, { recursive: true });
      const smoke = timedSpawnSync(process.execPath, [path.join(root, 'src/core/rxdb/tools/browser_rust_smoke.js')], {
        cwd: root,
        encoding: 'utf8',
        env: {
          ...process.env,
          CTOX_BIN: ctoxBin,
          CTOX_SMOKE_ROOT: smokeRoot,
          SMOKE_MODE: 'command-browser-to-rust',
          SMOKE_PAGE_PATH: '/index.html',
          BUSINESS_PORT: String(businessPort + 2),
          SIGNALING_PORT: String(businessPort + 102),
        },
        timeout: 180000,
      });
      fs.writeFileSync(path.join(evidenceDir, 'browser-command.log'), `${smoke.stdout || ''}\n${smoke.stderr || ''}`);
      if (smoke.status !== 0) {
        throw new Error(`post-cutover browser command path failed: ${smoke.stderr || smoke.stdout}`);
      }
      browser = { ok: true, status: smoke.status };
    }

    const evidence = {
      ok: true,
      schema: 'ctox.populated_store_recovery.acceptance.v1',
      source_sha: sourceSha,
      binary: binarySha,
      fixture: fixtureSha,
      work_root: workRoot,
      supported_historical: supported,
      materialized,
      matching_cutover: matchingCutover,
      matching_restore: matchingRestore,
      backup,
      before_inventory_sha256: before.inventory_sha256,
      after_inventory_sha256: after.inventory_sha256,
      inbound_channel: inbound,
      deletion_marker: deleted,
      attachment_hash: attachment.hash,
      attachment_bytes: attachment.bytes,
      pending_task_count: pendingTasks,
      post_cutover_command_id: acceptedWrite.command.command_id,
      post_cutover_command_status: acceptedWrite.command.status,
      post_cutover_task_id: acceptedWrite.task.id,
      post_cutover_command_and_task_preserved: true,
      stale_writer_rejected: true,
      unsafe_rollback_fail_closed: true,
      serve_ms: serveMs,
      elapsed_ms: Date.now() - startedAt,
      storage_bytes: after.bytes,
      browser,
      unsupported: supported.unsupported,
      unverified_environments: [
        'live tenant stores',
        'provider export/import/resume (issue 97)',
        'Windows native host acceptance',
      ],
    };
    fs.writeFileSync(path.join(evidenceDir, 'populated-store-recovery.json'), `${JSON.stringify(evidence, null, 2)}\n`);
    process.stdout.write(`${JSON.stringify(evidence)}\n`);
  } catch (error) {
    runError = error;
  } finally {
    try {
      await stopServe(serve);
    } catch (drainError) {
      runError = runError
        ? new Error(`${runError.message}; also ${drainError.message}`)
        : drainError;
    }
  }
  if (runError) throw runError;
}

main().catch((error) => {
  console.error(error.stack || String(error));
  process.exit(1);
});