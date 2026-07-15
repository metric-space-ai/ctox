# CTOX Business OS App Platform Refactoring Plan

Status: Technischer Refactor und automatisierte Abnahmematrix sind zu 100 %
abgeschlossen. Ausschließlich der menschliche Product-/Design-/Security-/
Privacy-Releaseentscheid bleibt `pending-signoff`. Revision 15 ergänzt den verbindlichen Client-only-
App-Vertrag: neue Business-Apps dürfen weder Rust-Änderungen noch einen CTOX-
Recompile benötigen; Schema, Echtzeitsync, Freigaben und deklarative Aktionen
werden zur Laufzeit aus dem App-Paket registriert. Die nachfolgenden Revisionen
dokumentieren die Umsetzung; Revision 74 schließt den technischen Refactor.
Revision 75 ergänzt den kompakten Settings-Vertrag: gekürzte Tab-Texte müssen
ihren vollständigen Namen bei Hover und über den Accessible Name offenlegen.
Revision 16 schließt die installierte interaktive Shell-/Window-/Mobile-
Abnahme für alle 35 Launch Targets und nimmt gleichzeitig das dabei sichtbar
gewordene kumulative Modul-Sync-Budget als neuen Plattformblocker auf.
Revision 17 implementiert die erste technische Schließung dieser beiden
Blocker: referenzgezählte Modul-Sync-Leases mit messbarem Ressourcenbudget
sowie eine additive, vor Cleanup vollständig verifizierte v0→v1-Migration
für Threads und Thread-Nachrichten. Revision 18 rollt Migration und
Ressourcenfix in die lokale Installation aus und beseitigt zusätzlich den
mehrminütigen Projektions-Kaltstart durch zielgebundene dauerhafte Cursor.
Revision 19 schließt den generischen nativen Migrationspfad für
runtime-installierte Apps und migriert den realen Sellify-Bestand.
Revision 20 schließt den vollständigen installierten 35-App-Langlauf, den
Shell-Grid-/Pane-Fehler, die späte Window-Mode-Race-Condition, die sichtbaren
Fenster-/Chat-Zustände sowie den mobilen Threads-Nachweis. Revision 21 schließt
den realen Denial→Threads→Reviewer→Reauthorization-Pfad mit nativer Policy,
persistierten Projektionen und einem reautorisierten Ziel-Command.
Revision 22 repariert den generischen Locked-State einer paketierten App und
schließt den bestehenden Dynamic-Apps-Guard-/Reload-Smoke erneut vollständig.
Revision 23 verschärft das Zielbild von einer lediglich responsiven Business-
App-Sammlung zu einem adaptiven, modernen Client-OS: Tablet und Mobile erhalten
die Übersichtlichkeit, direkten Touch-Ziele und eindeutigen Navigationswege
einer iPad-/iOS-Oberfläche; große Viewports behalten frei bedienbare Desktop-
Fenster. Nach dem einmaligen Shell-/Modul-Laden gelten native wirkende Warm-
Interaktionen als Releasevertrag. Außerdem ist die Modernisierung nicht mehr
mit Archetyp- oder Stichprobenabdeckung abgeschlossen: jede der 34 Core-Apps
wird einzeln gegen denselben visuellen, responsiven, funktionalen und
Performance-Vertrag migriert und interaktiv abgenommen.
Revision 24 beginnt diese Einzelmigration mit einem kanonischen kompakten
Record-Workbench, einem streng knappen Signature-Run-Control und einer
vollständigen 36-Flächen-Performance-Baseline. Revision 25 erweitert die
Einzelmigration um Credentials, CV Print Builder, Invoices, Reports, IoT und
Kalender. Revision 26 ergänzt Documents und behebt den mobilen Shell-Stack für
mehrspaltige Window-Apps. Revision 27 ergänzt Spreadsheets und schärft den
Mobile-Dichtevertrag für Center- und Automation-Panes. Revision 28 ergänzt
Buchhaltung und neutralisiert dort Routine-/Setup-Aktionen zugunsten kompakter
Workbench-Dichte. Revision 29 ergänzt den App Store und entfernt dessen
historische Glass-/Card-Hover-Optik zugunsten einer kompakten Store-Workbench;
Revision 30 ergänzt Matching als kompakte Pipeline-/Workbench-App und behebt
gleichzeitig den Mobile-Sheet-Inset für windowed Apps; der offene Sync-/
Signaling-Peer bleibt davon getrennt ein Runtime-Blocker. Revision 31 beginnt
die Threads-Modernisierung, schließt eine mobile Business-Chat-Regression
statisch und per Chat-Behavior-Harness, repariert lange Desktop-App-Labels im
Source-Build und trennt erstmals explizit den Source-Fix vom installierten
`ctox-real`-Asset-Snapshot auf Port 8765. Revision 32 repariert den lokalen
installierten Asset-/Registry-Drift, härtet den Desktop-Einzelklick und
schließt Threads plus Matching im echten Browser als windowed Shell-Apps
inklusive Resize, Header, Mobile-Sheet und Console-/Request-Gate. Revision 33
macht diesen zuvor manuellen Asset-/Registry-Fix maschinell prüfbar: ein
managed Asset Guard vergleicht Source, persistenten Business-OS-State,
aktuellen Release-App-Root und optional den laufenden HTTP-Host. Revision 34
ergänzt einen Installer-Smoke, der `sync_business_os_shell_assets()` und
`setup_managed_install()` auf einem synthetischen Source-/State-Root prüft:
Shell-Assets werden aktualisiert, Runtime-Apps bleiben erhalten, ausgeschlossene
Entwicklungsverzeichnisse werden nicht publiziert und das neue Release-Layout
setzt `business-os` sowie `runtime` als State-Symlinks.
Revision 35 beseitigt den für Release-/Installer-Nachweise sichtbaren
Voxtral/GGML-Buildblocker im optionalen Runtime-Pfad: der vendored GGML-
CMake-Build wird bei `CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS=1` ohne `GGML_LIB_DIR`
sauber übersprungen und als nicht gelinkter Runtime-Teil markiert. Der volle
`cargo check --bin ctox` ist damit im isolierten Skip-Target grün; die
Business-OS-Asset-, Chat-, Modul- und Live-Window-Gates bleiben grün.
Revision 36 versucht daraufhin den echten lokalen `install.sh --rebuild`-
Release-E2E zweimal. Beide Läufe erreichen den normalen Rust-Release-Build,
werden aber hostseitig beendet (`SIGTERM`/`SIGKILL`), während ein separater
schwerer `ctox`-Release-Testprozess parallel Speicher und CPU bindet. Damit
bleibt der zusammenhängende Full-Install-Releaseclaim offen; der Befund ist
kein erneuter Voxtral/GGML- oder Business-OS-Shell-Fehler.
Revision 37 setzt die sequenzielle App-Modernisierung fort und migriert
Conversations als kompakte Timeline-/Thread-App: keine Glass-Panes, keine
dominanten Side-Stripes, flachere Message-/Fact-Rows, engere Filter und ein
mobiltauglicher Einspalten-Rückfall. Source-Test, Source-Validator und echter
Browser-Window-QA-Lauf gegen den aktuellen Source sind grün.
Revision 38 wiederholt den echten Full-Install-E2E mit
`CARGO_BUILD_JOBS=1`. Der Lauf nutzt weiterhin den optionalen Runtime-Skip und
fällt nicht in GGML/CMake zurück, wird aber erneut während des normalen
Rust-Release-Builds mit `Killed: 9` beendet. Der lokale Releaseclaim bleibt
damit host-/ressourcenbedingt offen; der installierte `current`-Symlink bleibt
auf dem alten Snapshot.
Revision 39 modernisiert Notes als kompakte Knowledge-/Editor-App und behebt
nebenbei einen generischen Window-Manager-Hitbox-Fehler: App-Content mit
eigenem `z-index` konnte die Shell-Resize-Handles überdecken. Die
Resize-Handles liegen jetzt in einer höheren Chrome-Schicht; Notes besteht
danach Source-Test, Source-Validator, Chat-Layout-Guard und echten
Browser-Window-QA-Lauf mit Süd-/Corner-Resize.
Revision 40 modernisiert Knowledge als kompakte Editor-/Document-App: lokale
Panel-/Control-Tokens leiten wieder auf Shell-Radien, Workspace-Shadows sind
neutralisiert, die alten 10px-Row-Radien entfallen und das Bundle-Caret nutzt
keine 5px-Border-Geometrie mehr. Source-Test, Source-Validator und echter
Browser-Window-QA-Lauf mit Source-Assets sind grün.
Revision 41 modernisiert Shiftflow als kompakte Planner-/Workflow-App: lokale
Panel- und Control-Radien leiten wieder auf Shell-Tokens, dekorative Panel-,
Card-, Hover-, Drag- und Pulse-Shadows entfallen, Drag-Zustände nutzen
Outline/Border statt inset-shadow, und der 12px-Inline-Abteilungs-Chip wird
auf den gemeinsamen Control-Radius zurückgeführt. Source-Test, Source-Validator
und Browser-Window-QA sind grün.
Revision 42 modernisiert AppSec/Deployment Audit als kompakte Queue-/Workflow-
App: der letzte selected-row Side-Stripe wird entfernt und durch Border plus
Accent-Tint ersetzt. Der bestehende Contract-Test enthält jetzt einen
Präsentationsguard gegen Glass-Panes, breite Side-Stripes, Literal-Shadows und
große Radien. Source-Test, Source-Validator und Browser-Window-QA sind grün.
Revision 43 modernisiert Browser als kompakte Remote-Browser-/Automation-App:
Tabbar, Toolbar, Adresszeile, Session-Tabs und Statusstrip werden auf den
platzsparenden Shell-Vertrag zurückgeführt, schmale Fenster erhalten einen
eigenen Container-Breakpoint mit horizontal scrollenden Sessions statt
Layoutbruch, und der Browser-Test enthält jetzt einen Presentation-Guard gegen
historische Glass-/Shadow-/Side-Stripe-Patterns. Source-Test, Source-Validator
und Browser-Window-QA sind grün.
Revision 44 nimmt Reports als kompakte Reports-/Rollback-Workbench erneut als
Einzel-App ab: die App war visuell bereits weitgehend auf dem Kit, erhält aber
jetzt einen eigenen Presentation-Guard gegen historische Glass-/Shadow-/Side-
Stripe-Patterns und frische Source-, Validator- und Browser-Window-Evidence.
Revision 45 modernisiert Support als kompakte Drei-Pane-Timeline-App: Queue,
Timeline, Kontext und Composer nutzen dichtere Shell-Abstände und gemeinsame
Control-Radien. Der Support-Test enthält jetzt einen Presentation-Guard und
prüft die 1180-/760-px-Container-Breakpoints. Source-Test, Source-Validator,
Einzel-Browser-QA und ein Support+Reports+Browser-Batch sind grün; ein längerer
7-App-Batch zeigte einen späten Support-Mount-Timeout und bleibt als Langbatch-
Stabilitätssignal getrennt von der visuellen App-Abnahme offen.
Revision 46 nimmt App Store als kompakte Store-/Release-Workbench ab: letzte
lokale 8/10-px-Radien werden auf Shell-Radien zurückgeführt, der Test enthält
einen Presentation-Guard, und GitHub Discovery startet nicht mehr automatisch
beim Mount. Externe Discovery bleibt explizit über den Refresh-Button möglich;
Browser-QA und ein App-Store+Support+Reports+Browser-Batch sind grün.
Revision 47 nimmt Tickets als kompakte Drei-Pane-Command-Bus-Referenz ab:
Ticket-Zeilen und Timeline-/Kontext-Karten nutzen Shell-Control-Radien, der
Test sichert Presentation, Resizer und 1160-/640-px-Container-Breakpoints ab.
Source-Test, Source-Validator, Browser-QA und ein Tickets+App-Store+Support+
Reports+Browser-Batch sind grün.
Revision 48 modernisiert Creator als kompakte App-Erstellungs-Workbench:
Glassmorphism, Panel-Shadows, Glow-Dots und lokale 8/10/16-px-Radien werden
entfernt. Die drei Spalten bleiben resizable, aber mit flachen Shell-Panes,
dichteren Abständen und einem Presentation-Guard. Source-Test, Source-
Validator, Browser-QA und ein Creator+Tickets+App-Store+Support+Reports+
Browser-Batch sind grün.
Revision 49 modernisiert Research als kompakte Quellen-/Research-Workbench:
dekorative Map-/Source-Card-Shadows, Side-Stripes, lokale 8-px-Radien,
Gradient-Grid-Flächen und Inline-Report-Viewer-Styles werden entfernt oder in
klare Klassen überführt. Source-Test, Source-Validator, Browser-QA und ein
Research+Creator+Tickets+App-Store+Support+Browser-Batch sind grün.
Revision 50 modernisiert Outbound als kompakte Sales-/Automation-Workbench:
Workbench- und Overlay-Shadows, lokale Mailserver-Inline-Flächen, harte
Schwarz-Kontraste und die letzten dekorativen Großradien/Gradient-Patterns
werden entfernt. Source-Test, Audience-Test, Source-Validator, Browser-QA und
ein Outbound+Research+Creator+Tickets+App-Store+Browser-Batch sind grün.
Revision 51 modernisiert Coding Agents als kompakte Provider-/Run-Workbench:
alte Mesh-/Glow-Hintergründe, Glass-Panes, Logo-Gradients, Status-Shadows,
Terminal-Inset-Shadows und statische harte Schwarz/Weiß-Flächen werden entfernt.
Source-Test, Source-Validator, Browser-QA und ein
Coding-Agents+Outbound+Research+Creator+Tickets+Browser-Batch sind grün.
Revision 52 modernisiert CTOX als kompakte Control-Plane-/Run-Workbench:
App-/Canvas-/Timeline-Gradients, Pulse-/Status-/Range-Shadows, lokale
Großradien, breite Chevron-Borders und SVG-Icon-Gradients werden entfernt.
Source-Test, Source-Validator, Browser-QA und ein
CTOX+Coding-Agents+Outbound+Research+Creator+Tickets-Batch sind grün. Damit
sind 34/34 Core-Apps einzeln visuell migriert und browserseitig abgenommen.
Revision 53 schließt den zuvor hostseitig blockierten Release-Build-Schritt
teilweise: `install.sh --rebuild` und der reale Managed-Install-Pfad bauen mit
`CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS=1` und `CARGO_BUILD_JOBS=1` bis zur neuen
Release `v0.3.31-381-g4d1cc9e32-dirty`; `current` und Manifest zeigen auf
diese Release, und der Managed-Asset-Guard gegen `http://127.0.0.1:8765` ist
grün. Der Release-Lifecycle-Claim bleibt aber offen: der Installer hängt im
finalen `ctox-real update channel set-github`, mehrere `ctox-real`-Aufrufe
landen lokal im unkillbaren `UE`-Zustand, der aktive Service lief zunächst noch
mit altem realen CWD, und nach launchd-Kickstart ist kein sauberer
Business-OS-/RxDB-Peer-Start nachgewiesen. Der volle installierte Browser-QA
öffnete, resizte und schloss 35/35 Apps erfolgreich, fiel aber ausschließlich
wegen `ws://127.0.0.1:20876` `ERR_CONNECTION_REFUSED` durch. Als Sofortfix
läuft der optionale Update-Channel-Finalizer im Installer jetzt mit Soft-
Timeout, damit ein wedged lokaler Runtime-Prozess den erfolgreichen Managed-
Install nicht mehr dauerhaft offen hält.
Revision 54 härtet zusätzlich den `business-os rxdb status`-Diagnosepfad:
wenn der native Peer-Lock gehalten wird, aber keine frische Heartbeat-Datei
existiert, überspringt der Statusbefehl Turn-/Command-Plane-SQLite-Probes und
meldet stattdessen einen expliziten `diagnostic_skipped`-Grund. Die
Command-Plane-Statusabfrage nutzt zudem nur noch einen kurzen Busy-Timeout.
Damit kann der Statuspfad den wedged Zustand nicht weiter verschärfen.
Revision 55 trennt den eigentlichen Browser/Rust/WebRTC-Codepfad vom kaputten
installierten launchd-Hostzustand: ein isolierter `command-browser-to-rust`-
Smoke auf eigenen Ports akzeptiert einen Browser-Command und materialisiert den
zugehörigen Queue-Task ohne Browser-Fehler. Dieser Nachweis nutzt das neueste
vorhandene Debug-Binary, weil ein aktueller Source-Build im separaten Target
reproduzierbar still bei `ctox-core` stehen bleibt und kein neues Binary
erzeugt. Der installierte Release-Lifecycle bleibt deshalb weiterhin offen.
Revision 56 präzisiert den installierten launchd-Blocker: der LaunchAgent steht
weiter auf `spawn scheduled`, die letzten Exits laufen unter
`OS_REASON_CODESIGNING`, `spctl --assess --type execute` lehnt sowohl den
Shell-Launcher als auch `ctox-real` ab, und mehrere alte `ctox-real`-Prozesse
hängen weiter in `UE`. Der Release-Blocker ist damit nicht mehr nur „Port
20876 fehlt“, sondern ein lokaler macOS-Spawn-/Gatekeeper-/Provenance- und
Zombie-Prozess-Zustand, der vor dem nächsten installierten E2E aufgelöst werden
muss.
Revision 57 schließt den installierten lokalen Runtime-Pfad für den aktuellen
Host: der macOS-Service-LaunchAgent startet den Shell-Wrapper nicht mehr direkt,
sondern über `/bin/bash`; zusätzlich installiert macOS einen separaten
`com.metric-space.ctox.signaling`-LaunchAgent für den lokalen RxDB/WebRTC-
Signaling-Server auf 20876. Eine atomare Erneuerung der installierten
`ctox-real`-Kopie aus dem funktionierenden Release-Artefakt beseitigte den
lokalen Inode-/Exec-Hänger. Danach laufen 8765, 8788 und 20876 stabil,
`business-os rxdb status --json` kehrt schnell zurück, ein kleiner installierter
Browser-QA öffnet `ctox`, `threads` und `tickets`, der volle installierte
Window-QA öffnet/resizt/schließt 35/35 Apps ohne Console-Fehler, der
Native-Peer-Status meldet nach Browser-Verbindung `replicationUp=true`, und der
Managed-Asset-Guard ist grün.
Revision 58 schließt den veralteten Signature-Resttrack und erneuert die
34-App-Fachtests sowie Release-/Rollback-/Audience-Evidence. Revision 59 rollt
die neue Installer-/LaunchAgent-Logik automatisch als Managed-Upgrade aus und
schließt den installierten Release-E2E mit 35/35 Apps. Revision 60 belegt für
alle fünf kanonischen Runtime-App-Archetypen echte UI-Mutation, Reload, native
RxDB-Projektion und recordbezogene Command-Bus-Automation. Revision 61 repariert
Source-/Modify-Aktionen für windowed App-Taskbar-Einträge und erneuert Rollen-,
Auth-, Tenant-Scope- und Fresh-Profile-Evidence. Revision 62 schließt Coding
Agents mit echter Codex-Session, Follow-up und Eventprojektion. Revision 63
schließt Outbound als vollständige Approval-/Send-/Reply-/Meeting-Kette.
Revision 64 erneuert Thread-Rechtsklick, 10.000er-Skalierung und Restore/Resync.
Revision 65 schließt den sichtbaren und auditierten Agent-Scope einschließlich
App-Store-Kontextmenü erneut dynamisch.
Revision 66 erneuert die Office-/Finance-Fachtests und den 24-App-Runtime-
Rechte-/Reload-Guard gegen den aktuellen Source.
Revision 67 schließt Spreadsheets mit echter UI-Erstellung, Version, Blob,
Reopen-Resume und nativer Projektion.
Revision 68 schließt Documents mit echtem Markdown-Import, Version, Blob,
Reopen-Resume und nativer Projektion.
Revision 69 schließt Invoices mit echter UI-Erstellung, Typed Command Bus,
serverautoritativem Record, sofortiger RxDB/WebRTC-Projektion, Reopen-Resume und
nativer Projektion. Der generische Projektor verarbeitet dafür nur Collections
mit verändertem Projektions-Clock; der Command-Consumer projiziert geänderte
Fachrecords nach Command-Abschluss, ohne Browser- oder HTTP-Fallback.
Revision 70 schließt Buchhaltung mit einer sichtbaren manuellen Journalbuchung,
zwei ausgeglichenen Buchungszeilen, Reopen-Resume und nativer Projektion. Der
App-Mount markiert nur den aktuellen Root als bereit und bindet die sichtbare
Aktion an dessen DOM-Satz; das Browsergate verfolgt Shell-Remounts und gibt die
Aktion erst nach sichtbar geladenem Kontenrahmen frei.
Revision 71 schließt den Client-only-Lifecycle-Track mit einem kanonischen
35-App-Langlauf in einem frischen Browserprofil. App-Kontexte können Sync-
Bridges nicht mehr dauerhaft pinnen, Command-Core-Collections verwenden
scoped Leases und spät aufgelöste App-Leases werden nach Unmount verworfen.
Revision 72 schließt die erneut sichtbar gewordene Shell- und Command-Bus-
Integrität gegen den aktuellen Source. Die Shell belegt exakt den Browser-
Viewport; der redundante untere App-Umschalter ist entfernt und nur die obere
App-Leiste bleibt autoritativ. Alle 34 Core-Apps werden statisch gegen den
registrierten Command- und Harness-Vertrag und alle 35 Launch-Targets real bei
1600×1000 sowie 390×844 geprüft. Direkte App-Writes in `business_commands` und
private Fallback-Intents sind auf null reduziert.

Revision 73 nimmt die zu optimistische Abschlussbewertung aus Revision 72
zurück. Reale Browserbedienung hat drei harte Presentation-Regressionen
offengelegt: Der expandierte Chat reservierte seitlich beziehungsweise unten
Arbeitsfläche und verschob normale Fenster, ein schnelles Loslassen nach dem
Drag verlor das letzte Pointer-Frame und verhinderte freies Verschieben sowie
Links-/Rechts-Snap, und AppSec Pentest blieb in kleinen Fenstern eine starre
Desktop-Zweispaltenansicht. Die Shell behandelt den Chat nun ausschließlich
als nutzergesteuertes Bottom-Overlay; normale Fenster werden dadurch niemals
reflowt. Der Window Manager verarbeitet das finale Drag-Frame synchron und
prüft Links-, Rechts-, Top-Snap und freie Positionierung mit echten
Mausbewegungen. AppSec Pentest verwendet unter 980 px eine explizite
Tests-/Workbench-Navigation statt abgeschnittener oder leerer Spalten. Mobile
belegt App-Inhalt und Chat ohne doppelte 76-px-Reserve. Der Source-/Versions-
Lifecycle bleibt dagegen ausdrücklich offen: Die Headeraktionen müssen als
integrierte, appbezogene IDE- und Versionsansicht mit Dateibaum, aktivem App-
Kontext, Versionswechsel, Diff, Release und Rollback fertiggestellt werden.

Revision 74 schließt diese technischen Restpunkte. Source und Versionen sind
keine losgelösten Desktop-Apps oder Informationsdrawer mehr, sondern zwei
Zustände desselben laufenden App-Fensters. Der Source-Modus zeigt ausschließlich
den Dateibaum und Monaco-Editor der aktiven App; der Versionsmodus zeigt
Timeline, Metadaten, Release und Rollback mit typisierten Shell-Commands. Der
App-Zustand bleibt beim Wechsel erhalten. App Store und IoT umgehen den
globalen Kontextpfad nicht mehr: Record, Feld, Pane, Window und Pointer werden
zentral als `context_v2` erfasst; fehlende Rechte laufen weiterhin über die
persistierte Threads-Freigabe. Der korrigierte Stand besteht den frischen
35-App-Lauf bei 1600×1000 und 390×844 vollständig, die 34-App-Storymatrix, den
vollständigen Command-/Harness-Vertrag, Design-/Accessibility-/Starter-Gates,
Chat-/Window-Komposition und die RxDB-Prozessgrenze mit gebautem Rust-Wire-
Daemon. Damit ist die technische Refactoring-Umsetzung zu 100 % abgeschlossen.
Security/Privacy bleibt als menschlicher Releaseentscheid ausdrücklich
`pending-signoff`; diese Freigabe ist kein offener Implementierungspunkt und
wird nicht automatisiert vorgetäuscht.

Stand: 2026-07-15

Fortschrittsindikator nach Revision 74: Die technische Refactoring-Umsetzung
und die automatisierte Abnahmematrix liegen bei 100 %. Die neue
verbindliche Einzel-App-Modernisierung steht nominell
bei 34/34 vollständig visuell migrierten Apps. Der in Revision 24 noch offene
Zwischenstand der vier Signature-Automation-Flächen wurde durch die
Einzelmigrationen von Creator, Research, Outbound und Coding Agents in Revision
48 bis 51 geschlossen und durch den aktuellen 35-App-Lauf bei Desktop- und
Mobile-Breite erneut regressionsgeprüft. Source-/Versionswerkzeuge,
Rechtsklick-/Delegationspfad und die korrigierte Shell sind Bestandteil dieser
Abnahme. Menschliche Product-/Security-/Privacy-Freigaben werden separat als
Releaseentscheidung geführt und nicht in die technische Prozentzahl
eingerechnet. Revision 32 zählt
Threads erstmals als vollständig interaktiv migriert, weil der installierte
Windowed-Lauf gegen Port 8765 grün ist. Revision 33 erhöht die Quote nicht,
sondern härtet die Release-Evidence gegen erneuten Asset-Snapshot-Drift.
Revision 34 erhöht die Quote ebenfalls nicht, schließt aber den fehlenden
funktionalen Installer-Smoke für die Business-OS-Asset-Sync-Mechanik. Revision
35 erhöht die App-Zählung nicht, beseitigt aber den optionalen Voxtral/GGML-
Buildblocker für Skip-/Installer-Gates und macht den vollen `ctox`-Check wieder
als Release-Evidence nutzbar. Revision 36 erhöht die Quote nicht, grenzt aber
den verbleibenden Full-Install-E2E auf Host-Ressourcen/konkurrierende Release-
Builds statt auf Business-OS- oder Voxtral-Code ein. Revision 37 erhöht die
App-Zählung um Conversations und liefert dafür wieder App-spezifische Source-,
Validator- und Browser-Evidence; der Full-Install-E2E bleibt davon getrennt
offen. Revision 38 ändert die App-Zählung nicht, bestätigt aber den Full-
Install-Blocker auch bei `CARGO_BUILD_JOBS=1` als lokalen Release-Build-/Host-
Ressourcenbefund. Revision 39 erhöht die App-Zählung um Notes und schließt eine
reale Window-Resize-Hitbox-Klasse, die durch App-interne `z-index`-Panes
ausgelöst wurde. Revision 40 erhöht die App-Zählung um Knowledge und liefert
dafür wieder Source-, Validator- und Browser-Evidence gegen den aktuellen
Source. Revision 41 erhöht die App-Zählung um Shiftflow und belegt die Planner-
Variante inklusive Source-, Validator- und Browser-Evidence. Revision 42 erhöht
die App-Zählung um AppSec/Deployment Audit und belegt die Queue-/Workflow-
Variante inklusive Source-, Validator- und Browser-Evidence. Revision 43 erhöht
die App-Zählung um Browser und belegt den Remote-Browser-/Automation-Archetyp
inklusive Source-, Validator- und Browser-Evidence. Revision 44 erhöht die App-
Zählung um Reports und belegt die Reports-/Rollback-Workbench inklusive
Source-, Validator- und Browser-Evidence. Revision 45 erhöht die App-Zählung um
Support und belegt die Drei-Pane-Timeline inklusive Source-, Validator- und
Browser-Evidence; der lange 7-App-Batch-Mount-Timeout bleibt als separates
Stabilitätssignal offen. Revision 46 erhöht die App-Zählung um App Store und
belegt die Store-/Release-Workbench inklusive Source-, Validator- und Browser-
Evidence; die externe GitHub Discovery ist nun explizit statt Mount-seitig.
Revision 47 erhöht die App-Zählung um Tickets und belegt die Command-Bus-
Referenz inklusive Source-, Validator- und Browser-Evidence. Revision 48 erhöht
die App-Zählung um Creator und belegt die App-Erstellungs-Workbench inklusive
Source-, Validator- und Browser-Evidence. Revision 49 erhöht die App-Zählung um
Research und belegt die Quellen-/Research-Workbench inklusive Source-,
Validator- und Browser-Evidence. Revision 50 erhöht die App-Zählung um
Outbound und belegt die Sales-/Automation-Workbench inklusive Source-,
Validator-, Audience- und Browser-Evidence. Revision 51 erhöht die App-Zählung
um Coding Agents und belegt die Provider-/Run-Workbench inklusive Source-,
Validator- und Browser-Evidence. Revision 52 erhöht die App-Zählung um CTOX
und belegt die Control-Plane-/Run-Workbench inklusive Source-, Validator- und
Browser-Evidence. Revision 53 erhöht die Prozentzahl nicht: sie schließt den
reinen Release-Build-/Asset-Drift-Blocker, öffnet aber den härteren
Runtime-Lifecycle-Befund als Release-Gate (`ctox-real`-UE-Hänger, alter
Service-CWD, kein Port 20876, kein stabiler Status-/Peer-Nachweis). Revision 54
erhöht die Prozentzahl ebenfalls nicht; sie macht den Status-/Diagnosepfad
fail-fast und testbar, ersetzt aber keinen installierten Service-/Peer-
Nachweis. Der
Runtime-App-Track wird nicht rückwirkend als erledigt gezählt. Revision 14 nahm
den zuvor zu hoch bewerteten
34+2-Browsernachweis zurueck: API-gesteuertes Resize und Root-Overflow beweisen
weder echte Mausbedienung noch Pane-Nutzbarkeit, Mehrfachchat-Komposition,
vollstaendige Window-Header oder Mobile-Shell.
Revision 15 verankert diese Regeln zusaetzlich im Impeccable-Projektkontext,
im kanonischen App-Development-Skill und im separaten CTOX-Deploy-Skill; die
Installed-Abnahme ist in Revision 16 abgeschlossen; Revision 17–19 schließen
Lifecycle, statische und dynamische Schema-Migration sowie den
Projektions-Neustartpfad. Client-only-Runtime-Actions, dynamische
Fachstory-Tiefe und menschliche Signoffs bleiben offen.

Scope: Business OS Shell, Presentation Runtime, App SDK, Design System,
Context Actions, Rechte, Delegation, Creator, Templates, Store, Release und
Migration aller Apps

## 0B. Produktinvariante: Business OS fühlt sich wie ein echtes adaptives OS an

Business OS ist keine lose Sammlung responsiver Webseiten. Shell, Fenster,
Navigation und Apps bilden ein zusammenhängendes Client-Betriebssystem mit zwei
Darstellungsmodi derselben laufenden App-Instanz:

- **Mobile/Tablet:** übersichtlich wie iPadOS/iOS, mit unmittelbarem Single-
  Click/Single-Tap, klarer Hierarchie, mindestens 44 × 44 px großen effektiven
  Touch-Zielen, Safe-Area-Unterstützung und sichtbarem Vor-/Zurückweg zwischen
  Listen-, Arbeits- und Detail-Pane. Auf Mobil gibt es keine winzigen Desktop-
  Fenster und keine versteckten Kernaktionen hinter Hover.
- **Desktop:** dieselben Apps laufen in verschiebbaren, minimierbaren,
  maximierbaren und mit echter Maus frei resizbaren Fenstern. Window-Chrome,
  Taskbar, Startmenü, Chat-Dock, Snap und Pane-Resizer folgen einem gemeinsamen
  Z-Order-, Fokus- und Occlusion-Modell.
- **Kompakte Fachoberfläche:** wiederkehrende Aktionen wie Import, Suche,
  Filter, Sortierung, Editieren, Export und Ansichtswechsel verwenden kleine,
  benannte Standardklassen. Nur die fachlich einzigartige KI-Automation einer
  App darf als kontrastreicher Hero-/Run-Control sichtbar Raum beanspruchen.
- **Eine adaptive Instanz:** Window, maximized, Tablet und Mobile remounten die
  App nicht unnötig und verlieren weder Selection, Scrollposition, Draft noch
  laufenden Command. Layoutwechsel sind Container- und Zustandswechsel, keine
  zweite App-Implementierung.

### 0B.1 Snappiness-Vertrag

Ein einmaliger Bootstrap darf Shell, App-Katalog, Designsystem und für die
aktuelle App benötigte Daten laden. Danach muss sich die Oberfläche wie eine
lokale Client-App anfühlen:

- sichtbare Reaktion auf Tap, Click, Fokus, Pane-Wechsel und lokale Filterung
  p95 ≤ 100 ms;
- Warm-Mount einer bereits geladenen App p95 ≤ 500 ms; wiederholtes Öffnen darf
  unveränderte JS-/CSS-/Icon-Ressourcen nicht erneut übertragen;
- keine synchrone Netzwerk- oder Datenbankrunde im unmittelbaren Feedbackpfad;
  optimistische lokale Zustände werden bei serverautoritativen Entscheidungen
  eindeutig als ausstehend markiert;
- lange Automationen starten als persistierter Command/Run und blockieren nicht
  den Main Thread; Fortschritt, Abbruch, Resume und Ergebnis bleiben sichtbar;
- Modul-Subscriptions, Observer, Timer und Event-Handler werden je Fenster-
  Lifecycle geleast und nach Close wieder freigegeben;
- Layoutwechsel dürfen keinen vermeidbaren Full-DOM-Rebuild oder Layout-
  Thrashing erzeugen. Performance-Gates erfassen Warm-Mount, Interaction p95,
  Long Tasks, Layout Shifts, doppelte Asset-Requests und Restressourcen nach
  Close.

### 0B.2 Verbindliche 34-App-Migration

Jede Core-App erhält eine eigene Zeile in der Evidence-Matrix und wird erst als
migriert gezählt, wenn reale Browser-Evidence für folgende Punkte vorliegt:

1. gemeinsame kompakte Controls und genau bestimmte Signature-/Hero-Aktion,
2. Light und Dark ohne app-eigene konkurrierende Palette,
3. Desktop Window, frei bedienbares Resize, Maximize/Restore und Close,
4. Tablet-/Mobile-Navigation mit vollständigem Pane-Rückweg und Touch-Zielen,
5. fachliche Hauptstory einschließlich realer erlaubter und – wo relevant –
   verweigerter/delegierter Mutation,
6. Selection/Draft/Persistenz nach Layoutwechsel, Reload und Resume,
7. Tastatur, Fokus, Screenreader-Namen, Reduced Motion und Fehlerzustände,
8. Warm-Mount-/Interaktionsbudget sowie fehlerfreie Console und Requests.

Ein globales CSS-Overlay, ein statischer Regex-Check oder das bloße Öffnen der
App erfüllt diese Migration nicht. Gemeinsame Probleme werden zuerst im Kit
behoben; anschließend wird jede App einzeln angepasst, visuell geprüft und mit
ihren eigenen Fachtests belegt.

## 0A. Produktinvariante: Apps sind Client-Pakete

Das Zielmodell entspricht einer nativen Desktop-App-Plattform: Eine neue App
wird als installierbares HTML-/CSS-/ESM-Paket entwickelt und gegen eine bereits
laufende CTOX-Instanz installiert. Der Entwickler beschreibt Datenmodell,
Darstellung, Sharing und optionale Aktionen deklarativ. Persistenz, Offline-
Journal, Recovery, WebRTC-Sync, Reconnect, Multi-Tab und Multiuser-Verteilung
werden von Business OS bereitgestellt.

Nicht verhandelbar:

- Eine reine Daten-App benötigt keine Änderung unter `src/core/`, keine neue
  Rust-Match-Arm, keine Wire-Fixture und keinen `cargo build`.
- Neue Collections aus `collections.schema.json` werden beim Installieren oder
  Aktualisieren einer App automatisch validiert und im laufenden Datenpfad
  registriert. Ein interner, supervisierter Peer-Reconcile ist zulässig; ein
  manueller Daemon-Neustart oder Recompile ist es nicht.
- Echtzeitsync ist für App-Collections standardmäßig aktiv. „Standardmäßig“
  bedeutet nicht „für jeden sichtbar“: Workspace-, Rollen- und Record-ACLs
  bestimmen weiterhin serverautoritativ, welche Nutzer replizieren dürfen.
- Normales CRUD läuft direkt über die shellgelieferten Collection-Handles.
  App-Code baut keinen HTTP-Endpunkt, keinen eigenen WebRTC-Peer und keine
  zweite Persistenzschicht.
- App-spezifische Aktionen dürfen als Manifest-/Action-Definition zur Laufzeit
  hinzukommen. Der native Router interpretiert geprüfte generische Primitive;
  er erhält nicht für jeden neuen Action-Namen kompilierten Rust-Code.
- Unbekannte Browser-Payloads sind niemals frei ausführbare Backend-Programme.
  Der Runtime-Interpreter akzeptiert nur registrierte Input-Schemas, erlaubte
  Collection-Operationen, explizite Permissions, Idempotency Keys und
  gegebenenfalls deklarative Saga-/Compensation-Schritte.
- Hostzugriff, externe Side Effects oder Spezialprotokolle bleiben optionale
  Erweiterungen. Sie müssen über vorhandene Capability-Adapter oder zukünftig
  sandboxed, zur Laufzeit installierbare Extensions laufen – nicht über einen
  stillen Vertrauenssprung vom Browser und nicht zwingend über einen CTOX-
  Binary-Recompile.

Der Default-Entwicklerpfad lautet damit:

```text
module.json + collections.schema.json + index.html/js/css
                         │
                         ▼
       Business OS installiert und validiert das Paket
                         │
        ┌────────────────┼────────────────┐
        ▼                ▼                ▼
  IndexedDB/WAL     WebRTC/SQLite    ACL/Multiuser
        │                │                │
        └────────────────┴────────────────┘
             reaktive App-Collections
```

### 0A.1 Runtime Data Contract

`module.json` erhält beziehungsweise normalisiert einen deklarativen
`data_runtime`-Block. Die exakte Fixture wird im Implementierungstrack
festgelegt; semantisch muss er mindestens abbilden:

- `sync: "realtime" | "local"`, Default `realtime`;
- `scope: "workspace" | "actor" | "record"`, mit sicherem Draft-/Release-
  Default;
- Collection-Read/Write-Absicht und Grant-Vorschlag;
- Konfliktstrategie, Retention und optionale Demand-Loading-Hinweise;
- deklarative Actions mit Input-Schema, Policy, Idempotenz und zulässigen
  Effekten.

Die Schema-Datei bleibt die Quelle für Records, Indizes und deklarative
Migrationen. Browser und Native lesen dasselbe kanonische JSON; App-Entwickler
pflegen keine zweite Rust-Repräsentation.

### 0A.2 Drei Action-Stufen

1. **Client CRUD (Default):** `insert`, `incrementalUpsert`, `patch`, Delete
   und reaktive Queries auf app-eigenen Collections. Automatisch offline- und
   realtimefähig.
2. **Deklarative Runtime Action:** serverautoritativ geprüfte Mutation,
   Bulk-Operation oder registrierte Saga aus sicheren generischen Primitiven.
   Die Definition wird mit der App installiert; kein Recompile.
3. **Privilegierte Extension:** externer Dienst, Hostzugriff oder spezialisierte
   native Fähigkeit. Explizite Capability, Review und Sandbox/Adapter nötig;
   niemals der Standard für eine normale Business-App.

### 0A.3 Abnahmekriterium

Auf einem bereits gestarteten Release-Binary wird eine bislang unbekannte App
mit neuer Collection und neuem deklarativem Action-Namen installiert. Ohne
Source-Änderung, Backend-Build oder Entwickler-Neustart müssen zwei echte,
verschieden berechtigte Browserprofile Folgendes beweisen:

1. App öffnet und Schema wird automatisch registriert.
2. Profil A schreibt offline; der Write ist journalisiert und lokal sichtbar.
3. Nach Reconnect erreicht er native SQLite und Profil B in Echtzeit.
4. Ein Nutzer ohne Grant erhält weder Pull noch Push-Zugriff.
5. Die deklarative Action ist idempotent, auditiert und projiziert ihren Status.
6. Reload, App-Update und Leaderwechsel erhalten Daten und Checkpoints.
7. Kein geändertes Rust-Artefakt und kein während des Tests ausgeführter
   `cargo build` sind Teil des Installationspfads.

## 0. Verifizierter Implementierungs- und Rollout-Status

Der Plattform-Refactor ist im gemeinsamen Worktree weitgehend implementiert,
aber noch nicht produktionsreif. Die frühere Revision 3 leitete den technischen
Abschluss aus Source-Gates und einem Browserlauf gegen Workspace-Assets ab;
Revision 4 widerlegte das durch einen fehlschlagenden installierten Langlauf.
Revision 5 isolierte QA-Lifecycle-, Mount- und Transportursachen. Revision 6
schloss erstmals den installierten 34-App-Cold-Lifecycle sowie den realen
Rechtsklick→Command-Bus→Native-Store→Queue-Pfad. Revision 7 bestätigt den
finalen ausgelieferten 34-App-Stand und schließt Zwei-Profil- sowie
Edit/Reload/Browser-/Service-Restart-Persistenz. Revision 8 schließt den echten
Nicht-Admin-Denial→Threads→Reviewer→Reauthorization-Pfad, den isolierten
Login/Logout-Gate und einen erneuten installierten 34-App-Lauf. Als technischer
Restblocker blieb die Komposition des persistenten Chat-Docks mit fokussierten
App-Windows. Revision 9 schließt auch diesen Punkt mit einem adaptiven
Bottom-/Side-/Compact-Dock-Vertrag, reproduzierbarem E2E und einem interaktiven
Lauf gegen die installierte V16-Version.
Revision 10 macht zusätzlich den realen Rechtsklick-/Delegationsflow bei je
10.000 historischen Commands, Threads, Messages und Notifications zum
verbindlichen Production-Smoke der Release-Matrix.
Revision 11 prüft die Plananforderungen erstmals gegen ihre jeweils geforderte
Breite. Dabei wurde insbesondere festgestellt, dass ein 34-Oberflächen-
Langlauf zwei Apps der Migrationsmatrix ausgelassen, zwei andere Desktop-
Surfaces eingesetzt und nur Open/Overflow/unbenannte Buttons geprüft hatte.
Auch die vorhandene 12-fache Design-Matrix war ein Design-Lab-, kein per-App-
Gate, und ihr Locale-Schalter übersetzte zunächst keinen sichtbaren Text.

Im Worktree erreicht:

- alle 34 Source-Apps bestehen den offiziellen App-Validator,
- alle 33 normalen Apps besitzen den kanonischen Presentation Contract und
  unterstützen `window`, `maximized` und `focus` bei mindestens 640 × 480,
- `desktop` ist die einzige verbliebene Full-Workspace-/Shell-Surface,
- Registry, eingebetteter Offline-Katalog, Creator, Validator und Skill-Dokus
  schreiben denselben Presentation Contract,
- Window/Maximized/Focus wechseln ohne Remount; Window Hosts liefern echte
  Left/Main/Right-Slots und sind CSS-Container,
- Exact `data.read`/`data.write`, Grant-Seeding, fail-closed Capabilities und
  Actor-Epoch-Revocation sind nativ implementiert,
- SemVer allein veröffentlicht keine App mehr; bestehende SemVer-Sichtbarkeit
  wird in Release-Records migriert,
- Install, Reinstall, Template-Install und Catalog-Update validieren im
  Staging und aktivieren atomar; nicht vertrauenswürdige Third-Party-Bundles
  bleiben bis zu einer Sandbox gesperrt,
- Context v2 erfasst App, Window, Surface, Pane, Collection, Entity, Field,
  Auswahl, Pointer und Deep Link; `ctx.contextActions.register/capture/dispatch`
  ist Teil des gepinnten Mount-Vertrags,
- Rechtsklick, ContextMenu-Taste und Shift+F10 benutzen denselben zentralen
  Renderer und Typed Command Bus,
- verweigerte delegierbare Daten-/App-Aktionen werden als
  `threads.ctox_approval.request` mit Reviewer und vollständigem Kontext
  persistierbar gemacht,
- 14 tote App-Kontextmenüs wurden entfernt; nur das fachliche Desktop-Menü ist
  als Shell-Ausnahme erlaubt,
- Light und Dark verwenden das kompakte, flache Operational-Instrument-System;
  Design Lab und 12-fache Width/Theme/Locale-Matrix sind reproduzierbar,
- der App-Development-Skill ist validiert und über
  `business_os.read_app_skill_resource` remote abrufbar.

Diese Liste beschreibt implementierte Plattformfähigkeiten, nicht mehr
automatisch den vollständigen DoD aller Apps. Eine App gilt erst nach der
app-spezifischen Evidence aus Abschnitt 8 als vollständig migriert.

Automatisierte Evidence:

- Business-OS-JS-Suite: 114 Tests grün,
- Source-App-Validator: 34/34 grün,
- Registry-/Launcher-Smoke: 35 eindeutige Launch Targets,
- Design-Matrix: 12/12 Renderings grün,
- nativer Exact-Collection-Grant-Test grün,
- Browser-E2E gegen Workspace-Assets für Window/Focus ohne Remount, Context v2,
  Typed Ask, Shift+F10 und Denial→Threads-Approval grün,
- gespeicherte Browser- und Design-Artefakte unter
  `output/playwright/business-os-refactor-final-all/` und
  `output/playwright/business-os-design-matrix/`.

Historischer installierter Revision-4-Baseline-Test vom 2026-07-10
(durch Revision 6 überholt):

- `ctox-real` wurde aus dem Worktree als Release gebaut, lokal installiert und
  über den realen Service auf `http://127.0.0.1:8765/` getestet.
- Der native Peer erreicht nach zwei Transportkorrekturen
  `replicationUp=true`: Signaling bevorzugt IPv4 mit IPv6-Fallback und der
  Default-ICE-Kandidat veröffentlicht eine routbare lokale Adresse statt
  `0.0.0.0`.
- Der strikte installierte QA-Lauf öffnete 32 von 34 Core-Apps; `outbound` und
  `buchhaltung` überschritten jeweils das 25-Sekunden-Zeitbudget.
- 19 App-Surfaces erzeugten Console-/Page-Error-Gruppen; insgesamt wurden 484
  Events erfasst. Dominant sind Timeouts für `masterChangesSince` und
  `masterWrite`, Multi-Tab-Query-Owner-Timeouts sowie geschlossene
  `BroadcastChannel`-Instanzen.
- Alle 34 nummerierten Screenshots wurden erzeugt. Der isolierte App-Creator
  öffnet in ungefähr 14 Sekunden; sein Langlauf-Fehler ist damit ein
  Lifecycle-/Transportproblem und kein reiner Creator-Mount-Defekt.
- Rechtsklick und Shift+F10 öffnen auf einem Notes-Record dasselbe zentrale
  Menü. Der v2-Payload enthält korrekte App-, Window-, Pane-, Record-, Pointer-
  und Deep-Link-Daten (`record_id = notes_seed_ops_review`); Collection und
  Field Path bleiben leer.
- Der dabei erzeugte `business_os.context.ask`-Command war im Browser-Dispatcher
  sichtbar, ließ sich über den nativen MCP-Zugriff auf `business_commands` aber
  nicht nachweisen. Damit ist Command-Persistenz/Delivery im installierten
  System nicht bestanden.
- Ein zweiter paralleler Browser kann trotz aufgebauter RTC-Verbindung in
  `masterChangesSince`-Timeouts laufen. Single-Browser-Erfolg ist deshalb kein
  ausreichendes Production Gate.

Evidence:

- `output/playwright/business-os-local-installed-20260710-final/business-os-qa-baseline.md`
- `output/playwright/business-os-local-installed-20260710-final/business-os-qa-baseline.json`
- Screenshots `01-ctox.png` bis `34-creator.png` im selben Verzeichnis

Fortschritt der Revision 5 am 2026-07-10:

- Der zuvor beobachtete Renderer-Runaway war zu einem wesentlichen Teil ein
  Fehler im QA-Lifecycle selbst: Der Runner rief die nicht vorhandene
  `windowManager.close`-Methode auf. Dadurch blieben alle Fenster, Mounts,
  Subscriptions und Demand-Queries des gesamten 34-App-Laufs aktiv. Der Runner
  verwendet jetzt `destroy`, wartet auf das tatsächliche `window:closed` und
  räumt auch nach fehlgeschlagenen Starts auf. Ein Node-seitiger Watchdog
  beendet einen wirklich blockierten Renderer reproduzierbar.
- Der Demand-Transport wiederholt serverseitiges `RATE_LIMITED` mit begrenztem
  Backoff und frischer Request-ID. Ein transienter ICE-`disconnected`-Zustand
  erhält 30 Sekunden Grace; `failed` und `closed` bleiben sofort terminal.
- Die generische native Business-Record-Projektion schreibt kanonisch
  verglichen in Batches statt jeden Datensatz einzeln und gibt den
  Datenbank-Lock zwischen Seiten frei. Die nächste native Revision reduziert
  die Seite von 250 auf 25 Datensätze und verwendet einen zusammengesetzten
  `(updated_at_ms, record_id)`-Cursor, damit gleiche Zeitstempel weder
  übersprungen noch endlos wiederholt werden.
- Threads-, Documents-, Customers-, Buchhaltung-, Support- und Calendar-Mounts
  rendern die Window-Surface jetzt vor Demand-Abfragen, optionalem Seed und
  Realtime-Setup. Der Explorer rendert ebenfalls sofort, schreibt seine
  Standardordner nur bei semantischen Änderungen und verwendet keine vom
  nativen SQL-Query-Stream nicht unterstützte `$ne`-Abfrage mehr.
- Der strikte installierte Lifecycle-Lauf
  `output/playwright/business-os-local-installed-20260710-lifecycle-v15/`
  erreichte 33/34 Apps ohne Renderer-Hänger. Einzig Explorer scheiterte noch
  an `SQLITE_QUERY_STREAM_UNSUPPORTED`; der anschließende installierte
  Zieltest `business-os-target-explorer-creator-v16/` besteht Explorer und App
  Creator ohne Console-/Page-Fehler.
- Der vollständige RxDB-JS-Lauf besteht mit 59 grünen Tests, 0 Fehlern und 2
  bewusst übersprungenen Cross-Process-Tests; der Modul-Conformance-Validator
  bleibt bei 34/34.

### Fortschritt der Revision 6 am 2026-07-10

- Das native Release-Binary wurde in einem isolierten Cargo-Target gebaut,
  atomar als `ctox-real` installiert und über den realen LaunchAgent gestartet.
  Die aktuell installierte SHA-256 ist
  `4f07ae948a2250ba068175bb4225716e96423a6a6b859f608a98650f4ad880c8`.
- Der strikte installierte Cold-Lifecycle
  `output/playwright/business-os-local-installed-20260710-full-cold-v25/`
  besteht erstmals alle 34 von 34 Apps. Jede App ist nach Öffnen und Schließen
  gesund; es gibt null Console-/Page-Fehler. Die einzige erfasste Console-Zeile
  ist eine nicht-fehlschlagende Lexical/Chrome-Contenteditable-Warnung.
- Der Lauf fand unmittelbar nach einem echten Service-Restart statt. Parallel
  projizierte der native Peer 10.650 Business Records. Der erste aktive Lauf
  sank von zuvor etwa 894 Sekunden auf 145,836 Sekunden; der folgende
  inkrementelle Lauf benötigte 7,091 Sekunden. Die UI blieb dabei bedienbar.
- Conversations, Shiftflow und IoT rendern vor ihrer ersten Datenhydration;
  Creator gibt seine Loading-Surface vor dem inneren Modul-Mount zurück.
  Coding Agents liest beim Mount nur Projektionen und erzeugt keine drei
  impliziten Provider-Status-Commands mehr.
- Statische `docs/`-Assets werden aus dem bereits gebündelten Release-Verzeichnis
  aufgelöst. Der vorherige Notes-Logo-404 liefert jetzt byteidentisch HTTP 200
  mit `Content-Type: image/png`.
- Der reale Rechtsklick auf `Operations Review` erfasst
  `notes_seed_ops_review`, Pane `left`, Windowed-Surface, Pointer `(372,254)`
  und den Deep Link im `business-os-context-v2`-Envelope. Die drei Modi
  `Daten ändern`, `Frage stellen` und `App ändern` tragen getrennte Impact-
  Semantik.
- Context Commands warten nur noch auf einen verbundenen Native-Peer, nicht
  auf den vollständigen Pull von 11.000+ historischen Commands. Die UI erhält
  nach erfolgreichem Native-Push eine lokale Quittung und übergibt die weitere
  Statusverfolgung an Chat/Tracker.
- Finaler E2E-Command:
  `cmd_66d2033b-6527-4a59-9182-a007d9679699`. Die UI quittierte in 1,458
  Sekunden, schloss das Menü und öffnete den Chat. Native Status: `accepted`.
  Queue-Link: `queue:system::19a5c3c28393d0a296703ca2`, Status `queued`,
  Thread `business-os/notes/notes_seed_ops_review`.
- Capability-, Access- und API-Token-Schlüssel werden rekursiv aus Queue-
  Prompt-Previews entfernt. Im finalen Queue-Prompt, Command-Audit-Kontext und
  Communication-Metadaten sind `capability_token` und Secret-Marker jeweils
  nullfach vorhanden; der vollständige autorisierte Command-Datensatz bleibt
  intern erhalten.
- Aktuelle Regressionsevidence: Command Bus 14/14 grün, Redaction-Rust-Test
  1/1 grün, RxDB-JS 61/61 grün,
  Module Conformance 34/34.
- Der spätere finale installierte Lauf
  `output/playwright/business-os-local-installed-20260710-full-final-v32/`
  bestätigt den ausgelieferten Stand nach Context-Command-, Redaction- und
  Sync-Bundle-Änderungen erneut mit 34/34 Apps, durchgehend `OK/OK` nach
  Öffnen/Schließen sowie null Console-/Page-Fehlern.
- Die Collection-Observable-Schicht behandelt einen transient vollen,
  weiterhin hart begrenzten Demand-Query-Queue nun als Backpressure: Initiale
  Snapshots werden mit gekapptem Backoff wiederholt und beim Unmount
  abgebrochen. Der Regressionstest reproduziert zwei `QUERY_QUEUE_LIMIT`-
  Antworten und prüft die anschließende korrekte Initialemission.
- `Credentials` wartet beim Mount nicht mehr auf den Command-Bus-Roundtrip für
  `ctox.secret.list`. Die Window-Surface rendert sofort, Hydrierung läuft im
  Hintergrund und verspätete Ergebnisse werden nach Unmount verworfen. Im
  installierten Zieltest sank die Öffnung von einem reproduzierten
  25-Sekunden-Timeout auf 106 ms.

Evidence:

- `output/playwright/business-os-local-installed-20260710-full-cold-v25/business-os-qa-baseline.md`
- `output/playwright/business-os-local-installed-20260710-full-cold-v25/business-os-qa-baseline.json`
- `output/playwright/business-os-local-installed-20260710-full-final-v32/business-os-qa-baseline.md`
- `output/playwright/business-os-local-installed-20260710-full-final-v32/business-os-qa-baseline.json`
- `output/playwright/business-os-context-e2e-v29-redacted.png`
- `output/playwright/business-os-multi-profile-v33/business-os-multi-profile-evidence.md`
- `output/playwright/business-os-multi-profile-v33/after-service-restart.png`

### Fortschritt der Revision 7 am 2026-07-10

- Der finale strikte installierte Lifecycle-Lauf
  `output/playwright/business-os-local-installed-20260710-full-final-v32/`
  besteht 34/34 Apps auf dem tatsächlich installierten Stand. Alle Apps sind
  nach Öffnen und Schließen gesund; Console- und Page-Errors: null.
- Zwei getrennte Clean Profiles booteten und öffneten Notes gleichzeitig.
  Profil A erstellte einen Record über die sichtbare App-UI; Profil B sah ihn
  nach 6.898 ms. Profil B editierte ihn über den sichtbaren Editor; Profil A
  sah den finalen Stand nach 2.839 ms. Beide Profile blieben fehlerfrei.
- Reload in Profil A (2.387 ms) und ein neu erzeugtes Clean Profile
  (5.031 ms) fanden denselben finalen Stand.
- Nach echtem `ctox stop`/`ctox start` und neuem Service-PID fand ein weiteres
  Clean Profile denselben Record nach 7.398 ms. `status.ok=true`, null
  fehlerhafte oder reconnectende Collections, null Browserfehler.
- Der temporäre Testrecord wurde anschließend als Tombstone synchronisiert;
  die Notes-Unsynced-Zahl fiel von eins auf null und ein weiteres Clean Profile
  bestätigte seine Abwesenheit.

Revision 7 war noch kein vollständig grünes Production Gate, weil echter
Denial→Approval und Login/Logout nicht bestanden waren. Revision 8 schließt
diese beiden technischen Nachweise.

### Fortschritt der Revision 8 am 2026-07-10

- Das Release-Binary mit SHA-256
  `8fe36cfbb3e6bb877c929ef22ad580eba9f60feb1d99fcac997490fba41df2a0`
  wurde atomar als `ctox-real` installiert und mit neuem Service-PID gestartet.
- Ein echter Nicht-Admin-Command auf `notes_seed_ops_review` wurde nativ mit
  `data.write`, `role_or_scope_denied` und `allowed=false` abgelehnt. Der
  Decision-Shape liefert konsistent `requires_approval=true` sowie
  `delegation.available=true` und `threads.ctox_approval.request`.
- Der finale Approval-Zyklus lief mit getrennter Requester-/Reviewer-Identität:
  Request-Command `cmd_7cd69ae2-73d0-4f67-ad9d-d3239f4d414b`, Approval
  `approval_5fc235a9-1232-404f-85e2-0ddc4448e87a`, Decision-Command
  `cmd_e112c7e3-1feb-45f8-9343-002e511886f0`, reautorisierter Ziel-Command
  `cmd_30e06f94-3312-4131-97cb-1ac54be9e36c` und Queue-Task
  `queue:system::5b0411f71bddfc54c95aa020`.
- Threads und Command Bus ziehen in einem Clean Profile keine 10.000+ alten
  Thread-, Notification-, Command- oder Queue-Dokumente mehr vollständig.
  Append-heavy Projektionen bleiben push-fähig und werden über begrenzte
  Demand-Queries geladen. Threads rendert fachliche Daten vor optionaler
  Command-/Task-Anreicherung und revalidiert Approval-Status per Primärschlüssel.
- Der installierte Lauf
  `output/playwright/business-os-local-installed-20260710-full-final-v35/`
  besteht 34/34 Apps, jede mit `OK/OK` und null Console-/Page-Fehlern; Threads
  öffnet in 24 ms. Nach dem finalen V13-Embed-Rebuild, atomarer Installation
  und echtem Service-Restart bestätigt
  `output/playwright/business-os-local-installed-20260710-embedded-final-v37/`
  denselben 34/34-Stand gegen das dauerhaft eingebettete Artefakt.
- Der isolierte Browser/Rust-Smoke `business-os-auth-scope-ui` besteht Login,
  authentifizierten Reload, Logout, gesperrten Reload, Protected-Access-Block,
  Cross-Scope-Storage-Denial und final ausgeloggten Zustand. Browserfehler, 404
  und Request Failures: jeweils null. Für die lokale Auto-Admin-Installation
  bleibt Auth gemäß Gate-Definition `not-applicable`.
- Die temporäre Requester-Session wurde widerrufen, der User deaktiviert und
  sein QA-Grant deaktiviert. Keine Capability- oder Session-Tokens wurden in
  Evidence-Artefakten gespeichert.

Evidence:

- `output/playwright/business-os-approval-v34/business-os-approval-evidence.md`
- `output/playwright/business-os-approval-v34/pending-approval.png`
- `output/playwright/business-os-approval-v34/approved-v13-final.png`
- `output/playwright/business-os-local-installed-20260710-full-final-v35/business-os-qa-baseline.md`
- `output/playwright/business-os-local-installed-20260710-full-final-v35/business-os-qa-baseline.json`
- `output/playwright/business-os-local-installed-20260710-embedded-final-v37/business-os-qa-baseline.md`
- `output/playwright/business-os-local-installed-20260710-embedded-final-v37/business-os-qa-baseline.json`
- `runtime/build/business-os-auth-scope-final-v36.json`

### Fortschritt der Revision 9 am 2026-07-10

> Historischer Nachweis, durch Revision 73 teilweise widerrufen: Side-Dock,
> Dock-only Compact und das Reflow normaler App-Fenster sind kein gültiger
> Zielzustand mehr. Die damaligen grünen Smokes haben genau diese falsche
> Komposition bestätigt und gelten deshalb nicht als aktuelle UX-Evidence.

- Business Chat publiziert einen versionierten Layout-Contract mit realer
  Bounding-Box. `shell-chat-composition.js` leitet daraus gemeinsam mit der
  manifestierten Mindestarbeitsfläche der offenen Windows den Modus ab:
  Bottom-Dock bei ausreichender Höhe, Side-Dock bei ausreichender Breite und
  Dock-only Compact, wenn beides nicht möglich ist.
- Der Window Manager unterstützt vierseitige Insets, reflowt Normal-,
  Maximized- und alle Snap-Zustände, stellt temporär verdrängte Geometrie nach
  Collapse wieder her und bewahrt deklarierte Mindestgrößen. Die Taskbar wird
  bei expandiertem Chat zu einer 44-px-Rail im reservierten rechten Bereich.
- Der Stylesheet-Loader vergleicht jetzt die vollständige URL inklusive
  Cache-Buster. `index.html`, `app.css`, `app.js` und dynamische Shell-Imports
  stehen gemeinsam auf `20260710-chat-dock-composition-v16`.
- `assert-shell-chat-composition.mjs` testet die echten Chat-, Window-Manager-
  und Taskbar-Komponenten in fünf Phasen: Side-Dock, Pointer-Aktion,
  Pointer-/Keyboard-Taskbar, Collapse/Restore, Maximized, Bottom-Snap und
  900×600 Compact. App-/Dock- und Taskbar-/Dock-Occlusion sind null; 640×480
  Mindestfläche bleibt erhalten.
- Die installierte V16-Version wurde auf `http://127.0.0.1:8765/` interaktiv
  geprüft. Das reale Threads-Window liegt links vom Side-Dock; der Button
  `Freigabe anfragen` ist laut `elementsFromPoint` das oberste Pointerziel und
  wurde zusätzlich per koordinatenbasiertem Browser-Pointer aktiviert.
- Die installierte Taskbar minimiert und restauriert per Enter. Bei 900×600
  aktiviert die Shell automatisch Dock-only Compact, hält das CTOX-Window bei
  490 px Höhe und meldet jeweils 0 px² Window-/Dock- und Taskbar-/Dock-Overlap.
- Release-Binary und aktive Managed-Installation tragen SHA-256
  `2d0597d6463731b4ac2e675bd0037ebf97179fc762ff2fef3cff84fff3d7abc5`;
  Source-, Release- und Runtime-Webassets wurden bytegleich verifiziert.

Evidence:

- `output/playwright/shell-chat-composition-2026-07-10T21-01-11-459Z/shell-chat-composition.json`
- `output/playwright/shell-chat-composition-2026-07-10T21-01-11-459Z/shell-chat-composition-expanded.png`
- `output/playwright/business-chat-behavior-2026-07-10T20-52-32-406Z/business-chat-behavior.json`
- installierte interaktive V16-Prüfung im aktuellen Codex-Browserlauf

### Fortschritt der Revision 10 am 2026-07-10

- `business-os-threads-rightclick-ui` und `business-os-threads-scale-ui` sind
  jetzt getrennte Teile der zentralen `businessOsProductionSmokeModes` und
  damit verpflichtende Release-Gates. Registry-Selbsttest und Browser-Runner
  verlangen dieselben Evidence-Keys, Mindestmengen und Zeitbudgets.
- Der isolierte Smoke erzeugt reproduzierbar je 10.000 historische
  `business_commands`, `user_threads`, `user_thread_messages` und
  `user_notifications` im nativen RxDB-Store. Die Threads-App lädt davon nur
  begrenzte Fenster: 200 Thread-Zeilen, 200 zugehörige Messages und 50
  Notifications statt unbounded Initial Pulls.
- Der separat bestandene Matrix-Lauf des Scale-Gates rendert 200 sichtbare
  Thread-Zeilen in 8.163 ms und damit unter dem 30-Sekunden-Budget. Der
  Fixture-Seed benötigte 4.359 ms; alle Browser-/Netzwerkfehlerzähler sind null.
- Der Smoke verwendet den kanonischen Kontextanker
  `data-context-record-id/type/label`. Für Record
  `threads_rightclick_record` wurden Pointer `(283,688)`, Entity, Deep Link
  und Record-ID in `business-os-context-v2`, Payload und Client Context
  nachgewiesen.
- Die drei sichtbaren Modi entsprechen dem Produktvertrag: `Daten ändern`
  und `App ändern` persistieren für den simulierten Benutzer ohne
  Selbst-Ausführungsrecht je einen `threads.ctox_approval.request` mit
  Reviewer; `Frage stellen` persistiert direkt einen
  `business_os.context.ask` über den Typed Command Bus. Daten- und
  App-Delegation verwenden getrennte Kontext-Records, damit fachlich erlaubte
  Pending-Approval-Deduplizierung nicht mit Command-Persistenz verwechselt wird.
- Der gemeinsame Matrixlauf führt beide Gates nacheinander grün aus; der
  Right-click-Slice benötigte 35,249 Sekunden, der Scale-Slice 42,268 Sekunden.
  Die native
  Denial→Threads→Reviewer→Reauthorization-Projektion bleibt bewusst das
  getrennte Gate aus Revision 8; der Right-click-Gate verantwortet UI-Modi,
  Reviewer-Steuerung, exakten Kontext und Command-Persistenz. WebRTC-/
  Advanced-Status blieb gesund; Console Errors, Warnings, 404, Request
  Failures und Cache Repairs: jeweils null.

Evidence des bestandenen isolierten Laufs:

- `business_os_threads_scale_commands=10000`
- `business_os_threads_scale_threads=10000`
- `business_os_threads_scale_messages=10000`
- `business_os_threads_scale_notifications=10000`
- `business_os_threads_scale_visible_thread_rows=200`
- `business_os_threads_scale_first_render_ms=8163`
- `business_os_threads_scale_budget_passed=1`
- `business_os_threads_rightclick_source_context_captured=1`
- `runtime/build/business-os-smoke-matrix-summary.json`

### Fortschritt und Auditkorrektur der Revision 11 am 2026-07-11

Neu belastbar umgesetzt:

- `business-os-app-inventory.mjs` leitet die Core-Matrix aus der kanonischen
  Registry und den tatsächlichen Modulverzeichnissen ab. Der Guard verlangt
  exakt 34 Core-Apps, darunter `desktop` als Shell-Surface und `invoices` als
  normale App. `explorer` und `code-editor` werden zusätzlich und ausdrücklich
  als Compatibility-Surfaces geführt; sie ersetzen keine Core-App mehr.
- Der installierte Parameter-Runner prüft für jede normale App ohne Remount
  `window -> maximized -> focus -> window`, Containerbreiten 640/960/1180,
  Warm-Mount-Zeit, vollständige Ready-Zeit, sichtbare Modusreaktion,
  Runtime-Health und globale Console-/Page-/Request-Fehler.
- Der vollständige Source-Overlay-Lauf gegen die installierte Shell umfasst
  34/34 Core-Apps plus zwei Compatibility-Surfaces. Stand V46: Warm Mount p95
  42 ms, maximale sichtbare Mount-Zeit 44 ms, Interaktion p95 36,5 ms und kein
  Presentation-/Overflow-Fehler. Ein anschließend gefundener Monaco-Warm-
  Reopen-Fehler wurde in V47 behoben. Der abschließende V50-Gesamtlauf ist mit
  Warm Mount p95 69 ms, Interaktion p95 42,1 ms, 34/34 Core-Apps, beiden
  Compatibility-Surfaces sowie null Console Errors, Warnings und Request
  Failures grün.
- `explorer` und `code-editor` verwenden jetzt echte Container Queries statt
  Viewport-Media-Queries. Beide bestehen bei 638 px realer Content-Breite ohne
  horizontalen Root-Overflow.
- `research` blockiert den sichtbaren Mount nicht mehr mit 250+750+1500-ms-
  Empty-Data-Retries. Der initiale Refresh läuft mount-token-gesichert im
  Hintergrund; verspätete Ergebnisse dürfen nach Unmount nicht mehr rendern.
  Gemessener Source-Overlay-Warm-Mount: 40 ms statt 624 bis 3375 ms.
- Der Windowed Host besitzt nun denselben auto-derived Loading-Shadow-Vertrag
  wie der Tab-Pfad. Stale Fetches werden über einen Window-Mount-Token
  verworfen; der Shadow wird nach Erfolg oder Fehler entfernt.
- Die Design-Matrix übersetzt sichtbaren DE-/EN-Text wirklich und wartet den
  stabilen Theme-Endzustand ab. Ein toleranzbasierter Pixel-Diff mit explizitem
  Baseline-Update, Diff-JSON und unveränderlicher Normalprüfung ist vorhanden.
- Ein ausführbarer Accessibility-Contract prüft alle zwölf Design-Zellen auf
  Namen, Focus, Kontrast, Reduced Motion und Overflow. Er fand reale Light-
  Theme-Kontrastfehler der Success-/Warning-Badges; die zentralen semantischen
  Tokens wurden korrigiert. Der aktuelle 12-Zellen-Lauf ist grün.
- CI und Release führen Screenshot-Matrix, Visual Diff und Accessibility-
  Contract nach der Browserinstallation aus und laden die Artefakte hoch.

Zum Zeitpunkt des Revision-11-Audits technisch offen:

1. Die 12-fache Design-Lab-Matrix ist Infrastruktur- und Primitive-Evidence,
   aber noch keine Light/Dark/DE/EN/Compact/Standard/Wide-Evidence für jede
   einzelne App.
2. Der parametrisierte Lifecycle-Runner deckt Presentation, Overflow,
   Performance und Fehlergesundheit ab. Die app-spezifischen DoD-Stories für
   echte Daten, Filter, Sortierung, Editieren, Signature Action, erlaubte und
   verweigerte Fachaktion, Reload und Wiederaufnahme fehlen noch als
   vollständige 34-App-Matrix.
3. Der Template Store enthält weiterhin nur die historischen Templates
   `matching` und `documents`. Die fünf kanonischen Archetypen mit getrennten
   Namespaces, Tests, Context-/Command-/Grant-Beispielen und Zuständen sind
   nicht implementiert.
4. Der Runtime Starter wird weiterhin aus großen Rust-Strings erzeugt;
   Creator, MCP/Prepare-App-Source und Template Store sind deshalb noch nicht
   nachweislich ein einziger kanonischer Starter-/Compilerpfad.
5. Der neue Windowed Loading Shadow benötigt nach dem Source-Guard noch einen
   Browser-/Release-Nachweis auf der neu gebauten Installation.
6. Die menschlichen Product-/Design-/Security-/Privacy-Signoffs und die fünf
   historischen Hypoport-IDs bleiben offen.

Aktuelle ausführbare Evidence:

- `output/playwright/business-os-local-installed-source-overlay-contract-v46/business-os-qa-baseline.json`
- `output/playwright/business-os-local-installed-source-overlay-contract-v50/business-os-qa-baseline.json`
- `output/playwright/business-os-research-source-performance-v41/business-os-qa-baseline.json`
- `output/playwright/business-os-code-editor-reopen-v47/business-os-qa-baseline.json`
- `output/playwright/business-os-design-matrix/design-matrix.json`
- `output/playwright/business-os-design-diff/visual-diff.json`
- `output/playwright/business-os-accessibility-contract.json`

### Fortschritt der Revision 12 am 2026-07-11

Der in Revision 11 geöffnete Starter-/Archetypen-Track ist geschlossen:

- Eine versionierte Source-of-Truth liegt unter
  `src/apps/business-os/app-starter/v2/`. Die früheren großen, unbenutzten
  v1-Rust-String-Templates wurden entfernt; Rust bindet die kanonischen Assets
  ausschließlich mit `include_str!` ein.
- Die fünf Archetypen `record-workbench`, `queue-workflow`,
  `editor-document`, `automation` und `timeline-thread` sind als benannte
  Template-Store-Einträge vorhanden. Jede erzeugte Instanz erhält eine eigene
  Collection-Namespace, eigene App-ID/Entry-Pfade, Schema- und Testartefakte.
- Creator, direkter MCP-Create, `prepare_app_source` und Template Store
  materialisieren denselben v2-Starter. Der frühere MCP-Skip wurde entfernt;
  `prepare_app_source` führt den offiziellen Validator jetzt synchron aus.
- Der historische HTTP-Template-Installer wurde entfernt. HTTP und RxDB-
  Command-Pfad verwenden nun denselben serverseitig autorisierten, gestagten,
  validierten und atomar aktivierten Store-Pfad.
- Der Starter enthält direkte CRUD-, Filter-, Sort-, Import-, Edit-,
  Signature-Action-, Command-Bus- und Context-v2-Beispiele sowie Empty-,
  Error-, Offline- und Permission→Delegation-Zustände. DE/EN, Light/Dark und
  Containerzustände für 640/960/1180 px sind ausführbar gegatet.
- `assert-app-starter.mjs` rendert und validiert alle fünf Artefakte. Der
  native Rust-Test installiert alle fünf Archetypen über das echte Staging und
  beweist zusätzlich getrennte Namespaces zweier Template-Instanzen.
- `assert-app-starter-browser.mjs` prüft 60 Browserzellen
  (5 Archetypen × 3 Breiten × 2 Themes × 2 Sprachen), fünf CRUD-/Command-/
  Context-Flows sowie fünf verweigerte Permission→Delegation-Flows. Das Gate
  läuft als Teil von `qa:design-gates` in CI und Release.
- Die visuelle Browserprüfung fand und behob zusätzlich einen nicht klickbaren
  Permission-Button durch Empty-State-Überlagerung sowie eine fehlerhafte
  vertikale Navigation und eine leere responsive Hauptfläche nach Auswahl.

Neue Evidence:

- `output/playwright/business-os-app-starter-v2/report.json`
- `output/playwright/business-os-app-starter-v2/*.png`
- `business_os::store::tests::canonical_template_archetypes_share_staged_runtime_starter`
- `npm --prefix src/apps/business-os test`
- `npm --prefix src/apps/business-os run qa:design-gates`
- `cargo check --bin ctox --no-default-features`

Nach Revision 12 technisch offen (historischer Stand vor Revision 13):

1. Die per-App-Matrix für Light/Dark, DE/EN, Compact/Standard/Wide und
   Accessibility ist für die 34 bestehenden Apps noch nicht vollständig.
2. Die 34 app-spezifischen Daten-, Filter-, Sortier-, Edit-, Signature-,
   Rechte-/Delegations-, Reload- und Wiederaufnahmestories fehlen noch als
   vollständige Evidence-Matrix.
3. Der neue Windowed Loading Shadow und alle aktuellen Sourceänderungen
   benötigen einen Production-Smoke auf einer neu gebauten Installation.
4. Product-/Design-/Security-/Privacy-Signoffs und die historische Zuordnung
   der fünf Hypoport-Pilot-IDs bleiben offen.

Weiterhin organisatorisch offen:

- Security-/Privacy- und Design-Signoff bleiben organisatorische Freigaben des
  benannten Product Owners. Die Source-Gates sind grün, die
  Signoff-Dokumente bleiben bis zur menschlichen Freigabe auf
  `pending-signoff`; der installierte Production-Gate-Status ist rot.
- Die historische Bezeichnung „fünf Hypoport-Apps“ ist im Repository weiterhin
  nicht auf konkrete IDs abgebildet. Alle 34 Apps besitzen zwar den
  Presentation Contract; ohne IDs kann die explizit verlangte Pilotkohorte
  jedoch nicht als solche gegen ihre vollständige DoD ausgewiesen werden.

### Fortschritt der Revision 13 am 2026-07-11

Revision 13 schliesst die bisher fehlende aktuelle Source-, Fachvertrags- und
Installed-Evidence, ohne die verbleibenden Langlauf- und Signoff-Risiken
wegzudefinieren:

- Der offizielle Source-Validator besteht 34/34 Apps. Fehlende kanonische
  Testeinstiege fuer Matching, Reports, Shiftflow, CTOX, Buchhaltung, Desktop,
  Knowledge und Research wurden ergaenzt.
- `qa/app-quality-contracts.json` beschreibt fuer alle 34 Core-Apps die
  Fachstory-Signale fuer Daten, Filter, Sortierung, Editieren, Signature
  Action, Context/Command, Permission/Delegation, Reload und Resume. Der
  strikte Auditor meldet 34 Apps und null unbelegte statische Signale, null
  Locale-, Container- oder Testluecken.
- Creator bietet die exakt fuenf kanonischen Archetypen explizit an und reicht
  den gewaehlten Archetyp in Manifest, Command-Payload und Client Context
  weiter. CTOX kann abgeschlossene oder blockierte Aufgaben direkt ueber den
  Typed Command Bus als Folgeauftrag fortsetzen; die Aktion haengt nicht von
  einem gemounteten Chat ab.
- Der Source-Browserlauf
  `output/playwright/business-os-source-full-en-dark-v73/` besteht 36/36
  Oberflaechen: 34 Core-Apps plus Files und Source Editor. Geprueft wurden
  Registry/App Store, 640/960/1180 px, Window/Maximized/Focus/Restore,
  Runtime-Health sowie Console- und Request-Fehler.
- Die Design-Gates bestehen die zwoelf Width/Theme/Locale-Zellen, Visual Diff,
  Accessibility, Reduced Motion, Focus, Kontrast und Overflow. Der kanonische
  Starter besteht zusaetzlich 60 Browserzellen und fuenf reale
  Permission-zu-Delegation-Flows.
- AppSec-Listen sind begrenzt und sekundaere Formulare als tastaturbedienbare
  Disclosures ausgefuehrt. Der sichtbare Interaktionswert sank im Zieltest von
  rund 482 ms auf 40,1 ms. Window-Maximize/Restore/Snap animiert keine
  Layout-Eigenschaften mehr ueber `transition: all`.
- Ein Release wurde direkt aus dem aktuellen Worktree mit `install.sh`
  gebaut und erfolgreich zu 100 % in
  `~/.local/lib/ctox/releases/v0.3.31-353-g846392ba0-dirty` aktiviert. Der
  installierte Creator enthaelt nachweislich den aktuellen Archetypenstand;
  der Business-OS-Webserver liefert den installierten Build auf Port 8765.
- Der sichtbare Installed-Smoke deckt alle 36 Oberflaechen in drei sauberen
  Chromium-Profilen ab. Batch A und B bestehen je 12/12. Batch C besteht
  10/12 im gemeinsamen Lauf; die dort durch Daten-/Browserlast verspäteten
  Apps Support und Tickets bestehen unmittelbar danach isoliert jeweils 1/1.
  Alle erfolgreichen Zieltests liegen innerhalb der Warm-Mount- und
  Interaktionsbudgets und melden null Console Errors.
- Zwei Versuche, alle 36 installierten Apps in genau einem sichtbaren
  Browserprozess zu sequenzieren, erzeugten nach CV Print Builder anhaltende
  Chrome-GPU-/Rendererlast. CV Print Builder selbst besteht isoliert
  reproduzierbar mit 39,6 ms Interaktionszeit; Documents besteht ebenfalls.
  Der Befund bleibt deshalb als Langlauf-/Umgebungsrisiko offen und wird nicht
  durch die drei gruenen Teilmatrizen ersetzt.
- Neu gestartete `ctox-real`-CLI-Prozesse blieben auf diesem Rechner bereits in
  macOS `_dyld_start` haengen, waehrend der installierte Service und der
  Business-OS-Webserver weiter gesund antworteten. Das ist kein SQLite- oder
  App-Mount-Stack, verhindert aber einen frischen CLI-Status-/Peer-Nachweis und
  bleibt als Installationsbefund offen.

Aktuelle Evidence:

- `output/playwright/business-os-source-full-en-dark-v73/business-os-qa-baseline.json`
- `output/playwright/business-os-app-quality-audit.json`
- `output/playwright/business-os-app-starter-v2/report.json`
- `output/playwright/business-os-appsec-window-perf-v67/business-os-qa-baseline.json`
- `output/playwright/business-os-installed-visible-batch-a-v76/business-os-qa-baseline.json`
- `output/playwright/business-os-installed-visible-batch-b-v76/business-os-qa-baseline.json`
- `output/playwright/business-os-installed-visible-batch-c-v76/business-os-qa-baseline.json`
- `output/playwright/business-os-installed-support-v76/business-os-qa-baseline.json`
- `output/playwright/business-os-installed-tickets-v76/business-os-qa-baseline.json`

Nach Revision 13 technisch offen:

1. Der installierte 34+2-Lifecycle muss nochmals in einem sauberen
   Ein-Prozess-Browserprofil ohne GPU-/Renderer-Runaway bestehen; der aktuelle
   vollstaendige Nachweis ist app-vollstaendig, aber auf drei Profile plus zwei
   isolierte Wiederholungen verteilt.
2. Die statischen 34-App-Fachvertraege muessen fuer risikoreiche Apps noch
   durch dynamische Stories mit echten Datenmutationen, erlaubter und
   verweigerter Aktion, Persistenz, Reload und Resume vertieft werden. Die
   bereits vorhandenen skalierten Context-/Approval- und Persistenz-E2Es
   bleiben gueltige Plattformnachweise, ersetzen aber nicht jede Fachvariante.
3. Der macOS-Loaderbefund fuer neue `ctox-real`-CLI-Prozesse muss isoliert und
   der frische Status-/Peer-Smoke wiederholbar gemacht werden.
4. Product-, Design-, Security- und Privacy-Signoff sowie die Zuordnung der
   fuenf historischen Hypoport-Pilot-IDs bleiben menschliche beziehungsweise
   produktseitige Restpunkte.

### Revision 14: interaktive Shell-Korrektur am 2026-07-11

Eine sichtbare Abnahme waehrend des installierten Langlaufs hat gezeigt, dass
die bisherigen Presentation-Smokes zu technisch und zu wenig
benutzerorientiert waren. Der Screenshot des realen Zustands zeigte ein fast
vollflaechiges IoT-Fenster, mehrere alte QA-Chatfenster und eine nicht mehr
klar sichtbare beziehungsweise nicht frei nutzbare untere Arbeitsflaeche.
Damit gelten die folgenden Punkte ab sofort als verbindlicher Teil dieses
Plans und nicht als spaetere kosmetische Nacharbeit:

1. **Echtes Window-Handling statt Style-Mutation.** Jede der 35
   fensterfaehigen Oberflaechen wird ueber einen sichtbaren Launcher mit einem
   einzelnen Klick geoeffnet, am unteren und diagonalen Resize-Griff per
   echtem Maus-Drag verkleinert und vergroessert, maximiert, wiederhergestellt,
   minimiert, ueber die Taskbar wiederhergestellt und ueber den sichtbaren
   Close-Button geschlossen. `desktop` wird als Shell-Surface separat geprueft.
2. **Pane-Nutzbarkeit ist ein eigenes Gate.** Fruehere Fullscreen-Apps mit zwei
   oder drei Spalten muessen ihre linke Navigation, Hauptflaeche und rechte
   Kontext-/Detailflaeche in Window, Maximized und Mobile sinnvoll erhalten.
   Ein ausgeblendetes Pane braucht einen sichtbaren Drawer-/Stack- oder
   Zurueckweg. Root-Overflow allein ist keine Evidence.
3. **Chat bleibt Nutzerentscheidung.** Historische oder im Hintergrund
   abgeschlossene Chats duerfen ein vom Nutzer eingeklapptes Dock nicht
   selbststaendig wieder oeffnen und dadurch App-Fenster waehrend eines Drags
   reflowen. Der kompakte Chat-Button bleibt immer sichtbar.
4. **Mehrfachchat skaliert deterministisch.** Ein-/Ausblenden sowie die
   Zustaende mit 1, 6, 8, 12, 100 und 1000 Chats werden real geprueft. Chips
   und Overflow sind tastatur- und mausbedienbar, horizontal scrollbar und
   bleiben im Viewport. Der Chat bleibt ein Bottom-Overlay; inaktive, ehemals
   absolut positionierte Chats duerfen nicht aus dem Viewport ragen und die
   aktive Chat-Leiste muss bei wachsender Chatmenge horizontal scrollen.
5. **Vereinfachte Shell-Navigation.** Oben links bleibt nur das Startmenue-
   Symbol. Die separaten Symbole fuer Desktop und CTOX/Workflow sowie die dort
   obsolete Navigationsgruppe werden entfernt; Desktop und Apps bleiben ueber
   das Startmenue erreichbar.
6. **Single Click ist der Web-App-Standard.** App-Icons des Business-OS-
   Desktops starten mit einem Klick. Ein Doppelklick bleibt nur dort zulaessig,
   wo er fachlich einer Desktop-Dateiinteraktion entspricht, etwa im Explorer.
   Dragging eines Icons darf keinen App-Start ausloesen.
7. **Window-Header traegt den App-Lifecycle.** Direkt hinter App-Icon und
   App-Name stehen Version und Status wie privat, Team/public, Preview,
   modifiziert oder neu. Vor Minimize/Maximize/Close liegen sichtbare,
   berechtigungsgepruefte Aktionen fuer Source Editor und
   Versionen/Lifecycle/Rollback. Diese Aktionen verwenden dieselben nativen
   Permission- und Command-Pfade wie der bisherige Fullscreen-App-Bar.
8. **Responsive Shell mit Mobile-Untergrenze.** Die gesamte Business-OS-Shell,
   nicht nur App-Inhalte, funktioniert von Desktop bis zu einer expliziten
   mobilen Mindestbreite. Unterhalb der Mobile-Untergrenze wird nicht weiter
   verkleinert. Auf Mobile werden Fenster als bedienbare App-Sheets/
   Vollflaechen dargestellt, Startmenue, Headeraktionen, Zurueckweg, Chat und
   Taskbar bleiben erreichbar und Touch-Targets ausreichend gross.
9. **Visuelle Abnahme ist eigenstaendig.** Fuer alle Apps werden Screenshots
   nach dem echten Resize erzeugt. Zusaetzlich werden Initial-Shell, dichter
   Mehrfachchat, stabiler Bottom-Dock, 900-px-Shell und Mobile-Shell als eigene
   Bildartefakte geprueft. Leere, abgeschnittene, verrutschte oder durch
   andere Shell-Flaechen verdeckte Regionen sind harte Fehler.
10. **Ableton-Dichte ist ein Responsive-Vertrag.** Wiederkehrende Bedienung
    wie Import, Suche, Filter, Sortierung, Auswahl, Editieren, Export und
    Pane-Navigation verwendet eine flache, kompakte Operational-Chrome ohne
    generische Hero-Karten oder grossflaechige Buttons. Pro sichtbarem
    Arbeitsbereich darf hoechstens die fachlich einzigartige Automation als
    Hero-/Signature-Action Farbe und zusaetzlichen Raum beanspruchen. Auf
    kleinen Fenstern und Mobile werden Labels kontrolliert gekuerzt oder in
    Tooltips/Overflow ueberfuehrt; Funktion, Status und Rueckweg bleiben
    erhalten.
11. **Desktop-Labels bleiben rasterstabil.** Lange App-Namen belegen hoechstens
    zwei typografisch kontrollierte Zeilen innerhalb einer festen Icon-Zelle.
    Normale Wortgrenzen werden bevorzugt, untrennbare technische Namen duerfen
    kontrolliert umbrechen; danach greift Ellipsis/Clamping und der vollstaendige
    Name bleibt per Tooltip und Accessible Name verfuegbar. Kein Label darf
    Nachbar-Icons verschieben oder in die naechste Rasterzelle ragen.
12. **Creation- und Deploy-Skills bleiben synchron.** Root `PRODUCT.md`,
    `DESIGN.md` und `.impeccable/design.json`, der kanonische
    `business-os-app-module-development`-Skill sowie
    `/Users/michaelwelsch/Documents/ctox-business-os-deploy-skill/ctox/`
    enthalten denselben Operational-Density-, Signature-Automation-,
    Window-/Pane-/Mobile- und Browser-Evidence-Vertrag. Ein alter Deploy-Skill
    darf keine `full-workspace`-App oder generische Dashboard-Shell mehr
    erzeugen.

Bereits durch Revision 14 gefundene Defekte; Chat-Geometrie, Drag-Release und
AppSec-Kompaktlayout wurden in Revision 73 korrigiert, die per-App-
Langlaufabnahme bleibt offen:

- inaktive Mehrfachchat-Fenster behielten beim Wechsel in den Seitendock alte
  absolute X-Positionen und lagen ausserhalb des Viewports,
- historische beziehungsweise neue Hintergrundantworten oeffneten ein
  eingeklapptes Dock erneut und reduzierten waehrend der Arbeit die nutzbare
  Fensterhoehe,
- native Taskbar-Buttons behandelten Enter sowohl im eigenen Keydown-Handler
  als auch im synthetisierten Browser-Click und konnten doppelt toggeln,
- der bisherige 34+2-Runner verwendete fuer Presentation-Checks direkte
  Manager-/Style-Aufrufe und konnte echte Drag-, Launcher- und Controlfehler
  deshalb nicht ausschliessen,
- die 2-/3-Pane-Bedienbarkeit und Mobile-Shell waren nicht Teil des bisherigen
  per-App-Gates.

Revision-14-Evidence wird unter
`output/playwright/business-os-interactive-window-*` und
`output/playwright/shell-chat-composition-*` abgelegt. Der neue Lauf gilt erst
als abgeschlossen, wenn alle 35 Fenster-Apps plus Desktop/Shell gruen sind und
die Screenshots separat visuell geprueft wurden.

### Revision 16: installierte Shell-/Window-Abnahme am 2026-07-11

Revision 16 schliesst den in Revision 14 geforderten realen Bediennachweis,
ohne daraus den Abschluss des gesamten Plattformplans abzuleiten.

Umgesetzt und interaktiv gegen die lokale Installation geprueft:

- Die Shell zeigt oben links nur noch das Startmenue. Desktop-, CTOX-/Workflow-
  sowie Vor-/Zurueck-Symbole sind entfernt.
- Desktop-Apps starten mit Einzelklick; ein beendeter Icon-Drag startet keine
  App.
- Fenster-Header zeigen Name, Version, Lifecycle-/Sichtbarkeitsstatus, Source
  und Versionen vor Minimize, Maximize und Close. Source Editor und
  Lifecycle-Drawer wurden im realen App-Store-Fenster geoeffnet.
- Alle Business-Apps starten standardmaessig als `window`; `maximized` und
  `focus` bleiben explizite Modi. Die letzten Focus-Defaults von Notes,
  Documents, Spreadsheets und CV Print Builder wurden auf Window umgestellt.
- Die native Modulprojektion bewahrt `launch_kind` und `presentation`. Der
  Browser reichert vorhandene autoritative Katalogzeilen uebergangsweise mit
  dem gepackten Presentation Contract an, ohne fehlende tenantgesperrte Apps
  hinzuzufuegen.
- Aeussere Fenster-Resizer und Modul-Pane-Resizer committen den letzten
  Pointerwert auch dann, wenn `pointerup`/`mouseup` vor dem naechsten
  Animation-Frame eintrifft.
- App-Inhalte besitzen einen isolierten Stacking-Context. Selbst sehr hoch
  gestapelte app-interne Overlays wie in Matching koennen Header und aeussere
  Resize-Griffe nicht mehr ueberdecken.
- Ein Modul wird erst nach beendetem Mount als bereit markiert. Dekorative
  Loading-Shadow-Fetches blockieren die App-Bereitschaft nicht mehr und
  Skeleton-SVGs gelten im Browser-Gate nicht als echter App-Inhalt.
- Mobile bei 390 x 844 verwendet ein App-Sheet von y=48 bis y=778; Topbar und
  Chat-Dock bleiben sichtbar. Mehrpane-Apps behalten einen erreichbaren letzten
  Pane-Stack. Das unterstuetzte CSS-Minimum bleibt 360 px.
- Lange Desktop-App-Namen bleiben in festen Icon-Zellen auf maximal zwei
  Zeilen; alle 35 sichtbaren Zellen sowie ein synthetischer langer
  Enterprise-Name liegen innerhalb ihres Rasters.
- Das Impeccable-Preflight, der kanonische App-Development-Skill und der
  separate Deploy-Skill enthalten denselben Ableton-Dichte-, Signature-
  Automation-, Window-/Mobile-/Pane- und Real-Browser-Vertrag.

Installations- und Browserbefund:

- Ein kompletter lokaler Release-Build und das Managed-Install-Layout wurden
  erfolgreich erzeugt. Der optionale Voxtral/GGML-Neubau musste mit dem vom
  Installer vorgesehenen `CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS=1` ausgelassen
  werden, weil ein macOS-CMake-Probeprozess unkillbar in `_dyld_start` hing.
- Das installierte Binary wurde nach `OS_REASON_CODESIGNING` explizit ad-hoc
  neu signiert und mit `codesign --verify --strict` verifiziert.
- Die ausgelieferte Shell wurde direkt geprueft: genau ein
  `data-shell-start`, keine alten Navigationsziele und Build
  `20260711-catalog-parity-v29`.
- Alle 35 Launch Targets bestehen reales Oeffnen, vertikales und diagonales
  Maus-Resize, Minimum, Maximize/Restore, Minimize/Taskbar-Restore und Close.
  Die Abnahme wurde bewusst in frischen, begrenzten Browser-/Peer-Batches
  gespeichert, weil der monolithische 35-App-Stresstest einen separaten
  Ressourcenfehler sichtbar machte.

Gruene Installed-Evidence fuer alle 35 Targets:

- `output/playwright/business-os-interactive-installed-batch-a-v30/`
- `output/playwright/business-os-interactive-installed-batch-b-v30/`
- `output/playwright/business-os-interactive-installed-batch-c-v30/`
- `output/playwright/business-os-interactive-installed-batch-d-v30/`
- `output/playwright/business-os-interactive-installed-batch-e-v31/`
- Matching, Nachweise und Notes aus
  `output/playwright/business-os-interactive-installed-batch-f-v32/`,
  Outbound separat aus
  `output/playwright/business-os-interactive-installed-outbound-v33/`
- `output/playwright/business-os-interactive-installed-batch-g-v34/`
- Spreadsheets, Submissions und Support aus
  `output/playwright/business-os-interactive-installed-batch-h-v34/`, Threads
  separat aus `output/playwright/business-os-interactive-installed-threads-v35/`
- `output/playwright/business-os-interactive-installed-batch-i-v35/`
- regulaerer LaunchAgent-Nachweis nach Wiederherstellung des Dienstbetriebs:
  `output/playwright/business-os-interactive-installed-service-v36/`

Ein maschineller Union-Check ueber diese Reports ergibt 35/35 gruene Apps,
keine fehlende App-ID und elf Mobile-Reports mit gueltiger App-Sheet-Abnahme.

Neuer offener P0/P1-Plattformbefund:

- Nach ungefaehr sieben erstmals aktivierten Apps in einem Browserkontext
  beziehungsweise nach mehreren aufeinanderfolgenden Vierergruppen gegen
  denselben nativen Peer steigen Mount-Zeiten reproduzierbar ueber 30 Sekunden.
  Dieselben Apps bestehen unmittelbar nach Browser-/Peer-Neustart. Das ist
  kein App-Layoutfehler, sondern fehlendes Lifecycle-/Ressourcenbudget fuer
  inaktive Modul-Syncs und Subscriptions.
- Track B1 erhaelt deshalb ein zusaetzliches Gate: 35 Apps werden in einem
  Langzeitprofil sequenziell geoeffnet und geschlossen; aktive Collection-
  Subscriptions, Sync-Interessen, Timer und Modul-Teardowns muessen nach Close
  auf einen definierten Baseline-Korridor zurueckkehren. Der Runner speichert
  pro App Mount-Zeit, aktive Collection-Anzahl, Subscription-/Timer-Zaehler,
  Peer-Loop-Last und Speichertrend. Kein Dienstneustart darf fuer dieses
  Stress-Gate notwendig sein.
- Dieser neue Stress-Befund verhindert weiterhin die Bezeichnung
  `produktionsreif`, hebt aber nicht die gruene Einzel-App-/Shell-Abnahme auf.
- Dieser Thread-Drift-Befund ist durch Revision 17–19 abgelöst: Der installierte
  LaunchAgent migriert und repliziert jetzt 199 Collections. Auch die vier
  runtime-installierten Sellify-Collections sind additiv auf v1 migriert. Der reale
  Denial→Threads→Reviewer→Reauthorization-Pfad muss weiterhin erneut laufen.

### Revision 17: additive Threads-Migration und Modul-Leases am 2026-07-11

Der in Revision 16 diagnostizierte Lifecycle-Fehler ist im Plattformcode nicht
mehr nur beschrieben, sondern technisch adressiert:

- `shared/sync.js` besitzt referenzgezählte Collection- und Modul-Leases.
  Direkte Shell-Verbraucher pinnen ihre Bridges; App-Fenster tun dies nicht.
  Die letzte App-Lease stoppt eine nicht gepinnte Bridge, gemeinsam genutzte
  Collections bleiben bis zur letzten Lease aktiv.
- Window-Close und Fullscreen-Navigation geben die jeweilige Modul-Lease
  symmetrisch frei. Die permanente `syncStartedModules`-Menge ist entfernt.
- Der Runtime-Vertrag veröffentlicht Bridge-, Active-, Pin- und Lease-Zähler
  als unsichtbare DOM-Telemetrie. Der reale v31-Browserlauf zeigte beim Close
  des initialen CTOX-Fensters eine Rückkehr von 15 auf 4 Leases und von 15 auf
  9 Bridges, ohne Reload oder Peer-Neustart.
- Die kanonische Browser-Suite besteht nach der Änderung 68/68 Tests,
  einschließlich Cross-Process-Wire-Smokes, Demand-only-Reconnect und neuem
  `module-sync-lifecycle-smoke.mjs`.

Die Thread-Schema-Drift wird als echte Versionsmigration behandelt:

- `user_threads` und `user_thread_messages` wechseln deklarativ von v0 auf
  v1; Browser und Native verwenden neu generierte, identische Schema-Hashes.
- Der native Peer registriert zuerst die v1-Zieltabelle, kopiert danach in
  einer idempotenten Transaktion alle v0-Storage-Envelopes, behält bei Konflikt
  den neueren `lastWriteTime`-Stand und verifiziert anschließend jeden
  Quellschlüssel.
- Erst nach erfolgreicher Vollständigkeitsprüfung darf der vorhandene
  Stale-Version-Cleanup v0 entfernen. Ein Crash zwischen Meta-Anlage und Copy
  kann deshalb nicht mehr beim Folgestart die einzige vollständige Tabelle
  löschen.
- Der JSON-Migrations-Guard deckte zusätzlich einen leeren deklarativen
  `outbound_messages`-Migrationsblock auf; er ist jetzt mit der vorhandenen
  JavaScript-Migration synchron. Modul-Schema-Generator, gemeinsamer Vertrag,
  Hash-Registry, 34-Modul-Conformance und 34/34-App-Quality-Gate sind grün.

### Revision 18: installierter Rollout und dauerhafte Projektions-Cursor am 2026-07-11

Der native Rollout aus Revision 17 ist lokal installiert und gegen den realen
Bestandsdatensatz verifiziert:

- Vor der Migration wurde die reale RxDB-Datei gesichert und mit
  `PRAGMA integrity_check = ok` geprüft. Die v0→v1-Migration kopierte 22.294
  Storage-Envelopes aus `ctox_queue_tasks`, `user_threads` und
  `user_thread_messages`; erst danach entfernte der Cleanup drei v0-Tabellen
  und neun Trigger.
- Die Zieltabellen enthalten weiterhin 486 Queue-Tasks, 10.896 Threads und
  10.912 Thread-Nachrichten. Der LaunchAgent bringt 195 Collections auf einer
  multiplexierten WebRTC-Verbindung hoch. `replicationUp=false` bedeutet im
  aktuellen Nachweis ausschließlich, dass nach der vom In-App-Browser
  verweigerten Reload-Navigation kein Browser-DataChannel geöffnet ist;
  Pool, Signaling-Join und kritische Tasks sind grün.
- Der erste Ressourcenfix (250 statt 25 Records pro begrenzter Seite und Skip
  nicht registrierter optionaler Collections) beseitigt die fehlerhafte
  Wiederholung an Sellify-DB6, reicht allein bei 1,05 GB historischen
  `business_commands` aber nicht aus.
- Deshalb speichert der Peer den erfolgreich abgearbeiteten
  `since_ms`-Stand jetzt in
  `__ctox_business_record_projection_cursors` innerhalb der Ziel-RxDB. Wird
  die Ziel-RxDB entfernt, verschwinden die Cursor mit ihr und ein vollständiger
  Wiederaufbau bleibt korrekt. Nicht registrierte Collections werden beim
  Laden verworfen; Cursor werden erst nach einem vollständig erfolgreichen
  Projektionslauf fortgeschrieben.
- Auf dem realen Bestand sank der Warmstart der Business-Record-Projektion von
  zuvor maximal 435.960 ms mit mehr als 3,3 GB RSS auf 13.003 ms, 0 Fehler und
  0 unnötige Writes; der Gesamtprozess fiel nach dem Start von rund 1,13 GB
  weiter auf rund 709 MB, statt über 3 GB hinaus weiter anzuwachsen. Der
  unmittelbar davor notwendige Nachzug neuer AppSec-Daten dauerte 283 ms und
  schrieb 74 Records.
- Die Cursor-Roundtrip-/Filter-Regression, sechs Projektionstests, die additive
  Migration, Rustfmt und Diff-Checks sind grün. Der vollständige kanonische
  Browser-RxDB-Lauf bleibt bei 68/68 grün.

### Revision 19: generische Runtime-App-Migration und Sellify am 2026-07-11

Runtime-installierte Apps besitzen jetzt denselben ausführbaren
Schema-Migrationsvertrag auf Browser- und Native-Seite:

- Der native Peer liest `migration_strategies` direkt aus
  `installed-modules/*/collections.schema.json` und
  `local-modules/*/collections.schema.json`. Unterstützt werden
  Identitätsmigrationen sowie dieselben deklarativen Operationen
  `set_from_first_truthy` und `set_boolean` wie im Browser.
- Jede Zwischenversion von 1 bis zur Zielversion muss beschrieben sein. Fehlt
  eine Strategie, ist das nur bei abwesender Quelltabelle zulässig. Existieren
  persistierte Alt-Daten, bricht Bring-up vor dem Cleanup ab; die Quelltabelle
  bleibt vollständig erhalten.
- Der App-Static-Check erzwingt diesen JSON-Vertrag für runtime-installierte
  Apps. Seine Validator-Suite und der separate Sellify-Installed-Validator
  sind grün.
- Sellify 0.4.18 besitzt jetzt den kanonischen `windowed`-/Presentation-Vertrag
  mit 640×480-Untergrenze sowie additive v0→v1-Identitätsmigrationen für
  Activities, Campaigns, Companies und People.
- Vor dem realen Rollout wurde eine 2,0-GB-RxDB-Sicherung mit
  `PRAGMA integrity_check = ok` erstellt. Der installierte Peer migrierte und
  verifizierte 238.913 Envelopes (74.209 Activities, 86.549 Campaigns, 17.516
  Companies, 60.639 People), entfernte erst danach vier v0-Tabellen und zwölf
  Trigger und bringt nun 199 Collections auf der multiplexierten Verbindung
  hoch. Der Live-Store besteht anschließend erneut `integrity_check = ok`.

### Revision 20: installierter 35-App-Langlauf, Shell-Grid und Mobile am 2026-07-11

Die erneut interaktiv geprüfte installierte Shell schließt mehrere zuvor nur
scheinbar grüne Window-/Responsive-Claims:

- Sellify 0.4.19 verwendet seine vier responsiven Breakpoints als
  `@container business-app-window` statt als Viewport-Media-Queries. Bei der
  realen Mindestgröße 640×480 beträgt der Root-Overflow 0 px; nur die
  vorgesehenen Nav-, Tab- und Grid-Regionen scrollen intern. Der vertikale
  Pane-Griff blieb per echtem Maus-Drag bedienbar.
- Recovery-Journal v3 registriert Collections nicht mehr seriell gegen das
  vollständige Journal. Collection-scoped Pending-Indizes und das Überspringen
  bereits primary-committeter Batches reduzierten den installierten Shell-
  Kaltstart vom reproduzierbaren minutenlangen Stillstand auf 7,493 Sekunden
  bis zum vollständigen Launcher-Katalog. Neue lokale Writes bleiben durch das
  Await in jedem mutierenden Collection-Pfad fail-closed.
- Maximieren/Wiederherstellen besitzt jetzt einen sichtbaren und zugänglichen
  Zustand (`is-maximized`, `❐`, korrektes ARIA-Label). Dasselbe gilt für
  Chat-Dock und Chat-Fenster. Der reale Drei-Chat-Dock scrollte von 0 auf
  200 px, der Nachrichtenbereich blieb `overflow:auto`, und der aktive Chat
  skalierte kontrolliert von 320 auf 460 px Höhe.
- Der 2-/3-Pane-Fehler lag in der Shell: `display:none` auf leeren Side-Panes
  ließ den Hauptinhalt per Grid-Autoplacement in Spalte 1 rutschen und eine
  leere Mittelspalte stehen. Explizite Grid-Spalten 1/2/3 stellen den Vertrag
  wieder her. CTOX wechselte damit von 386/834 px auf 834/834 px nutzbaren
  Hauptinhalt; maximiert war das Raster 340+10+892 px und ein echter Drag
  änderte die linke Spalte auf 388 px. Unter dem Container-Breakpoint wechselt
  die App absichtlich in den kompakten Stack.
- Die Presentation-Initialisierung erfolgt nun vor dem asynchronen Modul-Mount.
  Damit kann ein spät beendetes Schema-/Mounting einer langsamen App keinen
  bereits ausgeführten Maximize-/Restore-Klick mehr rückgängig machen; der
  reproduzierbare Consent-Race-Nachtest ist grün.
- Alle 35 kanonischen Launch Targets wurden im selben installierten
  Browserprofil sequenziell per Einzelklick geöffnet, per echtem Maus-Drag
  vertikal resized, maximiert, wiederhergestellt und geschlossen. Ergebnis:
  35/35 grün, ohne Header-Overflow oder zurückbleibendes Fenster.
- Das reale 390×844-Threads-Gate ist grün: 390×730 Mobile-Sheet, 388/388 px
  Shell-Hauptinhalt, kein Dokument- oder Header-Overflow und erreichbare
  Lifecycle-, Source- und Versionsaktionen. Evidence:
  `/tmp/business-os-mobile-threads-v38b/business-os-interactive-window-qa.json`.
- Die kanonische RxDB-Suite besteht nach Recovery- und Shell-Rollout 69/69
  Tests. Chat-Kompositions-Harness und 26 Chat-/Composition-Unit-Tests sind
  ebenfalls grün.

Nach Revision 20 blieben insbesondere der reale
Denial→Threads→Reviewer→Reauthorization-Lauf, dynamische Fachstory-Tiefe, die
benannte Hypoport-Zuordnung sowie menschliche Product-, Design-, Security- und
Privacy-Signoffs offen. Revision 21 schließt den Delegationslauf; die übrigen
Punkte verhindern weiterhin ausdrücklich ein `production ready`.

### Revision 21: realer Rechte-, Rechtsklick- und Delegationspfad am 2026-07-11

Der zuvor nur teilweise beziehungsweise durch einen vorzeitigen Test-Return
scheinbar grüne Delegationspfad ist jetzt als isolierter Browser-/Rust-Smoke
vollständig belegt:

- `threads-requester` ist ein echter Nicht-Admin mit explizitem `data.read`-
  Grant für `notes`, aber ohne Schreibrecht. Der direkte
  `business_os.data.modify`-Versuch endet serverautoritativ mit
  `role_or_scope_denied`.
- Der globale Rechtsklick erfasst App, Record, Deep Link und Pointer; Daten-,
  Frage- und App-Modus gehen über den Typed Command Bus. Daten- und
  App-Änderung persistieren je einen `threads.ctox_approval.request`, die Frage
  persistiert `business_os.context.ask`.
- `threads-reviewer` wird im Picker sichtbar, sieht die native Approval-Karte
  in Threads und genehmigt sie als Admin. Die Approval wechselt auf
  `approved`; der erzeugte `business_os.data.modify`-Ziel-Command ist
  `accepted` und trägt dieselbe `approval_request_id`.
- Der Smoke verwendet echte Capability-Tokens beider Nutzer, wartet auf die
  vollständige Initialreplikation und behandelt Demand-Query-Fenster beim
  Nachweis neuer Projektionen explizit als stale-while-revalidate.
- Ergebnis des isolierten Laufs `/tmp/ctox-rightclick-v38r`: alle Registry-
  Anforderungen grün, Advanced Status v1 gesund, 0 Browser-/WebSocket-/Asset-
  Fehler, 0 Request-Failures und 0 Cache-Reparaturen.

Damit ist der technische Denial→Threads→Reviewer→Reauthorization-Track
geschlossen. Offen bleiben dynamische Fachstory-Tiefe, der macOS-Prozessstart-
Befund, Produktmetadaten und menschliche Signoffs; die Plattform ist weiterhin
nicht als `production ready` freigegeben.

### Revision 22: paketierter Locked-State und Dynamic-Apps-Gate am 2026-07-11

Der nächste dynamische Fachstory-Lauf fand einen realen Darstellungsfehler:
Support fing einen fehlenden `data.read`-Grant erst in seiner asynchronen
Hintergrundinitialisierung ab und blieb dadurch leer, statt den shellweiten
Permission-/Delegationszustand zu zeigen. Support prüft seine erforderlichen
Collections nun synchron beim Mount. Ein fehlender Grant propagiert als
`BusinessOsPermissionError` an die Shell; der normale Admin-Start bleibt
unverändert funktionsfähig.

Der erneute isolierte `business-os-dynamic-apps-ui`-Lauf ist vollständig grün:

- 16 paketierte Apps bestehen Collection-, Property-, Raw-, Context- und
  Cached-Handle-Denial sowie Read-Grant/Write-Denial;
- der Support-Fall zeigt den standardisierten Locked-State;
- 8 System-Apps bestehen ihre scoped-System-Allowlist inklusive Fremd-
  Collection-Denial;
- Open-Module-Reload, Storage-Scope, Runtime-Safety und Lifecycle-/Why-
  Diagnostik sind grün;
- 0 Browserwarnungen, WebSocket-Warnungen, Browserfehler, 404s,
  Request-Failures, Asset-Fehler oder Cache-Reparaturen.

Der v39-Rollout wurde in der lokalen Installation interaktiv nachgeprüft:
Support öffnet als Local-Admin per Einzelklick in einem normalen Fenster,
zeigt keinen falschen Locked-State und lässt sich wieder schließen.

### Revision 24: adaptive OS-Baseline und erste Einzel-App-Welle am 2026-07-11

Die neue adaptive OS-Invariante wurde nicht nur dokumentiert, sondern gegen
den realen App-Bestand instrumentiert und in einer ersten Welle umgesetzt.

Gemeinsame Plattform- und Performance-Evidence:

- Der Browser-/Rust-UI-Regression-Runner öffnet, rendert und bedient nun alle
  34 Core-Module plus Desktop statt der früheren Stichprobe. Der isolierte
  Lauf meldet 34 Module, 35 Desktop-Icons, 0 Browserfehler, 0 Warnungen,
  0 404 und 0 Request-Failures.
- Der App-Container heißt jetzt explizit `business-app-window`; Full-Workspace-
  Inhalte reagieren dadurch auf die reale Fenster-/Dockbreite statt auf den
  Viewport. Support erhielt einen sichtbaren Kontext-Hin-/Rückweg, Threads
  behält zwischen 601 und 720 px seine bedienbare Zwei-Pane-Ansicht.
- Die vollständige lokale 36-Flächen-Baseline unter
  `output/playwright/business-os-modern-os-v42-full/` ist grün: 34/34 Core-
  Apps plus Files und Source Editor, 0 Console Events, Warm-Mount p95 28 ms,
  Warm-Ready p95 51 ms und sichtbare Interaktion p95 51,7 ms.
- Outbound startet kompakt und reduzierte seine Default-Tabelle von ungefähr
  10.763 auf 1.382 px intrinsische Breite. Der große CTOX-Workflow-Canvas ist
  layout-/paint-contained; sein Window-Mode-Wechsel fiel von 143,5 auf
  52,4 ms. Das 100-ms-Interaktionsbudget wurde nicht gelockert.

Neue benannte Design-System-Verträge:

- `ctox-record-workbench`, `ctox-record-workbench__header/body/status`,
  `ctox-compact-form`, `ctox-compact-form__fields/actions`,
  `ctox-compact-field` und `ctox-record-list` ersetzen die wiederholt kopierten
  losen ATS-Form-/Listenfragmente.
- Die Formfelder besitzen sichtbare, lokalisierte Kurzlabels und bleiben bei
  640, 960 und 1180 px ohne Root-Overflow. Unterhalb des Container-Breakpoints
  stapelt das Kit die Form und vergrößert die effektiven Touch-Ziele.
- `ctox-run-control` ist die einzige akzentstarke Signature-Automation. Ein
  statischer Guard verlangt genau eine Source-Instanz in Research, Outbound,
  IoT, App Creator und Coding Agents. Create, Save, Delete, Filter und
  Freigaben bleiben kompakte Routineaktionen.
- `.impeccable/design.json`, der Impeccable-Projektkontext und der separate
  Skill unter `/Users/michaelwelsch/Documents/ctox-business-os-deploy-skill/`
  enthalten denselben Ableton-Dichte-, 640×480-/Mobile-Sheet-, Pane-Rückweg-
  und Signature-Automation-Vertrag; der Deploy-Skill-Validator ist grün.

Vollständig visuell migriert und einzeln per Browser bei 640/960/1180 px
abgenommen:

| App | Neuer Vertrag | Browser-Evidence | Status |
| --- | --- | --- | --- |
| `esign` | Compact Record Workbench | `business-os-compact-workbench-v42` | migriert |
| `intake` | Compact Record Workbench | `business-os-compact-workbench-v42` | migriert |
| `interviews` | Compact Record Workbench | `business-os-compact-workbench-v42` | migriert |
| `placements` | Compact Record Workbench | `business-os-compact-workbench-v42` | migriert |
| `consent` | Compact Record Workbench | `business-os-compact-workbench-v42b` | migriert |
| `nachweise` | Compact Record Workbench | `business-os-compact-workbench-v42b` | migriert |
| `submissions` | Compact Record Workbench | `business-os-compact-workbench-v42b` | migriert |

### Revision 25: zweite Einzel-App-Welle am 2026-07-11

Weitere vollständig visuell migrierte Apps mit eigenem Browsernachweis:

| App | Neuer Vertrag | Browser-Evidence | Status |
| --- | --- | --- | --- |
| `credentials` | Compact Credential Workbench | `business-os-credentials-workbench-v43` | migriert |
| `cv-print-builder` | Responsive Pane Grid mit echtem Pane-/Window-Resize | `business-os-cv-responsive-v44`, `business-os-cv-interactive-v44` | migriert |
| `invoices` | Responsive Rechnungs-Workbench mit mobilem Pane-Stack | `business-os-invoices-stack-v2`, `business-os-invoices-interactive-v2`, `output/rev70-invoices-active-ui.json` | migriert; UI-Erstellung/Command/Resume/native Projektion grün (Revision 69) |
| `iot` | Kompakte Zwei-Pane-Automation-App, Rechtsklick-Signalmenü verdrahtet | `business-os-reports-iot-compact-v4` | migriert; Runtime-Rechte/Reload grün, Fachmutation offen (Revision 66) |
| `reports` | Kompakte Reports-/Rollback-Workbench ohne Glass/Shadow/Side-Stripe | `business-os-reports-iot-compact-v4` | migriert; System-Scope/Reload grün, Fachmutation offen (Revision 66) |
| `calendar` | Kompakter Drei-Pane-Kalender mit dunklem EventCalendar-Surface | `business-os-calendar-compact-v3` | migriert; Runtime-Rechte/Reload grün, Fachmutation offen (Revision 66) |

Zusätzliche Befunde und Fixes:

- Der systemweite `.shell-window-module-root`-Grid-Fehler wurde behoben:
  explizite `minmax(0, 1fr)`-Zeile und `grid-row: 1` verhindern, dass
  Fullscreen-/Windowed-Apps in zwei implizite Zeilen auseinanderfallen und
  unten leer bleiben.
- Invoices erhielt bei 760 px und darunter einen echten vertikalen Pane-Stack;
  die vormals kollabierende Center-Spalte bei 640 px ist wieder bedienbar.
- IoT registriert `contextmenu` jetzt tatsächlich am App-Root; der vorhandene
  Signal-/Widget-Rechtsklickpfad war vorher implementiert, aber nicht
  erreichbar.
- Reports und IoT wurden von altem Glass-/Shadow-/Kartenstyling auf flache,
  kompakte Pane-Flächen zurückgeführt. Routine-Buttons bleiben neutral; die
  vorhandene IoT-Run-Control bleibt die einzige akzentstarke Automation.
- Der isolierte 34-Modul-UI-Lauf `business-os-ui-single-row-v45` ist grün:
  34/34 Module öffnen, rendern und interagieren, 0 Browserwarnungen/-fehler,
  35 Desktop-Icons.

### Revision 26: Documents und mobiler Shell-Pane-Stack am 2026-07-11

Weitere vollständig visuell migrierte App mit eigenem Browsernachweis:

| App | Neuer Vertrag | Browser-Evidence | Status |
| --- | --- | --- | --- |
| `documents` | Kompakte Dokumenten-Workbench mit flacher Drei-Pane-Shell, mobilem vertikalem Pane-Stack und Office-Engine-kompatiblem Editor-Surface | `business-os-documents-compact-v3`, `business-os-documents-interactive-v2`, `output/rev67-documents-active-ui.json` | migriert; UI-Import/Version/Blob/Resume/native Projektion grün (Revision 68) |

Zusätzliche Befunde und Fixes:

- Documents wurde von historischer Glass-/Shadow-/Card-Optik auf die kompakte
  Business-OS-Surface-Sprache zurückgeführt. Routine-Aktionen bleiben knapp und
  neutral; Import, Filter, Sortierung, Dokumentvorschau und Runbook-Auswahl
  behalten den Workbench-Fokus.
- Die globale Mobile-Sheet-Regel für `.shell-window-module-root` stapelt
  Shell-Panes bei maximal 600 px jetzt vertikal: linke Kontextpane, Center-
  Content und rechte Detail-/Runbook-Pane sind damit nacheinander scrollbar
  erreichbar, statt im Smartphone-Layout seitlich gegeneinander zu kollabieren.
- Die Documents-Regression ist grün: `documents.test.mjs` 7/7, Design-System-
  Contract grün, Browser-Baseline `business-os-documents-compact-v3` grün.
- Der interaktive Window-Lauf `business-os-documents-interactive-v2` besteht
  alle App-Schritte für Öffnen, Resizing, Maximieren, Minimieren, Schließen und
  Mobile-Overflow. Der Lauf bleibt insgesamt rot, weil der isolierte Browser
  weiterhin `ws://127.0.0.1:20876` nicht erreicht.
- Der globale Shell-Fix wurde zusätzlich gegen die bereits migrierten Apps
  `documents`, `calendar`, `reports` und `iot` mit
  `business-os-shell-slot-mobile-stack-v1` regressionsgeprüft.

### Revision 27: Spreadsheets und Mobile-Dichtekorrektur am 2026-07-11

Weitere vollständig visuell migrierte App mit eigenem Browsernachweis:

| App | Neuer Vertrag | Browser-Evidence | Status |
| --- | --- | --- | --- |
| `spreadsheets` | Kompakte Tabellen-Workbench mit flacher Editor-Surface, neutralen Routine-Controls und sichtbarem Runbook-Hero auf Mobile | `business-os-spreadsheets-compact-v1`, `business-os-spreadsheets-interactive-v5`, `output/rev66-spreadsheets-active-ui.json` | migriert; UI-Mutation/Version/Blob/Resume/native Projektion grün (Revision 67) |

Zusätzliche Befunde und Fixes:

- Spreadsheets wurde visuell auf denselben Ableton-Dichtevertrag wie Documents
  gebracht: keine Spring-/Scale-Hovers, keine Card-Shadows, keine Glass-
  Flächen, neutrale Import-/Export-/Add-/Filter-Controls.
- Der Spreadsheet-Editor nutzt weiterhin die bestehende Office-/JSpreadsheet-
  Logik; geändert wurde nur die Präsentationsschicht. Die bestehende Command-
  Bus-Anbindung für Runbooks bleibt unverändert.
- Der Runbook-Start bleibt die einzige bewusst dominante Signature-Automation.
  Der mobile Promptbereich wurde verdichtet, damit Prompt und Hero-Button
  zusammen im ersten Mobile-View sichtbar bleiben.
- Die globale Mobile-Sheet-Center-Pane wurde weiter komprimiert
  (`minmax(200px, auto)` / `min-height: min(220px, …)`), weil leere Center-
  Flächen sonst rechte Automation-Panes unter Chat/Taskbar verdrängen.
- Die Regression ist grün: `spreadsheets.test.mjs` 10/10,
  Design-System-Contract grün, Browser-Baseline
  `business-os-spreadsheets-compact-v1` grün.
- Der interaktive Window-Lauf `business-os-spreadsheets-interactive-v5` besteht
  alle App-Schritte für Öffnen, Resizing, Maximieren, Minimieren, Schließen und
  Mobile-Overflow. Der Lauf bleibt insgesamt rot, weil der isolierte Browser
  weiterhin `ws://127.0.0.1:20876` nicht erreicht.
- Die globale Pane-Änderung wurde gegen `calendar`, `documents`, `iot`,
  `reports` und `spreadsheets` mit `business-os-shell-slot-mobile-stack-v2`
  regressionsgeprüft; das JSON enthält 5 Apps und 0 Failures. Der Runner
  schrieb den Erfolg, musste aber beim Cleanup manuell beendet werden.

### Revision 28: Buchhaltung als kompakte Finanz-Workbench am 2026-07-11

Weitere vollständig visuell migrierte App mit eigenem Browsernachweis:

| App | Neuer Vertrag | Browser-Evidence | Status |
| --- | --- | --- | --- |
| `buchhaltung` | Kompakte Finanz-Workbench mit flachen Drei-Pane-Surfaces, neutralen Buchungs-/Setup-Aktionen und mobilem Pane-Stack | `business-os-buchhaltung-compact-v3`, `business-os-buchhaltung-interactive-v3`, `output/rev71-buchhaltung-active-ui.json` | migriert; UI-Buchung/Zeilen/Resume/native Projektion grün (Revision 70) |

Zusätzliche Befunde und Fixes:

- Buchhaltung wurde von Glass-/Panel-Shadow-/Premium-Card-Optik auf flache
  Shell-Surfaces mit `surface-radius` und dichter Navigation umgestellt.
- Die eigene Drei-Spalten-Appstruktur bleibt fachlich erhalten, nutzt aber
  kleinere Gutter, kleinere Pane-Mindesthöhen und mobile Container-Queries,
  damit SKR-Navigation, Kontenliste und Companion nacheinander erreichbar
  bleiben.
- Routine- und Setup-Aktionen wie `Kontenrahmen neu initialisieren`, Buchen,
  Storno oder DATEV/GoBD-Controls werden als kompakte Outline-/Kit-Aktionen
  behandelt. Buchhaltung besitzt in diesem Stand keinen eigenständigen KI-
  Automation-Hero; deshalb wird keine Routine-Aktion als Hero-Fläche
  hervorgehoben.
- Die Buchhaltungslogik, RxDB-/GoBD-Pfade, Parser, DATEV-/ELSTER-Exporter und
  bestehende Kontextmenü-/Chat-Delegation wurden nicht verändert.
- Die Regression ist grün: `buchhaltung.test.mjs` Exit 0,
  Design-System-Contract grün, Browser-Baseline
  `business-os-buchhaltung-compact-v3` grün.
- Der interaktive Window-Lauf `business-os-buchhaltung-interactive-v3` besteht
  alle App-Schritte für Öffnen, Resizing, Maximieren, Minimieren, Schließen und
  Mobile-Overflow. Der Lauf bleibt insgesamt rot, weil der isolierte Browser
  weiterhin `ws://127.0.0.1:20876` nicht erreicht.

### Revision 29: App Store als kompakte Store-Workbench am 2026-07-11

Weitere vollständig visuell migrierte App mit eigenem Browsernachweis:

| App | Neuer Vertrag | Browser-Evidence | Status |
| --- | --- | --- | --- |
| `app-store` | Kompakte Store-Workbench mit flacher Catalog-/Scope-Navigation, neutralen Install-/Open-/Detail-Aktionen und mobilem Sync-/Empty-State | `business-os-app-store-compact-v1`, `business-os-app-store-interactive-v1` | migriert; Release/Audience/Context Runtime grün (Revision 58/59/65) |

Zusätzliche Befunde und Fixes:

- Der App Store wurde von Glass-Pane, Panel-Shadow, Card-Hover-Lift und
  pulsierenden Shadow-Animationen auf flache Shell-Surfaces, kleinere Gutter
  und kompakte Cards umgestellt.
- Store-spezifische Routineaktionen wie Installieren, Öffnen, Details,
  GitHub, Aktualisieren, Versionen und Deinstallieren bleiben visuell neutral
  und dicht. `App von Scratch erstellen` bleibt als Store-spezifischer
  Creation-Akzent sichtbar, ohne die restlichen Controls zu dominieren.
- Grid- und List-View nutzen denselben flachen Card-Vertrag; Status-, Version-
  und Lifecycle-Badges bleiben in der Card lesbar, aber ohne Card-Shadow oder
  Side-Stripe.
- Die mobile Store-Ansicht wurde verdichtet: Startaktionen, Scope-Liste,
  GitHub-Sync-/Empty-State und Chat/Taskbar bleiben im ersten Mobile-Stack
  erreichbar, ohne quer zu überlaufen.
- App-Store-Logik, Command-Bus-Installationspfade, externe Zip/GitHub-Install-
  Guards, Release-/Rollback-Dialoge und Kontextmenü-/Chat-Delegation wurden
  nicht verändert.
- Die Regression ist grün: `app-store.test.mjs` 20/20,
  Design-System-Contract grün, Browser-Baseline
  `business-os-app-store-compact-v1` grün.
- Der interaktive Window-Lauf `business-os-app-store-interactive-v1` besteht
  alle App-Schritte für Öffnen, Resizing, Maximieren, Minimieren, Schließen und
  Mobile-Overflow. Der Lauf bleibt insgesamt rot, weil der isolierte Browser
  weiterhin `ws://127.0.0.1:20876` nicht erreicht.

### Revision 30: Matching als kompakte Pipeline-/Workbench-App am 2026-07-11

Weitere vollständig visuell migrierte App mit eigenem Browsernachweis:

| App | Neuer Vertrag | Browser-Evidence | Status |
| --- | --- | --- | --- |
| `matching` | Kompakte Matching-Workbench mit flachen Shell-Surfaces, neutralen Filter-/Board-Karten, einspaltigem Mobile-Stack und akzentuierten Matching-/CTOX-Run-Controls | `business-os-matching-compact-v1`, `business-os-matching-interactive-v1`, `source-mobile-stacked` | migriert; Runtime-Rechte/Reload grün, Fachmutation offen (Revision 66) |

Zusätzliche Befunde und Fixes:

- Matching wurde von dekorativen Gradients, Panel-Shadows, Hover-Lift und
  Premium-Micro-Animationen auf flache Shell-Surfaces, dichte Controls und
  ruhige Pipeline-Karten umgestellt.
- Bestehende Command-Bus-Anbindung bleibt erhalten:
  `setBusinessOsCommandBus(ctx.commandBus)` wird beim Mount weiter gesetzt.
- Routineaktionen für Suche, Filter, Sortierung, Import, Download, Upload und
  Ansichtswechsel bleiben neutral und kompakt. Matching-/CTOX-Ausführungen
  behalten als fachliche Automationsaktionen einen sichtbaren Accent.
- Das lokale Drei-Spalten-Prinzip bleibt auf Desktop/Window erhalten. Für
  Mobile-Sheets stapelt Matching Anforderungen, Matches und Objekte
  einspaltig; Spaltenresizer werden dort ausgeblendet, und Toolbars fallen auf
  eine Spalte zurück.
- Der generische Window-Manager wurde für Mobile-Sheets korrigiert: windowed
  Apps übernehmen auf Mobile nicht mehr den linken Desktop/Icon-Rail-Inset,
  sondern liegen viewport-bündig links/rechts, während Topbar- und Bottom-
  Inset erhalten bleiben.
- Source-Browsernachweis auf `127.0.0.1:8772` mit injizierter Session und
  Sync-Konfiguration zeigt `matching` als `desktop-app/windowed`, Mobile-
  Sheet `x=0`, `width=390`, kein Dokument-Overflow und sichtbare Chat-Leiste.
- Der echte lokale Host auf `127.0.0.1:8765` bleibt als Runtime-Befund rot:
  seine RxDB-Katalogprojektion liefert `matching` weiterhin als
  `layout.shell=full-workspace` ohne `launch_kind`, obwohl Source-Registry und
  eingebetteter Fallback-Katalog bereits `desktop-app/windowed` enthalten.
  Dadurch öffnen die generischen QA-Skripte Matching im Live-Host nicht als
  Fenster. Das ist Katalog-/Installationsdrift und kein CSS-Status.
- Die Regression ist grün: Matching-Unit-Test, Pipeline-Test, Screening-Test,
  `ctox-real business-os app validate matching --source --json`,
  Design-System-Contract, `node --check` für Window-Manager und Matching-
  Entry sowie Chat-Layout-Guard.

### Revision 31: Threads-Source-Verdichtung und Shell-Regressionen am 2026-07-12

Noch nicht als vollständig visuell migrierte Einzel-App gezählt:

| App/Surface | Neuer Vertrag | Evidence | Status |
| --- | --- | --- | --- |
| `threads` | kompakter Timeline-/Approval-Thread mit flachen Pane-Surfaces, neutralem Composer/Filter und akzentuierten Freigabe-/KI-Delegationsaktionen | `threads.test.mjs`, `ctox-real business-os app validate threads --source --json` | Source validiert, Live-Signoff blockiert |
| Business Chat | Mobile zeigt nur das aktive Chat-Fenster; Tabs/Dock bleiben der Wechselmechanismus; alte Desktop-Karussell-Positionen dürfen keine Mobile-Desktop-Icons blockieren | `business-chat-behavior-r31-shell-fix-v2`, `assert-business-chat-layout.mjs` | Guard grün |
| Desktop Icons | Lange und untrennbare App-Namen bleiben auf zwei Zeilen innerhalb der Icon-Zelle | Source-Browser-Probe `source-shell-fix-config` | Source grün, Live-Snapshot alt |

Zusätzliche Befunde und Fixes:

- Threads erhielt eine kompakte Revision-31-CSS-Schicht: reduzierte Pane-Gaps,
  flache Surfaces ohne Shadow, dichte Filter-/Thread-/Timeline-/Composer-
  Controls, mobile Detail-/Context-Stacks und akzentuierte fachliche
  Freigabe-/KI-Aktionen statt farbiger Routinebuttons.
- Der Chat-Layout-Guard prüft jetzt explizit, dass Mobile unter 780 px inaktive
  Chat-Fenster versteckt und deren Pointer deaktiviert. Damit kann ein altes,
  absolut positioniertes Desktop-Carousel nicht mehr über mobilen Desktop-Icons
  liegen.
- Der Chat-Behavior-Harness bleibt grün: 75 Szenarien inklusive 0/1/6/8/12/
  100/1000 Chats, Busy-Day-Panel, Date-Workload, Gruppierung und Persistenz-
  Fehlertoleranz.
- Desktop-Icon-Labels erhielten `box-sizing: border-box` im Label selbst. Die
  Source-Browser-Probe mit `AutomatisierungsfreigabeEnterprise2026` ergibt
  `lines=2` und `inside=true`.
- Die installierte lokale Oberfläche auf `127.0.0.1:8765` ist als aktueller
  Live-Nachweis weiterhin rot: `curl /business-os/app.js` liefert dort
  `APP_BUILD='20260711-app-runtime-v3'`, obwohl Source und
  `runtime/business-os/app.js` bereits `20260712-shell-chat-mobile-v42`
  enthalten. Der laufende `ctox-real service --foreground` serviert also einen
  alten Asset-Snapshot oder hält Assets im Prozess. Dadurch reproduziert der
  Live-Interaktivlauf weiter die alten Mobile-Chat- und Label-Failures.
- Zusätzlich bleibt die RxDB-Katalogprojektion für `threads` und `matching`
  stale: beide werden im Live-Katalog als `full-workspace` ohne `launch_kind`
  geliefert, obwohl die Source-Registry windowed/Desktop-App deklariert.
  Deshalb kann `business-os-interactive-window-qa.mjs` Threads auf Port 8765
  noch nicht als `.shell-window[data-owner-id="desktop-app:threads"]` öffnen.
- Source-/statische Regressionen sind grün: `node --check` für Shell/Chat,
  `assert-business-chat-layout.mjs`, `assert-design-system.mjs`,
  `threads.test.mjs`, `ctox-real business-os app validate threads --source
  --json` und der Chat-Behavior-Harness. Kein Production-Ready-Claim, solange
  der installierte Host nicht mit frischen Assets und frischem Katalog
  nachgewiesen wurde.

Teilweise migriert, Signature-Automation umgesetzt und Browser-Smoke grün,
aber noch ohne abschließendes visuelles Einzel-Signoff der gesamten App:
`research`, `outbound`, `creator` und `coding-agents`. Evidence:
`output/playwright/business-os-hero-run-control-v43/`.

Offene Runtime-Befunde bleiben getrennt von der App-Gestaltung:

- Der installierte LaunchAgent meldete zeitweise `replicationUp=true`, obwohl
  seine detaillierten Health-Stages keinen verbundenen Signaling-/DataChannel-
  Pfad zeigten. Später nahm Port 8765 TCP an, lieferte aber innerhalb von fünf
  Sekunden keine HTTP-Antwort. Die Source-identische isolierte Shell auf 8771
  blieb bedienbar und lieferte die grünen App-Nachweise.
- Die aktuelle isolierte Source-Shell auf 8772 bleibt UI-seitig bedienbar,
  aber Browserläufe mit scharfem Console-Gate scheitern an
  `ws://127.0.0.1:20876` mit `ERR_CONNECTION_REFUSED`. UI-Gates mit
  `BUSINESS_OS_QA_FAIL_ON_CONSOLE=0` sind nur Layout-/Interaktionsnachweise,
  keine Production-Ready-Freigabe.
- Dieser Supervisor-/Statuswiderspruch verhindert einen neuen installierten
  Releaseclaim. Er wird nicht durch größere UI-Wartezeiten und nicht durch das
  Hochstufen von App-Migrationsstatus kaschiert.

### Revision 32: installierter Windowed-Pfad für Threads/Matching am 2026-07-12

Revision 32 schließt den in Revision 31 noch roten Live-Nachweis für den
lokalen installierten Business-OS-Host auf `http://127.0.0.1:8765/`.

Als vollständig visuell migrierte Einzel-App neu gezählt:

| App/Surface | Neuer Vertrag | Evidence | Status |
| --- | --- | --- | --- |
| `threads` | kompakte Thread-/Approval-App im echten Shell-Window mit Version/Status/Header-Actions, frei bedienbarem Resize, Mobile-Sheet und erreichbarem Pane-Stack | `output/playwright/business-os-threads-matching-r32-live-windowed-green/business-os-interactive-window-qa.json`, `threads.test.mjs`, `ctox-real business-os app validate threads --source --json` | Live grün |
| `matching` | bereits in Revision 30 migrierte Pipeline-/Workbench-App, erneut gegen den installierten Windowed-Pfad geprüft | `output/playwright/business-os-threads-matching-r32-live-windowed-green/business-os-interactive-window-qa.json` | Live grün |
| Desktop Launcher | Web-App-Icons starten mit Einzelklick; Drag bleibt ohne Start; Module öffnen direkt als Shell-Window, wenn Presentation/Registry windowed deklariert | direkter Browser-Probe plus interaktiver QA-Lauf | Live grün |
| Installierter Asset-Snapshot | lokale Installation liefert wieder aktuelle Registry, Window-Manager-, Desktop- und Icon-Assets | Live-Probe auf `127.0.0.1:8765` | lokal repariert, Release-Prozess bleibt zu härten |

Technische Fixes und Befunde:

- `modules/desktop/iconDrag.js` besitzt jetzt einen expliziten Click-Fallback
  für Single-Click-Start. Der bisherige reine `mousedown`/`document mouseup`-
  Pfad war in realen Browserläufen zu empfindlich; ein Icon konnte nur
  selektiert werden oder erst per Doppelklick über den Hash in eine
  Fullscreen-Route fallen.
- Die Desktop-Import-Revision wurde auf `20260712-single-click-v2` angehoben,
  damit der aktualisierte Icon-Handler zuverlässig aus frischen Assets geladen
  wird.
- Der lokale Installationspfad
  `/Users/michaelwelsch/.local/lib/ctox/current/src/apps/business-os/` wurde
  für die Live-Abnahme mit den aktuellen Source-Assets synchronisiert. Dabei
  waren nicht nur `app.js`/`index.html`, sondern auch `modules/registry.json`,
  `shared/window-manager.js`, `modules/desktop/index.js`,
  `modules/desktop/iconDrag.js` und `modules/invoices/icon.svg` relevant.
- Der zuvor rote Live-Katalog war kein App-Layout-Fehler allein: die
  installierte `modules/registry.json` deklarierte `matching` noch als
  `full-workspace` ohne `launch_kind`, obwohl Source bereits `desktop-app` und
  `layout.shell=windowed` enthielt. Nach Registry-Sync öffnen Matching und
  Threads als `.shell-window[data-owner-id="desktop-app:<id>"]`.
- Der alte installierte `shared/window-manager.js` setzte noch kein
  `data-owner-id` auf dem Window-Element. Dadurch konnten Apps visuell als
  Fenster erscheinen, aber die interaktive QA fand sie nicht als app-eigene
  Shell-Windows. Nach Asset-Sync ist `data-owner-id="desktop-app:matching"` und
  `data-owner-id="desktop-app:threads"` vorhanden.
- Der letzte Console-/Request-Fehler war ein echter Asset-404:
  `modules/invoices/icon.svg`. Die aktuelle Source-Datei wurde in den
  installierten Asset-Snapshot übernommen; danach ist das strikte Browser-Gate
  für die Zielkohorte grün.
- Der wiederholte interaktive Lauf prüfte beide Apps per sichtbarem Launcher,
  Window-Header mit Version/Status/Source/Versionen, echten South- und
  SE-Resize-Drags, Maximize/Restore, Close, 900-px-Shell, Mobile-Shell,
  Mobile-App-Header und erreichbare mobile Pane-Navigation. Ergebnis:
  `OK 1/2 matching`, `OK 2/2 threads`, null Browser-Failures.

Offen bleibt als Release-/Installationsblocker:

- Der Live-Fix wurde durch gezieltes Synchronisieren des installierten lokalen
  Asset-Snapshots erreicht. Das beweist den aktuellen UI-Pfad, ersetzt aber
  noch keinen reproduzierbaren Installer-/Release-Nachweis, dass alle
  Registry-, Window-Manager-, Icon-, App- und Query-String-Assets atomar in
  die aktive Installation gelangen.
- Der Gesamtplan darf deshalb nicht als fertig gelten, bevor ein frischer
  Build-/Installationslauf denselben grünen
  `business-os-interactive-window-qa.mjs`-Nachweis ohne manuelle Asset-Kopie
  liefert.

### Revision 33: Managed-Asset-Guard für Business-OS-Installationen am 2026-07-12

Revision 33 schließt nicht den vollständigen Installer-Releaseclaim, beseitigt
aber die bisher fehlende maschinelle Sichtbarkeit des Asset-Snapshot-Drifts.

Neu:

- `src/apps/business-os/scripts/assert-managed-business-os-assets.mjs` prüft die
  kritischen Shell-Assets gegen drei autoritative Orte:
  1. Source: `src/apps/business-os`,
  2. persistenten managed State: `<STATE_ROOT>/business-os`,
  3. aktuell vom Release auflösbaren App-Root:
     `<current>/business-os` oder bei älteren Layouts
     `<current>/src/apps/business-os`.
- Optional prüft `--url http://127.0.0.1:8765`, ob der laufende HTTP-Host
  dieselbe `APP_BUILD` wie Source ausliefert.
- Der Guard deckt genau die Drift-Klasse ab, die Revision 31/32 sichtbar
  machte: `modules/registry.json`, `shared/window-manager.js`,
  `shared/shell-chat-composition.js`, Desktop-Launcher-Assets,
  App-CSS und fehlende Icon-Dateien wie `modules/invoices/icon.svg`.
- `package.json` enthält dafür `check:managed-assets`.

Gefundener Ist-Befund vor dem Sync:

- Der laufende HTTP-Host und der alte Release-App-Root unter
  `<current>/src/apps/business-os` waren bereits grün.
- Der neue managed State unter `~/.local/state/ctox/business-os` war noch
  stale: Registry, Window-Manager, Desktop-Launcher, Matching-CSS und
  `invoices/icon.svg` wichen von Source ab beziehungsweise fehlten.

Durchgeführt:

- Der State wurde mit demselben Exclude-Modell wie
  `sync_business_os_shell_assets()` aktualisiert:
  Shell-Assets werden überschrieben, `installed-modules`, `node_modules`,
  `notes` und Bench-Artefakte bleiben Tenant-/Runtime-State.
- Danach ist der neue Guard grün:
  `Managed Business OS asset guard OK: 12 critical assets`.
- Der reale Browserlauf gegen den lokalen Host ist im Rerun grün:
  `output/playwright/business-os-threads-matching-r33-managed-assets-rerun/business-os-interactive-window-qa.json`
  mit `OK 1/2 matching`, `OK 2/2 threads`, null Failures.

Weiterhin offen:

- Der Guard beweist aktuellen Source/State/HTTP-Gleichstand und verhindert
  erneuten stillen Drift. Er ersetzt noch keinen vollständigen frischen
  Build-/Installationslauf, bei dem `install.sh` ohne manuelle Vorarbeit den
  State synchronisiert, den `business-os`-Symlink im neuen Release-Layout setzt
  und anschließend dieselbe Browser-QA grün besteht.

### Revision 34: Installer-Smoke für Business-OS-Shell-Assets am 2026-07-12

Revision 34 schließt den direkten Funktionsnachweis der in Revision 33
eingeführten Installer-/State-Sync-Mechanik, ohne den vollständigen lokalen
Release-Build zu behaupten.

Neu:

- `tests/business_os_shell_asset_sync_smoke.sh` sourced `install.sh` ohne
  `main` auszuführen und prüft zwei Pfade:
  1. `sync_business_os_shell_assets(source_root, state_root)`,
  2. `setup_managed_install(source_root)` mit synthetischem Minimal-Source-Root.

Der Smoke beweist:

- stale Shell-Assets im State werden durch Source ersetzt,
- neue kritische Dateien wie `shared/shell-chat-composition.js`,
  `modules/desktop/iconDrag.js` und `modules/invoices/icon.svg` landen im
  State,
- `installed-modules` bleibt Runtime-/Tenant-State und wird nicht gelöscht,
- source-seitige `installed-modules` werden additiv übernommen, ohne
  vorhandene Runtime-Apps zu überschreiben,
- `node_modules`, `notes` und `app-creation-bench` werden nicht in den
  managed Business-OS-State publiziert,
- `setup_managed_install` erzeugt ein Release unter
  `releases/v<version>`, setzt `current`, setzt `<release>/business-os` als
  Symlink auf `<STATE_ROOT>/business-os` und `<release>/runtime` als Symlink
  auf `<STATE_ROOT>`.
- Während des erneuten installierten Browserlaufs wurde ein echter
  Window-Chrome-Blocker in Matching gefunden: das app-interne
  `sync-feedback`-Overlay lag mit extrem hohem Z-Index über dem rechten
  unteren Shell-Window-Resize-Griff und fing den `mousedown` des SE-Griffs ab.
  `modules/matching/index.css` begrenzt dieses Overlay jetzt unterhalb der
  Shell-Window-Layer, damit App-Feedback keine OS-Chrome-Bedienung verdeckt.

Evidence:

- `bash tests/business_os_shell_asset_sync_smoke.sh`:
  `business os shell asset sync smoke ok`
- `node src/apps/business-os/scripts/assert-managed-business-os-assets.mjs --url http://127.0.0.1:8765`:
  `Managed Business OS asset guard OK: 12 critical assets`
- Browser-Evidence nach dem Z-Order-Fix ist grün:
  `output/playwright/business-os-threads-matching-r34-installer-smoke-host-zfix/business-os-interactive-window-qa.json`
  mit `matching:true`, `threads:true`.

Weiterhin offen:

- Der Smoke beweist die Installerfunktionen isoliert und schnell. Er ersetzt
  noch keinen vollständigen realen `install.sh`-Durchlauf mit Build,
  Service-Neustart und anschließendem Browserlauf ohne vorbereiteten State.
  Dieser End-to-End-Releaseclaim blieb zu diesem Zeitpunkt wegen der bekannten
  optionalen Voxtral/GGML-CMake-Hänger ein separater Blocker; Revision 35
  behandelt genau diesen Buildpfad.

### Revision 35: Optionaler Runtime-Build-Skip und grüner `ctox`-Check am 2026-07-12

Revision 35 schließt den unmittelbar blockierenden Buildpfad aus Revision 34.
Der reale Hänger lag nicht in der Business-OS-Shell, sondern im optionalen
Voxtral-STT-Buildscript: ohne `GGML_LIB_DIR` baute
`ctox-voxtral-mini-4b-realtime-2602` vendored GGML per CMake standardmäßig,
auch wenn der übergeordnete Check mit
`CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS=1` lief. Auf macOS konnte dieser Pfad in
GGML/CMake-`TryCompile`-Prozessen hängen und dadurch volle Installer- oder
`cargo check --bin ctox`-Nachweise verdecken.

Geändert:

- `src/core/inference/models/voxtral_mini_4b_realtime_2602/build.rs`
  beobachtet jetzt `CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS`.
- Wenn kein explizites `GGML_LIB_DIR` gesetzt ist und der Skip aktiv ist,
  überspringt das Buildscript den vendored GGML/CMake-Pfad, setzt
  `ctox_ggml_unavailable` und meldet den nicht gelinkten Runtime-Teil als
  Warnung.
- Ein explizites `GGML_LIB_DIR` bleibt weiterhin stärker als der Skip, damit
  Entwickler vorhandene native GGML-Artefakte bewusst linken können.

Evidence:

- `cargo fmt --check --manifest-path src/core/inference/models/voxtral_mini_4b_realtime_2602/Cargo.toml`:
  grün.
- `CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS=1 cargo check -p ctox-voxtral-mini-4b-realtime-2602`:
  grün; Warnung:
  `GGML_LIB_DIR unset; vendored ggml build skipped by CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS`.
- `CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS=1 CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/ctox-cargo-check-skip-20260712 cargo check --bin ctox`:
  grün in 7m51s; nur bestehende Warnings.
- `bash tests/business_os_shell_asset_sync_smoke.sh`:
  `business os shell asset sync smoke ok`.
- `node src/apps/business-os/scripts/assert-managed-business-os-assets.mjs --url http://127.0.0.1:8765`:
  `Managed Business OS asset guard OK: 12 critical assets`.
- `node src/apps/business-os/scripts/assert-business-chat-layout.mjs`:
  `Business chat layout guard OK`.
- `node src/apps/business-os/modules/matching/tests/matching.test.mjs`:
  3/3 Tests grün.
- `node src/apps/business-os/modules/threads/tests/threads.test.mjs`:
  `threads module smoke ok`.
- `/Users/michaelwelsch/.local/bin/ctox-real business-os app validate threads --source --json`:
  `ok: true`.
- Browser-Evidence gegen den lokalen Host bleibt grün:
  `output/playwright/business-os-threads-matching-r35-build-skip-guard/business-os-interactive-window-qa.json`
  mit `OK 1/2 matching`, `OK 2/2 threads`.
- `git diff --check` auf den geänderten Business-OS-/Installer-/Voxtral-
  Plan-Dateien: grün.

Weiterhin offen:

- Revision 35 beweist den Build-/Check-Pfad mit optionalen Runtime-Builds im
  Skip-Modus und entfernt damit den bekannten Voxtral/GGML-Blocker. Sie ersetzt
  weiterhin keinen vollständigen realen `install.sh`-Durchlauf mit frischem
  Build, Service-Neustart und anschließendem Browserlauf ohne vorbereiteten
  State. Dieser End-to-End-Releaseclaim bleibt als nächstes Release-Gate offen.

### Revision 36: Full-Install-E2E versucht, durch Host-Ressourcen blockiert am 2026-07-12

Nach Revision 35 wurde der echte lokale Release-E2E unmittelbar versucht:

```bash
CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS=1 CARGO_BUILD_JOBS=2 \
  ./install.sh --rebuild /Users/michaelwelsch/Documents/ctox.nosync
```

Befund:

- Der Installer nahm den erwarteten Skip-Pfad:
  `CTOX_VOXTRAL_BUILD_GGML=0`,
  `CTOX_QWEN3_EMBEDDING_BUILD_CUDA=0`,
  `CTOX_VOXTRAL_TTS_BUILD_CUDA=0`.
- Der erste Lauf entfernte zusätzlich einen stale GGML/CMake-Cache aus dem
  State-Build-Verzeichnis und erreichte den normalen Release-Build des
  `ctox`-Binaries. Er wurde während `cargo build --release --bin ctox` mit
  `Terminated: 15` beendet.
- Ein zweiter Lauf nutzte die inkrementellen Artefakte, lief erneut in den
  normalen Release-Build und wurde später mit `Killed: 9` beendet.
- In beiden Läufen gab es keinen erneuten Voxtral/GGML-TryCompile-Pfad und
  keinen Business-OS-Shell-Fehler.
- Während des zweiten Kills lief parallel ein separater schwerer Release-Test:
  `cargo test --bin ctox web_stack_auth_assist_login_source_classifies_login_states --release -- --nocapture`
  mit einem `rustc --crate-name ctox` unter
  `/Volumes/models/ctox-runtime-build-18612041-target/...`, der mehrere GB RSS
  belegte. Zusätzlich liefen der Business-OS-Host, Browser-/WebKit-Prozesse und
  eine VM. Ein dritter paralleler Installlauf wurde deshalb bewusst nicht
  gestartet.
- Der installierte `current`-Symlink blieb unverändert auf
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260711T203418Z`;
  es wurde kein neuer Release-Claim erhoben.

Weiterhin offen:

- Den Full-Install-E2E in einem ruhigen Hostfenster wiederholen: keine
  parallelen `ctox`-Release-Tests, `CARGO_BUILD_JOBS=1` oder `2`, danach
  Service-Restart, Status-/Native-Peer-Nachweis, Managed-Asset-Guard gegen den
  frisch installierten Host und Live-Window-QA. Erst dieser Lauf schließt den
  Release-Claim.

### Revision 37: Conversations als kompakte Timeline-/Thread-App am 2026-07-12

Revision 37 setzt die sequenzielle Einzel-App-Modernisierung mit
`conversations` fort. Der fachliche Zuschnitt bleibt erhalten: Kanalfilter,
Threadliste, Timeline, Reply-Composer, Account-/Ordner-Fakten und
Channel-Metadaten. Geändert wurde die Präsentationsschicht, damit die App in
das moderne Business-OS-Shell-Raster passt:

- Die linke Threadliste nutzt keine `ctox-pane--glass`-Variante mehr. Damit
  wirkt sie wie eine dichte Arbeitsliste statt wie ein historischer Glass-Card-
  Block.
- Timeline-Nachrichten, Faktenkarten, Channel-Zeilen, Outbound-Status und
  Composer-Container verwenden `var(--control-radius)` statt großer 10/12-px-
  Rundungen.
- Unread-/Email-Zustände werden über Border-Farbe und leichte Flächen-Tints
  markiert, nicht mehr über dominante vertikale Side-Stripes. Das hält die App
  ruhiger und spart visuelles Gewicht für echte Signature-Automationen.
- Filterleisten, Thread-Zeilen, Header, Timeline-Gaps und rechte Detailkarten
  wurden verdichtet. Auf kleinen Viewports fallen Messages auf volle Breite
  zurück, ohne horizontales Overflow zu erzeugen.
- Der bestehende Conversations-Test enthält jetzt zusätzlich einen statischen
  Präsentationsvertrag gegen Glass-Panes, dominante Side-Stripes und große
  Bubble-Radien.

Evidence:

```bash
node --check src/apps/business-os/modules/conversations/index.js
node src/apps/business-os/modules/conversations/conversations.test.mjs
/Users/michaelwelsch/.local/bin/ctox-real \
  business-os app validate conversations --source --json
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=conversations-r37-source-route' \
BUSINESS_OS_INTERACTIVE_APP_IDS=conversations \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='modules/conversations' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-conversations-r37-source-route \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
```

Ergebnis:

- Conversations-Unit-/Presentation-Test: 7/7 grün.
- Source-App-Validator: `ok: true`, inklusive `node_check` und `module_test`.
- Browser-QA:
  `output/playwright/business-os-conversations-r37-source-route/business-os-interactive-window-qa.json`
  ist grün für Öffnen über Startmenü, Content-Mount, Header-Aktionen,
  Pane-Resize, Süd-/Corner-Resize, Maximieren/Wiederherstellen, Chat-Dock-
  Koexistenz und fehlendes horizontales Overflow. Der Lauf routet
  `modules/conversations` aus dem Source-Verzeichnis in den laufenden lokalen
  Host, weil der Full-Install-E2E aus Revision 36 noch offen ist.
- Console-/Page-Fehler: 0.

Offen bleibt:

- Der Conversations-Nachweis ist ein echter Browserlauf gegen aktuellen Source,
  aber noch kein frisch installierter Release-Snapshot. Das wird erst mit dem
  erneuten Full-Install-E2E geschlossen.

### Revision 38: Full-Install-E2E erneut mit `CARGO_BUILD_JOBS=1` hostseitig beendet am 2026-07-12

Nach dem Ende des zuvor konkurrierenden Release-Testprozesses wurde der echte
Installer noch einmal mit reduziertem Parallelismus gestartet:

```bash
CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS=1 CARGO_BUILD_JOBS=1 \
  ./install.sh --rebuild /Users/michaelwelsch/Documents/ctox.nosync
```

Befund:

- Der Installer setzte weiterhin den erwarteten optionalen Runtime-Skip:
  `CTOX_VOXTRAL_BUILD_GGML=0`,
  `CTOX_QWEN3_EMBEDDING_BUILD_CUDA=0`,
  `CTOX_VOXTRAL_TTS_BUILD_CUDA=0`.
- Der Lauf erreichte erneut den normalen `cargo build --release --bin ctox`-
  Pfad und kompilierte reguläre Rust-Abhängigkeiten sowie CTOX-/Harness-Crates.
- Es gab keinen erneuten GGML-/CMake-/TryCompile-Pfad.
- Der Prozess wurde nach mehreren Minuten mit
  `./install.sh: line 208: ... Killed: 9` beendet.
- Nach dem Kill liefen kein `cargo build --release --bin ctox`, kein
  `rustc --crate-name ctox` und kein `install.sh --rebuild` mehr. Der alte
  lokale Business-OS-Host lief weiter:
  `/Users/michaelwelsch/.local/bin/ctox-real business-os serve --addr 127.0.0.1:8765`.
- Der installierte `current`-Symlink blieb unverändert auf
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260711T203418Z`.

Einordnung:

- Revision 38 bestätigt den Blocker aus Revision 36 auch ohne den zuvor
  sichtbaren parallelen Release-Testprozess und mit `CARGO_BUILD_JOBS=1`.
- Der verbleibende Release-Claim ist damit kein Business-OS-Shell-, App-,
  Conversations-, Threads-, Matching- oder Voxtral/GGML-Codefehler, sondern ein
  lokales Host-/Release-Build-Ressourcenproblem.
- Bis ein frischer Install-Snapshot erzeugt werden kann, müssen App-
  Modernisierungen über Source-Validator, statische Guards und Browser-QA mit
  lokal gerouteten Source-Assets belegt werden.

### Revision 39: Notes kompakt migriert und Shell-Resize-Hitbox repariert am 2026-07-12

Revision 39 migriert `notes` aus dem historischen “Premium local-first /
macOS Notes”-Look in den Business-OS-Dichtevertrag:

- Der Modulkommentar und die lokalen Design-Tokens wurden auf einen kompakten
  operational editor zurückgeführt; lokale Paper-/Panel-Shadows sind auf
  `none` gesetzt.
- Sidebar, Notebook-/Tag-Gruppen, Footer-Lock, Popover, Notizliste und
  Editorfläche wurden enger gepaddet.
- Aktive Notizkarten nutzen keine inset-Side-Stripe-Shadow-Markierung mehr,
  sondern eine leichte Border-/Flächenmarkierung.
- Paper-Sheet, Popover, Dropdowns, PIN-Container und Note-Lock-Pad verwenden
  `var(--control-radius)` statt großer historischer Rundungen.
- Das PIN-Overlay nutzt Shell-Tokens, weniger Blur, keine Heavy Shadows und
  kompaktere Keypad-Ziele.
- Der Notes-Test enthält jetzt einen statischen Präsentationsvertrag gegen
  Premium-Port-Kommentar, Glass-Panes, Literal-Shadows, inset-Akzentstreifen
  und große 10/12/14/16/18/20/24-px-Radien.

Während der Browser-QA wurde ein generischer Shell-Bug sichtbar:

- Notes renderte korrekt, aber Süd- und Corner-Resize änderten weder Höhe noch
  Breite.
- Ursache: `.shell-window-resize` lag mit `z-index: 3` unter App-internen
  Pane-Schichten wie `.notes-sidebar-pane { z-index: 5 }`. Dadurch konnten
  App-Controls den äußeren Window-Resize-Hitbereich überdecken.
- Fix: `.shell-window-resize` liegt jetzt mit `z-index: 40` in einer höheren
  Shell-Chrome-Schicht. Das entspricht dem bestehenden Kommentar, dass Window-
  Chrome an der Außenkante immer gegen App-Content gewinnen muss.

Evidence:

```bash
node --check src/apps/business-os/modules/notes/index.js
node src/apps/business-os/modules/notes/notes.test.mjs
/Users/michaelwelsch/.local/bin/ctox-real \
  business-os app validate notes --source --json
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=notes-r39-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS=notes \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/notes' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-notes-r39-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
node src/apps/business-os/scripts/assert-business-chat-layout.mjs
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=conv-notes-r39-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS='conversations,notes' \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/conversations,modules/notes' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-conversations-notes-r39-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
git diff --check -- docs/business-os-app-platform-refactoring-plan.md \
  src/apps/business-os/app.css \
  src/apps/business-os/modules/notes/index.css \
  src/apps/business-os/modules/notes/notes.test.mjs \
  src/apps/business-os/modules/conversations/index.html \
  src/apps/business-os/modules/conversations/index.css \
  src/apps/business-os/modules/conversations/conversations.test.mjs
```

Ergebnis:

- Notes-Unit-/Presentation-Test: 7/7 grün.
- Source-App-Validator: `ok: true`.
- Browser-QA:
  `output/playwright/business-os-notes-r39-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün. Nach dem Shell-Resize-Hitbox-Fix funktionieren Süd-Shrink,
  Süd-Grow, Corner-Shrink, Corner-Grow, Maximieren/Wiederherstellen,
  Header-Aktionen, Content-Mount, Chat-Dock-Koexistenz und horizontales
  Overflow-Gate.
- Chat-Layout-Guard: grün.
- Kombinierter Conversations+Notes-Browserlauf mit Source-`app.css`:
  `output/playwright/business-os-conversations-notes-r39-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.
- `git diff --check`: grün für die berührten Dateien.

Offen bleibt:

- Wie bei Revision 37 ist der Browserlauf ein echter Source-gerouteter Lauf
  gegen den lokalen Host, aber noch kein frisch installierter Release-Snapshot.
  Revision 38 hält den Full-Install-Blocker separat fest.

### Revision 40: Knowledge kompakt migriert am 2026-07-12

Revision 40 migriert `knowledge` als Editor-/Document-Archetyp weiter in den
Business-OS-Dichtevertrag. Die fachliche Oberfläche bleibt erhalten: linke
Knowledge-/Runbook-/DataFrame-Navigation, shell-eigener Pane-Resizer, rechter
Markdown-/Runbook-/Data-Viewer und bestehende Context-/Command-Flächen.

Geändert wurde nur die Präsentationsschicht:

- Lokale `--knowledge-panel-radius` und `--knowledge-control-radius` leiten
  jetzt auf `var(--surface-radius)` und `var(--control-radius)` statt eigene
  14px-/6px-Werte zu etablieren.
- `--knowledge-shadow` ist `none`; stationäre Workspace-Flächen erzeugen keine
  eigene Elevation.
- Bundle-Heads, Knowledge-Items und Runbook-Items verwenden den gemeinsamen
  Control-Radius und kompaktere Innenabstände.
- Das Bundle-Caret wird nicht mehr als CSS-Dreieck über
  `border-left: 5px solid currentColor` gebaut. Es nutzt ein kleines
  typografisches Chevron in `.bundle-caret::before`, das weiterhin per Rotation
  den geöffneten Zustand zeigt.
- Der Knowledge-Test enthält jetzt einen statischen Präsentationsvertrag gegen
  Glass-Panes, breite Side-Borders, große Radien, Literal-Shadows und gegen
  lokale Token-Drift.

Evidence:

```bash
node --check src/apps/business-os/modules/knowledge/index.js
node src/apps/business-os/modules/knowledge/test.mjs
/Users/michaelwelsch/.local/bin/ctox-real \
  business-os app validate knowledge --source --json
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=knowledge-r40-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS=knowledge \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/knowledge' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-knowledge-r40-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=notes-knowledge-r40-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS='notes,knowledge' \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/notes,modules/knowledge' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-notes-knowledge-r40-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
```

Ergebnis:

- Knowledge-Unit-/Presentation-Test: 14/14 grün.
- Source-App-Validator: `ok: true`.
- Browser-QA:
  `output/playwright/business-os-knowledge-r40-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün für Startmenü-Launch, Content-Mount, Header-Aktionen, Pane-Resize,
  Süd-/Corner-Resize, Maximieren/Wiederherstellen, Chat-Dock-Koexistenz und
  horizontales Overflow-Gate.
- Kombinierter Notes+Knowledge-Browserlauf mit Source-`app.css`:
  `output/playwright/business-os-notes-knowledge-r40-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.

Offen bleibt:

- Wie Revision 37 und 39 ist dieser Lauf Source-geroutet, weil Revision 38 den
  echten lokalen Full-Install-E2E weiterhin als Host-/Release-Build-
  Ressourcenblocker festhält.

### Revision 41: Shiftflow kompakt migriert am 2026-07-12

Revision 41 migriert `shiftflow` als Planner-/Workflow-Archetyp weiter in den
Business-OS-Dichtevertrag. Fachlogik, Arbeitszeit-/Tarif-Tests, Planner-Filter,
Tab-State und Wochenlogik bleiben unverändert.

Geändert wurde die Präsentationsschicht:

- `--shiftflow-radius` und `--shiftflow-panel-radius` leiten jetzt auf
  `var(--control-radius)` und `var(--surface-radius)` statt eigene 8px-/12px-
  Radien zu etablieren.
- Stationäre Planner-Panes, Finanzkarten, Shift-Cards, Timesheet-Cards,
  Project-Cards und Add-Shift-Hover-Controls verwenden keine dekorativen
  Workspace-/Hover-Shadows mehr.
- Der aktive Employee-Avatar nutzt Border-Farbe statt Pulse-Shadow und
  Keyframe-Animation.
- Drag-over-Zellen nutzen `outline` mit negativem Offset statt
  `box-shadow: inset ...`.
- Das frühere `Premium Side Drawer Forms`-Label wurde neutralisiert.
- Der inline gerenderte Abteilungs-Chip nutzt `var(--control-radius)` statt
  `border-radius:12px`.
- Der Shiftflow-Test enthält jetzt einen statischen Präsentationsvertrag gegen
  Premium-Kommentar, Literal-Shadows, inset-Shadows, große Radien, breite
  Side-Borders, den alten Pulse-Keyframe und den 12px-Inline-Chip.

Evidence:

```bash
node --check src/apps/business-os/modules/shiftflow/index.js
node src/apps/business-os/modules/shiftflow/test.mjs
/Users/michaelwelsch/.local/bin/ctox-real \
  business-os app validate shiftflow --source --json
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=shiftflow-r41-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS=shiftflow \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/shiftflow' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-shiftflow-r41-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=notes-knowledge-shiftflow-r41-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS='notes,knowledge,shiftflow' \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/notes,modules/knowledge,modules/shiftflow' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-notes-knowledge-shiftflow-r41-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
```

Ergebnis:

- Shiftflow-Unit-/Presentation-Test: 5/5 grün.
- Source-App-Validator: `ok: true`.
- Browser-QA:
  `output/playwright/business-os-shiftflow-r41-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün für Startmenü-Launch, Content-Mount, Header-Aktionen, Süd-/Corner-
  Resize, Maximieren/Wiederherstellen, Chat-Dock-Koexistenz und horizontales
  Overflow-Gate.
- Kombinierter Notes+Knowledge+Shiftflow-Browserlauf mit Source-`app.css`:
  `output/playwright/business-os-notes-knowledge-shiftflow-r41-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.

Offen bleibt:

- Wie Revision 37, 39 und 40 ist dieser Lauf Source-geroutet, weil Revision 38
  den echten lokalen Full-Install-E2E weiterhin als Host-/Release-Build-
  Ressourcenblocker festhält.

### Revision 42: AppSec/Deployment Audit kompakt migriert am 2026-07-12

Revision 42 migriert `appsec-pentest` als Queue-/Workflow-Archetyp weiter in
den Business-OS-Dichtevertrag. Die App war bereits weitgehend auf das
gemeinsame Kit umgestellt; übrig war der selected-row-Akzent als inset
Side-Stripe.

Geändert wurde die Präsentationsschicht:

- `.appsec-row[aria-selected="true"]` nutzt keine
  `box-shadow: inset 3px 0 0 var(--accent)`-Markierung mehr.
- Ausgewählte AppSec-Zeilen werden über `border-color: var(--accent)` und
  `background: var(--accent-soft)` markiert.
- Der bestehende AppSec-Contract-Test liest nun zusätzlich `index.css` und
  sichert ab, dass keine Glass-Panes, breiten Side-Stripes, Literal-Shadows
  oder großen 10/12/14/16/18/20/24-px-Radien zurückkehren.

Evidence:

```bash
node --check src/apps/business-os/modules/appsec-pentest/index.js
node src/apps/business-os/modules/appsec-pentest/appsec-pentest.test.mjs
/Users/michaelwelsch/.local/bin/ctox-real \
  business-os app validate appsec-pentest --source --json
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=appsec-r42-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS=appsec-pentest \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/appsec-pentest' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-appsec-r42-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=appsec-notes-knowledge-shiftflow-r42-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS='appsec-pentest,notes,knowledge,shiftflow' \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/appsec-pentest,modules/notes,modules/knowledge,modules/shiftflow' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-appsec-notes-knowledge-shiftflow-r42-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
```

Ergebnis:

- AppSec-Contract-/Presentation-Test: grün.
- Source-App-Validator: `ok: true`.
- Browser-QA:
  `output/playwright/business-os-appsec-r42-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün für Startmenü-Launch, Content-Mount, Header-Aktionen, Süd-/Corner-
  Resize, Maximieren/Wiederherstellen, Chat-Dock-Koexistenz und horizontales
  Overflow-Gate.
- Kombinierter AppSec+Notes+Knowledge+Shiftflow-Browserlauf mit Source-
  `app.css`:
  `output/playwright/business-os-appsec-notes-knowledge-shiftflow-r42-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.

Offen bleibt:

- Wie Revision 37, 39, 40 und 41 ist dieser Lauf Source-geroutet, weil Revision
  38 den echten lokalen Full-Install-E2E weiterhin als Host-/Release-Build-
  Ressourcenblocker festhält.

### Revision 43: Browser kompakt migriert am 2026-07-12

Revision 43 migriert `browser` als Remote-Browser-/Automation-Archetyp in den
Business-OS-Dichtevertrag. Die App war funktional bereits windowed, verschenkte
aber in Tabbar, Toolbar, Session-Tabs und Statusstrip zu viel vertikalen Raum
und hatte noch keinen App-eigenen Presentation-Guard.

Geändert wurde die Präsentationsschicht:

- Browser-Tabbar, Toolbar, Adresszeile, Auth-Assist, Notice und Command-Rows
  nutzen kompaktere Abstände und gemeinsame Shell-Radien.
- Der Start-Button bleibt als einzige prominente Browser-Automation sichtbar;
  Routine-Aktionen bleiben kompakt.
- Kleine App-Fenster erhalten einen `business-app-window`-Container-Breakpoint:
  Session-Tabs scrollen horizontal, die Adresszeile stapelt kontrolliert, und
  der Statusstrip vermeidet horizontales Überlaufen.
- `browser.test.mjs` liest jetzt zusätzlich `index.css` und `index.html` und
  sichert ab, dass keine Glass-Panes, breiten Side-Stripes, Literal-Shadows
  oder großen 10/12/14/16/18/20/24-px-Radien zurückkehren.

Evidence:

```bash
node src/apps/business-os/modules/browser/browser.test.mjs
/Users/michaelwelsch/.local/bin/ctox-real \
  business-os app validate browser --source --json
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=browser-r43-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS=browser \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/browser' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-browser-r43-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=browser-appsec-notes-knowledge-shiftflow-r43-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS='browser,appsec-pentest,notes,knowledge,shiftflow' \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/browser,modules/appsec-pentest,modules/notes,modules/knowledge,modules/shiftflow' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-browser-appsec-notes-knowledge-shiftflow-r43-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
```

Ergebnis:

- Browser-Contract-/Presentation-Test: grün.
- Source-App-Validator: `ok: true`.
- Browser-QA:
  `output/playwright/business-os-browser-r43-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün für Startmenü-Launch, Content-Mount, Header-Aktionen, Süd-/Corner-
  Resize, Maximieren/Wiederherstellen, Chat-Dock-Koexistenz und horizontales
  Overflow-Gate.
- Kombinierter Browser+AppSec+Notes+Knowledge+Shiftflow-Browserlauf mit
  Source-`app.css`:
  `output/playwright/business-os-browser-appsec-notes-knowledge-shiftflow-r43-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.

Offen bleibt:

- Wie Revision 37, 39, 40, 41 und 42 ist dieser Lauf Source-geroutet, weil
  Revision 38 den echten lokalen Full-Install-E2E weiterhin als Host-/Release-
  Build-Ressourcenblocker festhält.

### Revision 44: Reports als kompakte Workbench abgenommen am 2026-07-12

Revision 44 nimmt `reports` als Reports-/Rollback-Workbench erneut als
vollständig migrierte Einzel-App ab. Die visuelle Schicht war bereits kompakt
und ohne die alten Glass-/Shadow-/Side-Stripe-Patterns; fehlend war vor allem
ein App-eigener Testvertrag, der diese Präsentationsqualität dauerhaft
absichert.

Geändert wurde die Test-/Abnahmeschicht:

- `reports/test.mjs` liest jetzt zusätzlich `index.css` und `index.html`.
- Der Test sperrt historische Glass-Panes, breite Side-Stripes, Literal-
  Shadows und große 10/12/14/16/18/20/24-px-Radien.
- Der Test sichert die responsive Zwei-Spalten-Workbench mit Resizer und den
  schmalen `business-app-window`-Breakpoint ab.

Evidence:

```bash
node src/apps/business-os/modules/reports/test.mjs
/Users/michaelwelsch/.local/bin/ctox-real \
  business-os app validate reports --source --json
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=reports-r44-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS=reports \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/reports' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-reports-r44-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=reports-browser-appsec-notes-knowledge-shiftflow-r44-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS='reports,browser,appsec-pentest,notes,knowledge,shiftflow' \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/reports,modules/browser,modules/appsec-pentest,modules/notes,modules/knowledge,modules/shiftflow' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-reports-browser-appsec-notes-knowledge-shiftflow-r44-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
```

Ergebnis:

- Reports-Contract-/Presentation-Test: 7/7 grün.
- Source-App-Validator: `ok: true`.
- Browser-QA:
  `output/playwright/business-os-reports-r44-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün für Startmenü-Launch, Content-Mount, Header-Aktionen, Süd-/Corner-
  Resize, Maximieren/Wiederherstellen, Chat-Dock-Koexistenz und horizontales
  Overflow-Gate.
- Kombinierter Reports+Browser+AppSec+Notes+Knowledge+Shiftflow-Browserlauf
  mit Source-`app.css`:
  `output/playwright/business-os-reports-browser-appsec-notes-knowledge-shiftflow-r44-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.

Offen bleibt:

- Wie Revision 37, 39, 40, 41, 42 und 43 ist dieser Lauf Source-geroutet, weil
  Revision 38 den echten lokalen Full-Install-E2E weiterhin als Host-/Release-
  Build-Ressourcenblocker festhält.

### Revision 45: Support kompakt migriert am 2026-07-12

Revision 45 migriert `support` als Drei-Pane-Timeline-App weiter in den
Business-OS-Dichtevertrag. Die App bleibt fachlich dreispaltig, wird aber in
Queue, Timeline, Kontext und Composer enger an die kompakte Shell geführt.

Geändert wurde die Präsentations- und Testschicht:

- Queue-, Kontext-, Timeline- und Composer-Abstände wurden reduziert.
- Conversation- und Timeline-Karten nutzen gemeinsame `--control-radius`-
  Radien statt lokaler Pixelradien.
- Der Support-Smoke-Test liest jetzt `index.css` und `index.html` und sperrt
  historische Glass-Panes, breite Side-Stripes, Literal-Shadows und große
  10/12/14/16/18/20/24-px-Radien.
- Der Test sichert die 1180-/760-px-Container-Breakpoints der Drei-Pane-
  Responsivität ab.

Evidence:

```bash
node src/apps/business-os/modules/support/tests/support.test.mjs
/Users/michaelwelsch/.local/bin/ctox-real \
  business-os app validate support --source --json
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=support-r45-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS=support \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/support' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-support-r45-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=support-reports-browser-r45-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS='support,reports,browser' \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/support,modules/reports,modules/browser' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-support-reports-browser-r45-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
```

Ergebnis:

- Support-Contract-/Presentation-Test: grün.
- Source-App-Validator: `ok: true`.
- Browser-QA:
  `output/playwright/business-os-support-r45-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün für Startmenü-Launch, Content-Mount, Header-Aktionen, Süd-/Corner-
  Resize, Maximieren/Wiederherstellen, Chat-Dock-Koexistenz und horizontales
  Overflow-Gate.
- Kombinierter Support+Reports+Browser-Browserlauf mit Source-`app.css`:
  `output/playwright/business-os-support-reports-browser-r45-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.

Separater Befund:

- Ein längerer 7-App-Batch
  `output/playwright/business-os-support-reports-browser-appsec-notes-knowledge-shiftflow-r45-source-route-shell-css/business-os-interactive-window-qa.json`
  lief für AppSec, Browser, Knowledge, Notes, Reports und Shiftflow grün, endete
  aber bei Support mit einem `waitForAppContent`-Timeout. Das ist als
  Langbatch-/Mount-Stabilitätssignal weiter zu prüfen; die isolierte Support-
  App-Abnahme und der kleine Cross-App-Batch sind grün.

Offen bleibt:

- Wie Revision 37, 39, 40, 41, 42, 43 und 44 ist dieser Lauf Source-geroutet,
  weil Revision 38 den echten lokalen Full-Install-E2E weiterhin als Host-/
  Release-Build-Ressourcenblocker festhält.

### Revision 46: App Store kompakt abgenommen am 2026-07-12

Revision 46 nimmt `app-store` als Store-/Release-Workbench vollständig visuell
ab. Die App war bereits weitgehend auf dem Kit; offen waren lokale Radien und
ein QA-störender externer Discovery-Start beim Mount.

Geändert wurde die Präsentations-, Test- und Mount-Schicht:

- Version-Timeline, Loading-Overlay und Release-Data-Rows nutzen
  `--app-store-panel-radius` statt lokaler 8/10-px-Radien.
- `app-store.test.mjs` liest jetzt `index.css` und `index.html` und sperrt
  Glass-Panes, breite Side-Stripes, Literal-Shadows und große lokale Radien.
- GitHub Discovery startet nicht mehr automatisch beim Mount. Die App zeigt
  projizierte/lokale Katalogdaten sofort; externe Discovery bleibt explizit
  über `data-refresh-marketplace` erreichbar.

Evidence:

```bash
node src/apps/business-os/modules/app-store/app-store.test.mjs
/Users/michaelwelsch/.local/bin/ctox-real \
  business-os app validate app-store --source --json
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=app-store-r46-source-route-shell-css-v2' \
BUSINESS_OS_INTERACTIVE_APP_IDS=app-store \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/app-store' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-app-store-r46-source-route-shell-css-v2 \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=app-store-support-reports-browser-r46-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS='app-store,support,reports,browser' \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/app-store,modules/support,modules/reports,modules/browser' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-app-store-support-reports-browser-r46-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
```

Ergebnis:

- App-Store-Contract-/Presentation-Test: 22/22 grün.
- Source-App-Validator: `ok: true`.
- Browser-QA:
  `output/playwright/business-os-app-store-r46-source-route-shell-css-v2/business-os-interactive-window-qa.json`
  ist grün.
- Kombinierter App-Store+Support+Reports+Browser-Lauf:
  `output/playwright/business-os-app-store-support-reports-browser-r46-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.

Offen bleibt:

- Wie Revision 37, 39, 40, 41, 42, 43, 44 und 45 ist dieser Lauf Source-
  geroutet, weil Revision 38 den echten lokalen Full-Install-E2E weiterhin als
  Host-/Release-Build-Ressourcenblocker festhält.

### Revision 47: Tickets kompakt abgenommen am 2026-07-12

Revision 47 nimmt `tickets` als Command-Bus-Referenz und Drei-Pane-Workbench
vollständig visuell ab. Die App behält linke Ticketliste, zentrales Detail und
rechte Operationskontext-Spalte, nutzt aber keine lokalen Kartenradien mehr.

Geändert wurde die Präsentations- und Testschicht:

- Ticket-Zeilen sowie Timeline-/Kontext-Karten nutzen `--control-radius`.
- `tickets.test.mjs` liest jetzt `index.css` und `index.html` und sperrt
  Glass-Panes, breite Side-Stripes, Literal-Shadows und lokale 8/10/12/14/16/
  18/20/24-px-Radien.
- Der Test sichert linke und rechte Resizer sowie die 1160-/640-px-Container-
  Breakpoints ab.

Evidence:

```bash
node src/apps/business-os/modules/tickets/tickets.test.mjs
/Users/michaelwelsch/.local/bin/ctox-real \
  business-os app validate tickets --source --json
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=tickets-r47-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS=tickets \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/tickets' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-tickets-r47-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=tickets-app-store-support-reports-browser-r47-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS='tickets,app-store,support,reports,browser' \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/tickets,modules/app-store,modules/support,modules/reports,modules/browser' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-tickets-app-store-support-reports-browser-r47-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
```

Ergebnis:

- Tickets-Contract-/Presentation-Test: grün.
- Source-App-Validator: `ok: true`.
- Browser-QA:
  `output/playwright/business-os-tickets-r47-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.
- Kombinierter Tickets+App-Store+Support+Reports+Browser-Lauf:
  `output/playwright/business-os-tickets-app-store-support-reports-browser-r47-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.

Offen bleibt:

- Wie Revision 37, 39, 40, 41, 42, 43, 44, 45 und 46 ist dieser Lauf Source-
  geroutet, weil Revision 38 den echten lokalen Full-Install-E2E weiterhin als
  Host-/Release-Build-Ressourcenblocker festhält.

### Revision 48: Creator kompakt migriert am 2026-07-12

Revision 48 migriert `creator` als App-Erstellungs-Workbench weiter in den
Business-OS-Dichtevertrag. Die App bleibt dreispaltig und resizable, verliert
aber die alte Glass-/Glow-/Panel-Optik.

Geändert wurde die Präsentations- und Testschicht:

- Linke, mittlere und rechte Spalte nutzen flache Shell-Surfaces,
  `--surface-radius` und keine Panel-Shadows.
- Glassmorphism (`backdrop-filter`, `--glass-*`) wurde aus der App-Oberfläche
  entfernt.
- Lokale 8/10/16-px-Radien und Sync-Glow-Shadows wurden entfernt.
- Routinebereiche wurden dichter: Sidebar, Center-Header/-Body, rechter Rail,
  Accordion, Collections-Editor und Status-Footer.
- `creator.test.mjs` liest jetzt `index.css` und `index.html` und sperrt Glass,
  Backdrop-Filter, breite Side-Stripes, Literal-Shadows, Panel-Shadows und
  lokale Großradien. Außerdem werden linke/rechte Resizer und 980-/560-px-
  Container-Breakpoints abgesichert.

Evidence:

```bash
node src/apps/business-os/modules/creator/creator.test.mjs
/Users/michaelwelsch/.local/bin/ctox-real \
  business-os app validate creator --source --json
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=creator-r48-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS=creator \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/creator' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-creator-r48-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=creator-tickets-app-store-support-reports-browser-r48-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS='creator,tickets,app-store,support,reports,browser' \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/creator,modules/tickets,modules/app-store,modules/support,modules/reports,modules/browser' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-creator-tickets-app-store-support-reports-browser-r48-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
```

Ergebnis:

- Creator-Contract-/Presentation-Test: 10/10 grün.
- Source-App-Validator: `ok: true`.
- Browser-QA:
  `output/playwright/business-os-creator-r48-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.
- Kombinierter Creator+Tickets+App-Store+Support+Reports+Browser-Lauf:
  `output/playwright/business-os-creator-tickets-app-store-support-reports-browser-r48-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.

Offen bleibt:

- Wie Revision 37, 39, 40, 41, 42, 43, 44, 45, 46 und 47 ist dieser Lauf
  Source-geroutet, weil Revision 38 den echten lokalen Full-Install-E2E
  weiterhin als Host-/Release-Build-Ressourcenblocker festhält.

### Revision 49: Research kompakt migriert am 2026-07-12

Revision 49 migriert `research` als Quellen-/Research-Workbench weiter in den
Business-OS-Dichtevertrag. Die fachliche Map-, Quellen- und Report-Funktion
bleibt erhalten, die alte dekorative Schatten-/Gradient-/Inline-Style-Schicht
wird entfernt.

Geändert wurde die Präsentations- und Testschicht:

- Portfolio-Map und Discovery-Graph behalten ihre Funktion, nutzen aber keine
  dekorativen Shadow-/Glow-Effekte mehr.
- Das Map-Grid nutzt eine flache Surface-Füllung statt mehrerer
  Gradient-Layer.
- Research-Notizen nutzen vollständige Border plus Accent-Tint statt
  Side-Stripe.
- Source-Cards, Empty Panels, Markdown-Codeblöcke und Context-Menü nutzen
  Shell-Radien und keine App-eigenen Workspace-Shadows.
- Der Report-Viewer ersetzt Inline-Styles durch Klassen
  (`research-report-loading`, `research-ai-banner-*`, `research-ai-prompt-*`).
- `research/test.mjs` sperrt Glass, breite Side-Stripes, lokale Großradien,
  Literal-Shadows und Gradient-Layer und sichert die 6px-Resizer-Spalten sowie
  Report-Viewer-Klassen ab.

Evidence:

```bash
node src/apps/business-os/modules/research/test.mjs
/Users/michaelwelsch/.local/bin/ctox-real \
  business-os app validate research --source --json
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=research-r49-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS=research \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/research' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-research-r49-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=research-creator-tickets-app-store-support-browser-r49-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS='research,creator,tickets,app-store,support,browser' \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/research,modules/creator,modules/tickets,modules/app-store,modules/support,modules/browser' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-research-creator-tickets-app-store-support-browser-r49-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
```

Ergebnis:

- Research-Contract-/Presentation-Test: 9/9 grün.
- Source-App-Validator: `ok: true`.
- Browser-QA:
  `output/playwright/business-os-research-r49-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.
- Kombinierter Research+Creator+Tickets+App-Store+Support+Browser-Lauf:
  `output/playwright/business-os-research-creator-tickets-app-store-support-browser-r49-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.

Offen bleibt:

- Wie Revision 37, 39, 40, 41, 42, 43, 44, 45, 46, 47 und 48 ist dieser Lauf
  Source-geroutet, weil Revision 38 den echten lokalen Full-Install-E2E
  weiterhin als Host-/Release-Build-Ressourcenblocker festhält.

### Revision 50: Outbound kompakt migriert am 2026-07-12

Revision 50 migriert `outbound` als Sales-/Automation-Workbench weiter in den
Business-OS-Dichtevertrag. Import, Kampagnen-Setup, Research-Quellen,
Mailserver-Konfiguration und Pipeline-Übernahme bleiben fachlich erhalten, die
alte dekorative Präsentationsschicht wird entfernt.

Geändert wurde die Präsentations- und Testschicht:

- Workbench- und Overlay-Flächen nutzen keine dekorativen Shell-Shadows mehr.
- Der Funnel-/Workbench-Resizer bleibt eine 6px-Shell-Spalte statt einer
  sichtbaren Layoutbarriere.
- Mailserver-Domain- und DNS-Blöcke nutzen Klassen statt isolierter
  Inline-Flächen; Eingaben folgen gemeinsamen Control-Radien.
- Harte `#000`-Kontraste in Aktionsbuttons werden durch themefähige Shell-
  Tokens ersetzt.
- DNS-Record-Codeblöcke nutzen Surface-, Border- und Accent-Tokens statt
  lokaler schwarzer Codeflächen.
- `outbound.test.mjs` enthält jetzt einen Presentation-Guard gegen Glass,
  Premium-Wording, breite Side-Stripes, lokale Großradien, Literal-Shadows und
  Gradient-Layer und sichert die 6px-Resizable-Shell-Spalte sowie die
  Mailserver-Klassen ab.

Evidence:

```bash
node src/apps/business-os/modules/outbound/outbound.test.mjs
node src/apps/business-os/modules/outbound/core/audience.test.mjs
/Users/michaelwelsch/.local/bin/ctox-real \
  business-os app validate outbound --source --json
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=outbound-r50-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS=outbound \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/outbound' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-outbound-r50-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=outbound-research-creator-tickets-app-store-browser-r50-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS='outbound,research,creator,tickets,app-store,browser' \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/outbound,modules/research,modules/creator,modules/tickets,modules/app-store,modules/browser' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-outbound-research-creator-tickets-app-store-browser-r50-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
```

Ergebnis:

- Outbound-Contract-/Presentation-Test: 14/14 grün, 1 optionaler XLSX-Fixture-
  Test übersprungen.
- Audience-Core-Test: 4/4 grün.
- Source-App-Validator: `ok: true`.
- Browser-QA:
  `output/playwright/business-os-outbound-r50-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.
- Kombinierter Outbound+Research+Creator+Tickets+App-Store+Browser-Lauf:
  `output/playwright/business-os-outbound-research-creator-tickets-app-store-browser-r50-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.

Offen bleibt:

- Wie Revision 37, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48 und 49 ist dieser
  Lauf Source-geroutet, weil Revision 38 den echten lokalen Full-Install-E2E
  weiterhin als Host-/Release-Build-Ressourcenblocker festhält.

### Revision 51: Coding Agents kompakt migriert am 2026-07-12

Revision 51 migriert `coding-agents` als Provider-/Run-Workbench weiter in den
Business-OS-Dichtevertrag. Providerwechsel, Workspace Grants, Sessions,
Diagnostik, Lifecycle-Control und Auth-Logs bleiben erhalten, die alte
Sci-Fi-/Glow-Präsentationsschicht wird entfernt.

Geändert wurde die Präsentations- und Testschicht:

- Der App-Root nutzt eine 6px-Resizable-Shell-Spalte statt der alten 12px-
  Layoutbarriere.
- Mesh-/Glow-Hintergründe und dekorative Glass-Panes werden entfernt.
- Provider-Logos nutzen flache Shell-Akzentflächen statt Gradients und Glow-
  Shadows.
- Statuspunkte, Workspace-Selection, Terminalcontainer und Detailflächen nutzen
  Border/Surface-Zustände statt Schatten.
- Statische `index.html`-Flächen für Lifecycle-Status und Browser-Logs nutzen
  Klassen (`lifecycle-status-row`, `browser-log-box`) statt harter Inline-
  Radien und `#000/#fff`-Terminalflächen.
- `coding-agents.test.mjs` enthält jetzt einen Presentation-Guard gegen Glass,
  Premium-Wording, breite Side-Stripes, lokale Großradien, Literal-Shadows,
  Gradient-Layer und harte Schwarz/Weiß-Kontraste und sichert die 6px-Resizable-
  Shell-Spalte sowie die neuen statischen Klassen ab.

Evidence:

```bash
node src/apps/business-os/modules/coding-agents/tests/coding-agents.test.mjs
/Users/michaelwelsch/.local/bin/ctox-real \
  business-os app validate coding-agents --source --json
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=coding-agents-r51-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS=coding-agents \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/coding-agents' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-coding-agents-r51-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=coding-agents-outbound-research-creator-tickets-browser-r51-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS='coding-agents,outbound,research,creator,tickets,browser' \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/coding-agents,modules/outbound,modules/research,modules/creator,modules/tickets,modules/browser' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-coding-agents-outbound-research-creator-tickets-browser-r51-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
```

Ergebnis:

- Coding-Agents-Contract-/Presentation-Test: 11/11 grün.
- Source-App-Validator: `ok: true`.
- Browser-QA:
  `output/playwright/business-os-coding-agents-r51-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.
- Kombinierter Coding-Agents+Outbound+Research+Creator+Tickets+Browser-Lauf:
  `output/playwright/business-os-coding-agents-outbound-research-creator-tickets-browser-r51-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.

Offen bleibt:

- Wie Revision 37, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49 und 50 ist
  dieser Lauf Source-geroutet, weil Revision 38 den echten lokalen
  Full-Install-E2E weiterhin als Host-/Release-Build-Ressourcenblocker
  festhält.

### Revision 52: CTOX kompakt migriert am 2026-07-12

Revision 52 migriert `ctox` als Control-Plane-/Run-Workbench weiter in den
Business-OS-Dichtevertrag. Runtime-Status, Queue, Runs, Flow-Canvas,
Timeline, Kontextmenü, Task-Edit und Drawer bleiben fachlich erhalten, die alte
dekorative Präsentationsschicht wird entfernt.

Geändert wurde die Präsentations- und Testschicht:

- App-, Control-, Drawer-, Canvas- und Timeline-Flächen nutzen flache
  Shell-Surfaces statt Gradients.
- Die linke Workbench-Spalte nutzt eine 6px-Resizable-Shell-Spalte statt der
  alten 10/12px-Resizer-Geometrie.
- Pulse-, Status-, Range-, Context-Chat- und Drawer-Step-Shadows werden durch
  Border-, Surface- und Opacity-Zustände ersetzt.
- Lokale 8/10/12/14px-Radien werden auf Shell-Radius-Tokens oder echte Pill-
  Tokens zurückgeführt.
- Das Work-Section-Chevron nutzt 1px-Borders statt breiter Side-Border-
  Akzente.
- `icon.svg` und `module.json` verwenden ein schlichtes `currentColor`-Icon
  ohne SVG-Gradient und ohne harten Weiß-Stroke.
- `ctox/test.js` enthält jetzt einen Presentation-Guard gegen Glass,
  Premium-Wording, breite Side-Stripes, lokale Großradien, Literal-Shadows,
  Gradient-Layer und harte Schwarz/Weiß-Kontraste und sichert die 6px-Resizable-
  Shell-Spalte sowie das manifestierte `currentColor`-Icon ab.

Evidence:

```bash
node src/apps/business-os/modules/ctox/tests/ctox.test.mjs
/Users/michaelwelsch/.local/bin/ctox-real \
  business-os app validate ctox --source --json
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=ctox-r52-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS=ctox \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/ctox' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-ctox-r52-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=ctox-coding-agents-outbound-research-creator-tickets-r52-source-route-shell-css' \
BUSINESS_OS_INTERACTIVE_APP_IDS='ctox,coding-agents,outbound,research,creator,tickets' \
BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES='app.css,modules/ctox,modules/coding-agents,modules/outbound,modules/research,modules/creator,modules/tickets' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-ctox-coding-agents-outbound-research-creator-tickets-r52-source-route-shell-css \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
```

Ergebnis:

- CTOX-Contract-/Presentation-Test: 10/10 grün.
- Source-App-Validator: `ok: true`.
- Browser-QA:
  `output/playwright/business-os-ctox-r52-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.
- Kombinierter CTOX+Coding-Agents+Outbound+Research+Creator+Tickets-Lauf:
  `output/playwright/business-os-ctox-coding-agents-outbound-research-creator-tickets-r52-source-route-shell-css/business-os-interactive-window-qa.json`
  ist grün.

Offen bleibt:

- Wie Revision 37, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50 und 51 ist
  dieser Lauf Source-geroutet, weil Revision 38 den echten lokalen
  Full-Install-E2E weiterhin als Host-/Release-Build-Ressourcenblocker
  festhält.
- Die Einzel-App-Modernisierung steht damit bei 34/34. Releaseclaim,
  installierter Snapshot und organisatorische Signoffs bleiben getrennte Gates.

### Revision 53: Managed-Install-E2E teilweise geschlossen, Runtime-Lifecycle-Gate rot am 2026-07-12

Revision 53 wiederholt den Release-/Installer-Track nach Abschluss der 34/34
App-Modernisierung. Der frühere Host-Ressourcenblocker aus Revision 36/38 ist
für den reinen Release-Build nicht mehr reproduziert: sowohl der Source-
Rebuild als auch der reale Managed-Install-Pfad bauen mit optionalem
Runtime-Skip und einem Cargo-Job bis zur neuen Release durch.

Evidence:

```bash
CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS=1 CARGO_BUILD_JOBS=1 \
  CTOX_BUSINESS_OS_AUTOSTART=0 \
  ./install.sh --rebuild 2>&1 | tee output/install-r53-full-e2e.log

CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS=1 CARGO_BUILD_JOBS=1 \
  BUSINESS_OS_AUTOSTART=0 \
  ./install.sh 2>&1 | tee output/install-r53-managed-e2e.log

node src/apps/business-os/scripts/assert-managed-business-os-assets.mjs \
  --url http://127.0.0.1:8765 \
  --json > output/install-r53-managed-asset-guard.json

BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?qa=r53-installed-full' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR=output/playwright/business-os-r53-installed-full \
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
```

Ergebnis:

- `install.sh --rebuild`: Release-Build grün nach ca. 39 Minuten; optionaler
  Desktop-/Qwen-GGML-Runtime-Pfad wurde erwartungsgemäß übersprungen.
- Managed-Install: Release-Build grün nach ca. 51 Minuten; `current` zeigt
  danach auf
  `/Users/michaelwelsch/.local/lib/ctox/releases/v0.3.31-381-g4d1cc9e32-dirty`.
- `/Users/michaelwelsch/.local/lib/ctox/install_manifest.json` nennt
  `current_release: v0.3.31-381-g4d1cc9e32-dirty` und
  `previous_release: branch-main-20260711T203418Z`.
- Managed-Asset-Guard:
  `output/install-r53-managed-asset-guard.json` ist grün, prüft 12 kritische
  Assets, erhält runtime-installierte Module als Tenant-State und bestätigt
  denselben `APP_BUILD` über `http://127.0.0.1:8765/business-os/app.js`.
- Installierter Browser-QA:
  `output/playwright/business-os-r53-installed-full/business-os-interactive-window-qa.json`
  öffnet, resizt und schließt 35/35 Window-Apps erfolgreich. Keine App liefert
  ein eigenes Window-/Header-/Resize-/Mobile-Finding.

Release-Gate bleibt rot:

- Der Managed-Installer kehrt nicht sauber zurück, weil
  `ctox-real update channel set-github --repo metric-space-ai/ctox` im finalen
  Schritt hängen bleibt.
- Als Sofortfix wurde `install.sh` so geändert, dass dieser optionale
  Update-Channel-Finalizer per `run_optional_soft_timeout` nach 15 Sekunden
  gelöst wird und den Installer nicht mehr blockiert. Syntax-/Whitespace-Check:
  `bash -n install.sh` und `git diff --check -- install.sh docs/business-os-app-platform-refactoring-plan.md`
  sind grün.
- Mehrere lokale `ctox-real`-Aufrufe bleiben im unkillbaren `UE`-Status:
  `update channel set-github`, `version`, `business-os rxdb status --json` und
  ein alter `service --foreground`.
- Der erste erreichbare Service auf Port 8765 lief nach dem Install zunächst
  noch mit altem realen CWD
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260711T203418Z`,
  obwohl Wrapper, launchd-Plist und `current` bereits auf die neue Release
  zeigen.
- Der volle installierte Browser-QA fällt ausschließlich wegen sieben
  WebSocket-Console-Errors auf `ws://127.0.0.1:20876` mit
  `ERR_CONNECTION_REFUSED` durch; `lsof` zeigt keinen Listener auf Port 20876.
- Nach `launchctl kickstart -k gui/$(id -u)/com.metric-space.ctox.service`
  bleibt launchd auf `spawn scheduled`; anschließend ist kein stabiler
  Business-OS-/MCP-/RxDB-Port-Nachweis mehr offen.

Damit ist der alte Build-/Asset-Snapshot-Blocker präzisiert, aber nicht der
Release-Lifecycle geschlossen. Nächster technischer Schritt ist nicht weitere
App-Optik, sondern eine gezielte Runtime-Lifecycle-Diagnose: warum bleiben
`ctox-real`-Kommandos im `UE`-Status, welcher Prozess hält den native
RxDB-Peer-Lock, warum startet launchd nach Symlink-Switch nicht sauber aus
`current`, und warum wird Port 20876 nicht stabil bereitgestellt.

### Revision 54: RxDB-Statusdiagnostik fail-fast gehärtet am 2026-07-12

Revision 54 behebt einen direkten Folgeschaden aus Revision 53: ein
diagnostischer `ctox business-os rxdb status --json` darf nicht selbst lange
SQLite-/Secret-/Runtime-Env-Probes ausführen, wenn bereits klar ist, dass ein
nativer Peer-Lock ohne frische Heartbeat-Datei vorliegt. Genau dieser Zustand
ist der wedged/Crash-Recovery-Fall; Status muss ihn melden, nicht weiter
blockieren.

Geändert:

- `src/core/business_os/rxdb_peer.rs` erkennt
  `process_lock_held && !heartbeat_fresh && !in_process_running` als
  `native_peer_lock_without_fresh_heartbeat`.
- In diesem Zustand wird `turn_readiness` als diagnostisch übersprungen
  markiert, statt Secrets/Runtime-Env zu lesen.
- `command_plane_status` erhält denselben Skip-Pfad und öffnet die RxDB-SQLite
  nicht.
- Für den normalen Command-Plane-Status gilt ein kurzer
  `NATIVE_PEER_STATUS_SQLITE_BUSY_TIMEOUT_MS = 250`, nicht der globale
  30-Sekunden-Busy-Timeout.
- Ein Guard-Test sichert den Skip-Pfad ab.

Evidence:

```bash
cargo fmt --check -- src/core/business_os/rxdb_peer.rs
CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS=1 CARGO_BUILD_JOBS=1 \
  cargo test command_plane_status_skips_sqlite_when_peer_lock_is_stale --bin ctox
python3 - <<'PY'
import subprocess, time
cmd=[
  '/private/tmp/ctox-sync95-clean/runtime/build/sync95-target/debug/ctox',
  'business-os', 'rxdb', 'status', '--json'
]
start=time.time()
out=subprocess.check_output(cmd, stderr=subprocess.STDOUT, timeout=5,
                            cwd='/Users/michaelwelsch/Documents/ctox.nosync')
print('exit=0 elapsed=%.2f bytes=%d' % (time.time()-start, len(out)))
PY
```

Ergebnis:

- Format-Check grün.
- Targeted Rust-Test grün:
  `business_os::rxdb_peer::tests::command_plane_status_skips_sqlite_when_peer_lock_is_stale`.
- Der Debug-Binary-Statuspfad kehrt mit hartem 5s-Python-Timeout nach ca.
  4.26s zurück und meldet `native_peer_not_running`, statt zu hängen.

Offen bleibt:

- Die installierte `ctox-real`-Binary ist dadurch noch nicht neu gebaut und
  nicht neu installiert.
- Die alten lokalen `UE`-Prozesse sind weiterhin nicht aus Userspace
  beendbar und blockieren den echten launchd-/Port-20876-Nachweis.
- Revision 54 ist deshalb ein Diagnose-/Recovery-Fix, kein geschlossener
  Release-E2E.

### Revision 55: isolierter Browser/Rust-Command-Bus-Smoke grün, aktueller Source-Build offen am 2026-07-12

Revision 55 prüft den Runtime-Codepfad ohne den bekannten installierten
launchd-/`ctox-real`-Hostzustand aus Revision 53. Dafür wurde der Smoke-Harness
auf separaten Ports gestartet und das neueste vorhandene Debug-Binary
verwendet:

```bash
CTOX_BIN=runtime/build/threads-production-target/debug/ctox \
SMOKE_MODE=command-browser-to-rust \
SMOKE_PAGE_PATH=/index.html \
BUSINESS_PORT=9180 \
SIGNALING_PORT=19180 \
SMOKE_SERVER_READY_TIMEOUT_MS=90000 \
SMOKE_SYNC_CONFIG_WAIT_MS=60000 \
node src/core/rxdb/tools/browser_rust_smoke.js
```

Evidence liegt unter:

- `output/rev55-command-browser-to-rust-smoke.log`

Ergebnis:

- Business OS startet isoliert auf `http://127.0.0.1:9180`.
- Signaling läuft isoliert auf `ws://127.0.0.1:19180`.
- WebRTC-Replikation meldet `replication up` für 175 Collections.
- Der Browser schreibt einen `business_commands`-Datensatz.
- Rust materialisiert daraus einen Queue-Task:
  `task_count_for_command=1`, `status=accepted`, `task_status=queued`.
- Browser-Fehler bleiben bei null:
  `browser_warning_count=0`, `browser_websocket_warning_count=0`,
  `browser_error_count=0`, `browser_request_failure_count=0`.

Einschränkung:

- Der Nachweis nutzt `runtime/build/threads-production-target/debug/ctox`
  vom 2026-07-12 01:52 und nicht ein frisch aus dem aktuellen Worktree gebautes
  Binary.
- Drei aktuelle Build-Versuche gegen
  `runtime/build/core-rxdb-integration-target` blieben reproduzierbar still bei
  `ctox-core` stehen und erzeugten kein
  `runtime/build/core-rxdb-integration-target/debug/ctox`.
- Das Build-Log des piped Versuchs liegt unter
  `output/rev55-source-ctox-build.log` und endet bei
  `Compiling ctox-core`.
- Revision 55 beweist deshalb: der isolierte Browser/Rust/WebRTC-/Command-
  Bus-Codepfad funktioniert grundsätzlich. Sie beweist nicht: die installierte
  Release-Binary enthält Revision 54 oder der launchd-Service startet sauber
  aus `current`.

Offen bleibt:

- Den `ctox-core`-Build-Stillstand im separaten Target entweder als
  Tool-/PTY-Artefakt erklären oder mit verwertbarer Rustc-/Cargo-Diagnose
  auflösen.
- Danach die installierte Binary neu bauen/installieren und den Status-,
  Port-20876-, Native-Peer- und Browser-QA-Release-Nachweis erneut führen.

### Revision 56: installierter launchd-Blocker auf macOS-Codesigning-/Provenance-Spur eingegrenzt am 2026-07-12

Revision 56 prüft den installierten Hostzustand nach dem grünen isolierten
Smoke. Der laufende Release-Blocker bleibt reproduzierbar hostseitig:

```bash
ps -axo pid,ppid,stat,%cpu,%mem,etime,command | rg 'ctox-real|ctox service'
lsof -nP -iTCP:8765 -iTCP:8788 -iTCP:20876 -sTCP:LISTEN
launchctl print gui/$(id -u)/com.metric-space.ctox.service
codesign --verify --strict --verbose=4 ~/.local/lib/ctox/current/bin/ctox-real
spctl --assess --type execute --verbose=4 ~/.local/bin/ctox
spctl --assess --type execute --verbose=4 ~/.local/lib/ctox/current/bin/ctox-real
```

Ergebnis:

- Mehrere alte installierte `ctox-real`-Prozesse hängen weiter in `UE`,
  darunter `service --foreground`, `version`, `business-os rxdb status --json`
  und alte Browser-Automation-Aufrufe.
- Auf 8765, 8788 und 20876 lauscht kein stabiler installierter Prozess.
- `launchctl print` meldet den Service als `state = spawn scheduled` mit
  `runs = 378` und `last exit reason = OS_REASON_CODESIGNING`.
- `codesign --verify --strict` ist für `ctox-real` zwar gültig.
- Gatekeeper lehnt die Ausführung trotzdem ab:
  `~/.local/bin/ctox: rejected`, `source=no usable signature`, und
  `~/.local/lib/ctox/current/bin/ctox-real: rejected`.
- Launcher und Real-Binary tragen `com.apple.provenance`; ein temporärer
  Kopier-/Cleanup-Test konnte diese Provenance nicht entfernen und änderte das
  `spctl`-Ergebnis nicht.
- Das Service-Log enthält zusätzlich alte Bind-Konflikte und native
  RxDB-Peer-Lock-Meldungen. Diese erklären die früheren Port-/Lock-Symptome,
  ersetzen aber nicht den aktuellen launchd-Codesigning-Befund.

Folgerung:

- Der isolierte Source-/Debug-Smoke aus Revision 55 beweist, dass WebRTC und
  Command-Bus grundsätzlich funktionieren.
- Der installierte Release-E2E ist durch den lokalen macOS-LaunchAgent-/
  Gatekeeper-/Provenance-Zustand blockiert. Vor weiterem App- oder
  Release-Claim muss der Installer/LaunchAgent-Pfad so angepasst oder
  dokumentiert werden, dass der Dienst aus `current` ohne `OS_REASON_CODESIGNING`
  startet und keine alten `UE`-Prozesse mehr den Peer-/Port-Zustand verfälschen.

### Revision 57: installierter macOS-Service, lokales Signaling und 35-App-Browser-QA grün am 2026-07-12

Revision 57 repariert den lokalen installierten macOS-Pfad so weit, dass die
Business-OS-Shell wieder als echte installierte Webapp bedienbar ist:

- Der macOS-Service-LaunchAgent startet den Shell-Wrapper nicht mehr als
  direktes `Program`, sondern über `/bin/bash <wrapper> service --foreground`.
  Dadurch bleibt das Wrapper-Sourcing erhalten, aber launchd muss nicht das
  unsignierte Shell-Script selbst als ausführbares Programm behandeln.
- `install.sh` installiert zusätzlich
  `com.metric-space.ctox.signaling`, wenn Node und
  `src/core/rxdb/tools/local_signaling_server.js` im Release vorhanden sind.
  Der Agent startet den lokalen Signaling-Server auf dem persistierten
  lokalen Standardport 20876.
- `src/core/install/mod.rs` erzeugt denselben Service-/Signaling-Vertrag für
  native Install-/Update-Pfade und besitzt Guard-Tests für beide macOS-
  LaunchAgents.
- Die installierte `ctox-real`-Kopie hing trotz identischem SHA-256 gegenüber
  dem funktionierenden Release-Artefakt bereits bei `--help`. Eine atomare
  Erneuerung beider installierter Kopien aus
  `runtime/build/cargo-target/release/ctox` beseitigte den lokalen
  Inode-/Exec-Hänger; danach kehren `ctox-real --help` und der Wrapper wieder
  in ca. 1.6 s bzw. 0.14 s zurück.

Evidence:

```bash
bash -n install.sh
cargo fmt --check -- src/core/install/mod.rs
git diff --check -- install.sh src/core/install/mod.rs
CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS=1 CARGO_BUILD_JOBS=1 \
  cargo test refresh_launchd --bin ctox
BUSINESS_OS_INTERACTIVE_HEADLESS=1 \
BUSINESS_OS_INTERACTIVE_URL='http://127.0.0.1:8765/?rxdbSmoke=1' \
BUSINESS_OS_INTERACTIVE_OUTPUT_DIR='output/playwright/rev57-installed-launchd-signaling-full-rerun' \
  node src/apps/business-os/scripts/business-os-interactive-window-qa.mjs
node src/apps/business-os/scripts/assert-managed-business-os-assets.mjs \
  --url http://127.0.0.1:8765 --json \
  > output/rev57-managed-asset-guard.json
```

Ergebnis:

- `launchctl print gui/$(id -u)/com.metric-space.ctox.signaling`:
  `state = running`, `program = /opt/homebrew/bin/node`, Port 20876 lauscht.
- `launchctl print gui/$(id -u)/com.metric-space.ctox.service`:
  `state = running`, `program = /bin/bash`, PID aktiv.
- `lsof` zeigt installierte Listener auf 8765, 8788 und 20876.
- `output/playwright/rev57-installed-launchd-signaling-small/`:
  `ctox`, `threads` und `tickets` öffnen/resizen/schließen grün.
- `output/playwright/rev57-installed-launchd-signaling-full-rerun/business-os-interactive-window-qa.json`:
  `ok=true`, 35 Apps, keine Failures, keine Console Events.
- `output/rev57-installed-rxdb-status-after-full-browser.json`:
  `running=true`, `replicationUp=true`, frischer Heartbeat,
  `health_errors=[]`, 13 Critical Tasks.
- `output/rev57-managed-asset-guard.json`:
  `ok=true`, 12 kritische Assets bytegleich zwischen Source, Managed State und
  aktuellem Release-Root; HTTP-served `app.js` trägt denselben Build-Stamp.

Einschränkung:

- Alte lokale `ctox-real`-Prozesse im macOS-Status `UE` bleiben weiterhin im
  Prozessbaum und sind nicht aus Userspace beendet. Sie blockieren den aktuell
  laufenden installierten Dienst nicht mehr, sind aber weiter ein Host-
  Recovery-Befund.
- Die installierte Runtime wurde durch Plist-/Binary-Reparatur und laufende
  Source-Änderungen stabilisiert. Für einen sauberen Releaseclaim muss der
  nächste Managed-Install/Upgrade die neuen LaunchAgent-Generatoren selbst
  ausrollen und danach dieselben Gates erneut bestehen.

### Revision 58: Signature-Resttrack korrigiert und 34-App-Fachtests erneuert am 2026-07-12

Revision 58 korrigiert einen veralteten aktuellen Planstatus: Die in Revision
24 noch teilweise migrierten Signature-Flächen von Creator, Research, Outbound
und Coding Agents wurden bereits in Revision 48 bis 51 vollständig einzeln
migriert. Revision 57 hat alle vier zusätzlich im installierten 35-App-Lauf
regressionsgeprüft. Sie sind deshalb kein offenes pauschales Design-Todo mehr.

Aktuelle Source-Evidence:

```bash
node src/apps/business-os/modules/creator/creator.test.mjs
node src/apps/business-os/modules/research/test.mjs
node src/apps/business-os/modules/outbound/outbound.test.mjs
node src/apps/business-os/modules/outbound/core/audience.test.mjs
node src/apps/business-os/modules/coding-agents/tests/coding-agents.test.mjs
/Users/michaelwelsch/.local/bin/ctox-real business-os app validate creator --source --json
/Users/michaelwelsch/.local/bin/ctox-real business-os app validate research --source --json
/Users/michaelwelsch/.local/bin/ctox-real business-os app validate outbound --source --json
/Users/michaelwelsch/.local/bin/ctox-real business-os app validate coding-agents --source --json
BUSINESS_OS_APP_STORY_TEST_REPORT=output/playwright/rev58-business-os-app-story-tests.json \
  node src/apps/business-os/scripts/run-app-story-tests.mjs
node src/apps/business-os/scripts/assert-app-quality-contracts.mjs
PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright \
CTOX_BIN=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/cargo-target/release/ctox \
SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 \
SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 \
SMOKE_MATRIX_RESULT_PATH=output/rev58-business-os-dynamic-apps-ui.json \
BUSINESS_PORT=62351 SIGNALING_PORT=62352 \
  /Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node \
  src/core/rxdb/tools/browser_rust_smoke_matrix.js
PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright \
CTOX_BIN=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/cargo-target/release/ctox \
SMOKE_MODES=business-os-app-release-ui,business-os-app-audience-ui \
SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html \
SMOKE_MODE_TIMEOUT_MS=300000 \
SMOKE_MATRIX_RESULT_PATH=output/rev58-business-os-release-audience-ui.json \
BUSINESS_PORT=62451 SIGNALING_PORT=62452 \
  /Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node \
  src/core/rxdb/tools/browser_rust_smoke_matrix.js
```

Ergebnis:

- Creator 10/10, Research 9/9, Outbound 14/14 plus Audience 4/4 und Coding
  Agents 11/11 grün; der optionale Outbound-XLSX-Fixture-Test bleibt ohne
  gesetztes Fixture erwartungsgemäß übersprungen.
- Alle vier Source-App-Validatoren melden `ok=true` und keine Failures.
- `output/playwright/rev58-business-os-app-story-tests.json`: 34/34 Core-Apps,
  65 ausführbare Testdateien, 297 bestanden, 0 fehlgeschlagen, 1 optionaler
  Test übersprungen.
- Der strikte App-Quality-Auditor meldet 34/34 Apps mit benanntem Archetyp,
  Variante, Fachstory und Aktionen.
- `output/rev58-business-os-dynamic-apps-ui.json` ist in einem frischen
  isolierten Browserprofil nach 19,48 s grün: reale multiplexed WebRTC-
  Replikation, Runtime-App-Reload, Lifecycle-/Why-Diagnostik, Storage-Scope,
  Read-Denial/Grant/Write-Denial für 16 paketierte Apps und exakte Scopes für
  acht System-/Internal-Apps. Browserwarnungen, WebSocket-Warnungen,
  Browserfehler, 404, Request-Failures, Asset-Fehler und Cache-Reparaturen
  stehen jeweils bei null.
- `output/rev58-business-os-release-audience-ui.json` ist für beide
  risikoreichen Fachstories grün. `business-os-app-release-ui` veröffentlicht
  eine private Runtime-App über den realen App-Store-/Business-Command-Pfad,
  prüft native Team-/Versions-/Data-Review-Projektion, Release-/Rollback-Audit,
  Reload, Storage-Boundary und echten Rollback. `business-os-app-audience-ui`
  prüft Private-/Preview-/Restricted-Sichtbarkeit mit getrennten Akteuren,
  Deep-Link-Lock, Reload, frisches Profil und manipulierten Browser-Storage.
  Beide Läufe melden null Browser-/WebSocket-Warnungen, Browserfehler, 404,
  Request-/Asset-Fehler und Cache-Reparaturen.

Diese Evidence schließt den veralteten Signature-Resttrack und erneuert den
gemeinsamen dynamischen Rechte-/Reload-/Runtime-Boundary-Nachweis. Release,
Rollback und Audience-Persistenz sind zusätzlich als echte risikoreiche
Fachmutationen belegt. Sie ersetzt noch nicht jede app-spezifische Fachaktion;
weitere priorisierte Apps benötigen weiterhin echte fachliche Datenmutation,
Persistenz und Resume im Browser. Revision 59 schließt den damals noch
laufenden Source-Release-Build und den automatischen Managed-Update-E2E.

### Revision 59: automatischer Managed-Upgrade und installierter 35-App-Release-E2E grün am 2026-07-12

Revision 59 schließt den seit Revision 53 offenen automatischen Release-
Lifecycle-Track. Der Installer verwendet für Managed-Source-Builds einen
release-spezifischen persistenten Cargo-Target-Cache außerhalb des atomar
kopierten Release-Roots, startet macOS-Installer explizit über `/bin/bash`,
entfernt Quarantine-/Provenance-Metadaten am kopierten Workspace und behandelt
transiente `launchctl bootout/bootstrap/kickstart`-Races mit begrenztem Retry
und sichtbarem stderr. Ein fehlgeschlagener Installer-Versuch wird genau einmal
mit demselben Buildcache wiederholt. Die Source-Kopie schließt Runtime-,
Archiv-, Output-, Agent- und andere nicht auszuliefernde Artefakte aus.

Der echte Managed-Upgrade auf `rev64-managed-20260712T1200Z` lief danach bis
zum atomaren Switch durch. Der finale Updater-Aufruf einschließlich des
Installer-Retrys endete automatisch mit Exitcode 0; `current`, Install-
Manifest und `update_state.json` zeigen auf den neuen Release und enthalten
keinen Fehler. Um die mehrfachen langen Release-Kompilierungen auf diesem
Testhost nicht erneut vollständig auszuführen, wurde der release-spezifische
Cache einmalig aus einem nahezu vollständigen Vorgängerlauf vorbefüllt. Das
ist Testhost-Optimierung, kein Produktpfad; produktiv werden Caches wegen des
Release-Schlüssels nicht releaseübergreifend geteilt.

Evidence:

```text
output/rev64-managed-asset-guard.json
output/rev64-installed-rxdb-status-before-browser.json
output/playwright/rev64-managed-retry-smoke/business-os-interactive-window-qa.json
output/playwright/rev64-managed-retry-full/business-os-interactive-window-qa.json
output/rev64-installed-rxdb-status-after-browser.json
```

Ergebnis:

- `~/.local/lib/ctox/current`, Install-Manifest und Update-State zeigen auf
  `rev64-managed-20260712T1200Z`; die Update-Phase ist `completed` und
  `last_error` ist leer.
- Service und lokales Signaling laufen aus dem neuen `current`: der Service-
  LaunchAgent startet `/bin/bash`, der Signaling-Agent den verwalteten
  Node-Prozess. Ports 8765, 8788 und 20876 lauschen.
- Der Asset-Guard meldet `ok=true`; zwölf kritische Shell-/App-Artefakte sind
  zwischen Source, Managed State und aktuellem Release bytegleich.
- Der frische Drei-App-Smoke für CTOX, Threads und Tickets ist grün.
- Der danach vollständig neu gestartete installierte Browserlauf öffnet,
  resizt und schließt 35/35 Apps. Er meldet `ok=true`, null App-Failures und
  null Console Events; Mobile-App-Navigation ist ebenfalls belegt.
- Der native RxDB-Status steht nach dem Browserlauf auf `running=true`,
  `replicationUp=true`, frischem Heartbeat und null Health-Fehlern.

Ein erster 35-App-Versuch wurde über einen mehrstündigen System-Sleep hinweg
unterbrochen und endete nach dem Host-Neustart mit `ERR_CONNECTION_REFUSED`.
Er ist kein bestandener Langlauf und wird nicht als solcher gezählt; der
anschließende Lauf verwendete einen frischen Browserkontext und lief ohne
Unterbrechung vollständig grün. Damit sind der automatische Managed-Rollout
und der installierte Release-E2E geschlossen. Offen bleiben app-spezifische
Fachmutation/Resume-Tiefe, menschliche Signoffs, der Auth-Build-Smoke und die
historische Hypoport-ID-Zuordnung.

### Revision 60: fünf Runtime-App-Archetypen mit Mutation, Reload und Automation belegt am 2026-07-12

Revision 60 vertieft den offenen Fachstory-Track mit dem realen
`business-os app e2e`-Harness. Für Inventory, Contracts, Projects, Quality und
Subscriptions wurde jeweils über die sichtbare installierte App ein fachlicher
Datensatz angelegt, nach vollständigem Page-Reload wiedergefunden, in der
nativen RxDB-SQLite-Projektion nachgewiesen und anschließend eine
recordbezogene Follow-up-/Review-Automation ausgelöst. Der Marker erschien
danach auch im nativen `business_commands`-Store. Alle fünf Läufe melden leere
Console-, Page- und Request-Fehlerlisten.

Der Lauf entdeckte und behob zusätzlich einen Fehler im generischen E2E-
Harness: Bei Apps ohne kanonisches `data-record-id` konnte die bisherige
`closest(..., div)`-Suche einen zu großen Container wählen und dadurch die
Automation eines alten ersten Datensatzes statt des neu angelegten
Marker-Datensatzes klicken. Der Harness steigt nun vom exakten Marker-Textknoten
bis zum kleinsten sichtbaren Vorfahren mit passender Automation auf. Die zuvor
roten Contracts- und Quality-Läufe sind mit diesem Fix grün.

Evidence:

```text
output/playwright/rev59-app-mutation-e2e/bench_inventory_run_20260625_5app_01.json
output/playwright/rev59-app-mutation-e2e/bench_contracts_run_20260625_5app_01-fixed.json
output/playwright/rev59-app-mutation-e2e/bench_projects_run_20260625_5app_01.json
output/playwright/rev59-app-mutation-e2e/bench_quality_run_20260625_5app_01-fixed.json
output/playwright/rev59-app-mutation-e2e/bench_subscriptions_run_20260625_5app_01.json
```

Damit ist die dynamische Mutation/Reload/Native-Projektion/Automation für alle
fünf kanonischen Runtime-App-Archetypen belegt. Das ist noch keine Behauptung,
dass jede individuelle Core-App-Fachmutation vollständig E2E-geprüft ist;
risikoreiche Core-Flows werden weiterhin über ihre spezialisierten Evidence-
Läufe und bei Änderungen durch zusätzliche Browserstories abgesichert.

### Revision 61: Windowed-Rechtsklick, Rollen, Auth und Fresh Profile erneuert am 2026-07-12

Revision 61 erneuert die risikoreichen Shell-/Policy-Stories gegen den aktuellen
Windowed-App-Vertrag. Dabei wurde eine echte Regression behoben: Taskbar-
Einträge windowed Business-Apps besitzen den Launch-Typ `app`; das Shell-
Kontextmenü bot deshalb trotz korrekter Grants nur Öffnen/Pin an, weil Source-
und Modify-Aktionen ausschließlich für den alten Typ `module` aufgebaut
wurden. `showTargetContextMenu()` löst einen App-Taskbar-Eintrag nun auf sein
zugrunde liegendes Business-Modul auf. Der Windowed-Launch bleibt unverändert,
aber „Source öffnen“ und „App ändern“ folgen wieder den exakten Modulrechten.

Der Rollen-Smoke wurde gleichzeitig vom erzwungenen historischen Fullscreen-
Mount auf den realen Windowed-Launch umgestellt. Seine Delegationsassertion
entspricht wieder dem aktuellen Vertrag: Ohne Modify-Recht bleibt „App ändern“
als approval-pflichtige Aktion sichtbar, statt fälschlich vollständig zu
verschwinden. Source- und Modify-Grants bleiben exakt auf die Ziel-App
beschränkt.

Evidence:

```text
output/rev60-core-policy-ticket-profile-ui.json
output/rev60-roles-permissions-ui-green.json
output/rev60-auth-scope-fresh-profile-ui.json
```

Ergebnis:

- Tickets Browser→Rust erstellt einen lokalen Ticket-Record über den Typed
  Command Bus; Command und Ticket-Projektion enden erfolgreich.
- Der Windowed-Rollenlauf belegt Team-Denial, exakten Source-Grant, exakten
  Modify-Grant, Owner-Rechte, Scope-Isolation, delegierbares App-Ändern,
  Reload, Why-Diagnostik und redaktiertes Support-Paket.
- Der Auth-Lauf belegt Login, authentifizierten Reload, Logout, blockierten
  Reload im ausgeloggten Zustand, geschützten Zugriff, Tenant-Scope und dass
  kopierter Browser-Storage keinen Scope erweitert.
- Fresh Profile startet mit leerem IndexedDB/localStorage/sessionStorage,
  lädt ausschließlich autoritative Projektionen, prüft Lifecycle-/Versions-
  und Disabled-Gründe in Desktop und Narrow Viewport und besteht das
  Skalierungsbudget mit 69 Katalog-Apps, 64 Grants und 96 Versionen.
- Alle drei finalen Läufe melden null Browser-/WebSocket-Warnungen, null
  Browserfehler, null 404, null Request-/Asset-Fehler und null Cache-Reparatur.

Damit ist auch der zuvor offene Auth-fähige Login/Logout-Smoke geschlossen.
Offen bleiben weitere priorisierte Core-App-Fachmutationen, menschliche
Signoffs und die historische Hypoport-ID-Zuordnung.

### Revision 62: Coding Agents als echter Windowed-Codex-Flow geschlossen am 2026-07-12

Revision 62 erneuert den Coding-Agents-Core-Flow als echte Windowed-App. Der
Smoke wartete historisch auf `body.dataset.activeModule=coding-agents`; nach
der Plattformmigration bleibt der Desktop aber korrekt die aktive Shell-
Fläche, während Coding Agents in einem Fenster läuft. Das Gate erkennt deshalb
nun das fertige Windowed-Modul-Root und berichtet die App als aktiven
Fachkontext, ohne den Desktopzustand umzuschreiben.

Der Lauf fand und behob zwei reale Funktionsfehler:

1. Ein Workspace-Item schloss beim Rendern den anfänglichen Provider
   `antigravity` ein. Wurde das Select danach auf Codex geändert, setzte ein
   anschließender Klick den aktiven Provider wieder auf den alten Closure-Wert.
   Die Session wurde dadurch trotz Codex-Grant an Antigravity delegiert und vom
   nativen Grant-Guard korrekt verweigert. Der Klick liest nun den aktuellen
   Select-Wert.
2. Nach erfolgreicher Session-/Prompt-Command-Ausführung lagen Session und
   Events nativ vor, erreichten aber die bereits laufende Browserprojektion
   nicht zuverlässig. Coding Agents erneuert die beiden Projection-Syncs nun
   nach erfolgreichen Provider-Mutationen mit begrenzten 15-Sekunden-Timeouts,
   bevor die UI auf Session und Events wartet.

Evidence:

```text
output/rev61-coding-agents-ui-projection-fix.json
```

Der finale echte Codex-Lauf belegt Workspace-Grant, Provider-Readiness,
Session-Erstellung, Initialantwort, Follow-up, eine laufende Sessionprojektion,
zwei User- und zwei Assistant-Events sowie beide Marker im sichtbaren Feed.
Cleanup stoppt die Session und widerruft den temporären Workspace-Grant. Der
Lauf meldet null Browser-/WebSocket-Warnungen, null Browserfehler, null 404,
null Request-/Asset-Fehler und null Cache-Reparatur. Damit ist Coding Agents
nicht mehr nur statisch oder visuell, sondern mit echter Provider-Delegation,
Persistenz und Resume-Projektion dynamisch abgenommen.

### Revision 63: Outbound-Approval bis Meeting dynamisch geschlossen am 2026-07-12

Revision 63 hebt den Outbound-Nachweis von der visuellen Sales-Workbench auf
eine vollständige fachliche Browser-/Rust-Story. Der reale UI-Lauf erzeugt
Kampagne, Pipeline-Eintrag, Engagement und Nachricht, prüft die blockierte
Ausführung vor Freigabe, genehmigt über den Approval-Pfad und verfolgt die
Nachricht bis `queued_for_provider`. Anschließend werden Provider-ID, positive
Inbound-Antwort, Scheduling-Entwurf und ein gebuchtes Meeting mit sichtbarer
Meeting-URL über die nativen Projektionen verifiziert.

Der Smoke registriert die Outbound-Schemas nun vor der Command-Collection-
Vorbedingung. Zuvor konnte der isolierte Modus die Fachstory gar nicht starten,
weil die modulbezogenen Collections erst beim späteren Mount verfügbar waren.
`outbound-active-ui` ist außerdem Teil der Standardmatrix und besitzt strikte
Evidence-Anforderungen für Approval-Gate, finalen Sendestatus,
Reply-Klassifikation und Meetingstatus.

Evidence:

```text
output/rev62-outbound-active-ui-schema-fix.json
```

Der finale Lauf belegt `approval_gate_verified=1`,
`final_send_status=queued_for_provider`, `reply_classification=positive` und
`meeting_status=booked`. Er meldet null Browser-/WebSocket-Warnungen, null
Browserfehler, null 404, null Request-/Asset-Fehler und null Cache-Reparatur.
Damit ist Outbound einschließlich Freigabe, Delegation, Persistenz und
fachlicher Fortsetzung dynamisch abgenommen.

### Revision 64: Threads-Skalierung und Restore/Resync erneuert am 2026-07-12

Revision 64 wiederholt die drei besonders regressionsanfälligen Shell-/Sync-
Stories gegen den aktuellen Windowed- und Responsive-Stand. Dabei wurden drei
veraltete Harness-Annahmen repariert: Die bereits erzeugte und nativ geprüfte
Thread-ID fehlte im verpflichtenden Evidence-Output, die Scale-Seeds verwendeten
für `user_threads` und `user_thread_messages` noch Schema-Version 0 statt 1,
und der Seed lief vor der Initialisierung der echten Shell-Datenbank. Der
isolierte Lauf registriert die Threads-Collections nun nach Shell-Readiness und
seedet anschließend die kanonischen nativen Tabellen.

Evidence:

```text
output/rev63-threads-scale-current.json
output/rev63-restore-resync-current.json
```

Der Rechtsklick-Flow belegt Zielmodul und Zielrecord, direkte Rechteverweigerung,
persistierte Daten-/Frage-/App-Delegation, Reviewer-Auswahl, Approval und
verknüpfte Reautorisierung. Der Scale-Lauf rendert 10.000 Commands, 10.000
Threads, 10.000 Nachrichten und 10.000 Benachrichtigungen auf höchstens 200
sichtbare Zeilen; der erste Render benötigt 17.697 ms und bleibt damit im
30-Sekunden-Budget. Restore/Resync belegt WebRTC-only, lokalen Offline-Write,
Peer-Neustart, elf Checkpoint-Epochen und native Konvergenz nach Restart. Alle
finalen Läufe melden null Browser-/WebSocket-Warnungen, null Browserfehler,
null 404, null Request-/Asset-Fehler und null Cache-Reparatur.

### Revision 65: Sichtbarer Agent-Scope und App-Store-Kontext repariert am 2026-07-12

Revision 65 erneuert den Agent-Scope-Produktionssmoke gegen den refaktorierten
Command-Bus-Pfad. Der Lauf fand zwei reale UI-Regressionen: Das globale
Kontextmenü berechnete den CTOX-Zugriffsumfang, renderte das bereits importierte
Scope-Panel aber nicht mehr. Im App Store war der lokale Kontextmenü-Handler
vollständig implementiert, wurde beim Mount jedoch nicht registriert. Beides
ist wieder aktiv; der App Store entfernt den Handler beim Unmount.

Zusätzlich ist die globale Menüinitialisierung nun resilient gegen detached
DOM-Nodes nach Legacy-/Full-Workspace-Mounts und bindet ihre Document-Listener
nur einmal. Der Context-Actions-Facade übernimmt den sichtbaren Scope und Actor
in den typisierten Command-Bus-Clientkontext, sodass UI und persistierter Audit-
Kontext nicht auseinanderlaufen. Der Umfang bleibt als standardmäßig
geschlossene Disclosure erreichbar, damit der vollständige Audit-Kontext das
Ableton-artige Routine-Layout nicht dauerhaft aufbläht. Der Smoke prüft die
aktuelle Command-Bus-Delegation statt des historischen Chat-CustomEvents.

Evidence:

```text
output/rev65-agent-scope-compact-current.json
```

Der finale Lauf belegt globales und App-Store-Scope-Panel, UI-/Command-Kontext-
Übereinstimmung, Business-Chat-Scope, Settings-Grant-Boundary, versteckte
Private-App, Read-Denial vor Grant, Read-Erlaubnis nach Grant, Write-Denial
ohne Grant, sichtbaren Audit und Denial-Grund. Er meldet null Browser-/
WebSocket-Warnungen, null Browserfehler, null 404, null Request-/Asset-Fehler
und null Cache-Reparatur.

### Revision 66: Office-/Finance-Regression und Runtime-Guard erneuert am 2026-07-12

Revision 66 erneuert die risikobasierte Basis für Documents, Spreadsheets,
Invoices und Buchhaltung. Die Fachtests sind grün: Documents 7/7,
Spreadsheets 10/10, Invoices 69/69; Buchhaltung beendet den aktuellen Testlauf
ohne Fehler. Der Spreadsheet-Test belegt dabei zusätzlich Blob-Batch-Persistenz,
Formelauswertung und Raw-Cell-Fallback; Documents belegt Blob-Batch-Persistenz
und Draft-Reclamation, Invoices unter anderem Posting, Nummernserie,
XRechnung, Steuern, Zustandsübergänge und Mount/Unmount.

Der aktuelle Browser-/Rust-Lauf `business-os-dynamic-apps-ui` prüft außerdem
16 paketierte Apps und acht System-/Internal-Apps. Enthalten sind insbesondere
`documents`, `spreadsheets`, `invoices` und `buchhaltung`. Belegt werden
Capability-Vertrag, Collection- und Property-Denial, Raw-/Context-Denial,
Read-Grant, Permission-Facade, Write-Denial ohne Write-Grant, Shell-Locked-
State, System-Scope-Isolation und Reload.

Evidence:

```text
output/rev65-dynamic-core-apps-current.json
```

Der finale Lauf meldet null Browser-/WebSocket-Warnungen, null Browserfehler,
null 404, null Request-/Asset-Fehler und null Cache-Reparatur. Dieser Nachweis
schließt den gemeinsamen Runtime-/Rechte-/Reload-Vertrag; app-spezifische
Office- und Finance-Mutationsketten werden dadurch ausdrücklich nicht mit
einem bloßen Guard gleichgesetzt.

### Revision 67: Spreadsheet-Erstellung bis native Blob-Projektion geschlossen am 2026-07-12

Revision 67 ergänzt `spreadsheets-active-ui` als verbindliches Produktionsgate.
Der Browser öffnet die echte Windowed-App, erstellt einen Tabellenentwurf über
das sichtbare Drawer-Formular und prüft Hauptrecord, Version und Blob-Chunk in
den App-Collections. Danach wird zum Desktop gewechselt, die App erneut
geöffnet und der Entwurf in der sichtbaren Liste als Resume-Nachweis gesucht.
Das Node-Gate prüft anschließend alle drei Records in den nativen SQLite-
Projektionen.

Der erste Lauf fand eine reale API-Inkompatibilität: Spreadsheet-Toast-Aufrufe
verwendeten `notifications?.success(...)` beziehungsweise `.error(...)` und
warfen, wenn das Notifications-Objekt existierte, die jeweilige Methode aber
nicht. Alle Spreadsheet-Aktionen verwenden nun optional callable Methoden.
Außerdem startet der isolierte Story-Modus die Record-, Version- und
demand-only Blob-Replication explizit und wartet nach der Mutation auf
`awaitInSync`.

Evidence:

```text
output/rev66-spreadsheets-active-ui.json
```

Der finale Lauf belegt Spreadsheet-ID, Version-ID, Blob-ID, mindestens einen
Chunk, sichtbares Resume nach Reopen und die native Projektion aller drei
Records. Er meldet null Browser-/WebSocket-Warnungen, null Browserfehler, null
404, null Request-/Asset-Fehler und null Cache-Reparatur. Spreadsheets ist damit
nicht mehr nur über Fachtests und Runtime-Guards, sondern als echte
UI-/Persistenz-/Resume-Kette dynamisch abgenommen.

### Revision 68: Document-Import bis native Blob-Projektion geschlossen am 2026-07-12

Revision 68 ergänzt `documents-active-ui` als verbindliches Produktionsgate.
Der Browser öffnet die echte Documents-App, öffnet den Import-Drawer, weist dem
sichtbaren File-Input eine reale Markdown-Datei zu und sendet das Formular.
Anschließend werden Dokumentrecord, Version und Blob-Chunk in den Browser-
Collections geprüft, die drei Replikationen bis `awaitInSync` verfolgt, zum
Desktop gewechselt und Documents erneut geöffnet. Der importierte Titel muss
nach Reopen wieder in der sichtbaren Liste erscheinen.

Evidence:

```text
output/rev67-documents-active-ui.json
```

Das Node-Gate prüft Dokument, Version und Blob-Chunk zusätzlich in den nativen
SQLite-Projektionen und vergleicht `current_version_id` und `blob_id`. Der
finale Lauf belegt mindestens einen Chunk, sichtbares Resume und native
Projektion. Er meldet null Browser-/WebSocket-Warnungen, null Browserfehler,
null 404, null Request-/Asset-Fehler und null Cache-Reparatur. Documents ist
damit als echte UI-/Persistenz-/Resume-Kette dynamisch abgenommen.

### Revision 69: Invoice-Command bis sofortige Fachprojektion geschlossen am 2026-07-12

Revision 69 ergänzt `invoices-active-ui` als verbindliches Produktionsgate.
Der Browser legt zunächst einen realen Customer-Record an, öffnet die echte
Invoices-App und betätigt den sichtbaren Create-Control. Der daraus entstehende
Typed Command muss `completed` erreichen, die serverseitig erzeugte Rechnung
muss über RxDB/WebRTC in `accounting_invoices` erscheinen und nach Rückkehr zum
Desktop sowie erneutem Öffnen sichtbar bleiben. Customer, Invoice und Command
werden anschließend zusätzlich direkt in den nativen SQLite-Collections
verglichen.

Der erste dynamische Lauf deckte dabei eine reale Projektionslücke auf: Der
aggregierte Business-Record-Projektor hatte `accounting_invoices` bereits leer
passiert und arbeitete anschließend die übrigen registrierten Collections ab.
Ein abgeschlossener Command konnte deshalb minutenlang ohne sichtbaren
Fachrecord bleiben. Der native Projektor fragt nun die serverautoritative
`business_records_projection_clock` ab und verarbeitet nur tatsächlich
veränderte Collections. Zusätzlich projiziert der Command-Consumer nach einem
konsumierten Command die geänderten direkten Fachcollections, nachdem sein
Command-Lock freigegeben wurde. Die Datenhoheit bleibt vollständig nativ; es
gibt weder Browser-Mirroring noch einen HTTP-Datenpfad.

Evidence:

```text
output/rev70-invoices-active-ui.json
```

Der finale Lauf verwendet genau einen Versuch und belegt Customer-ID,
Invoice-ID, Command-ID, terminalen Status `completed`, sichtbares Resume und
native Projektion. Er meldet null Browser-/WebSocket-Warnungen, null
Browserfehler, null 404, null Request-/Asset-Fehler, null Cache-Reparatur und
null unbekannte Prozesssignale. Invoices ist damit als echte
UI-/Command-/Persistenz-/Resume-Kette dynamisch abgenommen.

### Revision 70: Journalbuchung bis native Fachprojektion geschlossen am 2026-07-12

Revision 70 ergänzt `buchhaltung-active-ui` als verbindliches Produktionsgate.
Der Browser öffnet die echte Buchhaltungs-App, wartet auf den sichtbar
gerenderten Kontenrahmen, wechselt über die sichtbare Navigation ins Journal,
öffnet den Drawer über `Neue manuelle Buchung`, befüllt Buchungstext und Betrag
und sendet das echte Formular. Danach müssen ein Journal-Entry und genau zwei
ausgeglichene Buchungszeilen in den Browser-Collections vorhanden sein. Nach
Replikations-Sync, Rückkehr zum Desktop und erneutem Öffnen muss der
Buchungstext wieder sichtbar sein. Entry und Zeilen werden zusätzlich in den
nativen SQLite-Collections geprüft.

Die dynamische Prüfung deckte ein Lifecycle-Rennen auf: Während der
asynchronen Initialisierung konnte die Shell den App-Root ersetzen, obwohl ein
vorheriger Root bereits als bereit markiert war. Eine sichtbare Aktion öffnete
dann den Drawer in einem unmittelbar danach abgelösten DOM. Der Mount entfernt
nun die Readiness-Markierung des vorherigen Roots; der New-Entry-Handler bleibt
an den DOM-Satz seines Mounts gebunden. Das Gate folgt ausschließlich dem
aktuell verbundenen Root und aktiviert die Buchung erst, nachdem die Konten
auch im sichtbaren App-State gerendert sind. Damit entsteht auch bei schneller
Bedienung kein leeres Konto-Select.

Evidence:

```text
output/rev71-buchhaltung-active-ui.json
```

Der finale No-Retry-Lauf belegt Entry-ID, exakt zwei Zeilen, sichtbares Resume
und native Projektion. Er meldet null Browser-/WebSocket-Warnungen, null
Browserfehler, null 404, null Request-/Asset-Fehler, null Cache-Reparatur und
null unbekannte Prozesssignale. `buchhaltung.test.mjs` bleibt zusätzlich grün.
Buchhaltung ist damit als echte UI-/Persistenz-/Resume-Kette dynamisch
abgenommen.

### Revision 71: Client-only-Lifecycle für 35 Apps geschlossen am 2026-07-12

Revision 71 ergänzt `business-os-client-lifecycle-ui` als verbindliches
Production-Gate. Ein frisches persistentes Chromium-Profil öffnet und schließt
die 35 kanonischen Launch-Targets sequenziell. Für jede App werden Mount-Zeit,
aktive Collections, Bridges, Leases, offene Timer und JavaScript-Heap erfasst.
Nach jedem Close darf keine Ressource oberhalb der Startbaseline verbleiben;
weniger Core-Bridges sind zulässig. Kein Browser-, Shell- oder nativer
Peer-Neustart darf den Lauf reparieren.

Der Gate deckte mehrere echte Lifecycle-Verstöße auf und schloss sie zentral:

- `ctx.sync.startCollection()` pinnt in App-Kontexten keine permanente Shell-
  Bridge mehr; die Modul-Lease bleibt alleiniger Lifecycle-Owner.
- Aufrufe aus bereits getrennten Modul-Hosts werden abgewiesen. Eine vor Close
  begonnene, aber erst danach aufgelöste Demand-Lease wird sofort freigegeben.
- Der Command Bus verwendet auch für `business_commands` und
  `ctox_queue_tasks` scoped Leases und gibt sie über seinen vorhandenen
  Abschluss-Pfad wieder frei.
- Knowledge und Threads starten ihre bereits vom Shell-Lease verwalteten
  Manifest-Collections nicht erneut.
- Research verwaltet Demand-Leases mountgebunden und plant nach Unmount keine
  Post-Sync-Refresh-Timer mehr.

Evidence:

```text
output/rev72-client-lifecycle-ui.json
```

Der finale No-Retry-Lauf ist für 35/35 Apps grün. Die höchste gemessene
Mount-Zeit beträgt 259 ms. Nach dem letzten Close existiert kein Fenster mehr;
Lease-Delta ist 0, Bridge- und Active-Collection-Delta jeweils -2, Timer-Delta
-5 und der Heap-Zuwachs 48.070.380 Bytes innerhalb des 64-MiB-Korridors. Der
Sync-Runtime-Owner bleibt identisch, Peer-Restarts sind 0. Browserwarnungen,
Browserfehler, Request-Fehler, 404, Asset-Fehler, Cache-Reparaturen und
unbekannte Prozesssignale sind jeweils 0. Erfolgskriterium 24 und der
Client-only-Runtime-Track sind damit technisch geschlossen.

### Revision 72: Shell-Viewport und app-vollständiger Command-/Harness-Vertrag am 2026-07-14

Die sichtbare Browserabnahme deckte zwei gemeinsame Plattformfehler auf: Das
Business-OS-Root folgte nicht in jedem Zustand exakt dem Browser-Viewport und
ein zweiter App-Umschalter konkurrierte unten mit dem Chat-Dock, obwohl die
offenen Apps bereits oben dargestellt werden. Die Shell verwendet jetzt exakt
`100dvw`/`100dvh`, reagiert auf Viewport- und Visual-Viewport-Resize, reflowt
Fenster und Chat-Inset gemeinsam und besitzt nur noch die obere App-Leiste als
App-Umschalter. Der untere Bereich bleibt ausschließlich Chat, Verlauf und
Systemstatus vorbehalten.

Der Command-/Harness-Audit ist nicht mehr auf bekannte Einzel-Apps begrenzt:

- `assert-app-command-contracts.mjs` prüft alle 34 Core-Apps und ihre
  Qualitätsverträge. Literal- und Konstanten-Command-Typen, optionale
  `ctx.commandBus?.dispatch?.`-Aufrufe, lokale Dispatch-Aliase und
  `requireCommandBus(ctx).dispatch` werden aufgelöst.
- Das native Inventar wird aus exakten Rust-Routen, Control-Predicates und
  Browser-Runtime-Routen erzeugt. Jeder verwendete Typ landet eindeutig in
  nativer Control-Ausführung, Browser-Runtime-Ausführung oder der dauerhaften
  CTOX-Queue mit begrenztem Harness-Prompt. Der aktuelle Source belegt 87
  native Control-Typen, einen Browser-Runtime-Typ und 36 Queue-/Harness-Typen.
- Apps dürfen `business_commands` weder direkt schreiben noch bei fehlendem
  Bus einen eigenen Pending-Intent anlegen. Browser und Conversations wurden
  auf den offiziellen, fail-closed Shell-Bus umgestellt. Das Consumer-Gate
  belegt 36 Dispatch-Verbraucher, 14 Projektionsleser, null alte
  Projection-Waiter und null direkte Intent-Writer.
- `command_type` ist der kanonische v2-Identifier. `type` bleibt nur als
  gleichwertiger Eingabealias für historische externe Pakete erhalten;
  sämtliche Core-App-Quellen verwenden ausschließlich `command_type` und das
  Gate meldet null Legacy-Properties. Der gleiche Guard erfasst Shell,
  Settings und Desktop-Apps; nur der interne `ctox.command.cancel`-
  Kompatibilitätsalias im Bus selbst ist explizit zugelassen. Widersprüchliche
  Doppelangaben werden vor dem Write abgewiesen. Submit, Acceptance,
  Terminalstatus, Resume, Subscription und Cancel bleiben Funktionen desselben
  Shell-Facades.

Real-Browser-Evidence gegen den aktuellen Source:

- 35/35 Launch-Targets bei 1600×1000 und 35/35 bei 390×844: Öffnen,
  Verkleinern/Vergrößern, Maximize/Restore, Minimize/Restore über die obere
  App-Leiste, Close, Pane-Rückweg, Shell-Geometrie und Console-Gate sind grün.
- Der Root entspricht in beiden Viewports exakt der Browserfläche; es gibt
  keinen Document-Overflow und keinen unteren App-Umschalter.
- Spreadsheets verwendet auch für den letzten 600-px-Fall einen App-Container-
  statt Viewport-Breakpoint. Customers ordnet bei 390 px Suche und beide Filter
  vollständig und ohne abgeschnittene Labels an.
- Der reale `command-browser-to-rust`-Smoke repliziert einen kanonischen v2-
  Command über RxDB/WebRTC, erhält native Acceptance und findet exakt einen
  dazugehörigen `ctox_queue_task` im Status `queued`. Browser-, WebSocket-,
  Request-, Asset- und unbekannte Prozesssignalfehler sind null.
- Der aktuelle `business-os-threads-rightclick-ui`-E2E belegt den exakten
  Notes-Record-/Pointer-Kontext, Datenänderung, Frage und App-Änderung,
  persistierte Ask-/Approval-Commands, direkte native Verweigerung mit
  `role_or_scope_denied`, sichtbare Reviewer-Auswahl, Admin-Freigabe und
  verknüpfte Reautorisierung bis zum akzeptierten Ziel-Command. Browser-,
  WebSocket-, Request-, Asset- und unbekannte Prozesssignalfehler sind null.
- 62 betroffene App-Vertragstests plus Matching sind grün. Die RxDB-Suite ist
  mit 67 regulären Tests grün; die drei im Sandboxlauf am lokalen Listen-Socket
  blockierten Browser-Smokes wurden anschließend einzeln außerhalb dieser
  Einschränkung wiederholt und sind ebenfalls grün. Zwei Wire-Daemon-Smokes
  bleiben in der Suite erwartungsgemäß übersprungen, weil der optionale
  Test-Daemon für diesen Lauf nicht gebaut wurde.
- Der kanonische `business-os-app-module-development`-Skill, sein
  Modul-Static-Check, der App-Validator-Fixture und der separate
  `ctox-business-os-deploy-skill` erzeugen ebenfalls nur noch
  `command_type`. Die Deploy-Skill-Suite und Strukturvalidierung sind grün.
- Der App-Starter ergänzt `module`, `record_id` und `payload.title` im
  Signature-Command. Neue Starter-Collections beginnen korrekt bei Schema 0,
  statt ohne Migration fälschlich Schema 1 zu deklarieren. Fünf Archetypen
  bestehen Generierung und Modulvalidierung; die reale Browsermatrix ist mit
  60 Zellen und fünf Denial-/Delegation-Flows grün.

Evidence:

```text
output/playwright/business-os-all-apps-command-responsive-20260714/business-os-interactive-window-qa.json
output/playwright/business-os-customers-mobile-refine-20260714/business-os-interactive-window-qa.json
output/playwright/business-os-command-canonical-all-apps-20260714/business-os-interactive-window-qa.json
src/core/business_os/business_command_inventory.json
```

### Revision 74: integrierte Entwicklungstools und finale Plattformabnahme am 2026-07-15

Die Headeraktionen entsprechen jetzt ihrer sichtbaren Bedeutung:

- `Source` schaltet im selben App-Fenster auf eine IDE-Ansicht mit dem
  Dateibaum ausschließlich der aktiven App und dem Monaco-Editor um. Es wird
  weder ein fremder App-Katalog noch ein zweites Source-Fenster geöffnet.
- `Versionen` schaltet im selben Fenster auf die App-Timeline mit Version,
  Sichtbarkeit, Status, Release-Notiz, Kanal und Rollback-Ziel um. Release und
  Rollback verwenden `ctox.module.release` beziehungsweise
  `ctox.module.rollback_version` über den Shell-Command-Bus.
- `App` kehrt ohne Remount zum erhaltenen App-Zustand zurück. Selection,
  Drafts und Scrollposition bleiben daher im laufenden Window-Kontext.
- Desktop und Mobile verwenden denselben Moduswechsel. Unter 760 beziehungsweise
  520 Container-Pixeln stapeln sich Dateibaum, Editor, Timeline, Detail und
  Release-Formular ohne Document-Overflow; interne Bereiche bleiben scrollbar.

Der Rechtsklickpfad ist ebenfalls wieder ein Plattformvertrag. App Store und
IoT registrieren keine eigenen `contextmenu`-Handler mehr. Semantische
`data-context-*`-Attribute liefern App-/Record-/Feldkontext; die Shell ergänzt
Pane, Window-Instanz, Präsentationsmodus und exakte Pointerkoordinaten in
`business-os-context-v2`. Datenänderung, Frage, App-Änderung und die
rollenabhängige Threads-Delegation verwenden dadurch wieder denselben
persistierten Command-Pfad. IoT-spezifische Widgetaktionen bleiben getrennt
über den sichtbaren Aktionsbutton erreichbar.

Die abschließende visuelle Kontrolle ergänzt außerdem den bislang fehlenden
Mobile-Desktop-Vertrag: persistierte und runtime-installierte Shortcuts
ignorieren unter 560 px ihre absoluten Desktop-Koordinaten und fließen in ein
scrollbares Vier-Spalten-Raster. Lange Labels bleiben ohne automatische
Trennstriche auf höchstens zwei Zeilen. Das Browser-Gate verwirft horizontales
Entweichen und jede paarweise Icon-Überlappung; Startmenü sowie Account-/Menü-
Kontrollen müssen bei Compact und Mobile vollständig im Viewport bleiben.

Finale Evidence gegen den aktuellen Source:

- Interaktiver Browser: 35/35 Launch-Targets bei 1600×1000 und 35/35 bei
  390×844. Geprüft werden Launch, freies Verschieben, Süd-/Corner-Resize,
  Links-/Rechts-/Top-Snap, Maximize/Restore, Minimize/Restore, Close,
  Pane-Rückwege, Mobile-Sheet, Shell-/Chat-Geometrie und Document-Overflow.
- Derselbe Lauf prüft Source und Versionen explizit im ursprünglichen App-
  Fenster, den app-exklusiven Dateibaum und den Rückweg zum erhaltenen Root.
  Der echte globale Rechtsklick auf eine App-Store-Record-Fläche behält App,
  Window-Instanz, Entity und exakte Pointerkoordinaten. Failures und Console-
  Events sind jeweils 0.
- Der Lauf verwendet die Session und den RxDB/WebRTC-Datenpfad der lokalen
  CTOX-Installation auf Port 8765. `qaCatalog=all-source` projiziert nur auf
  lokaler `rxdbSmoke`-Oberfläche alle Source-Apps in den flüchtigen QA-Katalog;
  die produktive Installations- und Sichtbarkeitsmenge einer Instanz bleibt
  unverändert.
- App-Stories: 34/34 Apps, 66 Testdateien, 301 Tests bestanden, 0 fehlgeschlagen.
- Command-/Harness-Inventar: 34 Apps, 28 direkte Dispatch-Apps, 36 dynamische
  Dispatch-Stellen, 0 Legacy-Source-Properties, 0 verbotene Legacy-Properties
  und 0 Issues.
- Designmatrix: 12 Screenshots, Visual Diff und Accessibility grün; Starter-
  Browsermatrix: 60 Zellen einschließlich fünf Denial-/Delegation-Flows grün.
- Chat: 75 Behavior-Szenarien und 11 Shell-Kompositionsphasen grün. Der Chat
  bleibt ein Bottom-Overlay und löst keinen Window-Reflow aus.
- Module: 34/34 Conformance, App-Qualität, DB-Isolation, deklarative
  Migrationen, RxDB-only, globale Kontextpolicy und Platform Freeze grün.
- RxDB: 70 reguläre JS-Smokes grün. Nach dem Release-Build des echten
  `v15_wire_daemon` sind zusätzlich Query-Fetch mit 5.000 Records sowie
  File-Fetch mit 800 KiB über die Rust/JS-Prozessgrenze grün; damit bestehen
  alle 72 Suite-Einträge ohne Skip.

Evidence:

```text
output/playwright/business-os-interactive-final-current/business-os-interactive-window-qa.json
output/playwright/business-os-interactive-shell-controls-smoke/business-os-interactive-window-qa.json
output/playwright/business-os-app-story-tests.json
output/playwright/business-os-app-command-contracts.json
output/playwright/business-chat-behavior-2026-07-15T08-21-50-117Z/business-chat-behavior.json
output/playwright/shell-chat-composition-2026-07-15T08-22-02-126Z/shell-chat-composition.json
src/core/business_os/business_command_inventory.json
src/core/business_os/task_id_inventory.json
```

Technischer Status: abgeschlossen (100 %). Release-Status: automatisierte
Gates grün; menschlicher Security-/Privacy-Signoff bleibt
`pending-signoff` und ist ausdrücklich keine automatisierbare Codeaufgabe.

### Revision 75: vollständige Namen für kompakte Settings-Tabs am 2026-07-15

Die platzsparende Settings-Navigation bleibt bewusst einzeilig und darf ihre
Labels bei kleinen Drawer-Breiten mit Ellipsis kürzen. Jeder Tab trägt jetzt
zusätzlich denselben vollständigen Namen als Browser-Tooltip (`title`) und
Accessible Name (`aria-label`). Das gilt zentral für Runtime, Channels, Sync,
Design, MCP, Nutzer, Aktivität und Module; einzelne Tabs können die
Zugänglichkeit daher nicht mehr versehentlich verlieren.

Der Regressionstest rendert alle acht Admin-Tabs und prüft beide Attribute.
Der echte Chromium-Lauf hovert jeden Tab bei 900 × 1000 und 390 × 844, prüft
die vollständigen Namen und stellt sicher, dass die Tab-Leiste innerhalb des
Viewports bleibt. Die lokale laufende Installation sowie Infyoda, SKF, GPU3
und THESEN liefern den Fix mit einer neuen Asset-Version aus; die jeweiligen
Installed-/Local-App-Mengen blieben unverändert.

Evidence:

```text
output/playwright/business-os-settings-tooltips-v100/report.json
output/playwright/business-os-settings-tooltips-v100/settings-900.png
output/playwright/business-os-settings-tooltips-v100/settings-390.png
src/apps/business-os/shared/react-settings.test.mjs
```

Revision 75 ist eine Regression-Härtung des bereits abgeschlossenen
Responsive-/Accessibility-Vertrags und ändert den technischen Gesamtstand von
100 % nicht.

## 1. Executive Decision

CTOX Business OS wird von einer Sammlung historisch gewachsener Module zu einer
klar definierten App-Plattform refaktoriert.

Jede vorhandene App wird mindestens einmal vollständig gegen den neuen
Plattformvertrag geprüft, überarbeitet und neu abgenommen. Fachliche Logik,
RxDB-Schemas und funktionierende Workflows werden nicht pauschal neu
geschrieben. Erneuert werden die gemeinsamen Verträge für Präsentation,
responsive Darstellung, Standard-UI, Kontextaktionen, Command Bus,
Berechtigungen, Delegation, Themes, Loading und Tests.

Die bisherige Einteilung in `windowed` und `full-workspace` wird von der
App-Identität getrennt:

- Jede normale Business-App unterstützt eine responsive Windowed-Variante.
- Dieselbe App kann maximiert und optional in einem Focus-Modus laufen.
- Window, Maximized, Snap und Focus verwenden denselben Mount und Zustand.
- Minimized und die vorhandenen Snap-Zonen bleiben Window-Manager-Zustände.
- Nur echte Shell-Oberflächen wie der Desktop dürfen ausschließlich eine
  Vollflächen-Surface sein.

Normales lokales App-CRUD bleibt ein direkter RxDB-/Sync-Engine-Pfad. Es wird
nicht pauschal in Commands umgebaut. Der native Sync-Peer muss dafür exakte
`data.read`- und `data.write`-Entscheidungen erzwingen. Automationen,
Agentenaktionen, privilegierte Mutationen, App-Änderungen, Freigaben und
Delegationen laufen dagegen immer über den Typed Command Bus.

Die Migration erfolgt nicht als Big Bang. Zuerst werden der aktuelle Worktree
gesichert, der tatsächliche Ist-Stand inventarisiert und zwei parallele
Fundament-Tracks aufgebaut. Erst danach folgen Context Actions, Creator/Store,
Piloten und die App-Wellen.

## 2. Was Revision 2 gegenüber dem ersten Plan ändert

Das Review mit 33 Agents und adversarialen Gegenprüfungen hat folgende
Änderungen ausgelöst:

1. Der Plan unterscheidet jetzt zwischen `HEAD`, uncommitted Worktree und
   Zielzustand.
2. Acht der neun aktuell als Windowed sichtbaren Apps werden als in-flight
   Migration behandelt, nicht als bereits gelandete Plattformbasis.
3. Der bestehende Rechtsklick-v1-Vertrag wird erweitert und dual gelesen,
   nicht durch einen Greenfield-Vertrag ersetzt.
4. Der dominante Nicht-Command-Pfad, direkte RxDB-Schreibvorgänge, ist jetzt
   ausdrücklich entschieden und abgesichert.
5. Bereits vorhandene Release-, Update-, Rollback-, Session- und Peer-
   Mechanismen werden wiederverwendet, nicht neu gebaut.
6. Die bestehenden `.ctox-*`-Klassennamen bleiben stabil. Das Design System v2
   kuratiert Tokens und Verhalten, ohne eine zweite 34-App-Markup-Migration zu
   erzwingen.
7. Auto-derived Loading Shells aus `index.html` und `index.css` bleiben eine
   verbindliche Invariante.
8. Presentation-, Design- und Policy-Arbeit laufen teilweise parallel und
   treffen sich an expliziten Join Gates.
9. Visual Regression, Accessibility, Locale-/Width-Tests und der
   parametrisierte App-Contract-Test werden vor den Piloten als Infrastruktur
   gebaut.
10. Zwei frühe Risiken werden vorgezogen: Landing/Reconcile der Windowed-
    Arbeit und Delete-before-copy im Store-Install-Pfad.

### 2.1 Was Revision 4 nach dem installierten Browser-E2E ändert

1. „Source-Gates grün“ und „lokal installiert produktionsreif“ sind getrennte
   Zustände. Nur der erste ist erreicht.
2. Ein strikter QA-Modus darf keine Source-Manifeste in die installierte
   Registry injizieren und muss auf einen tatsächlich gesunden Shell-/Sync-
   Status warten.
3. Ein kompletter sequenzieller App-Lifecycle-Test und ein paralleler
   Zwei-Profil-Test werden zu Release-Gates.
4. Context-Payload, Browser-Dispatch, native Persistenz, Consumer-Ausführung
   und Approval-Delegation werden als fünf getrennte Prüfpunkte behandelt.
5. Transport-/Query-Owner-Fehler dürfen nicht als app-spezifische Mount-Fehler
   fehlklassifiziert werden.
6. Login/Logout kann erst abgenommen werden, wenn die Zielinstallation einen
   echten Auth-Gate besitzt; Auto-Admin im lokalen Profil ist dafür keine
   Evidence.

## 3. Verifizierter Ist-Stand

### 3.1 Modulbestand im aktuellen Worktree

Der aktuelle Bestand unter `src/apps/business-os/modules/` umfasst:

- 34 Module mit eigenem `index.js`
- 34 Module mit eigenem `index.css`
- ungefähr 61.670 Zeilen Modul-JavaScript
- ungefähr 27.259 Zeilen Modul-CSS
- 9 im Worktree als `windowed` deklarierte Module
- 23 im Worktree als `full-workspace` deklarierte Module
- 2 Module ohne eindeutige `layout.shell`-Angabe
- mindestens 15 Module mit `contextmenu`-Handlern irgendwo im Modulbaum

Die 34 Matrixzeilen und die Wellenpartition sind vollständig. Die
Hypoport-Kohorte ist im Repository nicht markiert.

### 3.2 HEAD versus uncommitted Windowed-Arbeit

Nur `browser` ist in `HEAD` bereits als Windowed-Modul etabliert. Folgende acht
Umstellungen existieren aktuell nur als uncommitted Worktree-Änderungen:

- `app-store` (Revision 46 abgenommen)
- `appsec-pentest`
- `creator` (Revision 48 abgenommen)
- `ctox`
- `knowledge`
- `reports` (Revision 44 visuell abgenommen; Landung bleibt Teil der Worktree-
  Konsolidierung)
- `threads`
- `tickets` (Revision 47 abgenommen)

Diese Änderungen umfassen nicht nur `layout.shell = "windowed"`, sondern einen
de-facto Presentation Contract:

- `launch_kind = "desktop-app"`, teilweise auf Root- und Layout-Ebene
- `default_width` und `default_height`
- `min_width` und `min_height`
- Anpassungen an `app.js`, `window-manager.js`, Desktop Launcher, Registry und
  Guards

Phase 0 muss diese Arbeit zuerst einem Eigentümer zuordnen, gegen `HEAD`
reconcilen, testen und als separaten, reviewbaren Stand landen. Kein weiterer
Planabschnitt darf sie als dauerhaft vorhanden voraussetzen, bevor dieses Gate
erfüllt ist.

### 3.3 Bereits vorhandene Plattformbausteine

Der Refactor beginnt nicht auf leerer Fläche. Bereits vorhanden sind:

- ein 43-Felder-`mount(ctx)`-Vertrag, gepinnt durch
  `scripts/assert-module-context-contract.mjs` und dokumentiert in
  `docs/business-os-module-context.md`
- ein globales Shell-Kontextmenü mit Capture-Phase-Handler
- der dokumentierte v1-Rechtsklick-Contract in
  `src/apps/business-os/ARCHITECTURE.md`
- kanonische Attribute `data-context-record-id`,
  `data-context-record-type`, `data-context-label`
- Fallbacks über `data-*-id`, Element-ID und Pane-Klassennamen
- `selected_text`, `clicked_text`, Actor und `visible_scope` im bestehenden
  Context-/Command-Payload
- `threads.ctox_approval.request` inklusive nativer Re-Autorisierung bei der
  Freigabe
- ein Shared Context Menu mit Keyboard-Navigation innerhalb des geöffneten Menüs
- ein Design-System-Kit in `shared/base.css`, auf das alle 34 Module am
  2026-07-06 migriert wurden
- grüne Token- und Modul-Conformance-Gates
- Window Manager mit Minimize, Maximize, Restore und acht Snap-Zonen
- workspace- und actor-spezifische Fenstergeometrie in RxDB plus lokalem Cache
- ein Catalog-Update-Pfad mit Staging, atomarem Swap, Backup-Restore,
  Recovery-Versionen, Update-Badge und Rollback
- Release-Records, Data-Access-Review-Gate sowie getrennte Release-/Rollback-
  Rechte
- Session-Revocation und Peer-Revocation
- automatisch aus echtem `index.html` und `index.css` abgeleitete Loading Shells

### 3.4 Verifizierte Restlücken

Die Arbeit konzentriert sich auf folgende Deltas:

1. Gewöhnliche Collection-Reads und -Writes werden im nativen Sync-Pfad nicht
   gegen exakte `data.read`-/`data.write`-Grants geprüft.
2. Capability-Tokens prüfen Signatur und Ablauf, aber keine Actor Epoch, `jti`
   oder Revocation.
3. Der native Denial Path erzeugt einen Fehler und Audit, aber keinen
   delegierbaren Threads-Vorgang.
4. Window-Events für Chat-/Agentenaktionen können ohne gemountete Chat-Surface
   stumm verloren gehen, obwohl erfolgreiche Zustellung später in einem
   Command endet.
5. Der globale Context Handler verwendet `state.activeModule` und kann einen
   Klick in einem Windowed-Modul dem Hintergrundmodul zuordnen.
6. Dem v1-Kontext fehlen `field.path`, Pointer-Koordinaten und strukturierte
   Multi-Selection-IDs.
7. Viele der 15 lokalen Kontextmenüs sind durch den Capture-Handler bereits
   toter Legacy-Code; App Store, Desktop und native Editor-Surfaces bleiben
   echte Sonderpfade.
8. Container Queries sind nur in zwei Modulen vorhanden; der Window Host ist
   kein Container.
9. `full-workspace` wird noch durch Validator und Skill-Dokumente für
   runtime-installierte Apps erzwungen.
10. Das sanktionierte Kit selbst verwendet große Radien, Glass, Gradients und
    starke Schatten; einige Module besitzen zusätzliche harte Ausreißer.
11. Release-Records existieren, aber SemVer steuert weiterhin Sichtbarkeit in
    Native und Browser.
12. Release führt Validator, Smoke und E2E nicht aus.
13. `install_app_module` löscht bei Reinstall das bestehende Ziel vor dem Copy.
14. Install, Template Install und Update besitzen keine einheitliche
    Pre-Activation-Validation und keine Signaturprüfung.
15. Creator, MCP-Create, Prepare-App-Source und Template Store starten aus
    unterschiedlichen Quellen und validieren unterschiedlich.
16. Der App-Development-Skill ist für Remote-Agenten nicht als Ressource
    abrufbar.
17. Same-Origin-Module sind kein Sicherheitsboundary für offenen Third-Party-
    Code.
18. Visual-Diff-, Accessibility-, Reduced-Motion- und gerenderte Locale-/Width-
    Gates fehlen.

### 3.5 Reproduzierbare Evidence Anchors

| Bereich | Aktuelle Referenz |
| --- | --- |
| Collection Read Policy | `src/core/business_os/policy.rs::role_may_read_collection` |
| Record Read/Write Hooks | `src/core/business_os/threads.rs::may_replicate_document`, `may_accept_peer_write` |
| Capability Verification | `src/core/business_os/capability.rs::verify_capability_token` |
| Exact Grants | `src/core/business_os/store.rs::active_permission_grant_allows` |
| Release Visibility | `src/core/business_os/store.rs::projected_module_lifecycle` |
| Staged Update | `src/core/business_os/store.rs::update_module_to_catalog` |
| Gefährlicher Install-Pfad | `src/core/business_os/store.rs::install_app_module` |
| Global Context | `src/apps/business-os/app.js::handleGlobalContextMenu` |
| Context Detection | `src/apps/business-os/app.js::extractGlobalCtoxContext`, `detectRecordFromElement` |
| Windowed Mount | `src/apps/business-os/app.js::openWindowedModule` |
| Auto Loading Shell | `src/apps/business-os/app.js::applyLoadingShadow` |
| Window States | `src/apps/business-os/shared/window-manager.js` |
| Kit | `src/apps/business-os/shared/base.css` |
| Shell Tokens | `src/apps/business-os/app.css` |
| Conformance | `src/apps/business-os/scripts/assert-module-conformance.mjs` |
| App Validator | `src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs` |

## 4. Zielprinzipien

CTOX Business OS wird als kompaktes, zeitloses Arbeitsinstrument gestaltet.
Ableton Live dient als Referenz für Dichte, räumliches Gedächtnis, direkte
Interaktion und funktionalen Farbeinsatz, nicht als zu kopierende Oberfläche.

- Wiederkehrende Arbeit ist visuell leise und überall konsistent.
- Import, Suche, Filter, Sortierung, Auswahl, Editieren und Export verwenden
  dieselben Primitive und Positionierungsregeln.
- Jede App besitzt höchstens eine besonders präsente Signature Action pro
  sichtbarem Arbeitsbereich.
- Wiederkehrende Controls belegen nur ihre notwendige Groesse: kompakte
  Toolbar-Hoehen, kleine Status-Chips, icon-first bei knapper Breite und keine
  dekorativen Grosskarten. Touch-Modus vergroessert die Trefferflaeche, nicht
  die visuelle Masse des gesamten Layouts.
- Farbe zeigt Auswahl, Status, Semantik oder die einzigartige Automation an.
- Light und Dark sind zwei Renderings desselben räumlichen Systems.
- Die erste Ansicht zeigt echte Arbeit, keine Hero- oder Dashboard-Dekoration.
- Rechte, Sync, Ausführung und Fehlerzustand sind sichtbar und verständlich.
- Deutsch und Englisch werden in allen unterstützten Größen getestet.
- Bestehende belastbare Verträge werden erweitert, nicht parallel neu erfunden.

## 5. Zielarchitektur

### 5.1 Plattformschichten

```text
Native Policy, Sync Hooks, Store, Release und Audit
                       |
                Typed Command Bus
                       |
       App SDK und Context Action Service
                       |
       Presentation Runtime und Window Manager
                       |
        Bestehendes Kit, Tokens und Patterns
                       |
              Templates und App Creator
                       |
                  Business Apps
```

Keine App darf eine untere Schicht durch eine eigene HTTP-Datenbrücke,
UI-only-Rechteprüfung oder ad hoc gestartete Prozesse umgehen.

### 5.2 Presentation Contract

App-Identität und Präsentationsmodus werden getrennt. Ein möglicher finaler
Vertrag ist:

```json
{
  "presentation": {
    "default_mode": "window",
    "supported_modes": ["window", "maximized", "focus"],
    "initial_size": { "width": 1120, "height": 760 },
    "minimum_size": { "width": 640, "height": 480 },
    "multi_instance": false
  }
}
```

Die endgültigen Feldnamen werden in einem ADR festgelegt. Die Migration ist
nicht nur ein Mapping von `layout.shell`. Folgende bestehende Felder und
Aliase werden gleichzeitig aufgenommen:

- `layout.shell = full-workspace`
- `layout.shell = windowed`
- `layout.shell = desktop-window`
- `layout.full_workspace`
- `layout.fullFrame`
- `layout.launch_kind`
- Root-`launch_kind`
- `default_width`, `default_height`, `min_width`, `min_height`
- doppelte Werte in `module.json` und `modules/registry.json`

Der Compatibility Reader akzeptiert während der Migration Alt und Neu. Writer
schreiben nach dem Cutover nur noch den kanonischen Vertrag.

#### Präsentationsmodi und Window-Zustände

| Kategorie | Werte | Bedeutung |
| --- | --- | --- |
| App-Modus | `window`, `maximized`, optional `focus` | vom Manifest unterstützte Darstellung |
| Window-Zustand | `normal`, `minimized`, `maximized` | bestehender Window-Manager-State |
| Snap-State | acht vorhandene Snap-Zonen | bestehende Geometrievariante |
| Shell-Ausnahme | `shell-surface` | nur Desktop oder explizit beschlossene Root-Surface |

Regeln:

- Jede normale App unterstützt `window` und `maximized`.
- `focus` ist optional und fachlich begründet.
- Minimize, Maximize, Restore und Snap verwenden den vorhandenen Window Manager.
- Der Wechsel remountet die App nicht.
- `window` zu `focus` darf Auswahl, Scrollposition und Entwürfe nicht verlieren.
- Single-Window bleibt Standard. Multi-Instance ist opt-in und verwendet eine
  Instance-ID in Owner Key, Deep Link und Geometrie-Key.
- Unterhalb der manifestierten Mindestbreite verhindert der Window Manager
  weiteres Verkleinern. Responsive Verhalten unterhalb dieser Breite ist kein
  impliziter Vertrag.
- Touch ist entweder ein expliziter späterer Track oder ein dokumentiertes
  Nichtziel der ersten Migration.

#### Registry und Guards

`module.json`, `modules/registry.json`, eingebetteter Fallback Catalog,
`moduleActivationSignature`, Creator, Desktop Launcher und native Projektionen
werden in einem atomaren Contract-Schritt migriert.

Insbesondere werden gleichzeitig aktualisiert:

- `modules/desktop/registry-launch-smoke.mjs`
- `scripts/assert-module-context-contract.mjs`
- `scripts/assert-global-context-menu-policy.mjs`
- `scripts/validate-app-module.test.mjs`
- `module_static_check.mjs`
- die sieben Skill-Referenzen mit Full-Workspace-Mandat

Guards werden nicht gelöscht oder geschwächt. Sie werden auf den neuen Vertrag
umgestellt.

#### Deep Links und Wiederherstellung

- Ein Deep Link identifiziert App, Record und optional Window Instance.
- Öffnen eines Links fokussiert ein existierendes Window oder öffnet es neu.
- Windowed Links werden nicht auf einen Hintergrund-Tab-Hash umgeschrieben.
- Launch Args sind in Tab-, Window- und Focus-Modus identisch verfügbar.
- Vorhandene Geometrie wird weiterverwendet.
- Ob beim Neustart alle zuvor offenen Fenster automatisch wieder geöffnet
  werden, ist eine Phase-0-Entscheidung und keine bereits vorhandene Funktion.

### 5.3 Responsive Workbench

Der Window Host erhält `container-type: inline-size`. Apps reagieren auf ihren
Container, nicht auf die Browser-Viewport-Breite.

| Zustand | Richtwert | Darstellung |
| --- | --- | --- |
| Compact | 640 bis 759 px | eine Hauptfläche, Navigation und Inspector als Drawer |
| Standard | 760 bis 1199 px | Hauptfläche plus eine kontextabhängige Nebenfläche |
| Wide | ab 1200 px | linke Navigation, Zentrum und rechter Inspector gleichzeitig |

Die Pilotapps kalibrieren die finalen Grenzen. Apps dürfen andere Pane-
Proportionen verwenden, aber keine eigene responsive Grundmechanik bauen. Die
Shell liefert Pane-Stacking, Drawer-Übergang, Resizer, Scroll Ownership und
Layout-Persistenz.

Im Windowed-Pfad erhalten `ctx.left` und `ctx.right` keine bedeutungslosen
Throwaway-Divs mehr. Entweder liefert der Presentation Host echte Slots oder
der Vertrag markiert die Slots als nicht vorhanden und stellt die gemeinsame
Drawer-/Pane-API bereit.

### 5.4 Context Action Service v2 als Erweiterung von v1

Der heutige Contract bleibt während der Migration gültig:

- `module`
- `column`
- `record_type`
- `record_id`
- `label`
- `deep_link`
- `selected_text`
- `clicked_text`
- `visible_scope`

Die v2-Erweiterung ergänzt:

- exakte Window-/App-Attribution statt `state.activeModule`
- `surface_id` und `pane_id`
- Collection und Entity
- `field.path`
- strukturierte Multi-Selection-IDs
- Pointer-Koordinaten
- Presentation Mode und Window Instance

Apps können Targets explizit registrieren:

```js
ctx.contextActions.register(element, {
  surface: 'ticket-timeline',
  pane: 'center',
  entity: {
    collection: 'ctox_tickets',
    type: 'ticket',
    id: ticket.id,
    label: ticket.title
  },
  field: { path: 'status' },
  selection: () => ({ ids: selectedTicketIds }),
  actions: ['context.ask', 'data.modify', 'app.modify']
});
```

Der normalisierte v2-Kontext enthält mindestens:

```json
{
  "schema_version": "business-os-context-v2",
  "app_id": "tickets",
  "window_instance_id": "tickets:default",
  "surface_id": "ticket-timeline",
  "pane_id": "center",
  "presentation_mode": "window",
  "entity": {
    "collection": "ctox_tickets",
    "type": "ticket",
    "id": "ticket_123",
    "label": "Drucker ausgefallen"
  },
  "field": { "path": "status" },
  "selection": { "ids": ["ticket_123"], "text": "" },
  "pointer": { "x": 410, "y": 286 },
  "deep_link": "#tickets?record=ticket_123"
}
```

#### Migrationsregeln

- `data-context-record-id/-type/-label` werden automatisch in v2-Targets
  gemappt.
- `data-left-content` und `data-right-content` bleiben während der Migration
  Pane-Marker.
- Native und Browser-Consumer akzeptieren v1 und v2 parallel.
- V1-Ankünfte werden gezählt, bevor Fallbacks entfernt werden.
- `visible_scope` bleibt erhalten und wird nicht durch ein kleineres v2-Schema
  ersetzt.
- Der Window Host liefert App-ID und Window-ID; der Context Handler verwendet
  nicht mehr das Hintergrundmodul.
- `ctx.contextActions` erweitert den gepinnten Mount-Contract. Pin, Dokument,
  Marker und Tests werden im selben Commit aktualisiert.
- `ARCHITECTURE.md` wird mit dem Code im selben Schritt aktualisiert.

#### Menü-Interop

- Die Shell besitzt Rendering, Fokus, Positionierung und Keyboard Opening.
- Shift+F10 und ContextMenu-Key öffnen denselben Ablauf.
- Das bereits vorhandene In-Menu-Keyboard-Verhalten wird wiederverwendet.
- Window-Header-Context, App Context und Desktop Context fließen in dieselbe
  Registry.
- App Store, Desktop, Monaco, contenteditable, Documents und Spreadsheets
  erhalten dokumentierte Adapter, bevor lokale Menüs entfernt werden.
- Die 15 lokalen Handler werden rekursiv über alle Modul-JS-Dateien erfasst.
- Bereits preempteter Legacy-Code wird gelöscht, nicht als Neuimplementierung
  geschätzt.

### 5.5 CRUD, Commands und Agentenaktionen

Der Plattformvertrag unterscheidet zwei Pfade.

Für runtime-installierte Apps ist direkter Sync-CRUD der Normalfall. Ein neuer
fachlicher Button darf nicht automatisch bedeuten, dass ein Rust-Handler in
den CTOX-Daemon kompiliert werden muss. Der Typed Command Path teilt sich daher
in generische, zur Laufzeit registrierte App-Actions und bewusst native
Capabilities. Nur letztere dürfen eine plattformspezifische Implementierung
verlangen.

#### Direkter Sync-CRUD-Pfad

Geeignet für:

- manuelles Erstellen und Bearbeiten app-eigener Records
- lokale, offline-fähige Formularänderungen
- einfache Drag-/Drop- und Inline-Edits

Regeln:

- Browser schreibt über die shellgelieferte Collection.
- Native Pull-Hooks prüfen exaktes `data.read`.
- Native Push-Hooks prüfen exaktes `data.write`.
- Collection, Record, Actor und Grant werden auditiert, wenn die Policy dies
  verlangt.
- Ein Client darf gewöhnliche Collections nicht allein deshalb schreiben, weil
  sie nicht auf einer Deny-Liste stehen.

#### Typed Command Path

Pflicht für:

- `context.ask`
- agentische oder automatisierte Datenänderung
- Signature Automation
- serverseitige Jobs und Imports
- Bulk- und Cross-Collection-Mutationen
- App-Source-Änderung
- Release, Install, Rollback und Store-Aktionen
- Freigaben und Delegationen

Standardaktionen:

| Aktion | Semantik | Mindestberechtigung |
| --- | --- | --- |
| `context.ask` | lesende Frage im erfassten Scope | exaktes `data.read` |
| `data.modify` | agentische Änderung von Record oder Feld | exaktes `data.write` |
| `app.modify` | Änderung von App-Quelle oder Konfiguration | `apps.modify` |
| `action.delegate` | Übergabe an Reviewer oder CTOX | Thread-/Delegationsrecht |
| `automation.run` | app-spezifische Signature Action | Command-spezifische Policy |

Globale Window-Events dürfen UI öffnen, sind aber kein Zustellungskanal für
fachliche Aktionen. Module verwenden einen shellgelieferten Dispatcher, der
entweder einen Command persistiert oder einen sichtbaren Fehler zurückgibt. Die
Aktion darf nicht davon abhängen, ob Business Chat gemountet ist.

Runtime-App-Regel:

- `ctx.commandBus` akzeptiert app-spezifische Typen nur, wenn sie in der
  installierten Action Registry stehen oder bewusst als generische CTOX-
  Aufgabe geroutet werden.
- Die Registry stammt aus dem signierten/validierten App-Paket und wird ohne
  Binary-Rebuild aktualisiert.
- Deklarative Daten-Actions verwenden einen generischen nativen Executor mit
  schema-validierten Inputs, exakten Grants, Effect Keys, Audit und optionaler
  Saga-Kompensation.
- Ein App-Command darf kein freies SQL, keinen Dateipfad und keinen beliebigen
  Prozessaufruf aus dem Browser transportieren.
- Native Spezialhandler bleiben für Core-Capabilities kompatibel, sind aber
  kein erforderlicher Erweiterungspunkt für gewöhnliche Apps.

Der bestehende Approval-Flow wird erweitert:

- `threads.ctox_approval.request` bleibt der kanonische Command.
- Die vorhandene Re-Autorisierung bei Approve bleibt bestehen.
- Neu ist die Konvergenz eines nativen Denials in einen delegierbaren Vorgang,
  sofern die Aktion delegierbar ist.
- Nicht delegierbare Denials bleiben harte Fehler mit Audit.

### 5.6 Serverautoritative Policy

#### Vorhanden und zu bewahren

- Permission-Vokabular für Collection- und Record-Scope
- explizite Grants in `business_permission_grants`
- Session-Revocation
- Peer-Revocation
- native Capability-Signaturen
- Data-Access-Review im Release-Flow
- Approve-time Re-Autorisierung

#### Zu ergänzen

- Pull-Hook verwendet exakte `data.read`-Entscheidung.
- Push-Hook verwendet exakte `data.write`-Entscheidung.
- Capability bindet Actor Epoch oder gleichwertigen Revocation State.
- Rollenänderung, Deaktivierung und Grant-Entzug erhöhen die Epoch oder
  widerrufen die Capability.
- Ungültige vorhandene Tokens sind fail-closed.
- Verhalten bei fehlendem Token und nicht erreichbarem Control Plane wird als
  typed Runtime Policy definiert, nicht über einen neuen Env-Schalter.
- Vor dem Enforcement werden bestehende Module, Rollen und Benutzer mit
  erforderlichen Grants migriert oder bewusst gelockt.
- Browser-Helper spiegeln Entscheidungen nur für UX.
- Der spätere Executor wird weiterhin nativ autorisiert.

Die maximale Revocation-Latenz wird in Phase 0 festgelegt und durch einen
Integrationstest belegt.

### 5.7 Release, Version, Install und Distribution

#### Vorhanden und wiederzuverwenden

- versionierte Release-Records
- Release- und Rollback-Status
- Data-Access-Review-Gate
- getrennte `apps.release`- und Rollback-Rechte
- Catalog Update mit Staging, atomarem Swap und Backup-Restore
- Recovery-Snapshot und Version Retention
- Update Detection und UI Badge

#### Konkrete Restarbeit

1. `install_app_module` übernimmt sofort das vorhandene Stage-Swap-Restore-
   Pattern. `remove_dir_all` vor erfolgreichem Copy wird entfernt.
2. Install, Reinstall, Template Install und Catalog Update verwenden dieselbe
   idempotente Staging-Hilfe.
3. Alle Aktivierungswege führen Manifest-, Schema-, Import-, Permission- und
   Source-Validation vor dem Swap aus.
4. Release führt Validator, Smoke und die für den Archetyp erforderlichen E2E-
   Gates aus.
5. Trusted Apps werden signiert und an eine veröffentlichte Revision gebunden.
6. Nicht vertrauenswürdige Third-Party-Apps benötigen eine Sandbox, bevor der
   Store geöffnet wird.
7. Sichtbarkeit stammt aus Release-/Lifecycle-Records, nicht aus SemVer.
8. Die SemVer-Regel wird gleichzeitig in Native, Browser-Mirror und Manifest-
   Lifecycle migriert.
9. Bestehende semver-abgeleitete Sichtbarkeit wird einmalig in explizite
   Release-/Lifecycle-Daten überführt.

Folgende Distributionsachsen bleiben explizit im Modell:

- `install_scope`: core, starter, store, internal, installed
- packaged Catalog
- installed-module Snapshot
- git-ignored `local-modules`-Drop
- Template Instantiation
- Remote/MCP App Creation

Jeder Kanal besitzt eine definierte Validation-, Trust- und Update-Policy.

### 5.8 Design System v2 ohne Klassen-Neustart

Alle 34 Module wurden gerade auf das bestehende Kit migriert. Revision 2 friert
die vorhandenen Namen ein. Es gibt keine zweite flächendeckende Umbenennung.

#### Bestehende Namen bleiben kanonisch

| Funktion | Kanonische bestehende Klasse |
| --- | --- |
| Workbench | `ctox-workspace` |
| Pane | `ctox-pane`, `ctox-pane-header`, `ctox-pane-body` |
| Toolbar | `ctox-toolbar` |
| Buttons | `ctox-button`, `ctox-icon-button` |
| Felder | `ctox-input`, `ctox-select`, `ctox-textarea` |
| Suche/Filter | `ctox-pane-search`, `ctox-pane-filter` |
| Tabelle | `ctox-table`, `ctox-table-wrap`, `ctox-table-sort` |
| Liste | `ctox-list`, `ctox-list-item` |
| Modal | `ctox-modal` |
| Context Menu | `ctox-context-menu` und Shell-Menu-Primitive |
| Status | `ctox-badge` |
| Empty State | `ctox-empty` |

Neue Namen wie `ctox-workbench`, `ctox-data-grid`, `ctox-dialog`,
`ctox-empty-state` oder `ctox-status` werden nicht parallel eingeführt. Falls
ein späterer Rename unvermeidbar ist, erfolgt er über Aliase, Deprecation-
Telemetry und ein eigenes Migrations-Gate.

#### Echte Ergänzungen

- Tool Group
- Tree
- gemeinsamer Drawer-/Inspector-Controller
- Error State
- Progress
- Run Control
- optionaler In-Pane-Data-Skeleton, klar getrennt von der App Loading Shell

#### Token- und Stilrichtung

- kompakt, präzise, flach und funktional
- getönte neutrale Flächen in Light und Dark
- verbundene Panes mit 1-px-Separatoren statt schwebender Karten
- 2 bis 4 px Radien für normale Controls
- Pill-Form nur für Tags, Chips und Status
- Schatten nur für Fenster, Popover, Kontextmenüs und notwendige Overlays
- keine dekorativen Gradients oder Glass-Flächen als Standard
- feste Density-Stufen und Row Heights
- Motion zwischen 150 und 250 ms nur für Zustandswechsel

Die Hauptarbeit beginnt in `app.css` und `shared/base.css`, nicht in 34
Einzelapps. Gleichzeitig werden harte Ausreißer wie `matching` und
`desktop` separat bereinigt.

Eine Retokenisierung aktualisiert gemeinsam:

- `app.css`
- `shared/base.css`
- `SHELL_TOKEN_NAMES` in `assert-module-conformance.mjs`
- Token-Regeln in `module_static_check.mjs`
- `references/design-guide.md`
- `DESIGN.md`

`DESIGN.md` beschreibt die kanonische visuelle Absicht. `design-guide.md`
bleibt der technische Agentenvertrag und verweist auf oder wird aus dem
kanonischen Stand aktualisiert. Es entstehen keine zwei widersprüchlichen
Designquellen.

#### Signature Action und Run Control

Jede App benennt ihre einzigartige Hauptautomation mit einem fachlichen Verb,
beispielsweise `Recherche starten` oder `Abrechnung erzeugen`.

Der Run Control bildet ab:

```text
Bereit -> Freigabe nötig -> Warteschlange -> Läuft -> Fertig
                                      \-> Fehlgeschlagen -> Wiederholen
```

Er zeigt Command-ID, Run-ID, Status, Fortschritt, Abbruch, Retry und
Ergebniszugang. Er ist kein dekorativer Primary Button.

### 5.9 Loading-Shell-Invariante

App-Level-Loading bleibt automatisch aus dem echten statischen Layout
abgeleitet:

- `index.html` bleibt für jede App und jedes Template verpflichtend.
- `index.css` beschreibt die reale initiale Geometrie.
- Die Shell leitet daraus die Loading Shadow ab.
- Windowed Mounts erhalten denselben Mechanismus wie Tab-/Full-Workspace-
  Mounts.
- Templates erzeugen keinen handgebauten Full-App-Skeleton.
- Ein Skeleton-Primitive ist ausschließlich für Datenplatzhalter nach dem Mount
  erlaubt.
- Validator und Archetyp-Fixtures prüfen diese Unterscheidung.

### 5.10 App-Archetypen und Templates

Es gibt fünf kanonische Archetypen:

1. **Record Workbench:** Liste, Suche, Filter, Tabelle, Inspector, Editieren.
2. **Queue/Workflow:** Inbox, Status, Detail, Freigabe, Übergabe.
3. **Editor/Document:** Explorer, Editor oder Canvas, Eigenschaften, Save State.
4. **Automation:** Eingangsdaten, Konfiguration, Run Control, Ergebnisse.
5. **Timeline/Thread:** Quellen, Verlauf, Kontext, Antwort und Delegation.

Planner, Secure Form, Store, Browser, Review und Multi-Pane sind Varianten
dieser fünf Archetypen, keine zusätzlichen ungebauten Templates.

Jeder Archetyp enthält:

- Windowed-, Maximized- und gegebenenfalls Focus-Darstellung
- Container-basierte responsive Zustände
- echte Command-Bus-Beispiele
- direkte CRUD-Beispiele mit Grants
- v1/v2 Context Targets
- erlaubte und verweigerte Rechtepfade
- bestehende Threads-Approval-Delegation
- Empty, Error, Offline und Permission States
- auto-derived App Loading Shell
- Light und Dark
- DE und EN
- Tests und Validator-Fixtures

Der Template Compiler schreibt App-ID, Collection-Namen, Schema-IDs, Imports,
Command Types, Persistenzschlüssel und Tests konsistent um. Mehrere Template-
Instanzen teilen niemals Collections oder lokale Schlüssel.

### 5.11 Creator und Agenten-Skills

Creator, MCP-Create, Prepare-App-Source, Template Store und externer Skill
werden auf einen kanonischen Starter zurückgeführt.

Heute zu beseitigende Divergenzen:

- MCP-sourced `ctox.business_os.app.create` überspringt die Starter-
  Materialisierung.
- `prepare_app_source` materialisiert, validiert aber nicht synchron wie der
  Creator-Pfad.
- Der Starter liegt als harte Rust-Strings vor.
- Template Store kopiert andere Quellen und validiert nicht gleichwertig.

Zielablauf:

1. fachliche Aufgabe und Benutzer bestimmen
2. Datenmodell und Grants bestimmen
3. Archetyp auswählen
4. Signature Action und Command definieren
5. Context Targets definieren
6. Presentation Modes und Mindestgröße bestimmen
7. aus dem kanonischen Starter erzeugen
8. statisch und im Browser validieren
9. als Draft installieren
10. Review und Release durchführen

Der App-Development-Skill wird als MCP Resource oder eigenständig
installierbares, versioniertes Paket ausgeliefert. Ein Host-lokaler Pfad ist
kein gültiger Remote-Contract.

## 6. Programmstruktur und Sequenzierung

Die folgenden Tracks sind nicht vollständig seriell.

```text
Phase 0: Baseline, Landing, Freeze und Entscheidungen
       |                         |
       v                         v
Track A: Policy/Release      Track B: Presentation/Design/QA
       |                         |
       +-----------+-------------+
                   v
        Phase 2: Context/Command Integration
                   v
        Phase 3: Creator, Templates, Store, Skills
                   v
        Phase 4: Piloten und Contract Cleanup
                   v
        Phase 5: App-Migrationswellen
                   v
        Phase 6: Legacy Removal und Release
```

Track A blockiert nicht den Beginn von Presentation oder Design. Track A muss
aber vor dem Exit von Phase 2 und vor jeder App-Migrationswelle grün sein.

### Phase 0: Baseline, Landing und Quick Wins

#### 0A: In-flight Windowed-Arbeit sichern

- Owner für die uncommitted Änderungen bestimmen.
- Diff aus App Manifests, Registry, `app.js`, Window Manager, Desktop Launcher,
  Tests und Docs als Einheit inventarisieren.
- doppelte Root-/Layout-`launch_kind`-Felder entscheiden.
- gegen aktuellen `HEAD` reconcilen.
- Tests ausführen und den Stand separat landen.
- erst danach Welle A als bestehende Kohorte behandeln.

#### 0B: Verifiziertes Gap-Inventar

- für jede Zielinvariante Status `vorhanden`, `teilweise`, `fehlend` erfassen
- Beweis aus Code und Test verlinken
- vorhandene Release-/Rollback-/Revocation-Arbeit nicht duplizieren
- Ist-Dokumentation gegen Code korrigieren, besonders
  `src/apps/business-os/ARCHITECTURE.md`

#### 0C: Früher Store-Quick-Win

- `install_app_module` auf die vorhandene Stage-Swap-Restore-Hilfe umstellen
- Reinstall idempotent machen
- Delete-before-copy als Guard-Test abdecken

#### 0D: Divergenz-Freeze

- die bestehenden 15 Contextmenu-Module rekursiv als schrumpfende Allowlist
  erfassen
- neue lokale Kontextmenüs in CI verbieten
- aktuelles app-spezifisches Standard-Komponenten-CSS als Baseline erfassen
- neue Full-Workspace-only-Apps und neue Primitive-Reimplementierungen sperren
- vorhandene Token-Gates weiterverwenden

#### 0E: Entscheidungen und Sizing

- fünf Hypoport-Modul-IDs benennen
- Accessibility-Ziel festlegen, empfohlen WCAG 2.2 AA
- Revocation-Latenzbudget festlegen
- Mindestfenstergröße bestätigen
- Focus-Chrome festlegen
- Touch als Scope oder Nichtziel entscheiden
- Warm-Mount- und Interaction-Latency-Budgets festlegen
- Auto-Restore offener Fenster entscheiden
- Design-Signoff-Owner benennen
- Third-Party-Trust-Modell entscheiden
- jede App mit S/M/L und Risiko bewerten
- Teamkapazität und maximal parallele High-Risk-Migrationen festlegen

Exit Gate:

- Windowed-Diff ist gelandet oder bewusst verworfen.
- Quick-Win verhindert Delete-before-copy.
- Gap-Inventar und Entscheidungen sind dokumentiert.
- Freeze-Guards sind grün.
- Piloten und Aufwand sind benannt.

### Track A: Policy und Release Hardening

- exakte `data.read`-Entscheidung im Pull-Hook
- exakte `data.write`-Entscheidung im Push-Hook
- Grant-Seeding und Locked-State-Migration für bestehende Apps
- Capability Epoch oder gleichwertige Revocation
- typed Fail-Closed-/Offline-Policy ohne neuen Env-Schalter
- Release-Record als Sichtbarkeitsquelle
- koordinierte Native-/Browser-/Manifest-Migration der SemVer-Regel
- Validator, Smoke und E2E als Release Gates
- Pre-Activation-Validation für alle Install-/Update-Wege
- Signatur für Trusted Apps oder Sandbox-Entscheidung

Track-A-Exit:

- negative Read-/Write-Tests bestehen nativ und im Browser.
- ungültiger oder widerrufener Actor verliert Zugriff innerhalb des Budgets.
- bestehende Benutzer besitzen korrekte Grants oder sichtbare Locked States.
- SemVer allein veröffentlicht keine App.
- alle Aktivierungswege sind stage-validiert und rollbackfähig.

### Track B1: Presentation Runtime

- finalen Presentation Contract festlegen
- Compatibility Reader für alle alten Felder implementieren
- in-flight `launch_kind`-Arbeit integrieren
- Registry und Guard-Duplikate atomar migrieren
- Window Host als CSS Container ausstatten
- echte Pane-/Drawer-Slots für Windowed Mounts liefern
- Focus Mode ergänzen
- bestehende Minimize-/Maximize-/Snap-/Geometriepfade bewahren
- Deep Links und Launch Args vereinheitlichen
- Windowed-attribution im Context Handler korrigieren
- auto-derived Loading Shell in Windowed Mount integrieren
- Modul-/Collection-Syncs, Subscriptions und Timer an Window-Lifecycle und
  sichtbares Interesse koppeln; Close/Unmount muss ein messbares Ressourcen-
  Budget freigeben
- 35-App-Sequenzstress mit Mount-Latenz, aktiven Collections, Peer-Loops,
  Timern und Speichertrend als eigenes Gate aufnehmen
- Full-Workspace-Validator und sieben Skill-Docs in diesem Track ändern, nicht
  erst am Projektende

Track-B1-Exit:

- Referenzapp läuft Windowed, Maximized und Focus ohne Remount.
- Compact, Standard und Wide reagieren auf Container Resize.
- Auswahl, Scrollposition und Entwurf bleiben erhalten.
- Deep Link fokussiert die korrekte App und Record-ID.
- Loading Shadow funktioniert im Windowed-Pfad.
- 35 nacheinander geoeffnete/geschlossene Apps benoetigen keinen Peer- oder
  Browserneustart und ueberschreiten das definierte Warm-Mount-Budget nicht.
- alle Presentation Guards sind grün.

### Track B2: Design System und QA-Infrastruktur

- `PRODUCT.md` und `DESIGN.md` bestätigen
- Shell Tokens und `base.css` auf die kompakte Richtung kuratieren
- bestehende Klassennamen behalten
- harte Modulausreißer bereinigen
- Run Control und fehlende Primitive ergänzen
- Design Lab aufbauen
- Screenshot-Matrix für drei Breiten, zwei Themes und zwei Locales bauen
- Visual-Diff-Vergleich mit Baseline und Review-Artefakt bauen
- Keyboard-/Accessibility-Harness bauen
- Reduced-Motion-Plattformregel und Gate bauen
- parametrisierten per-App Contract-Test auf bestehendem Playwright-Harness bauen
- Performance-Messung für Warm Mount und Kerninteraktion ergänzen

Track-B2-Exit:

- jedes DoD-Gate besitzt ausführbares Tooling oder ist ausdrücklich manuell.
- manuelle visuelle Gates besitzen Owner und gespeichertes Signoff-Artefakt.
- kein neuer Klassen-Rename ist nötig.
- Light/Dark und Compact/Standard/Wide sind reproduzierbar prüfbar.

### Phase 2: Context, Commands und Delegation

Join Gate: Track A muss für Grant-Semantik und Revocation grün sein. Track B1
muss Windowed Attribution und Presentation Context liefern.

- v1/v2 Context Dual-Read implementieren
- `ctx.contextActions` in Mount-Contract, Doc und Guard aufnehmen
- Attribut-Mapping für bestehende v1-Targets
- Window-, Pane-, Field-, Pointer- und Multi-Selection-Kontext ergänzen
- ContextMenu-Key und Shift+F10 unterstützen
- Adapter für native Editor-/Spreadsheet-/Desktop-/Window-Header-Menüs
- stumme Event-Zustellung durch shellgelieferten Agent Dispatcher ersetzen
- `threads.ctox_approval.request` wiederverwenden
- delegierbare native Denials in Threads überführen
- Permission Decision, Scope und Resultat auditieren
- Legacy-Handler erst nach Coverage entfernen

Exit Gate:

- Rechtsklick auf Windowed Record wird der richtigen App zugeordnet.
- Record, Collection, Feld, Pointer und Auswahl stimmen im Command überein.
- erlaubte Direct-CRUD- und Command-Pfade funktionieren.
- verweigerte delegierbare Aktion erscheint dauerhaft in Threads.
- nicht gemounteter Chat kann keine Aktion mehr stumm verschlucken.
- Refresh erhält Delegation und Audit.

### Phase 3: Creator, Templates, Store und Skills

- kanonischen Starter aus einer Quelle herstellen
- Client-only-App-Vertrag und `data_runtime`-Manifest normalisieren
- Runtime Action Registry plus sicheren generischen Mutation-/Saga-Executor
  ohne per-App-Rust-Handler implementieren
- automatische Hot-Registrierung neuer App-Collections und Actions beim
  Install/Update implementieren
- Creator-, MCP- und Prepare-App-Source-Pfade angleichen
- synchronen Validator überall gleich ausführen
- fünf Archetypen und Varianten implementieren
- Template Compiler mit echtem Namespace-Rewrite
- `index.html`- und Loading-Shell-Invariante in Validator aufnehmen
- Presentation Contract in Creator und Skills lehren
- Full-Workspace-Mandat vollständig entfernen
- Remote Skill Resource ausliefern
- Draft, Review, Release, Update und Rollback in Store UI darstellen
- install_scope- und local-modules-Policy dokumentieren

Exit Gate:

- extern erzeugte App besteht ohne manuelle Reparatur alle Gates.
- eine neue realtimefähige Multiuser-App mit eigener Collection und eigener
  deklarativer Action läuft auf einem bestehenden Binary ohne Rust-Änderung,
  Recompile oder manuellen Daemon-Neustart.
- zwei Template-Instanzen besitzen getrennte Namespaces.
- Starter validiert sich selbst in Creator, MCP und Template Store.
- Remote-Agent kann alle Skill-Ressourcen lesen.
- Windowed App wird vom offiziellen Validator akzeptiert.

### Phase 4: Piloten und Contract Cleanup

Pilotkohorten:

1. die fünf benannten Hypoport-Apps
2. die gelandete Windowed-Kohorte
3. mindestens eine komplexe Drei-Pane-App
4. mindestens eine Editor-/Canvas-App mit Focus Mode
5. mindestens eine Automation mit Run Control

Eine Pilotapp wird genau einmal migriert. Nach der Pilotbereinigung werden alle
Piloten gegen den finalisierten Contract erneut gegated; sie erscheinen in
späteren Wellen nur noch als bereits erledigte Zeilen.

Exit Gate:

- fünf Archetypen oder ihre notwendigen Varianten sind validiert.
- keine Pilotkorrektur erfordert app-spezifische Plattformlogik.
- alle Piloten bestehen Production Gates nach der letzten Contract-Bereinigung.

### Phase 5: Migration aller Apps

#### Welle A: Windowed-/Presentation-Kohorte

- `app-store`
- `appsec-pentest`
- `browser`
- `creator`
- `ctox` (Revision 52 abgenommen)
- `knowledge`
- `reports` (Revision 44 abgenommen)
- `threads`
- `tickets`

Voraussetzung: Die acht in-flight Änderungen sind in Phase 0 gelandet. Bereits
als Pilot abgeschlossene Apps werden nicht erneut implementiert, nur erneut
gegated.

#### Welle B: kompakte Register und Workflows

- `consent`
- `credentials`
- `esign`
- `intake`
- `interviews`
- `nachweise`
- `placements`
- `submissions`

#### Welle C: operative Kernworkbenches

- `customers`
- `matching`
- `outbound` (Revision 50 abgenommen)
- `research` (Revision 49 abgenommen)
- `support` (Revision 45 abgenommen; langer 7-App-Batch-Mount-Timeout bleibt
  separates Stabilitätssignal)
- `conversations`
- `coding-agents` (Revision 51 abgenommen)

#### Welle D: Fachsysteme und Planung

- `buchhaltung`
- `calendar`
- `invoices`
- `iot`
- `shiftflow`

#### Welle E: Dokument, Tabelle und Content

- `cv-print-builder`
- `documents`
- `notes`
- `spreadsheets`

#### Welle F: Shell-Ausnahme

- `desktop`

Zusätzlicher Compatibility Track:

- installierte Store-Snapshots
- `local-modules`
- kundenspezifische nicht im Core-Registry-Inventar enthaltene Apps

Sie werden nicht still durch `base.css` v2 restyled. Sie benötigen Alias-
Kompatibilität, Re-Release oder eine explizite Inkompatibilitätsmeldung.

Vor jeder Welle wird S/M/L-Sizing gegen verfügbare Teamkapazität bestätigt.
Maximal zwei High-Risk-Apps werden gleichzeitig migriert. Eine Welle schließt
erst, wenn ihre Apps und Compatibility-Fälle grün sind.

### Phase 6: Legacy Removal und Release

- v1 Context Fallback erst nach Null-Telemetrie entfernen
- tote lokale Kontextmenüs entfernen
- Event-only-Agent-Dispatcher entfernen
- alte Presentation-Felder nur nach vollständigem Compatibility-Nachweis
  entfernen
- deprecated Design-Aliase entfernen
- veraltete Starter und Skills entfernen
- widersprüchliche Dokumentation aktualisieren
- finalen Security-/Privacy-Signoff durchführen
- offenen Store nur nach Signatur- oder Sandbox-Gate freigeben

## 7. App-Migrationsmatrix

`Heute Worktree` beschreibt den bei Planrevision sichtbaren Checkout und ist
nicht gleichbedeutend mit `HEAD`. Alle normalen Apps zielen auf Window und
Maximized; Focus ist fachlich optional.

| App | Heute Worktree | Ziel | Kanonischer Archetyp | Variante / besondere Prüfung |
| --- | --- | --- | --- | --- |
| app-store | windowed, verified r46 | window + maximized | Record Workbench | Store, atomare Installation, Release |
| appsec-pentest | windowed, in-flight | window + maximized | Queue/Workflow | Freigaben, lange Evidence |
| browser | windowed, verified r43 | window + maximized + focus | Editor/Document | Browser-Variante, Security Boundary |
| buchhaltung | full-workspace | window + maximized | Record Workbench | Tabellen, DATEV, dichte Formulare |
| calendar | full-workspace | window + maximized | Queue/Workflow | Planner-Variante, Zeitraster, Drag |
| coding-agents | full-workspace, verified r51 | window + maximized | Queue/Workflow | Runs, Terminalstatus, Rechte |
| consent | full-workspace | window + maximized | Record Workbench | kompaktes Register |
| conversations | full-workspace | window + maximized | Timeline/Thread | Kanalfilter, lange Verläufe |
| creator | windowed, verified r48 | window + maximized | Queue/Workflow | Starter und App Review |
| credentials | full-workspace | window + maximized | Record Workbench | Secure-Form-Variante, Write-only |
| ctox | windowed, verified r52 | window + maximized | Queue/Workflow | Live Runs und Statusdichte |
| customers | full-workspace | window + maximized | Record Workbench | CRM und Auswahlpersistenz |
| cv-print-builder | full-workspace | window + maximized + focus | Editor/Document | PDF Preview und Split View |
| desktop | full-workspace | shell-surface | Shell-Ausnahme | Window Manager und Launcher |
| documents | unspecified | window + maximized + focus | Editor/Document | native Menüs, Loading Shell |
| esign | full-workspace | window + maximized | Queue/Workflow | Status und Freigabe |
| intake | full-workspace | window + maximized | Queue/Workflow | Import und Deduplizierung |
| interviews | full-workspace | window + maximized | Queue/Workflow | Scorecards und Termine |
| invoices | full-workspace | window + maximized | Record Workbench | Dokumente und Finanzstatus |
| iot | full-workspace | window + maximized + focus | Automation | Canvas-Variante, Widgets, Signalbaum |
| knowledge | windowed, in-flight | window + maximized + focus | Editor/Document | Data-Variante, Markdown, Dataframes |
| matching | full-workspace | window + maximized | Record Workbench | Multi-Pane-Variante, Design-Ausreißer |
| nachweise | full-workspace | window + maximized | Record Workbench | Ablaufstatus und Evidence |
| notes | full-workspace | window + maximized + focus | Editor/Document | native Textinteraktion |
| outbound | full-workspace, verified r50 | window + maximized | Automation | Import, Qualifizierung, Runs |
| placements | full-workspace | window + maximized | Queue/Workflow | Garantie- und Honorarstatus |
| reports | windowed, verified r44 | window + maximized | Queue/Workflow | Review-Variante, Rollback, Evidence |
| research | full-workspace, verified r49 | window + maximized + focus | Automation | Quellen, Karten, Ergebnis |
| shiftflow | full-workspace | window + maximized | Queue/Workflow | Planner-Variante, Zeitplanung, Drag |
| spreadsheets | unspecified | window + maximized + focus | Editor/Document | Spreadsheet-Context-Interop |
| submissions | full-workspace | window + maximized | Queue/Workflow | Consent und Doppelvorstellung |
| support | full-workspace, verified r45 | window + maximized | Queue/Workflow | Timeline-Variante, SLA, Makros |
| threads | windowed, in-flight | window + maximized | Timeline/Thread | Approval, Denial Path, Delegation |
| tickets | windowed, verified r47 | window + maximized | Queue/Workflow | Command-Bus-Referenz |

## 8. Definition of Done pro App

Eine App gilt erst als migriert, wenn alle zutreffenden Punkte erfüllt sind.

### Plattform

- [ ] kanonischer Presentation Contract oder geprüfter Compatibility-Eintrag
- [ ] ein Mount für Window, Maximized und optional Focus
- [ ] keine eigene HTTP-Datenbrücke
- [ ] keine app-eigene Standard-Persistenz außerhalb der vorgesehenen Stores
- [ ] sauberes Mount/Unmount ohne Listener- oder Subscription-Leak
- [ ] `index.html` bleibt repräsentativer statischer App-Aufbau
- [ ] App Loading Shell wird automatisch abgeleitet

### Responsive und Window Manager

- [ ] Compact bei der vereinbarten Mindestgröße nutzbar
- [ ] Standard und Wide ohne unerwarteten horizontalen Seitenscroll
- [ ] Pane-Reduktion nutzt gemeinsame Drawer-/Stack-Mechanik
- [ ] Resize erhält Auswahl und Entwurf
- [ ] Maximize, Restore, Minimize, Snap und Reload getestet
- [ ] Focus verliert keinen App-Zustand
- [ ] Deep Link öffnet oder fokussiert die richtige App und den Record

### Daten, Kontext und Aktionen

- [ ] direkte Reads besitzen native `data.read`-Entscheidung
- [ ] direkte Writes besitzen native `data.write`-Entscheidung
- [ ] fachliche Records als v1/v2 Context Targets registriert
- [ ] relevante Felder und Mehrfachauswahl erfasst
- [ ] Windowed Context trägt korrekte App- und Window-ID
- [ ] Agentenaktionen und Automationen über Typed Command Bus
- [ ] keine fachliche Aktion hängt von gemountetem Business Chat ab
- [ ] fehlende delegierbare Rechte führen über bestehenden Approval-Command zu Threads
- [ ] Audit enthält Scope, Permission Decision und Resultat

### Design

- [ ] bestehende kanonische Kit-Namen verwendet
- [ ] keine lokale Reimplementierung von Standard-UI
- [ ] fachliche Signature Action als Run Control
- [ ] Light und Dark über Screenshot-Matrix geprüft
- [ ] keine nicht genehmigten Glass-, Gradient- oder Card-Stacks
- [ ] Empty, Error, Offline und Permission States vollständig
- [ ] In-Pane-Skeleton klar von App Loading Shell getrennt
- [ ] DE und EN bei Compact, Standard und Wide geprüft

### Accessibility und Performance

- [ ] vollständige Tastaturbedienung
- [ ] sichtbarer Focus
- [ ] semantische Rollen und Labels
- [ ] Context Menu per Tastatur erreichbar
- [ ] festgelegtes Kontrastziel erfüllt
- [ ] Reduced Motion respektiert
- [ ] Warm Mount und Kerninteraktion liegen innerhalb der Phase-0-Budgets

### Browser-E2E

- [ ] App in frischem Profil öffnen
- [ ] echte Daten sichtbar
- [ ] Filter, Sortierung, manuelles Editieren und Persistenz prüfen
- [ ] Signature Action starten und Status verfolgen
- [ ] Rechtsklick auf Windowed Record und Feld prüfen
- [ ] erlaubte Aktion prüfen
- [ ] verweigerte Aktion und Delegation prüfen
- [ ] Reload und Wiederaufnahme prüfen
- [ ] keine Console Errors, Request Failures oder unerwartete 404

## 9. Test- und Release-Gates

### 9.1 Vorhandene Gates, die erweitert werden

- Manifest- und Schema-Validierung
- `assert-module-conformance.mjs`
- `module_static_check.mjs`
- Mount-Context-Contract-Guard
- Global-Context-Policy-Guard
- Module Registry-/Launcher-Smokes
- Business OS JS Suite
- native RxDB- und Policy-Tests
- Browser/Rust Smoke Matrix

### 9.2 Phase-0-Freeze-Gates

- keine neuen lokalen `contextmenu`-Handler außerhalb der schrumpfenden Allowlist
- rekursiver Scan aller Modul-JS-Dateien, nicht nur `index.js`
- keine neuen Full-Workspace-only-Apps
- keine neue lokale Standard-Komponentenfamilie
- keine neue Token-Neudefinition

### 9.3 Neue Infrastruktur vor den Piloten

1. **Screenshot Matrix Runner**
   Drei Containergrößen, Light/Dark, DE/EN, deterministische Fixtures.
2. **Visual Diff Runner**
   Vergleich gegen Baseline, tolerierte Abweichung, gespeichertes Artefakt.
3. **Accessibility Contract Runner**
   Keyboard, Focus, Rollen, Namen, Kontrast und Reduced Motion.
4. **Parametrisierter App Contract Runner**
   Mount, Resize, Deep Link, Context v2, CRUD Grant, Command, Denial,
   Delegation, Reload und Console-/Network-Health.
5. **Performance Probe**
   Warm Mount, Loading-Shadow-Sichtbarkeit und Kerninteraktionslatenz.

Wenn ein Gate nicht automatisierbar ist, benennt der Plan einen Owner und das
zu speichernde Signoff-Artefakt. `visuell abgenommen` ohne Owner und Evidence
ist kein Gate.

### 9.4 Release-Gate

Ein Release benötigt:

- statischen Validator
- Archetyp-Smoke
- erforderliche E2E-Story
- Policy- und Grant-Negativtests
- Screenshot-/Visual-Diff-Artefakt
- Accessibility-Ergebnis
- Design-Signoff bei materieller UI-Änderung
- Source-/Manifest-Signatur für Trusted Store Apps
- funktionierendes Rollback-Ziel

### 9.5 Installiertes Production Gate

Die Source- und Workspace-Gates reichen nicht für einen Rollout. Vor einer
Produktionsfreigabe muss dieselbe gebaute und installierte Release-Version
folgende Stories in frischen Browserprofilen bestehen:

1. **Bootstrap:** Shell meldet `status.ok=true`, alle erforderlichen Peers sind
   aktiv und der native Peer meldet `replicationUp=true`.
2. **34-App-Langlauf:** jede Core-App öffnet innerhalb des festgelegten Budgets;
   danach funktionieren die zuerst geöffneten Apps weiterhin ohne Remount-
   oder Query-Owner-Schaden.
3. **Transport Health:** keine `masterChangesSince`-/`masterWrite`-Timeouts,
   keine unerwartet geschlossenen Broadcast Channels und keine anhaltenden
   Multi-Tab-Owner-Timeouts.
4. **Zwei Profile:** zwei parallele Clean Profiles können dieselben erlaubten
   Collections lesen und schreiben, ohne dass eines den Query-/Replication-
   Owner des anderen beschädigt.
5. **Command End-to-End:** UI-Aktion erzeugt einen typisierten Payload, der
   Command wird in `business_commands` persistiert, nativ konsumiert und sein
   finaler Status samt Audit ist über Browser und MCP auffindbar.
6. **Context Matrix:** Rechtsklick, ContextMenu-Key und Shift+F10 liefern für
   Record und Feld identischen App-/Window-/Pane-/Collection-/Entity-/Field-
   Kontext.
7. **Policy Matrix:** erlaubte Admin-Aktion, verweigerte Nicht-Admin-Aktion und
   delegierbarer Denial→Threads-Approval werden mit getrennten Benutzern
   getestet; Approve re-autorisiert nativ.
8. **Persistenz:** manuelles Editieren, Reload, Browserneustart und zweites
   Profil zeigen denselben erlaubten Datenstand.
9. **Auth:** Login, Logout, Session-Revocation und Clean-Profile-Zugriff werden
   gegen die reale Zielkonfiguration getestet. Für reine Local-Auto-Admin-
   Builds wird dieses Gate ausdrücklich als `not-applicable`, nicht als grün,
   protokolliert.
10. **Health Budget:** keine ungeklärten Console Errors, Page Errors, Request
    Failures oder unerwarteten 404; bekannte harmlose Meldungen benötigen eine
    enge, begründete Allowlist.
11. **Window/Dock Composition:** persistente Docks, Chat und App-Windows dürfen
    keine sichtbaren Controls anderer fokussierter Surfaces unbedienbar machen;
    Pointer-, Keyboard- und Taskbar-Focus werden mit überlappenden Surfaces
    getestet.

Gate 11 ist seit Revision 9 technisch grün: automatisierter Fünf-Phasen-E2E
und installierter V16-Lauf belegen Side-/Bottom-/Compact-Komposition, echten
Pointer auf der Threads-Freigabeaktion, Enter-Aktivierung der Taskbar sowie
null pointeraktive Overlap-Fläche bei 1280×720 und 900×600.

Der Runner speichert pro Release JSON, Markdown, Screenshots und Console-/
Network-Logs. Ein Timeout wird weiter untersucht und nicht durch eine größere
pauschale Wartezeit grün gerechnet.

## 10. Migrationsstrategie

- Bestehende Daten und Command Types bleiben zunächst kompatibel.
- Presentation und Context v2 werden additiv eingeführt.
- Legacy-Verwendung wird telemetriert.
- Apps werden wellenweise migriert.
- Gemeinsame Probleme werden in SDK, Policy oder Kit behoben, nicht kopiert.
- Businesslogik wird nur geändert, wenn ein Test oder Vertrag einen Fehler zeigt.
- Jede Migration besitzt eine Rückfallversion.
- Pilotapps werden nicht doppelt implementiert.
- Installierte und lokale Apps erhalten einen eigenen Compatibility Track.
- Keine Massenkonvertierung darf 34 Apps syntaktisch ändern und statisch als
  fertig markieren.
- Änderungen an `layout.shell` oder Presentation können aktive Module neu
  aktivieren; Rollout und User-Kommunikation berücksichtigen dies.

## 11. Risiken und Gegenmaßnahmen

| Risiko | Gegenmaßnahme |
| --- | --- |
| In-flight Windowed-Arbeit geht verloren | Phase-0-Landing vor jeder abhängigen Welle |
| UI-Refactor beschädigt Fachlogik | Daten- und Command-Tests vor UI-Migration einfrieren |
| Direct CRUD umgeht Grants | native Pull-/Push-Entscheidung pro Collection/Record |
| Chat-Event verschluckt Aktion | shellgelieferter persistierender Dispatcher |
| Windowed Context zeigt falsche App | Window-/Host-Attribution statt `state.activeModule` |
| Design System erzwingt zweite Markup-Migration | bestehende Kit-Namen einfrieren |
| Flat Design ändert alle Apps unkontrolliert | Shell-Token-Change plus Visual-Matrix und Ausreißerinventar |
| Loading Shell wird pro App neu gebaut | `index.html`-Derivation und Validator-Gate |
| Responsive Regeln werden app-spezifisch | Container-Layout als SDK und Contract-Test |
| Delegation umgeht Rechte | bestehende Approve-time Re-Autorisierung beibehalten |
| Grant-Enforcement sperrt Bestandsnutzer aus | Grant-Seeding und sichtbare Locked-State-Migration |
| Capability-Fail-Closed bricht Offline-Modus | typed Offline-Policy und Rollout-Test |
| Store-Reinstall verliert aktive App | früher Stage-Swap-Restore-Quick-Win |
| SemVer-Migration versteckt Apps | koordinierte Native-/Browser-/Manifest-Datenmigration |
| Third-Party-App gefährdet Same Origin | Signatur oder Sandbox vor offenem Store |
| DoD bleibt Papierliste | Tooling vor Piloten, Owner für manuelle Gates |
| Piloten werden doppelt migriert | migrate-once und Re-Gate-Regel |
| Ein Browser ist grün, weitere Profile verhungern | paralleler Zwei-Profil-Test mit getrennten Clean Profiles |
| Langlauf zerstört Query-/Replication-Owner | 34-App-Sequenz plus erneuter Zugriff auf frühe Apps als Release-Gate |
| UI zeigt einen Command, Native Store kennt ihn nicht | Command-E2E mit Browser-ID, nativer Store-Suche, Consumerstatus und Audit |
| Transportfehler erscheinen als App-Mountfehler | gemeinsame Transport-Telemetrie und Fehlerklassifikation vor App-Triage |
| Persistenter Chat verdeckt fokussierte App-Controls | Dock reserviert Workspace-Fläche oder nimmt am Window-Manager-Occlusion-/Z-Order-Modell teil; Overlap-E2E |

## 12. Erfolgskriterien

Der Refactor ist abgeschlossen, wenn:

1. jede normale Core-App responsive in einem Fenster nutzbar ist,
2. Maximized, Snap und optional Focus denselben Zustand weiterverwenden,
3. normales CRUD native Read-/Write-Grants erzwingt,
4. Agentenaktionen und Automationen als Commands persistieren,
5. jeder relevante Rechtsklick exakten App-, Window-, Record- und Feldkontext
   erzeugt,
6. fehlende delegierbare Rechte dauerhaft in Threads landen,
7. Light und Dark dieselbe kompakte, zeitlose Designsprache verwenden,
8. bestehende Kit-Namen stabil bleiben,
9. Loading Shells automatisch aus dem echten App-Aufbau entstehen,
10. Creator, Templates, Store und Agent-Skills denselben Starter verwenden,
11. Release und Aktivierung validiert, atomar, signiert oder sandboxed und
    rollbackfähig sind,
12. jede App das gemeinsame Browser-, Policy-, Visual- und Accessibility-Gate
    besteht,
13. installierte und lokale Apps eine dokumentierte Compatibility-Policy haben,
14. persistente Docks und Fenster sich nicht gegenseitig sichtbar, aber
    unbedienbar überlagern.
15. jede Fenster-App mit echtem Single-Click, Maus-Resize und sichtbaren
    Window-Controls bedienbar ist,
16. Zwei-/Drei-Pane-Apps in Window und Mobile fuer jedes reduzierte Pane einen
    sichtbaren Bedien- und Rueckweg behalten,
17. der Chat-Dock bei wachsender Chatanzahl scrollbar im Viewport bleibt und
    ein eingeklappter Dock nicht durch Hintergrundantworten geoeffnet wird,
18. jeder Window-Header Version, Lifecycle-/Sichtbarkeitsstatus, Source-Aktion
    und Versionskontrolle entsprechend den Rechten zeigt,
19. oben links nur das Startmenue verbleibt und App-Icons im Web mit einem
    Klick starten,
20. die Shell bis zur festgelegten Mobile-Untergrenze ohne verdeckte
    Navigation, Headeraktionen, Chat- oder Taskbar-Flaechen funktioniert.
21. alle Apps den Ableton-Dichtevertrag erfuellen: nur Signature-Automationen
    sind raeumlich/visuell dominant; wiederkehrende Controls bleiben in
    Window, Mobile und Touch kompakt, vollstaendig und erreichbar.
22. Desktop-App-Namen mit kurzen, langen und untrennbaren Labels bleiben in
    Desktop und Mobile innerhalb ihrer festen zweizeiligen Rasterzelle.
23. Impeccable-Preflight, lokaler App-Development-Skill und externer
    CTOX-Deploy-Skill schreiben und validieren denselben aktuellen
    App-Presentation-/Design-Vertrag.
24. sequenzielles Oeffnen und Schliessen aller Apps laesst inaktive Sync-
    Interessen, Subscriptions, Timer und Speicher auf einen definierten
    Baseline-Korridor zurueckkehren; kein Browser-/Peer-Neustart ist noetig.

Erfolgskriterium 14 ist mit Revision 9 erfüllt. Der zusätzliche skalierte
Context-/Threads-Nachweis ist mit Revision 10 dauerhaft in der Release-Matrix.
Revision 13 belegt Kriterien 7 und 10 sowie Teile der statischen Breite von
Kriterium 12. Revision 16 belegt die technische Installed-Einzel-App-Abnahme
fuer Kriterien 15 bis 23. Revision 71 belegt Kriterium 24 und schließt den
Client-only-Runtime-Track. Nur die ausdrücklich menschlichen Signoffs und
Produktmetadaten halten die Gesamtfreigabe offen. Revision
24 belegt die gemeinsame Performance-Baseline; Revision 25 belegt
13/34 abgeschlossene visuelle Einzel-App-Migrationen; Revision 26 belegt
14/34 abgeschlossene visuelle Einzel-App-Migrationen und den mobilen Shell-
Stack für mehrspaltige Apps; Revision 27 belegt 15/34 abgeschlossene visuelle
Einzel-App-Migrationen und den kompakten Mobile-Automation-View für
Spreadsheets; Revision 28 belegt 16/34 abgeschlossene visuelle Einzel-App-
Migrationen und die kompakte Finanz-Workbench für Buchhaltung; Revision 29
belegt 17/34 abgeschlossene visuelle Einzel-App-Migrationen und die kompakte
Store-Workbench für den App Store; Revision 30 belegt 18/34 abgeschlossene
visuelle Einzel-App-Migrationen, die kompakte Matching-Workbench und den
Mobile-Sheet-Inset-Fix für windowed Apps. Revision 31 belegt Shell-/Chat-Guard-
Fortschritt, Desktop-Label-Fix und Threads-Source-Verdichtung, zählt aber ohne
frischen Live-Window-Signoff keine zusätzliche App. Das ersetzt ausdrücklich
keinen Nachweis für die damals verbleibenden 16 Apps. Revision 32 belegt den
installierten Live-Window-Signoff für Threads und Matching; Revision 33 belegt
den managed Asset Guard; Revision 34 belegt den isolierten Installer-Smoke für
Business-OS-Shell-Assets; Revision 35 belegt den optionalen Runtime-Build-Skip
und einen vollen grünen `ctox`-Check im Skip-Target. Revision 36 belegt, dass
der echte Full-Install-E2E danach am lokalen Host-Ressourcenfenster scheitert,
nicht an einem erneuten Business-OS- oder Voxtral-Codefehler. Revision 37
belegt 20/34 abgeschlossene visuelle Einzel-App-Migrationen durch die kompakte
Conversations-Timeline inklusive Source-, Validator- und Browser-QA. Revision
38 belegt den weiter offenen Full-Install-Blocker auch bei
`CARGO_BUILD_JOBS=1` als Host-/Release-Build-Ressourcenproblem. Revision 39
belegt 21/34 abgeschlossene visuelle Einzel-App-Migrationen durch Notes und
schließt den generischen Window-Resize-Hitbox-Fix für App-Content mit höherem
lokalem `z-index`. Revision 40 belegt 22/34 abgeschlossene visuelle Einzel-App-
Migrationen durch Knowledge inklusive Source-, Validator- und Browser-QA.
Revision 41 belegt 23/34 abgeschlossene visuelle Einzel-App-Migrationen durch
Shiftflow inklusive Source-, Validator- und Browser-QA. Revision 42 belegt
24/34 abgeschlossene visuelle Einzel-App-Migrationen durch AppSec/Deployment
Audit inklusive Source-, Validator- und Browser-QA. Revision 43 belegt 25/34
abgeschlossene visuelle Einzel-App-Migrationen durch Browser inklusive Source-,
Validator- und Browser-QA. Revision 44 belegt 26/34 abgeschlossene visuelle
Einzel-App-Migrationen durch Reports inklusive Source-, Validator- und Browser-
QA. Revision 45 belegt 27/34 abgeschlossene visuelle Einzel-App-Migrationen
durch Support inklusive Source-, Validator- und Browser-QA; der 7-App-Batch-
Mount-Timeout bleibt getrennt offen. Revision 46 belegt 28/34 abgeschlossene
visuelle Einzel-App-Migrationen durch App Store inklusive Source-, Validator-
und Browser-QA. Revision 47 belegt 29/34 abgeschlossene visuelle Einzel-App-
Migrationen durch Tickets inklusive Source-, Validator- und Browser-QA.
Revision 48 belegt 30/34 abgeschlossene visuelle Einzel-App-Migrationen durch
Creator inklusive Source-, Validator- und Browser-QA. Revision 49 belegt 31/34
abgeschlossene visuelle Einzel-App-Migrationen durch Research inklusive
Source-, Validator- und Browser-QA. Revision 50 belegt 32/34 abgeschlossene
visuelle Einzel-App-Migrationen durch Outbound inklusive Source-, Audience-,
Validator- und Browser-QA. Revision 51 belegt 33/34 abgeschlossene visuelle
Einzel-App-Migrationen durch Coding Agents inklusive Source-, Validator- und
Browser-QA. Revision 52 belegt 34/34 abgeschlossene visuelle Einzel-App-
Migrationen durch CTOX inklusive Source-, Validator- und Browser-QA. Revision
53 belegt, dass lokaler Release-Build und Managed-Asset-Install wieder
durchlaufen, hält den Releaseclaim aber wegen `ctox-real`-UE-Hängern,
altem Service-CWD, fehlendem Port 20876 und nicht sauberem launchd-Neustart
offen. Revision 55 belegt den isolierten Browser/Rust/WebRTC-/Command-Bus-
Codepfad auf separaten Ports, hält den Releaseclaim aber wegen fehlendem
aktuellen Source-Binary und weiterhin offenem installierten launchd-/Peer-
Nachweis ebenfalls offen. Revision 56 grenzt diesen installierten Blocker auf
einen macOS-LaunchAgent-/Gatekeeper-/Provenance-Zustand mit
`OS_REASON_CODESIGNING` und alten `UE`-Prozessen ein. Revision 57 repariert den
installierten lokalen Pfad: Service und Signaling laufen per launchd,
Business-OS, MCP und lokales Signaling lauschen auf 8765/8788/20876, der
installierte 35-App-Browser-QA ist grün, `replicationUp=true` ist belegt und
der Managed-Asset-Guard ist grün. Offen bleibt der automatische Nachweis, dass
ein frischer Managed-Install-/Upgrade-Lauf diese neue LaunchAgent-/Signaling-
Generatorlogik ohne manuelle Reparatur ausrollt. Dieser historische offene
Punkt ist durch Revision 59 geschlossen; Revision 60 ergänzt danach die
dynamische Runtime-App-Fachstory-Evidence.

## 13. Getroffene Entscheidungen und verbleibende Produktmetadaten

Die Phase-0-Entscheidungen sind in `PRODUCT.md`, `DESIGN.md`, der technischen
Baseline und den Manifesten festgeschrieben:

1. Die globale Mindestgröße ist 640 × 480; Apps dürfen größere Startgrößen,
   aber keine höhere Mindestgröße verlangen.
2. Focus behält eine kompakte 28-px-Chrome mit Rückweg in den Window-Modus.
3. Touch unterstützt die grundlegende Aktivierung, ist aber kein
   Optimierungsziel für dichte Workflows.
4. Auto-Restore und Multi-Instance sind für alle migrierten Apps zunächst
   deaktiviert (`auto_restore: false`, `multi_instance: false`).
5. Rollen- oder Grant-Änderungen wirken bei der nächsten nativen Entscheidung
   und spätestens innerhalb von fünf Sekunden; fehlende oder nicht prüfbare
   Tokens sind fail-closed und erzeugen einen typisierten Fehler.
6. Apps erhalten keine beliebige eigene Theme-Farbe. Der gemeinsame Akzent
   bleibt operativ; eine App darf damit genau ihre fachliche Signature Action
   hervorheben.
7. WCAG 2.2 AA, vollständige Tastaturbedienung und Reduced Motion sind
   verbindliche Ziele.
8. Product Owner ist der menschliche Design-Signoff-Owner; die 12-fache
   Screenshot-Matrix liegt unter
   `output/playwright/business-os-design-matrix/`.
9. Warm Mount p95 ≤ 500 ms und sichtbare Interaktionsantwort p95 ≤ 100 ms sind
   die Zielbudgets. Längere Arbeit wird als persistierter Command/Run gezeigt.
10. Trusted Apps benötigen revisionsgebundene Vertrauensevidence. Untrusted
    Third-Party-Apps bleiben bis zu einer echten Sandbox deaktiviert.
11. Keine der 34 vorhandenen Apps wurde archiviert. Alle besitzen den
    Presentation Contract; die vollständige app-spezifische Definition of Done
    ist laut Revision-11-Audit noch nicht für alle Apps gegated.
12. Hochriskante Migrationen werden mit höchstens zwei Apps parallel
    durchgeführt.

Die Chat-Dock-/Window-Komposition ist mit Revision 9 technisch geschlossen;
das skalierte Threads-/Context-Gate ist mit Revision 10 technisch geschlossen.
Revision 11 öffnete keine dieser belegten Fähigkeiten erneut, korrigierte aber
die Breite des Gesamtclaims. Revision 12 schließt die fünf Archetypen und den
vereinheitlichten Starterpfad. Revision 13 schliesst die aktuelle 34-App-
Fachvertragsmatrix und die app-vollstaendige sichtbare Installed-Matrix.
Revision 59 schließt den frischen Managed-Install-/Upgrade-Rollout, den
Current-Source-Build und den installierten Release-E2E mit Native Peer und
Signaling. Die späteren Revisionen schließen die app-vollständige Story-,
Window-, Responsive-, Source-/Versions- und Kontextmatrix. Offen bleibt nur
der menschliche Signoff; die Benennung der fünf historischen Hypoport-Pilot-
IDs ist reine Produktmetadatenpflege und kein technischer Refactoring-Track.

## 14. Abschluss und Release-Freigabe

Es gibt nach Revision 74 keinen offenen technischen Refactoring-Track mehr.
Die folgenden Punkte sind dauerhaft als Regression-Gates zu halten:

1. der 35-App-Desktop-/Mobile-Lauf einschließlich Window-Manager, Snap,
   Source, Versionen und Chat-Komposition,
2. der globale `context_v2`-/Command-/Approval-Pfad ohne modul-lokale
   Rechtsklick-Sonderwege,
3. die 34-App-Story-, Qualitäts-, Command-, Design-, Accessibility-,
   Isolation- und Lifecycle-Matrix,
4. die RxDB/JS/Rust-Prozessgrenze einschließlich gebautem Wire-Daemon,
5. Managed Upgrade, Service, Signaling, Asset-Guard und Native-Peer-Evidence
   aus Revision 59 bei jeder späteren Installer- oder Shell-Änderung.

Vor einem öffentlichen Produktionsrelease bleibt ausschließlich der
menschliche Product-/Design-/Security-/Privacy-Entscheid erforderlich. Die
vorhandenen Dokumente sind strukturell valide, tragen aber korrekt den Status
`pending-signoff`. Reviewer, Datum, Release-Commit, `evidence_revision` und
`source_hashes` dürfen erst durch den verantwortlichen Menschen auf
`signed-off` gesetzt werden. Das ist kein fehlender Code und reduziert den
technischen Abschlussstand nicht.

Die historische Benennung der fünf Hypoport-Pilot-IDs bleibt reine
Produktmetadatenpflege. Sie verändert weder den App-Lifecycle noch das
instanzspezifische Installationsset und blockiert den technischen Abschluss
nicht.
