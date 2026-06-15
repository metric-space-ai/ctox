# Business OS Electron Desktop: Production Readiness Plan

Status: In Umsetzung, Stand 2026-06-15

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
- ctox.dev-managed Instanzen, die im Session-Package zwar noch erscheinen, aber
  `launchAllowed:false` tragen, werden jetzt fail-closed als `needs_auth`
  behandelt: Der SourceManager fordert fuer solche Instanzen keinen
  Launch-Token mehr an. Damit ist auch der Sperrfall abgedeckt, wenn ctox.dev
  einen Tenant nicht entfernt, sondern nicht launchbar meldet.
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
  App-Resource auf.
- `npm run pack:dir:bundled-runtime-smoke` baut lokal eine unpacked macOS
  `.app` mit temporaerem gebuendeltem CTOX-Helper, prueft den ausfuehrbaren
  Helper in `Contents/Resources/ctox/ctox` und nutzt genau diesen verpackten
  Helper aus einem frischen Desktop-Profil fuer Installation, Inspect, Attach,
  simulierten App-Neustart und WebRTC-only Launch ohne Registry-Secret-Leak.
  Der `main`-/PR-CI-Desktop-E2E-Job fuehrt diesen Smoke auf macOS ebenfalls
  aus, damit der Nachweis nicht nur lokal existiert. GitHub-Actions-Run
  `27517440788` fuer Commit `a182ea06` hat den Step
  `Packaged bundled helper smoke` im Job `Business OS Desktop E2E (mac)`
  erfolgreich abgeschlossen. Der aktuelle Desktop-CI-Recheck `27518440351`
  fuer Commit `ebb0b83a` bestaetigt denselben Desktop-E2E-Pfad auf macOS,
  Linux und Windows; der macOS-Job fuehrt den `Packaged bundled helper smoke`
  erneut erfolgreich aus. Noch offen ist der echte Tag-/Signed-Run auf sauberer
  Maschine.
- Der Folge-Run `27518721915` fuer Commit `e5d98bf8` bestaetigt den
  Desktop-Pfad nach dem Plan-Nachtrag erneut: `Business OS Desktop E2E (mac)`,
  `Business OS Desktop E2E (linux)`, `Business OS Desktop E2E (win)` und
  `Desktop extra check (linux)` sind gruen; macOS enthaelt wieder den
  erfolgreichen Step `Packaged bundled helper smoke`. Die breite
  Linux-x86_64-CLI-Matrix war zum letzten Pruefzeitpunkt noch aktiv, hatte aber
  die RxDB-/Business-OS-Guards bis einschliesslich Datei-Tombstones erfolgreich
  abgeschlossen.
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
  Pfad mit WebRTC-only Launch-Konfig und ohne Secret-Leak gruen. Nach
  Installation des lokal SHA256-verifizierten Linux-x64-CLI-Artefakts aus dem
  `v0.3.29`-Release-Run erzeugt SKF initiale und rotierte Invites ueber den
  echten `ctox business-os desktop invite --format json` Pfad ohne
  `--allow-peer-status-invite`-Fallback.
- Der Browser/Rust-Rollover-Beweis fuer Pairing-Rotation ist jetzt
  retry-frei gruen: RxDB-Soak `27503869533` lief mit
  `SOAK_FAIL_ON_RETRY=1` auf Commit `6e352b7f` im Modus
  `rollover-native-peer-browser-to-rust` im ersten Versuch durch,
  replizierte nach Native-Peer-Restart ueber `desktop_files` und
  `desktop_file_chunks` jeweils mit `peerCount=1`, sah
  `checkpoint_epoch_count=11` und meldete keine Browser-/Request-/Assetfehler.
- SSH-managed Quelle deckt Host-Key-Fingerprint-Trust, app-eigene
  `ssh_known_hosts`, OpenSSH key/agent Attach, Remote-Preflight und
  Existing-CTOX-Upgrade auf Contract-Ebene ab.
- SSH-managed Fresh-Install ist auf Contract-Ebene vorhanden: nach
  Host-Key-Trust und Preflight nutzt der Stable-Pfad
  das offizielle GitHub-Release-Bundle, validiert die `.sha256`-Pruefsumme,
  installiert das Binary user-local nach `~/.local/bin/ctox` und fuehrt danach
  `peer ensure` aus. Stable mit API-backed Seed-Flags nutzt denselben
  Release-Bundle-Pfad und schreibt die Runtime-Auswahl in `runtime_env_kv`
  (`CTOX_CHAT_SOURCE=api`, `CTOX_API_PROVIDER`, Modellschluessel) statt aus
  Source zu bauen. Der Source-Installer bleibt fuer `dev` erhalten;
  Passwortargumente, `sshpass` und `sudo -S` bleiben verboten.
- SSH-managed Fresh-Install kann API-backed Setups seed-en
  (`--api-provider`, `--model`, `--backend`), damit CPU-only VPS nicht auf das
  Default-Profil mit lokaler GPU-Inferenz fallen. Im Stable-Pfad werden
  Provider und Modell als SQLite-Runtime-Config geschrieben, nicht als
  Runtime-Env-Toggles; `backend` bleibt Dev-/Source-Installer-Option und wird
  im Stable-Pfad nicht als Runtime-Toggle geschrieben. Im Dev-/Source-Pfad
  werden die Flags weiterhin an `install.sh` durchgereicht.
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
- Der Online-Stable-Fresh-Install ist live gegen den SKF-Testhost gruen: Der
  Desktop-Smoke laedt `ctox-linux-x64.tar.gz` aus dem neuesten GitHub-Release,
  prueft `sha256sum -c`, installiert `target/release/ctox` user-local, seedet
  optional API-backed Runtime-Config per SQLite, startet CTOX, fuehrt
  `peer ensure` aus und erzeugt eine WebRTC-only `ssh_managed` Launch-Konfig
  ohne Registry-Secret-Leak. Der Fresh-Nachweis nutzt den
  `file-askpass-fallback`; der platform-keychain-backed SSH-Passwortpfad ist
  separat fuer Existing-Attach gruen und nutzt denselben SSH-Askpass-Vertrag.
- Der lokale Artefaktpfad ist live gegen den SKF-Testhost gruen: Das
  GitHub-Release-Artefakt `ctox-linux-x64.tar.gz` aus `v0.3.27` wurde per
  SHA256 verifiziert, das enthaltene Linux-x64-`ctox` per `scp` hochgeladen,
  nach `~/.local/bin/ctox` installiert, mit `ctox start/status` gestartet und
  danach per `peer ensure` als `ssh_managed` Desktop-Instanz angebunden. Der
  Launch bleibt `transport=webrtc` / `http_bridge_available=false`; Registry
  und Evidenz bleiben secret-frei. Der Smoke nutzte fuer diesen Artefaktpfad
  bewusst den File-Askpass-Fallback, waehrend der staerkere platform-keychain-
  Passwortpfad fuer SKF separat gruen belegt ist.
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
- `npm run release:secrets:check` prueft vor einem Tag-Run per `gh secret
  list`, ob die benoetigten Repo-Secret-Namen fuer signierte/notarisierte
  Business-OS-Desktop-Releases vorhanden sind. Der aktuelle GitHub-Befund ist
  negativ: `gh secret list --repo metric-space-ai/ctox --json name,updatedAt`
  liefert `[]`; ohne `APPLE_ID`, `APPLE_ID_PASSWORD`, `APPLE_TEAM_ID`,
  `CTOX_BUSINESS_OS_DESKTOP_CSC_LINK` und
  `CTOX_BUSINESS_OS_DESKTOP_CSC_KEY_PASSWORD` wuerde ein neuer Tag-Run am
  macOS-Preflight scheitern. Der Recheck vom 2026-06-15 bleibt negativ:
  `npm run release:secrets:check` bricht fail-closed mit genau diesen fuenf
  fehlenden Secrets ab, und die GitHub-Secret-Liste liefert weiter `[]`.
