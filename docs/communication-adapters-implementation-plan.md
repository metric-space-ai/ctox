# CTOX Communication Adapters: Implementation Plan

Status: In Umsetzung seit 2026-06-26

Ziel: CTOX erweitert die bestehenden nativen Kommunikationsadapter um weitere
Business- und Community-Kanaele. Prioritaet haben Slack und Discord, danach
Telegram, Matrix, Mattermost, Zulip, Google Chat und ausgewaehlte
Power-User-/Self-hosted-Adapter. Alle Adapter muessen den bestehenden
CTOX-Kommunikationsvertrag einhalten: typisierte native Adapter, dauerhafte
Message-/Thread-Persistenz, server-authoritative Policy, explizite
Outbound-Review-Gates und keine Browser-HTTP-Datenbruecke.

## Aktueller Befund

CTOX hat bereits native Adapter fuer:

- `email`: `src/core/communication/email_native.rs`
- `whatsapp`: `src/core/communication/whatsapp_native.rs`
- `teams`: `src/core/communication/teams_native.rs`
- `jami`: `src/core/communication/jami_native.rs`
- `meeting`: `src/core/communication/meeting_native.rs`

Die zentrale Adapter-Registry liegt in:

- `src/core/communication/adapters.rs`
- `src/core/communication/gateway.rs`
- `src/core/mission/channels.rs`
- `src/core/business_os/rxdb_peer.rs`

`slack`, `discord`, `telegram`, `matrix`, `mattermost`, `zulip` und
`google_chat` sind seit dem ersten Umsetzungsschritt als native CTOX
Communication Adapter registriert. Aeltere Fundstellen fuer Slack/Discord im
Repo bleiben ausserhalb des Communication-Adapter-Pfads, solange sie nicht
explizit in diese Registry integriert sind.

## Nicht Verhandelbare Regeln

1. Neue Adapter duerfen Business-OS-Daten nicht ueber HTTP zwischen Browser und
   CTOX proxien. Business-OS-Daten laufen weiter ueber CTOX Sync Engine / RxDB / WebRTC.
2. Browser-Code darf keine Anbieter-Tokens halten, persistieren oder direkt fuer
   Provider-API-Zugriffe verwenden.
3. Runtime-Konfiguration, Tokens, Bot-IDs, Workspace-IDs und Secret-Material
   muessen ueber typed config, SQLite runtime store oder CTOX secret store
   laufen, nicht ueber neue Produktions-Environment-Toggles.
4. Jeder Adapter muss `sync`, `send` und `test` als typisierte native Operationen
   anbieten.
5. Ausgehende externe Nachrichten muessen durch den bestehenden Review-/Approval
   Gate laufen, bevor Provider-Send ausgefuehrt wird.
6. Eingehende Nachrichten muessen in die bestehenden
   `communication_accounts`, `communication_threads`,
   `communication_messages` und Routing-/Evidence-Pfade normalisiert werden.
7. Anbieter-spezifische Rate Limits, Retry-After-Werte, fehlende Scopes,
   abgelaufene Tokens und Deauth-Zustaende muessen als dauerhafte
   Account-/Channel-Statusinformationen sichtbar sein.
8. Neue Adapter muessen in Business OS konfigurierbar und testbar sein, duerfen
   aber keine UI-only Permission Gates fuer mutierende Aktionen einfuehren.
9. Service-Sync darf keine unbounded Background-Prozesse starten. Laufende
   WebSocket-/Long-Poll-Verbindungen brauchen supervisebare Start-/Stop- und
   Health-Semantik.
10. Falls Docs und Code auseinanderlaufen, wird der absichtlich neue Vertrag im
    selben Aenderungssatz dokumentiert.

## Zielarchitektur

Neue Adapter folgen dem bestehenden Muster:

```text
Business OS / CLI command
  -> typed CommunicationAdapterRequest
    -> native Rust adapter module
      -> provider API / gateway / local daemon
        -> normalized CTOX communication records
          -> Business OS projection over RxDB / WebRTC
```

Adapter-spezifische Authentisierung bleibt im jeweiligen nativen Modul:

- Slack: OAuth/Bot Token/App Token/Socket Mode.
- Discord: Bot Token, Gateway Intents, Channel/Guild-Konfiguration.
- Telegram: Bot Token, Long Polling oder Webhook.
- Matrix: Homeserver Login/Access Token, Room Membership, optional E2EE state.
- Mattermost/Zulip/Rocket.Chat: Server URL plus Bot/API Token.
- Google Chat: Google Workspace OAuth und Space Membership.
- Signal: lokaler `signal-cli` Daemon ueber JSON-RPC/DBus.
- IRC/XMPP: Server-/Account-Konfiguration, TLS, Channel/JID Routing.

Eine kleine interne Normalisierungsschicht kann spaeter gemeinsame Chat-Konzepte
abbilden, soll aber keine neue generische Provider-Abstraktion erzwingen. Jeder
Adapter bleibt fuer Auth, Sync-Mechanik, Provider-IDs und Fehlermodell
verantwortlich.

## Progress Model

Der Projektfortschritt wird ueber Wellengewichtung berechnet. Eine Welle gilt
erst als abgeschlossen, wenn Implementierung, Migrationen, Tests,
Business-OS-Konfiguration, Observability und Akzeptanzkriterien erledigt sind.

| Welle | Gewicht | Status | Fortschritt |
| --- | ---: | --- | ---: |
| 0. Baseline & Adapter Foundation | 8% | In Umsetzung | 94% |
| 1. Slack Adapter | 15% | In Umsetzung | 90% |
| 2. Discord Adapter | 14% | In Umsetzung | 76% |
| 3. Telegram Adapter | 10% | In Umsetzung | 70% |
| 4. Matrix Adapter | 13% | In Umsetzung | 68% |
| 5. Mattermost & Zulip Adapter | 12% | In Umsetzung | 78% |
| 6. Google Chat Adapter | 8% | In Umsetzung | 58% |
| 7. Incubation: Signal, Rocket.Chat, XMPP, IRC | 7% | Abgeschlossen | 100% |
| 8. Business OS Settings, Pairing & Status UI | 8% | In Umsetzung | 86% |
| 9. Hardening, Guards & Release Evidence | 5% | In Umsetzung | 96% |
| **Gesamt** | **100%** | **In Umsetzung** | **81%** |

Fortschritt je Welle:

- `0%`: Noch nicht begonnen.
- `25%`: Datenmodell/API/UI-Schnittstellen der Welle sind festgelegt.
- `50%`: Kernverhalten funktioniert lokal mit Fake- oder Test-Provider.
- `75%`: Tests, Statusprojektionen, Fehlerfaelle und Guards sind vorhanden.
- `100%`: Akzeptanzkriterien erfuellt, Doku aktualisiert, keine bekannten
  Release-Blocker.

Aktueller Umsetzungsstand:

- Native Adapter-Registry, Runtime-Specs, CLI `sync`/`send`/`test`,
  Service-Sync-Wiring und Business-OS Channel-Projektion sind fuer `slack`,
  `discord`, `telegram`, `matrix`, `mattermost`, `zulip` und `google_chat`
  additiv verdrahtet.
- `src/core/communication/chat_native.rs` stellt eine gemeinsame
  REST/Long-Poll-Basis fuer `test`, text-only `send` und provider-spezifische
  Pull-Syncs bereit.
- REST-Pull-Syncs persistieren High-Water-Cursor fuer Slack
  `conversations.history`, Discord Channel Messages, Mattermost Channel Posts
  und Zulip Messages; Telegram Offset und Matrix Sync Token waren bereits
  persistent.
- Text-only Send nutzt vorhandene Thread-Key-Marker fuer provider-spezifische
  Reply-/Thread-Metadaten: Discord `message_reference`, Telegram
  `reply_to_message_id`, Mattermost `root_id` und Google Chat `thread.name`.
- Business OS Settings koennen die neuen Adapter speichern und testen; die
  Conversations UI kennt die neuen Channel-IDs, Labels, Filter und Dots.
- Channel Setup/Test/Disconnect fuer die neuen Adapter laeuft ueber denselben
  serverseitigen `integrations.manage`-Policy-Gate wie bestehende Kanaele; die
  RxDB-Projection-Allowlist enthaelt alle neuen Channel-IDs.
- `docs/communication-adapter-operator-runbook.md` beschreibt den gemeinsamen
  Betreiber-Smoke-Test sowie provider-spezifische Mindestkonfigurationen und
  erwartete Ergebnisse.
- `docs/communication-adapter-developer-guide.md` beschreibt den
  Entwicklervertrag fuer native Adapter: Datenboundary, Message-Identity,
  Statusfelder, Realtime-Verhalten, Review-Gates und Testanforderungen.
- Der Business-OS-Service-Status enthaelt eine aggregierte
  `communication_channels`-Health aus `communication_accounts.adapterStatus`;
  die Conversations-UI nutzt dieselben Statusdaten fuer Account-Health.
- Eine lokale Fake-Provider-Basis kann fuer alle neuen Chat-Adapter `test`,
  Pull-`sync`, text-only `send` und den Attachment-Fehlerpfad ohne echte Tokens
  ausfuehren und persistiert Account-Health in `profile_json.adapterStatus`.
- Attachment-Send ist fuer die neuen Bot-Chat-Adapter in v1 explizit
  text-only begrenzt: CLI-Guard und native Adapter lehnen Attachments ab, bis
  provider-spezifische Upload-, MIME-, Groessen-, Persistenz- und
  Security-Review-Pfade umgesetzt sind.
- Provider-Fehler werden fuer Business OS klassifiziert:
  `deauthorized`, `missing_scope`, `missing_permission`, `missing_intent`,
  `rate_limited` und generisches `failed`. Zusaetzlich schreibt der native
  Adapter `provider_remediation` in `adapterStatus`, und Business OS zeigt
  dazu lokalisierte Hinweise fuer Scope-, Intent-, Permission-, Deauth- und
  Rate-Limit-Faelle an.
- Realtime-/Gateway-Bereitschaft ist als durable Account-Health sichtbar:
  `adapterStatus` enthaelt Transport, Konfigurationszustand,
  Supervision-Zustand, Cursor-State-Key, letzten Realtime-Cursor und Backoff.
  Business OS zeigt diese Felder in den Accountdetails und zaehlt nicht
  implementierte bzw. nicht konfigurierte Realtime-Supervision im Service-
  Status. Echte WebSocket-/Gateway-Clients bleiben bewusst offen.
- Provider-spezifische Readiness-Luecken werden sichtbar: Telegram persistiert
  Gruppenprivacy aus `getMe`, Matrix meldet erkannte `m.room.encrypted`-Events
  als `encrypted_events_not_supported`, und Google-/Matrix-Admin-, Scope-,
  Token- und Permission-Fehler werden genauer klassifiziert.
- Noch offen sind supervisebare Slack Socket Mode / Discord Gateway /
  Mattermost WebSocket, echte Provider-Attachment-Uploads,
  Real-Provider-Smokes und ausgefuehrte breite Integrationstests.

## Welle 0: Baseline & Adapter Foundation

Gewicht: 8%

Ziel: Eine kleine, stabile Grundlage schaffen, damit Slack, Discord und weitere
Chat-Adapter nicht dieselben Registry-, Status- und Testpfade mehrfach
uneinheitlich implementieren.

Aufgaben:

- [x] Bestehende Adapterpfade in `adapters.rs`, `gateway.rs`,
  `channels.rs`, `rxdb_peer.rs` und Service-Sync dokumentieren.
- [x] `CommunicationAdapterKind` um neue Kandidaten nur dann erweitern, wenn
  der jeweilige Adapter in derselben Welle mindestens `test` und Fake-Provider
  Sync/Send liefern kann. Die neue Umsetzung liefert `test`, Pull-`sync` und
  text-only `send`; die Fake-Provider-Basis deckt diese drei Pfade ab.
- [x] Gemeinsame Account-Statusfelder fuer Bot/WebSocket/Long-Poll-Adapter
  festlegen: `auth_state`, `scope_state`, `sync_state`, `last_cursor`,
  `rate_limited_until_ms`, `last_error`, `last_success_at_ms`.
- [x] Gemeinsame Provider-ID-Normalisierung definieren:
  `provider_workspace_id`, `provider_channel_id`, `provider_thread_id`,
  `provider_message_id`, `provider_user_id`.
- [x] Einen Fake-Provider-Testpfad fuer Chat-Adapter anlegen, damit Sync/Send
  ohne echte Slack-/Discord-Tokens testbar ist.
- [x] Outbound-Review-Gate fuer alle neuen externen Channel-IDs erweitern.
- [x] Business-OS-Projection-Vertrag fuer neue Channel-IDs additiv definieren.

Akzeptanzkriterien:

- [x] Ein neuer Adapter kann ohne Browser-HTTP-Datenpfad registriert werden.
- [x] Fake-Sync erzeugt stabile Communication-Message-/Thread-Datensaetze.
- [x] Fake-Send wird ohne Approval blockiert und mit Approval auditierbar. Der
  Full-Path-Test fuer Slack-Fake-Send prueft direkte Review-Blockade,
  fehlende Approval-Row, erfolgreichen reviewed Send, `sent_at`/`send_result`
  in `communication_founder_reply_reviews` und persistierte Outbound-Message.
- [x] Fehlerstatus wird in Business OS sichtbar, ohne Secrets offenzulegen.
  Remediation-Hinweise sind Bestandteil von `adapterStatus`; Provider-Tokens
  werden vor Persistenz und UI-Anzeige redigiert.

