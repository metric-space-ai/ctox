// Excel-Export der Recherche (Owner 22.09.2026: "bisher kann man nur zu
// Sellify exportieren, hier sollte noch ein Download-Icon rein, um die
// jeweilige Recherche als gut formatiertes Excel herunterzuladen").
//
// Erzeugt eine echte .xlsx-Datei (Office Open XML) im Browser. JSZip kommt aus
// dem Business-OS-Vendor-Ordner und wird erst beim Export geladen; die App
// bleibt ohne Paketmanager und ohne Server-Umweg.

const STATUS_TEXT = Object.freeze({
  verified: 'belegt',
  no_match: 'kein Treffer',
  action_required: 'offen – Prüfung nötig',
  unsupported: 'nicht unterstützt',
  conflict: 'Widerspruch',
});

// Stil-Indizes in styles.xml (cellXfs).
const S = Object.freeze({
  normal: 0,
  header: 1,
  wrap: 2,
  link: 3,
  ok: 4,
  open: 5,
  none: 6,
  title: 7,
  label: 8,
  muted: 9,
  okStrong: 10,
});

function xmlEscape(value) {
  return String(value ?? '')
    // Steuerzeichen sind in XML 1.0 verboten und machen die Datei unlesbar.
    .replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F]/g, '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function columnName(index) {
  let name = '';
  let n = index + 1;
  while (n > 0) {
    const rest = (n - 1) % 26;
    name = String.fromCharCode(65 + rest) + name;
    n = Math.floor((n - 1) / 26);
  }
  return name;
}

function textOf(value) {
  if (value === null || value === undefined) return '';
  if (Array.isArray(value)) return value.map(textOf).filter(Boolean).join('; ');
  if (typeof value === 'object') {
    return textOf(value.value ?? value.email ?? value.address ?? value.name ?? '');
  }
  return String(value).trim();
}

// Sellify fuehrt Positionen mehrsprachig als `GE:"Geschaeftsfuehrung";US:"Managing
// director"`. Im Export steht die deutsche Fassung, sonst die erste.
export function lesbarerMehrsprachText(value) {
  const text = textOf(value);
  const teile = [...text.matchAll(/\b([A-Z]{2}):"([^"]*)"/g)];
  if (!teile.length) return text;
  const deutsch = teile.find(([, sprache]) => sprache === 'GE' || sprache === 'DE');
  return String((deutsch || teile[0])[2] || '').trim();
}

function hostOf(url) {
  try {
    return new URL(String(url)).hostname.replace(/^www\./, '');
  } catch {
    return String(url || '').trim();
  }
}

function isHttpUrl(value) {
  return /^https?:\/\//i.test(String(value || '').trim());
}

// Owner 23.09.2026: eine passende Quelle genuegt; mehr Quellen belegen
// staerker. Der Status nennt deshalb die Zahl der unabhaengigen Quellen.
function statusMitQuellen(field) {
  const text = STATUS_TEXT[field.status] || field.status || '';
  if (field.status !== 'verified') return text;
  const extern = Number(field.externalCount) || 0;
  const plusSellify = field.sellify ? ' + Sellify' : '';
  if (extern >= 2) return `stärker belegt · ${extern} externe Quellen${plusSellify}`;
  if (extern === 1) return `belegt · 1 externe Quelle${plusSellify}`;
  // Sellify allein belegt nichts (Owner 23.09.2026).
  return field.sellify ? 'unbelegt – nur Sellify' : 'unbelegt';
}

function feldStatusStil(field) {
  if (field.status === 'verified') {
    const extern = Number(field.externalCount) || 0;
    if (extern >= 2) return S.okStrong;
    if (extern === 1) return S.ok;
    return S.open;
  }
  return statusStyle(field.status);
}

// Sperrvermerk-Urteil der App je Kontakt: frei, gesperrt (mit Grund) oder zu
// pruefen. Ohne Urteil ausdruecklich "nicht geprueft" statt leer.
function empfaengerZelle(decision) {
  if (!decision) return { v: 'Sperrvermerk nicht geprüft', s: S.open };
  const status = String(decision.status || '');
  if (decision.pending) return { v: 'Sperrvermerk wird geprüft', s: S.open };
  if (status === 'free') return { v: 'frei', s: S.ok };
  const grund = textOf(decision.label || decision.reason);
  const quelle = textOf(decision.originalRemark);
  if (status === 'blocked') return { v: `gesperrt: ${grund}${quelle ? ` (${quelle})` : ''}`, s: S.none };
  return { v: `zu prüfen: ${grund}`, s: S.open };
}

