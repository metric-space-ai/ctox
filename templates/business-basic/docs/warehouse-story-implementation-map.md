# Warehouse Story Implementation Map

Diese Map buendelt WH-001 bis WH-050 nach produktiver Arbeitsfaehigkeit. Sie ist der Arbeitsplan fuer die Umsetzung, nicht eine zweite Spezifikation.

## G1 Virtuelle Lagerstruktur

Stories: WH-001, WH-002, WH-003, WH-004, WH-005, WH-006, WH-007, WH-008, WH-017, WH-018, WH-029

Status: umgesetzt und getestet fuer den produktiven Kern.

Bereits vorhanden:
- Lagerauswahl links.
- Virtuelles Lager als mittige Hauptansicht.
- Bereiche und Slots visuell.
- Slot-Selektion oeffnet unteres Arbeitsmodul.
- Slot umbenennen, sperren/entsperren, duplizieren.
- Struktur anlegen und duplizieren.
- Suche nach Lager, Zone oder Slot.
- Lager aus der Such-/Namenseingabe anlegen.
- Lager und Zonen im unteren Strukturmodul bearbeiten, umbenennen, aktivieren/deaktivieren und duplizieren.
- Zonen aus einem Lager anlegen.
- Slots aus einer Zone als Serie anlegen.
- Deaktivierung zeigt Impact auf untergeordnete Plaetze und Bestaende.
- Rechtsklick-Kontext fuer Lager, Zone und Slot mit direkten Aktionen: bearbeiten, Bereich/Slots anlegen, duplizieren, sperren/entsperren, Inventurmodus und Auditmodus.
- Slot-Eigenschaften werden persisted: Kapazitaet, Slot-Typ, Gang, Regal, Ebene und Hinweis.
- Die Lagerkarte zeigt Slot-Auslastung als `Bestand/Kapazitaet`, sobald eine Kapazitaet gepflegt ist.
- Browser-QA: A2 mit Kapazitaet 24, Pickfach, Gang A, Regal 02, Ebene 2 und Hinweis gespeichert; Reload zeigt `2/24` aus Postgres.
- Kapazitaet wird in Wareneingang, Putaway und Umlagerung serverseitig geprueft.
- Browser-QA: Wareneingang `PO-CAPACITY-BLOCK` mit 30 Einheiten nach A2 wird mit `Target slot capacity exceeded (32/24).` abgelehnt.
- Strukturpflege zeigt vor der Massenanlage eine konkrete Vorschau der naechsten Slots mit Name, Typ, Regal, Ebene und Kapazitaet.
- Browser-Regression: Lager `Regression Lager pxltw` angelegt, Bereich und Slots erstellt, Slot `A1-QA` gespeichert, Rechtsklick-Duplizieren erzeugt `A1-QA Copy`, Sperren/Entsperren funktioniert.
- Responsive QA: Im engen In-App-Browser bleibt die virtuelle Lagerkarte der erste sichtbare Arbeitsanker; Wareneingang und Ausgang folgen darunter, statt die Karte zu verdraengen.
- Browser-QA: Rechtsklick auf Slot A2 oeffnet das CTOX-Kontextmenue mit Bearbeiten, Duplizieren, Sperren/Entsperren, Inventur, Audit und Business-Datensatz-Pruefung.

## G2 Artikel, Owner und Lagerregeln

Stories: WH-009, WH-010, WH-019, WH-021, WH-024, WH-025, WH-041, WH-042

Status: umgesetzt und getestet fuer Artikel, Owner, 3PL-Baseline und kritische Rechtepruefung.