Verifikation:

```sh
cargo fmt --check
cargo check
cargo test communication
```

## Welle 1: Slack Adapter

Gewicht: 15%

Ziel: Slack wird als erster neuer Business-Chat-Adapter umgesetzt. CTOX kann
Slack-Nachrichten aus Channels, DMs und Threads einsammeln, Threads persistieren
und freigegebene Antworten senden.

Technischer Ansatz:

- Native Moduldatei: `src/core/communication/slack_native.rs`.
- Empfohlene Rust-Bibliothek: `slack-morphism-rust`.
- Inbound fuer private CTOX-Instanzen bevorzugt ueber Socket Mode.
- Hosted/Enterprise kann alternativ Events API ueber HTTPS nutzen.
- Outbound ueber Slack Web API `chat.postMessage`.

Aufgaben:

- [x] Slack Adapter-Kind, Runtime-Spec und typed Requests ergaenzen.
- [x] Secret-/Config-Modell definieren: `workspace_id`, `bot_user_id`,
  `bot_token`, optional `app_token`, `signing_secret`, erlaubte Channel-IDs.
- [x] `test` implementieren: Auth pruefen, Bot-User und Workspace lesen.
  Konfigurierte Channel-IDs werden per `conversations.info` best-effort
  geprueft; fehlende Scopes oder Bot-Membership werden als persistenter
  `adapterStatus` sichtbar.
- [x] `sync` implementieren: `conversations.history` fuer konfigurierte
  Channels lesen und in CTOX Threads/Messages normalisieren. REST-Pull-Cursor
  werden pro Channel persistiert; Socket/Event Cursor bleiben offen.
- [x] `send` implementieren: Channel/Thread Reply und Text. Attachments und
  erweitertes Slack Formatting bleiben offen.
- [x] Socket-Mode-Service-Sync supervisebar starten/stoppen. Service-Sync
  oeffnet `apps.connections.open`, betreibt pro Tick einen bounded WebSocket-
  Zyklus, acked Envelopes, dedupliziert Envelope-IDs, speichert erlaubte
  Message-Events und schliesst den Socket wieder. Account Health meldet
  `supervised_via_service_sync`, Start/Stop/Fehler/Backoff-State und den
  letzten Envelope-Cursor; echte Slack-Live-Smokes bleiben Release-Evidence.
- [x] Rate Limits und Retry-After persistent behandeln. Gemeinsame Statusfelder,
  `Retry-After`-Erfassung, Probe-Fehlerpersistenz und ein Slack-spezifischer
  429/Retry-After-Status-Test sind vorhanden; echte Real-Provider-429-Smokes
  bleiben im Release-Evidence-Block offen.
- [x] Business OS Pairing/Settings fuer Slack Workspaces und Channels bauen.
- [x] Fake-Provider- und Tokenless-Unit-Tests ergaenzen.

Akzeptanzkriterien:

- [x] Slack Account kann in Business OS gepairt und getestet werden.
- [x] Eine Channel-Nachricht und ein Thread Reply werden korrekt persistiert.
- [x] Eine ausgehende Slack-Antwort wird ohne Freigabe blockiert.
- [x] Eine freigegebene Slack-Antwort erzeugt Provider-ID und Audit-Evidence.
- [x] Socket Mode kann reconnecten, ohne doppelte Messages zu erzeugen.
  Lokale Envelope-Dedupe- und Ack-State-Logik ist getestet; der Live-
  Reconnect-Smoke bleibt im Hardening-/Release-Evidence-Block offen.

Quellen:

- Slack Events API: https://docs.slack.dev/apis/events-api/
- Slack Socket Mode: https://docs.slack.dev/apis/events-api/using-socket-mode/
- Slack `chat.postMessage`: https://docs.slack.dev/reference/methods/chat.postMessage/
- Slack Morphism Rust: https://github.com/abdolence/slack-morphism-rust

## Welle 2: Discord Adapter

Gewicht: 14%

Ziel: Discord wird als Bot-basierter Adapter umgesetzt. CTOX kann Events ueber
Discord Gateway empfangen, Messages in erlaubten Guild-/Channel-Kontexten
persistieren und freigegebene Antworten senden.

Technischer Ansatz:

- Native Moduldatei: `src/core/communication/discord_native.rs`.
- Empfohlene Rust-Bibliothek: `serenity` fuer schnelle Umsetzung oder
  `twilight` fuer modularere Kontrolle.
- Inbound ueber Discord Gateway WebSocket.
- Outbound ueber Discord REST.

Aufgaben:

- [x] Discord Adapter-Kind, Runtime-Spec und typed Requests ergaenzen.
- [x] Secret-/Config-Modell definieren: `bot_token`, `application_id`,
  erlaubte `guild_ids`, erlaubte `channel_ids`, Intent-Konfiguration.
- [x] `test` implementieren: Bot Auth pruefen. Gateway URL,
  Application-Info, Guild-Zugriff und Channel-Zugriff werden best-effort
  geprueft; fehlende Permissions werden als persistenter `adapterStatus`
  sichtbar. Privileged-Intent-Erkennung bleibt ohne Gateway-Event offen.
- [x] `sync` implementieren: REST-Backfill fuer erlaubte Channels
  normalisieren. Discord `after`-Cursor wird pro Channel persistiert; Gateway
  Events, DMs und Mentions bleiben offen.
- [x] Message Content Limitation sichtbar machen: ohne privilegierten
  `MESSAGE_CONTENT` Intent nur DMs, Mentions oder erlaubte Event-Felder
  verwenden. Fehlende Discord-Intents werden als `missing_intent` klassifiziert
  und mit einem UI-Hinweis angezeigt.
- [x] `send` implementieren: Channel Message plus Reply/Reference, wenn der
  Thread-Key eine Provider-Message-ID enthaelt. DM bleibt offen.
- [x] Reconnect, Resume und deduplizierte Gateway-Sequenzen persistieren.
  Identify-/Resume-Payloads, monotone `discord-gateway-sequence`,
  `discord-gateway-session-id` und ein secret-freier
  `discord_gateway_resume_state` sind abgedeckt; der echte
  Gateway-Dauerlaeufer und Live-Reconnect-Smoke bleiben offen.
- [x] Business OS Pairing/Settings fuer Guilds und Channels bauen.
  Permission-Probe-Status ist sichtbar; fehlende Intents werden als
  `missing_intent` klassifiziert. Gateway-Event-basierte Intent-Validierung
  bleibt offen.

Akzeptanzkriterien:

- [x] Discord Bot kann ueber Business OS `test` geprueft werden.
- [x] Gateway-Reconnect erzeugt keine doppelten Messages. `MESSAGE_CREATE`-
  Gateway-Events werden ueber stabile Provider-Message-Keys normalisiert,
  persistieren die Gateway-Sequenz und sind per DB-Upsert dedupliziert; der
  Live-Gateway-Reconnect-Smoke bleibt im Hardening-/Release-Evidence-Block
  offen.
