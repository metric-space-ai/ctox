# Feldbefund 10.09.2026 — Queue-Worker können `ctox scrape` nicht nutzen

Mandant THESEN (`thesen.ctox.dev`), Release `branch-main-20260910T084601Z`.

## Symptom, dreifach gemessen

1. **Adapter-Abgleich** (`outbound.research.adapters.reconcile`,
   `cmd_outbound_adapter_reconcile_44104c34-…`). Der Worker baute 20
   Extraktor-Skripte und meldete: „the contract's mandatory gates
   (`ctox scrape upsert-target` x2, `register-script` x20, fresh `execute` x20)
   remain 0/N satisfied because a sandbox blocker prevents the CTOX CLI from
   starting". Der native Parser verwirft die Antwort anschließend mit
   „adapter reconciliation result has an unsupported status" — die Oberfläche
   zeigt nur diese Folgemeldung, nicht die Ursache.
2. **Scrape-Reparaturen** (`repair scrape target shab-ch`,
   `justizonline-gv-at`, `bundesanzeiger-de`): 12 von 12 gescheitert,
   `shab-ch` mit attempt 18. Die Worker legten ihre Ergebnisnotiz als JSON in
   `targets/<ziel>/ctox.sqlite3` ab; die Update-Sicherung meldete danach
   „file is not a database". Die beiden Dateien sind in
   `repair-note-from-worker.json` umbenannt.
3. **Recherche-Worker**: „Das ctox-CLI ist in dieser Umgebung nicht
   funktionsfähig (SQLite-Lock)", früher „CapEff=0, NoNewPrivs=1,
   /home/ctox/.local/state/ctox/ Permission denied".

Gegenprobe: `ctox business-os commands dispatch` funktioniert aus derselben
Sandbox — Writebacks kommen an.

## Was der Code dazu sagt

- `service::sandboxed_cli_command_allowed` (`src/core/service/service.rs`)
  relayt nur `scrape register-script`, `scrape register-source-module`,
  `scrape execute` und `continuity-update` über die Service-IPC in den Daemon.
- `scrape upsert-target` und `scrape query-records` laufen im Worker-Prozess
  selbst und öffnen den State-Root direkt — in der Sandbox gesperrt. Der
  Adapter-Abgleichsvertrag verlangt aber `upsert-target` zweimal.
- Dass auch die relayten Befehle scheitern („CLI from starting"), deutet auf
  einen Schritt vor dem Relais hin, der den State-Root berührt — etwa die
  Auflösung des IPC-Sockets unterhalb von `~/.local/state/ctox`. Das ließ sich
  aus einer nicht sandboxierten SSH-Sitzung nicht nachstellen.

## Auswirkung

- Jede Scrape-Reparatur und jeder Adapter-Abgleich belegt einen vollen
  Worker-Durchlauf und scheitert sicher. Lead-Recherchen warten dahinter.
- Die Login-Adapter (D&B Hoovers, Leadfeeder, XING, LinkedIn) können auf
  diesem Weg nie registriert werden.

## Bereits abgefangen

- E-Mail-Prüfung läuft seit `6aacf2ea8` nativ im Daemon, nicht mehr über den
  Worker.
- App 1.0.122: Textänderungen am Rechercheablauf lösen keinen
  Adapter-Abgleich mehr aus (der Wortlaut ist nicht mehr Teil des
  Adapter-Fingerabdrucks).
- `outbound.research_policy.publish` läuft nativ statt als Worker-Aufgabe.

## Vorschlag

1. Reproduzieren im Worker-Sandbox-Pfad (bwrap-Profil des Harness) mit
   `ctox scrape execute --target-key experte-de --input-json '{"email":"…"}'`
   und prüfen, an welchem Pfad der Start scheitert.
2. `upsert-target` und `query-records` ins Relais aufnehmen — mit derselben
   Pfadprüfung wie `register-script` (Eingabedateien nur unterhalb des
   Ziel-Workspace). Das verschiebt eine Sicherheitsgrenze und gehört deshalb in
   ein eigenes Review.
3. Bis dahin Reparatur- und Abgleichsaufträge nicht vergeben, wenn der
   Relais-Pfad nicht erreichbar ist, statt sie scheitern und wiederholen zu
   lassen.
