# CTOX Business Basic Warehouse User Stories

Diese Stories beschreiben das Warehouse-Modul als produktive Anwendung fuer ein virtuelles Lager. Jede Story ist bewusst doppelt formuliert: einmal fuer die optimale manuelle UI und einmal fuer die KI-gestuetzte Bedienung ueber CTOX Chat oder Rechtsklick.

Die Auftragsabwicklung ist ein eigenes Modul. Die Warehouse-Stories beruehren Auftraege nur dort, wo Lagerverantwortung entsteht: Verfuegbarkeit, Reservierung, Pick, Pack, Versanduebergabe, Retoure, Bestandssync und Audit.

## WH-001 Lager anlegen

**Situation/Ziel**  
Als Warehouse-Manager richte ich ein neues physisches Lager in CTOX ein, bevor dort Bestaende, Zonen und Arbeitsprozesse abgebildet werden. Ich moechte das Lager so anlegen, dass Kolleginnen und Kollegen es sofort eindeutig erkennen und spaeter keine Verwechslung mit anderen Standorten entsteht.

**Manuell optimale UI**  
Die UI fuehrt mich durch eine klare Eingabemaske mit Lagername, Standort, Kurzcode, Adresse, Zeitzone, aktivem Status und optionaler Beschreibung. Bereits beim Anlegen sehe ich, wie das Lager in Lagerauswahl, Bestandsansichten und Strukturuebersicht erscheint.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Ich kann im CTOX Chat schreiben: "Lege ein neues Lager fuer unseren Standort Hamburg an." CTOX fragt fehlende Angaben gezielt nach und kann per Rechtsklick auf einen Standort vorhandene Stammdaten als Vorschlag uebernehmen.

**Erfuellt wenn**  
Das Lager ist eindeutig benannt, aktiv, im Warehouse-Modul sichtbar und als Ausgangspunkt fuer Bereiche, Zonen, Slots, Bestaende und Rechte nutzbar.

## WH-002 Lager bearbeiten, umbenennen oder deaktivieren

**Situation/Ziel**  
Als Operations Lead muss ich ein bestehendes Lager korrigieren, weil sich der Standortname geaendert hat oder ein Lager nicht mehr operativ genutzt wird. Ich moechte die Aenderung sauber durchfuehren, ohne historische Bestaende oder Bewegungen unverstaendlich zu machen.

**Manuell optimale UI**  
Die UI zeigt Stammdaten, Status, offene Bestaende, aktive Aufgaben und Auswirkungen der Aenderung, bevor ich speichere. Beim Deaktivieren sehe ich klar, ob noch offene Bewegungen, Reservierungen oder verknuepfte Zonen existieren.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Ich kann CTOX fragen: "Benenne Lager BER-ALT in Berlin Nord um und pruefe Auswirkungen." Per Rechtsklick auf ein Lager kann ich "Umbenennen", "Details bearbeiten" oder "Deaktivieren" starten, wobei CTOX betroffene Referenzen zusammenfasst.

**Erfuellt wenn**  
Das Lager ist korrekt aktualisiert, deaktivierte Lager werden nicht mehr fuer neue operative Vorgaenge angeboten, bleiben aber in Historie, Audit und Auswertungen nachvollziehbar.

## WH-003 Bereiche und Zonen anlegen

**Situation/Ziel**  
Als Lagerplaner bilde ich innerhalb eines Lagers reale Arbeitsbereiche ab, zum Beispiel Wareneingang, QS, Hochregal, Kleinteile, Sperrbestand und Versand. Die digitale Struktur soll den Wegen und Verantwortlichkeiten im physischen Lager entsprechen.

**Manuell optimale UI**  
Die UI zeigt das Lager als strukturierbare Hierarchie. Ich kann Bereiche und Zonen mit Name, Code, Zweck, Reihenfolge, Status und optionalen Regeln anlegen und sehe sofort, wo sie im Lagerkontext eingeordnet sind.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Ich kann CTOX schreiben: "Erstelle im Lager Hamburg die Zonen Wareneingang, QS, Hochregal und Versand." CTOX schlaegt sinnvolle Kuerzel und eine klare Struktur vor; per Rechtsklick auf ein Lager kann ich "Zone hinzufuegen" oder "Bereich strukturieren" ausloesen.

**Erfuellt wenn**  
Bereiche und Zonen sind im richtigen Lager sichtbar, eindeutig unterscheidbar und als Grundlage fuer Slots, Bestaende, Aufgaben, Filter und Berechtigungen verwendbar.

## WH-004 Slots und Faecher definieren

**Situation/Ziel**  
Als Lagerverantwortlicher erfasse ich konkrete Lagerplaetze, damit Ware einem tatsaechlich auffindbaren Fach, Regalplatz oder Stellplatz zugeordnet werden kann. Typische Lagerplatzmuster sollen schnell und fehlerarm abbildbar sein.

**Manuell optimale UI**  
Die UI erlaubt, einzelne Slots anzulegen oder Serien wie "Regal A, Ebene 01-05, Fach 01-20" zu erzeugen. Vor dem Speichern sehe ich eine Vorschau der entstehenden Slot-Codes und Konflikte bei Namens- oder Kapazitaetsregeln.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Ich kann CTOX sagen: "Lege in Zone Kleinteile 10 Regale mit je 5 Ebenen und 20 Faechern an." CTOX erstellt eine pruefbare Slot-Struktur mit konsistenter Benennung und fragt nach, wenn Kapazitaeten oder Regeln fehlen.

**Erfuellt wenn**  
Slots sind eindeutig codiert, der richtigen Zone zugeordnet und fuer Einlagerung, Umlagerung, Suche, Inventur und Bestandsanzeige auswählbar.

## WH-005 Physische Lagerstruktur wiedererkennbar machen

**Situation/Ziel**  
Als Lagermitarbeiter oeffne ich CTOX waehrend der Arbeit und muss die digitale Ansicht sofort mit der realen Umgebung abgleichen koennen. Ich moechte nicht interpretieren muessen, ob ein Code ein Regal, Gang, Fach oder Sonderplatz ist.

**Manuell optimale UI**  
Die UI stellt Lager, Bereiche, Zonen und Slots mit klaren Codes, sprechenden Namen, Hierarchie, Status und optionalen Hinweisen wie Gang, Regal, Ebene oder Fach dar. Die Hauptansicht hat Wiedererkennungswert und ist nicht nur eine Liste.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Ich kann CTOX fragen: "Wo finde ich Slot HR-A-03-12 im echten Lager?" und bekomme eine verstaendliche Beschreibung anhand der gepflegten Struktur. Per Rechtsklick auf einen Slot kann ich "Lage erklaeren" oder "Benennung verbessern" waehlen.

**Erfuellt wenn**  
Nutzer koennen anhand der digitalen Struktur zuverlaessig erkennen, wo sich ein Lagerplatz physisch befindet, und die Darstellung bleibt auch bei vielen Zonen und Slots konsistent verstaendlich.

## WH-006 Slot per Rechtsklick im Lagerlayout bearbeiten

**Situation/Ziel**  
Ein Warehouse-Operator erkennt im virtuellen Lagerlayout, dass ein Slot falsch benannt oder einer falschen Zone zugeordnet ist. Er will den Slot direkt aus dem visuellen Lagerplan heraus korrigieren, ohne in eine separate Stammdatenmaske wechseln zu muessen.

