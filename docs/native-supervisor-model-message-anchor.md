# Native model response message identity

The selected Supervisor's native model proxy records response_message_id from the
successful upstream message_start.message.id or buffered message.id, alongside
the separately observed response_model. Outgoing request IDs, the requested
model and Source SDK correlation are never copied into this field.

A conflicting response message identity fails the native observation. Content
deltas and non-success HTTP responses cannot create the identity. Existing
request rows receive a nullable column and retain NULL until a real upstream
message is observed; this does not reclassify previous requested-only records.

This provides an anchor for joining a genuine parent assistant SDK event to its
native upstream response. It does not itself prove an SDK session, completed
turn, physical process exit or drained event stream. The default Supervisor
route and service activation remain unchanged.
