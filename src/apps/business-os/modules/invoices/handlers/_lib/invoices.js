// Gemeinsame Fachlogik der Rechnungen-App (Port aus invoices.rs, Stand 4794af03d, 2 883 Z.).
// Jede Funktion nennt ihr Rust-Vorbild. Kein Dateisystem, kein Netz.
//
// Speicher: Rust las/schrieb ausschliesslich die SQL-Tabelle `business_records`
// (load_business_record / invoices_upsert_invoice / upsert_business_record_helper).
// v4 kennt nur ctx.get/put/remove. `deleted = 1` (SQL-Spalte) -> ctx.remove.
// Die von Rust ins Dokument geschriebenen Speicherfelder `_rev` ("rev_<id>_<ms>") und
// `_deleted` entfallen: Revision und Loeschmarke fuehrt in v4 der Core.

export const COL = {
  invoices: 'accounting_invoices',
  journal: 'accounting_journal_entries', // Eigentuemer buchhaltung -> foreign write
  journalLines: 'accounting_journal_entry_lines', // Eigentuemer buchhaltung -> foreign write
  numberSeries: 'accounting_number_series', // Eigentuemer buchhaltung -> foreign write
  allocations: 'accounting_payment_allocations',
  dunningRuns: 'accounting_dunning_runs',
  dunningLetters: 'accounting_dunning_letters',
};

export function fail(code, message) {
  throw { code, message };
}

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
export const str = (v) => (typeof v === 'string' ? v : undefined);
// serde_json as_i64: ganze Zahl im i64-Bereich. (JSON 5.0 ist in Rust KEIN i64; JS kann das nicht unterscheiden.)
export const asI64 = (v) => (Number.isInteger(v) ? v : undefined);
export const asU64 = (v) => (Number.isInteger(v) && v >= 0 ? v : undefined);
export const asF64 = (v) => (typeof v === 'number' ? v : undefined);
export const asBool = (v) => (typeof v === 'boolean' ? v : undefined);
// Rust f64::round: Haelfte weg von null.
export const roundAway = (n) => (n < 0 ? -Math.round(-n) : Math.round(n));
const clamp = (n, lo, hi) => Math.min(Math.max(n, lo), hi);

// Rust: command.id.clone().unwrap_or_default()
export const commandIdOf = (command) => str(command?.id) ?? '';

// ref: invoices.rs handle_invoices_active_command :62 – Modulpruefung
export function ensureModule(command) {
  if (command?.app_id !== 'invoices') {
    fail('forbidden', `invoices handler requires module=invoices, got module=${command?.app_id}`);
  }
}

// ref: invoices.rs invoices_record_requirement :431
export function recordRequirement(command, candidates) {
  const payload = isObject(command?.payload) ? command.payload : undefined;
  for (const key of candidates) {
    const v = payload?.[key];
    if (typeof v === 'string' && v) return v;
  }
  const recordId = str(command?.record_id);
  if (recordId) return recordId;
  fail(
    'invalid',
    `missing required field for invoices command ${command?.type} (expected one of ${JSON.stringify(candidates).replaceAll(',', ', ')})`,
  );
}

// ref: invoices.rs load_accounting_invoice :128 / load_business_record :132
export const loadRecord = (ctx, collection, id) => ctx.get(collection, id);
export const loadInvoice = (ctx, id) => ctx.get(COL.invoices, id);

// ref: invoices.rs invoices_upsert_invoice :153 / upsert_business_record_helper :1060
// Rust setzt id und updated_at_ms (plus _rev/_deleted, entfallen) und schreibt den ganzen Datensatz.
export async function upsertRecord(ctx, collection, id, updatedAtMs, payload) {
  const stored = isObject(payload) ? { ...payload, id, updated_at_ms: updatedAtMs } : payload;
  const result = await ctx.put(collection, stored);
  if (result?.status === 'rejected') fail('conflict', `write to ${collection}/${id} rejected: ${result.reason}`);
  return stored;
}
export const upsertInvoice = (ctx, id, payload, now) => upsertRecord(ctx, COL.invoices, id, now, payload);

// ref: SQL `UPDATE business_records SET deleted = 1` – Grabstein nach dem Soft-Delete-Schreiben.
export async function tombstone(ctx, collection, id) {
  await ctx.remove(collection, id);
}

