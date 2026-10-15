// Owner 15.10.2026: "auf mailtester ausweichen". experte.de zeigte in einer
// Nacht 71-mal eine Zugangssperre; der Ablauf meldete die Adresse dann als
// pruefbeduerftig, statt mailtester zu fragen. Ersetzt werden nur die beiden
// alten Saetze, wortgleich; jede eigene Fassung bleibt stehen.
const VERALTETE_PRUEFDIENST_SAETZE = Object.freeze([
  ["Die gebildete Adresse über den Adapter experte-de prüfen, bei unklarem Ergebnis einmal über mailtester-com.",
    "Die gebildete Adresse über den Adapter experte-de prüfen; bei unklarem Ergebnis oder wenn experte-de blockiert oder nicht erreichbar ist, einmal über mailtester-com."],
  ["Jede Adresse höchstens einmal je Prüfdienst; ist ein Dienst blockiert, das Feld als prüfbedürftig mit diesem Grund melden.",
    "Jede Adresse höchstens einmal je Prüfdienst; ist ein Dienst blockiert, den anderen nehmen, und erst wenn beide blockiert sind, das Feld als prüfbedürftig mit diesem Grund melden."],
]);
export function hebePruefdienstRegelAn(text) {
  return VERALTETE_PRUEFDIENST_SAETZE.reduce((neu, [alt, ersatz]) => neu.split(alt).join(ersatz), String(text));
}
