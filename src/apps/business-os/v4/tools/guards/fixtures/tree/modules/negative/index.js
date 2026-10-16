// Negativfaelle: nichts hiervon darf zaehlen.
// ctx.sync.startCollection('x'); commandBus; location.reload(); setInterval(); fetch('/api/x')
/* Daten werden synchronisiert. Loading workspace. Stand unbestätigt. ?v=1
   OFFLINE_FALLBACK_CATALOG wireShellV2ModuleTitle createDynamicAppDataGuard until: 'accepted' */
import { helper } from './helper.js';

export function mount(root, api) {
  // Zustandstexte nur ueber die Zustandszone:
  api.status.report({ level: 'info', text: 'Daten werden synchronisiert.' });
  api.status.report({ level: 'warn', text: `Stand unbestätigt (${api.actor.id})` });
  api.status.clear('Loading workspace');
  console.warn('[demo] Loading workspace, syncing data', { reason: 'not ready' });
  // Schluessel- und Codeform zaehlt nicht:
  const state = { phase: 'syncing', key: 'sync.loading', i18n: 'loadingModule', mode: 'read-only' };
  root.innerHTML = '<div class="ctox-syncing app-loading" data-read-only="true"></div>';
  // Familien an falscher Wortgrenze:
  releaseModule(root);
  requestNativeProviders();
  // Regex-Literal mit Anfuehrungszeichen und // darf den Lexer nicht verwirren:
  const re = /["'`]\/\/startCollection/g;
  const half = 10 / 2; const other = half / 5;
  // Deklarierte UI-Animation:
  // v4-allow: ui-animation
  const blink = setInterval(() => root.classList.toggle('on'), 500);
  const tick = setInterval(() => {}, 16); // v4-allow: ui-animation
  // Relative und app-lokale Ladewege:
  fetch('./index.html');
  fetch(api.shell.assetUrl('icon.svg'));
  fetch(`${base}/data.json`);
  // String-Inhalte, die nur wie Bezeichner aussehen:
  const label = 'commandBus startCollection';
  const until = { until: 'local' };
  return { unmount() { clearInterval(blink); clearInterval(tick); }, snapshot() { return {}; } };
}
