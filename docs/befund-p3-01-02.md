# P3-01 / P3-02 — Store-Crate

Stand: 15.10.2026, Implementierung und erste Qualifizierung abgeschlossen. Basis `4794af03db8cbf28babee0369af9c416f755019d`, Zweig `v4/p3-01-store-crate`.

## Skelett und Entscheidungen

- Eigenständiges Workspace-Crate `ctox-store`, ohne Rückabhängigkeit auf CTOX oder RxDB.
- Gemeinsame DbId-/Owner-Generations-, Fehler-, Deadline-, Abbruch- und Read/Write-Job-Typen. Async-Ergebnisse via Tokio-Oneshot, keine SQLite-Arbeit im Aufrufer.
- Unbedingte Moduldeklarationen. Zusätzlich zur exklusiven Dateiliste notwendige Stubs: `config.rs` (CacheBackup), `change_log.rs`/`schema.rs` (Transfer), `read_pool.rs` (später P3-03). Nicht implementiert oder als fertig ausgegeben.
- Keine produktive Altweg-Umschaltung in dieser Scheibe; Root-Dependency ist das Wiring, `main.rs` bleibt unverändert.
- Lane-Aufgabe/Target werden wegen des vorgeschriebenen Zeichensatzes ohne Branch-Schrägstrich geführt: `v4-p3-01-store-crate` / `v4-v4-p3-01-store-crate`.

## Implementierter Stand

- Registry reserviert Dateipfade prozessweit bereits während Start/Shutdown. Symlinks werden kanonisiert, harte Dateilinks werden abgewiesen. Zusätzlich kooperative OS-Dateisperre über eine bleibende Sidecar-Datei; explizites Unlock vor Close verhindert geerbte Lock-Deskriptoren im Fork/Exec-Fenster.
- Writer öffnet genau eine Connection auf genau einem benannten OS-Thread; Queue 1024, nichtwartendes `try_send`, Überlast `Busy`. SQLite-Busy wird ebenfalls `Busy`, kein versteckter Sleep/Retry.
- `BEGIN IMMEDIATE`; benachbarte Jobs nur bei gleichem explizitem `batch_key`, maximal 64, Trennung nach 10 ms Ausführungszeit. Je Job Savepoint; Fehler und Panik rollen diesen Job zurück. Erfolgreiche Ergebnisse erst nach äußerem Commit. Ein Fehler beim Commit/Head-Hook verwirft die ganze Transaktion.
- `WriterOptions.head_seq`: `Arc<Fn(&Connection) -> Result<u64>>`, Auswertung innerhalb der Schreibtransaktion, Veröffentlichung nach Commit via Watch mit DbId und Owner-Generation. Coalescing absichtlich: Leser holen per Cursor bis zum jüngsten Head auf, nicht einmal je Ereignis. Kein Polling. `CommitReceiver.latest()` liefert einen kopierten Wert, `changed().await` den jüngsten Head oder `Closed` beim Shutdown. Keine Watch-Borrow-Guards für Leser; ein wartender Leser kann den Writer nicht über eine gehaltene Referenz sperren.
- Startup/Ready/Closing/Stopped, abwartbarer Shutdown ohne `block_on`, Annahmeschluss atomar mit Queue-Admission. Bereits angenommene Jobs werden abgearbeitet. Letzter Handle-Drop schließt die Annahme ebenfalls.

## Lane-Belege

Erster Lauf `v4-p3-01-store-crate-20261015T105958Z`: 9/9 Tests grün, Exit 0. Receipt: `/Volumes/tmp/dev-artifacts/build-lane/v4-p3-01-store-crate/v4-p3-01-store-crate-20261015T105958Z.receipt.json` (Kopf 9b668e01d + Dirty-Hash 304d273e09dcda31). Zweiter Lauf `v4-p3-01-store-crate-20261015T110259Z`: 14/14 Tests, Clippy aller Crate-Targets mit `-D warnings`, Formatprüfung grün, Exit 0 (Dirty-Hash `0bd64cfd0ea42a31`).

## Integrationsvertrag / Grenzen

