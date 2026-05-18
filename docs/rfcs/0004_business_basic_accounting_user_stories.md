# RFC 0004: Business Basic Accounting User Stories

Status: Draft

Diese User Stories beschreiben die Ziel-UX fuer die Business-Basic-Buchhaltung. Jede Story hat zwei Bedienpfade: manuell optimale UI und KI-gestuetzt ueber CTOX. CTOX darf vorbereiten, pruefen, erklaeren und Vorschlaege erzeugen. Festschreibende Buchungen passieren erst nach Freigabe.

## 1. Angebot fuer Projektpaket erstellen

Als Vertriebsmitarbeiter erstelle ich fuer die REM Capital AG ein Angebot ueber ein CTOX Automatisierungspaket, damit der Kunde Preis, Leistungsumfang und Zahlungsziel sauber pruefen kann.

**Manuell optimale UI**  
Ich oeffne `Geschaeft > Angebote`, waehle `REM Capital AG` und lege ein Angebot im Status `Entwurf` an. Ich erfasse `CTOX Business Setup` fuer 4.800,00 EUR netto auf `8400` und `Workshop Agentic Workflows` fuer 1.200,00 EUR netto auf `8337`. Die UI zeigt netto 6.000,00 EUR, Umsatzsteuer 1.140,00 EUR und brutto 7.140,00 EUR, ohne eine Ledger-Buchung zu erzeugen.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Erstelle REM Capital ein Angebot fuer CTOX Business Setup 4.800 netto und Workshop 1.200 netto, Zahlungsziel 14 Tage." CTOX erkennt Kunde, Leistungen, Steuerlogik und Erloskonten, erstellt eine Angebotsvorschau und fragt vor dem Versand nach Freigabe.

**Erfuellt, wenn**  
Das Angebot hat den Status `gesendet`, traegt `ANG-2026-0042`, enthaelt 7.140,00 EUR brutto, ist beim Kunden hinterlegt und hat noch keinen Ledger-Eintrag.

## 2. Angebot in Ausgangsrechnung uebernehmen

Als Buchhalterin ueberfuehre ich ein angenommenes Angebot in eine Rechnung, damit aus dem bestaetigten Auftrag eine nummerierte und gebuchte Forderung wird.

**Manuell optimale UI**  
Ich oeffne `ANG-2026-0042` mit Status `angenommen` und klicke `Rechnung erstellen`. Die UI uebernimmt Positionen, Steuer, Kunde und Zahlungsziel. Beim Senden wird `RE-2026-0089` vergeben und die Vorschau zeigt Soll `1400 Forderungen` 7.140,00 EUR, Haben `8400` 4.800,00 EUR, Haben `8337` 1.200,00 EUR und Haben `1776 Umsatzsteuer` 1.140,00 EUR.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Wandle das angenommene Angebot ANG-2026-0042 in eine Rechnung um und sende sie heute." CTOX prueft Annahmestatus, Pflichtfelder nach Paragraf 14 UStG und Buchungsvorschau. Erst nach meiner Freigabe wird gesendet.

**Erfuellt, wenn**  
`RE-2026-0089` ist gesendet, das Angebot ist als `abgerechnet` markiert und der Ledger enthaelt eine ausgeglichene Forderungsbuchung ueber 7.140,00 EUR.

## 3. Geschaeftskunden mit Steuerdaten anlegen

Als Office-Mitarbeiterin lege ich die Tank Technology GmbH als Kunden an, damit Angebote und Rechnungen mit korrekter Adresse, USt-IdNr. und Zahlungsbedingungen entstehen.

**Manuell optimale UI**  
Ich oeffne `Geschaeft > Kunden`, klicke `Neuer Kunde` und erfasse Name, Adresse, `DE312456789`, Zahlungsziel `14 Tage netto`, Standard-Forderungskonto `1400` und Standard-Erloskonto `8400`. Die UI validiert Pflichtfelder und zeigt den Steuerstatus `Inland 19 Prozent`.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Lege Tank Technology GmbH als deutschen B2B-Kunden mit 14 Tagen Zahlungsziel und USt-IdNr. DE312456789 an." CTOX prueft Dubletten, schlaegt Konten vor und legt den Kunden nach Freigabe an.

**Erfuellt, wenn**  
Der Kunde ist `aktiv`, Steuerdaten sind gespeichert, neue Rechnungen nutzen `1400` als Forderungskonto und das Zahlungsziel wird automatisch uebernommen.

## 4. SaaS-Abo als Ausgangsrechnung senden

Als Buchhalterin stelle ich der Tank Technology GmbH das monatliche CTOX SaaS-Abo in Rechnung, damit wiederkehrender Umsatz korrekt gebucht wird.

**Manuell optimale UI**  
Ich oeffne `Rechnungen`, waehle `Neue Rechnung`, Kunde `Tank Technology GmbH` und Produkt `CTOX SaaS Business Basic, Mai 2026` fuer 980,00 EUR netto. Die UI zeigt 186,20 EUR Umsatzsteuer auf `1776`, brutto 1.166,20 EUR und beim Senden die Nummer `RE-2026-0094`.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Erstelle die Mai-Rechnung fuer Tank Technology ueber das CTOX SaaS-Abo, 980 EUR netto, heute senden." CTOX nutzt Produktkonto `8400`, erzeugt PDF und Buchungsvorschau und wartet auf Freigabe.

**Erfuellt, wenn**  
`RE-2026-0094` ist gesendet, 1.166,20 EUR stehen im Soll auf `1400`, 980,00 EUR im Haben auf `8400`, 186,20 EUR auf `1776`, und die Rechnung ist offen faellig.

## 5. Zahlung einer offenen Rechnung zuordnen

Als Buchhalterin ordne ich einen Bankeingang einer offenen Rechnung zu, damit die Forderung ausgeglichen und die Rechnung bezahlt ist.

**Manuell optimale UI**  
Ich oeffne `Zahlungen`, sehe den Bankumsatz `Tank Technology GmbH`, Verwendungszweck `RE-2026-0094`, Betrag 1.166,20 EUR. Die UI schlaegt die offene Rechnung vor. Ich bestaetige die Zuordnung, die Buchung lautet Soll `1200 Bank`, Haben `1400 Forderungen`.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Ordne den Bankeingang 1.166,20 von Tank Technology der Rechnung RE-2026-0094 zu." CTOX prueft Betrag, Nummer und offenen Saldo und bucht nach meiner Freigabe.

