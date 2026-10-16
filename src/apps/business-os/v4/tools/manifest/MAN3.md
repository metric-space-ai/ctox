# MAN3: kleinere App-Icons, CAP3B- und AGENT-Nachträge

Klon `~/Documents/ctox-shell-v4`, Branch `shell-v4/prototype`, Basis `src/apps/business-os/`. Kein git außer Lesen.

## 1. Icons: 256 px + @512 statt 1024-px-Original

**Vorher:** 38 Apps trugen unter `modules/<app>/assets/icon.{jpg,png}` eine byte-gleiche Kopie des gewählten
Bildes. Das waren 34 JPEG mit 1024 px und 4 PNG mit 512 px (creator, desktop, explorer, file-viewer), zusammen 4,63 MB.

**Jetzt:** Der Generator legt je App ab:

- `assets/icon.<ext>` mit 256 px, zusammen 0,46 MB
- `assets/icon@512.<ext>` mit 512 px, zusammen 1,41 MB

`icon@512` entsteht nur, wenn die Quelle größer als 256 px ist. Ist die Quelle nicht größer als die Zielgröße, wird
byte-gleich kopiert. Das trifft die vier 512-px-PNG bei `@512`. Hochskaliert wird nie.

Das Manifest trägt jetzt:

- `layout.icon_asset = "assets/icon.<ext>"`
- `layout.icon_asset_srcset = "assets/icon.<ext> 1x, assets/icon@512.<ext> 2x"`
- `_icon_asset_source` nennt Quelle, Quell-px und die Paketgrößen.

knowledge bleibt unverändert. Ihr Bild liegt schon im eigenen Paket, samt eigenem srcset 256/512/1024 aus module.json.

### Verkleinern, plattformneutral

- In `src/apps/business-os` gibt es kein `node_modules`, also weder sharp noch ein anderes JS-Bildwerkzeug.
- Ein reines JS-Verfahren hätte einen JPEG-De- und -Encoder gebraucht (34 Baseline-JPEG). Das war im Zeitbudget nicht
  vertretbar.
- **Schreiblauf:** `resizeIcon()` in `gen-manifest.mjs` nutzt `ffmpeg`. Das läuft unter Linux, Windows und macOS und
  rechnet so: `scale=px:px:force_original_aspect_ratio=decrease:flags=lanczos`, JPEG `-q:v 2`, `-map_metadata -1`,
  bitexact. Zwei Läufe ergaben byte-gleiche Ausgaben.
- **Rückfall** nur unter macOS: `sips -Z`.
- **Ohne Werkzeug** meldet der Schreiblauf `icon_package` als Verstoß. Es gibt keine stille Kopie.
- **Bildmaße** (PNG IHDR, JPEG SOFn) liest `imageSize()` in reinem JS.

### --check gegen Quell-Hash statt Byte-Gleichheit

Die Bytes hängen vom Werkzeug ab. Deshalb gibt es das Ableitungsbuch `v4/tools/manifest/icon-derivations.json`, Format
`ctox-shell-v4-icon-derivations-1`. Es hält je App:

- `source` und `source_sha256`; letzterer muss gleich dem Auswahl-sha256 sein
- `source_px`
- je Ausgabedatei `px`, `sha256` und `tool`

`--check` braucht kein Werkzeug. Es meldet `icon_package` (Exit 1), wenn

- die Quelle nicht den sha256 der Auswahl trägt (wie MAN2),
- die Auswahl oder Quelle gewechselt hat (Ableitungsbuch veraltet),
- eine Paketdatei fehlt oder nicht den sha256 aus dem Ableitungsbuch trägt,
- sich die Menge der Ausgabedateien geändert hat.

Kopie und Quelle laufen also weiterhin nicht unbemerkt auseinander. Das Manifest übernimmt die abgeleiteten Icons nur,
wenn `files` zum Ableitungsbuch passt.

### srcset im v4-Rahmen: Vermerk für FR6d (nicht angefasst)

Der v4-Rahmen nutzt srcset **nicht**:

