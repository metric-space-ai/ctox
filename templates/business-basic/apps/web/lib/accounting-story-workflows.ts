import {
  createAccountingAuditEvent,
  createAccountingCommand,
  createAccountingProposal,
  createBusinessOutboxEvent
} from "@ctox-business/accounting";
import type { AccountingCommandType, AccountingProposal } from "@ctox-business/accounting";
import type { AccountingJournalDraft } from "@ctox-business/db/accounting";
import type { BusinessBundle, SupportedLocale } from "./business-seed";

export type AccountingStoryWorkflow = {
  acceptance: { de: string; en: string };
  ctoxPrompt: { de: string; en: string };
  id: string;
  manual: { de: string; en: string };
  primaryAction: { de: string; en: string };
  submoduleId: "bookkeeping" | "fixed-assets" | "invoices" | "ledger" | "payments" | "receipts" | "reports";
  title: { de: string; en: string };
};

const storyRows = [
  ["01", "invoices", "Angebot fuer Projektpaket erstellen", "Quote project package", "Angebot anlegen, Positionen, Steuer und Zahlungsziel pruefen.", "Create a quote, review lines, tax, and terms.", "Erstelle REM Capital ein Angebot fuer CTOX Business Setup 4.800 netto und Workshop 1.200 netto.", "Create a quote for REM Capital: CTOX Business Setup 4,800 net and workshop 1,200 net.", "Angebot ist gesendet, enthaelt Bruttosumme und erzeugt noch keine Ledger-Buchung.", "Quote is sent, contains gross total, and has no ledger posting.", "Angebot vorbereiten", "Prepare quote"],
  ["02", "invoices", "Angebot in Ausgangsrechnung uebernehmen", "Convert quote to invoice", "Angenommenes Angebot oeffnen, Rechnung erzeugen, Buchungsvorschau pruefen.", "Open accepted quote, create invoice, review posting preview.", "Wandle ANG-2026-0042 in eine Rechnung um und sende sie heute.", "Convert quote ANG-2026-0042 to an invoice and send it today.", "Rechnung ist gesendet, Angebot ist abgerechnet, Forderung ist gebucht.", "Invoice is sent, quote is billed, receivable is posted.", "Rechnung erzeugen", "Create invoice"],
  ["03", "invoices", "Geschaeftskunden mit Steuerdaten anlegen", "Create business customer with tax data", "Kundenstamm mit Adresse, USt-IdNr., Zahlungsziel und Standardkonten anlegen.", "Create customer with address, VAT id, terms, and default accounts.", "Lege Tank Technology GmbH als deutschen B2B-Kunden mit 14 Tagen Zahlungsziel an.", "Create Tank Technology GmbH as German B2B customer with 14 day terms.", "Kunde ist aktiv und neue Rechnungen nutzen die Standardkonten.", "Customer is active and new invoices use default accounts.", "Kunde anlegen", "Create customer"],
  ["04", "invoices", "SaaS-Abo als Ausgangsrechnung senden", "Send SaaS subscription invoice", "Rechnung mit Produkt, Steuer, PDF und Sendefreigabe erstellen.", "Create invoice with product, tax, PDF, and send approval.", "Erstelle die Mai-Rechnung fuer Tank Technology ueber 980 EUR netto und sende sie.", "Create and send the May invoice for Tank Technology for 980 net.", "Rechnung ist gesendet und Forderung, Erlos, Umsatzsteuer sind im Ledger.", "Invoice is sent and receivable, revenue, VAT are in the ledger.", "Rechnung senden", "Send invoice"],
  ["05", "payments", "Zahlung einer offenen Rechnung zuordnen", "Match payment to open invoice", "Bankzeile mit Rechnungsnummer bestaetigen und Zahlungsbuchung pruefen.", "Confirm bank line with invoice number and review payment posting.", "Ordne den Bankeingang 1.166,20 von Tank Technology der Rechnung RE-2026-0094 zu.", "Match the 1,166.20 payment from Tank Technology to RE-2026-0094.", "Rechnung ist bezahlt, Bankzeile und Journal Entry sind verknuepft.", "Invoice is paid, bank line and journal entry are linked.", "Match bestaetigen", "Confirm match"],
  ["06", "payments", "Teilzahlung auf Rechnung verbuchen", "Post partial customer payment", "Bankeingang kleiner als Restbetrag als Teilzahlung bestaetigen.", "Confirm bank receipt below balance as partial payment.", "Buche 3.000 EUR von REM Capital als Teilzahlung auf RE-2026-0089.", "Post 3,000 from REM Capital as partial payment on RE-2026-0089.", "Rechnung ist teilweise bezahlt und der Rest bleibt offen.", "Invoice is partially paid and the balance remains open.", "Teilzahlung buchen", "Post partial payment"],
  ["07", "reports", "Erste Mahnung senden", "Send first dunning notice", "Ueberfaellige Rechnung waehlen, Mahnstufe 1 pruefen und versenden.", "Select overdue invoice, review level 1 notice, and send.", "Sende fuer Rechnungen ueber 10 Tage ueberfaellig Mahnstufe 1 ohne Gebuehr.", "Send level 1 dunning without fee for invoices overdue more than 10 days.", "Mahnung ist dokumentiert und offener Betrag bleibt unveraendert.", "Dunning notice is documented and open amount is unchanged.", "Mahnung vorbereiten", "Prepare dunning"],
  ["08", "reports", "Zweite Mahnung mit Gebuehr buchen", "Post second dunning notice with fee", "Mahnstufe 2 mit Gebuehr und Steuerbuchung pruefen.", "Review level 2 dunning with fee and tax posting.", "Erstelle Mahnstufe 2 fuer REM Capital mit 12 EUR Mahngebuehr.", "Create level 2 dunning for REM Capital with 12 fee.", "Gebuehr ist im Ledger gebucht und Versand ist dokumentiert.", "Fee is posted in the ledger and delivery is documented.", "Gebuehr pruefen", "Review fee"],
  ["09", "invoices", "Rechnung per Storno-Gutschrift stornieren", "Cancel invoice with reversal credit note", "Festgeschriebene Rechnung per Storno-Gutschrift korrigieren.", "Correct posted invoice through reversal credit note.", "Storniere RE-2026-0101 wegen falschem Leistungszeitraum.", "Cancel RE-2026-0101 because service period is wrong.", "Original bleibt unveraendert, Storno verweist auf Original.", "Original stays unchanged, reversal points to original.", "Storno erstellen", "Create reversal"],
  ["10", "invoices", "Preisnachlass als Teilgutschrift erfassen", "Record discount credit note", "Teilbetrag, Steuerkorrektur und neuen offenen Saldo pruefen.", "Review partial amount, VAT correction, and new balance.", "Erstelle fuer RE-2026-0098 eine Teilgutschrift ueber 500 EUR netto.", "Create a 500 net partial credit note for RE-2026-0098.", "Offener Saldo, Erlos und Umsatzsteuer sind korrigiert.", "Open balance, revenue, and VAT are corrected.", "Gutschrift erstellen", "Create credit note"],
  ["11", "invoices", "Kleinunternehmer-Rechnung ohne Umsatzsteuer erstellen", "Create small-business invoice without VAT", "Rechnung ohne Steuerzeile mit Pflichttext senden.", "Send invoice without tax line and with required note.", "Erstelle Kern Design eine Rechnung ueber 850 EUR als Kleinunternehmerrechnung.", "Create an 850 small-business invoice for Kern Design.", "Kein Steuerkonto ist bebucht und Pflichttext ist im PDF.", "No tax account is posted and the required note is in the PDF.", "Ohne USt senden", "Send without VAT"],
  ["12", "invoices", "EU-Rechnung mit Reverse Charge erstellen", "Create EU reverse-charge invoice", "EU-Kunde, USt-IdNr., 0 EUR Steuer und Pflichttext pruefen.", "Review EU customer, VAT id, zero VAT, and required note.", "Erstelle PayPal Europe eine Reverse-Charge-Rechnung ueber 1.800 EUR.", "Create a 1,800 reverse-charge invoice for PayPal Europe.", "Ledger enthaelt Forderung und Erlos ohne Umsatzsteuerkonto.", "Ledger contains receivable and revenue without VAT account.", "Reverse Charge senden", "Send reverse charge"],
  ["13", "receipts", "Eingangsbeleg per OCR erfassen", "Capture inbound receipt by OCR", "PDF hochladen, erkannte Felder pruefen und Konto waehlen.", "Upload PDF, review extracted fields, and choose account.", "Erfasse diese Hetzner-Rechnung als Eingangsbeleg und kontiere Softwarekosten.", "Capture this Hetzner invoice as inbound receipt and code software cost.", "Beleg ist geprueft, Datei verknuepft und Buchungsvorschlag vorhanden.", "Receipt is reviewed, file linked, and posting proposal exists.", "OCR pruefen", "Review OCR"],
  ["14", "receipts", "Neuen Lieferanten aus Rechnung anlegen", "Create vendor from receipt", "Dubletten pruefen, Lieferantendaten bestaetigen und Beleg zuordnen.", "Check duplicates, confirm vendor data, and assign receipt.", "Lege Designbuero Altona als Lieferanten an und ordne diese Rechnung zu.", "Create Designbuero Altona as vendor and assign this invoice.", "Lieferant ist aktiv und Folgebelege werden vorgeschlagen.", "Vendor is active and later receipts are suggested.", "Lieferant anlegen", "Create vendor"],
  ["15", "receipts", "Eingangsrechnung gegen Bestellung pruefen", "Match supplier invoice to purchase order", "Rechnung, Bestellung und Wareneingang nebeneinander pruefen.", "Review invoice, purchase order, and receipt side by side.", "Pruefe diese Office-Partner-Rechnung gegen PO-2026-0042.", "Check this Office Partner invoice against PO-2026-0042.", "Beleg ist sachlich geprueft und buchbar.", "Receipt is approved and ready to post.", "Bestellung pruefen", "Check PO"],
  ["16", "receipts", "Preisabweichung bei Eingangsrechnung klaeren", "Resolve supplier invoice variance", "Differenzzeile pruefen und Klaerung oder Gutschrift anfordern.", "Review variance line and request clarification or credit note.", "Warum weicht diese Printwerk-Rechnung von der Bestellung ab?", "Why does this Printwerk invoice differ from the purchase order?", "Beleg ist blockiert, bis die Abweichung entschieden ist.", "Receipt is blocked until variance is decided.", "Abweichung klaeren", "Resolve variance"],
  ["17", "receipts", "Mitarbeiterauslage erfassen", "Submit employee expense", "Belegfoto, Projekt, Betrag und Erstattungsstatus pruefen.", "Review photo, project, amount, and reimbursement state.", "Erfasse diese Taxiquittung als Auslage fuer REM Capital.", "Capture this taxi receipt as expense for REM Capital.", "Auslage ist zur Erstattung offen und projektbezogen.", "Expense is open for reimbursement and linked to project.", "Auslage erfassen", "Capture expense"],
  ["18", "receipts", "Unlesbaren Beleg stoppen", "Stop unreadable receipt", "Niedrige OCR-Konfidenz erkennen und Rueckfrage senden.", "Detect low OCR confidence and send clarification request.", "Pruefe, ob dieser Bewirtungsbeleg buchbar ist.", "Check whether this entertainment receipt can be posted.", "Beleg bleibt ungebucht und liegt in Nachbearbeitung.", "Receipt stays unposted and waits for cleanup.", "Rueckfrage senden", "Request clarification"],
  ["19", "bookkeeping", "CAMT.053-Bankauszug importieren", "Import CAMT.053 statement", "Datei, Konto, Salden und Dubletten vor Import pruefen.", "Review file, account, balances, and duplicates before import.", "Importiere den April-Kontoauszug der Sparkasse und bereite den Abgleich vor.", "Import the April Sparkasse statement and prepare reconciliation.", "Neue Zeilen stehen zum Abgleich bereit und Dubletten sind blockiert.", "New lines are ready for reconciliation and duplicates are blocked.", "Bankimport starten", "Start bank import"],
  ["20", "payments", "Eingangsrechnung per Bankzahlung abgleichen", "Match vendor invoice by bank payment", "Abbuchung mit offener Lieferantenrechnung bestaetigen.", "Confirm debit against open vendor invoice.", "Gleiche die Adobe-Abbuchung mit der passenden Rechnung ab.", "Reconcile the Adobe debit with the matching invoice.", "Bankzeile ist abgeglichen und Lieferantenrechnung bezahlt.", "Bank line is matched and vendor invoice is paid.", "Zahlung matchen", "Match payment"],
  ["21", "payments", "Teilzahlung an Lieferanten verbuchen", "Post partial supplier payment", "Teilzahlung auf offene Verbindlichkeit erfassen.", "Record partial payment on open payable.", "Buche 500 EUR Teilzahlung auf CH-2026-0178.", "Post 500 partial payment on CH-2026-0178.", "Restbetrag bleibt offen und Zahlung ist zugeordnet.", "Balance remains open and payment is allocated.", "Teilzahlung erfassen", "Record partial"],
  ["22", "payments", "Sammelzahlung auf mehrere Verbindlichkeiten aufteilen", "Split supplier batch payment", "Bankzeile auf mehrere offene Rechnungen verteilen.", "Allocate bank line across several open invoices.", "Ordne die Telekom-Zahlung ueber 2.380 EUR den offenen Telekom-Rechnungen zu.", "Allocate the 2,380 Telekom payment to open Telekom invoices.", "Alle Allocations sind gebucht und Bankzeile ist erledigt.", "All allocations are posted and bank line is cleared.", "Sammelzahlung aufteilen", "Split payment"],
  ["23", "payments", "Skonto bei Lieferantenzahlung beruecksichtigen", "Apply supplier cash discount", "Skontofrist, Zahlbetrag und Steuerkorrektur pruefen.", "Review discount period, payment amount, and VAT correction.", "Zahle die Buerobedarf-Nord-Rechnung mit 2 Prozent Skonto.", "Pay the Buerobedarf Nord invoice with 2 percent cash discount.", "Rechnung ist bezahlt und kein Rest bleibt offen.", "Invoice is paid and no balance remains.", "Skonto anwenden", "Apply discount"],
  ["24", "receipts", "Doppelte Lieferantenrechnung erkennen", "Detect duplicate supplier invoice", "Lieferant, Nummer, Betrag und Datum mit Bestand vergleichen.", "Compare vendor, number, amount, and date with existing receipts.", "Pruefe, ob diese Figma-Rechnung schon existiert.", "Check whether this Figma invoice already exists.", "Keine zweite Verbindlichkeit entsteht.", "No second payable is created.", "Dublette pruefen", "Check duplicate"],
  ["25", "payments", "Zahlungslauf fuer Payables vorbereiten", "Prepare payables payment run", "Faellige freigegebene Verbindlichkeiten auswaehlen.", "Select due approved payables.", "Bereite alle faelligen Lieferantenzahlungen bis 15. Mai vor, aber lasse Klaerung aus.", "Prepare due vendor payments until May 15 but exclude blocked items.", "Zahlungslauf enthaelt nur freigegebene Payables.", "Payment run contains only approved payables.", "Zahlungslauf vorbereiten", "Prepare payment run"],
  ["26", "ledger", "Ledger-Zeile aus Rechnung pruefen", "Trace invoice ledger line", "Rechnung suchen und Soll/Haben-Zeilen nachvollziehen.", "Find invoice and inspect debit/credit lines.", "Zeig mir, wie Rechnung RE2026-0042 gebucht wurde.", "Show how invoice RE2026-0042 was posted.", "Soll und Haben sind ausgeglichen und verlinkt.", "Debit and credit are balanced and linked.", "Ledger oeffnen", "Open ledger"],
  ["27", "ledger", "Manuelle Journalbuchung erfassen", "Create manual journal entry", "Zwei ausgeglichene Journalzeilen erfassen.", "Enter two balanced journal lines.", "Buche 12,90 EUR Bankgebuehren am 30.04.2026 von der Sparkasse.", "Post 12.90 bank fees on 2026-04-30 from Sparkasse.", "Journal ist ausgeglichen und Bankgebuehr ist gebucht.", "Journal is balanced and bank fee is posted.", "Journal erfassen", "Create journal"],
  ["28", "ledger", "SKR03-Konto fuer Produkt zuordnen", "Assign SKR03 product account", "Produktkonto und Steuer-Vorschau speichern.", "Save product account and tax preview.", "Setze beim Produkt CTOX Automation Retainer das Erloskonto auf SKR03 8400.", "Set product CTOX Automation Retainer revenue account to SKR03 8400.", "Neue Rechnungen nutzen das Konto, alte bleiben unveraendert.", "New invoices use the account, old ones stay unchanged.", "Konto setzen", "Set account"],
  ["29", "ledger", "SKR04 als Kontenrahmen waehlen", "Choose SKR04 chart", "Kontenrahmen vor erster Festschreibung auswaehlen.", "Choose chart before first posting is locked.", "Richte die Buchhaltung mit SKR04 fuer eine UG ein.", "Set up accounting with SKR04 for a UG.", "SKR04-Konten und Steuerkonten sind installiert.", "SKR04 accounts and tax accounts are installed.", "SKR04 vorbereiten", "Prepare SKR04"],
  ["30", "invoices", "Umsatzsteuer auf Ausgangsrechnung pruefen", "Check invoice VAT", "Pflichtfelder, Steuersatz und Steuerkonto vor Versand pruefen.", "Check required fields, tax rate, and tax account before send.", "Pruefe die Umsatzsteuer auf dieser Rechnung.", "Check VAT on this invoice.", "Rechnung kann nur mit gueltiger Steuerlogik gesendet werden.", "Invoice can only be sent with valid tax logic.", "Steuer pruefen", "Check tax"],
  ["31", "reports", "UStVA fuer April erstellen", "Create April VAT statement", "Steuerkonten zusammenfassen und Zahllast pruefen.", "Summarize tax accounts and review payable.", "Bereite die UStVA fuer April 2026 vor und zeig mir Ausreisser.", "Prepare April 2026 VAT statement and show outliers.", "UStVA basiert auf festgeschriebenem Ledger.", "VAT statement is based on posted ledger.", "UStVA pruefen", "Check VAT return"],
  ["32", "bookkeeping", "DATEV-Export erzeugen", "Create DATEV export", "Periode, Parameter, Beleglinks und EXTF-Datei pruefen.", "Review period, parameters, document links, and EXTF file.", "Erzeuge den DATEV-Export fuer April und pruefe fehlende Beleglinks.", "Create DATEV export for April and check missing document links.", "EXTF, Hash und Exportaudit sind vorhanden.", "EXTF, hash, and export audit exist.", "DATEV erzeugen", "Create DATEV"],
  ["33", "reports", "Bilanz aus Ledger ableiten", "Derive balance sheet from ledger", "Aktiva und Passiva aus Kontensalden pruefen.", "Review assets and liabilities from account balances.", "Erklaere mir die Bilanz zum 30.04.2026 und zeige, warum sie aufgeht.", "Explain the 2026-04-30 balance sheet and why it balances.", "Bilanz ist ausgeglichen und jede Position ist rueckverfolgbar.", "Balance sheet balances and every row is traceable.", "Bilanz pruefen", "Check balance"],
  ["34", "reports", "GuV fuer den Monat pruefen", "Review monthly P&L", "Erloese, Aufwand und Ergebnis aus Ledgerkonten pruefen.", "Review income, expense, and net income from ledger accounts.", "Warum ist der Gewinn im April niedriger als im Maerz?", "Why is April profit lower than March?", "GuV stimmt mit Ledger und Abweichungsanalyse ueberein.", "P&L matches ledger and variance analysis.", "GuV analysieren", "Analyze P&L"],
  ["35", "reports", "BWA fuer Geschaeftsueberblick erzeugen", "Create business analysis report", "Monatsvergleich und kumulierte Werte anzeigen.", "Show monthly comparison and cumulative values.", "Erstelle eine kurze BWA-Auswertung Januar bis April und markiere Kostenanstiege.", "Create a short Jan-Apr business analysis and flag cost increases.", "BWA nutzt dieselben Ledgerdaten wie GuV und Bilanz.", "Business analysis uses same ledger data as P&L and balance sheet.", "BWA erzeugen", "Create BWA"],
  ["36", "reports", "Periode abschliessen", "Close fiscal period", "Offene Entwuerfe, Exporte und Bankabgleich vor Sperre pruefen.", "Check drafts, exports, and bank reconciliation before lock.", "Pruefe, ob April abgeschlossen werden kann.", "Check whether April can be closed.", "Periode ist geschlossen und Korrekturen laufen per Storno.", "Period is closed and corrections use reversal.", "Periode schliessen", "Close period"],
  ["37", "ledger", "GoBD-Korrektur per Storno buchen", "Correct by GoBD reversal", "Festgeschriebene Buchung per Gegenbuchung korrigieren.", "Correct posted entry by counter-entry.", "Die Rechnung RE2026-0031 wurde falsch besteuert. Bitte korrekt stornieren.", "Invoice RE2026-0031 was taxed incorrectly. Reverse it correctly.", "Original bleibt unveraendert und Storno verweist auf Original.", "Original stays unchanged and reversal points to original.", "Storno pruefen", "Review reversal"],
  ["38", "ledger", "Offene Postenliste pruefen", "Review open items", "Forderungen und Verbindlichkeiten nach Faelligkeit pruefen.", "Review receivables and payables by due date.", "Zeige mir alle offenen Posten mit Risiko bis Ende Mai.", "Show all open items with risk until end of May.", "Summen stimmen mit Forderungs- und Verbindlichkeitskonten.", "Totals match receivable and payable accounts.", "OPOS pruefen", "Review open items"],
  ["39", "fixed-assets", "Laptop als Anlage aktivieren", "Capitalize laptop as asset", "Eingangsbeleg als Anlage mit AfA-Plan aktivieren.", "Capitalize receipt as asset with depreciation plan.", "Aktiviere den Apple-Beleg vom 15.05.2026 als Laptop ueber 3 Jahre.", "Capitalize the Apple receipt from 2026-05-15 as laptop over 3 years.", "Anlage ist aktiv, Beleg verknuepft und AfA-Plan auditierbar.", "Asset is active, receipt linked, and depreciation plan auditable.", "Anlage aktivieren", "Capitalize asset"],
  ["40", "fixed-assets", "Monatliche Abschreibung buchen", "Post monthly depreciation", "Faellige AfA-Liste pruefen und Buchungslauf bestaetigen.", "Review due depreciation list and confirm posting run.", "Buche alle faelligen Abschreibungen fuer Mai 2026, aber ueberspringe Anlagen ohne Beleg.", "Post all due depreciation for May 2026 but skip assets without receipt.", "AfA ist gebucht und keine Anlage doppelt abgeschrieben.", "Depreciation is posted and no asset is double-posted.", "AfA buchen", "Post depreciation"],
  ["41", "fixed-assets", "Fahrzeugleasing mit Sonderzahlung erfassen", "Record vehicle lease down payment", "Sonderzahlung, Vorsteuer und Anlagenkonto trennen.", "Separate down payment, input VAT, and asset account.", "Erfasse die VW-Leasing-Sonderzahlung als Fahrzeuganlage, netto 5.000 EUR.", "Record the VW lease down payment as vehicle asset, 5,000 net.", "Fuhrpark-Buchwert und Vorsteuer sind korrekt gebucht.", "Vehicle book value and input VAT are posted correctly.", "Leasing erfassen", "Record lease"],
  ["42", "bookkeeping", "Darlehensaufnahme buchen", "Post loan drawdown", "Auszahlung, Darlehenskonto und Tilgungsplan anlegen.", "Create drawdown, loan account, and repayment schedule.", "Lege das Sparkassen-Darlehen ueber 50.000 EUR ab 01.06.2026 an.", "Create Sparkasse loan for 50,000 from 2026-06-01.", "Bank und Darlehenskonto zeigen 50.000 EUR.", "Bank and loan account show 50,000.", "Darlehen anlegen", "Create loan"],
  ["43", "payments", "Darlehensrate aufteilen", "Split loan installment", "Bankabbuchung in Zins und Tilgung aufteilen.", "Split debit into interest and principal.", "Ordne die Sparkassen-Abbuchung vom 30.06.2026 dem Darlehen zu.", "Assign Sparkasse debit from 2026-06-30 to the loan.", "Zinsaufwand und Tilgung sind getrennt gebucht.", "Interest expense and principal are posted separately.", "Rate aufteilen", "Split installment"],
  ["44", "receipts", "Reisekostenabrechnung buchen", "Post travel expense report", "Mehrere Belege, Pauschale und Projekt in einer Abrechnung pruefen.", "Review multiple receipts, allowance, and project in one claim.", "Erstelle aus diesen drei Belegen eine Reisekostenabrechnung fuer CTOX Onboarding Berlin.", "Create travel expense report for CTOX Onboarding Berlin from these three receipts.", "Abrechnung ist freigegeben und Erstattung offen.", "Claim is approved and reimbursement is open.", "Reise buchen", "Post travel"],
  ["45", "receipts", "Kostenstelle fuer Projektrechnung setzen", "Assign project cost center", "Projekt und Aufwandskonto am Eingangsbeleg setzen.", "Set project and expense account on inbound invoice.", "Buche die Netlify-Rechnung auf REM Capital Rollout.", "Post the Netlify invoice to REM Capital Rollout.", "Projektbericht enthaelt Aufwand und Vorsteuer ist gebucht.", "Project report includes expense and input VAT is posted.", "Projekt setzen", "Set project"],
  ["46", "receipts", "Reverse-Charge-Eingangsrechnung buchen", "Post reverse-charge supplier invoice", "EU-Lieferant, Steuer und Zahlbetrag pruefen.", "Review EU vendor, tax, and payment amount.", "Buche die Stripe-Rechnung als Reverse Charge.", "Post the Stripe invoice as reverse charge.", "Steuer und Vorsteuer sind spiegelbildlich gebucht.", "Output VAT and input VAT are mirrored.", "RC buchen", "Post RC"],
  ["47", "bookkeeping", "Wiederkehrende Miete buchen", "Create recurring rent posting", "Rhythmus, Konto, Steuer und Freigabestatus einstellen.", "Set recurrence, account, tax, and approval state.", "Richte die Bueromiete an Alster Offices monatlich ueber 2.380 EUR ab Juni 2026 ein.", "Create monthly office rent to Alster Offices for 2,380 from June 2026.", "Monatsvorschau existiert und bucht nicht ohne Freigabe.", "Monthly preview exists and does not post without approval.", "Serie anlegen", "Create recurrence"],
  ["48", "bookkeeping", "Steuerberater-Uebergabe vorbereiten", "Prepare tax advisor handoff", "Blocker, DATEV, Belege und Pruefprotokoll zusammenstellen.", "Collect blockers, DATEV, documents, and review log.", "Bereite die Steuerberater-Uebergabe fuer April 2026 vor und zeige nur Blocker.", "Prepare April 2026 tax advisor handoff and show only blockers.", "Uebergabe ist erst ohne Blocker exportierbar.", "Handoff can export only without blockers.", "Uebergabe pruefen", "Review handoff"],
  ["49", "reports", "Agentischen Monatsabschluss vorbereiten", "Prepare agentic month close", "Abschlussarbeitsliste bis Status abschlussbereit abarbeiten.", "Work close checklist until ready-to-close.", "Bereite den Monatsabschluss Mai 2026 vor, buche nichts ohne Freigabe.", "Prepare May 2026 month close, post nothing without approval.", "Alle Vorschlaege haben Freigabehistorie und Berichte stimmen.", "All proposals have approval history and reports agree.", "Abschluss vorbereiten", "Prepare close"],
  ["50", "bookkeeping", "CTOX-Arbeitsauftrag mit menschlicher Freigabe ausfuehren", "Run CTOX accounting task with approval", "Vorschlaege nach Risiko pruefen und einzeln freigeben.", "Review proposals by risk and approve individually.", "Pruefe alle offenen Belege und Zahlungen der letzten Woche, bereite Buchungen vor, aber buche nichts ohne Freigabe.", "Review all open receipts and payments from last week, prepare postings, but post nothing without approval.", "Keine Buchung wird ohne Freigabe festgeschrieben.", "No posting is locked without approval.", "CTOX beauftragen", "Ask CTOX"]
] as const;

