/** Host orchestration needs Node filesystem/process APIs, which Playwright CLI run-code does not expose. */
import { readFileSync, writeFileSync, mkdirSync, realpathSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { validateConfig, measureShellRollback, runAcceptance, stopOwnedAcceptance } from './installed-sync-acceptance.mjs';

const [configPath, playwrightRoot] = process.argv.slice(2);
if (!configPath || !playwrightRoot || !process.env.TMPDIR || !process.env.CARGO_TARGET_DIR)
  throw new Error('Use the admitted GPU launcher with private config and task-local Playwright1.64.0');
const config = validateConfig(JSON.parse(readFileSync(configPath)), configPath);
const tools = realpathSync(playwrightRoot), temporary = realpathSync(process.env.TMPDIR);
if (!tools.startsWith(temporary + '/') || JSON.parse(readFileSync(join(tools, 'package.json'))).version !== '1.64.0')
  throw new Error('Pinned Playwright package must be installed inside the gate TMPDIR');
const { chromium } = await import(pathToFileURL(join(tools, 'index.mjs')).href);
const out = join(config.acceptanceBase, `browser-controller-${Date.now()}-${process.pid}`); mkdirSync(out, { mode: 0o700 });
const receipt = { owner: config.owner, source: config.source, host: config.host,
  launcherPid: process.pid, startedAt: new Date().toISOString(),
  stop: 'first uncaught failure or3000s; own browser group and native groups only',
  playwright: '1.64.0', maximumClientWorkers: 2, terminal: false, pass: false };
const save = () => writeFileSync(join(out, 'receipt.json'), JSON.stringify(receipt, null, 2) + '\n', { mode: 0o600 });
let server, browser, browserPid, browserPgid;
const stop = async () => {
  try { await stopOwnedAcceptance(); receipt.nativeGroupsStopped = true; }
  catch { receipt.nativeGroupsStopped = false; }
  try { await browser?.close(); } catch {}
  if (server) {
    await Promise.race([server.close().catch(() => {}), new Promise(r => setTimeout(r, 5000))]);
  }
  if (browserPid && browserPgid === browserPid) {
    try { process.kill(-browserPgid, 'SIGKILL'); }
    catch (e) { if (e.code !== 'ESRCH') receipt.browserCleanupError = e.code || e.name; }
    receipt.browserGroupAbsent = false;
    for (let attempt = 0; attempt < 100; attempt++) {
      try { process.kill(-browserPgid, 0); }
      catch (e) { if (e.code === 'ESRCH') receipt.browserGroupAbsent = true; break; }
      await new Promise(resolve => setTimeout(resolve, 50));
    }
  }
};
const deadline = setTimeout(() => { receipt.deadlineReached = true; save(); void stop(); }, 3000000);
process.once('SIGTERM', () => { receipt.interrupted = true; save(); void stop(); });
save();
try {
  // Component proof comes first and cannot touch a service outside the synthetic prefix.
  receipt.phase = 'component-rollback'; save();
  receipt.rollback = await measureShellRollback(configPath); save();
  if (receipt.interrupted || receipt.deadlineReached) throw new Error('Owned acceptance unit interrupted');
  receipt.phase = 'browser-launch'; save();
  server = await chromium.launchServer({ executablePath: '/usr/bin/google-chrome', headless: true });
  receipt.phase = 'browser-group-check'; save();
  browserPid = server.process().pid;
  browserPgid = Number(spawnSync('ps', ['-o', 'pgid=', '-p', String(browserPid)], { encoding: 'utf8' }).stdout.trim());
  receipt.browserPid = browserPid; receipt.browserPgid = browserPgid; save();
  if (browserPgid !== browserPid) throw new Error('Browser has no isolated owned process group');
  if (receipt.interrupted || receipt.deadlineReached) throw new Error('Owned acceptance unit interrupted');
  receipt.phase = 'browser-connect'; save();
  browser = await chromium.connect(server.wsEndpoint()); // Private endpoint never enters logs or receipts.
  receipt.browserVersion = browser.version(); save();
  receipt.phase = 'sync-measurement'; save();
  receipt.measurements = await runAcceptance(browser, configPath);
  receipt.pass = receipt.rollback.componentRollbackPassed && receipt.measurements.goals.every(row => row.pass);
} catch (error) {
  receipt.failure = { name: error.name, message: 'Installed runner failed; native credentials and browser endpoint suppressed' };
  receipt.failure.errorSha256 = createHash('sha256').update(String(error.message)).digest('hex');
  // No invitations or private browser endpoint exist before launch succeeds.
  if (receipt.phase === 'browser-launch') receipt.failure.launchDetail = String(error.message)
    .replace(/(?:wss?|https?):\/\/[^\s"'<>]+/g, '[URL omitted]').slice(0, 16000);
} finally {
  clearTimeout(deadline);
  await stop();
  receipt.terminal = true; receipt.finishedAt = new Date().toISOString();
  if (browserPid) {
    try { process.kill(browserPid, 0); receipt.browserProcessAbsent = false; }
    catch (error) { receipt.browserProcessAbsent = error.code === 'ESRCH'; }
  }
  receipt.pass = Boolean(receipt.pass && receipt.nativeGroupsStopped
    && receipt.browserGroupAbsent && receipt.browserProcessAbsent
    && !receipt.interrupted && !receipt.deadlineReached && !receipt.browserCleanupError);
  save();
}
console.log(JSON.stringify({ receipt: join(out, 'receipt.json'), pass: receipt.pass, terminal: receipt.terminal }));
process.exitCode = receipt.pass ? 0 : 1;
