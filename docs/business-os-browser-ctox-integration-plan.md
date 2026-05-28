# Business OS Browser: CTOX Integration Implementation Plan

Status: Planungsstand 2026-05-28

Ziel: Die Business-OS-App `Browser` wird eine echte windowed Desktop-App wie
`Files` oder `Source Editor` und ein produktiver CTOX Web-Stack-Baustein fuer
Human-in-the-loop Browserarbeit. CTOX soll klar entscheiden koennen, ob eine Web
Stack Aufgabe headless laeuft, eine sichtbare Remote-Browser-Session braucht
oder nach einem Human-Handoff wieder automatisiert fortgesetzt wird.

## Readiness Statement

Der aktuelle Stand ist keine fertige Browser-Integration. Er enthaelt wichtige
Bausteine, aber die Produktform und die Runtime-Grenzen sind noch falsch:

- `Browser` ist derzeit als Full-Workspace-Modul registriert, nicht als
  windowed Desktop-App.
- Die UI wirkt noch zu stark wie eine Debug-/Diagnoseflaeche und nicht wie ein
  Browser.
- Der CTOX Web Stack hat zwar Auth-Assist-, Context-Capture- und
  Extract-Commands, aber noch keine zentrale Runtime-Policy fuer
  `headless`, `remote_browser` und `human_assist`.
- Eine persistente Remote-Browser-Runtime, die Input-Events konsumiert und
  laufend Frames ueber RxDB schreibt, muss als Kernvertrag geschlossen werden.

Dieser Plan behandelt den Browser nicht als Demo. Jede Welle muss fuer sich
testbar sein und entweder eine sichtbare Produktverbesserung, eine Runtime-
Faehigkeit oder eine CTOX-Orchestrierungsgrenze abschliessen.

## Fortschrittsmodell

| Welle | Gewicht | Status | Fortschritt |
| --- | ---: | --- | ---: |
| 0. Baseline, Contracts & Risikoabgrenzung | 5% | Abgeschlossen | 100% |
| 1. Browser als windowed Desktop-App | 12% | Abgeschlossen | 100% |
| 2. Browser-UI als echte Browser-Chrome | 12% | Abgeschlossen | 100% |
| 3. Persistente RxDB Remote-Browser-Runtime | 18% | Geplant | 0% |
| 4. Web-Stack Routing Policy | 12% | Geplant | 0% |
| 5. Human-in-the-loop Auth & Credentials | 14% | Geplant | 0% |
| 6. Browser Context Extract & Resume | 12% | Geplant | 0% |
| 7. CTOX Harness Tools & Operator Flow | 7% | Geplant | 0% |
| 8. Production Hardening, QA & Release Gates | 8% | Geplant | 0% |
| **Gesamt** | **100%** | **In Umsetzung** | **29%** |

Fortschritt je Welle:

- `0%`: Noch nicht begonnen.
- `25%`: Zielvertrag, Dateien und Tests sind festgelegt.
- `50%`: Kernverhalten ist implementiert und lokal nutzbar.
- `75%`: Unit-/Smoke-/E2E-Tests sind vorhanden und gruen.
- `100%`: Akzeptanzkriterien erfuellt, Plan aktualisiert, keine bekannten
  Blocker fuer die naechste Welle.

Wenn Befunde waehrend der Umsetzung den Plan aendern, wird der Abschnitt
`Plan-Aenderungslog` aktualisiert.

## Nicht Verhandelbare Produktregeln

1. `Browser` ist eine windowed Desktop-App, kein Fullscreen-Modul.
2. Die App sieht wie ein Browser aus: Tabs, Adressleiste, Navigation,
   Ladezustand, Webseitenflaeche, Downloads/Blocker/Auth-Hinweise nur dort, wo
   Nutzer sie erwarten.
3. Keine kryptischen Statuslabels in der Nutzerflaeche: keine RxDB-, Command-,
   Pending-, Frame- oder Debug-Begriffe.
4. Diagnose bleibt zugaenglich, aber nur hinter Dev-/Inspector-Schaltern oder
   Tests, nicht im Hauptfluss.
5. Der komplette Browser-Stream und alle Inputs laufen ueber bestehende
   Business-OS/RxDB-Collections. Es werden keine neuen User-Frontend-Kanaele
   eingefuehrt.
