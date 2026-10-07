# Workjet project owner identity

Project ownership remains an immutable user ID. `ctox.workjet.project.list`
returns that server-resolved ID in `result.owner_user_id`, along with its bounded
active project IDs. UI read selectors must use this correlated receipt and retain
the original authenticated session for commands.

The existing authenticated managed-user capability issuer records a UUID user's
verified email in a native-only identity registry. An authenticated legacy email
account can resolve to that UUID only while both native accounts are active.
Display names, editable profiles, command payloads and claimed email fields never
establish this relationship. A conflicting UUID claim fails closed; a new verified
email removes the old alias. Policy checks retain the original actor and role.
Project/chat ownership uses the verified canonical ID without rewriting any
existing project or working-copy owner.

Installation needs one normal authenticated managed owner bootstrap with verified
email to register the existing identity relationship. No token is rotated or
invalidated by enrollment. Until that proof exists, an email account receives only
its own projects; there is no name-based or admin-wide fallback. Existing grants,
device revocation, collection authorization and owner receipt fences remain.
