// SPDX-License-Identifier: MIT OR AGPL-3.0-only
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const importLine = "import { readTerminalCommandResult } from './terminal-command-result.mjs';\n";
const registryBefore = `    const result = receipt?.result?.result || receipt?.result || receipt;
    const ziele = Array.isArray(result?.targets) ? result.targets : [];`;
const registryAfter = `    const result = await readTerminalCommandResult(receipt, {
      commandId, commandBus: state.ctx?.commandBus,
      accepts: (value) => Array.isArray(value?.targets),
    });
    const ziele = result.targets;`;
const secretsBefore = `    const outcome = receipt?.result?.result || receipt?.result || receipt || {};
    const eintraege = [...(outcome.catalog || []), ...(outcome.extra || [])];`;
const secretsAfter = `    const outcome = await readTerminalCommandResult(receipt, {
      commandId, commandBus: state.ctx?.commandBus,
      accepts: (value) => Array.isArray(value?.catalog)
        && (value.extra === undefined || Array.isArray(value.extra)),
    });
    const eintraege = [...outcome.catalog, ...(outcome.extra || [])];`;

export function patchOutboundTerminalReads(source) {
  if (source.includes(importLine)) throw new Error('Terminal-result patch already present');
  let patched = source;
  for (const [before, after] of [[registryBefore, registryAfter], [secretsBefore, secretsAfter]]) {
    if (patched.split(before).length !== 2) throw new Error('App source changed: expected exactly one read-result block');
    patched = patched.replace(before, after);
  }
  return importLine + patched;
}

// Produces source overrides for the existing app release path. It never edits
// the input file, registers adapters or activates a production module.
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [input, output] = process.argv.slice(2);
  if (!input || !output) throw new Error('Usage: stage-outbound-terminal-results.mjs source-index.js new-stage-directory');
  const sourcePath = resolve(input);
  const outputDir = resolve(output);
  if (dirname(sourcePath) === outputDir) throw new Error('Use a separate source staging directory');
  const source = readFileSync(sourcePath, 'utf8');
  const patched = patchOutboundTerminalReads(source);
  mkdirSync(outputDir, { recursive: false });
  writeFileSync(resolve(outputDir, 'index.js'), patched);
  writeFileSync(resolve(outputDir, 'terminal-command-result.mjs'), readFileSync(new URL('../shared/terminal-command-result.mjs', import.meta.url)));
  const sha = (text) => createHash('sha256').update(text).digest('hex');
  console.log(JSON.stringify({ source_sha256: sha(source), staged_sha256: sha(patched), output: outputDir, installed: false }));
}