6. Playwright/CDP-Sockets, Cookies, Secrets und Frame-Bytes duerfen nicht in
   CTOX-Handoff-Payloads auftauchen.
7. Credentials werden im CTOX Secret Store oder im lokalen Browserprofil
   gehalten, nicht als RxDB-Secret-Werte.
8. Web Stack Headless-Automation bleibt der schnelle Standardpfad.
9. Der Remote Browser ist der explizite Pfad fuer Login, 2FA, Bot-Wall,
   Review, manuelle Navigation und Mensch-Freigabe.
10. Jede Welle braucht reproduzierbare Tests, mindestens lokal, fuer die
    geaenderte Grenze.

## Zielarchitektur

```text
CTOX Web Stack Task
  |
  |-- simple / public / scripted
  |     -> headless Web Stack runtime
  |
  |-- login required / 2FA / blocked / user review
  |     -> Browser Desktop-App
  |        -> browser_sessions
  |        -> browser_tabs
  |        -> browser_frames
  |        -> browser_input_events
  |
  |-- after human action
        -> browser.capture.extract
        -> ctox.browser_context.capture
        -> Web Stack resume / CTOX task continuation
```

Die Business-OS Browser-App ist dabei kein separater Browser-Service fuer den
Nutzer, sondern die sichtbare Bedienoberflaeche fuer eine CTOX-seitig laufende
Browser-Runtime. Die UI liest Frames aus RxDB und schreibt Input-Events in RxDB.
Die Runtime konsumiert diese Events und schreibt neue Frames zurueck.

## Welle 0: Baseline, Contracts & Risikoabgrenzung

Ziel: Der aktuelle Stand wird technisch und produktseitig eindeutig
eingefroren, bevor weitere Aenderungen die Grenzen vermischen.

Aufgaben:

- Aktuellen Browser-, CTOX-, Web-Stack- und Harness-Code inventarisieren:
  - `src/apps/business-os/modules/browser/*`
  - `src/apps/business-os/app.js`
  - `src/apps/business-os/shared/window-manager.js`
  - `src/core/business_os/rxdb_peer.rs`
  - `src/core/service/business_os.rs`
  - `src/tools/web-stack/src/*`
  - `src/core/rxdb/tools/browser_rust_smoke.js`
- Bestehende Commands klassifizieren:
  - Browser runtime: `browser.session.start`, `browser.navigate`,
    `browser.reload`, `browser.back`, `browser.forward`, `browser.reset`,
    `browser.session.stop`
  - HITL/Web Stack: `web_stack.auth_assist.request`,
    `web_stack.auth_assist.complete`, `browser.credential.fill`,
    `browser.capture.extract`, `ctox.browser_context.capture`
- Festlegen, welche Commands in welcher Welle wirklich ausgefuehrt werden.
- Akzeptanzmatrix fuer nicht sichtbare Debugbegriffe erstellen.
- Plan mit Ist-Befunden aktualisieren.

Tests:

- `git diff --check`
- `node --input-type=module --check < src/apps/business-os/modules/browser/index.js`
- `node --input-type=module --check < src/apps/business-os/app.js`
- Bestehende Browser-Handoff-Smokes unveraendert gruenerhalten.

Akzeptanzkriterien:

- Es ist dokumentiert, welche Teile nur UI/Command-Huelle sind und welche
  Runtime-Ausfuehrung besitzen.
- Keine Codeaenderung in spaeteren Wellen beginnt ohne diese Grenzliste.

## Welle 1: Browser als windowed Desktop-App

Ziel: `Browser` wird wie `Files` oder `Source Editor` ueber den Business-OS
Window Manager geoeffnet.

Aufgaben:

- Neues Desktop-App-Bundle anlegen:
  - `src/apps/business-os/desktop-apps/browser/app.js`
  - optional `browser.test.mjs`
- Bestehendes Modul `src/apps/business-os/modules/browser` in ein kleines
  Launcher-/Store-Metadata-Modul umwandeln oder komplett auf Desktop-App-Launch
  delegieren.
- `DESKTOP_APPS` in `src/apps/business-os/app.js` um `browser` erweitern.
- App Store und Registry so anpassen, dass `Browser` als System-App korrekt
  angezeigt wird, aber beim Oeffnen ein Fenster startet.
- `module.json`/Registry-Layout von `full-workspace` auf windowed Contract
  umstellen, z.B. Defaultgroesse `1120 x 760`, Mindestgroesse `720 x 460`.
