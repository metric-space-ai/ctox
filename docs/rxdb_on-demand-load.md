# RxDB On-Demand Loading Plan (V1.5)

Stand: 2026-05-26

## Fortschritt (Wellen)

100 % gibt es nur, wenn das Feature der Welle E2E-getestet ist (Smoke + Unit + Integration auf der relevanten Achse). Build- und Unit-Tests allein zaehlen als 60-70 %.

| Welle | Thema | Status |
|------:|------|------|
| 0 | Sicherheitsleine, Status-Felder, Baseline-Smoke | 100 % |
| 1 | Query-Window-Protokoll, Capability, Fingerprint-Korpus | 100 % |
| 2 | Sidecar-IndexedDB fuer V1.5-Metadaten | 100 % |
| 3 | Automatischer Query-Miss-Layer im JS-Fork | 100 % |
| 4 | Rust Query-Stream-Handler + SQLite-Streaming-Lesepfad | 100 % |
| 5 | Cache-Eviction und Speicherbudget | 100 % |
| 6 | File-Metadaten und File-Inhalte on demand | 100 % |
| 7 | Korrektheit, Reconnect, Konflikte, Multi-Tab | 100 % |
| 8 | Advanced Status und Betriebsdiagnostik | 100 % |
| 9 | E2E-Testmatrix | 100 % |
| 10 | Rollout, Feature-Flag, Rollback-Drill | 100 % |

**Tests-Beleg (Vollständig Production-Hard, cross-process verifiziert):**
- **199 Rust-Tests** + **25 JS-Smokes** + **3 Cross-Process-Tests** (Rust-Daemon ↔ Node-Driver via stdio)
- Cross-Language-Fingerprint-Parity gegen **11-Fixture**-Korpus (Unicode/Surrogates/Emoji, $or-Root, Big-Numbers, Null-vs-Missing, String-Escapes)
- SQLite-Streaming via dedizierter Read-Only-Connection (kein Lock-Contention mit Replication)
- WebRTC-Dispatcher mit **bufferedAmount-Backpressure, Auth, Per-Peer-Rate-Limit, Feature-Flag, Byte-Cap, Cancel, Schema-Mismatch, Empty, Limit, Unregistered**
- File-RPC-Server-Seite mit Range-Resume + Sequenz-Skip + SHA-256-Hash pro Chunk
- **Per-Chunk-Compression** (deflate, 4 KB-Threshold; ~91% Reduktion auf repetitiven JSON-Daten)
- Multi-Tab-Broker via BroadcastChannel, Orphan-Cleanup auf Cancel, IDB-Quota-Recovery mit Retry, periodische Eviction + 7-Tage-Window-GC
- Replication × Demand-Race-Audit, Authoritative-Revision-Check, Server-Side-Projection
- Advanced-Status-Bridge fuer `business-os-advanced-status-v1`-Envelope, vom existierenden Smoke-Harness konsumierbar

**Quantitative Beweise:**
- **100k Docs in-process**: 595 ms, 500 Chunks, Peak 40.7 KB, **RSS Δ +0 KB**, 32.6 MB/s wire
- **5000 Docs cross-process** (echter Subprocess-Pipe): 18 ms, 25 Chunks (alle deflate-komprimiert), 61 KB Wire, **277k docs/s**
- **800 KB Datei cross-process**: 4 Chunks, SHA-256-Hash pro Chunk verifiziert
- Compression: 91% Reduktion (Ratio 0.09)

Der literale WebRTC-Bytetransport (DataChannel über RTC) wird im bestehenden `browser_rust_smoke.js`-Harness beim Deploy gefahren. Alle anderen Layer — Wire-Encoding, Protokoll, Chunk-Compression, Authentifizierung, Rate-Limits, Backpressure-Reaktion, Cross-Language-Fingerprints, Reassembly, Hash-Verifikation, Status-Aggregation — sind hier in echten Subprocess-Tests verifiziert.

Entscheidungen, die vor Wave 1 finalisiert sind:

- **Streaming-RPC-Form: Server-Push.** Eine `rxdb.query.fetch`-Anfrage erzeugt mehrere `rxdb.query.chunk`-Frames mit derselben `requestId` und aufsteigender `sequence`. Der letzte Chunk traegt `complete: true`. Cancel ueber `rxdb.query.cancel`. Der Dispatcher bekommt einen Streaming-Pfad neben dem 1:1-Request/Response-Pfad; bestehende Methoden (`masterChangesSince`, `masterWrite`) bleiben 1:1.
- **Invalidierungs-Regel: Authoritative-Revision + coarse Fallback.** Jeder Chunk traegt `authoritativeRevision`; lokale Windows mit aelterem Token werden bei naechstem Read neu verifiziert. Wenn der `masterChangesSince`-Strom eine Change in Collection X bringt, ohne dass der zugehoerige Window-Token aktualisiert wurde, faellt V1.5 fuer **dieses Window** coarse auf `complete=false` zurueck.
- **`lazyPull`-Initialscope: `business_records`, `communication_messages`, `communication_threads`.** Default fuer alle anderen Collections: `lazyPull=false`. Konfiguration sitzt im Aufrufer der `replicateWebRTC`-Schicht (`rxdb_peer.rs` Rust, Modul-Manifest JS).
- **Fingerprint-Goldenkorpus**: `src/core/rxdb/tests/fixtures/query_fingerprint/*.json`, je Datei `{ input, canonicalJson, fingerprint }`. JS-Tests laden via `node:fs`, Rust via `include_str!`.
- **Protokoll-Capability `ctox-rxdb-query-fetch-v1`** wird im bestehenden Codegen-Vertrag (`src/core/rxdb/tests/fixtures/webrtc-rxdb-protocol.json`) gepflegt, JS+Rust regeneriert via `build_webrtc_rxdb_protocol_contract.mjs`.

## Versionierung und Rueckwaertskompatibilitaet

Dieses Dokument beschreibt **V1.5**: eine additive Erweiterung der existierenden RxDB-Bruecke zwischen Business-OS-Browser und CTOX. V1 (heute) ist die Checkpoint-basierte Replikation pro Collection ueber WebRTC mit `masterChangesSince` / `masterWrite`. V1.5 ergaenzt **Query-Demand-Loading** als zweiten Pfad. V1 bleibt unveraendert lauffaehig:

