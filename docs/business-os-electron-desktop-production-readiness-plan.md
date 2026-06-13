# Business OS Electron Desktop: Production Readiness Plan

Status: In Umsetzung, Stand 2026-06-14

Ziel: Die CTOX Business-OS Desktop-App wird eine Slack-artige Desktop-Oberfläche,
in der Nutzer schnell zwischen mehreren CTOX Instanzen wechseln können. Die App
kann gleichzeitig ctox.dev-managed Instanzen, lokale CTOX Instanzen,
SSH-managed unmanaged Instanzen und manuell gekoppelte Signaling-Instanzen
führen.

Der Business-OS-Datenpfad bleibt unverändert: Business-Daten, Commands, Dateien,
Modul-Kataloge und Runtime-Status laufen ausschließlich über CTOX DB/RxDB
WebRTC. HTTP darf nur Shell, Login, Launch-Kontext, Pairing-Bootstrap und
administrative Control-Plane-Aktionen liefern.

## Aktueller Stand

Der Electron-Zielpfad ist unter `src/apps/business-os-desktop/` neu angelegt.
Der aktuelle Stand ist noch nicht production-ready, aber die Baseline ist jetzt
wieder testbar:

- Electron Main/Preload/Renderer-Shell ist angelegt.
- Gemischtes Instanzmodell für `ctox_dev`, `local_daemon`, `ssh_managed` und
  `pairing_invite` ist vorhanden.
- Registry normalisiert Instanzen, trennt `usage.lastUsedAt` von
  Instanz-Metadaten und blockiert secret-artige Klartextfelder.
- ctox.dev Session-Package- und Launch-Token-Quelle ist als Contract-Adapter
  vorhanden.
- ctox.dev Login-Fenster ist im Main-Prozess verdrahtet; nach erfolgreichem
  Desktop-Auth-Redirect wird die Instanzliste über denselben Electron
  Cookie-Jar aktualisiert.
- Der Desktop-Login-Einstieg nutzt jetzt den live vorhandenen
  `/dashboard?desktop=1&client=ctox-business-os-desktop` statt des nicht
  ausgerollten `/desktop/auth` Pfads. Completion-Erkennung akzeptiert den
  echten `ctox-business-os-desktop://auth/callback`, den Dashboard-
  `auth_completed=1`-Fallback und den alten `/desktop/auth/complete` Contract.
- ctox.dev-managed Instanzen haben eine Verwaltungsaktion zurück in die
  ctox.dev Control Plane; die Zielroute ist das live vorhandene
  `/dashboard?tenant=<tenant-id>` statt des live 404 liefernden
  `/desktop/instances`. Die Desktop-Abmeldung löscht den ctox.dev-Origin aus
  dem Electron Default-Session-Jar und aktualisiert die gemischte Instanzliste.
- ctox.dev Session-Package und Launch-Token werden nicht gecacht: lokale
  Contract-Tests und der Electron-Smoke beweisen, dass serverseitig entzogene
  Tenant-Mitgliedschaften aus der Desktop-Liste verschwinden, unmanaged
  Instanzen sichtbar bleiben und pro Aktivierung ein frischer Launch-Token
  samt rotiertem Launch-Kontext geholt wird.
- Jede Instanz kann aus der Sidebar in eine native Detail-/Settings-Fläche
  geöffnet werden; managed Instanzen zeigen ctox.dev-Verwaltung statt lokaler
  Löschung, unmanaged Instanzen können lokal aus der App entfernt werden.
- Local-Daemon-Quelle kann `ctox business-os peer status`, `peer ensure` und
  `business-os install --target` auf CLI-Contract-Ebene nutzen.
- Lokale unmanaged Instanzen ueberleben einen App-Neustart ohne ctox.dev
  Account: Registry wird neu geladen, SecretStore-Referenzen bleiben extern,
  und der Launch bleibt WebRTC-only.
- Local-Daemon-Kommandos binden einen ausgewaehlten `ctoxRoot` fail-closed an
  den Child-Prozess. Ein Business-OS-Kundenrepo aus `business-os install` wird
  nicht mehr stillschweigend als CTOX Runtime-Root akzeptiert und kann dadurch
  nicht versehentlich die globale CTOX-Installation steuern.
- `npm run smoke:local-runtime` validiert einen echten lokalen Runtime-Flow
  gegen ein reales `ctox` Binary und ein frisches Desktop-Profil: Business OS
  wird in ein Temp-Ziel installiert, dieses Ziel wird als Runtime-Root korrekt
  abgelehnt, `peer ensure` laeuft gegen einen validen CTOX Runtime-Root, die
  Desktop-Quelle attached lokal, Secrets bleiben im SecretStore und der Launch
  bleibt WebRTC-only.
- Der lokale Desktop-Quellpfad kann ohne expliziten `ctoxBinary` und ohne
  globales PATH-`ctox` einen gebuendelten CTOX-Helper aus den App-Resources
  verwenden. `npm run smoke:local-bundled-runtime` beweist den frischen
  Desktop-Profilpfad ohne ctox.dev Account und ohne `ctoxRoot`: lokale
  Installation, Inspect, Attach, App-Neustart und WebRTC-only Launch bleiben
  moeglich. Der Release-Workflow baut den plattformpassenden CTOX-Helper vor
  dem Electron-Package und `electron-builder` nimmt `resources/ctox` als externe
  App-Resource auf. Noch offen ist der echte Tag-/Signed-Run auf sauberer
  Maschine.
- Pairing-Invite und manuelles Signaling-Pairing speichern Secret-Material im
  SecretStore statt in der Registry.
- `ctox business-os desktop invite` erzeugt ein Electron-kompatibles Pairing
  Invite als JSON oder Deep-Link, bleibt beim WebRTC-only Datenpfad und markiert
  das Secret-Material im Payload explizit.
- Pairing-Instanzen koennen per Ersatz-Invite rotiert und lokal widerrufen
  werden; Rotation invalidiert gecachte BrowserViews, und der Widerruf entfernt
  Registry-Eintrag und SecretStore-Referenzen.
- Pairing-Identitaet ist jetzt an `instance_id` statt an `sync_room` gebunden.
  Damit bleibt eine echte Remote-`peer rotate`-Rotation dieselbe Desktop-
  Instanz, obwohl sich das `sync_room` durch den neuen Room-Secret-Hash
  aendert; alte sync-room-basierte Desktop-IDs werden beim naechsten Rotate in
  die neue ID migriert.
- Der native RxDB-Peer erkennt persistierte Sync-Konfigurationsaenderungen
  (`sync_room`, Room-Passwort, Signaling-URLs) im Watchdog und beendet sich
  kontrolliert fuer den bestehenden Supervisor-Respawn. Damit ist die lokale
  Produktionslogik vorhanden, damit ein Remote-`peer rotate` nicht nur den
  Desktop-Launch-Vertrag aktualisiert, sondern auch einen laufenden Peer mit der
  neuen Konfiguration neu startet.
- `npm run smoke:pairing-ssh-live` ist als opt-in Live-Smoke vorhanden. Gegen
  den SKF-Testhost ist der Remote-`peer rotate` + Desktop-Import/Rotate/Revoke-
  Pfad mit WebRTC-only Launch-Konfig und ohne Secret-Leak gruen; die Remote-
  Instanz erzeugt das Desktop-Invite derzeit aber noch nicht selbst, weil der
  ausgerollte VPS-`ctox` den neuen `business-os desktop invite` CLI-Befehl noch
  nicht enthaelt.
- SSH-managed Quelle deckt Host-Key-Fingerprint-Trust, app-eigene
  `ssh_known_hosts`, OpenSSH key/agent Attach, Remote-Preflight und
  Existing-CTOX-Upgrade auf Contract-Ebene ab.
- SSH-managed Fresh-Install ist auf Contract-Ebene vorhanden: nach
  Host-Key-Trust und Preflight wird auf Linux-Hosts mit `bash`, `curl`,
  `systemd` und entweder passwordless `sudo -n` oder einer SecretStore-basierten
  Sudo-Passwort-Referenz der offizielle CTOX-Installer ausgefuehrt, danach
  `peer ensure`; Passwortargumente, `sshpass` und `sudo -S` bleiben verboten.
- SSH-managed Fresh-Install kann dem offiziellen Installer CLI-Argumente fuer
  API-backed Setups durchreichen (`--api-provider`, `--model`, `--backend`),
  damit CPU-only VPS nicht zwangslaeufig auf das Default-Profil mit lokaler
  GPU-Inferenz fallen. Diese Parameter werden als Installer-Flags uebergeben,
  nicht als Runtime-Env-Toggles.
- Fuer Hosts ohne passwordless sudo nutzt der Fresh-Install-Vertrag einen
  remote temporaeren Askpass/FIFO-Pfad: das Sudo-Secret kommt aus dem
  SecretStore und wird nur ueber stdin an den SSH-Prozess geschrieben.
- Die SSH-Detailansicht hat einen Passwortdialog mit `type=password`, der das
  Remote-Sudo-Passwort direkt im SecretStore speichert und nur eine
  `keychain://...`-`sudoPasswordRef` in der UI/Install-Konfiguration sichtbar
  macht.
- SSH-Login-Passwortauthentifizierung ist als OpenSSH-Askpass-Vertrag
  vorhanden: Die App speichert das SSH-Passwort im SecretStore, erzeugt eine
  `sshPasswordRef`, schaltet SSH/SCP auf `BatchMode=no` und laesst ein
  temporaeres Askpass-Skript das Passwort zur Laufzeit aus dem OS-Keychain
  holen. Passwortargumente und `sshpass` bleiben verboten.
- Ein optionaler Live-Smoke fuer SSH-Passwortauthentifizierung ist vorhanden.
  Gegen den SKF-Testhost wurde echte Passwort-SSH-Anmeldung mit OpenSSH
  Askpass, strikter Host-Key-Pruefung, macOS Keychain-backed
  `sshPasswordRef` und Remote-Preflight validiert. Der File-Askpass-Fallback
  bleibt nur ein optionales Harness-Werkzeug.
- Derselbe Live-Smoke kann mit `--attach` den vollstaendigen Existing-CTOX
  SSH-managed Attach ausfuehren. Gegen den SKF-Testhost ist dieser Pfad gruen:
  Remote-`peer ensure`, lokale `ssh_managed` Registry-Instanz, eigene
  Session-Partition und WebRTC-only Launch-Konfig wurden mit platform-
  keychain-backed `sshPasswordRef` validiert, ohne Signaling-Room-Secret in
  Registry oder Evidenz.
- SSH Fresh-Install kann alternativ ein lokales CTOX-Binary-Artefakt per
  `scp` auf den Remote-Host laden, user-local nach `~/.local/bin/ctox`
  installieren und danach `ctox start/status` sowie `peer ensure` nutzen. Der
  Online-Installer bleibt dafuer unbeteiligt.
