// Materialize only inputs needed for this standalone test; never copies credentials/profiles.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const out = path.resolve(process.argv[2] || '');
if (!process.argv[2] || !out.startsWith('/Volumes/tmp/dev-artifacts/')) throw new Error('Stage directory must be on designated disposable volume');
fs.mkdirSync(out, { recursive: true });
for (const relative of ['tests/sync-v4-bench', 'scripts/sync-v4', 'src/core/rxdb', 'src/core/coding_agents/pi-sidecar', 'src/apps/business-os/rxdb/src', '.cargo']) {
  fs.cpSync(path.join(root, relative), path.join(out, relative), { recursive: true,
    filter: source => !['node_modules', 'target', 'dist'].includes(path.basename(source)) });
}
for (const relative of ['Cargo.toml', 'Cargo.lock']) fs.copyFileSync(path.join(root, relative), path.join(out, relative));
const hash = relative => crypto.createHash('sha256').update(fs.readFileSync(path.join(out, relative))).digest('hex');
const sourceHead = execFileSync('git', ['-C', root, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
fs.writeFileSync(path.join(out, 'source-manifest.json'), JSON.stringify({ sourceHead, stagedAt: new Date().toISOString(),
  nativeTransportSha256: hash('src/core/rxdb/src/plugins/replication_webrtc/connection_handler_rs.rs'),
  browserTransportSha256: hash('src/apps/business-os/rxdb/src/webrtc-native.mjs') }, null, 2));
console.log(out);
