# CTOX App Handler-Kit

Plain Node-ESM ohne Installation oder Build. Das Kit ist für lokale Portierungs-
und Vertragstests vertrauenswürdiger Handler gedacht; es ersetzt weder die
Core-Autorität noch die Prozessgrenze der produktiven Laufzeit.
Benötigt Node >=22.15 (ESM-Lader-Hooks); der gemeinsame Linux-Prüfer verwendet
das gepinnte Node 26.

## Handler und Beispiel

```js
// handlers/add.js
export default async function handle(command, ctx) {
  const { id, title } = command.payload;
  return ctx.transaction(async tx => {
    const current = await tx.get("entries", id);
    return tx.put("entries", { id, title }, { base_rev: current?.rev ?? null });
  });
}
```

```js
import { runHandler } from "./index.mjs";

const manifest = {
  app_id: "example", app_ver: "1",
  collections: { entries: { schema: {}, class: "business" } },
  handlers: { "example.add": "handlers/add.js" },
  capabilities_required: [],
};
const fixtures = {
  root: "/absolute/path/to/app",
  actor: { id: "writer", role: "writer", display_name: "Autor", is_admin: false },
  collections: { entries: [] },
};
const result = await runHandler(manifest, "handlers/add.js", {
  id: "command-1", type: "example.add",
  payload: { id: "entry-1", title: "Erster Eintrag" },
}, { fixtures });
```

Der Handler erhält den vollständigen Befehl:
`{id,type,payload,actor,app_id,app_ver,record_id?,module?,client_context?}`.
Export `handle` wird ebenfalls akzeptiert. Abschlussreferenzen dürfen benannte
Exporte mit `handlers/queue.js#onReply` auswählen. Ein Datei-Basisname mit
`_` ist eine Bibliothek und kein Einstieg.

Relative statische Importe bleiben innerhalb von `handlers/`; statische
Builtin- und Paketimporte werden abgewiesen. Der produktive Worker verbietet
auch dynamische Importe. Das Kit ist keine Sandbox: nach dem Laden läuft der
vertrauenswürdige Handler mit den Rechten des Testprozesses. Jede Ausführung lädt
eine eigene Modul-Generation, auch bei unverändertem Einstieg und geändertem
Helper. `manifest.files` enthält SHA-256 pro Datei. Alternativ zum lokalen
`fixtures.root` liefert `fixtures.artifacts[hash]` die CAS-Quellen; Hashes
werden geprüft. Absolute Pfade, ausbrechende Pfade und Symlinks sind verboten.

## Kontext

- `read(collection,{selector,sort?,limit?,skip?})` liefert Kopien, ohne
  implizites Zeilenlimit. Mango: Vergleichsoperatoren, `$in/$nin/$exists`,
  `$regex` mit höchstens Flag `i`, `$and/$or/$not`, eine Ebene `$elemMatch`.
  Fehlend unterscheidet sich von null; Sortiergleichstände werden nach id gelöst.
- `get(collection,id)` liefert ein Dokument oder null.
- `put(collection,doc,{base_rev?})`, `patch(collection,id,patch,{base_rev?})`,
  `remove(collection,id,{base_rev?})`; alternativ `write(collection,op)`.
  Revisionen des Kits sind ausschließlich Testrevisionen.
  Remove hinterlässt nur id, rev und deleted; alte Revisionen erlauben keine
  Wiederbelebung.
- `transaction(async tx => …)` bündelt Datenänderungen atomar.
  tx hat ausschließlich read/get/write/put/patch/remove. Fehler rollen zurück;
  zwischenzeitliche Änderungen führen zu conflict. Äußeres ctx und externe
  Wirkungen sind während des Callbacks gesperrt.
- `command(type,payload,{id?,record_id?,module?,client_context?})` ruft
  `fixtures.command` auf und liefert dessen Annahme:
  `{id,status:"accepted"|"rejected",task_id?,reason?}`, keinen Abschluss.
- `model(req)`, `http(req)`, `cap.<name>.<fn>(req)` benutzen ausschließlich
  Fixtures. Fähigkeit muss deklariert, administrativ gewährt und implementiert
  sein. Keine Netz- oder Ersatzroute. `fixtures.capabilities` unterstützt
  flache oder verschachtelte Namen.
- `now()` verwendet `fixtures.now` oder die lokale Testuhr; `uuid()` erzeugt
  eine UUID. Globals: URL, URLSearchParams, TextEncoder/TextDecoder,
  crypto.subtle.digest und crypto.randomUUID; CtoxError unterstützt
  `new CtoxError({code,message,path?,uncertain?})`.
- `cursor.get(key="default")`, `cursor.set(value,key="default")` benutzen
  einen durch `fixtures.trigger_id` gebundenen Cursor.
- `log(level,message,data)`: debug/info/warn/error in `fixtures.logs`.

`fixtures.policy(request)` prüft optional jede Operation; nur true erlaubt sie.
`command_rights` wird vor dem Laden des Einstiegs ausgewertet. Der produktive
Core muss seine authentifizierte Policy und aktuelle Grants selbst durchsetzen;
Fixture-Rollen sind kein Berechtigungsnachweis.

## Fremde Daten, Trigger und Abschlüsse

Beide Manifeste müssen zustimmen: z. B.
`foreign_collections: { people: {owner_app:"directory",ops:["read","watch"]} }`
und `fixtures.apps.directory.exports: {people:["read","watch"]}`.
Fixture-Daten müssen bereits auf erlaubte Zeilen/Felder reduziert sein.

Das Kit validiert `triggers:[{handler,on:{collection,op}}|{handler,every:"PT1M"}]`
und Mindestabstand eine Minute. Fremd-Trigger benötigen watch.
Es startet selbst keinen Scheduler und keine Change-Stream-Verbindung.
Trigger- und Abschluss-Einstiege können gezielt mit runHandler aufgerufen
werden; die dauerhafte Auslieferung und höchstens einmalige Task-Bindung besitzt
die produktive Core-Laufzeit. `agent.task.enqueue`-Fixtures müssen echte
Annahmeobjekte ihrer Testumgebung liefern.

## Migrationen

`runMigration(manifest,collection,docs,{fixtures})` lädt
`manifest.migrations[collection].migrate_fn` (vierter Parameter optional).
Signatur `default async(doc,ctx)`, Rückgabe Dokument mit gleicher ID oder
exakt `{archive:true}`. Keine Datenänderungen oder externen Wirkungen.

Ergebnis: Vorschläge `{id,base_rev,action:"write",doc}` oder
`{id,base_rev,action:"archive",archived_doc}`.
Der Core übernimmt sie separat in seinem Writer mit erneuter
Basisrevisions-/Generationsprüfung. Ein Vorschlag ist kein Commit.

## Prüfung

Nur über die zugewiesene Linux-Lane:
```sh
bash src/core/app_runtime/verify.sh kit
```
Die produktive Node-Prozessgrenze wird durch `verify.sh bundle` geprüft.
Das Kit selbst läuft mit den Rechten seines Testprozesses; fremden, ungeprüften
Code deshalb nicht auf dem Operator-Host ausführen.