- Launch-Config setzt `transport=webrtc` und `http_bridge_available=false`.
- BrowserView-Hosting nutzt pro Instanz die deterministische
  `sessionPartition`.
- Ein echter Electron-Smoke prüft zwei getrennte persistente
  Electron-Session-Partitionen gegen Cookies, LocalStorage und IndexedDB; der
  BrowserView-Host selbst ist per Unit-Test auf korrekte Partition-Übergabe
  abgedeckt.
- Ein zweiter Electron-Smoke prüft OS-Protokoll-Lifecycle für Cold-Start-URL,
  macOS `open-url` und `second-instance` Dispatch.
- Die Sidebar rendert Source-, Role-, Status- und RxDB/WebRTC-Health-Badges
  fuer managed und unmanaged Instanzen; ein Renderer-Smoke prueft DOM und
  Suche.
- Der Renderer-Smoke beweist den gemischten Switcher-Pfad lokal: Aktivierung
  per Sidebar-Klick und `Cmd/Ctrl+K` + Suche + `Enter` funktioniert fuer
  ctox.dev-managed, SSH-managed unmanaged und Pairing-Instanzen; alle
  Aktivierungsnachweise bleiben `rxdb-webrtc` ohne HTTP-Datenproxy.
- Der BrowserView installiert einen HTTP-Datenpfad-Guard: erlaubte
  Business-OS-Control-Plane-Requests bleiben moeglich, verbotene
  `/api/business-os/*`, `/rxdb/*` und `/commands` Datenpfade werden vor dem
  Netzwerkrequest abgebrochen. Ein Electron-Smoke beweist, dass der lokale
  Server diese Datenpfade nicht sieht.
- macOS Keychain ist auf Command-Contract- und Runtime-Smoke-Ebene validiert;
  Secrets werden per stdin-Prompt uebergeben und nicht als Prozessargumente.
- Linux Secret Service/libsecret und Windows Credential Manager sind auf
  Command-Contract-Ebene implementiert und geben Secrets ueber stdin weiter;
  Windows nutzt jetzt den echten Credential Manager ueber `advapi32` statt
  eines Stub-Pfads.
- Redaction für URL-/Log-/Support-Snapshot-Secrets ist auf Unit-Ebene
  abgedeckt.
- Crash-Reporter-Hook startet ohne Upload und registriert nur redigierte,
  kurze Extra-Metadaten.
- Release-Konfiguration ist manifestiert: `electron-builder`, Lockfile,
  macOS Hardened Runtime/Entitlements/Notarize-Hook, Desktop-Protokoll,
  HTTPS-Auto-Update-Feed und `release:check` Verifier.
- Der Tag-Release-Workflow enthält eine eigene Business-OS-Desktop-Matrix für
  macOS arm64/x64, Linux x64 und Windows x64; `release:check` verifiziert
  Matrix, npm-Gates, Electron-Smokes, Distribution-Build und plattformweiten
  Release-Artefakt-Smoke.
- Derselbe Workflow fuehrt den Keychain-Runtime-Smoke auf macOS, Linux und
  Windows aus; Linux startet dafuer eine echte Secret-Service-Session ueber
  `dbus-run-session` und `gnome-keyring`.
- `npm run smoke:signed-artifacts` ist jetzt plattformweit: macOS prueft
  `.app`, `app.asar`, gebuendelten CTOX-Helper sowie `codesign`/`spctl`;
  Linux prueft AppImage, `.deb`, `linux-unpacked`, `app.asar` und Helper;
  Windows prueft NSIS-Installer, `win-unpacked`, `app.asar` und `ctox.exe`.
  Der Workflow laesst diesen Smoke nach dem Plattform-Build mit
  `matrix.builderPlatform` laufen und schreibt pro Matrix-Ziel eine
  uploadbare JSON-Evidenzdatei unter `release/artifact-smoke-*.json`.
- Lokaler macOS Pack-Directory-Smoke baut eine unpacked `.app`, ueberspringt
  Notarization nur fuer `--dir`, und validiert `Info.plist` sowie `app.asar`
  ohne Test-/Release-Artefakte im Paket; das CTOX-App-Icon ist als PNG/ICNS
  Build-Ressource verdrahtet.
- Ein Electron-Smoke gegen einen lokalen ctox.dev-Mock beweist, dass Login,
  Session-Package, Launch-Token und Launch-Config denselben Desktop-Cookie-Jar
  nutzen und die Launch-Konfig `transport=webrtc` /
  `http_bridge_available=false` bleibt.
- Der globale Desktop-Protokollhandler erkennt jetzt auch
  `ctox-business-os-desktop://auth/callback` und reicht ihn an den aktiven
  ctox.dev Login-Promise weiter; Unit-Tests und der Electron Protocol-Smoke
  decken den OS-Callback-Lifecycle ab.
- Der echte ctox.dev BrowserWindow/AuthPanel-Login ist als opt-in Live-Smoke
  gruen: Die Login-UI wird im Electron-Fenster ausgefuellt, die App erkennt die
  danach gueltige ctox.dev Session per Session-Package-Check und schliesst das
  Loginfenster ab. Damit haengt die Desktop-App nicht mehr daran, dass ctox.dev
  live zuverlaessig einen Custom-Scheme-Callback navigiert.
- Ein opt-in Live-Smoke gegen echte ctox.dev-Produktion ist vorhanden und
  gruen gelaufen: Passwort kommt nur ueber stdin, Auth laeuft ueber den echten
  ctox.dev Passwort-Endpunkt in Electron Default-Session, das
  `/api/desktop/session-package` liefert `desktopProtocol:
  "ctox-business-os-desktop"`, sechs managed Instanzen, darunter Kunstmen und
  SKF, und ein verbrauchter Desktop-Launch-Token fuer Kunstmen liefert eine
  WebRTC-only Launch-Konfig ohne HTTP-Bridge.
- Derselbe Live-Smoke kann mit `--manage-first` einen konkreten ctox.dev
  Management-Deep-Link pruefen: `/dashboard?tenant=<tenant-id>` laedt im
  authentifizierten Electron Cookie-Jar mit HTTP 200, redirectet nicht zum
  Login und der gerenderte Dashboard-DOM enthaelt einen Hinweis auf den
  ausgewaehlten Tenant.
- Derselbe Smoke beweist lokal auch ctox.dev-Access-Revocation und
  Launch-Token-Rotation: Nach serverseitigem Tenant-Entzug bleibt nur die noch
  berechtigte managed Instanz sichtbar, die lokale unmanaged Instanz bleibt
  erhalten, und ein zweiter Launch nutzt einen neuen Token/Launch-Kontext.
- Derselbe Smoke beweist den lokalen Desktop-Deauth-Pfad: vor Logout sieht das
  Session-Package den ctox.dev-Cookie, nach Logout nicht mehr; unmanaged lokale
  Instanzen bleiben sichtbar.
- Der Renderer-Smoke prüft Per-instance Settings für managed und unmanaged
  Instanzen, inklusive ctox.dev-Verwaltungsaktion, fehlender lokaler
  Managed-Löschung und lokalem Entfernen einer unmanaged SSH-Instanz.
- Die breiten RxDB/Data-Plane-Gates laufen lokal gruen: JS-Smokes inklusive
  Cross-Process-Wire-Daemon und Rust-Crate-Tests.

Nicht umgesetzt oder noch nicht bewiesen:

- Server-seitige Access Revocation und Desktop-Session-Rotation gegen echte
  ctox.dev-Tenants; lokal ist der Contract inklusive Electron-Smoke gruen.
- Komplett frisches OS ohne vorhandenes lokales CTOX-Binary/validen CTOX
  Runtime-Root ist als Desktop-Vertrag naeher dran, aber noch nicht final
  bewiesen: Der lokale Quellpfad kann jetzt einen gebuendelten CTOX-Helper aus
  den App-Resources nutzen, der Release-Workflow baut diesen Helper pro
  Plattform vor dem Electron-Package, und der Smoke laeuft ohne ctox.dev
  Account, `ctoxRoot` oder explizites `ctoxBinary`. Offen bleibt der echte
  Tag-/Signed-Run, der das auf sauberer Maschine ausfuehrt.
- Pairing-Rotation/Widerruf ist gegen den SKF-Testhost auf Remote-`peer
  rotate`, lokalen Desktop-Import/Rotate/Revoke und WebRTC-only Launch-Konfig
  gruen. Der lokale native Peer erkennt Sync-Konfigurationsaenderungen jetzt
  und triggert einen Supervisor-Respawn. Noch nicht bewiesen ist der volle
  Zielpfad, in dem die Remote-Instanz selbst
  `ctox business-os desktop invite` bereitstellt und ein Browser nach Rotation
  tatsaechlich wieder ueber den neu gestarteten nativen Peer verbindet; auf
  beiden Test-VPS ist der Remote-CLI-Stand dafuer noch zu alt.
- SSH/Sudo Fresh-Install ist gegen echte VPS noch nicht gruen: Der erste
  Kunstmen-Live-Versuch ohne API-Provider scheiterte korrekt am GPU-only
  Default des offiziellen Installers; der zweite Versuch mit
  `--install-api-provider openai` kam bis zum realen Cargo-Source-Build, lief
  aber nach 900s in den Desktop-Smoke-Timeout. Existing-CTOX Live-Attach gegen
  SKF/Kunstmen ist gruen, ersetzt aber keinen unattended Fresh-Install-
  Nachweis mit produktionsfaehiger Dauer und Rollback-/Progress-UX.
- Linux Secret Service und Windows Credential Manager sind im Tag-Release-
  Workflow als echte Runtime-Smokes verdrahtet; ein tatsaechlich gruen
  gelaufener Matrix-Run steht noch aus.
- Signierte/notarisierte Installer-Artefakte sind aus einem echten Tag-Run noch
  nicht live erzeugt und verifiziert. Die plattformweite Artefakt-Smoke-Logik
  ist lokal gegen synthetische Release-Verzeichnisse und per `release:check`
  gegen den Workflow bewiesen, aber nicht durch einen echten Tag-Run.
- Vollständige Live-/Cross-Platform-Electron-E2E für managed + unmanaged Mixed
  Switching.

## Nicht Verhandelbare Produktregeln

1. `ctox.dev` bleibt die zentrale Control Plane für managed Instanzen.
2. Die Desktop-App darf managed und unmanaged Instanzen gleichzeitig anzeigen.
3. Unmanaged Instanzen bleiben lokal registriert und werden nicht mit
   ctox.dev Tenant-Mitgliedschaften vermischt.
