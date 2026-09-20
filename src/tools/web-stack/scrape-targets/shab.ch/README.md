# SHAB adapter draft

Work in progress; not deployed, registered, compiled or live-accepted.
The source uses the existing roxmltree dependency and public official API:
https://www.amtsblattportal.ch/docs/api/ . The generic target runner routes
SHAB to the native source, whose results depend on native field serialization.

The initial bounded implementation checks exact company names (including legal
form), company UID, publication state and provider origin. It extracts only the
subject's new/current block, never the auditor or previous company block.
Evidence carries publication date and is explicitly historical; it is not a
confirmation of current register state. No activity status or officers are
inferred. Inline fixtures are synthetic, not live provider evidence.

Open before acceptance:

- Compile and execute parser, source routing and serializer regressions.
- Review historical notice versus current-field writeback semantics end to end.
- Add identity-refined pagination: currently more than twenty title hits fail
  explicitly with partial_output; no incomplete search becomes no_match.
- Strengthen date/schema/metadata identity validation and verify actual HR
  publication variants with appropriate raw fixtures.
- Exercise positive public API cases and same-name/multiple-UID cases.
- Register a versioned runtime script/target and verify the real native
  research, persistence and reload path after reviewed deployment.

This draft must not be merged as an accepted complete adapter. Work is deferred
behind the first real existing-campaign lead acceptance, not a replacement for it.