- Die normale `main`-/PR-CI enthält jetzt zusätzlich ein Business-OS-Desktop-
  E2E-Gate auf macOS, Linux und Windows. Es läuft ohne Distribution-Build,
  aber mit `npm test`, `npm run check`, `npm run release:check`,
  Electron-Smokes und Plattform-Keychain-Runtime-Smoke; Linux nutzt dafür
  `xvfb`, `dbus-run-session`, `gnome-keyring` und `libsecret-tools`.
- Dieses Desktop-E2E-Gate ist in GitHub Actions live gruen auf macOS, Linux
  und Windows: CI-Run `27484687888` fuer Commit `1b1940d5` hat alle drei
  `Business OS Desktop E2E`-Jobs erfolgreich abgeschlossen; der Folge-Run
  `27484995659` fuer Commit `01e258b9` bestaetigt dieselben Desktop-Jobs
  erneut. Weitere `main`-Runs `27485327715` fuer Commit `0e982165` und
  `27486101670` fuer Commit `80b11085` bestaetigen den Desktop-E2E-Pfad
  ebenfalls auf macOS, Linux und Windows. Der `main`-CI-Run `27489031650` fuer
  Commit `4dc20c71` bestaetigt den Desktop-E2E-Pfad erneut und ist als
  Gesamt-CI inklusive CTOX-CLI-Matrix gruen; Run `27501383594` fuer Commit
  `761da15a` ist ebenfalls vollstaendig gruen und
  bestaetigt Business-OS-Desktop-E2E auf macOS/Linux/Windows sowie die gesamte
  CTOX-CLI-Matrix.
- Derselbe Workflow fuehrt den Keychain-Runtime-Smoke auf macOS, Linux und
  Windows aus; Linux startet dafuer eine echte Secret-Service-Session ueber
  `dbus-run-session` und `gnome-keyring`. Die Runs `27485327715` und
  `27486101670` sowie der aktuelle gruene Run `27489031650` beweisen diesen
  Runtime-SecretStore-Pfad live fuer alle drei Desktop-Zielplattformen.
- Der aktuelle `IoT Engine Soak`-Run `27489031659` fuer Commit `4dc20c71` ist
  gruen. Damit ist auch der durch `src/core/business_os/**` getriggerte
  Business-OS-/RxDB-only Soak nachgezogen; der vorherige Fehlstand war ein
  fehlender gepinnter JS-Test-Dependency-Install fuer `esbuild`.
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
- Der aktuelle Live-Recheck vom 2026-06-15 ist gruen: Die echte AuthPanel-UI
  wurde im Electron BrowserWindow automatisiert, ctox.dev lieferte sechs
  Tenants inklusive Kunstmen und SKF, `/dashboard?tenant=<tenant-id>` lud mit
  HTTP 200 ohne Login-Redirect, und der Kunstmen-Launch blieb
  `transport=webrtc` / `httpBridgeAvailable=false`.
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

- Server-seitige Access Revocation gegen echte ctox.dev-Tenants; lokal ist der
  Contract inklusive Electron-Smoke gruen, und der Desktop blockiert jetzt auch
  den Live-kompatiblen Sperrfall `launchAllowed:false` vor dem
  Launch-Token-Request. Desktop-Session-Rotation ist live bewiesen: Logout
  entfernt jetzt auch ctox.dev-Domaincookies aus Electron, Session-Package ist
  danach 401, alte Launches blockieren, Re-Login stellt die Tenants wieder her.
  Der Live-Smoke hat jetzt einen opt-in Zwei-Account-Modus fuer reversible
  Revocation per temporaerer Rollenherabstufung auf `viewer`: Er prueft vorher
  einen WebRTC-only Launch fuer ein launchfaehiges Nicht-Owner-Mitglied, setzt
  dieses Mitglied per ctox.dev Membership-API auf `viewer`, akzeptiert danach
  beide gueltigen ctox.dev-Reaktionen aus Desktop-Sicht (`needs_auth` oder
  nicht mehr gelistet), beweist einen blockierten Launch und stellt danach die
  urspruengliche Rolle wieder her. Der Modus ist noch nicht gegen Produktion
  gelaufen.
  Der aktuelle Live-Account ist laut read-only ctox.dev Membership-Check auf
  allen sichtbaren Tenants `owner`; ein sicherer, reversibler
  Membership-Entzug ist damit nicht beweisbar. Dafuer braucht es einen
  separaten Nicht-Owner-Testmember oder eine explizit dafuer angelegte
  Testinstanz. Die ctox.dev-Produktionsquelle bestaetigt den externen Haken:
  neue Nicht-Owner entstehen ueber Einladung/Magic-Link oder einen bereits
  vorhandenen User; bei konfiguriertem Resend liefert `sendInvitationEmail`
  keinen `previewLink`. Ohne Zugriff auf die Empfaenger-Mailbox oder ein
  vorhandenes zweites Passwortkonto kann der Smoke keinen authentifizierten
  Mitglieds-Login herstellen.
- Komplett frisches OS ohne vorhandenes lokales CTOX-Binary/validen CTOX
  Runtime-Root ist lokal bis zur unpacked `.app` bewiesen: Der lokale
  Quellpfad kann einen gebuendelten CTOX-Helper aus den App-Resources nutzen,
  der Release-Workflow baut diesen Helper pro Plattform vor dem
  Electron-Package, und die Smokes laufen ohne ctox.dev Account, `ctoxRoot`
  oder explizites `ctoxBinary`. Offen bleibt der echte Tag-/Signed-Run, der
  denselben Pfad auf sauberer Maschine mit signiertem/notarisiertem Artefakt
  ausfuehrt.
- Signierte/notarisierte Installer-Artefakte sind aus einem echten Tag-Run noch
  nicht live erzeugt und verifiziert. Die plattformweite Artefakt-Smoke-Logik
  und die Plattform-Keychain-Smokes sind lokal beziehungsweise im `main`-CI
  bewiesen, aber noch nicht durch einen echten Tag-Run mit signierten
  Installer-Artefakten. Der Release-Workflow prueft jetzt zusaetzlich vor dem
  Packaging, dass der frisch gebaute gebuendelte `ctox`-Helper den
  `business-os desktop invite` JSON-Vertrag ausfuehren kann; das verhindert
  kuenftige Desktop-Releases mit einem zu alten Helper, ersetzt aber nicht den
  echten Tag-Run. Ein lokaler Secret-Namen-Preflight ist vorhanden; aktuell
  meldet GitHub aber keine konfigurierten Repo-Secrets, daher ist ein neuer
  Tag-Release ohne Secret-Konfiguration nicht sinnvoll. Der historische
  `v0.3.28` Tag-Release ist kein verwertbarer Produktionsnachweis: Windows
  scheiterte dort noch an Unix-only Service-IPC-Symbolen, waehrend der aktuelle
  `main`-Stand den Windows-CLI-Check wieder besteht; Linux arm64 scheiterte im
  selben Release-Lauf erst beim Artefakt-Upload mit `ETIMEDOUT`, also nicht an
  einem Desktop-Produktcheck.

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
| 2. ctox.dev Managed Source | 14% | In Umsetzung | 99% |
| 3. Local Daemon Source | 12% | In Umsetzung | 97% |
| 4. Pairing Invite Source | 12% | Abgeschlossen | 100% |
| 5. SSH/Sudo Remote Install Source | 14% | Abgeschlossen | 100% |
| 6. Unified Switcher UX | 10% | Abgeschlossen | 100% |
| 7. Secret Storage & Hardening | 10% | Abgeschlossen | 100% |
| 8. Production E2E, Packaging & Release | 8% | In Umsetzung | 89% |
| **Gesamt** | **100%** | **In Umsetzung** | **98%** |

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