// ref: invoices.rs invoices_search_text_map :417
export function searchTextMap(obj) {
  const parts = [];
  for (const key of ['party_id', 'invoice_number', 'currency']) {
    if (typeof obj?.[key] === 'string') parts.push(obj[key].toLowerCase());
  }
  return parts.join(' ');
}

// ref: invoices.rs invoices_object_from_command :188 – Achtung: currency wird IMMER "EUR".
export function objectFromCommand(command, invoiceId) {
  const obj = { invoice_id: invoiceId, command_id: commandIdOf(command) };
  if (isObject(command?.payload)) {
    for (const [key, value] of Object.entries(command.payload)) {
      if (key === 'invoice_id' || key === 'id') continue;
      obj[key] = value;
    }
  }
  obj.currency = 'EUR';
  for (const key of ['subtotal_cents', 'tax_cents', 'total_cents', 'paid_cents', 'open_cents']) {
    if (!Object.prototype.hasOwnProperty.call(obj, key)) obj[key] = 0;
  }
  obj.search_text = searchTextMap(obj);
  return obj;
}

// ref: invoices.rs validate_invoice_for_command :242
export function validateInvoiceForCommand(invoice, strictPost) {
  if (!isObject(invoice)) fail('invalid', 'invoice must be a JSON object');
  const obj = invoice;
  const invoiceType = str(obj.invoice_type) ?? '';
  if (!['sale_out', 'sale_in', 'credit_note_out', 'credit_note_in', 'recurring_template'].includes(invoiceType)) {
    fail(
      'invalid',
      `invoice_type must be one of: sale_out, sale_in, credit_note_out, credit_note_in, recurring_template (got: ${JSON.stringify(invoiceType)})`,
    );
  }
  if (!(str(obj.party_id) ?? '')) fail('invalid', 'party_id is required for invoices.* commands');
  if (!(str(obj.currency) ?? '')) fail('invalid', 'currency is required for invoices.* commands');
  const dateMs = asI64(obj.invoice_date_ms);
  if (dateMs !== undefined) {
    if (dateMs <= 0) fail('invalid', 'invoice_date_ms must be a positive integer ms timestamp');
  } else if (strictPost) {
    fail('invalid', 'invoice_date_ms is required when posting');
  }
  const smallBusiness = asBool(obj.small_business) ?? false;
  const reverseCharge = asBool(obj.reverse_charge) ?? false;
  const euIcSupply = asBool(obj.eu_ic_supply) ?? false;
  if (smallBusiness && Array.isArray(obj.tax_breakdown) && obj.tax_breakdown.length) {
    fail('invalid', 'small_business invoices must not carry tax_breakdown entries');
  }
  if (reverseCharge && invoiceType !== 'sale_out' && invoiceType !== 'sale_in') {
    fail('invalid', 'reverse_charge is only valid for sale_out and sale_in invoices');
  }
  if (euIcSupply && invoiceType !== 'sale_out') {
    fail('invalid', 'eu_ic_supply (innergemeinschaftliche Lieferung) is only valid for sale_out invoices');
  }
  if ((invoiceType === 'credit_note_out' || invoiceType === 'credit_note_in') && !(str(obj.credit_note_for_id) ?? '')) {
    fail('invalid', 'credit_note_{out,in} invoice types require a non-empty credit_note_for_id reference');
  }
  if (strictPost) {
    const lines = obj.lines;
    if (!Array.isArray(lines) || !lines.length) fail('invalid', 'at least one line item is required when posting');
    lines.forEach((line, idx) => {
      if (!isObject(line)) fail('invalid', `lines[${idx}] must be a JSON object`);
      if (!(str(line.description) ?? '')) fail('invalid', `lines[${idx}].description is required when posting`);
      if ((asI64(line.quantity) ?? 0) <= 0) fail('invalid', `lines[${idx}].quantity must be a positive integer (in thousandths)`);
      const unitPrice = asI64(line.unit_price_cents);
      if (unitPrice === undefined || unitPrice < 0) fail('invalid', `lines[${idx}].unit_price_cents must be a non-negative integer`);
      const taxRate = asF64(line.tax_rate) ?? NaN;
      if (!Number.isFinite(taxRate) || taxRate < 0 || taxRate > 1) fail('invalid', `lines[${idx}].tax_rate must be in [0, 1]`);
      if (!(str(line.account_code) ?? '')) fail('invalid', `lines[${idx}].account_code is required when posting`);
    });
  }
  const skontoPercent = asF64(obj.skonto_percent);
  if (skontoPercent !== undefined) {
    if (skontoPercent < 0 || skontoPercent > 100) fail('invalid', 'skonto_percent must be in [0, 100]');
    if ((asF64(obj.skonto_days) ?? 0) <= 0) fail('invalid', 'skonto_days must be positive when skonto_percent is set');
  }
  if (Array.isArray(obj.tax_breakdown)) {
    obj.tax_breakdown.forEach((entry, idx) => {
      if (!isObject(entry)) fail('invalid', `tax_breakdown[${idx}] must be a JSON object`);
      const rate = asF64(entry.tax_rate) ?? NaN;
      if (!Number.isFinite(rate) || rate < 0 || rate > 1) fail('invalid', `tax_breakdown[${idx}].tax_rate must be in [0, 1]`);
      if ((asI64(entry.net_cents) ?? -1) < 0) fail('invalid', `tax_breakdown[${idx}].net_cents must be a non-negative integer`);
      if ((asI64(entry.tax_cents) ?? -1) < 0) fail('invalid', `tax_breakdown[${idx}].tax_cents must be a non-negative integer`);
    });
  }
}