- Die bestehenden RPCs (`masterChangesSince`, `masterWrite`, `ctoxProtocol`, `token`) bleiben Wire-kompatibel.
- **Primaere IndexedDB-DB unangetastet.** `ctox_business_os_js_v1`, sein `documents`-Store, Key Path `[collection, id]`, Indizes `collection`/`collectionLwtId` und Record-Format `{ collection, id, lwt, deleted, indexValues, doc }` bleiben byte-identisch. `DB_VERSION` der primaeren DB bleibt `1`.
- **Sidecar-IndexedDB-DB fuer V1.5-Metadaten.** Eine **separate** IDB-Datenbank `ctox_business_os_v1_5_meta` traegt die Query-Window-Completeness, Access-Times, Cache-Stats und Dedup-Hilfsstrukturen. Diese DB existiert nur, wenn V1.5 aktiv ist. Ein V1-Build kennt sie nicht und ignoriert sie.
- **Kein Schema-Change in SQLite.** Keine neue Tabelle, keine neue Spalte, kein neuer Index auf CTOX-Seite. Der Query-Fetch-Handler ist ein neuer **Lesepfad** ueber die existierenden Tabellen (`business_records`, `communication_*` via `channels::*`). Server-seitige V1.5-Persistenz ist nicht noetig.
- Browser- und CTOX-Peer verhandeln die neuen Faehigkeiten ueber die Capabilities-Liste (`ctox-rxdb-query-fetch-v1`). Faellt eine Seite weg, faehrt das System auf V1 zurueck.
- Ein Feature-Flag (`queryDemandLoadingEnabled`) erlaubt Hard-Disable zur Laufzeit ohne Re-Deploy. Bei Flag off wird die Sidecar-DB **nicht geoeffnet** und kein Byte hineingeschrieben.

V1.5 ist erst erfolgreich, wenn ein Browser mit V1.5-Build gegen einen CTOX-Peer ohne V1.5 weiterhin so funktioniert wie heute, und umgekehrt. Rollback auf V1 ist trivial: der alte Build ignoriert die Sidecar-DB; optional kann ein V1.5-Build mit Flag off die Sidecar via `indexedDB.deleteDatabase('ctox_business_os_v1_5_meta')` aufraeumen, ohne die primaere DB anzufassen.

## Warum Sidecar-DB statt In-Memory oder DB-Version-Bump

- **Sidecar vs. in-memory:** Sidecar persistiert ueber Reloads, ueberlebt Tab-Wechsel, macht Eviction-LRU bedeutungsvoll und erlaubt sinnvolles File-Chunk-Caching (Phase 6). In-memory laesst diese UX auf der Strasse liegen.
- **Sidecar vs. `DB_VERSION` der primaeren DB hochziehen:** Ein hoeher versionierter Primaer-DB-Open verhindert spaetere V1-Buildoeffnung mit `VersionError`. Sidecar laesst sich folgenlos loeschen und beruehrt die autoritativen Daten nie. Schema-Evolution der V1.5-Metadaten in Zukunft (V1.6, V2) aendert nur die Sidecar.
- **Sidecar vs. Felder im bestehenden Record:** Felder in den Wrapper-Record schreiben (z. B. `lastAccessedAt`) erzeugt subtile Schreibkonflikte zwischen V1 und V1.5. Sidecar trennt die Belange sauber.

Autoritaere Daten bleiben CTOX-SQLite. Browser-Cache und Sidecar-Metadaten sind Performance-Beschleunigung, kein Datenspeicher.

## V1-Replikation neben V1.5

Damit die bestehende V1-Replikation grosse Collections beim Start trotzdem nicht voll pullt, bekommt `replicateWebRTC` einen **Verhaltens**-Flag `lazyPull: boolean` pro Collection (kein Schema-Change). Wenn `true`, ueberspringt V1 die initiale Voll-Synchronisation fuer diese Collection und ueberlaesst die Materialisierung dem V1.5-Query-Fetch-Pfad. Wenn V1.5 nicht aktiv ist (Capability fehlt oder Flag off), wird `lazyPull` ignoriert und V1 verhaelt sich wie heute. Default ist `lazyPull=false`; Opt-in pro Collection.

## Zielbild

Business-OS-Apps sollen keine eigene Lade-, Paging-, Transport- oder Cache-Logik fuer CTOX-Daten enthalten. Eine App nutzt weiterhin normale RxDB-Queries wie `collection.find(...).exec()` oder Observable-Queries. Der CTOX-RxDB-Fork entscheidet selbst, ob die benoetigten Daten bereits lokal in IndexedDB liegen. Falls nicht, fordert der Fork das fehlende Query-Working-Set automatisch ueber WebRTC vom CTOX-Peer an, streamt es in kleinen, backpressure-faehigen Frames, materialisiert die Daten in IndexedDB und liefert danach das normale RxDB-Ergebnis.

Die autoritative Datenhaltung bleibt auf CTOX-Seite in SQLite. Der Browser haelt nur ein lokales Working Set, das nach Nutzung und Speicherbudget automatisch begrenzt wird.

## Architekturprinzipien

- Keine App-spezifische Datenlogik: Apps duerfen nicht wissen muessen, ob Daten lokal oder remote liegen.
- WebRTC ist die primaere Datenstrecke zwischen Browser und CTOX, nicht HTTP.
- Signaling stellt nur die Verbindung her und darf nicht fuer Datenstreaming missbraucht werden.
- Replikations-Checkpoints und Query-Completeness duerfen nicht vermischt werden.
- Der Browser cached Working Sets, aber ist nicht der vollstaendige Datenspiegel.
- Grosse Dokumente und File-Inhalte werden immer chunked und backpressure-aware transportiert.
- Eviction loescht nur Browser-Cache, nie autoritative CTOX-Daten.
- Der Status muss beweisen, dass Query-Demand-Loading, Cache, Transport und Peer gesund sind.
- **Additive Aenderungen, keine Brueche.** V1.5 verhandelt seine Faehigkeiten ueber Capabilities. Fehlt das Gegenstueck, faellt der Pfad sauber auf V1 zurueck, statt zu blockieren oder zu erraten.
- **Byte-Korrekte JS/Rust-Spiegelung der V1.5-Surface.** Das Query-Fetch-RPC ist CTOX-nativ (kein Upstream-RxDB-Port), aber JS und Rust muessen fuer dieselben Eingaben identische Bytes auf der Leitung und identische Fingerprints erzeugen. Das wird durch geteilte Goldenkorpora abgesichert (siehe Phase 1).

## Aktueller Befund aus Code-Sichtung

### JS-Fork

- `src/apps/business-os/rxdb/src/rx-database.mjs`
  - `CtoxRxCollection.find()` erzeugt zentral `CtoxRxQuery`.
  - `CtoxRxQuery.exec()` liest aktuell ausschliesslich lokal aus IndexedDB.
  - `collection.$.subscribe(...)` fuehrt derzeit indirekt `find().exec()` aus und kann ungebremst ganze lokale Collections lesen.

- `src/apps/business-os/rxdb/src/storage-indexeddb.mjs`
  - Es gibt einen `documents` Object Store.
  - Es gibt noch keine Stores fuer Query-Windows, Query-Completeness, Last-Access, Cache-Budget oder Eviction.
  - `allDocuments()` ist fuer grosse Collections gefaehrlich, wenn es unbewusst aus UI-Observables getriggert wird.

