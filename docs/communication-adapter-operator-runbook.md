# CTOX Communication Adapter Operator Runbook

Status: Draft fuer die neuen Chat-Adapter, Stand 2026-06-26.

Dieses Runbook beschreibt Betreiber-Smoke-Tests fuer die neuen nativen
Communication Adapter: Slack, Discord, Telegram, Matrix, Mattermost, Zulip und
Google Chat. Es ergaenzt den Umsetzungsplan in
`docs/communication-adapters-implementation-plan.md`.

## Sicherheitsregeln

- Provider-Tokens werden nur ueber Business OS Settings / native
  `ctox.channel.settings.save`-Commands in den CTOX Runtime-Settings-Pfad
  geschrieben.
- Tokens, App Secrets, Signing Secrets und API Keys duerfen nicht in
  Browser-replizierte Collections, Tickets, Logs, Screenshots oder Testreports.
- Testausgaben muessen vor Weitergabe redigiert werden. Erlaubt sind
  Workspace-/Guild-/Room-/Channel-IDs, Account-Keys und Provider-Message-IDs.
- Die neuen Bot-Chat-Adapter sind in v1 text-only. Attachment-Uploads bleiben
  blockiert, bis provider-spezifische Upload-, Groessen-, MIME-, Persistenz-
  und Security-Review-Pfade implementiert sind.
- Business-OS-Daten laufen weiter ueber CTOX Sync Engine / RxDB / WebRTC. Es gibt keinen
  Browser-HTTP-Datenproxy fuer Chat-Nachrichten.

## Gemeinsamer Smoke-Test

1. Channel in Business OS Settings konfigurieren und speichern.
2. `ctox.channel.test` aus dem Settings-UI ausfuehren oder lokal testen:

```sh
ctox channel test --channel <channel>
```

3. Pull-Sync ausfuehren:

```sh
ctox channel sync --channel <channel>
```

4. Persistierte Nachrichten pruefen:

```sh
ctox channel list --channel <channel> --limit 10
```

5. Text-only Send mit Review-Flag in einen erlaubten Zielkanal testen:

```sh
ctox channel send --channel <channel> --account-key <account-key> --thread-key <thread-key> --to <provider-channel-id> --body "CTOX smoke test" --reviewed-communication-send
```

6. Business OS Settings pruefen: Account Health muss Auth, Sync, letzten Erfolg,
   letzten Fehler, Rate-Limit und konkrete Scope-/Intent-/Permission-Hinweise
   ohne Secret-Leakage anzeigen.
7. Realtime-Readiness pruefen: Account Health muss `realtime_transport`,
   `realtime_config_state`, `realtime_supervision_state` und, falls vorhanden,
   `realtime_last_cursor` anzeigen. Slack Socket Mode meldet bei vollstaendiger
   App-Token-Konfiguration `supervised_via_service_sync` oder einen konkreten
   Tick-Zustand wie `starting`, `running`, `stopped`, `backing_off` oder
   `failed`. Fuer Discord Gateway und Mattermost WebSocket ist
   `not_implemented` in v1 ein erwarteter Zustand, solange kein
   supervisebarer Dauerlaeufer freigegeben ist. Zulip meldet bei vollstaendiger
   Konfiguration `events_api` plus `polling_via_service_sync`.

## Slack

Mindestkonfiguration:

- Bot Token: `xoxb-...`
- Workspace-ID
- Bot-User-ID
- erlaubte Channel-IDs
- optional App Token `xapp-...` fuer Socket Mode im Service-Sync
- optional Signing Secret fuer spaetere Events API

Provider-Voraussetzungen:

- Bot ist in den erlaubten Channels eingeladen.
- App hat die fuer `auth.test`, `conversations.history` und `chat.postMessage`
  benoetigten Bot-Scopes.
- Socket Mode ist in v1 ein bounded Service-Sync-Zyklus: CTOX oeffnet die
  Socket-URL, verarbeitet maximal einen kurzen Envelope-Burst, acked
  Envelopes, persistiert Message-Events und schliesst den WebSocket wieder.
  Events API per HTTPS ist weiterhin nicht der private Default.

Erwartetes Ergebnis:

- `test` liefert `ok: true` und einen Slack Account-Key.
- `sync` persistiert Channel- und Thread-Nachrichten mit Slack `ts` als
  Provider-ID.
- `send` persistiert ein outbound `communication_messages`-Record und speichert
  die Slack Response-ID im Message-Metadata.
- Bei konfiguriertem App Token zeigt Account Health Socket-Mode-Start/Stop,
  letzten Envelope-Cursor, Backoff-Versuch und Backoff-Grund ohne URL- oder
  Token-Leakage.
- Fehlende Scopes erscheinen als `missing_scope`; fehlende Channel-Mitgliedschaft
  oder Allowlist-Probleme als `missing_permission`.

## Discord

Mindestkonfiguration:

- Bot Token
- Application-ID
- Bot-User-ID
- erlaubte Guild-IDs
- erlaubte Channel-IDs

Provider-Voraussetzungen:

- Bot ist in der Guild installiert.
- Bot hat Leserechte und Senderechte in den erlaubten Channels.
- Fuer vollstaendigen Message Content muss der privilegierte
  `MESSAGE_CONTENT` Intent aktiviert sein. Ohne diesen Intent muss v1 auf DMs,
  Mentions oder sichtbar gelieferte Event-Felder begrenzt bleiben.

