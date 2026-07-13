# CTOX Sync Engine 9.5/10 Production Readiness Plan

Verbleibender Arbeitsplan nach Abschluss der technischen Baseline.

Status: active readiness plan

Stand: 2026-07-13

Qualifizierte Baseline: `ec5bd97382696bffe7fe53e8d9ebb9d028c367b6`

## Ziel und unveränderliche Grenzen

9.5/10 ist erst erreicht, wenn die bereits qualifizierte technische Baseline
auch unter realen Netzwerkbedingungen, über lange Laufzeiten und in zwei
Produktpiloten nachweisbar stabil ist.

Unverändert gelten:

- Browser IndexedDB ist die lokale Arbeitskopie, native SQLite die autoritative
  Instanz.
- Browser-Geschäftsdaten laufen ausschließlich über RxDB/WebRTC; es gibt keinen
  HTTP-Datenfallback.
- Bestätigte, journalisierte Writes haben RPO 0.
- Command-, Projection- und Saga-Effekte müssen idempotent bleiben.
- Runtime Apps dürfen ohne Rust-Änderung, Backend-Recompile oder manuellen
  Daemon-Neustart installiert und aktualisiert werden.
- Kein Gate darf Wiederholungsversuche als Erfolg akzeptieren.

Die abgeschlossenen Implementierungs- und Kurzzeit-Qualifikationsarbeiten sind
nicht mehr Teil dieses Plans. Ihre commit-gebundene Evidenz liegt unter
`/Volumes/models/ctox-sync95-evidence/ec5bd9738/`.

### Unveränderlicher Readiness-Vertrag

Dieser Block ist keine erneute Aufgabenliste. Er hält die vom Repository-Guard
geprüften Zielwerte und die bereits akzeptierte Baseline fest:

- confirmed journaled writes have RPO 0;
- LAN replication p95 is at most 2 seconds;
- WAN replication p95 is at most 5 seconds;
- reconnect after network, signaling or TURN failure p95 is at most 60 seconds;
- at least 99.9 percent of writes converge innerhalb ihres SLO-Fensters;
- native off-host backup RPO is at most 15 minutes and RTO is at most 60 minutes;
- one clean full matrix mit at least 40 unique modes ist abgenommen;
- 3 cycles x 33 required modes sind ohne Retry abgenommen;
- 9 cycles x 33 required modes bleiben das offene Nightly-Gate;
- ein 72 hour persistent canary bleibt verpflichtend;
- der TURN-only path muss real, nicht simuliert, nachgewiesen werden;
- external review is required;
- Two 30-day pilots are required;
- Rollout state must be typed and persisted;
- vor Abschluss müssen runbooks exist and have been exercised.

Die produktive Statusschnittstelle bleibt `ctox business-os rxdb status --json`
mit dem Objekt `productionReadiness`. Die bereits implementierte Runtime-
Baseline umfasst signed app packages und declarative schema migrations. Die
Backup-Policy verlangt weiterhin ein encrypted off-host snapshot every 15 minutes.

Bereits akzeptierte, aber weiterhin auditierte Artefaktklassen:

- `runtime/build/ctox-sync-production-readiness-95-browser-recovery-matrix.json`;
- `runtime/build/ctox-sync-production-readiness-95-app-runtime-package-gate.json`.

Die Guard- und Operatorwerkzeuge bleiben Teil des Abschlussaudits:

```bash
node src/core/rxdb/tools/print_sync_production_readiness_95_templates.js
node src/core/rxdb/tools/build_sync_production_readiness_95_artifact.js
node src/core/rxdb/tools/run_sync_production_readiness_95_app_runtime_package_gate.js
node src/core/rxdb/tools/run_sync_production_readiness_95_browser_recovery_matrix.js
node src/core/rxdb/tools/print_sync_production_readiness_95_report.js
```

Templates bleiben mit `template_artifact` ungültig. Der Operatorreport wird als
`runtime/build/ctox-sync-production-readiness-95-operator-report.json`
geschrieben.

## 1. Nightly-Soak abschließen

Aktueller Status: `9 x 33` läuft auf der qualifizierten Baseline.