// Zeilenrechnung wie in post_invoice_in_conn :623, invoices_build_journal_entry :903 und line_net :1323.
// quantity in Tausendsteln (XRechnung/UBL): net = round(round(unit*(1-rabatt)) * quantity / 1000).
export function lineAmounts(line) {
  const quantity = asI64(line?.quantity) ?? 0;
  const unitPrice = asI64(line?.unit_price_cents) ?? 0;
  const discount = clamp(asF64(line?.discount_percent) ?? 0, 0, 100) / 100;
  const taxRate = asF64(line?.tax_rate) ?? 0;
  const grossUnit = roundAway(unitPrice * (1 - discount));
  const net = roundAway((grossUnit * quantity) / 1000);
  const tax = roundAway(net * taxRate);
  return { net, tax, taxRate };
}

// ref: invoices.rs iso_year_from_ms :732 (Hinnant, mit Jan/Feb-Korrektur) = UTC-Kalenderjahr
export const isoYearFromMs = (ms) => new Date(ms).getUTCFullYear();
// ref: invoices.rs iso_date_from_ms :1039
export const isoDateFromMs = (ms) => {
  const d = new Date(ms);
  const y = String(d.getUTCFullYear()).padStart(4, '0');
  return `${y}-${String(d.getUTCMonth() + 1).padStart(2, '0')}-${String(d.getUTCDate()).padStart(2, '0')}`;
};

// ref: invoices.rs find_journal_entry_for_invoice :757 (exakter ref_id-Vergleich, kein LIKE)
export async function findJournalEntryForInvoice(ctx, invoiceId) {
  const found = await ctx.read(COL.journal, { selector: { ref_id: invoiceId }, limit: 1 });
  return found[0] ?? null;
}

// ref: invoices.rs reserve_next_invoice_number :797
const SERIES_PREFIX = { sale_out: 'RE-', sale_in: 'ER-', credit_note_out: 'GS-', credit_note_in: 'EG-', recurring_template: 'RT-' };
export async function reserveNextInvoiceNumber(ctx, invoice, fiscalYear, now) {
  const seriesKey = str(invoice.invoice_type) ?? 'sale_out';
  const seriesId = `${seriesKey}_${fiscalYear}`;
  const stored = await loadRecord(ctx, COL.numberSeries, seriesId);
  const nextValue = stored ? (asI64(stored.next_value) ?? 1) : 1;
  const prefix = SERIES_PREFIX[seriesKey] ?? 'RE-';
  const invoiceNumber = `${prefix}${fiscalYear}-${String(nextValue).padStart(4, '0')}`;
  await upsertRecord(ctx, COL.numberSeries, seriesId, now, {
    id: seriesId,
    series_key: seriesKey,
    fiscal_year: fiscalYear,
    prefix,
    next_value: nextValue + 1,
    last_issued_number: invoiceNumber,
    gap_policy: 'strict_no_gaps',
    updated_at_ms: now,
    payload: {},
  });
  return invoiceNumber;
}