**Erfuellt, wenn**  
Die Rechnung steht auf `bezahlt`, der offene Saldo ist 0,00 EUR und der Bankumsatz ist mit Journal Entry und Rechnung verknuepft.

## 6. Teilzahlung auf Rechnung verbuchen

Als Buchhalterin verbuche ich eine Teilzahlung der REM Capital AG, damit der Restbetrag weiter offen und mahnfaehig bleibt.

**Manuell optimale UI**  
Ich oeffne `RE-2026-0089` ueber 7.140,00 EUR und waehle den Bankeingang ueber 3.000,00 EUR. Die UI zeigt `Teilzahlung`, berechnet den Rest 4.140,00 EUR und setzt die Rechnung auf `teilweise bezahlt`.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Buche die 3.000 EUR Zahlung von REM Capital als Teilzahlung auf RE-2026-0089." CTOX erkennt, dass der Betrag kleiner als der offene Saldo ist, und schlaegt eine Teilzuordnung vor.

**Erfuellt, wenn**  
`RE-2026-0089` zeigt 3.000,00 EUR bezahlt, 4.140,00 EUR offen, Status `teilweise bezahlt`, und der Ledger enthaelt die Bankbuchung gegen `1400`.

## 7. Erste Mahnung senden

Als Buchhalterin sende ich eine erste Mahnung fuer eine ueberfaellige Rechnung, damit der Kunde freundlich an den offenen Betrag erinnert wird.

**Manuell optimale UI**  
Ich oeffne `Mahnungen` und sehe `RE-2026-0089` mit 4.140,00 EUR offen, 12 Tage ueberfaellig. Ich waehle `Mahnstufe 1`, pruefe den Text und sende ohne Gebuehr.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Sende fuer alle Rechnungen, die mehr als 10 Tage ueberfaellig sind, Mahnstufe 1 ohne Gebuehr." CTOX prueft Zahlungseingaenge bis heute, zeigt die Versandvorschau und fuehrt nach Freigabe aus.

**Erfuellt, wenn**  
Die Mahnung ist dokumentiert, der offene Betrag bleibt 4.140,00 EUR und der Mahnlauf hat Datum, Textversion und Benutzer im Audit.

## 8. Zweite Mahnung mit Gebuehr buchen

Als Buchhalterin erstelle ich eine zweite Mahnung mit Gebuehr, damit wiederholt ueberfaellige Forderungen konsequent verfolgt werden.

**Manuell optimale UI**  
Ich filtere `Mahnstufe 1 aelter als 14 Tage` und waehle `RE-2026-0089`. Die UI schlaegt 12,00 EUR Mahngebuehr vor und zeigt Soll `1400` 12,00 EUR, Haben `8409 Mahngebuehren` 10,08 EUR, Haben `1776` 1,92 EUR.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Erstelle Mahnstufe 2 fuer REM Capital mit 12 EUR Mahngebuehr, falls noch keine Zahlung eingegangen ist." CTOX prueft offene Zahlungen, berechnet Steuer und bucht erst nach Freigabe.

**Erfuellt, wenn**  
Die Rechnung zeigt 4.152,00 EUR offen, Mahnstufe `2`, die Gebuehr ist im Ledger gebucht und der Versand dokumentiert.

## 9. Rechnung per Storno-Gutschrift stornieren

Als Buchhalterin storniere ich eine fehlerhafte Rechnung, damit die urspruengliche Buchung nicht geloescht, sondern sauber aufgehoben wird.

**Manuell optimale UI**  
Ich oeffne `RE-2026-0101` ueber 2.380,00 EUR brutto und waehle `Stornieren`. Ich erfasse den Grund `falscher Leistungszeitraum`. Die UI erzeugt `GS-2026-0012` mit Soll `8400` 2.000,00 EUR, Soll `1776` 380,00 EUR, Haben `1400` 2.380,00 EUR.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Storniere RE-2026-0101 wegen falschem Leistungszeitraum und erstelle die Storno-Gutschrift." CTOX prueft Zahlungsstatus, zeigt die Gegenbuchung und wartet auf Freigabe.

**Erfuellt, wenn**  
Die Rechnung ist `storniert`, die Gutschrift ist festgeschrieben, beide Dokumente sind verknuepft und kein Original-Ledger-Eintrag wurde veraendert.

## 10. Preisnachlass als Teilgutschrift erfassen

Als Buchhalterin erstelle ich eine Teilgutschrift fuer einen Preisnachlass, damit Umsatz, Steuer und offener Saldo korrekt reduziert werden.

**Manuell optimale UI**  
Ich oeffne `RE-2026-0098` ueber 5.950,00 EUR brutto und klicke `Gutschrift erstellen`. Ich erfasse 500,00 EUR netto Nachlass, die UI zeigt 95,00 EUR Steuerkorrektur und reduziert den offenen Saldo um 595,00 EUR.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Erstelle fuer RE-2026-0098 eine Teilgutschrift ueber 500 EUR netto wegen Lieferverzug." CTOX erkennt Rechnung, Steuerlogik und Erloskonto und fragt vor dem Buchen.

**Erfuellt, wenn**  
`GS-2026-0013` ist erstellt, der offene Saldo ist um 595,00 EUR reduziert und die Erlos- und Umsatzsteuerkorrektur steht im Ledger.

## 11. Kleinunternehmer-Rechnung ohne Umsatzsteuer erstellen

Als Kleinunternehmer erstelle ich eine Ausgangsrechnung ohne Umsatzsteuer, damit Paragraf 19 UStG korrekt ausgewiesen wird.

**Manuell optimale UI**  
Ich erstelle eine Rechnung an `Kern Design` ueber 850,00 EUR Beratung. Die UI erkennt `Kleinunternehmerregelung aktiv`, zeigt 0,00 EUR Steuer und fuegt den Pflichttext ins PDF ein.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Erstelle Kern Design eine Rechnung ueber 850 EUR Beratung als Kleinunternehmerrechnung." CTOX verhindert Steuerpositionen und zeigt Soll `1400` 850,00 EUR, Haben `8337` 850,00 EUR.

