#!/usr/bin/env node
'use strict';

const { spawn, spawnSync } = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const root = path.resolve(__dirname, '../../../..');
const ctoxBin = process.env.CTOX_BIN || path.join(root, 'target/debug/ctox');
const scratchBase = fs.existsSync('/Volumes/tmp/dev-artifacts')
  ? '/Volumes/tmp/dev-artifacts/ctox/populated-store-recovery'
  : os.tmpdir();
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
  const result = spawnSync(ctoxBin, args, {
    cwd: root,
    encoding: 'utf8',
    env: { ...process.env, CTOX_ROOT: workRoot, ...options.env },
    maxBuffer: 16 * 1024 * 1024,
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
  const result = spawnSync('sqlite3', ['-cmd', '.timeout 10000', dbPath], {
    encoding: 'utf8',
    input: statement,
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(`sqlite failed: ${result.stderr || result.stdout}`);
  }
  return result.stdout;
}

function wait(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
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

function stopServe(child) {
  if (!child || child.exitCode !== null) return;
  child.kill('SIGTERM');
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline && child.exitCode === null) wait(100);
  if (child.exitCode === null) child.kill('SIGKILL');
}

function waitForServe(child, timeoutMs = 90000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) {
      throw new Error(`ctox serve exited: ${fs.readFileSync(child.__logPath, 'utf8').slice(-8000)}`);
    }
    const log = fs.existsSync(child.__logPath) ? fs.readFileSync(child.__logPath, 'utf8') : '';
    if (log.includes('CTOX Business OS listening')) return;
    wait(250);
  }
  throw new Error(`ctox serve did not listen: ${fs.readFileSync(child.__logPath, 'utf8').slice(-8000)}`);
}

function waitForCutover(timeoutMs = 90000) {
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
    wait(250);
  }
  throw new Error(`production cutover did not finish: ${lastError}`);
}