Status: In Umsetzung, 99%.

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
- [x] Desktop blockiert ctox.dev-managed Sperrstatus fail-closed:
  `launchAllowed:false` wird als `needs_auth` normalisiert und vor dem
  Launch-Token-Request abgewiesen, statt eine stale/entzogene Instanz zu
  aktivieren.
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
- [x] Live Desktop-Session-Rotation gegen echte ctox.dev-Session: Logout
  entfernt ctox.dev-Storage und Domaincookies, `session-package` wird 401,
  die managed Instanzliste ist leer, ein alter Launch-Token-Pfad blockiert,
  Re-Login bringt die Tenants zurueck und Relaunch bleibt WebRTC-only.
- [x] Opt-in Live-Smoke-Vertrag fuer echten ctox.dev Access-Revocation-Proof:
  Mit separatem launchfaehigem Nicht-Owner-Mitglied kann der Smoke in zwei
  Electron-Sessions Admin- und Mitgliedssicht trennen, das Mitglied temporaer
  auf `viewer` setzen, danach entweder `needs_auth` oder eine verschwundene
  Tenant-Sichtbarkeit plus blockierten Launch pruefen und die urspruengliche
  Rolle wiederherstellen.
- [ ] Live Access Revocation gegen echte ctox.dev-Tenant-Mitgliedschaft.

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
- [x] Unit-/SourceManager-Test: `launchAllowed:false` aus dem ctox.dev
  Session-Package erzeugt eine nicht launchbare managed Instanz und verhindert
  den `/api/desktop/launch-token` Request.
- [x] Live-Recheck 2026-06-15:
  `smoke:ctox-dev-live -- --auth-window --manage-first --launch-first
  --expected-tenant Kunstmen --expected-tenant SKF` ist gegen Produktion gruen.
  Evidenz: sechs Tenants, Kunstmen/SKF vorhanden, Management-Link HTTP 200 ohne
  Login-Redirect, Kunstmen-Launch `transport=webrtc` und
  `httpBridgeAvailable=false`.
- [x] Live-Session-Rotation 2026-06-15:
  `smoke:ctox-dev-live -- --auth-window --manage-first --launch-first
  --session-rotation --expected-tenant Kunstmen --expected-tenant SKF` ist
  gegen Produktion gruen. Evidenz: erster AuthPanel-Login sieht sechs Tenants,
  Logout entfernt ein ctox.dev-Cookie, `/api/desktop/session-package` liefert
  danach 401, managed Count ist 0, alter Launch blockiert mit
  `ctox.dev launch token failed: 400`, Re-Login sieht wieder sechs Tenants und
  Relaunch fuer Kunstmen bleibt `transport=webrtc` /
  `httpBridgeAvailable=false`.
- [x] `smoke:ctox-dev-live -- --access-revocation
  --access-revocation-tenant <tenant> --access-revocation-member-email
  <member-email>` ist als opt-in Produktions-Smoke verdrahtet. Im
  Revocation-Modus erwartet stdin zwei Zeilen: Owner/Admin-Passwort, dann
  Mitglieds-Passwort. Der Smoke nutzt getrennte Electron-Sessions, prueft
  vorab WebRTC-only Launch fuer das Mitglied, setzt dessen Rolle temporaer auf
  `viewer`, akzeptiert danach entweder `needs_auth` mit lokalem
  Launch-Blocker oder eine nicht mehr gelistete Instanz mit serverseitiger
  Launch-Token-Verweigerung, und stellt die urspruengliche Rolle wieder her.
- [x] Desktop-JS-Test: Der Live-Revocation-Contract akzeptiert beide
  Produktionsreaktionen nach ctox.dev Access-Entzug, `needs_auth` und
  `removed`, lehnt aber weiter launchfaehige oder nur netzwerkfehlerhafte
  Zustaende ab.
- [ ] Live-Entzug einer Mitgliedschaft entfernt oder sperrt die Instanz.

## Welle 3: Local Daemon Source

Status: In Umsetzung, 97%.

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
- [x] Lokaler Packaged-App-Smoke baut eine unpacked macOS `.app`, prueft den
  gebuendelten Helper im tatsaechlichen App-Resource-Pfad und fuehrt
  Installation, Inspect, Attach, Neustart und WebRTC-only Launch aus einem
  frischen Desktop-Profil ueber genau diesen verpackten Helper aus.
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
- [x] `npm run pack:dir:bundled-runtime-smoke`: Baut lokal eine unpacked macOS
  `.app` mit temporaerem gebuendeltem CTOX-Helper, validiert
  `Contents/Resources/ctox/ctox` als ausfuehrbare Datei und nutzt den
  verpackten Helper aus einem frischen Desktop-Profil fuer Install, Inspect,
  Attach, App-Neustart und WebRTC-only Launch ohne Registry-Secret-Leak.
- [x] `main`-/PR-CI fuehrt `npm run pack:dir:bundled-runtime-smoke` im
  macOS-Desktop-E2E-Job aus; `npm run release:check` erzwingt diesen
  Workflow-Vertrag.
- [x] Live-CI-Nachweis fuer diesen Gate: GitHub-Actions-Run `27517440788`
  fuer Commit `a182ea06`, Job `Business OS Desktop E2E (mac)`, Step
  `Packaged bundled helper smoke`, ist gruen.

## Welle 4: Pairing Invite Source

Status: Abgeschlossen.

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
- [x] Fallback-freier Live-Recheck gegen SKF `57.129.123.108` nach
  Fresh-Install des Linux-x64-CLI-Artefakts aus Release-Run `27495671970`:
  `smoke:pairing-ssh-live -- --rotate --revoke-local` authentifiziert per
  Passwort-SSH, bezieht initiales und rotiertes Invite aus dem echten
  `ctox business-os desktop invite --format json` Pfad
  (`inviteSource=desktop-invite-cli`, `remoteInviteCliAvailable=true`),
  importiert und rotiert die Pairing-Instanz, sieht geaendertes `sync_room`
  und Room Secret, startet weiter WebRTC-only (`transport=webrtc`,
  `httpBridgeAvailable=false`) und leakt weder Registry- noch Evidence-
  Secrets.
- [x] `cargo test native_peer_ -- --nocapture`: Native-Peer-Tests beweisen, dass
  eine unveraenderte Sync-Konfiguration nicht respawnt und eine
  `peer rotate`-Room-Aenderung als Respawn-Grund erkannt wird.
- [x] RxDB WebRTC Soak `27503869533`:
  `rollover-native-peer-browser-to-rust` auf Commit `6e352b7f` mit
  `SOAK_FAIL_ON_RETRY=1` ist retry-frei gruen. Versuch 1 repliziert
  `replicated_id=browser_rollover_smoke_1781452549731`, sieht fuer
  `desktop_files` und `desktop_file_chunks` jeweils `peerCount=1` /
  `forkPeerCount=1`, schreibt `checkpoint_epoch_count=11` und meldet
  `browser_error_count=0`, `browser_request_failure_count=0` sowie
  `browser_asset_response_error_count=0`.

Noch offen:

- Keine offenen Welle-4-Punkte. Der alte negative Rollover-Run `27501450384`
  bleibt als Historie im Changelog dokumentiert; der retry-freie Nachweis ist
  Run `27503869533`.

## Welle 5: SSH/Sudo Remote Install Source

Status: Abgeschlossen.

Aufgaben:

- [x] SSH Host-Key-Fingerprint-Trust-Flow.
- [x] Key/Agent-basierter Attach ohne Passwortargumente.
- [x] Remote Preflight für OS, systemd, sudo, `ctox`.
- [x] Fresh Ubuntu Install auf Contract-Ebene mit Stable-Release-Bundle:
  Linux/bash/curl/systemd/sudo Preflight, GitHub-Release-Download,
  `.sha256`-Pruefung, user-local Binary-Install nach `~/.local/bin/ctox` und
  anschliessendem `peer ensure`.
- [x] Stable-Fresh-Install kann API-backed Parameter seed-en: `apiProvider`,
  `model` und `backend` werden validiert; Provider/Modell landen als
  SQLite-`runtime_env_kv` in `runtime/ctox.sqlite3` und
  `runtime/ctox-runtime.sqlite3`, ohne Source-Build und ohne Runtime-Env-
  Toggle. `backend` bleibt im Stable-Pfad ohne Runtime-Schreibwirkung, weil
  der API-Pfad keine lokale Inferenz startet.