- P3-04 liefert die kanonische Konfiguration über `WriterOptions.configure`; bis zur Integration stehen die sicheren Bootstrap-Pragmas im Writer (WAL/NORMAL/FK/Timeout 0, Journalgrenze 64 MiB, Autocheckpoint 1000). Danach nur noch kanonische Konfiguration, keine dauerhafte zweite Quelle.
- P3-05 installiert den Head-Reader über `WriterOptions.head_seq`; keine vorweggenommene `_seq`-Tabellenform. Nur Sync-Store veröffentlicht eine globale seq, andere Dateien nicht.
- P3-03 kann `ReadJob`/`JobOptions` und Owner-Generation benutzen. Eine DbGeneration bezeichnet die prozesslokale Owner-Inkarnation, nicht die persistente Collection-Generation oder Epoche.
- Lange native Closures lassen sich nicht sicher präemptieren; Deadline/Abbruch werden vor Operation und unmittelbar vor Commit geprüft. Kein Nachweis des p99-20-ms-Budgets mit beliebigen Closures. Migrationsberechnung/I/O müssen außerhalb bleiben.
- Dateisperre kooperativ für den neuen Store. Fremde Legacy-Öffner sind noch nicht umgeschaltet; IPC/P3-08 und Adapter/M2 beseitigen diese. Diese Scheibe behauptet nicht, dass der installierte Daemon bereits einziger Schreiber ist.
- Keine Tests, Builds oder Instanzänderungen auf dem Mac; lokales Rustfmt und `git diff --check` sind leichtgewichtige Quellprüfung. Keine Deploys.

## Abschluss und reproduzierbare Prüfung

Skelett unter `6e4eea194` um 12:56 Berlin auf origin gepusht, also nach etwa drei Minuten. Erster Bericht ebenfalls innerhalb des 25-Minuten-Fensters. Draft-PR: https://github.com/metric-space-ai/ctox/pull/779, Ziel `v4/integration`.

Sauberer Commit `c0c41fc362f3dd3a3fd7e07ebc8f207db54e5172` wurde in Lauf `v4-p3-01-store-crate-20261015T110637Z` mit `--locked` geprüft: 14 Tests, Clippy `-D warnings`, Fmt grün. Die anschließende Kapselung des Commit-Empfängers einschließlich Shutdown-Regression ist in `v4-p3-01-store-crate-20261015T110907Z` grün: **15/15 Tests**, Clippy und Fmt, Exit 0 (Dirty-Hash `0bc151c0a8e6754b`).

Finaler sauberer Head wird mit genau denselben Befehlen qualifiziert; Abschluss-Receipt und Log werden dauerhaft unter `/Users/michaelwelsch/.local/state/ctox-sync-v4/evidence/p3-01-02/final-head.{receipt.json,log}` abgelegt. Der Abschluss-SHA steht im PR und im kanonischen Bericht `~/.local/state/ctox-sync-v4/docs/befund-p3-01-02.md`.

```sh
ctox-prep.sh && cargo test --locked -p ctox-store -- --test-threads=2 && cargo clippy --locked -p ctox-store --all-targets -- -D warnings && cargo fmt --check -p ctox-store
```

Ausführung ausschließlich via gpu-build-run.sh, Owner `01a0879f-d2e0-72c3-9353-4ef802b2998b`, Host auto/gpu3, zwei Jobs, Task/Target wie oben. Kein Vollbuild des CTOX-Binaries und keine installierte Produktabnahme. Bestehender `sync-scope-guard`: keine seiner Sync-Core-Pfadregeln betroffen; keine fremde Stage-Zuordnung erfunden.

Alle Läufe beendet; keine eigenen Dienste, Browser, Worker oder Ressourcen-Leases bleiben zurück. Workspace bleibt für die Integration erhalten; keine fremde Bereinigung.

## Zusätzliche notwendige Dateien

- Vier beauftragte Modul-Stubs, oben benannt, sowie dieser Bericht.
- Root `Cargo.lock`: exakt der durch die Lane erzeugte Workspace-Eintrag für `ctox-store` plus Dependency am Root-Paket; keine Paketversion aktualisiert. Notwendig für reproduzierbare `--locked`-Prüfung. Andere fremde Dateien unberührt.

## Nächste Integration

P3-04: kanonische Konfiguration anbinden; P3-05/P3-06: Head-Hook und Online-DDL auf derselben Writer-Queue; P3-03: Reader-Pool. IPC und Legacy-Adapter folgen getrennt. Kein Merge und kein Deploy durch diesen Strang.