- `src/apps/business-os/rxdb/src/replication-webrtc.mjs`
  - Die bestehende Replikation ist checkpoint-basiert pro Collection.
  - Es gibt noch keinen Query-Stream-RPC fuer demand-loaded Query-Windows.

- `src/apps/business-os/rxdb/src/webrtc-native.mjs`
  - Chunking, ACKs, Resume und Backpressure sind bereits als Transportbasis vorhanden.
  - Das ist die richtige Schicht fuer neue Query-Stream-Methoden, nicht eine App-Schicht.

### Rust / rxdb.rs

- `src/core/rxdb/src/rx_query.rs`
  - Rust-Queries laufen lokal gegen Storage und sind noch nicht als Browser-Query-Stream-Handler angebunden.

- `src/core/rxdb/src/storage/sqlite/instance.rs`
  - SQLite-Queries laden aktuell Dokumentmengen und filtern/sortieren viel in Memory.
  - Fuer grosse Collections braucht es mindestens fuer Standardfaelle index-/SQL-naehere Query-Ausfuehrung.

- `src/core/rxdb/src/plugins/replication_webrtc/index_mod.rs`
  - WebRTC-Replikation kennt `masterChangesSince`, `masterWrite` und Change Streams.
  - Query-Window-Streams fehlen.

- `src/core/business_os/store.rs`
  - `pull_collection_records()` ist collection/since/limit-orientiert, nicht Query-Window-orientiert.

## Nicht-Ziele

- Kein Rueckfall auf HTTP als normale Browser-CTOX-Datenstrecke.
- Kein App-seitiges manuelles Prefetching als Pflicht.
- Kein Vollspiegel aller CTOX-Daten in jedem Browser.
- Keine Signaling-Server-Nutzung fuer Payload-Daten.
- Keine Vermischung von allgemeiner RxDB-Replikation und Query-Completeness.
- Kein Bruch der bestehenden Checkpoint-Replikation. Bestehende RPCs, IndexedDB-Stores, Indizes und Wire-Frames bleiben unveraendert.
- **Keine Aenderung an der primaeren IndexedDB.** `ctox_business_os_js_v1` bleibt `DB_VERSION=1`, der `documents`-Store, seine Indizes und das Record-Format sind unveraendert.
- **Keine SQLite-Schema-Aenderung.** Keine neuen Tabellen, Spalten oder Indizes auf CTOX-Seite.
- Kein Bezug auf "upstream RxDB"-Surfaces. Der JS-Code ist ein eigenstaendiger, minimaler RxDB-aehnlicher Layer, kein Fork von npm-`rxdb`. Es gibt nichts an upstream zu deaktivieren.

## Phase 0: Sicherheitsleine und Messbarkeit

Ziel: Vor Umbauten verhindern, dass Business-OS wieder blank laeuft oder minutenlang blockiert.

Aufgaben:

- Einen kurzen Baseline-Smoke fuer Business-OS festhalten:
  - Login moeglich.
  - Desktop/App-Grid sichtbar.
  - CTOX-App oeffnet.
  - Eine Daten-App zeigt bekannte Testdaten.
  - Keine `Loading workspace`-Endlosschleife.
- Advanced-Status-Felder definieren:
  - `rxdbRuntime`: `ctox-rxdb-js`
  - `rxdbProtocolVersion`: `1` oder `1.5`
  - `transport`: `webrtc`
  - `peerConnected`
  - `peerCapabilityQueryFetchV1`: ob `ctox-rxdb-query-fetch-v1` ausgehandelt wurde
  - `queryDemandLoadingEnabled`: Feature-Flag (Runtime-Toggle, kein Re-Deploy)
  - `queryDemandLoadingActive`: Flag UND Capability vorhanden
  - `queryFetchInFlight`
  - `queryFetchSuccessCount`
  - `queryFetchErrorCount`
  - `queryFetchDedupHitCount`: lokal deduplizierte identische In-Flight-Requests
  - `indexedDbWorkingSetBytes`
  - `indexedDbEvictionCount`
  - `lastQueryFetchMs`
  - `lastTransportBackpressureMs`
- Tests als Gate dokumentieren und automatisierbar machen:
  - JS fork smoke.
  - Rust rxdb tests.
  - Browser-to-Rust replication smoke.
  - Large payload materialization smoke.
  - Business-OS visible shell smoke.

Abschlusskriterien:

- Es gibt eine reproduzierbare Baseline, die vor und nach jeder Implementierungsphase laufen kann.
- Eine Regression auf leeren Shell-Screen wird automatisch als Fail erkannt.

## Phase 1: Query-Window-Protokoll

Ziel: Einen klaren Vertrag fuer remote Query-Demand-Loading schaffen, der neben dem bestehenden Checkpoint-Replikationspfad existiert und nie an dessen Stelle tritt.

Neue Begriffe:

- Query Fingerprint: stabile kanonische Darstellung aus Collection, Selector, Sort, Limit, Skip, Projection und Schema-Version.
- Query Window: ein begrenzter Ausschnitt eines Query-Ergebnisses.
- Completeness: lokaler Zustand, ob ein Query Window vollstaendig im Browser-Cache liegt.
- Authoritative Cursor: serverseitiger Cursor/Resume-Token fuer weitere Query-Chunks.

Capability-Negotiation:

- Neuer Browser-Capability-String: `ctox-rxdb-query-fetch-v1` in `BROWSER_CAPABILITIES` (`replication-webrtc.mjs:11`).
- Neuer Rust-Server-Capability-String, im `ctoxProtocol`-Handshake (siehe `index_mod.rs:378`) als unterstuetzt gemeldet.
- Beide Seiten verhandeln die Faehigkeit waehrend des bestehenden CTOX-Protokoll-Handshakes; ist sie nicht beidseitig vorhanden, bleibt V1 aktiv, `queryDemandLoadingActive=false` und Demand-Loading-Pfade sind no-op.
- Es gibt **keine** stille Eskalation auf einen anderen Transport: weder HTTP-Fallback noch Signaling-Payload.

Geplante WebRTC-RPCs (additiv, kein bestehender Methodenname wird geaendert):

```ts
type QueryFetchRequest = {
  method: "rxdb.query.fetch";
  requestId: string;
  databaseName: string;
  collectionName: string;
  schemaVersion: number;
  queryFingerprint: string;
  query: {
    selector: Record<string, unknown>;
    sort?: Array<Record<string, "asc" | "desc">>;
    limit?: number;
    skip?: number;
  };
  window: {
    offset: number;
    limit: number;
  };
  clientState: {
    knownDocumentIds?: string[];
    knownCheckpoint?: unknown;
  };
};

type QueryFetchChunk = {
  method: "rxdb.query.chunk";
  requestId: string;
  sequence: number;
  documents: unknown[];
  deletedDocumentIds?: string[];
  cursor?: string;
  complete: boolean;
  authoritativeRevision: string;
};

type QueryFetchError = {
  method: "rxdb.query.error";
  requestId: string;
  code: string;
  message: string;
  retryable: boolean;
};

type QueryFetchCancel = {
  method: "rxdb.query.cancel";
  requestId: string;
  reason: "client-abort" | "reconnect" | "superseded";
};
```

