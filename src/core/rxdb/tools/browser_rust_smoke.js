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
 *   SMOKE_MODE=workspace-agent-artifacts-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-agent-artifacts-stress-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-agent-artifacts-churn-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-agent-artifacts-background-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-update-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-large-materialize-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-large-file-viewer-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-large-file-viewer-restart-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=command-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=tickets-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=tickets-clarification-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=outbound-active-ui SMOKE_PAGE_PATH=/index.html#outbound node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=browser-lifecycle-ui SMOKE_PAGE_PATH=/index.html#browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=browser-input-runtime SMOKE_PAGE_PATH=/index.html#browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=browser-handoff-ui SMOKE_PAGE_PATH=/index.html#browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=migration-version-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=command-burst-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=command-reload-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=command-restart-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=command-midflight-restart-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=rollover-native-peer-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=tab-freeze-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=network-flap-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=restart-signaling-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=signaling-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=peer-lifecycle-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=checkpoint-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=schema-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=native-schema-drift-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=replication-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=replication-push-contract-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=file-chunk-metadata-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=file-chunk-tombstone-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=file-chunk-stale-generation-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-agent-artifacts-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-agent-artifacts-stress-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-agent-artifacts-churn-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-agent-artifacts-background-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-update-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-large-materialize-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-large-file-viewer-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-large-file-viewer-restart-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=command-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=tickets-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=tickets-clarification-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html#browser SMOKE_MODE=browser-lifecycle-ui node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html#browser SMOKE_MODE=browser-handoff-ui node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=migration-version-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=command-burst-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=command-reload-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=command-restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=command-midflight-restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=rollover-native-peer-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=tab-freeze-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=network-flap-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=restart-signaling-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=signaling-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=peer-lifecycle-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=checkpoint-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=schema-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=native-schema-drift-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=replication-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=replication-push-contract-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=file-chunk-metadata-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=file-chunk-tombstone-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=file-chunk-stale-generation-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 */
const net = require('net');
const path = require('path');
const crypto = require('crypto');
const fs = require('fs');
const os = require('os');
const zlib = require('zlib');
const { spawn, spawnSync } = require('child_process');

const root = path.resolve(__dirname, '../../../..');
const runtimeRootProvided = !!process.env.CTOX_SMOKE_ROOT;
const runtimeRoot = process.env.CTOX_SMOKE_ROOT || fs.mkdtempSync(path.join(os.tmpdir(), 'ctox-rxdb-smoke-'));
const keepSmokeArtifacts = process.env.CTOX_SMOKE_KEEP_ARTIFACTS === '1';
const smokeChildren = new Set();
const restoreProtectedBusinessOsSources = protectBusinessOsSourceFiles();
const smokeRootPrepareStartedAt = Date.now();
prepareSmokeRoot(runtimeRoot);
const smokeRootPrepareMs = Date.now() - smokeRootPrepareStartedAt;
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
  const playwrightChromium = chromium.executablePath?.();
  const candidates = [
    playwrightChromium,
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/usr/bin/google-chrome',
  ].filter(Boolean);
  return candidates.find((candidate) => fs.existsSync(candidate));
}

function chromiumLaunchOptions() {
  const executablePath = existingChromeExecutable();
  const options = {
    headless: true,
    args: [
      '--disable-gpu',
      '--disable-features=WebRtcHideLocalIpsWithMdns',
    ],
  };
  if (executablePath) options.executablePath = executablePath;
  return options;
}

const ctoxBin = process.env.CTOX_BIN || path.join(root, 'runtime/build/core-rxdb-integration-target/debug/ctox');
const businessPort = parsePositiveIntegerEnv('BUSINESS_PORT', process.env.BUSINESS_PORT || '8877', { max: 65535 });
const signalingPort = parsePositiveIntegerEnv('SIGNALING_PORT', process.env.SIGNALING_PORT || '18876', { max: 65535 });
const signalingUrl = `ws://127.0.0.1:${signalingPort}`;
const signalingDebug = process.env.SIGNALING_DEBUG === '1';
const sqlitePath = process.env.CTOX_SQLITE || path.join(runtimeRoot, 'runtime/business-os-rxdb.sqlite3');
const pagePath = process.env.SMOKE_PAGE_PATH || '/__rxdb_smoke__.html';
const smokeMode = process.env.SMOKE_MODE || 'browser-to-rust';
const smokeDbId = process.env.SMOKE_DB_ID || `${smokeMode}_${Date.now()}_${token(8)}`;
const useAppDb = process.env.SMOKE_USE_APP_DB === '1'
  || /^\/index\.html(?:[?#]|$)/.test(pagePath)
  || /^\/business-os\/?(?:[?#]|$)/.test(pagePath);
const syncConfigWaitMs = parsePositiveIntegerEnv(
  'SMOKE_SYNC_CONFIG_WAIT_MS',
  process.env.SMOKE_SYNC_CONFIG_WAIT_MS || '60000',
  { max: 300000 }
);
const smokeHookWaitTimeoutMs = parsePositiveIntegerEnv(
  'SMOKE_HOOK_WAIT_TIMEOUT_MS',
  process.env.SMOKE_HOOK_WAIT_TIMEOUT_MS || '60000',
  { max: 300000 }
);
const hasOwn = (object, key) => Object.prototype.hasOwnProperty.call(object, key);

if (![
  'browser-to-rust',
  'rust-to-browser',
  'workspace-rust-to-browser',
  'workspace-agent-artifacts-rust-to-browser',
  'workspace-agent-artifacts-stress-rust-to-browser',
  'workspace-agent-artifacts-churn-rust-to-browser',
  'workspace-agent-artifacts-background-rust-to-browser',
  'workspace-update-rust-to-browser',
  'workspace-large-materialize-rust-to-browser',
  'workspace-large-file-viewer-rust-to-browser',
  'workspace-large-file-viewer-restart-rust-to-browser',
  'command-browser-to-rust',
  'tickets-browser-to-rust',
  'tickets-clarification-browser-to-rust',
  'outbound-active-ui',
  'business-os-ui-regression',
  'browser-lifecycle-ui',
  'browser-input-runtime',
  'browser-handoff-ui',
  'browser-responsive-ui',
  'migration-version-browser-to-rust',
  'command-burst-browser-to-rust',
  'command-reload-browser-to-rust',
  'command-restart-browser-to-rust',
  'command-midflight-restart-browser-to-rust',
  'rollover-native-peer-browser-to-rust',
  'tab-freeze-browser-to-rust',
  'network-flap-browser-to-rust',
  'restart-browser-to-rust',
  'restart-signaling-browser-to-rust',
  'signaling-error-browser-status',
  'peer-lifecycle-browser-status',
  'checkpoint-error-browser-status',
  'rxdb-protocol-error-browser-status',
  'schema-error-browser-status',
  'native-schema-drift-browser-status',
  'replication-error-browser-status',
  'replication-push-contract-error-browser-status',
  'file-chunk-metadata-error-browser-status',
  'file-chunk-tombstone-error-browser-status',
  'file-chunk-stale-generation-error-browser-status',
].includes(smokeMode)) {
  throw new Error(`Unsupported SMOKE_MODE=${smokeMode}`);
}
if ([
  'tickets-browser-to-rust',
  'tickets-clarification-browser-to-rust',
  'outbound-active-ui',
  'signaling-error-browser-status',
  'peer-lifecycle-browser-status',
  'checkpoint-error-browser-status',
  'rxdb-protocol-error-browser-status',
  'schema-error-browser-status',
  'native-schema-drift-browser-status',
  'replication-error-browser-status',
  'replication-push-contract-error-browser-status',
  'file-chunk-metadata-error-browser-status',
  'file-chunk-tombstone-error-browser-status',
  'file-chunk-stale-generation-error-browser-status',
  'business-os-ui-regression',
].includes(smokeMode) && !useAppDb) {
  throw new Error(`SMOKE_MODE=${smokeMode} requires an app shell SMOKE_PAGE_PATH such as /index.html or /business-os#ctox`);
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

function paethPredictor(left, up, upLeft) {
  const estimate = left + up - upLeft;
  const leftDistance = Math.abs(estimate - left);
  const upDistance = Math.abs(estimate - up);
  const upLeftDistance = Math.abs(estimate - upLeft);
  if (leftDistance <= upDistance && leftDistance <= upLeftDistance) return left;
  if (upDistance <= upLeftDistance) return up;
  return upLeft;
}

function analyzePngScreenshot(buffer) {
  const signature = '89504e470d0a1a0a';
  if (!Buffer.isBuffer(buffer) || buffer.subarray(0, 8).toString('hex') !== signature) {
    throw new Error('Business OS visual evidence screenshot is not a PNG');
  }
  let offset = 8;
  let width = 0;
  let height = 0;
  let bitDepth = 0;
  let colorType = 0;
  const idatChunks = [];
  while (offset + 12 <= buffer.length) {
    const length = buffer.readUInt32BE(offset);
    const type = buffer.subarray(offset + 4, offset + 8).toString('ascii');
    const dataStart = offset + 8;
    const dataEnd = dataStart + length;
    if (dataEnd + 4 > buffer.length) break;
    const data = buffer.subarray(dataStart, dataEnd);
    if (type === 'IHDR') {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8];
      colorType = data[9];
    } else if (type === 'IDAT') {
      idatChunks.push(data);
    } else if (type === 'IEND') {
      break;
    }
    offset = dataEnd + 4;
  }
  const channels = colorType === 6 ? 4 : colorType === 2 ? 3 : 0;
  if (!width || !height || bitDepth !== 8 || !channels || idatChunks.length === 0) {
    throw new Error(`Unsupported PNG screenshot format: ${JSON.stringify({ width, height, bitDepth, colorType, idatChunks: idatChunks.length })}`);
  }
  const inflated = zlib.inflateSync(Buffer.concat(idatChunks));
  const stride = width * channels;
  const raw = Buffer.alloc(width * height * channels);
  let sourceOffset = 0;
  for (let y = 0; y < height; y++) {
    const filter = inflated[sourceOffset++];
    const rowOffset = y * stride;
    const prevRowOffset = (y - 1) * stride;
    for (let x = 0; x < stride; x++) {
      const value = inflated[sourceOffset++];
      const left = x >= channels ? raw[rowOffset + x - channels] : 0;
      const up = y > 0 ? raw[prevRowOffset + x] : 0;
      const upLeft = y > 0 && x >= channels ? raw[prevRowOffset + x - channels] : 0;
      if (filter === 0) {
        raw[rowOffset + x] = value;
      } else if (filter === 1) {
        raw[rowOffset + x] = (value + left) & 255;
      } else if (filter === 2) {
        raw[rowOffset + x] = (value + up) & 255;
      } else if (filter === 3) {
        raw[rowOffset + x] = (value + Math.floor((left + up) / 2)) & 255;
      } else if (filter === 4) {
        raw[rowOffset + x] = (value + paethPredictor(left, up, upLeft)) & 255;
      } else {
        throw new Error(`Unsupported PNG filter type ${filter}`);
      }
    }
  }

  const stepX = Math.max(1, Math.floor(width / 96));
  const stepY = Math.max(1, Math.floor(height / 54));
  const colors = new Map();
  let sampleCount = 0;
  let visibleSamples = 0;
  let lumaSum = 0;
  let lumaSquaredSum = 0;
  for (let y = 0; y < height; y += stepY) {
    for (let x = 0; x < width; x += stepX) {
      const index = (y * width + x) * channels;
      const red = raw[index];
      const green = raw[index + 1];
      const blue = raw[index + 2];
      const alpha = channels === 4 ? raw[index + 3] : 255;
      if (alpha > 8) visibleSamples++;
      const luma = 0.2126 * red + 0.7152 * green + 0.0722 * blue;
      lumaSum += luma;
      lumaSquaredSum += luma * luma;
      const bucket = `${red >> 3},${green >> 3},${blue >> 3}`;
      colors.set(bucket, (colors.get(bucket) || 0) + 1);
      sampleCount++;
    }
  }
  const mean = sampleCount ? lumaSum / sampleCount : 0;
  const variance = sampleCount ? Math.max(0, (lumaSquaredSum / sampleCount) - (mean * mean)) : 0;
  const dominantCount = Math.max(0, ...colors.values());
  return {
    width,
    height,
    sampleCount,
    visibleSamples,
    uniqueSampledColors: colors.size,
    luminanceStdDev: Math.round(Math.sqrt(variance) * 10) / 10,
    dominantColorRatioPct: sampleCount ? Math.round((dominantCount / sampleCount) * 1000) / 10 : 100,
  };
}

async function captureBusinessOsVisualScreenshotEvidence(page) {
  const buffer = await page.screenshot({ fullPage: false });
  if (process.env.SMOKE_VISUAL_SCREENSHOT_PATH) {
    fs.writeFileSync(process.env.SMOKE_VISUAL_SCREENSHOT_PATH, buffer);
  }
  const evidence = analyzePngScreenshot(buffer);
  const problems = [];
  if (evidence.width < 900 || evidence.height < 600) {
    problems.push(`viewport too small: ${evidence.width}x${evidence.height}`);
  }
  if (evidence.uniqueSampledColors < 48) {
    problems.push(`too few sampled colors: ${evidence.uniqueSampledColors}`);
  }
  if (evidence.luminanceStdDev < 8) {
    problems.push(`low luminance variation: ${evidence.luminanceStdDev}`);
  }
  if (evidence.dominantColorRatioPct > 92) {
    problems.push(`dominant color ratio too high: ${evidence.dominantColorRatioPct}%`);
  }
  if (evidence.visibleSamples < Math.floor(evidence.sampleCount * 0.98)) {
    problems.push(`transparent sample ratio too high: ${evidence.visibleSamples}/${evidence.sampleCount}`);
  }
  if (problems.length) {
    throw new Error(`Business OS visual screenshot evidence failed: ${problems.join('; ')} ${JSON.stringify(evidence)}`);
  }
  return evidence;
}

function trackSmokeChild(child) {
  if (!child) return child;
  smokeChildren.add(child);
  child.once('exit', () => smokeChildren.delete(child));
  return child;
}

function killTrackedSmokeChildren(signal = 'SIGTERM') {
  for (const child of smokeChildren) {
    if (child.exitCode !== null || child.signalCode !== null) continue;
    try {
      child.kill(signal);
    } catch {
      // Best-effort cleanup during process shutdown.
    }
  }
}

function protectBusinessOsSourceFiles() {
  const protectedPaths = [
    path.join(root, 'src/apps/business-os/app.js'),
    path.join(root, 'src/apps/business-os/index.html'),
  ];
  const snapshots = protectedPaths
    .filter((file) => fs.existsSync(file))
    .map((file) => ({ file, content: fs.readFileSync(file, 'utf8') }));
  let restored = false;
  const restore = () => {
    if (restored) return;
    restored = true;
    const restoredFiles = [];
    for (const snapshot of snapshots) {
      try {
        if (fs.existsSync(snapshot.file) && fs.readFileSync(snapshot.file, 'utf8') === snapshot.content) {
          continue;
        }
        fs.writeFileSync(snapshot.file, snapshot.content);
        restoredFiles.push(path.relative(root, snapshot.file));
      } catch (error) {
        process.stderr.write(`[business-os-source-guard] failed to restore ${snapshot.file}: ${error?.message || error}\n`);
      }
    }
    if (restoredFiles.length) {
      process.stderr.write(`business_os_source_restored=${restoredFiles.join(',')}\n`);
    }
  };
  process.once('exit', () => {
    killTrackedSmokeChildren('SIGKILL');
    restore();
  });
  for (const signal of ['SIGINT', 'SIGTERM']) {
    process.once(signal, () => {
      killTrackedSmokeChildren('SIGTERM');
      restore();
      process.exit(signal === 'SIGINT' ? 130 : 143);
    });
  }
  return restore;
}

function prepareSmokeRoot(targetRoot) {
  fs.mkdirSync(path.join(targetRoot, 'runtime'), { recursive: true });
  for (const entry of ['Cargo.toml', 'contracts']) {
    const target = path.join(targetRoot, entry);
    if (fs.existsSync(target)) continue;
    fs.symlinkSync(path.join(root, entry), target, entry === 'Cargo.toml' ? 'file' : 'dir');
  }
  prepareSmokeSourceRoot(targetRoot);
}

function prepareSmokeSourceRoot(targetRoot) {
  const sourceRoot = path.join(root, 'src');
  const targetSourceRoot = path.join(targetRoot, 'src');
  const targetAppsRoot = path.join(targetSourceRoot, 'apps');
  fs.mkdirSync(targetAppsRoot, { recursive: true });

  for (const entry of fs.readdirSync(sourceRoot)) {
    if (entry === 'apps') continue;
    const target = path.join(targetSourceRoot, entry);
    if (fs.existsSync(target)) continue;
    fs.symlinkSync(path.join(sourceRoot, entry), target, 'dir');
  }

  const sourceAppsRoot = path.join(sourceRoot, 'apps');
  for (const entry of fs.readdirSync(sourceAppsRoot)) {
    if (entry === 'business-os') continue;
    const target = path.join(targetAppsRoot, entry);
    if (fs.existsSync(target)) continue;
    fs.symlinkSync(path.join(sourceAppsRoot, entry), target, 'dir');
  }

  const businessOsSource = path.join(sourceAppsRoot, 'business-os');
  const businessOsTarget = path.join(targetAppsRoot, 'business-os');
  if (fs.existsSync(businessOsTarget)) return;
  fs.mkdirSync(businessOsTarget, { recursive: true });
  for (const entry of fs.readdirSync(businessOsSource)) {
    const source = path.join(businessOsSource, entry);
    const target = path.join(businessOsTarget, entry);
    if (entry === 'app.js' || entry === 'index.html') {
      fs.copyFileSync(source, target);
      continue;
    }
    const type = fs.statSync(source).isDirectory() ? 'dir' : 'file';
    fs.symlinkSync(source, target, type);
  }
}

function removeSmokePath(targetPath) {
  if (keepSmokeArtifacts || !targetPath) return;
  try {
    fs.rmSync(targetPath, {
      recursive: true,
      force: true,
      maxRetries: 3,
      retryDelay: 100,
    });
  } catch {
    // Best-effort cleanup only; smoke correctness is reported by the test assertions.
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

async function startSignalingServer() {
  if (process.env.SMOKE_INLINE_SIGNALING !== '1' && smokeMode !== 'signaling-error-browser-status') {
    return startExternalSignalingServer();
  }
  return startInlineSignalingServer();
}

async function startExternalSignalingServer() {
  const script = path.join(root, 'src/core/rxdb/tools/local_signaling_server.js');
  const startupWaitMs = Number(process.env.SMOKE_SIGNALING_START_WAIT_MS || '20000');
  const child = trackSmokeChild(spawn(process.execPath, [script, String(signalingPort)], {
    cwd: root,
    env: {
      ...process.env,
      SIGNALING_HOST: '127.0.0.1',
      SIGNALING_PORT: String(signalingPort),
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  }));
  child.__ctoxExternalSignaling = true;
  let resolveListening;
  let rejectListening;
  let sawListening = false;
  child.__ctoxListening = new Promise((resolve, reject) => {
    resolveListening = resolve;
    rejectListening = reject;
  });
  child.stdout.on('data', (d) => {
    const text = d.toString();
    if (signalingDebug) process.stdout.write(`[signaling] ${d}`);
    if (text.includes('CTOX RxDB signaling listening')) {
      sawListening = true;
      resolveListening?.();
    }
  });
  child.stderr.on('data', (d) => process.stderr.write(`[signaling:err] ${d}`));
  child.on('exit', (code, signal) => {
    if (signalingDebug) console.error(`[signaling:exit] code=${code} signal=${signal}`);
    if (!sawListening) rejectListening?.(new Error(`signaling server exited before listening: code=${code} signal=${signal}`));
  });
  try {
    await waitForChildReady(child.__ctoxListening, startupWaitMs, 'signaling server');
    await waitForTcpPort('127.0.0.1', signalingPort, startupWaitMs, child);
    return child;
  } catch (error) {
    child.kill('SIGTERM');
    await new Promise((resolve) => {
      const timeout = setTimeout(resolve, 1000);
      child.once('exit', () => {
        clearTimeout(timeout);
        resolve();
      });
    });
    if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
    throw error;
  }
}

function waitForChildReady(promise, timeoutMs, label) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`timeout waiting for ${label} startup output`)), timeoutMs);
    Promise.resolve(promise)
      .then(() => {
        clearTimeout(timer);
        resolve();
      })
      .catch((error) => {
        clearTimeout(timer);
        reject(error);
      });
  });
}

function waitForTcpPort(host, port, timeoutMs, child) {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const check = () => {
      if (child && (child.exitCode !== null || child.signalCode !== null)) {
        reject(new Error(`signaling server exited before listening: code=${child.exitCode} signal=${child.signalCode}`));
        return;
      }
      const socket = net.createConnection({ host, port });
      let settled = false;
      socket.once('connect', () => {
        settled = true;
        socket.end();
        resolve();
      });
      socket.once('error', () => {
        if (settled) return;
        socket.destroy();
        if (Date.now() >= deadline) {
          reject(new Error(`timeout waiting for signaling server on ${host}:${port}`));
        } else {
          setTimeout(check, 50);
        }
      });
    };
    check();
  });
}