// ref: invoices.rs invoices_build_journal_entry :859
export function buildJournalEntry(invoice, party, invoiceNumber, now) {
  const invoiceType = str(invoice.invoice_type) ?? 'sale_out';
  const isCreditNote = invoiceType === 'credit_note_out' || invoiceType === 'credit_note_in';
  const isInput = invoiceType === 'sale_in' || invoiceType === 'credit_note_in';
  const smallBusiness = asBool(invoice.small_business) ?? false;
  const reverseCharge = asBool(invoice.reverse_charge) ?? false;
  const partyId = str(party?.party_id) ?? '';
  const partyAccountId = invoiceType === 'sale_in' || invoiceType === 'credit_note_in' ? '1600' : '1400';
  const revenueIsDebit = isInput !== isCreditNote;
  const taxIsDebit = isInput !== isCreditNote;
  const partyIsCredit = isInput !== isCreditNote;

  const revenueLines = [];
  const taxLines = [];
  let netTotal = 0;
  let taxTotal = 0;
  const taxByRate = new Map(); // Schluessel format!("{:.4}") – Rust BTreeMap -> sortiert
  for (const line of Array.isArray(invoice.lines) ? invoice.lines : []) {
    const { net, tax, taxRate } = lineAmounts(line);
    netTotal += net;
    taxTotal += tax;
    revenueLines.push({
      account_id: str(line?.account_code) ?? (isInput ? '3400' : '8400'),
      debit: revenueIsDebit ? net : 0,
      credit: revenueIsDebit ? 0 : net,
      description: str(line?.description) ?? '',
    });
    const key = taxRate.toFixed(4);
    const entry = taxByRate.get(key) ?? [taxRate, 0];
    entry[1] += tax;
    taxByRate.set(key, entry);
  }
  if (!smallBusiness && !reverseCharge) {
    for (const key of [...taxByRate.keys()].sort()) {
      const [taxRate, taxCents] = taxByRate.get(key);
      let accountId = '';
      if (Math.abs(taxRate - 0.19) < 1e-9) accountId = isInput ? '1406' : '3806';
      else if (Math.abs(taxRate - 0.07) < 1e-9) accountId = isInput ? '1407' : '3801';
      if (!accountId) continue; // Rust: andere Saetze bekommen keine Steuerzeile (siehe Befund I3)
      taxLines.push({
        account_id: accountId,
        debit: taxIsDebit ? taxCents : 0,
        credit: taxIsDebit ? 0 : taxCents,
        tax_rate_id: `tax_${taxRate}`,
      });
    }
  }
  const partyBase = taxLines.length === 0 ? netTotal : netTotal + taxTotal;
  const partyLine = {
    account_id: partyAccountId,
    debit: partyIsCredit ? 0 : partyBase,
    credit: partyIsCredit ? partyBase : 0,
    party_id: partyId,
  };
  const lines = [...revenueLines, ...taxLines, partyLine];
  const totalDebit = lines.reduce((s, l) => s + (asI64(l.debit) ?? 0), 0);
  const totalCredit = lines.reduce((s, l) => s + (asI64(l.credit) ?? 0), 0);
  if (totalDebit !== totalCredit) fail('invalid', `journal entry unbalanced: debit=${totalDebit} credit=${totalCredit}`);
  const dateMs = asI64(invoice.invoice_date_ms);
  const postingDate = isoDateFromMs(dateMs !== undefined ? dateMs : now);
  const narration = isCreditNote ? `Credit note for ${invoiceNumber}` : `Invoice ${invoiceNumber}`;
  const invoiceId = str(invoice.id) ?? '';
  const header = {
    id: `je_${invoiceId}_post`,
    posting_date: postingDate,
    type: 'invoice',
    ref_type: 'invoice',
    ref_id: invoiceId,
    number: invoiceNumber,
    narration,
    total_debit_cents: totalDebit,
    total_credit_cents: totalCredit,
    balanced: true,
    posted_at: now,
    updated_at_ms: now,
  };
  return { header, lines };
}

