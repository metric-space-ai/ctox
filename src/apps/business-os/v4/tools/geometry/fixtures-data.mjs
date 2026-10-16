// Shell v4 Geometrie-Labor – gemeinsamer deutscher Mittelstands-Datensatz (Strang SEED).
// Eine Welt für alle Apps: dieselben 12 Firmen, ihre Ansprechpartner mit passenden Rollen, glaubwürdige Beträge,
// Mail-Betreffe mit Absendern. fixtures.mjs bildet daraus die Zeilen je Collection; Zeile i einer Collection gehört
// (sofern sie auf eine Firma verweist) zu genau der Firma, auf die ihr Verweis zeigt – Name, Domain, Mail passen.
//
// Domains sind ASCII-transliteriert (ü → ue, ß → ss), nie verstümmelt: „Müller Maschinenbau“ → mueller-maschinenbau.de.

export const translit = (s) => String(s).toLowerCase()
  .replace(/ä/g, 'ae').replace(/ö/g, 'oe').replace(/ü/g, 'ue').replace(/ß/g, 'ss')
  .replace(/é|è/g, 'e').replace(/&/g, 'und');

// name, Kurzname, Domain, Ort, Straße, PLZ, Branche, Mitarbeitende, Jahresumsatz mit uns (ARR, Euro)
export const COMPANIES = [
  { name: 'Müller Maschinenbau GmbH', short: 'Müller Maschinenbau', domain: 'mueller-maschinenbau.de', city: 'Augsburg', street: 'Industriestraße 12', zip: '86167', industry: 'Maschinenbau', employees: 240, arr: 86400 },
  { name: 'Schneider Logistik AG', short: 'Schneider Logistik', domain: 'schneider-logistik.de', city: 'Hamburg', street: 'Am Hafen 3', zip: '20457', industry: 'Logistik', employees: 1150, arr: 214000 },
  { name: 'Becker & Söhne KG', short: 'Becker & Söhne', domain: 'becker-soehne.de', city: 'Köln', street: 'Hauptstraße 5', zip: '50667', industry: 'Großhandel', employees: 85, arr: 32500 },
  { name: 'Hoffmann Medizintechnik GmbH', short: 'Hoffmann Medizintechnik', domain: 'hoffmann-medizintechnik.de', city: 'Tuttlingen', street: 'Bahnhofstraße 41', zip: '78532', industry: 'Medizintechnik', employees: 410, arr: 128000 },
  { name: 'Wagner Elektrotechnik GmbH', short: 'Wagner Elektrotechnik', domain: 'wagner-elektrotechnik.de', city: 'Nürnberg', street: 'Gewerbering 8', zip: '90451', industry: 'Elektrotechnik', employees: 160, arr: 54800 },
  { name: 'Fischer Software Solutions GmbH', short: 'Fischer Software', domain: 'fischer-software.de', city: 'Leipzig', street: 'Lindenallee 22', zip: '04109', industry: 'Software', employees: 60, arr: 27600 },
  { name: 'Krause Facility Services GmbH', short: 'Krause Facility', domain: 'krause-facility.de', city: 'Bremen', street: 'Hafenkante 17', zip: '28217', industry: 'Gebäudedienste', employees: 520, arr: 71200 },
  { name: 'Richter Bau & Planung GmbH', short: 'Richter Bau', domain: 'richter-bau.de', city: 'Stuttgart', street: 'Königstraße 30', zip: '70173', industry: 'Bauwesen', employees: 310, arr: 98500 },
  { name: 'Weiß Lebensmittel GmbH & Co. KG', short: 'Weiß Lebensmittel', domain: 'weiss-lebensmittel.de', city: 'Hannover', street: 'Messeweg 9', zip: '30521', industry: 'Lebensmittel', employees: 720, arr: 156000 },
  { name: 'Schäfer Automotive Zulieferer GmbH', short: 'Schäfer Automotive', domain: 'schaefer-automotive.de', city: 'Ingolstadt', street: 'Ettinger Straße 104', zip: '85057', industry: 'Automobilzulieferer', employees: 1900, arr: 342000 },
  { name: 'Neumann Energie GmbH', short: 'Neumann Energie', domain: 'neumann-energie.de', city: 'Dortmund', street: 'Rheinlanddamm 24', zip: '44139', industry: 'Energie', employees: 380, arr: 118000 },
  { name: 'Zimmermann Verlag GmbH', short: 'Zimmermann Verlag', domain: 'zimmermann-verlag.de', city: 'München', street: 'Leopoldstraße 51', zip: '80802', industry: 'Verlagswesen', employees: 95, arr: 19800 },
];

