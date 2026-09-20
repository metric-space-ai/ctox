// Candidate code executes only in the OS-isolated process started by module_semantic_isolation.mjs.
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
const request = JSON.parse(readFileSync(0, 'utf8'));
const candidate = await import(pathToFileURL(request.path).href);
let result;
if (request.kind === 'schema') {
  result = candidate.collections;
  if (!result || typeof result !== 'object' || Array.isArray(result)) {
    throw new Error('schema.js must export a collections object');
  }
} else if (request.kind === 'records') {
  result = [];
  for (const [name, value] of Object.entries(candidate)) {
    if (!/^normalize[A-Z]/.test(name) || typeof value !== 'function') continue;
    try {
      const record = value(request.sample, { nowMs: 1781990000000 });
      const types = record && typeof record === 'object' && !Array.isArray(record)
        ? Object.fromEntries(Object.entries(record).map(([key, item]) =>
          [key, item === null ? 'null' : Array.isArray(item) ? 'array' : typeof item]))
        : null;
      result.push({ name, types });
    } catch (error) {
      result.push({ name, error: String(error.message) });
    }
  }
} else {
  throw new Error('unknown semantic validation request');
}
process.stdout.write(JSON.stringify(result));
