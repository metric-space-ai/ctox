import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../../..');
execFileSync(process.execPath, [
  path.join(repoRoot, 'src/core/business_os/tools/build_task_id_inventory.mjs'),
  '--check',
], { stdio: 'inherit' });