**Manuell optimale UI**  
Per Rechtsklick auf den Slot oeffnet sich ein Kontextmenue mit klaren Aktionen wie "Slot bearbeiten", "Umbenennen", "Duplizieren", "Sperren" und "Entsperren". Beim Bearbeiten erscheint ein kompaktes Panel mit Slot-ID, Bezeichnung, Zone, Kapazitaet, Slot-Typ und Hinweisen.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann den Slot rechtsklicken und CTOX bitten: "Benenne diesen Slot in A-03-02 um und ordne ihn der Zone Wareneingang zu." CTOX erkennt den selektierten Slot, zeigt die geplante Aenderung an und fuehrt sie nach Bestaetigung aus.

**Erfuellt wenn**  
Ein Slot kann direkt im Lagerlayout bearbeitet werden. Die Aenderung ist validiert, auditierbar und sofort im virtuellen Warehouse sichtbar.

## WH-007 Slot in operativer Stoerung sperren oder entsperren

**Situation/Ziel**  
Ein Lagerleiter erfaehrt, dass ein Regalplatz wegen Beschaedigung oder Inventurdifferenz voruebergehend nicht genutzt werden darf. Der betroffene Slot muss schnell gesperrt werden, damit keine neuen Einlagerungen oder Umlagerungen darauf geplant werden.

**Manuell optimale UI**  
Der Nutzer klickt mit der rechten Maustaste auf den Slot und waehlt "Slot sperren". Ein Dialog fragt Grund, Zeitraum, Verantwortlichen und optionale Notiz ab; beim Entsperren ist sichtbar, warum der Slot gesperrt war und wer die Sperre gesetzt hat.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Nach Rechtsklick auf den Slot kann der Nutzer schreiben: "Diesen Slot wegen Inventur bis Freitag sperren." CTOX erstellt daraus eine Sperre mit Grund, Zeitraum und Status und fragt bei unklaren Angaben nach.

**Erfuellt wenn**  
Ein Slot kann gesperrt und entsperrt werden, ohne widerspruechliche Verfuegbarkeiten zu erzeugen. Grund, Zeitraum, Benutzer und Historie sind nachvollziehbar.

## WH-008 Lagerstruktur fuer neuen Standort duplizieren

**Situation/Ziel**  
Ein Unternehmen eroeffnet ein zweites Lager, das strukturell fast identisch zum bestehenden Lager ist. Der Warehouse-Administrator will nicht jede Zone, Reihe, Regalebene und jeden Slot erneut manuell anlegen.

**Manuell optimale UI**  
In der Lagerstruktur gibt es "Struktur duplizieren". Der Nutzer waehlt Quelle, Zielname, Nummerierungspraefix, zu uebernehmende Ebenen und Ausnahmen aus; eine Vorschau zeigt neue Zonen, Slots und Konflikte.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann CTOX formulieren: "Dupliziere Lager Nord als Lager Sued, aber mit Slot-Prefix S statt N und ohne Kuehlzone." CTOX bereitet die Zielstruktur vor und weist auf Konflikte hin.

**Erfuellt wenn**  
Eine bestehende Lagerstruktur kann als belastbare Vorlage genutzt werden. IDs, Namen, Zonen und Ausnahmen sind vor Erstellung pruefbar und nach Erstellung konsistent.

## WH-009 Artikel fuer Einlagerung neu anlegen

**Situation/Ziel**  
Im Wareneingang trifft ein neuer Artikel ein, der noch nicht im virtuellen Warehouse existiert. Der Mitarbeiter muss ihn schnell erfassen, damit Bestand, Einlagerung und spaetere Kommissionierung korrekt funktionieren.

**Manuell optimale UI**  
Die UI bietet "Artikel anlegen" mit Pflichtfeldern wie Artikelnummer, Name, Einheit, Abmessungen, Gewicht, Lagerbedingungen und Status. Validierungen verhindern Dubletten und unvollstaendige Stammdaten.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann schreiben: "Lege Artikel 4711 als 500ml Glasflasche an, Normalzone, 24 Stueck pro Karton." CTOX extrahiert Stammdaten, fragt fehlende Pflichtangaben ab und zeigt vor dem Speichern eine Zusammenfassung.

**Erfuellt wenn**  
Ein neuer Artikel steht nach Anlage sofort fuer Einlagerung, Bestand und Slot-Zuordnung zur Verfuegung. Dubletten und fehlende Pflichtdaten werden verhindert.

## WH-010 Bestehenden Artikel bearbeiten, duplizieren oder deaktivieren

**Situation/Ziel**  
Ein Produktmanager aktualisiert Artikeldaten, weil sich Verpackung, Gewicht oder Lagerbedingungen geaendert haben. In anderen Faellen braucht er eine neue Artikelvariante oder muss einen auslaufenden Artikel fuer neue Lagerbewegungen deaktivieren.

**Manuell optimale UI**  
Auf der Artikeldetailseite sind "Bearbeiten", "Duplizieren" und "Deaktivieren" klar getrennt. Kritische Aenderungen zeigen Auswirkungen auf Bestand, offene Auftraege, Lagerregeln und Historie.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann einen Artikel rechtsklicken und CTOX anweisen: "Dupliziere diesen Artikel als neue 250ml Variante" oder "Deaktiviere diesen Artikel nach Abverkauf." CTOX prueft Abhaengigkeiten und schlaegt eine sichere Aktion vor.

**Erfuellt wenn**  
Artikel koennen kontrolliert geaendert, dupliziert oder deaktiviert werden. Bestehende Bestaende und Historien bleiben erhalten, neue Bewegungen arbeiten nur mit gueltigen Artikeldaten.

## WH-011 Wareneingang pruefen und vereinnahmen

**Situation/Ziel**  
Ein Lagermitarbeiter steht am Wareneingang vor einer angelieferten Palette und muss klaeren, ob die Lieferung zur erwarteten Bestellung passt. Artikel, Mengen, Chargen und sichtbare Abweichungen muessen erfasst werden, bevor Ware ins Lager wandert.

**Manuell optimale UI**  
Die UI zeigt erwartete Wareneingaenge nach Lieferant, Bestellung, Avis oder Lieferschein. Der Mitarbeiter kann Positionen abhaken, Mengen korrigieren, beschaedigte Ware markieren, Fotos ergaenzen und den Eingang teilweise oder vollstaendig bestaetigen.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Mitarbeiter kann fragen: "Was ist bei dieser Lieferung noch offen?" oder "Vergleiche erfasste Mengen mit der Bestellung." Per Rechtsklick auf eine Position kann CTOX Abweichungen zusammenfassen oder eine Klaerungsnotiz vorbereiten.

**Erfuellt wenn**  
Der Wareneingang ist mit tatsaechlicher Menge, Zustand und Abweichungen dokumentiert. Teilannahmen, Ueberlieferungen, Unterlieferungen und beschaedigte Ware sind eindeutig markiert.

## WH-012 Bestand auf einen Slot buchen

**Situation/Ziel**  
Nach dem Wareneingang bringt ein Mitarbeiter Ware an einen freien Lagerplatz und muss den Bestand dort buchen. Das System soll sofort wissen, welcher Artikel in welcher Menge auf welchem Slot liegt.

**Manuell optimale UI**  
Die UI bietet Slot-Suche oder Scan-Eingabe, zeigt freie und belegte Plaetze, prueft Lagerregeln und zeigt, ob Artikel, Charge, MHD oder Gefahrgutklasse zum Slot passen. Der Mitarbeiter bestaetigt Artikel, Menge und Zielslot in einem kompakten Buchungsdialog.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Mitarbeiter kann fragen: "Welcher Slot eignet sich fuer diese Palette?" oder "Warum darf ich hier nicht einlagern?" CTOX kann passende Lagerplaetze vorschlagen und die Empfehlung begruenden.

