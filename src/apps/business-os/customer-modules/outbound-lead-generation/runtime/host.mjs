// Host für erzeugte Scrape-Skripte der App outbound-lead-generation (Strang OLG725, Kern §16.48 Nr. 3).
//
// Allowlist-Eintrag `olg-scrape-js` = node + runtime/host.mjs (untrusted_code:true → läuft nur in der Sandbox E20).
// Das erzeugte Skript ist App-Daten (Registry, files); es kommt zusammen mit der Konfiguration als EIN JSON-Dokument
// auf stdin (bis 1 MiB, darüber per stdin_ref – die Plattform liefert den Inhalt ebenfalls auf stdin):
//   {schema:'ctox.olg.scrape_host.v1', language:'javascript'|'typescript', script_sha256, script, env:{CTOX_SCRAPE_*…}}
// Ablauf: Dokument prüfen (Schema, Sprache, sha256 des getrimmten Skripts = Registrierung), env-Werte setzen (nur
// CTOX_SCRAPE_* und CTOX_OWNER_USER_ID; PATH/CTOX_BIN/LANG setzt die Plattform und bleiben unberührt), Skript ins
// Arbeitsverzeichnis schreiben (cwd = <workspace>/runs/<run_id>) und ausführen. Das Ergebnis schreibt das Skript wie
// bisher als JSON auf stdout; der Host selbst schreibt nichts auf stdout. Fehler → stderr + Exit 1.
// JavaScript ohne import/export läuft als CommonJS (gleich dem früheren `node -`), sonst als ES-Modul.
import { createHash } from 'node:crypto';
import { writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export const HOST_SCHEMA = 'ctox.olg.scrape_host.v1';
const ENV_KEY = /^(CTOX_SCRAPE_[A-Z0-9_]+|CTOX_OWNER_USER_ID)$/;
const ESM = /^\s*(import\s[\s\S]*?from\s|import\s*['"]|export\s)/m;

export class HostError extends Error {}

/** stdin-Dokument prüfen → {language, script, env}. Wirft HostError mit klarer Ursache. */
export function parseHostInput(text) {
  let doc;
  try {
    doc = JSON.parse(String(text ?? ''));
  } catch {
    throw new HostError('stdin ist kein gültiges JSON-Dokument');
  }
  if (doc?.schema !== HOST_SCHEMA) throw new HostError(`stdin-Schema ${doc?.schema ?? '-'} statt ${HOST_SCHEMA}`);
  if (doc.language !== 'javascript' && doc.language !== 'typescript') throw new HostError(`Sprache ${doc.language ?? '-'} nicht zugelassen`);
  if (typeof doc.script !== 'string' || doc.script.trim() === '') throw new HostError('kein Skript im stdin-Dokument');
  const sha = createHash('sha256').update(doc.script.trim()).digest('hex');
  if (sha !== doc.script_sha256) throw new HostError('Skript passt nicht zur registrierten SHA-256');
  const env = {};
  for (const [key, value] of Object.entries(doc.env ?? {})) {
    if (!ENV_KEY.test(key)) throw new HostError(`env-Schlüssel ${key} nicht erlaubt (Grundumgebung setzt die Plattform)`);
    if (typeof value !== 'string') throw new HostError(`env-Wert ${key} ist kein Text`);
    env[key] = value;
  }
  return { language: doc.language, script: doc.script, env };
}

async function readStdin(stream) {
  const chunks = [];
  for await (const chunk of stream) chunks.push(chunk);
  return Buffer.concat(chunks).toString('utf8');
}

/** Ein Lauf: stdin lesen, prüfen, Skript im cwd ablegen und ausführen. `extension` erzwingt die Dateiendung (host.ts). */
export async function runHost({ stdin = process.stdin, cwd = process.cwd(), extension = null } = {}) {
  const { language, script, env } = parseHostInput(await readStdin(stdin));
  if (language === 'typescript' && extension !== '.ts') throw new HostError('TypeScript nur über olg-scrape-ts (tsx + runtime/host.ts)');
  const ext = extension ?? (ESM.test(script) ? '.mjs' : '.cjs');
  if (ext === '.ts' && language !== 'typescript') throw new HostError('olg-scrape-ts führt nur TypeScript aus');
  for (const [key, value] of Object.entries(env)) process.env[key] = value;
  const file = join(resolve(cwd), `.olg-scrape-script${ext}`);
  writeFileSync(file, script);
  if (ext === '.cjs') createRequire(file)(file);
  else await import(pathToFileURL(file).href);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  runHost().catch((error) => {
    process.stderr.write(`olg-scrape-host: ${error?.message || error}\n`);
    process.exitCode = 1;
  });
}
