// knowledge.book.ingest – deklarierter Eingang für Knowledge Books anderer Apps (Strang KNOW2). Kein UI-Sender.
// Sender heute: research.knowledge.refresh.complete (ctx.command, module 'knowledge'). Payload:
//   {book:{ref}, knowledge_domain, knowledge_version_id?, title?, record_id?, source:{app, …}}
// Die Datei (Format ctox.knowledge.book.v1) liegt als files-Datensatz vor (BlobRef, Kern §16.32). knowledge prüft
// sie und legt die Elemente in knowledge_items / knowledge_runbooks ab; der Vorgang steht in knowledge_tasks.
import { ingestBook } from './_lib/book.js';

export default async function handle(command, ctx) {
  return ingestBook(command, ctx);
}