**Erfuellt wenn**  
Der Bestand ist eindeutig einem Slot zugeordnet. Menge, Artikel, Charge und optionale Haltbarkeitsdaten sind korrekt uebernommen; unzulaessige Slot-Buchungen werden verhindert oder zur Freigabe markiert.

## WH-013 Bestand per Drag and Drop verschieben

**Situation/Ziel**  
Ein Lagerkoordinator sieht im virtuellen Warehouse, dass Ware auf einem unguenstigen Slot liegt, und moechte sie auf einen besseren Platz verschieben. Die visuelle Umlagerung soll der realen Bewegung im Lager entsprechen.

**Manuell optimale UI**  
Die UI zeigt Lagerbereiche, Slots und Bestaende als interaktive Lageransicht. Der Nutzer zieht eine Bestandseinheit von einem Slot auf einen anderen; vor Abschluss zeigt die UI Quelle, Ziel, Artikel, Menge und moegliche Regelverletzungen.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann fragen: "Schlage bessere Slots fuer diesen Bestand vor." Per Rechtsklick auf einen Bestand kann CTOX Umlagerungsoptionen anzeigen, etwa naeher an Kommissionierung, sortenrein oder temperaturgeeignet.

**Erfuellt wenn**  
Der Bestand wird nach Bestaetigung vom Quellslot auf den Zielslot umgebucht. Die Historie zeigt, wer wann welche Umlagerung ausgeloest hat, und ungueltige Zielslots werden vor der Buchung erkannt.

## WH-014 Teilmengen zwischen Slots verschieben

**Situation/Ziel**  
Ein Mitarbeiter benoetigt nur einen Teilbestand auf einem anderen Slot, etwa fuer Nachschub, Kommissionierung oder Konsolidierung. Nicht die gesamte Bestandseinheit, sondern eine genaue Teilmenge soll sauber umgebucht werden.

**Manuell optimale UI**  
Beim Ziehen oder Auswaehlen eines Bestands fragt die UI, ob die gesamte Menge oder eine Teilmenge verschoben werden soll. Der Nutzer gibt Menge, Einheit und Zielslot ein; die UI zeigt Restbestand und neuen Zielbestand vor der Bestaetigung.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Mitarbeiter kann sagen: "Verschiebe 12 Stueck von Slot A-03 nach B-11" oder fragen: "Wie viel bleibt danach auf A-03?" CTOX kann typische Teilmengen wie Karton, Lage, Palette oder Nachschubmenge vorschlagen.

**Erfuellt wenn**  
Nur die gewaehlte Teilmenge wird umgebucht. Restbestand, Einheiten, Rundungen und Mindestmengen bleiben korrekt und die Buchung ist als Teilumlagerung nachvollziehbar.

## WH-015 Inventur und Zaehlen am Slot

**Situation/Ziel**  
Ein Mitarbeiter steht waehrend einer Inventur oder Stichprobenzaehlung an einem Lagerplatz und muss den physischen Bestand mit dem Systembestand abgleichen. Ziel ist, Zaehlwerte schnell zu erfassen und Differenzen sauber zu klaeren.

**Manuell optimale UI**  
Die UI zeigt den Slot mit erwartetem Bestand, Artikelinformationen, Charge und Einheit. Der Mitarbeiter traegt gezaehlte Mengen ein, markiert leere Slots, unbekannte Ware oder beschaedigte Artikel und kann Differenzen begruenden.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Mitarbeiter kann fragen: "Welche Slots muss ich heute zaehlen?" oder "Fasse die Differenzen dieses Bereichs zusammen." Per Rechtsklick auf einen Slot kann CTOX fruehere Bewegungen und moegliche Ursachen anzeigen.

**Erfuellt wenn**  
Der gezaehlte Bestand ist je Slot erfasst. Abweichungen zwischen Soll und Ist sind sichtbar, begruendet und zur Freigabe oder Korrekturbuchung vorbereitet.

## WH-016 Inventurdifferenz buchen

**Situation/Ziel**  
Ein Warehouse Operator stellt bei der Stichinventur fest, dass auf Slot A-03-12 physisch 47 Stueck einer SKU liegen, im System aber 50 Stueck gefuehrt werden. Die Differenz soll nachvollziehbar und mit Begruendung gebucht werden.

**Manuell optimale UI**  
Der Operator oeffnet Slot oder SKU, sieht Sollbestand, Istbestand, Owner, Charge oder Seriennummern und gibt den gezaehlten Wert ein. Die UI berechnet die Differenz, verlangt Grundcode und Notiz und zeigt vor dem Buchen die konkrete Bestandsaenderung.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Per Rechtsklick auf Slot oder SKU kann der Operator "Inventurdifferenz buchen" waehlen. CTOX fasst aktuellen Systembestand, gezaehlten Wert, Differenz, Owner, letzte Bewegungen und moegliche Ursachen zusammen.

**Erfuellt wenn**  
Die Differenzbuchung ist nur mit eindeutigem Slot, SKU, Owner, Menge, Grund und Benutzer moeglich. Bestand und Bewegungsverlauf werden korrekt aktualisiert und Freigaberegeln eingehalten.

## WH-017 Slot-Zustaende visuell erkennen

**Situation/Ziel**  
Ein Schichtleiter muss vor einer Kommissionierwelle erkennen, welche Lagerplaetze blockiert, leer, ueberfuellt, reserviert, inventurgesperrt oder nutzbar sind. Er braucht eine Lageuebersicht, ohne einzelne Slots manuell zu oeffnen.

**Manuell optimale UI**  
Die Warehouse-Ansicht zeigt Gaenge, Regale und Slots mit unterscheidbaren Statusfarben, Icons und Tooltips. Filter grenzen nach Status, Zone, Owner, SKU oder Prozesszustand ein; ein Klick zeigt Bestand, Reservierungen, Sperrgrund und naechste Aktion.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann fragen: "Welche Slots in Zone B verhindern gerade die Kommissionierung?" oder per Rechtsklick auf eine Zone "Probleme analysieren" waehlen. CTOX gruppiert Auffaelligkeiten nach Ursache und schlaegt eine operative Reihenfolge vor.

**Erfuellt wenn**  
Ein Schichtleiter erkennt in Sekunden die relevanten Slot-Zustaende. Jeder visuelle Status ist erklaerbar, filterbar und mit konkreten Bestands- oder Prozessdaten verknuepft.

## WH-018 Suche nach SKU, Slot oder Owner

**Situation/Ziel**  
Ein Mitarbeiter im Warenausgang sucht dringend eine SKU, kennt aber nur Teile der Artikelnummer und den Owner. Er muss herausfinden, wo Bestand liegt, ob er verfuegbar ist und welcher Slot fuer die naechste Aktion relevant ist.

**Manuell optimale UI**  
Eine zentrale Suche akzeptiert SKU, Slot, Owner, Barcode, Artikelname oder Teilbegriffe. Ergebnisse sind nach Artikel, Slots, Owner-Bestaenden und Bewegungen gruppiert und zeigen verfuegbare Menge, reservierte Menge, Slot-Zustand, Owner und letzte Aktivitaet.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann schreiben: "Finde alle verfuegbaren Bestaende von SKU 88421 fuer Owner Nordhandel." CTOX interpretiert unvollstaendige Angaben, fragt bei Mehrdeutigkeit nach und liefert priorisierte Slots mit Mengen und Einschraenkungen.

**Erfuellt wenn**  
Die Suche findet relevante Treffer bei Teilangaben und gemischten Suchbegriffen. Ergebnisse unterscheiden klar zwischen physischem Bestand, verfuegbarem Bestand und reservierter Menge.

