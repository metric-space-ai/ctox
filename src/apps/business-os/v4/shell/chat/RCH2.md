# RCH2 – Web Research übergibt den systematischen Lauf an den Business Chat (v4)

Auftrag: V2-Verhalten (research → Chat mit vollem Prompt als Entwurf, Bediener sendet, Chat schickt
`research.systematic.run`) mit v4-Mitteln: `api.shell.navigate('ctox', {chat_draft, command_type, payload, source})`
(E37), Rahmen fängt ab und öffnet die Chatleiste mit Entwurf (E39 Nr. 4). Stub-Pin b6624eb.

## Schritte
- [x] 1 Baseline messen (10:00–10:05)
- [x] 2 Rahmen: navigate-Abfang + Entwurf in der Leiste
- [x] 3 research: Übergabe statt Direktweg (Rückfall Direktweg)
- [x] 4 Tests + Gegenprobe + scripts/research-chat-handoff.v4.mjs
- [x] 5 Abnahme-Läufe nachher, Screenshot angesehen

## Messungen

| Lauf | vorher | nachher |
|---|---|---|
| `node v4/shell/chat/tests/chat.test.mjs` | 14/14 | 14/14 |
| research v4 (`node --import ./v4/tools/node/ctox-shared.mjs --test --test-force-exit modules/research/tests/{v4-api,fix9b,fix10,research-graph-data}.test.mjs`) | 25/25 | 25/25 |
| research legacy (`cd modules/research && node --test test.mjs`) | 57/57 | 57/57 |
| research handler (`node --import … --test modules/research/handlers/tests/*.mjs`) | 24/24 | 24/24 |
| `node v4/debt/ratchet.mjs` | OK (guard 1838 · scan 3) | OK (guard 1838 · scan 3) |
| `node v4/tools/guards/guard-v4.mjs` | 1838 Treffer | 1838 Treffer (Testdateien getrennt 783 → 788, neue Testdatei) |
| `node scripts/assert-shell-v2-contract.mjs` | 38/38 | 38/38 |
| `node v4/tools/guards/textdiaet-rule.mjs --app modules/research` | 4 (help-text, vorbestehend) | 4 (dieselben) |
| `node v4/frame/lab/frame.test.mjs stub` | 219/220 laut CHAT4 (K-1k Monaco); eigener Vorher-Lauf 242/242 (lief teils schon mit der Änderung) | 242/242, Exit 0; Lab-Bilder zurückgesetzt |

## Umsetzung (Stand 10:06)
- `handlers/_lib/enqueue.js`: `systematicRunPromptName(command)`, `systematicRunInstruction(template, command)` exportiert;
  Handler nutzt dieselbe Funktion (Reihenfolge Werte → Template unverändert). Handler-Tests 24/24 danach.
- `v4/shell/chat/index.js`: `handle.openDraft(spec)` → Ereignis `ctox-business-os-chat-open` (Detail wie V2 openBusinessChat:
  draft, command_type, payload, record_id, title, source_module/source_title, reuseActive:false); `handle.lastDraft`.
- `v4/frame/index.js`: navigate('ctox', {chat_draft}) abgefangen (openChatDraft), Quelle aus dem rufenden Fenster,
  command_type nur mit eigenem Handler oder commands_out der rufenden App (Manifest), sonst Entwurf ohne Typ;
  ohne Leiste false. `canNavigate(appId, params)` im Fenster-Shell-API.
- `v4/shell/app-host.js`: `api.shell.canNavigate` (ohne Zone); navigate mit chat_draft → false ohne Zoneneintrag.
- `modules/research/index.js`: `researchCommandParams` (gemeinsam für beide Wege), `offerRunToChat` (canNavigate →
  Template per relativem fetch → systematicRunInstruction → navigate); Chat-Weg: Lauf-Kopf `chat`, kein collecting,
  kein Sprung; sonst direkter Weg unverändert.
| `node v4/shell/chat/tests/research-handoff.test.mjs` (neu) | – | 16/16, Exit 0 |
| `… research-handoff.test.mjs counter` (research = HEAD vor RCH2) | – | 5/16, Exit 1: (a) Entwurf fehlt + Befehl direkt, (b) Absender research statt Chat, (d) kein Entwurf, (c) Zone shell.navigate |
| `node scripts/research-chat-handoff.v4.mjs` (neu, CI) | – | Exit 0 (Pin b6624eb und Repo-Kopie modules/ctox/tests/fixtures/app-api-stub 68b83a3) |
| `node scripts/research-chat-handoff.v4.mjs counter` | – | Exit 1 |
| `node v4/tools/isolation/check-isolation.mjs` | 242 | 242 |

Nachtrag (10:10): `focusCtoxRun` springt nur, wenn `api.shell.canNavigate` (falls vorhanden) zustimmt – sonst meldete
der direkte Weg ohne ctox `shell.navigate` als Fehler in der Zone (vorbestehend, Gegenprobe (c) zeigt ihn).

Bilder (angesehen): `v4/shell/chat/tests/shots/rch2-1280-dock-draft.png` (Leiste offen, Anweisung im Eingabefeld),
`rch2-1280-dock-sent.png` (gesendet, „Deine Aufgabe steht auf der Aufgabenliste“).

## Manifest-Deklarationen (für den Generator-Strang, hier nicht geändert)
- shell `commands_out`: `{app: 'research', type: 'research.systematic.run'}` (der Chat sendet als App shell);
  der UI-Scan findet das nicht, weil der Typ über navigate-Parameter kommt (Generatorlauf in Scratch: shell nur ctox).
- research `commands_in`: `{type: 'research.systematic.run', from: ['shell']}`.

## Status: FERTIG (10:17)
