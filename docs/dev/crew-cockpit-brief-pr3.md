# Auftrag CREW-COCKPIT · PR-3 „Das Zuhause der Crew“ (Neubau `modules/ctox`) — ENTWURF, wird nach PR-2 finalisiert

Lies zuerst `docs/dev/crew-cockpit-vision.md`, dann `docs/dev/crew-cockpit-board-20260905.md`, dann `docs/dev/crew-cockpit-brief-pr1-pr2.md` §2j (Lehren) und §6 der PR-Texte von PR #58 und PR-2 (Verträge). Rollen: Fable orchestriert und reviewt, du setzt um; Abweichungen im PR-Text, keine eigenen Ziele.

## 0. Ziel in einem Satz

Die CTOX-App wird das Zuhause der Crew: Man sieht, welches Wesen gerade arbeitet und woran, warum etwas wartet oder gescheitert ist, was es gekostet hat, und man kann eingreifen; die anderen Wesen sind zu Hause, jedes mit Profil, Seele, Lebenslauf, Learnings und Stundenzettel. Kein Poster, keine Leerflächen, keine Enum-Namen, kein Zufall.

## 1. Was weg muss (aus dem Audit vom 05.09.)

Das statische 16-Knoten-SVG-Poster samt fünf SVG-Buildern und der Kommunikations-Zustandsmaschine; das Web-Stack-Panel; der `ctoxSeed`-Datenpfad (`runs/communications/tools` immer leer) und alles, was daran hängt; die Regex-Redaktion (`hasSensitiveUiLeak`); `cleanUiCopy`; die doppelte i18n-Tabelle (nur `locales/*.json` bleibt, tote Schlüssel raus); JSON-Import/-Export von Tasks; `find().limit(200)` + `slice(0,20)`; der 4-s-`innerHTML`-Neuaufbau; der nie gelöschte Fokus-Task in `sessionStorage`; das `role="listitem"` auf Buttons. Behalten und säubern: `mergeBundleWithCommands`, `commandTaskFromProjection`, `normalizeExecutionProgress` (Projektionslogik) und die dort vorhandenen Tests.

## 2. Informationsarchitektur

Drei Ansichten in einem Fenster, Shell-Vertrag v2, Drawer für Details (Business-OS-Philosophie: zentrale Hauptansicht plus Slide-in-Drawer, kein permanentes Drei-Spalten-Raster).

**A · Zuhause (Startansicht).** Oben ein schmaler Statuskopf aus `ctox_harness_status`: läuft/pausiert, wer im Einsatz ist, Kapazität, Warteschlange (pending/leased/blocked), Druck, Arbeitszeitfenster. Darunter die Crew: alle Mitglieder aus `ctox_crew_members` mit Zustand `on_duty | home | resting_after_failure`; das Mitglied im Einsatz steht am Arbeitsplatz (groß), die anderen zu Hause (klein, ruhend). Klick auf ein Mitglied öffnet sein Profil (Drawer). Wenn nichts läuft, ist das Zuhause die ganze Ansicht; kein „nicht erfasst“.

**B · Arbeitsplatz (Task im Einsatz oder ausgewählter Task).** Wer arbeitet (Wesen, Name), woran (Titel, Quelle, Prompt lesbar; Geheimnisse werden serverseitig nie projiziert, also keine Client-Redaktion), Plan-Schritte aus `execution_progress` als Hauptelement, darunter Live-Aktivität aus `ctox_harness_events` (denkt / Werkzeug X / Plan geändert / Turn fertig, mit Zeit), Runs aus `ctox_runs` (Versuch, Modell, Tokens, Kosten, Dauer, Urteil, Rückblick). Zustand als Satz: „wartet auf Freigabe bis 14:30“, „Wiederholung 2 von 3 ab 14:05“, „gescheitert: <failure_class>“; Quelle sind `hold_reason`, `wait_entity_*`, `retry_not_before`, `failure_class`, `failure_attempt_count`, `lease_expires_at`. Steuerung als Knopfleiste: abbrechen (`ctox.command.cancel`), freigeben (`ctox.queue.release`), blockieren (`ctox.queue.block`), wiederholen (`ctox.queue.retry`), Priorität (`ctox.task.update`), zuweisen (`ctox.crew.assign`, nur vor Lease); jeder Knopf nur, wenn der Server ihn erlaubt (Policy-Mirror in `shared/permissions.js` folgt `BusinessOsScope::task(id,false,false)`), sonst ausgeblendet mit Grund im Tooltip.