- [x] Fehlende Intents/Permissions werden als Account-Status gemeldet.
  Channel-/Guild-Permissions werden ueber Test-Probes gemeldet; Gateway-Event-
  basierte Intent-Validierung bleibt offen.
- [x] Ausgehende Discord-Nachrichten laufen durch Approval und Audit.

Quellen:

- Discord Gateway: https://docs.discord.com/developers/events/gateway
- Discord Message Resource: https://docs.discord.com/developers/resources/message
- Serenity: https://github.com/serenity-rs/serenity
- Twilight: https://github.com/twilight-rs/twilight

## Welle 3: Telegram Adapter

Gewicht: 10%

Ziel: Telegram wird als schneller Bot-Adapter fuer DMs, Gruppen,
Supergruppen und Channels umgesetzt.

Technischer Ansatz:

- Native Moduldatei: `src/core/communication/telegram_native.rs`.
- Empfohlene Rust-Bibliothek: `teloxide`.
- Local/private Mode ueber Long Polling.
- Hosted Mode optional ueber Webhook.

Aufgaben:

- [x] Telegram Adapter-Kind, Runtime-Spec und typed Requests ergaenzen.
- [x] Bot-Token und erlaubte Chat-IDs konfigurieren. Privacy-Mode-Hinweise
  werden in `adapterStatus.telegram_group_privacy_state` sichtbar.
- [x] `test` ueber `getMe` implementieren. Erlaubte Chat-Probes bleiben offen.
- [x] `sync` ueber `getUpdates` mit persisted offset implementieren.
- [x] `send` fuer Text implementieren. Reply-Metadaten werden gesetzt, wenn der
  Thread-Key eine numerische Telegram Message-ID enthaelt. Dokument/Attachment
  bleibt offen.
- [x] Gruppen-Privacy und nicht sichtbare Nachrichten als Status erklaeren.
  `getMe.can_read_all_group_messages` wird persistiert; bei konfigurierten
  Gruppen zeigt Business OS `Telegram-Privacy: privacy_mode_limited` bzw.
  `privacy_mode_unknown_for_groups`.
- [x] Business OS Pairing/Settings fuer Bot und erlaubte Chats bauen.

Akzeptanzkriterien:

- [x] Long Polling kann nach Restart am letzten Offset fortsetzen.
- [x] Bot-DM und Gruppenmessage werden korrekt normalisiert.
- [x] Reply-Send nutzt Telegram Reply-Metadaten, soweit verfuegbar.
- [x] Bot-Privacy-Einschraenkungen sind fuer den Nutzer sichtbar.

Quellen:

- Telegram Bot API: https://core.telegram.org/bots/api
- Teloxide: https://github.com/teloxide/teloxide

## Welle 4: Matrix Adapter

Gewicht: 13%

Ziel: Matrix wird als Open-Source-/Federation-Adapter umgesetzt. CTOX kann
Rooms synchronisieren, Messages senden und optional E2EE-faehige Accounts
unterstuetzen, sofern der SDK-State sauber persistiert wird.

Technischer Ansatz:

- Native Moduldatei: `src/core/communication/matrix_native.rs`.
- Empfohlene Rust-Bibliothek: `matrix-rust-sdk`.
- Inbound ueber Matrix Client-Server `/sync`.
- Outbound ueber Room Send API.

Aufgaben:

- [x] Matrix Adapter-Kind, Runtime-Spec und typed Requests ergaenzen.
- [x] Homeserver URL, User/Device, Access Token und erlaubte Room-IDs
  konfigurieren.
- [x] SDK-State-Persistenz klaeren, besonders fuer E2EE. v1 bleibt
  plaintext-only ohne Matrix-Rust-SDK-State-Store; `adapterStatus` meldet
  `matrix_sdk_state_persistence: not_required_plaintext_v1`. Sobald
  verschluesselte Events gesehen werden, persistiert CTOX
  `required_for_e2ee_not_configured` und
  `matrix_e2ee_policy: disabled_until_sdk_state_store`.
- [x] `test` implementieren: Login/Token und Homeserver pruefen. Room
  Membership bleibt offen.
- [x] `sync` implementieren: `/sync` Token persistieren,
  Room/Thread/Event-IDs normalisieren.
- [x] `send` implementieren: Room Message. Reply/Fallback und optional
  Markdown bleiben offen.
- [x] E2EE als explizites Feature-Gate behandeln, nicht halb implementieren.
  `/sync` zaehlt `m.room.encrypted`-Events, persistiert
  `matrix-encrypted-events-seen` und meldet `matrix_e2ee_state:
  encrypted_events_not_supported`, statt verschluesselte Inhalte halb zu
  normalisieren.
- [x] Business OS Pairing/Settings fuer Homeserver und Rooms bauen.

Akzeptanzkriterien:

- [x] Nicht-E2EE Room Sync und Send funktionieren deterministisch fuer
  text-only Pull-Sync.
- [x] Sync Token ueberlebt Restart ohne Message-Duplizierung.
- [x] E2EE wird entweder voll unterstuetzt oder klar als nicht bereit gemeldet.
- [x] Federation-/Homeserver-Fehler werden als Account-Status sichtbar.
  Matrix-spezifische `M_UNKNOWN_TOKEN`, `M_LIMIT_EXCEEDED`, `M_FORBIDDEN` und
  `M_NOT_JOINED`-Fehler werden in die bestehenden Auth-/Rate-/Permission-
  Statusklassen einsortiert; unbekannte Homeserver-/Federation-Fehler bleiben
  mit redigiertem `last_error` sichtbar.

Quellen:

- Matrix Client-Server API: https://spec.matrix.org/latest/client-server-api/
- Matrix Rust SDK: https://github.com/matrix-org/matrix-rust-sdk

## Welle 5: Mattermost & Zulip Adapter

Gewicht: 12%

Ziel: CTOX unterstuetzt zwei starke Self-hosted-/Open-Source-Business-Chat-
Systeme. Mattermost deckt Slack-aehnliche Enterprise-Installationen ab; Zulip
bringt topic-basierte Threads, die gut zu CTOX-Konversationen passen.

### Mattermost

Technischer Ansatz:

- Native Moduldatei: `src/core/communication/mattermost_native.rs`.
- REST API fuer Send/Test.
- WebSocket Events fuer Inbound.

Aufgaben:

- [x] Server URL, Bot/API Token, Team-/Channel-IDs konfigurieren.
- [x] Auth testen. Channel Membership und WebSocket-Verbindung bleiben offen.
- [x] Posts und Threads in CTOX Messages normalisieren. Files bleiben offen.
- [x] REST-Pull-Cursor pro Channel ueber Mattermost `since` persistieren.
- [x] Send fuer Posts implementieren. Thread Replies setzen `root_id`, wenn der
  Thread-Key den Mattermost Root Post enthaelt.
