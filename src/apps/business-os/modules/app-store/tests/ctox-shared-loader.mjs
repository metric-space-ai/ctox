// Node-Testhilfe (PORTING §4 Falle 2, OFFEN-F): bildet das Umschreiben des Service Workers nach,
// `ctox:shared/x.js` → `<business-os>/shared/x.js`. Nur für Tests; die App lädt `ctox:shared/` über den SW.
// Nutzung: `import './tests/ctox-shared-loader.mjs';` vor dem dynamischen Import von index.js.
import { register } from 'node:module';

const SHARED = new URL('../../../shared/', import.meta.url).href;
const hooks = `
export async function resolve(specifier, context, next) {
  if (specifier.startsWith('ctox:shared/')) return { url: ${JSON.stringify(SHARED)} + specifier.slice('ctox:shared/'.length), shortCircuit: true };
  return next(specifier, context);
}`;
register(`data:text/javascript,${encodeURIComponent(hooks)}`);
