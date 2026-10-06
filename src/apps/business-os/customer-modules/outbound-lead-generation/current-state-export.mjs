// Click-time snapshot; pure, synchronous and isolated from later live changes.
// Native Pi artifact cmd_af8021fe was repaired here for lead scoping and aliases.
export function captureResearchExport(leads, recipientStatus, capturedAt = Date.now()) {
  const clonedLeads = structuredClone((Array.isArray(leads) ? leads : []).filter(Boolean));
  const entries = new WeakMap();
  const string = value => String(value ?? '').trim();
  const aliases = contact => [contact?.person_key, contact?.sellify_person_id]
    .map(string).filter(Boolean);
  const nameEmail = contact => {
    const name = string(contact?.name || [contact?.person_vorname || contact?.first_name,
      contact?.person_nachname || contact?.last_name].filter(Boolean).join(' ')).toLowerCase();
    const email = string(contact?.person_email || contact?.email).toLowerCase();
    return name && email ? JSON.stringify([name, email]) : '';
  };
  const pick = (candidates, contact) => {
    let matches = [];
    const id = string(contact?.id);
    if (id) matches = candidates.filter(entry => string(entry.contact?.id) === id);
    if (!matches.length) {
      const keys = aliases(contact);
      if (keys.length) matches = candidates.filter(entry => aliases(entry.contact).some(key => keys.includes(key)));
    }
    if (!matches.length) {
      const key = nameEmail(contact);
      if (key) matches = candidates.filter(entry => nameEmail(entry.contact) === key);
    }
    return matches.length === 1 ? structuredClone(matches[0].status) : null;
  };
  for (const lead of clonedLeads) entries.set(lead, (lead.contacts || []).map(contact => ({
    contact: structuredClone(contact), status: structuredClone(recipientStatus(lead, contact) ?? null),
  })));
  return {
    leads: clonedLeads, capturedAt,
    sourceRecordIds: [...new Set(clonedLeads.map(lead => string(lead.id)).filter(Boolean))],
    recipientStatus: (lead, contact) => pick(entries.get(lead) || [], contact),
  };
}

export async function openResearchSnapshot(actions, blob, fileName, snapshot, cryptoApi = globalThis.crypto) {
  if (typeof actions?.openApp !== 'function') throw new Error('Spreadsheet-Anwendung kann hier nicht geöffnet werden.');
  if (!snapshot.sourceRecordIds.length) throw new Error('Dem Export fehlen gespeicherte Lead-IDs.');
  const file = new File([blob], fileName, { type: blob.type });
  const digest = await cryptoApi.subtle.digest('SHA-256', await file.arrayBuffer());
  const fileSha256 = Array.from(new Uint8Array(digest), value => value.toString(16).padStart(2, '0')).join('');
  return actions.openApp('spreadsheets', { openFile: {
    file, source_kind: 'research_generated', open_purpose: 'snapshot_report',
    report_snapshot: {
      source_module: 'outbound-lead-generation', source_collection: 'outbound_lead_generation_leads',
      source_record_ids: snapshot.sourceRecordIds.slice(), captured_at_ms: snapshot.capturedAt,
      file_sha256: fileSha256,
    },
  } });
}
