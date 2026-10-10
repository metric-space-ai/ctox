# Durable native Supervisor source transport

`ctox sync supervisor-source TARGET IPC_DIRECTORY` opens a separately owned
query-only native data session for an existing encrypted native account. The
managed Root/NodeService consumer must own this process and its private IPC directory across UI Quit;
a BrowserWindow or guest never owns this connection. This command neither
installs a service nor enrolls, assigns, grants, claims or starts an SDK producer.

The original native target public identity, instance, account epoch, actor and
possession-bound device come from NativeTransferAccountHost's existing native
BusinessData account store. First enrollment remains the real protected native
pairing path. Its pairing must be associated with the selected workjet_computers
record by the existing Owner computer.assign policy. A computer label, desktop
guest credential, default-account choice or transfer grant cannot replace it.

The process waits for native pin/nonce admission, checks the actual source
identity/principal proof, then calls ctox.workjet.consumer.v1 on the exact
accepted generation. Returned non-secret association facts must match that
original native actor, epoch, pairing, device and key thumbprint. They cannot
construct AdmittedConsumerAuthority. The receiving guarded Supervisor handler
alone captures that private Rust authority and owns lease/account/SDK validation.

The only forwarded operation method is
ctox.workjet.project.supervisor.execution.v1. Before each exchange the current
association must still match. The private IPC accepts a four-byte big-endian
length followed by JSON {version:1,requestId,params:[operation]}; the operation
must be one object and is validated by the receiving native handler. Neither
target, computer identity nor admission credentials are selectable in this
envelope. Responses are correlated by requestId, with result.kind=reply and
result.reply, or kind=unavailable with a fixed transport code. Unavailability
is not an operation rejection or cancellation receipt; an already-dispatched
operation may have an unknown outcome. The managed consumer must resolve the
original operation/lease state rather than infer that no remote action occurred. Requests are at most 256 KiB, responses
at most 1 MiB; two local clients and ten-second RPC/publication deadlines bound
work. IPC uses the existing private Unix host with same-UID checks, private
rights, directory lock and inode-preserving cleanup; no HTTP/TCP bridge exists.

Dispatch uses mandatory physical WebRTC send guards. Local response writes
re-enter the exact peer generation and the original encrypted enrollment and
credential fingerprint at every physical poll. Rotation, revocation, epoch or
pairing replacement, expiry and shutdown fence pending writes. Shutdown retires
publication synchronously before draining IPC/native resources. Current-generation
transport fences alone establish no actor or execution permission.

Expired initial routing may renew through the original authenticated native
provisioning path once, before exposing IPC. It cannot select a new account.
An exposed source never silently rebinds a controller/proxy/SDK handle to a
replacement generation. An expired or replaced connection fails closed; its
owner must drain the producer/controller and explicitly reopen transport.

The start response reports transportReady:true and executionReady:false. It is
not a lease, model request, SDK witness, completed Supervisor turn or installed
B6 acceptance. Crew supplies the protected offer/claim/SDK lifecycle DTOs and
native controller; Models supplies the private scoped account proxy; Harness
and the actual Workjet managed service must consume this process and retain
the genuine SDK session/turn through Quit. Those integrations remain open.
