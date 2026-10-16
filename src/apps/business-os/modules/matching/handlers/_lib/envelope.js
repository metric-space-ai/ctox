// Schema-Hülle der matching-Collections. collections.schema.json verlangt
// {id, kind, title, status, data, updated_at_ms}; die nativen Rust-Importer
// schrieben flache Dokumente direkt in business_records (an der Schema-
// Prüfung vorbei). Die Hülle entspricht ui/businessOsDataSource.js
// toPersistedDocument; die UI liest beide Formen (fromPersistedDocument).
const KIND = { sources: 'source', requirements: 'requirement', requirementSources: 'requirementSource', objects: 'object', matches: 'match' };
const SOURCE_TYPE = { objects: 'object', matches: 'match' };

export function envelope(uiName, doc, nowMs, extra = {}) {
  const kind = doc.kind || KIND[uiName] || uiName;
  return {
    id: doc.id,
    kind,
    title: doc.title || doc.name || doc.label || `${kind} ${doc.id}`,
    source_type: SOURCE_TYPE[uiName] || 'requirement',
    source_ref: doc.sourceUrl || doc.source_ref || doc.id,
    definition_id: extra.definition_id || '',
    schema_version: extra.schema_version || '',
    data: { ...doc, __ctox_kind: kind },
    status: doc.status || 'active',
    created_at_ms: nowMs,
    updated_at_ms: nowMs,
  };
}

// Lesen wie fromPersistedDocument: `data` falls Objekt, sonst flach; eine
// UI-geschriebene Hülle mit data.requirement/data.object wird aufgefaltet.
export function unwrap(row, nested) {
  if (!row) return null;
  const data = row.data && typeof row.data === 'object' && !Array.isArray(row.data) ? row.data : row;
  if (nested && data[nested] && typeof data[nested] === 'object') return { ...data, ...data[nested] };
  return data;
}