- Desktop/Icon/Taskbar-Launch testen:
  - aus Desktop
  - aus App Store
  - aus CTOX Web Stack Auth Assist
  - aus Harness Deep Link

Tests:

- Neuer Desktop-App-Mount-Test fuer `desktop-apps/browser/app.js`.
- App Store Test: Browser oeffnet nicht als Fullscreen-Modul.
- Playwright/E2E: Browser-Fenster erscheint in `.shell-window`, laesst sich
  verschieben, minimieren, schliessen und wieder oeffnen.

Akzeptanzkriterien:

- `Browser` nimmt nicht mehr die gesamte Business-OS-Arbeitsflaeche ein.
- Taskbar zeigt ein Browser-Fenster.
- Fensterposition/-groesse werden wie bei anderen Desktop-Apps behandelt.

Umsetzungsstand 2026-05-28:

- `Browser` ist als Desktop-App in `DESKTOP_APPS` registriert.
- `modules/browser` deklariert `launch_kind: desktop-app` und wird bei
  Deep Links/App-Store-Launch in ein Shell-Fenster geroutet.
- Desktop-Apps registrieren zugehoerige Modul-Schemas vor dem Mount, damit
  Browser-Collections auch im Windowed-Modus verfuegbar sind.
- Der lokale packaged Module Catalog aktualisiert bestehende eingebaute
  Modulmetadaten, damit alte RxDB-Katalogeintraege den Browser nicht auf
  `full-workspace` einfrieren.
- Verifiziert mit `SMOKE_MODE=browser-handoff-ui` und
  `SMOKE_MODE=browser-lifecycle-ui`.

## Welle 2: Browser-UI als echte Browser-Chrome

Ziel: Die Oberflaeche wird als Browser wahrgenommen, nicht als Debug-App.

Aufgaben:

- UI-Topologie:
  - kompakte Tab-Leiste oben
  - Zurueck/Vor/Reload
  - Adressleiste mit Sicherheits-/Domain-Hinweis
  - Hauptflaeche nur fuer Webseitenframe
  - dezente Statuszeile fuer Lade-/Fehlerzustand
  - Auth-Assist Banner nur bei Web-Stack-Login-Sessions
- Entfernen oder verbergen aller sichtbaren Debugbegriffe:
  - `pending_command`
  - `Waiting for the next RxDB frame`
  - `RxDB`
  - `Frame`
  - `Seq`
  - `Command`
  - `browser_stream`
- Nutzertexte in Deutsch/Englisch uebersetzen.
- Leere und Fehlerzustaende umformulieren:
  - "Browser wird gestartet"
  - "Webseite wird geladen"
  - "CTOX Browser ist nicht verbunden"
  - "Diese Seite braucht eine Anmeldung"
- Dev-Diagnose hinter verstecktem Inspector:
  - nur per Debug-Toggle, Testflag oder Entwickler-Modus
  - nicht im normalen Browser-Fenster

Tests:

- DOM-Guard-Test gegen verbotene Debugstrings.
- Screenshot-Smoke fuer Fenster in Desktop- und Mobile-Breite.
- Canvas/Frame-Pixelcheck: leerer Zustand und echter Frame unterscheiden sich.
- Keyboard-Fokus: Adressleiste, Canvas, Tab-Leiste.

Akzeptanzkriterien:

- Ein Screenshot der App sieht wie ein Browserfenster aus.
- Normale Nutzer sehen keine internen Collection-/Command-/Frame-Begriffe.
- Auth Assist wirkt wie ein Login-Hinweis, nicht wie ein Runtime-Debugpanel.

Umsetzungsstand 2026-05-28:

- Sichtbare Haupttexte fuer Start-, Lade-, Fehler-, Auth-Assist- und
  Handoff-Zustaende sind produktsprachlich formuliert.
- Alte Debugtexte wie `Waiting for the next RxDB frame` und sichtbare
  `pending_command`-Labels sind aus der normalen Browserflaeche entfernt.
- Technische Diagnose bleibt in versteckten Test-/Dev-Elementen, damit Smokes
  weiter pruefen koennen, ohne Nutzern interne Begriffe zu zeigen.
