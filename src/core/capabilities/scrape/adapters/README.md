# Owner-bound Google adapter revisions

These are source artifacts for the registered adapters, not the excluded local
web-stack crate. They are successors to the deployed Google revision25 and
Maps revision31 inspected on 13 October. Their extraction logic stays intact;
the Google provider leg now invokes native `business-os web-stack
authenticated-automation` with GOOGLE_BROWSER_LOGIN's official secret reference
and the research command's bound owner/task. The native login gate and post-login
capture run in the same owner-bound session. A missing binding, unfinished login
or missing session fails as authorization_required; it never falls back to an
anonymous Google call. Maps no longer borrows the previous company's stored data
when the input lacks a company. Successful output retains the native session ID
for acceptance correlation; it contains no password or token.

Requires #643's Google/Maps login and email-code recipes. These files do not change
an installed adapter merely by merging or installing a binary. The customer-instance writer
must publish them as new registered script revisions in the release bundle.

## Installation by the existing customer-instance writer

1. Confirm #643 is installed and the two current script revision/hash pairs equal
   `google-session-migration.json`. If either source has changed, reconcile its
   newer extraction logic first; do not overwrite it with an older source.
2. Deliver these two source files from the merged commit and verify their trimmed
   UTF-8 SHA-256 against the manifest. Keep the target configurations and secrets.
3. Use the existing native `ctox scrape register-script --target-key <target>
   --script-file <file> --language javascript --change-reason <reason>` once per
   target. This is a deliberate version update, separate from the script-preserving
   Bright Data reprobe. Never use the app's Data Access Test to install them.
4. Read back the registered hashes and new revisions. Retain the previous script
   revisions for rollback. No target upsert or credential rotation is needed.
5. After the bundled release's load-time checks, run an actual authorized research
   lookup for each adapter. Correlate source_session.session_id with native
   auth-assist/session ownership and the same command. Successful mock tests do
   not prove Google accepted the login, nor that its access challenge is cleared.

`node --test src/core/capabilities/scrape/adapters/*test.cjs` exercises
both real adapter scripts through their child-command boundary. Provider calls
are mocked: tests cover argument/session binding, blocked login, no anonymous
fallback, missing-company isolation and empty-result handling. No production
provider requests, source registrations or emails are made by these tests.

This slice is account/session connectivity. The seven-role search and discovery
of LinkedIn names, Bright Data enrichment, contact evidence and the 20 complete
plus 10 gap-closure research acceptance remain separate required work. This
company extractor is not evidence that those role searches already work.
