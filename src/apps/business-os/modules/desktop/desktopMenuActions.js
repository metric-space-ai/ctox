// Desktop-Menüaktionen (App-API 4.0): reine Funktionen über den Live-Stand `docs` von desktop_icons; geschrieben
// wird über `api.put/patch/remove` (lokal sofort, Outbox). Rückgabe = WriteHandles.

export function addMissingDesktopIcons({ api, docs, entries, gridPosition, glyphFor }) {
  const visibleTargets = new Set(docs.filter((doc) => !doc.hidden).map((doc) => doc.target_module));
  const missing = entries.filter((entry) => !visibleTargets.has(entry.id));
  const handles = [];
  for (const [offset, entry] of missing.entries()) {
    const id = `desk_icon_${entry.id}`;
    if (docs.some((doc) => doc.id === id)) {
      handles.push(api.patch('desktop_icons', id, { hidden: false, updated_at_ms: Date.now() }));
      continue;
    }
    const index = docs.length + offset;
    handles.push(api.put('desktop_icons', {
      id,
      target_type: entry.kind || 'module',
      target_module: entry.id,
      target_record_id: '',
      label: entry.title || entry.id,
      glyph: glyphFor(entry.id),
      ...gridPosition(index),
      pinned: false,
      hidden: false,
      sort_index: index,
      updated_at_ms: Date.now(),
    }));
  }
  return { count: missing.length, handles };
}

export function arrangeDesktopIcons({ api, docs, knows, labelFor, gridPosition, order = 'current', locale = 'de' }) {
  const visible = docs
    .filter((doc) => !doc.hidden && knows(doc.target_module))
    .sort((left, right) => order === 'name'
      ? labelFor(left).localeCompare(labelFor(right), locale) || left.id.localeCompare(right.id)
      : (left.sort_index ?? 0) - (right.sort_index ?? 0));
  return visible.map((doc, index) => api.patch('desktop_icons', doc.id, {
    ...gridPosition(index), sort_index: index, updated_at_ms: Date.now() + index,
  }));
}

export function replaceDesktopIcons({ api, docs, snapshot }) {
  const keep = new Set(snapshot.map((icon) => icon.id));
  const handles = docs.filter((doc) => !keep.has(doc.id)).map((doc) => api.remove('desktop_icons', doc.id));
  for (const icon of snapshot) handles.push(api.put('desktop_icons', icon));
  return handles;
}

export async function runDesktopActionOnce(pending, key, action) {
  if (pending.has(key)) return undefined;
  pending.add(key);
  try {
    return await action();
  } finally {
    pending.delete(key);
  }
}