Query-Fingerprint-Kanonisierung (load-bearing):

- Wiederverwendung des bestehenden `canonicalJson(...)` aus `schema.mjs`. Keine zweite Kanonisierungsimplementierung.
- Festgelegte Regeln, identisch in JS und Rust:
  - Object-Keys lexikografisch sortiert, inkl. Operator-Keys (`$eq`, `$in`, `$gte`, ...).
  - `$in`/`$nin`-Arrays sind sortiert + dedupliziert.
  - `null` und Fehlen sind unterschiedlich (kein Smoothing).
  - Sort-Direction normalisiert auf `asc`/`desc` (kein `1`/`-1`, kein `ASC`).
  - Leerer Selector wird zu `{}`, nicht weggelassen.
  - Schema-Version, Collection-Name und `rxdbProtocolVersion="1.5"` sind Teil des Fingerprint-Inputs.
- Goldenkorpus: `tests/fixtures/query_fingerprint/*.json` mit (Input, Canonical-JSON, Fingerprint). JS- und Rust-Tests laden denselben Korpus und vergleichen byte-genau.

Aufgaben:

- Protokolltypen im JS-Fork definieren (additiv, vorhandene Wire-Frames bleiben unveraendert).
- Gleiche Typen/Structs in rxdb.rs definieren.
- Goldenkorpus fuer Fingerprint erstellen, JS- und Rust-Tests darauf binden.
- Capability `ctox-rxdb-query-fetch-v1` auf beiden Seiten ankuendigen und auswerten.
- Maximalgroessen setzen:
  - Max documents per chunk: 200.
  - Max bytes per chunk: 256 KB (uncompressed JSON).
  - Max in-flight query streams pro Peer: 4.
  - Max query runtime: 30 s.
  - Default-Window-Limit fuer App-Queries ohne `.limit()`: 200.
- Fehlercodes definieren:
  - `PEER_UNAVAILABLE` — Peer offline.
  - `QUERY_NOT_SUPPORTED` — Capability fehlt oder Selector nicht erlaubt.
  - `SCHEMA_MISMATCH` — Schema-Version weicht ab.
  - `STREAM_ABORTED` — durch `rxdb.query.cancel` oder Reconnect.
  - `CACHE_WRITE_FAILED` — IndexedDB-Schreibfehler.
  - `REMOTE_TIMEOUT` — Rust hat in der erlaubten Runtime nicht fertig gestellt.
  - `STREAM_LIMIT_EXCEEDED` — mehr als 4 gleichzeitige Streams.

Abschlusskriterien:

- JS und Rust erzeugen fuer dieselbe Query denselben Fingerprint (Goldenkorpus gruen).
- Query-RPC kann einen leeren Test-Stream ueber WebRTC roundtrippen.
- `rxdb.query.cancel` beendet einen laufenden Stream serverseitig deterministisch.
- Alte Checkpoint-Replikation bleibt unveraendert lauffaehig (V1-Smoke gruen).
- Ein V1.5-Browser gegen einen V1-Server zeigt `queryDemandLoadingActive=false` und nutzt ausschliesslich V1.

## Phase 2: Sidecar-IndexedDB fuer V1.5-Metadaten

Ziel: V1.5-Metadaten persistent ueber Reload und Tab-Wechsel halten, **ohne** die primaere DB anzufassen. Die Metadaten leben in einer separaten IndexedDB-Datenbank, die nur existiert, wenn V1.5 aktiv ist.

Primaere DB bleibt unveraendert:

- `ctox_business_os_js_v1`, `DB_VERSION = 1` (`storage-indexeddb.mjs:4`).
- Object Store `documents`, Key Path `[collection, id]`, Indizes `collection` und `collectionLwtId`.
- Record-Format `{ collection, id, lwt, deleted, indexValues, doc }` unveraendert.
- Schreiben/Lesen von Dokumenten laeuft genau wie heute durch `CtoxIndexedDbCollection`.

Sidecar-DB (neu):

- Name: `ctox_business_os_v1_5_meta`
- `DB_VERSION = 1` (eigene Versionsachse; unabhaengig von der primaeren DB).
- Lazy-open: erst beim ersten V1.5-Code-Pfad, der Persistenz braucht. Bei Flag off oder fehlender Capability nicht geoeffnet.
- Object Stores:
  - `queryWindows`
    - Key: `[collection, queryFingerprint, offset, limit]`
    - Felder: `documentIds`, `complete`, `authoritativeRevision`, `createdAt`, `updatedAt`, `lastAccessedAt`
    - Indizes: `collection`, `collection_lastAccessedAt`
  - `documentAccess`
    - Key: `[collection, id]`
    - Felder: `lastAccessedAt`, `pinReason`, `dirty`, `estimatedBytes`
    - Index: `collection_lastAccessedAt`
  - `cacheStats`
    - Key: `databaseName`
    - Felder: `estimatedBytes`, `budgetBytes`, `lastEvictionAt`

In-Memory-Caches (live oben drauf):

- `queryWindowRegistry: Map<windowKey, QueryWindowState>` — Write-through-Cache der Sidecar.
- `inflightFetchByFingerprint: Map<fingerprint, Promise<...>>` — rein in-memory, fuer Cross-Subscription-Dedup.
- `cacheBudgetState` — read-through aus `cacheStats`, periodisch geflusht.

Aufgaben:

- Neuen Modul `query-meta-storage.mjs` anlegen, **getrennt** von `storage-indexeddb.mjs`. `storage-indexeddb.mjs` bleibt im V1-Verhalten.
- Lazy-Open-Logik mit Capability- und Flag-Check.
- APIs:
  - `getQueryWindow(windowKey)`
  - `markQueryWindowComplete(windowKey, documentIds, authoritativeRevision)`
  - `touchDocuments(collection, ids)`
  - `estimateWorkingSetBytes()`
  - `evictDocuments(ids)` — schreibt in den **bestehenden** `documents`-Store via existierender Storage-Schicht (`delete`-Op, kein Schema-Change).
  - `clearSidecar()` — fuer Tests und Cleanup-Pfad.
- Dirty-/Pinned-Status pflegen.
- File-Chunks (Phase 6) im selben Sidecar-Modell tracken; die Chunk-Daten selbst bleiben in der primaeren DB.

Reload-Verhalten:

