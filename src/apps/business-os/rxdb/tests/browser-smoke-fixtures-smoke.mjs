import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

execFileSync(process.execPath, ['--test', '--test-concurrency=2',
  fileURLToPath(new URL('../../../../core/rxdb/tools/browser_demand_chunk_decode.test.js', import.meta.url)),
  fileURLToPath(new URL('../../../../core/rxdb/tools/native_profile_fixture.test.js', import.meta.url)),
], { stdio: 'inherit', timeout: 30_000 });