Bereits vorhanden:
- Owner-scharfe Bestaende im Datenmodell.
- Balance-Key mit Sentinel-Werten fuer Lot/Serial.
- Charge/Serial im Kernel und in Tests.
- Artikelstamm links im Warehouse-Arbeitsbild.
- Artikel anlegen, bearbeiten, duplizieren und deaktivieren ueber die Warehouse-API.
- Artikelauswahl oeffnet das untere Arbeitsmodul mit SKU, Einheit und Tracking-Modus.
- Rechtsklick-Kontext fuer Artikel bearbeiten, duplizieren und Deaktivierung pruefen.
- Rechtsklick auf Artikel fuehrt Bearbeiten/Duplizieren/Deaktivieren direkt im Warehouse-Modul aus; KI-Pruefung bleibt als separater Kontextprompt.
- Owner wird in Suche, Slot-Chips, Bestandslisten und Umlagerungspruefung sichtbar.
- Umlagerungen zeigen und erhalten Owner-scharfe Balance-Keys.
- Bewegungen fuer deaktivierte Artikel werden im Runtime-Pfad blockiert.
- Browser-Regression: Artikel `Regression Artikel pxltw` mit SKU `REG-PXLTW-001` angelegt, bearbeitet und Kontextaktionen geprueft.
- Sonderlagerbedingungen werden ueber Slot-Typen, QS-/Sperrbestand, Kapazitaet und Tracking-Pflichten geprueft.
- Rechtekritische Aktionen schreiben eine explizite `role_gate_reviewed` Integration-Event-Spur.
- 3PL-Leistungen koennen owner-scharf als `three_pl_charges` gebucht werden.
- Browser-QA: `recordRoleReview` und `recordThreePlCharge` aus der rechten Operationsrail ausgefuehrt; Postgres zeigt `role_gate_reviewed` und `3pl-cust-nova-20260507-2`.

## G3 Wareneingang und Einlagerung

Stories: WH-011, WH-012, WH-021, WH-024, WH-025, WH-026, WH-027, WH-040

Status: umgesetzt und browsergetestet.

Bereits vorhanden:
- Wareneingangs- und Einlagerungsrail links.
- Putaway-Aufgaben aus Snapshot sichtbar.
- Zielslot im Bestand sichtbar.
- Wareneingang links als echte Receipt-Buchung gegen Postgres.
- Receipt erzeugt receiving-Bestand am Inbound Dock und offene Putaway-Aufgabe fuer den Zielslot.
- Offene Putaway-Aufgaben werden direkt am Arbeitselement abgeschlossen.
- Abschluss bucht receiving nach available in den Zielslot und setzt den Receipt auf `putaway_complete`.
- Lot-/Serial-Pflicht wird beim Wareneingang anhand des Artikel-Tracking-Modus validiert; seriengefuehrte Artikel werden auf Menge 1 begrenzt.
- Browser-QA: CTOX Core Kit mit Beleg `PO-G3-QA-2` nach A2 angenommen und Einlagerung abgeschlossen.
- Browser-Regression: `REG-PXLTW-001` mit Beleg `REG-RECEIPT-PXLTW` angenommen und in A2 eingelagert.
- Teilannahme mit erwarteter Menge, OK-Menge und Sperr-/Schadmenge im linken Wareneingang.
- Nur die OK-Menge erzeugt eine Putaway-Aufgabe; Sperr-/Schadmenge wird als `quarantine` oder `damaged` am Wareneingang gebucht.
- Browser-QA: `REG-PARTIAL-DAMAGE` mit 5 erwartet, 3 OK, 2 Schaden gebucht; Putaway nur 3 nach A4, Postgres zeigt 2 `damaged` am Wareneingang.
- QS-Pruefung ist ein eigener linker Arbeitsblock: Ausnahmebestand kann freigegeben oder als Ausschuss ausgebucht werden.
- Browser-QA: 1 Einheit `damaged` aus `loc-receiving` nach `A1-QA` freigegeben; Rest per Ausschuss ausgebucht. Postgres zeigt `quality_review`/`quality_review_scrap` Bewegungen und keinen alten Schadbestand fuer `REG-PXLTW-001`.
- Persistenz bereinigt entfernte `stock_balances`, damit Zero-Balances nach Reload nicht wieder als alte QS-/Move-Zeilen erscheinen.
- Putaway kann per Scan gebucht werden: falscher Scan wird blockiert, gueltiger Artikel-/Slot-/Task-Scan schreibt `scan_events` und schliesst Putaway ab.
- Browser-QA: `REG-SCAN-PUTAWAY` erzeugt offene Putaway-Aufgabe; `WRONG-CODE` wird abgelehnt, `REG-PXLTW-001` bucht per Scan nach `A1-QA Copy`. Postgres zeigt `scan_events.action=putaway` und Receipt `putaway_complete`.