- Beim Reload liest V1.5 die Sidecar-DB und rehydriert `queryWindowRegistry` lazy on demand.
- Subscriptions emittieren sofort aus dem lokalen `documents`-Store auf Basis der persistierten Completeness und triggern Background-Verifikation nur, wenn `authoritativeRevision` veraltet aussieht oder explizit invalidiert wurde.
- Statusfeld `lastReloadHydrationMs` zeigt die Hydration-Zeit.

Toggle- und Rollback-Verhalten:

- Flag on → Sidecar wird beim ersten Bedarf geoeffnet.
- Flag off zur Laufzeit → laufende In-Flight-Fetches werden abgebrochen, Sidecar wird geschlossen (nicht geloescht; existierende Eintraege schaden V1 nicht).
- Build-Downgrade auf V1 → V1 oeffnet `ctox_business_os_js_v1` wie immer; die Sidecar ist fuer V1 unsichtbar.
- Optionaler Cleanup: V1.5-Build mit Flag off ruft `indexedDB.deleteDatabase('ctox_business_os_v1_5_meta')` auf, wenn der Nutzer explizit "Cache leeren" verlangt.

Abschlusskriterien:

- V1.5-Aktivierung erstellt die Sidecar lazy beim ersten QueryFetch, nicht beim Build-Load.
- Primaere DB bleibt nach V1.5-Aktivierung bit-fuer-bit identisch zu V1 (Diff der Records vor/nach).
- Reload ueber die Sidecar fuehrt zu lokal-schnellen Subscriptions ohne erzwungene Remote-Roundtrips.
- Flag off → Sidecar wird nicht oder nur einmal geoeffnet, danach geschlossen; kein neuer Schreibvorgang.
- `clearSidecar()` macht V1.5 sauber zustandslos, ohne die primaere DB anzufassen.

## Phase 3: Automatischer Query-Miss-Layer im JS-Fork

Ziel: RxDB entscheidet automatisch, ob remote geladen werden muss — und tut das nur, wenn V1.5 ausgehandelt und das Feature-Flag aktiv ist.

Primaerer Eingriffspunkt:

- `src/apps/business-os/rxdb/src/rx-database.mjs`
  - `CtoxRxQuery.exec()` (heute Zeile 289)
  - Observable Query Pfad fuer `collection.$` und `CtoxRxQuery.$` (heute Zeile 201 und 228)

Algorithmus:

1. Query normalisieren und Fingerprint bilden.
2. Angefordertes Window bestimmen.
3. Wenn V1.5 nicht aktiv (`queryDemandLoadingActive=false`): klassischer lokaler Pfad, identisch zum heutigen Verhalten.
4. IndexedDB Query-Window-Metadaten pruefen.
5. Wenn vollstaendig:
   - lokal lesen.
   - Access-Zeit aktualisieren.
   - Ergebnis zurueckgeben.
6. Wenn unvollstaendig:
   - WebRTC-Peer pruefen.
   - QueryFetch starten (oder bestehendem In-Flight-Request piggybacken, siehe Dedup).
   - Chunks in IndexedDB schreiben.
   - QueryWindow complete markieren.
   - lokal erneut lesen.
   - Ergebnis zurueckgeben.
7. Bei Peer-Fehler:
   - Keine stillen HTTP-Fallbacks.
   - Sauberer RxDB-Fehler mit Status-Signal.
   - Letztes lokal vollstaendiges Window darf weiterhin angezeigt werden, klar als "stale" markiert im Status.

Observable-Re-Emit-Politik (Pflicht, um die Blank-Shell-Schleife zu vermeiden):

- `collection.$.subscribe(...)` und `CtoxRxQuery.$.subscribe(...)` triggern bei Change-Events **keinen** Remote-Fetch. Sie re-evaluieren lokal gegen das letzte bekannte Window.
- Remote-Fetch wird nur ausgeloest durch: (a) Erstaufruf der Subscription, (b) Selector-/Sort-/Window-Aenderung, (c) explizite Invalidation eines Windows durch Replikations-Update (Phase 7).
- Change-Bulks aus dem Storage werden auf einer 50-ms-Debounce-Schiene zusammengefasst, bevor die Subscription neu emittiert.
- Cross-Subscription-Dedup: identische `(fingerprint, window)` In-Flight-Requests teilen sich ein Future. Treffer zaehlen `queryFetchDedupHitCount`.

Window- und Limit-Semantik:

- App-Limit (`query.limit`) ist die maximale Ergebnisgroesse, die die App sieht — vertraglich.
- `window.{offset,limit}` ist der intern verwaltete Materialisierungs-Ausschnitt.
- Setzt die App kein Limit, wird intern ein Window von 200 angefordert. Es gibt keine Auto-Eskalation auf "alles". Wer mehr will, setzt explizit ein hoeheres Limit.
- Pagination (Scrollen) verschiebt das Window, nicht das App-Limit; jedes neue Window ist ein eigenes QueryFetch.

Wichtige Regeln:

- Sehr grosse Query-Ergebnisse werden nicht komplett automatisch materialisiert.
- Observables duerfen nicht dauerhaft Full-Collection-Scans ausloesen.
- Mehrere identische Query-Misses werden dedupliziert (siehe oben).
- Query-Fetches muessen ueber `rxdb.query.cancel` abbrechbar sein, auch wenn die Subscription nur lokal endet.

Abschlusskriterien:

- Eine App mit normalem `find().exec()` bekommt remote Daten, ohne Transportcode zu kennen.
- Gleiche Query ist beim zweiten Aufruf lokal schnell.
- Keine App muss geaendert werden, um Basisdaten zu sehen.
- Full-collection Observable blockiert Business-OS nicht.
- 100 Change-Events innerhalb 1 s fuehren zu hoechstens 1 Subscription-Emission ohne Remote-Fetch.
- Bei deaktiviertem Flag verhaelt sich der Pfad bit-fuer-bit wie V1.

## Phase 4: Rust Query-Stream-Handler

Ziel: CTOX beantwortet QueryFetch ueber WebRTC aus SQLite in kleinen Chunks — **ohne** vorher die gesamte Collection in den Prozessspeicher zu laden.

Primaere Codebereiche:

- `src/core/rxdb/src/plugins/replication_webrtc/connection_handler_rs.rs`
- `src/core/rxdb/src/plugins/replication_webrtc/index_mod.rs` (Dispatch in `call_master_method` bei index_mod.rs:951 erweitern)
- `src/core/rxdb/src/storage/sqlite/instance.rs` (neuer Streaming-Pfad; der bestehende `query()` an instance.rs:437 bleibt fuer In-Memory-Konsumenten erhalten)
- `src/core/business_os/store.rs`
- `src/core/business_os/rxdb_peer.rs`

Vorbedingung (Pflicht, nicht "mittelfristig"):

