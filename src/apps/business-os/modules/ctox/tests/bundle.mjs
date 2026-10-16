// index.js für Node-Tests bündeln: ctox:shared/x.js wie der Service Worker (§16.15) auf shared/
// abbilden (native Importe kennen das Schema ctox: nicht, OFFEN-F).
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { join as joinPath } from 'node:path';
// Lader (OLDTEST): esbuild aus dem Klon, sonst ESBUILD_MODULE, sonst aus dem Haupt-Checkout
// ~/Documents/ctox/src/apps/business-os/node_modules (wie PLAYWRIGHT_MODULE in den v4-api-Tests). Nicht installieren.
const requireDev = createRequire(import.meta.url);
function loadDev(name, envVar) {
  try { return requireDev(name); } catch {
    return requireDev(process.env[envVar] || joinPath(homedir(), 'Documents/ctox/src/apps/business-os/node_modules', name));
  }
}
const { build } = loadDev('esbuild', 'ESBUILD_MODULE');

export async function importCtoxBundle(relativePath = '../index.js') {
  const bundled = await build({
    entryPoints: [fileURLToPath(new URL(relativePath, import.meta.url))],
    bundle: true,
    format: 'esm',
    platform: 'browser',
    write: false,
    logLevel: 'silent',
    plugins: [{
      name: 'ctox-shared',
      setup(b) {
        b.onResolve({ filter: /^ctox:shared\// }, (args) => ({ path: fileURLToPath(new URL(`../../../shared/${args.path.slice('ctox:shared/'.length)}`, import.meta.url)) }));
      },
    }],
  });
  const [{ text }] = bundled.outputFiles;
  return import(`data:text/javascript;base64,${Buffer.from(text).toString('base64')}`);
}