- [x] Rate Limits und Server-Version sichtbar machen. Gemeinsame
  Retry-After-/429-Statusfelder sind vorhanden; Server-URL, TLS-Status,
  Probe-Status und Server-Version werden fuer Mattermost und Zulip persistiert
  und in Business OS angezeigt. Provider-spezifische Rate-Limit-Smokes bleiben
  im Hardening-Block offen.

### Zulip

Technischer Ansatz:

- Native Moduldatei: `src/core/communication/zulip_native.rs`.
- REST API fuer Send/Test.
- Real-time Events API fuer Inbound.

Aufgaben:

- [x] Realm URL, Bot Email/API Key, erlaubte Streams/Topics konfigurieren.
- [x] Event Queue registrieren, Cursor persistieren und sauber loeschen.
  REST-Pull-Cursor fuer `get messages` ist persistent; zusaetzlich registriert
  der Sync-Pfad eine bounded Zulip Event Queue fuer `message`- und
  `update_message`-Events, persistiert `zulip-event-last-id` und loescht die
  Queue nach dem Read. Ein dauerhafter Supervisor bleibt offen, ist fuer den
  v1-Pull-Sync aber nicht erforderlich.
- [x] Streams und Topics auf CTOX Threads abbilden. DMs bleiben offen.
- [x] Send fuer Channel/Topic implementieren. Direct Messages bleiben offen.
- [x] Topic-Verschiebungen und Edit-Events als Status-/Message-Updates
  behandeln. `update_message`-Events aktualisieren Content nur fuer
  `message_id`; Topic-/Channel-Moves verschieben alle `message_ids` auf einen
  stabilen CTOX-Thread, ohne bestehende Message-Bodies zu ueberschreiben.

Akzeptanzkriterien:

- [x] Mattermost und Zulip koennen jeweils Account Test, Sync und Send.
- [x] Mattermost Thread Replies und Zulip Topics bleiben in CTOX stabil
  gruppiert.
- [x] Self-hosted Server-URLs und TLS-Fehler werden klar diagnostiziert.

Quellen:

- Mattermost API Reference: https://github.com/mattermost/mattermost-api-reference
- Mattermost Server: https://github.com/mattermost/mattermost
- Zulip REST API: https://zulip.com/api/rest
- Zulip Real-time Events: https://zulip.com/api/real-time-events
- Zulip Get Events: https://zulip.com/api/get-events
- Zulip Send Message: https://zulip.com/api/send-message
- Zulip Server: https://github.com/zulip/zulip

## Welle 6: Google Chat Adapter

Gewicht: 8%

Ziel: Google Chat wird fuer Google-Workspace-Kunden als Enterprise-Adapter
angeboten.

Technischer Ansatz:

- Native Moduldatei: `src/core/communication/google_chat_native.rs`.
- Google Workspace OAuth fuer Spaces, Messages und Events.
- Inbound ueber Workspace Events API oder Chat API Event Listing.
- Outbound ueber Google Chat Message API.

Aufgaben:

- [x] OAuth Client, Scopes, Workspace/App-Konfiguration dokumentieren.
  Runtime-Keys sind verdrahtet; das Betreiber-Runbook beschreibt Google Cloud
  Project, Chat API, OAuth Consent Screen, Workspace App Access Control,
  minimale User-/App-Scopes und Space-Membership.
- [x] `test` fuer Auth implementieren. Space Membership und Scope-Status
  bleiben offen.
- [x] Message Listing fuer konfigurierte Spaces implementieren.
  Subscription-basierter Sync bleibt offen.
- [x] Send fuer Space Messages implementieren. Threads bleiben offen.
  `thread.name` wird gesetzt, wenn der Thread-Key einen Google-Chat-Thread
  enthaelt; vollstaendige Thread-Lifecycle-/Subscription-Semantik bleibt offen.
- [x] Admin-/Domain-restricted-Fehler als Account-Status abbilden.
  Google Chat `ACCESS_TOKEN_SCOPE_INSUFFICIENT`, Admin-Policy- und
  Domain-Restriction-Fehler werden als Scope-/Permission-Status klassifiziert.
- [x] Business OS Pairing/Settings fuer Google Workspace und Spaces bauen.

Akzeptanzkriterien:

- [ ] Google Chat Adapter kann mit einem Workspace-Testkonto verbunden werden.
  Der native `test`-Pfad ist implementiert; Real-Provider-Smoke fehlt.
- [x] Space Messages werden ohne HTTP-Datenproxy in CTOX persistiert.
- [x] Fehlende Admin-Freigaben/Scopes sind fuer Nutzer verstaendlich sichtbar.

Quellen:

- Google Chat API Reference: https://developers.google.com/workspace/chat/api/reference/rest
- Google Chat Events: https://developers.google.com/workspace/chat/events-overview
- Google Chat Space Events: https://developers.google.com/workspace/chat/list-space-events
- Google Chat auth/scopes: https://developers.google.com/workspace/chat/authenticate-authorize
- Google OAuth consent: https://developers.google.com/workspace/guides/configure-oauth-consent
- Google Workspace App Access Control: https://knowledge.workspace.google.com/admin/apps/control-which-apps-access-google-workspace-data

## Welle 7: Incubation: Signal, Rocket.Chat, XMPP, IRC

Gewicht: 7%

Ziel: Weitere Adapter werden als Incubation bewertet. Sie werden erst
produktionsnah umgesetzt, wenn Produktnutzen, Wartbarkeit und Rechts-/Policy-
Risiken geklaert sind.

### Signal

Technisch moeglich ueber `signal-cli` JSON-RPC/DBus. Risiko: inoffizieller
Client, Telefonnummergebindung, moegliche Betriebs-/Policy-Fragen.

Aufgaben:

- [x] Nur als optionaler lokaler Power-User-Adapter planen.
  Entscheidung: Signal wird nicht als Hosted-Default geplant; ein Adapter darf
  nur gegen einen lokal betriebenen `signal-cli`-Daemon arbeiten.
- [x] Lokalen Daemon-Healthcheck und Account-Pairing pruefen.
  Minimalvertrag: Healthcheck gegen lokalen JSON-RPC/DBus-Daemon,
  serverseitiges Pairing fuer Telefonnummer/Device-Linking, keine Tokens oder
  Telefonnummern in Browser-Collections.
- [x] Kein Hosted-Default, solange Betriebsrisiken ungeklaert sind.

### Rocket.Chat

Technisch moeglich ueber REST und Realtime APIs. Kandidat fuer self-hosted
Kunden, falls Mattermost/Zulip nicht ausreichen.

Aufgaben:

- [x] API-/Realtime-Stabilitaet und Rust-Client-Lage pruefen.
  REST und WebSocket-Realtime API sind dokumentiert; Rust-Lage wirkt nicht so
  stabil/kanonisch wie Mattermost/Zulip-REST, daher erst nach konkretem
  self-hosted Kundenbedarf.