**Erfuellt, wenn**  
Die Rechnung ist gesendet, enthaelt den Paragraf-19-Hinweis und kein Steuerkonto wurde bebucht.

## 12. EU-Rechnung mit Reverse Charge erstellen

Als Buchhalterin stelle ich einem EU-Geschaeftskunden eine Reverse-Charge-Rechnung aus, damit keine deutsche Umsatzsteuer berechnet wird.

**Manuell optimale UI**  
Ich erstelle `RE-2026-0114` an `PayPal Europe S.a r.l.` ueber 1.800,00 EUR netto. Die UI setzt Steuerart `Reverse Charge`, Steuerbetrag 0,00 EUR und zeigt den Pflichttext zur Steuerschuldnerschaft des Leistungsempfaengers.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Erstelle PayPal Europe eine Reverse-Charge-Rechnung ueber 1.800 EUR fuer Integration Support." CTOX prueft EU-Land, USt-IdNr., Steuerregel und Buchung ohne Steuerkonto.

**Erfuellt, wenn**  
Die Rechnung ist gesendet, weist 0,00 EUR Umsatzsteuer aus, enthaelt den Reverse-Charge-Hinweis und der Ledger enthaelt nur Forderung und Erlos.

## 13. Eingangsbeleg per OCR erfassen

Als Buchhalterin lade ich eine Hetzner-Rechnung ueber 142,80 EUR brutto hoch und will daraus einen pruefbaren Eingangsbeleg erstellen.

**Manuell optimale UI**  
Ich oeffne `Eingangsbelege`, ziehe die PDF in die Upload-Flaeche und sehe rechts das Dokument, links die erkannten Felder: Lieferant, Rechnungsnummer `HET-2026-0517`, Datum, 120,00 EUR netto, 22,80 EUR Vorsteuer, 142,80 EUR brutto. Ich waehle `4930 Softwarekosten` und bestaetige.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Erfasse diese Hetzner-Rechnung als Eingangsbeleg und kontiere sie als Softwarekosten." CTOX extrahiert Daten, schlaegt Konto `4930` und Vorsteuerkonto `1576` vor und zeigt den Buchungssatz.

**Erfuellt, wenn**  
Der Beleg ist `geprueft`, die PDF ist verknuepft und die Buchung lautet Soll `4930` 120,00 EUR, Soll `1576` 22,80 EUR, Haben `1600` 142,80 EUR.

## 14. Neuen Lieferanten aus Rechnung anlegen

Als Buchhalterin erhalte ich erstmals eine Rechnung von Designbuero Altona und will den Lieferanten direkt aus dem Beleg anlegen.

**Manuell optimale UI**  
Nach dem Upload erkennt die Software keinen bestehenden Lieferanten. Im Pruefbereich erscheint `Neuer Lieferant erkannt`. Ich bestaetige Name, Adresse, USt-IdNr., Zahlungsziel und Standardkonto `1600`.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Lege Designbuero Altona als Lieferanten an und ordne diese Rechnung zu." CTOX prueft Dubletten nach Name, IBAN und USt-IdNr. und erstellt einen Stammdatenvorschlag.

**Erfuellt, wenn**  
Der Lieferant ist angelegt, der Beleg ist zugeordnet und zukuenftige Rechnungen mit gleicher IBAN werden vorgeschlagen.

## 15. Eingangsrechnung gegen Bestellung pruefen

Als Operations-Mitarbeiter pruefe ich eine Office-Partner-Rechnung ueber 1.487,50 EUR brutto gegen eine Bestellung ueber 25 Monitorarme.

**Manuell optimale UI**  
Ich oeffne den Beleg im Status `zu pruefen`. Die UI zeigt Rechnung, Bestellung `PO-2026-0042` und Wareneingang nebeneinander. Bestellt, geliefert und berechnet sind jeweils 25 Stueck zu 50,00 EUR netto. Ich bestaetige `sachlich geprueft`.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Pruefe diese Office-Partner-Rechnung gegen PO-2026-0042." CTOX vergleicht Menge, Einzelpreis, Lieferstatus und Steuerbetrag und schlaegt die Freigabe vor.

**Erfuellt, wenn**  
Der Beleg ist sachlich geprueft, Bestellung und Wareneingang sind referenziert und die Buchung kann vorbereitet werden.

## 16. Preisabweichung bei Eingangsrechnung klaeren

Als Buchhalterin sehe ich eine Printwerk-Rechnung ueber 714,00 EUR brutto, obwohl die Bestellung nur 595,00 EUR brutto vorsieht.

**Manuell optimale UI**  
Die Software markiert nur die Differenzzeile: bestellt 500,00 EUR netto, berechnet 600,00 EUR netto. Ich kann `klaeren`, `Differenz akzeptieren` oder `Gutschrift anfordern` waehlen und setze `Klaerung mit Lieferant`.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Warum weicht diese Printwerk-Rechnung von der Bestellung ab?" CTOX vergleicht Bestellung, Rechnung und E-Mail-Kontext und erkennt eine Expresspauschale von 100,00 EUR netto.

**Erfuellt, wenn**  
Der Beleg wird im Status `Klaerung` nicht gebucht, die Abweichung ist dokumentiert und die Rechnung erscheint als blockiert.

## 17. Mitarbeiterauslage erfassen

Als Mitarbeiterin reiche ich eine Taxiquittung ueber 38,60 EUR fuer einen Kundentermin ein, damit sie erstattet wird.

**Manuell optimale UI**  
Ich oeffne `Auslagen`, lade das Foto hoch und waehle Projekt `REM Capital`. Die Software erkennt Datum, Betrag, Zahlungsart `privat bezahlt` und schlaegt `4650 Reisekosten Arbeitnehmer` vor.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Erfasse diese Taxiquittung als Auslage fuer REM Capital." CTOX liest Betrag und Datum, erkennt Taxi, setzt Projekt und erstellt eine Erstattungsvorschau.

**Erfuellt, wenn**  
Die Auslage hat Belegfoto, Projektbezug, Status `zur Erstattung` und die spaetere Zahlung bucht Mitarbeiterverbindlichkeit gegen Bank.

