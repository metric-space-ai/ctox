# Leadfeeder production dependency repair

The active web-stack dependency is Workjet `native/web-stack`, not the excluded
CTOX `src/tools/web-stack` Rust copy. The pin advances from
`b80535e2d49f578e183abf891ff45db31f1dc483` to its direct child
`aea789e1d097162472527cd3da2d4afc69e3bdeb` (Workjet PR70). Only the Leadfeeder
adapter changed upstream; no unrelated Workjet main commits are included.

The legacy adapter no longer requests an undocumented contacts endpoint after
a valid lead response. It requires an explicit numeric account ID and uses the
documented date/pagination parameters. It extracts documented company fields,
not synthetic or administrative email addresses. This remains bounded legacy
visitor lookup, not a new v1 API integration or proof of a configured account.

Verification must resolve the new Git source and run its new tests, including
`successful_leads_survive_missing_contacts_endpoint` and
`unsupported_contact_fixture_is_not_person_evidence`. The expected filtered
result is 17 passing tests and one ignored live test. Never use `--ignored` as
an automatic fallback. Run on THESEN with the shared exclusive verification
lease, two workers and the bounded supervisor; no local compilation:

```sh
cargo test --release --locked -p ctox-web-stack --lib --jobs 2 -- sources::leadfeeder::tests --test-threads=2
```

The older 12-pass result tested the old Git pin and is not evidence for this
repair. Live credentials, account entitlement and actual provider data still
require separate verification. No account is marked connected by this change.