- Pick-Buchung per Scan ist umgesetzt: falsche Codes blockieren, gueltige Picklisten-/Artikel-/Slot-Scans schreiben `scan_events` und buchen reserved nach picked.
- Browser-QA: Pickliste `manual-reserve-20260507-4` per Scan `REG-PXLTW-001` gebucht; Postgres zeigt `scan_events.action=pick`, Pickliste `picked`, Reservierungszeile 1/1 gepickt.

## G4 Bestandsarbeit, Inventur und Audit

Stories: WH-013, WH-014, WH-015, WH-016, WH-020, WH-028, WH-039, WH-048

Status: umgesetzt und getestet fuer operative Bestandsarbeit, Audit und Replay-Kern.

Bereits vorhanden:
- Bestand per Drag and Drop zwischen Slots verschieben.
- Drop auf einen Slot bereitet eine Umlagerung im unteren Modul vor, statt sofort blind zu buchen.
- Teilmengen-Umlagerung mit Quelle, Zielslot und Menge im unteren Modul bestaetigen.
- Rechter Bestandspanel und unteres Slot-Bestandsmodul.
- Bewegungs-/Command-Log im Modell.
- Audit-Modus am Slot zeigt chronologische Bewegungen und Warehouse-Commands.
- Inventur am Slot starten, Zaehlmengen erfassen und speichern.
- Inventurdifferenzen beim Abschluss als Cycle-Count-Adjustments und Stock-Movements buchen.
- Rechtsklick auf Bestand oeffnet direkte Aktionen fuer Ausgangsreservierung, Umlagerungsmodus und Auditmodus.
- Browser-QA: Rechtsklick auf A2 oeffnet Inventurmodus; Rechtsklick auf A2-Bestand oeffnet Umlagerungsmodus und reserviert 1 Einheit in den Ausgang.
- Bestandsmodus hat jetzt eine zweite Arbeitsflaeche fuer Korrektur/Sperrbestand mit Grundcode.
- Bestand kann im Slot von `available` nach `quarantine`/`damaged` und zurueck nach `available` gebucht werden.
- Schnellkorrekturen setzen eine Balance auf eine neue Menge und schreiben Command plus Stock-Movement fuer Audit/Replay.
- Browser-QA: A2-Bestand in QS/Sperrbestand gebucht, wieder freigegeben und anschliessend mit Grund `count_fix` mengenmaessig korrigiert.
- Browser-Regression: 1 Einheit von A2 nach A3 umgelagert, Inventur auf A3 gestartet, Ist 1 gespeichert und Differenz gebucht.
- Replay-Faehigkeit ist im Warehouse-Kernel abgesichert; `movement replay recreates ledger balances` laeuft als automatisierter Kernel-Test.
- Der Audit-Modus zeigt Command- und Movement-Spuren als Grundlage fuer Fehleranalyse und kontrollierte Wiederholung.

## G5 Ausgang, Warenkorb und Fulfillment-Uebergabe

Stories: WH-031, WH-032, WH-033, WH-034, WH-035, WH-036, WH-037, WH-038

Status: umgesetzt und getestet; Fulfillment bleibt eigenes Modul mit Warehouse-Uebergabepunkten.