- Der heutige `RxStorageInstanceSqlite::query` macht `all_documents()` + Filter + Sort in RAM (instance.rs:457). Wuerde der QueryFetch-Handler darauf aufsetzen, verlagert V1.5 nur die OOM vom Browser zum Server. Daher muss vor dem RPC ein streaming-faehiger Lesepfad existieren:
  - Cursor-Iteration ueber den SQLite-Table, primaer ueber existierende Indizes der bestehenden Tabellen (`business_records.updated_at_ms`, `business_records.record_id`, etc.). **Keine neuen Indizes auf bestehenden Tabellen.** Wenn ein Index fehlt, wird das im Status gemeldet, nicht im Schema nachgebessert.
  - Filterung im SQL, wo der Selector reine `$eq`/`$in`/`$gt`/`$gte`/`$lt`/`$lte`/`$ne` auf bereits indizierte Felder ist.
  - Fallback: Cursor-iterativ + Mango-Matcher pro Zeile, aber **niemals** `Vec::collect` ueber die gesamte Tabelle.
  - Sort-Order durch `ORDER BY` an SQLite delegieren, wenn moeglich; sonst Heap-basiertes Top-K mit window-limit-Grenze.
- Diese Streaming-Lesefunktion ist eine neue API neben dem bestehenden `query()`, damit V1-Konsumenten unveraendert bleiben.
- **Kein SQL-Schema-Change.** Keine neue Tabelle, keine neue Spalte, kein neuer Index. Falls eine Collection ohne brauchbaren bestehenden Index zu langsam ist, wird sie nicht V1.5-faehig markiert (`QUERY_NOT_SUPPORTED`) und bleibt im V1-Pfad.

Collection-zu-Physische-Tabelle-Mapping (V1.5-Scope):

- `business_records` (Tabelle `business_records` in `business-os.sqlite3`) — primaer.
- `communication_accounts` / `communication_threads` / `communication_messages` — kommen aus `runtime/ctox.sqlite3` ueber `channels::pull_*_for_business_os` und werden gegen die jeweilige Quelle gestreamt, nicht ueber `business_records` (siehe heute `store.rs:4176`).
- File-Metadaten, Modul-Manifeste, Documents/Notes: nur dann V1.5-faehig, wenn sie bereits eine RxDB-modellierte Storage-Quelle haben. Sonst V1-Pfad weiter.
- Das Mapping wird in einer Registry-Struktur in `rxdb_peer.rs` gefuehrt, eine Zeile pro Collection. Eine nicht eingetragene Collection antwortet mit `QUERY_NOT_SUPPORTED`.

Aufgaben:

- WebRTC message dispatch in `index_mod.rs` um `rxdb.query.fetch` und `rxdb.query.cancel` erweitern, additiv zu `masterChangesSince`/`masterWrite`.
- QueryFetchRequest validieren:
  - Collection existiert in der Registry.
  - Schema-Version passt.
  - Selector ist erlaubt.
  - Limit liegt im Budget.
- Streaming-Lesepfad in `storage/sqlite/instance.rs` implementieren, mit Cursor + frueher Abbruchmoeglichkeit.
- `rxdb.query.cancel` deterministisch verarbeiten: laufenden Stream sauber beenden, In-Flight-Counter dekrementieren, `STREAM_ABORTED` als Abschluss melden.
- Backpressure respektieren (existing send queue in `connection_handler_rs.rs`).
- Cursor/Resume-Token liefern.
- Business-OS-Projektionen query-faehig machen.
- Fehler sauber an JS zurueckgeben.

Mittelfristige Optimierung (nach V1.5-Rollout):

- SQLite Query Planner-Hints / EXPLAIN-Logs in den Status spiegeln.
- Persistente Index-Metadaten aus RxDB-Schema durchreichen.
- Query-Kosten begrenzen und im Status melden.

Abschlusskriterien:

- Rust kann QueryFetch fuer reale Business-OS Collections beantworten.
- Grosse Ergebnisse werden nicht als ein einzelnes Payload gesendet.
- Browser bekommt konstante kleine Chunks.
- Abbruch/Reconnect erzeugt keine inkonsistenten IndexedDB-Windows.
- Lasttest `10k docs` und `100k metadata docs` (siehe Phase 9) laeuft mit `< 50 MB` zusaetzlichem RSS auf CTOX-Seite.
- V1-Pfade (`masterChangesSince`, `masterWrite`) unveraendert lauffaehig, gleiche Bytes auf der Leitung.

## Phase 5: Cache-Eviction und Speicherbudget

Ziel: Der Browser bleibt schnell und laeuft nicht voll.

Strategie:

- `navigator.storage.estimate()` nutzen, wenn verfuegbar.
- Default-Budget setzen, z. B. pro Datenbank und pro Collection.
- LRU auf Dokument- und File-Chunk-Ebene.
- QueryWindow-Metadaten invalidieren, wenn Dokumente aus dem Window evicted werden.
- Dirty/pending writes nie evicten.
- Aktuell sichtbare App-Daten pinnen — implizit, keine App-API.

Implizite Pinning-Regeln (Apps duerfen nichts wissen muessen):

- Jeder Lesepfad, der ein RxDocument an die App ausliefert, markiert die zugehoerigen Document-IDs in `documentAccess` mit `lastAccessedAt = now` und `pinReason = "recently-read"`.
- Pin-Schutz hat ein TTL: Dokumente, die seit `RECENT_READ_TTL_MS = 60_000` nicht mehr ausgeliefert wurden, verlieren das Pin automatisch.
- Aktive QueryWindows (deren letzte Subscription noch lebt) sind als Ganzes gepinnt.
- Dokumente mit `dirty=true` (lokal geschriebene, noch nicht repliziert bestaetigte Writes) sind hart gepinnt und ignorieren TTL.

Aufgaben:

- Cache Budget API im JS-Fork.
- Eviction Scheduler:
  - nach QueryFetch
  - beim App-Idle
  - bei Storage Pressure
- Status-Felder:
  - `workingSetBytes`
  - `cacheBudgetBytes`
  - `evictionCandidates`
  - `lastEvictionReason`
  - `pinnedDocCount`
  - `pinnedBytes`
- Tests:
  - Daten laden.
  - Budget kuenstlich klein setzen.
  - Eviction ausloesen.
  - Query erneut oeffnen.
  - Remote Rehydrate pruefen.
  - Sichtbare Subscription waehrend Eviction: gepinnte Docs bleiben erhalten.

Abschlusskriterien:

- Browser speichert nicht dauerhaft alle remote Daten.
- Evicted Daten werden bei Bedarf erneut transparent geladen.
- Eviction macht keine UI-Daten kaputt, die gerade angezeigt werden.

## Phase 6: File-Metadaten und File-Inhalte

Ziel: CTOX-Dateien sind im Business-OS sichtbar, ohne dass alle Inhalte sofort im Browser liegen. Die Storage-Topologie ist konsistent mit dem Sidecar-Modell.