- `v4/frame/index.js` `appView()` bildet nur `iconAsset = artifactUrl(id, ver, layout.icon_asset)`.
- `resolveIcon()`, `iconFor()` (Launcher, Reiter, Taskleiste) und das Fenster-Icon setzen nur `src`.
- `shared/window-manager.js` setzt `iconEl.srcset = options.iconSrcSet` nur im Zweig ohne Promise, um Zeile 746.
- v4 übergibt `iconAsset` aber als Promise, Zeile 585. Damit greift `iconSrcSet` in v4 nie.
- Nur Shell-V2 reicht `layout.icon_asset_srcset` durch (`app.js:4756`).

**Für FR6d:**

1. In `appView()` `iconSrcSet` aus `layout.icon_asset_srcset` bilden. Jeden Eintrag einzeln über
   `artifactUrl(id, ver, pfad)` auflösen und die Deskriptoren behalten.
2. `iconSrcSet` in `iconFor()` (Launcher, Reiter, Taskleiste) und im Fenster-Icon setzen.
3. Im Promise-Zweig von `window-manager.js` srcset ebenfalls setzen.

Ohne srcset zeigt v4 überall das 256-px-Bild. Bei 40–80 px Anzeige reicht das bis DPR 3.

## 2. CAP3B-Nachträge

- **`iot`:** `model.run` ist nicht mehr in `capabilities_required`, weil der Handler-Code kein `ctx.cap.model.run` mehr
  aufruft. Neu leitet der Generator aus `ctx.model(` den Grant `model` ab, und zwar in `scanHandlerCode`.
  - Ergebnis: `iot.capabilities_required` enthält jetzt `model`. Beleg: `_capability_evidence.model` =
    `handlers/_codegen.js`.
- **`research`:** `agent.command.settle` ist nicht mehr enthalten. Das kommt aus dem Code, `_lib/records.js` ruft es
  nicht mehr auf.
- **Ratchet-Baseline** `v4/os-primitives/check-caps.baseline.json`: `model.run` und `agent.command.settle` gestrichen,
  von 9 auf 7 Einträge. Übrig sind `agent.flow.record`, `command.get` und `pentest.*` ×5.
- **Abweichung vom Whitelist-Rahmen, begründet:**
  - Datei: `v4/os-primitives/catalog.mjs`, drei Zeilen. `PRIMITIVES.model = ['model']`, exakter Name ohne Präfix.
  - Ohne diesen Eintrag stuft check-caps den Grant `model` als UNBEKANNT ein und schlägt an. Dann ließe sich
    „Grant `model` im Manifest“ plus „Baseline senken“ plus „check-caps OK“ nicht gleichzeitig erfüllen.
  - `model.run` bleibt App-Logik (APP_LOGIC) und ist in keinem Manifest mehr.
- **Abgeleitetes Dokument:** Den generierten Block in `v4/os-primitives/MAPPING.md` habe ich mit `gen-mapping.mjs` neu
  eingesetzt. `--check` ist jetzt „up to date“.
  - Der Block war schon vorher veraltet, wegen OLG `files.materialize` und coding-agents.

## 3. AGENT-Nachtrag (coding-agents, coding_turns:read)

- Die Deklaration `modules/coding-agents/v4-manifest.json` enthält `foreign_collections.coding_turns: ["read"]`.
  Das Manifest ist neu erzeugt.
- **Offen, außerhalb dieses Strangs:** Das Plattform-Manifest
  `~/.local/state/ctox-sync-v4/docs/v4-manifests/ctox-platform.manifest.json` kennt `coding_turns` als Collection,
  **exportiert sie aber nicht**.
- Der Generator klemmt den Zugriff deshalb ab, wie bei jeder Plattform-Collection ohne Export. Das Manifest zeigt
  `foreign_collections` ohne `coding_turns`, dazu das `_todo` „coding_turns: Plattform exportiert die Collection
  nicht“. DRIFT.md führt das unter `platform_clamps`.
- Damit der Zugriff im Manifest erscheint, muss die Plattform `exports.coding_turns = ["read"]` eintragen. Das ist eine
  Aufgabe für den Plattform- bzw. AGENT-Strang.
