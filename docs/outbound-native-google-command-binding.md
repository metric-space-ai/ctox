# Native person-research adapter command binding

A real THESEN `web_stack.person_research` command on 2026-10-14
(`cmd_person_research_e89c52602e44add28d2f8841`) returned
`authorization_required` from Google and Maps without a source session.
The native handler used the portable Web Stack entry point. That path passed
company/country to a CLI child but not the signed owner/task binding.
Registered Google/Maps revision 32 rejects missing owner/task before login;
this failure therefore does not establish that the Google password was rejected.

The native handler now supplies the existing registered-target dispatch callback.
Before each target it revalidates the persisted command's current authorization,
issues an internal command session in memory and calls the existing Scrape
executor directly. The executor verifies that session and binds the input to
the command's actor, task, company, country and lead.

An explicit nonempty source policy remains authoritative. Only a typed
person-research command with the default source policy can select a built-in
target from the same country, requested-field and privacy tables as Web Stack.
Chat workers still require their explicit source policy. No account-state
suppression or probe grant is bypassed.

Successful command-bound adapter outcomes may expose a bounded
`source_session`: source ID, browser session ID, native owner/task, run ID and
completed login status. Raw authentication output is discarded. Native research
results collect these references as `source_sessions`. This correlation is
adapter-reported login evidence; it does not itself prove correct extracted
contacts or completion of all requested roles.

Regression coverage uses a real signed capability admitted through the normal
command path, then the actual native dispatch and registered script executor.
The script is an isolated fixture with no network or real credentials.
Foreign input, an unplanned target, a missing command and a role revoked after
admission are rejected. Scope tests retain country/field/privacy and explicit
policy restrictions; trace tests exclude private auth fields.

Production acceptance still requires installation by the sole THESEN writer,
a genuine Google and Maps result with matching source-session references,
the authorized Bright Data reprobe, then 20 full researches and 10 gap closures.
Fixture results are not provider acceptance. No customer email is a test.
