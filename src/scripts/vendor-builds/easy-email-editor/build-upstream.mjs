#!/usr/bin/env node

import { execFileSync } from 'node:child_process';
import {
  cpSync,
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const PIN = '16bb02926a20af20dc6dc473c72619f4a0b4f64b';
const REPOSITORY = 'https://github.com/zalify/easy-email-editor.git';
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const vendorDir = path.resolve(
  scriptDir,
  '../../../apps/business-os/modules/mail/vendor/easy-email-editor',
);
const bundleDir = path.join(vendorDir, 'bundle');
const suppliedUpstream = process.argv[2] ? path.resolve(process.argv[2]) : null;
const workRoot = mkdtempSync(path.join(tmpdir(), 'ctox-easy-email-build-'));
const upstreamDir = suppliedUpstream || path.join(workRoot, 'upstream');

function run(command, args, cwd) {
  execFileSync(command, args, { cwd, stdio: 'inherit' });
}

if (!suppliedUpstream) {
  run('git', ['clone', REPOSITORY, upstreamDir], workRoot);
  run('git', ['checkout', '--detach', PIN], upstreamDir);
}

const actualPin = execFileSync('git', ['rev-parse', 'HEAD'], {
  cwd: upstreamDir,
  encoding: 'utf8',
}).trim();
if (actualPin !== PIN) throw new Error(`Expected upstream ${PIN}, received ${actualPin}`);

const bridgeDir = path.join(upstreamDir, 'bridge');
cpSync(scriptDir, bridgeDir, { recursive: true });
rmSync(path.join(bridgeDir, 'dist'), { recursive: true, force: true });
rmSync(path.join(bridgeDir, 'build-upstream.mjs'), { force: true });

writeFileSync(
  path.join(upstreamDir, 'pnpm-workspace.yaml'),
  'packages:\n  - "packages/*"\n  - "bridge"\n',
);
writeFileSync(
  path.join(bridgeDir, 'package.json'),
  JSON.stringify(
    {
      name: 'ctox-easy-email-bridge-build',
      private: true,
      type: 'module',
      dependencies: {
        '@arco-design/web-react': '^2.36.1',
        'easy-email-core': 'workspace:*',
        'easy-email-editor': 'workspace:*',
        'easy-email-extensions': 'workspace:*',
        lodash: '^4.17.21',
        'mjml-browser': '^4.10.4',
        react: '18.2.0',
        'react-dom': '18.2.0',
        'react-final-form': '^6.5.7',
      },
      devDependencies: {
        '@vitejs/plugin-react': '^4.3.4',
        less: '^4.2.1',
        sass: '^1.83.0',
        typescript: '^5.7.2',
        vite: '^5.4.11',
      },
    },
    null,
    2,
  ) + '\n',
);

run('pnpm', ['install', '--ignore-scripts', '--frozen-lockfile=false'], upstreamDir);
run('pnpm', ['exec', 'vite', 'build', '--config', 'vite.config.mjs'], bridgeDir);

const distDir = path.join(bridgeDir, 'dist');
if (!existsSync(path.join(distDir, 'frame.html'))) throw new Error('Vite did not emit frame.html');
rmSync(bundleDir, { recursive: true, force: true });
cpSync(distDir, bundleDir, { recursive: true });
writeFileSync(
  path.join(bundleDir, 'BUILD.json'),
  JSON.stringify(
    {
      upstream_repository: REPOSITORY,
      upstream_commit: PIN,
      upstream_version: '4.17.1',
      built_at: new Date().toISOString(),
      entry: 'frame.html',
      files: ['frame.html'],
    },
    null,
    2,
  ) + '\n',
);

if (!suppliedUpstream) rmSync(workRoot, { recursive: true, force: true });
console.log(`Built Easy Email ${PIN} into ${bundleDir}`);