// Ansprechpartner: zwei je Firma (Zeile k → Firma k % 12), Rolle passend zur Funktion.
export const CONTACTS = [
  ['Anna', 'Schmidt', 'Leiterin Einkauf'], ['Lukas', 'Weber', 'Geschäftsführer'], ['Sophie', 'Meyer', 'Head of Operations'],
  ['Jonas', 'Wagner', 'IT-Leiter'], ['Marie', 'Becker', 'Kaufmännische Leiterin'], ['Felix', 'Hoffmann', 'Vertriebsleiter'],
  ['Lena', 'Schulz', 'Projektleiterin'], ['Paul', 'Koch', 'Leiter Finanzen'], ['Julia', 'Bauer', 'Personalleiterin'],
  ['Maximilian', 'Krüger', 'Technischer Leiter'], ['Laura', 'Wolf', 'Assistentin der Geschäftsführung'], ['Tobias', 'Schröder', 'Qualitätsmanager'],
  ['Katharina', 'Lehmann', 'Leiterin Marketing'], ['Florian', 'Hartmann', 'Werkleiter'], ['Sarah', 'König', 'Controllerin'],
  ['David', 'Lange', 'Einkäufer'], ['Johanna', 'Peters', 'Prokuristin'], ['Niklas', 'Jung', 'Leiter Instandhaltung'],
  ['Hannah', 'Möller', 'Buchhalterin'], ['Moritz', 'Schmitz', 'Key Account Manager'], ['Clara', 'Werner', 'Leiterin Recht'],
  ['Simon', 'Frank', 'Logistikleiter'], ['Emma', 'Braun', 'Teamleiterin Kundenservice'], ['Benedikt', 'Vogel', 'Geschäftsführer'],
].map(([first, last, role], k) => ({ first, last, role, company: k % COMPANIES.length }));

// Interne Mitarbeitende (Absender eigener Mails, Zuständige, Bearbeitende).
export const STAFF = [
  ['Tanja', 'Albrecht', 'Account Managerin'], ['Holger', 'Brandt', 'Vertriebsleiter'], ['Jule', 'Engel', 'Projektmanagerin'],
  ['Martin', 'Seidel', 'Buchhaltung'], ['Nina', 'Kaiser', 'Kundenservice'], ['Oliver', 'Roth', 'Technik'],
];
export const OWN_DOMAIN = 'havel-software.de';

export const personEmail = (p, domain) => `${translit(p.first)}.${translit(p.last)}@${domain}`;
export const contactOf = (k) => CONTACTS[((k % CONTACTS.length) + CONTACTS.length) % CONTACTS.length];
export const companyOf = (k) => COMPANIES[((k % COMPANIES.length) + COMPANIES.length) % COMPANIES.length];
// Erster Ansprechpartner einer Firma (Zeile k der Firma → CONTACTS[k] oder CONTACTS[k + 12]).
export const contactForCompany = (c, alt = 0) => CONTACTS[(c % COMPANIES.length) + (alt % 2) * COMPANIES.length];