- [x] Dev-/Source-Fresh-Installer kann dieselben API-backed Parameter weiterhin
  als CLI-Flags an `install.sh` durchreichen; dieser Pfad ist Diagnose/Dev, nicht
  der Production-Stable-Pfad.
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
- [x] Online-Stable-Fresh-Install gegen echten VPS: SKF laedt das aktuelle
  GitHub-Release-Bundle online, validiert die SHA256-Datei, installiert das
  enthaltene Linux-x64-Binary user-local, startet CTOX, fuehrt `peer ensure`
  aus und liefert eine WebRTC-only `ssh_managed` Launch-Konfig.
- [x] Stable-Fresh-Install mit API-Seed-Flags gegen echten VPS: SKF laedt das
  Release-Bundle online, seedet `apiProvider=openai` und `model=gpt-5.4` in
  Runtime-SQLite statt `install.sh` zu starten, fuehrt `peer ensure` aus und
  bleibt WebRTC-only. Der fruehere Kunstmen-Source-Build-Timeout ist damit fuer
  den Production-Stable-Pfad umgangen; Dev-/Source-Installer bleibt als
  bekannter langsamer Diagnosepfad bestehen.
- [x] Live-Test des lokalen Artefaktpfads gegen einen echten VPS: SKF
  akzeptiert das verifizierte GitHub-Release-Binary `ctox-linux-x64` aus
  `v0.3.27`, Installation nach `~/.local/bin/ctox`, `ctox start/status`,
  Remote-`peer ensure`, lokale `ssh_managed` Registrierung und WebRTC-only
  Launch-Konfig.

Tests:

- [x] Desktop-JS-Test: Stable-Fresh-SSH-Install-Command ohne Seed-Flags nutzt
  das offizielle GitHub-Release-Bundle, `curl -fsSL`, `sha256sum -c`,
  `tar -xzf`, user-local `target/release/ctox`, optional `ctox start` und
  `ctox status`, aber kein `sshpass`/`sudo -S`.
- [x] Desktop-JS-Test: Stable-Fresh-SSH-Install kann API-backed Seed-Argumente
  ohne Source-Build in `runtime_env_kv` schreiben, validiert untrusted Zeichen
  und lehnt die Kombination mit lokalem Artefaktpfad ab.
- [x] Desktop-JS-Test: Dev-Fresh-SSH-Install gibt API-backed Installer-
  Argumente weiter per `bash -s -- '--api-provider' ...`.
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
  der Source-Installer-Flag-Contract live bestaetigt; dieser Source-Pfad ist
  nicht production-ready, solange der Installer auf kleinen VPS aus Source baut
  oder der Desktop keine laengere/progressfaehige Install-Session fuehrt. Dieser
  Befund ist jetzt auf den Dev-/Source-Pfad begrenzt; Stable nutzt Release-
  Bundle plus SQLite-Seed.
- [x] Online-Stable-Fresh-Install Live-Nachweis SKF-Testhost `57.129.123.108`:
  `smoke:ssh-password-live -- --fresh-install --file-askpass-fallback` laedt
  das aktuelle Release-Bundle online, meldet `install.artifact=release`,
  `releaseChannel=stable`, prueft `ctox start/status`, fuehrt `peer ensure`
  aus und erzeugt eine `ssh_managed` Instanz mit
  `transport=webrtc`, `http_bridge_available=false` und
  `registrySecretLeak=false`. Der Nachweis nutzt den schwaecheren
  `file-askpass-fallback`; der staerkere platform-keychain-backed
  SSH-Passwortpfad ist separat fuer SKF Existing-Attach gruen.
- [x] Online-Stable-Fresh-Install mit API-Seed Live-Nachweis SKF-Testhost
  `57.129.123.108`: `smoke:ssh-password-live -- --fresh-install
  --file-askpass-fallback --install-api-provider openai --install-model gpt-5.4
  --install-backend cpu --no-restart-service` meldet `install.artifact=release`,
  `apiProvider=openai`, `model=gpt-5.4`, `backend=cpu`, fuehrt keinen
  Source-Build aus und erzeugt eine WebRTC-only `ssh_managed` Instanz ohne
  Registry-Secret-Leak.
- [x] Live-Nachweis SKF-Testhost `57.129.123.108` mit lokalem Release-
  Artefaktpfad: `ctox-linux-x64.tar.gz` aus GitHub Release `v0.3.27` wurde
  lokal per SHA256 verifiziert, das enthaltene ELF-x86_64-Binary ueber
  `--local-artifact-path` per `scp` auf den Host geladen, user-local nach
  `~/.local/bin/ctox` installiert, via `ctox start/status` geprueft und danach
  per `peer ensure` angebunden. Smoke-Evidenz: `install.artifact=local`,
  `secretBackend=file-askpass-fallback`, `source=ssh_managed`,
  `transport=webrtc`, `http_bridge_available=false`, kein Registry-Secret-
  Leak. Der staerkere platform-keychain-backed SSH-Passwortpfad fuer SKF bleibt
  separat durch den Existing-Attach-Smoke belegt.
- [x] Zusaetzlicher Live-Nachweis SKF-Testhost `57.129.123.108` mit lokalem
  CLI-Artefakt aus dem gescheiterten Release-Run `27495671970` fuer `v0.3.29`:
  `ctox-linux-x64.tar.gz` wurde lokal per SHA256 verifiziert, extrahiert und
  per `--local-artifact-path` user-local nach `~/.local/bin/ctox` installiert.
  Die Smoke-Evidenz meldet `install.artifact=local`, `source=ssh_managed`,
  `transport=webrtc`, `http_bridge_available=false`,
  `registrySecretLeak=false`; dieser Stand liefert danach den echten
  `business-os desktop invite` CLI-Pfad fuer Welle 4.

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

Status: Abgeschlossen.

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
- [x] Gruener Live-Nachweis aus Linux-/Windows-Zielplattform-Run: `main`-CI-
  Runs `27485327715` und `27486101670` fuehren den Desktop-Keychain-Runtime-
  Smoke auf macOS, Linux und Windows erfolgreich aus; Linux nutzt dabei eine
  echte Secret-Service-Session und Windows den Credential-Manager-Runtime-Pfad.

Tests:

- [x] `npm run release:check` validiert Builder-Konfiguration,
  Runtime-Update-Abhaengigkeit, Desktop-Protokoll, macOS Hardened
  Runtime/Entitlements/Notarize-Hook und HTTPS-Update-Feed.
- [x] Desktop-JS-Test: Auto-Update bleibt in Development inert, deaktiviert
  stille Downloads und redigiert Update-Fehlerlogs.
- [x] `npm run smoke:keychain-runtime` validiert macOS Keychain Set/Get/Delete
  gegen die echte Plattform-Keychain; Linux/Windows werden durch die
  Release-Matrix und das `main`-/PR-Desktop-E2E-Gate ausgefuehrt.

## Welle 8: Production E2E, Packaging & Release

Status: In Umsetzung, 89%.

Release Gates:

- [x] `npm test`
- [x] `npm run check`
- [x] `npm run release:check`
- [x] `npm run smoke:keychain-runtime`
- [x] `npm run pack:dir:smoke` baut lokal eine macOS `.app` und validiert
  Bundle-Metadaten, App-Icon plus `app.asar` Inhalt.
- [x] `npm run pack:dir:bundled-runtime-smoke` baut lokal eine macOS `.app` mit
  App-Resource-Helper und beweist, dass ein frisches Desktop-Profil ohne
  globales CTOX-Binary ueber den verpackten Helper lokal installieren,
  attachen, neu starten und WebRTC-only launchen kann.
- [x] macOS `main`-/PR-CI fuehrt denselben Packaged-Helper-Smoke auf einem
  sauberen GitHub-Runner aus; `release:check` prueft, dass der CI-Gate nicht
  versehentlich entfernt wird.