## 18. Unlesbaren Beleg stoppen

Als Buchhalterin bekomme ich ein unscharfes Restaurantfoto und will verhindern, dass es ungeprueft gebucht wird.

**Manuell optimale UI**  
Die OCR zeigt `Konfidenz niedrig`, Betrag unklar und fehlende Anbieteradresse. Statt einer Buchung sehe ich `Nachbearbeitung noetig` und fordere einen besseren Beleg an.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Pruefe, ob dieser Bewirtungsbeleg buchbar ist." CTOX erkennt fehlende Pflichtangaben und schlaegt keine Buchung vor, sondern eine Rueckfrage.

**Erfuellt, wenn**  
Der Beleg bleibt ungebucht, fehlende Angaben sind sichtbar und der Vorgang liegt in `Nachbearbeitung`.

## 19. CAMT.053-Bankauszug importieren

Als Buchhalterin importiere ich den Sparkassen-Kontoauszug fuer April 2026, damit alle Bankbewegungen fuer den Abgleich bereitstehen.

**Manuell optimale UI**  
Ich oeffne `Bankimport`, waehle `camt053-april-2026.xml` und ordne `1200 Sparkasse Geschaeftskonto` zu. Die UI zeigt Zeitraum, Anfangssaldo, Endsaldo und 86 Umsaetze. Ich bestaetige den Import.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Importiere den April-Kontoauszug der Sparkasse und bereite den Abgleich vor." CTOX erkennt das Format, prueft Dubletten und importiert nur neue Umsaetze nach Freigabe.

**Erfuellt, wenn**  
Der Bankauszug ist gespeichert, Dubletten sind blockiert und neue Umsaetze stehen auf `unabgeglichen` oder `Vorschlag vorhanden`.

## 20. Eingangsrechnung per Bankzahlung abgleichen

Als Buchhalterin sehe ich eine Adobe-Abbuchung ueber 71,39 EUR und will sie mit der passenden Eingangsrechnung abgleichen.

**Manuell optimale UI**  
Ich oeffne `Zahlungen`, waehle die Bankzeile und sehe die vorgeschlagene Rechnung `ADB-2026-0441` ueber 59,99 EUR netto plus 11,40 EUR Vorsteuer. Ich bestaetige den Match.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Gleiche die Adobe-Abbuchung mit der passenden Rechnung ab." CTOX findet Betrag, Lieferant und Datum und zeigt die Zahlungsbuchung zur Freigabe.

**Erfuellt, wenn**  
Die Bankzeile ist abgeglichen, die Rechnung ist bezahlt und das Journal bucht Soll `1600`, Haben `1200`.

## 21. Teilzahlung an Lieferanten verbuchen

Als Buchhalterin bezahle ich an CloudHost GmbH 500,00 EUR auf eine offene Rechnung ueber 1.190,00 EUR brutto.

**Manuell optimale UI**  
Ich oeffne `CH-2026-0178`, klicke `Teilzahlung erfassen`, waehle Bankkonto, Datum und Betrag. Die UI zeigt danach Rest 690,00 EUR und Status `teilbezahlt`.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Buche 500 EUR Teilzahlung auf die CloudHost-Rechnung CH-2026-0178." CTOX prueft offenen Betrag und erstellt einen Zahlungsvorschlag.

**Erfuellt, wenn**  
Die Verbindlichkeit ist um 500,00 EUR reduziert, der Rest betraegt 690,00 EUR und die Zahlung ist eindeutig zugeordnet.

## 22. Sammelzahlung auf mehrere Verbindlichkeiten aufteilen

Als Buchhalterin sehe ich eine Ueberweisung ueber 2.380,00 EUR an Telekom und will sie auf drei offene Rechnungen verteilen.

**Manuell optimale UI**  
Ich oeffne die Bankzeile und sehe drei offene Telekom-Rechnungen ueber 820,00 EUR, 760,00 EUR und 800,00 EUR. Die UI summiert exakt 2.380,00 EUR. Ich bestaetige `alle zuordnen`.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Ordne die Telekom-Zahlung ueber 2.380 EUR den offenen Telekom-Rechnungen zu." CTOX sucht offene Payables desselben Lieferanten und schlaegt eine Sammelzuordnung vor.

**Erfuellt, wenn**  
Eine Zahlung mit drei Allocations existiert, alle Rechnungen sind bezahlt und die Bankbewegung ist vollstaendig abgeglichen.

## 23. Skonto bei Lieferantenzahlung beruecksichtigen

Als Buchhalterin zahle ich eine Rechnung ueber 1.190,00 EUR brutto innerhalb der Skontofrist mit 2 Prozent Skonto.

**Manuell optimale UI**  
Die Rechnung zeigt `2 Prozent Skonto bis 10.05.2026`, zahlbar 1.166,20 EUR und Skonto 23,80 EUR. Ich bestaetige die Zahlung, die Differenz wird als Skonto und Steuerkorrektur gebucht, nicht als Restschuld.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Zahle die Buerobedarf-Nord-Rechnung mit 2 Prozent Skonto." CTOX prueft Frist, berechnet Skonto und zeigt die Buchung inklusive Steuerkorrektur.

**Erfuellt, wenn**  
Die Rechnung ist bezahlt, die Zahlung betraegt 1.166,20 EUR, der Skontobetrag ist korrekt gebucht und kein Rest bleibt offen.

## 24. Doppelte Lieferantenrechnung erkennen

Als Buchhalterin lade ich eine Figma-Rechnung ueber 178,50 EUR hoch und will verhindern, dass sie zweimal bezahlt wird.

**Manuell optimale UI**  
Nach dem Upload zeigt die Software `moegliche Dublette` und vergleicht Lieferant, Rechnungsnummer, Betrag und Datum mit einem vorhandenen Beleg. Ich kann anhaengen oder verwerfen.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Pruefe, ob diese Figma-Rechnung schon existiert." CTOX findet den vorhandenen Beleg und schlaegt `als Dublette markieren` vor.

**Erfuellt, wenn**  
Keine zweite Verbindlichkeit entsteht und der Dublettenentscheid ist im Audit-Log sichtbar.

## 25. Zahlungslauf fuer Payables vorbereiten

