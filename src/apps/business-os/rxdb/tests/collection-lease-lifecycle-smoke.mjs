// Component/runtime lifecycle guards. Real Browser/WebRTC/native acceptance
// remains a separate CI job; this smoke makes these regressions mandatory.
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const tests = [
  '../../shared/sync-collection-registry.test.mjs',
  '../../shared/command-bus.test.mjs',
  '../../shared/documents-facade.test.mjs',
].map(path => fileURLToPath(new URL(path, import.meta.url)));
execFileSync(process.execPath, ['--test', ...tests], { stdio: 'inherit', timeout: 120000 });