4. Jede Instanz läuft in einer eigenen Electron Session-Partition.
5. Business-OS-Daten laufen ausschließlich über RxDB/WebRTC.
6. SSH/Sudo ist Provisioning und Wartung, nie Business-OS-Datenpfad.
7. Secrets werden nicht in JSON, URLs, Logs, Prozessargumenten oder Crash
   Reports gespeichert.
8. Ein fehlender ctox.dev Account darf lokale, SSH-installierte oder per Invite
   gekoppelte Instanzen nicht blockieren.
9. Managed und unmanaged Instanzen verwenden dieselbe Sidebar, dieselben
   Switcher-Shortcuts und dieselbe Launch-Oberfläche.
10. Jede Release-Welle braucht reproduzierbare Tests und mindestens einen
    Nutzer-Flow, der per Electron-/Browser-Automation geprüft wird.

## Progress Model

| Welle | Gewicht | Status | Fortschritt |
| --- | ---: | --- | ---: |
| 0. Baseline & Architekturentscheidung | 8% | Abgeschlossen | 100% |
| 1. Electron Shell & Session Isolation | 12% | Abgeschlossen | 100% |
| 2. ctox.dev Managed Source | 14% | In Umsetzung | 95% |
| 3. Local Daemon Source | 12% | In Umsetzung | 92% |
| 4. Pairing Invite Source | 12% | In Umsetzung | 97% |
| 5. SSH/Sudo Remote Install Source | 14% | In Umsetzung | 97% |
| 6. Unified Switcher UX | 10% | Abgeschlossen | 100% |
| 7. Secret Storage & Hardening | 10% | In Umsetzung | 97% |
| 8. Production E2E, Packaging & Release | 8% | In Umsetzung | 74% |
| **Gesamt** | **100%** | **In Umsetzung** | **95%** |

## Welle 0: Baseline & Architekturentscheidung

Status: Abgeschlossen.

- [x] Electron-Zielpfad unter `src/apps/business-os-desktop/`.
- [x] ctox.dev bleibt Source of Truth für managed Instanzen.
- [x] Unmanaged Instanzen sind separate lokale Quellen.
- [x] RxDB/WebRTC bleibt alleiniger Business-OS-Datenpfad.

## Welle 1: Electron Shell & Session Isolation

Status: Abgeschlossen.

Aufgaben:

- [x] Electron Main/Preload/Renderer Struktur.
- [x] BrowserView Host für Business OS.
- [x] Deterministische Session-Partition pro Instanz.
- [x] URL-Bootstrap und `ctox_config` Scrubbing.
- [x] OS-Level Protocol Handler Smoke.

Tests:

- [x] Unit-Test für Session-Partitionen.
- [x] Unit-Test für BrowserView-Partition-Übergabe.
- [x] Electron-Smoke: zwei persistente Session-Partitionen mit getrennten
  Cookies, LocalStorage und IndexedDB.
- [x] Electron-Smoke: Cold-Start-URL, `open-url`, `second-instance` und
  ctox.dev Auth-Callback Protocol-Dispatch.
- [x] Syntaxcheck für Main/Preload/Renderer.
- [x] Unit-Test für `ctox_config` URL-Scrubbing.

## Welle 2: ctox.dev Managed Source

Status: In Umsetzung, 96%.

Aufgaben:

- [x] `GET /api/desktop/session-package` normalisieren.
- [x] `POST /api/desktop/launch-token` und Launch-Config konsumieren.
- [x] Rollen und Health aus Session Package normalisieren.
- [x] ctox.dev Desktop-Login-Fenster und Cookie-Jar-Flow gegen lokalen Mock.
- [x] Desktop-Protokoll-Callback `ctox-business-os-desktop://auth/callback`
  beendet ein aktives ctox.dev Login-Fenster auch dann, wenn der Callback ueber
  den OS-Protokollhandler statt nur als Browser-Navigation ankommt.
- [x] Live BrowserWindow/AuthPanel-Login mit echtem Testaccount: Das echte
  ctox.dev Formular wird im Electron-Fenster ausgefuellt; wenn ctox.dev nicht
  zum Desktop-Callback navigiert, beendet der Desktop den Login ueber einen
  authentifizierten Session-Package-Check.
- [x] Desktop-lokaler ctox.dev Logout löscht den ctox.dev-Origin und
  aktualisiert die gemischte Instanzliste.
- [x] Lokaler ctox.dev-Mock-Contract für serverseitigen Tenant-Entzug:
  betroffene managed Instanz verschwindet, lokale/SSH/Pairing-Quellen bleiben
  getrennt.
- [x] Lokaler ctox.dev-Mock-Contract für Desktop-Session-/Launch-Rotation:
  jede Aktivierung holt einen frischen Launch-Token und Launch-Kontext.
- [x] Live ctox.dev Account-/Session-Package-Pfad mit echtem Testaccount:
  Passwort-Auth ueber den echten ctox.dev Endpoint im Electron-Cookie-Jar,
  `desktopProtocol:"ctox-business-os-desktop"`, Tenant-Liste mit Kunstmen und
  SKF.
- [x] Live ctox.dev Launch-Token-Pfad mit echtem Testaccount: ein
  kurzlebiger Desktop-Launch-Token fuer Kunstmen wird verbraucht und liefert
  `transport=webrtc` / `http_bridge_available=false`.
- [x] Live ctox.dev Verwaltungsroute auf authentifiziertes Dashboard-Ziel
  `/dashboard?tenant=<tenant-id>` umgestellt; alter `/desktop/instances` Pfad
  ist live 404, der neue Deep-Link laedt im Electron Cookie-Jar ohne
  Login-Redirect und zeigt Tenant-spezifischen Dashboard-Inhalt.
- [x] Visueller BrowserWindow-Login ueber die echte ctox.dev AuthPanel-UI; der
  Custom-Scheme-Callback ist lokal/Electron-gruen, live wird der Abschluss
  ueber Session-Check bewiesen.
- [ ] Live Access Revocation und Desktop-Session-Rotation gegen echte
  ctox.dev-Tenants.

Tests:

- [x] Electron-Smoke: lokaler ctox.dev-Mock setzt Login-Cookie; Session-Package,
  Launch-Token und Launch-Config sehen denselben Cookie.
- [x] Unit-/Electron-Smoke: ctox.dev Auth-Callback wird als eigene
  Desktop-Protokollaktion geparst, nicht als unbekannter Link geloggt, und an
  den aktiven Login-Promise dispatcht.
- [x] Electron-Smoke: gemanagte Mock-Tenants SKF und Kunstmen erscheinen in der
  Instanzliste; Sortierung folgt App-Logik (`Kunstmen`, dann `SKF`).
- [x] Electron-Smoke: ctox.dev Logout entfernt managed Mock-Tenants aus der
  Liste und lässt die unmanaged lokale Instanz sichtbar.
- [x] Unit-Test: ctox.dev Source reflektiert geänderte Session-Packages ohne
  Tenant-Cache.
- [x] Unit-Test: ctox.dev Source fordert pro Aktivierung einen frischen
  Launch-Token an.
- [x] SourceManager-Test: ctox.dev Revocation entfernt nur managed Tenants und
  lässt unmanaged Quellen sichtbar.
- [x] Electron-Smoke: lokaler ctox.dev-Mock rotiert Launch-Token/Launch-Kontext
  und entzieht einen Tenant, ohne die lokale unmanaged Instanz zu entfernen.
- [x] `npm run smoke:ctox-dev-live`: optionaler Live-Smoke liest das Passwort
  nur ueber stdin, meldet sich ueber ctox.dev Passwort-Auth in Electron
  Default-Session an, prueft `/api/desktop/session-package` im selben
  Cookie-Jar, validiert `desktopProtocol:"ctox-business-os-desktop"` und
  redigiert die Evidenz.
- [x] Live-Tenant-Liste enthaelt Kunstmen und SKF; die aktuelle Produktion
  liefert fuer den Testaccount sechs managed Instanzen:
  `GPU1 A6000`, `GPU3 A4500`, `GPU4 A4500`, `infyoda`, `kunstmen`, `SKF`.
- [x] Live-Launch-Nachweis fuer Kunstmen: Launch-Origin
  `https://cto1.kunstmen.com`, `transport=webrtc`,
  `http_bridge_available=false`, Signaling-URL vorhanden und Room-Passwort nur
  als redigierter Presence-Nachweis.
- [x] Live-Management-Smoke: `/dashboard?tenant=<tenant-id>` liefert im
  authentifizierten Electron Cookie-Jar 200, bleibt auf ctox.dev, redirectet
  nicht zum Login und der BrowserWindow-DOM enthaelt einen Hinweis auf den
  ausgewaehlten Tenant.
- [x] BrowserWindow/AuthPanel Login E2E mit echtem Testaccount:
  `smoke:ctox-dev-live -- --auth-window` meldet sich ueber die echte UI an und
  schliesst via `session-check`.
- [ ] Live-Entzug einer Mitgliedschaft entfernt oder sperrt die Instanz.

## Welle 3: Local Daemon Source

Status: In Umsetzung, 92%.

Aufgaben:

- [x] Lokales CTOX erkennen auf CLI-Contract-Ebene.
- [x] `ctox business-os peer status` / `peer ensure` anbinden.
- [x] Lokale Installation per `ctox business-os install --target` delegieren.
- [x] App-Restart-Smoke fuer persistierte lokale Instanz ohne ctox.dev Account.
- [x] Runtime-Smoke mit realem lokalem `ctox` Binary: echte Installation in ein
  Temp-Ziel, `peer ensure`, Desktop-Attach und WebRTC-only Launch.
- [x] `ctoxRoot` wird fail-closed an Child-Prozesse gebunden; Nicht-Runtime-
  Roots werden abgelehnt statt heimlich auf die globale Installation
  zurueckzufallen.
- [x] Fresh-Desktop-Profile-Smoke ohne ctox.dev Account: persistierte Registry
  in Temp-Profil, simulierter App-Neustart, lokaler WebRTC-Launch.
- [x] Lokaler Quellpfad findet einen gebuendelten CTOX-Helper aus
  App-Resources, wenn kein explizites `ctoxBinary` gesetzt ist; ein
  vorhandenes explizites Binary gewinnt weiterhin.
- [x] Fresh-Desktop-Profile-Smoke ohne ctox.dev Account, ohne `ctoxRoot` und
  ohne explizites `ctoxBinary`: Installation, Inspect, Attach, App-Neustart und
  WebRTC-only Launch laufen ueber den gebuendelten Helper-Vertrag.
- [x] Release-Workflow baut vor `electron-builder` einen plattformpassenden
  CTOX-Helper und legt ihn in `resources/ctox`; die Builder-Konfiguration
  paketiert diesen Pfad als externe App-Resource.