// ref: invoices.rs post_invoice_in_conn :569 – Nummer reservieren, Summen, ausgeglichene Buchung,
// Status posted (GoBD-unveraenderlich). Idempotent ueber die Buchung zur Rechnung. SAVEPOINT -> ctx.transaction.
// `tx` darf bereits eine Transaktion sein (Storno-Pfad), dann wird keine neue geoeffnet.
export async function postInvoice(ctx, invoiceId, commandId, now, { inTransaction = false } = {}) {
  const existingJe = await findJournalEntryForInvoice(ctx, invoiceId);
  if (existingJe) return { ok: true, idempotent: true, journal_entry: existingJe };
  const run = async (tx) => {
    const invoice = await loadInvoice(tx, invoiceId);
    if (!invoice) fail('not_found', `invoice ${invoiceId} not found`);
    const state = str(invoice.state) ?? 'draft';
    if (state !== 'draft') fail('conflict', `invoices.invoice.post requires state=draft (current: ${state})`);
    validateInvoiceForCommand(invoice, true);
    const dateMs = asI64(invoice.invoice_date_ms);
    const fiscalYear = isoYearFromMs(dateMs !== undefined ? dateMs : now);
    // ref: resolve_party_account :781 – nur party_id aus der Rechnung
    const party = { party_id: str(invoice.party_id) ?? '' };

    const invoiceNumber = await reserveNextInvoiceNumber(tx, invoice, fiscalYear, now);
    invoice.invoice_number = invoiceNumber;
    invoice.updated_at_ms = now;

    let netTotal = 0;
    let taxTotal = 0;
    for (const line of Array.isArray(invoice.lines) ? invoice.lines : []) {
      const { net, tax } = lineAmounts(line);
      netTotal += net;
      taxTotal += tax;
    }
    const smallBusiness = asBool(invoice.small_business) ?? false;
    const reverseCharge = asBool(invoice.reverse_charge) ?? false;
    const effectiveTax = smallBusiness || reverseCharge ? 0 : taxTotal;
    const totalCents = netTotal + effectiveTax;
    invoice.subtotal_cents = netTotal;
    invoice.tax_cents = effectiveTax;
    invoice.total_cents = totalCents;
    invoice.paid_cents = 0;
    invoice.open_cents = totalCents;

    const journal = buildJournalEntry(invoice, party, invoiceNumber, now);
    const journalId = `je_${invoiceId}_post`;
    await upsertInvoice(tx, invoiceId, invoice, now);
    await upsertRecord(tx, COL.journal, journalId, now, journal.header);
    for (const [idx, line] of journal.lines.entries()) {
      const lineId = `${journalId}_l${idx + 1}`;
      await upsertRecord(tx, COL.journalLines, lineId, now, {
        ...line,
        id: lineId,
        journal_entry_id: journalId,
        line_no: idx + 1,
        updated_at_ms: now,
      });
    }
    invoice.state = 'posted';
    invoice.post_journal_entry_id = journalId;
    invoice.posted_at = now;
    invoice.state_changed_at_ms = now;
    invoice.state_changed_by_command_id = commandId;
    invoice.updated_at_ms = now;
    await upsertInvoice(tx, invoiceId, invoice, now);
    return { ok: true, invoice: { ...invoice, id: invoiceId, updated_at_ms: now }, journal_entry: { ...journal.header, id: journalId, updated_at_ms: now } };
  };
  return inTransaction ? run(ctx) : ctx.transaction(run);
}

