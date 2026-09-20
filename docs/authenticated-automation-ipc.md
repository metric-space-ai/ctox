# Authenticated automation script transport

`ctox business-os web-stack authenticated-automation` reads the custom JavaScript
from the calling CLI standard input. Only this subcommand reads stdin. The input
must be nonempty UTF-8 and at most 1 MiB; oversize input is rejected, never truncated.

When the daemon is available, the existing BusinessOsWebStack IPC request carries
the script in its optional `source` field. The daemon validates the same byte limit
and command binding and passes the source to the existing authenticated login plus
continuation function. It never reads daemon stdin. Other commands reject supplied
source; older requests without source remain valid for those commands. An older
client calling authenticated-automation without source receives an explicit error.
Upgrade client and daemon together: an older daemon does not implement this field.

The existing source/task/command-session owner resolution, credential lookup,
redaction, and same-session post-login continuation remain authoritative. Scripts
use secret references through that existing contract; no provider-specific code or
new credential permissions are introduced. A session ID passed to the standalone
browser automation CLI is not a substitute for this owner-bound login flow.

The local fallback receives the same already-read source, so connection fallback
does not try to read stdin a second time. The existing IPC envelope limit remains
32 MiB, allowing JSON escaping of the bounded source. This transport change does
not alter script registration or grant writes to a canonical scrape workspace.
