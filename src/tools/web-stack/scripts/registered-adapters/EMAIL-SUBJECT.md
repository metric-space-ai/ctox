# Email validation result identity

These standalone scripts preserve the live THESEN provider workflows while
adding an explicit, provider-evidenced `subject_email` to each accepted
`person_email_validation` record. The research consumer must bind that subject
to the same person's email; request metadata alone is not evidence.

Native source baselines inspected on 2026-09-13:

- Experte revision 26, trimmed SHA-256
  `8ece6820f332313024e91338b5c9705676aea3ddfb1ee0af23681e7a35efcf16`.
- MailTester revision 1, trimmed SHA-256
  `2df110df60e643ab2d948502deb1889b240d0a901bea5ed8183cd3e0153053de`.

The registry hashes `script_body.trim()`, not raw file bytes. Both inspected
files match their registered hashes under that existing rule.

Experte extracts the subject from the matching provider table row. MailTester
extracts the unique email address in its rendered result region. Final emission
requires the requested address, returned address, explicit subject and unique
address in the provider evidence to agree. Missing or contradictory subjects,
substring collisions and ambiguous evidence cannot emit a successful field.
Existing origin, login and access-challenge checks remain enforced. These
scripts do not send email, supply login credentials or bypass provider blocks.

Run the bounded fixture tests:

```sh
node --test --test-concurrency=1 src/tools/web-stack/scripts/registered-adapters/email-subject.test.cjs
```

The 27 tests execute actual adapter entrypoints with a captured native-command
fixture and execute the generated browser programs against controlled DOM
fixtures. They are not live provider tests. Before acceptance, register each
script through `ctox scrape register-script`, execute a real provider query,
read back its persisted `subject_email`, and verify the same subject in the
actual Outbound research/UI. No registration or live acceptance is asserted by
this source document.