Abnahme:

- neun vollständige Zyklen mit jeweils 33 Pflichtmodi;
- `SMOKE_MATRIX_ATTEMPTS=1` und Retry-Zahl null;
- alle Queue-, Speicher-, Collector-, Outbox- und Checkpoint-Budgets eingehalten;
- Commit, `dirty=false`, Browserbundle-Hash und Testbinary-Hash stimmen in jedem
  Zyklus überein;
- Ergebnis wird als
  `/Volumes/models/ctox-sync95-evidence/ec5bd9738/nightly-soak-9x33.json`
  archiviert.

Ein Fehler stoppt das Gate. Er wird behoben und die vollständigen neun Zyklen
beginnen auf einem neuen, sauberen Kandidaten erneut.

## 2. Langzeittest-Infrastruktur bereitstellen

### 2.1 Benötigte Systeme

Für die noch offenen Nachweise werden mindestens folgende Systeme benötigt:

1. **Dauerhafter Test-Runner**
   - Linux oder macOS, mindestens 8 CPU-Kerne, 16 GiB RAM und 100 GiB freier SSD-
     Speicher;
   - Chromium/Playwright-fähig;
   - darf mindestens 72 Stunden, für Pilot-Telemetrie idealerweise 30 Tage,
     ohne Sleep, Reboot, Benutzer-Logout oder automatische Updates laufen;
   - synchronisierte Systemzeit und stabile Log-/Artefaktspeicherung;
   - eigener, nicht produktiver CTOX-Testbenutzer.
2. **Externer WebRTC-Peer**
   - anderes physisches Netz bzw. anderer ISP/NAT als der Test-Runner;
   - ebenfalls dauerhaft erreichbar;
   - Browser und CTOX-Testbinary müssen gegen denselben Kandidaten laufen;
   - direkte Peer-Verbindung muss für TURN-only-Tests gezielt unterbindbar sein.
3. **TURN-Endpunkt**
   - produktionsnaher TURN/TLS-Endpunkt mit kontrollierbarer Credential-Rotation;
   - getrennte kurzlebige Test-Credentials;
   - Zugriff auf Servermetriken oder mindestens verbindliche Session-/Relay-
     Nachweise;
   - kein gemeinsam genutztes persönliches Secret.

Ein einzelner Rechner kann den 72-Stunden-Canary ausführen, aber keine echte
WAN-/NAT-/TURN-Qualifikation belegen. Für diesen Teil sind zwei getrennte Netze
und der TURN-Endpunkt zwingend.

### 2.2 Erforderlicher Zugang

Für den Runner und den externen Peer werden benötigt:

- SSH-Zugang per dediziertem Schlüssel oder ein vergleichbar auditierbarer
  Remote-Zugang;
- Repository-Lesezugriff auf den qualifizierten Commit;
- Schreibzugriff ausschließlich auf ein isoliertes Test-Root und das festgelegte
  Evidenzverzeichnis;
- Berechtigung, langlebige Prozesse über `systemd`, `launchd` oder `tmux` zu
  betreiben;
- begrenztes `sudo`/Administratorrecht für Netzwerkprofile, Firewallregeln,
  Prozessdiagnostik und kontrollierte Neustarts;
- ausgehender Zugriff auf Git, Signaling und TURN sowie WebRTC UDP/TCP/TLS;
- Secrets nur über den CTOX Secret Store oder einen freigegebenen Secret Manager;
- ein Alarmziel und ein verantwortlicher Ansprechpartner für P0/P1-Ereignisse.

Nicht benötigt werden Zugriff auf Produktivdaten, persönliche Benutzerkonten
oder unbeschränkte Root-Secrets.

### 2.3 Zugangshandoff

Vor Start der externen Tests müssen folgende Angaben vorliegen:

```text
Runner hostname/VPN:
Runner OS und Architektur:
SSH-Benutzer und Public-Key-Verfahren:
Erlaubtes Test-Root:
Erlaubte Laufzeit ohne Unterbrechung:
Sudo-/Netzwerkberechtigungen:

Externer Peer hostname/VPN:
Peer-Netz/ISP vom Runner getrennt: ja/nein
Direkte WebRTC-Verbindung blockierbar: ja/nein

TURN URL/TLS-Port:
Credential-Quelle und Rotationsverfahren:
TURN-Metrik-/Logzugriff:

Evidenzziel:
Alarmziel:
Technischer Ansprechpartner:
Wartungs- oder Rebootfenster:
```

Nach dem Handoff erfolgt zuerst ein read-only Zugangstest, danach ein maximal
30-minütiger Preflight. Erst wenn Isolation, Zeit, Disk, Browser, Netzwerk und
Artefaktpfad geprüft sind, startet ein Langzeitgate.

## 3. Reale WAN-/TURN-Matrix

Die Matrix muss folgende Profile auf zwei getrennten Netzen messen:

- LAN-Referenz: 20 ms RTT, 0,1 Prozent Loss, 50 Mbit/s;
- WAN: 120 ms RTT, 1 Prozent Loss, 10 Mbit/s;
- adverse WAN: 300 ms RTT, 3 Prozent Loss, 2 Mbit/s;
- TURN-only, während direkte ICE-Kandidaten blockiert sind;
- TURN-Credential-Ablauf und Rotation während aktiver Replikation;
- Signaling-Partition während lokaler Writes und einer Command-/Saga-Ausführung;
- fünf Benutzer, zehn Tabs und 50.000 Dokumente;
- acht Stunden Offline-Writes mit anschließendem Catch-up über gültige
  Checkpoints, ohne Full Resync.

Abnahme:

- kein Verlust bestätigter Writes und keine doppelten Effekte;
- LAN-Replikation p95 höchstens 2 Sekunden;
- WAN-Replikation p95 höchstens 5 Sekunden;
- Reconnect p95 höchstens 60 Sekunden;
- mindestens 99,9 Prozent der Writes konvergieren innerhalb ihres SLO-Fensters;
- Terminalfehler sind typed und über Status-/Transportdaten erklärbar;
- reale ICE-/Relay- und Credential-Rotationsnachweise liegen vor;
- keine simulierte Messung wird als reale WAN-/TURN-Evidenz akzeptiert.

Ausführung und Zielartefakt:

```bash
node src/core/rxdb/tools/run_sync_production_readiness_95_wan_turn_matrix.js \
  --external-measurements runtime/build/ctox-sync-production-readiness-95-wan-turn-external.json \
  --smoke-binary <qualifiziertes-ctox-binary>
```

Das finale Artefakt ist
`runtime/build/ctox-sync-production-readiness-95-wan-turn-matrix.json`.

## 4. 72-Stunden-Canary

Der Canary läuft durchgehend auf dem exakt qualifizierten Commit. Währenddessen
werden planmäßig Netzwerk-, Leader-, Quota-, Daemon-Restart-, Checkpoint-,
Konflikt- und Command-/Saga-Fehler injiziert.

Abnahme:

- volle 72 Stunden ohne Test-Neustart oder verdeckten Retry;
- keine verlorenen bestätigten Writes und keine doppelten Effekte;
- keine ungeklärte `manual_intervention`;
- SLOs und Ressourcenbudgets bleiben innerhalb der definierten Grenzen;
- alle injizierten Fehler werden erkannt, erklärt und automatisch bzw. gemäß
  Runbook wiederhergestellt;
- P0/P1 beendet das Gate und erzeugt einen neuen Kandidaten.

## 5. Runbooks praktisch üben

Alle 13 Runbooks aus
`docs/ctox-sync-production-readiness-runbooks.md` müssen gegen den Kandidaten
ausgeführt werden:

- Signaling-/TURN-Ausfall;
- WebRTC-Backpressure-Stall;
- Journalwachstum und Replay;
- Quota-Erschöpfung;
- blockierte IndexedDB-Primary;
- Browser-Origin-Verlust;
- native SQLite-Wiederherstellung;
- blockierte Schemamigration;
- Saga-Kompensationsfehler;
- Konfliktflut;
- App-Package-Revocation;
- Key-Revocation;
- MCP-Zugriffsincident.

