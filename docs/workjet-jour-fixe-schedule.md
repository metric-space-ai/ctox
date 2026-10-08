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

The native project integration authenticates the project owner and registered
Workjet supervisor binding, reconciles one preparation schedule on configuration
change and pauses it on archive. Due preparation persists one occurrence's
meeting metadata and submits a turn to that supervisor with the embedded
JourFix skill; retries keep the same meeting and actual preparation-task ID.
No database writer transaction spans supervisor execution.

`ctox.workjet.jour_fixe.meeting.read` accepts `{project_id,meeting_id?}`. It
rechecks the current project owner and supervisor binding and returns
`{ok:true,meeting:null}` for an absent latest occurrence, or the shared `Meeting`
with the actual `preparation_task_id`. An explicitly requested missing meeting
is rejected. The shell bridge exposes `project.jour_fixe.meeting.read` with
`{commandId,projectId,meetingId?}` and returns
`{action,commandId,projectId,contract,meeting,preparationTaskId?}`. The meeting
retains the shared snake_case wire fields and canonical owner; a verified alias
is not substituted into the persisted meeting. A foreign or malformed receipt
is rejected before the UI sees it.

This preparation/read foundation does not generate a completed deck/audio,
deliver comments or confirm a goal. Those remain subsequent native handlers
against the shared v1 contract; source checks do not establish installed meeting
acceptance.
