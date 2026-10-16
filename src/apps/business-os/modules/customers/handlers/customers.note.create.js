// customers.note.create – ref: store_customer_commands.rs:755-787 (customers_note_create) @ 4794af03d
import {
  idFromCommand, ensureAbsent, objectPayload, validateLinks, putDefaultString, putDefaultBool, putDefaultI64,
  putDefaultObject, refreshSearchText, putRecord, writeNoteActivity,
} from './_lib.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const noteId = idFromCommand(command, ['note_id', 'id'], 'note');
    await ensureAbsent(tx, 'customer_notes', noteId, 'customer note already exists');
    const note = objectPayload(command.payload);
    note.id = noteId;
    await validateLinks(tx, note, true);
    putDefaultString(note, 'title', 'Notiz');
    putDefaultString(note, 'body', '');
    putDefaultString(note, 'body_format', 'markdown');
    putDefaultBool(note, 'is_deleted', false);
    putDefaultI64(note, 'created_at_ms', now);
    putDefaultObject(note, 'payload');
    refreshSearchText(note, ['title', 'body']);
    await putRecord(tx, 'customer_notes', noteId, now, note);
    const activity = await writeNoteActivity(tx, command, now, note, 'note_created', 'Notiz erstellt');
    return { ok: true, note, activity };
  });
}
