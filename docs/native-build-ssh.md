# Bounded native SSH execution

The native build adapter needs short pinned SSH probes and detached-launch
commands. `ctox_transfers::ssh_exec::exec` provides one pure-Rust SSH command
using the same russh 0.64.1 ring/RSA stack as the Storage transport. It
does not call an external SSH/vendor CLI or grant build authority.

A native caller must resolve the owner/computer-bound Build endpoint and enter
`with_current_computer_endpoint` for each call, using the job's saved
endpoint/grant/credential fingerprint. Only borrowed SecretStore key/passphrase
text enters the function. A scoped thread owns a current-thread Tokio runtime,
joins before the callback returns, and discards its connection tasks. A weak
key reference checks that authentication did not retain the signing credential.

The server's SHA256 public-key pin is mandatory; an optional negotiated key
algorithm further constrains it. Certificates, a wrong pin, rejected
authentication and rejected exec requests fail closed. There is one connection
attempt, with no retry, agent, PTY or environment forwarding. Command text is
generated/quoted by the native planner, bounded to 64 KiB and cannot contain
NUL. Binary stdin and combined stdout/stderr are separately bounded to 1 MiB.

Connection, authentication, command acknowledgement, input, output and
disconnect share a ten-second deadline. Runtime shutdown is bounded to one
additional second. A reported nonzero SSH exit is returned as data. Closing
without an exit status, an exit signal, duplicate status or oversized output
is an error. Callers must redact remote command output in user-visible errors.

Long source preparation and builds must be launched detached with their own
bounded runner and reconciled from persistent remote receipts. This function
does not hold the native credential fence for a full build, upload source
archives, persist jobs, select computers or perform installed fleet acceptance.

Four actual loopback SSH tests cover pinned authenticated binary transport and
exit17, a wrong pin before authentication, excessive output/missing status and
an unresponsive command's total deadline. Fixtures create temporary test keys,
listen on loopback only and terminate their owned sessions; they never use NAS
keys. Storage SFTP/SMB acceptance belongs to the Transfer PR; it is not implied
by this independent exec transport.

The native business_os::build_ssh binding resolves an owner/computer-bound
Build grant and saves its endpoint fingerprint. Resume rejects a changed saved
fingerprint; every later short operation re-enters the current grant/credential
fence before constructing borrowed SSH options. Its nonblocking Linux capacity
probe uses the prototype's slot-N.lock files, canonical lane path and granted
disk floor. Native time stamps the observation; the runner still obtains the
actual lease. Two native tests cover resumed binding/rotation/disable before a
protocol callback and denial of wrong-owner or storage-only endpoint use.
