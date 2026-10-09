# Native Supervisor turn history

`project.supervisor.turn.history` reads earlier admitted requests for one native
project and its registered Supervisor UUID. It uses the existing authenticated
RxDB/WebRTC command plane and returns `ctox.workjet.supervisor_history.v1`.
No prompt is resubmitted and no assistant reply is invented or copied locally.

Workjet sends `{ action, commandId, projectId, threadId, historyPage? }`.
The optional page has `limit` (1–20, default 10) and a cursor
`{ before_created_at_ms, before_command_id }`. The native request uses
`project_id`, `thread_id`, and `history_page` with those same page fields.

The result has `historyContract` and `historyPage`. A page contains the native
`project_id`, `thread_id`, `thread_key`, and newest-first `turns`. Each entry
contains its actual `command_id`, linked `task_id`, immutable
`created_at_ms`, and at most 4096 Unicode characters of the original
`user_text`, with `user_text_truncated`. `has_more` and `next_cursor` page
backwards through earlier requests. Equal timestamps use command ID as the
stable tie breaker; a cursor must name an actual authorized native anchor.

Each selected entry reuses the submit/watch admission checks: current native
Owner, registered project/Supervisor binding, admitted envelope, canonical
intent and durable queue link. The reader uses a deferred read-only Core
snapshot and never initializes the database or enters its writer fence.
A partial native index keeps this read scoped to Supervisor chat tasks.
A removed or conflicting binding is rejected, including on final revalidation.

For assistant messages and run/attempt facts, the client watches each returned
command ID using the existing `turn.watch` execution page with
`include_public_text: true`. That reads persisted typed public chunks from the
actual attempt, rather than a local single-intent journal. Older producers
without that capability must show their actual bounded terminal result or
explicitly report unavailable public history. This operation never restores a
pending approval or claims that an imported vendor session can be resumed live.

The additive request/page types are generated from the existing native/browser
wire fixture. Matching native and signed Shell delivery, followed by the Workjet
consumer, are required before installed acceptance can pass. Unit and integration
checks and installed B3/B6 acceptance are pending for this draft.
