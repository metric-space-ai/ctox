# Native Jour fixe deck narration

A signed Supervisor `prepare_deck` publication (including presentation
`publish_deck`) commits a native narration outbox row in the same Policy
transaction as the deck and publication receipt. The agent publishes its
presentation; it no longer calls `narrate` once per slide.

One file-locked native producer per state root processes one bounded deck at
a time. Publication kicks the producer immediately. The existing daemon
router timer resumes queued/running jobs before its workload early returns,
so closing Workjet or ending the Supervisor lease does not suppress speech.
No speech request runs inside a writer transaction or a Supervisor turn.

Each slide is read from the current native `narration_input`. Its exact stored
text is synthesized through the existing configured native speech gateway;
the existing narration path freezes WAV files/chunks, hashes and provenance
and sets the meeting to `ready` only when every slide retains audio.

The publication captures canonical Owner, project, Supervisor and actor epoch.
Fresh active actor, project ownership/binding, epoch and file-policy checks
still run before synthesis and before the audio commit. A new deck revision
invalidates the old job. This private native authority is not a caller-supplied
MCP context and grants no extra tool or collection access.

Native operation IDs are stable per meeting/deck/slide. Existing slide
operations keep their IDs and retry budgets. Completed audio is retained;
uncertain/reserved operations are never resynthesized automatically. A failure
keeps the meeting preparing and blocks the outbox with a bounded diagnostic.
After correcting a prerequisite, replay the original publication operation
and request: the same publication receipt requeues the blocked deck, and the
existing native narration path permits only safe failed attempts below its
three-attempt limit. A daemon restart resumes an interrupted job but does not
infer that a potentially in-flight provider request failed.

The six-slide regression publishes through the real meeting tool, expires the
Supervisor lease, then drives native production with a verified WAV fixture.
It checks six immutable audio records, ready state and replay without another
producer call. Other regressions cover same-operation retry after missing
voice, Owner revocation and uncertain provider results.

WELSCH installation and real meeting/audio acceptance are performed by the
Supervisor after merge. Unit results alone do not certify that acceptance.