export const accountingStoryWorkflows: AccountingStoryWorkflow[] = storyRows.map((row) => ({
  acceptance: { de: row[8], en: row[9] },
  ctoxPrompt: { de: row[6], en: row[7] },
  id: `story-${row[0]}`,
  manual: { de: row[4], en: row[5] },
  primaryAction: { de: row[10], en: row[11] },
  submoduleId: row[1],
  title: { de: row[2], en: row[3] }
}));

export function storyWorkflowsForSubmodule(submoduleId: string) {
  return accountingStoryWorkflows.filter((story) => story.submoduleId === submoduleId);
}

export function buildStoryWorkflowExecution(input: {
  data: BusinessBundle;
  locale: SupportedLocale;
  storyId: string;
}) {
  const story = accountingStoryWorkflows.find((item) => item.id === input.storyId);
  if (!story) throw new Error(`unknown_story_workflow:${input.storyId}`);
  const implementation = implementationForStory(input.data, story);
  const refId = implementation.refId;
  const command = createAccountingCommand({
    companyId: "business-basic-company",
    payload: {
      acceptance: story.acceptance[input.locale],
      amountMinor: implementation.amountMinor,
      implementationLevel: "direct",
      journalDraftRef: implementation.journalDraft?.refId,
      manualPath: story.manual[input.locale],
      prompt: story.ctoxPrompt[input.locale],
      storyId: story.id,
      storyTitle: story.title[input.locale],
      submoduleId: story.submoduleId,
      ...implementation.payload
    },
    refId,
    refType: implementation.refType,
    requestedBy: "ctox-story-workflow",
    type: implementation.commandType
  });
  const proposal = createAccountingProposal({
    companyId: "business-basic-company",
    confidence: implementation.confidence,
    createdByAgent: implementation.agent,
    evidence: {
      acceptance: story.acceptance[input.locale],
      commandType: implementation.commandType,
      hasJournalDraft: Boolean(implementation.journalDraft),
      manualPath: story.manual[input.locale],
      prompt: story.ctoxPrompt[input.locale],
      storyId: story.id,
      submoduleId: story.submoduleId,
      ...implementation.evidence
    },
    kind: implementation.proposalKind,
    proposedCommand: command,
    refId,
    refType: implementation.refType
  });
  const audit = createAccountingAuditEvent({
    action: `story.${story.id}.prepare`,
    actorId: "business-runtime",
    actorType: "system",
    after: {
      commandType: implementation.commandType,
      journalDraft: implementation.journalDraft,
      storyId: story.id,
      storyTitle: story.title[input.locale],
      submoduleId: story.submoduleId
    },
    companyId: "business-basic-company",
    refId,
    refType: implementation.refType
  });
  const outbox = createBusinessOutboxEvent({
    companyId: "business-basic-company",
    id: `outbox-business.story.${story.id}`,
    payload: {
      command,
      journalDraft: implementation.journalDraft,
      prompt: story.ctoxPrompt[input.locale],
      proposalId: proposal.id,
      storyId: story.id,
      submoduleId: story.submoduleId
    },
    topic: implementation.topic
  });
  return { audit, command, journalDraft: implementation.journalDraft, outbox, proposal, story };
}