## WH-019 Owner-getrennter Bestand

**Situation/Ziel**  
Ein 3PL-Lager verwaltet dieselbe SKU fuer mehrere Owner. Ein Operator darf beim Umlagern oder Kommissionieren nicht versehentlich Bestand eines falschen Owners verwenden, obwohl Artikelnummer und Verpackung identisch sind.

**Manuell optimale UI**  
Bestandsansichten zeigen Owner als primaeres Unterscheidungsmerkmal neben SKU, Slot und Menge. Bei gemischten Slots werden Owner getrennt gelistet; Aktionen wie Pick, Move, Adjust oder Reserve verlangen eine eindeutige Owner-Auswahl.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann fragen: "Zeige verfuegbaren Bestand von SKU 1200 getrennt nach Owner" oder "Kann ich fuer Owner Alpha aus Slot C-11 picken?" CTOX prueft Owner, Verfuegbarkeit, Reservierungen und Regeln.

**Erfuellt wenn**  
Bestand unterschiedlicher Owner wird systemweit nie stillschweigend vermischt. Jede Anzeige, Suche, Buchung und Bewegung bleibt owner-scharf und verhindert falsche Entnahmen.

## WH-020 Bewegungsverlauf und Audit

**Situation/Ziel**  
Nach einer Kundenreklamation muss ein Warehouse Manager nachvollziehen, warum eine Lieferung mit zu geringer Menge versendet wurde. Er braucht eine lueckenlose Historie aller Bewegungen, Korrekturen und Reservierungen.

**Manuell optimale UI**  
Der Audit-Verlauf zeigt eine chronologische Timeline mit Wareneingang, Umlagerung, Reservierung, Pick, Pack, Versand, Inventurkorrektur und manuellen Aenderungen. Jeder Eintrag enthaelt Zeitpunkt, Benutzer, Quelle, Ziel, Menge, Owner, Referenzbeleg und Grund.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann fragen: "Warum fehlen 3 Stueck bei Auftrag SO-4921?" CTOX analysiert den Bewegungsverlauf, erkennt relevante Ereignisse und fasst die wahrscheinlichste Ursache zusammen.

**Erfuellt wenn**  
Jede Bestandsveraenderung ist vollstaendig nachvollziehbar und nicht unbemerkt veraenderbar. Der Verlauf verbindet operative Aktionen mit Belegen und Benutzern.

## WH-021 Charge oder Seriennummer bei Wareneingang erfassen

**Situation/Ziel**  
Ein Mitarbeiter bucht Ware ein, bei der einzelne Artikel chargen- oder seriennummernpflichtig sind. Jede Einheit muss eindeutig rueckverfolgbar sein, bevor sie eingelagert oder kommissioniert werden kann.

**Manuell optimale UI**  
Im Wareneingang erkennt die Maske automatisch, ob Charge, Seriennummer oder beides erforderlich ist. Der Mitarbeiter scannt oder erfasst Werte pro Position; fehlende, doppelte oder formal ungueltige Nummern werden sofort markiert.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Per Rechtsklick auf eine Wareneingangsposition kann der Mitarbeiter CTOX fragen: "Pruefe Seriennummern" oder "Welche Chargen fehlen noch?" CTOX erkennt Luecken, Dubletten und Abweichungen zur Bestellung oder zum Lieferavis.

**Erfuellt wenn**  
Chargen- oder seriennummernpflichtige Ware kann nicht ohne vollstaendige, eindeutige und pruefbare Nummern eingebucht werden.

## WH-022 Mindestbestand erkennen und bewerten

**Situation/Ziel**  
Eine Lagerleitung prueft waehrend des Tages, ob kritische Artikel unter Mindestbestand fallen oder bald darunter rutschen. Sie moechte verstehen, ob sofort gehandelt werden muss.

**Manuell optimale UI**  
Die Bestandsuebersicht zeigt pro Artikel aktuellen Bestand, reservierte Menge, frei verfuegbaren Bestand, Mindestbestand und erwartete Zu- oder Abgaenge. Kritische Artikel sind nach Dringlichkeit sortiert.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Per Rechtsklick auf einen Artikel kann die Lagerleitung fragen: "Warum ist dieser Artikel kritisch?" CTOX erklaert die Ursache anhand von Reservierungen, offenen Auftraegen, Lieferterminen und Verbrauchsmustern.

**Erfuellt wenn**  
Kritische Mindestbestaende sind ohne manuelle Auswertung erkennbar, und die Lagerleitung kann Ursache, ausloesende Bewegung und noetige Massnahme nachvollziehen.

## WH-023 Nachbestellhinweis aus Lagerlage ableiten

**Situation/Ziel**  
Ein Disponent erhaelt Hinweise, dass bestimmte Artikel nachbestellt werden sollten. Er muss entscheiden, ob echter Bestellbedarf besteht oder erwartete Eingaenge, Umlagerungen oder gesperrte Bestaende die Lage entschaerfen.

**Manuell optimale UI**  
Der Nachbestellhinweis zeigt verfuegbaren Bestand, Mindestbestand, offene Kundenauftraege, erwartete Lieferungen, durchschnittlichen Verbrauch und empfohlene Bestellmenge. Der Disponent kann bestaetigen, zurueckstellen oder als nicht relevant markieren.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Per Rechtsklick kann der Disponent CTOX fragen: "Begruende die empfohlene Menge" oder "Was passiert, wenn ich nicht bestelle?" CTOX fasst Risiko, Zeithorizont und betroffene Auftraege zusammen.

**Erfuellt wenn**  
Ein Nachbestellhinweis ist nachvollziehbar, priorisiert und handlungsfaehig. Der Disponent kann ohne Zusatzlisten entscheiden, ob bestellt, gewartet oder eskaliert wird.

## WH-024 Slot-Kapazitaet vor Einlagerung pruefen

**Situation/Ziel**  
Ein Staplerfahrer soll Ware in einen vorgeschlagenen Lagerplatz einlagern. Vor Ort muss klar sein, ob der Slot Volumen-, Gewichts- und Belegungsgrenzen einhaelt.

**Manuell optimale UI**  
Der Einlagerungsdialog zeigt aktuelle Belegung, freie Kapazitaet, maximales Gewicht, zulaessige Abmessungen und Artikelrestriktionen. Wenn der Slot ungeeignet ist, bietet die UI alternative Slots mit Begruendung an.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Mitarbeiter kann fragen: "Passt diese Palette hier hinein?" oder "Finde den besten freien Slot in der Naehe." CTOX bewertet Kapazitaet, Wege, Zugriffshaeufigkeit und Lagerregeln.

**Erfuellt wenn**  
Ware wird nur auf Slots gebucht, deren Kapazitaet und Regeln zur Ware passen. Ungeeignete Plaetze werden vor der physischen Einlagerung erkannt.

## WH-025 Sonderlagerbedingungen einhalten

**Situation/Ziel**  
Ein Mitarbeiter lagert Ware ein, die Gefahrgut-, Temperatur- oder andere Sonderbedingungen hat. Sie darf nur an Plaetzen landen, die diese Bedingungen erfuellen und keine unvertraeglichen Waren enthalten.

**Manuell optimale UI**  
Die Einlagerungsmaske zeigt Gefahrklasse, Temperaturbereich, Feuchteanforderung, Sperrzone, Separationsregel und sonstige Einschraenkungen. Geeignete Lagerplaetze sind gekennzeichnet; ungeeignete Slots sind gesperrt oder begruendet.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Per Rechtsklick auf Artikel, Charge oder Slot kann der Mitarbeiter fragen: "Darf diese Ware hier gelagert werden?" CTOX prueft Artikelstammdaten, Chargenhinweise, Slot-Eigenschaften und Unvertraeglichkeiten.