export const MAIL_SUBJECTS = [
  'Angebot Wartungsvertrag Halle 4', 'Rückfrage zur Rechnung RE-2026-1043', 'Terminbestätigung Abnahme vor Ort',
  'Lieferverzug Ersatzteile – neue Liefertermine', 'Rahmenvertrag 2027: Entwurf zur Durchsicht', 'Protokoll Kickoff Kundenportal',
  'Preisanpassung ab 01.01.2027', 'Einladung Quartalsgespräch Q4', 'Reklamation Lieferung KW 41', 'Zugangsdaten Kundenportal – Rückfrage',
  'Bestellung 4500018832', 'Freigabe Projektplan Phase 2',
];
export const MAIL_BODIES = [
  'Guten Tag,\n\nanbei erhalten Sie unser überarbeitetes Angebot. Bitte geben Sie uns bis Freitag Bescheid, ob wir so verfahren können.\n\nViele Grüße',
  'Hallo zusammen,\n\nwir haben die Rechnung geprüft und eine Rückfrage zu Position 3. Können Sie uns die Stundenaufstellung nachreichen?\n\nBeste Grüße',
  'Sehr geehrte Damen und Herren,\n\nhiermit bestätigen wir den Termin zur Abnahme am Dienstag um 10:00 Uhr vor Ort.\n\nMit freundlichen Grüßen',
  'Hallo,\n\nleider verzögert sich die Lieferung der Ersatzteile um etwa zwei Wochen. Die neuen Termine finden Sie in der Anlage.\n\nGruß',
];

export const TITLES = ['Quartalsbericht Q3 2026', 'Angebot Wartungsvertrag Halle 4', 'Projektplan Relaunch Kundenportal',
  'Lieferantenbewertung 2026', 'Onboarding neue Mitarbeitende', 'Rahmenvertrag über Wartung und Instandhaltung der Produktionsanlagen am Standort Augsburg',
  'Kundentermin Vorbereitung', 'Budgetplanung Vertrieb Süd', 'Messeplanung Hannover 2027', 'Jahresgespräch Lieferanten', 'Migrationskonzept ERP', 'Sicherheitsunterweisung Lager'];
export const SENTENCES = ['Bitte bis Freitag prüfen und freigeben.', 'Abstimmung mit dem Einkauf ist erfolgt, Unterlagen liegen vor.',
  'Rückfrage zur Lieferzeit der Ersatzteile, Antwort steht aus Sicht des Kunden noch offen.', 'Termin für die Abnahme vor Ort ist vereinbart.',
  'Zusammenfassung der Ergebnisse aus dem Workshop mit dem Fachbereich und nächste Schritte für das zweite Quartal.', 'Rechnung wurde an die Buchhaltung weitergeleitet.'];
export const TAGS = ['Kunde', 'Q4', 'Priorität', 'Vertrieb', 'Intern', 'Projekt'];
export const FILES = ['Angebot_Wartung_2026.pdf', 'Protokoll_Kickoff.docx', 'Preisliste_Q4.xlsx', 'Lageplan_Halle4.png', 'Vertrag_Rahmen_signiert.pdf', 'Notizen_Workshop.md'];

// Netto-Beträge in Euro für Belege/Rechnungen/Zahlungen (Hunderte bis Zehntausende), Zeile i → Betrag i.
export const NET_EUR = [4850, 12400, 980, 27650, 3215.5, 8740, 46200, 1560, 18900, 640, 9875, 33480];
export const VAT = 0.19;
// Positionspreise (Euro) für Rechnungs-/Angebotszeilen und Stundensätze
export const UNIT_EUR = [95, 1250, 48.5, 320, 12.9, 2400];

// Verkaufschancen/Projekte: „<Firma> – <Vorhaben>“
export const DEALS = ['Wartungsvertrag 2027', 'Lizenzerweiterung 50 Arbeitsplätze', 'Rollout Kundenportal', 'Rahmenvertrag Ersatzteile',
  'Schulungspaket Q1', 'Upgrade Enterprise-Paket'];
