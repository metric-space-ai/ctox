# Registered Supervisor meeting tools

The existing T−2 h preparation schedule admits a real turn into the project's
registered Supervisor. That execution receives a signed, restricted native MCP
session. The following tools use that session; an ordinary Owner MCP token is
not permission to impersonate a Supervisor.

- `business_os.jour_fixe_read`: `read_meeting`, `read_comments` or `read_transcript`, with
  `request: {project_id, meeting_id}`. The explicit meeting is required. The
  result names contract, project, meeting, state and revision and contains only
  the requested retained evidence. `read_meeting` also returns the shared
  meeting DTO and an allowlisted project configuration, with no arbitrary
  project runtime fields or secrets.
- `business_os.jour_fixe_update`: `prepare_deck` with the shared
  `PublishDeckRequest` DTO, or `propose_todos` with `ProposeTodosRequest`.
  Requests are wrapped as `{action, request}`, with strict bounded DTOs.

Every call rechecks the currently leased native command, exact payload hash,
lease identity and expiry, current actor/authority epoch, project ownership,
registered Supervisor and real thread history. The meeting must name that same
Supervisor and native thread key. Module allowlisting and MCP read/write policy
still apply. Caller-supplied project, actor or thread claims confer no authority.
The restricted session still cannot invoke generic actions, confirm goals or
admit unrelated work.

Reads use existing read-only DEFERRED Core/Policy snapshots, with no schema
initialization, master-key fence or writer reservation. Mutations acquire Core
then Policy IMMEDIATE reservations; the Core reservation fences cancellation
until the Policy metadata and immutable operation receipt commit together.
Neither path holds these transactions across provider or file operations.

A draft accepts an ordered, nonempty bounded slide list, unique stable IDs,
matching meeting IDs and the exact next deck revision. It rejects all audio
references and remains `preparing`. Replacing a draft cannot discard retained
comments, transcript or todos. This tool does not declare a narrated deck ready.

Todo proposals are accepted only during review, at the exact next proposal
revision. Each item must have a nonblank `owner`, title and acceptance criteria;
its evidence IDs must belong to this meeting. The shared optional `Todo.owner`
field preserves legacy reads, while new proposals require it. `due_at_ms`
retains the existing bounded optional deadline. Proposals remain `proposed`
with no confirmation or Core goal reference.

An exact operation replay returns its original compact mutation receipt, but
only after current authority and lease are checked again. Reuse with a different
intent, stale meeting/proposal revision or a failed metadata write has no effect.
The receipt records the actual native execution command. The operation table
is private native metadata, never a new Browser/HTTP data surface.

Narration synthesis and authorized file publication, live Supervisor
conversation and comment-triggered turns, and Owner confirmation installing a
durable Core goal remain required follow-up work. Source regression tests are
not installed Workjet meeting acceptance.

The native and served Shell must include the regenerated optional Todo.owner
contract. The canonical Shell cache stamp is bumped with this change; an older
signed slot overrides a new native source tree and must be updated by its writer.