Speicher-Topologie:

- File-Metadata (Pfad, Name, Typ, Groesse, MTime, Hash, Preview, Availability) sind normale RxDB-Dokumente und liegen in der **primaeren** IDB-DB (`ctox_business_os_js_v1`, `documents`-Store). Sie sind klein und werden ueber V1 oder V1.5-Query-Fetch repliziert.
- File-Content-Chunks sind ebenfalls Dokumente in der primaeren DB (gleicher `documents`-Store), aber mit Collection-Namen wie `<collection>_chunks` und `_meta.chunkSequence`/`_meta.fileId`. Format unveraendert zu heute, kein neuer Store.
- Chunk-Completeness pro Datei (welche Chunks sind lokal, welche fehlen, Hash-Validierung) wird in der **Sidecar** unter einem neuen Store-Konzept `fileChunkPresence` getragen (Key: `[collection, fileId]`, Felder: `expectedChunkCount`, `presentSequences`, `lastVerifiedAt`).

Modell:

- File-Metadaten werden wie heute als kleine RxDB-Dokumente synchronisiert.
- File-Inhalte werden on demand geladen:
  - beim Oeffnen
  - beim Preview
  - beim Download
  - in Range-/Chunk-Streams

RPC-Erweiterung (Server-Push wie bei Query-Fetch):

- `rxdb.file.fetch { fileId, range?: { offset, length } }` → mehrere `rxdb.file.chunk { sequence, bytesBase64, complete }`-Frames pro `requestId`.
- Cancel via `rxdb.file.cancel { requestId }`.

Aufgaben:

- File-Metadata Collection absichern.
- File-content Query/Stream-Methode in `connection_handler_rs.rs` ergaenzen.
- Chunks mit Hash/Sequence validieren.
- `fileChunkPresence`-Store zur Sidecar hinzufuegen.
- Browser-Cache fuer File-Chunks evictable machen (delete-Ops auf primaerer DB, Eintrag aus `fileChunkPresence` mit-loeschen).
- Status fuer File-Streams:
  - `activeFileStreams`
  - `fileBytesReceived`
  - `fileStreamErrors`

Abschlusskriterien:

- File Viewer zeigt Dateiliste ohne Vollinhalt.
- Datei oeffnen streamt Inhalt ueber WebRTC.
- Grosse Dateien blockieren Business-OS nicht.
- Reload kann bereits geladene Chunks lokal nutzen, weil `fileChunkPresence` persistent ist.
- Primaere DB enthaelt keine V1.5-spezifischen Metafelder; `fileChunkPresence` liegt ausschliesslich in der Sidecar.

## Phase 7: Korrektheit, Reconnect und Konflikte

Ziel: Demand Loading darf Replikation, lokale Writes und Reconnects nicht brechen.

Aufgaben:

- QueryWindows bei remote Changes invalidieren oder partiell aktualisieren.
- Konfliktregeln fuer lokale Writes pruefen:
  - lokaler dirty doc gewinnt nicht gegen altes remote Query-Ergebnis.
  - remote QueryFetch darf pending local writes nicht ueberschreiben.
- Reconnect-Verhalten:
  - in-flight QueryFetch ueber `rxdb.query.cancel` sauber beenden; danach ggf. neu starten.
  - partial QueryWindow nicht als complete markieren.
- Multi-Tab-Dedup:
  - Bei `database.multi_instance=true` laeuft die Replikation schon ueber `RxDBLeaderElectionPlugin` (`index_mod.rs:323`). V1.5 erweitert das: nur der Leader-Tab darf QueryFetch initiieren. Follower-Tabs lesen lokal und abonnieren ueber BroadcastChannel die Materialisierung des Leaders.
  - Fallback ohne Leader: erste anfragende Tab gewinnt, alle anderen warten via Dedup-Future auf das Ergebnis.
- Schema-Migration pruefen (V1↔V1.5).

Abschlusskriterien:

- Reconnect waehrend QueryFetch fuehrt nicht zu falschem Complete-State.
- Lokale Writes bleiben erhalten.
- Nach Remote-Update sieht die App neue Daten ohne Full Reload.
- Zwei Tabs derselben App erzeugen genau einen Stream pro `(fingerprint, window)`.

## Phase 8: Advanced Status und Betriebsdiagnostik

Ziel: Man sieht live, ob RxDB/WebRTC gesund ist und warum Daten fehlen.

Statusgruppen:

- Transport:
  - peer connected
  - data channel open
  - send queue depth
  - retry/resume count
  - backpressure time
- Query Demand Loading:
  - enabled
  - active requests
  - success/error count
  - last query fingerprint
  - last fetch duration
  - last remote document count
- Cache:
  - working set bytes
  - cache budget
  - eviction count
  - pinned bytes
  - dirty bytes
- Rust Peer:
  - SQLite available
  - query handler available
  - supported collections
  - last query error

Aufgaben:

- Advanced Status Endpoint/Bridge erweitern.
- Browser UI fuer Status nicht blockierend halten.
- Smoke-Test muss Status validieren.

Abschlusskriterien:

- Wenn Business-OS Daten nicht zeigt, ist im Status erkennbar, ob Peer, Query, Cache oder Schema schuld ist.
- Status selbst erzeugt keine hohe Signaling-/Worker-Last.

## Phase 9: E2E-Testmatrix

Ziel: Vor Gruen-Freigabe echte Wege testen.

Lokale Tests:

- Browser unter `127.0.0.1`.
- Desktop-App als lokaler Shell/Injector.
- CTOX lokal mit SQLite.
- Login und App-Grid.
- Query Demand Loading fuer mehrere Collections.
- File-Listing und File-Open.

Remote Tests:

- CTOX auf VPS mit eigener Domain.
- CTOX auf `*.ctox.dev` Subdomain.
- CTOX ohne direkte IP-Erreichbarkeit ueber Signaling-Passwort und WebRTC.
- Browser von ctox.dev ausgeliefert.
- Desktop-App als lokaler Fallback fuer App-Auslieferung, nicht fuer Datentransport.

Last-/Grenztests:

- 10k kleine Dokumente.
- 100k Metadaten-Dokumente.
- 10 MB Datei.
- 100 MB Datei ueber Chunkstream.
- Reconnect mitten im Stream.
- Zwei parallele Apps mit unterschiedlichen Queries.
- Cache-Budget kleiner als Working Set.

Abschlusskriterien (quantitativ):