Erwartetes Ergebnis:

- `test` prueft den Bot ueber Discord REST.
- `sync` backfillt erlaubte Channels per REST.
- `send` erzeugt eine Discord Channel Message.
- Fehlende Intents erscheinen als `missing_intent`; fehlende Channel-Rechte als
  `missing_permission`.

## Telegram

Mindestkonfiguration:

- Bot Token
- Bot Username
- erlaubte Chat-IDs

Provider-Voraussetzungen:

- Bot ist im Zielchat vorhanden.
- Gruppen-Privacy ist passend konfiguriert, wenn Gruppenmessages sichtbar sein
  sollen.

Erwartetes Ergebnis:

- `test` nutzt `getMe`.
- Account Health zeigt `telegram_group_privacy_state`; bei Gruppen-Chats muss
  eine eingeschraenkte oder unbekannte Privacy-Konfiguration sichtbar sein.
- `sync` nutzt `getUpdates` und persistiert den naechsten Update-Offset.
- `send` nutzt `sendMessage`.
- Nach einem Restart setzt `sync` am letzten Offset fort.

## Matrix

Mindestkonfiguration:

- Homeserver URL
- Access Token
- User-ID
- erlaubte Room-IDs

Provider-Voraussetzungen:

- Der konfigurierte Account ist Mitglied der erlaubten Rooms.
- E2EE ist in v1 nicht als vollstaendiger Pfad freigegeben, solange SDK-State-
  Persistenz und Key-Handling nicht fertig sind.

Erwartetes Ergebnis:

- `test` prueft `whoami`.
- `sync` nutzt `/sync` und persistiert `next_batch`.
- Verschluesselte `m.room.encrypted`-Events werden nicht halb normalisiert;
  Account Health meldet stattdessen `matrix_e2ee_state:
  encrypted_events_not_supported`.
- `send` schreibt `m.room.message` Text-Events.

## Mattermost

Mindestkonfiguration:

- Server URL
- Bot/API Token
- Bot-User-ID oder Bot Username
- Team-ID
- erlaubte Channel-IDs

Provider-Voraussetzungen:

- Bot ist Mitglied der erlaubten Channels.
- Server-Zertifikat und API-Pfad sind vom CTOX Host erreichbar.

Erwartetes Ergebnis:

- `test` prueft `/users/me`.
- `sync` liest Channel Posts.
- `send` erzeugt Posts.
- TLS-, Auth- und Permission-Fehler erscheinen als Account-Status.

## Zulip

Mindestkonfiguration:

- Realm URL
- Bot Email
- API Key
- erlaubte Streams
- optional Topic

Provider-Voraussetzungen:

- Bot ist im Realm aktiv und fuer die Streams berechtigt.
- Topic-Umbenennungen und Edit-Events sind in v1 noch nicht vollstaendig als
  Updates abgebildet.

Erwartetes Ergebnis:

- `test` prueft den Bot Account.
- `sync` liest Stream-/Topic-Nachrichten, registriert eine bounded Event Queue
  fuer Message-Events, persistiert `zulip-event-last-id` und loescht die Queue
  nach dem Read.
- `send` schreibt Stream Messages.

## Google Chat

Mindestkonfiguration:

- OAuth Access Token
- User/App Label
- optional App-ID
- erlaubte Space Names

Provider-Voraussetzungen:

- Google Cloud Project mit aktivierter Google Chat API.
- OAuth Consent Screen ist fuer den Workspace passend konfiguriert; bei
  externen Apps muessen Test-/Produktionsstatus und User-Zugriff freigegeben
  sein.
- Workspace App Access Control erlaubt die App und die benoetigten Scopes.
- Workspace Admin hat die App und Scopes freigegeben.
- App oder Account ist Mitglied der erlaubten Spaces.
- Fuer User-auth Listing/Read: mindestens `chat.messages.readonly` oder
  breiter `chat.messages`.
- Fuer User-auth Send: `chat.messages.create` oder breiter `chat.messages`.
- Fuer App-auth Send als Chat-App: `chat.bot`.
- Fuer spaetere Space Events muessen die Eventtypen mit den passenden Chat-
  Scopes und Space-Memberships freigegeben werden.

Erwartetes Ergebnis:

- `test` prueft Spaces-Zugriff.
- `sync` listet Messages fuer konfigurierte Spaces.
- `send` erzeugt Space Messages.
- Fehlende Admin-Freigaben oder Scopes erscheinen als `missing_scope` oder
  `missing_permission`.

## Release-Evidence

Fuer jede Adapterfreigabe muss ein redigierter Smoke-Test-Bericht abgelegt
werden:

- Datum, CTOX Commit, Betriebssystem.
- Provider, Workspace/Server/Guild/Room/Space IDs.
- Auszug von `ctox channel test`, `sync`, `list` und optional `send`.
- Screenshot oder kopierter Text aus Business OS Account Health ohne Secrets.
- Realtime-Readiness-Felder aus Account Health:
  `realtime_transport`, `realtime_config_state`,
  `realtime_supervision_state`, optional `realtime_last_cursor`.
- Bestaetigung, dass Attachments fuer Bot-Chat-v1 abgelehnt werden.
- Bekannte Einschraenkungen: langfristiger Realtime-Dauerlaeufer, Gateway
  Resume, Mattermost WebSocket Events, E2EE, Provider-Attachments,
  OAuth-Rotation.
