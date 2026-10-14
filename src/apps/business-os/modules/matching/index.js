const MATCHING_BUILD = '20260831-shell-v2-single-view-toggle';

export async function mount(ctx) {
  await ensureStyles();
  const dataSource = await import('./ui/businessOsDataSource.js');
  dataSource.setBusinessOsDatabaseContext?.(ctx);
  ctx.host.innerHTML = await loadModuleMarkup();
  ctx.host.dataset.matchingModule = 'native';
  wireViewToggles(ctx.host);
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();

  let disposed = false;
  const dashboardStartup = (async () => {
    if (ctx.matchingDefinition || globalThis.CTOX_MATCHING_DEFINITION) {
      const definitionModule = await import('./ui/matchingDefinition.js');
      if (disposed) return;
      definitionModule.setActiveMatchingDefinition?.(ctx.matchingDefinition || globalThis.CTOX_MATCHING_DEFINITION);
    }
    const controls = await import(`./ui/businessOsControls.js?v=${MATCHING_BUILD}`);
    controls.setBusinessOsRuntimeContext?.(ctx);
    if (disposed) return;
    const matchingUi = await import(`./ui/index.js?v=${MATCHING_BUILD}`);
    if (disposed) return;
    await matchingUi.mountMatchingDashboard?.(ctx);
  })().catch((error) => {
    if (disposed) return;
    console.warn('[matching] dashboard startup failed:', error);
    ctx.notifications?.show?.({
      type: 'error',
      title: 'Matching hat nicht geklappt.',
      time: 9000
    });
  });

  return () => {
    disposed = true;
    try { window.teardownRxdbLiveUiSync?.(); } catch {}
    dashboardStartup.catch(() => {});
    ctx.host.replaceChildren();
    delete ctx.host.dataset.matchingModule;
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

function wireViewToggles(host) {
  for (const button of host.querySelectorAll('[data-matching-view-toggle]')) {
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
    apply(pane?.dataset.pgDefaultView || button.dataset.matchingView || 'cards');
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
  }
}

async function ensureStyles() {
  const cssVersion = String(import.meta.url).split('?v=')[1] || MATCHING_BUILD;
  const cssHref = new URL('./index.css', import.meta.url).pathname + (cssVersion ? `?v=${cssVersion}` : '');
  let link = document.querySelector('link[data-matching-style]');
  if (!link) {
    link = document.createElement('link');
    link.rel = 'stylesheet';
    link.dataset.matchingStyle = 'true';
    document.head.append(link);
  }
  if (link.getAttribute('href') !== cssHref) link.href = cssHref;
}

async function loadModuleMarkup() {
  const markupVersion = String(import.meta.url).split('?v=')[1] || MATCHING_BUILD;
  const markupHref = new URL('./index.html', import.meta.url).pathname + (markupVersion ? `?v=${markupVersion}` : '');
  const html = await fetch(markupHref).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}
