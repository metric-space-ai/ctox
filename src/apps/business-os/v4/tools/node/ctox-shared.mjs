// Eine gemeinsame Node-Auflösung für Tests: `ctox:shared/<datei>` → <business-os>/shared/<datei>,
// wie der Service Worker im Browser und der AppRuntimeService im Daemon (§16.15, §16.36 Nr. 3).
// Nutzung (vor dem Laden jedes Moduls, auch statischer Importe):
//   node --import ./src/apps/business-os/v4/tools/node/ctox-shared.mjs --test <dateien>
// Apps und Tests legen keine eigenen Lader an.
import { registerHooks } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

const SHARED = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../shared');

registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('ctox:shared/')) {
      const rel = specifier.slice('ctox:shared/'.length);
      if (rel.split('/').includes('..')) throw new Error(`ctox:shared: unzulässiger Pfad ${rel}`);
      return { url: pathToFileURL(path.join(SHARED, rel)).href, shortCircuit: true };
    }
    return next(specifier, context);
  },
});
