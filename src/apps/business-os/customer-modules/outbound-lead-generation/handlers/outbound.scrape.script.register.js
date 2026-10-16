// outbound.scrape.script.register — Skriptstand eines Ziels registrieren (Strang OLGSCR).
// ref: capabilities/scrape/registry.rs:221-428 register_script (ported in _lib/scrape-registry.js):
// gleicher Inhalt (sha256 des getrimmten Texts) reaktiviert die bekannte Revision, sonst max+1.
// Der Text landet in `files`; die Registry hält nur die Referenz.
import { isObject, requiredString } from './_lib/olg.js';
import { registerScript } from './_lib/scrape-registry.js';

export default async function handle(command, ctx) {
  const payload = isObject(command.payload) ? command.payload : {};
  const targetKey = requiredString(payload, ['target_key']);
  return registerScript(ctx, {
    target_key: targetKey,
    script_text: payload.script_text,
    language: typeof payload.language === 'string' && payload.language.trim() !== '' ? payload.language.trim() : 'javascript',
    change_reason: typeof payload.change_reason === 'string' ? payload.change_reason : null,
    notes: typeof payload.notes === 'string' ? payload.notes : null,
    entry_command: Array.isArray(payload.entry_command) && payload.entry_command.every((p) => typeof p === 'string') ? payload.entry_command : null,
  });
}