// Eine Adresse gilt im Export nur als recherchiert, wenn ein externes Zitat
// genau sie nennt und keine Pruefung sie als ungueltig meldet (Carbosulf
// 23.09.2026: die ungueltige Akzo-Adresse stand wie ein Treffer da).
function emailZelle(email, pruefung, belegt) {
  if (!email) return '';
  if (/invalid|ungültig|ungueltig|unzustellbar/i.test(pruefung)) return { v: `${email} (ungültig, nicht verwenden)`, s: S.none };
  if (!belegt) return { v: `${email} (nicht extern belegt)`, s: S.open };
  return { v: email, s: S.link, link: `mailto:${email}` };
}

function statusStyle(status) {
  if (status === 'verified') return S.ok;
  if (status === 'no_match' || status === 'unsupported') return S.none;
  if (status) return S.open;
  return S.normal;
}

// Eine Tabelle: Kopfzeile, Zeilen aus Zellen {v, s, link}, Spaltenbreiten.
function sheetXml({ columns, rows, freezeHeader = true, autoFilter = true, links = [] }) {
  const cols = columns
    .map((column, index) => `<col min="${index + 1}" max="${index + 1}" width="${column.width || 18}" customWidth="1"/>`)
    .join('');
  const lastColumn = columnName(Math.max(columns.length - 1, 0));
  const headerRow = columns.length
    ? `<row r="1">${columns
      .map((column, index) => `<c r="${columnName(index)}1" t="inlineStr" s="${S.header}"><is><t xml:space="preserve">${xmlEscape(column.title)}</t></is></c>`)
      .join('')}</row>`
    : '';
  const firstDataRow = columns.length ? 2 : 1;
  const bodyRows = rows
    .map((row, rowIndex) => {
      const r = rowIndex + firstDataRow;
      const cells = row
        .map((cell, colIndex) => {
          if (cell === null || cell === undefined) return '';
          const value = typeof cell === 'object' ? cell.v : cell;
          const style = typeof cell === 'object' && cell.s !== undefined ? cell.s : S.wrap;
          const ref = `${columnName(colIndex)}${r}`;
          if (typeof cell === 'object' && cell.link) links.push({ ref, url: cell.link });
          if (typeof value === 'number' && Number.isFinite(value)) {
            return `<c r="${ref}" s="${style}"><v>${value}</v></c>`;
          }
          const text = textOf(value);
          if (!text) return `<c r="${ref}" s="${style}"/>`;
          return `<c r="${ref}" t="inlineStr" s="${style}"><is><t xml:space="preserve">${xmlEscape(text)}</t></is></c>`;
        })
        .join('');
      // Umgebrochene Texte brauchen Zeilenhoehe: Excel passt sie beim
      // Oeffnen nicht an, 16 pt schnitten 300-Zeichen-Zellen ab (Review 23.09.).
      const zeilen = row.reduce((max, cell, colIndex) => {
        const value = cell && typeof cell === 'object' ? cell.v : cell;
        const text = textOf(value);
        if (!text) return max;
        const breite = Math.max(8, Number(columns[colIndex]?.width) || 18);
        const umbrueche = text.split('\n').reduce((sum, teil) => sum + Math.max(1, Math.ceil(teil.length / (breite * 1.15))), 0);
        return Math.max(max, umbrueche);
      }, 1);
      const hoehe = zeilen > 1 ? ` ht="${Math.min(409, Math.round(zeilen * 15 + 3))}" customHeight="1"` : '';
      return `<row r="${r}"${hoehe}>${cells}</row>`;
    })
    .join('');
  const lastRow = rows.length + (columns.length ? 1 : 0);
  const pane = freezeHeader && columns.length
    ? '<sheetViews><sheetView workbookViewId="0"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews>'
    : '<sheetViews><sheetView workbookViewId="0"/></sheetViews>';
  const filter = autoFilter && columns.length && rows.length
    ? `<autoFilter ref="A1:${lastColumn}${lastRow}"/>`
    : '';
  const hyperlinks = links.length
    ? `<hyperlinks>${links.map((link, index) => `<hyperlink ref="${link.ref}" r:id="rIdLink${index + 1}"/>`).join('')}</hyperlinks>`
    : '';
  return `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">${pane}<sheetFormatPr defaultRowHeight="16"/><cols>${cols}</cols><sheetData>${headerRow}${bodyRows}</sheetData>${filter}${hyperlinks}</worksheet>`;
}