**Erfuellt wenn**  
Sonderlagerware kann nur auf regelkonformen Plaetzen eingelagert werden. Sonderbedingungen sind im Arbeitsmoment sichtbar und verhindern Fehlbuchungen.

## WH-026 Artikel und Slot per Barcode oder QR erfassen

**Situation/Ziel**  
Ein Lagermitarbeiter steht vor einem Regalplatz und muss sicherstellen, dass der physische Artikel zum erwarteten Slot passt, bevor er einlagert, kommissioniert oder korrigiert.

**Manuell optimale UI**  
Die mobile Ansicht zeigt einen Scan-Flow: zuerst Slot scannen, dann Artikel scannen. Nach jedem Scan wird angezeigt, welcher Slot oder Artikel erkannt wurde, inklusive Name, SKU, Menge, Lager und Status.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann schreiben: "Ich habe Slot A-03-02 und Artikel 871234 gescannt, passt das?" CTOX erklaert die Zuordnung, weist auf Konflikte hin und schlaegt die wahrscheinlich richtige Aktion vor.

**Erfuellt wenn**  
Ein Mitarbeiter kann mit zwei Scans erkennen, ob Artikel und Slot zusammenpassen. Fehlerhafte Zuordnungen werden vor der Bestandsaenderung sichtbar.

## WH-027 Mobile Nutzung im laufenden Lagerbetrieb

**Situation/Ziel**  
Ein Mitarbeiter bewegt sich mit Smartphone oder Handscanner durch das Lager und muss ohne Desktop, Papierliste oder Nachfragen Bestaende pruefen, Artikel finden und Aufgaben abschliessen koennen.

**Manuell optimale UI**  
Die mobile Oberflaeche ist auf kurze Interaktionen optimiert: grosse Scan-Schaltflaeche, lesbare Artikel- und Slotinformationen, klare Statusfarben, Verbindungsstatus und minimale Texteingabe. Nach einem Scan erscheinen die naechsten sinnvollen Aktionen.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann unterwegs fragen: "Wo finde ich SKU WH-883 im Store Berlin?" oder "Was ist meine naechste offene Lageraufgabe?" CTOX gibt Lager, Gang, Slot, verfuegbare Menge und naechsten Schritt aus.

**Erfuellt wenn**  
Ein Lagerprozess kann vollstaendig mobil durchgefuehrt werden, ohne Desktop-Wechsel. Die UI bleibt auch bei schlechter Verbindung verstaendlich.

## WH-028 Schnellkorrektur bei falscher Menge oder falschem Slot

**Situation/Ziel**  
Beim Zaehlen oder Picken stellt ein Mitarbeiter fest, dass reale Menge oder realer Lagerplatz nicht mit CTOX uebereinstimmen. Die Abweichung muss schnell korrigiert werden, ohne den laufenden Prozess zu unterbrechen.

**Manuell optimale UI**  
Auf Artikel-, Slot- und Scan-Ansichten gibt es eine Schnellkorrektur: Menge aendern, Slot aendern, Artikel fehlt, Artikel unerwartet gefunden. Die UI zeigt vor dem Speichern alter Bestand, neuer Bestand, Slot, Standort und Korrekturgrund.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann schreiben: "In Slot B-12 liegen nur 7 statt 10 Stueck von SKU 4412." CTOX erstellt eine pruefbare Korrektur, fragt Pflichtangaben nach und warnt bei Konflikten mit Picks, Transfers oder Inventur.

**Erfuellt wenn**  
Eine reale Bestandsabweichung kann schnell, begruendet und auditiert erfasst werden. Kritische Korrekturen werden bestaetigt und nicht still gebucht.

## WH-029 Mehrere Lager und Stores unterscheiden

**Situation/Ziel**  
Ein Operations-Mitarbeiter betreut mehrere Lager und Stores und muss verhindern, dass Bestand versehentlich im falschen Standort geprueft, reserviert, korrigiert oder umgelagert wird.

**Manuell optimale UI**  
Jede Warehouse-Ansicht zeigt den aktiven Standort dauerhaft sichtbar. Standortwechsel sind bewusst gestaltet; Suchergebnisse, Artikelbestaende und Scan-Ergebnisse sind nach Lager und Store getrennt.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann fragen: "Wie viel Bestand von SKU 9004 haben wir in Hamburg und Muenchen?" CTOX antwortet standortgetrennt und weist darauf hin, wenn der Nutzer gerade in einem anderen aktiven Lager arbeitet.

**Erfuellt wenn**  
Bestaende gleicher Artikel werden pro Lager und Store eindeutig getrennt angezeigt und bearbeitet. Standortuebergreifende Aktionen benoetigen klare Quelle und klares Ziel.

## WH-030 Umlagerung zwischen Lagern durchfuehren

**Situation/Ziel**  
Ein Warehouse-Koordinator muss Artikel von einem Lager in ein anderes oder vom Zentrallager in einen Store umlagern. Der Bestand soll waehrend des Transports sauber abgebildet bleiben.

**Manuell optimale UI**  
Die Umlagerungsmaske fuehrt durch Quelle, Ziel, Artikel, Menge, Versandstatus und Empfangsbestaetigung. Der Transfer hat Zustaende wie geplant, gepickt, unterwegs, teilweise empfangen und abgeschlossen.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann schreiben: "Lagere 20 Stueck SKU 7721 von Lager Hamburg nach Store Berlin um." CTOX prueft Bestand, Reservierungen und Zielbedarf und formuliert einen bestaetigbaren Transferauftrag.

**Erfuellt wenn**  
Eine Umlagerung reduziert verfuegbaren Bestand am Quelllager nachvollziehbar und erhoeht Bestand am Ziel erst bei Empfang oder definierter Buchungsregel. Teilmengen und Abweichungen sind dokumentierbar.

## WH-031 Pick-Aufgaben aus freigegebenen Auftraegen erzeugen

**Situation/Ziel**  
Als Lagerdisponent moechte ich aus freigegebenen Kunden- oder Produktionsauftraegen konkrete Pick-Aufgaben erzeugen. Lagerkraefte sollen eindeutig sehen, was aus welchem Lagerplatz in welcher Menge zu entnehmen ist.

**Manuell optimale UI**  
Die UI zeigt freigegebene Auftraege mit Artikel, Menge, Prioritaet, Liefertermin, Lagerbestand, Reservierung und empfohlenem Entnahmeplatz. Der Nutzer kann Pick-Aufgaben erzeugen, Mengen splitten, Prioritaeten setzen und Aufgaben Zone, Person oder Pickliste zuweisen.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Per Rechtsklick auf einen Auftrag kann der Nutzer "Pick-Aufgaben vorschlagen" waehlen. CTOX erklaert, welche Positionen pickbar oder blockiert sind, und schlaegt eine Aufteilung nach Zone, Termin und Laufweg vor.

**Erfuellt wenn**  
Aus einem freigegebenen Auftrag entstehen eindeutige Pick-Aufgaben mit Artikel, Menge, Lagerplatz, Ziel, Prioritaet und Auftragsbezug. Nicht pickbare Positionen werden begruendet.

## WH-032 Kommissionierwelle planen

**Situation/Ziel**  
Als Schichtleiter moechte ich mehrere Auftraege zu einer Kommissionierwelle buendeln, damit aehnliche Artikel, gleiche Zonen oder gleiche Versandzeitfenster effizient zusammen gepickt werden.

