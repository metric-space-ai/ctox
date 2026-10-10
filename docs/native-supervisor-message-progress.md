# Native Supervisor message progress

Selected Claude-Code Supervisors now publish incremental public Messages text
through the existing native supervisor watch command and RxDB/WebRTC transport.
The project with no selected Luma retains its existing execution path.

Request `execution_page.include_native_message_text: true` to opt in. Each
`worker.native_message_text` event includes `native_message_text` with the
native upstream message ID, native model operation ID, upstream request witness,
observed model, character offset, text, and message-completed marker. The event
remains bound to the original command, task and worker attempt. SDK turn/result IDs
are separate identities and are never substituted for these native IDs.

The bounded Models publication callback only extracts/enqueues bytes. Native
persistence runs after the callback returns, inside the original retained
controller's current Owner/computer/account/catalog/lease reservation. Each
reservation rechecks authority. No controller or provider binding is reconstructed
from a DTO. Cancelled, superseded or expired executions cannot publish late text.
A Core transaction contains the publication, including any schema initialization.

Only native successful Messages response text blocks enter this feed. Thinking,
tool arguments, rejected responses and reserved Crew metadata do not. Public text
is bounded to 64 Ki characters per message and 4096 per event; Unicode offsets,
deterministic replay and cursor backfill survive reader reopen. Existing public
assistant-text and non-opted watch responses keep their original shapes.

`completed` means the upstream message ended. It does not mean the SDK query
drained, a child worker closed, the original Supervisor result was computed,
the goal was completed, or physical execution stopped. The final computation
reader still requires its native model/journal join and guarded final publication.

Tests use isolated native/HTTP-parser fixtures; they do not prove an installed
Molecularity run or live SDK account. Installed product acceptance stays separate.