type StoryImplementation = {
  agent: string;
  amountMinor?: number;
  commandType: AccountingCommandType;
  confidence: number;
  evidence?: Record<string, unknown>;
  journalDraft?: AccountingJournalDraft;
  payload?: Record<string, unknown>;
  proposalKind: AccountingProposal["kind"];
  refId: string;
  refType: string;
  topic: string;
};

type StoryImplementationInput =
  Omit<StoryImplementation, "confidence" | "refId" | "topic">
  & Partial<Pick<StoryImplementation, "confidence" | "refId" | "topic">>;

function referenceForStory(data: BusinessBundle, story: AccountingStoryWorkflow) {
  if (story.submoduleId === "invoices") return data.invoices[0]?.id ?? story.id;
  if (story.submoduleId === "payments") return data.bankTransactions.find((item) => item.status !== "Matched")?.id ?? data.bankTransactions[0]?.id ?? story.id;
  if (story.submoduleId === "receipts") return data.receipts[0]?.id ?? story.id;
  if (story.submoduleId === "fixed-assets") return data.fixedAssets[0]?.id ?? story.id;
  if (story.submoduleId === "bookkeeping") return data.bookkeeping[0]?.id ?? story.id;
  if (story.submoduleId === "reports") return data.reports[0]?.id ?? story.id;
  return data.journalEntries[0]?.id ?? story.id;
}

