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

## Existing enrollment selection

The managed service owns the actual installed native executable and its original
native root. Workjet computer labels, SSH runtime roots and managed/paired UI
instance IDs do not identify an encrypted native account. Nothing here pairs a
computer, copies credentials, or creates an Owner assignment.

Use `ctox sync supervisor-source-selected INSTANCE COMPUTER IPC_DIRECTORY --root ROOT`
to resolve and retain the actual existing account in one process. INSTANCE is
the original native account's canonical instance ID; COMPUTER is the actual
assigned Workjet computer ID. These expected non-secret identities select an
association to verify, never authorization. The original root remains a managed
runtime input and cannot be selected by an IPC operation.

Selection examines at most 128 encrypted authority records and four active
candidates for that instance. Each candidate must authenticate and answer the
protected `ctox.workjet.consumer.v1` exchange. Any unverifiable candidate,
expired routing, changed candidate set, missing association or multiple matches
fails closed. This path never renews routing or changes the encrypted enrollment;
normal protected enrollment maintenance remains separately owned. Every probe
retains its existing bounded native readiness/RPC deadlines; probes run serially.
Unselected peers are shut down, and no IPC listener is published before a unique
match and a fresh native association check. The retained Source keeps the same
credential and exact peer-generation guards on dispatch/publication.

The startup JSON adds `source:{version:1,targetId,instanceId,publicIdentity,
accountEpoch,peerId,generation,consumer}`. Consumer is the authenticated native
association, including Owner, actor, computer revision and original device
pairing/proof-key facts. No token, credential fingerprint, private key, routing
password or account credential is serialized. Verify the emitted instance and
computer against the managed selection before using the socket. These facts
are a snapshot, not an execution permit; the receiver still captures and
revalidates real native authority for every operation.

`ctox sync supervisor-source-lookup INSTANCE COMPUTER IPC_DIRECTORY --root ROOT`
performs the same current selection, emits the source facts with
`transportReady:false,executionReady:false`, and shuts down without opening a
socket. It uses only existing enrollment but creates disposable query databases
for the authenticated probes. Its peer generation has already retired when the
command exits: do not treat it as a retained controller or session. Prefer the
selected serving command for SDK integration so that lookup and start cannot
silently bind different accounts. The old explicit TARGET serving command is
compatible and now also reports these original association facts.