**Manuell optimale UI**  
Die UI zeigt Auftraege mit Versandtermin, Kunde, Zone, Artikelstruktur, Volumen, Gewicht und Pickstatus. Der Nutzer kann Auftraege per Drag and Drop in eine Welle ziehen, Kapazitaet pruefen und die Welle fuer Picker oder Lagerbereiche freigeben.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann fragen: "Erstelle eine Welle fuer alle Express-Auftraege bis 14:00 mit kurzen Laufwegen." CTOX schlaegt enthaltene Auftraege, erwartete Pickdauer, Engpaesse und Ausschluesse vor.

**Erfuellt wenn**  
Eine Welle enthaelt nur pickbare oder bewusst teilpickbare Auftraege, hat Status, Prioritaet, Pick-Aufgaben, Verantwortliche und eine nachvollziehbare Sortierung.

## WH-033 Bestand fuer Auftraege reservieren

**Situation/Ziel**  
Als Auftragsverantwortlicher moechte ich verfuegbaren Bestand fuer bestimmte Auftraege reservieren, damit zugesagte Ware nicht durch spaetere Auftraege oder interne Bewegungen verbraucht wird.

**Manuell optimale UI**  
Die UI zeigt je Artikel verfuegbaren Bestand, reservierten Bestand, offenen Bedarf, Lagerplatz, Charge, Seriennummer, Mindesthaltbarkeit und Sperrstatus. Reservierungen koennen gesetzt, geaendert, geloest oder priorisiert werden.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Per Rechtsklick auf eine Auftragsposition kann "Bestand optimal reservieren" ausgefuehrt werden. CTOX schlaegt konkrete Bestaende nach FEFO, Charge, Kundenvorgaben, Lagerzone und Liefertermin vor.

**Erfuellt wenn**  
Reservierter Bestand ist eindeutig Auftrag oder Auftragsposition zugeordnet. Andere Prozesse sehen die Reservierung und koennen den Bestand nicht unbeabsichtigt verwenden.

## WH-034 Fehlbestand beim Pick erfassen und loesen

**Situation/Ziel**  
Als Picker moechte ich waehrend der Entnahme einen Fehlbestand direkt melden, damit der Auftrag nicht stillschweigend falsch kommissioniert wird und der Innendienst eine belastbare Entscheidung treffen kann.

**Manuell optimale UI**  
Die Pick-Oberflaeche zeigt Sollmenge, gescannte Menge, Lagerplatz, Artikelbild, Charge und offene Restmenge. Bei Fehlbestand erfasst der Nutzer Istmenge, Foto oder Notiz und markiert den Pick als teilgepickt, blockiert oder zur Klaerung.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Picker kann sagen: "Am Platz A-03-02 fehlen 6 Stueck von Artikel X." CTOX prueft alternative Lagerplaetze, offene Wareneingaenge, andere Reservierungen und passende Ersatzartikel.

**Erfuellt wenn**  
Ein Fehlbestand reduziert nicht automatisch den Auftrag ohne Entscheidung. Vorfall, Auftrag, Reservierungen und Pick-Aufgaben werden aktualisiert oder in Klaerstatus gesetzt.

## WH-035 Ersatz- oder Alternativartikel verwenden

**Situation/Ziel**  
Als Auftragsbearbeiter moechte ich bei nicht verfuegbarem Artikel einen zulaessigen Ersatzartikel waehlen, damit der Auftrag weiter bearbeitet werden kann, ohne Kunden-, Qualitaets- oder Vertragsregeln zu verletzen.

**Manuell optimale UI**  
Die UI zeigt zur betroffenen Auftragsposition freigegebene Alternativen mit Bestand, Lagerplatz, Kompatibilitaet, Preisabweichung, Kundenzulassung, Qualitaetsstatus und Auswirkungen auf Auftrag, Lieferschein und Rechnung.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Per Rechtsklick auf die fehlende Position kann der Nutzer "Alternativen pruefen" waehlen. CTOX bewertet Ersatzartikel anhand von Artikelstamm, Kundenregeln, bisherigen Lieferungen, technischen Merkmalen und Verfuegbarkeit.

**Erfuellt wenn**  
Ein Ersatzartikel kann nur verwendet werden, wenn er fuer den Kunden- und Auftragskontext zulaessig ist oder explizit freigegeben wurde. Grund, Freigabe und Belegwirkung bleiben gespeichert.

## WH-036 Packplatz: Ware fehlerfrei verpacken

**Situation/Ziel**  
Als Packplatz-Mitarbeiter will ich eine kommissionierte Sendung scannen, pruefen, verpacken und abschliessen, damit nur vollstaendige und korrekt dokumentierte Ware in den Versand geht.

**Manuell optimale UI**  
Die UI zeigt Auftrag, Kunde, Lieferadresse, Positionen, Soll-/Ist-Mengen, Serien-/Chargenpflichten, Sonderhinweise, Packmittel und Gewicht. Ich scanne Behaelter, Artikel und Seriennummern, bestaetige Mengen, waehle Packmittel und drucke Packliste sowie Versandlabel.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Per Rechtsklick auf eine Sendung kann ich "Packvorschlag erzeugen", "Abweichung erklaeren" oder "Pflichtdaten pruefen" ausloesen. Im Chat kann ich fragen: "Warum ist Auftrag 4711 nicht packbar?"

**Erfuellt wenn**  
Die Sendung kann nur abgeschlossen werden, wenn Pflichtscans, Mengen, Chargen, Seriennummern, Gewichte und Packmittel plausibel sind. Packstatus, Packstueckdaten, Label, Packliste und Bestand sind konsistent.

## WH-037 Versanduebergabe an Frachtfuehrer

**Situation/Ziel**  
Als Versandmitarbeiter will ich fertig gepackte Packstuecke gesammelt, nachvollziehbar und fristgerecht an den richtigen Frachtfuehrer uebergeben, damit keine Sendung verloren geht oder falsch verladen wird.

**Manuell optimale UI**  
Die UI zeigt versandbereite Packstuecke nach Carrier, Tour, Cut-off-Zeit, Ladezone, Servicelevel und Sonderhandling. Ich scanne jedes Packstueck beim Verladen und bestaetige die Uebergabe mit Fahrer, Kennzeichen, Uhrzeit und Anzahl.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Per Rechtsklick auf eine Tour kann ich "Uebergabe pruefen" oder "fehlende Packstuecke suchen" waehlen. Im Chat kann ich fragen: "Welche Sendungen verpassen den DHL-Cut-off um 17:00?"

**Erfuellt wenn**  
Eine Uebergabe ist nur moeglich, wenn alle Packstuecke korrekt gescannt, dem Carrier zugeordnet und nicht gesperrt sind. Status, Tracking und Uebergabeprotokoll sind revisionssicher.

## WH-038 Retourenannahme pruefen und einbuchen

**Situation/Ziel**  
Als Retourenmitarbeiter will ich eingehende Retouren identifizieren, gegen urspruengliche Lieferung oder RMA pruefen und den richtigen Folgeprozess starten, damit verkaufsfaehige Ware wieder verfuegbar wird und beschaedigte Ware nicht in den Bestand gelangt.

**Manuell optimale UI**  
Die UI ermoeglicht Scan von Paket, RMA, Lieferschein, Seriennummer oder Artikel. Sie zeigt Originalauftrag, erwartete Positionen, Fristen, Zustandshistorie und Zielbereiche; ich erfasse Zustand, Menge, Fotos, Zubehoer, Seriennummern und Grundcode.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Per Rechtsklick auf eine Retoure kann ich "Retoure identifizieren", "Zustand aus Fotos vorschlagen" oder "Folgeprozess empfehlen" ausloesen. CTOX nutzt Auftragsdaten, RMA-Regeln, Fotos und Seriennummernhistorie.