**C · Warteschlange.** Taskliste nach echten Zuständen gruppiert: Läuft · Wartet (mit Grund und Bis-wann) · Review · Blockiert · Fertig · Gescheitert; Filter nach Quelle/Modul, Suche; Abfragen mit Selektor, Sortierung `updated_at_ms desc`, Limit, Paging; Retention respektieren (aktive plus N terminale). Kapazität und Pause als Owner-Regler (`ctox.queue.capacity`, `ctox.queue.pause`) hier, nicht im Statuskopf.

**Profil (Drawer).** Name, Form, Farbe; Seele als fünf Slider (Admin editiert, `ctox.crew.member.update`), Charakterskizze; Spezialitäten; Lebenslauf aus `stats`; Learnings (bestätigen / bearbeiten / löschen, `ctox.crew.learning.*`); Stundenzettel = Runs des Mitglieds, neueste zuerst, gebunden.

## 3. Wesen-Komponente (gemeinsam mit PR-4)

Eine Komponente `shared/crew-creature.js` (aus dem heutigen `crewCreatureHtml` in `business-chat.js` herausgelöst) mit Identität aus `ctox_crew_members` (nie Hash), Größen `fab | dock | home | workplace`, und genau diesen Zuständen, jeder mit eigener Animation ohne Farbabhängigkeit: `sleeping` (zu Hause), `queued` (wartet), `waking` (Lease), `thinking`, `tooling` (Werkzeug), `reviewing`, `waiting` (auf X; zeigt Sanduhr), `failed` (X-Augen), `done` (zufrieden). Mapping aus `ctox_queue_tasks.status/route_status` + `ctox_harness_events.kind` der letzten 10 s + `execution_progress.review.status`. `prefers-reduced-motion` respektieren. Tests: Zustandsmapping je Kombination, Snapshot der Klassen.

## 4. Gestaltung

Hierarchie statt Labels: eine große Sache pro Ansicht. Zahlen nur, wenn sie eine Entscheidung ändern (Kosten, Wartezeit). Deutsch und Englisch vollständig aus `locales/*.json`, keine Inline-Strings, keine Enum-Namen. Nichts außerhalb des App-Hosts. Geometrie-Labor und Shell-V2-Vertrag müssen grün sein. Drei Zustände klar unterschieden: Harness idle (Zuhause), Sync nicht verbunden (Wesen schlafen mit Hinweis), Laden fehlgeschlagen (Fehler mit Grund). Reaktion auf Live-Updates über `commandBus.subscribe` und RxDB-Subscriptions mit gezielten DOM-Updates, kein Vollneuaufbau.

## 5. Abnahme (Entwurf)

`node src/apps/business-os/scripts/assert-shell-v2-contract.mjs`, `node src/apps/business-os/scripts/shell-v2-geometry-lab.mjs` (Breiten 640/1180/1440), Modul-Tests (`modules/ctox/test.js` neu: Zustandsmapping, Query-Grenzen, Policy-Mirror, i18n-Vollständigkeit beider Sprachen), Browser-Beweis: eine echte Aufgabe über die Crew-Leiste auf thesen (src/ ist live) mit Screenshots aller drei Ansichten und des Profils im Verlauf (wartet → arbeitet → fertig oder gescheitert), plus ein Abbruch, der wirkt. Unabhängiges UI/UX-Review durch Kimi vor dem Merge.

## 6. Nicht-Ziele

Keine Server-Änderung außer Policy-Mirror in `shared/permissions.js`; keine Änderung an Chat-Leiste (PR-4) oder Tickets (PR-5); kein Ersatz des Shell-Fenstersystems.
