import { collectionFreshnessFromDiagnostics } from './sync-contract.js?v=20261004-shell-v2-native-lease-badges-v449';

// A warning changes the interpretation of cached data, not its availability.
export function renderCollectionFreshnessWarning(warning, { collections = [], diagnostics, language = 'de' } = {}) {
  if (!warning) return;
  const pending = collections.map((collection) => collectionFreshnessFromDiagnostics(
    collection, diagnostics?.collections?.[collection], { syncMode: diagnostics?.mode },
  )).filter((entry) => entry.requiresPullConfirmation && !entry.ready);
  warning.hidden = pending.length === 0;
  if (!pending.length) {
    warning.textContent = '';
    return;
  }
  const offline = pending.some((entry) => entry.state === 'offline-pending');
  warning.textContent = language === 'de'
    ? (offline ? 'Offline: angezeigte Daten können veraltet sein' : 'Daten werden abgeglichen: angezeigter Stand noch nicht bestätigt')
    : (offline ? 'Offline: displayed data may be out of date' : 'Syncing data: displayed state is not yet confirmed');
}
