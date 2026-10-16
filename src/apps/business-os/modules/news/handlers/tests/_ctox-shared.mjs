// Node-Aufloesung `ctox:shared/<datei>` → <business-os>/shared/<datei> (wie Service Worker/Harness, §16.15/§16.34).
// Handler-Module erst NACH diesem Import dynamisch laden (statische Importe werden vor der Registrierung gebunden).
import { registerHooks } from 'node:module';
import { pathToFileURL, fileURLToPath } from 'node:url';
import path from 'node:path';

const SHARED = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../../shared');
registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('ctox:shared/')) {
      return { url: pathToFileURL(path.join(SHARED, specifier.slice('ctox:shared/'.length))).href, shortCircuit: true };
    }
    return next(specifier, context);
  },
});

export const APP_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