- **Hinweis:** Während MAN3 lief, hat ein paralleler Strang coding-agents geändert. Neu sind `ctox.coding.cancel.js`
  und `_coding_turn.js`, deshalb trägt `capabilities_required` jetzt auch `agent.coding.cancel` und
  `agent.coding.turn`. Das Manifest gibt den Stand zum Zeitpunkt des letzten Laufs wieder.

## 4. Tests und Abnahme

Neue und angepasste Tests in `gen-manifest.test.mjs`:

- **MAN3 imageSize/iconOutputs/iconSrcsetFor:** prüft PNG-Kopf, Nicht-Bild, mit und ohne @512 sowie das srcset.
- **MAN3 heutiger Baum:**
  - jede abgeleitete Paketdatei hat 256 bzw. 512 px, mit Prüfung des Bildkopfs
  - mindestens 38 Apps und mindestens 76 Dateien
- **MAN3 Mini-Repo:**
  - Ablauf: 600-px-PNG, dann Schreiblauf mit 256 px + @512 und srcset, dann ein sauberer Prüflauf ohne Werkzeug.
  - Eine manipulierte `@512` ergibt einen Verstoß. Der nächste Schreiblauf leitet neu ab.
  - Eine neue Quelle samt Auswahl ergibt ebenfalls einen Verstoß.
  - Ohne Werkzeug erwartet der Test im Schreiblauf einen Verstoß statt einer stillen Kopie.
- **MAN2-Tests angepasst:**
  - Das Mini-Repo nutzt ein echtes 8×8-PNG und damit den Kopierpfad.
  - Die Paketdatei wird gegen das Ableitungsbuch statt gegen den Auswahl-sha256 geprüft.
  - Statt „srcset entfällt“ wird jetzt das neue Paket-srcset erwartet.

Läufe am 16.10.2026, kurz nach 00:00:

| Befehl | Ergebnis |
|---|---|
| `node v4/tools/manifest/gen-manifest.mjs` | Exit 0, 76 Icon-Dateien abgeleitet |
| `node v4/tools/manifest/gen-manifest.mjs --check` | Exit 0, keine Abweichung (auch shell.manifest.json nicht) |
| `node --test v4/tools/manifest/gen-manifest.test.mjs` | 43 pass, 0 fail |
| `node v4/os-primitives/check-caps.mjs` | OK: 66 OS-primitive capabilities, 7 app/platform caps within ratchet baseline |
| `node v4/os-primitives/check-caps.mjs --calls` | 200 Aufrufe, ABWEICHUNG 0, HUELLE 0, Exit 0 |
| `node v4/os-primitives/gen-mapping.mjs --check` | MAPPING.md up to date |

FR6d, FIX8 und AGENT arbeiten parallel an Shell- und App-Dateien. Ein späteres `--check` kann deshalb Abweichungen in
deren Manifesten zeigen, besonders in `shell.manifest.json`. Das liegt am Parallelbetrieb, nicht an MAN3. Abhilfe ist
ein erneutes `node v4/tools/manifest/gen-manifest.mjs`.

## Geänderte und neue Dateien

- `v4/tools/manifest/gen-manifest.mjs`: Icon-Ableitung, Ableitungsbuch, srcset, Grant `model`
- `v4/tools/manifest/gen-manifest.test.mjs`
- `v4/tools/manifest/icon-derivations.json` (neu)
- `v4/tools/manifest/MAN3.md` (neu)
- `modules/<38 Apps>/assets/icon.{jpg,png}`: jetzt 256 px
- `modules/<38 Apps>/assets/icon@512.{jpg,png}` (neu)
- `v4/manifests/*.manifest.json` und `v4/tools/manifest/out/*`, `REPORT-M2.md` (neu erzeugt)
- `v4/os-primitives/check-caps.baseline.json`: von 9 auf 7 Einträge
- `v4/os-primitives/catalog.mjs`: Grant `model`, siehe Begründung unter 2.
- `v4/os-primitives/MAPPING.md`: nur der generierte Block