- [ ] Signierter Release-/Fresh-Machine-Smoke mit echtem, gebuendeltem
  CTOX-Helper und ohne vorhandenes globales CTOX-Binary.

Tests:

- [x] Desktop-JS-Test: lokale Instanz attachen, Registry auf Platte schreiben,
  App/Source neu laden, ohne ctox.dev Account listen und WebRTC-Launch mit
  SecretStore-Referenz bauen.
- [x] `npm run smoke:local-runtime`: reales `ctox business-os install --target`
  in ein Temp-Ziel, Ablehnung dieses Business-OS-Kundenroots als Runtime-Root,
  anschliessend `peer ensure`, Attach ueber `SourceManager` in einem frischen
  Desktop-Profil, ctox.dev 401 ohne managed Instanzen, Registry secret-frei,
  simulierter Neustart und Launch `transport=webrtc` /
  `http_bridge_available=false`.
- [x] Desktop-JS-Test: Local-Command-Optionen setzen fuer valide CTOX Runtime-
  Roots `CTOX_ROOT` im Child-Prozess und lehnen Business-OS-Kundenroots als
  Runtime-Roots ab.
- [x] Desktop-JS-Test: Lokales Profil waehlt einen ausfuehrbaren gebuendelten
  CTOX-Helper vor PATH-Fallback und laesst explizite Binary-Auswahl gewinnen.
- [x] `npm run smoke:local-bundled-runtime`: Frisches Desktop-Profil, ctox.dev
  401, kein `ctoxRoot`, kein explizites `ctoxBinary`, lokale Installation ueber
  gebuendelten Helper-Vertrag, Attach, persistierter Neustart und WebRTC-only
  Launch ohne Registry-Secret-Leak.
- [x] `npm run release:check`: Prueft, dass die Business-OS-Desktop-
  Release-Matrix den gebuendelten CTOX-Helper baut und dass
  `electron-builder` `resources/ctox` als externe App-Resource aufnehmen kann.

## Welle 4: Pairing Invite Source

Status: In Umsetzung, 97%.

Aufgaben:

- [x] `.ctox-invite`/Deep-Link Payload parser.
- [x] Manuelle Signaling-Konfiguration als Advanced-Pfad.
- [x] SecretStore-Trennung für Room Secret.
- [x] `ctox business-os desktop invite` CLI-Vertrag.
- [x] Rotation per Ersatz-Invite und lokaler Widerruf.
- [x] Pairing-Instanz-ID ist rotationsstabil: echte Remote-Rotation darf das
  `sync_room` aendern, ohne dass der Desktop eine neue Instanz statt einer
  Rotation erzeugt.
- [x] Opt-in Live-Smoke fuer Remote-`peer rotate` via SSH, Desktop-Import,
  lokale Pairing-Rotation, WebRTC-only Launch-Konfig und lokalen Widerruf.
- [x] Laufender nativer RxDB-Peer erkennt Sync-Konfigurationsaenderungen und
  beendet sich kontrolliert, damit der Supervisor ihn mit neuer Room-/Signaling-
  Konfiguration neu startet.

Tests:

- [x] Rust-Unit-Test: Desktop-Invite-Vertrag erzeugt Electron-Pairing-Schema,
  WebRTC-only Marker, `http_bridge_available=false` und Deep-Link ohne
  rekursives `desktop_link` Feld.
- [x] Desktop-JS-Test: CLI-geformtes Invite JSON und
  `ctox-business-os-desktop://pair` Deep-Link importieren in denselben
  Pairing-Source-Vertrag.
- [x] Desktop-JS-Test: Rotation akzeptiert nur ein Ersatz-Invite fuer dieselbe
  Pairing-Identitaet, ersetzt das Secret im SecretStore und weist fremde
  Invites ab.
- [x] Desktop-JS-Test: Rotation akzeptiert geaenderte `sync_room`-Werte fuer
  dieselbe `instance_id`, migriert alte sync-room-basierte IDs, entfernt alte
  SecretRefs und erzeugt keine doppelte Registry-Instanz.
- [x] Electron-Renderer-Smoke: Pairing-Details zeigen Rotation und Widerruf;
  Rotation uebergibt nur einen neuen Payload, Widerruf entfernt die lokale
  Pairing-Instanz.
- [x] `npm run smoke:pairing-ssh-live -- --rotate --revoke-local
  --allow-peer-status-invite`: Gegen SKF `57.129.123.108` mit gepinntem
  ED25519-Fingerprint rotiert der echte Remote-Peer sein Room-Secret, der
  Desktop importiert und rotiert die Pairing-Instanz, Launch bleibt
  `transport=webrtc` / `http_bridge_available=false`, lokaler Widerruf entfernt
  Registry und SecretStore-Eintrag, Evidenz/Registry bleiben secret-frei. Der
  Invite kam wegen altem Remote-CLI-Stand aus `peer status` statt aus
  `desktop invite`.
- [x] `cargo test native_peer_ -- --nocapture`: Native-Peer-Tests beweisen, dass
  eine unveraenderte Sync-Konfiguration nicht respawnt und eine
  `peer rotate`-Room-Aenderung als Respawn-Grund erkannt wird.

Noch offen:

- [ ] Remote-`ctox business-os desktop invite` auf den echten Test-VPS
  ausrollen und live gegen diesen CLI-Pfad pruefen; aktuell melden SKF und
  Kunstmen `unknown business-os command desktop`.
- [ ] Full Live-E2E: Nach `ctox business-os peer rotate` muss ein Browser ueber
  den neu gestarteten nativen Peer wieder verbinden. Lokal ist der Respawn-
  Trigger jetzt getestet; live muss noch ein aktualisierter Remote-`ctox` Stand
  ausgerollt und die echte Browser-WebRTC-Reconnect-Session beobachtet werden.
  Die bisherige SKF-Evidenz zeigte noch keine sichtbar geaenderte aktive Peer-
  Session.

## Welle 5: SSH/Sudo Remote Install Source

Status: In Umsetzung, 97%.

Aufgaben:

- [x] SSH Host-Key-Fingerprint-Trust-Flow.
- [x] Key/Agent-basierter Attach ohne Passwortargumente.
- [x] Remote Preflight für OS, systemd, sudo, `ctox`.
- [x] Fresh Ubuntu Install auf Contract-Ebene mit offiziellem Installer,
  Linux/bash/curl/systemd/passwordless-sudo Preflight und `peer ensure`.
- [x] Offizieller Fresh-Installer kann API-backed Parameter als CLI-Flags
  bekommen: `apiProvider`, `model` und `backend` werden validiert und an
  `install.sh` weitergereicht, ohne Env-Var-Runtime-Toggle.
- [x] Existing-CTOX Upgrade/Restart auf CLI-Contract-Ebene.
- [x] SourceManager routet `freshInstall: true` getrennt vom Existing-Upgrade.
- [x] Hosts ohne passwordless sudo koennen den Fresh-Install-Vertrag mit einer
  SecretStore-basierten `sudoPasswordRef` nutzen; der Remote-Befehl verwendet
  `sudo -A`/Askpass und ein FIFO statt `sudo -S`.
- [x] Nativer Renderer-Passwortdialog erzeugt die `sudoPasswordRef`, speichert
  das Secret direkt im OS-SecretStore und zeigt danach nur die
  `keychain://...`-Referenz.
- [x] SSH-Login-Passwortauthentifizierung ueber SecretStore-basierte
  `sshPasswordRef` und OpenSSH-Askpass; SSH/SCP wechseln nur dann auf
  `BatchMode=no` und Passwort/KbdInteractive-Auth.
- [x] Lokales CTOX-Binary-Artefakt kann per `scp` auf den Host geladen und
  user-local nach `~/.local/bin/ctox` installiert werden, ohne offiziellen
  Online-Installer, `curl`, `sshpass` oder `sudo -S`.
- [x] Opt-in Live-Smoke fuer SSH-Passwortauthentifizierung gegen echten VPS:
  OpenSSH Askpass, strikte Host-Key-Pruefung und Remote-Preflight sind gegen
  den SKF-Testhost gruen.
- [x] Voller OS-Keychain-backed Live-Test der SSH-Login-
  Passwortauthentifizierung gegen den SKF-Testhost: `sshPasswordRef` im macOS
  Keychain, OpenSSH Askpass und Remote-Preflight sind gruen.
- [x] Existing-CTOX Live-Attach gegen den SKF-Testhost: `--attach` fuehrt
  Remote-`peer ensure` aus, registriert lokal eine `ssh_managed` Instanz,
  erzeugt eine eigene Electron-Session-Partition und prueft eine WebRTC-only
  Launch-Konfig ohne Secret-Leak in Registry oder Evidenz.
- [x] Zweiter Live-VPS-Nachweis gegen Kunstmen: gepinnter ED25519-Host-Key,
  echter Passwort-SSH-Preflight, Remote-`peer ensure`, `ssh_managed`
  Registry-Shape und WebRTC-only Launch sind gruen. Dieser zweite Host nutzt
  wegen macOS-Keychain-TTY-Harness-Problemen bewusst den schwächeren
  `file-askpass-fallback` mit In-Memory-Smoke-Secrets; der voll
  platform-keychain-backed Attach bleibt durch SKF belegt.
- [ ] Fresh Ubuntu Install gegen echten VPS ist noch nicht gruen: Kunstmen mit
  `--install-api-provider openai` erreicht den echten offiziellen Installer und
  startet den Cargo-Source-Build, laeuft aber im Desktop-Smoke nach 900s in den
  Timeout; der verwaiste Build wurde danach gezielt gestoppt.
- [ ] Live-Test des lokalen Artefaktpfads gegen einen echten VPS.

Tests:

- [x] Desktop-JS-Test: Fresh-SSH-Install-Command nutzt den offiziellen
  `install.sh`, `curl -fsSL | bash`, `sudo -n true`, `ctox upgrade`, optional
  `ctox start` und `ctox status`, aber kein `sshpass`/`sudo -S`.
- [x] Desktop-JS-Test: Fresh-SSH-Install kann API-backed Installer-Argumente
  per `bash -s -- '--api-provider' ...` weitergeben, validiert untrusted
  Zeichen und lehnt die Kombination mit lokalem Artefaktpfad ab.
- [x] Desktop-JS-Test: Fresh-SSH-Install bricht ohne passwordless sudo
  oder Sudo-Secret-Ref fail-closed ab.
- [x] Desktop-JS-Test: Fresh-SSH-Install akzeptiert eine Sudo-Secret-Ref,
  baut einen `sudo -A`/Askpass-FIFO-Remote-Befehl und schreibt das Secret nur
  per stdin in den SSH-Prozess.
- [x] Desktop-JS-Test: SSH-Sudo-Passwort wird in den SecretStore geschrieben,
  die Registry bleibt frei vom Passwort und es wird nur die Secret-Referenz
  zurueckgegeben.