- E2E-Smokes pruefen sichtbare Browserflaeche gegen verbotene Debugstrings.
- Neuer Smoke `browser-responsive-ui` prueft die Browser-Chrome bei Desktop-
  (1280) und Mobile-Breite (414): Adressleiste, Vor/Zurueck/Reload, Start und
  Webseitenflaeche sind sichtbar, Adressleiste liegt ueber der Seitenflaeche,
  kein horizontaler Overflow, Auth-Assist-Banner bleibt auch schmal sichtbar
  (kein `display:none` mehr), keine Debugbegriffe. Screenshots beider Breiten
  werden als Abnahme-Artefakte gespeichert und sehen wie ein Browserfenster aus.
- CSS-Fix: Mobile-Media-Query versteckt Auth-Assist/Notice nicht mehr; neue
  `.browser-kicker`-Stilregel fuer das Login-Banner.
- Verifiziert mit `SMOKE_MODE=browser-responsive-ui` (Desktop + Mobile gruen).

## Welle 3: Persistente RxDB Remote-Browser-Runtime

Ziel: Der Browser wird technisch zu einer echten remote steuerbaren
CTOX-Chromium-Session.

Aufgaben:

- Native Runtime-Manager einfuehren:
  - Session Registry fuer `browser_sessions`
  - persistente Chromium/Patchright Contexts pro Session
  - Lifecycle: start, navigate, reload, back, forward, stop, reset
- Frame-Produktion:
  - initial Screenshot nach Navigation
  - periodische oder eventbasierte Frames in `browser_frames`
  - Frame-GC ueber `expires_at_ms`
  - keine Frame-Daten in Commands oder Handoff-Payloads
- Input-Konsum:
  - `browser_input_events` pending lesen
  - Maus, Wheel, Keyboard gegen Playwright Page ausfuehren
  - Events als consumed/failed markieren
  - `last_input_seq` und `pending_input_count` korrekt pflegen
- Navigation-State:
  - `can_go_back`, `can_go_forward`, title, url, loading
  - Runtime-Fehler nutzerfreundlich in Session/Tab speichern
- Recovery:
  - Runtime-Neustart erkennt offene Sessions
  - stale Sessions werden sauber als getrennt markiert

Tests:

- Rust Unit Tests fuer Runtime-Command-Routing.
- Browser/Rust E2E:
  - `browser.session.start` erzeugt echten Frame
  - Adressleisten-Navigation aendert URL und Frame
  - Klick/Input/Wheel werden konsumiert
  - Stop beendet Runtime ohne Zombie-Prozess
- Negative Tests:
  - invalid URL
  - closed session input
  - Runtime crash
  - stale command superseded by newer command

Akzeptanzkriterien:

- Ein User kann im Browserfenster eine echte Webseite oeffnen und bedienen.
- Der komplette Stream und alle Inputs laufen ueber RxDB.
- Kein WebSocket/HTTP-Livekanal wird als User-Frontend eingefuehrt.

## Welle 4: Web-Stack Routing Policy

Ziel: CTOX entscheidet bewusst, welcher Browsermodus fuer eine Web-Stack-Aufgabe
verwendet wird.

Aufgaben:

- Policy-Modell definieren:
  - `headless`
  - `remote_browser`
  - `human_assist`
  - `resume_after_handoff`
- Source-Rezepte erweitern:
  - `preferred_browser_mode`
  - `requires_login`
  - `supports_headless`
  - `supports_remote_browser`
  - `credential_required`
  - `auth_assist_available`
  - `capture_script`
- Entscheidungslogik im Web Stack:
  - Public/source-simple: headless
  - Credential fehlt: human_assist request
  - Login/2FA/Bot-Wall: remote_browser + human_assist
  - Review erforderlich: remote_browser
  - Nach Auth abgeschlossen: resume/extract
- Entscheidung als strukturierte, redaktierte Evidence speichern.
- CTOX UI zeigt "Warum Browser?" in menschlicher Sprache.

Tests:

- Unit Tests fuer Routing Matrix.
- Web Stack Smoke:
  - headless bleibt Standard fuer einfache Quellen
  - LinkedIn/Xing/DNB-Hoovers koennen Auth Assist erzeugen
  - Policy erzeugt keine Secrets in Payloads

Akzeptanzkriterien:

- CTOX kann pro Web-Stack-Quelle erklaeren, warum headless oder Browser genutzt
  wird.
- Kein Nutzer muss manuell wissen, welchen technischen Modus er waehlen soll.