Als Geschaeftsfuehrerin pruefe ich alle faelligen Lieferantenrechnungen bis 15.05.2026 und bereite einen Zahlungslauf vor.

**Manuell optimale UI**  
Ich oeffne `Zahlungen > Zahlungslauf`. Die UI gruppiert faellige Payables nach ueberfaellig, heute faellig, Skonto laeuft ab und spaeter faellig. Ich waehle fuenf Rechnungen aus, Gesamtsumme 5.246,80 EUR.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Bereite alle faelligen Lieferantenzahlungen bis 15. Mai vor, aber lasse Rechnungen in Klaerung aus." CTOX filtert offene Payables, ignoriert blockierte Belege und erstellt einen Zahlungslauf.

**Erfuellt, wenn**  
Der Zahlungslauf enthaelt nur freigegebene faellige Verbindlichkeiten, die Gesamtsumme stimmt und jede spaetere Bankzahlung ist abgleichbar.

## 26. Ledger-Zeile aus Rechnung pruefen

Als Buchhalterin will ich eine gebuchte Ausgangsrechnung im Ledger nachvollziehen, damit ich die betroffenen Konten sofort sehe.

**Manuell optimale UI**  
Ich oeffne `Ledger`, suche `RE2026-0042` und sehe Debitor `1400` 2.380,00 EUR im Soll, Erlos `8400` 2.000,00 EUR im Haben und `1776 Umsatzsteuer` 380,00 EUR im Haben. Jede Zeile verlinkt zu Rechnung, Kunde und PDF.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Zeig mir, wie Rechnung RE2026-0042 gebucht wurde." CTOX oeffnet die markierte Buchung und erklaert Forderung, Erlos und Steuer.

**Erfuellt, wenn**  
Soll und Haben sind ausgeglichen, die Summe stimmt mit der Rechnung ueberein und jede Ledger-Zeile ist rueckverfolgbar.

## 27. Manuelle Journalbuchung erfassen

Als Buchhalterin erfasse ich eine einmalige Bankgebuehr, damit der Aufwand korrekt gebucht wird.

**Manuell optimale UI**  
Ich oeffne `Journal`, klicke `Neue Buchung` und erfasse 12,90 EUR am 30.04.2026. Die UI zeigt Soll `4970 Nebenkosten des Geldverkehrs`, Haben `1200 Bank`. Solange Soll und Haben nicht gleich sind, bleibt `Festschreiben` deaktiviert.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Buche 12,90 EUR Bankgebuehren am 30.04.2026 von der Sparkasse." CTOX schlaegt den Buchungssatz vor und wartet auf Freigabe.

**Erfuellt, wenn**  
Das Journal ist ausgeglichen, der Aufwand steht auf `4970` und das Bankkonto ist reduziert.

## 28. SKR03-Konto fuer Produkt zuordnen

Als Administrator ordne ich einem Produkt ein SKR03-Erloskonto zu, damit Rechnungen automatisch korrekt gebucht werden.

**Manuell optimale UI**  
Ich oeffne `Produkte`, waehle `CTOX Automation Retainer` und setze `8400 Erloese 19 Prozent USt`. Die UI zeigt eine Buchungsvorschau fuer neue Rechnungen, ohne bestehende Buchungen zu aendern.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Setze beim Produkt CTOX Automation Retainer das Erloskonto auf SKR03 8400." CTOX prueft, ob `8400` bebuchbar ist, und speichert nach Freigabe.

**Erfuellt, wenn**  
Neue Rechnungspositionen verwenden `8400`, bestehende festgeschriebene Rechnungen bleiben unveraendert und die Aenderung ist auditierbar.

## 29. SKR04 als Kontenrahmen waehlen

Als Gruender waehle ich beim Setup SKR04, damit die Kontenstruktur zum Steuerberater passt.

**Manuell optimale UI**  
Ich oeffne die Buchhaltungseinrichtung, waehle `SKR04` und sehe zentrale Konten wie `1400 Forderungen`, `1800 Bank`, `3806 Umsatzsteuer 19 Prozent` und `4400 Erloese`. Die UI warnt, dass ein Wechsel nach Festschreibung eine Migration braucht.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Richte die Buchhaltung mit SKR04 fuer eine UG ein." CTOX prueft, ob Journalbuchungen existieren, und installiert nach Freigabe Konten, Steuerkonten und Nummernkreise.

**Erfuellt, wenn**  
SKR04 ist aktiv, Standardkonten sind angelegt, Steuerkonten sind verbunden und die erste Rechnung nutzt SKR04-Konten.

## 30. Umsatzsteuer auf Ausgangsrechnung pruefen

Als Buchhalterin pruefe ich die Umsatzsteuer einer Rechnung vor dem Versand, damit Pflichtfelder und Steuerbuchung stimmen.

**Manuell optimale UI**  
Ich oeffne `RE2026-DRAFT-17`: 3.000,00 EUR netto Implementierung, 570,00 EUR Steuer, 3.570,00 EUR brutto. Die UI zeigt geplantes Steuerkonto `1776` und ruhige Hinweise fuer fehlende Pflichtfelder.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Pruefe die Umsatzsteuer auf dieser Rechnung." CTOX kontrolliert Kunde, Leistungsdatum, Steuersatz, Pflichtfelder und Kontierung.

**Erfuellt, wenn**  
Die Rechnung kann nur mit gueltiger Steuerlogik gesendet werden und nach Versand entstehen Forderung, Erlos und Umsatzsteuer im Ledger.

## 31. UStVA fuer April erstellen

Als Buchhalterin bereite ich die Umsatzsteuer-Voranmeldung fuer April 2026 vor, damit die Zahllast aus Steuerkonten nachvollziehbar ist.

**Manuell optimale UI**  
Ich oeffne `Buchhaltung > UStVA`, waehle April 2026 und sehe Umsatzsteuer `1776` 8.740,00 EUR, Vorsteuer `1576` 2.115,20 EUR und Zahllast 6.624,80 EUR. Details sind einklappbar.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Bereite die UStVA fuer April 2026 vor und zeig mir Ausreisser." CTOX berechnet die Zahllast und markiert Belege mit ungewoehnlich hoher Vorsteuer.

