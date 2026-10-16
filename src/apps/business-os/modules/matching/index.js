// matching – App-API 4.0 (V4-PORT.md). Einstieg mount(root, api, { restored }) → { unmount, snapshot }.
import { setMatchingDataApi, disposeMatchingData } from './ui/businessOsDataSource.js';
import { bindMatchingApi, disposeMatchingRuntime, onMatchingUnmount, snapshotMatchingStore } from './ui/v4Runtime.js';
import { initBusinessOsControls } from './ui/businessOsControls.js';
import { mountMatchingDashboard, snapshotMatchingDrafts } from './ui/index.js';
import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js';

const MATCHING_COLLECTIONS = ['matching_requirements', 'matching_objects', 'matching_results'];

export async function mount(root, api, { restored } = {}) {
  if (!root.dataset.app) root.dataset.app = 'matching';
  bindMatchingApi(api, { restored });
  api.shell.setTitle('Matching');
  api.setVisible(MATCHING_COLLECTIONS);
  setMatchingDataApi(api);
  onMatchingUnmount(disposeMatchingData);

  root.innerHTML = await loadModuleMarkup();
  root.dataset.matchingModule = 'native';
  wireViewToggles(root, restored);
  autoWirePaneGrammar(root);

  try {
    initBusinessOsControls({ host: root });
    await mountMatchingDashboard({ host: root, restored });
  } catch (error) {
    console.error('[matching] dashboard startup failed:', error, error?.stack);
    api.status.report({ level: 'error', code: 'matching.mount', text: `Matching: ${String(error?.message || error)}` });
  }

  let unmounted = false;
  return {
    unmount() {
      if (unmounted) return;
      unmounted = true;
      disposeMatchingRuntime();
      root.replaceChildren();
      delete root.dataset.matchingModule;
    },
    snapshot() {
      return {
        v: 1,
        store: snapshotMatchingStore(),
        views: Object.fromEntries([...root.querySelectorAll('[data-matching-view-toggle]')]
          .map((button, index) => [String(index), button.dataset.matchingView === 'list' ? 'list' : 'cards'])),
        drafts: snapshotMatchingDrafts(),
      };
    },
  };
}

// Ein-Knopf-Umschalter (Betreiber-Direktive 31.08.2026): die Darstellung wird
// von EINEM Knopf getauscht, nicht von zwei nebeneinanderliegenden Zustaenden.
// Der Knopf ist eine Aktion, kein Zustand - deshalb kein aria-pressed, und das
// Icon zeigt die Ansicht, zu der gewechselt wird.
//
// shared/pane-grammar.js leitet den View-Zustand aus [data-pg-view] +
// aria-pressed ab; genau dieses Zwei-Knopf-Muster faellt hier weg. Statt die
// Shell zu aendern, traegt der Umschalter den Zustand in
// `pane.dataset.pgDefaultView`, den die Grammatik als Rueckfallwert liest. So
// bleibt jedes Grammatik-Ereignis (Suche, Filter, Band) beim aktuellen View,
// und die App bleibt die einzige Besitzerin des Umschaltens.
const VIEW_TOGGLE_LABELS = {
  cards: { label: 'Als Liste anzeigen', title: 'Als Liste anzeigen' },
  list: { label: 'Als Karten anzeigen', title: 'Als Karten anzeigen' }
};

function wireViewToggles(host, restored) {
  const savedViews = restored && typeof restored === 'object' && restored.views && typeof restored.views === 'object' ? restored.views : {};
  [...host.querySelectorAll('[data-matching-view-toggle]')].forEach((button, index) => {
    const pane = button.closest('.ctox-pane');
    const apply = (view) => {
      const next = view === 'list' ? 'list' : 'cards';
      button.dataset.matchingView = next;
      if (pane) {
        pane.dataset.pgDefaultView = next;
        pane.dataset.matchingView = next;
      }
      const copy = VIEW_TOGGLE_LABELS[next];
      button.setAttribute('aria-label', copy.label);
      button.title = copy.title;
      button.removeAttribute('aria-pressed');
    };
    apply(savedViews[String(index)] || pane?.dataset.pgDefaultView || button.dataset.matchingView || 'cards');
    button.addEventListener('click', () => {
      apply(button.dataset.matchingView === 'list' ? 'cards' : 'list');
      if (!pane) return;
      // Dieselbe Meldung, die die Pane-Grammatik fuer Suche/Filter/Band sendet:
      // die App rendert auf genau einem Pfad neu.
      const detail = pane.__ctoxPaneGrammar?.state?.() || { view: pane.dataset.pgDefaultView };
      try {
        pane.dispatchEvent(new CustomEvent('ctox-pane-grammar-change', { detail, bubbles: true }));
      } catch {}
    });
  });
}

// Markup relativ zur App, ohne Cache-Buster (der Service Worker liefert versioniert, §16.15).
// CSS lädt die Shell aus dem Manifest.
async function loadModuleMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}