## Welle 5: Human-in-the-loop Auth & Credentials

Ziel: Nutzer koennen Logins und Credentials fuer Web-Stack-Quellen sinnvoll im
Browser erledigen.

Aufgaben:

- `web_stack.auth_assist.request` voll ausfuehren:
  - Browserfenster oeffnen/fokussieren
  - Quelle, Domain und erwartete Aktion anzeigen
  - Session auf erlaubte Domains beschraenken
- `browser.credential.fill` Runtime-seitig implementieren:
  - Secret aus CTOX Secret Store lesen
  - in fokussiertes oder recipe-spezifisches Feld eintragen
  - Secret nie in RxDB schreiben
  - Ergebnis nur als redaktierter Status
- User-Fertigstellung:
  - `web_stack.auth_assist.complete` prueft optional `verify_selector`
  - Session wird `authenticated` oder `needs_attention`
  - CTOX Task wird aktualisiert
- UX:
  - Banner: "Anmeldung fuer LinkedIn erforderlich"
  - Buttons: "Zugangsdaten einsetzen", "Ich bin angemeldet",
    "An CTOX uebergeben"
  - Keine technischen Script-/Selector-Begriffe im Normalmodus

Tests:

- Secret-Redaction Tests.
- Auth Assist E2E mit lokaler Fixture-Login-Seite.
- Credential-Fill E2E:
  - Wert wird im Browser eingetragen
  - Wert erscheint nicht in RxDB, Commands, Logs oder Harness-Output
- Domain-Restriction Negative Test.

Akzeptanzkriterien:

- Ein Nutzer kann einen Login im Browserfenster abschliessen.
- CTOX erkennt den abgeschlossenen Handoff und kann weiterarbeiten.
- Secrets bleiben ausserhalb der RxDB-Payloads.

## Welle 6: Browser Context Extract & Resume

Ziel: CTOX kann nach einem Human-Handoff strukturierte Daten aus der
Remote-Browser-Session extrahieren und Web-Stack-Arbeit fortsetzen.

Aufgaben:

- `browser.capture.extract` Runtime-seitig implementieren:
  - nur allowlisted `capture_script`
  - Quelle und erlaubte Domains pruefen
  - Script gegen die echte Browser-Page ausfuehren
  - Ergebnis strukturiert in `business_commands.result.extract`
  - keine Screenshots, Frame-Bytes, Cookies oder Secrets im Ergebnis
- `ctox.browser_context.capture` als Handoff-Referenz schaerfen:
  - nur IDs, URL, title, frame metadata, source id, capture script
  - keine Rohdaten
- Web Stack Resume:
  - Person Research / Source Research liest Extract-Evidence
  - Headless-Pfad kann nach Login fortgesetzt werden, wenn moeglich
  - sonst bleibt Remote-Browser-Session als Quelle fuer Extracts aktiv
- Dedupe/Correlation:
  - `requesting_task_id`
  - `source_id`
  - `session_id`
  - `capture_script`

Tests:

- Extract Unit Tests pro Source-Script.
- E2E: Auth Assist -> User markiert angemeldet -> Extract -> Web Stack nutzt
  Extract Evidence.
- Redaction-Audit bleibt gruen.

Akzeptanzkriterien:

- CTOX kann aus einer menschlich vorbereiteten Browser-Session verwertbare
  Web-Stack-Daten erzeugen.
- Der Nutzer muss keine Daten manuell kopieren.

## Welle 7: CTOX Harness Tools & Operator Flow

Ziel: Harness, Agents und CTOX UI koennen den Browser korrekt anfordern,
statusen und fortsetzen.

Aufgaben:

- Harness Tools finalisieren:
  - `ctox_web_auth_assist_request`
  - `ctox_web_auth_assist_status`
  - `ctox_browser_context_capture`
  - `ctox_browser_context_extract`
- Tool-Beschreibungen auf Produktvertrag pruefen:
  - keine Remote-Control-Versprechen, die nicht stimmen
  - keine Secret-/Frame-Rohdaten
  - klare Statusantworten
- CTOX Task UI:
  - Web-Stack-Handoff als Aufgabe sichtbar
  - Button "Im Browser oeffnen"
  - Button "Status pruefen"
  - Button "Extraktion starten", wenn bereit
