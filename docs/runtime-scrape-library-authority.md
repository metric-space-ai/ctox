# Runtime scrape library authority

Outbound adapter registration first reads the target and its activated revision
from the SQLite scrape registry. It preserves configuration, active revision and
hash, including intentional activation of an older revision. A script is usable
only when its nonempty persisted body and materialized file match the active
registered hash. Missing or invalid materialization requires generation/repair;
it never causes a source-tree provider script to replace the library.

A target absent from the registry is created from the adapter manifest (or the
generic prospect schema). First test/use without a usable script queues the same
bounded universal-scraping generation path as an explicit generate action. Native
registration remains the only revision writer. Source-tree fixtures remain on disk
for inspection; production adapter registration does not select them.

Generation reuses the existing target-scoped scrape repair admission function and
registered target workspace. This matches the daemon relay's existing allowed
input prefix, so the leaf writes only inside its assigned workspace and invokes
normal register-script/execute commands. It does not need daemon-root access,
upsert-target permissions, a new promotion endpoint, or wider writable roots.
Existing target repair admission deduplicates open work and preserves operator
cancellation/retry limits; nonrunnable returned work is reported generation_blocked.

The fixture first-use test creates a novel adapter, checks workspace and duplicate
admission, supplies synthetic generated JavaScript, registers it through the native
command dispatcher, and checks subsequent actual script execution/evidence. It
is not a live model-generated/browser/tenant acceptance test. Live dynamic app
creation, correct account authorization and persisted/reloaded results remain
separate acceptance requirements owned by the app task.
