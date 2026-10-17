// customers.note.update – ref: store_customer_commands.rs:789-830 (customers_note_update) @ 4794af03d
import {
  requiredFromPayloadOrRecord, loadRequired, mergeFields, validateLinks, refreshSearchText, putRecord,
  writeNoteActivity,
} from './_lib.js';

const FIELDS = [
  'title', 'body', 'body_format', 'author_id', 'account_id', 'contact_id', 'opportunity_id', 'linked_note_id',
  'payload',
];

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const noteId = requiredFromPayloadOrRecord(command, ['note_id', 'id'], 'note_id is required');
    const note = await loadRequired(tx, 'customer_notes', noteId, 'customer note not found');
    mergeFields(note, command.payload, FIELDS);
    await validateLinks(tx, note, true);
    refreshSearchText(note, ['title', 'body']);
    await putRecord(tx, 'customer_notes', noteId, now, note);
    const activity = await writeNoteActivity(tx, command, now, note, 'note_updated', 'Notiz aktualisiert');
    return { ok: true, note, activity };
  });
}
