const theme = document.querySelector('[data-lab-theme]');
const width = document.querySelector('[data-lab-width]');
const frame = document.querySelector('[data-lab-frame]');

const messages = {
  de: {
    title: 'Operatives Instrument · Design Lab', register: 'Register', records: 'Vorgänge', search: 'Suchen…',
    filter: 'Filter', reviewRunning: 'Prüfung läuft', ready: 'Bereit', draft: 'Entwurf', import: 'Import',
    sort: 'Sortieren', edit: 'Bearbeiten', synced: 'Synchron', startAutomation: 'Automatisierung starten',
    principle: 'Routinearbeit bleibt ruhig; die Automation ist der klare Kontrast.', field: 'Feld', value: 'Wert',
    dataReview: 'Datenprüfung', open: 'Offen', approval: 'Freigabe', done: 'Fertig', scheduled: 'Geplant',
    formStates: 'Formularzustände', name: 'Bezeichnung',
  },
  en: {
    title: 'Operational Instrument · Design Lab', register: 'Register', records: 'Records', search: 'Search…',
    filter: 'Filter', reviewRunning: 'Review running', ready: 'Ready', draft: 'Draft', import: 'Import',
    sort: 'Sort', edit: 'Edit', synced: 'Synced', startAutomation: 'Start automation',
    principle: 'Routine work stays quiet; automation provides the clear contrast.', field: 'Field', value: 'Value',
    dataReview: 'Data review', open: 'Open', approval: 'Approval', done: 'Done', scheduled: 'Scheduled',
    formStates: 'Form states', name: 'Name',
  },
};

theme?.addEventListener('change', () => {
  document.documentElement.dataset.theme = theme.value === 'light' ? 'light' : 'dark';
});

width?.addEventListener('change', () => {
  frame.style.width = `${Number.parseInt(width.value, 10) || 960}px`;
});

frame.style.width = `${Number.parseInt(width?.value, 10) || 960}px`;

export function renderDesignLabLocale(locale = document.documentElement.lang) {
  const normalized = String(locale || 'de').toLowerCase().startsWith('en') ? 'en' : 'de';
  document.documentElement.lang = normalized;
  for (const node of document.querySelectorAll('[data-i18n]')) {
    const value = messages[normalized][node.dataset.i18n];
    if (value) node.textContent = value;
  }
  for (const node of document.querySelectorAll('[data-i18n-placeholder]')) {
    const value = messages[normalized][node.dataset.i18nPlaceholder];
    if (value) node.setAttribute('placeholder', value);
  }
  for (const node of document.querySelectorAll('[data-i18n-aria-label]')) {
    const value = messages[normalized][node.dataset.i18nAriaLabel];
    if (value) node.setAttribute('aria-label', value);
  }
  document.documentElement.dataset.designLabLocale = normalized;
}

globalThis.renderDesignLabLocale = renderDesignLabLocale;
renderDesignLabLocale();