// ref: invoices.rs build_credit_note_draft :1103 – §17 UStG Gegenrechnung als Entwurf.
export function buildCreditNoteDraft(original, newId, command, now) {
  const originalType = str(original.invoice_type) ?? '';
  let cnType;
  if (originalType === 'sale_out') cnType = 'credit_note_out';
  else if (originalType === 'sale_in') cnType = 'credit_note_in';
  else fail('invalid', `cannot credit an invoice of type ${JSON.stringify(originalType)} (only sale_out / sale_in can be credited)`);
  const creditNoteForId = str(original.id) ?? '';
  const partyId = str(original.party_id) ?? '';
  const currency = str(original.currency) ?? 'EUR';
  const originalReverseCharge = asBool(original.reverse_charge) ?? false;
  const originalSmallBusiness = asBool(original.small_business) ?? false;
  const payload = isObject(command?.payload) ? command.payload : {};
  const rawLines = payload.lines !== undefined ? payload.lines : original.lines !== undefined ? original.lines : [];
  // Unter Reverse Charge oder Kleinunternehmer bleibt die Gutschrift steuerfrei: tax_rate je Zeile = 0.
  const lines =
    originalReverseCharge || originalSmallBusiness
      ? (Array.isArray(rawLines) ? rawLines : []).map((line) => (isObject(line) ? { ...line, tax_rate: 0 } : line))
      : rawLines;
  const invoiceNumber = str(original.invoice_number) ?? '';
  const cn = {
    id: newId,
    invoice_type: cnType,
    credit_note_for_id: creditNoteForId,
    credit_note_for_number: invoiceNumber,
    party_id: partyId,
    currency,
    invoice_date_ms: asI64(payload.invoice_date_ms) ?? now,
    lines,
    reason: str(payload.reason) ?? '§17 UStG Korrektur',
    small_business: original.small_business !== undefined ? original.small_business : false,
    reverse_charge: false,
    reverse_charge_origin: originalReverseCharge,
    state: 'draft',
    is_deleted: false,
    search_text: `${partyId.toLowerCase()} ${currency.toLowerCase()}`,
    subtotal_cents: 0,
    tax_cents: 0,
    total_cents: 0,
    paid_cents: 0,
    open_cents: 0,
    created_at_ms: now,
    updated_at_ms: now,
  };
  validateInvoiceForCommand(cn, false);
  return cn;
}

// ref: invoices.rs load_draft_invoice_lines :1387
export async function loadDraftInvoiceLines(ctx, invoiceId) {
  const invoice = await loadInvoice(ctx, invoiceId);
  if (!invoice) fail('not_found', `invoice ${invoiceId} not found`);
  const state = str(invoice.state) ?? 'draft';
  if (state !== 'draft') fail('conflict', `invoices.line.* is only allowed in state=draft (current: ${state})`);
  return { invoice, lines: Array.isArray(invoice.lines) ? [...invoice.lines] : [] };
}

// ref: invoices.rs save_draft_invoice_lines :1409
export async function saveDraftInvoiceLines(ctx, invoiceId, invoice, lines, now) {
  invoice.lines = lines;
  invoice.updated_at_ms = now;
  validateInvoiceForCommand(invoice, false);
  return upsertInvoice(ctx, invoiceId, invoice, now);
}

// SQLite CAST(json_extract(..) AS INTEGER): NULL bleibt NULL, Zahl wird abgeschnitten,
// Text liefert das fuehrende Ganzzahl-Praefix (sonst 0), true/false -> 1/0.
export function sqliteCastInt(v) {
  if (v === undefined || v === null) return null;
  if (typeof v === 'number') return Math.trunc(v);
  if (typeof v === 'boolean') return v ? 1 : 0;
  if (typeof v === 'string') {
    const m = /^\s*([+-]?\d+)/.exec(v);
    return m ? Number(m[1]) : 0;
  }
  return 0; // Objekt/Array: json_extract liefert Text '{…}' -> 0
}

// ref: invoices.rs invoices_list_due_invoices :1791 + INVOICES_DUE_INVOICES_SQL :1813
export async function listDueInvoices(ctx, filter, nowMs) {
  const minOpenCents = asI64(filter?.min_open_cents) ?? 0;
  const targetState = str(filter?.state) ?? 'overdue';
  const MAX = Number.MAX_SAFE_INTEGER; // SQL: 9223372036854775807
  const docs = await ctx.read(COL.invoices, { selector: { state: targetState } });
  return docs
    .map((d) => ({ id: d.id, due: sqliteCastInt(d.due_date_ms) ?? MAX, open: sqliteCastInt(d.open_cents) ?? 0 }))
    .filter((d) => d.due < nowMs && d.open > minOpenCents)
    .sort((a, b) => a.due - b.due || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
    .map((d) => d.id);
}

// Echte Rust-Stubs (anyhow::bail! „not yet implemented“) -> unavailable mit woertlicher Meldung.
export function notImplemented(message) {
  fail('unavailable', message);
}