- [x] Electron-Renderer-Smoke: SSH-Details zeigen den Passwortdialog, das Feld
  ist `type=password`, der Store-Call bekommt Host/User/Port und im UI erscheint
  nur die `keychain://...`-Referenz, nicht das Passwort.
- [x] Desktop-JS-Test: `sshPasswordRef` aktiviert OpenSSH-Askpass fuer SSH/SCP,
  setzt `BatchMode=no`, laesst Passwortauthentifizierung zu und haelt das
  Passwort aus Prozessargumenten heraus.
- [x] Desktop-JS-Test: Das temporaere Askpass-Skript enthaelt nur die
  SecretStore-Referenz, liest aus dem OS-Keychain und wird nach dem SSH/SCP-
  Prozess entfernt.
- [x] Desktop-JS-Test: SSH-Login-Passwort wird in den SecretStore geschrieben,
  die Registry bleibt frei vom Passwort und es wird nur die Secret-Referenz
  zurueckgegeben.
- [x] Electron-Renderer-Smoke: SSH-Details speichern SSH-Login- und
  Sudo-Passwort getrennt; im UI erscheinen nur `keychain://...`-Referenzen.
- [x] Desktop-JS-Test: Lokaler Artefaktpfad akzeptiert nur absolute lokale
  Pfade, bereitet den Remote-Cache per SSH vor, laedt per `scp` mit strikter
  Host-Key-Pruefung hoch und installiert nur nach `~/.local/bin/ctox`.
- [x] Desktop-JS-Test: Lokaler Artefaktpfad benoetigt fuer den Contract weder
  `curl` noch `sudo`, nutzt nicht den Online-Installer und haelt
  Passwortargumente aus SSH/SCP-Args heraus.
- [x] Desktop-JS-Test: Fresh-SSH-Install registriert nach `peer ensure` eine
  SSH-managed Instanz und speichert das Room Secret nur im SecretStore.
- [x] Desktop-JS-Test: SourceManager routet `freshInstall: true` in den
  Fresh-Pfad und default weiter in Existing-Upgrade.
- [x] `npm run smoke:ssh-password-live`: optionaler Live-Smoke liest das
  Passwort nur ueber stdin, schreibt keine Klartextargumente, prueft Host-Key-
  Trust, OpenSSH Askpass und Remote-Preflight und redigiert die Evidenz.
- [x] Live-Nachweis SKF-Testhost `57.129.123.108`: Passwort-SSH via
  platform-keychain-backed `sshPasswordRef`, Fingerprint
  `SHA256:ZIFGq4ACB3opMov6dULHDo6LeWwKQh85CQ1Ocj7jSKA`, Linux x86_64, Shell,
  Bash, Curl, systemd, sudo, passwordless sudo und vorhandenes `ctox`
  erreichbar.
- [x] Live-Nachweis SKF-Testhost `57.129.123.108` mit `--attach`:
  platform-keychain-backed SSH-Passwort, strikter Host-Key-Trust,
  Remote-`peer ensure`, `ssh_managed` Registry-Eintrag, Session-Partition
  `persist:ctox-ssh-...`, `transport=webrtc`,
  `http_bridge_available=false`, Signaling-URL vorhanden, kein Room-Secret in
  Registry oder redigierter Evidenz.
- [x] Live-Nachweis Kunstmen-Testhost `51.210.246.120` mit
  `--file-askpass-fallback --attach`: gepinnter ED25519-Fingerprint
  `SHA256:L005h7I+9bmVcd7yipnIXu3dElimUMjuINLmrt26Y0A`, Linux x86_64, Shell,
  Bash, Curl, systemd, sudo, passwordless sudo, vorhandenes `ctox`,
  Remote-`peer ensure`, `ssh_managed` Registry-Eintrag, eigene
  Session-Partition, `transport=webrtc`, `http_bridge_available=false`,
  Signaling-URL vorhanden und kein Registry-Secret-Leak. Dieser zweite Host ist
  ein Remote-/Launch-Nachweis mit schwächerem Secret-Backend; kein zweiter
  platform-keychain-backed Attach.
- [x] Negativer Live-Befund Kunstmen Fresh-Install:
  `--fresh-install --install-api-provider openai` vermeidet den frueheren
  GPU-only Installer-Abbruch und startet real `cargo build --release --bin
  ctox`; der Desktop-Smoke bricht aber nach 900s mit SSH-Timeout ab. Damit ist
  der Flag-Contract live bestaetigt, aber Fresh-Install ist nicht
  production-ready, solange der offizielle Installer auf kleinen VPS aus Source
  baut oder der Desktop keine laengere/progressfaehige Install-Session fuehrt.

## Welle 6: Unified Switcher UX

Status: Abgeschlossen.

Aufgaben:

- [x] Gemeinsame Sidebar.
- [x] Quick Switch mit `Cmd/Ctrl+K`, `Enter`, `Escape`.
- [x] Empty State für Instanz-Auswahl.
- [x] Source-, Role-, Health- und Offline-Badges vollständig.
- [x] ctox.dev-managed Verwaltungs-/Deauth-Verweise.
- [x] Per-instance Settings.
- [x] Gemischter Quick Switch fuer managed, SSH-managed unmanaged und
  Pairing-Instanzen.

Tests:

- [x] Renderer-Smoke: Details-Panel zeigt Quelle, Status, Host/Domain,
  Session-Partition und RxDB/WebRTC-Datenpfad.
- [x] Renderer-Smoke: managed Instanz zeigt ctox.dev-Verwaltung, aber keine
  lokale Entfernen-Aktion.
- [x] Renderer-Smoke: unmanaged SSH-Instanz kann lokal entfernt werden.
- [x] Electron-Renderer-Smoke: Sidebar-Klick aktiviert managed Instanz;
  `Cmd/Ctrl+K`, Suche und `Enter` aktivieren SSH- und Pairing-Instanzen.
- [x] Electron-Renderer-Smoke: alle gemischten Aktivierungen melden
  `rxdb-webrtc` und `httpDataProxy=false`.

## Welle 7: Secret Storage & Hardening

Status: In Umsetzung, 97%.

Aufgaben:

- [x] Registry weist secret-artige Felder zurück.
- [x] Launch-URL-Scrubbing.
- [x] Navigation-Allowlist.
- [x] macOS Keychain Adapter auf Command-Contract-Ebene.
- [x] macOS Keychain Runtime-Smoke mit stdin-Prompt und ohne Secret in
  Prozessargumenten.
- [x] Linux Secret Service/libsecret Adapter auf Command-Contract-Ebene.
- [x] Windows Credential Manager Adapter auf Command-Contract-Ebene.
- [x] Windows Credential Manager nutzt im Runtime-Pfad `CredWriteW`,
  `CredReadW` und `CredDeleteW` ueber `advapi32`.
- [x] Secret-Store-Prozessrunner schreibt stdin wirklich und bricht haengende
  OS-Keychain-Kommandos kontrolliert ab.
- [x] Support Bundle Redaction auf Unit-Ebene.
- [x] Crash Report Redaction Hook auf Unit-Ebene.
- [x] Code-Signing-/Auto-Update-Konzept als pruefbare Release-Konfiguration.
- [x] Runtime-Smoke-Skript deckt macOS Keychain, Linux Secret Service und
  Windows Credential Manager mit demselben Set/Get/Delete-Vertrag ab.
- [x] Tag-Release-Workflow fuehrt den Runtime-Smoke auf allen Desktop-
  Zielplattformen aus; Linux installiert `gnome-keyring`/`libsecret-tools` und
  startet eine Secret-Service-Session.
- [ ] Gruener Live-Nachweis aus Linux-/Windows-Zielplattform-Run.

Tests:

- [x] `npm run release:check` validiert Builder-Konfiguration,
  Runtime-Update-Abhaengigkeit, Desktop-Protokoll, macOS Hardened
  Runtime/Entitlements/Notarize-Hook und HTTPS-Update-Feed.
- [x] Desktop-JS-Test: Auto-Update bleibt in Development inert, deaktiviert
  stille Downloads und redigiert Update-Fehlerlogs.
- [x] `npm run smoke:keychain-runtime` validiert macOS Keychain Set/Get/Delete
  gegen die echte Plattform-Keychain; Linux/Windows werden durch die
  Release-Matrix ausgefuehrt.

## Welle 8: Production E2E, Packaging & Release

Status: In Umsetzung, 74%.

Release Gates:

- [x] `npm test`
- [x] `npm run check`
- [x] `npm run release:check`
- [x] `npm run smoke:keychain-runtime`
- [x] `npm run pack:dir:smoke` baut lokal eine macOS `.app` und validiert
  Bundle-Metadaten, App-Icon plus `app.asar` Inhalt.
- [x] `npm run test:electron-smoke` inklusive Session-Isolation,
  Protocol-Lifecycle, Renderer-Badges, ctox.dev Login-Cookie-Jar und
  ctox.dev Logout-Cookie-Clear, Access-Revocation/Launch-Rotation gegen
  lokalen ctox.dev-Mock, Per-instance Settings sowie Pairing-Rotation/Widerruf,
  lokalem Mixed-Switching per Sidebar und
  Quick Switch sowie HTTP-Datenpfad-Blockade im BrowserView.
- [x] `cargo check`
- [x] `cargo test desktop_invite_contract_matches_electron_pairing_schema`
- [x] `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
- [x] `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- [x] `node src/apps/business-os/rxdb/tests/run-all.mjs`
- [x] `cargo test --manifest-path src/core/rxdb/Cargo.toml`
- [x] Tag-Release-Workflow baut Business OS Desktop als eigene Matrix fuer
  macOS arm64/x64, Linux x64 und Windows x64.
- [x] `npm run release:check` verifiziert die Business-OS-Desktop
  Release-Matrix inklusive npm-Gates, Electron-Smokes, Distribution-Build und
  plattformweitem Release-Artefakt-Smoke.
- [x] Release-Matrix fuehrt plattformweite Keychain-Runtime-Smokes aus:
  macOS Keychain, Linux Secret Service und Windows Credential Manager.
- [x] Business-OS-Desktop Release-Matrix baut vor `electron-builder` den
  plattformpassenden CTOX-Helper und paketiert ihn als `resources/ctox`
  App-Resource fuer lokale Fresh-Machine-Flows.
- [x] Release-Matrix fuehrt `npm run smoke:signed-artifacts -- --platform
  ${{ matrix.builderPlatform }} --evidence-json
  release/artifact-smoke-${{ matrix.builderPlatform }}-${{ matrix.arch }}.json`
  auf macOS, Linux und Windows aus.
