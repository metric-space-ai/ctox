# Reported limits in the native Models account read

The existing authorized `ctox.workjet.providers.list` result can include
`reportedQuota` per canonical account. Its states are `unknown`, `unavailable`
with the static storage class `quota_read_failed`, or `reported` with
`observedAtMs` and one or two normalized windows (`kind`, `status`,
`resetAtMs`, nullable `utilization`). Percentages are never inferred.

This increment reads existing Claude observations captured from real inference
responses by the gateway. It does not make a quota request, refresh a token,
change account selection, or attest successful inference. Other providers,
remote holders, inherited routes and accounts without a verified current local
binding remain unknown. Expired reported windows retain their timestamps and
must be presented as historical, not current capacity.

The native adapter captures only currently owned local targets, releases its
Policy read transaction, reads the holder-private quota under the credential
lifecycle/Runtime fence, then revalidates current Owner/Admin identity,
verified alias, holder, canonical account, revision and binding in a new Policy
snapshot before returning. Re-login cannot transfer an old observation to a
new credential or associate new limits with a stale adopted account.
Private selectors, fingerprints, generations and Secret references never enter
the result. The shell projects only the explicit public fields, including inside
quota windows.

The existing persisted registry projection stays independent of private-store
IO and does not acquire a new projection chain or collection. Mutation receipts
remain unchanged; consumers should use a subsequent normal account read for
current limits. The Workjet typed contract and Limits cell are a separate
consumer follow-up. This PR alone does not prove installed UI acceptance.

Shell generation tokens and the multi-tab epoch remain equal. This stacked
source change follows the quota lifecycle PR and is held for the Supervisor's
post-Monday release, after 12 October 13:00 Europe/Berlin. Only the Supervisor
merges and the existing writer installs it.
