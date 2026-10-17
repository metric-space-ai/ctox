#!/usr/bin/env node
// Mutationsbeleg Strang SKL-A4 (iot): je Invariante eine Mutation in einer TEMP-SPIEGELUNG von business-os
// (modules/iot echte Kopie, alles andere Symlinks); die iot-Handler-Tests müssen rot werden. Arbeitsbaum bleibt unberührt.
// Aufruf: node modules/iot/handlers/tests/mutation-proof-skl.mjs  (Exit 0 nur, wenn Baseline grün und jede Mutation rot)
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const BOS = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
const SHARED = path.join(BOS, 'v4/tools/node/ctox-shared.mjs');

const MUTATIONS = [
  { file: 'handlers/_iotcore.js', what: 'Regel-Enqueue ohne on_complete', from: "on_complete: 'ctox.iot.task.complete', on_failed: 'ctox.iot.task.failed',\n      });\n    }", to: "on_failed: 'ctox.iot.task.failed',\n      });\n    }" },
  { file: 'handlers/_iotcore.js', what: 'Regel-Enqueue mit Core-Skill', from: "skill: 'iot/operations', thread_key: `iot-ruleset", to: "skill: 'iot-operations', thread_key: `iot-ruleset" },
  { file: 'handlers/ctox.iot.signal.evaluate.js', what: 'Widget-Enqueue ohne on_failed', from: "on_complete: 'ctox.iot.task.complete', on_failed: 'ctox.iot.task.failed',", to: "on_complete: 'ctox.iot.task.complete'," },
  { file: 'handlers/ctox.iot.signal.evaluate.js', what: 'Widget-Enqueue ohne Idempotenzschlüssel', from: "      idempotency_key: `iot-alarm:${alarm.id}`, on_complete", to: '      on_complete' },
  { file: 'handlers/_agenttask.js', what: 'Doppelzustellung wirkt erneut', from: 'if (prior && prior.task_id === cb.taskId && TERMINAL.includes(prior.status)) {', to: 'if (false) {' },
  { file: 'handlers/_agenttask.js', what: 'complete quittiert nicht', from: "if (decision === 'acknowledge') return current === 'Open'", to: "if (decision === 'acknowledge') return false" },
  { file: 'handlers/_agenttask.js', what: 'Übergangsregeln ignoriert (resolve aus Closed)', from: "return current !== 'Resolved' && canTransition(current, 'Resolved') ? 'Resolved' : null;", to: "return current !== 'Resolved' ? 'Resolved' : null;" },
  { file: 'handlers/_agenttask.js', what: 'Abbruch als failed verbucht', from: "const status = cb.status === 'cancelled' ? 'cancelled' : 'failed';", to: "const status = 'failed';" },
  { file: 'handlers/_agenttask.js', what: 'failed setzt Alarm auf quittiert', from: "recordOnAlarm(ctx, cb, (_alarm, now) => ({ status: null,", to: "recordOnAlarm(ctx, cb, (_alarm, now) => ({ status: 'Acknowledged'," },
  { file: 'handlers/_agenttask.js', what: 'Schreibvorschläge ungefiltert', from: ".filter((w) => isObject(w) && text(w.asset_id) && text(w.attribute))", to: '.filter(() => true)' },
  { file: 'handlers/ctox.iot.webhook.register.js', what: 'webhook.register ohne command', from: '    command: INGEST_COMMAND,\n', to: '' },
  { file: 'handlers/ctox.iot.webhook.register.js', what: 'webhook.register ohne aliases', from: '    aliases: [...INGEST_ALIASES],\n', to: '' },
  { file: 'skills/operations/SKILL.md', what: 'Skill-Datei fehlt im Paket', remove: true },
];

function mirror() {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'skl-a4-'));
  const bos = path.join(tmp, 'business-os');
  fs.mkdirSync(path.join(bos, 'modules'), { recursive: true });
  for (const e of fs.readdirSync(BOS)) if (e !== 'modules') fs.symlinkSync(path.join(BOS, e), path.join(bos, e));
  for (const e of fs.readdirSync(path.join(BOS, 'modules'))) {
    if (e === 'iot') fs.cpSync(path.join(BOS, 'modules/iot'), path.join(bos, 'modules/iot'), { recursive: true });
    else fs.symlinkSync(path.join(BOS, 'modules', e), path.join(bos, 'modules', e));
  }
  return { tmp, iot: path.join(bos, 'modules/iot') };
}

function runTests(iot) {
  const dir = path.join(iot, 'handlers/tests');
  const files = fs.readdirSync(dir).filter((f) => f.endsWith('.test.mjs')).map((f) => path.join(dir, f));
  const r = spawnSync(process.execPath, ['--import', SHARED, '--test', ...files], { encoding: 'utf8' });
  return r.status === 0;
}

let ok = true;
const base = mirror();
const green = runTests(base.iot);
fs.rmSync(base.tmp, { recursive: true, force: true });
console.log(`Baseline: ${green ? 'grün' : 'ROT'}`);
if (!green) process.exit(1);
for (const m of MUTATIONS) {
  const { tmp, iot } = mirror();
  const file = path.join(iot, m.file);
  if (m.remove) fs.rmSync(file);
  else {
    const src = fs.readFileSync(file, 'utf8');
    if (!src.includes(m.from)) { console.log(`  FEHLT  ${m.what} (Ausgangstext nicht gefunden)`); ok = false; fs.rmSync(tmp, { recursive: true, force: true }); continue; }
    fs.writeFileSync(file, src.replace(m.from, m.to));
  }
  const red = !runTests(iot);
  fs.rmSync(tmp, { recursive: true, force: true });
  console.log(`  ${red ? 'rot   ' : 'GRÜN! '} ${m.what}`);
  if (!red) ok = false;
}
console.log(ok ? `Mutationsbeleg: ${MUTATIONS.length}/${MUTATIONS.length} rot` : 'Mutationsbeleg: FEHLGESCHLAGEN');
process.exit(ok ? 0 : 1);
