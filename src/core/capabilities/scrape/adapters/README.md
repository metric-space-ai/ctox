# Google adapter revisions (Bright Data SERP)

These are source artifacts for the registered `google-de` and `maps-google-com`
adapters, not the excluded local web-stack crate. They are the successors of
the deployed google-de revision 32 and maps-google-com revision 32 and were
registered on the production instance on 14 October 2026 as google-de
revision 38 and maps-google-com revision 34 (byte-identical to these files).

Why: Google answers the server IP's own searches and Maps requests only with
its robot check ("ungewöhnlicher Datenverkehr"), whether or not the owner-bound
Google login succeeds, so both targets ended `blocked` /
`authorization_required`. With the owner's approval both adapters now read
Google through the Bright Data SERP API:

- `POST https://api.brightdata.com/request` with
  `{"zone": <zone>, "url": "https://www.google.de/search?q=…&hl=de&gl=…&brd_json=1", "format": "raw"}`
  and `Authorization: Bearer <BRIGHTDATA_API_KEY>`. Do not send
  `Accept: application/json`: Bright Data then answers HTTP 200 with an empty
  body.
- The key is read at runtime with `ctox secret get --scope credentials --name
  BRIGHTDATA_API_KEY` and never printed. The zone comes from the runtime store
  key `CTOX_WEB_SEARCH_BRIGHTDATA_ZONE` (default `serp_api1`).
- Bright Data intermittently answers 200 with an empty or non-JSON body
  ("This query recently failed …") or times out; each search retries up to
  three times within its budget, each time with a different `num` variant of
  the URL because Bright Data refuses to repeat a recently failed URL.
  401/403/407 map to `authorization_required`, persistent failures to
  `temporary_unreachable` (with the provider's answer in `detail`, so an
  inactive account is classified as `provider_account_inactive`).
- Only answers whose `general.search_engine` is `google` are used.

`google-de` company mode keeps the previous extraction: candidate websites
(the website named by Google's business panel first, then organic results that
name the company and are not directories) are read with `ctox web read`, falling
back to `ctox web browser-automation`; the page must name the company (or, for
the business-panel website, carry its first distinctive name token). The
`/impressum` page of the confirmed site is read for contact data. Person mode
starts when the input carries `titles`, `title_groups` (OR-combined per group)
or `query`: all `linkedin "<company without legal form>" <titles>` searches run
in parallel within 55 s and return LinkedIn profile candidates
(`person_vorname`, `person_nachname`, `person_position`, `person_linkedin`,
source = normalized profile URL, note = hit title and snippet) whose hit names
the company's first distinctive token. Confirmation of the current employer is
left to `linkedin-com`.

`maps-google-com` reads the Google business profile from the same SERP API:
the business panel of the web search for the company (plus `city` /
`firma_ort` when given) and, in parallel, Google's local results (`tbm=lcl`).
The Maps endpoint itself is disabled on the SERP zone ("disabled due to low
success rate"). Records keep the previous format and source URL (the Maps
search URL for the company); opening hours from the panel are quoted in the
`firma_name` note.

## Installation by the existing customer-instance writer

1. Confirm the two current script revision/hash pairs equal
   `google-session-migration.json`. If either source has changed, reconcile its
   newer extraction logic first; do not overwrite it with an older source.
2. Deliver these two source files from the merged commit and verify their trimmed
   UTF-8 SHA-256 against the manifest. Keep the target configurations and secrets;
   the instance needs `BRIGHTDATA_API_KEY` (scope `credentials`) and a SERP zone.
3. Use `ctox scrape register-script --target-key <target> --script-file <file>
   --change-reason <reason>` once per target. Retain the previous revisions for
   rollback.
4. Run `ctox scrape execute --target-key <target> --trigger-kind manual
   --input-file <file>` with a real company for each adapter (and one person-mode
   input for google-de) and confirm `succeeded` with records.

`node --test src/core/capabilities/scrape/adapters/*test.cjs` exercises both
real adapter scripts through their process boundary with `child_process` and
`https` mocked: request shape (no Accept header, zone, brd_json URL), key
handling, retries with URL variants, foreign-engine rejection, company-page
challenge, person-mode queries and filtering, Maps panel/local extraction and
missing-company isolation. No provider requests are made by these tests.