**Erfuellt, wenn**  
Die UStVA basiert nur auf festgeschriebenen Ledgerbuchungen, die Zahllast ist nachvollziehbar und der Status kann auf `geprueft` gesetzt werden.

## 32. DATEV-Export erzeugen

Als Buchhalterin erzeuge ich einen DATEV-Export fuer April 2026, damit der Steuerberater den Buchungsstapel importieren kann.

**Manuell optimale UI**  
Ich oeffne `Buchhaltung > DATEV`, waehle 01.04. bis 30.04.2026 und sehe 184 Buchungssaetze `exportbereit`. Beraternummer, Mandantennummer, Wirtschaftsjahresbeginn und Kontenlaenge sind kompakt sichtbar. Ich klicke `EXTF erzeugen`.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Erzeuge den DATEV-Export fuer April und pruefe vorher fehlende Beleglinks." CTOX findet Blocker, zeigt sie als Aufgaben und exportiert erst nach Freigabe.

**Erfuellt, wenn**  
Die EXTF-Datei ist erzeugt, alle Buchungen sind festgeschrieben und der Exportlauf hat Zeitraum, Hash und Benutzer im Audit.

## 33. Bilanz aus Ledger ableiten

Als Geschaeftsfuehrer sehe ich die Bilanz zum 30.04.2026, damit ich Vermoegen, Schulden und Eigenkapital aus echten Buchungen beurteilen kann.

**Manuell optimale UI**  
Ich oeffne `Berichte > Bilanz`. Oben steht die Kernaussage: Aktiva 128.420,50 EUR, Passiva 128.420,50 EUR, Differenz 0,00 EUR. Darunter sind Anlagevermoegen, Bank, Forderungen, Verbindlichkeiten, Steuerzahllast und Eigenkapital verdichtet.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Erklaere mir die Bilanz zum 30.04.2026 und zeige, warum sie aufgeht." CTOX prueft Aktiva gegen Passiva und verlinkt die groessten Bewegungen.

**Erfuellt, wenn**  
Aktiva und Passiva sind gleich, jede Position kommt aus Ledgerkonten und jede Summe ist bis zur Journalbuchung verfolgbar.

## 34. GuV fuer den Monat pruefen

Als Geschaeftsfuehrer pruefe ich die GuV fuer April 2026, damit ich Umsatz, Aufwand und Ergebnis verstehe.

**Manuell optimale UI**  
Ich oeffne `Berichte > GuV` und sehe zuerst Umsatz 42.300,00 EUR, Aufwand 31.870,40 EUR und Gewinn 10.429,60 EUR. Kontengruppen stehen darunter ruhiger und koennen aufgeklappt werden.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Warum ist der Gewinn im April niedriger als im Maerz?" CTOX vergleicht Monate und erklaert Abschreibungen und hoehere Softwarekosten mit Ledgerlinks.

**Erfuellt, wenn**  
Die GuV basiert auf Einnahmen- und Aufwandskonten, stimmt mit dem Ledger ueberein und Abweichungen sind belegbar.

## 35. BWA fuer Geschaeftsueberblick erzeugen

Als Geschaeftsfuehrer sehe ich eine BWA fuer Januar bis April, damit ich die Entwicklung ohne Steuerberaterexport beurteilen kann.

**Manuell optimale UI**  
Ich oeffne `Berichte > BWA`, waehle Januar bis April 2026 und sehe Gesamtleistung 168.900,00 EUR, Kosten 124.730,80 EUR und Ergebnis 44.169,20 EUR. Die Tabelle zeigt Monate nebeneinander und kumuliert.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Erstelle eine kurze BWA-Auswertung Januar bis April und markiere Kostenanstiege." CTOX analysiert Reisekosten, Softwarekosten und Erlostrends.

**Erfuellt, wenn**  
Die BWA nutzt dieselben Ledgerdaten wie GuV und Bilanz und CTOX kann Abweichungen mit Buchungsbelegen belegen.

## 36. Periode abschliessen

Als Buchhalterin schliesse ich April 2026 ab, damit nach UStVA und DATEV keine stillen Aenderungen mehr moeglich sind.

**Manuell optimale UI**  
Ich oeffne `Buchhaltung > Perioden`, waehle April und sehe: UStVA geprueft, DATEV exportiert, alle Journalbuchungen ausgeglichen, 0 Entwuerfe mit April-Datum. Ich klicke `Periode schliessen`.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Pruefe, ob April abgeschlossen werden kann." CTOX kontrolliert Entwurfsrechnungen, ungebuchte Belege, Bankabgleich und Exporte und schlaegt den Abschluss vor.

**Erfuellt, wenn**  
April ist `geschlossen`, neue Buchungen mit April-Datum sind blockiert und Korrekturen laufen nur per Storno oder Korrekturbuchung.

## 37. GoBD-Korrektur per Storno buchen

Als Buchhalterin korrigiere ich eine falsch gebuchte Rechnung GoBD-konform, damit keine festgeschriebene Buchung veraendert wird.

**Manuell optimale UI**  
Ich oeffne `RE2026-0031`, sehe `festgeschrieben` und kann Betraege nicht direkt editieren. Ich waehle `Storno erstellen`, die Software erzeugt eine Gegenbuchung ueber 1.190,00 EUR und danach eine neue korrigierte Rechnung.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Die Rechnung RE2026-0031 wurde falsch besteuert. Bitte korrekt stornieren und neu vorbereiten." CTOX schlaegt Storno plus neue Rechnung vor und zeigt beide Auswirkungen.

**Erfuellt, wenn**  
Die Originalbuchung bleibt unveraendert, der Storno verweist auf das Original und die neue Rechnung hat eine eigene Nummer.

## 38. Offene Postenliste pruefen

Als Buchhalterin pruefe ich offene Debitoren und Kreditoren, damit ich Liquiditaet und Handlungsbedarf erkenne.

**Manuell optimale UI**  
Ich oeffne `Buchhaltung > Offene Posten`. Die UI trennt Forderungen und Verbindlichkeiten, zeigt Faelligkeit, Restbetrag, Kontakt und naechste Aktion. Ueberfaellige Posten sind sortiert, nicht visuell ueberladen.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Zeige mir alle offenen Posten mit Risiko bis Ende Mai." CTOX priorisiert ueberfaellige Kunden, Skontofristen und blockierte Lieferantenrechnungen.

