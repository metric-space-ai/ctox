// The research fields as the operator maintains them (owner 09.10.2026: "die
// zu recherchierenden Infos anlegen, ändern, hinzufügen, löschen"). Built-in
// fields keep their keys; the operator renames them, switches them off, or
// adds own fields. Own fields carry a `custom_` key and travel to the research
// worker apart from the native field vocabulary.

export const CUSTOM_FIELD_PREFIX = 'custom_';
export const CUSTOM_GROUP_ID = 'custom';
export const CUSTOM_GROUP_LABEL = 'Eigene Felder';
const CUSTOM_KEY = /^custom_[a-z0-9_]{1,48}$/;
const AREAS = new Set(['company', 'contact']);

export function isCustomFieldKey(key) {
  return CUSTOM_KEY.test(String(key || ''));
}

export function normalizeCustomFields(value) {
  const seen = new Set();
  return (Array.isArray(value) ? value : [])
    .map((field) => ({
      key: String(field?.key || '').trim(),
      label: String(field?.label || '').trim().slice(0, 80),
      area: AREAS.has(field?.area) ? field.area : 'company',
      description: String(field?.description || '').trim().slice(0, 400),
    }))
    .filter((field) => isCustomFieldKey(field.key) && field.label && !seen.has(field.key) && seen.add(field.key));
}

export function normalizeFieldLabels(value) {
  const labels = {};
  for (const [key, label] of Object.entries(value && typeof value === 'object' ? value : {})) {
    const text = String(label || '').trim().slice(0, 80);
    if (/^[a-z][a-z0-9_]{1,63}$/.test(key) && text) labels[key] = text;
  }
  return labels;
}

// "Umsatz 2024 (Mio €)" -> custom_umsatz_2024_mio; unique against existing keys.
export function customFieldKey(label, existingKeys = []) {
  const base = String(label || '')
    .toLowerCase()
    .replace(/ä/g, 'ae').replace(/ö/g, 'oe').replace(/ü/g, 'ue').replace(/ß/g, 'ss')
    .normalize('NFKD').replace(/[̀-ͯ]/g, '')
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 40) || 'feld';
  const taken = new Set(existingKeys);
  let key = `${CUSTOM_FIELD_PREFIX}${base}`;
  for (let n = 2; taken.has(key); n += 1) key = `${CUSTOM_FIELD_PREFIX}${base}_${n}`;
  return key;
}

// Built-in groups with the operator's names, without switched-off fields, plus
// one group for the own fields.
export function fieldGroupsFor(baseGroups, { labels = {}, disabled = new Set(), custom = [] } = {}) {
  const groups = baseGroups.map((group) => ({
    ...group,
    fields: group.fields
      .filter(([key]) => !disabled.has(key))
      .map(([key, label]) => [key, labels[key] || label]),
  })).filter((group) => group.fields.length);
  if (custom.length) {
    groups.push({
      id: CUSTOM_GROUP_ID,
      label: CUSTOM_GROUP_LABEL,
      fields: custom.map((field) => [field.key, labels[field.key] || field.label]),
    });
  }
  return groups;
}
