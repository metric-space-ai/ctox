# Native Codex subscription account controls

Workjet's existing provider-account enable/remove commands also cover explicitly
configured Codex subscription accounts on the native CTOX holder. Adoption binds
the concrete configuration and all three encrypted token generations. The public
receipt carries only the existing opaque account ID, revision and control flags.
No token, secret-store selector or binding fingerprint is projected.

The holder must own distinct `provider-subscriptions` references. The automatic
instance ChatGPT account, inherited main runtime route, references outside this
scope, and credentials aliased by another account remain read-only. Existing
Claude bindings and unfinished private receipts keep their prior interpretation.

Mutation retains the existing Owner/Admin admission, account/policy revision
checks, current Core command validation and Runtime CAS. Enabling or disabling
retains the configured default, including when dormant. Removing deletes only
that account's ID/access/refresh tokens at their recorded generations; a replaced
credential or recreated account prevents stale cleanup. Other accounts and their
secrets remain untouched. Last-account removal clears only that provider's model
selection. A committed holder mutation is recovered through its durable private
proof, without repeating the mutation.

This change does not add API-key account storage or Codex catalog/probe discovery.
Those remain separate provider-parity work. Qualification uses isolated fixtures;
installed Workjet acceptance and deployment belong to the existing release writer.