// Kopfblock (Beschriftung | Wert) ohne Filter, fuer die Uebersicht.
function summarySheetXml(title, pairs) {
  const rows = [
    `<row r="1" ht="24" customHeight="1"><c r="A1" t="inlineStr" s="${S.title}"><is><t xml:space="preserve">${xmlEscape(title)}</t></is></c></row>`,
  ];
  pairs.forEach(([label, value], index) => {
    const r = index + 3;
    const text = textOf(value);
    rows.push(`<row r="${r}"><c r="A${r}" t="inlineStr" s="${S.label}"><is><t xml:space="preserve">${xmlEscape(label)}</t></is></c>${text
      ? `<c r="B${r}" t="inlineStr" s="${S.wrap}"><is><t xml:space="preserve">${xmlEscape(text)}</t></is></c>`
      : ''}</row>`);
  });
  return `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetViews><sheetView workbookViewId="0" showGridLines="0"/></sheetViews><sheetFormatPr defaultRowHeight="16"/><cols><col min="1" max="1" width="30" customWidth="1"/><col min="2" max="2" width="70" customWidth="1"/></cols><sheetData>${rows.join('')}</sheetData></worksheet>`;
}

const STYLES_XML = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<fonts count="6">
<font><sz val="11"/><name val="Calibri"/><family val="2"/></font>
<font><b/><sz val="11"/><color rgb="FFFFFFFF"/><name val="Calibri"/><family val="2"/></font>
<font><u/><sz val="11"/><color rgb="FF1F5FBF"/><name val="Calibri"/><family val="2"/></font>
<font><b/><sz val="16"/><color rgb="FF1F2A36"/><name val="Calibri"/><family val="2"/></font>
<font><b/><sz val="11"/><color rgb="FF1F2A36"/><name val="Calibri"/><family val="2"/></font>
<font><sz val="10"/><color rgb="FF6B7785"/><name val="Calibri"/><family val="2"/></font>
</fonts>
<fills count="7">
<fill><patternFill patternType="none"/></fill>
<fill><patternFill patternType="gray125"/></fill>
<fill><patternFill patternType="solid"><fgColor rgb="FF1F2A36"/><bgColor indexed="64"/></patternFill></fill>
<fill><patternFill patternType="solid"><fgColor rgb="FFE3F2E7"/><bgColor indexed="64"/></patternFill></fill>
<fill><patternFill patternType="solid"><fgColor rgb="FFFFF1D6"/><bgColor indexed="64"/></patternFill></fill>
<fill><patternFill patternType="solid"><fgColor rgb="FFEEF0F2"/><bgColor indexed="64"/></patternFill></fill>
<fill><patternFill patternType="solid"><fgColor rgb="FFB7E1C1"/><bgColor indexed="64"/></patternFill></fill>
</fills>
<borders count="2">
<border><left/><right/><top/><bottom/><diagonal/></border>
<border><left/><right/><top/><bottom style="thin"><color rgb="FFD9DEE4"/></bottom><diagonal/></border>
</borders>
<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>
<cellXfs count="11">
<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>
<xf numFmtId="0" fontId="1" fillId="2" borderId="0" xfId="0" applyFont="1" applyFill="1" applyAlignment="1"><alignment vertical="center" wrapText="1"/></xf>
<xf numFmtId="0" fontId="0" fillId="0" borderId="1" xfId="0" applyBorder="1" applyAlignment="1"><alignment vertical="top" wrapText="1"/></xf>
<xf numFmtId="0" fontId="2" fillId="0" borderId="1" xfId="0" applyFont="1" applyBorder="1" applyAlignment="1"><alignment vertical="top" wrapText="1"/></xf>
<xf numFmtId="0" fontId="0" fillId="3" borderId="1" xfId="0" applyFill="1" applyBorder="1" applyAlignment="1"><alignment vertical="top" wrapText="1"/></xf>
<xf numFmtId="0" fontId="0" fillId="4" borderId="1" xfId="0" applyFill="1" applyBorder="1" applyAlignment="1"><alignment vertical="top" wrapText="1"/></xf>
<xf numFmtId="0" fontId="0" fillId="5" borderId="1" xfId="0" applyFill="1" applyBorder="1" applyAlignment="1"><alignment vertical="top" wrapText="1"/></xf>
<xf numFmtId="0" fontId="3" fillId="0" borderId="0" xfId="0" applyFont="1"/>
<xf numFmtId="0" fontId="4" fillId="0" borderId="0" xfId="0" applyFont="1" applyAlignment="1"><alignment vertical="top"/></xf>
<xf numFmtId="0" fontId="0" fillId="6" borderId="1" xfId="0" applyFill="1" applyBorder="1" applyAlignment="1"><alignment vertical="top" wrapText="1"/></xf>
<xf numFmtId="0" fontId="5" fillId="0" borderId="1" xfId="0" applyFont="1" applyBorder="1" applyAlignment="1"><alignment vertical="top" wrapText="1"/></xf>
</cellXfs>
<cellStyles count="1"><cellStyle name="Standard" xfId="0" builtinId="0"/></cellStyles>
</styleSheet>`;

// Firmenfelder in der Reihenfolge der App-Gruppen; Personenfelder stehen im
// Personenblatt, Governance-Felder in der Uebersicht.
function companyFieldRows(lead, groups, fieldLabel, sourceProvider = null) {
  const fieldStatus = lead.field_status && typeof lead.field_status === 'object' ? lead.field_status : {};
  const data = lead.data && typeof lead.data === 'object' ? lead.data : {};
  const seen = new Set();
  const keys = [];
  for (const group of groups) {
    for (const [key] of group.fields) {
      if (key.startsWith('person_') || seen.has(key)) continue;
      if (group.id === 'governance') continue;
      seen.add(key);
      keys.push(key);
    }
  }
  for (const key of Object.keys(fieldStatus)) {
    if (!key.startsWith('person_') && !seen.has(key)) {
      seen.add(key);
      keys.push(key);
    }
  }
  return keys
    .map((key) => {
      const status = fieldStatus[key] || {};
      // Ein Rohwert aus data (Import, verworfener Kandidat) gehoert nicht
      // neben "kein Treffer": Carbosulf zeigte Fax "+49 221 7496 190" mit
      // Status kein Treffer (23.09.2026). Negativer Status → kein Wert.
      const negativ = ['no_match', 'unsupported'].includes(String(status.status || ''));
      const value = textOf(status.value) || (negativ ? '' : textOf(data[key]));
      const sources = Array.isArray(status.sources) ? status.sources : [];
      if (!value && !status.status) return null;
      const hosts = [...new Set(sources.map((source) => source?.source_id || hostOf(source?.url)).filter(Boolean))];
      // Unabhaengige Quellen je Anbieter wie in der App (northdata.de und
      // northdata.com sind EINE Quelle); Sellify zaehlt nie als Beleg.
      const istSellify = (source) => /^sellify/i.test(String(source?.source_id || '')) || /^sellify:/i.test(String(source?.url || ''));
      const provider = (source) => {
        const host = hostOf(source?.url) || String(source?.source_id || '').split('/')[0];
        return typeof sourceProvider === 'function' ? sourceProvider(host) : host;
      };
      const externalCount = new Set(sources.filter((source) => !istSellify(source)).map(provider).filter(Boolean)).size;
      const sellify = sources.some(istSellify);
      return { key, label: fieldLabel(key), value, status: status.status || '', hosts, externalCount, sellify, reason: textOf(status.reason) };
    })
    .filter(Boolean);
}

function contactRows(lead) {
  const contacts = Array.isArray(lead.contacts) ? lead.contacts : [];
  const seen = new Map();
  for (const contact of contacts) {
    const name = textOf(contact.name)
      || [textOf(contact.person_vorname || contact.first_name), textOf(contact.person_nachname || contact.last_name)].filter(Boolean).join(' ');
    const key = name.toLowerCase() || contact.id;
    if (!key || seen.has(key)) continue;
    seen.set(key, { contact, name });
  }
  return [...seen.values()];
}

export function exportFileName(label) {
  const base = String(label || 'Recherche')
    .normalize('NFKD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/[^A-Za-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 60) || 'Recherche';
  const date = new Date().toISOString().slice(0, 10);
  return `Recherche_${base}_${date}.xlsx`;
}

/**
 * Baut die Arbeitsmappe fuer einen oder mehrere Leads.
 * @param {object[]} leads
 * @param {{ title: string, groups: object[], fieldLabel: Function, researchLabel: Function, sellifyLabel: Function, jszipUrl: string }} options
 * @returns {Promise<Blob>}
 */
export async function buildResearchWorkbook(leads, options) {
  const { default: JSZip } = await import(options.jszipUrl);
  const fieldLabel = options.fieldLabel || ((key) => key);
  const many = leads.length > 1;
  const exportedAt = new Date().toLocaleString('de-DE');

  // Blatt 1: Uebersicht (ein Lead) bzw. Leads (Kampagne).
  const sheets = [];
  if (many) {
    const columns = [
      { title: 'Firma', width: 34 },
      { title: 'Ort', width: 18 },
      { title: 'Land', width: 8 },
      { title: 'Domain', width: 24 },
      { title: 'Recherche', width: 16 },
      { title: 'Sellify', width: 20 },
      { title: 'Felder belegt', width: 14 },
      { title: 'Felder offen', width: 14 },
      { title: 'Personen', width: 10 },
      { title: 'Recherche beendet', width: 20 },
    ];
    const rows = leads.map((lead) => {
      const fields = companyFieldRows(lead, options.groups, fieldLabel);
      const verified = fields.filter((field) => field.status === 'verified').length;
      const open = fields.filter((field) => field.status && field.status !== 'verified').length;
      const finished = lead.payload?.research_finished_at_ms
        ? new Date(lead.payload.research_finished_at_ms).toLocaleString('de-DE')
        : '';
      const domain = textOf(lead.field_status?.firma_domain?.value) || textOf(lead.domain);
      return [
        { v: textOf(lead.field_status?.firma_name?.value) || textOf(lead.name), s: S.label },
        textOf(lead.field_status?.firma_ort?.value) || textOf(lead.city),
        textOf(lead.country),
        domain ? { v: domain, s: S.link, link: /^https?:/i.test(domain) ? domain : `https://${domain}` } : '',
        options.researchLabel ? options.researchLabel(lead) : textOf(lead.research_status),
        options.sellifyLabel ? options.sellifyLabel(lead) : textOf(lead.sellify_status),
        verified,
        open,
        contactRows(lead).length,
        finished,
      ];
    });
    sheets.push({ name: 'Leads', xml: null, table: { columns, rows } });
  } else {
    const lead = leads[0] || {};
    const fields = companyFieldRows(lead, options.groups, fieldLabel);
    sheets.push({
      name: 'Übersicht',
      xml: summarySheetXml(textOf(lead.field_status?.firma_name?.value) || textOf(lead.name) || 'Recherche', [
        ['Kampagne', lead.campaign],
        ['Ort / Land', [textOf(lead.field_status?.firma_ort?.value) || textOf(lead.city), textOf(lead.country)].filter(Boolean).join(', ')],
        ['Domain', textOf(lead.field_status?.firma_domain?.value) || textOf(lead.domain)],
        ['Recherche-Status', options.researchLabel ? options.researchLabel(lead) : lead.research_status],
        ['Sellify', options.sellifyLabel ? options.sellifyLabel(lead) : lead.sellify_status],
        ['Felder belegt', `${fields.filter((field) => field.status === 'verified').length} von ${fields.length}`],
        ['Personen', String(contactRows(lead).length)],
        ['Recherche beendet', lead.payload?.research_finished_at_ms ? new Date(lead.payload.research_finished_at_ms).toLocaleString('de-DE') : ''],
        ['Exportiert', exportedAt],
      ]),
    });
  }

  // Blatt: Firmendaten (Langformat, filterbar).
  {
    const columns = [
      ...(many ? [{ title: 'Firma', width: 30 }] : []),
      { title: 'Feld', width: 28 },
      { title: 'Wert', width: 48 },
      { title: 'Status', width: 20 },
      { title: 'Quellen', width: 30 },
      { title: 'Begründung', width: 44 },
    ];
    const rows = [];
    for (const lead of leads) {
      const company = textOf(lead.field_status?.firma_name?.value) || textOf(lead.name);
      for (const field of companyFieldRows(lead, options.groups, fieldLabel, options.sourceProvider)) {
        rows.push([
          ...(many ? [company] : []),
          { v: field.label, s: S.label },
          field.value,
          { v: statusMitQuellen(field), s: feldStatusStil(field) },
          field.hosts.join(', '),
          { v: field.reason, s: S.muted },
        ]);
      }
    }
    sheets.push({ name: 'Firmendaten', table: { columns, rows } });
  }

  // Blatt: Personen.
  {
    const columns = [
      ...(many ? [{ title: 'Firma', width: 30 }] : []),
      { title: 'Anrede/Geschlecht', width: 16 },
      { title: 'Titel', width: 12 },
      { title: 'Name', width: 26 },
      { title: 'Funktion', width: 28 },
      { title: 'Position', width: 28 },
      { title: 'E-Mail', width: 32 },
      { title: 'E-Mail-Prüfung', width: 16 },
      { title: 'Telefon', width: 18 },
      { title: 'LinkedIn', width: 34 },
      { title: 'XING', width: 34 },
      { title: 'In Sellify', width: 11 },
      { title: 'Empfängerstatus', width: 34 },
    ];
    const rows = [];
    for (const lead of leads) {
      const company = textOf(lead.field_status?.firma_name?.value) || textOf(lead.name);
      for (const { contact, name } of contactRows(lead)) {
        const email = textOf(contact.person_email) || textOf(contact.email);
        const linkedin = textOf(contact.person_linkedin);
        const xing = textOf(contact.person_xing);
        // Leere Personenfelder sagen, ob fuer DIESE Person gesucht und nichts
        // gefunden wurde oder ob sie gar nicht recherchiert ist.
        const leer = (feld) => ({
          v: typeof options.personFeldStatus === 'function' ? options.personFeldStatus(lead, contact, feld) : '',
          s: S.none,
        });
        const oder = (wert, feld, zelle) => (wert ? (zelle || wert) : leer(feld));
        const identitaet = typeof options.identitaet === 'function' ? options.identitaet(lead, contact) : null;
        const nameZelle = identitaet?.status === 'widerspruch'
          ? { v: `${name} (Name widerspricht Quelle: ${identitaet.quelleName})`, s: S.open }
          : { v: name, s: S.label };
        const anrede = lesbarerMehrsprachText(contact.person_geschlecht) || lesbarerMehrsprachText(contact.person_anrede) || lesbarerMehrsprachText(contact.salutation);
        const titel = lesbarerMehrsprachText(contact.person_titel) || lesbarerMehrsprachText(contact.title);
        const telefon = textOf(contact.person_telefon) || textOf(contact.phone);
        rows.push([
          ...(many ? [company] : []),
          oder(anrede, 'person_geschlecht'),
          oder(titel, 'person_titel'),
          nameZelle,
          lesbarerMehrsprachText(contact.person_funktion) || lesbarerMehrsprachText(contact.role),
          lesbarerMehrsprachText(contact.person_position) || lesbarerMehrsprachText(contact.position),
          oder(email, 'person_email', emailZelle(email, textOf(contact.person_email_validation),
            typeof options.emailBelegt === 'function' ? options.emailBelegt(lead, contact) : true)),
          textOf(contact.person_email_validation),
          oder(telefon, 'person_telefon'),
          oder(linkedin, 'person_linkedin', isHttpUrl(linkedin) ? { v: linkedin, s: S.link, link: linkedin } : linkedin),
          oder(xing, 'person_xing', isHttpUrl(xing) ? { v: xing, s: S.link, link: xing } : xing),
          contact.crm_known === true ? 'ja' : contact.crm_known === false ? 'nein' : '',
          empfaengerZelle(typeof options.recipientStatus === 'function' ? options.recipientStatus(lead, contact) : null),
        ]);
      }
    }
    sheets.push({ name: 'Personen', table: { columns, rows } });
  }

  // Blatt: Belege.
  {
    const columns = [
      ...(many ? [{ title: 'Firma', width: 30 }] : []),
      { title: 'Feld', width: 26 },
      { title: 'Quelle', width: 22 },
      { title: 'URL', width: 50 },
      { title: 'Zitat', width: 60 },
      { title: 'Person', width: 22 },
    ];
    const rows = [];
    for (const lead of leads) {
      const company = textOf(lead.field_status?.firma_name?.value) || textOf(lead.name);
      const evidence = Array.isArray(lead.evidence) ? lead.evidence : [];
      const fromStatus = Object.entries(lead.field_status || {}).flatMap(([key, status]) =>
        (Array.isArray(status?.sources) ? status.sources : []).map((source) => ({ field_key: key, ...source })));
      const all = evidence.length ? evidence : fromStatus;
      const seen = new Set();
      for (const entry of all) {
        const url = textOf(entry.url);
        const key = `${entry.field_key}|${url}|${textOf(entry.quote).slice(0, 80)}`;
        if (seen.has(key)) continue;
        seen.add(key);
        rows.push([
          ...(many ? [company] : []),
          { v: fieldLabel(entry.field_key), s: S.label },
          textOf(entry.source_id) || hostOf(url),
          isHttpUrl(url) ? { v: url, s: S.link, link: url } : url,
          { v: textOf(entry.quote), s: S.muted },
          textOf(entry.person_key),
        ]);
      }
    }
    sheets.push({ name: 'Belege', table: { columns, rows } });
  }

  const zip = new JSZip();
  const sheetEntries = sheets.map((sheet, index) => {
    const links = [];
    const xml = sheet.xml || sheetXml({ ...sheet.table, links });
    return { ...sheet, xml, links, file: `sheet${index + 1}.xml`, id: index + 1 };
  });

  zip.file('[Content_Types].xml', `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>${sheetEntries
    .map((sheet) => `<Override PartName="/xl/worksheets/${sheet.file}" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>`)
    .join('')}<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/></Types>`);
  zip.file('_rels/.rels', `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/></Relationships>`);
  zip.file('docProps/core.xml', `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>${xmlEscape(options.title || 'Recherche')}</dc:title><dc:creator>CTOX Outbound Lead Generation</dc:creator><dcterms:created xsi:type="dcterms:W3CDTF">${new Date().toISOString().replace(/\.\d{3}Z$/, 'Z')}</dcterms:created></cp:coreProperties>`);
  zip.file('xl/workbook.xml', `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>${sheetEntries
    .map((sheet) => `<sheet name="${xmlEscape(sheet.name)}" sheetId="${sheet.id}" r:id="rId${sheet.id}"/>`)
    .join('')}</sheets></workbook>`);
  zip.file('xl/_rels/workbook.xml.rels', `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">${sheetEntries
    .map((sheet) => `<Relationship Id="rId${sheet.id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/${sheet.file}"/>`)
    .join('')}<Relationship Id="rId${sheetEntries.length + 1}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>`);
  zip.file('xl/styles.xml', STYLES_XML);
  for (const sheet of sheetEntries) {
    zip.file(`xl/worksheets/${sheet.file}`, sheet.xml);
    if (sheet.links.length) {
      zip.file(`xl/worksheets/_rels/${sheet.file}.rels`, `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">${sheet.links
        .map((link, index) => `<Relationship Id="rIdLink${index + 1}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="${xmlEscape(link.url)}" TargetMode="External"/>`)
        .join('')}</Relationships>`);
    }
  }
  return zip.generateAsync({
    type: 'blob',
    mimeType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    compression: 'DEFLATE',
  });
}

export function downloadBlob(blob, fileName, host = document.body) {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = fileName;
  anchor.style.display = 'none';
  host.appendChild(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 30_000);
}
