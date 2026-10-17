// V4 regression contract: execute the real kernel invariants; legacy transport is retired.
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const tests=["session.test.mjs"].map(name=>fileURLToPath(new URL('../../kernel/tests/'+name,import.meta.url)));
execFileSync(process.execPath,['--test','--test-concurrency=2',...tests],{stdio:'inherit',timeout:120000});
