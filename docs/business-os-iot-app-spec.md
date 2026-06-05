# CTOX IoT — Spec

> **Keine Monitoring-App, keine Grafana.** Eine Oberfläche, auf der du **CTOX in Klartext beauftragst**, auf die
> physische Welt aufzupassen und zu **handeln**. Das Produkt ist die **Delegation**, nicht das Anschauen von Kurven.
> Du schreibst Sätze. CTOX wählt die Signale, schreibt den Wächter-Code, wacht und handelt.

---

## 1. Die eine Einheit: der **Auftrag** (nicht „Widget", nicht „Panel")
Eine Kachel ist **ein stehender Auftrag an CTOX**, formuliert als ein Satz:
> *„Pass auf den Serverraum auf — wenn's zu heiß wird, fahr die Kühlung hoch und meld dich, wenn's nicht runtergeht."*

Das ist die ganze Konfiguration. Kein Formular, kein Schwellen-Feld, kein JSON. Der Auftrag ist ein **lebendes** Ding, kein Chart: er zeigt, **dass CTOX wacht und arbeitet**.

### Lebenszyklus eines Auftrags (das ist das eigentliche Design)
1. **Entwurf** — du schreibst den Satz. CTOX liest die Absicht, **wählt die Signale** aus deinen Assets, und **antwortet im Klartext**: *„Ich beobachte `Serverraum·Temperatur`. Ich handle, wenn sie über längere Zeit ungewöhnlich steigt. Dann: Kühlung hochfahren, dich benachrichtigen, eskalieren wenn's nicht hilft. Passt das?"* Du bestätigst oder **redest nach** („nur nachts" / „auch die Luftfeuchte"). CTOX generiert im Hintergrund den Wächter — du siehst davon nichts außer dem Satz.
2. **Scharf** — die Kachel zeigt: deinen **Auftrags-Satz** (groß, das Wichtigste), das **gebundene Signal** als kleiner Live-Kontext (Wert/Sparkline — *untergeordnet*), und **„CTOX wacht"**. Editieren = den Satz ändern → CTOX baut neu.
3. **CTOX handelt** — löst der Wächter aus, **kippt die Kachel** auf Live-Aktivität: *„CTOX bearbeitet: Ursache prüfen → Kühlung +2 → beobachten."* Du siehst den Agenten arbeiten, nicht eine rote Linie.
4. **Erledigt / Verlauf** — Ergebnis + Zeit auf der Kachel („vor 3 min: Kühlung hochgefahren, Temp fällt"), dann zurück auf *scharf*. Verlauf erkundbar.

→ Eine Kachel beantwortet nie „wie sieht der Graph aus", sondern **„worauf passt CTOX für mich auf, und was hat es zuletzt getan".**

## 2. Auftrag anlegen — Gespräch, keine Konfiguration
- **`+ Auftrag`** → **ein** Feld: *„Was soll CTOX überwachen und tun?"*
- Du schreibst einen Satz. CTOX schlägt den fertigen Auftrag in Klartext vor (Signal · wann · was). Du bestätigst oder verfeinerst **im Gespräch**.
- Null Dropdowns, null Bedingungs-Baukasten, null JSON. **Das Gespräch ist die Konfiguration.**
- Technische Umsetzung (CTOX generiert den Wächter-Code, hält den Handlungs-Prompt) bleibt **unsichtbar** — nur „im Detail anzeigen" für Neugierige.

## 3. Mitte — zwei Ansichten deiner Aufträge (umschaltbar)
- **Kacheln** (Default): deine laufenden CTOX-Aufträge als lebende Karten — was ist scharf, was feuert gerade, was hat CTOX zuletzt getan. Konfigurierbar/anordnenbar, **persistent**.
- **Liste**: dichte Tabelle für viele Aufträge — *Auftrag · Signal · Status (scharf/handelt/Fehler) · letzte Aktion · Ergebnis*. Sortier-/filterbar.
- Segmented-Umschalter (wie `customers` Board/Tabelle). **Keine rechte Spalte.**

## 4. Links — Assets & Signale (das Vokabular, über das du delegierst)
- Baum der Assets/Geräte und ihrer **Signale**; anlegen / strukturieren / Live-Zustand übersehen.
- Eine **Quelle anschließen = ansagen**: *„hier kommt ein MQTT-Broker"* / *„auf diesen Webhook pushen unsere Sensoren"* → CTOX richtet den Connector + das Payload-Mapping ein. Kein JSON.
- Das Linke liefert die Begriffe, die CTOX nutzt, wenn du rechts einen Auftrag schreibst.

## 5. I/O — rein & raus (inkl. **Webhooks**)
- **Rein (Signale):** MQTT · HTTP-Poll · WebSocket · **Webhook (inbound)**. Mapping macht CTOX aus deinem Satz.
- **Raus (Handlung):** der Agent erfüllt den Auftrags-Prompt — **Webhook (outbound)** · Geräte-Write (MQTT/HTTP) · Benachrichtigung · Ticket · Kommunikation. *„…und meld's per Webhook ans ERP"* ist gültige Handlung.

## 6. Warum es eine **CTOX**-App ist (nicht beigeklatscht)
- **CTOX baut die Aufträge selbst:** *„CTOX, richte die Kühlketten-Überwachung für alle Kühlhäuser ein"* → CTOX legt die Aufträge an. Aufträge sind CTOX-native Records, also vom Agenten **und** vom Menschen editierbar.
- **CTOX führt aus:** Auslösung → durable Queue-Task mit dem Handlungs-Prompt → Agent leased & handelt unter Review/Outcome/Spawn-Budget-Gates *(Kette im E2E belegt)*.
- Die Kachel ist die **sichtbare „CTOX arbeitet"-Fläche** — der Unterschied zu jedem Dashboard-Tool.

## 7. Wie ein Trigger **tatsächlich** funktioniert — CTOX schreibt den Wächter, das Rust-Backend führt ihn aus
Das ist die eigentliche Funktionsweise. Nicht „Grafana-Alert-Engine", sondern **CTOX als Coding-Agent gegen das Rust-Backend**:

1. **CTOX schreibt den Wächter.** Aus dem Auftrags-Satz erzeugt ein Agent-Turn (`ctox.iot.auftrag.compile`) ein **kleines Programm** in der eingebetteten Skript-Runtime der IoT-Engine — Rust-nativ **Rhai** (sandboxed, kein FS/Netz; alternativ die in der Harness vorhandene JS-Runtime). Beliebige Logik, **kein festes Schema, keine Schwelle in der Engine**.
2. **Das Rust-Backend stellt die Signal-API bereit**, die der Wächter aufruft:
   - lesen: `signal.last()`, `signal.window("15m")`, `signal.rate()`, mehrere via `signals["raum.temp"]`;
   - `state` — **persistenter Zustand** zwischen Aufrufen (für „seit 5 min", Hysterese, Zähler, gleitende Mittel);
   - melden: `fire(grund)`.
   Mehr kann der Wächter nicht — nur diese Lese-Zugriffe → echte Sandbox.
3. **Die Engine führt ihn stateful aus** — pro neuem Datenpunkt des gebundenen Signals (bzw. Tick für Zeitbedingungen), mit Zeit-/Speicher-Limit. **Kein LLM pro Messwert**: die Intelligenz steckt einmalig im generierten Code, nicht in der Schleife.
4. **`fire(grund)` → Handlungs-Prompt als Queue-Task → Agent** (Pfad `iot-event-queue-task`, im E2E belegt).
5. **Lifecycle als Code-Artefakt:** Satz ↔ Wächter-Version in `iot_aufträge` persistiert. Satz ändern → CTOX **regeneriert** den Wächter. Compile-/Laufzeitfehler → Auftrag „braucht Aufmerksamkeit", CTOX bekommt den Fehler und **repariert ihn selbst** (Coding-Agent); Rollback auf letzte gute Version.
6. **„Das Rust-Backend entsprechend nutzen":** kein Daemon-Recompile — das Backend hostet **Runtime + Signal-API + Scheduler + Sandbox** und persistiert das Artefakt. CTOX *nutzt* diese Bausteine, indem es den Wächter schreibt, der sie aufruft. Backend-Deltas dafür: Rhai (o.ä.) einbetten, Signal-API exponieren, Wächter-Tabelle + Scheduler-Loop in der IoT-Engine, `ctox.iot.auftrag.compile` (Satz → Code via Agent-Turn).

## 8. Visualisierung (bewusst untergeordnet)
Signal-Kontext auf der Kachel: Live-Wert + Sparkline; bei Bedarf größerer Verlauf (LTTB), Gauge, Geo-Pin. **Dient dem Auftrag** — ist nie der Zweck der Kachel.

## 9. CTOX-Business-OS-Vertrag (kurz)
- Records: `iot_aufträge` (Satz + gebundene Signale + Wächter-Ref + Handlungs-Prompt + Status), `iot_dashboards`; Signale/Assets in den bestehenden `iot_*` (alle nach `collection_creators` + Schema-Contract + Hashes).
- Commands `ctox.iot.auftrag.*` / `.dashboard.*` / `.connector.*`; CLI + `business_commands` teilen denselben `iot::`-Code.
- Shell-Tokens · `ctox-pane`-Chrome · `CtoxResizer` · 2-Pane · **keine JSON-Felder** · App-Store-installierbar · i18n.

## 10. Qualitäts-Gate
- [ ] Liest sich als **„meine laufenden CTOX-Aufträge"**, nicht als „Dashboard aus Charts".
- [ ] Auftrag anlegen = **einen Satz schreiben** + Gespräch; nie Formular/JSON.
- [ ] Kacheln sind **lebendig** (zeigen CTOX wachen/handeln/Ergebnis), keine statischen Panels.
- [ ] Umschaltbar **Kacheln ⇄ Liste**; Aufträge persistent/konfigurierbar.
- [ ] Trigger = von CTOX generierter Wächter (keine Schwelle/Heuristik im UI); Handlung = Prompt → Agent.
- [ ] **Webhooks rein & raus**; MQTT/HTTP/WS rein.
- [ ] Politur auf `customers`/`shiftflow`-Niveau; Shell-Resizer/Chrome/Tokens; 2-Pane.
- [ ] Alles **vom Agenten baubar**; gegen Live-Daemon über RxDB/WebRTC.