- [x] Prioritaet nach Kundenbedarf gegen Mattermost/Zulip bewerten.
  Entscheidung: niedriger als Mattermost/Zulip, aber hoeher als IRC, falls
  Rocket.Chat beim Kunden bereits betrieben wird.

### XMPP

Technisch moeglich ueber `xmpp-rs`. Wertvoll fuer offene/federierte Setups,
aber niedriger Business-Default-Nutzen.

Aufgaben:

- [x] TLS, SASL, MUC, Direct Chat und Message IDs pruefen.
  `xmpp-rs`/`tokio-xmpp` sind plausible Rust-Bausteine; produktionsnaher Wert
  haengt von Serverprofil, MUC-Konventionen, Stanza-ID-Policy und
  Federation-Fehlern ab.
- [x] Minimalen Bot-/Account-Vertrag skizzieren.
  Vertrag: JID, Passwort/OAuth-Token, erlaubte MUCs/JIDs, TLS-required,
  SASL-Mechanismus, MUC-Nick, Stanza-ID-Dedupe und Reconnect-Cursor.

### IRC

Technisch einfach, aber wenig modernes Business-Feature-Set. Als Legacy-Adapter
moeglich.

Aufgaben:

- [x] TLS, NickServ, Channels und reconnectbare Cursor-Semantik pruefen.
  Rust `irc` ist nutzbar; Cursor-Semantik bleibt wegen IRC-Verlaufsluecken
  server-/bouncerabhaengig und muss ueber ZNC/IRCv3-History oder aehnliche
  Betreiberinfrastruktur abgesichert werden.
- [x] Nur implementieren, wenn konkrete Betreiberanforderung besteht.

Quellen:

- signal-cli: https://github.com/AsamK/signal-cli
- signal-cli JSON-RPC: https://github.com/AsamK/signal-cli/blob/master/man/signal-cli-jsonrpc.5.adoc
- Rocket.Chat API: https://developer.rocket.chat/apidocs/rocketchat-api
- Rocket.Chat Realtime API: https://developer.rocket.chat/apidocs/realtimeapi
- Rocket.Chat Server: https://github.com/RocketChat/Rocket.Chat
- xmpp-rs: https://xmpp.rs/
- tokio-xmpp: https://docs.rs/tokio-xmpp
- IRC Rust crate: https://github.com/aatxe/irc
- IRC Rust crate docs: https://docs.rs/irc/

## Welle 8: Business OS Settings, Pairing & Status UI

Gewicht: 8%

Ziel: Neue Adapter sind fuer Nutzer sicher konfigurierbar, testbar und
diagnostizierbar, ohne Tokens im Browser zu persistieren oder Business OS als
HTTP-Datenproxy zu verwenden.

Aufgaben:

- [x] Communication Settings um Adapter-Typen erweitern:
  Slack, Discord, Telegram, Matrix, Mattermost, Zulip, Google Chat.
- [x] Pairing-Flows in server-authoritative Commands abbilden.
- [x] Secret-Eingabe so gestalten, dass Browser keine Secret-Kopie in
  replizierten Collections speichert.
- [x] Account-Status anzeigen: Auth, Scopes, Sync Health, Last Success, Last
  Error, Rate Limit, Provider Workspace/Server sowie Realtime-Konfiguration
  und Supervision-Zustand.
- [x] Channel-Auswahl anzeigen: Workspace/Guild/Room/Space/Stream/Topic.
- [x] Test-Button pro Adapter ueber native `test` Operation fuehren.
- [x] Send-UI und Conversations UI um neue Channel-Labels/Icons erweitern.
- [x] Permissions mit `src/core/business_os/policy.rs` und
  `src/apps/business-os/shared/permissions.js` synchron halten. Die neuen
  Channel-Commands nutzen serverseitig `integrations.manage`; es wurde bewusst
  kein UI-only Gate fuer mutierende Aktionen eingefuehrt.

Akzeptanzkriterien:

- [x] Kein Provider-Token landet in einer Browser-replizierten Collection.
- [x] Nutzer sehen konkrete Hinweise fuer fehlende Scopes/Intents/Permissions.
- [x] Neue Channel-IDs erscheinen in Conversations und Routing konsistent.
- [x] Mutierende Aktionen laufen ueber Business-OS-Commands und native Policy.

## Welle 9: Hardening, Guards & Release Evidence

Gewicht: 5%

Ziel: Die Adapter-Erweiterung ist releasefaehig, beobachtbar und gegen die
wichtigsten Ausfallmodi abgesichert.

Aufgaben:

- [x] Dedupe-Keys fuer alle Adapter pruefen:
  `<adapter>:<workspace/server>:<channel/room>:<message_id>`.
- [x] Cursor-/Offset-/Gateway-Sequence-State pro Account persistieren fuer
  Telegram Offset, Matrix Sync Token, Slack/Discord/Mattermost Channel-
  High-Water-Marks, Zulip REST Message ID und `zulip-event-last-id`;
  Slack-Socket-/Discord-Gateway-/Mattermost-WebSocket-Sequenzen bleiben offen.
- [x] Reconnect- und Backoff-Verhalten fuer WebSocket/Long-Poll Adapter testen.
  Durable Statusfelder fuer Realtime-Transport, Cursor-Key, letzten Cursor,
  Backoff-Zielzeit, Backoff-Versuch und Backoff-Grund sind vorhanden; Zulip
  Event Queue Register/Fetch/Delete und der gemeinsame capped Backoff sind als
  bounded Sync-Pfade abgedeckt, echte Dauerlaeufer-Reconnect-Smokes bleiben
  Release-Evidence.
- [x] Provider Rate Limits als testbare Fehlerpfade abdecken. Gemeinsame
  Klassifikation fuer 429/Retry-After ist vorhanden; Real-Provider-Smokes
  bleiben offen.
- [x] Attachment-Handling explizit begrenzen: Groesse, MIME, Persistenzpfad,
  Security Review. Fuer Bot-Chat-Adapter ist v1 text-only; Attachments werden
  mit begruendetem Fehler abgelehnt, bis die provider-spezifischen Upload- und
  Security-Pfade implementiert sind.
- [x] Audit-Evidence fuer Send, Failed Send und Approval Block speichern. Retry
  und Provider Deauth bleiben offen.
- [x] Runtime-Status in Service-/Business-OS-Health einhaengen. Der
  Business-OS-Status enthaelt `communication_channels` mit Ok/Warn/Bad-Counts,
  Rate-Limit-/Scope-/Permission-/Intent-Zaehlern, Realtime-Supervision-
  Zaehlern und begrenzten Issue-Zeilen.
