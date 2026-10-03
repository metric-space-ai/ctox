# Office notification contract repair

## Observed failure

On Welsch beta17, opening a not-yet-prepared spreadsheet failed with
`CTOX product sync push timed out: spreadsheet_blob_chunks`. Our previously
saved beta9 acceptance spreadsheet could open and edit, but two C1=23 save
attempts returned to unsaved state, with no readable error notification.
The draft was retained. No durable-save success is claimed.

Both Office modules called optional `notifications.error()` / `.success()`.
The actual Shell V2 service exposes `show`, `showSystem`, `close`, `clearAll`,
and `destroy`, not those convenience methods. Optional chaining silently
discarded 23 notification callsites. This is a feedback delivery defect;
it does not establish the underlying WebRTC failure's cause.

## Correction

- All 23 callsites use the existing `show({type,message})` contract, retaining
  messages and the conditions under which they are emitted.
- The two editor save-error handlers use `time:0`, with a translated Close
  action. The service has no separate close button: its existing action
  mechanism dismisses the toast. Dismissal does not save or clear the draft.
- Spreadsheet console output includes code/message strings and the original
  error, supporting direct and nested `payload.error` errors.
- No automatic retry, false success, permission/data-path changes, Shell API
  compatibility aliases, or layout changes.
- Persistent here means no expiry timer. Existing capacity eviction, clearAll,
  and Shell teardown can still remove a notification.

## Evidence

Native bounded Pi proposal `1788752487987`: one request, success, no writes.
The actual Shell API was independently checked: the proposal's assumed close
control did not exist, so the implementation uses its existing action button.

`qa:office-notifications` runs Chromium against the actual Shell notifications
module and actual extracted Office error-handler bodies. The editor lifecycle
and dirty-marker collaborator are controlled fixtures, not a full mounted SDK.
It verifies direct/nested errors, inactive handlers, visible `role=alert`, dirty
and saving state, persistent error options, and user dismissal. A supplementary
inventory forbids the invalid convenience-method calls in either Office module.
Baseline: 2 passed / 6 failed. Corrected: 8 passed / 0 failed.
Existing Documents, Spreadsheets, and Shell notification tests: 91 passed.
Existing vendored document-format duplicate-key warnings remain unchanged.

The browser regression is included in both CI and the signed shell release
workflow. Live save/reopen, native CLI writeback, and full production acceptance
remain separate required gates. This fix alone does not make Office production
ready or justify a success claim for any saved document.