Bereits vorhanden:
- Rechter Warenkorb/Ausgangsrail.
- Reservierungen sichtbar.
- Pick/Ship Aktionen auf Reservierungen.
- Fulfillment-Modul separat erreichbar.
- Verfuegbarer Bestand kann aus der rechten Ausgangsrail direkt in einen Ausgangsvorgang reserviert werden.
- Die Reservierung ist owner-, lot-, serial- und slot-scharf und nutzt denselben Warehouse-Kernel wie Webshop/Stripe-Reservierungen.
- Reservierungen koennen explizit in Picklisten ueberfuehrt werden; die rechte Rail zeigt Pick-Aufgaben mit Status, Menge, Slot und Artikel.
- Browser-QA: A2-Bestand in `manual-2026-05-07` reserviert, danach gepickt und shipped bis Status `consumed`.
- Browser-Regression: A3-Bestand ueber `+ Ausgang` reserviert, gepickt und per `Ship` ausgeliefert.
- Browser-QA: A4-Bestand in `manual-reserve-20260507-4` reserviert und per `Liste` als Pick-Aufgabe `ready` erzeugt; Postgres `pick_lists` zeigt A4, `REG-PXLTW-001`, Menge 1.
- Pick-Scan bucht Picklisten revisionssicher und verhindert stille Falschpicks.
- Kommissionierwellen werden aus offenen Reservierungen geplant und persisted (`warehouse_wave_plans`).
- Umlagerung zwischen Lagern nutzt echte Transfer-Objekte mit `draft -> shipped -> received` und `in_transit` Bewegungen.
- Pack-/Versanduebergabe ist an Shipment-Package, Fulfillment-Label und Carrier-Tracking-Event angebunden.
- Retouren werden autorisiert und als `available` oder `damaged` wieder eingebucht.
- Browser-QA: `wave-20260507-2` geplant, `transfer-20260507-2` erstellt/versendet/empfangen, Paket/Label erzeugt, Carrier-Handover aufgezeichnet, Retoure autorisiert und empfangen.
- Postgres-QA: `warehouse_wave_plans=2`, `warehouse_transfers.transfer-20260507-2=received`, `shipment_packages=2`, `fulfillment_labels=2`, `shipment_tracking_events.carrier_handover=1`, Retourenstatus `received`.

## G6 Operative Steuerung

Stories: WH-022, WH-023, WH-043, WH-044, WH-045

Status: umgesetzt als operative Steuerungsrail.

Bereits vorhanden:
- Mindestbestandsrisiken werden aus Artikelbestand gegen Schwelle 5 angezeigt.
- Nachbestellhinweise erscheinen als Low-Stock-Arbeitskarten in der rechten Operationsrail.
- Schichtuebergaben werden aus offenen Putaway-, Pick- und QS-Risiken als `shift_handover_signed` Event gespeichert.
- Aufgabenpriorisierung nutzt offene Putaways, ready Picklisten und QS/Sperrbestand als Risikoqueue.
- SLA-/Deadline-Risiken werden als offene Pick-/QS-/Einlagerungsblocker sichtbar und per CTOX-Event auditierbar.
- Browser-QA: `Schicht` ausgefuehrt; Postgres zeigt `shift_handover_signed` mit offenem Arbeitskontext.

## G7 Import, Export, Sync und Go-live

Stories: WH-046, WH-047, WH-048, WH-049, WH-050

Status: umgesetzt fuer Import-Dry-Run, Export, Sync-Konflikte und Go-live-Pruefpfad.

Bereits vorhanden:
- Webshop-/Stripe-Checkout-Adapter fuer Reservierung/Versand.
- Integration-Events im Warehouse-Modell.
- Initialimport-Dry-Run wird als deduplizierter `offline_sync_batches`-Datensatz mit validierten Startbestandszeilen gespeichert.
- Reporting-Export ist ueber `/api/business/warehouse?format=csv` reproduzierbar und enthaelt Standort, Artikel, Owner, Status und Menge.
- API-/Webshop-Sync-Konflikte werden als `stock_conflict_detected` Integration-Events persisted.
- Go-live-Pruefung stuetzt sich auf die Story-Map, Browser-QA, Postgres-QA und automatisierte Tests.
- Browser-QA: `Import pruefen`, `Sync pruefen`, `Rechte`, `Schicht` und `3PL` ausgefuehrt.
- Postgres-QA: `offline_sync_batches.import-dry-run-20260507-2=accepted`, `warehouse_integration_events.stock_conflict_detected`, `role_gate_reviewed`, `shift_handover_signed`, `three_pl_charges` vorhanden.
- Automatisierte Abnahme: `@ctox-business/web typecheck`, `@ctox-business/web test:ui-contract`, `@ctox-business/warehouse test` gruen.
