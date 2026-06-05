# Outbound App Production-Ready Plan

Stand: 2026-06-05

Ziel: Die CTOX Business OS Outbound-App wird production-ready auf `cto1.kunstmen.com`: Campaigns werden per natuerlichem Briefing angelegt, CTOX richtet die Campaign ueber einen Skill ein, alle Daten laufen ueber RxDB/native Commands, Kommunikation bleibt approval-gated, E-Mail und physische Briefe sind auditierbar, Conversations ist die Detailansicht fuer Kommunikation.

Aktueller Gesamtstand: **76%**

Production-ready ist erst erreicht, wenn die kritischen Flows lokal und live im Browser mit sauberem Profil, Reload, Console-/Network-Check und Screenshot-/JSON-Evidence bestanden sind.

## Harte Architekturregeln

- Business OS und Outbound verwenden keine HTTP-Datenkanaele. Daten, Commands und Writebacks laufen ueber RxDB/native Peer/Command Bus.
- Einzige Business-OS-Ausnahme ist der ChatGPT-Subscription-Login.
- Fachliche Outbound-Aktionen, bei denen CTOX interpretieren oder konfigurieren soll, erzeugen normale CTOX Prompt-Tasks wie das Rechtsklick-Menue: `ctox-business-os-chat-submit` -> Chat -> `business_os.chat.task` -> RxDB `business_commands`.
- Die Outbound-App interpretiert Kampagnenbriefings nicht lokal und baut keine Heuristik-Sonderwege.
- Skills schreiben Ergebnisse nur ueber explizite Writeback-Commands zurueck, zum Beispiel `outbound.campaign.apply_setup`.
- Keine ausgehende Nachricht wird ohne explizite Freigabe gesendet oder als Brief markiert.
- Physische Briefe laufen ueber Draft, Freigabe, PDF/Print und manuelle Versandmarkierung.
- Conversations bleibt Detail-App fuer Threads, Messages, Freigaben und Antworten; Outbound verlinkt dorthin.

## Fortschrittsmodell

| Welle | Gewicht | Status | Fortschritt | Beitrag |
| --- | ---: | --- | ---: | ---: |
| W0 Baseline, Release-Schutz, Deploy-Pfad | 5% | In Arbeit | 85% | 4.3% |
| W1 RxDB-only Runtime und Command Bus | 15% | In Arbeit | 95% | 14.3% |
| W2 Campaign-Erstellung per natuerlichem Briefing | 15% | In Arbeit | 95% | 14.3% |
| W3 Research Funnel und Import | 10% | In Arbeit | 85% | 8.5% |
| W4 Active Outreach Core | 15% | In Arbeit | 75% | 11.3% |
| W5 Kanaele E-Mail und physischer Brief | 10% | In Arbeit | 65% | 6.5% |
| W6 Conversations-Integration | 10% | In Arbeit | 55% | 5.5% |
| W7 Reply Loop, Follow-ups, Terminfindung | 8% | Offen | 15% | 1.2% |
| W8 Safety, Approval, Audit, Governance | 6% | In Arbeit | 70% | 4.2% |
| W9 Browser-E2E, Live-Evidence, Regression Gates | 6% | In Arbeit | 70% | 4.2% |
| **Gesamt** | **100%** | **In Arbeit** |  | **76%** |

## Aktuelle Evidenz

- Frontend-Test `node --test src/apps/business-os/modules/outbound/outbound.test.mjs`: bestanden mit 9 Tests.
- RxDB-only Guard `node src/apps/business-os/scripts/assert-rxdb-only.mjs`: bestanden.
- `git diff --check`: bestanden.
- Live-Stand vor dieser Planaktualisierung: `38e4a8d16`, Build `20260605-outbound-templates-ready3`, Release `branch-main-20260605T162106Z`.
- Lokale Aenderung fuer Build `20260605-outbound-templates-ready4`: Campaign-Setup spawnt jetzt explizit einen neuen Kontext-Chat (`action: context-chat`, `reuseActive: false`) und bleibt ein `business_os.chat.task` mit `required_skills: ["business-os-outbound-campaign-setup"]` und Writeback `outbound.campaign.apply_setup`.

## W0 Baseline, Release-Schutz, Deploy-Pfad

- [x] Sauberen Deploy-Worktree von dirty Haupt-Checkout getrennt.
- [x] Divergierenden lokalen Hauptstand nicht blind gepusht.
- [x] Build-Key fuer neue Outbound-Aenderung erhoeht.
- [ ] Neue Aenderung committen, pushen und per `ctox upgrade --dev` live deployen.
- [ ] Live-Service-Status, Release-Symlink und Build-Key pruefen.
- [ ] Automatische Release-Bereinigung bestaetigen: nur letzte zwei Builds bleiben.

## W1 RxDB-only Runtime und Command Bus