function startInlineSignalingServer() {
  const peers = new Map();
  const rooms = new Map();
  const sockets = new Set();
  const knownRoles = new Set(['browser', 'ctox_instance', 'desktop_shell', 'desktop_terminal', 'ctox_desktop_app']);

  function metadataFromHandshake(header) {
    const requestLine = header.split('\r\n')[0] || '';
    const target = requestLine.split(' ')[1] || '/';
    try {
      const url = new URL(target, 'ws://local');
      const client = (url.searchParams.get('client') || '').trim();
      const role = normalizeRole(url.searchParams.get('role') || url.searchParams.get('peer_role') || '', client);
      return {
        client,
        role,
        instanceId: (url.searchParams.get('instance_id') || url.searchParams.get('instance') || '').trim(),
        protocol: (url.searchParams.get('protocol') || '').trim(),
        capabilities: parseCapabilities(url),
      };
    } catch {
      return { client: '', role: 'unknown', instanceId: '', protocol: '', capabilities: [] };
    }
  }

  function normalizeRole(value, client) {
    const role = String(value || '').trim();
    if (knownRoles.has(role)) return role;
    const normalizedClient = String(client || '').toLowerCase();
    if (normalizedClient.includes('business') || normalizedClient.includes('browser')) return 'browser';
    if (normalizedClient.includes('ctox')) return 'ctox_instance';
    if (normalizedClient.includes('desktop')) return 'desktop_shell';
    return 'unknown';
  }

  function parseCapabilities(url) {
    return [
      ...url.searchParams.getAll('cap'),
      ...url.searchParams.getAll('capability'),
      ...url.searchParams.getAll('capabilities'),
    ]
      .join(',')
      .split(/[,\s]+/)
      .map((entry) => entry.trim())
      .filter(Boolean);
  }

  function peerSummary(peer) {
    if (!peer) return null;
    return {
      peerId: peer.id,
      role: peer.role || 'unknown',
      protocol: peer.protocol || '',
      instanceId: peer.instanceId || '',
      client: peer.client || '',
      capabilities: Array.isArray(peer.capabilities) ? peer.capabilities : [],
    };
  }

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
      const otherPeerIds = Array.from(room);
      const peerDescriptors = otherPeerIds.map((id) => peerSummary(peers.get(id))).filter(Boolean);
      if (signalingDebug) console.error(`[smoke-signaling] joined room=${roomId} peers=${room.size}`);
      for (const id of room) {
        peers.get(id)?.send({ type: 'joined', otherPeerIds, peers: peerDescriptors });
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
        peer = {
          id: token(),
          ...metadataFromHandshake(header),
          rooms: new Set(),
          send,
          injectedControlPlaneError: false,
        };
        peers.set(peer.id, peer);
        if (signalingDebug) console.error(`[smoke-signaling] open peer=${peer.id} role=${peer.role} instance=${peer.instanceId || '-'} protocol=${peer.protocol || '-'} caps=${peer.capabilities.join(',') || '-'}`);
        send({ type: 'init', yourPeerId: peer.id, peer: peerSummary(peer) });
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
          if (signalingDebug) {
            const data = msg.data || msg.signal || {};
            const signalType = data?.type || (data?.sdp ? 'sdp' : (data?.candidate ? 'candidate' : 'unknown'));
            const candidateLine = typeof data?.candidate?.candidate === 'string'
              ? data.candidate.candidate
              : (typeof data?.candidate === 'string' ? data.candidate : '');
            const candidateType = /\styp\s+([a-z0-9-]+)/i.exec(candidateLine)?.[1] || '';
            const candidateAddress = candidateLine
              .replace(/^candidate:\S+\s+\S+\s+\S+\s+\S+\s+/i, '')
              .split(/\s+/)
              .slice(0, 2)
              .join(':')
              .slice(0, 80);
            const sdpBytes = typeof data?.sdp === 'string' ? Buffer.byteLength(data.sdp) : 0;
            console.error(`[smoke-signaling] signal from=${peer.id} to=${msg.receiverPeerId} receiver=${peers.has(msg.receiverPeerId) ? 'yes' : 'no'} room=${msg.room} type=${signalType} candidateType=${candidateType || '-'} candidate=${candidateAddress || '-'} sdpBytes=${sdpBytes}`);
          }
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
  if (server.__ctoxExternalSignaling) {
    server.kill('SIGINT');
    await withHostTimeout(new Promise((resolve) => server.once('exit', resolve)), 5000);
    if (server.exitCode === null && server.signalCode === null) {
      server.kill('SIGKILL');
      await withHostTimeout(new Promise((resolve) => server.once('exit', resolve)), 2000);
    }
    return;
  }
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
    if (globalThis.__ctoxProcess
      && (globalThis.__ctoxProcess.exitCode !== null || globalThis.__ctoxProcess.signalCode !== null)) {
      throw new Error(`ctox exited before ${url}: code=${globalThis.__ctoxProcess.exitCode} signal=${globalThis.__ctoxProcess.signalCode}`);
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

async function waitForLaunchSyncConfig(ms = 20000) {
  const url = `http://127.0.0.1:${businessPort}/index.html?rxdbSmoke=1`;
  const deadline = Date.now() + ms;
  let lastError = null;
  while (Date.now() < deadline) {
    if (globalThis.__ctoxProcess
      && (globalThis.__ctoxProcess.exitCode !== null || globalThis.__ctoxProcess.signalCode !== null)) {
      throw new Error(`ctox exited before launch sync config: code=${globalThis.__ctoxProcess.exitCode} signal=${globalThis.__ctoxProcess.signalCode}`);
    }
    try {
      const res = await fetch(url);
      if (res.ok) {
        const html = await res.text();
        const config = parseLaunchSyncConfig(html);
        if (config) return config;
      }
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`timeout waiting for launch sync config${lastError ? `: ${lastError.message}` : ''}`);
}

function parseLaunchSyncConfig(html) {
  const marker = 'window.CTOX_BUSINESS_OS_CONFIG=';
  const start = html.indexOf(marker);
  if (start === -1) return null;
  const bodyStart = start + marker.length;
  const end = html.indexOf(';</script>', bodyStart);
  if (end === -1) return null;
  return JSON.parse(html.slice(bodyStart, end));
}

async function waitForNativePeerSyncConfig(ms = 60000) {
  const deadline = Date.now() + ms;
  let lastConfig = null;
  while (Date.now() < deadline) {
    lastConfig = await waitForLaunchSyncConfig(Math.min(2000, Math.max(500, deadline - Date.now())));
    const status = lastConfig?.native_rxdb_peer_status || {};
    if (lastConfig?.native_rxdb_peer_available === true && status.peer_session_id) {
      return lastConfig;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`timeout waiting for native RxDB peer sync config: ${JSON.stringify(lastConfig)}`);
}

function sqlite(statement) {
  const deadline = Date.now() + 20000;
  let lastOutput = '';
  while (Date.now() <= deadline) {
    const result = spawnSync('/usr/bin/sqlite3', [
      '-cmd',
      '.timeout 10000',
      sqlitePath,
      statement,
    ], { encoding: 'utf8' });
    if (result.status === 0) return result.stdout;
    lastOutput = result.stderr || result.stdout || '';
    if (!/database is locked|SQLITE_BUSY/i.test(lastOutput)) {
      throw new Error(`sqlite failed: ${lastOutput}`);
    }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 250);
  }
  throw new Error(`sqlite failed: ${lastOutput}`);
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

function pollSqliteJson(tableName, id, ms = 30000) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const row = sqlite(`SELECT data FROM ${quoteSqlIdentifier(tableName)} WHERE id='${sqlString(id)}' LIMIT 1;`).trim();
    if (row) return JSON.parse(row);
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
  }
  throw new Error(`sqlite row not replicated for ${tableName}.${id}`);
}

function sqliteTableExists(tableName) {
  return sqlite(`SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='${sqlString(tableName)}';`).trim() === '1';
}

function sqliteRowCount(tableName, whereClause = '1=1') {
  if (!sqliteTableExists(tableName)) return 0;
  const out = sqlite(`SELECT COUNT(*) FROM ${quoteSqlIdentifier(tableName)} WHERE ${whereClause};`).trim();
  return Number(out || 0);
}

function seedNativeOptionalSchemaDriftFixture() {
  const schemaPath = path.join(root, 'src/core/business_os/business_os_schema_contract.json');
  const contract = JSON.parse(fs.readFileSync(schemaPath, 'utf8'));
  const driftedSchema = typeof structuredClone === 'function'
    ? structuredClone(contract.outbound_messages)
    : JSON.parse(JSON.stringify(contract.outbound_messages));
  if (!driftedSchema || typeof driftedSchema !== 'object') {
    throw new Error('outbound_messages schema missing from Business OS schema contract');
  }
  driftedSchema.xLegacyDrift = 'legacy optional schema';
  const schemaVersion = Number.isInteger(Number(driftedSchema.version))
    ? Number(driftedSchema.version)
    : 0;
  const collectionKey = `outbound_messages-${schemaVersion}`;
  const internalTable = 'ctox_business_os___rxdb_internal__v0';
  const lwt = Date.now();
  const document = {
    id: `collection|${collectionKey}`,
    key: collectionKey,
    context: 'collection',
    data: {
      name: 'outbound_messages',
      schemaHash: 'legacy-outbound-messages-schema-hash',
      schema: driftedSchema,
      version: schemaVersion,
      connectedStorages: [],
    },
    _deleted: false,
    _meta: { lwt },
    _rev: '1-native-schema-drift-smoke',
    _attachments: {},
  };
  sqlite(`
    CREATE TABLE IF NOT EXISTS ${quoteSqlIdentifier(internalTable)}(
      id TEXT NOT NULL PRIMARY KEY UNIQUE,
      revision TEXT,
      deleted INTEGER NOT NULL CHECK (deleted IN (0, 1)),
      lastWriteTime REAL NOT NULL,
      data TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS ${quoteSqlIdentifier(`${internalTable}_lwt_id_idx`)}
      ON ${quoteSqlIdentifier(internalTable)}(lastWriteTime, id);
    CREATE INDEX IF NOT EXISTS ${quoteSqlIdentifier(`${internalTable}_deleted_lwt_id_idx`)}
      ON ${quoteSqlIdentifier(internalTable)}(deleted, lastWriteTime, id);
    INSERT OR REPLACE INTO ${quoteSqlIdentifier(internalTable)}
      (id, revision, deleted, lastWriteTime, data)
    VALUES (
      '${sqlString(document.id)}',
      '${sqlString(document._rev)}',
      0,
      ${lwt},
      '${sqlString(JSON.stringify(document))}'
    );
  `);
  return { table: internalTable, collection: 'outbound_messages', documentId: document.id };
}

function sqlString(value) {
  return String(value).replaceAll("'", "''");
}

function quoteSqlIdentifier(identifier) {
  return `"${String(identifier).replaceAll('"', '""')}"`;
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
  if (!isWebRtcStatusMode(status.sync?.mode)) problems.push(`sync.mode is ${JSON.stringify(status.sync?.mode)}`);
  if (status.sync?.protocol !== 'ctox-rxdb-protocol-v1') {
    problems.push(`sync.protocol is ${JSON.stringify(status.sync?.protocol)}`);
  }
  if (status.checks?.rxdbRuntimeAppLocal !== true) {
    problems.push(`checks.rxdbRuntimeAppLocal is ${JSON.stringify(status.checks?.rxdbRuntimeAppLocal)}`);
  }
  if (status.rxdbRuntime?.name !== 'ctox-rxdb-js'
    || status.rxdbRuntime?.publicName !== 'CTOX DB'
    || status.rxdbRuntime?.source !== 'app-local'
    || status.rxdbRuntime?.packageManager !== 'none'
    || status.rxdbRuntime?.apiContract !== 'ctox-db-business-os-v1'
    || status.rxdbRuntime?.upstreamCompatibility !== 'not-upstream-rxdb'
    || status.rxdbRuntime?.upstreamCompatible !== false) {
    problems.push(`rxdbRuntime is not app-local ctox-rxdb-js: ${JSON.stringify(status.rxdbRuntime)}`);
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
    if (!Array.isArray(initialSync.missingCheckpointEpoch)) {
      problems.push('sync.initialSync.missingCheckpointEpoch is not an array');
    } else if (initialSync.missingCheckpointEpoch.length > 0) {
      problems.push(`sync.initialSync.missingCheckpointEpoch is not empty: ${JSON.stringify(initialSync.missingCheckpointEpoch)}`);
    }
    if (!Array.isArray(initialSync.entries) || initialSync.entries.length === 0) {
      problems.push('sync.initialSync.entries is empty');
    } else if (initialSync.entries.some((entry) => entry?.state !== 'complete' || !entry?.initialReplicationAt)) {
      problems.push(`sync.initialSync.entries contains incomplete collection: ${JSON.stringify(initialSync.entries)}`);
    } else if (initialSync.entries.some((entry) => entry?.checkpointEpochAdvertised !== true || !entry?.checkpointEpoch)) {
      problems.push(`sync.initialSync.entries missing checkpoint epoch evidence: ${JSON.stringify(initialSync.entries)}`);
    }
  }
  if (status.checks?.frameTransportRealtimeHealthy !== true) {
    problems.push(`checks.frameTransportRealtimeHealthy is ${JSON.stringify(status.checks?.frameTransportRealtimeHealthy)}`);
  }
  const frameTransport = status.sync?.frameTransport;
  if (!frameTransport || typeof frameTransport !== 'object') {
    problems.push('sync.frameTransport is missing');
  } else {
    if (frameTransport.protocol !== 'ctox-rxdb-frame-v1') {
      problems.push(`sync.frameTransport.protocol is ${JSON.stringify(frameTransport.protocol)}`);
    }
    if (!Array.isArray(frameTransport.unhealthyCollections)) {
      problems.push('sync.frameTransport.unhealthyCollections is not an array');
    } else if (frameTransport.unhealthyCollections.length > 0) {
      problems.push(`sync.frameTransport.unhealthyCollections is not empty: ${JSON.stringify(frameTransport.unhealthyCollections)}`);
    }
    if (!Array.isArray(frameTransport.entries) || frameTransport.entries.length === 0) {
      problems.push('sync.frameTransport.entries is empty');
    } else if (frameTransport.entries.some((entry) => entry?.protocol !== 'ctox-rxdb-frame-v1')) {
      problems.push(`sync.frameTransport.entries contain non-frame protocol entries: ${JSON.stringify(frameTransport.entries)}`);
    } else if (frameTransport.entries.some((entry) => Number(entry?.maxInlineFrameBytes || 0) <= 0 || Number(entry?.maxChunkChars || 0) <= 0 || Number(entry?.ackWindow || 0) <= 0)) {
      problems.push(`sync.frameTransport.entries missing frame limits: ${JSON.stringify(frameTransport.entries)}`);
    }
    if (!frameTransport.totals || typeof frameTransport.totals !== 'object') {
      problems.push('sync.frameTransport.totals is missing');
    } else if (Number(frameTransport.totals.pendingAcks || 0) > 16 || Number(frameTransport.totals.priorityQueueDepth || 0) > 128) {
      problems.push(`sync.frameTransport.totals exceed realtime thresholds: ${JSON.stringify(frameTransport.totals)}`);
    }
  }
  if (problems.length) {
    throw new Error(`Business OS advanced status contract failed: ${problems.join('; ')}\n${JSON.stringify(status, null, 2)}`);
  }
}

function isWebRtcStatusMode(mode) {
  return typeof mode === 'string' && mode.split('+').includes('webrtc');
}

async function collectStartupState(page) {
  return page.evaluate(() => ({
    url: location.href,
    title: document.title,
    readyState: document.readyState,
    hasSmoke: Boolean(globalThis.ctoxBusinessOsSmoke),
    smokeBootstrap: globalThis.ctoxBusinessOsSmoke?.bootstrap || '',
    hasAdvancedStatus: Boolean(globalThis.CTOX_BUSINESS_OS_STATUS),
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

async function assertNoVisibleBrowserDebugText(page) {
  const forbidden = await page.evaluate(() => {
    const root = document.querySelector('[data-browser-root]');
    if (!root) return [];
    const terms = [
      'Waiting for the next RxDB frame',
      'pending_command',
      'browser.session',
      'RxDB',
      'Command',
      'Seq',
    ];
    const visibleText = [];
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        const parent = node.parentElement;
        if (!parent) return NodeFilter.FILTER_REJECT;
        if (parent.closest('[hidden], .browser-diagnostics, script, style')) return NodeFilter.FILTER_REJECT;
        const style = getComputedStyle(parent);
        if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0) {
          return NodeFilter.FILTER_REJECT;
        }
        return NodeFilter.FILTER_ACCEPT;
      },
    });
    while (walker.nextNode()) {
      const text = walker.currentNode.nodeValue || '';
      if (text.trim()) visibleText.push(text);
    }
    const body = visibleText.join(' ').replace(/\s+/g, ' ');
    return terms.filter((term) => body.includes(term));
  });
  if (forbidden.length) {
    throw new Error(`Browser UI exposes debug text: ${forbidden.join(', ')}`);
  }
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

function seedRustWorkspaceArtifacts() {
  const now = Date.now();
  const workspaceName = `agent_workspace_${now}_${token(5)}`;
  const workspacePath = path.join(runtimeRoot, 'runtime/business-os/workspaces/rxdb-smoke', workspaceName);
  const artifacts = [
    {
      relativePath: 'reports/brief.md',
      content: `# Agent Brief\n\nGenerated by CTOX smoke ${now}\n`,
    },
    {
      relativePath: 'plans/runtime-plan.json',
      content: JSON.stringify({
        id: `plan_${now}`,
        status: 'ready',
        steps: ['inspect', 'edit', 'verify'],
      }, null, 2),
    },
    {
      relativePath: 'logs/codex-output.txt',
      content: `ctox agent loop wrote a workspace artifact at ${now}\nstdout: ok\n`,
    },
    {
      relativePath: 'src/generated/module-note.md',
      content: `Generated source note ${now}\n\nThis file must appear in Business OS via RxDB.\n`,
    },
  ];
  const files = artifacts.map((artifact) => {
    const filePath = path.join(workspacePath, artifact.relativePath);
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    fs.writeFileSync(filePath, artifact.content);
    const canonicalPath = fs.realpathSync(filePath);
    return {
      id: `ctox_file_${crypto.createHash('sha256').update(canonicalPath).digest('hex')}`,
      content: artifact.content,
      path: canonicalPath,
      relativePath: artifact.relativePath,
      expectedVirtualPath: `/CTOX/${workspaceName}/${artifact.relativePath}`,
    };
  });
  return {
    id: files[0].id,
    content: files[0].content,
    path: files[0].path,
    syncMode: 'workspace',
    workspacePath: fs.realpathSync(workspacePath),
    expectedVirtualPath: files[0].expectedVirtualPath,
    files,
  };
}

function seedRustWorkspaceArtifactStress() {
  const now = Date.now();
  const workspaceName = `agent_stress_${now}_${token(5)}`;
  const workspacePath = path.join(runtimeRoot, 'runtime/business-os/workspaces/rxdb-smoke', workspaceName);
  const largeBlock = (label) => Array.from({ length: 6200 }, (_, index) => `${label}:${index}:${now}:ctox-rxdb-stress\n`).join('');
  const artifacts = [
    { relativePath: 'reports/brief.md', content: `# Stress Brief\n\nGenerated ${now}\n` },
    { relativePath: 'reports/summary.md', content: `# Summary\n\nAll agent artifacts must sync over RxDB/WebRTC.\n${now}\n` },
    { relativePath: 'plans/runtime-plan.json', content: JSON.stringify({ id: `plan_${now}`, status: 'ready', steps: ['inspect', 'edit', 'verify', 'publish'] }, null, 2) },
    { relativePath: 'plans/retry-plan.json', content: JSON.stringify({ id: `retry_${now}`, retries: [0, 1, 2], policy: 'idempotent' }, null, 2) },
    { relativePath: 'logs/codex-output.txt', content: `ctox agent loop wrote artifacts at ${now}\nstdout: ok\n` },
    { relativePath: 'logs/tool-calls.jsonl', content: `${JSON.stringify({ t: now, tool: 'apply_patch', ok: true })}\n${JSON.stringify({ t: now + 1, tool: 'cargo test', ok: true })}\n` },
    { relativePath: 'src/generated/module-note.md', content: `Generated source note ${now}\n` },
    { relativePath: 'src/generated/config.toml', content: `generated_at = ${now}\nmode = "rxdb-webrtc"\n` },
    { relativePath: 'docs/checklist.md', content: `- [x] rxdb\n- [x] sqlite\n- [x] business-os\n${now}\n` },
    { relativePath: 'docs/notes/session.md', content: `Session ${now}\n\nNo HTTP data-plane fallback.\n` },
    { relativePath: 'artifacts/table.csv', content: `name,status,ts\nrxdb,ok,${now}\nwebrtc,ok,${now + 1}\n` },
    { relativePath: 'artifacts/state.json', content: JSON.stringify({ generatedAt: now, state: 'synced', collections: ['desktop_files', 'desktop_file_chunks'] }, null, 2) },
    { relativePath: 'large/transcript-a.txt', content: largeBlock('transcript-a') },
    { relativePath: 'large/transcript-b.txt', content: largeBlock('transcript-b') },
    { relativePath: 'large/report-c.txt', content: largeBlock('report-c') },
    { relativePath: 'large/report-d.txt', content: largeBlock('report-d') },
  ];
  const files = artifacts.map((artifact) => {
    const filePath = path.join(workspacePath, artifact.relativePath);
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    fs.writeFileSync(filePath, artifact.content);
    const canonicalPath = fs.realpathSync(filePath);
    return {
      id: `ctox_file_${crypto.createHash('sha256').update(canonicalPath).digest('hex')}`,
      content: artifact.content,
      path: canonicalPath,
      relativePath: artifact.relativePath,
      expectedVirtualPath: `/CTOX/${workspaceName}/${artifact.relativePath}`,
    };
  });
  return {
    id: files[0].id,
    content: files[0].content,
    path: files[0].path,
    syncMode: 'workspace',
    workspacePath: fs.realpathSync(workspacePath),
    expectedVirtualPath: files[0].expectedVirtualPath,
    files,
    stress: true,
  };
}

function mutateRustWorkspaceArtifactStress(seed) {
  if (!seed?.workspacePath || !Array.isArray(seed.files)) {
    throw new Error('workspace artifact churn requires a stress seed');
  }
  const now = Date.now();
  const byRelativePath = new Map(seed.files.map((file) => [file.relativePath, file]));
  const updates = [
    ['reports/summary.md', `# Summary\n\nUpdated during churn ${now}\nNo stale chunk generation may be shown.\n`],
    ['plans/runtime-plan.json', JSON.stringify({ id: `plan_${now}`, status: 'updated', steps: ['inspect', 'edit', 'verify', 'publish', 'churn'] }, null, 2)],
    ['large/transcript-a.txt', Array.from({ length: 6900 }, (_, index) => `transcript-a-updated:${index}:${now}:ctox-rxdb-churn\n`).join('')],
    ['large/report-c.txt', Array.from({ length: 6700 }, (_, index) => `report-c-updated:${index}:${now}:ctox-rxdb-churn\n`).join('')],
  ];
  const added = [
    ['reports/churn-result.md', `# Churn Result\n\nAdded ${now}\n`],
    ['logs/churn-events.jsonl', `${JSON.stringify({ t: now, event: 'created' })}\n${JSON.stringify({ t: now + 1, event: 'synced' })}\n`],
    ['src/generated/churn.rs', `pub const CHURN_TS: u128 = ${now};\n`],
    ['large/churn-delta.txt', Array.from({ length: 5200 }, (_, index) => `delta:${index}:${now}:ctox-rxdb-churn\n`).join('')],
  ];

  for (const [relativePath, content] of updates) {
    const file = byRelativePath.get(relativePath);
    if (!file) throw new Error(`missing churn update target ${relativePath}`);
    fs.writeFileSync(file.path, content);
    file.content = content;
  }

  const workspaceName = path.basename(seed.workspacePath);
  const addedFiles = added.map(([relativePath, content]) => {
    const filePath = path.join(seed.workspacePath, relativePath);
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    fs.writeFileSync(filePath, content);
    const canonicalPath = fs.realpathSync(filePath);
    return {
      id: `ctox_file_${crypto.createHash('sha256').update(canonicalPath).digest('hex')}`,
      content,
      path: canonicalPath,
      relativePath,
      expectedVirtualPath: `/CTOX/${workspaceName}/${relativePath}`,
      added: true,
    };
  });
  seed.files.push(...addedFiles);
  syncRustSeedFile(seed);
  return {
    files: seed.files,
    updatedRelativePaths: updates.map(([relativePath]) => relativePath),
    addedRelativePaths: added.map(([relativePath]) => relativePath),
  };
}

function createWorkspaceQueueTaskForBackgroundIndex(seed) {
  if (!seed?.workspacePath) throw new Error('background workspace index smoke requires a workspace seed');
  const result = spawnSync(ctoxBin, [
    'queue',
    'add',
    '--title',
    `RxDB background workspace index ${Date.now()}`,
    '--prompt',
    `Index this CTOX smoke workspace through the native Business OS background scanner.\nWorkspace root: ${seed.workspacePath}`,
    '--workspace-root',
    seed.workspacePath,
    '--priority',
    'normal',
  ], {
    cwd: root,
    env: {
      ...process.env,
      CTOX_ROOT: runtimeRoot,
      CARGO_TARGET_DIR: path.join(root, 'runtime/build/core-rxdb-integration-target'),
    },
    encoding: 'utf8',
  });
  if (result.status !== 0) {
    throw new Error(`ctox queue add failed for background workspace index: ${result.stderr || result.stdout || 'no output'}`);
  }
  let parsed = null;
  try {
    parsed = JSON.parse(result.stdout || '{}');
  } catch {
    parsed = { raw: result.stdout || '' };
  }
  return {
    created: true,
    taskId: parsed?.task?.id || parsed?.task?.message_key || '',
    workspacePath: seed.workspacePath,
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

function corruptRustSeedChunkMetadata(seed) {
  const deadline = Date.now() + 60000;
  let rowId = '';
  let chunk = null;
  while (Date.now() < deadline) {
    const row = sqlite(`
      SELECT id || char(9) || data
      FROM ctox_business_os__desktop_file_chunks__v0
      WHERE json_extract(data, '$.file_id')='${sqlString(seed.id)}'
      ORDER BY CAST(json_extract(data, '$.idx') AS INTEGER), id
      LIMIT 1;
    `).trim();
    if (row) {
      const splitAt = row.indexOf('\t');
      rowId = splitAt >= 0 ? row.slice(0, splitAt) : '';
      chunk = JSON.parse(splitAt >= 0 ? row.slice(splitAt + 1) : row);
      break;
    }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
  }
  if (!rowId || !chunk) {
    throw new Error(`no SQLite chunk row found for metadata corruption: ${seed.id}`);
  }
  const now = Date.now();
  const revisionHeight = Number(String(chunk._rev || '').split('-')[0] || 1) + 1;
  const revision = `${revisionHeight}-${token(10)}`;
  chunk.size_bytes = Number(chunk.size_bytes || 0) + 1;
  chunk._rev = revision;
  chunk._meta = { ...(chunk._meta || {}), lwt: now };
  sqlite(`
    UPDATE ctox_business_os__desktop_file_chunks__v0
    SET data='${sqlString(JSON.stringify(chunk))}',
        revision='${sqlString(revision)}',
        lastWriteTime=${now}
    WHERE id='${sqlString(rowId)}';
  `);
  return {
    rowId,
    fileId: seed.id,
    revision,
    expectedSizeBytes: chunk.size_bytes,
    actualSizeBytes: String(chunk.data || '').length,
  };
}

function tombstoneRustSeedChunk(seed) {
  const deadline = Date.now() + 60000;
  let fileRowId = '';
  let file = null;
  let chunkRowId = '';
  let chunk = null;
  while (Date.now() < deadline) {
    const fileRow = sqlite(`
      SELECT id || char(9) || data
      FROM ctox_business_os__desktop_files__v0
      WHERE id='${sqlString(seed.id)}'
      LIMIT 1;
    `).trim();
    const chunkRow = sqlite(`
      SELECT id || char(9) || data
      FROM ctox_business_os__desktop_file_chunks__v0
      WHERE json_extract(data, '$.file_id')='${sqlString(seed.id)}'
      ORDER BY CAST(json_extract(data, '$.idx') AS INTEGER), id
      LIMIT 1;
    `).trim();
    if (fileRow && chunkRow) {
      const fileSplitAt = fileRow.indexOf('\t');
      const chunkSplitAt = chunkRow.indexOf('\t');
      fileRowId = fileSplitAt >= 0 ? fileRow.slice(0, fileSplitAt) : '';
      chunkRowId = chunkSplitAt >= 0 ? chunkRow.slice(0, chunkSplitAt) : '';
      file = JSON.parse(fileSplitAt >= 0 ? fileRow.slice(fileSplitAt + 1) : fileRow);
      chunk = JSON.parse(chunkSplitAt >= 0 ? chunkRow.slice(chunkSplitAt + 1) : chunkRow);
      break;
    }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
  }
  if (!fileRowId || !file || !chunkRowId || !chunk) {
    throw new Error(`no SQLite file/chunk row found for tombstone corruption: ${seed.id}`);
  }
  const now = Date.now();
  const fileRevisionHeight = Number(String(file._rev || '').split('-')[0] || 1) + 1;
  const chunkRevisionHeight = Number(String(chunk._rev || '').split('-')[0] || 1) + 1;
  const fileRevision = `${fileRevisionHeight}-${token(10)}`;
  const chunkRevision = `${chunkRevisionHeight}-${token(10)}`;
  file.path = '';
  file.local_path = '';
  file.content_state = 'available';
  file._rev = fileRevision;
  file._meta = { ...(file._meta || {}), lwt: now };
  chunk._deleted = true;
  chunk.deleted = true;
  chunk.is_deleted = true;
  chunk._rev = chunkRevision;
  chunk._meta = { ...(chunk._meta || {}), lwt: now + 1 };
  sqlite(`
    UPDATE ctox_business_os__desktop_files__v0
    SET data='${sqlString(JSON.stringify(file))}',
        revision='${sqlString(fileRevision)}',
        lastWriteTime=${now}
    WHERE id='${sqlString(fileRowId)}';
  `);
  sqlite(`
    UPDATE ctox_business_os__desktop_file_chunks__v0
    SET data='${sqlString(JSON.stringify(chunk))}',
        revision='${sqlString(chunkRevision)}',
        lastWriteTime=${now + 1}
    WHERE id='${sqlString(chunkRowId)}';
  `);
  return {
    fileRowId,
    chunkRowId,
    fileId: seed.id,
    fileRevision,
    chunkRevision,
  };
}

function staleRustSeedChunkGeneration(seed) {
  const deadline = Date.now() + 60000;
  let fileRowId = '';
  let file = null;
  let chunks = [];
  while (Date.now() < deadline) {
    const fileRow = sqlite(`
      SELECT id || char(9) || data
      FROM ctox_business_os__desktop_files__v0
      WHERE id='${sqlString(seed.id)}'
      LIMIT 1;
    `).trim();
    const chunkRows = sqlite(`
      SELECT id || char(9) || data
      FROM ctox_business_os__desktop_file_chunks__v0
      WHERE json_extract(data, '$.file_id')='${sqlString(seed.id)}'
      ORDER BY CAST(json_extract(data, '$.idx') AS INTEGER), id;
    `).trim().split(/\n+/).filter(Boolean);
    if (fileRow && chunkRows.length) {
      const fileSplitAt = fileRow.indexOf('\t');
      fileRowId = fileSplitAt >= 0 ? fileRow.slice(0, fileSplitAt) : '';
      file = JSON.parse(fileSplitAt >= 0 ? fileRow.slice(fileSplitAt + 1) : fileRow);
      chunks = chunkRows.map((row) => {
        const splitAt = row.indexOf('\t');
        const rowId = splitAt >= 0 ? row.slice(0, splitAt) : '';
        const data = JSON.parse(splitAt >= 0 ? row.slice(splitAt + 1) : row);
        return { rowId, data };
      });
      break;
    }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
  }
  if (!fileRowId || !file || !chunks.length) {
    throw new Error(`no SQLite file/chunk rows found for stale generation corruption: ${seed.id}`);
  }
  const now = Date.now();
  const requestedGenerationId = `gen_missing_${token(16)}`;
  const fileRevisionHeight = Number(String(file._rev || '').split('-')[0] || 1) + 1;
  const fileRevision = `${fileRevisionHeight}-${token(10)}`;
  file.path = '';
  file.local_path = '';
  file.content_state = 'available';
  file.content_generation_id = requestedGenerationId;
  file._rev = fileRevision;
  file._meta = { ...(file._meta || {}), lwt: now };
  sqlite(`
    UPDATE ctox_business_os__desktop_files__v0
    SET data='${sqlString(JSON.stringify(file))}',
        revision='${sqlString(fileRevision)}',
        lastWriteTime=${now}
    WHERE id='${sqlString(fileRowId)}';
  `);
  return {
    fileRowId,
    fileId: seed.id,
    fileRevision,
    requestedGenerationId,
    availableGenerationIds: [...new Set(chunks.map((chunk) => chunk.data.generation_id || '').filter(Boolean))],
    liveChunkCount: chunks.length,
  };
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
  const env = {
    ...process.env,
    CTOX_BUSINESS_OS_SIGNALING_URLS: signalingUrl,
    CTOX_BUSINESS_OS_ENABLE_SMOKE_CONTROLS: '1',
    CTOX_ROOT: runtimeRoot,
    CARGO_TARGET_DIR: path.join(root, 'runtime/build/core-rxdb-integration-target'),
    CTOX_BROWSER_AUTOMATION_MODULE: process.env.CTOX_BROWSER_AUTOMATION_MODULE || playwrightModule,
    CTOX_WEBRTC_UDP_BIND_ADDR: process.env.CTOX_WEBRTC_UDP_BIND_ADDR || '127.0.0.1:0',
  };
  const browserExecutable = process.env.CTOX_BROWSER_EXECUTABLE || existingChromeExecutable();
  if (browserExecutable) env.CTOX_BROWSER_EXECUTABLE = browserExecutable;
  if (smokeMode !== 'workspace-agent-artifacts-background-rust-to-browser') {
    env.CTOX_BUSINESS_OS_DISABLE_BACKGROUND_FILE_INDEX = '1';
  }
  const child = trackSmokeChild(spawn(ctoxBin, ['business-os', 'serve', '--addr', `127.0.0.1:${businessPort}`], {
    cwd: root,
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
  }));
  let resolveListening;
  let rejectListening;
  let sawListening = false;
  child.__ctoxListening = new Promise((resolve, reject) => {
    resolveListening = resolve;
    rejectListening = reject;
  });
  child.stdout.on('data', (d) => {
    const text = d.toString();
    process.stdout.write(`[ctox] ${d}`);
    if (text.includes('CTOX Business OS listening')) {
      sawListening = true;
      resolveListening?.();
    }
  });
  child.stderr.on('data', (d) => process.stderr.write(`[ctox:err] ${d}`));
  globalThis.__ctoxProcess = child;
  child.on('exit', (code, signal) => {
    if (!sawListening) rejectListening?.(new Error(`ctox exited before listening: code=${code} signal=${signal}`));
    console.log(`[ctox:exit] code=${code} signal=${signal}`);
  });
  return child;
}

async function waitForCtoxServerListening(child, ms = 60000) {
  const deadline = Date.now() + ms;
  await waitForChildReady(child?.__ctoxListening, ms, 'ctox business-os server');
  while (Date.now() < deadline) {
    if (child && child.exitCode !== null) {
      throw new Error(`ctox exited after startup output before HTTP readiness: code=${child.exitCode} signal=${child.signalCode}`);
    }
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 500);
    try {
      const response = await fetch(`http://127.0.0.1:${businessPort}/index.html?rxdbSmoke=1`, {
        signal: controller.signal,
      });
      if (response.ok) return;
    } catch {
      // The server is still starting.
    } finally {
      clearTimeout(timer);
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`timeout waiting for ctox HTTP readiness on 127.0.0.1:${businessPort}`);
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
  console.log(`smoke_root_prepare_ms=${smokeRootPrepareMs}`);
  const workspaceFileMode = smokeMode === 'workspace-rust-to-browser'
    || smokeMode === 'workspace-agent-artifacts-rust-to-browser'
    || smokeMode === 'workspace-agent-artifacts-stress-rust-to-browser'
    || smokeMode === 'workspace-agent-artifacts-churn-rust-to-browser'
    || smokeMode === 'workspace-agent-artifacts-background-rust-to-browser'
    || smokeMode === 'workspace-update-rust-to-browser'
    || smokeMode === 'workspace-large-materialize-rust-to-browser'
    || smokeMode === 'workspace-large-file-viewer-rust-to-browser'
    || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser'
    || smokeMode === 'file-chunk-metadata-error-browser-status'
    || smokeMode === 'file-chunk-tombstone-error-browser-status'
    || smokeMode === 'file-chunk-stale-generation-error-browser-status';
  const workspaceArtifactMode = smokeMode === 'workspace-agent-artifacts-rust-to-browser'
    || smokeMode === 'workspace-agent-artifacts-stress-rust-to-browser'
    || smokeMode === 'workspace-agent-artifacts-churn-rust-to-browser'
    || smokeMode === 'workspace-agent-artifacts-background-rust-to-browser';
  const rustSeed = smokeMode === 'workspace-agent-artifacts-rust-to-browser'
    || smokeMode === 'workspace-agent-artifacts-background-rust-to-browser'
    ? seedRustWorkspaceArtifacts()
    : (smokeMode === 'workspace-agent-artifacts-stress-rust-to-browser'
        || smokeMode === 'workspace-agent-artifacts-churn-rust-to-browser')
      ? seedRustWorkspaceArtifactStress()
      : workspaceFileMode
        ? seedRustWorkspaceFile({
          large: smokeMode === 'workspace-large-materialize-rust-to-browser'
            || smokeMode === 'workspace-large-file-viewer-rust-to-browser'
            || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser',
        })
        : seedRustSideFile(smokeMode === 'rust-to-browser' ? 'rust_smoke' : 'rust_ready');
  const backgroundQueueTask = smokeMode === 'workspace-agent-artifacts-background-rust-to-browser'
    ? createWorkspaceQueueTaskForBackgroundIndex(rustSeed)
    : null;
  const nativeSchemaDriftFixture = smokeMode === 'native-schema-drift-browser-status'
    ? seedNativeOptionalSchemaDriftFixture()
    : null;
  if (nativeSchemaDriftFixture) {
    console.log(`native_schema_drift_seed=${JSON.stringify(nativeSchemaDriftFixture)}`);
  }
  let ctox = startCtoxServer();
  const browserDiagnostics = {
    warnings: 0,
    websocketWarnings: 0,
    errors: 0,
    resource404Errors: 0,
    requestFailures: 0,
    assetResponseErrors: 0,
    smokeHookReloads: 0,
    smokeHookWaitMs: 0,
    cacheRepairs: 0,
    expectedNetworkFlapWarnings: 0,
    expectedNetworkFlapErrors: 0,
    expectedNetworkFlapRequestFailures: 0,
  };
  let browserDiagnosticsEmitted = false;
  function emitBrowserDiagnostics() {
    if (browserDiagnosticsEmitted) return;
    browserDiagnosticsEmitted = true;
    console.log(`browser_warning_count=${browserDiagnostics.warnings}`);
    console.log(`browser_websocket_warning_count=${browserDiagnostics.websocketWarnings}`);
    console.log(`browser_error_count=${browserDiagnostics.errors}`);
    console.log(`browser_resource_404_count=${browserDiagnostics.resource404Errors}`);
    console.log(`browser_request_failure_count=${browserDiagnostics.requestFailures}`);
    console.log(`browser_asset_response_error_count=${browserDiagnostics.assetResponseErrors}`);
    console.log(`browser_cache_repair_count=${browserDiagnostics.cacheRepairs}`);
    console.log(`expected_network_flap_warning_count=${browserDiagnostics.expectedNetworkFlapWarnings}`);
    console.log(`expected_network_flap_error_count=${browserDiagnostics.expectedNetworkFlapErrors}`);
    console.log(`expected_network_flap_request_failure_count=${browserDiagnostics.expectedNetworkFlapRequestFailures}`);
    console.log(`startup_smoke_hook_reload_count=${browserDiagnostics.smokeHookReloads}`);
    console.log(`startup_smoke_hook_wait_ms=${browserDiagnostics.smokeHookWaitMs}`);
    if (browserDiagnostics.cacheRepairs > 0) {
      throw new Error(`Business OS local RxDB cache repair was triggered during smoke: ${browserDiagnostics.cacheRepairs}`);
    }
  }
  function isExpectedNetworkFlapConsole(text) {
    return smokeMode === 'network-flap-browser-to-rust' && (
      /ERR_INTERNET_DISCONNECTED/i.test(text)
      || /ctox_signaling_socket_error/i.test(text)
      || /packaged module catalog seed unavailable/i.test(text)
    );
  }
  function isExpectedNetworkFlapRequestFailure(request) {
    const failureText = request.failure()?.errorText || '';
    return smokeMode === 'network-flap-browser-to-rust'
      && /ERR_INTERNET_DISCONNECTED/i.test(failureText)
      && request.url().includes('/business-os/modules/registry.json');
  }

  let browser;
  let browserUserDataDir = null;
  const outerPhaseTimings = {};
  try {
    const ctoxServerWaitStartedAt = Date.now();
    await waitForCtoxServerListening(ctox);
    outerPhaseTimings.ctoxServerWaitMs = Date.now() - ctoxServerWaitStartedAt;
    const configWaitStartedAt = Date.now();
    const config = await waitForLaunchSyncConfig(syncConfigWaitMs);
    outerPhaseTimings.syncConfigWaitMs = Date.now() - configWaitStartedAt;
    console.log(`ctox_sync_config_wait_ms=${outerPhaseTimings.syncConfigWaitMs}`);
    if (!config.native_rxdb_peer_available) {
      throw new Error(`native peer unavailable: ${JSON.stringify(config)}`);
    }
    if (smokeMode === 'native-schema-drift-browser-status') {
      await waitForNativePeerSyncConfig(syncConfigWaitMs);
    }
    const browserLaunchStartedAt = Date.now();
    browserUserDataDir = fs.mkdtempSync(path.join(runtimeRoot, 'browser-profile-'));
    browser = await chromium.launchPersistentContext(browserUserDataDir, chromiumLaunchOptions());
    const page = await browser.newPage();
    outerPhaseTimings.browserLaunchMs = Date.now() - browserLaunchStartedAt;
    page.on('console', (msg) => {
      const type = msg.type();
      const text = msg.text();
      if (/local RxDB cache repair triggered/i.test(text)) {
        browserDiagnostics.cacheRepairs += 1;
      }
      if (type === 'warning') {
        if (isExpectedNetworkFlapConsole(text)) {
          browserDiagnostics.expectedNetworkFlapWarnings += 1;
        } else {
          browserDiagnostics.warnings += 1;
          if (/websocket/i.test(text)) browserDiagnostics.websocketWarnings += 1;
        }
      } else if (type === 'error') {
        if (/failed to load resource/i.test(text) && /status of 404/i.test(text)) {
          browserDiagnostics.resource404Errors += 1;
        } else if (isExpectedNetworkFlapConsole(text)) {
          browserDiagnostics.expectedNetworkFlapErrors += 1;
        } else {
          browserDiagnostics.errors += 1;
        }
      }
      console.log(`[browser:${type}] ${text}`);
    });
    page.on('pageerror', (err) => {
      browserDiagnostics.errors += 1;
      console.error(`[browser:error] ${err.stack || err.message}`);
    });
    page.on('requestfailed', (request) => {
      const url = request.url();
      if (url.includes('/app.js') || url.includes('/shared/') || url.includes('/modules/') || url.includes('/vendor/')) {
        if (isExpectedNetworkFlapRequestFailure(request)) {
          browserDiagnostics.expectedNetworkFlapRequestFailures += 1;
        } else {
          browserDiagnostics.requestFailures += 1;
        }
        console.error(`[browser:requestfailed] ${request.method()} ${url} ${request.failure()?.errorText || ''}`);
      }
    });
    page.on('response', (response) => {
      const url = response.url();
      if (response.status() >= 400 && (url.includes('/app.js') || url.includes('/shared/') || url.includes('/modules/') || url.includes('/vendor/'))) {
        browserDiagnostics.assetResponseErrors += 1;
        console.error(`[browser:response] ${response.status()} ${url}`);
      }
    });
    const outboundScreenshotDir = process.env.SMOKE_OUTBOUND_SCREENSHOT_DIR
      ? path.resolve(process.env.SMOKE_OUTBOUND_SCREENSHOT_DIR)
      : '';
    if (outboundScreenshotDir) fs.mkdirSync(outboundScreenshotDir, { recursive: true });
    await page.exposeFunction('__ctoxCaptureSmokeScreenshot', async (name) => {
      if (!outboundScreenshotDir) return '';
      const safeName = String(name || 'screenshot').replace(/[^a-z0-9_.-]+/gi, '-').replace(/^-+|-+$/g, '') || 'screenshot';
      const target = path.join(outboundScreenshotDir, `${safeName}.png`);
      await page.screenshot({ path: target, fullPage: false });
      return target;
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
    await page.exposeFunction('__ctoxMutateRustWorkspaceArtifacts', () => mutateRustWorkspaceArtifactStress(rustSeed));
    await page.exposeFunction('__ctoxCreateWorkspaceQueueTaskForBackgroundIndex', () => createWorkspaceQueueTaskForBackgroundIndex(rustSeed));
    await page.exposeFunction('__ctoxCorruptRustSeedChunkMetadata', () => corruptRustSeedChunkMetadata(rustSeed));
    await page.exposeFunction('__ctoxTombstoneRustSeedChunk', () => tombstoneRustSeedChunk(rustSeed));
    await page.exposeFunction('__ctoxStaleRustSeedChunkGeneration', () => staleRustSeedChunkGeneration(rustSeed));
    await page.exposeFunction('__ctoxRestartNativePeer', async () => {
      await stopChild(ctox);
      ctox = startCtoxServer();
      await waitForCtoxServerListening(ctox);
      await waitForNativePeerSyncConfig(60000);
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
      await waitForNativePeerSyncConfig(60000);
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
      await waitForCtoxServerListening(ctox);
      await waitForNativePeerSyncConfig(60000);
      await waitForSqliteTables([
        'ctox_business_os__desktop_files__v0',
        'ctox_business_os__desktop_file_chunks__v0',
      ]);
      return true;
    });
    const browserPath = useAppDb
      ? addQueryParam(addQueryParam(pagePath, 'rxdbSmoke', '1'), 'smokeDbId', smokeDbId)
      : pagePath;
    const smokeUrl = `http://127.0.0.1:${businessPort}${browserPath}`;
    const pageGotoStartedAt = Date.now();
    await page.goto(smokeUrl, { waitUntil: 'commit', timeout: 10000 });
    outerPhaseTimings.pageGotoMs = Date.now() - pageGotoStartedAt;
    let advancedStatusEvidenceVersion = '';
    let advancedStatusEvidenceRuntime = null;
    const backgroundIndexerSmokeMode = smokeMode === 'workspace-agent-artifacts-background-rust-to-browser';
    const largeFileMaterializeSmokeMode = smokeMode === 'workspace-large-materialize-rust-to-browser'
      || smokeMode === 'workspace-large-file-viewer-rust-to-browser'
      || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser';
    const deferredFileCollectionStartupMode = backgroundIndexerSmokeMode
      || smokeMode === 'file-chunk-tombstone-error-browser-status';
    if (useAppDb) {
      let startupState = null;
      const smokeHookWaitStartedAt = Date.now();
      for (let attempt = 0; attempt < 2; attempt += 1) {
        try {
          await page.waitForFunction(() => Boolean(
            globalThis.ctoxBusinessOsSmoke
              && globalThis.ctoxBusinessOsSmoke.bootstrap !== 'inline'
              && globalThis.CTOX_BUSINESS_OS_STATUS
          ), null, { timeout: smokeHookWaitTimeoutMs });
          startupState = null;
          break;
        } catch (error) {
          startupState = await collectStartupState(page);
          if (attempt === 0 && isPreHookModuleGraphStall(startupState)) {
            console.warn(`[smoke] Business OS module graph stalled before smoke hook; reloading once: ${JSON.stringify({
              url: startupState.url,
              search: startupState.search,
              readyState: startupState.readyState,
              hasSmoke: startupState.hasSmoke,
              smokeBootstrap: startupState.smokeBootstrap,
              hasAdvancedStatus: startupState.hasAdvancedStatus,
              scriptSrcs: startupState.scriptSrcs,
              resources: startupState.resources,
              bodyDataset: startupState.bodyDataset,
            })}`);
            browserDiagnostics.smokeHookReloads += 1;
            await page.goto('about:blank', { waitUntil: 'commit', timeout: 10000 }).catch(() => {});
            await page.goto(smokeUrl, { waitUntil: 'commit', timeout: 10000 });
            continue;
          }
          break;
        }
      }
      browserDiagnostics.smokeHookWaitMs = Date.now() - smokeHookWaitStartedAt;
      outerPhaseTimings.smokeHookWaitMs = browserDiagnostics.smokeHookWaitMs;
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
        if (errorStatus.snapshot?.version) console.log(`advanced_status=${errorStatus.snapshot.version}`);
        if (errorStatus.snapshot?.rxdbRuntime) console.log(`rxdb_runtime=${JSON.stringify(errorStatus.snapshot.rxdbRuntime)}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'peer-lifecycle-browser-status') {
        const lifecycleStatus = await page.evaluate(async () => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          if (!state) return { ok: false, reason: 'missing smoke state' };
          state.session = { authenticated: true };
          state.sync = { mode: 'webrtc' };
          state.syncDiagnostics = {
            mode: 'webrtc',
            phase: 'reconnecting',
            protocol: 'ctox-rxdb-protocol-v1',
            capabilities: ['ctox-peer-session-v1'],
            collections: {
              peer_lifecycle_fixture: {
                collection: 'peer_lifecycle_fixture',
                status: 'reconnecting',
                connectionStatus: 'reconnecting',
                remoteProtocol: 'ctox-rxdb-protocol-v1',
                remoteCapabilities: ['ctox-peer-session-v1'],
                remotePeerSession: 'ctox_instance:lifecycle-fixture',
                remoteCheckpoint: {
                  source: 'ctox-rs',
                  state: 'advertised',
                  collection: 'peer_lifecycle_fixture',
                  epoch: 'peer-lifecycle-fixture-epoch',
                },
                peerGeneration: 2,
                previousPeerSession: 'ctox_instance:lifecycle-fixture-old',
                peerSessionSeenAt: new Date().toISOString(),
                reconnectingSince: new Date().toISOString(),
                lastError: null,
                lastLifecycleEvent: {
                  name: 'CtoxWebRtcPeerLifecycleEvent',
                  code: 'peer_connection_lost',
                  phase: 'peer-reconnect',
                  severity: 'recoverable',
                  retryable: true,
                  lifecycle: true,
                  message: 'WebRTC peer connection was lost; reconnect repair is scheduled.',
                },
              },
            },
            lastError: null,
            lastLifecycleEvent: {
              name: 'CtoxWebRtcPeerLifecycleEvent',
              code: 'peer_connection_lost',
              phase: 'peer-reconnect',
              severity: 'recoverable',
              retryable: true,
              lifecycle: true,
              message: 'WebRTC peer connection was lost; reconnect repair is scheduled.',
            },
          };
          const snapshot = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
            includeCounts: false,
            requiredCollections: ['peer_lifecycle_fixture'],
          });
          const lifecycleEvents = Array.isArray(snapshot?.sync?.lifecycleEvents)
            ? snapshot.sync.lifecycleEvents
            : [];
          const reconnectingCollections = Array.isArray(snapshot?.sync?.reconnectingCollections)
            ? snapshot.sync.reconnectingCollections
            : [];
          const match = lifecycleEvents.find((event) => (
            event?.name === 'CtoxWebRtcPeerLifecycleEvent' &&
            event?.code === 'peer_connection_lost' &&
            event?.phase === 'peer-reconnect' &&
            event?.severity === 'recoverable' &&
            event?.retryable === true
          ));
          return {
            ok: Boolean(
              match &&
              reconnectingCollections.includes('peer_lifecycle_fixture') &&
              snapshot?.checks?.noStalledReconnect === false &&
              snapshot?.checks?.noReplicationIoErrors === true
            ),
            error: match || null,
            snapshot,
          };
        });
        if (!lifecycleStatus?.ok) {
          throw new Error(`Business OS did not expose peer lifecycle event in advanced status: ${JSON.stringify(lifecycleStatus, null, 2)}`);
        }
        console.log(`peer_lifecycle_collection=${lifecycleStatus.error.collection}`);
        console.log(`peer_lifecycle_code=${lifecycleStatus.error.code}`);
        console.log(`peer_lifecycle_name=${lifecycleStatus.error.name}`);
        console.log(`peer_lifecycle_phase=${lifecycleStatus.error.phase}`);
        if (lifecycleStatus.snapshot?.version) console.log(`advanced_status=${lifecycleStatus.snapshot.version}`);
        if (lifecycleStatus.snapshot?.rxdbRuntime) console.log(`rxdb_runtime=${JSON.stringify(lifecycleStatus.snapshot.rxdbRuntime)}`);
        emitBrowserDiagnostics();
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
        if (errorStatus.snapshot?.version) console.log(`advanced_status=${errorStatus.snapshot.version}`);
        if (errorStatus.snapshot?.rxdbRuntime) console.log(`rxdb_runtime=${JSON.stringify(errorStatus.snapshot.rxdbRuntime)}`);
        emitBrowserDiagnostics();
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
        if (errorStatus.snapshot?.version) console.log(`advanced_status=${errorStatus.snapshot.version}`);
        if (errorStatus.snapshot?.rxdbRuntime) console.log(`rxdb_runtime=${JSON.stringify(errorStatus.snapshot.rxdbRuntime)}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'rxdb-protocol-error-browser-status') {
        const errorStatus = await page.evaluate(async () => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          if (!state) return { ok: false, reason: 'missing smoke state' };
          state.session = { authenticated: true };
          state.sync = { mode: 'webrtc' };
          state.syncDiagnostics = {
            mode: 'webrtc',
            phase: 'collection-sync',
            protocol: 'ctox-rxdb-protocol-v1',
            capabilities: ['ctox-peer-session-v1', 'ctox-schema-hash-v1', 'ctox-checkpoint-epoch-v1'],
            collections: {
              protocol_fixture: {
                collection: 'protocol_fixture',
                status: 'error',
                connectionStatus: 'error',
                remoteProtocol: 'ctox-rxdb-protocol-v0',
                remoteCapabilities: ['ctox-peer-session-v1', 'ctox-schema-hash-v1', 'ctox-checkpoint-epoch-v1'],
                remotePeerSession: 'ctox_instance:protocol-fixture',
                peerGeneration: 1,
                peerSessionSeenAt: new Date().toISOString(),
                lastError: {
                  name: 'CtoxRxdbProtocolError',
                  code: 'ctox_rxdb_protocol_mismatch',
                  phase: 'rxdb-protocol-handshake',
                  severity: 'error',
                  retryable: false,
                  expected: 'ctox-rxdb-protocol-v1',
                  actual: 'ctox-rxdb-protocol-v0',
                  message: 'Incompatible CTOX RxDB WebRTC protocol.',
                },
              },
            },
            lastError: null,
          };
          const snapshot = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
            includeCounts: false,
            requiredCollections: ['protocol_fixture'],
          });
          const schemaErrors = Array.isArray(snapshot?.sync?.schemaErrors)
            ? snapshot.sync.schemaErrors
            : [];
          const match = schemaErrors.find((error) => (
            error?.name === 'CtoxSchemaProtocolError' &&
            error?.code === 'ctox_rxdb_protocol_mismatch' &&
            error?.phase === 'schema-handshake' &&
            error?.expected === 'ctox-rxdb-protocol-v1' &&
            error?.actual === 'ctox-rxdb-protocol-v0'
          ));
          return { ok: Boolean(match && snapshot?.checks?.noSchemaProtocolErrors === false), error: match || null, snapshot };
        });
        if (!errorStatus?.ok) {
          throw new Error(`Business OS did not expose RxDB protocol incompatibility in advanced status: ${JSON.stringify(errorStatus, null, 2)}`);
        }
        console.log(`rxdb_protocol_error_collection=${errorStatus.error.collection}`);
        console.log(`rxdb_protocol_error_code=${errorStatus.error.code}`);
        console.log(`rxdb_protocol_error_name=${errorStatus.error.name}`);
        if (errorStatus.snapshot?.version) console.log(`advanced_status=${errorStatus.snapshot.version}`);
        if (errorStatus.snapshot?.rxdbRuntime) console.log(`rxdb_runtime=${JSON.stringify(errorStatus.snapshot.rxdbRuntime)}`);
        emitBrowserDiagnostics();
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
                  name: 'RxError (RC_PULL)',
                  code: 'RC_PULL',
                  phase: 'replication-pull',
                  message: 'Rust WebRTC masterChangesSince failed.',
                  parameters: {
                    type: 'ctoxError',
                    scope: 'replication',
                    rxdb: true,
                    code: 'RC_PULL',
                    phase: 'replication-pull',
                    direction: 'pull',
                    checkpoint: { sequence: 1 },
                    batchSize: 20,
                    errors: [
                      {
                        rxdb: true,
                        code: 'TEST_PULL',
                        name: 'RxError (TEST_PULL)',
                        message: 'pull failed',
                        parameters: { attempt: 1 },
                      },
                    ],
                  },
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
            error?.upstreamCode === 'RC_PULL' &&
            error?.batchSize === 20 &&
            error?.rowCount === null
          ));
          return { ok: Boolean(match && snapshot?.checks?.noReplicationIoErrors === false), error: match || null, snapshot };
        });
        if (!errorStatus?.ok) {
          throw new Error(`Business OS did not expose replication I/O error in advanced status: ${JSON.stringify(errorStatus, null, 2)}`);
        }
        console.log(`replication_error_collection=${errorStatus.error.collection}`);
        console.log(`replication_error_code=${errorStatus.error.code}`);
        console.log(`replication_error_name=${errorStatus.error.name}`);
        if (errorStatus.snapshot?.version) console.log(`advanced_status=${errorStatus.snapshot.version}`);
        if (errorStatus.snapshot?.rxdbRuntime) console.log(`rxdb_runtime=${JSON.stringify(errorStatus.snapshot.rxdbRuntime)}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'replication-push-contract-error-browser-status') {
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
              replication_push_fixture: {
                collection: 'replication_push_fixture',
                status: 'error',
                connectionStatus: 'error',
                remoteProtocol: 'ctox-rxdb-protocol-v1',
                remoteCapabilities: ['ctox-peer-session-v1'],
                remotePeerSession: 'ctox_instance:replication-push-fixture',
                remoteCheckpoint: {
                  source: 'ctox-rs',
                  state: 'advertised',
                  collection: 'replication_push_fixture',
                  epoch: 'replication-push-fixture-epoch',
                },
                peerGeneration: 1,
                peerSessionSeenAt: new Date().toISOString(),
                lastError: {
                  name: 'RxError (RC_PUSH_NO_AR)',
                  code: 'RC_PUSH_NO_AR',
                  phase: 'replication-push',
                  message: 'Rust WebRTC masterWrite returned an invalid push contract.',
                  parameters: {
                    type: 'ctoxError',
                    scope: 'replication',
                    rxdb: true,
                    code: 'RC_PUSH_NO_AR',
                    phase: 'replication-push',
                    direction: 'push',
                    pushRows: [
                      { newDocumentState: { id: 'push-a', _deleted: false } },
                      { newDocumentState: { id: 'push-b', _deleted: false } },
                    ],
                    message: 'fork masterWrite decode: invalid type: map, expected a sequence',
                  },
                },
              },
            },
            lastError: null,
          };
          const snapshot = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
            includeCounts: false,
            requiredCollections: ['replication_push_fixture'],
          });
          const replicationErrors = Array.isArray(snapshot?.sync?.replicationErrors)
            ? snapshot.sync.replicationErrors
            : [];
          const match = replicationErrors.find((error) => (
            error?.name === 'CtoxReplicationIoError' &&
            error?.code === 'ctox_replication_push_contract_invalid' &&
            error?.phase === 'replication-push' &&
            error?.direction === 'push' &&
            error?.upstreamCode === 'RC_PUSH_NO_AR' &&
            error?.rowCount === 2 &&
            error?.retryable === false
          ));
          return { ok: Boolean(match && snapshot?.checks?.noReplicationIoErrors === false), error: match || null, snapshot };
        });
        if (!errorStatus?.ok) {
          throw new Error(`Business OS did not expose replication push contract error in advanced status: ${JSON.stringify(errorStatus, null, 2)}`);
        }
        console.log(`replication_push_error_collection=${errorStatus.error.collection}`);
        console.log(`replication_push_error_code=${errorStatus.error.code}`);
        console.log(`replication_push_error_name=${errorStatus.error.name}`);
        if (errorStatus.snapshot?.version) console.log(`advanced_status=${errorStatus.snapshot.version}`);
        if (errorStatus.snapshot?.rxdbRuntime) console.log(`rxdb_runtime=${JSON.stringify(errorStatus.snapshot.rxdbRuntime)}`);
        emitBrowserDiagnostics();
        return;
      }
      try {
        const shellReadyWaitStartedAt = Date.now();
        if (deferredFileCollectionStartupMode) {
          await page.waitForFunction(() => {
            const state = globalThis.ctoxBusinessOsSmoke?.state;
            const modulesLoaded = Array.isArray(state?.modules) && state.modules.length > 0;
            return modulesLoaded && Boolean(state?.db?.raw) && Boolean(state?.sync);
          }, null, { timeout: 60000 });
        } else {
          await page.waitForFunction(() => {
            const state = globalThis.ctoxBusinessOsSmoke?.state;
            const modulesLoaded = Array.isArray(state?.modules) && state.modules.length > 0;
            const shellOpened = Boolean(document.body?.dataset?.moduleShell);
            const loading = Boolean(document.body?.dataset?.moduleLoading);
            return modulesLoaded && shellOpened && !loading;
          }, null, { timeout: 60000 });
        }
        outerPhaseTimings.shellReadyWaitMs = Date.now() - shellReadyWaitStartedAt;
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
      const startupRequiredCollections = deferredFileCollectionStartupMode
        || largeFileMaterializeSmokeMode
        ? [
            'business_module_catalog',
            'ctox_runtime_settings',
          ]
        : [
            'business_module_catalog',
            'ctox_runtime_settings',
            'business_commands',
            'ctox_queue_tasks',
            'desktop_files',
            'desktop_file_chunks',
          ];
      const startupAdvancedStatusStartedAt = Date.now();
      const advancedStatus = await page.evaluate((requiredCollections) => globalThis.CTOX_BUSINESS_OS_STATUS?.waitForHealthy?.({
        timeoutMs: 60000,
        requiredCollections,
      }), startupRequiredCollections);
      outerPhaseTimings.startupAdvancedStatusMs = Date.now() - startupAdvancedStatusStartedAt;
      if (!advancedStatus?.ok) {
        throw new Error(`Business OS advanced status unhealthy after startup: ${JSON.stringify(advancedStatus, null, 2)}`);
      }
      assertHealthyAdvancedStatusContract(advancedStatus);
      advancedStatusEvidenceVersion = advancedStatus.version || '';
      advancedStatusEvidenceRuntime = advancedStatus.rxdbRuntime || null;
      if (smokeMode === 'native-schema-drift-browser-status') {
        const driftStatus = await page.evaluate(() => globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
          includeCounts: false,
          requiredCollections: [
            'business_module_catalog',
            'ctox_runtime_settings',
            'business_commands',
            'ctox_queue_tasks',
            'desktop_files',
            'desktop_file_chunks',
          ],
        }));
        assertHealthyAdvancedStatusContract(driftStatus);
        const nativePeer = driftStatus?.sync?.nativePeer || {};
        const degraded = Array.isArray(nativePeer.degradedOptionalCollections)
          ? nativePeer.degradedOptionalCollections
          : [];
        const outboundDrift = degraded.find((item) => item?.collection === 'outbound_messages');
        if (!outboundDrift || !String(outboundDrift.error || '').includes('DB6')) {
          throw new Error(`Native optional schema drift was not exposed as degraded outbound_messages collection: ${JSON.stringify(nativePeer, null, 2)}`);
        }
        if (nativePeer.requiredCollectionsRegistered !== true) {
          throw new Error(`Native peer required collections were not fully registered during optional schema drift: ${JSON.stringify(nativePeer, null, 2)}`);
        }
        if (Array.isArray(nativePeer.requiredMissingCollections) && nativePeer.requiredMissingCollections.length > 0) {
          throw new Error(`Native peer reports missing required collections during optional schema drift: ${JSON.stringify(nativePeer.requiredMissingCollections)}`);
        }
        const registered = Array.isArray(nativePeer.registeredCollections)
          ? nativePeer.registeredCollections
          : [];
        for (const required of ['business_commands', 'ctox_runtime_settings', 'desktop_files']) {
          if (!registered.includes(required)) {
            throw new Error(`Native peer did not register required collection ${required}: ${JSON.stringify(nativePeer, null, 2)}`);
          }
        }
        if (registered.includes('outbound_messages')) {
          throw new Error(`Native peer registered drifted optional outbound_messages collection instead of degrading it: ${JSON.stringify(nativePeer, null, 2)}`);
        }
        console.log(`native_schema_drift_collection=${outboundDrift.collection}`);
        console.log(`native_schema_drift_phase=${outboundDrift.phase || ''}`);
        console.log(`native_schema_drift_degraded_count=${nativePeer.degradedOptionalCount || degraded.length}`);
        console.log(`native_schema_drift_registered_count=${nativePeer.registeredCount || registered.length}`);
        console.log(`native_required_missing_count=${Array.isArray(nativePeer.requiredMissingCollections) ? nativePeer.requiredMissingCollections.length : -1}`);
        console.log(`advanced_status=${driftStatus.version}`);
        console.log(`rxdb_runtime=${JSON.stringify(driftStatus.rxdbRuntime || null)}`);
        emitBrowserDiagnostics();
        return;
      }
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
        advancedStatusEvidenceRuntime = resumedStatus.rxdbRuntime || advancedStatusEvidenceRuntime;
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
        advancedStatusEvidenceRuntime = resumedStatus.rxdbRuntime || advancedStatusEvidenceRuntime;
      }
      if (smokeMode === 'browser-responsive-ui') {
        if (!/#browser(?:$|[?&])/.test(page.url())) {
          const browserUrl = new URL(page.url());
          browserUrl.pathname = '/index.html';
          browserUrl.searchParams.set('rxdbSmoke', '1');
          if (useAppDb && smokeDbId) browserUrl.searchParams.set('smokeDbId', smokeDbId);
          browserUrl.hash = '#browser';
          await page.goto(browserUrl.toString(), { waitUntil: 'domcontentloaded' });
        }
        await page.waitForFunction(() => Boolean(document.querySelector('[data-browser-root]')), null, { timeout: 60000 });
        await page.waitForFunction(() => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          return Boolean(state?.sync?.startCollection && state?.db?.raw?.browser_frames);
        }, null, { timeout: 60000 });
        // Render a synthetic frame so the page area looks like a real browser viewport.
        await page.evaluate(async () => {
          const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          for (const collection of ['browser_sessions', 'browser_tabs', 'browser_frames', 'browser_input_events', 'business_commands']) {
            const bridge = await state.sync.startCollection(collection);
            await Promise.race([bridge?.state?.awaitInitialReplication?.().catch(() => {}) || delay(0), delay(8000)]);
          }
          const root = document.querySelector('[data-browser-root]');
          const address = root.querySelector('[data-browser-address]');
          if (address) {
            address.value = 'https://example.com';
            address.dispatchEvent(new Event('input', { bubbles: true }));
          }
          root.querySelector('[data-browser-seed]')?.click();
          await delay(1500);
          root.querySelector('[data-browser-refresh]')?.click();
          await delay(1200);
        });

        const measureChrome = async (label) => {
          const probe = await page.evaluate((forceAuth) => {
            const root = document.querySelector('[data-browser-root]');
            const rectOf = (sel) => {
              const el = root.querySelector(sel);
              if (!el) return null;
              const r = el.getBoundingClientRect();
              const visible = r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== 'hidden';
              return { x: r.x, y: r.y, w: r.width, h: r.height, top: r.top, bottom: r.bottom, visible };
            };
            // Auth-assist must remain laid out (not display:none) at any width.
            const authEl = root.querySelector('[data-browser-auth-assist]');
            let authDisplayWhenShown = 'n/a';
            if (authEl) {
              const wasHidden = authEl.hidden;
              const prevHtml = authEl.innerHTML;
              authEl.hidden = false;
              if (!prevHtml.trim()) authEl.innerHTML = '<div><strong>probe</strong></div>';
              authDisplayWhenShown = getComputedStyle(authEl).display;
              authEl.hidden = wasHidden;
              authEl.innerHTML = prevHtml;
            }
            return {
              innerWidth: window.innerWidth,
              innerHeight: window.innerHeight,
              scrollWidth: document.documentElement.scrollWidth,
              clientWidth: document.documentElement.clientWidth,
              address: rectOf('[data-browser-address]'),
              back: rectOf('[data-browser-back]'),
              forward: rectOf('[data-browser-forward]'),
              reload: rectOf('[data-browser-reload]'),
              start: rectOf('[data-browser-start]'),
              canvas: rectOf('[data-browser-canvas]'),
              statusChip: rectOf('[data-browser-status-chip]'),
              authDisplayWhenShown,
              canvasHasFrame: !root.querySelector('[data-browser-empty]') || Boolean(root.querySelector('[data-browser-empty]')?.hidden),
            };
          });
          await assertNoVisibleBrowserDebugText(page);
          const problems = [];
          const need = ['address', 'back', 'forward', 'reload', 'start', 'canvas'];
          for (const key of need) {
            if (!probe[key] || !probe[key].visible) problems.push(`${key} not visible`);
          }
          if (probe.scrollWidth > probe.clientWidth + 2) {
            problems.push(`horizontal overflow scrollWidth=${probe.scrollWidth} clientWidth=${probe.clientWidth}`);
          }
          if (probe.address && probe.canvas && probe.address.bottom > probe.canvas.top + 4) {
            problems.push(`address bar not above page area (address.bottom=${probe.address.bottom} canvas.top=${probe.canvas.top})`);
          }
          if (probe.authDisplayWhenShown === 'none') {
            problems.push('auth-assist banner is display:none when shown');
          }
          if (problems.length) {
            throw new Error(`Browser responsive layout failed at ${label}: ${problems.join('; ')} ${JSON.stringify(probe)}`);
          }
          const saved = await page.evaluate((name) => globalThis.__ctoxCaptureSmokeScreenshot?.(name), `browser-responsive-${label}`);
          console.log(`browser_responsive_${label}_ok=1 innerWidth=${probe.innerWidth} canvasHasFrame=${probe.canvasHasFrame ? 1 : 0} screenshot=${saved || '-'}`);
          return probe;
        };

        await page.setViewportSize({ width: 1280, height: 800 });
        await page.waitForTimeout(400);
        await measureChrome('desktop');

        await page.setViewportSize({ width: 414, height: 896 });
        await page.waitForTimeout(500);
        await measureChrome('mobile');

        console.log('browser_responsive_ui_ok=1');
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'browser-lifecycle-ui') {
        if (!/#browser(?:$|[?&])/.test(page.url())) {
          const browserUrl = new URL(page.url());
          browserUrl.pathname = '/index.html';
          browserUrl.searchParams.set('rxdbSmoke', '1');
          if (useAppDb && smokeDbId) browserUrl.searchParams.set('smokeDbId', smokeDbId);
          browserUrl.hash = '#browser';
          await page.goto(browserUrl.toString(), { waitUntil: 'domcontentloaded' });
          await page.waitForFunction(() => Boolean(document.querySelector('[data-browser-root]')), null, { timeout: 60000 });
          await assertNoVisibleBrowserDebugText(page);
          await page.waitForFunction(() => {
            const state = globalThis.ctoxBusinessOsSmoke?.state;
            return Boolean(state?.sync?.startCollection && state?.db?.raw?.business_commands);
          }, null, { timeout: 60000 });
        }
        const lifecycle = await page.evaluate(async () => {
          const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
          const bounded = (promise, ms) => promise
            ? Promise.race([promise.catch?.(() => undefined) || promise, delay(ms)])
            : delay(0);
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const db = state?.db?.raw;
          if (!state?.sync?.startCollection || !db?.business_commands) {
            throw new Error('Business OS command collections are not available for Browser lifecycle UI smoke');
          }
          for (const collection of [
            'business_commands',
            'browser_sessions',
            'browser_tabs',
            'browser_frames',
            'browser_input_events',
          ]) {
            const bridge = await state.sync.startCollection(collection);
            await bounded(bridge?.state?.awaitInitialReplication?.(), 20000);
            await bounded(bridge?.state?.awaitInSync?.(), 20000);
          }
          const waitForBrowserRoot = async () => {
            const deadline = Date.now() + 60000;
            while (Date.now() < deadline) {
              const root = document.querySelector('[data-browser-root]');
              if (root) return root;
              location.hash = 'browser';
              await delay(250);
            }
            throw new Error(`Browser module root did not appear: ${document.body?.innerText?.slice(0, 500) || ''}`);
          };
          const root = await waitForBrowserRoot();
          const address = root.querySelector('[data-browser-address]');
          if (!address) throw new Error('Browser address input not found');
          address.value = 'https://example.com';
          address.dispatchEvent(new Event('input', { bubbles: true }));

          const startedAt = Date.now();
          const commandTypes = [
            ['browser.session.start', '[data-browser-start]'],
            ['browser.navigate', '[data-browser-address-form]'],
            ['browser.reload', '[data-browser-reload]'],
            ['browser.back', '[data-browser-back]'],
            ['browser.forward', '[data-browser-forward]'],
            ['browser.reset', '[data-browser-reset]'],
            ['browser.session.stop', '[data-browser-stop]'],
          ];
          const clickControl = async (type, selector) => {
            const target = root.querySelector(selector);
            if (!target) throw new Error(`Browser lifecycle control missing for ${type}: ${selector}`);
            if (selector === '[data-browser-address-form]') {
              target.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
            } else {
              target.click();
            }
            await delay(450);
          };
          const waitForAcceptedCommand = async (type, timeoutMs = 45000) => {
            const deadline = Date.now() + timeoutMs;
            let last = null;
            while (Date.now() < deadline) {
              const commands = (await db.business_commands.find().exec())
                .map((doc) => doc.toJSON?.() || doc)
                .filter((command) => {
                  const commandType = command.command_type || command.type || '';
                  return commandType === type
                    && command.module === 'browser'
                    && Number(command.created_at_ms || command.updated_at_ms || 0) >= startedAt - 1000;
                })
                .sort((a, b) => Number(b.created_at_ms || b.updated_at_ms || 0) - Number(a.created_at_ms || a.updated_at_ms || 0));
              last = commands[0] || null;
              if (last && String(last.status || '') !== 'pending_sync') return last;
              await delay(250);
            }
            throw new Error(`Browser lifecycle command ${type} was not accepted: ${JSON.stringify(last, null, 2)}`);
          };
          await clickControl(...commandTypes[0]);
          {
            const deadline = Date.now() + 45000;
            let sawSession = false;
            while (Date.now() < deadline) {
              const session = (await db.browser_sessions?.findOne('browser_session_default').exec())?.toJSON?.() || null;
              if (session?.id === 'browser_session_default') {
                sawSession = true;
                break;
              }
              await delay(250);
            }
            if (!sawSession) throw new Error('Browser lifecycle UI smoke did not observe session after Start Remote');
          }
          root.querySelector('[data-browser-refresh]')?.click();
          {
            const deadline = Date.now() + 30000;
            let renderedSession = false;
            while (Date.now() < deadline) {
              const text = root.querySelector('[data-browser-session-card]')?.textContent || '';
              if (text.includes('Browser') && text.includes('https://example.com') && !text.includes('pending_command')) {
                renderedSession = true;
                break;
              }
              await delay(250);
            }
            if (!renderedSession) {
              throw new Error(`Browser lifecycle UI smoke did not render session after Start Remote: ${root.querySelector('[data-browser-session-card]')?.textContent || ''}`);
            }
          }
          await waitForAcceptedCommand(commandTypes[0][0]);
          for (const command of commandTypes.slice(1)) {
            await clickControl(...command);
            await waitForAcceptedCommand(command[0]);
          }

          const expected = commandTypes.map(([type]) => type);
          const deadline = Date.now() + 90000;
          let last = null;
          while (Date.now() < deadline) {
            const commands = (await db.business_commands.find().exec())
              .map((doc) => doc.toJSON?.() || doc)
              .filter((command) => {
                const type = command.command_type || command.type || '';
                return expected.includes(type)
                  && command.module === 'browser'
                  && Number(command.created_at_ms || command.updated_at_ms || 0) >= startedAt - 1000;
              });
            const acceptedTypes = new Set(commands
              .filter((command) => String(command.status || '') !== 'pending_sync')
              .map((command) => command.command_type || command.type || ''));
            const session = (await db.browser_sessions?.findOne('browser_session_default').exec())?.toJSON?.() || null;
            const tab = (await db.browser_tabs?.findOne('browser_tab_default').exec())?.toJSON?.() || null;
            last = {
              commandCount: commands.length,
              acceptedTypes: [...acceptedTypes],
              commands: commands.map((command) => ({
                id: command.id,
                type: command.command_type || command.type || '',
                status: command.status || '',
                result: command.result || null,
                error: command.error || '',
              })),
              session,
              tab,
            };
            if (
              expected.every((type) => acceptedTypes.has(type)) &&
              session?.status === 'stopped' &&
              session?.runtime_status === 'stopped' &&
              session?.payload?.last_command_type === 'browser.session.stop' &&
              tab?.status === 'stopped'
            ) {
              return {
                commandTypes: expected,
                commandCount: commands.length,
                acceptedTypes: [...acceptedTypes],
                sessionStatus: session.status,
                runtimeStatus: session.runtime_status,
                tabStatus: tab.status,
                lastCommandType: session.payload?.last_command_type || '',
              };
            }
            await delay(500);
          }
          throw new Error(`Browser lifecycle UI smoke did not converge: ${JSON.stringify(last, null, 2)}`);
        });
        console.log(`browser_lifecycle_command_count=${lifecycle.commandCount}`);
        console.log(`browser_lifecycle_command_types=${lifecycle.commandTypes.join(',')}`);
        console.log(`browser_lifecycle_accepted_types=${lifecycle.acceptedTypes.join(',')}`);
        console.log(`browser_lifecycle_session_status=${lifecycle.sessionStatus}`);
        console.log(`browser_lifecycle_runtime_status=${lifecycle.runtimeStatus}`);
        console.log(`browser_lifecycle_tab_status=${lifecycle.tabStatus}`);
        console.log(`browser_lifecycle_last_command=${lifecycle.lastCommandType}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'browser-input-runtime') {
        if (!/#browser(?:$|[?&])/.test(page.url())) {
          const browserUrl = new URL(page.url());
          browserUrl.pathname = '/index.html';
          browserUrl.searchParams.set('rxdbSmoke', '1');
          if (useAppDb && smokeDbId) browserUrl.searchParams.set('smokeDbId', smokeDbId);
          browserUrl.hash = '#browser';
          await page.goto(browserUrl.toString(), { waitUntil: 'domcontentloaded' });
        }
        await page.waitForFunction(() => Boolean(document.querySelector('[data-browser-root]')), null, { timeout: 60000 });
        await assertNoVisibleBrowserDebugText(page);
        await page.waitForFunction(() => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          return Boolean(state?.sync?.startCollection && state?.db?.raw?.business_commands);
        }, null, { timeout: 60000 });

        const phase1 = await page.evaluate(async () => {
          const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
          const bounded = (promise, ms) => promise
            ? Promise.race([promise.catch?.(() => undefined) || promise, delay(ms)])
            : delay(0);
          const docsToJson = (docs) => (docs || []).map((doc) => doc?.toJSON?.() || doc).filter(Boolean);
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const db = state?.db?.raw;
          if (!state?.sync?.startCollection || !db?.business_commands) {
            throw new Error('Business OS command collections are not available for Browser input runtime smoke');
          }
          for (const collection of [
            'business_commands',
            'browser_sessions',
            'browser_tabs',
            'browser_frames',
            'browser_input_events',
          ]) {
            const bridge = await state.sync.startCollection(collection);
            await bounded(bridge?.state?.awaitInitialReplication?.(), 20000);
            await bounded(bridge?.state?.awaitInSync?.(), 20000);
          }
          const waitForBrowserRoot = async () => {
            const deadline = Date.now() + 60000;
            while (Date.now() < deadline) {
              const root = document.querySelector('[data-browser-root]');
              if (root) return root;
              location.hash = 'browser';
              await delay(250);
            }
            throw new Error(`Browser module root did not appear: ${document.body?.innerText?.slice(0, 500) || ''}`);
          };
          const root = await waitForBrowserRoot();
          const address = root.querySelector('[data-browser-address]');
          if (!address) throw new Error('Browser address input not found');

          // The local Business OS server is a deterministic, offline navigation
          // target. Two distinct query strings prove the real runtime actually
          // navigates and produces a fresh frame per navigation.
          const origin = location.origin;
          const url1 = `${origin}/index.html?probe=1`;
          const url2 = `${origin}/index.html?probe=2`;
          const setAddress = (value) => {
            address.value = value;
            address.dispatchEvent(new Event('input', { bubbles: true }));
          };
          const submitAddress = () => {
            const form = root.querySelector('[data-browser-address-form]');
            if (!form) throw new Error('Browser address form not found');
            form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
          };

          setAddress(url1);
          const startedAt = Date.now();
          root.querySelector('[data-browser-start]')?.click();

          const latestRealFrame = async () => docsToJson(await db.browser_frames?.find().exec())
            .filter((item) => item.session_id === 'browser_session_default' && item.data && Number(item.expires_at_ms || 0) > Date.now())
            .sort((a, b) => Number(b.seq || 0) - Number(a.seq || 0))[0] || null;
          const b64Len = (data) => {
            const clean = String(data || '').replace(/=+$/, '');
            return Math.floor((clean.length * 3) / 4);
          };

          let firstFrame = null;
          let firstState = null;
          {
            const deadline = Date.now() + 90000;
            while (Date.now() < deadline) {
              const frame = await latestRealFrame();
              const session = (await db.browser_sessions?.findOne('browser_session_default').exec())?.toJSON?.() || null;
              const tab = (await db.browser_tabs?.findOne('browser_tab_default').exec())?.toJSON?.() || null;
              firstState = {
                frameSeq: frame?.seq ?? null,
                frameBytes: frame ? b64Len(frame.data) : 0,
                runtimeStatus: session?.runtime_status || '',
                tabUrl: tab?.url || '',
                sessionError: session?.error || '',
              };
              if (
                frame?.data &&
                Number(frame.seq || 0) >= 1 &&
                b64Len(frame.data) > 1000 &&
                session?.runtime_status === 'active' &&
                String(tab?.url || '').includes('probe=1')
              ) {
                firstFrame = frame;
                break;
              }
              await delay(500);
            }
          }
          if (!firstFrame) {
            throw new Error(`Browser input runtime smoke did not observe a real first frame: ${JSON.stringify(firstState, null, 2)}`);
          }

          // Navigate to a second URL; assert URL and frame both advance.
          setAddress(url2);
          submitAddress();
          let secondFrame = null;
          let secondState = null;
          {
            const deadline = Date.now() + 60000;
            while (Date.now() < deadline) {
              const frame = await latestRealFrame();
              const tab = (await db.browser_tabs?.findOne('browser_tab_default').exec())?.toJSON?.() || null;
              secondState = {
                frameSeq: frame?.seq ?? null,
                tabUrl: tab?.url || '',
              };
              if (
                frame?.data &&
                Number(frame.seq || 0) > Number(firstFrame.seq || 0) &&
                String(tab?.url || '').includes('probe=2')
              ) {
                secondFrame = frame;
                break;
              }
              await delay(500);
            }
          }
          if (!secondFrame) {
            throw new Error(`Browser input runtime smoke did not advance frame after navigation: ${JSON.stringify({ firstSeq: firstFrame.seq, secondState }, null, 2)}`);
          }

          const canvas = root.querySelector('[data-browser-canvas]');
          if (!canvas) throw new Error('Browser canvas not found for input runtime smoke');
          const rect = canvas.getBoundingClientRect();
          if (!(rect.width > 0 && rect.height > 0)) {
            throw new Error(`Browser canvas has no layout box: ${JSON.stringify({ width: rect.width, height: rect.height })}`);
          }
          return {
            startedAt,
            firstSeq: Number(firstFrame.seq || 0),
            firstBytes: b64Len(firstFrame.data),
            secondSeq: Number(secondFrame.seq || 0),
            canvasRect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
          };
        });

        // Drive a trusted pointer click through Playwright so the canvas input
        // handler enqueues a real browser_input_events row.
        const clickX = phase1.canvasRect.x + Math.min(80, phase1.canvasRect.width / 2);
        const clickY = phase1.canvasRect.y + Math.min(80, phase1.canvasRect.height / 2);
        await page.mouse.move(clickX, clickY);
        await page.mouse.down();
        await page.mouse.up();

        const phase2 = await page.evaluate(async (prevSeq) => {
          const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
          const docsToJson = (docs) => (docs || []).map((doc) => doc?.toJSON?.() || doc).filter(Boolean);
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const db = state?.db?.raw;
          const root = document.querySelector('[data-browser-root]');

          let consumed = null;
          let inputState = null;
          {
            const deadline = Date.now() + 60000;
            while (Date.now() < deadline) {
              const events = docsToJson(await db.browser_input_events?.find().exec())
                .filter((event) => event.session_id === 'browser_session_default');
              const frame = docsToJson(await db.browser_frames?.find().exec())
                .filter((item) => item.session_id === 'browser_session_default' && item.data && Number(item.expires_at_ms || 0) > Date.now())
                .sort((a, b) => Number(b.seq || 0) - Number(a.seq || 0))[0] || null;
              const session = (await db.browser_sessions?.findOne('browser_session_default').exec())?.toJSON?.() || null;
              const consumedEvents = events.filter((event) => event.status === 'consumed');
              const failedEvents = events.filter((event) => event.status === 'failed');
              inputState = {
                totalEvents: events.length,
                consumed: consumedEvents.length,
                failed: failedEvents.length,
                statuses: events.map((event) => `${event.type}:${event.status}`),
                frameSeq: frame?.seq ?? null,
                lastInputSeq: session?.last_input_seq ?? null,
                pendingInputCount: session?.pending_input_count ?? null,
              };
              if (failedEvents.length > 0) {
                throw new Error(`Browser input runtime smoke saw failed input events: ${JSON.stringify(inputState, null, 2)}`);
              }
              if (
                consumedEvents.length >= 1 &&
                frame?.data &&
                Number(frame.seq || 0) > Number(prevSeq || 0) &&
                Number(session?.last_input_seq || 0) > 0 &&
                Number(session?.pending_input_count || 0) === 0
              ) {
                consumed = { event: consumedEvents[0], frame, session };
                break;
              }
              await delay(500);
            }
          }
          if (!consumed) {
            throw new Error(`Browser input runtime smoke did not consume the input event: ${JSON.stringify(inputState, null, 2)}`);
          }

          // Stop the session and assert it tears down cleanly.
          root.querySelector('[data-browser-stop]')?.click();
          let stopped = null;
          {
            const deadline = Date.now() + 45000;
            while (Date.now() < deadline) {
              const session = (await db.browser_sessions?.findOne('browser_session_default').exec())?.toJSON?.() || null;
              const tab = (await db.browser_tabs?.findOne('browser_tab_default').exec())?.toJSON?.() || null;
              stopped = {
                status: session?.status || '',
                runtimeStatus: session?.runtime_status || '',
                tabStatus: tab?.status || '',
              };
              if (session?.status === 'stopped' && session?.runtime_status === 'stopped' && tab?.status === 'stopped') {
                break;
              }
              await delay(500);
            }
          }
          if (!(stopped?.status === 'stopped' && stopped?.runtimeStatus === 'stopped')) {
            throw new Error(`Browser input runtime smoke session did not stop cleanly: ${JSON.stringify(stopped, null, 2)}`);
          }
          return {
            consumedSeq: Number(consumed.event.seq || 0),
            consumedType: consumed.event.type || '',
            frameSeqAfterInput: Number(consumed.frame.seq || 0),
            lastInputSeq: Number(consumed.session.last_input_seq || 0),
            pendingInputCount: Number(consumed.session.pending_input_count || 0),
            sessionStatus: stopped.status,
            runtimeStatus: stopped.runtimeStatus,
            tabStatus: stopped.tabStatus,
          };
        }, phase1.secondSeq);

        console.log(`browser_input_first_frame_seq=${phase1.firstSeq}`);
        console.log(`browser_input_first_frame_bytes=${phase1.firstBytes}`);
        console.log(`browser_input_second_frame_seq=${phase1.secondSeq}`);
        console.log(`browser_input_consumed_type=${phase2.consumedType}`);
        console.log(`browser_input_consumed_seq=${phase2.consumedSeq}`);
        console.log(`browser_input_frame_seq_after_input=${phase2.frameSeqAfterInput}`);
        console.log(`browser_input_last_input_seq=${phase2.lastInputSeq}`);
        console.log(`browser_input_pending_count=${phase2.pendingInputCount}`);
        console.log(`browser_input_session_status=${phase2.sessionStatus}`);
        console.log(`browser_input_runtime_status=${phase2.runtimeStatus}`);
        console.log(`browser_input_tab_status=${phase2.tabStatus}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'browser-handoff-ui') {
        if (!/#browser(?:$|[?&])/.test(page.url())) {
          const browserUrl = new URL(page.url());
          browserUrl.pathname = '/index.html';
          browserUrl.searchParams.set('rxdbSmoke', '1');
          if (useAppDb && smokeDbId) browserUrl.searchParams.set('smokeDbId', smokeDbId);
          browserUrl.hash = '#browser';
          await page.goto(browserUrl.toString(), { waitUntil: 'domcontentloaded' });
        }
        await page.waitForFunction(() => Boolean(document.querySelector('[data-browser-root]')), null, { timeout: 60000 }).catch(async (error) => {
          const diagnostics = await page.evaluate(() => {
            const state = globalThis.ctoxBusinessOsSmoke?.state;
            return {
              url: location.href,
              hash: location.hash,
              title: document.title,
              bodyText: (document.body?.textContent || '').replace(/\s+/g, ' ').slice(0, 500),
              activeModule: state?.activeModule?.id || null,
              modules: (state?.modules || []).map((mod) => ({
                id: mod.id,
                launch_kind: mod.launch_kind || null,
                shell: mod.layout?.shell || null,
              })),
              windows: Array.from(document.querySelectorAll('.shell-window')).map((win) => ({
                title: win.querySelector('[data-window-title]')?.textContent?.trim() || '',
                owner: win.getAttribute('data-owner-id') || win.dataset.ownerId || '',
                text: (win.textContent || '').replace(/\s+/g, ' ').slice(0, 180),
              })),
              hasCtoxRoot: Boolean(document.querySelector('[data-ctox-root]')),
              hasBrowserRoot: Boolean(document.querySelector('[data-browser-root]')),
            };
          }).catch((diagError) => ({ diagnosticError: diagError?.message || String(diagError) }));
          throw new Error(`Browser handoff UI did not render Browser root: ${error.message}; diagnostics=${JSON.stringify(diagnostics)}`);
        });
        await assertNoVisibleBrowserDebugText(page);
        await page.waitForFunction(() => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          return Boolean(state?.sync?.startCollection && state?.commandBus?.dispatch && state?.db?.raw?.business_commands);
        }, null, { timeout: 60000 });
        const handoff = await page.evaluate(async () => {
          const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
          const bounded = (promise, ms) => promise
            ? Promise.race([promise.catch?.(() => undefined) || promise, delay(ms)])
            : delay(0);
          const docsToJson = (docs) => (docs || []).map((doc) => doc?.toJSON?.() || doc).filter(Boolean);
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const db = state?.db?.raw;
          if (!state?.sync?.startCollection || !state?.commandBus?.dispatch || !db?.business_commands) {
            throw new Error('Business OS command runtime is not available for Browser handoff UI smoke');
          }
          for (const collection of [
            'business_commands',
            'ctox_queue_tasks',
            'browser_sessions',
            'browser_tabs',
            'browser_frames',
            'browser_input_events',
          ]) {
            const bridge = await state.sync.startCollection(collection);
            await bounded(bridge?.state?.awaitInitialReplication?.(), 20000);
            await bounded(bridge?.state?.awaitInSync?.(), 20000);
          }
          const waitForBrowserRoot = async () => {
            const deadline = Date.now() + 60000;
            while (Date.now() < deadline) {
              const root = document.querySelector('[data-browser-root]');
              if (root) return root;
              location.hash = 'browser';
              await delay(250);
            }
            throw new Error(`Browser module root did not appear: ${document.body?.innerText?.slice(0, 500) || ''}`);
          };
          const root = await waitForBrowserRoot();
          const address = root.querySelector('[data-browser-address]');
          if (!address) throw new Error('Browser address input not found');
          address.value = 'https://example.com';
          address.dispatchEvent(new Event('input', { bubbles: true }));

          const startedAt = Date.now();
          root.querySelector('[data-browser-seed]')?.click();
          let frame = null;
          let lastSeedState = null;
          {
            const deadline = Date.now() + 30000;
            while (Date.now() < deadline) {
              const frames = docsToJson(await db.browser_frames?.find().exec())
                .filter((item) => item.session_id === 'browser_session_synthetic' && item.data && Number(item.expires_at_ms || 0) > Date.now())
                .sort((a, b) => Number(b.seq || 0) - Number(a.seq || 0));
              const session = (await db.browser_sessions?.findOne('browser_session_synthetic').exec())?.toJSON?.() || null;
              lastSeedState = {
                frameCount: frames.length,
                sessionStatus: session?.status || '',
                runtimeStatus: session?.runtime_status || '',
                sessionError: session?.error || '',
              };
              if (frames[0]?.id) {
                frame = frames[0];
                break;
              }
              await delay(500);
            }
          }
          if (!frame?.id) {
            throw new Error(`Browser handoff UI smoke did not observe a synthetic RxDB frame: ${JSON.stringify(lastSeedState, null, 2)}`);
          }

          root.querySelector('[data-browser-refresh]')?.click();
          await delay(300);
          const send = root.querySelector('[data-browser-send-to-ctox]');
          if (!send) throw new Error('Browser Send to CTOX control not found');
          if (send.disabled) throw new Error('Browser Send to CTOX control is disabled despite an active session');
          send.click();

          const exerciseWebStackCapture = async () => {
            const webStartedAt = Date.now();
            const webSessionId = `browser_session_web_stack_auth_smoke_${webStartedAt}`;
            const webTabId = `browser_tab_web_stack_auth_smoke_${webStartedAt}`;
            const webFrameId = `browser_frame_web_stack_auth_smoke_${webStartedAt}`;
            const webFrameData = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=';
            const webSeq = webStartedAt + 1000000;
            const sourceId = 'linkedin.com';
            const captureScript = 'linkedin.profile_capture.v1';
            const verifySelector = 'a[href*="/in/"]';
            await db.browser_sessions.upsert({
              id: webSessionId,
              owner_user_id: state.user?.id || 'browser-smoke',
              controller_user_id: state.user?.id || 'browser-smoke',
              status: 'active',
              runtime_status: 'active',
              auth_status: 'authenticated',
              current_tab_id: webTabId,
              current_url: 'https://www.linkedin.com/in/smoke',
              title: 'LinkedIn Smoke Profile',
              viewport_w: 1280,
              viewport_h: 720,
              device_scale_factor: 1,
              frame_rate_target: 0,
              active_frame_id: webFrameId,
              last_frame_seq: webSeq,
              last_input_seq: 0,
              pending_input_count: 0,
              payload: {
                purpose: 'web_stack_auth',
                source_id: sourceId,
                secret_name: 'LINKEDIN_SALES_NAV_TOKEN',
                target_url: 'https://www.linkedin.com/login',
                allowed_domains: ['linkedin.com', 'www.linkedin.com', 'api.linkedin.com'],
                verify_selector: verifySelector,
                credential_selector: 'input[name="session_password"]',
                capture_script: captureScript,
                auth_assist_status: 'completed',
                authenticated: true,
                browser_stream: 'rxdb',
                secret_value_in_rxdb: false,
              },
              created_at_ms: webStartedAt,
              updated_at_ms: webStartedAt,
            });
            await db.browser_tabs.upsert({
              id: webTabId,
              session_id: webSessionId,
              title: 'LinkedIn Smoke Profile',
              url: 'https://www.linkedin.com/in/smoke',
              status: 'active',
              loading: false,
              active: true,
              can_go_back: false,
              can_go_forward: false,
              frame_seq: webSeq,
              last_frame_id: webFrameId,
              last_frame_at_ms: webStartedAt,
              payload: { source: 'browser-handoff-ui-smoke' },
              created_at_ms: webStartedAt,
              updated_at_ms: webStartedAt,
            });
            await db.browser_frames.upsert({
              id: webFrameId,
              session_id: webSessionId,
              tab_id: webTabId,
              seq: webSeq,
              mime_type: 'image/png',
              encoding: 'base64',
              data: webFrameData,
              width: 1280,
              height: 720,
              viewport_w: 1280,
              viewport_h: 720,
              quality: 100,
              size_bytes: webFrameData.length,
              frame_hash: 'browser-web-stack-smoke-frame',
              captured_at_ms: webStartedAt,
              expires_at_ms: webStartedAt + 300000,
              updated_at_ms: webStartedAt,
            });
            root.querySelector('[data-browser-refresh]')?.click();
            {
              const sessionDeadline = Date.now() + 30000;
              let sessionButton = null;
              while (Date.now() < sessionDeadline) {
                sessionButton = root.querySelector(`[data-browser-session-id="${webSessionId}"]`);
                if (sessionButton) break;
                root.querySelector('[data-browser-refresh]')?.click();
                await delay(250);
              }
              if (!sessionButton) {
                throw new Error(`Browser Web Stack session did not render in session list: ${root.querySelector('[data-browser-session-list]')?.textContent || ''}`);
              }
              sessionButton.click();
            }
            let captureButton = null;
            {
              const renderDeadline = Date.now() + 30000;
              while (Date.now() < renderDeadline) {
                captureButton = root.querySelector('[data-browser-web-stack-capture]');
                const authText = root.querySelector('[data-browser-auth-assist]')?.textContent || '';
                if (captureButton && !captureButton.disabled && authText.includes(captureScript)) break;
                await delay(250);
              }
            }
            if (!captureButton || captureButton.disabled) {
              throw new Error(`Browser Web Stack capture control did not become available: ${root.querySelector('[data-browser-auth-assist]')?.textContent || ''}`);
            }
            captureButton.click();
            const exerciseWebStackExtract = async () => {
              let extractButton = null;
              const renderDeadline = Date.now() + 30000;
              while (Date.now() < renderDeadline) {
                extractButton = root.querySelector('[data-browser-web-stack-extract]');
                if (extractButton && !extractButton.disabled) break;
                await delay(250);
              }
              if (!extractButton || extractButton.disabled) {
                throw new Error(`Browser Web Stack extract control did not become available: ${root.querySelector('[data-browser-auth-assist]')?.textContent || ''}`);
              }
              extractButton.click();
              const extractDeadline = Date.now() + 90000;
              let extractLast = null;
              while (Date.now() < extractDeadline) {
                const commands = docsToJson(await db.business_commands.find().exec())
                  .filter((command) => {
                    const type = command.command_type || command.type || '';
                    return type === 'browser.capture.extract'
                      && command.record_id === webSessionId
                      && Number(command.created_at_ms || command.updated_at_ms || 0) >= webStartedAt - 1000;
                  })
                  .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0));
                const command = commands[0] || null;
                const payloadJson = command ? JSON.stringify(command.payload || {}) : '';
                extractLast = { command, payloadJson };
                if (
                  command &&
                  ['pending_sync', 'accepted'].includes(command.status) &&
                  command.payload?.source_id === sourceId &&
                  command.payload?.capture_script === captureScript &&
                  command.payload?.frame_id === webFrameId &&
                  command.payload?.secret_value_in_payload === false &&
                  command.payload?.frame_data_in_payload === false &&
                  command.payload?.browser_context_artifact?.kind === 'browser_context' &&
                  command.payload?.browser_context_artifact?.source_id === sourceId &&
                  command.payload?.browser_context_artifact?.capture_script === captureScript &&
                  command.payload?.browser_context_artifact?.frame_data_in_payload === false &&
                  command.payload?.browser_context_artifact?.browser_context?.frame_id === webFrameId &&
                  command.payload?.browser_context_artifact?.browser_context?.frame_data_in_payload === false &&
                  !payloadJson.includes(webFrameData) &&
                  !payloadJson.includes('smoke-secret')
                ) {
                  return {
                    commandId: command.command_id || command.id,
                    status: command.status,
                    frameDataInPayload: command.payload.frame_data_in_payload,
                    artifactKind: command.payload.browser_context_artifact.kind,
                  };
                }
                await delay(500);
              }
              throw new Error(`Browser Web Stack extract UI smoke did not converge: ${JSON.stringify(extractLast, null, 2)}`);
            };
            const webDeadline = Date.now() + 90000;
            let webLast = null;
            while (Date.now() < webDeadline) {
              const commands = docsToJson(await db.business_commands.find().exec())
                .filter((command) => {
                  const type = command.command_type || command.type || '';
                  return type === 'ctox.browser_context.capture'
                    && command.module === 'ctox'
                    && command.record_id === webSessionId
                    && Number(command.created_at_ms || command.updated_at_ms || 0) >= webStartedAt - 1000;
                })
                .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0));
              const command = commands[0] || null;
              const payloadJson = command ? JSON.stringify(command.payload || {}) : '';
              const taskId = command?.task_id || '';
              const task = taskId
                ? ((await db.ctox_queue_tasks.findOne(taskId).exec())?.toJSON?.() || null)
                : null;
              webLast = { command, task, payloadJson };
              if (
                command?.status === 'accepted' &&
                command?.payload?.source_module === 'web_stack' &&
                command?.payload?.source_id === sourceId &&
                command?.payload?.capture_script === captureScript &&
                command?.payload?.secret_value_in_payload === false &&
                command?.payload?.browser_context?.session_id === webSessionId &&
                command?.payload?.browser_context?.frame_id === webFrameId &&
                command?.payload?.browser_context?.source_id === sourceId &&
                command?.payload?.browser_context?.capture_script === captureScript &&
                command?.payload?.browser_context?.verify_selector === verifySelector &&
                command?.payload?.browser_context?.frame_data_in_payload === false &&
                !payloadJson.includes(webFrameData) &&
                !payloadJson.includes('smoke-secret') &&
                task?.command_id === command.command_id &&
                task?.command_type === 'ctox.browser_context.capture' &&
                task?.inbound_channel === 'browser' &&
                task?.browser_context_artifact?.kind === 'browser_context' &&
                task?.browser_context_artifact?.source_id === sourceId &&
                task?.browser_context_artifact?.capture_script === captureScript &&
                task?.browser_context_artifact?.frame_data_in_payload === false &&
                task?.browser_context_artifact?.browser_context?.frame_id === webFrameId &&
                !JSON.stringify(task.browser_context_artifact).includes(webFrameData) &&
                !JSON.stringify(task.browser_context_artifact).includes('smoke-secret')
              ) {
                const webStackExtract = await exerciseWebStackExtract();
                return {
                  commandId: command.command_id || command.id,
                  sourceId,
                  captureScript,
                  frameDataInPayload: command.payload.browser_context.frame_data_in_payload,
                  artifactKind: task.browser_context_artifact.kind,
                  taskId: task.id,
                  extractCommandId: webStackExtract.commandId,
                  extractStatus: webStackExtract.status,
                  extractFrameDataInPayload: webStackExtract.frameDataInPayload,
                  extractArtifactKind: webStackExtract.artifactKind,
                };
              }
              await delay(500);
            }
            throw new Error(`Browser Web Stack capture UI smoke did not converge: ${JSON.stringify(webLast, null, 2)}`);
          };

          const deadline = Date.now() + 90000;
          let last = null;
          while (Date.now() < deadline) {
            const commands = docsToJson(await db.business_commands.find().exec())
              .filter((command) => {
                const type = command.command_type || command.type || '';
                return type === 'ctox.browser_context.capture'
                  && command.module === 'ctox'
                  && command.record_id === frame.session_id
                  && Number(command.created_at_ms || command.updated_at_ms || 0) >= startedAt - 1000;
              })
              .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0));
            const command = commands[0] || null;
            const taskId = command?.task_id || '';
            const task = taskId
              ? ((await db.ctox_queue_tasks.findOne(taskId).exec())?.toJSON?.() || null)
              : null;
            const capturedFrameId = command?.payload?.browser_context?.frame_id || '';
            const capturedFrame = capturedFrameId
              ? ((await db.browser_frames.findOne(capturedFrameId).exec())?.toJSON?.() || null)
              : null;
            const handoffText = root.querySelector('[data-browser-handoff-history]')?.textContent || '';
            last = {
              command,
              task,
              capturedFrame,
              handoffText,
              frameId: frame.id,
              frameSeq: frame.seq,
            };
            if (
              command?.status === 'accepted' &&
              command?.payload?.browser_context?.session_id === frame.session_id &&
              capturedFrame?.id === capturedFrameId &&
              capturedFrame?.session_id === frame.session_id &&
              capturedFrame?.data &&
              task?.command_id === command.command_id &&
              task?.command_type === 'ctox.browser_context.capture' &&
              task?.inbound_channel === 'browser' &&
              task?.browser_context_artifact?.kind === 'browser_context' &&
              task?.browser_context_artifact?.browser_context?.frame_id === capturedFrameId &&
              task?.browser_context_artifact?.browser_context?.frame_data_in_payload === false &&
              handoffText.includes(task.id)
            ) {
              const webStackCapture = await exerciseWebStackCapture();
              root.querySelector('[data-browser-stop]')?.click();
              return {
                commandId: command.command_id || command.id,
                commandStatus: command.status,
                commandType: command.command_type || command.type || '',
                taskId: task.id,
                taskStatus: task.status || task.route_status || '',
                taskInboundChannel: task.inbound_channel || '',
                frameId: capturedFrame.id,
                frameSeq: capturedFrame.seq,
                handoffVisible: 1,
                webStackCommandId: webStackCapture.commandId,
                webStackSourceId: webStackCapture.sourceId,
                webStackCaptureScript: webStackCapture.captureScript,
                webStackFrameDataInPayload: webStackCapture.frameDataInPayload,
                webStackArtifactKind: webStackCapture.artifactKind,
                webStackTaskId: webStackCapture.taskId,
                webStackExtractCommandId: webStackCapture.extractCommandId,
                webStackExtractStatus: webStackCapture.extractStatus,
                webStackExtractFrameDataInPayload: webStackCapture.extractFrameDataInPayload,
                webStackExtractArtifactKind: webStackCapture.extractArtifactKind,
              };
            }
            await delay(500);
          }
          throw new Error(`Browser handoff UI smoke did not converge: ${JSON.stringify(last, null, 2)}`);
        });
        console.log(`browser_handoff_command_id=${handoff.commandId}`);
        console.log(`browser_handoff_command_status=${handoff.commandStatus}`);
        console.log(`browser_handoff_command_type=${handoff.commandType}`);
        console.log(`browser_handoff_task_id=${handoff.taskId}`);
        console.log(`browser_handoff_task_status=${handoff.taskStatus}`);
        console.log(`browser_handoff_task_inbound_channel=${handoff.taskInboundChannel}`);
        console.log(`browser_handoff_frame_id=${handoff.frameId}`);
        console.log(`browser_handoff_frame_seq=${handoff.frameSeq}`);
        console.log(`browser_handoff_visible=${handoff.handoffVisible}`);
        console.log(`browser_handoff_web_stack_command_id=${handoff.webStackCommandId}`);
        console.log(`browser_handoff_web_stack_source_id=${handoff.webStackSourceId}`);
        console.log(`browser_handoff_web_stack_capture_script=${handoff.webStackCaptureScript}`);
        console.log(`browser_handoff_web_stack_frame_data_in_payload=${handoff.webStackFrameDataInPayload}`);
        console.log(`browser_handoff_web_stack_artifact_kind=${handoff.webStackArtifactKind}`);
        console.log(`browser_handoff_web_stack_task_id=${handoff.webStackTaskId}`);
        console.log(`browser_handoff_web_stack_extract_command_id=${handoff.webStackExtractCommandId}`);
        console.log(`browser_handoff_web_stack_extract_status=${handoff.webStackExtractStatus}`);
        console.log(`browser_handoff_web_stack_extract_frame_data_in_payload=${handoff.webStackExtractFrameDataInPayload}`);
        console.log(`browser_handoff_web_stack_extract_artifact_kind=${handoff.webStackExtractArtifactKind}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'command-reload-browser-to-rust') {
        const dispatched = await page.evaluate(async () => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          if (!state?.commandBus?.dispatch) throw new Error('Business OS command bus is not available for reload smoke');
          if (!state?.sync?.startCollection) throw new Error('Business OS sync runtime is not available for reload smoke');
          await state.sync.startCollection('business_commands');
          await state.sync.startCollection('ctox_queue_tasks');
          const id = `command_reload_smoke_${Date.now()}`;
          await state.commandBus.dispatch({
            id,
            module: 'ctox',
            type: 'business_os.smoke',
            record_id: '',
            inbound_channel: 'ctox',
            payload: { title: 'WebRTC command reload smoke', instruction: 'smoke test only' },
            client_context: { source: 'rxdb-smoke', reload: true },
          });
          return { id };
        });
        await page.reload({ waitUntil: 'domcontentloaded' });
        await page.waitForFunction(() => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const modulesLoaded = Array.isArray(state?.modules) && state.modules.length > 0;
          const shellOpened = Boolean(document.body?.dataset?.moduleShell);
          const loading = Boolean(document.body?.dataset?.moduleLoading);
          return modulesLoaded && shellOpened && !loading;
        }, null, { timeout: 60000 });
        const reloadedStatus = await page.evaluate(() => globalThis.CTOX_BUSINESS_OS_STATUS?.waitForHealthy?.({
          timeoutMs: 60000,
          requiredCollections: [
            'business_module_catalog',
            'ctox_runtime_settings',
            'business_commands',
            'ctox_queue_tasks',
          ],
        }));
        if (!reloadedStatus?.ok) {
          throw new Error(`Business OS advanced status unhealthy after command reload: ${JSON.stringify(reloadedStatus, null, 2)}`);
        }
        assertHealthyAdvancedStatusContract(reloadedStatus);
        const result = await page.evaluate(async ({ id }) => {
          const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
          const bounded = (promise, ms) => promise
            ? Promise.race([promise.catch?.(() => undefined) || promise, delay(ms)])
            : delay(0);
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const db = state?.db?.raw;
          if (!db?.business_commands || !db?.ctox_queue_tasks) {
            throw new Error('Business OS command collections are not available after reload');
          }
          const commandBridge = await state.sync.startCollection('business_commands');
          const queueBridge = await state.sync.startCollection('ctox_queue_tasks');
          await bounded(commandBridge?.state?.awaitInitialReplication?.(), 20000);
          await bounded(queueBridge?.state?.awaitInitialReplication?.(), 20000);
          await bounded(commandBridge?.state?.awaitInSync?.(), 30000);
          await bounded(queueBridge?.state?.awaitInSync?.(), 30000);
          const deadline = Date.now() + 60000;
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
                  throw new Error(`command ${id} produced ${queueTasksForCommand.length} queue tasks after reload: ${JSON.stringify(queueTasksForCommand)}`);
                }
                return {
                  id,
                  status: command.status,
                  taskId,
                  taskStatus: command.task_status || task.status || '',
                  taskCountForCommand: queueTasksForCommand.length,
                  reloaded: true,
                };
              }
            }
            await delay(500);
          }
          const commandDoc = await db.business_commands.findOne(id).exec();
          const queueDocs = await db.ctox_queue_tasks.find({ limit: 10 }).exec();
          throw new Error(`command ${id} was not accepted after Browser reload: ${JSON.stringify({
            command: commandDoc?.toJSON?.() || null,
            queueCount: queueDocs.length,
          })}`);
        }, dispatched);
        const commandTable = 'ctox_business_os__business_commands__v1';
        const taskTable = 'ctox_business_os__ctox_queue_tasks__v0';
        const commandRow = pollSqliteJson(commandTable, result.id);
        const taskRow = pollSqliteJson(taskTable, result.taskId);
        if (commandRow.command_id !== result.id || taskRow.command_id !== result.id) {
          throw new Error(`reload command/task rows mismatch: ${JSON.stringify({ commandRow, taskRow })}`);
        }
        console.log(`command_id=${result.id}`);
        console.log(`task_id=${result.taskId}`);
        console.log(`task_count_for_command=${result.taskCountForCommand}`);
        console.log(`status=${result.status}`);
        console.log(`task_status=${result.taskStatus}`);
        console.log(`reload_verified=${result.reloaded ? 1 : 0}`);
        emitBrowserDiagnostics();
        return;
      }
    }

    const browserPayload = hasOwn(process.env, 'SMOKE_BROWSER_FILE_CONTENT')
      ? process.env.SMOKE_BROWSER_FILE_CONTENT
      : 'hello';
    const pageEvaluateStartedAt = Date.now();
    const result = await page.evaluate(async ({ signalingUrl, smokeMode, rustSeed, useAppDb, browserPayload, backgroundQueueTask, advancedStatusEvidenceVersion, advancedStatusEvidenceRuntime }) => {
      if (!globalThis.process) globalThis.process = {};
      if (typeof globalThis.process.nextTick !== 'function') {
        globalThis.process.nextTick = (callback, ...args) => Promise.resolve().then(() => callback(...args));
      }
      const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const bounded = (promise, ms) => promise
        ? Promise.race([promise.catch?.(() => undefined) || promise, delay(ms)])
        : delay(0);
      const describeReplicationPool = (state) => {
        const peerStates = state?.peerStates$?.getValue?.();
        const entries = peerStates && typeof peerStates.values === 'function'
          ? Array.from(peerStates.values())
          : [];
        return {
          peerCount: entries.length,
          forkPeerCount: entries.filter((entry) => entry?.replicationState).length,
          masterPeerCount: entries.filter((entry) => !entry?.replicationState).length,
        };
      };
      const waitForNativePeerOpen = async (state, label, timeoutMs = 60000) => {
        const deadline = Date.now() + timeoutMs;
        let lastSnapshot = null;
        while (Date.now() < deadline) {
          await bounded(state?.awaitInitialReplication?.(), 5000);
          await bounded(state?.awaitInSync?.(), 5000);
          const peerStates = state?.peerStates$?.getValue?.();
          const entries = peerStates && typeof peerStates.entries === 'function'
            ? Array.from(peerStates.entries())
            : [];
          const nativeEntry = entries.find(([, entry]) => entry?.remoteProtocol?.peerSession?.role === 'ctox_instance');
          const nativePeerId = nativeEntry?.[0] || '';
          const connection = nativePeerId ? state?.peer?.connections?.get?.(nativePeerId) : null;
          const channelState = connection?.channel?.readyState || '';
          const pcState = connection?.peer?.connectionState || '';
          lastSnapshot = {
            label,
            peerCount: entries.length,
            nativePeerId,
            channelState,
            pcState,
            peers: entries.map(([peerId, entry]) => ({
              peerId,
              role: entry?.remoteProtocol?.peerSession?.role || '',
              sessionId: entry?.remoteProtocol?.peerSession?.sessionId || '',
            })),
          };
          if (nativePeerId && channelState === 'open' && !['closed', 'failed', 'disconnected'].includes(pcState)) {
            return lastSnapshot;
          }
          await delay(500);
        }
        throw new Error(`Timed out waiting for open native peer on ${label}: ${JSON.stringify(lastSnapshot)}`);
      };
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
          || haystack.includes('ctox_data_channel_error')
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
      let appTicketItemReplicationState = null;
      let appTicketEventReplicationState = null;
      let appTicketClarificationReplicationState = null;
      let ownsDb = false;
      let advancedStatusVersion = advancedStatusEvidenceVersion || '';
      let advancedStatusRuntime = advancedStatusEvidenceRuntime || null;
      const replicationStates = [];
      const ticketClarificationSmokeMode = smokeMode === 'tickets-clarification-browser-to-rust';
      const ticketSmokeMode = smokeMode === 'tickets-browser-to-rust' || ticketClarificationSmokeMode;
      const outboundActiveUiSmokeMode = smokeMode === 'outbound-active-ui';
      const commandSmokeMode = smokeMode === 'command-browser-to-rust'
        || smokeMode === 'migration-version-browser-to-rust'
        || smokeMode === 'command-burst-browser-to-rust'
        || smokeMode === 'command-reload-browser-to-rust'
        || smokeMode === 'command-restart-browser-to-rust'
        || smokeMode === 'command-midflight-restart-browser-to-rust';
      const materializeSmokeMode = smokeMode === 'workspace-large-materialize-rust-to-browser'
        || smokeMode === 'workspace-large-file-viewer-rust-to-browser'
        || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser';
      const backgroundIndexerSmokeMode = smokeMode === 'workspace-agent-artifacts-background-rust-to-browser';
      const deferInitialFileCollections = smokeMode === 'file-chunk-tombstone-error-browser-status';
      const needsCommandCollections = commandSmokeMode || materializeSmokeMode || ticketSmokeMode || outboundActiveUiSmokeMode;
      const needsTicketCollections = ticketSmokeMode;
      const needsFileCollections = (!commandSmokeMode && !outboundActiveUiSmokeMode || materializeSmokeMode) && !deferInitialFileCollections;
      const setupPhaseTimings = {};

      let appState = null;
      if (useAppDb) {
        const appDbReadyStartedAt = Date.now();
        const deadline = Date.now() + 30000;
        while (Date.now() < deadline) {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const raw = state?.db?.raw;
          const hasCollections = backgroundIndexerSmokeMode
            ? Boolean(raw)
            : ((!needsFileCollections || (raw?.desktop_files && raw?.desktop_file_chunks))
              && (!needsCommandCollections || (raw?.business_commands && raw?.ctox_queue_tasks)));
          if (hasCollections && state?.sync) {
            appState = state;
            break;
          }
          await new Promise((resolve) => setTimeout(resolve, 250));
        }
        setupPhaseTimings.appDbReadyMs = Date.now() - appDbReadyStartedAt;
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
        if (needsTicketCollections) {
          const requiredTicketCollections = ['ctox_ticket_items', 'ctox_ticket_events'];
          if (ticketClarificationSmokeMode) requiredTicketCollections.push('ctox_ticket_clarification_requests');
          const missingTicketCollections = requiredTicketCollections.filter((name) => !appState.db.raw?.[name]);
          if (missingTicketCollections.length) {
            const ticketSchemaMod = await import('/modules/tickets/schema.js');
            const ticketCollections = {};
            for (const name of missingTicketCollections) {
              ticketCollections[name] = { schema: ticketSchemaMod.collections[name] };
            }
            await appState.db.raw.addCollections(ticketCollections);
          }
        }
        if (needsCommandCollections) {
          const commandCollectionsStartedAt = Date.now();
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
          await waitForNativePeerOpen(appCommandReplicationState, 'business_commands');
          await waitForNativePeerOpen(appQueueReplicationState, 'ctox_queue_tasks');
          setupPhaseTimings.commandCollectionsReadyMs = Date.now() - commandCollectionsStartedAt;
        }
        if (needsFileCollections) {
          const fileCollectionsStartedAt = Date.now();
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
          await waitForNativePeerOpen(appFileReplicationState, 'desktop_files');
          await waitForNativePeerOpen(appChunkReplicationState, 'desktop_file_chunks');
          setupPhaseTimings.fileCollectionsReadyMs = Date.now() - fileCollectionsStartedAt;
        }
        if (needsTicketCollections) {
          const ticketCollectionsStartedAt = Date.now();
          const ticketItemBridge = await appState.sync.startCollection('ctox_ticket_items');
          const ticketEventBridge = await appState.sync.startCollection('ctox_ticket_events');
          const ticketClarificationBridge = ticketClarificationSmokeMode
            ? await appState.sync.startCollection('ctox_ticket_clarification_requests')
            : null;
          ticketItemBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app ctox_ticket_items replication error', error));
          ticketEventBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app ctox_ticket_events replication error', error));
          ticketClarificationBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app ctox_ticket_clarification_requests replication error', error));
          appTicketItemReplicationState = ticketItemBridge?.state || null;
          appTicketEventReplicationState = ticketEventBridge?.state || null;
          appTicketClarificationReplicationState = ticketClarificationBridge?.state || null;
          await bounded(appTicketItemReplicationState?.awaitInitialReplication?.(), 15000);
          await bounded(appTicketEventReplicationState?.awaitInitialReplication?.(), 15000);
          await bounded(appTicketClarificationReplicationState?.awaitInitialReplication?.(), 15000);
          await bounded(appTicketItemReplicationState?.awaitInSync?.(), 15000);
          await bounded(appTicketEventReplicationState?.awaitInSync?.(), 15000);
          await bounded(appTicketClarificationReplicationState?.awaitInSync?.(), 15000);
          await waitForNativePeerOpen(appTicketItemReplicationState, 'ctox_ticket_items');
          await waitForNativePeerOpen(appTicketEventReplicationState, 'ctox_ticket_events');
          if (ticketClarificationSmokeMode) {
            await waitForNativePeerOpen(appTicketClarificationReplicationState, 'ctox_ticket_clarification_requests');
          }
          setupPhaseTimings.ticketCollectionsReadyMs = Date.now() - ticketCollectionsStartedAt;
        }
        db = appState.db.raw;
      } else {
        const config = globalThis.CTOX_BUSINESS_OS_CONFIG
          || await fetch('/index.html?rxdbSmoke=1')
            .then((res) => res.text())
            .then((html) => {
              const marker = 'window.CTOX_BUSINESS_OS_CONFIG=';
              const start = html.indexOf(marker);
              if (start === -1) throw new Error('launch sync config missing from index.html');
              const bodyStart = start + marker.length;
              const end = html.indexOf(';</script>', bodyStart);
              if (end === -1) throw new Error('launch sync config script is malformed');
              return JSON.parse(html.slice(bodyStart, end));
            });
        const rxdb = await import('/rxdb/dist/ctox-rxdb-js.mjs');
        registerRxdbPlugin(rxdb, rxdb.RxDBMigrationSchemaPlugin || rxdb.RxDBMigrationPlugin);
        const desktopSchemaMod = await import('/modules/desktop/schema.js');
        const ctoxSchemaMod = await import('/modules/ctox/schema.js');
        db = await rxdb.createRxDatabase({
          name: `ctox_smoke_${Date.now()}`,
          storage: rxdb.getCtoxIndexedDbStorage(),
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
          const batchSize = collectionName === 'desktop_file_chunks' ? 2 : 10;
          const replicationState = await rxdb.replicateWebRTC({
            collection: db[collectionName],
            topic: `${config.sync_room}:${collectionName}`,
            connectionHandlerCreator: rxdb.getConnectionHandlerSimplePeer({ signalingServerUrl: signalingUrl }),
            pull: { batchSize },
            push: { batchSize },
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
            const payloadMatches = expectedPayload === null || payload === expectedPayload;
            const metadataReady = expectedPayload === null || file.content_state === 'available';
            if (payloadMatches && metadataReady) return lastSeen;
          }
          await new Promise((resolve) => setTimeout(resolve, 500));
        }
        const fileDoc = await db.desktop_files.findOne(id).exec();
        const allFileDocs = await db.desktop_files.find().exec();
        const allChunkDocs = await db.desktop_file_chunks.find().exec();
        const fileDocs = allFileDocs.map((doc) => doc.toJSON?.() || doc);
        const chunkDocs = allChunkDocs
          .map((doc) => doc.toJSON?.() || doc)
          .filter((doc) => doc.file_id === id);
        throw new Error(`browser did not receive rust-side file ${id}: ${JSON.stringify({
          hasFile: Boolean(fileDoc),
          fileCount: fileDocs.length,
          fileIds: fileDocs.map((doc) => doc.id).slice(0, 8),
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

      async function waitForWorkspaceArtifacts(expectedFiles, ms = 60000) {
        const expected = Array.isArray(expectedFiles) ? expectedFiles : [];
        const deadline = Date.now() + ms;
        let lastSeen = null;
        while (Date.now() < deadline) {
          const allFileDocs = (await db.desktop_files.find().exec()).map((doc) => doc.toJSON?.() || doc);
          const allChunkDocs = (await db.desktop_file_chunks.find().exec()).map((doc) => doc.toJSON?.() || doc);
          const filesById = new Map(allFileDocs.map((doc) => [doc.id, doc]));
          const chunksByFileId = new Map();
          for (const chunk of allChunkDocs) {
            const fileId = chunk.file_id || '';
            if (!fileId) continue;
            const list = chunksByFileId.get(fileId) || [];
            list.push(chunk);
            chunksByFileId.set(fileId, list);
          }
          const receivedFiles = [];
          const missing = [];
          const mismatched = [];
          for (const file of expected) {
            const receivedFileDoc = filesById.get(file.id);
            const allChunks = chunksByFileId.get(file.id) || [];
            const active = selectActiveFileChunks(allChunks, receivedFileDoc?.content_generation_id || '');
            if (!receivedFileDoc || !active.complete) {
              missing.push({
                id: file.id,
                relativePath: file.relativePath || '',
                hasFile: Boolean(receivedFileDoc),
                chunkCount: allChunks.length,
                expectedTotal: active.expectedTotal || 0,
              });
              continue;
            }
            const payload = atob(active.chunks.map((doc) => doc.data).join(''));
            const actualPath = receivedFileDoc.virtual_path || receivedFileDoc.path || '';
            if (actualPath !== file.expectedVirtualPath) {
              throw new Error(`workspace artifact virtual path mismatch: ${JSON.stringify({
                id: file.id,
                expected: file.expectedVirtualPath,
                actual: actualPath,
                file: receivedFileDoc,
              })}`);
            }
            if (payload !== file.content) {
              mismatched.push({
                id: file.id,
                relativePath: file.relativePath || '',
                expectedLength: String(file.content || '').length,
                actualLength: payload.length,
                generationId: receivedFileDoc.content_generation_id || active.chunks[0]?.generation_id || '',
              });
              continue;
            }
            receivedFiles.push({
              file: receivedFileDoc,
              chunks: active.chunks,
              payload,
              generationId: receivedFileDoc.content_generation_id || active.chunks[0]?.generation_id || '',
              allChunkCount: allChunks.length,
              expectedVirtualPath: file.expectedVirtualPath,
              relativePath: file.relativePath || '',
            });
          }
          lastSeen = {
            expectedCount: expected.length,
            receivedCount: receivedFiles.length,
            totalFileCount: allFileDocs.length,
            totalChunkCount: allChunkDocs.length,
            missing: missing.slice(0, 8),
            mismatched: mismatched.slice(0, 8),
          };
          if (receivedFiles.length === expected.length) return receivedFiles;
          await delay(500);
        }
        throw new Error(`browser did not receive expected workspace artifacts: ${JSON.stringify(lastSeen)}`);
      }

      async function runBusinessOsUiRegression() {
        const waitFor = async (predicate, ms, label) => {
          const deadline = Date.now() + ms;
          let last = null;
          while (Date.now() < deadline) {
            last = await predicate();
            if (last?.ok) return last;
            await delay(100);
          }
          throw new Error(`${label} timed out: ${JSON.stringify(last)}`);
        };
        const failureText = () => {
          const text = document.body?.innerText || '';
          return /App-Start fehlgeschlagen|Module startup failed|System-Start fehlgeschlagen/i.test(text)
            ? text.slice(0, 1000)
            : '';
        };
        const expectedRequiredModules = ['ctox', 'documents', 'knowledge', 'research'];
        const expectedSecondaryModules = [
          'matching',
          'conversations',
          'outbound',
          'tickets',
          'shiftflow',
          'buchhaltung',
          'coding-agents',
          'app-store',
          'browser',
          'calendar',
          'creator',
          'notes',
          'reports',
          'spreadsheets',
        ];
        const moduleCatalog = await waitFor(() => {
          const moduleIds = Array.isArray(appState?.modules)
            ? appState.modules.map((mod) => mod?.id).filter(Boolean)
            : [];
          const requiredModules = expectedRequiredModules.filter((id) => moduleIds.includes(id));
          const secondaryModules = expectedSecondaryModules.filter((id) => moduleIds.includes(id));
          const expectedCatalogModules = [...expectedRequiredModules, ...expectedSecondaryModules];
          return {
            ok: requiredModules.length === expectedRequiredModules.length
              && secondaryModules.length === expectedSecondaryModules.length,
            moduleIds,
            requiredModules,
            secondaryModules,
            missingModules: expectedCatalogModules.filter((id) => !moduleIds.includes(id)),
          };
        }, 15000, 'required Business OS module catalog');
        const moduleIds = moduleCatalog.moduleIds;
        const requiredModules = moduleCatalog.requiredModules;
        const secondaryModules = moduleCatalog.secondaryModules;
        const openStartMenu = async () => {
          const startButton = document.querySelector('[data-shell-start]');
          if (!startButton) throw new Error('Business OS start menu button is missing');
          let panel = document.querySelector('.shell-start-menu-panel');
          if (!panel?.classList?.contains('is-active')) {
            startButton.click();
          }
          return waitFor(() => {
            panel = document.querySelector('.shell-start-menu-panel');
            const items = panel ? [...panel.querySelectorAll('.start-menu-item')] : [];
            return {
              ok: Boolean(panel?.classList?.contains('is-active') && items.length > 0),
              itemCount: items.length,
              labels: items.map((item) => item.textContent?.trim() || '').slice(0, 12),
            };
          }, 5000, 'start menu open');
        };
        const waitForOpenedModule = async (expectedModuleId, label) => {
          const opened = await waitFor(() => {
            const activeModule = document.body?.dataset?.activeModule || appState?.activeModule?.id || '';
            const loading = Boolean(document.body?.dataset?.moduleLoading);
            const errorText = failureText();
            return {
              ok: activeModule === expectedModuleId && !loading && !errorText,
              activeModule,
              loading,
              errorText,
            };
          }, 10000, label);
          return {
            expectedModuleId,
            activeModule: opened.activeModule,
          };
        };
        const openModuleByHash = async (expectedModuleId) => {
          location.hash = expectedModuleId;
          window.dispatchEvent(new HashChangeEvent('hashchange'));
          return waitForOpenedModule(expectedModuleId, `open module ${expectedModuleId}`);
        };
        const rectEvidence = (selector) => {
          const element = document.querySelector(selector);
          if (!element) return null;
          const rect = element.getBoundingClientRect();
          const style = getComputedStyle(element);
          return {
            selector,
            width: Math.round(rect.width),
            height: Math.round(rect.height),
            top: Math.round(rect.top),
            left: Math.round(rect.left),
            visible: rect.width > 0
              && rect.height > 0
              && style.display !== 'none'
              && style.visibility !== 'hidden'
              && Number(style.opacity || 1) > 0,
          };
        };
        const moduleRenderContracts = {
          ctox: {
            selectors: ['[data-ctox-harness]', '[data-ctox-left]', '[data-ctox-main]', '.ctox-task-board'],
            minTextLength: 40,
          },
          documents: {
            selectors: ['[data-documents-module]', '.documents-explorer', '[data-documents-list]', '[data-documents-editor]'],
            minTextLength: 40,
          },
          knowledge: {
            selectors: ['[data-knowledge-root]', '[data-knowledge-list]', '[data-markdown-view]'],
            minTextLength: 40,
          },
          research: {
            selectors: ['[data-research-root]', '.research-left', '.research-center', '.research-right'],
            minTextLength: 60,
          },
          matching: {
            selectors: ['.app', '#left', '#center', '#right'],
            minTextLength: 60,
          },
          conversations: {
            selectors: ['[data-conv-root]', '.conv-left', '.conv-center', '.conv-right'],
            minTextLength: 60,
          },
          outbound: {
            selectors: ['[data-outbound-root]', '.outbound-left', '.outbound-center'],
            minTextLength: 30,
          },
          tickets: {
            selectors: ['[data-tickets-root]', '[data-ticket-list]', '[data-ticket-detail]', '[data-ticket-context]'],
            minTextLength: 40,
          },
          shiftflow: {
            selectors: ['[data-shiftflow-root]', '#shiftflow-left', '#shiftflow-center', '#schedulerView'],
            minTextLength: 80,
          },
          buchhaltung: {
            selectors: ['[data-fibu-root]', '.fibu-left', '.fibu-center', '[data-fibu-nav]'],
            minTextLength: 80,
          },
          'coding-agents': {
            selectors: ['[data-coding-agents-root]', '.coding-agents-left', '.coding-agents-center', '#workbench-chat-feed'],
            minTextLength: 80,
          },
          'app-store': {
            selectors: ['[data-app-store-root]', '.store-left', '.store-center', '[data-apps-grid]'],
            minTextLength: 80,
          },
          browser: {
            selectors: ['[data-browser-root]', '.browser-sidebar', '.browser-workbench', '[data-browser-canvas]'],
            minTextLength: 70,
          },
          calendar: {
            selectors: ['[data-calendar-root]', '#calendar-left', '#calendar-center', '#calendar-right'],
            minTextLength: 80,
          },
          creator: {
            selectors: ['[data-creator-root]', '.creator-left', '.creator-center', '#expert-accordion-btn'],
            minTextLength: 120,
          },
          notes: {
            selectors: ['[data-notes-root]', '.notes-sidebar-pane', '.notes-list-pane', '.notes-center'],
            minTextLength: 120,
          },
          reports: {
            selectors: ['[data-reports-root]', '.reports-rail', '[data-reports-list]', '[data-reports-detail]'],
            minTextLength: 50,
          },
          spreadsheets: {
            selectors: ['[data-spreadsheets-module]', '[data-spreadsheets-editor]', '.spreadsheets-workbench'],
            minTextLength: 50,
          },
        };
        const collectModuleRenderEvidence = (moduleId) => {
          const contract = moduleRenderContracts[moduleId];
          if (!contract) return { moduleId, ok: true, selectors: [], textLength: 0 };
          const text = document.body?.innerText || '';
          const selectorEvidence = contract.selectors.map((selector) => rectEvidence(selector));
          const missing = selectorEvidence
            .filter((entry) => !entry?.visible || entry.width < 24 || entry.height < 16)
            .map((entry, index) => entry || { selector: contract.selectors[index], missing: true });
          const errorText = failureText();
          const moduleTextLength = contract.selectors
            .map((selector) => document.querySelector(selector)?.innerText || document.querySelector(selector)?.textContent || '')
            .join('\n')
            .trim()
            .length;
          const loadingTextVisible = /Workspace wird geladen|Loading CTOX runtime|Loading tasks|Documents\s+Workspace wird geladen/i.test(text);
          const ok = missing.length === 0
            && !errorText
            && !loadingTextVisible
            && moduleTextLength >= contract.minTextLength;
          return {
            moduleId,
            ok,
            selectors: selectorEvidence,
            textLength: moduleTextLength,
            missing,
            errorText,
            loadingTextVisible,
          };
        };
        const waitForModuleRendered = (moduleId) => waitFor(() => collectModuleRenderEvidence(moduleId), 10000, `render module ${moduleId}`);
        const waitForElement = async (selector, label, ms = 5000) => waitFor(() => {
          const entry = rectEvidence(selector);
          return {
            ok: Boolean(entry?.visible),
            selector,
            entry,
          };
        }, ms, label || selector);
        const waitForAbsent = async (selector, label, ms = 5000) => waitFor(() => ({
          ok: !document.querySelector(selector),
          selector,
          exists: Boolean(document.querySelector(selector)),
        }), ms, label || `${selector} absent`);
        const runModuleInteraction = async (moduleId) => {
          const evidence = { moduleId, actions: [] };
          if (moduleId === 'ctox') {
            const zoomLabel = document.querySelector('[data-flow-control] span');
            const zoomIn = document.querySelector('[data-flow-control] [data-zoom="+"]');
            const zoomReset = document.querySelector('[data-flow-control] [data-zoom="reset"]');
            if (!zoomLabel || !zoomIn || !zoomReset) {
              throw new Error(`CTOX zoom controls missing: ${JSON.stringify({
                zoomLabel: Boolean(zoomLabel),
                zoomIn: Boolean(zoomIn),
                zoomReset: Boolean(zoomReset),
              })}`);
            }
            const before = zoomLabel.textContent?.trim() || '';
            zoomIn.click();
            const changed = await waitFor(() => {
              const after = document.querySelector('[data-flow-control] span')?.textContent?.trim() || '';
              return { ok: after && after !== before, before, after };
            }, 5000, 'ctox zoom interaction');
            document.querySelector('[data-flow-control] [data-zoom="reset"]')?.click();
            evidence.actions.push(`ctox-zoom:${before}->${changed.after}`);
          } else if (moduleId === 'documents') {
            const newButton = document.querySelector('[data-documents-new-markdown]');
            if (!newButton) throw new Error('Documents new-document action is missing');
            newButton.click();
            await waitForElement('[data-documents-new-form]', 'documents new-document drawer');
            document.querySelector('[data-documents-drawer-close], [data-documents-drawer-cancel]')?.click();
            await waitForAbsent('[data-documents-new-form]', 'documents drawer close');
            evidence.actions.push('documents-new-drawer');
          } else if (moduleId === 'knowledge') {
            const root = document.querySelector('[data-knowledge-root]');
            if (!root) throw new Error('Knowledge root is missing');
            const selectableEvidence = () => {
              const selected = root.querySelector('.knowledge-item[aria-current="true"], .knowledge-bundle[aria-current="true"]');
              const firstItem = root.querySelector('.knowledge-item');
              return {
                ok: Boolean(selected || firstItem),
                selected: selected?.getAttribute('data-knowledge-id') || selected?.getAttribute('data-bundle-id') || '',
                firstItem: firstItem?.getAttribute('data-knowledge-id') || '',
              };
            };
            let selectionReady = selectableEvidence();
            if (!selectionReady.ok) {
              await seedBusinessOsUiRegressionFixtures();
              selectionReady = await waitFor(selectableEvidence, 15000, 'knowledge selectable item');
            }
            if (!root.querySelector('.knowledge-item[aria-current="true"]') && selectionReady.firstItem) {
              root.querySelector('.knowledge-item')?.click();
              await waitFor(() => {
                const selected = root.querySelector('.knowledge-item[aria-current="true"]');
                return {
                  ok: Boolean(selected),
                  selected: selected?.getAttribute('data-knowledge-id') || '',
                };
              }, 5000, 'knowledge selected item after click');
            }
            for (const tab of ['runbooks', 'data', 'skill']) {
              const ready = await waitFor(() => {
                const button = root.querySelector(`[data-tab="${tab}"]`);
                return {
                  ok: Boolean(button && !button.disabled && button.getAttribute('aria-disabled') !== 'true'),
                  tab,
                  exists: Boolean(button),
                  disabled: button?.disabled ?? null,
                  ariaDisabled: button?.getAttribute('aria-disabled') ?? null,
                };
              }, 10000, `knowledge tab ${tab} ready`);
              const button = root.querySelector(`[data-tab="${tab}"]`);
              if (!button || !ready.ok) throw new Error(`Knowledge tab button missing or disabled: ${tab}`);
              button.click();
              await waitFor(() => {
                const panel = root.querySelector(`[data-panel="${tab}"]`);
                const pressed = root.querySelector(`[data-tab="${tab}"]`)?.getAttribute('aria-pressed') === 'true';
                return {
                  ok: Boolean(panel && !panel.hidden && pressed),
                  tab,
                  panelHidden: panel?.hidden ?? null,
                  pressed,
                };
              }, 5000, `knowledge tab ${tab}`);
              evidence.actions.push(`knowledge-tab-${tab}`);
            }
          } else if (moduleId === 'research') {
            const newTask = document.querySelector('[data-action="new-task"]');
            if (!newTask) throw new Error('Research new-task action is missing');
            newTask.click();
            await waitForElement('[data-research-task-form]', 'research new-task modal');
            document.querySelector('.research-modal [data-close]')?.click();
            await waitForAbsent('[data-research-task-form]', 'research modal close');
            evidence.actions.push('research-new-task-modal');
          } else if (moduleId === 'matching') {
            const tabMap = document.querySelector('#tabMap');
            const tabList = document.querySelector('#tabList');
            if (!tabMap || !tabList) throw new Error('Matching list/matrix tabs are missing');
            tabMap.click();
            await waitFor(() => {
              const mapWrap = document.querySelector('#mapWrap');
              const requirementList = document.querySelector('#requirementList');
              return {
                ok: Boolean(mapWrap?.classList?.contains('active')
                  && tabMap.classList.contains('active')
                  && !tabList.classList.contains('active')
                  && requirementList?.style?.display === 'none'),
                mapActive: Boolean(mapWrap?.classList?.contains('active')),
                tabMapActive: tabMap.classList.contains('active'),
                tabListActive: tabList.classList.contains('active'),
                requirementDisplay: requirementList?.style?.display || '',
              };
            }, 5000, 'matching matrix tab');
            tabList.click();
            await waitFor(() => {
              const mapWrap = document.querySelector('#mapWrap');
              const requirementList = document.querySelector('#requirementList');
              return {
                ok: Boolean(!mapWrap?.classList?.contains('active')
                  && tabList.classList.contains('active')
                  && !tabMap.classList.contains('active')
                  && requirementList?.style?.display !== 'none'),
                mapActive: Boolean(mapWrap?.classList?.contains('active')),
                tabMapActive: tabMap.classList.contains('active'),
                tabListActive: tabList.classList.contains('active'),
                requirementDisplay: requirementList?.style?.display || '',
              };
            }, 5000, 'matching list tab');
            evidence.actions.push('matching-list-matrix-tabs');
          } else if (moduleId === 'conversations') {
            const whatsapp = document.querySelector('[data-conv-channel-filters] [data-channel="whatsapp"]');
            const all = document.querySelector('[data-conv-channel-filters] [data-channel="all"]');
            if (!whatsapp || !all) throw new Error('Conversations channel filters are missing');
            whatsapp.click();
            await waitFor(() => ({
              ok: whatsapp.classList.contains('is-active') && !all.classList.contains('is-active'),
              whatsappActive: whatsapp.classList.contains('is-active'),
              allActive: all.classList.contains('is-active'),
            }), 5000, 'conversations whatsapp filter');
            all.click();
            await waitFor(() => ({
              ok: all.classList.contains('is-active') && !whatsapp.classList.contains('is-active'),
              whatsappActive: whatsapp.classList.contains('is-active'),
              allActive: all.classList.contains('is-active'),
            }), 5000, 'conversations all filter');
            evidence.actions.push('conversations-channel-filter');
          } else if (moduleId === 'outbound') {
            const toggle = document.querySelector('#toggle-compact');
            if (!toggle) throw new Error('Outbound compact view toggle is missing');
            const before = Boolean(toggle.checked);
            toggle.click();
            await waitFor(() => {
              const current = document.querySelector('#toggle-compact');
              const after = Boolean(current?.checked);
              return {
                ok: after !== before,
                before,
                after,
              };
            }, 5000, 'outbound compact view toggle');
            document.querySelector('#toggle-compact')?.click();
            evidence.actions.push('outbound-compact-view-toggle');
          } else if (moduleId === 'tickets') {
            const search = document.querySelector('[data-ticket-search]');
            const stateFilter = document.querySelector('[data-ticket-state]');
            if (!search || !stateFilter) throw new Error('Tickets search or state filter controls are missing');
            search.value = 'regression-smoke';
            search.dispatchEvent(new Event('input', { bubbles: true }));
            stateFilter.value = 'open';
            stateFilter.dispatchEvent(new Event('change', { bubbles: true }));
            await waitFor(() => ({
              ok: document.querySelector('[data-ticket-search]')?.value === 'regression-smoke'
                && document.querySelector('[data-ticket-state]')?.value === 'open',
              search: document.querySelector('[data-ticket-search]')?.value || '',
              state: document.querySelector('[data-ticket-state]')?.value || '',
            }), 5000, 'tickets search and state filter');
            search.value = '';
            search.dispatchEvent(new Event('input', { bubbles: true }));
            stateFilter.value = 'all';
            stateFilter.dispatchEvent(new Event('change', { bubbles: true }));
            evidence.actions.push('tickets-search-status-filter');
          } else if (moduleId === 'shiftflow') {
            const scheduler = document.querySelector('#viewSchedulerTabBtn');
            const timesheets = document.querySelector('#viewTimesheetsTabBtn');
            const billing = document.querySelector('#viewBillingTabBtn');
            if (!scheduler || !timesheets || !billing) throw new Error('Shiftflow center tabs are missing');
            timesheets.click();
            await waitFor(() => ({
              ok: timesheets.classList.contains('active')
                && !document.querySelector('#timesheetsView')?.classList?.contains('hidden')
                && document.querySelector('#schedulerView')?.classList?.contains('hidden'),
              title: document.querySelector('#centerPaneTitle')?.textContent?.trim() || '',
            }), 5000, 'shiftflow timesheets tab');
            billing.click();
            await waitFor(() => ({
              ok: billing.classList.contains('active')
                && !document.querySelector('#billingView')?.classList?.contains('hidden')
                && document.querySelector('#timesheetsView')?.classList?.contains('hidden'),
              title: document.querySelector('#centerPaneTitle')?.textContent?.trim() || '',
            }), 5000, 'shiftflow billing tab');
            scheduler.click();
            await waitFor(() => ({
              ok: scheduler.classList.contains('active')
                && !document.querySelector('#schedulerView')?.classList?.contains('hidden')
                && document.querySelector('#billingView')?.classList?.contains('hidden'),
              title: document.querySelector('#centerPaneTitle')?.textContent?.trim() || '',
            }), 5000, 'shiftflow scheduler tab');
            evidence.actions.push('shiftflow-center-tabs');
          } else if (moduleId === 'buchhaltung') {
            for (const navId of ['journal', 'reports', 'skr']) {
              const button = document.querySelector(`[data-nav="${navId}"]`);
              if (!button) throw new Error(`Buchhaltung nav item missing: ${navId}`);
              button.click();
              await waitFor(() => {
                const panel = document.querySelector(`[data-panel="${navId}"]`);
                const title = document.querySelector('[data-active-title]')?.textContent?.trim() || '';
                return {
                  ok: Boolean(panel && !panel.hidden && button.classList.contains('active') && title),
                  navId,
                  panelHidden: panel?.hidden ?? null,
                  active: button.classList.contains('active'),
                  title,
                };
              }, 5000, `buchhaltung nav ${navId}`);
            }
            evidence.actions.push('buchhaltung-nav-switch');
          } else if (moduleId === 'coding-agents') {
            const openSettings = document.querySelector('#open-settings-btn');
            const closeSettings = document.querySelector('#close-settings-btn');
            const modal = document.querySelector('#settings-modal');
            if (!openSettings || !closeSettings || !modal) {
              throw new Error(`Coding Agents settings modal controls missing: ${JSON.stringify({
                openSettings: Boolean(openSettings),
                closeSettings: Boolean(closeSettings),
                modal: Boolean(modal),
              })}`);
            }
            openSettings.click();
            await waitFor(() => ({
              ok: !document.querySelector('#settings-modal')?.hasAttribute('hidden'),
              hidden: document.querySelector('#settings-modal')?.hasAttribute('hidden') ?? null,
            }), 5000, 'coding agents settings open');
            closeSettings.click();
            await waitFor(() => ({
              ok: document.querySelector('#settings-modal')?.hasAttribute('hidden') === true,
              hidden: document.querySelector('#settings-modal')?.hasAttribute('hidden') ?? null,
            }), 5000, 'coding agents settings close');
            evidence.actions.push('coding-agents-settings-modal');
          } else if (moduleId === 'app-store') {
            const listView = document.querySelector('[data-view="list"]');
            const gridView = document.querySelector('[data-view="grid"]');
            const installedScope = document.querySelector('[data-scope="installed"]');
            const marketplaceScope = document.querySelector('[data-scope="marketplace"]');
            if (!listView || !gridView || !installedScope || !marketplaceScope) {
              throw new Error('App Store view or scope controls are missing');
            }
            listView.click();
            await waitFor(() => ({
              ok: listView.classList.contains('active') && !gridView.classList.contains('active'),
              listActive: listView.classList.contains('active'),
              gridActive: gridView.classList.contains('active'),
            }), 5000, 'app-store list view');
            installedScope.click();
            await waitFor(() => ({
              ok: installedScope.classList.contains('active') && !marketplaceScope.classList.contains('active'),
              installedActive: installedScope.classList.contains('active'),
              marketplaceActive: marketplaceScope.classList.contains('active'),
            }), 5000, 'app-store installed scope');
            gridView.click();
            marketplaceScope.click();
            evidence.actions.push('app-store-view-scope');
          } else if (moduleId === 'browser') {
            const address = document.querySelector('[data-browser-address]');
            const refresh = document.querySelector('[data-browser-refresh]');
            if (!address || !refresh) throw new Error('Browser address or refresh controls are missing');
            address.value = 'https://example.com';
            address.dispatchEvent(new Event('input', { bubbles: true }));
            refresh.click();
            await waitFor(() => ({
              ok: document.querySelector('[data-browser-address]')?.value === 'https://example.com'
                && Boolean(document.querySelector('[data-browser-status-chip]')?.textContent?.trim()),
              address: document.querySelector('[data-browser-address]')?.value || '',
              status: document.querySelector('[data-browser-status-chip]')?.textContent?.trim() || '',
            }), 5000, 'browser address input');
            evidence.actions.push('browser-address-refresh');
          } else if (moduleId === 'calendar') {
            const newEvent = document.querySelector('#btnNewEvent');
            const closeDrawer = document.querySelector('#closeDrawerBtn');
            if (!newEvent || !closeDrawer) throw new Error('Calendar new-event drawer controls are missing');
            newEvent.click();
            await waitFor(() => ({
              ok: document.querySelector('#calendarInspectorDrawer')?.classList?.contains('open')
                || document.querySelector('#calendarInspectorDrawer')?.classList?.contains('is-open')
                || document.querySelector('#calendarInspectorDrawer')?.getAttribute('aria-hidden') === 'false'
                || Boolean(document.querySelector('#calendarInspectorDrawer form')),
              drawerClass: document.querySelector('#calendarInspectorDrawer')?.className || '',
              hasForm: Boolean(document.querySelector('#calendarInspectorDrawer form')),
            }), 5000, 'calendar new-event drawer');
            document.querySelector('#closeDrawerBtn')?.click();
            await delay(100);
            evidence.actions.push('calendar-new-event-drawer');
          } else if (moduleId === 'creator') {
            const trigger = document.querySelector('#expert-accordion-btn');
            const content = document.querySelector('#expert-accordion-content');
            if (!trigger || !content) throw new Error('Creator expert accordion controls are missing');
            const beforeCollapsed = content.classList.contains('is-collapsed');
            trigger.click();
            await waitFor(() => ({
              ok: content.classList.contains('is-collapsed') !== beforeCollapsed,
              beforeCollapsed,
              afterCollapsed: content.classList.contains('is-collapsed'),
            }), 5000, 'creator accordion toggle');
            trigger.click();
            evidence.actions.push('creator-expert-accordion');
          } else if (moduleId === 'notes') {
            const favorites = document.querySelector('[data-nav-category="favorites"]');
            const notes = document.querySelector('[data-nav-category="notes"]');
            const filter = document.querySelector('[data-action="toggle-filter"]');
            if (!favorites || !notes || !filter) throw new Error('Notes navigation/filter controls are missing');
            favorites.click();
            await waitFor(() => ({
              ok: favorites.classList.contains('active') && !notes.classList.contains('active'),
              favoritesActive: favorites.classList.contains('active'),
              notesActive: notes.classList.contains('active'),
            }), 5000, 'notes favorites nav');
            notes.click();
            filter.click();
            await delay(100);
            evidence.actions.push('notes-nav-filter');
          } else if (moduleId === 'reports') {
            const kind = document.querySelector('[data-report-kind]');
            const status = document.querySelector('[data-report-status]');
            if (!kind || !status) throw new Error('Reports filter controls are missing');
            kind.value = 'bug';
            kind.dispatchEvent(new Event('change', { bubbles: true }));
            await waitFor(() => ({
              ok: document.querySelector('[data-report-kind]')?.value === 'bug',
              kind: document.querySelector('[data-report-kind]')?.value || '',
            }), 5000, 'reports kind filter');
            status.value = 'open';
            status.dispatchEvent(new Event('change', { bubbles: true }));
            kind.value = 'all';
            kind.dispatchEvent(new Event('change', { bubbles: true }));
            status.value = 'all';
            status.dispatchEvent(new Event('change', { bubbles: true }));
            evidence.actions.push('reports-filter-controls');
          } else if (moduleId === 'spreadsheets') {
            const search = document.querySelector('[data-spreadsheets-search]');
            if (!search) throw new Error('Spreadsheets search control is missing');
            search.value = 'regression-smoke';
            search.dispatchEvent(new Event('input', { bubbles: true }));
            await waitFor(() => ({
              ok: document.querySelector('[data-spreadsheets-search]')?.value === 'regression-smoke',
              value: document.querySelector('[data-spreadsheets-search]')?.value || '',
            }), 5000, 'spreadsheets search input');
            document.querySelector('[data-spreadsheets-search]').value = '';
            document.querySelector('[data-spreadsheets-search]').dispatchEvent(new Event('input', { bubbles: true }));
            evidence.actions.push('spreadsheets-search-filter');
          }
          return evidence.actions.length ? evidence : null;
        };
        const seedBusinessOsUiRegressionFixtures = async () => {
          const rawDb = appState?.db?.raw;
          if (!rawDb?.knowledge_items || !rawDb?.knowledge_runbooks || !rawDb?.knowledge_tables) return;
          const now = Date.now();
          const upsert = async (collection, document) => {
            if (collection.incrementalUpsert) {
              await collection.incrementalUpsert(document);
              return;
            }
            if (collection.upsert) {
              await collection.upsert(document);
              return;
            }
            try {
              await collection.insert(document);
            } catch (error) {
              throw new Error(`failed to seed Business OS UI regression fixture ${document.id}: ${error?.message || String(error)}`);
            }
          };
          await upsert(rawDb.knowledge_items, {
            id: 'ui_regression_skillbook',
            kind: 'skillbook',
            title: 'UI Regression Skillbook',
            subtitle: 'Business OS smoke fixture',
            summary: 'Deterministic fixture for Business OS UI regression tab coverage.',
            source_path: 'ui-regression',
            linked_runbook_ids: ['ui_regression_runbook'],
            updated_at: new Date(now).toISOString(),
            updated_at_ms: now,
          });
          await upsert(rawDb.knowledge_items, {
            id: 'ui_regression_skill',
            kind: 'skill',
            title: 'UI Regression Skill',
            subtitle: 'Business OS smoke fixture',
            summary: 'Allows deterministic Knowledge tab interactions in the UI regression harness.',
            source_path: 'ui-regression',
            skillbook_id: 'ui_regression_skillbook',
            linked_runbook_ids: ['ui_regression_runbook'],
            updated_at: new Date(now).toISOString(),
            updated_at_ms: now,
          });
          await upsert(rawDb.knowledge_runbooks, {
            id: 'ui_regression_runbook',
            kind: 'runbook',
            title: 'UI Regression Runbook',
            subtitle: 'Business OS smoke fixture',
            summary: 'Runbook fixture for tab coverage.',
            prompt: 'Verify the Knowledge module tab surface.',
            source_path: 'ui-regression',
            updated_at: new Date(now).toISOString(),
            updated_at_ms: now,
          });
          await upsert(rawDb.knowledge_tables, {
            id: 'ui_regression_table',
            kind: 'dataframe',
            title: 'UI Regression Data',
            subtitle: 'Business OS smoke fixture',
            summary: 'Table fixture for tab coverage.',
            row_count: 1,
            source_path: 'ui-regression',
            updated_at: new Date(now).toISOString(),
            updated_at_ms: now,
          });
        };
        const openAndVerifyModule = async (moduleId, opener, label) => {
          const opened = await opener();
          const renderEvidence = await waitForModuleRendered(moduleId);
          const interactionEvidence = await runModuleInteraction(moduleId);
          return {
            ...opened,
            expectedModuleId: moduleId,
            renderEvidence,
            interactionEvidence,
            label,
          };
        };
        const collectVisualEvidence = () => {
          const workspace = rectEvidence('.workspace-frame');
          const moduleHost = rectEvidence('[data-module-host]');
          const startButton = rectEvidence('[data-shell-start]');
          const desktopIconCount = document.querySelectorAll('.desktop-icon').length;
          const desktopSurface = rectEvidence('[data-module-root], .desktop-root, [data-desktop-icons]');
          const bodyText = document.body?.innerText || '';
          const loadingTextVisible = /Loading workspace|waiting for module manifests|App-Start fehlgeschlagen|Module startup failed|System-Start fehlgeschlagen/i.test(bodyText);
          const evidence = {
            workspace,
            moduleHost,
            startButton,
            desktopSurface,
            desktopIconCount,
            bodyLoading: Boolean(document.body?.dataset?.moduleLoading),
            activeModule: document.body?.dataset?.activeModule || appState?.activeModule?.id || '',
            loadingTextVisible,
          };
          const problems = [];
          if (!workspace?.visible || workspace.width < 700 || workspace.height < 450) {
            problems.push(`workspace frame not visible enough: ${JSON.stringify(workspace)}`);
          }
          if (!moduleHost?.visible || moduleHost.width < 500 || moduleHost.height < 350) {
            problems.push(`module host not visible enough: ${JSON.stringify(moduleHost)}`);
          }
          if (!startButton?.visible) {
            problems.push(`start button not visible: ${JSON.stringify(startButton)}`);
          }
          if (!desktopSurface?.visible || desktopSurface.width < 400 || desktopSurface.height < 300) {
            problems.push(`desktop surface not visible enough: ${JSON.stringify(desktopSurface)}`);
          }
          if (desktopIconCount < 6) {
            problems.push(`desktop icon count too low: ${desktopIconCount}`);
          }
          if (evidence.bodyLoading || loadingTextVisible) {
            problems.push(`Business OS still shows loading or failure state: ${JSON.stringify({ bodyLoading: evidence.bodyLoading, loadingTextVisible })}`);
          }
          if (problems.length) {
            throw new Error(`Business OS visual layout evidence failed: ${problems.join('; ')}`);
          }
          return evidence;
        };

        await seedBusinessOsUiRegressionFixtures();
        const startMenu = await openStartMenu();
        if (startMenu.itemCount < 8) {
          throw new Error(`Business OS start menu rendered too few launch targets: ${JSON.stringify(startMenu)}`);
        }
        const openedModules = [];
        document.querySelector('.shell-start-menu-panel .start-menu-item')?.click();
        openedModules.push(await openAndVerifyModule(
          'ctox',
          () => waitForOpenedModule('ctox', 'open first start menu item'),
          'start-menu',
        ));
        for (const moduleId of requiredModules) {
          if (moduleId === 'ctox') continue;
          openedModules.push(await openAndVerifyModule(
            moduleId,
            () => openModuleByHash(moduleId),
            'hash',
          ));
        }
        const secondaryOpenedModules = [];
        for (const moduleId of secondaryModules) {
          secondaryOpenedModules.push(await openAndVerifyModule(
            moduleId,
            () => openModuleByHash(moduleId),
            'secondary-hash',
          ));
        }
        await openStartMenu();
        document.querySelector('.shell-start-menu-panel .show-desktop-btn')?.click();
        const desktop = await waitFor(() => {
          const activeModule = document.body?.dataset?.activeModule || appState?.activeModule?.id || '';
          const loading = Boolean(document.body?.dataset?.moduleLoading);
          const errorText = failureText();
          return {
            ok: activeModule === 'desktop' && !loading && !errorText,
            activeModule,
            loading,
            errorText,
          };
        }, 10000, 'show desktop');
        const visualEvidence = collectVisualEvidence();
        const status = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
          includeCounts: false,
          requiredCollections: ['business_module_catalog', 'ctox_runtime_settings', 'desktop_files', 'desktop_file_chunks'],
        });
        if (status?.version !== 'business-os-advanced-status-v1') {
          throw new Error(`Business OS UI regression smoke lost advanced status evidence: ${JSON.stringify(status)}`);
        }
        return {
          mode: smokeMode,
          moduleCount: moduleIds.length,
          moduleIds,
          startMenuItemCount: startMenu.itemCount,
          openedModules,
          secondaryOpenedModules,
          desktopOpened: desktop.activeModule === 'desktop',
          activeModule: desktop.activeModule,
          visualEvidence,
          advancedStatusVersion: status.version || '',
          advancedStatusRuntime: status.rxdbRuntime || null,
        };
      }

      async function openDesktopFileViewerAndWait(fileArgs, expectedPayload) {
        const smoke = globalThis.ctoxBusinessOsSmoke;
        if (typeof smoke?.openDesktopApp !== 'function') {
          throw new Error('Business OS smoke API does not expose openDesktopApp for File Viewer UI regression');
        }
        const beforeWindows = document.querySelectorAll('.shell-window').length;
        const windowId = await smoke.openDesktopApp('file-viewer', {
          title: fileArgs.name || 'File Viewer',
          args: fileArgs,
        });
        const deadline = Date.now() + 60000;
        let last = null;
        while (Date.now() < deadline) {
          const windows = [...document.querySelectorAll('.shell-window')];
          const viewerWindows = windows
            .map((win) => {
              const text = win.querySelector('[data-file-text]')?.textContent || '';
              const errorText = win.querySelector('.file-viewer .is-error')?.textContent || '';
              const title = win.querySelector('[data-window-title]')?.textContent?.trim() || '';
              return { win, text, errorText, title };
            })
            .filter((entry) => entry.text || entry.errorText || /File Viewer|brief\.md|smoke/i.test(entry.title));
          last = {
            windowId,
            beforeWindows,
            windowCount: windows.length,
            viewerWindowCount: viewerWindows.length,
            titles: viewerWindows.map((entry) => entry.title).slice(0, 4),
            textLength: Math.max(0, ...viewerWindows.map((entry) => entry.text.length)),
            errorText: viewerWindows.find((entry) => entry.errorText)?.errorText || '',
          };
          if (last.errorText) {
            throw new Error(`Business OS File Viewer desktop app rendered an error: ${JSON.stringify(last)}`);
          }
          if (viewerWindows.some((entry) => entry.text === expectedPayload)) {
            return {
              windowId,
              beforeWindows,
              windowCount: windows.length,
              viewerWindowCount: viewerWindows.length,
              renderedLength: expectedPayload.length,
            };
          }
          await delay(500);
        }
        throw new Error(`Business OS File Viewer desktop app did not render the expected payload: ${JSON.stringify(last)}`);
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

      async function waitForCorruptChunkMetadata(id, ms = 60000) {
        const deadline = Date.now() + ms;
        let lastSeen = null;
        while (Date.now() < deadline) {
          const chunks = (await db.desktop_file_chunks.find().exec())
            .map((doc) => doc.toJSON?.() || doc)
            .filter((doc) => doc.file_id === id)
            .sort((left, right) => Number(left.idx || 0) - Number(right.idx || 0));
          const match = chunks.find((chunk) => {
            const expectedSize = Number(chunk.size_bytes);
            return Number.isFinite(expectedSize) && expectedSize !== String(chunk.data || '').length;
          });
          lastSeen = chunks.map((chunk) => ({
            id: chunk.id,
            idx: chunk.idx,
            size_bytes: chunk.size_bytes,
            actualSizeBytes: String(chunk.data || '').length,
            rev: chunk._rev || '',
          }));
          if (match) {
            return {
              chunk: match,
              expectedSizeBytes: Number(match.size_bytes),
              actualSizeBytes: String(match.data || '').length,
            };
          }
          await delay(500);
        }
        throw new Error(`browser did not receive corrupt chunk metadata ${id}: ${JSON.stringify(lastSeen)}`);
      }

      function isDeletedSmokeChunk(chunk) {
        return chunk?._deleted === true || chunk?.deleted === true || chunk?.is_deleted === true;
      }

      async function tombstoneLocalFileCache(id) {
        const chunkDocs = (await db.desktop_file_chunks.find().exec())
          .filter((doc) => {
            const chunk = doc?.toJSON?.() || doc;
            return chunk?.file_id === id && !isDeletedSmokeChunk(chunk);
          });
        for (const doc of chunkDocs) {
          await doc?.remove?.();
        }
        const fileDoc = await db.desktop_files.findOne(id).exec();
        if (fileDoc) {
          await fileDoc.incrementalPatch?.({
            path: '',
            local_path: '',
            content_state: 'available',
            updated_at_ms: Date.now(),
          });
        }
      }

      async function startDeferredAppFileCollections(label = 'desktop_files') {
        const state = appState || globalThis.ctoxBusinessOsSmoke?.state;
        if (!state?.sync?.startCollection) {
          throw new Error(`Business OS sync runtime is not available for deferred ${label} replication`);
        }
        const fileBridge = await state.sync.startCollection('desktop_files');
        const chunkBridge = await state.sync.startCollection('desktop_file_chunks');
        fileBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app desktop_files replication error', error));
        chunkBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app desktop_file_chunks replication error', error));
        appFileReplicationState = fileBridge?.state || null;
        appChunkReplicationState = chunkBridge?.state || null;
        await bounded(appFileReplicationState?.awaitInitialReplication?.(), 15000);
        await bounded(appChunkReplicationState?.awaitInitialReplication?.(), 15000);
        await bounded(appFileReplicationState?.awaitInSync?.(), 15000);
        await bounded(appChunkReplicationState?.awaitInSync?.(), 15000);
        await waitForNativePeerOpen(appFileReplicationState, 'desktop_files');
        await waitForNativePeerOpen(appChunkReplicationState, 'desktop_file_chunks');
      }

      async function waitForTombstonedChunkState(id, ms = 60000) {
        const deadline = Date.now() + ms;
        let lastSeen = null;
        while (Date.now() < deadline) {
          const fileDoc = await db.desktop_files.findOne(id).exec();
          const file = fileDoc?.toJSON?.() || fileDoc;
          const chunks = (await db.desktop_file_chunks.find().exec())
            .map((doc) => doc.toJSON?.() || doc)
            .filter((doc) => doc.file_id === id)
            .sort((left, right) => Number(left.idx || 0) - Number(right.idx || 0));
          const liveChunks = chunks.filter((chunk) => !isDeletedSmokeChunk(chunk));
          const tombstonedChunks = chunks.filter((chunk) => isDeletedSmokeChunk(chunk));
          lastSeen = {
            file: file ? {
              id: file.id,
              path: file.path || '',
              local_path: file.local_path || '',
              content_state: file.content_state || '',
              deleted: isDeletedSmokeChunk(file),
              rev: file._rev || '',
            } : null,
            liveChunkCount: liveChunks.length,
            tombstonedChunkCount: tombstonedChunks.length,
            chunks: chunks.map((chunk) => ({
              id: chunk.id,
              idx: chunk.idx,
              deleted: isDeletedSmokeChunk(chunk),
              rev: chunk._rev || '',
            })),
          };
          if (file && (isDeletedSmokeChunk(file) || (!file.path && !file.local_path)) && liveChunks.length === 0) {
            return { file, chunks, liveChunks, tombstonedChunks };
          }
          await delay(500);
        }
        throw new Error(`browser did not receive tombstoned chunk state ${id}: ${JSON.stringify(lastSeen)}`);
      }

      async function waitForStaleGenerationState(id, requestedGenerationId, ms = 60000) {
        const deadline = Date.now() + ms;
        let lastSeen = null;
        while (Date.now() < deadline) {
          const fileDoc = await db.desktop_files.findOne(id).exec();
          const file = fileDoc?.toJSON?.() || fileDoc;
          const chunks = (await db.desktop_file_chunks.find().exec())
            .map((doc) => doc.toJSON?.() || doc)
            .filter((doc) => doc.file_id === id)
            .sort((left, right) => Number(left.idx || 0) - Number(right.idx || 0));
          const liveChunks = chunks.filter((chunk) => !isDeletedSmokeChunk(chunk));
          const requestedGenerationChunks = liveChunks.filter((chunk) => chunk.generation_id === requestedGenerationId);
          const availableGenerationIds = [...new Set(liveChunks.map((chunk) => chunk.generation_id || '').filter(Boolean))];
          lastSeen = {
            file: file ? {
              id: file.id,
              path: file.path || '',
              local_path: file.local_path || '',
              content_state: file.content_state || '',
              content_generation_id: file.content_generation_id || '',
              rev: file._rev || '',
            } : null,
            requestedGenerationId,
            requestedGenerationChunkCount: requestedGenerationChunks.length,
            liveChunkCount: liveChunks.length,
            availableGenerationIds,
          };
          if (file
            && file.content_generation_id === requestedGenerationId
            && !file.path
            && !file.local_path
            && liveChunks.length > 0
            && requestedGenerationChunks.length === 0) {
            return { file, chunks, liveChunks, requestedGenerationChunks, availableGenerationIds };
          }
          await delay(500);
        }
        throw new Error(`browser did not receive stale generation state ${id}: ${JSON.stringify(lastSeen)}`);
      }

      async function waitForFileIntegrityStatus(expectedCode, ms = 30000) {
        const deadline = Date.now() + ms;
        let lastSnapshot = null;
        while (Date.now() < deadline) {
          lastSnapshot = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({ includeCounts: false });
          const errors = Array.isArray(lastSnapshot?.fileIntegrity?.errors)
            ? lastSnapshot.fileIntegrity.errors
            : [];
          const match = errors.find((error) => error?.name === 'CtoxFileChunkIntegrityError' && error?.code === expectedCode);
          if (match) return { error: match, snapshot: lastSnapshot };
          await delay(250);
        }
        throw new Error(`Business OS did not expose file integrity error ${expectedCode}: ${JSON.stringify(lastSnapshot, null, 2)}`);
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
        const criticalCollections = [
          'business_module_catalog',
          'ctox_runtime_settings',
          'business_commands',
          'ctox_queue_tasks',
          'desktop_files',
          'desktop_file_chunks',
        ];
        const preRestartState = globalThis.ctoxBusinessOsSmoke?.state;
        const preRestartDiagnostics = preRestartState?.syncDiagnostics?.collections || {};
        const activeCollectionsBeforeRestart = Object.entries(preRestartDiagnostics)
          .filter(([, entry]) => {
            const status = entry?.connectionStatus || entry?.status || '';
            return status && status !== 'stopped' && status !== 'paused';
          })
          .map(([collection]) => collection);
        const suspendCollections = [...new Set([
          ...activeCollectionsBeforeRestart,
          ...criticalCollections,
        ])];
        const usedSuspend = typeof preRestartState?.sync?.suspendCollections === 'function';
        if (usedSuspend) {
          await preRestartState.sync.suspendCollections(suspendCollections, 'native-peer-controlled-restart');
        } else {
          for (const collection of suspendCollections) {
            await preRestartState?.sync?.stopCollection?.(collection).catch(() => null);
          }
        }
        await globalThis.__ctoxRestartNativePeer?.();
        const repairedState = globalThis.ctoxBusinessOsSmoke?.state;
        if (!repairedState?.db?.raw?.desktop_files || !repairedState?.db?.raw?.desktop_file_chunks) {
          throw new Error('Business OS file collections were not available after native peer restart');
        }
        db = repairedState.db.raw;
        if (typeof repairedState.sync?.resumeCollections === 'function' && usedSuspend) {
          const repairedBridges = await repairedState.sync.resumeCollections(criticalCollections);
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
          await waitForNativePeerOpen(appCommandReplicationState, 'business_commands');
          await waitForNativePeerOpen(appQueueReplicationState, 'ctox_queue_tasks');
          await waitForAppSyncCollections(criticalCollections);
          return repairedState;
        }
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
          await waitForNativePeerOpen(appCommandReplicationState, 'business_commands');
          await waitForNativePeerOpen(appQueueReplicationState, 'ctox_queue_tasks');
          await waitForAppSyncCollections(criticalCollections);
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
        await waitForNativePeerOpen(appCommandReplicationState, 'business_commands');
        await waitForNativePeerOpen(appQueueReplicationState, 'ctox_queue_tasks');
        await waitForAppSyncCollections(criticalCollections);
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

      async function runOutboundActiveUiSmoke() {
        const state = globalThis.ctoxBusinessOsSmoke?.state;
        const rawDb = state?.db?.raw;
        if (!state?.sync?.startCollection || !rawDb?.outbound_campaigns || !rawDb?.outbound_pipeline_items) {
          throw new Error('Outbound active UI smoke requires Business OS app DB and outbound collections');
        }
        const waitFor = async (probe, timeoutMs, label) => {
          const deadline = Date.now() + timeoutMs;
          let last = null;
          while (Date.now() < deadline) {
            last = await probe();
            if (last?.ok) return last;
            await delay(250);
          }
          throw new Error(`Timed out waiting for ${label}: ${JSON.stringify(last)}`);
        };
        const upsert = async (collection, document) => {
          if (collection.incrementalUpsert) return collection.incrementalUpsert(document);
          if (collection.upsert) return collection.upsert(document);
          const existing = await collection.findOne(document.id).exec();
          if (existing?.incrementalPatch) return existing.incrementalPatch(document);
          if (existing?.patch) return existing.patch(document);
          return collection.insert(document);
        };
        const jsonDocs = async (collection) => (await collection.find().exec()).map((doc) => doc.toJSON?.() || doc);
        const css = (value) => {
          if (globalThis.CSS?.escape) return globalThis.CSS.escape(String(value));
          return String(value).replace(/"/g, '\\"');
        };
        const click = (selector, label) => {
          const element = document.querySelector(selector);
          if (!element) throw new Error(`${label} not found: ${selector}`);
          element.click();
          return true;
        };
        const screenshotPaths = [];
        const capture = async (name) => {
          const path = await globalThis.__ctoxCaptureSmokeScreenshot?.(name);
          if (path) screenshotPaths.push(path);
        };
        const outboundCollections = [
          'business_commands',
          'ctox_queue_tasks',
          'outbound_campaigns',
          'outbound_pipeline_items',
          'outbound_engagements',
          'outbound_messages',
          'outbound_approvals',
          'outbound_sender_assignments',
          'outbound_account_limits',
          'outbound_meeting_requests',
          'outbound_suppression_entries',
          'outbound_sequences',
          'outbound_skillbooks',
          'outbound_letter_templates',
        ].filter((collection) => rawDb[collection]);
        const bridges = [];
        for (const collection of outboundCollections) {
          const bridge = await state.sync.startCollection(collection);
          bridges.push(bridge?.state);
          await bounded(bridge?.state?.awaitInitialReplication?.(), 20000);
          await bounded(bridge?.state?.awaitInSync?.(), 20000);
        }
        const commandBridge = bridges[outboundCollections.indexOf('business_commands')];
        const queueBridge = bridges[outboundCollections.indexOf('ctox_queue_tasks')];
        if (commandBridge) await waitForNativePeerOpen(commandBridge, 'business_commands');
        if (queueBridge) await waitForNativePeerOpen(queueBridge, 'ctox_queue_tasks');
        const projectCommandResult = async (result) => {
          const projections = [
            ['outbound_engagements', result?.engagement],
            ['outbound_messages', result?.message],
            ['outbound_approvals', result?.approval],
            ['outbound_sender_assignments', result?.assignment],
            ['outbound_sequences', result?.sequence],
            ['outbound_meeting_requests', result?.meeting_request || result?.request],
          ];
          for (const [collectionName, record] of projections) {
            if (!record?.id || !rawDb[collectionName]) continue;
            await upsert(rawDb[collectionName], record);
          }
        };
        const dispatchNativeOutboundCommand = async (type, recordId, payload, label) => {
          const commandId = `cmd_${type.replaceAll('.', '_')}_${Date.now()}_${Math.random().toString(16).slice(2)}`;
          await rawDb.business_commands.insert({
            id: commandId,
            command_id: commandId,
            module: 'outbound',
            command_type: type,
            record_id: recordId || '',
            status: 'pending_sync',
            inbound_channel: 'outbound',
            payload,
            client_context: { source: 'outbound-active-ui-smoke' },
            updated_at_ms: Date.now(),
          });
          await bounded(commandBridge?.awaitInSync?.(), 25000);
          const command = await waitFor(async () => {
            const doc = (await rawDb.business_commands.findOne(commandId).exec())?.toJSON?.();
            return {
              ok: Boolean(doc && doc.status && doc.status !== 'pending_sync'),
              command: doc,
              status: doc?.status || '',
              error: doc?.error || '',
            };
          }, 60000, label || type);
          if (command.command?.status === 'failed') {
            throw new Error(`${label || type} failed: ${command.command.error || 'unknown error'}`);
          }
          await projectCommandResult(command.command?.result);
          return command.command;
        };

        if (!/#outbound(?:$|[?&])/.test(location.hash)) {
          location.hash = '#outbound';
        }
        await waitFor(async () => ({
          ok: Boolean(document.querySelector('[data-outbound-root]')),
          activeModule: state.activeModule?.id || '',
          body: document.body?.dataset?.activeModule || '',
        }), 60000, 'Outbound module root');

        const now = Date.now();
        const suffix = String(now).slice(-8);
        const campaignId = `outbound_active_ui_${suffix}`;
        const pipelineId = `outbound_active_ui_lead_${suffix}`;
        const senderAccount = 'email:outbound-smoke@example.com';
        const recipientEmail = `lead-${suffix}@example.com`;
        await upsert(rawDb.outbound_campaigns, {
          id: campaignId,
          name: 'Outbound Active UI Smoke',
          objective: 'Browser E2E approval-gated outbound communication',
          market: 'DACH',
          status: 'active',
          owner_id: 'browser-smoke',
          source_count: 1,
          company_count: 1,
          qualified_count: 1,
          pipeline_count: 1,
          communication_account_key: senderAccount,
          communication_account_address: 'outbound-smoke@example.com',
          payload: {
            subtitle: 'Browser smoke',
            scope: 'Approval gate and mailserver queue',
            communication_account_key: senderAccount,
            communication_account_address: 'outbound-smoke@example.com',
            active_outreach: {
              default_channel: 'email',
              strategy_text: 'Initial message, user approval, then provider queue.',
            },
          },
          created_at_ms: now,
          updated_at_ms: now,
        });
        await upsert(rawDb.outbound_account_limits, {
          id: senderAccount,
          sender_account_id: senderAccount,
          campaign_id: campaignId,
          daily_limit: 20,
          sent_today: 0,
          daily_sent_count: 0,
          status: 'ready',
          send_window: { timezone: 'Europe/Berlin', text: 'Mo-Fr 09:00-17:00' },
          payload: {},
          created_at_ms: now,
          updated_at_ms: now,
        });
        await upsert(rawDb.outbound_pipeline_items, {
          id: pipelineId,
          campaign_id: campaignId,
          company_id: `company_${suffix}`,
          company_name: 'Smoke GmbH',
          stage: 'lead_qualified',
          contact_research_status: 'qualified',
          outreach_status: 'qualified',
          priority: 'high',
          contacts: [{
            id: `contact_${suffix}`,
            name: 'Erika Smoke',
            email: recipientEmail,
          }],
          contact_id: `contact_${suffix}`,
          contact_name: 'Erika Smoke',
          contact_email: recipientEmail,
          lead_name: 'Erika Smoke',
          lead_status: 'qualified',
          payload: {
            company_id: `company_${suffix}`,
            contact_id: `contact_${suffix}`,
            contact_name: 'Erika Smoke',
            contact_email: recipientEmail,
            company_name: 'Smoke GmbH',
            lead_name: 'Erika Smoke',
          },
          created_at_ms: now,
          updated_at_ms: now,
        });

        await bounded(commandBridge?.awaitInSync?.(), 20000);
        await waitFor(async () => {
          const button = document.querySelector(`[data-action="select-campaign"][data-id="${css(campaignId)}"]`);
          return {
            ok: Boolean(button),
            campaignCards: document.querySelectorAll('.outbound-campaign-item').length,
            bodyText: document.body?.innerText?.slice(0, 300) || '',
          };
        }, 15000, 'seeded campaign in left pane');
        click(`[data-action="select-campaign"][data-id="${css(campaignId)}"]`, 'seeded campaign selector');
        await waitFor(async () => ({
          ok: Boolean(document.querySelector(`[data-action="import-source"][data-id="${css(campaignId)}"]`)),
          selected: document.querySelector('.outbound-campaign-item[aria-current="true"] strong')?.textContent?.trim() || '',
        }), 10000, 'seeded campaign selected');

        const outreachToggle = document.querySelector('[data-action="toggle-outreach"]');
        if (!outreachToggle) throw new Error('Active Outreach toggle not found');
        if (outreachToggle.getAttribute('aria-pressed') !== 'true') outreachToggle.click();
        await waitFor(async () => ({
          ok: Boolean(document.querySelector(`[data-action="ao-auto-draft"][data-lead-id="${css(pipelineId)}"]`)),
          text: document.querySelector('[data-outbound-root]')?.innerText?.slice(0, 500) || '',
        }), 15000, 'lead queue auto-draft action');
        await capture('outbound-active-01-lead-queue');

        click(`[data-action="ao-auto-draft"][data-lead-id="${css(pipelineId)}"]`, 'auto-draft button');
        const draftReady = await waitFor(async () => {
          const messages = await jsonDocs(rawDb.outbound_messages);
          const message = messages.find((doc) => doc.campaign_id === campaignId && doc.engagement_id);
          return {
            ok: Boolean(message && message.approval_status === 'awaiting_approval' && message.send_status === 'awaiting_approval'),
            message: message ? {
              id: message.id,
              approval_status: message.approval_status,
              send_status: message.send_status,
              subject: message.subject || '',
              channel: message.channel || '',
            } : null,
            count: messages.length,
          };
        }, 60000, 'approval-gated auto draft');
        const messageId = draftReady.message.id;
        await waitFor(async () => ({
          ok: Boolean(document.querySelector(`[data-action="ao-approve"][data-message-id="${css(messageId)}"]`)),
          text: document.querySelector('[data-outbound-root]')?.innerText?.slice(0, 700) || '',
        }), 15000, 'approval card rendered');
        await capture('outbound-active-02-approval-inbox');

        const preApprovalMessage = (await rawDb.outbound_messages.findOne(messageId).exec())?.toJSON?.();
        if (!preApprovalMessage || preApprovalMessage.send_status !== 'awaiting_approval') {
          throw new Error(`approval gate was not enforced before approval: ${JSON.stringify(preApprovalMessage)}`);
        }

        click(`[data-action="ao-approve"][data-message-id="${css(messageId)}"]`, 'approve message button');
        await waitFor(async () => {
          const message = (await rawDb.outbound_messages.findOne(messageId).exec())?.toJSON?.();
          return {
            ok: message?.approval_status === 'approved' && message?.send_status === 'approved_not_sent',
            approval_status: message?.approval_status || '',
            send_status: message?.send_status || '',
          };
        }, 60000, 'approved message ready to send');
        await waitFor(async () => ({
          ok: Boolean(document.querySelector(`[data-action="ao-send-approved"][data-message-id="${css(messageId)}"]`)),
          text: document.querySelector('[data-outbound-root]')?.innerText?.slice(0, 700) || '',
        }), 15000, 'ready-to-send action rendered');
        await capture('outbound-active-03-ready-to-send');

        click(`[data-action="ao-send-approved"][data-message-id="${css(messageId)}"]`, 'send approved button');
        const queued = await waitFor(async () => {
          const message = (await rawDb.outbound_messages.findOne(messageId).exec())?.toJSON?.();
          const commandDocs = (await jsonDocs(rawDb.business_commands))
            .filter((doc) => doc.module === 'outbound')
            .slice(-8)
            .map((doc) => ({
              id: doc.id,
              command_type: doc.command_type,
              record_id: doc.record_id,
              status: doc.status,
              error: doc.error || '',
            }));
          return {
            ok: message?.send_status === 'queued_for_provider' && Boolean(message?.provider_message_id || message?.payload?.provider_queue_id),
            send_status: message?.send_status || '',
            provider_message_id: message?.provider_message_id || message?.payload?.provider_queue_id || '',
            communication_message_key: message?.communication_message_key || message?.payload?.communication_message_key || '',
            errorBanner: document.querySelector('.outbound-outreach-error')?.textContent?.trim() || '',
            commands: commandDocs,
          };
        }, 60000, 'approved message queued in mailserver');
        click('[data-action="ao-view"][data-view="engagements"]', 'engagements tab');
        await waitFor(async () => ({
          ok: /Im Versand|scheduled_to_send|Versand/.test(document.querySelector('[data-outbound-root]')?.innerText || ''),
          text: document.querySelector('[data-outbound-root]')?.innerText?.slice(0, 900) || '',
        }), 15000, 'queued engagement visible');
        await capture('outbound-active-04-queued-engagement');

        const finalQueuedMessage = (await rawDb.outbound_messages.findOne(messageId).exec())?.toJSON?.();
        const engagementId = finalQueuedMessage?.engagement_id || '';
        const engagement = engagementId
          ? (await rawDb.outbound_engagements.findOne(engagementId).exec())?.toJSON?.()
          : (await jsonDocs(rawDb.outbound_engagements))
            .find((doc) => doc.campaign_id === campaignId && doc.payload?.pipeline_id === pipelineId);
        const approval = (await jsonDocs(rawDb.outbound_approvals))
          .find((doc) => doc.message_id === messageId);
        if (!engagement?.id) {
          throw new Error('queued outbound message did not produce an engagement');
        }
        const replyMessageKey = `email:${recipientEmail}:inbound:${suffix}`;
        const replyCommand = await dispatchNativeOutboundCommand('outbound.reply.match', engagement.id, {
          engagement_id: engagement.id,
          reply_message_id: replyMessageKey,
          outbound_message_id: messageId,
          classification: 'positive',
        }, 'positive reply matched to engagement');
        await waitFor(async () => {
          const doc = (await rawDb.outbound_engagements.findOne(engagement.id).exec())?.toJSON?.();
          return {
            ok: doc?.status === 'reply_received' && doc?.payload?.reply_classification === 'positive',
            status: doc?.status || '',
            classification: doc?.payload?.reply_classification || '',
          };
        }, 60000, 'positive reply visible in outbound engagement');
        click('[data-action="ao-view"][data-view="replies"]', 'replies tab');
        await waitFor(async () => ({
          ok: Boolean(document.querySelector(`[data-action="ao-draft-scheduling"][data-id="${css(engagement.id)}"]`)),
          text: document.querySelector('[data-outbound-root]')?.innerText?.slice(0, 900) || '',
        }), 15000, 'positive reply scheduling action rendered');
        await capture('outbound-active-05-positive-reply');

        click(`[data-action="ao-draft-scheduling"][data-id="${css(engagement.id)}"]`, 'prepare scheduling draft button');
        const schedulingReady = await waitFor(async () => {
          const messages = await jsonDocs(rawDb.outbound_messages);
          const message = messages
            .filter((doc) => doc.engagement_id === engagement.id && doc.message_type === 'scheduling')
            .sort((a, b) => (b.created_at_ms || 0) - (a.created_at_ms || 0))[0];
          const slots = Array.isArray(message?.payload?.proposed_slots) ? message.payload.proposed_slots : [];
          return {
            ok: Boolean(message && message.approval_status === 'awaiting_approval' && slots.length >= 1 && message.payload?.meeting_request_id),
            message: message ? {
              id: message.id,
              approval_status: message.approval_status,
              send_status: message.send_status,
              meeting_request_id: message.payload?.meeting_request_id || '',
              slots: slots.length,
            } : null,
            count: messages.length,
          };
        }, 60000, 'scheduling draft with proposed slots');
        const schedulingMessageId = schedulingReady.message.id;
        const meetingRequestId = schedulingReady.message.meeting_request_id;
        await waitFor(async () => ({
          ok: Boolean(document.querySelector(`[data-action="ao-book-slot"][data-meeting-request-id="${css(meetingRequestId)}"]`))
            && Boolean(document.querySelector(`[data-action="ao-approve"][data-message-id="${css(schedulingMessageId)}"]`)),
          text: document.querySelector('[data-outbound-root]')?.innerText?.slice(0, 1000) || '',
        }), 15000, 'scheduling approval card with bookable slot');
        await capture('outbound-active-06-scheduling-draft');

        const originalPrompt = window.prompt;
        window.prompt = () => 'https://meet.example.com/outbound-smoke';
        let booked;
        try {
          click(`[data-action="ao-book-slot"][data-meeting-request-id="${css(meetingRequestId)}"]`, 'book proposed slot button');
          booked = await waitFor(async () => {
            const request = (await rawDb.outbound_meeting_requests.findOne(meetingRequestId).exec())?.toJSON?.();
            const updatedEngagement = (await rawDb.outbound_engagements.findOne(engagement.id).exec())?.toJSON?.();
            return {
              ok: request?.status === 'booked' && updatedEngagement?.status === 'meeting_booked',
              request_status: request?.status || '',
              meeting_url: request?.meeting_url || '',
              engagement_status: updatedEngagement?.status || '',
            };
          }, 60000, 'meeting request booked from proposed slot');
        } finally {
          window.prompt = originalPrompt;
        }
        click('[data-action="ao-view"][data-view="done"]', 'done tab');
        await waitFor(async () => ({
          ok: /Termin gebucht|meeting_booked/.test(document.querySelector('[data-outbound-root]')?.innerText || ''),
          text: document.querySelector('[data-outbound-root]')?.innerText?.slice(0, 900) || '',
        }), 15000, 'booked engagement visible as done');
        await capture('outbound-active-07-meeting-booked');
        const conversationLink = document.querySelector('.outbound-outreach-conv-link')?.getAttribute('href') || '';
        return {
          mode: smokeMode,
          campaignId,
          pipelineId,
          engagementId: engagement?.id || '',
          messageId,
          schedulingMessageId,
          meetingRequestId,
          approvalId: approval?.id || '',
          approvalGateVerified: true,
          finalSendStatus: queued.send_status,
          providerMessageId: queued.provider_message_id,
          communicationMessageKey: queued.communication_message_key,
          replyMessageKey,
          replyClassification: replyCommand?.result?.classification || '',
          meetingStatus: booked.request_status,
          meetingUrl: booked.meeting_url,
          conversationLink,
          screenshotPaths,
          advancedStatusVersion,
          advancedStatusRuntime,
        };
      }

      if (smokeMode === 'outbound-active-ui') {
        return await runOutboundActiveUiSmoke();
      }

      if (smokeMode === 'business-os-ui-regression') {
        return await runBusinessOsUiRegression();
      }

      if (ticketSmokeMode) {
        const now = Date.now();
        const id = `ticket_command_smoke_${now}`;
        const title = `WebRTC ticket smoke ${now}`;
        await db.business_commands.insert({
          id,
          command_id: id,
          module: 'tickets',
          command_type: 'ctox.ticket.local.create',
          record_id: '',
          status: 'pending_sync',
          inbound_channel: 'tickets',
          payload: {
            title,
            body: 'created by Business OS RxDB/WebRTC smoke',
            priority: 'medium',
            labels: ['smoke'],
          },
          client_context: { source: 'rxdb-ticket-smoke' },
          updated_at_ms: now,
        });
        await bounded(appCommandReplicationState?.awaitInSync?.(), 25000);
        await bounded(appTicketItemReplicationState?.awaitInSync?.(), 25000);
        await bounded(appTicketEventReplicationState?.awaitInSync?.(), 25000);
        const deadline = Date.now() + 60000;
        while (Date.now() < deadline) {
          const commandDoc = await db.business_commands.findOne(id).exec();
          const command = commandDoc?.toJSON?.();
          const ticketDocs = (await db.ctox_ticket_items.find().exec()).map((doc) => doc.toJSON?.() || doc);
          const ticket = ticketDocs.find((doc) => doc.title === title && doc.source_system === 'local');
          if (command && command.status === 'completed' && ticket) {
            if (ticketClarificationSmokeMode) {
              const clarificationNow = Date.now();
              const clarificationCommandId = `ticket_clarification_command_smoke_${clarificationNow}`;
              const question = `Welche Kundennummer gehoert zu ${ticket.ticket_key}?`;
              await db.business_commands.insert({
                id: clarificationCommandId,
                command_id: clarificationCommandId,
                module: 'tickets',
                command_type: 'ctox.ticket.request_clarification',
                record_id: ticket.ticket_key || '',
                status: 'pending_sync',
                inbound_channel: 'tickets',
                payload: {
                  ticket_key: ticket.ticket_key,
                  target_type: 'requester',
                  target_channel: 'ticket',
                  question,
                  missing_inputs: ['customer_id', 'approval_context'],
                  unblock_criteria: 'Requester supplies customer_id and approval_context.',
                },
                client_context: { source: 'rxdb-ticket-clarification-smoke' },
                updated_at_ms: clarificationNow,
              });
              await bounded(appCommandReplicationState?.awaitInSync?.(), 25000);
              await bounded(appTicketClarificationReplicationState?.awaitInSync?.(), 25000);
              const clarificationDeadline = Date.now() + 60000;
              while (Date.now() < clarificationDeadline) {
                const clarificationCommandDoc = await db.business_commands.findOne(clarificationCommandId).exec();
                const clarificationCommand = clarificationCommandDoc?.toJSON?.();
                const clarificationDocs = (await db.ctox_ticket_clarification_requests.find().exec()).map((doc) => doc.toJSON?.() || doc);
                const clarification = clarificationDocs.find((doc) => doc.ticket_key === ticket.ticket_key && doc.question === question);
                if (clarificationCommand?.status === 'completed' && clarification?.status) {
                  if (clarification.status === 'waiting_for_response') {
                    await Promise.all(replicationStates.map((state) => state.cancel?.()));
                    if (ownsDb) await db.close();
                    return {
                      mode: smokeMode,
                      createCommandId: id,
                      clarificationCommandId,
                      createStatus: command.status,
                      clarificationStatus: clarificationCommand.status,
                      publishStatus: 'already_waiting',
                      ticketKey: ticket.ticket_key || '',
                      clarificationId: clarification.clarification_id || '',
                      clarificationRequestStatus: clarification.status || '',
                      missingInputCount: Array.isArray(clarification.missing_inputs)
                        ? clarification.missing_inputs.length
                        : 0,
                    };
                  }
                  if (clarification.status === 'draft') {
                    const publishNow = Date.now();
                    const publishCommandId = `ticket_clarification_publish_smoke_${publishNow}`;
                    await db.business_commands.insert({
                      id: publishCommandId,
                      command_id: publishCommandId,
                      module: 'tickets',
                      command_type: 'ctox.ticket.publish_clarification',
                      record_id: clarification.clarification_id || '',
                      status: 'pending_sync',
                      inbound_channel: 'tickets',
                      payload: {
                        clarification_id: clarification.clarification_id,
                        reviewed_by: 'rxdb-ticket-clarification-smoke',
                        review_summary: 'Clarification question reviewed by browser smoke.',
                      },
                      client_context: { source: 'rxdb-ticket-clarification-smoke' },
                      updated_at_ms: publishNow,
                    });
                    await bounded(appCommandReplicationState?.awaitInSync?.(), 25000);
                    await bounded(appTicketClarificationReplicationState?.awaitInSync?.(), 25000);
                    const publishDeadline = Date.now() + 60000;
                    while (Date.now() < publishDeadline) {
                      await bounded(appCommandReplicationState?.awaitInSync?.(), 5000);
                      await bounded(appTicketClarificationReplicationState?.awaitInSync?.(), 5000);
                      const publishCommandDoc = await db.business_commands.findOne(publishCommandId).exec();
                      const publishCommand = publishCommandDoc?.toJSON?.();
                      let latestClarification = (await db.ctox_ticket_clarification_requests.findOne(clarification.clarification_id).exec())?.toJSON?.();
                      if (
                        publishCommand?.status === 'completed'
                        && latestClarification?.status !== 'waiting_for_response'
                        && globalThis.ctoxBusinessOsSmoke?.state?.sync?.restartCollection
                      ) {
                        const refreshedBridge = await globalThis.ctoxBusinessOsSmoke.state.sync.restartCollection('ctox_ticket_clarification_requests');
                        appTicketClarificationReplicationState = refreshedBridge?.state || appTicketClarificationReplicationState;
                        await bounded(appTicketClarificationReplicationState?.awaitInitialReplication?.(), 10000);
                        await bounded(appTicketClarificationReplicationState?.awaitInSync?.(), 10000);
                        latestClarification = (await db.ctox_ticket_clarification_requests.findOne(clarification.clarification_id).exec())?.toJSON?.();
                      }
                      const publishedClarification = publishCommand?.result?.clarification || latestClarification || null;
                      if (publishCommand?.status === 'completed' && publishedClarification?.status === 'waiting_for_response') {
                        await Promise.all(replicationStates.map((state) => state.cancel?.()));
                        if (ownsDb) await db.close();
                        return {
                          mode: smokeMode,
                          createCommandId: id,
                          clarificationCommandId,
                          publishCommandId,
                          createStatus: command.status,
                          clarificationStatus: clarificationCommand.status,
                          publishStatus: publishCommand.status,
                          ticketKey: ticket.ticket_key || '',
                          clarificationId: publishedClarification.clarification_id || '',
                          clarificationRequestStatus: publishedClarification.status || '',
                          projectionStatus: latestClarification?.status || '',
                          outboundMessageKey: publishedClarification.outbound_message_key || '',
                          missingInputCount: Array.isArray(publishedClarification.missing_inputs)
                            ? publishedClarification.missing_inputs.length
                            : 0,
                        };
                      }
                      await delay(500);
                    }
                    const publishCommandDoc = await db.business_commands.findOne(publishCommandId).exec();
                    const latestClarification = (await db.ctox_ticket_clarification_requests.findOne(clarification.clarification_id).exec())?.toJSON?.();
                    throw new Error(`ticket clarification publish command ${publishCommandId} was not completed via RxDB/WebRTC: ${JSON.stringify({
                      command: publishCommandDoc?.toJSON?.() || null,
                      clarification: latestClarification || null,
                      syncMode: globalThis.ctoxBusinessOsSmoke?.state?.sync?.mode || '',
                    })}`);
                  }
                }
                await delay(500);
              }
              const clarificationCommandDoc = await db.business_commands.findOne(clarificationCommandId).exec();
              const clarificationDocs = (await db.ctox_ticket_clarification_requests.find({ limit: 10 }).exec()).map((doc) => doc.toJSON?.() || doc);
              throw new Error(`ticket clarification command ${clarificationCommandId} was not completed via RxDB/WebRTC: ${JSON.stringify({
                command: clarificationCommandDoc?.toJSON?.() || null,
                clarificationCount: clarificationDocs.length,
                clarifications: clarificationDocs.map((doc) => ({
                  clarification_id: doc.clarification_id || '',
                  ticket_key: doc.ticket_key || '',
                  status: doc.status || '',
                  question: doc.question || '',
                })),
                syncMode: globalThis.ctoxBusinessOsSmoke?.state?.sync?.mode || '',
              })}`);
            }
            await Promise.all(replicationStates.map((state) => state.cancel?.()));
            if (ownsDb) await db.close();
            return {
              mode: smokeMode,
              id,
              status: command.status,
              taskId: command.task_id || '',
              ticketKey: ticket.ticket_key || '',
              ticketSource: ticket.source_system || '',
              ticketTitle: ticket.title || '',
            };
          }
          await delay(500);
        }
        const commandDoc = await db.business_commands.findOne(id).exec();
        const ticketDocs = (await db.ctox_ticket_items.find({ limit: 10 }).exec()).map((doc) => doc.toJSON?.() || doc);
        throw new Error(`ticket command ${id} was not completed via RxDB/WebRTC: ${JSON.stringify({
          command: commandDoc?.toJSON?.() || null,
          ticketCount: ticketDocs.length,
          tickets: ticketDocs.map((doc) => ({
            title: doc.title || '',
            ticket_key: doc.ticket_key || '',
            source_system: doc.source_system || '',
          })),
          syncMode: globalThis.ctoxBusinessOsSmoke?.state?.sync?.mode || '',
          syncConfig: globalThis.ctoxBusinessOsSmoke?.state?.sync?.config || null,
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
          if (useAppDb) {
            const repairedState = await repairAppFileAndCommandReplicationAfterNativeRestart();
            if (!repairedState?.db?.raw?.business_commands || !repairedState?.db?.raw?.ctox_queue_tasks) {
              throw new Error('Business OS command collections were not available after native peer restart');
            }
            db = repairedState.db.raw;
          } else {
            await globalThis.__ctoxRestartNativePeer?.();
          }
          await bounded(appCommandReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appQueueReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appCommandReplicationState?.awaitInSync?.(), 30000);
          await bounded(appQueueReplicationState?.awaitInSync?.(), 30000);
          await waitForNativePeerOpen(appCommandReplicationState, 'business_commands');
          await waitForNativePeerOpen(appQueueReplicationState, 'ctox_queue_tasks');
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
          await waitForNativePeerOpen(appCommandReplicationState, 'business_commands');
          await waitForNativePeerOpen(appQueueReplicationState, 'ctox_queue_tasks');
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

      let hostFileChunkMutation = null;
      const workspacePhaseTimings = {};
      const received = smokeMode === 'restart-browser-to-rust'
        || smokeMode === 'restart-signaling-browser-to-rust'
        || smokeMode === 'rollover-native-peer-browser-to-rust'
        ? { payload: rustSeed.content, file: {} }
        : await (async () => {
            if (smokeMode === 'file-chunk-tombstone-error-browser-status') {
              await globalThis.__ctoxSyncRustSeedFile?.();
              hostFileChunkMutation = await globalThis.__ctoxTombstoneRustSeedChunk?.();
              return { payload: '', file: { id: rustSeed.id, content_state: 'available' } };
            }
            if (smokeMode !== 'workspace-agent-artifacts-background-rust-to-browser') {
              await globalThis.__ctoxSyncRustSeedFile?.();
            }
            if (smokeMode === 'workspace-agent-artifacts-rust-to-browser'
              || smokeMode === 'workspace-agent-artifacts-stress-rust-to-browser'
              || smokeMode === 'workspace-agent-artifacts-churn-rust-to-browser'
              || smokeMode === 'workspace-agent-artifacts-background-rust-to-browser') {
              const waitStartedAt = Date.now();
              const artifacts = await waitForWorkspaceArtifacts(
                rustSeed.files || [],
                smokeMode === 'workspace-agent-artifacts-background-rust-to-browser' ? 90000 : 60000,
              );
              if (smokeMode === 'workspace-agent-artifacts-background-rust-to-browser') {
                workspacePhaseTimings.waitBackgroundArtifactsMs = Date.now() - waitStartedAt;
              }
              artifacts.backgroundQueueTask = backgroundQueueTask;
              artifacts.phaseTimings = workspacePhaseTimings;
              return artifacts;
            }
            if (smokeMode === 'workspace-large-materialize-rust-to-browser'
              || smokeMode === 'workspace-large-file-viewer-rust-to-browser'
              || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser') {
              const initialMetadataStartedAt = Date.now();
              try {
                return await waitForFileMetadata(rustSeed.id, 60000);
              } finally {
                setupPhaseTimings.initialMetadataMs = Date.now() - initialMetadataStartedAt;
              }
            }
            return waitForFile(rustSeed.id);
          })();
      if (smokeMode === 'workspace-agent-artifacts-churn-rust-to-browser') {
        const phaseTimings = {};
        const mark = async (name, action) => {
          const started = Date.now();
          try {
            return await action();
          } finally {
            phaseTimings[name] = Date.now() - started;
          }
        };
        const initialByRelativePath = new Map(received.map((file) => [file.relativePath, file]));
        const mutation = await mark('mutateAndSyncMs', () => globalThis.__ctoxMutateRustWorkspaceArtifacts?.());
        const updatedRelativePaths = Array.isArray(mutation?.updatedRelativePaths) ? mutation.updatedRelativePaths : [];
        const addedRelativePaths = Array.isArray(mutation?.addedRelativePaths) ? mutation.addedRelativePaths : [];
        const changed = await mark('waitChangedArtifactsMs', () => waitForWorkspaceArtifacts(mutation?.files || rustSeed.files || [], 90000));
        const changedByRelativePath = new Map(changed.map((file) => [file.relativePath, file]));
        const staleGenerations = [];
        for (const relativePath of updatedRelativePaths) {
          const before = initialByRelativePath.get(relativePath);
          const after = changedByRelativePath.get(relativePath);
          if (!before || !after || before.generationId === after.generationId) {
            staleGenerations.push({
              relativePath,
              before: before?.generationId || '',
              after: after?.generationId || '',
            });
          }
        }
        if (staleGenerations.length) {
          throw new Error(`workspace churn did not advance updated file generations: ${JSON.stringify(staleGenerations)}`);
        }
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          files: changed.map((file) => ({
            id: file.file?.id || '',
            payloadLength: file.payload.length,
            chunkCount: file.chunks.length,
            generationId: file.generationId || '',
            virtualPath: file.file?.virtual_path || file.file?.path || '',
            relativePath: file.relativePath || '',
          })),
          updatedRelativePaths,
          addedRelativePaths,
          updatedGenerationChanges: updatedRelativePaths.length,
          addedCount: addedRelativePaths.length,
          advancedStatusVersion,
          advancedStatusRuntime,
          phaseTimings,
        };
      }
      if (smokeMode === 'workspace-agent-artifacts-background-rust-to-browser') {
        const phaseTimings = received.phaseTimings || {};
        const advancedStatusStartedAt = Date.now();
        const advancedStatus = await globalThis.CTOX_BUSINESS_OS_STATUS?.waitForHealthy?.({
          timeoutMs: 60000,
          requiredCollections: [
            'business_module_catalog',
            'ctox_runtime_settings',
            'desktop_files',
            'desktop_file_chunks',
          ],
        });
        phaseTimings.advancedStatusMs = Date.now() - advancedStatusStartedAt;
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          backgroundQueueTask: received.backgroundQueueTask || null,
          advancedStatus,
          files: received.map((file) => ({
            id: file.file?.id || '',
            payloadLength: file.payload.length,
            chunkCount: file.chunks.length,
            generationId: file.generationId || '',
            virtualPath: file.file?.virtual_path || file.file?.path || '',
            relativePath: file.relativePath || '',
          })),
          phaseTimings,
        };
      }
      if (smokeMode === 'workspace-agent-artifacts-rust-to-browser'
        || smokeMode === 'workspace-agent-artifacts-stress-rust-to-browser') {
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          files: received.map((file) => ({
            id: file.file?.id || '',
            payloadLength: file.payload.length,
            chunkCount: file.chunks.length,
            generationId: file.generationId || '',
            virtualPath: file.file?.virtual_path || file.file?.path || '',
            relativePath: file.relativePath || '',
          })),
          advancedStatusVersion,
          advancedStatusRuntime,
        };
      }
      if ((smokeMode === 'workspace-rust-to-browser'
        || smokeMode === 'workspace-update-rust-to-browser'
        || smokeMode === 'workspace-large-materialize-rust-to-browser'
        || smokeMode === 'workspace-large-file-viewer-rust-to-browser'
        || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser'
        || smokeMode === 'file-chunk-metadata-error-browser-status'
        || smokeMode === 'file-chunk-stale-generation-error-browser-status')
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
      if (smokeMode === 'file-chunk-stale-generation-error-browser-status') {
        const hostCorruption = await globalThis.__ctoxStaleRustSeedChunkGeneration?.();
        await bounded(appFileReplicationState?.awaitInSync?.(), 30000);
        await bounded(appChunkReplicationState?.awaitInSync?.(), 30000);
        const stale = await waitForStaleGenerationState(rustSeed.id, hostCorruption?.requestedGenerationId || '', 60000);
        const mount = document.createElement('section');
        mount.setAttribute('data-file-viewer-smoke', 'file-chunk-stale-generation-error');
        mount.style.cssText = 'position:fixed;left:0;top:0;width:760px;height:560px;z-index:99999;background:#0b1117;';
        document.body.append(mount);
        const viewer = await import(`/desktop-apps/file-viewer/app.js?v=file-viewer-stale-generation-smoke-${Date.now()}`);
        const smokeState = globalThis.ctoxBusinessOsSmoke?.state;
        const teardown = await viewer.mount(mount, {
          db: smokeState?.db || db,
          sync: smokeState?.sync,
          commandBus: smokeState?.commandBus,
          session: smokeState?.session,
          reportFileIntegrityError: (error, details = {}) => globalThis.ctoxBusinessOsSmoke?.reportFileIntegrityError?.('desktop-app:file-viewer', error, {
            appId: 'file-viewer',
            ...details,
          }),
          setTitle: () => {},
          args: {
            fileId: rustSeed.id,
            name: stale.file?.name || received.file?.name || 'brief.md',
            mimeType: stale.file?.mime_type || received.file?.mime_type || 'text/markdown',
            sizeBytes: stale.file?.size_bytes || received.file?.size_bytes || rustSeed.content.length,
            path: stale.file?.local_path || stale.file?.path || '',
            contentState: stale.file?.content_state || 'available',
            contentGenerationId: stale.file?.content_generation_id || '',
            contentHash: stale.file?.content_hash || received.file?.content_hash || '',
            contentHashScheme: stale.file?.content_hash_scheme || received.file?.content_hash_scheme || '',
          },
        });
        const deadline = Date.now() + 30000;
        let errorText = '';
        while (Date.now() < deadline) {
          errorText = mount.querySelector('.is-error')?.textContent || '';
          if (errorText.includes('Dateiinhalt fehlt.')) break;
          await delay(250);
        }
        try { teardown?.(); } catch {}
        mount.remove();
        if (!errorText.includes('Dateiinhalt fehlt.')) {
          throw new Error(`file viewer did not reject stale file chunk generation: ${JSON.stringify({
            errorText,
            hostCorruption,
            stale: {
              file: stale.file,
              liveChunkCount: stale.liveChunks.length,
              requestedGenerationChunkCount: stale.requestedGenerationChunks.length,
              availableGenerationIds: stale.availableGenerationIds,
            },
          })}`);
        }
        const status = await waitForFileIntegrityStatus('ctox_file_chunk_missing', 30000);
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          id: rustSeed.id,
          fileIntegrityName: status.error.name,
          fileIntegrityCode: status.error.code,
          fileIntegrityPhase: status.error.phase || '',
          fileIntegritySource: status.error.source || '',
          advancedStatusVersion,
          advancedStatusRuntime,
          requestedGenerationId: stale.file?.content_generation_id || hostCorruption?.requestedGenerationId || '',
          requestedGenerationChunkCount: stale.requestedGenerationChunks.length,
          liveChunkCount: stale.liveChunks.length,
          availableGenerationIds: stale.availableGenerationIds,
        };
      }
      if (smokeMode === 'file-chunk-tombstone-error-browser-status') {
        let hostCorruption = hostFileChunkMutation || await globalThis.__ctoxTombstoneRustSeedChunk?.();
        if (!appFileReplicationState || !appChunkReplicationState) {
          await startDeferredAppFileCollections('file-chunk-tombstone-error');
        }
        let tombstoned = null;
        let tombstoneWaitError = null;
        for (let attempt = 0; attempt < 3; attempt += 1) {
          if (attempt > 0) {
            hostCorruption = await globalThis.__ctoxTombstoneRustSeedChunk?.();
            if (typeof appFileReplicationState?.reSync === 'function') appFileReplicationState.reSync();
            if (typeof appChunkReplicationState?.reSync === 'function') appChunkReplicationState.reSync();
          }
          await tombstoneLocalFileCache(rustSeed.id);
          await bounded(appFileReplicationState?.awaitInSync?.(), 30000);
          await bounded(appChunkReplicationState?.awaitInSync?.(), 30000);
          try {
            tombstoned = await waitForTombstonedChunkState(rustSeed.id, attempt === 0 ? 15000 : 30000);
            tombstoneWaitError = null;
            break;
          } catch (error) {
            tombstoneWaitError = error;
          }
        }
        if (!tombstoned) throw tombstoneWaitError || new Error('browser did not receive tombstoned chunk state');
        const mount = document.createElement('section');
        mount.setAttribute('data-file-viewer-smoke', 'file-chunk-tombstone-error');
        mount.style.cssText = 'position:fixed;left:0;top:0;width:760px;height:560px;z-index:99999;background:#0b1117;';
        document.body.append(mount);
        const viewer = await import(`/desktop-apps/file-viewer/app.js?v=file-viewer-tombstone-smoke-${Date.now()}`);
        const smokeState = globalThis.ctoxBusinessOsSmoke?.state;
        const teardown = await viewer.mount(mount, {
          db: smokeState?.db || db,
          sync: smokeState?.sync,
          commandBus: smokeState?.commandBus,
          session: smokeState?.session,
          reportFileIntegrityError: (error, details = {}) => globalThis.ctoxBusinessOsSmoke?.reportFileIntegrityError?.('desktop-app:file-viewer', error, {
            appId: 'file-viewer',
            ...details,
          }),
          setTitle: () => {},
          args: {
            fileId: rustSeed.id,
            name: tombstoned.file?.name || received.file?.name || 'brief.md',
            mimeType: tombstoned.file?.mime_type || received.file?.mime_type || 'text/markdown',
            sizeBytes: tombstoned.file?.size_bytes || received.file?.size_bytes || rustSeed.content.length,
            path: tombstoned.file?.local_path || tombstoned.file?.path || '',
            contentState: tombstoned.file?.content_state || 'available',
            contentGenerationId: tombstoned.file?.content_generation_id || received.file?.content_generation_id || '',
            contentHash: tombstoned.file?.content_hash || received.file?.content_hash || '',
            contentHashScheme: tombstoned.file?.content_hash_scheme || received.file?.content_hash_scheme || '',
          },
        });
        const deadline = Date.now() + 30000;
        let errorText = '';
        while (Date.now() < deadline) {
          errorText = mount.querySelector('.is-error')?.textContent || '';
          if (errorText.includes('Dateiinhalt fehlt.')) break;
          await delay(250);
        }
        try { teardown?.(); } catch {}
        mount.remove();
        if (!errorText.includes('Dateiinhalt fehlt.')) {
          throw new Error(`file viewer did not reject tombstoned active chunk: ${JSON.stringify({
            errorText,
            hostCorruption,
            tombstoned: {
              file: tombstoned.file,
              chunks: tombstoned.chunks,
              liveChunkCount: tombstoned.liveChunks.length,
              tombstonedChunkCount: tombstoned.tombstonedChunks.length,
            },
          })}`);
        }
        const status = await waitForFileIntegrityStatus('ctox_file_chunk_missing', 30000);
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          id: rustSeed.id,
          fileIntegrityName: status.error.name,
          fileIntegrityCode: status.error.code,
          fileIntegrityPhase: status.error.phase || '',
          fileIntegritySource: status.error.source || '',
          advancedStatusVersion,
          advancedStatusRuntime,
          liveChunkCount: tombstoned.liveChunks.length,
          tombstonedChunkCount: tombstoned.tombstonedChunks.length,
          chunkId: tombstoned.tombstonedChunks[0]?.id || hostCorruption?.chunkRowId || '',
        };
      }
      if (smokeMode === 'file-chunk-metadata-error-browser-status') {
        const hostCorruption = await globalThis.__ctoxCorruptRustSeedChunkMetadata?.();
        await bounded(appChunkReplicationState?.awaitInSync?.(), 30000);
        const corrupted = await waitForCorruptChunkMetadata(rustSeed.id, 60000);
        const mount = document.createElement('section');
        mount.setAttribute('data-file-viewer-smoke', 'file-chunk-metadata-error');
        mount.style.cssText = 'position:fixed;left:0;top:0;width:760px;height:560px;z-index:99999;background:#0b1117;';
        document.body.append(mount);
        const viewer = await import(`/desktop-apps/file-viewer/app.js?v=file-viewer-integrity-smoke-${Date.now()}`);
        const smokeState = globalThis.ctoxBusinessOsSmoke?.state;
        const teardown = await viewer.mount(mount, {
          db: smokeState?.db || db,
          sync: smokeState?.sync,
          commandBus: smokeState?.commandBus,
          session: smokeState?.session,
          reportFileIntegrityError: (error, details = {}) => globalThis.ctoxBusinessOsSmoke?.reportFileIntegrityError?.('desktop-app:file-viewer', error, {
            appId: 'file-viewer',
            ...details,
          }),
          setTitle: () => {},
          args: {
            fileId: rustSeed.id,
            name: received.file?.name || 'brief.md',
            mimeType: received.file?.mime_type || 'text/markdown',
            sizeBytes: received.file?.size_bytes || rustSeed.content.length,
            path: received.file?.local_path || received.file?.path || rustSeed.path,
            contentState: received.file?.content_state || '',
            contentGenerationId: received.file?.content_generation_id || '',
            contentHash: received.file?.content_hash || '',
            contentHashScheme: received.file?.content_hash_scheme || '',
          },
        });
        const deadline = Date.now() + 30000;
        let errorText = '';
        while (Date.now() < deadline) {
          errorText = mount.querySelector('.is-error')?.textContent || '';
          if (errorText.includes('Dateiinhalt ist unvollständig oder beschädigt.')) break;
          await delay(250);
        }
        try { teardown?.(); } catch {}
        mount.remove();
        if (!errorText.includes('Dateiinhalt ist unvollständig oder beschädigt.')) {
          throw new Error(`file viewer did not reject corrupt chunk metadata: ${JSON.stringify({
            errorText,
            hostCorruption,
            corrupted,
          })}`);
        }
        const status = await waitForFileIntegrityStatus('ctox_file_chunk_integrity_mismatch', 30000);
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          id: rustSeed.id,
          fileIntegrityName: status.error.name,
          fileIntegrityCode: status.error.code,
          fileIntegrityPhase: status.error.phase || '',
          fileIntegritySource: status.error.source || '',
          advancedStatusVersion,
          advancedStatusRuntime,
          chunkId: corrupted.chunk?.id || hostCorruption?.rowId || '',
          expectedSizeBytes: corrupted.expectedSizeBytes,
          actualSizeBytes: corrupted.actualSizeBytes,
        };
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
          phaseTimings: setupPhaseTimings,
        };
      }
      if (smokeMode === 'workspace-large-file-viewer-rust-to-browser'
        || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser') {
        const phaseTimings = { ...setupPhaseTimings };
        const mark = () => performance.now();
        if (received.file?.content_state !== 'lazy') {
          throw new Error(`large workspace file was not indexed lazily for file viewer: ${JSON.stringify(received.file)}`);
        }
        if (received.chunks.length !== 0) {
          throw new Error(`large workspace file wrote eager chunks before file viewer materialize: ${received.chunks.length}`);
        }
        if (smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser') {
          const restartStartedAt = mark();
          await repairAppFileAndCommandReplicationAfterNativeRestart();
          phaseTimings.restartMs = Math.round(mark() - restartStartedAt);
          const waitCollectionsStartedAt = mark();
          await waitForAppSyncCollections([
            'business_commands',
            'desktop_files',
            'desktop_file_chunks',
          ]);
          phaseTimings.waitCollectionsMs = Math.round(mark() - waitCollectionsStartedAt);
        }
        const mount = document.createElement('section');
        mount.setAttribute('data-file-viewer-smoke', 'true');
        mount.style.cssText = 'position:fixed;left:0;top:0;width:760px;height:560px;z-index:99999;background:#0b1117;';
        document.body.append(mount);
        const importStartedAt = mark();
        const viewer = await import(`/desktop-apps/file-viewer/app.js?v=file-viewer-smoke-${Date.now()}`);
        phaseTimings.importViewerMs = Math.round(mark() - importStartedAt);
        const smokeState = globalThis.ctoxBusinessOsSmoke?.state;
        const mountStartedAt = mark();
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
        phaseTimings.mountViewerMs = Math.round(mark() - mountStartedAt);
        const deadline = Date.now() + 120000;
        const renderStartedAt = mark();
        let text = '';
        while (Date.now() < deadline) {
          const pre = mount.querySelector('[data-file-text]');
          text = pre?.textContent || '';
          if (text === rustSeed.content) break;
          const errorText = mount.querySelector('.is-error')?.textContent || '';
          if (errorText) {
            throw new Error(`file viewer failed to materialize large file: ${JSON.stringify({
              errorText,
              diagnostics: await fileViewerMaterializeDiagnostics(rustSeed.id),
            }, null, 2)}`);
          }
          await delay(500);
        }
        phaseTimings.renderPayloadMs = Math.round(mark() - renderStartedAt);
        try { teardown?.(); } catch {}
        mount.remove();
        if (text !== rustSeed.content) {
          throw new Error(`file viewer did not render materialized payload: ${JSON.stringify({
            expectedLength: rustSeed.content.length,
            actualLength: text.length,
            prefix: text.slice(0, 80),
          })}`);
        }
        const waitForFileStartedAt = mark();
        const materialized = await waitForFile(rustSeed.id, 30000, rustSeed.content);
        phaseTimings.waitForFileMs = Math.round(mark() - waitForFileStartedAt);
        const desktopViewerStartedAt = mark();
        const desktopViewer = await openDesktopFileViewerAndWait({
          fileId: rustSeed.id,
          name: rustSeed.name || 'brief.md',
          mimeType: materialized.file?.mime_type || received.file?.mime_type || 'text/markdown',
          sizeBytes: materialized.file?.size_bytes || received.file?.size_bytes || rustSeed.content.length,
          path: materialized.file?.local_path || materialized.file?.path || received.file?.local_path || received.file?.path || rustSeed.path,
          contentState: materialized.file?.content_state || 'available',
          contentGenerationId: materialized.file?.content_generation_id || materialized.generationId || '',
          contentHash: materialized.file?.content_hash || '',
          contentHashScheme: materialized.file?.content_hash_scheme || '',
        }, rustSeed.content);
        phaseTimings.desktopViewerRenderMs = Math.round(mark() - desktopViewerStartedAt);
        const advancedStatusStartedAt = mark();
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
        phaseTimings.advancedStatusMs = Math.round(mark() - advancedStatusStartedAt);
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
          desktopViewer,
          advancedStatus,
          phaseTimings,
        };
      }

      async function fileViewerMaterializeDiagnostics(fileId) {
        const docsToJson = (docs) => (docs || []).map((doc) => doc?.toJSON?.() || doc);
        const fileDoc = await db.desktop_files?.findOne(fileId).exec();
        const file = fileDoc?.toJSON?.() || null;
        const chunks = docsToJson(await db.desktop_file_chunks?.find().exec())
          .filter((chunk) => chunk?.file_id === fileId);
        const commands = docsToJson(await db.business_commands?.find().exec())
          .filter((command) => command?.record_id === fileId || command?.payload?.file_id === fileId)
          .map((command) => ({
            id: command.id || command.command_id || '',
            type: command.type || command.command_type || '',
            status: command.status || '',
            error: command.error || command.result?.error || '',
            updated_at_ms: command.updated_at_ms || null,
          }));
        const advancedStatus = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
          includeCounts: false,
          requiredCollections: ['business_commands', 'desktop_files', 'desktop_file_chunks'],
        }).catch((error) => ({ error: error?.message || String(error) }));
        return {
          file: file ? {
            id: file.id,
            content_state: file.content_state || '',
            content_generation_id: file.content_generation_id || '',
            local_path: file.local_path || '',
            path: file.path || '',
            updated_at_ms: file.updated_at_ms || null,
          } : null,
          liveChunkCount: chunks.filter((chunk) => !chunk?._deleted).length,
          chunkCount: chunks.length,
          commands,
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
          advancedStatusVersion,
          advancedStatusRuntime,
        };
      }

      let peerCheckpointRefresh = null;
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
          await waitForNativePeerOpen(appFileReplicationState, 'desktop_files after native peer restart');
          await waitForNativePeerOpen(appChunkReplicationState, 'desktop_file_chunks after native peer restart');
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
          const restartedSessions = peerSessionsAfterRepair.filter((session) => {
            const before = beforeByCollection.get(session.collection);
            return before
              && before.peerSession
              && session.peerSession
              && before.peerSession !== session.peerSession
              && Number(session.generation || 0) > Number(before.generation || 0);
          });
          const generationChanged = restartedSessions.length > 0;
          if (!generationChanged) {
            throw new Error(`Peer generation did not advance after native peer restart: ${JSON.stringify({
              before: peerSessionsBeforeRestart,
              after: peerSessionsAfterRepair,
              advancedStatusAfterRepair,
              mode: smokeMode,
            }, null, 2)}`);
          }
          const missingCheckpointEpoch = restartedSessions.filter((session) => (
            !session?.checkpoint
            || session.checkpoint.state !== 'advertised'
            || !session.checkpoint.epoch
            || session.checkpoint.collection !== session.collection
          ));
          if (missingCheckpointEpoch.length) {
            throw new Error(`Restarted peer sessions did not refresh checkpoint epoch evidence: ${JSON.stringify({
              missingCheckpointEpoch,
              before: peerSessionsBeforeRestart,
              after: peerSessionsAfterRepair,
              advancedStatusAfterRepair,
              mode: smokeMode,
            }, null, 2)}`);
          }
          peerCheckpointRefresh = {
            restartedCollections: restartedSessions.map((session) => session.collection),
            checkpointEpochs: restartedSessions.map((session) => session.checkpoint?.epoch || ''),
          };
          advancedStatusVersion = advancedStatusAfterRepair?.version || advancedStatusVersion;
          advancedStatusRuntime = advancedStatusAfterRepair?.rxdbRuntime || advancedStatusRuntime;
          const stableFileBridges = typeof repairedState.sync.restartCollections === 'function'
            ? await repairedState.sync.restartCollections(['desktop_files', 'desktop_file_chunks'])
            : [
                await repairedState.sync.restartCollection('desktop_files'),
                await repairedState.sync.restartCollection('desktop_file_chunks'),
              ];
          appFileReplicationState = stableFileBridges[0]?.state || appFileReplicationState;
          appChunkReplicationState = stableFileBridges[1]?.state || appChunkReplicationState;
          await bounded(appFileReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appChunkReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appFileReplicationState?.awaitInSync?.(), 30000);
          await bounded(appChunkReplicationState?.awaitInSync?.(), 30000);
          await waitForNativePeerOpen(appFileReplicationState, 'desktop_files after stable restart');
          await waitForNativePeerOpen(appChunkReplicationState, 'desktop_file_chunks after stable restart');
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
      if (typeof appFileReplicationState?.reSync === 'function') appFileReplicationState.reSync();
      if (typeof appChunkReplicationState?.reSync === 'function') appChunkReplicationState.reSync();
      await bounded(appFileReplicationState?.awaitInSync?.(), 25000);
      await bounded(appChunkReplicationState?.awaitInSync?.(), 25000);
      return {
        mode: smokeMode,
        id,
        readinessPayload: received.payload,
        browserPayload,
        advancedStatusVersion,
        advancedStatusRuntime,
        peerCheckpointRefresh,
        replicationDirections: {
          desktop_files: describeReplicationPool(appFileReplicationState),
          desktop_file_chunks: describeReplicationPool(appChunkReplicationState),
        },
      };
    }, { signalingUrl, smokeMode, rustSeed, useAppDb, browserPayload, backgroundQueueTask, advancedStatusEvidenceVersion, advancedStatusEvidenceRuntime });
    outerPhaseTimings.pageEvaluateMs = Date.now() - pageEvaluateStartedAt;

    if (result.mode === 'business-os-ui-regression') {
      result.screenshotEvidence = await captureBusinessOsVisualScreenshotEvidence(page);
    }

    if (result.mode === 'workspace-agent-artifacts-rust-to-browser'
      || result.mode === 'workspace-agent-artifacts-stress-rust-to-browser'
      || result.mode === 'workspace-agent-artifacts-churn-rust-to-browser'
      || result.mode === 'workspace-agent-artifacts-background-rust-to-browser') {
      const files = Array.isArray(result.files) ? result.files : [];
      const expectedFiles = Array.isArray(rustSeed.files) ? rustSeed.files : [];
      if (files.length !== expectedFiles.length) {
        throw new Error(`workspace artifact count mismatch: ${files.length} !== ${expectedFiles.length}`);
      }
      const totalChunkCount = files.reduce((sum, file) => sum + Number(file.chunkCount || 0), 0);
      const maxChunkCount = files.reduce((max, file) => Math.max(max, Number(file.chunkCount || 0)), 0);
      console.log(`replicated_count=${files.length}`);
      console.log(`replicated_ids=${files.map((file) => file.id).join(',')}`);
      console.log(`virtual_paths=${files.map((file) => file.virtualPath).join(',')}`);
      console.log(`payload_lengths=${files.map((file) => file.payloadLength).join(',')}`);
      console.log(`chunk_counts=${files.map((file) => file.chunkCount).join(',')}`);
      console.log(`total_chunk_count=${totalChunkCount}`);
      console.log(`max_chunk_count=${maxChunkCount}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
      if (result.mode === 'workspace-agent-artifacts-churn-rust-to-browser') {
        console.log(`updated_generation_changes=${Number(result.updatedGenerationChanges || 0)}`);
        console.log(`added_count=${Number(result.addedCount || 0)}`);
        console.log(`updated_relative_paths=${(result.updatedRelativePaths || []).join(',')}`);
        console.log(`added_relative_paths=${(result.addedRelativePaths || []).join(',')}`);
        if (result.phaseTimings) console.log(`phase_timings=${JSON.stringify(result.phaseTimings)}`);
      }
      if (result.mode === 'workspace-agent-artifacts-background-rust-to-browser') {
        if (result.advancedStatus) {
          assertHealthyAdvancedStatusContract(result.advancedStatus);
          console.log(`advanced_status=${result.advancedStatus.version}`);
          console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatus.rxdbRuntime || null)}`);
        }
        console.log(`background_indexer=1`);
        console.log(`background_queue_task_created=${result.backgroundQueueTask?.created ? 1 : 0}`);
        console.log(`background_queue_task_id=${result.backgroundQueueTask?.taskId || ''}`);
        if (result.phaseTimings) console.log(`phase_timings=${JSON.stringify(result.phaseTimings)}`);
      }
    } else if (result.mode === 'rust-to-browser' || result.mode === 'workspace-rust-to-browser') {
      if (result.payload !== rustSeed.content) throw new Error(`browser payload mismatch: ${result.payload}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
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
        console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatus.rxdbRuntime || null)}`);
      }
      console.log(`replicated_id=${result.id}`);
      if (result.commandId) console.log(`command_id=${result.commandId}`);
      if (result.commandStatus) console.log(`command_status=${result.commandStatus}`);
      if (result.virtualPath) console.log(`virtual_path=${result.virtualPath}`);
      console.log(`generation=${result.generationId}`);
      console.log(`chunk_count=${result.chunkCount}`);
      console.log(`payload_length=${result.payloadLength}`);
      if (result.desktopViewer) {
        console.log(`file_viewer_desktop_window_count=${result.desktopViewer.viewerWindowCount || 0}`);
        console.log(`file_viewer_desktop_rendered_length=${result.desktopViewer.renderedLength || 0}`);
      }
      if (result.phaseTimings) console.log(`phase_timings=${JSON.stringify(result.phaseTimings)}`);
    } else if (result.mode === 'file-chunk-metadata-error-browser-status'
      || result.mode === 'file-chunk-tombstone-error-browser-status'
      || result.mode === 'file-chunk-stale-generation-error-browser-status') {
      console.log(`file_integrity_error_name=${result.fileIntegrityName}`);
      console.log(`file_integrity_error_code=${result.fileIntegrityCode}`);
      console.log(`file_integrity_error_phase=${result.fileIntegrityPhase}`);
      console.log(`file_integrity_error_source=${result.fileIntegritySource}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
      console.log(`replicated_id=${result.id}`);
      if (result.chunkId) console.log(`chunk_id=${result.chunkId}`);
      if (Number.isFinite(Number(result.expectedSizeBytes))) console.log(`expected_size_bytes=${result.expectedSizeBytes}`);
      if (Number.isFinite(Number(result.actualSizeBytes))) console.log(`actual_size_bytes=${result.actualSizeBytes}`);
      if (Number.isFinite(Number(result.liveChunkCount))) console.log(`live_chunk_count=${result.liveChunkCount}`);
      if (Number.isFinite(Number(result.tombstonedChunkCount))) console.log(`tombstoned_chunk_count=${result.tombstonedChunkCount}`);
      if (result.requestedGenerationId) console.log(`requested_generation=${result.requestedGenerationId}`);
      if (Number.isFinite(Number(result.requestedGenerationChunkCount))) console.log(`requested_generation_chunk_count=${result.requestedGenerationChunkCount}`);
      if (Array.isArray(result.availableGenerationIds)) console.log(`available_generations=${result.availableGenerationIds.join(',')}`);
    } else if (result.mode === 'business-os-ui-regression') {
      console.log(`business_os_ui_module_count=${result.moduleCount}`);
      console.log(`business_os_ui_start_menu_items=${result.startMenuItemCount}`);
      console.log(`business_os_ui_opened_modules=${result.openedModules.map((entry) => entry.activeModule).join(',')}`);
      console.log(`business_os_ui_rendered_modules=${result.openedModules.map((entry) => entry.renderEvidence?.moduleId).filter(Boolean).join(',')}`);
      console.log(`business_os_ui_interacted_modules=${result.openedModules.map((entry) => entry.interactionEvidence?.moduleId).filter(Boolean).join(',')}`);
      console.log(`business_os_ui_interaction_names=${result.openedModules.flatMap((entry) => (entry.interactionEvidence?.actions || []).map((action) => String(action).replace(/:.+$/, ''))).join(',')}`);
      console.log(`business_os_ui_interaction_actions=${result.openedModules.flatMap((entry) => entry.interactionEvidence?.actions || []).join(',')}`);
      console.log(`business_os_ui_min_module_text_length=${Math.min(...result.openedModules.map((entry) => Number(entry.renderEvidence?.textLength || 0)))}`);
      console.log(`business_os_ui_secondary_opened_modules=${(result.secondaryOpenedModules || []).map((entry) => entry.activeModule).join(',')}`);
      console.log(`business_os_ui_secondary_rendered_modules=${(result.secondaryOpenedModules || []).map((entry) => entry.renderEvidence?.moduleId).filter(Boolean).join(',')}`);
      console.log(`business_os_ui_secondary_interacted_modules=${(result.secondaryOpenedModules || []).map((entry) => entry.interactionEvidence?.moduleId).filter(Boolean).join(',')}`);
      console.log(`business_os_ui_secondary_interaction_names=${(result.secondaryOpenedModules || []).flatMap((entry) => (entry.interactionEvidence?.actions || []).map((action) => String(action).replace(/:.+$/, ''))).join(',')}`);
      console.log(`business_os_ui_min_secondary_text_length=${Math.min(...(result.secondaryOpenedModules || []).map((entry) => Number(entry.renderEvidence?.textLength || 0)))}`);
      console.log(`business_os_ui_desktop_opened=${result.desktopOpened ? 1 : 0}`);
      console.log(`business_os_ui_active_module=${result.activeModule || ''}`);
      console.log(`business_os_visual_workspace_visible=${result.visualEvidence?.workspace?.visible ? 1 : 0}`);
      console.log(`business_os_visual_desktop_icon_count=${result.visualEvidence?.desktopIconCount || 0}`);
      console.log(`business_os_visual_screenshot_unique_colors=${result.screenshotEvidence?.uniqueSampledColors || 0}`);
      console.log(`business_os_visual_screenshot_luma_stddev=${result.screenshotEvidence?.luminanceStdDev || 0}`);
      console.log(`business_os_visual_screenshot_dominant_ratio_pct=${result.screenshotEvidence?.dominantColorRatioPct || 0}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
    } else if (result.mode === 'outbound-active-ui') {
      console.log(`outbound_active_ui_campaign_id=${result.campaignId}`);
      console.log(`outbound_active_ui_pipeline_id=${result.pipelineId}`);
      console.log(`outbound_active_ui_engagement_id=${result.engagementId}`);
      console.log(`outbound_active_ui_message_id=${result.messageId}`);
      console.log(`outbound_active_ui_scheduling_message_id=${result.schedulingMessageId || ''}`);
      console.log(`outbound_active_ui_meeting_request_id=${result.meetingRequestId || ''}`);
      console.log(`outbound_active_ui_approval_id=${result.approvalId}`);
      console.log(`outbound_active_ui_approval_gate_verified=${result.approvalGateVerified ? 1 : 0}`);
      console.log(`outbound_active_ui_final_send_status=${result.finalSendStatus || ''}`);
      console.log(`outbound_active_ui_provider_message_id=${result.providerMessageId || ''}`);
      console.log(`outbound_active_ui_communication_message_key=${result.communicationMessageKey || ''}`);
      console.log(`outbound_active_ui_reply_message_key=${result.replyMessageKey || ''}`);
      console.log(`outbound_active_ui_reply_classification=${result.replyClassification || ''}`);
      console.log(`outbound_active_ui_meeting_status=${result.meetingStatus || ''}`);
      console.log(`outbound_active_ui_meeting_url=${result.meetingUrl || ''}`);
      console.log(`outbound_active_ui_conversation_link=${result.conversationLink || ''}`);
      console.log(`outbound_active_ui_screenshots=${(result.screenshotPaths || []).join(',')}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
    } else if (result.mode === 'command-burst-browser-to-rust') {
      console.log(`command_count=${result.commandCount}`);
      console.log(`task_count_for_commands=${result.taskCountForCommands}`);
      console.log(`command_ids=${result.ids.join(',')}`);
      console.log(`task_ids=${result.taskIds.join(',')}`);
    } else if (result.mode === 'command-browser-to-rust'
      || result.mode === 'migration-version-browser-to-rust'
      || result.mode === 'command-restart-browser-to-rust'
      || result.mode === 'command-midflight-restart-browser-to-rust') {
      if (result.mode === 'migration-version-browser-to-rust') {
        const commandTable = 'ctox_business_os__business_commands__v1';
        const staleCommandTable = 'ctox_business_os__business_commands__v0';
        const taskTable = 'ctox_business_os__ctox_queue_tasks__v0';
        const commandRow = pollSqliteJson(commandTable, result.id);
        const taskRow = pollSqliteJson(taskTable, result.taskId);
        const staleRows = sqliteRowCount(staleCommandTable, `id='${sqlString(result.id)}'`);
        if (staleRows !== 0) {
          throw new Error(`business_commands stale schema table received command rows: ${staleRows}`);
        }
        if (commandRow.command_id !== result.id || taskRow.command_id !== result.id) {
          throw new Error(`migration-version command/task rows mismatch: ${JSON.stringify({ commandRow, taskRow })}`);
        }
        console.log(`schema_collection=business_commands`);
        console.log(`schema_version=1`);
        console.log(`schema_table=${commandTable}`);
        console.log(`stale_schema_table=${staleCommandTable}`);
        console.log(`stale_schema_table_rows=${staleRows}`);
        console.log(`task_table=${taskTable}`);
      }
      console.log(`command_id=${result.id}`);
      console.log(`task_id=${result.taskId}`);
      console.log(`task_count_for_command=${result.taskCountForCommand}`);
      console.log(`status=${result.status}`);
      console.log(`task_status=${result.taskStatus}`);
    } else if (result.mode === 'tickets-browser-to-rust') {
      console.log(`command_id=${result.id}`);
      console.log(`task_id=${result.taskId}`);
      console.log(`status=${result.status}`);
      console.log(`ticket_key=${result.ticketKey}`);
      console.log(`ticket_source=${result.ticketSource}`);
      console.log(`ticket_title=${result.ticketTitle}`);
    } else if (result.mode === 'tickets-clarification-browser-to-rust') {
      console.log(`create_command_id=${result.createCommandId}`);
      console.log(`clarification_command_id=${result.clarificationCommandId}`);
      console.log(`publish_command_id=${result.publishCommandId || ''}`);
      console.log(`create_status=${result.createStatus}`);
      console.log(`clarification_status=${result.clarificationStatus}`);
      console.log(`publish_status=${result.publishStatus || ''}`);
      console.log(`ticket_key=${result.ticketKey}`);
      console.log(`clarification_id=${result.clarificationId}`);
      console.log(`clarification_request_status=${result.clarificationRequestStatus}`);
      console.log(`clarification_projection_status=${result.projectionStatus || ''}`);
      console.log(`outbound_message_key=${result.outboundMessageKey || ''}`);
      console.log(`missing_input_count=${result.missingInputCount}`);
    } else {
      if (result.readinessPayload !== rustSeed.content) {
        throw new Error(`browser readiness payload mismatch: ${result.readinessPayload}`);
      }
      if (result.replicationDirections) {
        console.log(`replication_directions=${JSON.stringify(result.replicationDirections)}`);
      }
      let replicated;
      try {
        replicated = pollSqliteFileAndChunk(result.id);
      } catch (error) {
        throw new Error(`${error.message}; replicationDirections=${JSON.stringify(result.replicationDirections || null)}`);
      }
      if (replicated.payload !== result.browserPayload) {
        throw new Error(`sqlite payload mismatch: ${replicated.payload}`);
      }
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
      if (result.peerCheckpointRefresh) {
        console.log(`checkpoint_restarted_collections=${result.peerCheckpointRefresh.restartedCollections.join(',')}`);
        console.log(`checkpoint_epoch_count=${result.peerCheckpointRefresh.checkpointEpochs.filter(Boolean).length}`);
      }
      console.log(`readiness_payload=${result.readinessPayload}`);
      console.log(`replicated_id=${result.id}`);
      console.log(JSON.stringify({
        file: replicated.file.id,
        chunk: replicated.chunk.id,
        payload: replicated.payload,
      }));
    }
    console.log(`outer_phase_timings=${JSON.stringify(outerPhaseTimings)}`);
    emitBrowserDiagnostics();
  } finally {
    if (browser) await withHostTimeout(browser.close(), 5000).catch(() => {});
    if (browserUserDataDir) removeSmokePath(browserUserDataDir);
    await stopChild(ctox);
    await stopSignalingServer(signaling);
    if (!runtimeRootProvided && !keepSmokeArtifacts) removeSmokePath(runtimeRoot);
  }
})().then(() => {
  restoreProtectedBusinessOsSources();
  process.exit(0);
}).catch((error) => {
  console.error(error.stack || error.message || error);
  if (!runtimeRootProvided && !keepSmokeArtifacts) removeSmokePath(runtimeRoot);
  process.exit(1);
});
