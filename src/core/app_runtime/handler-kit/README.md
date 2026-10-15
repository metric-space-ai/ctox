# CTOX App Handler-Kit

Dieses Kit führt App-Handler ohne laufenden CTOX-Core aus. Es besteht aus
Node-ESM-Modulen ohne Installation oder Build. Der Speicher und die nativen
Fähigkeiten werden für lokale Tests als Fixtures bereitgestellt.

## Handler und Manifest

Ein Handler exportiert eine asynchrone Funktion `default(input, ctx)`. Sein
Rückgabewert muss JSON sein und darf höchstens 1 MiB groß sein.

```js
// handlers/add.mjs
export default async function add(input, ctx) {
  const existing = await ctx.read("entries", {
    selector: { id: input.id },
  });
  await ctx.write("entries", {
    type: "put",
    doc: { id: input.id, title: input.title },
    base_rev: existing[0]?.rev ?? null,
  });
  ctx.log("info", "Eintrag gespeichert", { id: input.id });
  return { id: input.id };
}
```

```js
import { runHandler } from "./index.mjs";

const manifest = {
  app_id: "example",
  app_ver: "1",
  collections: { entries: { schema: {}, class: "business" } },
  handlers: { "example.add": "handlers/add.mjs" },
  capabilities_required: [],
};
const fixtures = {
  root: "/absolute/path/to/app",
  collections: { entries: [] },
  logs: [],
};
const result = await runHandler(manifest, "handlers/add.mjs", {
  id: "entry-1", title: "Erster Eintrag",
}, { fixtures });
```

Pfade sind relativ zu `fixtures.root` (standardmäßig Arbeitsverzeichnis).
Absolute Pfade, `..` und Symlinks aus dem App-Verzeichnis werden abgewiesen.
Wenn `manifest.files` gesetzt ist, muss der SHA-256 des Moduls mit seinem Eintrag
übereinstimmen. Produktive App-Versionen benötigen unveränderliche Verzeichnisse:
der Einstieg wird anhand seines Hashes geladen, relative Modulimporte folgen
aber dem normalen Node-Modulcache.

## Kontext

- `await ctx.read(collection, { selector, sort?, limit?, skip? })` liefert Kopien.
  Ohne `limit` gibt es keine implizite Zeilengrenze. Mango unterstützt
  `$eq/$ne/$gt/$gte/$lt/$lte/$in/$nin/$exists/$regex` (nur Flag `i`),
  `$and/$or/$not` und eine Ebene `$elemMatch`. Fehlend ist nicht `null`.
  Sortierungen sind Listen aus `{ feld: "asc" | "desc" }`; Gleichstände
  werden nach `id` aufgelöst.
- `await ctx.write(collection, op)` unterstützt `{type:"put",doc}`,
  `{type:"patch",id,patch}` und `{type:"remove",id}`.
  Ein optionales `base_rev` muss zur vorhandenen Revision passen;
  `null` bedeutet „noch nicht vorhanden“. IDs sind unveränderlich.
  Fixture-Revisionen sind nur Testrevisionen. Das Kit simuliert keine
  produktiven Tombstones, Transaktionen oder Replikation.
- `await ctx.command(type, payload)` dispatcht ausschließlich im eigenen
  Manifest deklarierte Befehle an `fixtures.command`.
- `await ctx.model(req)`, `await ctx.http(req)` und
  `await ctx.cap.name(...args)` / `ctx.cap.name.method(...args)` rufen
  ausschließlich bereitgestellte Core-Fixtures auf.
- `ctx.log(level, message, fields?)` schreibt mit App-Zuordnung in
  `fixtures.logs`; Level: debug, info, warn oder error.

Fähigkeiten müssen sowohl in `manifest.capabilities_required` als auch in
`fixtures.grants` stehen. Die Grants stellen die Core-/Admin-Freigabe dar,
nicht eine selbst erteilte Manifest-Erlaubnis. HTTP ist nur mit expliziter
Deklaration, Grant und `fixtures.http` verfügbar. Es gibt keinen Netz-Fallback.
Die Implementierungen liegen in `fixtures.capabilities` (flache Namen oder
verschachtelte Objekte) beziehungsweise `fixtures.model/http/command`.
Die Deklaration eines Namespace erlaubt dessen Unterfunktionen.

Optional prüft `fixtures.policy(request)` jede Operation mit App-ID, Actor und
Operation erneut. Nur der Rückgabewert `true` erlaubt sie. Ohne diese Fixture
prüft das lokale Kit Manifest und Grants; im Core ist die Actor-Policy Pflicht.

## Fremde Collections

Die aufrufende App deklariert beispielsweise
`foreign_collections: { people: { owner_app: "directory", ops: ["read"] } }`.
Zusätzlich muss das Eigentümermanifest in
`fixtures.apps.directory` die Collection besitzen und
`exports: { people: ["read"] }` enthalten. Beide Seiten und die Actor-Policy
werden bei jedem Zugriff geprüft. Fixtures dürfen nur bereits autorisierte
Datensätze enthalten; das Kit ersetzt keine serverseitige Mandantentrennung.

## Migrationen

`runMigration(manifest, collection, docs, { fixtures })` lädt
`manifest.migrations[collection].migrate_fn`. Der optionale vierte Parameter
dient der lokalen Testumgebung; der Drei-Parameter-Aufruf verwendet das
Arbeitsverzeichnis. Die Funktion exportiert `default(doc, ctx)` und gibt ein
Dokument mit derselben ID oder exakt `{ archive: true }` zurück.
Schreibzugriffe, Befehle und native Fähigkeiten sind während der Migration
verboten. Originale werden nicht verändert.

Das Ergebnis sind Vorschläge `{id, base_rev, action:"write", doc}` oder
`{id, base_rev, action:"archive", archived_doc}`. Der Core muss sie in seinem
Writer-Thread mit erneuter Prüfung von `base_rev` übernehmen, echte Revisionen
erzeugen und archivierte Originale erhalten. Ein Ergebnis des Kits ist kein
Commit und keine Berechtigung, einen Versionskonflikt zu überschreiben.
Die Rückgabe ist auf 1 MiB begrenzt; größere Migrationen benötigen begrenzte
Batches im Core.

## Tests und Grenzen

```sh
node --test src/core/app_runtime/handler-kit/handler-kit.test.mjs
```

Dies ist ein Testwerkzeug für vertrauenswürdigen App-Quelltext, keine Sandbox.
Dynamisch importierter Testcode besitzt die Rechte des Testprozesses. Deshalb
keinen fremden App-Code auf dem Operator-Host damit prüfen. Die produktive
Laufzeit benötigt einen eigenen, dauerhaft überwachten Node-Prozess pro App,
256 MiB Heap, Node-Permissions ohne zusätzliche Freigaben, ausschließlich
LocalTransport, Policy-Prüfungen im Core und Abbruch bei CPU-Zeitüberschreitung.