- [x] Live-CI-Nachweis fuer den macOS Packaged-Helper-Smoke: Run
  `27517440788`, Commit `a182ea06`, Job `Business OS Desktop E2E (mac)`,
  Step `Packaged bundled helper smoke`, erfolgreich.
- [x] Aktueller Desktop-CI-Recheck fuer den gehaerteten Revocation-Contract:
  Run `27518440351`, Commit `ebb0b83a`, Jobs `Business OS Desktop E2E (mac)`,
  `Business OS Desktop E2E (linux)` und `Business OS Desktop E2E (win)`,
  erfolgreich; der macOS-Job enthaelt erneut den erfolgreichen Step
  `Packaged bundled helper smoke`.
- [x] Folge-Recheck nach Plan-Nachtrag: Run `27518721915`, Commit `e5d98bf8`,
  Jobs `Business OS Desktop E2E (mac)`, `Business OS Desktop E2E (linux)`,
  `Business OS Desktop E2E (win)` und `Desktop extra check (linux)`,
  erfolgreich; der macOS-Job enthaelt erneut den erfolgreichen Step
  `Packaged bundled helper smoke`. Die Gesamt-CI war beim letzten Check noch
  in der Linux-x86_64-CLI-Matrix aktiv, ohne roten Job.
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
- [x] Release-Matrix prueft nach `cargo build --locked --release` und vor
  `electron-builder`, dass der frisch gebaute gebuendelte `ctox`-Helper
  `business-os desktop invite --format json` ausfuehren kann und ein
  Electron-kompatibles WebRTC-only Invite mit Desktop-Deep-Link liefert.
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
- [x] `main`-/PR-CI hat ein Business-OS-Desktop-E2E-Gate auf macOS, Linux und
  Windows: Unit-/Syntax-/Release-Checks, Electron-Smokes und Plattform-
  Keychain-Runtime-Smoke laufen in derselben Plattformbreite wie der Release-
  Vorbau, aber ohne Installer-Build.
- [x] Live-/Cross-Platform Desktop Electron E2E auf macOS, Windows und Linux:
  GitHub-Actions-Run `27484687888` fuer Commit `1b1940d5` und Folge-Run
  `27484995659` fuer Commit `01e258b9` sind auf allen drei
  Desktop-Zielplattformen gruen; Run `27485327715` fuer Commit `0e982165`
  und Run `27486101670` fuer Commit `80b11085` bestaetigen denselben Desktop-
  Pfad erneut. Die aktuellen `main`-CI-Runs `27489031650` fuer Commit
  `4dc20c71`, `27492399333` fuer Commit `ea685cbb`, `27493297770` fuer Commit
  `d2dcd21f`, `27493992898` fuer Commit `34558c2f`, `27494101063` fuer Commit
  `af5f87e8`, `27494885618` fuer Commit `baae47d8`, `27495707818` fuer Commit
  `5451dc16`, `27496841760` fuer Commit `427f64a8` und `27501383594` fuer
  Commit `761da15a` sind als Gesamt-CI gruen und bestaetigen Desktop-E2E,
  Plattform-Keychain-Runtime-Smoke, RxDB-only Guards, `cargo check` und
  CLI-Matrix.
- [x] `IoT Engine Soak` ist fuer den aktuellen `main`-Stand gruen:
  GitHub-Actions-Run `27489031659` fuer Commit `4dc20c71` laeuft nach
  gepinntem `esbuild@0.28.0` Install der JS-Modultests erfolgreich durch.
- [x] Keine HTTP-Business-OS-Datenrequests im lokalen Electron-E2E:
  Control-Plane-Status wird erlaubt, verbotene Datenpfade werden durch den
  BrowserView-Guard vor dem lokalen Server abgebrochen.
- [x] Kein Secret in Registry, Logs, Support Bundle oder Crash Report:
  Registry weist secret-artige Felder zurueck, Support-Snapshots redigieren
  Logs, und Crash-Reporter-Extras werden redigiert, geflattet und begrenzt.
- [x] Linux-Release-Metadaten fuer `.deb` sind gesetzt: `homepage`, Autor mit
  E-Mail und `linux.maintainer`; `release:check` erzwingt den Vertrag.
- [x] macOS-Signing-/Notarization-Secrets werden im Release-Workflow vor
  `electron-builder` explizit fail-closed geprueft, damit fehlende Secrets
  nicht mehr als kryptisches Packaging-Problem erscheinen.
- [x] Lokaler Tag-Release-Preflight fuer Secret-Namen:
  `npm run release:secrets:check` prueft per `gh secret list`, ob
  `APPLE_ID`, `APPLE_ID_PASSWORD`, `APPLE_TEAM_ID`,
  `CTOX_BUSINESS_OS_DESKTOP_CSC_LINK` und
  `CTOX_BUSINESS_OS_DESKTOP_CSC_KEY_PASSWORD` als Repo-Secrets vorhanden sind,
  ohne Secret-Werte zu lesen.
- [ ] Live Code signing, notarization und installer smoke aus einem echten
  Tag-Run.

## Plan-Aenderungslog

