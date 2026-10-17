// Node-Testhilfe (wie modules/app-store/tests/ctox-shared-loader.mjs, PORTING §4 Falle 2): bildet das Umschreiben des
// Service Workers nach, `ctox:shared/x.js` → `<business-os>/shared/x.js`. Nur für Tests; die App lädt `ctox:shared/` über
// den SW. Nutzung: `import './ctox-shared-loader.mjs';` vor den dynamischen Importen der Handler (outbound.campaign.setup.request.js
// und outbound.pipeline.outreach_draft.js importieren ctox:shared/prompt-template.js).
import { register } from 'node:module';

const SHARED = new URL('../../../../shared/', import.meta.url).href;
const hooks = `
export async function resolve(specifier, context, next) {
  if (specifier.startsWith('ctox:shared/')) return { url: ${JSON.stringify(SHARED)} + specifier.slice('ctox:shared/'.length), shortCircuit: true };
  return next(specifier, context);
}`;
register(`data:text/javascript,${encodeURIComponent(hooks)}`);