- [x] HTTP-Command-Sonderweg entfernt und durch RxDB-only Guard abgesichert.
- [x] Outbound Campaign Briefing Update laeuft als `outbound.campaign.briefing.update` ueber `business_commands`.
- [x] Skill-Writeback-Vertrag fuer Campaign Setup ist `outbound.campaign.apply_setup`.
- [ ] Live erneut pruefen, dass Outbound keine `/api/business-os/commands` Requests erzeugt.
- [ ] Remaining Active-Outreach-Commands fuer Send, Letter, Reply und Scheduler auf Reload-Persistenz pruefen.

## W2 Campaign-Erstellung per Natuerlichem Briefing

- [x] 20 deutsche und 20 englische Kampagnenideen vorhanden.
- [x] Templates sind kanalexplizit: E-Mail oder physischer Brief.
- [x] Campaign-Editor hat zentrales frei editierbares Briefing.
- [x] Template-Auswahl schreibt in dasselbe zentrale Briefing.
- [x] Sprachwechsel rendert Templates neu und behaelt Draft-Zustand.
- [x] Speichern erzeugt normalen CTOX Prompt-Task wie Rechtsklick-Menue.
- [x] Prompt enthaelt Skill, Template-ID, Template-Titel, RxDB-only-Regel und Writeback-Contract.
- [ ] Live-Browser beweisen: alle Templates sichtbar, Auswahl funktioniert, ein Speichern erzeugt Chat-Task und keinen HTTP-Command.
- [ ] Echter Skill-Worker-Lauf bis `outbound.campaign.apply_setup` live beweisen, ohne ChatGPT-Login-Automation.

## W3 Research Funnel und Import

- [x] Import-/Research-Grundpfade existieren.
- [x] Excel-/Text-Importvalidierung ist getestet.
- [ ] Live mit realer Excel-Datei erneut pruefen: Import, Reload, Firmen sichtbar, kein leerer Funnel.
- [ ] Fehlerzustand bei kaputter Datei, leerer Datei und Dopplungen sichtbar machen.
- [ ] Research-Progress und Command-Fehler fuer Nutzer klar anzeigen.

## W4 Active Outreach Core

- [x] Active-Outreach-Modul und Lead Queue existieren.
- [x] Draft- und Approval-Pfade sind angelegt.
- [ ] Lead einem Sender/Mailbox-Account zuweisen.
- [ ] Draft automatisch vorbereiten, aber approval-gated halten.
- [ ] Freigabe, Ablehnung, Edit und Reload live im Browser pruefen.
- [ ] Keine automatische Kommunikation ohne Review-Status.

## W5 Kanaele E-Mail und Physischer Brief

- [x] Campaign-Briefings koennen E-Mail und physische Briefe ausdruecken.
- [x] Writeback-Vertrag erlaubt `active_outreach`, `communication_strategy`, `sequence_strategy` und `landing_page`.
- [ ] Campaign-Postfach pro Campaign konfigurieren und in Conversations verlinken.
- [ ] Brief-Draft, Dokument-/PDF-Export und manuelle Versandmarkierung live pruefen.
- [ ] E-Mail-Send-Gate und Provider-Queue live pruefen.

## W6 Conversations-Integration

- [x] Grundidee: Outbound verlinkt zur Kommunikations-/Conversations-App.
- [ ] Outbound Message Deep Link in Conversations stabilisieren.
- [ ] Approval aus Conversations zurueck nach Outbound synchronisieren.
- [ ] Reply-Status, Bounce, Unsubscribe und Antwortklassifikation sichtbar machen.
- [ ] Live-Browser-E2E fuer Outbound -> Conversations -> Approval/Reply -> Outbound.

## W7 Reply Loop, Follow-ups, Terminfindung

- [ ] Sequence-Strategie aus Skill-Writeback in Campaign Settings darstellen.
- [ ] Follow-up-Timer approval-gated vorbereiten.
- [ ] Antwortklassifikation und Stop-Regeln pruefen.
- [ ] Terminvorschlaege automatisch vorbereiten, aber vor Versand freigeben lassen.

## W8 Safety, Approval, Audit, Governance

- [x] Campaign Setup darf keine Nachrichten senden.
- [x] Prompt fordert explizit keine HTTP-Datenkanaele und nur Command-Bus-Writeback.
- [ ] Approval-Audit fuer alle Kanaele live pruefen.
- [ ] Rollen/Rechte fuer Freigabe pruefen.
- [ ] Rate Limits, Sender Limits, Suppression List und Abmelde-/Beschwerdepfade pruefen.

## W9 Browser-E2E, Live-Evidence, Regression Gates

- [x] Lokale Frontend- und RxDB-only-Tests laufen.
- [ ] Live-Browser-E2E fuer Campaign Template Flow nach Build `ready4`.
- [ ] Live-Browser-E2E fuer Import/Funnel mit realer Datei.
- [ ] Live-Browser-E2E fuer Active Outreach, Approval, E-Mail und Brief.
- [ ] Screenshots fuer 1920px, 1440px, 1024px und mobile Breite.
- [ ] Console Errors, Page Errors und unexpected failed Requests je Flow = 0.
