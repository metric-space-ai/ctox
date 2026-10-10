# Verified project Supervisor computation read

The Owner command ctox.workjet.project.supervisor.route.capabilities.v2
advertises ctox.workjet.project.supervisor.route.read.v2.
Both require the existing DataRead policy and the native Owner/project/Supervisor
binding. Payload: {project_id, thread_id}. These additive handlers leave v1
capabilities, v1 route display, default Supervisor execution and strict turn
receipts unchanged.

The generated ctox.workjet.supervisor.route-display.v2 fixture separates
configured requested selection from nullable actual published computation.
actual.selected_route identifies the native configuration sealed for that
computation; it is not an upstream claim. actual.model and native
message/request/operation IDs come from the persisted successful Messages HTTP
response joined to the original native SDK journal, including actual child
closure and query/stream drains. SDK session/turn/assistant/result IDs keep their
SDK names. They are never substituted for native command/task/attempt or run IDs.
The receipt ID is the native evidence SHA256, not an authority token.

The latest published matching Owner/project/thread/computer/configuration receipt
is revalidated in read-only Core and Policy snapshots. Unpublished completions
and changed selections are absent; changed hashes or SDK/model witnesses fail
closed. Source custody may already be closed and the execution lease replaced:
this historical data read cannot restore either. No controller, lease,
credential, consumer or model capability is reconstructed from rows or DTOs.
Private local account selectors, tokens, consumer facts and reply text are not
exported. There is no HTTP Business OS data path.

Regression fixtures cover real handler policy, publication, historical reads,
Core writer coexistence, unchanged v1, witness tampering and both generated
validators. Synthetic DB fixtures are component tests, not proof that an SDK
ran or an installed Molecularity turn succeeded.