- [x] `smoke:signed-artifacts` prueft plattformweite Release-Struktur:
  macOS `.app` plus `codesign`/`spctl` und Helper, Linux AppImage/`.deb` plus
  `linux-unpacked`/Helper, Windows NSIS-Installer plus `win-unpacked`/Helper.
- [x] `smoke:signed-artifacts` schreibt ein maschinenlesbares Evidence-JSON
  (`ctox-business-os-desktop-release-artifact-smoke/v1`) mit geprueften
  relativen Artefaktpfaden, Groessen, Helper-Pruefung und
  macOS-Signaturchecks; der Release-Workflow laedt diese Datei zusammen mit
  den Artefakten hoch.
- [ ] Live-/Cross-Platform Desktop Electron E2E auf macOS, Windows und Linux.
- [x] Keine HTTP-Business-OS-Datenrequests im lokalen Electron-E2E:
  Control-Plane-Status wird erlaubt, verbotene Datenpfade werden durch den
  BrowserView-Guard vor dem lokalen Server abgebrochen.
- [x] Kein Secret in Registry, Logs, Support Bundle oder Crash Report:
  Registry weist secret-artige Felder zurueck, Support-Snapshots redigieren
  Logs, und Crash-Reporter-Extras werden redigiert, geflattet und begrenzt.
- [ ] Live Code signing, notarization und installer smoke aus einem echten
  Tag-Run.

## Plan-Aenderungslog