| Datum | Änderung |
| --- | --- |
| 2026-06-15 | Status-Recheck nach Plan-Nachtrag: `npm run release:secrets:check` bricht weiterhin fail-closed ab, weil `APPLE_ID`, `APPLE_ID_PASSWORD`, `APPLE_TEAM_ID`, `CTOX_BUSINESS_OS_DESKTOP_CSC_LINK` und `CTOX_BUSINESS_OS_DESKTOP_CSC_KEY_PASSWORD` fehlen; `gh secret list --repo metric-space-ai/ctox --json name,updatedAt` liefert weiter `[]`. Der aktuelle CI-Run `27518721915` fuer Commit `e5d98bf8` ist in den Desktop-relevanten Jobs auf macOS/Linux/Windows gruen, inklusive `Packaged bundled helper smoke` auf macOS; die Gesamt-CI war beim Check noch im Linux-x86_64-CLI-Job aktiv. Kein Fortschrittsanstieg: echter Zwei-Account-ctox.dev-Revocation-Beweis und echter signed/notarized Tag-Run bleiben externe Gates. |
| 2026-06-15 | Externen ctox.dev Live-Revocation-Blocker verifiziert und Plan nachgezogen: Neue Nicht-Owner entstehen in ctox.dev Produktion ueber Einladung/Magic-Link oder vorhandene User; bei konfiguriertem Resend liefert `sendInvitationEmail` keinen `previewLink`. Ohne Zugriff auf die Empfaenger-Mailbox oder ein zweites Passwortkonto kann der Zwei-Account-Smoke keinen authentifizierten Mitglieds-Login herstellen. Gleichzeitig ist der neue Desktop-CI-Run `27518440351` fuer Commit `ebb0b83a` in den relevanten Desktop-E2E-Jobs auf macOS/Linux/Windows gruen; macOS fuehrt den `Packaged bundled helper smoke` erneut erfolgreich aus. |
| 2026-06-15 | Welle 2 Live-Revocation-Vertrag gehaertet: Der Zwei-Account-Smoke akzeptiert nach temporaerer Rollenherabstufung auf `viewer` jetzt beide korrekten ctox.dev-Reaktionen aus Desktop-Sicht, entweder `needs_auth` mit lokalem Launch-Blocker oder eine nicht mehr gelistete Instanz mit serverseitiger Launch-Token-Verweigerung. Neu ist `scripts/ctox-dev-live-contract.cjs` mit Unit-Tests; lokal gruen: `npm test` mit 115 Tests und `npm run check`. Der echte Produktionshaken bleibt weiter offen, bis ein separater Nicht-Owner-Testmember mit Passwort verfuegbar ist. |
| 2026-06-15 | Welle 2 Live-Revocation-Harness vorbereitet: `smoke:ctox-dev-live` unterstuetzt jetzt einen opt-in Zwei-Account-Modus mit `--access-revocation`. Der Smoke trennt Owner/Admin- und Zielmitglied in zwei Electron-Sessions, prueft vorab WebRTC-only Launch fuer ein launchfaehiges Nicht-Owner-Mitglied, setzt dieses Mitglied temporaer auf `viewer`, erwartet `needs_auth` plus blockierten Launch vor dem Launch-Token-Request und stellt danach die urspruengliche Rolle wieder her. Lokal gruen: `npm run check`, `node --check scripts/smoke-ctox-dev-live.cjs`, `node --check scripts/fixtures/ctox-dev-live-main.cjs`. Der echte Produktionshaken bleibt offen, bis ein separater Nicht-Owner-Testmember mit Passwort verfuegbar ist. |
| 2026-06-15 | Welle 3/8 macOS-CI-Evidenz fuer Packaged-App-Helper nachgetragen: GitHub-Actions-Run `27517440788` fuer Commit `a182ea06` hat im Job `Business OS Desktop E2E (mac)` den Step `Packaged bundled helper smoke` erfolgreich abgeschlossen. Welle 3 steigt auf 97%, Welle 8 auf 89%; Gesamt bleibt konservativ bei 98%, weil echter signierter/notarisierter Tag-Run und echte ctox.dev Membership-Revocation weiter fehlen. |
| 2026-06-15 | Welle 3/8 CI-Gate fuer Packaged-App-Helper ergaenzt: Der `Business OS Desktop E2E (mac)` Job in `.github/workflows/ci.yml` fuehrt jetzt `npm run pack:dir:bundled-runtime-smoke` aus. `npm run release:check` erzwingt den Workflow-Befehl, damit der unpacked-`.app`-Helper-Nachweis auf einem sauberen macOS-Runner nicht wieder verloren geht. Offen bleibt weiter der signierte/notarisierte Tag-Run mit echten Installer-Artefakten. |
| 2026-06-15 | Welle 3 Packaged-App-Helper-Nachweis ergaenzt: `npm run pack:dir:bundled-runtime-smoke` baut lokal eine unpacked macOS `.app` mit temporaerem gebuendeltem CTOX-Helper, validiert `Contents/Resources/ctox/ctox` als ausfuehrbar und nutzt den verpackten Helper aus einem frischen Desktop-Profil fuer Install, Inspect, Attach, App-Neustart und WebRTC-only Launch ohne Registry-Secret-Leak. Welle 3 steigt auf 96%; production-ready bleibt durch echten signierten/notarisierten Tag-Run und echte ctox.dev Membership-Revocation blockiert. |
| 2026-06-15 | ctox.dev Live-Revocation eingeordnet: Der aktuelle Live-Account ist per read-only Membership-Check auf allen sechs sichtbaren Tenants `owner`. Ein sicherer, reversibler Entzug der eigenen Mitgliedschaft ist mit diesem Account kein valider Test; fuer den Produktionsnachweis braucht es einen separaten Nicht-Owner-Testmember oder eine dedizierte Testinstanz mit administrativ kontrollierbarem Mitglied. |
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
| 2026-06-14 | Welle 8 Main-CI-E2E-Gate ergaenzt: `.github/workflows/ci.yml` fuehrt Business OS Desktop jetzt auf macOS, Linux und Windows aus, jeweils mit `npm test`, `npm run check`, `npm run release:check`, Electron-Smokes und Plattform-Keychain-Runtime-Smoke. Linux nutzt `xvfb-run` fuer Electron und eine echte Secret-Service-Session ueber `dbus-run-session`/`gnome-keyring`; `release:check` verifiziert diesen CI-Vertrag. Welle 8 steigt auf 76%; ein tatsaechlich gruener GitHub-Actions-Run fuer alle drei Plattformen bleibt als Live-Evidenz noch abzuwarten. Gruen lokal: `npm run release:check`, CI-/Release-YAML-Parse. |
| 2026-06-14 | Welle 8 Live-CI-Evidenz nachgezogen: GitHub-Actions-Run `27484687888` fuer Commit `1b1940d5` hat `Business OS Desktop E2E` auf macOS, Linux und Windows gruen abgeschlossen. Windows scheiterte vorher an einem Test-Harness-Problem im Electron Protocol-Smoke: echte `ctox-business-os-desktop://...` Deep-Link-URLs wurden als Prozess-ARGV an Electron uebergeben und Windows/Electron beendete den Fixture vor der Result-Datei. Der Smoke startet Electron jetzt nur noch mit Datei-Pfaden und liest die Deep-Link-Testdaten aus JSON, testet intern aber weiterhin echte Protocol-URLs. Welle 8 steigt auf 82%; offen bleibt der echte signierte/notarisierte Tag-Run mit Installer-/Fresh-Machine-Smoke. Gruen: `npm run test:electron-smoke`, `npm run check`, `node test/electron-protocol-handler-smoke.cjs`, GitHub Actions Desktop E2E mac/linux/win. Die Gesamt-CI bleibt separat von bekannten CTOX-CLI/Rust-Themen abhaengig. |
| 2026-06-14 | Welle 8 Plan-Konsistenz bereinigt: Der Folge-Run `27484995659` fuer Commit `01e258b9` bestaetigt erneut gruenes `Business OS Desktop E2E` auf macOS, Linux und Windows, inklusive Plattform-Keychain-Runtime-Smokes. Der offene Punkt fuer vollstaendiges Cross-Platform-Mixed-Switching wurde aus der Restliste entfernt; die Restliste unterscheidet jetzt klar zwischen gruenem `main`-/PR-CI-E2E und dem weiterhin offenen echten Tag-Release mit signierten/notarisierten Installer-Artefakten. |
| 2026-06-14 | Welle 7 abgeschlossen: Die `main`-CI-Runs `27485327715` fuer Commit `0e982165` und `27486101670` fuer Commit `80b11085` bestaetigen den Desktop-Keychain-Runtime-Smoke auf macOS, Linux und Windows. Damit sind Linux Secret Service und Windows Credential Manager nicht mehr nur Workflow-Vertrag, sondern live im Desktop-E2E bewiesen; der verbleibende Release-Beweis gehoert jetzt nur noch zu Welle 8 Tag-Run/Installer-Artefakten. |
| 2026-06-14 | Welle 5 lokaler Artefaktpfad live gruen: Das GitHub-Release-Artefakt `ctox-linux-x64.tar.gz` aus `v0.3.27` wurde lokal per SHA256 verifiziert, daraus `target/release/ctox` extrahiert und gegen SKF `57.129.123.108` ueber `smoke:ssh-password-live -- --fresh-install --local-artifact-path ... --file-askpass-fallback` ausgefuehrt. Der Smoke installierte das Binary nach `~/.local/bin/ctox`, pruefte `ctox start/status`, fuehrte Remote-`peer ensure` aus und erzeugte eine `ssh_managed` Desktop-Instanz mit WebRTC-only Launch-Konfig ohne Registry-Secret-Leak. Der offizielle Online-Fresh-Install bleibt wegen Source-Build-Dauer offen. |
| 2026-06-14 | Welle 8 CI-Evidenz geschlossen: `main`-CI-Run `27489031650` fuer Commit `4dc20c71` ist vollstaendig gruen, inklusive Business OS Desktop E2E auf macOS/Linux/Windows, Plattform-Keychain-Runtime-Smokes, RxDB-only Guards, CTOX-CLI-Matrix und Harness-Tests. Der zusaetzliche `IoT Engine Soak` `27489031659` ist ebenfalls gruen; der vorherige Soak-Fehler war ein fehlender gepinnter `esbuild@0.28.0` Install fuer die JS-Modultests und wurde im Workflow behoben. Welle 8 steigt auf 86%, Gesamt auf 97%; production-ready bleibt blockiert durch echten Tag-Run mit signierten/notarisierten Installer-Artefakten, echte ctox.dev Revocation und offiziellen Online-SSH-Fresh-Install. |
| 2026-06-14 | Welle 5 Online-Stable-Fresh-Install gruen gemacht: Der Desktop-SSH-Fresh-Install nutzt fuer `stable` ohne API-Seed-Flags jetzt das offizielle GitHub-Release-Bundle statt Source-Installer, validiert die `.sha256`-Datei, extrahiert `target/release/ctox`, installiert user-local nach `~/.local/bin/ctox` und fuehrt danach `ctox start/status` sowie `peer ensure` aus. Der Source-Installer bleibt fuer `dev` und API-backed Installer-Flags erhalten; dessen Kunstmen-Befund bleibt negativ, weil der echte Cargo-Source-Build nach 900s in den Smoke-Timeout laeuft. Live gruen gegen SKF `57.129.123.108`: `smoke:ssh-password-live -- --fresh-install --file-askpass-fallback`, Evidenz `install.artifact=release`, `transport=webrtc`, `http_bridge_available=false`, `registrySecretLeak=false`. Lokal gruen: `node --test src/apps/business-os-desktop/test/ssh-source.test.cjs`, `npm run check`, `npm test` 106/106. Welle 5 steigt auf 99%; Gesamt bleibt 97%. |
| 2026-06-14 | Welle 5 Stable-API-Seed geschlossen: Stable-Fresh-Install nutzt jetzt auch mit `--install-api-provider`/`--install-model` das verifizierte GitHub-Release-Bundle statt `install.sh`, schreibt `CTOX_CHAT_SOURCE=api`, `CTOX_API_PROVIDER`, `CTOX_CHAT_MODEL`, `CTOX_CHAT_MODEL_BASE` und `CTOX_ACTIVE_MODEL` per SQLite in `runtime/ctox.sqlite3` und `runtime/ctox-runtime.sqlite3` und fuehrt danach `peer ensure` WebRTC-only aus. `--install-backend` bleibt im Stable-API-Pfad validiert, aber ohne Runtime-Schreibwirkung; im Dev-/Source-Pfad wird es weiter an `install.sh` durchgereicht. Live gruen gegen SKF `57.129.123.108`: `smoke:ssh-password-live -- --fresh-install --file-askpass-fallback --install-api-provider openai --install-model gpt-5.4 --install-backend cpu --no-restart-service`, Evidenz `install.artifact=release`, `apiProvider=openai`, `model=gpt-5.4`, `backend=cpu`, `transport=webrtc`, `http_bridge_available=false`, `registrySecretLeak=false`. Lokal gruen: `node --test src/apps/business-os-desktop/test/ssh-source.test.cjs`, `npm run check`, `npm test` 107/107, `git diff --check`. Welle 5 ist 100%; Gesamt bleibt 97%. |
| 2026-06-14 | Welle 4/8 Remote-Invite-Blocker eingegrenzt und Release-Gate nachgezogen: Der Live-Smoke gegen SKF ohne `--allow-peer-status-invite` scheitert weiterhin beim echten Remote-Befehl `ctox business-os desktop invite --format json` mit `unknown business-os command desktop`; der veroeffentlichte Stable-Remote-Stand ist also noch zu alt fuer den Zielpfad. Damit derselbe Fehler nicht in einem neuen Desktop-Release landet, fuehrt `.github/workflows/release.yml` nach dem Helper-Build und vor `electron-builder` den frisch gebauten `resources/ctox/ctox`-Helper mit `business-os desktop invite --format json` aus und validiert Invite-Typ, Version, WebRTC-only Marker, Secret-Payload-Marker und Desktop-Deep-Link. Gruen: `npm run release:check`, `node --check scripts/check-release-config.cjs`. Welle 8 steigt auf 87%; Gesamt bleibt 97%. |
| 2026-06-14 | Aktuelle `main`-CI fuer den Stable-API-Seed-Stand ist gruen: GitHub-Actions-Run `27492399333` fuer Commit `ea685cbb` bestaetigt die Gesamt-CI inklusive Business OS Desktop E2E auf macOS/Linux/Windows, Plattform-Keychain-Runtime-Smokes, RxDB-only Guards, `cargo check` und CLI-Matrix. |
| 2026-06-14 | Release-/CI-Risiko eingeordnet: Der fehlgeschlagene Tag-Release `v0.3.28` ist nicht production-ready verwertbar. Der Windows-Release-Job scheiterte auf dem alten Tag an nicht vollstaendig `#[cfg(unix)]`-gekapselten Service-IPC-Symbolen; die aktuellen `main`-CI-Runs `27493297770` fuer Commit `d2dcd21f` und `27493992898` fuer Commit `34558c2f` beweisen dagegen wieder gruene Windows-CLI-Checks sowie gruene Business-OS-Desktop-E2E-Jobs auf macOS, Linux und Windows. Der Linux-arm64-Fehler im alten Tag-Release war ein GitHub-Artefakt-Upload-Timeout nach dem Build. Offen bleibt weiterhin ein neuer echter Tag-Run mit signierten/notarisierten Installer-Artefakten und dem neuen Helper-Invite-Gate. Der erste Versuch von `27493297770` hatte zusaetzlich einen Linux-x64-CLI-Abbruch vor den repo-eigenen Checks im `Swatinem/rust-cache@v2`-Schritt; der `rerun --failed` ist inzwischen gruen abgeschlossen. |
| 2026-06-14 | Aktueller `main`-Stand vollstaendig gruen: CI-Run `27494101063` fuer Commit `af5f87e8` und der Folge-Run `27494885618` fuer Commit `baae47d8` sind beide vollstaendig gruen. Damit sind Business OS Desktop E2E auf macOS/Linux/Windows, Plattform-Keychain-Runtime-Smokes, RxDB-only Guards, Windows-/macOS-/Linux-CLI-Matrix, `cargo check`, Harness-Core-Tests und Spawn-Liveness-Proof fuer den aktuellen Release-Kandidaten belegt. Der naechste Welle-8-Gate-Test ist ein neuer echter Tag-Release, weil `v0.3.28` historisch fehlgeschlagen und `v0.3.27` der letzte verwertbare Release bleibt. |
| 2026-06-14 | Echter Tag-Release `v0.3.29` gestartet und final negativ eingeordnet: Release-Run `27495671970` scheiterte in der Business-OS-Desktop-Matrix. Linux-x64 kam durch Unit-/Syntax-/Release-Checks, Electron-Smokes, Keychain-Smoke, Helper-Build und Helper-Invite-Gate, scheiterte aber beim `.deb`-Build an fehlender `homepage`/Autor-E-Mail/Maintainer-Metadaten. macOS arm64 und x64 kamen ebenfalls bis `Build packaged app`, scheiterten dann an leeren macOS Signing-/Notarization-Secrets (`APPLE_ID`, `APPLE_ID_PASSWORD`, `APPLE_TEAM_ID`, `CSC_LINK`, `CSC_KEY_PASSWORD`), die electron-builder als `not a file` meldete. Business-OS-Desktop Windows-x64, die klassische Desktop-Matrix und die CTOX-CLI-Matrix waren gruen; der GitHub-Release-Job wurde wegen der Desktop-Fehler uebersprungen. |
| 2026-06-14 | Release-Metadaten-/Preflight-Fix nachgezogen: Commit `427f64a8` setzt Business-OS-Desktop `homepage`, Autor mit E-Mail und `linux.maintainer`, und der Release-Workflow prueft macOS Signing-/Notarization-Secrets vor `electron-builder` explizit fail-closed. Lokal gruen: `npm run release:check`, `npm test` 107/107, `npm run check`, `git diff --check`. `main`-CI-Run `27496841760` fuer Commit `427f64a8` ist vollstaendig gruen. Offen bleibt ein neuer echter Tag-Release nach Konfiguration der macOS-Secrets; ohne diese Secrets bleibt Welle 8 nicht production-ready. |
| 2026-06-14 | Welle 4 Remote-Invite-Blocker geschlossen: Das Linux-x64-CLI-Artefakt aus dem fehlgeschlagenen `v0.3.29`-Release-Run `27495671970` wurde lokal per SHA256 verifiziert und gegen SKF ueber den lokalen Artefaktpfad installiert. Danach lief `smoke:pairing-ssh-live -- --rotate --revoke-local` ohne `--allow-peer-status-invite` gruen: initiales und rotiertes Invite kamen aus `ctox business-os desktop invite --format json`, `sync_room` und Room Secret wechselten, Launch blieb `transport=webrtc` / `httpBridgeAvailable=false`, Registry/Evidence blieben secret-frei. Nicht geschlossen ist der Browser-Reconnect-Beweis nach Rotation, weil die Live-Evidenz weiter `activePeerSessionChanged=false` meldet. Welle 4 steigt auf 99%; Gesamt bleibt 97%. |
| 2026-06-14 | Welle 4/8 Verifikationsharness gehaertet: Der lokale RxDB-Rollover-Smoke `SMOKE_MODE=rollover-native-peer-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js` wurde als richtiger lokaler Beweis fuer Browser-Reconnect nach Native-Peer-Rollover identifiziert, scheiterte lokal aber vor der eigentlichen App-Pruefung an einem stillen `ctox`-Smoke-Binary-Build mit schwerem Default-Feature/ggml-CMake-Pfad. `browser_rust_smoke.js` baut das Smoke-Binary jetzt sichtbar mit dem dokumentierten `--no-default-features`-Vertrag und einem konfigurierbaren Timeout (`CTOX_SMOKE_BUILD_TIMEOUT_MS`, Default 30 Minuten), damit kuenftige Rollover-Laeufe nicht mehr ohne Ausgabe haengen. Gruen: `node --check src/core/rxdb/tools/browser_rust_smoke.js`. Kein Fortschrittsanstieg: Der eigentliche Rollover-/Browser-Reconnect-Smoke muss noch gruen laufen. |
| 2026-06-14 | Welle 4 Rollover-Soak auf CI ausgefuehrt und ehrlich negativ eingeordnet: Workflow-Dispatch `27501450384` fuer `rollover-native-peer-browser-to-rust` auf Commit `761da15a` baute das Smoke-Binary auf GitHub Actions und startete den echten Browser/Rust-Rollover-Smoke. Versuch 1 scheiterte nach 68,8s mit `Timed out waiting for open native peer on desktop_files after native peer restart` (`peerCount=0`); Versuch 2 war funktional gruen mit `replicated_id=browser_rollover_smoke_1781447190564`, `replication_directions` fuer `desktop_files`/`desktop_file_chunks` jeweils `peerCount=1`, `checkpoint_epoch_count=11`, `browser_error_count=0`, `browser_request_failure_count=0` und `browser_asset_response_error_count=0`. Der Workflow ist wegen `SOAK_FAIL_ON_RETRY=1` korrekt rot; Welle 4 bleibt bei 99%, bis ein retry-freier Rollover-Run gruen ist oder die Erstversuch-Flakiness behoben ist. Der normale `main`-CI-Run `27501383594` fuer denselben Commit ist vollstaendig gruen. |
| 2026-06-14 | Welle 4 abgeschlossen: Commit `6e352b7f` stabilisiert WebRTC-Restart-Batches nach Native-Peer-Rollover. Der gezielte RxDB-Soak `27503869533` lief mit `SOAK_FAIL_ON_RETRY=1` im Modus `rollover-native-peer-browser-to-rust` im ersten Versuch gruen (`ok=true`, `retryCount=0`, `durationMs=12163`). Die Evidenz zeigt `replicated_id=browser_rollover_smoke_1781452549731`, `replication_directions` fuer `desktop_files` und `desktop_file_chunks` jeweils `peerCount=1`/`forkPeerCount=1`, `checkpoint_epoch_count=11`, `browser_error_count=0`, `browser_request_failure_count=0` und `browser_asset_response_error_count=0`. Der vorherige Zwischenfix `4ca640cd`/Run `27503042581` war noch negativ auf Versuch 1 und bestaetigte damit die Batch-Restart-Ursache. Welle 4 ist 100%; Gesamt bleibt gerundet 97%. |
| 2026-06-15 | Welle 2 ctox.dev Sperrpfad gehaertet: `launchAllowed:false` aus dem ctox.dev Session-Package wird als `needs_auth` normalisiert und der SourceManager verweigert den Launch vor dem `/api/desktop/launch-token` Request. Damit ist der Desktop fail-closed, wenn ctox.dev einen entzogenen oder nicht launchbaren Tenant noch in der Liste meldet. Live-Recheck gegen Produktion ist gruen: `smoke:ctox-dev-live -- --auth-window --manage-first --launch-first --expected-tenant Kunstmen --expected-tenant SKF` automatisiert die echte AuthPanel-UI, sieht sechs Tenants inklusive Kunstmen/SKF, laedt `/dashboard?tenant=<tenant-id>` mit HTTP 200 ohne Login-Redirect und startet Kunstmen WebRTC-only (`transport=webrtc`, `httpBridgeAvailable=false`). Gruen: `node --test test/ctox-dev-source.test.cjs test/source-manager.test.cjs`, `npm run check`, `npm test` 109/109, `npm run release:check`, `npm run test:electron-smoke` nach einem transienten ersten HTTP-Guard-Smoke-Fehler; der isolierte HTTP-Guard-Smoke war direkt gruen. Welle 2 steigt auf 97%; echte serverseitige Live-Revocation/Session-Rotation bleibt offen. |
| 2026-06-15 | Welle 8 Release-Secret-Preflight ergaenzt: `npm run release:secrets:check` prueft lokal per GitHub CLI die benoetigten Repo-Secret-Namen fuer signierte/notarisierte Business-OS-Desktop-Releases, ohne Secret-Werte zu lesen. Aktueller Befund ist negativ: `gh secret list --repo metric-space-ai/ctox --json name,updatedAt` und `gh variable list --repo metric-space-ai/ctox --json name,updatedAt,value` liefern jeweils `[]`; ein neuer Tag-Release wuerde deshalb am macOS-Preflight fuer `APPLE_ID`, `APPLE_ID_PASSWORD`, `APPLE_TEAM_ID`, `CTOX_BUSINESS_OS_DESKTOP_CSC_LINK` und `CTOX_BUSINESS_OS_DESKTOP_CSC_KEY_PASSWORD` scheitern. Welle 8 steigt auf 88%, weil der Preflight reproduzierbar ist; production-ready bleibt bis zur Secret-Konfiguration und einem echten gruenen Tag-Run offen. |
| 2026-06-15 | Welle 2 Live-Session-Rotation geschlossen und echten Logout-Bug beseitigt: Der erste Produktions-Smoke mit `--session-rotation` zeigte, dass `clearStorageData` allein ctox.dev nicht ausloggt; `/api/desktop/session-package` blieb authentifiziert. `clearCtoxDevSession` entfernt jetzt zusaetzlich passende ctox.dev-Domaincookies aus Electron. Der erneute Live-Smoke `smoke:ctox-dev-live -- --auth-window --manage-first --launch-first --session-rotation --expected-tenant Kunstmen --expected-tenant SKF` ist gegen Produktion gruen: initial sechs Tenants, Kunstmen-Management-Link HTTP 200 ohne Login-Redirect, erster Launch WebRTC-only, Logout entfernt ein ctox.dev-Cookie, Session-Package danach 401, managed Count 0, alter Launch blockiert mit `ctox.dev launch token failed: 400`, Re-Login sieht wieder sechs Tenants, Relaunch fuer Kunstmen bleibt `transport=webrtc` / `httpBridgeAvailable=false`. Der Smoke schreibt bei Timeouts jetzt Teil-Evidenz mit letzter Stage. Gruen: `node --test test/ctox-dev-login.test.cjs test/ctox-dev-source.test.cjs test/source-manager.test.cjs`, `npm run check`, Live-Rotations-Smoke. Welle 2 steigt auf 99%, Gesamt auf 98%; offen bleibt echter serverseitiger Membership-Entzug. |
