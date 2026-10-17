# BOOTR – echter v4-Startweg im Labor (boot-real.browser.mjs)

Stand: 2026-10-16, Zweig shell-v4/boot-real (Basis 62a71ed3e). Belege: /Volumes/tmp/shell-v4-boot-evidence/.

Lauf (Repo-Wurzel):
`PLAYWRIGHT_MODULE=$HOME/Documents/ctox/src/apps/business-os/node_modules/playwright node src/apps/business-os/v4/tools/boot/tests/boot-real.browser.mjs [counter] [--probe=routes,caps,swdb,files]`
Exit 0 = grün, 2 = rot an einem benannten offenen Bruch (EXPECTED_OPEN), 1 = sonst rot / Gegenprobe rot.

Prüfstand: Laborserver mit genau den E38-Routen (/, /sw.js = echter v4/sw.js, /kernel/*, /business-os/*, Startkontext mit
Cookie; sonst 404, jeder Pfad protokolliert). Fixture per addInitScript vor boot.js (WebSocket-Relais, nativer
RTCPeerConnection-Partner = FakeNativePeer mit Ed25519-signierten Seiten). Native Seite veröffentlicht business_apps
'shell' (active_ver v4lab-<hash>) + business_app_versions 'shell@<ver>' mit dem Manifest aus
v4/manifests/shell.manifest.json (120 Dateien, alle sha256 = Dateibytes, also nicht veraltet) und eine Prüfstand-App 'notes'.
Bytes nur per transfer_get über WebRTC; /~fixture/* ist ausschließlich der Speicher/Code des nativen Partners.

## Echter Weg (ohne Probe): rot an OPEN-1, Exit 2
- (a) grün: 200 / → 200 /business-os/v4/boot.js → 200 /kernel/boot.mjs.
- OPEN-1: kernel/boot.mjs:8 und kernel/transport-webrtc.mjs:3 importieren `../rxdb/src/hello.mjs`, kernel/session.mjs:2
  `../shared/async-timeout.js` → `404 /rxdb/src/hello.mjs`, `404 /shared/async-timeout.js`; Konsole
  `[ctox v4 boot] TypeError: Failed to fetch dynamically imported module: …/kernel/boot.mjs`. bootV4 läuft nie.

## Hinter dem Bruch (nur Probe; jede Stufe weicht vom echten Weg ab)
- routes → OPEN-2: hello/hello_ok, 6 Spuren, 120 transfer_get, Aktivierung received → fetched → failed
  `needs_update:agent.coding.cancel,agent.coding.turn`. kernel/kernel.mjs:1067 prüft opts.capabilities ∪
  transport.capabilities; WebRTCKernelTransport hat kein capabilities, bootV4 (kernel/boot.mjs:164-167) reicht keine weiter.
- routes,caps → OPEN-3: activated, dann Loader `load_failed: Failed to fetch dynamically imported module:
  …/~a/shell@<ver>/v4/frame/index.js` (SW 404). kernel/loader.mjs:10,17-18 registriert `/sw.js` ohne `?db=`;
  v4/sw.js:73 liest den Kernel-DB-Namen nur aus `?db=` → jede /~a/-Antwort 404.
- routes,caps,swdb → OPEN-4: Rahmen steht, aber SW 404 für `/~a/shell@<ver>/shared/shell-v2-chrome.css` und
  `/shared/shell-v2-topbar.css`: v4/frame/index.js:200,204 lädt sie, v4/manifests/shell.manifest.json führt sie nicht.
- routes,caps,swdb,files: (a)–(f) grün, Rahmen gestylt, Starter zeigt 'Notizen'; (g) rot nur wegen der Probe-Routen.

## Gegenprobe (counter, falsche sha256 für v4/frame/index.js): Exit 1
- mit allen Proben: Rahmen fehlt, Kern meldet activation failed `artifact_missing:v4/frame/index.js` (der Transport
  verwirft die Bytes schon mit 'Transfer hash mismatch', kernel/transport-webrtc.mjs:127; runActivation
  kernel/kernel.mjs:1057-1058 macht daraus artifact_missing, der hash_mismatch-Zweig ist über WebRTC unerreichbar).
- ohne Proben: Rahmen fehlt, aber schon wegen OPEN-1 (nicht aussagekräftig, bis OPEN-1..4 geschlossen sind).