- [x] Doku fuer Betreiber und Entwickler aktualisieren. Dieser Plan,
  `docs/communication-adapter-operator-runbook.md` und
  `docs/communication-adapter-developer-guide.md` sind aktualisiert; echte
  redigierte Real-Provider-Smoke-Berichte bleiben als Release-Kriterium offen.

Verifikation:

```sh
cargo fmt --check
cargo check
cargo test communication
node src/apps/business-os/rxdb/tests/run-all.mjs
cargo test --manifest-path src/core/rxdb/Cargo.toml
cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml
```

Release-Kriterien:

- [x] Keine neuen Produktions-Environment-Toggles.
- [x] Keine HTTP-Bridge fuer Business-OS-Daten. `node
  src/apps/business-os/scripts/assert-rxdb-only.mjs` bestaetigt den
  RxDB-only-Vertrag.
- [x] Alle neuen Channel-IDs sind outbound-review-gated.
- [x] Alle Adapter haben Fake-/Unit-Tests fuer Sync, Send, Test und Fehler.
- [ ] Mindestens Slack und Discord haben einen dokumentierten Real-Provider
  Smoke-Test mit redigierten Secrets.
- [x] Business OS zeigt Account Health ohne Secret-Leakage.

## Priorisierte Reihenfolge

1. Slack: hoechster Business-Nutzen, sehr gute API, Socket Mode passt zu
   privaten CTOX-Instanzen.
2. Discord: technisch gut, hohe Community-/DevOps-Relevanz, aber Intent-
   Einschraenkungen beachten.
3. Telegram: schnellster Bot-Adapter mit geringem Integrationsaufwand.
4. Matrix: strategisch wichtig fuer Open Source, Federation und Self-hosting.
5. Mattermost/Zulip: starke Self-hosted Business-Chats.
6. Google Chat: wichtig fuer Google-Workspace-Kunden, aber hoehere OAuth- und
   Admin-Komplexitaet.
7. Signal/Rocket.Chat/XMPP/IRC: nur nach Bedarf oder als klar markierte
   Incubation.

## Voraussichtliche Dateiberuehrungen

Core:

- `src/core/communication/adapters.rs`
- `src/core/communication/gateway.rs`
- `src/core/communication/runtime.rs`
- `src/core/communication/*_native.rs`
- `src/core/mission/channels.rs`
- `src/core/service/service.rs`
- `src/core/business_os/policy.rs`
- `src/core/business_os/rxdb_peer.rs`
- `src/core/execution/models/runtime_env.rs`
- `src/core/execution/models/runtime_state.rs`

Business OS:

- `src/apps/business-os/app.js`
- `src/apps/business-os/shared/permissions.js`
- `src/apps/business-os/shared/command-bus.js`
- `src/apps/business-os/shared/sync.js`
- Conversations/Communication module files, sobald der konkrete Modulpfad
  bestaetigt ist.

RxDB/Contracts, falls Channel-/Status-Collections erweitert werden:

- `src/core/rxdb/tests/fixtures/*.json`
- `src/apps/business-os/rxdb/src/schema.mjs`
- `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs`

Docs:

- `docs/ctox-rxdb.md`, falls neue replizierte Contracts betroffen sind.
- Betreiber-/Runbook-Doku pro Adapter.

## Offene Entscheidungen

- Soll Slack v1 nur Socket Mode unterstuetzen oder direkt Events API Webhook als
  Hosted-Mode mitliefern?
- Wird Discord v1 auf DMs/Mentions begrenzt, solange `MESSAGE_CONTENT` Intent
  nicht garantiert ist?
- Werden Attachments in v1 nur als Metadaten/Links persistiert oder bereits in
  CTOX File Storage gespiegelt?
- Soll Matrix E2EE in v1 ausgeschlossen, optional oder voll unterstuetzt
  werden?
- Soll es einen generischen `chat_platform` Helper geben oder bleiben die ersten
  zwei Adapter bewusst duplizierter, bis echte Wiederholung sichtbar ist?
- Welche Adapter brauchen Real-Provider-Smokes in CI, und welche bleiben lokale
  Betreiber-Checks wegen Secret-/Account-Abhaengigkeit?

## Plan-Aenderungslog

- 2026-06-26: Initialer Plan nach Repo-Pruefung und Online-Recherche erstellt.
- 2026-06-26: Native Adapter-Registry, REST/Long-Poll-Grundpfade,
  Business-OS-Settings, Conversations-Labels, Fake-Provider-Basis und
  `adapterStatus`-Health-Profil fuer die neuen Chat-Adapter umgesetzt. Offen
  bleiben Realtime-/Gateway-Dauerlaeufer, provider-spezifische
  Attachment-Uploads, Real-Provider-Smokes und ausgefuehrte breite
  Unit-/Integrationstests.
- 2026-06-26: Attachment-Handling fuer neue Bot-Chat-Adapter explizit auf
  text-only v1 begrenzt; CLI-Guard und native Adapter blockieren Attachments
  mit Security-Review-Hinweis, Teams-Graph-Attachments bleiben unveraendert
  erlaubt.
- 2026-06-26: Provider-Fehlerklassifikation fuer Auth-/Scope-/Permission-,
  Discord-Intent- und Rate-Limit-Zustaende ergaenzt; Business OS kann diese
  ueber `adapterStatus.provider_error_kind` und die Statusfelder anzeigen.
- 2026-06-26: `adapterStatus.provider_remediation` und lokalisierte Business-
  OS-Hinweise fuer Scope-, Intent-, Permission-, Deauth- und Rate-Limit-Faelle
  ergaenzt.
- 2026-06-26: Betreiber-Runbook fuer die neuen Chat-Adapter mit gemeinsamen
  Smoke-Test-Schritten, Slack-/Discord-spezifischen Pruefpunkten und
  Release-Evidence-Anforderungen angelegt.
- 2026-06-26: Business-OS-Service-Status um `communication_channels`-Health
  erweitert und Conversations-Account-Health an `adapterStatus` angebunden.
- 2026-06-26: REST-Pull-Sync-Cursor fuer Slack, Discord, Mattermost und Zulip
  ergaenzt; Gateway-/WebSocket-Sequenzpersistenz bleibt separater offener
  Realtime-Block.
- 2026-06-26: Text-only Reply-/Thread-Send fuer Discord, Telegram,
  Mattermost und Google Chat an vorhandene Thread-Key-Marker angebunden.
- 2026-06-26: Self-hosted-Diagnostik fuer Mattermost und Zulip ergaenzt:
  native Tests fuehren best-effort Server-Probes aus, persistieren
  URL-/TLS-/Probe-/Versionsstatus in `adapterStatus` und zeigen die Hinweise in
  Business OS an.
- 2026-06-26: Slack- und Discord-Tests um best-effort Provider-Probes
  erweitert: Slack prueft konfigurierte Channels via `conversations.info`,
  Discord prueft Gateway, Application, Guilds und Channels; Probe-Fehler werden
  in `adapterStatus` klassifiziert und in Business OS sichtbar.
