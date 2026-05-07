# Workforce User Stories

Jede Story hat zwei Bedienwege: manuell in der UI und KI-gestuetzt ueber CTOX-Kontext/Rechtsklick.

1. Als Einsatzplaner will ich einen neuen Einsatz fuer Person, Tag, Zeit und Arbeitsplatz anlegen, damit Arbeit nicht ausserhalb des Plans entsteht. Manuell: linke Spalte `Neuer Einsatz` ausfuellen und speichern. KI: freie Zelle rechtsklicken und CTOX bitten, den passenden Einsatz fuer Person/Tag anzulegen.
2. Als Einsatzplaner will ich einen Einsatz per Drag-Drop auf eine andere Person verschieben, damit Umplanung direkt am Plan passiert. Manuell: Karte auf andere Person/Tag-Zelle ziehen. KI: Karte rechtsklicken und Umplanung mit Zielperson ansagen.
3. Als Einsatzplaner will ich einen Einsatz fuer den naechsten Tag duplizieren, damit wiederkehrende Arbeit nicht neu getippt wird. Manuell: Karte rechtsklicken oder unten `Naechsten Tag duplizieren`. KI: Karte rechtsklicken und `morgen nochmal planen`.
4. Als Teamleiter will ich Ueberschneidungen blockiert bekommen, damit niemand doppelt geplant wird. Manuell: Drag-Drop oder Anlage versucht Ueberschneidung, UI zeigt Fehler. KI: CTOX prueft geplante Aenderung vor Ausfuehrung.
5. Als Teamleiter will ich inaktive Personen sehen und blockierte Einsaetze erkennen. Manuell: Teamliste und Score unten pruefen. KI: Einsatz rechtsklicken und Score pruefen lassen.
6. Als Mitarbeiter will ich Istzeit zu einem Einsatz erfassen, damit Planung und Arbeit getrennt bleiben. Manuell: Karte rechtsklicken `Zeit erfassen`. KI: `Zeit fuer diesen Einsatz 08:05 bis 15:50 mit 30 Minuten Pause eintragen`.
7. Als Pruefer will ich eingereichte Zeiten rechts freigeben. Manuell: rechte Spalte `Zeitpruefung`, Button `Freigeben`. KI: Zeitnachweis rechtsklicken und Freigabe anweisen.
8. Als Pruefer will ich eine Korrektur anfordern. Manuell: rechte Spalte `Korrektur`. KI: CTOX schreibt Korrekturgrund und setzt Status.
9. Als Abrechnung will ich freigegebene abrechenbare Zeiten vorbereiten. Manuell: Bottom-Drawer `Rechnung vorbereiten`. KI: Einsatz rechtsklicken und Uebergabe an Rechnung vorbereiten.
10. Als Planer will ich pro Einsatz einen Score sehen. Manuell: Karte anklicken, unten Basis/Leistung/Bonus lesen. KI: Karte rechtsklicken `Score pruefen`.
11. Als Planer will ich Basis-Anforderungen separat von Leistungsanforderungen sehen. Manuell: Bottom-Drawer-Spalten lesen. KI: CTOX nennt nur fehlerhafte Basischecks zuerst.
12. Als Planer will ich eine leere Zelle direkt belegen. Manuell: Plus in Zelle oder Rechtsklick auf Zelle. KI: freie Zelle rechtsklicken und Einsatz beschreiben.
13. Als Stammdatenpfleger will ich Schichttypen anlegen. Manuell: links `Verwalten`, Formular Schichttyp. KI: CTOX Chat `Schichttyp Spaetdienst 14-22 anlegen`.
14. Als Stammdatenpfleger will ich Mitarbeiter anlegen. Manuell: linker Side-Drawer, Mitarbeiterformular. KI: CTOX Chat `Lea Maier, Service, Team Sued, 30h anlegen`.
15. Als Planer will ich Einsaetze archivieren, statt sie hart zu loeschen. Manuell: Rechtsklick `Archivieren`. KI: `diesen Einsatz archivieren, Grund Krankheit`.
16. Als Pruefer will ich alle Blocker rechts sehen. Manuell: rechte Spalte `Blocker`. KI: CTOX fasst Blocker der Woche zusammen.
17. Als Planer will ich Blocker loesen. Manuell: Button `Loesen`. KI: CTOX setzt nach Korrektur den Einsatz wieder auf geplant.
18. Als Controller will ich alle Events sehen. Manuell: rechter Side-Drawer `Ereignisse`. KI: CTOX erklaert die letzten Aenderungen.
19. Als CTOX-Nutzer will ich jede Einsatzkarte als Kontext verwenden. Manuell: Rechtsklick CTOX-Menue. KI: `Pruefe diesen Einsatz`.
20. Als Planer will ich sehen, welche Einsaetze noch keine Istzeit haben. Manuell: Karte zeigt `keine Istzeit`, rechte offene Arbeit. KI: CTOX listet Einsaetze ohne Istzeit.
21. Als Planer will ich Zeiten mit Abweichung erkennen. Manuell: Bottom-Score `Abweichung im Rahmen`. KI: CTOX markiert Abweichungen ueber 30 Minuten.
22. Als Abrechnung will ich nur genehmigte Zeiten weitergeben. Manuell: Rechnung vorbereiten ist erst nach Freigabe aktiv. KI: CTOX verweigert Uebergabe ohne Freigabe.
23. Als Planer will ich Auftrag/Kunde am Einsatz pflegen. Manuell: Bottom-Drawer Felder `Kunde`, `Auftrag`. KI: `diesen Einsatz SO-128/Muster GmbH zuordnen`.
24. Als Teamleiter will ich Person, Schichttyp und Arbeitsplatz im Bottom-Drawer aendern. Manuell: Dropdowns bearbeiten und speichern. KI: `auf Marc und Werkbank 2 umbuchen`.
25. Als Planer will ich Tagesplan ohne Modals scannen. Manuell: Mitte zeigt Personen x Tage. KI: CTOX fasst Tageslast zusammen.
26. Als Planer will ich Rollen am Schichttyp sehen. Manuell: Setup-Side-Drawer. KI: CTOX prueft Rollenpassung.
27. Als Planer will ich Schichtfarben als Orientierung sehen. Manuell: Kartenrandfarbe. KI: keine Aktion, nur visuelle Orientierung.
28. Als Admin will ich aktive/inaktive Personen unterscheiden. Manuell: Teamliste. KI: CTOX findet Einsaetze inaktiver Personen.
29. Als Pruefer will ich Nachweise sehen. Manuell: Bottom-Drawer Istzeit-Fakt. KI: CTOX nennt fehlenden Nachweis.
30. Als Planer will ich Fehler direkt am Objekt bearbeiten. Manuell: Karte anklicken, Bottom-Drawer. KI: Rechtsklick auf Karte, Korrektur ansagen.
31. Als Planer will ich keine zentrale Aktionsleiste fuer Einzelkarten brauchen. Manuell: Karte hat Rechtsklick und Bottom-Aktionen. KI: Kontext ist die Karte.
32. Als Controller will ich sehen, welche Payloads an CTOX entstanden. Manuell: rechter Side-Drawer. KI: CTOX liest letzten Payload.
33. Als Operations Lead will ich freie Kapazitaeten sehen. Manuell: leere Zellen im Plan. KI: CTOX nennt freie Personen/Tage.
34. Als Planer will ich Standardzeiten vom Schichttyp uebernehmen. Manuell: neuer Einsatz waehlt Schichttyp. KI: `Fruehservice fuer Anna am Mittwoch`.
35. Als Pruefer will ich Statuswechsel persistent nach Reload sehen. Manuell: Seite neu laden. KI: CTOX prueft API-Snapshot.
36. Als Planer will ich Drag-Drop-Fehler nicht still verlieren. Manuell: Fehlermeldung oben. KI: CTOX nennt Konflikt-ID.
37. Als Admin will ich Arbeitsplatzkapazitaeten spaeter erweitern. Manuell: Location Slots im Modell. KI: CTOX kann Slots anlegen.
38. Als Planer will ich ohne Tabellenform arbeiten. Manuell: Wochenplan statt Tabelle. KI: CTOX operiert auf Karten und Zellen.
39. Als Teamleiter will ich Arbeit und Zeitnachweis getrennt pruefen. Manuell: Einsatzkarte vs. Zeitpruefung rechts. KI: CTOX unterscheidet Planung/Ist.
40. Als Abrechnung will ich interne nicht abrechenbare Schichten nicht an Rechnung geben. Manuell: Runtime blockt nicht billable Schichttypen. KI: CTOX erklaert Ablehnung.
41. Als Planer will ich im Plan direkt neue Arbeit erfassen. Manuell: Plus in leerer Zelle. KI: freie Zelle rechtsklicken.
42. Als Planer will ich Wochenplanung mit echtem Wiedererkennungswert. Manuell: Einsatzplan bleibt immer Mitte. KI: CTOX referenziert Person/Tag/Einsatz.
43. Als Pruefer will ich alle eingereichten Zeiten gesammelt rechts haben. Manuell: rechte Pruefungsliste. KI: CTOX fasst Freigabeliste zusammen.
44. Als Planer will ich eine Karte aufklappen, ohne die Seite zu wechseln. Manuell: Bottom-Drawer. KI: CTOX arbeitet auf gleichem Kontext.
45. Als Planer will ich Stamm- und Arbeitsansicht trennen, aber verbunden halten. Manuell: linker Side-Drawer aus Hauptansicht. KI: CTOX kann Stammdaten aus Kontext anlegen.
46. Als Controller will ich Audit ohne Datenbanksuche sehen. Manuell: rechter Side-Drawer Events. KI: CTOX erklaert Eventfolge.
47. Als Planer will ich fehlende Anforderungen pro Einsatz konkret sehen. Manuell: Score-Checks. KI: CTOX nennt nur die Checks, die fehlschlagen.
48. Als Pruefer will ich Korrekturgruende am Blocker wiederfinden. Manuell: rechte Blockerkarte. KI: CTOX setzt/liest Blocker.
49. Als Planer will ich die Bedienung durch Wiederholung lernen. Manuell: jede Einsatzkarte hat dieselben Aktionen. KI: jedes Kontextmenue spricht dieselben Commands.
50. Als CTOX Operator will ich pruefen koennen, ob die UI nur Fassade ist. Manuell: Smoke plus Browser: Klick, Rechtsklick, Drag-Drop, Reload. KI: CTOX fuehrt genau diese Akzeptanzschritte aus.
51. Als Planer will ich Abwesenheiten direkt aus der Einsatzplanung erfassen, damit Urlaub, Krankheit oder Sperrzeiten nicht neben dem Plan gepflegt werden. Manuell: linker Drawer `Abwesenheit`, Person und Zeitraum speichern. KI: CTOX Chat `Marc ist am 18.07. nicht verfuegbar, bitte eintragen und Planung blockieren`.
52. Als Teamleiter will ich Einsaetze waehrend Abwesenheit blockiert bekommen, damit keine ungueltige Planung entsteht. Manuell: Einsatzanlage auf abwesenden Tag zeigt Fehler. KI: CTOX verweigert die Anlage und nennt die konkrete Abwesenheit.
53. Als Planer will ich wiederkehrende Schichten als Muster anlegen, damit regelmaessige Arbeit nicht einzeln kopiert wird. Manuell: linker Drawer `Wiederkehrende Schicht`, Wochentag und Zeitraum speichern. KI: CTOX Chat `Anna jeden Montag Fruehservice fuer die naechsten drei Wochen planen`.
54. Als Planer will ich ein Schichtmuster materialisieren, damit aus einer Regel konkrete Karten im Plan werden. Manuell: Musterformular speichert und plant die Vorkommen. KI: CTOX fuehrt `materialize_recurring_shift_pattern` fuer den Zeitraum aus.
55. Als Pruefer will ich Arbeitszeitregeln im Score sehen, damit Konflikte dort erscheinen, wo der Einsatz bearbeitet wird. Manuell: Bottom-Score zeigt Tagesgrenze, Wochenlast und Ruhezeit. KI: CTOX `Score pruefen` nennt nur verletzte Regeln.
56. Als Payroll-Verantwortlicher will ich freigegebene Zeiten als Lohnkandidaten vorbereiten, damit Lohnabrechnung nicht erneut Zeiten suchen muss. Manuell: Bottom oder Rechtsklick `Payroll vorbereiten`. KI: CTOX erstellt den Kandidaten mit Periode, Mitarbeiter, Stunden und Betrag.
57. Als Payroll-Verantwortlicher will ich Workforce-Stunden im Payroll-Run sehen, damit der Uebergabepunkt wirklich funktioniert. Manuell: Payroll-Run erzeugt eine `workforce_hours` Position. KI: CTOX startet den Run und prueft die Position.
58. Als Abrechnung will ich aus freigegebenen Zeiten einen Rechnungsdraft erzeugen, damit der Schritt zwischen Einsatz und Rechnung sichtbar ist. Manuell: Bottom oder Rechtsklick `Rechnungsdraft erstellen`. KI: CTOX erstellt Draft mit Kunde, Projekt, Stunden, Satz und Business-Invoice-Link.
59. Als Operations Lead will ich alle Uebergaben rechts sehen, damit offene Payroll- und Rechnungsarbeit nicht im Score versteckt bleibt. Manuell: rechter Drawer zeigt Payroll-Kandidaten und Rechnungsdrafts. KI: CTOX fasst die offenen Uebergaben zusammen.
60. Als CTOX Operator will ich M2 gegen echte Interaktion pruefen, damit keine Fassade ausgeliefert wird. Manuell: Browser oeffnet Route, Drawer, Bottom, Rechtsklick und prueft Console. KI: CTOX fuehrt denselben Browserproof nach jedem M2-Change aus.
