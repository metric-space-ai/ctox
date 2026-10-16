# ROUTE – commands_out/commands_in from the UI code (E39, E40; precising §16.40)

Status 16.10.2026. Nothing committed. Only gen-manifest.mjs, gen-manifest.test.mjs, regenerated outputs and this file were changed.

## Rule (gen-manifest.mjs)
- `scanUiCommandTypes`: in the app UI code (no handlers/, tests, vendor/, dist/) or the shell trees (v4/frame, v4/shell, v4/source),
  the *type position* of `…api.command(type, …)` counts, plus the type position of app-local wrappers. Wrappers are found
  transitively across all files of the app: a parameter, a `param.type|command_type|commandType`, or a destructured field
  that flows into api.command. Literals in the expression count (including ternaries); a bare identifier resolves via the
  last `const|let|var` before the call or a unique app constant. `command_type:`/`commandType:` with a literal also counts.
  Comments are blanked out. Keys of mapping objects (`{ 'a.b': … }[x]`) do not count.
- `routeUiCommands` order (E40): own handler → own route. Platform route (`src/core/business_os/business_command_inventory.json`
  exact_control_types + control_prefixes, plus `PLATFORM_REGISTRY_ROUTES` ctox.app.{publish,update,rollback,release,remove},
  ctox.coding.{turn,cancel}) → no declaration. Exactly one foreign handler app → commands_out at the sender and commands_in
  `{from:[<sender>]}` at the target, never '*', with `_commands_in_evidence` and `drift.commands_in_derived`. Several
  handler apps → `drift.commands_out_ambiguous` (red). None → `PLATFORM_ROUTES_PENDING` (report only) or
  `drift.commands_unrouted` (red).
- `route_check.violations` → exit 1 (also without --check). `SHELL_COMMANDS_OUT` (next to SHELL_FOREIGN) carries
  {ctox, business_os.chat.task} and {ctox, ctox.chat.task}.

## Added declarations
| App | commands_out + | commands_in + |
|---|---|---|
| mail | outbound: outbound.campaign.status.set, outbound.message.approve, outbound.message.compose, outbound.message.request_approval, outbound.message.send_approved, outbound.provider.reconcile | – |
| conversations | outbound: outbound.message.approve, outbound.message.reject, outbound.reply.classify, outbound.reply.match | – |
| outbound | – | from mail: the 6 mail types; from conversations: the 4 conversations types (approve from [conversations, mail]) |
| shell | ctox: business_os.chat.task, ctox.chat.task | – |
| research | – (evidence only: modules/research/index.js:2550 for documents research.systematic.report.create) | – |

Platform routes with a handler anyway (so no commands_out derived): outbound → ctox.mailserver.* (mail), OLG → ctox.secret.* (credentials),
tickets → ctox.crew.assign (ctox), shell → ctox.coding.turn/cancel (coding-agents).

## Neither in the inventory nor with a handler (E40) – complete
Red (`drift.commands_unrouted`, 24 places, 21 types):
| App | Type | Location |
|---|---|---|
| browser | browser.capture.extract | modules/browser/index.js:1424 |
| browser | browser.http_auth.respond | modules/browser/index.js:627 |
| browser | browser.observer.grant | modules/browser/index.js:587 |
| browser | browser.observer.revoke | modules/browser/index.js:592 |
| browser | browser.upload.select | modules/browser/index.js:555 |
| browser | browser.webauthn.respond | modules/browser/index.js:636 |
| browser | ctox.browser_context.capture | modules/browser/index.js:1369 |
| buchhaltung | business_os.data.modify | modules/buchhaltung/index.js:814 |
| research | business_os.data.modify | modules/research/index.js:4898 |
| creator | ctox.business_os.app.create | modules/creator/index.js:1025 |
| desktop | ctox.business_os.app.create | modules/desktop/appCommands.js:28 |
| importer | ctox.business_os.app.create | modules/importer/index.js:173 |
| ctox | web_stack.auth_assist.request | modules/ctox/index.js:4906 |
| documents | ctox.knowledge.runbooks.manage | modules/documents/index.js:3332 |
| mail | mail.series_email.import | modules/mail/index.js:2044 |
| matching | business_os.i18n.translate | modules/matching/ui/businessOsControls.js:789 |
| outbound-lead-generation | external_sql.write | customer-modules/outbound-lead-generation/index.js:10766 |
| outbound-lead-generation | outbound-lead-generation.import | customer-modules/outbound-lead-generation/index.js:8486 |
| shell | ctox.workjet.calendar.read.v1 | v4/shell/workjet/actions/meeting.js:150 |
| shell | ctox.workjet.device.v1 | v4/shell/workjet/actions/device.js:45 |
| shell | ctox.workjet.grok.v1 | v4/shell/workjet/actions/instance.js:18 |
| shell | ctox.workjet.jour_fixe.speech.v1 | v4/shell/workjet/actions/meeting.js:204 |
| shell | ctox.workjet.speech.dictation.v1 | v4/shell/workjet/actions/meeting.js:28 |
| shell | ctox.workjet.speech.settings.v1 | v4/shell/workjet/actions/meeting.js:105 |

Not red (`PLATFORM_ROUTES_PENDING`, "reported to the kernel 16.10.2026, transfer acceptance list E40"):
| Type | Location |
|---|---|
| ctox.plan.step.retry | shell v4/shell/workjet/actions/supervisor.js:414 |
| ctox.workjet.calendar.connect.v1 | shell v4/shell/workjet/actions/calendar-connect.js:11 |
| ctox.ask | not found as a command type; only a menu ID at v4/shell/app-host.js:500, which sends ctox.chat.task |

Notes: the shell ctox.workjet.*.v1 types go through requestNative → api.command (v4/shell/workjet/v2-compat.js:309).
14 of the 21 red types are in the inventory's `browser_literal_types`, which E40 does not count as a route.
6 browser.* types appear in no inventory list at all.

## Acceptance (16.10.2026)
- gen-manifest.mjs (write): exit 1 (route_check only). --check: no differences after writing; exit 1 only from the 24 route lines.
  Later differences in research/shell come only from file hashes of other workers' edits in progress
  (research/handlers/_lib/enqueue.js, v4/shell/chat/index.js, v4/frame/index.js).
- gen-manifest.test.mjs 50/50; check-schemas 0 red; ratchet OK; workjet tests 302/304 (unchanged); mail v4-api FIX9a green.
