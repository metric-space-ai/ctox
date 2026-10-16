// Shell v4 – Workjet-Brücke: V2-Vertragstests als Orakel (WJB-0).
//
// Die V2-Tests (shared/workjet-*.test.mjs auf V2-main) schneiden die Brückenfunktion als Quelltext aus app.js und
// führen sie in vm mit einem V2-`state`-Aufbau aus. Dieses Harness lädt die V2-Testdatei UNVERÄNDERT aus dem
// gepinnten V2-Stand (git show <PIN>:…), tauscht nur die Funktionsquelle aus: an die Stelle von app.js tritt eine
// Hülle, deren workjetProjectControl die v4-Brücke (createWorkjetBridges) mit dem vm-`state` des Tests aufruft.
// Damit laufen dieselben Requests mit derselben erwarteten Antwortform gegen den v4-Code.
//
// Freigabe je Test über ORACLE_TESTS[<testdatei>] (Testnamen). Nicht freigegebene Tests werden nicht registriert
// und gezählt; Quelltext-Regex-Tests auf app.js (assert.match(controlSource, …)) sind keine Verhaltensorakel.
//
// Lauf: node --test src/apps/business-os/v4/shell/workjet/tests/oracle.test.mjs
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createWorkjetBridges } from '../index.js';

// V2-main zum Messzeitpunkt der Aktionsliste (Workjet eb65947d × V2 e35a116ae, 16.10.2026).
export const V2_PIN = 'e35a116aed0c333444aae80dd93b995d51c8d5c8';
const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = execFileSync('git', ['rev-parse', '--show-toplevel'], { cwd: HERE, encoding: 'utf8' }).trim();
const BOS = 'src/apps/business-os';

function gitShow(rel) {
  return execFileSync('git', ['show', `${V2_PIN}:${BOS}/${rel}`], { cwd: REPO, encoding: 'utf8', maxBuffer: 64 << 20 });
}

// Hülle an Stelle von app.js. Die V2-Tests schneiden von „const WORKJET_PROJECT_CONTROL_MAX_RESULTS“ bis
// „async function waitForSyncBridgeReady“; dazwischen steht hier nur die Weiterleitung an die v4-Brücke.
const APP_SHIM = `// v4-Orakel-Hülle (WJB-0), kein V2-Code
globalThis.workjetProjectControl = workjetProjectControl;
const WORKJET_PROJECT_CONTROL_MAX_RESULTS = 100;
async function workjetProjectControl(request = {}, diagnostics = {}) {
  return __wjb.project(globalThis, request, diagnostics);
}
async function waitForSyncBridgeReady() {}
`;

// v4-Seite: die V2-Umgebung (state, actorContext, newId) kommt live aus dem vm-Global des Tests.
const factory = Object.freeze({
  project(g, request, diagnostics) {
    const env = {
      get state() { return g.state; },
      actorContext: (session) => g.actorContext(session),
      newId: () => (typeof g.newId === 'function' ? g.newId() : globalThis.crypto.randomUUID()),
      now: () => (typeof g.Date?.now === 'function' ? g.Date.now() : Date.now()),
    };
    return createWorkjetBridges(env).workjetProjectControl(request, diagnostics);
  },
});

function materialize(testFile) {
  const dir = path.join(os.tmpdir(), `wjb0-oracle-${V2_PIN.slice(0, 9)}-${process.pid}`);
  const shared = path.join(dir, 'shared');
  fs.mkdirSync(shared, { recursive: true });
  fs.writeFileSync(path.join(dir, 'app.js'), APP_SHIM);
  const seen = new Set();
  const copy = (rel) => {
    if (seen.has(rel)) return;
    seen.add(rel);
    let text = gitShow(`shared/${rel}`);
    if (rel === testFile) {
      text = text
        .replace(/^import test from 'node:test';$/m, `import test from ${JSON.stringify(pathToFileURL(path.join(HERE, 'oracle-test-shim.mjs')).href)};`)
        .replace(/^import vm from 'node:vm';$/m, `import vm from ${JSON.stringify(pathToFileURL(path.join(HERE, 'oracle-vm-shim.mjs')).href)};`);
    }
    fs.writeFileSync(path.join(shared, rel), text);
    for (const m of text.matchAll(/from '\.\/([^']+?)(?:\?[^']*)?'/g)) copy(m[1]);
  };
  copy(testFile);
  return path.join(shared, testFile);
}

/**
 * Führt die freigegebenen Tests einer V2-Testdatei gegen die v4-Brücke aus (registriert sie bei node:test).
 * @returns {Promise<{file: string, registered: string[], skipped: string[]}>}
 */
export async function runOracle(testFile, allow) {
  const registered = [];
  const skipped = [];
  globalThis.__WJB_ORACLE = { factory, allow: (name) => allow.has(name), registered, skipped };
  const file = materialize(testFile);
  await import(pathToFileURL(file).href);
  return { file, registered, skipped };
}
