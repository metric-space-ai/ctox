# Native Claude SDK account binding

The native Supervisor selection and an admitted computer must resolve the same
Owner, native account, holding instance, provider, private account selector,
account/policy revision, exact live model, discovery timestamp and catalog
fingerprint. A configured Luma label or a Workjet gateway account ID cannot
establish this binding.

`NativeClaudeSdkAccountReservation::prepare` accepts only the real
`AdmittedConsumerAuthority` and Crew's sealed `SupervisorModelEligibility`.
It reads the exact account and encrypted OAuth access/refresh credentials from
the admitted native host's secret store outside the policy/issuer/transport
fences. A double snapshot and the private discovery fingerprint reject a
changed account, credential, disabled account or unsupported upstream. Only
the official HTTPS Anthropic endpoint without a configured proxy is supported
by this SDK preparation.

Before each asynchronous dispatch/publication boundary, the registered holder
calls `with_current_configuration` and also re-enters Crew's actual native
command/confirmed-plan lease and controller-claim fence. The callback borrows
the exact model, upstream and zeroizing access token. It is synchronous and
bounded; it may not perform network/secret-store/transport reentry or re-enter
the reservation. Neither the reservation nor the configuration implements
Serialize, Debug or Clone. The refresh token is never exported to the SDK
callback.

Credentials stay on the holding computer's private registered SDK execution
path. They must not appear in a renderer claim response, browser DTO, public
producer receipt, log or replicated account metadata. The account helper does
not publish a WebRTC method or reconstruct consumer authority from request
fields.

Re-login, configuration/catalog/account/policy changes, source revocation and
release reject further use. Release is idempotent and destroys the held
zeroizing credential snapshot, including after a failed callback. Crew and
Harness must separately stop the actual SDK producer and fence its late
events; account release is not proof of an SDK/process stop or a successful
turn.

## Integration boundary

This is the account-preparation component for the genuine Crew lease and
Harness ClaudeDriver/ClaudeAdapter producer. It neither claims a native
Supervisor lease nor launches a Claude session. The private lease model dispatcher described in
[native-claude-lease-model-proxy.md](native-claude-lease-model-proxy.md)
composes the account callback with Crew's actual held-policy controller fence;
its Source broker receives only a scoped capability. A native control receiver,
registered producer session/turn witness, native review and installed
end-to-end acceptance remain separate requirements. No configured
route is reported as an executed Supervisor turn.

Regression coverage uses an isolated encrypted CTOX secret store and native
policy fixtures. It rejects selector substitution, access/refresh rotation,
configuration changes, unsupported destinations and every sealed model pin.
The model fixture comes from the recorded real Claude account GET /models
(`g3-claude-live-models-20261009.json`).