**Erfuellt, wenn**  
Jeder offene Posten ist mit Rechnung, Zahlungshistorie und naechster Aktion verknuepft und die Summen stimmen mit Forderungs- und Verbindlichkeitskonten ueberein.

## 39. Laptop als Anlage aktivieren

Als Buchhalterin erfasse ich einen MacBook-Kauf ueber 2.379,00 EUR brutto als Anlage, damit Anschaffung, Vorsteuer und Abschreibung korrekt in Bilanz und GuV landen.

**Manuell optimale UI**  
Ich oeffne den Apple-Eingangsbeleg und klicke `Als Anlage aktivieren`. Die UI schlaegt `0480 Betriebs- und Geschaeftsausstattung`, `1576 Vorsteuer` und 3 Jahre Nutzungsdauer vor. Ich sehe Aktivierungssatz und AfA-Plan.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Aktiviere den Apple-Beleg vom 15.05.2026 als Laptop ueber 3 Jahre." CTOX erkennt Betrag, Vorsteuer, Lieferant und Anlagenklasse und prueft Doppelaktivierung.

**Erfuellt, wenn**  
Die Anlage ist `aktiv`, der Beleg ist verknuepft, 1.999,16 EUR stehen auf `0480`, 379,84 EUR auf `1576` und der AfA-Plan ist auditierbar.

## 40. Monatliche Abschreibung buchen

Als Buchhalter buche ich die faellige AfA fuer Mai 2026, damit die GuV Aufwand und die Bilanz reduzierte Buchwerte zeigt.

**Manuell optimale UI**  
Ich oeffne `Anlagen`, waehle 2026 und sehe `AfA faellig: 812,50 EUR`. Ich klicke `AfA buchen`, pruefe die vorgeschlagenen Anlagen und bestaetige den Buchungslauf.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Buche alle faelligen Abschreibungen fuer Mai 2026, aber ueberspringe Anlagen ohne Beleg." CTOX prueft Perioden, Belege und bestehende AfA-Buchungen.

**Erfuellt, wenn**  
Der Journal Entry ist festgeschrieben, `4830 Abschreibungen` ist mit 812,50 EUR belastet und keine Anlage wurde doppelt abgeschrieben.

## 41. Fahrzeugleasing mit Sonderzahlung erfassen

Als Geschaeftsfuehrer erfasse ich ein Fahrzeugleasing mit 5.950,00 EUR Sonderzahlung, damit Anlage, Aufwand und Verbindlichkeit nachvollziehbar sind.

**Manuell optimale UI**  
Ich oeffne die Leasingrechnung, markiere `Fahrzeug / Leasing` und trenne Sonderzahlung, Vorsteuer und monatliche Rate. Die UI bucht 5.000,00 EUR netto auf `0485 Fuhrpark`, 950,00 EUR auf `1576` und die Verbindlichkeit auf den Lieferanten.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Erfasse die VW-Leasing-Sonderzahlung als Fahrzeuganlage, netto 5.000 EUR, Vorsteuer 950 EUR." CTOX prueft Anlage versus Leasingaufwand und verlangt Freigabe.

**Erfuellt, wenn**  
Der Beleg ist gebucht, der Fuhrpark-Buchwert enthaelt 5.000,00 EUR und spaetere Monatsraten werden nicht automatisch aktiviert.

## 42. Darlehensaufnahme buchen

Als Unternehmer nehme ich ein Bankdarlehen ueber 50.000,00 EUR auf, damit Auszahlung, Darlehenskonto und Tilgungsplan korrekt abgebildet sind.

**Manuell optimale UI**  
Ich oeffne `Buchhaltung > Darlehen`, erfasse Sparkasse Hamburg, Auszahlung 50.000,00 EUR, Start 01.06.2026, Zinssatz 5,2 Prozent und Rate 1.500,00 EUR. Die UI zeigt Soll `1200 Bank`, Haben `0650 Darlehen`.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Lege das Sparkassen-Darlehen ueber 50.000 EUR ab 01.06.2026 mit 5,2 Prozent Zins und 1.500 EUR Monatsrate an." CTOX berechnet Plan und zeigt die erste Buchung.

**Erfuellt, wenn**  
Bank und Darlehenskonto zeigen 50.000,00 EUR, der Tilgungsplan ist sichtbar und spaetere Zahlungen werden in Zins und Tilgung getrennt.

## 43. Darlehensrate aufteilen

Als Buchhalter ordne ich eine Bankabbuchung ueber 1.500,00 EUR einer Darlehensrate zu, damit Zins und Tilgung getrennt im Ledger stehen.

**Manuell optimale UI**  
Ich oeffne die Bankzeile der Sparkasse und waehle `Darlehensrate zuordnen`. Die UI schlaegt 216,67 EUR auf `2110 Zinsaufwand` und 1.283,33 EUR auf `0650 Darlehen` vor.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Ordne die Sparkassen-Abbuchung vom 30.06.2026 dem Darlehen zu." CTOX findet das Darlehen, liest den Plan und wartet auf Freigabe.

**Erfuellt, wenn**  
Die Zahlung ist zugeordnet, der Darlehenssaldo sinkt um 1.283,33 EUR und 216,67 EUR Zinsaufwand sind gebucht.

## 44. Reisekostenabrechnung buchen

Als Mitarbeiter reiche ich eine Berlin-Reise ueber 438,60 EUR ein, damit Bahn, Hotel und Verpflegung korrekt geprueft und erstattet werden.

**Manuell optimale UI**  
Ich oeffne `Reisekosten`, lade drei Belege hoch und erfasse Projekt `CTOX Onboarding Berlin`. Die UI gruppiert Bahn, Hotel mit 7 Prozent Vorsteuer und Verpflegungspauschale ohne Vorsteuer.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Erstelle aus diesen drei Belegen eine Reisekostenabrechnung fuer CTOX Onboarding Berlin." CTOX extrahiert Datum, Ort, Betrag, Steuersatz und Zahlungsart.

**Erfuellt, wenn**  
Die Abrechnung ist freigegeben, das Projekt ist gesetzt, Vorsteuer ist nur auf zulaessigen Belegen gebucht und die Erstattung ist offen.

