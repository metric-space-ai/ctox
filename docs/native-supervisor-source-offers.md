# Native selected Supervisor Source offers

The original native service lease may issue one bounded offer for its selected, enrolled Source. The default Supervisor path does not create an offer, controller, table or token. The Source transport is Architecture's `ctox sync supervisor-source` client; this handler receives its actual admitted WebRTC peer and credential, never authority reconstructed from JSON.

The private auxiliary method is `ctox.workjet.project.supervisor.execution.v1`. Exactly one strict operation is passed:
- `{version:1, action:"poll"}`
- `{version:1, action:"claim", offer_id}`
- `{version:1, action:"status", offer_id, controller_id}`
- `{version:1, action:"cancel", offer_id, controller_id}`

UUIDs identify native objects; they grant no authority. The native Owner, computer enrollment, pairing, original command session, lease, project/thread, selected route/model/account and controller are checked again before publication. Claim replay on the same surviving native controller returns its original prompt without another controller. After a lost handler process, an already-claimed offer is rejected rather than reconstructed or started again.

Offer metadata and its prompt are stored in Core. The original restricted MCP session is stored only as an encrypted native secret with the offer UUID and bounded deadline; it is never a Source response field. A foreign Owner/computer is rejected before resolving that secret. Closing the original offer closes its controller and deletes only that secret.

This first handshake is additive and deliberately reports `execution_ready:false`. The existing service rejects an unavailable selected executor. The handshake alone neither invokes a provider nor authenticates an SDK result and cannot make a turn complete. Native model invocation and a genuinely registered SDK session/turn/result/stop producer are separate composition requirements; arbitrary Source reports of "actual" execution are rejected.

Source operations are generated from `src/core/rxdb/tests/fixtures/workjet-supervisor-source-v1.json`. Run `node src/core/rxdb/tools/build_workjet_jour_fixe_contract.mjs` to regenerate both sides. Tests use isolated native command/policy fixtures and are not installed SDK evidence.
