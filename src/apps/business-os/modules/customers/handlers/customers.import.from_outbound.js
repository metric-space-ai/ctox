// customers.import.from_outbound – ref: store_customer_commands.rs:922-1133 (customers_import_from_outbound),
// :1580 (find_by_string_field), :1605 (find_outbound_pipeline_for_company), :1627, :1643 @ 4794af03d
// Liest die Fremd-Collections outbound_companies und outbound_pipeline_items (Manifest-Wunsch W3).
import {
  str, firstString, loadRequired, loadDoc, domainFromWebsite, asciiLower, idFromCommand, putRecord,
  refreshSearchText, splitName, stableDigest, jsonDisplay, writeActivity, fail, clone,
} from './_lib.js';

// ref: :1580 – Rust scannt alle nicht entfernten Datensätze (auch is_deleted=true) und vergleicht
// getrimmt + ASCII-klein; der erste Treffer gewinnt (SQL ohne ORDER BY; hier: id aufsteigend, Unklarheit U4).
async function findByStringField(tx, collection, field, value) {
  const expected = asciiLower(value.trim());
  if (expected === '') return null;
  const rows = await tx.read(collection, { selector: { [field]: { $exists: true } } });
  for (const row of rows) {
    if (row._deleted === true) continue;
    if (asciiLower(str(row, [field]) ?? '') === expected) return clone(row);
  }
  return null;
}

// ref: :1605 – Fehler werden in Rust verschluckt (.ok().flatten()).
async function findOutboundPipelineForCompany(tx, companyId) {
  try {
    const rows = await tx.read('outbound_pipeline_items', { selector: { company_id: companyId } });
    const hit = rows.find((row) => row._deleted !== true);
    return hit ? hit.id : undefined;
  } catch {
    return undefined;
  }
}

async function loadOptional(tx, collection, id) {
  try {
    return await loadDoc(tx, collection, id);
  } catch {
    return null;
  }
}

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const payload = command.payload;
    const recordId = typeof command.record_id === 'string' ? command.record_id : undefined; // v3-Übergang
    const sourceRecordId = firstString([
      str(payload, ['source_record_id']),
      str(payload, ['outbound_company_id']),
      str(payload, ['company_id']),
      recordId,
    ]);
    if (sourceRecordId === undefined) fail('invalid', 'source_record_id or outbound company_id is required');
    const company = await loadRequired(tx, 'outbound_companies', sourceRecordId, 'outbound company not found');
    const pipelineId = str(payload, ['pipeline_id']) ?? await findOutboundPipelineForCompany(tx, sourceRecordId);
    const pipeline = pipelineId !== undefined ? await loadOptional(tx, 'outbound_pipeline_items', pipelineId) : null;
    let domain = str(company, ['domain']);
    if (domain === undefined) {
      const website = str(company, ['website']);
      domain = website !== undefined ? domainFromWebsite(website) : undefined;
    }
    const existingAccount = domain !== undefined
      ? await findByStringField(tx, 'customer_accounts', 'domain', domain)
      : null;
    const batchId = idFromCommand(command, ['import_batch_id'], 'import');
    const batch = {
      id: batchId,
      source: 'outbound',
      source_record_id: sourceRecordId,
      source_filename: '',
      status: 'completed',
      object_type: 'account',
      imported_count: 0,
      skipped_count: 0,
      failed_count: 0,
      dedupe_count: 0,
      payload: clone(payload) ?? null,
      is_deleted: false,
      created_at_ms: now,
    };

    if (existingAccount) {
      const existingId = str(existingAccount, ['id']);
      if (existingId === undefined) fail('invalid', 'id is required');
      const candidate = {
        id: `dedupe_${stableDigest(`outbound:${sourceRecordId}:${existingId}`)}`,
        object_type: 'account',
        match_key: domain ?? sourceRecordId,
        match_type: 'domain',
        source_record_id: sourceRecordId,
        existing_record_id: existingId,
        import_batch_id: batch.id,
        status: 'open',
        confidence: 0.95,
        payload: { outbound_company: company },
        is_deleted: false,
        created_at_ms: now,
      };
      await putRecord(tx, 'customer_dedupe_candidates', candidate.id, now, candidate);
      batch.dedupe_count = 1;
      batch.status = 'needs_review';
      await putRecord(tx, 'customer_import_batches', batch.id, now, batch);
      // Rust: in diesem Zweig keine Aktivität.
      return { ok: true, status: 'needs_review', import_batch: batch, dedupe_candidate: candidate };
    }

    // TREUE: kein Existenz-Check – ein vorhandenes Konto gleicher id wird überschrieben (Befund B3).
    const accountId = idFromCommand(command, ['account_id'], 'acct');
    const accountName = str(company, ['name']) ?? str(pipeline ?? null, ['company_name']);
    if (accountName === undefined) fail('invalid', 'outbound company name is required');
    const account = {
      id: accountId,
      name: accountName,
      domain: domain ?? '',
      website_url: str(company, ['website']) ?? '',
      account_status: 'active',
      customer_stage: 'active',
      health_status: 'unknown',
      source: 'outbound',
      source_record_id: sourceRecordId,
      payload: { outbound_company: company, outbound_pipeline: pipeline },
      is_deleted: false,
      created_at_ms: now,
    };
    refreshSearchText(account, ['name', 'domain']);
    await putRecord(tx, 'customer_accounts', account.id, now, account);

    const contacts = [];
    const rawContacts = account.payload.outbound_pipeline?.contacts;
    if (Array.isArray(rawContacts)) {
      for (const [index, rawContact] of rawContacts.entries()) {
        const contactId = `contact_${stableDigest(`${account.id}:${index}:${jsonDisplay(rawContact)}`)}`;
        const [firstName, lastName] = splitName(str(rawContact, ['name']) ?? '');
        const contact = {
          id: contactId,
          account_id: account.id,
          first_name: firstName,
          last_name: lastName,
          email: str(rawContact, ['email']) ?? '',
          phone: str(rawContact, ['phone']) ?? '',
          job_title: str(rawContact, ['job_title']) ?? str(rawContact, ['title']) ?? '',
          is_primary_contact: index === 0,
          source: 'outbound',
          source_record_id: str(rawContact, ['id']) ?? '',
          payload: clone(rawContact),
          is_deleted: false,
          created_at_ms: now,
        };
        refreshSearchText(contact, ['first_name', 'last_name', 'email', 'job_title']);
        await putRecord(tx, 'customer_contacts', contact.id, now, contact);
        contacts.push(contact);
      }
    }
    batch.imported_count = 1 + contacts.length;
    await putRecord(tx, 'customer_import_batches', batch.id, now, batch);
    const activity = await writeActivity(tx, command, now, {
      activity_type: 'outbound_imported',
      name: 'Outbound-Uebergabe importiert',
      account_id: str(account, ['id']),
      linked_record_type: 'account',
      linked_record_id: str(account, ['id']),
      linked_record_name: str(account, ['name']),
      properties: { import_batch: clone(batch), contact_count: contacts.length },
    });
    return { ok: true, status: 'imported', account, contacts, import_batch: batch, activity };
  });
}