## 45. Kostenstelle fuer Projektrechnung setzen

Als Projektmanager buche ich eine Eingangsrechnung ueber 4.760,00 EUR brutto auf `REM Capital Rollout`, damit die Projektrentabilitaet stimmt.

**Manuell optimale UI**  
Ich oeffne den Netlify-Beleg, setze Projekt `REM Capital Rollout` und sehe 4.000,00 EUR netto, 760,00 EUR Vorsteuer und Konto `4950 Softwarelizenzen`.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Buche die Netlify-Rechnung auf REM Capital Rollout." CTOX erkennt Projektbezug aus Belegtext und E-Mail-Kontext und schlaegt Konto und Dimension vor.

**Erfuellt, wenn**  
Der Aufwand steht auf `4950`, die Projektdimension ist gesetzt, `1576` enthaelt die Vorsteuer und der Projektbericht zeigt den Aufwand.

## 46. Reverse-Charge-Eingangsrechnung buchen

Als Buchhalterin erfasse ich eine Stripe-Rechnung ueber 1.200,00 EUR netto aus Irland, damit Reverse Charge korrekt gebucht wird.

**Manuell optimale UI**  
Ich oeffne den Beleg und waehle `EU-Leistung Reverse Charge`. Die UI zeigt 1.200,00 EUR Aufwand, 228,00 EUR Umsatzsteuer Reverse Charge und 228,00 EUR Vorsteuer, aber Zahlbetrag 1.200,00 EUR.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Buche die Stripe-Rechnung als Reverse Charge." CTOX prueft EU-Lieferant, USt-IdNr., Leistungsort und fehlende Umsatzsteuer auf dem Beleg.

**Erfuellt, wenn**  
Umsatzsteuer und Vorsteuer sind spiegelbildlich gebucht, der Zahlbetrag bleibt 1.200,00 EUR und die UStVA enthaelt die Reverse-Charge-Werte.

## 47. Wiederkehrende Miete buchen

Als Buchhalter richte ich die monatliche Bueromiete ueber 2.380,00 EUR brutto ein, damit der Aufwand jeden Monat vorbereitet wird.

**Manuell optimale UI**  
Ich oeffne `Wiederkehrende Buchungen`, erfasse Vermieter, 2.000,00 EUR netto, 380,00 EUR Vorsteuer, Konto `4210 Miete` und Faelligkeit am dritten Werktag. Die naechsten Buchungen bleiben `wartet auf Freigabe`.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Richte die Bueromiete an Alster Offices monatlich ueber 2.380 EUR brutto ab Juni 2026 ein." CTOX schlaegt Konto, Steuer und Rhythmus vor.

**Erfuellt, wenn**  
Fuer Juni existiert eine Buchungsvorschau, keine automatische Festschreibung ohne Freigabe erfolgt und jede Monatsbuchung auditierbar bleibt.

## 48. Steuerberater-Uebergabe vorbereiten

Als Geschaeftsfuehrer uebergebe ich April 2026 an den Steuerberater, damit DATEV, Belege und Pruefpunkte vollstaendig exportiert werden.

**Manuell optimale UI**  
Ich oeffne `Buchhaltung > Steuerberater`, waehle April und sehe 126 Buchungen festgeschrieben, 4 Belege unklar, 2 Zahlungen ungeklärt. Nach Bereinigung klicke ich `DATEV-Paket erstellen`.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Bereite die Steuerberater-Uebergabe fuer April 2026 vor und zeige mir nur Blocker." CTOX prueft Ledger, Belege, DATEV-Konten, UStVA und Bankabstimmung.

**Erfuellt, wenn**  
April ist `uebergabebereit`, EXTF, Belegliste und Pruefprotokoll sind erzeugt und ungeklärte Zahlungen blockieren die Uebergabe.

## 49. Agentischen Monatsabschluss vorbereiten

Als Unternehmer lasse ich CTOX den Monatsabschluss Mai 2026 vorbereiten, damit ich nur Ausnahmen pruefe und kontrolliert abschliessen kann.

**Manuell optimale UI**  
Ich oeffne `Monatsabschluss` und sehe eine Arbeitsliste: Bank abstimmen, Eingangsbelege pruefen, AfA buchen, wiederkehrende Buchungen freigeben, UStVA pruefen. Ich arbeite Punkte ab, bis der Monat `abschlussbereit` ist.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Bereite den Monatsabschluss Mai 2026 vor, buche nichts ohne Freigabe und zeige mir alle Risiken." CTOX importiert Bankbewegungen, schlaegt Zuordnungen vor, erkennt fehlende Belege, berechnet AfA und prueft Steuerkonten.

**Erfuellt, wenn**  
Mai 2026 ist `abschlussbereit`, alle CTOX-Vorschlaege haben Freigabehistorie und Bilanz, GuV, UStVA und Ledger zeigen denselben Datenstand.

## 50. CTOX-Arbeitsauftrag mit menschlicher Freigabe ausfuehren

Als Unternehmer gebe ich CTOX einen breiten Buchhaltungsauftrag, damit Routinearbeit vorbereitet wird, ohne dass Buchungen unkontrolliert festgeschrieben werden.

**Manuell optimale UI**  
Ich oeffne `Buchhaltung > CTOX Vorschlaege` und sehe vorbereitete Arbeitsauftraege nach Risiko: sichere Zuordnungen, pruefpflichtige Belege, blockierte Faelle. Ich kann jeden Vorschlag oeffnen, Buchungssatz und Beleg sehen, einzeln freigeben oder ablehnen.

**KI-gestuetzt ueber CTOX**  
Ich schreibe: "Pruefe alle offenen Belege und Zahlungen der letzten Woche, bereite Buchungen vor, aber buche nichts ohne meine Freigabe." CTOX erstellt Vorschlaege mit Beleg, Konto, Betrag, Begruendung und Risiko. Ich bestaetige gesammelt nur die sicheren Vorschlaege.

**Erfuellt, wenn**  
Keine Buchung wurde ohne Freigabe festgeschrieben, jeder Vorschlag hat Kontext und Begruendung, abgelehnte Vorschlaege bleiben nachvollziehbar und freigegebene Buchungen laufen durch dieselbe Accounting Engine wie manuelle Aktionen.
