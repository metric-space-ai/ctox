// Firmen-Import für outbound.source.import als Browser-SCHRITTE (Strang OLG725, Kern §16.47/§16.48 Nr. 2).
//
// Vorher: wörtliches Playwright-Skript aus importer.rs:2055-2131 (OUTBOUND_COMPANY_BROWSER_SCRIPT), ausgeführt über
// browser.automation.run({source}). Der Kern nimmt keinen Skript-Quelltext mehr an (CAP-3d, #844 ebddaa2e8); erlaubt
// sind nur Schritte {op, tab_id?, params}, alle vorab geprüft, Abbruch beim ersten Fehler, höchstens 32 je Aufruf.
// Das Ergebnis ist ein files-Datensatz der App (`artifact.ref`) mit {session_id, steps:[{op, ok, result?, code?}]}.
// Neue Schritte nach §16.48 Nr. 2 (Kern baut sie noch):
//   {op:'extract', params:{selector, fields:{<name>:{selector, attr?|text:true}}, limit}} → Ergebnis-JSON ≤ 1 MiB
//   {op:'click',   params:{selector}}
//   {op:'wait',    params:{selector} | {ms ≤ 30000}}
// Abbildung des alten Skripts (ref: importer.rs:2055-2131):
//   page.goto(importUrl) + waitForTimeout(1800)        → navigate + wait {ms:1800}
//   collectVisible (a.teaser, innerText-Zeilen, href)   → extract a.teaser {text:{selector:':scope', text:true},
//                                                          href:{selector:':scope', attr:'href'}}; die Zeilenauswertung
//                                                          (Land, Messe, Name, Stand, Beschreibung) ist App-Logik unten
//   clickSortCharacter(B…Z) + waitForTimeout(1800)      → click `.search-filters-sorting-characters a[data-value="X"]`
//                                                          + wait {ms:1800} + extract; fehlt ein Buchstabe, scheitert
//                                                          dessen click → dieser Buchstabe wird übersprungen (wie
//                                                          `if (!clicked) continue`), es geht mit dem nächsten weiter
//   clickGermanyFilter (Link per TEXT „Deutschland“)    → nicht als Selektor ausdrückbar (kein Textselektor); der
//                                                          Länderfilter greift stattdessen auf die Länderzeile jeder
//                                                          Zeile (das alte Skript filterte zusätzlich genau so)
// Annahmen, die der Kern mit den neuen Schritten bestätigen muss (OLG725.md): `:scope` meint das gefundene Element,
// `text:true` liefert innerText (Zeilenumbrüche erhalten), das extract-Ergebnis ist {items:[{<feld>: string|null}]}.

export const MAX_ROWS = 100;
export const MAX_STEPS = 32;
export const SETTLE_MS = 1800;
export const SORT_LETTERS = ['B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z'];
export const STEPS_PER_LETTER = 3;
export const LETTERS_PER_RUN = Math.floor(MAX_STEPS / STEPS_PER_LETTER);

export const TEASER_EXTRACT = Object.freeze({
  op: 'extract',
  params: {
    selector: 'a.teaser',
    fields: { text: { selector: ':scope', text: true }, href: { selector: ':scope', attr: 'href' } },
    limit: MAX_ROWS,
  },
});

/** Erster Aufruf: Seite öffnen, Laden abwarten, sichtbare Teaser lesen. */
export function openSteps(url) {
  return [{ op: 'navigate', params: { url } }, { op: 'wait', params: { ms: SETTLE_MS } }, TEASER_EXTRACT];
}

/** Buchstaben-Seitenschaltung: je Buchstabe click + wait + extract. */
export function letterSteps(letters) {
  return letters.flatMap((letter) => [
    { op: 'click', params: { selector: `.search-filters-sorting-characters a[data-value="${letter}"]` } },
    { op: 'wait', params: { ms: SETTLE_MS } },
    TEASER_EXTRACT,
  ]);
}

const normalize = (text) => String(text || '').replace(/\s+/g, ' ').trim();

/** ref: importer.rs collectVisible – eine Teaser-Zeile aus innerText-Zeilen und href. */
export function teaserRow(item) {
  const lines = String(item?.text || '').split(/\n+/).map((line) => normalize(line)).filter(Boolean);
  const countryIndex = lines.findIndex((line) => /^(Deutschland|Germany)$/i.test(line));
  const event = countryIndex >= 2 ? lines[countryIndex - 1] : '';
  const name = countryIndex >= 2 ? lines[countryIndex - 2] : (lines[1] || lines[0] || '');
  const booth = countryIndex >= 2 ? lines.slice(0, countryIndex - 2).join(', ') : '';
  // Abweichung vom alten Skript: dort war die Seite per Klick schon auf Deutschland gefiltert, eine Zeile ohne
  // Länderzeile galt deshalb als deutsch. Ohne den Klick (kein Textselektor) braucht der Filter die Länderzeile.
  const country = countryIndex >= 0 ? lines[countryIndex] : '';
  const description = countryIndex >= 0 ? lines.slice(countryIndex + 1).join(' ') : lines.slice(2).join(' ');
  const href = typeof item?.href === 'string' ? item.href : '';
  return { name, company_name: name, booth, event, country, description, href, website: href };
}

/** Zeilen eines extract-Ergebnisses; Länderfilter wie das alte Skript (Zeile mit Land Deutschland/Germany). */
export function teaserRows(result, onlyGermany) {
  const items = Array.isArray(result?.items) ? result.items : Array.isArray(result) ? result : [];
  return items.map((item) => teaserRow(item))
    .filter((row) => row.name && (!onlyGermany || /^(Deutschland|Germany)$/i.test(row.country)));
}