function waitForAcceptedWrites(receiptHash, timeoutMs = 60000) {
  const deadline = Date.now() + timeoutMs;
  let dispatched = false;
  let lastError = '';
  while (Date.now() < deadline) {
    try {
      if (!dispatched) {
        runCtox(['business-os', 'commands', 'dispatch', '--json', JSON.stringify({
          id: 'cmd-post-cutover-001',
          command_id: 'cmd-post-cutover-001',
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
      const inventory = runCtox(['business-os', 'rxdb', 'inventory']);
      if (inventory.inventory_sha256 && inventory.inventory_sha256 !== receiptHash) {
        return inventory;
      }
      lastError = `inventory still ${inventory.inventory_sha256}`;
    } catch (error) {
      lastError = String(error.message || error);
    }
    wait(500);
  }
  throw new Error(`serve did not accept post-cutover writes before restore probe: ${lastError}`);
}

function table(collection, version) {
  return `ctox_business_os__${collection}__v${version}`;
}

function assertDuplicatePeerRejected() {
  const logPath = path.join(workRoot, 'duplicate-serve.log');
  const log = fs.openSync(logPath, 'w');
  const child = spawn(ctoxBin, ['business-os', 'serve', '--addr', `127.0.0.1:${businessPort + 1}`], {
    cwd: root,
    env: { ...process.env, CTOX_ROOT: workRoot },
    stdio: ['ignore', log, log],
  });
  const deadline = Date.now() + 15000;
  let found = false;
  while (Date.now() < deadline) {
    const text = fs.existsSync(logPath) ? fs.readFileSync(logPath, 'utf8') : '';
    if (/native rxdb peer already runs in another process|lock held by another process/i.test(text)) {
      found = true;
      break;
    }
    wait(200);
  }
  if (child.exitCode === null) {
    child.kill('SIGTERM');
    const stopBy = Date.now() + 5000;
    while (Date.now() < stopBy && child.exitCode === null) wait(100);
    if (child.exitCode === null) child.kill('SIGKILL');
  }
  if (!found) {
    throw new Error(`stale writer was not rejected: ${fs.readFileSync(logPath, 'utf8').slice(-4000)}`);
  }
}

function restoreClosed(fromPath, extraArgs = []) {
  return spawnSync(ctoxBin, [
    'business-os', 'rxdb', 'restore-immutable-backup', '--confirm-restore', '--from', fromPath, ...extraArgs,
  ], {
    cwd: root,
    encoding: 'utf8',
    env: { ...process.env, CTOX_ROOT: workRoot },
  });
}

function main() {
  mkdirp(evidenceDir);
  prepareWorkRoot(workRoot);
  const sourceSha = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).stdout.trim();
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
  try {
    waitForServe(serve);
    const liveReceipt = waitForCutover();
    const serveMs = Date.now() - serveStarted;
    const after = runCtox(['business-os', 'rxdb', 'inventory']);
    const inbound = sqlite(`SELECT json_extract(data, '$.inbound_channel') FROM ${table('business_commands', 2)} WHERE id='cmd-history-001';`).trim();
    if (inbound !== 'ctox') throw new Error(`inbound_channel not migrated: ${inbound}`);
    const deleted = sqlite(`SELECT deleted FROM ${table('business_commands', 2)} WHERE id='cmd-deleted-001';`).trim();
    if (deleted !== '1') throw new Error(`deletion marker lost: ${deleted}`);
    const attachment = sqlite(`SELECT json_extract(data, '$.content_hash') FROM ${table('desktop_files', 0)} WHERE id='file-attachment-001';`).trim();
    if (attachment !== '70b24f43af4f6268646054f87b87b2509adfd9c7e4f7198bd03bf6c281fb01b0') {
      throw new Error(`attachment hash lost: ${attachment}`);
    }
    const pendingTasks = Number(sqlite(`SELECT COUNT(*) FROM ${table('ctox_queue_tasks', 3)} WHERE json_extract(data, '$.command_id')='cmd-pending-001';`).trim());
    if (pendingTasks > 1) throw new Error(`pending command duplicated: ${pendingTasks} tasks`);

    assertDuplicatePeerRejected();

    const restoreWhileRunning = restoreClosed(backupPath);
    const restoreWhileRunningClosed = restoreWhileRunning.status !== 0
      && /active native peer holds/i.test(`${restoreWhileRunning.stderr}${restoreWhileRunning.stdout}`);
    if (!restoreWhileRunningClosed) {
      throw new Error(`unsafe rollback while peer held lock was not fail-closed: ${restoreWhileRunning.stderr || restoreWhileRunning.stdout}`);
    }

    const receiptHash = liveReceipt.receipt?.inventory_sha256;
    waitForAcceptedWrites(receiptHash);
    stopServe(serve);
    wait(500);

    const restoreAfterStop = restoreClosed(backupPath);
    const restoreAfterStopClosed = restoreAfterStop.status !== 0
      && /post-cutover writes were accepted/i.test(`${restoreAfterStop.stderr}${restoreAfterStop.stdout}`);
    if (!restoreAfterStopClosed) {
      throw new Error(`unsafe rollback after stop was not fail-closed: ${restoreAfterStop.stderr || restoreAfterStop.stdout}`);
    }

    let browser = { skipped: skipBrowser };
    if (!skipBrowser) {
      const smokeRoot = fs.mkdtempSync(path.join(mkdirp(scratchBase), 'ctox-populated-store-smoke-'));
      fs.cpSync(workRoot, smokeRoot, { recursive: true });
      const smoke = spawnSync(process.execPath, [path.join(root, 'src/core/rxdb/tools/browser_rust_smoke.js')], {
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
        maxBuffer: 16 * 1024 * 1024,
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
      attachment_hash: attachment,
      pending_task_count: pendingTasks,
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
  } finally {
    stopServe(serve);
  }
}

try {
  main();
} catch (error) {
  console.error(error.stack || String(error));
  process.exit(1);
}
