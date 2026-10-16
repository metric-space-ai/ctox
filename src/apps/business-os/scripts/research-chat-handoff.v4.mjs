#!/usr/bin/env node
// CI-Prüfung (crew-ui-acceptance): Web Research übergibt den systematischen Lauf im Shell-v4-Rahmen an den Business
// Chat – dieselbe Bedienbeobachtung wie shared/business-chat.test.mjs:3564 für V2, hier im Browser gefahren
// (Playwright, echte Maus/Tastatur, Rahmen + Kern-Stub): research öffnet den Chat mit der vollständigen Anweisung als
// Entwurf, solange der Chat steht wird nichts direkt gesendet, der Chat sendet research.systematic.run; ohne Chat
// direkter Weg ohne Zonenfehler; Entwurf byte-gleich zur Handler-Anweisung.
// Fälle und Ablauf: v4/shell/chat/tests/research-handoff.test.mjs (einzige Implementierung, hier nur Umgebung).
// Lauf (aus src/apps/business-os): node scripts/research-chat-handoff.v4.mjs            → Exit 0
//                                  node scripts/research-chat-handoff.v4.mjs counter    → Gegenprobe, Exit 1
// Stub: CTOX_APP_API_STUB, sonst ~/.local/state/shell-v4/stub-pins/b6624eb, sonst die Repo-Kopie
// modules/ctox/tests/fixtures/app-api-stub (GitHub-Runner ohne lokale Pins). Bilder: RESEARCH_CHAT_HANDOFF_OUTPUT_DIR.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const bos = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const stubCandidates = [
  process.env.CTOX_APP_API_STUB,
  path.join(os.homedir(), '.local/state/shell-v4/stub-pins/b6624eb'),
  path.join(bos, 'modules/ctox/tests/fixtures/app-api-stub'),
].filter(Boolean);
const stub = stubCandidates.find((dir) => fs.existsSync(path.join(dir, 'ctox-app-api-stub.mjs')));
if (!stub) {
  console.error(`research-chat-handoff: kein Kern-Stub gefunden (${stubCandidates.join(', ')})`);
  process.exit(1);
}
const env = { ...process.env, CTOX_APP_API_STUB: stub };
if (process.env.RESEARCH_CHAT_HANDOFF_OUTPUT_DIR) env.RCH2_SHOTS = process.env.RESEARCH_CHAT_HANDOFF_OUTPUT_DIR;
console.log(`research-chat-handoff: Stub ${stub}`);
const run = spawnSync(process.execPath, [path.join(bos, 'v4/shell/chat/tests/research-handoff.test.mjs'), ...process.argv.slice(2)], {
  cwd: bos, env, stdio: 'inherit',
});
process.exit(run.status ?? 1);
