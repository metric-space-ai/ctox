# THESEN Google authentication and independent SERP fallback

Scope: native source/auth repair; production is read-only during diagnosis.
The 12 October diagnosis performed no source test, login, provider call,
script registration, deployment, restart, queue change, secret read or mailbox
read. On 13 October, the supplied Google credentials were verified by normal
login in the Codex browser as lena thesen. That separate browser session is not
proof of a CTOX server auth session or a successful Google/Maps adapter run.

## Evidence and cause

Read-only SSH on 12 October 2026 identified `srvki1` serving
`native-main-6e23c6f24ba9`. The Google registry manifest reports revision 25;
its public script is SHA256
`55adaa626512dc582d682a0a9704e5b3a19d249e0dbe20a415c24a2145c29e27`.
`serpPage` calls plain `web browser-automation`; it neither supplies a
source/owner auth session nor uses `GOOGLE_BROWSER_LOGIN`. The script sends
`failure_mode: blocked` after an access challenge. The adapter's configured
`browser_session` label therefore does not establish a completed login.

The supervisor's independently verified 10:30 report states 476/485 Google
runs have hit an access challenge since the 5 October move. This count was not
recomputed by this change. The sampled public script confirms the missing
login integration; it does not prove that a login would clear the IP challenge.

Code boundaries on base `8fdc9cf3ec3fac84fd35e4cd17c9c7212c8f9a8c`:

- `src/core/capabilities/scrape/execute.rs`: automatic reauthorization runs
  only for `AuthorizationRequired`. A `Blocked` search challenge does not
  enter it. Relabeling CAPTCHA as expired credentials would be incorrect.
- `src/core/capabilities/scrape/reauth.rs`: protected login configuration uses
  the compiled provider recipe or an operator target plus validated script
  login URL. Google had no native fallback profile; rev25 has no protected
  login declaration either.
- `src/core/service/business_os.rs::enqueue_web_stack_auth_assist_request`:
  native source defaults previously covered only RocketReach. Google had no
  native account-login URL/domain/credential fallback. A worker still needs
  the requesting owner's bound session; no synthetic owner is acceptable.
- `business_os.rs::run_business_os_web_stack_auth_assist_login_with_continuation`:
  email OTP is attempted only after a detected MFA/login challenge and only
  if `web_stack_email_otp_recipe` supports the source. Google was absent.
  The existing mailbox lookup requires the bound recipient/owner, recent
  inbound mail, provider sender domain, and rejects ambiguous concurrent codes.
- `src/tools/web-stack/src/unlock.rs::run_public_browser_fallback`: this is a
  credential-free browser warm-up for consent/ordinary redirects and challenge
  observation. It is not an account-login/mailbox-code workflow.

The local `src/tools/web-stack` crate is excluded from the CTOX workspace;
CTOX consumes the pinned Workjet web-stack dependency. Editing only that
local copy would not repair the installed binary. This slice changes compiled
CTOX code instead.

Bright Data is separate: the LinkedIn target is revision 9, starts at
`https://api.brightdata.com/`, and its script SHA256 is
`a910e742d26f0abca1ab904c41ffa37ca05a8ed291f6147b8e450cfebe5e60d1`.
The supervisor verified `HTTP 400: Customer is not active` since
26 September 13:29 UTC (640 runs), with no SERP zone. These counts/account
facts are supervisor evidence, not a new provider probe. A stored key is not
proof that the provider account or the separate SERP product is enabled.

## First slice

Compiled native profiles now bind Google's account login to
`https://accounts.google.com/ServiceLogin`, domains `google.de`/`google.com`,
and only the `GOOGLE_BROWSER_LOGIN` secret reference. Existing RocketReach
behavior is retained. Reauthorization and manual auth assist share that native
profile; a hot-revised adapter cannot select a Bright Data key or foreign host.
Google and the actual Outbound Google Maps source ID `maps.google.com` share
this compiled Google login/credential boundary. Google Maps previously had no
native default when the app omitted its credential reference. Both source IDs
join the existing provider-domain-restricted email OTP path; source identity
normalization is consistent with the native login profile, including case and
surrounding whitespace. Foreign lookalike source/sender domains and Bright Data
remain excluded. The existing mailbox ownership, freshness, recipient and
ambiguity checks remain unchanged.
A search challenge remains blocked. This is auth plumbing, not proof that
Google SERP works or that the access challenge has been cleared.

## Remaining implementation, in order

1. Keep separate provider targets/status/run receipts: Google browser,
   Bright Data LinkedIn, and Bright Data Google SERP. A Bright Data failure
   must neither disable Google nor replace Google's browser/login settings.
2. Wire the versioned Google SERP script to an owner-bound authenticated
   browser session using the native profile, never raw credentials in script,
   argv or output. Reuse that exact source/owner session for subsequent searches.
   Detect account login/MFA separately from `/sorry/` traffic challenges.
   Test owner resolution, stored session reuse, MFA/no-email-code and CAPTCHA.
3. Complete a Google login through existing auth assist only in the released,
   authorized product flow. If Google offers an email factor, use the existing
   owned mailbox path. Push/SMS/security-key factors or a surviving search
   challenge must expose the exact remaining operator step. Do not claim
   access merely because a password form disappeared.
4. Michael must reactivate the Bright Data account and configure its SERP
   zone. The native SERP adapter needs a typed zone setting and secret reference;
   no guessed zone, no use of the LinkedIn dataset as search. For a browser
   challenge only, dispatch one bounded fallback through that separate target.
   Preserve the browser failure and actual fallback run/transport provenance;
   API account inactivity is not another login attempt or script repair.
5. Test Google success while Bright Data is inactive/missing, independent
   LinkedIn failure, challenge with SERP success, missing zone, account
   inactivity, timeout, malformed provider output and no duplicate fallback.
   Stored company/person claims still require real URL/quote evidence.
6. Supervisor merges the reviewed PR; Claude as sole THESEN writer registers
   compatible script revisions and ships one bundle. Verify exact binary/script
   revisions, native run receipts and persisted results. Never use the App
   button 'Datenzugriff prüfen': it can overwrite registered scripts.

## Primary references

- [Google account verification](https://support.google.com/accounts/answer/63950?hl=EN)
  documents verification mail and `account-verification-noreply@google.com`.
- [Bright Data SERP request](https://docs.brightdata.com/api-reference/rest-api/serp/scrape-serp)
  requires the dedicated SERP zone in the request.

Installation and live acceptance remain outstanding. Releasing or testing
this slice must not read installed app protected files or bypass policy.