- 2026-06-26: Full-Path-Guard-Test fuer Slack-Fake-Send ergaenzt: unreviewed
  externe Chat-Sends werden blockiert, reviewed Sends benoetigen eine exakte
  Approval-Row und erfolgreiche Fake-Sends markieren die Review als verbraucht
  plus persistieren eine Outbound-Message.
- 2026-06-26: Realtime-/Gateway-Supervision als durable Readiness-Status
  vorbereitet: Chat-Adapter persistieren Transport, Konfigurationszustand,
  Supervision-Zustand, Cursor-Key, letzten Realtime-Cursor und Backoff; Business
  OS zeigt diese Felder und zaehlt offene Realtime-Supervision im Service-
  Status.
- 2026-06-26: All-Adapter-Fake-Smoke ergaenzt: Slack, Discord, Telegram,
  Matrix, Mattermost, Zulip und Google Chat laufen tokenlos durch `test`,
  `sync`, `send` und den erwarteten Attachment-Fehlerpfad.
- 2026-06-26: Runtime-Key-Guard fuer die neuen Chat-Adapter ergaenzt: Slack,
  Discord, Telegram, Matrix, Mattermost, Zulip und Google Chat duerfen keine
  neuen Produktions-Feature-Toggles in ihrer Runtime-Spec einfuehren.
- 2026-06-26: Zulip Real-time Events API als bounded Sync-Pfad angebunden:
  Registrierung mit Message-Event-Filter, dokumentationskonformer Narrow,
  persistenter `zulip-event-last-id`, Queue-Cleanup und Realtime-Status
  `polling_via_service_sync`.
- 2026-06-26: Provider-spezifische Account-Health erweitert: Telegram
  persistiert Gruppenprivacy aus `getMe`, Matrix meldet erkannte
  `m.room.encrypted`-Events als nicht unterstuetzte E2EE-Readiness, Business OS
  zeigt beide Hinweise, und Matrix-/Google-Providerfehler fuer Token, Rate
  Limits, Scopes, Admin-/Domain-Policies und Permissions werden genauer
  klassifiziert.
- 2026-06-26: Google-Chat-OAuth-/Workspace-Betreiberkonfiguration im Runbook
  dokumentiert: Cloud Project, Chat API, OAuth Consent Screen, Workspace App
  Access Control, minimale Scopes fuer Message Read/Create, App-auth `chat.bot`
  und kuenftige Space-Events.
- 2026-06-26: Telegram-Akzeptanztests ergaenzt: private Bot-DMs und
  Supergroup-Messages normalisieren stabile Message-/Thread-Keys,
  Senderdaten, Body und Recipients; Reply-Send-Metadaten bleiben ueber den
  bestehenden Payload-Test abgesichert.
- 2026-06-26: Incubation-Bewertung fuer Signal, Rocket.Chat, XMPP und IRC
  abgeschlossen: Signal nur lokaler `signal-cli`-Power-User-Adapter,
  Rocket.Chat nachrangig zu Mattermost/Zulip bei konkretem Kundenbetrieb,
  XMPP mit explizitem JID/TLS/SASL/MUC/Stanza-ID-Vertrag und IRC nur bei
  konkreter Betreiberanforderung plus bouncer-/historygestuetzter Cursor-
  Semantik.
- 2026-06-26: Slack-Akzeptanztests ergaenzt/geschaerft: Channel Messages und
  Thread Replies normalisieren stabile Message-/Thread-Keys; der
  Review-Gate-Full-Path prueft jetzt zusaetzlich gespeicherte Provider-Remote-
  ID und Slack-Fake-Provider-Response im Outbound-Datensatz.
- 2026-06-26: Slack-Rate-Limit-Status geschaerft: ein
  `conversations.history`-429 mit `Retry-After` wird als `rate_limited`
  klassifiziert und mit persistentem `rate_limited_until_ms` in
  `adapterStatus` sichtbar.
- 2026-06-26: Zulip `update_message`-Events angebunden: Event Queue registriert
  jetzt `message` und `update_message`; Content-Edits aktualisieren nur die
  betroffene Message, Topic-/Channel-Moves verschieben alle `message_ids` auf
  den neuen CTOX-Thread, ohne bestehende Bodies zu leeren.
- 2026-06-26: Entwicklerdoku fuer native Communication Adapter angelegt:
  `docs/communication-adapter-developer-guide.md` beschreibt Adaptervertrag,
  Datenboundary, stabile Identitaeten, Statusfelder, Realtime-Semantik,
  Review-Gates und erwartete Tests.
- 2026-06-26: Discord-Gateway-Resume-Grundlage ergaenzt: Identify-/Resume-
  Payloads, persistente monotone Gateway-Sequenz, Session-State und
  `discord_gateway_resume_state` in `adapterStatus` sind getestet; der echte
  Gateway-Dauerlaeufer bleibt separater offener Block.
- 2026-06-26: Matrix-SDK-State-Entscheidung fuer v1 festgeschrieben:
  plaintext-only benoetigt keinen SDK-State-Store; erkannte E2EE-Events
  setzen `matrix_sdk_state_persistence: required_for_e2ee_not_configured` und
  `matrix_e2ee_policy: disabled_until_sdk_state_store`, Business OS zeigt den
  Hinweis ohne Secret- oder Store-Pfad-Leak.
- 2026-06-26: Slack-Socket-Mode-Grundlage ergaenzt: `apps.connections.open`-
  Request-Shape, Envelope-Ack-Payload, persistenter Envelope-Dedupe-State und
  `slack_socket_mode_state` in `adapterStatus` sind getestet.
- 2026-06-26: Discord-Gateway-Message-Dedupe ergaenzt: `MESSAGE_CREATE`-
  Events normalisieren ueber dieselben stabilen Message-Keys wie REST-Backfill,
  persistieren die Gateway-Sequenz und upserten doppelte Resume-Events ohne
  zweite CTOX-Message.
- 2026-06-26: Gemeinsamen Realtime-Backoff-Test ergaenzt: Adapterstatus zeigt
  neben `realtime_backoff_until_ms` jetzt auch Versuch und Grund; der Backoff
  ist deterministisch gecappt und Zulip-Queue-Fehler schreiben den Status.
- 2026-06-26: RxDB-only Guard fuer Business-OS-Datenboundary erneut
  erfolgreich ausgefuehrt; keine HTTP-Bridge fuer Communication-Adapter-Daten
  gefunden.
- 2026-06-26: Slack Socket Mode an Service-Sync angebunden: CTOX startet pro
  Tick einen bounded WebSocket-Zyklus, acked Envelopes, dedupliziert
  Envelope-IDs, speichert erlaubte Message-Events und persistiert
  Start/Stop/Backoff-Status ohne unbounded Hintergrundprozess.
