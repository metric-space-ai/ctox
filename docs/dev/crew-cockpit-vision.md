# CREW-COCKPIT · Worum es wirklich geht

Diese Seite ist die Einführung für alle, die an CREW-COCKPIT mitbauen. Die Briefs sagen *was* gebaut wird, diese Seite sagt *wofür*. Wer einen Brief umsetzt, ohne diese Seite gelesen zu haben, baut mit hoher Wahrscheinlichkeit die richtigen Felder für das falsche Produkt.

## 1. Das Problem, wie es der Owner erlebt

CTOX ist ein Harness, der Tag und Nacht Arbeit für ein Unternehmen erledigt: Chat-Aufträge, App-Entwicklung, Importe, Recherche, Tickets, Outbound. Er ist seriell, gründlich, mit Review- und Validierungs-Gates. Diese Maschine ist gut. Aber der Mensch davor sieht sie nicht.

Heute öffnet der Owner die CTOX-App in Business OS und sieht ein statisches Poster einer Zustandsmaschine mit Enum-Namen, fünfmal „nicht erfasst“, eine Liste mit „Arbeitet (4)“ bei vier gescheiterten Tasks. Er schreibt der Crew im Chat eine Aufgabe, sieht einen leeren Balken, der Composer verschwindet, und wenn der Harness fertig ist, kommt keine Antwort, weil sie in `retry_wait` hängt. Das Wesen, das ihm eben noch „Tavi“ hieß, heißt nach dem ersten Senden „Milo“, weil sein Name ein Hash der Command-ID ist. Tickets zeigen Fremdsystem-Status und lassen sich nicht bedienen.

Das Ergebnis: Ein System, das ununterbrochen arbeitet, wirkt kaputt, launisch und unerreichbar. Der Owner kann weder verstehen, was gerade passiert, noch eingreifen, noch Vertrauen aufbauen. Genau das ist das Kernprodukt von Business OS, und es ist heute vielleicht zu einem Viertel fertig.

## 2. Das Zielbild in einem Bild

**Die CTOX-App wird das Zuhause der Crew.**

Die Crew besteht aus wenigen, eindeutigen Wesen. Jedes hat einen Namen, eine Form, eine Farbe, eine Seele, einen Lebenslauf und einen Stundenzettel. Sie sind keine Dekoration und keine Zufallsvisualisierung, sie sind die Arbeiter des Harness, sichtbar gemacht. Der Harness bleibt seriell, also ist immer genau ein Wesen im Einsatz. Die anderen sind zu Hause: sie ruhen, warten, lesen ihre Learnings, schauen zu.

Wenn ein Task startet, wählt der Harness das passendste Wesen: nach Spezialität (Modul, Auftragstyp, Skill), nach Erfolgsgeschichte auf ähnlichen Aufgaben, mit Kontinuität zum laufenden Gespräch. Dieses Wesen steht auf, geht an den Arbeitsplatz, und man sieht ihm zu: Es denkt, es greift zu Werkzeug, es wartet auf etwas, es bekommt eine Review-Rückmeldung, es scheitert oder es ist fertig. Danach schreibt es seinen Rückblick in den Stundenzettel und nimmt ein oder zwei Learnings mit. Wird es wieder für ähnliche Arbeit gewählt, bekommt es diese Learnings in seinen Kontext. So werden die Wesen mit der Zeit klüger, jedes auf seine Art.

Der Owner kann jedem Wesen ins Profil schauen: Seele als fünf Regler, Spezialitäten, Lebenslauf, Learnings (bestätigen, korrigieren, löschen), Stundenzettel mit Runs, Modell, Tokens, Kosten, Dauer, Urteil. Und er kann eingreifen: abbrechen, freigeben, blockieren, wiederholen, Priorität ändern, Kapazität setzen, die Queue pausieren, einem Wesen einen Task zuweisen.

Die Crew-Leiste ist derselbe Gedanke im Kleinen: Der Owner spricht nicht mit „dem System“, sondern mit einem Wesen, das ihm antwortet, Zwischenstände gibt, Fragen stellt und ihn in sein Zuhause mitnimmt, wenn er mehr sehen will.

## 3. Die Wesen: Regeln der Verankerung

