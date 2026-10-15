# P3-01 / P3-02 — Store-Crate

Stand: 15.10.2026, erster Zwischenstand. Basis `4794af03db8cbf28babee0369af9c416f755019d`, Zweig `v4/p3-01-store-crate`.

## Skelett und Entscheidungen

- Eigenständiges Workspace-Crate `ctox-store`, ohne Rückabhängigkeit auf CTOX oder RxDB.
- Gemeinsame DbId-/Owner-Generations-, Fehler-, Deadline-, Abbruch- und Read/Write-Job-Typen. Async-Ergebnisse via Tokio-Oneshot, keine SQLite-Arbeit im Aufrufer.
- Unbedingte Moduldeklarationen. Zusätzlich zur exklusiven Dateiliste notwendige Stubs: `config.rs` (CacheBackup), `change_log.rs`/`schema.rs` (Transfer), `read_pool.rs` (später P3-03). Nicht implementiert oder als fertig ausgegeben.
- Keine produktive Altweg-Umschaltung in dieser Scheibe; Root-Dependency ist das Wiring, `main.rs` bleibt unverändert.
- Lane-Aufgabe/Target werden wegen des vorgeschriebenen Zeichensatzes ohne Branch-Schrägstrich geführt: `v4-p3-01-store-crate` / `v4-v4-p3-01-store-crate`.

## Noch offen

Registry, Writer, Regressionen und Lane-Qualifizierung folgen. Skelett unter `6e4eea194` um 12:56 Berlin auf origin gepusht. Das Skelett wurde noch nicht kompiliert. Keine Builds auf dem Mac; keine Instanz verändert.