**Erfuellt wenn**  
Eine Retoure wird nur abgeschlossen, wenn Artikel, Menge, Zustand und Zielbestand eindeutig dokumentiert sind. Kundenfall, Erstattungsfreigabe und Lagerort werden konsistent aktualisiert.

## WH-039 Quarantaene und Sperrbestand verwalten

**Situation/Ziel**  
Als Lagerverantwortlicher will ich Ware mit Qualitaets-, Compliance- oder Bestandsrisiko gezielt sperren und verwalten, damit sie nicht kommissioniert, verkauft oder versendet wird, bevor sie freigegeben ist.

**Manuell optimale UI**  
Die UI zeigt Sperrbestaende nach Artikel, Charge, Seriennummer, Lagerplatz, Sperrgrund, Verantwortlichem, Frist und Folgeaktion. Ich kann Bestand per Scan, Auswahl oder Massenaktion sperren und Freigaben nur mit Berechtigung durchfuehren.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Per Rechtsklick auf Bestand, Charge oder Lagerplatz kann ich "Sperrgrund analysieren" oder "betroffene Auftraege finden" waehlen. CTOX verbindet Lagerbestand, Wareneingang, Qualitaetsfaelle, Lieferantenhistorie und offene Auftraege.

**Erfuellt wenn**  
Gesperrte Ware ist technisch nicht kommissionierbar, nicht reservierbar und nicht versandfaehig. Jede Sperrung und Freigabe enthaelt Grund, Zeitpunkt, Benutzer, Menge und Nachweis.

## WH-040 Qualitaetspruefung nach Wareneingang

**Situation/Ziel**  
Als Qualitaetspruefer will ich pruefpflichtige Wareneingaenge strukturiert kontrollieren, dokumentieren und freigeben oder sperren, damit nur konforme Ware in verfuegbaren Lagerbestand gelangt.

**Manuell optimale UI**  
Die UI zeigt Wareneingang, Bestellung, Lieferant, Artikel, Charge, Pruefvorschrift, Stichprobe, Messwerte, Toleranzen, Fotos und Historie. Ich erfasse Ergebnisse und entscheide ueber Freigabe, Nacharbeit, Teilfreigabe oder Sperrbestand.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Per Rechtsklick kann ich "Pruefplan vorschlagen", "Messwerte bewerten" oder "Abweichungsbericht erstellen" ausloesen. CTOX beruecksichtigt Pruefvorschriften, Lieferantenqualitaet, Fotos, Messwerte und fruehere Reklamationen.

**Erfuellt wenn**  
Pruefpflichtige Ware wird erst nach dokumentierter Entscheidung verfuegbar. Bei Nichtkonformitaet entsteht automatisch Sperrbestand oder ein Qualitaetsfall.

## WH-041 Benutzerrollen und Rechte

**Situation/Ziel**  
Als Lagerleiter will ich steuern, wer Bestaende sieht, bucht, sperrt, freigibt oder korrigiert, damit Schichtmitarbeiter schnell arbeiten koennen, kritische Lagerentscheidungen aber nur von berechtigten Rollen ausgefuehrt werden.

**Manuell optimale UI**  
Die UI zeigt Rollen wie Kommissionierer, Wareneingang, Packplatz, Schichtleiter, Lagerleiter und 3PL-Kundenbetreuer mit konkreten Rechten pro Mandant, Lager, Zone, Owner, Status und Vorgang. Temporare Schichtrechte und Vier-Augen-Freigaben sind sichtbar.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Per Rechtsklick auf Benutzer, Rolle, Lagerzone oder gesperrten Bestand kann ich CTOX fragen, warum eine Aktion blockiert wurde und welche minimale Rechteaenderung noetig ist. CTOX erstellt nur einen Vorschlag, keine stille Rechteaenderung.

**Erfuellt wenn**  
Ein Mitarbeiter sieht und bearbeitet nur erlaubte Bestaende, Auftraege und Lagerbereiche. Jede Rechteaenderung ist mandanten-, rollen- und zeitbezogen auditierbar.

## WH-042 Mehrmandantenfaehiges 3PL-Lager

**Situation/Ziel**  
Als 3PL-Operations-Manager will ich mehrere Kunden in denselben physischen Lagerzonen bedienen, ohne Bestaende, Reservierungen, Auftraege, SLA-Regeln oder Auswertungen zwischen Mandanten und Ownern zu vermischen.

**Manuell optimale UI**  
Die UI trennt physische Lagerstruktur und kaufmaennische Eigentuemerdimension. Ich kann Bestaende nach Mandant, Kunde, Artikel, Charge, Status und SLA filtern und direkt sehen, aus welchen erlaubten Quellen kommissioniert werden darf.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Per Rechtsklick auf Auftrag, Slot, Bestand oder Kundenkonto kann CTOX pruefen, ob Umlagerung, Reservierung oder Kommissionierung mandantenrein ist. Chat-Antworten werden nach Owner, SLA und erlaubter Aktion getrennt.

**Erfuellt wenn**  
Bestand, Bewegung, Reservierung, Pickliste, Versand und Korrektur tragen immer Mandant und Owner. Fremder Kundenbestand kann ohne explizite, auditierte Regel nicht verwendet werden.

## WH-043 Schichtuebergabe ohne Informationsverlust

**Situation/Ziel**  
Als Schichtleiter will ich am Ende meiner Schicht offene Risiken, blockierte Arbeit und Entscheidungen strukturiert uebergeben, damit die naechste Schicht ohne Nachfragen weiterarbeiten kann.

**Manuell optimale UI**  
Die UI bietet eine Uebergabeansicht mit offenen Picklisten, nicht gepackten Sendungen, gesperrten Slots, fehlenden Scans, Zaehldifferenzen, Rueckstaenden und SLA-nahem Auftragbestand. Eintraege koennen bestaetigt, Verantwortliche gesetzt und Nachweise angehaengt werden.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
CTOX erzeugt aus Lagerereignissen, Kommentaren, Scanfehlern, Auftragsstatus und SLA-Daten einen Uebergabeentwurf. Per Rechtsklick auf Auftrag oder Zone kann eine Lageeinschaetzung mit Ursache, naechstem Schritt und spaetestem Handlungszeitpunkt erstellt werden.

**Erfuellt wenn**  
Jede Schicht hat ein signiertes Uebergabeprotokoll mit offenen Aufgaben, Risiken, Entscheidungen und Verantwortlichen. Kritische Punkte werden in die Aufgabenqueue der Folgeschicht uebernommen.

## WH-044 Operative Aufgaben priorisieren

**Situation/Ziel**  
Als Lagerkoordinator will ich naechste Aufgaben nach echter operativer Dringlichkeit sortieren, damit Mitarbeiter nicht nur Listen abarbeiten, sondern Cutoffs, Wege, Qualifikationen und Abhaengigkeiten beruecksichtigen.

**Manuell optimale UI**  
Die Aufgabenansicht zeigt Typ, Zone, Artikel, Menge, Mandant, Deadline, SLA-Status, Abhaengigkeiten, benoetigte Rolle, Equipment und erwartete Dauer. Aufgaben koennen gefiltert, umpriorisiert, begruendet und zugewiesen werden.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
CTOX schlaegt aus Auftraegen, Lagerwegen, Cutoffs, Personal, Rechten, Scannerstatus und Sperren eine priorisierte Reihenfolge vor. Per Rechtsklick kann ich fragen, warum eine Aufgabe hoch oder niedrig priorisiert ist.