- Activity/Audit:
  - Wer hat Browser geoeffnet?
  - Wann wurde Login als fertig markiert?
  - Welche Quelle wurde extrahiert?
  - Welcher CTOX Task wurde fortgesetzt?

Tests:

- Harness command tests.
- CTOX module test fuer Web-Stack-Handoff-Karten.
- E2E: Harness fordert Auth Assist an, Business OS zeigt Fenster, Status wird
  redaktiert gelesen.

Akzeptanzkriterien:

- CTOX und externe Agenten koennen den Browser-Handoff reproduzierbar steuern,
  ohne neue Kanaele oder Debug-Oberflaechen.

## Welle 8: Production Hardening, QA & Release Gates

Ziel: Browser und Web-Stack-Integration sind production-ready.

Aufgaben:

- Performance:
  - Frame-Groessen begrenzen
  - adaptive Frame-Rate
  - Backpressure bei langsamer RxDB-Replikation
  - Session-/Frame-GC
- Sicherheit:
  - Domain Allowlist
  - Secret redaction
  - Clipboard-Policy
  - Download-/Upload-Policy
  - Audit fuer Browser-Handoffs
- UX-Hardening:
  - Small window, large window, snapped window
  - Light/Dark Mode
  - Deutsch/Englisch
  - Tastaturbedienung
  - klare Fehlerzustaende
- Release-Smokes:
  - Browser window UI smoke
  - Browser runtime smoke
  - Auth assist smoke
  - Credential fill redaction smoke
  - Extract/resume smoke
  - App Store launch smoke

Tests:

- `cargo check --locked`
- Browser module/desktop-app tests
- App Store tests
- CTOX module tests
- RxDB critical pool smoke
- Browser/Rust E2E matrix
- Redaction audit
- Playwright screenshots fuer Browser-Fenster

Akzeptanzkriterien:

- Ein Screenshot sieht wie ein Browser aus.
- Ein Nutzer kann eine echte Webseite bedienen.
- CTOX kann bewusst zwischen Headless und Remote Browser entscheiden.
- Human-in-the-loop Login und Extract funktionieren ohne Secret-Leak.
- Browser ist windowed, taskbar-faehig und App-Store-startbar.

## Offene Designentscheidungen

- Ob `modules/browser` komplett entfernt oder als Launcher-/Manifest-Modul fuer
  App Store und Deep Links behalten wird.
- Ob Browserprofile pro User, pro Quelle oder pro Task isoliert werden.
- Wie streng Domain-Allowlisting bei allgemeiner Browsernutzung ausserhalb des
  Web Stack sein soll.
- Ob Downloads in `desktop_files` landen oder zunaechst deaktiviert bleiben.
- Ob Clipboard nur manuell oder auch fuer Runtime-Automation erlaubt wird.

## Plan-Aenderungslog

| Datum | Aenderung | Grund |
| --- | --- | --- |
| 2026-05-28 | Initialer Plan erstellt. | Browser muss von Debug-/Fullscreen-Surface zu windowed CTOX Web-Stack-App werden. |
| 2026-05-28 | Welle 0 abgeschlossen; Welle 1/2 gestartet. Browser bekommt Desktop-App-Wrapper, App-/Launcher-Routing und produktsprachliche Haupt-UI-Texte. | Erste Umsetzung nach Nutzerfreigabe gestartet. |
| 2026-05-28 | Welle 1 abgeschlossen; Welle 2 auf 75%. Handoff- und Lifecycle-Smokes laufen windowed gruen. | Browser ist jetzt Shell-Fenster/App-Store/Deep-Link faehig; verbleibend sind Screenshot-/DOM-Guards und die echte persistente Runtime aus Welle 3. |
| 2026-05-28 | Welle 2 auf 90% erhoeht. DOM-Guard gegen sichtbare Browser-Debugtexte im E2E-Smoke ergaenzt. | Nutzerflaeche darf keine RxDB-/Command-/Pending-Begriffe mehr zeigen. |
| 2026-05-28 | Welle 2 abgeschlossen (100%). Responsive Screenshot-Abnahme via neuem `browser-responsive-ui`-Smoke (Desktop+Mobile), CSS-Fix fuer Auth-Assist-Sichtbarkeit auf schmalen Fenstern. Gesamt 29%. | Letzter offener Punkt von Welle 2 war die responsive Abnahme; Browser sieht jetzt bei beiden Breiten wie ein Browserfenster aus. |