function implementationForStory(data: BusinessBundle, story: AccountingStoryWorkflow): StoryImplementation {
  const invoice = data.invoices.find((item) => item.status !== "Paid") ?? data.invoices[0];
  const receipt = data.receipts[0];
  const asset = data.fixedAssets[0];
  const bankLine = data.bankTransactions.find((item) => item.status !== "Matched") ?? data.bankTransactions[0];
  const exportBatch = data.bookkeeping[0];
  const journal = data.journalEntries.find((entry) => entry.status === "Posted") ?? data.journalEntries[0];
  const refId = referenceForStory(data, story);
  const base = {
    confidence: 0.93,
    evidence: { source: "business_basic_story_coverage" },
    payload: {},
    refId,
    topic: `business.accounting.${story.id}.prepare`
  };
  const storyNumber = story.id.replace("story-", "");

  const implementations: Record<string, StoryImplementationInput> = {
    "01": { agent: "invoice-checker", commandType: "PrepareQuote", proposalKind: "quote_prepare", refId: "quote-story-01", refType: "quote" },
    "02": { agent: "invoice-checker", commandType: "ConvertQuoteToInvoice", journalDraft: invoiceJournal("story-02", "quote", 600000, 114000), proposalKind: "quote_to_invoice", refId: "quote-story-02", refType: "quote" },
    "03": { agent: "customer-masterdata-agent", commandType: "CreateCustomer", proposalKind: "customer_masterdata", refId: "cust-story-03", refType: "customer", payload: { paymentTermsDays: 14, vatIdRequired: true } },
    "04": { agent: "invoice-checker", commandType: "SendInvoice", journalDraft: invoiceJournal(invoice?.id ?? "story-04", "invoice", 98000, 18620), proposalKind: "invoice_check", refId: invoice?.id ?? refId, refType: "invoice" },
    "05": { agent: "bank-reconciler", commandType: "AcceptBankMatch", journalDraft: paymentJournal(bankLine?.id ?? "story-05", 116620), proposalKind: "bank_match", refId: bankLine?.id ?? refId, refType: "bank_transaction" },
    "06": { agent: "bank-reconciler", commandType: "PostPartialCustomerPayment", journalDraft: paymentJournal("partial-customer-payment-story-06", 300000), proposalKind: "bank_match", refType: "payment" },
    "07": { agent: "dunning-assistant", commandType: "RunDunning", proposalKind: "dunning_run", refId: invoice?.id ?? refId, refType: "invoice", payload: { feeMinor: 0, level: 1 } },
    "08": { agent: "dunning-assistant", commandType: "RunDunning", journalDraft: dunningFeeJournal("story-08", 1200, 228), proposalKind: "dunning_run", refId: invoice?.id ?? refId, refType: "invoice", payload: { feeMinor: 1200, level: 2 } },
    "09": { agent: "invoice-checker", commandType: "CreateCancellationCreditNote", journalDraft: creditNoteJournal("story-09", 98000, 18620), proposalKind: "invoice_cancellation_credit_note", refId: invoice?.id ?? refId, refType: "credit_note" },
    "10": { agent: "invoice-checker", commandType: "CreatePartialCreditNote", journalDraft: creditNoteJournal("story-10", 50000, 9500), proposalKind: "invoice_partial_credit_note", refType: "credit_note" },
    "11": { agent: "invoice-checker", commandType: "SendInvoice", journalDraft: invoiceJournal("story-11", "invoice", 85000, 0), proposalKind: "invoice_check", refType: "invoice", payload: { kleinunternehmer: true } },
    "12": { agent: "invoice-checker", commandType: "SendInvoice", journalDraft: invoiceJournal("story-12", "invoice", 180000, 0), proposalKind: "invoice_check", refType: "invoice", payload: { reverseCharge: true } },
    "13": { agent: "receipt-ocr-reviewer", commandType: "ReviewReceiptExtraction", proposalKind: "receipt_extraction", refId: receipt?.id ?? refId, refType: "receipt" },
    "14": { agent: "vendor-masterdata-agent", commandType: "CreateVendorFromReceipt", proposalKind: "vendor_creation", refId: receipt?.id ?? refId, refType: "receipt" },
    "15": { agent: "purchase-order-matcher", commandType: "CheckPurchaseOrderMatch", proposalKind: "purchase_order_match", refId: receipt?.id ?? refId, refType: "receipt", payload: { purchaseOrderId: "PO-2026-0042" } },
    "16": { agent: "purchase-order-matcher", commandType: "ResolveReceiptVariance", proposalKind: "receipt_variance", refId: receipt?.id ?? refId, refType: "receipt" },
    "17": { agent: "expense-agent", commandType: "SubmitEmployeeExpense", journalDraft: expenseJournal("story-17", 4200, 0, "party-employee"), proposalKind: "employee_expense", refType: "employee_expense" },
    "18": { agent: "receipt-ocr-reviewer", commandType: "RequestReceiptClarification", proposalKind: "receipt_clarification", refId: receipt?.id ?? refId, refType: "receipt", confidence: 0.74 },
    "19": { agent: "bank-import-agent", commandType: "ImportBankStatement", proposalKind: "bank_match", refType: "bank_statement", payload: { format: "camt053" } },
    "20": { agent: "payables-agent", commandType: "PostSupplierPayment", journalDraft: supplierPaymentJournal("story-20", 22194), proposalKind: "payables_payment", refId: bankLine?.id ?? refId, refType: "payment" },
    "21": { agent: "payables-agent", commandType: "PostSupplierPayment", journalDraft: supplierPaymentJournal("story-21", 50000), proposalKind: "payables_payment", refType: "payment", payload: { partial: true } },
    "22": { agent: "payables-agent", commandType: "PostSupplierPayment", journalDraft: supplierPaymentJournal("story-22", 238000), proposalKind: "payables_payment", refType: "payment", payload: { allocationCount: 3 } },
    "23": { agent: "payables-agent", commandType: "ApplySupplierDiscount", journalDraft: supplierDiscountJournal("story-23", 98000, 1960, 372), proposalKind: "supplier_discount", refType: "payment" },
    "24": { agent: "receipt-duplicate-agent", commandType: "MarkDuplicateReceipt", proposalKind: "receipt_duplicate", refId: receipt?.id ?? refId, refType: "receipt" },
    "25": { agent: "payables-agent", commandType: "PreparePaymentRun", proposalKind: "payables_payment_run", refType: "payment_run" },
    "26": { agent: "ledger-agent", commandType: "TraceLedgerEntry", proposalKind: "open_items_review", refId: journal?.id ?? refId, refType: "journal_entry" },
    "27": { agent: "ledger-agent", commandType: "CreateManualJournal", journalDraft: manualBankFeeJournal("story-27", 1290), proposalKind: "manual_journal", refType: "journal_entry" },
    "28": { agent: "product-account-agent", commandType: "AssignProductAccount", proposalKind: "product_account_assignment", refId: data.products[0]?.id ?? refId, refType: "product", payload: { revenueAccountCode: "8400" } },
    "29": { agent: "setup-agent", commandType: "SetupChartOfAccounts", proposalKind: "chart_setup", refType: "accounting_setup", payload: { chart: "SKR04" } },
    "30": { agent: "tax-agent", commandType: "ValidateInvoiceTax", proposalKind: "invoice_check", refId: invoice?.id ?? refId, refType: "invoice" },
    "31": { agent: "tax-agent", commandType: "PrepareVatReturn", proposalKind: "vat_return", refType: "vat_statement", payload: { period: "2026-04" } },
    "32": { agent: "datev-export-agent", commandType: "ExportDatev", proposalKind: "datev_export", refId: exportBatch?.id ?? refId, refType: "bookkeeping_export" },
    "33": { agent: "report-agent", commandType: "DeriveBalanceSheet", proposalKind: "report_balance_sheet", refType: "report", payload: { date: "2026-04-30" } },
    "34": { agent: "report-agent", commandType: "AnalyzeProfitAndLoss", proposalKind: "profit_and_loss_analysis", refType: "report", payload: { period: "2026-04" } },
    "35": { agent: "report-agent", commandType: "CreateBusinessAnalysis", proposalKind: "business_analysis", refType: "report", payload: { from: "2026-01", to: "2026-04" } },
    "36": { agent: "period-close-agent", commandType: "CloseFiscalPeriod", proposalKind: "month_close", refType: "fiscal_period", payload: { periodId: "fy-2026-04", strict: true } },
    "37": { agent: "ledger-agent", commandType: "PrepareGoBDReversal", journalDraft: reversalJournal("story-37", 98000, 18620), proposalKind: "gobd_reversal", refId: journal?.id ?? refId, refType: "journal_entry" },
    "38": { agent: "ledger-agent", commandType: "ReviewOpenItems", proposalKind: "open_items_review", refType: "open_items", payload: { until: "2026-05-31" } },
    "39": { agent: "asset-accountant", commandType: "CapitalizeReceipt", journalDraft: assetAcquisitionJournal("story-39", 123697, 23503), proposalKind: "asset_activation", refId: receipt?.id ?? refId, refType: "receipt" },
    "40": { agent: "asset-accountant", commandType: "PostDepreciation", journalDraft: depreciationJournal(asset?.id ?? "story-40", 24800), proposalKind: "asset_depreciation", refId: asset?.id ?? refId, refType: "asset" },
    "41": { agent: "asset-accountant", commandType: "CapitalizeReceipt", journalDraft: assetAcquisitionJournal("story-41", 500000, 95000), proposalKind: "asset_activation", refType: "asset", payload: { category: "vehicle_lease_down_payment" } },
    "42": { agent: "treasury-agent", commandType: "PostLoanDrawdown", journalDraft: loanDrawdownJournal("story-42", 5000000), proposalKind: "loan_drawdown", refType: "loan" },
    "43": { agent: "treasury-agent", commandType: "SplitLoanInstallment", journalDraft: loanInstallmentJournal("story-43", 84000, 18000), proposalKind: "loan_installment", refType: "payment" },
    "44": { agent: "expense-agent", commandType: "PostTravelExpenseReport", journalDraft: expenseJournal("story-44", 38400, 1900, "party-employee"), proposalKind: "travel_expense_report", refType: "travel_expense" },
    "45": { agent: "project-accounting-agent", commandType: "AssignCostCenter", journalDraft: expenseJournal("story-45", 7900, 1501, "vendor-netlify", "cc-rem-capital", "proj-rem-rollout"), proposalKind: "cost_center_assignment", refId: receipt?.id ?? refId, refType: "receipt" },
    "46": { agent: "tax-agent", commandType: "PostReverseChargeReceipt", journalDraft: reverseChargeReceiptJournal("story-46", 120000, 22800), proposalKind: "reverse_charge_receipt", refId: receipt?.id ?? refId, refType: "receipt" },
    "47": { agent: "recurring-posting-agent", commandType: "CreateRecurringPosting", journalDraft: expenseJournal("story-47", 200000, 38000, "vendor-alster-offices"), proposalKind: "recurring_posting", refType: "recurring_posting", payload: { cadence: "monthly", startsOn: "2026-06-01" } },
    "48": { agent: "tax-advisor-agent", commandType: "PrepareTaxAdvisorHandoff", proposalKind: "tax_advisor_handoff", refType: "tax_advisor_handoff", payload: { period: "2026-04", blockersOnly: true } },
    "49": { agent: "month-close-agent", commandType: "PrepareMonthClose", proposalKind: "month_close", refType: "fiscal_period", payload: { period: "2026-05", requireApproval: true } },
    "50": { agent: "business-accounting-supervisor", commandType: "PrepareAccountingApprovalBatch", proposalKind: "story_workflow", refType: "approval_batch", payload: { requireHumanApproval: true } }
  };

  const implementation = implementations[storyNumber];
  if (!implementation) {
    throw new Error(`missing_story_workflow_implementation:${story.id}`);
  }

  return {
    ...base,
    ...implementation,
    confidence: implementation.confidence ?? base.confidence,
    refId: implementation.refId ?? base.refId,
    topic: implementation.topic ?? base.topic
  };
}