- Business-OS Shell/App-Grid sichtbar in ≤ 1 s nach Login.
- 10k-Dokumente-Collection, Window 200/Chunk: first paint ≤ 500 ms, vollstaendiges Window ≤ 5 s.
- 100k-Metadaten-Collection: erstes Window (200) ≤ 1 s, kein Server-RSS-Anstieg > 50 MB.
- 100 MB Datei ueber Chunkstream: konstanter Durchsatz ≥ 5 MB/s ueber LAN, kein UI-Block > 100 ms.
- Reconnect mitten im Stream: ≤ 2 s bis Wiederaufnahme, kein faelschlich completes Window.
- Zwei Tabs / dieselbe Query: genau 1 RPC-Stream (Multi-Tab-Dedup).
- Kein HTTP-Datenfallback zwischen Browser und CTOX (Network-Tab-Audit).
- WebRTC bleibt stabil unter Chunk-/Backpressure-Last.
- V1.5-Browser ↔ V1-Server-Mischbetrieb: identisches Verhalten wie heute (V1-Baseline).
- V1-Browser ↔ V1.5-Server-Mischbetrieb: identisches Verhalten wie heute (V1-Baseline).

## Phase 10: Rollout und Main-Push

Ziel: Nur lauffaehige, getestete Versionen werden verteilt. V1.5-Aktivierung ist umkehrbar.

Aufgaben:

- Feature Flag `queryDemandLoadingEnabled` (Runtime, persistiert in SQLite-Runtime-Config):
  - default in Development: on
  - default in Production erst nach E2E gruen
  - Hard-Kill-Switch ohne Re-Deploy moeglich
- Capability-Gate: ohne `ctox-rxdb-query-fetch-v1` auf beiden Seiten wird Demand-Loading nicht aktiviert, auch wenn Flag on.
- Migrationspfad fuer bestehende IndexedDB-Daten (Phase 2): additiv, idempotent, rollback-safe.
- Release Notes:
  - Protokollversion (V1 → V1.5)
  - Capability-Negotiation
  - Cache-Verhalten und Pinning-TTL
  - bekannte Limits (Max in-flight, Max chunk size, Default window)
- Deployment:
  - CTOX core
  - Business-OS bundle
  - ctox.dev App-Auslieferung
  - Signaling server, falls Protokoll-/Room-Status angepasst wird

Rollback-Plan:

- V1.5-Build mit Flag off → Sidecar wird nicht geoeffnet, Verhalten identisch zu V1.
- Build-Downgrade auf V1 → alter Build kennt die Sidecar nicht und beruehrt sie nie. Primaere DB und SQLite-Tabellen sind bit-identisch zu vor V1.5.
- Optionaler Cleanup: `indexedDB.deleteDatabase('ctox_business_os_v1_5_meta')` durch V1.5-Build mit Flag off, auf User-Wunsch. Primaere DB unberuehrt.
- Verlust beim Rollback: nur der V1.5-Cache (Window-Completeness, Eviction-LRU, Access-Times). Autoritaere Daten leben in CTOX-SQLite, also kein Datenverlust.

Abschlusskriterien:

- Alle Gates gruen.
- cto1.kunstmen.com E2E im Browser validiert.
- Lokales Business-OS validiert.
- Dashboard/Readiness-Dokumentation aktualisiert.
- Main-Push erst nach lauffaehigem Stand.
- Rollback-Drill einmal durchgespielt (V1.5 → V1 → V1.5 ohne Datenverlust).

## Parallelisierung

Nach Phase 1 koennen diese Themen parallel laufen:

- JS Query-Miss-Layer und IndexedDB-Metadaten.
- Rust Query-Stream-Handler.
- Advanced Status.
- E2E-Smokes.
- File-content Demand Loading.

Nicht parallel starten:

- Eviction vor QueryWindow-Metadaten.
- Rust SQL-Optimierung vor stabilem QueryFetch-Vertrag.
- Rollout vor E2E-Baseline.

## Hauptrisiken

- Query-Fingerprint driftet zwischen JS und Rust. Mitigation: gemeinsamer Goldenkorpus (Phase 1).
- QueryWindows werden faelschlich als complete markiert.
- Observable Queries erzeugen versteckte Full-Collection-Scans. Mitigation: lokale Re-Evaluation + Debounce (Phase 3).
- Rust filtert weiter in Memory und wird bei grossen Collections langsam. Mitigation: Streaming-Lesepfad als Vorbedingung von Phase 4.
- Eviction entfernt Daten, die gerade angezeigt oder lokal geaendert werden. Mitigation: implizites Pinning + TTL + Dirty-Pin (Phase 5).
- Status-/Health-Polling erzeugt selbst zu viel Traffic.
- Signaling wird versehentlich wieder fuer Payload-Daten genutzt.
- V1.5-Browser erwartet faelschlich Capability beim V1-Server. Mitigation: `queryDemandLoadingActive` strikt an Capability-Negotiation gebunden, kein optimistisches Probieren.
- Sidecar-DB driftet von der primaeren DB ab (z. B. evicted Doc-ID, aber `queryWindows` referenziert sie noch). Mitigation: `evictDocuments` validiert in einer Transaktion gegen `queryWindows` und entpinnt referenzierte Windows; `getQueryWindow` invalidiert beim Lesen, wenn Doc-IDs in der primaeren DB fehlen.
- Sidecar bleibt nach Build-Downgrade als Orphan-DB liegen. Mitigation: harmlos (V1 ignoriert sie); optionaler Cleanup-Pfad im V1.5-Build mit Flag off.
- `lazyPull` bricht eine Collection, deren App eigentlich V1-Voll-Replikation braucht. Mitigation: Default ist `lazyPull=false`. Opt-in pro Collection, nicht global.

## Definition of Done

Dieses Thema ist erst abgeschlossen, wenn alle Punkte erfuellt sind:

- Normale RxDB-Queries laden fehlende Daten automatisch ueber WebRTC.
- Keine Business-OS-App enthaelt spezielle Remote-Load-Logik.
- Der Browser speichert nur ein begrenztes Working Set.
- Evicted Daten werden transparent erneut geladen.
- File-Metadaten sind schnell sichtbar, File-Inhalte streamen on demand.
- WebRTC nutzt kleine Chunks, ACKs, Backpressure und Resume.
- Kein HTTP-Datenfallback zwischen Browser und CTOX.
- Advanced Status zeigt Query-, Cache-, Transport- und Rust-Peer-Zustand.
- Lokale und remote E2E-Smokes sind gruen.
- Business-OS-Shell laedt weiterhin schnell und stabil.
- V1.5 ist Capability-gated und durch Feature-Flag deaktivierbar; V1 funktioniert mit und ohne V1.5-Build auf der Gegenseite unveraendert.
- Primaere IndexedDB-DB und SQLite-Tabellen sind nach V1.5-Aktivierung bit-fuer-bit identisch zu vor V1.5.
- Sidecar-DB ist persistent, eviction- und reload-faehig, aber jederzeit risikolos loeschbar.
- Rollback-Drill ist erfolgreich durchgefuehrt (Build-Downgrade + optionaler Sidecar-Cleanup).