Jede Übung braucht Datum, Operator, Dauer, Evidenz-URI/-Hash und geschlossene
Follow-ups. Übungen älter als 90 Tage zählen nicht.

Das Ergebnis wird als
`runtime/build/ctox-sync-production-readiness-95-runbook-exercises.json`
archiviert.

## 6. Security, Privacy und Schlüsselbetrieb abnehmen

Offen sind:

- externe Security-/Privacy-Prüfung auf dem exakten Release-Commit;
- Sign-off aller Controls in
  `docs/business-os-security-privacy-signoff.json`;
- Prüfung von Workspace-Isolation, Grants, Package-Tampering, Replay,
  Peer-Impersonation, Recovery-Krypto, Saga-Bypass, MCP-Bypass und Audit-
  Manipulation;
- organisatorischer Nachweis, dass der Schlüssel für verschlüsselte Off-host-
  Backups getrennt vom Backup, zugriffskontrolliert und wiederherstellbar
  verwahrt wird;
- ein erfolgreicher Restore mit dem aus dem Escrow-Verfahren geholten Schlüssel.

Ein lokal vorhandener Schlüssel oder lediglich extern kopiertes Backup erfüllt
das Escrow-Gate nicht.

## 7. Zwei 30-Tage-Piloten

### Record Workbench

Der Pilot muss Runtime-Installation, neue Collection, CRUD, reaktive Queries,
Offline-Writes, zwei Geräte je Benutzer, Konfliktauflösung, Recovery-
Export/Import und ein Schema-Upgrade real verwenden.

### Multi-Collection Workflow

Der Pilot muss eine deklarative Action mit mindestens drei Saga-Schritten,
Grant-Prüfungen, Crash-Replay, Idempotenz, Kompensation, Audit und einen gezielt
ausgelösten Kompensationsfehler inklusive Runbook-Behandlung verwenden.

Gemeinsame Abnahme:

- 30 zusammenhängende Kalendertage je Pilot;
- keine Datenverluste, doppelten Effekte oder unautorisierten Zugriffe;
- keine unerklärten Terminalzustände;
- keine `manual_intervention` älter als 24 Stunden;
- mindestens 99,9 Prozent SLO-Konvergenz;
- keine offenen P0/P1-Incidents;
- Restore und Recovery werden im Pilotzeitraum nachweislich ausgeführt.

Die Piloten können nach grünem WAN-/TURN-Preflight und stabilem Canary parallel
laufen. Ihre 30-Tage-Frist kann nicht durch synthetische Kurztests ersetzt
werden.

## 8. Abschlussaudit und Rollout

Nach Abschluss aller Gates wird ein commit-gebundener Evidenzaudit erzeugt. Er
muss insbesondere prüfen:

- identischer Commit, `dirty=false`, Bundle- und Binary-Hashes;
- genau ein akzeptierter Versuch und null Retries;
- frische Restore-, Runbook-, Canary-, WAN-/TURN- und Pilotnachweise;
- vollständiger Security-/Privacy-Sign-off;
- keine Templates oder manuell erfundenen Erfolgsevidenzen;
- keine offenen P0/P1- oder Release-Blocker.

Erst danach darf die Engine als 9.5/10 eingestuft werden. Der Rollout erfolgt
über persistierte Kohorten `internal`, `pilot`, `10_percent`, `25_percent`,
`50_percent` und `100_percent`; jede Promotion benötigt sieben grüne Tage.

## Empfohlene Reihenfolge ab jetzt

1. Laufenden `9 x 33` Nightly-Soak abschließen.
2. Runner, externen Peer und TURN-Zugang anhand des Handoff-Blocks bereitstellen.
3. 30-Minuten-Preflight und reale WAN-/TURN-Matrix ausführen.
4. 72-Stunden-Canary starten.
5. Runbook-Übungen und externes Security-/Privacy-Sign-off durchführen.
6. Schlüssel-Escrow durch einen echten Restore belegen.
7. Beide 30-Tage-Piloten parallel starten und vollständig beobachten.
8. Abschlussaudit ausführen und anschließend kohortenweise ausrollen.