function invoiceJournal(refId: string, refType: string, netMinor: number, taxMinor: number): AccountingJournalDraft {
  return journal(refId, refType, "invoice", [
    debit("acc-ar", netMinor + taxMinor, "cust-story"),
    credit("acc-revenue-saas", netMinor, "cust-story"),
    ...(taxMinor ? [credit("acc-vat-output", taxMinor, "cust-story")] : [])
  ], "Story invoice posting");
}

function paymentJournal(refId: string, amountMinor: number) {
  return journal(refId, "payment", "payment", [debit("acc-bank", amountMinor, "cust-story"), credit("acc-ar", amountMinor, "cust-story")], "Story customer payment");
}

function supplierPaymentJournal(refId: string, amountMinor: number) {
  return journal(refId, "payment", "payment", [debit("acc-ap", amountMinor, "vendor-story"), credit("acc-bank", amountMinor, "vendor-story")], "Story supplier payment");
}

function supplierDiscountJournal(refId: string, payableMinor: number, discountMinor: number, vatCorrectionMinor: number) {
  return journal(refId, "payment", "payment", [
    debit("acc-ap", payableMinor, "vendor-story"),
    credit("acc-bank", payableMinor - discountMinor - vatCorrectionMinor, "vendor-story"),
    credit("acc-software", discountMinor, "vendor-story"),
    credit("acc-vat-input", vatCorrectionMinor, "vendor-story")
  ], "Story supplier cash discount");
}