1. **Ein Wesen ist eine Entität mit Identität**, gespeichert im Harness, nicht im Browser. Name, Form und Farbe stehen fest, solange der Owner sie nicht ändert. Kein Hash, kein Zufall, kein Wechsel mitten im Gespräch.
2. **Ausdruck = Harness-Zustand.** Die Animation eines Wesens ist die Anzeige des Zustands, nicht Farbe, nicht ein roter Balken. Die Zustände, die ein Wesen zeigen muss: schläft/zu Hause · wartet in der Queue · aufgewacht (Lease) · denkt · arbeitet mit Werkzeug · legt vor (Review) · wartet auf X (Wartegrund sichtbar) · gescheitert (X-Augen) · fertig, zufrieden. Der Owner soll die Lage aus dem Wesen lesen können, bevor er ein Wort liest.
3. **Ein Wesen hat eine Seele**, die im Prompt wirkt: fünf Achsen (Gründlichkeit ↔ Tempo, Vorsicht ↔ Mut, knapp ↔ ausführlich, regeltreu ↔ kreativ, nachfragen ↔ annehmen) und eine Charakterskizze. Die Seele kommt nach allen Sicherheits- und Ausführungsregeln und relativiert sie nie.
4. **Ein Wesen lernt.** Learnings sind kurze, konkrete Sätze mit Nachweis (Run), nach Scope (Modul, Auftragstyp) abrufbar, vom Owner kuratierbar. Sie entstehen im ohnehin stattfindenden Abschluss eines Versuchs, nicht durch zusätzliche Modellaufrufe.
5. **Ein Wesen hat einen Stundenzettel.** Jeder Run ist eine Zeile: Beginn, Ende, Task, Ergebnis, Modell, Tokens, Kosten, Urteil, Rückblick.
6. **Die Wesen ändern den Harness nicht.** Serialität, Queue-Semantik, Review- und Validierungs-Gates bleiben. Die Wesen sind Identität, Kontext und Sichtbarkeit obendrauf.

## 4. Die visuelle Sprache

- **Hierarchie statt Beschriftung.** Heute sind die Oberflächen Textlabels auf Leerflächen. Künftig trägt die Form die Information: Was gerade passiert, ist groß und lebendig; was wartet, ist ruhig; was fertig ist, tritt zurück. Zahlen nur, wenn sie eine Entscheidung ändern.
- **Keine Leerflächen als Layout.** Eine Ansicht ist gefüllt mit dem, was zählt: dem Wesen im Einsatz, seinem Plan, seiner Aktivität, seinem Grund. Wenn nichts läuft, ist das Zuhause zu sehen, nicht ein leerer Rahmen mit „nicht erfasst“.
- **Bewegung erklärt.** Animation ist Zustandsanzeige, sparsam, lesbar, ohne Farbabhängigkeit. Ein Wesen mit X-Augen sagt „kaputt“ deutlicher als jeder rote Chip.
- **Ein Wesen, eine Komponente.** Chat, Zuhause und Tickets benutzen dieselbe Wesen-Komponente mit denselben Zuständen.
- **Sprache des Owners.** Zustände heißen „wartet auf Freigabe bis 14:30“, nicht `AwaitingReview`. Deutsch und Englisch vollständig, aus einer Quelle.
- **Nichts außerhalb der App.** Dialoge, Drawer, Overlays rendern im eigenen Host, nie auf der Shell.

## 5. Warum PR-1 und PR-2 zuerst kommen

Alles in Abschnitt 2 scheitert heute an einer Tatsache: Der Browser weiß nicht, was der Harness tut. `ctox_runs` hat keinen Writer, Tool- und Token-Ereignisse erreichen den Browser nicht, Lease- und Wartefelder werden nicht projiziert, es gibt keinen Harness-Status und keine Steuerbefehle außer cancel. Deshalb ist **PR-1** kein Backend-Aufräumen, sondern die Voraussetzung dafür, dass ein Wesen überhaupt etwas zeigen kann: Jede Projektion in PR-1 wird später eine sichtbare Regung eines Wesens oder ein Griff des Owners. Und **PR-2** macht aus zufälligen Figuren echte Mitglieder: Entität, Auswahl, Seele im Prompt, Learnings, Stundenzettel.

Erst dann lohnt es sich, die drei Oberflächen zu bauen (PR-3 Zuhause, PR-4 Crew-Leiste, PR-5 Tickets), weil sie dann Wahrheit anzeigen statt Spezifikation.

## 6. Woran wir Erfolg messen

Der Owner gibt im Chat eine echte Aufgabe. Er sieht, welches Wesen sie übernimmt und warum. Er sieht es denken, arbeiten, warten, mit Grund. Er kann jederzeit abbrechen oder nachsteuern. Die Antwort kommt als Nachricht des Wesens, auch als Zwischenstand. Im Zuhause sieht er dasselbe Wesen im Einsatz, die anderen zu Hause, den Stundenzettel wachsen. Beim nächsten ähnlichen Auftrag ist dasselbe Wesen dran und macht es besser. Nichts davon hängt von Farbe ab, nichts davon zeigt Enum-Namen, nichts davon ist Zufall.
