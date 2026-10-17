// Shell v4 – Glossar des Rahmens: deutsche Standardbegriffe für Rohwerte, die aus Manifesten und Daten kommen.
//
// Rohwerte (manifest layout.category, App-Titel aus Manifesten, Statuscodes) bleiben in den Daten englisch und
// klein; SICHTBAR wird in deutscher Oberfläche nur der Begriff aus diesem Glossar. Unbekannte Werte gehen
// unverändert durch (kein Raten). Der Wächter v4/tools/guards/guard-v4.mjs (Regel english-raw-term) liest dieselben
// Tabellen und meldet englische Rohwerte in sichtbaren Texten.
//
//   categoryLabel(raw, lang?)  'Analytics' | 'analytics' → 'Analyse'
//   statusLabel(raw, lang?)    'open' | 'Open' | 'in_progress' → 'Offen' | 'In Bearbeitung'
//   appTitle(raw, lang?)       'Conversations' → 'Konversationen' (nur ganze, bekannte Titel)
//   rawKey(raw)                Normalform: klein, Leerraum/-/_ → ' '

export const CATEGORIES = Object.freeze({
  analytics: 'Analyse',
  collaboration: 'Zusammenarbeit',
  communication: 'Kommunikation',
  development: 'Entwicklung',
  finance: 'Finanzen',
  governance: 'Steuerung',
  knowledge: 'Wissen',
  marketing: 'Marketing',
  operations: 'Betrieb',
  productivity: 'Produktivität',
  recruiting: 'Personalgewinnung',
  research: 'Recherche',
  sales: 'Vertrieb',
  security: 'Sicherheit',
  system: 'System',
  workspace: 'Arbeitsplatz',
  office: 'Büro',
  administration: 'Verwaltung',
  admin: 'Verwaltung',
  other: 'Weitere',
  misc: 'Weitere',
  tools: 'Werkzeuge',
  utilities: 'Werkzeuge',
});

export const STATUSES = Object.freeze({
  new: 'Neu',
  open: 'Offen',
  opened: 'Geöffnet',
  pending: 'Wartend',
  waiting: 'Wartet',
  on_hold: 'Angehalten',
  hold: 'Angehalten',
  in_progress: 'In Bearbeitung',
  running: 'Läuft',
  queued: 'In Warteschlange',
  scheduled: 'Geplant',
  active: 'Aktiv',
  inactive: 'Inaktiv',
  enabled: 'Aktiviert',
  disabled: 'Deaktiviert',
  paused: 'Pausiert',
  draft: 'Entwurf',
  ready: 'Bereit',
  review: 'Prüfung',
  in_review: 'In Prüfung',
  approved: 'Freigegeben',
  accepted: 'Angenommen',
  rejected: 'Abgelehnt',
  declined: 'Abgelehnt',
  blocked: 'Blockiert',
  done: 'Erledigt',
  completed: 'Abgeschlossen',
  complete: 'Abgeschlossen',
  closed: 'Geschlossen',
  solved: 'Gelöst',
  resolved: 'Gelöst',
  failed: 'Fehlgeschlagen',
  error: 'Fehler',
  cancelled: 'Abgebrochen',
  canceled: 'Abgebrochen',
  archived: 'Archiviert',
  deleted: 'Gelöscht',
  expired: 'Abgelaufen',
  sent: 'Gesendet',
  delivered: 'Zugestellt',
  received: 'Empfangen',
  read: 'Gelesen',
  unread: 'Ungelesen',
  snoozed: 'Zurückgestellt',
  won: 'Gewonnen',
  lost: 'Verloren',
  unknown: 'Unbekannt',
  writeback_pending: 'Rückschreiben ausstehend',
  healthy: 'Gesund',
  at_risk: 'Gefährdet',
  churned: 'Abgewandert',
});

// Nur ganze, eindeutig englische App-Titel aus den Manifesten (v4/manifests). Produkt- und Eigennamen
// (Mail, Browser, Support, Tickets, IoT, Matching, Outbound) bleiben.
export const APP_TITLES = Object.freeze({
  conversations: 'Konversationen',
  knowledge: 'Wissen',
  news: 'Nachrichten',
  notes: 'Notizen',
  'bugs & features': 'Fehler & Wünsche',
  'coding agents': 'Coding-Agenten',
  credentials: 'Zugangsdaten',
  'web research': 'Webrecherche',
  'decision hub': 'Entscheidungen',
  'penetration testing': 'Sicherheitstests',
  'app importer': 'App-Import',
  'cv print builder': 'Lebenslauf-Druck',
});

export function rawKey(raw) {
  return String(raw ?? '').normalize('NFKC').trim().toLocaleLowerCase('en').replace(/[\s_-]+/g, ' ');
}

const keyed = (table) => new Map(Object.entries(table).map(([k, v]) => [rawKey(k), v]));
const CAT = keyed(CATEGORIES);
const STAT = keyed(STATUSES);
const TITLE = keyed(APP_TITLES);

const german = (lang) => !lang || String(lang).toLowerCase().startsWith('de');

function lookup(map, raw, lang) {
  const text = String(raw ?? '').trim();
  if (!text || !german(lang)) return text;
  return map.get(rawKey(text)) ?? text;
}

export const categoryLabel = (raw, lang) => lookup(CAT, raw, lang);
export const statusLabel = (raw, lang) => lookup(STAT, raw, lang);
export const appTitle = (raw, lang) => lookup(TITLE, raw, lang);

/** Englische Rohwerte (normalisiert) aller Tabellen; Grundlage der Wächter-Regel english-raw-term. */
export function englishRawTerms() {
  return new Set([...CAT.keys(), ...STAT.keys(), ...TITLE.keys()]
    // Wörter, die im Deutschen identisch sind, sind kein englischer Rohwert.
    .filter((k) => rawKey(CAT.get(k) ?? STAT.get(k) ?? TITLE.get(k)) !== k));
}