| Datum | Änderung |
| --- | --- |
| 2026-06-13 | Plan im aktuellen Worktree neu angelegt und an den sichtbaren Stand angepasst. Electron-Scaffold wieder aufgebaut: Instanzmodell, Registry, ctox.dev Source-Adapter, Pairing-Source, Launch-Config, BrowserView-Host, Renderer-Shell und Electron Session-Isolation-Smoke. |
| 2026-06-13 | Wiederaufgetauchte stärkere Contract-Tests integriert und Implementierung nachgezogen: Local-Daemon CLI-Vertrag, SSH Host-Key/Preflight/Attach/Existing-Upgrade und OS-Keychain-Adapter-Contracts sind wieder grün. `npm test` läuft mit 50/50 Tests grün. |
| 2026-06-13 | Verifiziert: `npm run check` grün und `npm run test:electron-smoke` grün. Der Electron-Smoke nutzt einen frischen `userData`-Pfad und weist getrennte persistente Session-Partitionen für Cookies, LocalStorage und IndexedDB nach. |
| 2026-06-13 | Welle 7 erweitert: Crash-Report-Hook ergänzt. Electron startet Crash Reporting mit deaktiviertem Upload; Registry-/Instanz-Zusammenfassungen und laufende Extra-Parameter werden vor Übergabe redigiert, geflattet und begrenzt. `npm test` läuft mit 54/54 Tests grün. |
| 2026-06-13 | Final verifiziert: `npm test` (54/54), `npm run check`, `npm run test:electron-smoke`, RxDB-only Guard, Modul-Conformance und `git diff --check` sind grün. Der Smoke-Harness beendet Electron nach beobachtetem Ergebnis deterministisch. |
| 2026-06-13 | Welle 1 abgeschlossen: OS-Protokoll-Handling ist im Main-Prozess verdrahtet, puffert Cold-Start/`open-url` vor App-Ready und behandelt `second-instance`. `npm test` läuft mit 57/57 Tests grün; `npm run test:electron-smoke` umfasst Session-Isolation und Protocol-Lifecycle. |
| 2026-06-13 | Welle 6 erweitert: Sidebar-Badges fuer Quelle, Rolle, Status und RxDB/WebRTC-Health implementiert. `npm test` laeuft mit 60/60 Tests; `npm run test:electron-smoke` umfasst jetzt auch einen Renderer-Badge-Smoke mit echter Sidebar-DOM und Suche. |
| 2026-06-13 | Breites Data-Plane-Gate vollstaendig lokal verifiziert: Release-Wire-Daemon gebaut, `node src/apps/business-os/rxdb/tests/run-all.mjs` laeuft mit 39 passed, 0 failed, 0 skipped. `CARGO_TARGET_DIR=/tmp/ctox-rxdb-test-target cargo test --manifest-path src/core/rxdb/Cargo.toml` laeuft mit 239 Unit-Tests und 30 Conformance-Tests gruen. |
| 2026-06-13 | Welle 2 erweitert: ctox.dev Desktop-Login-Fenster verdrahtet. Nach Auth-Complete wird die Instanzliste ueber denselben Electron Default-Session-Cookie-Jar geladen. Neuer Electron-Smoke gegen lokalen ctox.dev-Mock beweist Cookie-Persistenz fuer Session-Package, Launch-Token und Launch-Config sowie WebRTC-only Launch-Konfig. `npm test` laeuft mit 62/62 Tests; `npm run check`, `npm run test:electron-smoke`, RxDB-only Guard, Modul-Conformance und `git diff --check` sind gruen. |
| 2026-06-13 | Welle 2/6 erweitert: ctox.dev-managed Instanzen haben jetzt eine Verwaltungsaktion zur ctox.dev Control Plane, und die Desktop-App kann den ctox.dev-Origin aus dem Electron Default-Session-Jar abmelden. Renderer-Smoke prueft Manage-Aktion und Logout-UI; ctox.dev-Smoke beweist Cookie vorhanden vor Logout, Cookie weg nach Logout, unmanaged lokale Instanz bleibt sichtbar. `npm test` laeuft mit 64/64 Tests; `npm run check`, `npm run test:electron-smoke`, RxDB-only Guard, Modul-Conformance und `git diff --check` sind gruen. |
| 2026-06-13 | Welle 6 Per-instance Settings umgesetzt: Sidebar-Details loesen aktive BrowserViews ab, zeigen Metadaten/Badges/Session/Datenpfad und trennen managed ctox.dev-Aktionen von lokal entfernbaren unmanaged Instanzen. Renderer-Smoke prueft managed Details ohne lokale Loeschung, ctox.dev-Verwaltung und unmanaged SSH-Entfernung. `npm test` bleibt 64/64 gruen; `npm run check`, `npm run test:electron-smoke`, RxDB-only Guard, Modul-Conformance und `git diff --check` sind gruen. |
| 2026-06-13 | Welle 4 erweitert: `ctox business-os desktop invite` erzeugt jetzt Electron-kompatible JSON- und Deep-Link-Invites fuer unmanaged Signaling-Pairing. Der Vertrag bleibt WebRTC-only, markiert Secret-Material im Payload und verhindert rekursive Deep-Link-Payloads. `npm test` laeuft mit 65/65 Tests; `npm run check`, `npm run test:electron-smoke`, RxDB-only Guard, Modul-Conformance und `cargo test desktop_invite_contract_matches_electron_pairing_schema` sind gruen. |
| 2026-06-13 | Welle 4 Rotation/Widerruf umgesetzt: Pairing-Quellen rotieren nur ueber ein passendes Ersatz-Invite, ersetzen dabei das Secret im SecretStore und lehnen fremde Invites ab; der Main-Prozess invalidiert dabei gecachte BrowserViews. Der Renderer zeigt fuer Pairing-Instanzen eigene Aktionen fuer Rotation und Widerruf; Widerruf entfernt die lokale Pairing-Instanz. `npm test` laeuft mit 66/66 Tests; `npm run check`, `npm run test:electron-smoke`, `cargo check` und `git diff --check` sind gruen. |
| 2026-06-13 | Welle 3 App-Restart-Smoke ergaenzt: Eine lokale unmanaged Instanz wird ueber `peer ensure` angebunden, Registry wird auf Platte geschrieben, nach simuliertem App-Neustart neu geladen und ohne ctox.dev Account wieder als WebRTC-only Launch mit SecretStore-Referenz verfuegbar. `npm test` laeuft mit 67/67 Tests; Fresh-Machine-Installation im echten Runtime-Flow bleibt offen. |
| 2026-06-13 | Welle 7/8 Release-Konfiguration ergaenzt: `electron-builder`-Config, Lockfile, macOS Hardened Runtime/Entitlements, Notarize-Hook, HTTPS-Auto-Update-Feed, runtime `electron-updater` Wiring und `release:check` Verifier sind vorhanden. `npm test` laeuft mit 70/70 Tests; `npm run release:check` und `npm run check` sind gruen. Signierte/notarisierte Plattform-Artefakte bleiben offen. |
| 2026-06-13 | Welle 8 Packaging-Smoke ergaenzt: `npm run pack:dir:smoke` baut lokal eine unpacked macOS `.app`, ueberspringt Notarization nur fuer `--dir`, und prueft Bundle-ID, App-Icon, Desktop-Protokoll, `app.asar` sowie Ausschluss von Tests/Release-Artefakten. Notarization bleibt fuer Distributionsziele ohne Apple-Secrets fail-closed. |
| 2026-06-13 | Welle 7 macOS-Keychain-Runtime validiert: Der Secret-Store nutzt einen stdin-faehigen Prozessrunner mit Timeout, macOS `security add-generic-password -w` bekommt das Secret ueber stdin statt ueber Prozessargumente, und `npm run smoke:keychain-runtime` prueft Set/Get/Delete gegen die echte macOS Keychain. `npm test` laeuft mit 72/72 Tests; `npm run check` und der Keychain-Smoke sind gruen. Linux/Windows Runtime-Keychain-Smokes bleiben offen. |
| 2026-06-13 | Welle 6 lokal abgeschlossen: Der Renderer-Smoke aktiviert eine ctox.dev-managed Instanz per Sidebar-Klick und SSH-/Pairing-Instanzen per `Cmd/Ctrl+K`, Suche und `Enter`. Die Smoke-Evidenz protokolliert pro Aktivierung `rxdb-webrtc` und `httpDataProxy=false`. `npm run check` und `node test/electron-renderer-badges-smoke.cjs` sind gruen; Live-/Cross-Platform-E2E bleibt Welle 8. |
| 2026-06-13 | Welle 5 erweitert: SSH Fresh-Install ist als eigener Contract-Pfad umgesetzt. Nach Host-Key-Trust und Preflight verlangt der Pfad Linux, `bash`, `curl`, `systemd` und passwordless `sudo -n`, nutzt den offiziellen `install.sh`, fuehrt danach `ctox upgrade`, optional `ctox start` und `peer ensure` aus und speichert Pairing-Secrets nur im SecretStore. `npm test` laeuft mit 76/76 Tests; `npm run check` ist gruen. Live-VPS, interaktiver Sudo-Prompt und lokales Artefakt bleiben offen. |
| 2026-06-13 | Welle 8 HTTP-Datenpfad-Gate ergaenzt: BrowserViews blockieren verbotene Business-OS-HTTP-Datenpfade per `webRequest.onBeforeRequest`, erlauben aber explizite Control-Plane-Pfade wie Status und Sync-Konfig. Der neue Electron-Smoke laedt eine lokale Shell, laesst Status durch und beweist, dass `/api/business-os/records`, `/rxdb/pull` und `/commands` nicht beim Server ankommen. `npm test` laeuft mit 78/78 Tests; `npm run check`, `npm run test:electron-smoke` und `git diff --check` sind gruen. |
| 2026-06-13 | Welle 8 Release-Gate nachgezogen: Secret-Freiheit ist nun als Gate markiert, belegt durch Registry-Rejects, Support-Bundle-Log-Redaction und Crash-Reporter-Extra-Sanitizing. Die bereits gelaufene Desktop-Suite deckt diese Pfade mit ab (`npm test` 78/78). |
| 2026-06-13 | Welle 8 Release-Matrix ergaenzt: `.github/workflows/release.yml` baut Business OS Desktop jetzt in einer eigenen Matrix fuer macOS arm64/x64, Linux x64 und Windows x64, fuehrt npm-Tests, Syntax-/Release-Checks und Electron-Smokes aus, baut Distribution-Artefakte und prueft macOS signed artifacts nach dem Build. `npm run release:check`, `npm run check` und YAML-Parse des Release-Workflows sind gruen. Live-Tag-Run mit signierten/notarisierten Artefakten bleibt offen. |
| 2026-06-13 | Welle 7/8 Keychain-Gate erweitert: `smoke:keychain-runtime` deckt macOS Keychain, Linux Secret Service und Windows Credential Manager ab; Windows nutzt nun echte `advapi32` Credential-Manager-Aufrufe statt Stub-Output. Die Release-Matrix startet den Smoke auf allen Desktop-Zielplattformen, inklusive `dbus-run-session`/`gnome-keyring` fuer Linux. Lokal gruen: `npm test` 78/78, `npm run check`, `npm run release:check`, YAML-Parse und macOS-Keychain-Runtime-Smoke. Linux/Windows Live-Matrix-Ergebnis bleibt offen. |
| 2026-06-13 | Welle 5 Sudo-Askpass-Vertrag ergaenzt: Fresh-SSH-Install kann Hosts ohne passwordless sudo ueber eine SecretStore-`sudoPasswordRef` bedienen. Der Remote-Befehl nutzt `sudo -A` mit temporaerem Askpass/FIFO, schreibt das Secret nur ueber SSH-stdin und bleibt frei von `sshpass`, Passwortargumenten und `sudo -S`. `npm test` laeuft mit 81/81 Tests, `npm run check` ist gruen. Offen bleibt Live-VPS-E2E. |
| 2026-06-13 | Welle 5 lokaler Artefaktpfad ergaenzt: Fresh-SSH-Install akzeptiert nun ein absolutes `localArtifactPath`, bereitet den Remote-Cache per SSH vor, laedt das Binary per `scp` mit strikter Host-Key-Pruefung hoch und installiert es user-local nach `~/.local/bin/ctox`. Der Pfad nutzt weder offiziellen Online-Installer noch `curl`/`sudo` und bleibt frei von `sshpass`/`sudo -S`. `node --test test/ssh-source.test.cjs` laeuft mit 20/20 Tests, `npm test` mit 85/85 und `npm run check` ist gruen. Offen bleibt der Live-VPS-Nachweis. |
| 2026-06-13 | Welle 5 nativer Sudo-Secret-Prompt ergaenzt: SSH-Details zeigen einen Passwortdialog mit `type=password`, speichern das Remote-Sudo-Passwort ueber `ssh:store-sudo-password` direkt im SecretStore und zeigen danach nur die `keychain://...`-`sudoPasswordRef`. Unit-Tests belegen SecretStore-only Speicherung ohne Registry-Secret; der Electron-Renderer-Smoke prueft Dialog, Host/User/Port-Uebergabe und fehlende Passwortanzeige. `npm test` laeuft mit 87/87 Tests, `npm run check` und `npm run test:electron-smoke` sind gruen. |
| 2026-06-13 | Welle 5 SSH-Login-Askpass ergaenzt: Passwort-only VPS-Zugaenge koennen jetzt ueber eine SecretStore-`sshPasswordRef` angebunden werden. SSH/SCP wechseln nur mit dieser Ref auf `BatchMode=no`, aktivieren Passwort/KbdInteractive-Auth und nutzen ein temporaeres OpenSSH-Askpass-Skript, das das Passwort per Ref aus dem OS-Keychain liest und danach geloescht wird. Der Renderer speichert SSH-Login- und Sudo-Passwort getrennt und zeigt nur Referenzen. `npm test` laeuft mit 92/92 Tests, `npm run check` und `npm run test:electron-smoke` sind gruen. Offen bleibt die Live-Verifikation gegen einen echten Passwort-only VPS. |
| 2026-06-13 | Welle 3 Local-Runtime-Smoke ergaenzt und echten Ensure-Shape-Bug korrigiert: `peer ensure` liefert nur `ok/running`, daher lesen Local/SSH-Ensure-Runner danach explizit `peer status`, bevor Desktop attached. Neuer `npm run smoke:local-runtime` nutzt ein reales lokales `ctox`, installiert Business OS in ein Temp-Ziel, attached via `peer ensure`, prueft secret-freie Registry und WebRTC-only Launch. Gruen: `npm run smoke:local-runtime`, `npm test` 93/93, `npm run check`, `npm run test:electron-smoke`. Clean-Profile/Fresh-Machine ohne vorhandene lokale CTOX-Installation bleibt offen. |
| 2026-06-13 | Welle 2 lokal gehaertet: ctox.dev Source und SourceManager cachen weder Session-Package noch Launch-Token. Neue Unit-Tests belegen serverseitigen Tenant-Entzug ohne Verlust unmanaged Instanzen und frische Launch-Token pro Aktivierung; der ctox.dev Electron-Smoke beweist denselben Pfad ueber den Electron Cookie-Jar mit lokalem ctox.dev-Mock. Gruen: `node --test test/ctox-dev-source.test.cjs test/source-manager.test.cjs`, `node test/electron-ctox-dev-login-smoke.cjs`, `npm test` 96/96, `npm run check`, `npm run test:electron-smoke`. Live-ctox.dev-Login, Live-Revocation und Live-Management-Route bleiben offen. |
| 2026-06-13 | Welle 3 Root-Bindung gehaertet: Local-Daemon-Kommandos setzen fuer valide `ctoxRoot`-Pfade den bestehenden `CTOX_ROOT`-Child-Kontext und lehnen Business-OS-Kundenroots als Runtime-Root ab, statt unbemerkt die globale CTOX-Installation zu verwenden. `npm run smoke:local-runtime` nutzt jetzt ein frisches Desktop-Profil, installiert einen Business-OS-Kundenroot, beweist dessen Ablehnung als Runtime-Root, attached ueber einen validen CTOX Runtime-Root, laedt nach simuliertem Neustart ohne ctox.dev Account und bleibt WebRTC-only. Gruen: `node --test test/local-source.test.cjs`, `npm run smoke:local-runtime`, `npm test` 98/98, `npm run check`, `npm run test:electron-smoke`, `npm run release:check`. Fresh-Machine ohne vorhandenes CTOX-Binary bleibt offen. |
| 2026-06-13 | Welle 5 Live-SSH-Passwortpfad ergaenzt und keychain-backed verifiziert: `smoke:ssh-password-live` liest das SSH-Passwort nur ueber stdin, nutzt strikte Host-Key-Pruefung, macOS Keychain-backed `sshPasswordRef`, OpenSSH Askpass und Remote-Preflight, und redigiert die Evidenz. Gegen den SKF-Testhost `57.129.123.108` ist der platform-keychain Livepfad gruen; Evidenz: Fingerprint `SHA256:ZIFGq4ACB3opMov6dULHDo6LeWwKQh85CQ1Ocj7jSKA`, Linux x86_64, Shell/Bash/Curl/systemd/sudo/passwordless-sudo/ctox erreichbar. Fuer TTY-Harness-Laeufe wurde der macOS-Keychain-Set-Timeout auf 120s erhoeht; `--file-askpass-fallback` bleibt nur ein schwaecheres optionales Diagnosewerkzeug. Gruen: Keychain-backed Live-Smoke SKF, `npm run smoke:keychain-runtime`, `npm run check`, `npm test` 98/98, `npm run test:electron-smoke`, `npm run release:check`. |
| 2026-06-13 | Welle 5 Existing-CTOX Live-Attach ergaenzt: `smoke:ssh-password-live -- --attach` nutzt denselben platform-keychain-backed `sshPasswordRef`, fuehrt gegen den SKF-Testhost `57.129.123.108` Remote-`peer ensure` aus, registriert lokal eine `ssh_managed` Instanz, prueft die eigene Session-Partition und erzeugt eine WebRTC-only Launch-Konfig ohne Secret-Leak in Registry oder redigierter Evidenz. Welle 5 steigt auf 96%; Fresh-Install, lokaler Artefaktpfad und zweiter Live-VPS bleiben offen. Gruen: Live-Smoke SKF mit `--attach`, `node --test test/ssh-source.test.cjs`, `npm run check`, `npm test` 98/98, `npm run test:electron-smoke`, `npm run release:check`. |
| 2026-06-13 | Welle 2 ctox.dev Live-Pfad nachgezogen: Desktop-Login-URL nutzt jetzt den live vorhandenen `/dashboard?desktop=1&client=ctox-business-os-desktop` Pfad; Completion akzeptiert den echten `ctox-business-os-desktop://auth/callback` und Dashboard-`auth_completed=1`, und die ctox.dev-Verwaltung oeffnet das live erreichbare `/dashboard?tenant=<tenant-id>` statt des 404-Pfads `/desktop/instances`. Neuer opt-in Live-Smoke `smoke:ctox-dev-live` liest das ctox.dev Passwort nur ueber stdin, meldet sich ueber den echten Passwort-Endpunkt im Electron-Cookie-Jar an, prueft Session-Package, Tenant-Liste und Launch-Token. Live gruen gegen Produktion: Testaccount sieht sechs Tenants inklusive Kunstmen und SKF; Kunstmen-Launch liefert Origin `https://cto1.kunstmen.com`, `transport=webrtc`, `http_bridge_available=false`, Signaling-URL und redigierten Room-Secret-Presence-Nachweis. Welle 2 steigt auf 85%, Gesamt auf 92%; offen bleiben BrowserWindow/AuthPanel-Callback-E2E und echte Access-Revocation. Gruen: `smoke:ctox-dev-live -- --expected-tenant Kunstmen --expected-tenant SKF --launch-first`, `node --test test/ctox-dev-login.test.cjs test/ctox-dev-source.test.cjs test/source-manager.test.cjs`, `node test/electron-ctox-dev-login-smoke.cjs`, `node test/electron-renderer-badges-smoke.cjs`, `npm run check`, `npm test` 98/98, `npm run test:electron-smoke`, `npm run release:check`. |
| 2026-06-13 | Welle 2 Auth-Callback-Luecke geschlossen: Der Desktop-Protokollhandler parst `ctox-business-os-desktop://auth/callback` jetzt als eigene Auth-Aktion, dispatcht ihn an den aktiven ctox.dev Login-Promise und behandelt fehlende aktive Loginfenster als No-op statt als unbekannten Deep-Link. Der Electron Protocol-Smoke deckt den Auth-Callback ueber `open-url` nach App-Ready ab; der ctox.dev Login-Unit-Test beweist, dass ein aktives Loginfenster per OS-Custom-Scheme abgeschlossen wird. Welle 2 steigt auf 87%; der echte visuelle ctox.dev AuthPanel-Login bleibt weiterhin offen. Gruen: `node --test test/ctox-dev-login.test.cjs test/protocol-handler.test.cjs`, `node --test test/ctox-dev-source.test.cjs test/source-manager.test.cjs`, `node test/electron-protocol-handler-smoke.cjs`, `node test/electron-ctox-dev-login-smoke.cjs`, `npm run check`, `npm test` 101/101, `npm run test:electron-smoke`, `npm run release:check`, `git diff --check`. |
| 2026-06-13 | Welle 2 Live-Management-Deep-Link gruen: `smoke:ctox-dev-live -- --manage-first` laedt nach ctox.dev Passwort-Auth den konkreten `/dashboard?tenant=<tenant-id>` Link im Electron Default-Session-Cookie-Jar, prueft HTTP 200, ctox.dev-Origin, keinen Login-Redirect und Tenant-Hinweis im gerenderten BrowserWindow-DOM. Gegen Produktion gruen fuer Kunstmen; danach bleibt der Launch-Pfad WebRTC-only (`transport=webrtc`, `http_bridge_available=false`). Welle 2 steigt auf 90%, Gesamt auf 93%; offen bleiben echter visueller AuthPanel-Login und echte Access-Revocation. Gruen: `smoke:ctox-dev-live -- --expected-tenant Kunstmen --manage-first --launch-first`. |
| 2026-06-13 | Welle 2 echter AuthPanel-Login gruen: `smoke:ctox-dev-live -- --auth-window --manage-first --launch-first` fuellt die echte ctox.dev Login-UI im Electron BrowserWindow aus, erkennt die danach gueltige Session ueber `/api/desktop/session-package`, schliesst das Loginfenster via `session-check`, prueft den Kunstmen-Management-Deep-Link und konsumiert danach einen WebRTC-only Launch. Dabei wurde ein Produktionsrisiko beseitigt: Der Desktop haengt nicht mehr an einem live nicht zuverlaessig navigierten Custom-Scheme-Callback; der Callback bleibt lokal/Electron ueber den Protocol-Smoke abgedeckt. Welle 2 steigt auf 95%, Gesamt auf 94%; offen bleibt echte ctox.dev Access-Revocation/Session-Rotation. Gruen: Live-Smoke AuthPanel, `node --test test/ctox-dev-login.test.cjs test/protocol-handler.test.cjs`, `npm run check`, `npm test` 102/102, `npm run test:electron-smoke`, `npm run release:check`, `git diff --check`. |
| 2026-06-13 | Welle 5 zweiter VPS-Nachweis gruen: `smoke:ssh-password-live -- --host 51.210.246.120 --file-askpass-fallback --attach --display-name Kunstmen` prueft den Kunstmen-Testhost mit gepinntem ED25519-Fingerprint, echtem Passwort-SSH, Remote-Preflight, Remote-`peer ensure`, `ssh_managed` Registry-Shape, eigener Session-Partition und WebRTC-only Launch ohne Registry-Secret-Leak. Der Smoke-Harness erlaubt `--file-askpass-fallback --attach` jetzt explizit und speichert Live-Smoke-Secrets dabei nur in Memory; das ist ein schwaecherer zweiter Host-Nachweis, waehrend der voll platform-keychain-backed Attach weiter durch SKF belegt ist. Welle 5 steigt auf 97%; Fresh-Install und lokaler Artefaktpfad gegen echte VPS bleiben offen. Gruen: Kunstmen Fallback-Attach-Smoke, `node --check scripts/smoke-ssh-password-live.cjs`, `node --test test/ssh-source.test.cjs test/source-manager.test.cjs test/secret-store.test.cjs`. |
| 2026-06-13 | Welle 5 API-backed Fresh-Installer-Flags umgesetzt und live negativ validiert: Desktop-SSH-Fresh-Install kann `apiProvider`, `model` und `backend` validiert an den offiziellen `install.sh` per CLI-Argumenten weitergeben; `smoke:ssh-password-live` bietet dafuer `--install-api-provider`, `--install-model` und `--install-backend`. Gegen Kunstmen zeigte der Live-Test zwei echte Befunde: ohne API-Provider scheitert der offizielle Installer am lokalen GPU-Default, mit `--install-api-provider openai` startet der reale Cargo-Source-Build, ueberschreitet aber den 900s Desktop-Smoke-Timeout. Der verwaiste Build wurde gestoppt. Fresh-Install bleibt daher nicht production-ready; noetig sind prebuilt Linux-Artefakte oder ein installer/progress/timeout-faehiger Desktop-Fresh-Install-Pfad. Gruen: `node --check src/main/sources.cjs`, `node --check scripts/smoke-ssh-password-live.cjs`, `node --test test/ssh-source.test.cjs`, `npm run check`, `npm test` 103/103, `npm run release:check`, `npm run test:electron-smoke`, `git diff --check`. |
| 2026-06-13 | Welle 4 Live-Rotation nachgezogen und echten Pairing-ID-Bug behoben: Remote-`peer rotate` aendert das `sync_room`; der Desktop band die Pairing-ID bisher an `instance_id + sync_room` und wies dadurch echte Rotations-Invites als falsche Instanz ab. Die ID ist jetzt `instance_id`-stabil, alte sync-room-basierte IDs werden beim Rotate migriert, alte SecretRefs geloescht. Neuer opt-in Smoke `smoke:pairing-ssh-live` prueft gegen SKF echtes SSH mit gepinntem Host-Key, Remote-`peer rotate`, lokalen Desktop-Import/Rotate/Revoke, WebRTC-only Launch und Secret-Freiheit von Registry/Evidenz. Live-Befund bleibt nicht voll gruen: Beide Test-VPS haben noch keinen ausgerollten `ctox business-os desktop invite` CLI-Befehl, daher nutzte der Smoke explizit den schwaecheren `--allow-peer-status-invite` Fallback; die aktive Peer-Session-Aenderung ist ebenfalls nicht bewiesen. Welle 4 steigt auf 96%. Gruen: `node --test test/invite-source.test.cjs`, `node --check scripts/smoke-pairing-ssh-live.cjs`, `npm run check`, Live-Smoke SKF mit `--rotate --revoke-local --allow-peer-status-invite`. |
| 2026-06-13 | Welle 4 Native-Peer-Rotation lokal gehaertet: Der laufende native RxDB-Peer vergleicht im Watchdog seine aktive Room-/Passwort-/Signaling-Konfiguration mit der persistierten Sync-Konfiguration und beendet sich bei Aenderung kontrolliert, damit der bestehende Supervisor mit frischer Konfiguration respawnt. Damit ist die lokale Produktionslogik fuer Remote-`peer rotate` vorhanden; live bleibt der Browser-WebRTC-Reconnect offen, bis der aktualisierte `ctox` Stand auf den Test-VPS ausgerollt und gegen echte Reconnect-Evidenz geprueft ist. Welle 4 steigt auf 97%. Gruen: `cargo test native_peer_ -- --nocapture`. |
| 2026-06-13 | Welle 3 Fresh-Machine-Vertrag verbessert: Der lokale Desktop-Quellpfad sucht bei fehlendem explizitem `ctoxBinary` jetzt zuerst nach einem gebuendelten CTOX-Helper in den App-Resources und faellt erst danach auf PATH-`ctox` zurueck. Neuer Smoke `smoke:local-bundled-runtime` beweist ein frisches Desktop-Profil ohne ctox.dev Account, ohne `ctoxRoot` und ohne explizites `ctoxBinary`: lokale Installation, Inspect, Attach, persistierter Neustart und WebRTC-only Launch bleiben secret-frei. Die Release-Matrix baut den plattformpassenden CTOX-Helper vor `electron-builder`; `electron-builder` paketiert `resources/ctox` als externe App-Resource, sobald der Helper liegt. Welle 3 steigt auf 92%, Gesamt auf 95%. Offen bleibt der signierte Release-/Fresh-Machine-Nachweis auf sauberer Maschine. Gruen: `npm run smoke:local-bundled-runtime`, `node --test test/local-source.test.cjs`, `npm test` 105/105, `npm run check`, `npm run release:check`. |
| 2026-06-14 | Welle 8 Release-Artefakt-Smoke plattformweit gemacht: `smoke:signed-artifacts` ist nicht mehr mac-only. macOS prueft `.app`, `app.asar`, gebuendelten Helper sowie `codesign`/`spctl`; Linux prueft AppImage, `.deb`, `linux-unpacked`, `app.asar` und Helper; Windows prueft NSIS-Installer, `win-unpacked`, `app.asar` und `ctox.exe`. Der Release-Workflow ruft den Smoke nun mit `matrix.builderPlatform` auf allen drei Plattformen auf, und `release:check` erzwingt diesen Vertrag. Welle 8 steigt auf 72%; der echte Tag-/Signed-/Notarization-Run bleibt offen. Gruen: synthetischer mac/linux/win Artefakt-Smoke, `npm run check`, `npm run release:check`, Release-Workflow-YAML-Parse. |
| 2026-06-14 | Welle 8 Release-Smoke-Evidenz ergaenzt: `smoke:signed-artifacts` kann jetzt pro Plattform ein JSON nach `ctox-business-os-desktop-release-artifact-smoke/v1` mit relativen Artefaktpfaden schreiben. Der Tag-Release-Workflow erzeugt `release/artifact-smoke-${{ matrix.builderPlatform }}-${{ matrix.arch }}.json` und laedt diese Evidenz zusammen mit den Artefakten hoch; `release:check` erzwingt Workflow- und Script-Vertrag. Welle 8 steigt auf 74%; echte signierte/notarisierte Tag-Artefakte bleiben weiter offen. Gruen: `node --test test/signed-artifacts-smoke.test.cjs`, `npm test`, `npm run check`, `npm run release:check`, Release-Workflow-YAML-Parse. |