function creditNoteJournal(refId: string, netMinor: number, taxMinor: number) {
  return journal(refId, "credit_note", "reverse", [
    debit("acc-revenue-saas", netMinor, "cust-story"),
    ...(taxMinor ? [debit("acc-vat-output", taxMinor, "cust-story")] : []),
    credit("acc-ar", netMinor + taxMinor, "cust-story")
  ], "Story credit note");
}

function dunningFeeJournal(refId: string, netMinor: number, taxMinor: number) {
  return journal(refId, "dunning", "manual", [debit("acc-ar", netMinor + taxMinor, "cust-story"), credit("acc-revenue-saas", netMinor, "cust-story"), credit("acc-vat-output", taxMinor, "cust-story")], "Story dunning fee");
}

function manualBankFeeJournal(refId: string, amountMinor: number) {
  return journal(refId, "manual", "manual", [debit("acc-fees", amountMinor), credit("acc-bank", amountMinor)], "Story manual bank fee");
}

function expenseJournal(refId: string, netMinor: number, taxMinor: number, partyId?: string, costCenterId?: string, projectId?: string) {
  return journal(refId, "receipt", "receipt", [
    debit("acc-software", netMinor, partyId, costCenterId, projectId),
    ...(taxMinor ? [debit("acc-vat-input", taxMinor, partyId, costCenterId, projectId)] : []),
    credit("acc-ap", netMinor + taxMinor, partyId, costCenterId, projectId)
  ], "Story expense posting");
}

