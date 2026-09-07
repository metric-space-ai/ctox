# Feldbefund Sync 07.09.2026 — lokale Instanz, Browser-Replikation stallt trotz PR #65

Kontext: Beweislauf für das Crew-Cockpit auf der lokalen Instanz (`~/.local/state/ctox`,
2,1 GB `business-os-rxdb.sqlite3`, 224 Collections), Binary = main `d87bc2dd3`
(enthält PR #65 „preserve framed transfers across interleaved send receipts“),
Release-Build (`cargo build --release`), Browser = Chrome 152 (eigenes Profil, CDP), Rolle `user`.

## Beobachtung
- Nativer Peer: `peerAuthenticated=true`, `dataChannelOpen=true`, Log
  „multiplexed WebRTC replication up for 224 collections“ — und danach wiederholt
  „native rxdb peer watchdog: backlog without progress for 73506ms (outbox=0, transport=1)“ /
  „… 122601ms (outbox=0, transport=5)“ sowie „request failed: Connection reset by peer (os error 54)“.
- Browser: 12 Collections mit `Timed out waiting for WebRTC response masterChangesSince`
  (business_workspace_branding, user_thread_states, business_module_catalog, business_chats,
  desktop_icons, desktop_layout, ctox_bug_reports, business_module_releases,
  business_module_reports, workjet_projects, workjet_sessions, workjet_session_transfers);
  Diagnose-Verteilung nach 3 min: 14× `connected/pending/catching-up`, 5× `error/complete/live`,
  3× `connected/complete/live`. Phase bleibt `collection-sync`.
- Peer-Performance (`business-os-rxdb-peer.status.json`, Release-Build): Loop-Dauern
  `knowledge_tables` max 61 209 ms, `business_records` max 55 249 ms, `desktop_file_index`
  max 47 630 ms, `channel_state` 19 287 ms, `ticket_state` 17 011 ms. Mit dem Debug-Build
  lagen dieselben Loops bei 200 s+ und der Prozess bei 99 % CPU.
- Dieselbe Instanz lief am 05.09. mit dem installierten Release (v0.1.44) und einem normalen
  Browser mit vollständiger Replikation (Board D4).

## These
Die langsamen Projektions-/Index-Loops des nativen Peers blockieren die Beantwortung von
`masterChangesSince` (Anfragen laufen in denselben Takt oder auf derselben Verbindung),
der Browser läuft in den Antwort-Timeout, markiert die Collection als `error`, der Peer
sieht „backlog without progress“. PR #65 behebt das Verschränken der Frames, nicht die
Antwortlatenz.

## Nicht geprüft
Ob es an der Größe dieser Instanz liegt (221 übergroße Dokumente werden beim Start
getrimmt) oder an einer Regression seit v0.1.44 — dafür fehlt ein Lauf desselben Browsers
gegen das alte Release auf derselben DB.

## Reproduktion
1. `launchctl bootout gui/$UID/com.metric-space.ctox.service`
2. Worktree mit `runtime -> ~/.local/state/ctox`; `CTOX_ROOT=<worktree> … ctox service --foreground`
3. `open -na "Google Chrome" --args --remote-debugging-port=9222 --user-data-dir=~/.cache/ctox-crew-chrome http://127.0.0.1:8765/`
4. `node src/apps/business-os/output/crew-shots/diag.mjs` (Playwright `connectOverCDP`)
