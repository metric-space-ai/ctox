# Native calendar foundation for Workjet Jour fixe

Workjet stores the weekly meeting's IANA time zone, weekday and local time in
`workjet_projects.jour_fixe`. The native scheduler now accepts the same local
calendar semantics and an elapsed preparation lead, without converting that
weekly appointment to a fixed UTC cron that drifts at daylight-saving changes.

`mission::schedule::ensure_task_with_calendar(root, request, calendar)` upserts
by the existing name plus explicit native thread key. `ScheduleCalendar` carries
an IANA `timezone` and `lead_minutes` (0–1440). For a Thursday 14:00 Berlin meeting,
use cron `0 14 * * 4`, zone `Europe/Berlin`, lead 120: native preparation runs at
12:00 local time in both winter and summer. Cron weekdays remain POSIX 0=Sunday;
convert the project's ISO weekday 1–7 at the verified project binding boundary.

The preparation instant is two elapsed hours before the occurrence, even across
midnight or a DST transition. A nonexistent local occurrence is skipped; a
repeated local time fires once, on its earlier UTC occurrence. Rows persist the
calendar, and due emission, manual trigger, pause/resume and subsequent runs all
use it. The prompt names the occurrence and zone. `run-now` addresses the same
explicit native thread and retains the existing durable cron message/spawn edge.

CLI example for an already verified native supervisor thread:

```sh
ctox schedule add --name 'Jour fixe preparation' --cron '0 14 * * 4' \
  --timezone Europe/Berlin --lead-minutes 120 \
  --thread-key VERIFIED_EXISTING_SUPERVISOR_THREAD \
  --skill workjet-jour-fixe --prompt 'Prepare against the confirmed goal revision.'
```

This command creates a schedule; the example is not a deployed skill or a
registration tool. Older callers and stored rows keep UTC/zero-lead semantics
and the previous serialized view. Invalid calendars fail before database effects.

The next integration must authenticate the project owner and the real Workjet
supervisor UUID/native execution binding, reconcile exactly one preparation
schedule when configuration changes, pause it on archive, and deliver artifacts
and comments to that same supervisor chat. This scheduler API alone does not
prove that bridge, install a meeting skill, generate a deck/audio, or confirm a
goal. Those remain subsequent native handlers against the merged v1 contract.