function assetAcquisitionJournal(refId: string, netMinor: number, taxMinor: number) {
  return journal(refId, "asset", "manual", [debit("acc-fixed-assets", netMinor), debit("acc-vat-input", taxMinor), credit("acc-ap", netMinor + taxMinor)], "Story asset acquisition");
}

function depreciationJournal(refId: string, amountMinor: number) {
  return journal(refId, "depreciation", "depreciation", [debit("acc-depreciation", amountMinor), credit("acc-accumulated-depreciation", amountMinor)], "Story depreciation");
}

function loanDrawdownJournal(refId: string, amountMinor: number) {
  return journal(refId, "loan", "manual", [debit("acc-bank", amountMinor), credit("acc-ap", amountMinor)], "Story loan drawdown");
}

function loanInstallmentJournal(refId: string, principalMinor: number, interestMinor: number) {
  return journal(refId, "payment", "payment", [debit("acc-ap", principalMinor), debit("acc-fees", interestMinor), credit("acc-bank", principalMinor + interestMinor)], "Story loan installment");
}

function reverseChargeReceiptJournal(refId: string, netMinor: number, taxMinor: number) {
  return journal(refId, "receipt", "receipt", [debit("acc-software", netMinor), debit("acc-vat-input", taxMinor), credit("acc-vat-output", taxMinor), credit("acc-ap", netMinor)], "Story reverse-charge receipt");
}

function reversalJournal(refId: string, netMinor: number, taxMinor: number) {
  return journal(refId, "reverse", "reverse", [debit("acc-revenue-saas", netMinor), debit("acc-vat-output", taxMinor), credit("acc-ar", netMinor + taxMinor)], "Story GoBD reversal");
}

function journal(refId: string, refType: string, type: string, lines: AccountingJournalDraft["lines"], narration: string): AccountingJournalDraft {
  return {
    companyId: "business-basic-company",
    lines,
    narration,
    postingDate: "2026-05-07",
    refId,
    refType,
    type
  };
}

function debit(accountId: string, minor: number, partyId?: string, costCenterId?: string, projectId?: string) {
  return { accountId, costCenterId, credit: { minor: 0 }, debit: { minor }, partyId, projectId };
}

function credit(accountId: string, minor: number, partyId?: string, costCenterId?: string, projectId?: string) {
  return { accountId, costCenterId, credit: { minor }, debit: { minor: 0 }, partyId, projectId };
}