**Erfuellt wenn**  
Die Aufgabenqueue begruendet Prioritaeten nachvollziehbar. Manuelle Overrides sind moeglich, aber begruendungspflichtig und auditierbar.

## WH-045 SLA- und Deadline-Risiken erkennen

**Situation/Ziel**  
Als Fulfillment-Verantwortlicher will ich gefaehrdete Auftraege vor dem Cutoff sehen und gezielt retten, damit SLA-Brueche nicht erst auffallen, wenn Versand oder Kundendeadline verpasst sind.

**Manuell optimale UI**  
Die UI zeigt eine Risikoansicht mit Auftraegen, Sendungen, Picklisten und Wareneingaengen, deren Deadline gefaehrdet ist. Pro Risiko sehe ich Deadline, Restzeit, fehlenden Schritt, blockierende Ursache, Bestandssituation, moegliche Sofortaktion und Eskalationskontakt.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann fragen: "Welche Auftraege brechen heute voraussichtlich SLA und was rettet sie?" CTOX liefert Rettungsvorschlaege wie alternative Lagerquelle, Teilversand, Express-Pick, Freigabe oder Kundeneskalation.

**Erfuellt wenn**  
Deadline-Risiken werden vor Verletzung sichtbar und nach Ursache gruppiert. Jede Empfehlung nennt betroffene Auftraege, spaetesten Eingriffszeitpunkt und naechste ausfuehrbare Aktion.

## WH-046 Initiale Bestandsuebernahme

**Situation/Ziel**  
Als Lagerleiter will ich bestehende Artikel-, Lagerplatz- und Bestandsdaten aus ERP, CSV oder Excel uebernehmen, damit das virtuelle Lager mit realen Startbestaenden produktiv arbeiten kann.

**Manuell optimale UI**  
Der Nutzer laedt Importdateien in einem Assistenten hoch, sieht erkannte Spalten, Datenqualitaet, Dubletten, fehlende Pflichtfelder und Konflikte. Er kann Spalten zuordnen, Testlaeufe ausfuehren und die finale Uebernahme freigeben.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann eine Datei oder fehlerhafte Zeile per Rechtsklick an CTOX uebergeben und fragen: "Pruefe diesen Import und schlage ein Mapping vor." CTOX erkennt Spalten, Artikelnummern, Slot-Codes, Mengeneinheiten und Konflikte.

**Erfuellt wenn**  
Eine reale Startbestandsdatei kann validiert, korrigiert, teilweise ausgeschlossen und nachvollziehbar importiert werden. Kein Bestand wird ohne explizite Freigabe veraendert.

## WH-047 Export- und Reporting-Center

**Situation/Ziel**  
Als Operations Manager will ich Bestaende, Bewegungen, offene Aufgaben und Abweichungen exportieren und auswerten, damit Lagerleistung, Inventurstatus und ERP-relevante Daten belastbar verfuegbar sind.

**Manuell optimale UI**  
Das Reporting-Center bietet Filter fuer Zeitraum, Mandant, Lager, Zone, Artikel, Charge, Seriennummer, Auftrag und Status. Ergebnisse lassen sich pruefen, gruppieren, sortieren und als CSV, XLSX oder PDF exportieren.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann fragen: "Zeige alle Artikel mit negativem Bestand seit Montag" oder per Rechtsklick "Erklaere diese Abweichung" waehlen. CTOX erstellt Filter, fasst Ergebnisse zusammen und bereitet Exporte vor.

**Erfuellt wenn**  
Reports liefern konsistente Zahlen zur UI und API. Jeder Export enthaelt Filter, Erstellungszeitpunkt, Benutzer und Datenstand und ist reproduzierbar.

## WH-048 Fehleranalyse und Replay von Lagerprozessen

**Situation/Ziel**  
Als Support- oder Lagerverantwortlicher will ich fehlerhafte Bewegungen, Sync-Probleme oder Prozessabbrueche analysieren und kontrolliert erneut ausfuehren, damit operative Stoerungen schnell behoben werden.

**Manuell optimale UI**  
Ein Ereignisprotokoll zeigt Wareneingang, Einlagerung, Umlagerung, Kommissionierung, Verpackung, Versand, Inventur und API-Sync mit Status, Eingabedaten, Systementscheidung, Folgeaktionen, Fehler und betroffenen Objekten. Ein Replay-Dialog zeigt vorab Nebenwirkungen.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann auf einen Fehler rechtsklicken und "Ursache analysieren" waehlen. CTOX prueft Prozesshistorie, Stammdaten, Reservierungen, API-Antworten und konkurrierende Bewegungen.

**Erfuellt wenn**  
Ein fehlgeschlagener Prozess ist lueckenlos nachvollziehbar. Replay ist idempotent oder zeigt Nebenwirkungen klar, laeuft nur nach Berechtigungspruefung und expliziter Bestaetigung.

## WH-049 API- und Webshop-Synchronisation

**Situation/Ziel**  
Als E-Commerce-Verantwortlicher will ich Bestaende, Reservierungen, Auftraege, Versandstatus und Retouren zwischen CTOX Warehouse, ERP und Webshop synchronisieren, damit Shop und Lager denselben operativen Stand verwenden.

**Manuell optimale UI**  
Die UI zeigt pro angebundenem System Verbindungsstatus, letzte Synchronisation, Fehler, Warteschlange und Datenobjekte. Einzelne Auftraege, Artikel oder Bestaende koennen neu synchronisiert und Konflikte bearbeitet werden.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann fragen: "Warum zeigt Shopify fuer SKU-100 weniger Bestand als CTOX?" CTOX vergleicht Lagerbestand, Reservierungen, offene Picks, API-Antworten und Webshop-Werte und schlaegt eine Korrektur oder Neusynchronisation vor.

**Erfuellt wenn**  
Bestands- und Auftragsdaten werden zuverlaessig synchronisiert, Konflikte werden sichtbar statt ueberschrieben, und manuelle Neusynchronisation erzeugt keine Dubletten, doppelten Reservierungen oder falschen Bestaende.

## WH-050 Produktionsreife Abnahme des Warehouse-Moduls

**Situation/Ziel**  
Als CTO, Produktverantwortlicher und Lagerleitung will ich klare Abschlusskriterien fuer das Warehouse-Modul, damit es als produktive Lageranwendung freigegeben werden kann und nicht nur als Demo funktioniert.

**Manuell optimale UI**  
Die Abnahme erfolgt ueber eine produktionsnahe Checkliste in CTOX: Stammdaten, Bestaende, Bewegungen, Rollen, Audit-Logs, Import, Export, Replay, API-Sync, Performance, Fehlerfaelle und Berechtigungen. Kritische Punkte blockieren die Freigabe.

**KI-gestuetzt ueber CTOX Chat/Rechtsklick**  
Der Nutzer kann fragen: "Welche Punkte verhindern aktuell den Go-live?" oder "Erstelle eine Abnahmezusammenfassung fuer das Warehouse-Modul." CTOX liest Testergebnisse, offene Fehler und fehlende Konfigurationen aus.

**Erfuellt wenn**  
Das Modul kann reale Lagerprozesse end-to-end ausfuehren: Bestandsuebernahme, Wareneingang, Einlagerung, Umlagerung, Reservierung, Pick, Pack, Versand, Inventur, Reporting, Fehleranalyse und Webshop-Sync. Rollen, Rechte, Audit, Wiederherstellung, Performance und Datenkonsistenz sind produktionsnah nachgewiesen.
