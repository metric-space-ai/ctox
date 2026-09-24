export async function addMissingDesktopIcons({ collection, entries, gridPosition, glyphFor, insertMissingSeed }) {
  const existing = await collection.find().exec();
  const visibleTargets = new Set(existing.filter((doc) => !doc.hidden).map((doc) => doc.target_module));
  const missing = entries.filter((entry) => !visibleTargets.has(entry.id));
  for (const [offset, entry] of missing.entries()) {
    const hidden = existing.find((doc) => doc.id === `desk_icon_${entry.id}`);
    if (hidden) {
      await hidden.incrementalPatch({ hidden: false, updated_at_ms: Date.now() });
      continue;
    }
    const index = existing.length + offset;
    await insertMissingSeed(collection, `desk_icon_${entry.id}`, {
      id: `desk_icon_${entry.id}`,
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
    });
  }
  return missing.length;
}

export async function arrangeDesktopIcons({ collection, knows, labelFor, gridPosition, rememberPosition, order = 'current', locale = 'de' }) {
  const docs = (await collection.find().exec())
    .filter((doc) => !doc.hidden && knows(doc.target_module))
    .sort((left, right) => order === 'name'
      ? labelFor(left).localeCompare(labelFor(right), locale) || left.id.localeCompare(right.id)
      : (left.sort_index ?? 0) - (right.sort_index ?? 0));
  for (const [index, doc] of docs.entries()) {
    const position = gridPosition(index);
    const updatedAt = Date.now() + index;
    await doc.incrementalPatch({ ...position, sort_index: index, updated_at_ms: updatedAt });
    rememberPosition(doc.id, position, updatedAt);
  }
  return docs.length;
}

export async function replaceDesktopIcons({ collection, snapshot, insertMissingSeed }) {
  const existing = await collection.find().exec();
  await Promise.all(existing.map((doc) => doc.remove()));
  for (const icon of snapshot) await insertMissingSeed(collection, icon.id, icon);
}

export function desktopIconWriteAvailability({ collection, readiness, reason, notify, retry }) {
  const disabled = !collection || readiness?.ready !== true;
  return {
    disabled,
    disabledReason: disabled ? reason : '',
    onDisabled: disabled ? () => {
      notify(reason);
      retry();
    } : undefined,
  };
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

export function dispatchDesktopChatOpen({ detail, openBusinessChat, dispatchEvent, onPersistError }) {
  const chatDetail = { ...detail, onOpenPersistError: onPersistError };
  if (typeof openBusinessChat === 'function') return openBusinessChat(chatDetail);
  return dispatchEvent(chatDetail);
}
