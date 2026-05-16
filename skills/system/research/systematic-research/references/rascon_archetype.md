# RASCON Feasibility Study — Archetype Reference

This file is the goldreferenz for what feasibility-study substance looks
like inside the CTOX deep-research skill. It is consulted by humans
calibrating the asset pack and by the `release_guard_check` lints (see
`release_guard_lints.md`) to anchor structural and stylistic decisions in
a real, well-written feasibility study.

## Report type

This archetype maps to exactly one report-type / domain combination.
It is **not** a generic deep-research goldreferenz; it is the
calibration source for the feasibility-study report type in an
NDT-aerospace domain at decision-grade depth.

- `report_type_id`: `feasibility_study`
- `domain_profile_id`: `ndt_aerospace`
- `style_profile_id`: `scientific_engineering_dossier`
- `depth_profile_id`: `decision_grade`

For the six other report types the deep-research skill supports
(`market_research`, `competitive_analysis`, `technology_screening`,
`whitepaper`, `literature_review`, `decision_brief`) and for domains
outside NDT-aerospace, separate archetypes are needed. RASCON does
**not** stand in for them. Lints, sub-skill instructions, and the asset
pack must read this archetype only when the run is feasibility / NDT;
for other runs, the partial-archetype notes under `## Sibling
archetypes (placeholders)` apply until full archetypes are supplied.

## Source

- File: `/Users/michaelwelsch/Downloads/Machbarkeitsstudie_RASCON_Blitsschutzgitter_kontaktlose_Pruefung.docx`
- Title: `Machbarkeitsstudie`
- Subtitle: `Kontaktlose Prüfung des Blitzschutz-Kupfergitters (LSP) in CFK-Strukturen– Technologiebewertung und Vorschlag für ein Forschungs- und Versuchsdesign`
- Document version line (verbatim): `Arbeitsfassung | Stand: 26.02.2026`
- Context note (verbatim): `Kontext: RASCON – erweitertes Forschungskonzept (Blitzschutzprüfung)`
- Scope note (verbatim, used as the canonical clean example for
  LINT-MISSING-DISCLAIMER):
  `Hinweis: Diese Studie basiert auf öffentlich zugänglichen Informationen, Plausibilitätsannahmen und typischen Material-/Schichtsystemen. Eine belastbare Aussage zur Detektierbarkeit erfordert Validierung an repräsentativen Proben (Schichtaufbau, Gittergeometrie, Defektkatalog).`
- License / usage: this archetype is used internally as a structural
  reference for the deep-research skill. The source document is a working
  draft owned by its authors; do not redistribute the source verbatim
  outside CTOX. Quoted excerpts in this file are short structural
  evidence used solely to calibrate CTOX's research-quality lints.

## Document anatomy

Each entry: chapter number + heading text (verbatim), one-sentence
purpose, planned asset-pack `block_id` (the `references/asset_pack.json`
contract is forward-looking; this file fixes the names so the asset pack
can implement them), measured length in characters, and a verbatim 1–3-
sentence excerpt drawn from the chapter. Lengths are computed over
`document.xml` paragraph text, ignoring the `[styleName]` prefix.

### Title block

- Heading: `Machbarkeitsstudie` / `Kontaktlose Prüfung des Blitzschutz-Kupfergitters (LSP) in CFK-Strukturen– Technologiebewertung und Vorschlag für ein Forschungs- und Versuchsdesign`
- Purpose: name the object of study, the technological framing, and the
  document's status as a draft.
- `block_id`: `title_block`
- Length: 546 characters (incl. context line and scope-disclaimer note).
- Verbatim excerpt: `Arbeitsfassung | Stand: 26.02.2026`

### Abkürzungsverzeichnis

- Heading (verbatim): `Abkürzungsverzeichnis`
- Purpose: define every domain abbreviation used downstream so the
  detail chapters can stay dense.
- `block_id`: `abbreviations_table`
- Length: 559 characters.
- Verbatim excerpt (a row pair):
  `LSP` — `Lightning Strike Protection (Blitzschutz)`

### 1. Management Summary

- Heading (verbatim): `1. Management Summary`
- Purpose: give a decision-grade overview of which methods qualify, why,
  and where the largest technical risk sits.
- `block_id`: `management_summary`
- Length: 1 938 characters.
- Verbatim excerpt:
  `Ziel dieser Machbarkeitsstudie ist die Bewertung kontaktloser Prüfverfahren, mit denen die Struktur eines in CFK eingebetteten Blitzschutz-Kupfergitters (Lightning Strike Protection, LSP) durch Deckschichten (Lack, Primer, Surfacer usw.) sichtbar gemacht und Anomalien (z.B. Unterbrechungen, Fehlstellen, Ablösung) detektiert werden können – idealerweise flächig und mit hoher Geschwindigkeit.`
- Verbatim risk close-out (used as example clean for LINT-UNCITED-CLAIM
  when the run carries `used_reference_ids[]` linkage):
  `Die größten technischen Risiken liegen in der genauen Ausprägung des Schichtaufbaus: Insbesondere eine zusätzliche, nahezu geschlossene metallische Folie in der Blitzschutzlage kann THz- und Mikrowellenverfahren sowie bestimmte induktive Messgeometrien stark beeinflussen (Abschirmung/Dominanz der obersten leitfähigen Schicht).`

### 2. Ausgangslage, Prüfobjekt und Fragestellung

- Heading (verbatim): `2. Ausgangslage, Prüfobjekt und Fragestellung`
- Purpose: name the part, the operational context, and the four leading
  questions the study must answer.
- `block_id`: `context_and_questions`
- Length: 1 007 characters.
- Verbatim excerpt:
  `Im Rahmen von RASCON wird ein erweitertes Forschungskonzept zur Prüfung von Blitzschutzstrukturen in Luftfahrtverbundbauteilen entwickelt. Der Blitzschutz wird durch ein in kohlenstofffaserverstärktem Kunststoff (CFK) eingebettetes Kupfergitter bzw. eine Kupfer-Expanded-Metal-Foil (EMF) realisiert.`
- Verbatim leading-question example:
  `Welche kontaktlosen Technologien eignen sich, um die Gitterstruktur durch Lack/Primer/Surfacer sichtbar zu machen?`

### 3. Bauteilaufbau und Referenzabbildungen

- Heading (verbatim): `3. Bauteilaufbau und Referenzabbildungen`
- Purpose: pin down the layer stack, including the failure-mode-relevant
  distinction between an open mesh and an additional closed foil.
- `block_id`: `part_structure_with_figures`
- Length: 1 483 characters (incl. three figure captions).
- Verbatim excerpt (figure caption verbatim):
  `Abbildung 1: Beispielhafter Schichtaufbau mit metallischer Blitzschutzlage (schematisch). Quelle: Segui (2015), COMSOL Blog „Protecting Aircraft Composites from Lightning Strike Damage“.`
- Verbatim body sentence:
  `Für die Prüfbarkeit ist entscheidend, ob die Blitzschutzlage eine offene Gitterstruktur ist oder ob zusätzlich eine weitgehend geschlossene Metallfolie vorhanden ist.`

### 4. Anforderungen und Randbedingungen

- Heading (verbatim): `4. Anforderungen und Randbedingungen`
- Purpose: enumerate the inspection constraints (contactless, single-
  sided, industrial robustness, etc.).
- `block_id`: `requirements_and_constraints`
- Length: 900 characters (header + bullet list).
- Verbatim bullets (selected):
  `Kontaktloser Betrieb; keine Koppelmittel (z.B. Wasser/Gel) und kein direkter Sensor-Kontakt mit der Oberfläche.`
  `Standoff (Sensorabstand) im mm- bis cm-Bereich je nach Verfahren; möglichst tolerant gegenüber Abstandsschwankungen.`
  `Sicherheits- und Zulassungsaspekte (insbesondere bei ionisierender Strahlung).`

### 5. Technologie-Screening: kontaktlose Verfahren — intro

- Heading (verbatim): `5. Technologie-Screening: kontaktlose Verfahren`
- Purpose: bridge into the screening; declare the qualitative nature of
  the rating.
- `block_id`: `screening_intro`
- Length: 336 characters (incl. heading).
- Verbatim excerpt:
  `Dieses Kapitel fasst die wichtigsten kontaktlosen Verfahren zusammen und bewertet deren Eignung für die Abbildung der Blitzschutzstruktur unter Deckschichten. Die Bewertung ist qualitativ; numerische Kennwerte (POD, Falschalarme, Auflösung, Durchsatz) müssen experimentell bestimmt werden.`

### 5.1 Bewertungslogik

- Heading (verbatim): `5.1 Bewertungslogik`
- Purpose: name the five evaluation axes used in the matrix below.
- `block_id`: `assessment_logic`
- Length: 258 characters.
- Verbatim excerpt:
  `Bewertet wurden insbesondere: (a) Fähigkeit, die Leiterstruktur abzubilden, (b) Sensitivität gegenüber typischen Defekten, (c) Flächenleistung/Single-Shot-Potenzial, (d) Robustheit/Integrationsaufwand, (e) Reifegrad (TRL/Industrieeinsatz).`

### 5.2 Bewertungsmatrix (qualitativ)

- Heading (verbatim): `5.2 Bewertungsmatrix (qualitativ)`
- Purpose: rate each candidate method on five axes (Fläche, Gitterbild,
  Gitterdefekt, Delamination, Reifegrad).
- `block_id`: `evaluation_matrix`
- Length: 1 020 characters (header + cells + legend).
- Axis vocabulary used (verbatim): `niedrig`, `mittel`, `hoch`,
  `sehr hoch`, plus the bridge labels `niedrig–mittel`, `mittel–hoch`.
- Verbatim row example (Eddy Current (ECT) / Arrays):
  `mittel` / `sehr hoch` / `sehr hoch` / `mittel` / `hoch`
- Verbatim row example (Induktions-Thermografie (ECT + IR)):
  `hoch` / `hoch` / `hoch` / `hoch` / `hoch`
- Verbatim legend close-out:
  `Hinweis (*): THz kann eine Gitterstruktur sehr gut kontrastieren, sofern die Blitzschutzlage die erste dominante leitfähige Schicht ist bzw. keine geschlossene Folie das Signal vollständig reflektiert.`

### 5.3 Erfolgsaussichten nach Schichtaufbau (Szenarien)

- Heading (verbatim): `5.3 Erfolgsaussichten nach Schichtaufbau (Szenarien)`
- Purpose: condition the matrix on three layup variants A/B/C, exposing
  how a closed foil flips the verdict.
- `block_id`: `scenario_matrix`
- Length: 894 characters.
- Verbatim scenario column headers:
  `Szenario A:Gitter/EMF = ersteMetallschicht`,
  `Szenario B:zusätzliche (nahezu)geschlossene Folie oben`,
  `Szenario C:größere Deckschichtdicke/Gitter tiefer`.
- Verbatim row example (Induktions‑Thermografie (EC+IR)):
  `hoch` / `hoch*` / `mittel–hoch`
- Verbatim footnote (cited verbatim by the asset pack as the example
  clean for unanchored hedges):
  `Auch bei dominanter Folie kann die thermische bzw. magnetische Signatur von Leiteranomalien detektierbar bleiben, weil Unterbrechungen/Engstellen die Stromverteilung und Joule-Erwärmung beeinflussen. Für THz gilt hingegen: Eine geschlossene Metallschicht wirkt i.d.R. als starke Reflexions-/Abschirmbarriere.`

### 5.4 Kurzbewertung weiterer Verfahren

- Heading (verbatim): `5.4 Kurzbewertung weiterer Verfahren`
- Purpose: park the second-tier candidates with a one-line verdict each.
- `block_id`: `short_assessment_other_methods`
- Length: 978 characters.
- Verbatim bullet (MIT, full):
  `Magnetische Induktionstomografie (MIT): Kontaktloses Leitfähigkeitsbild; in der Praxis oft begrenzte Auflösung für feine Gitter, aber als Forschungsansatz interessant.`
- Verbatim bullet (NV/OPM, full):
  `Optisch gepumpte Magnetometer / NV‑Zentren (Forschung): Potenzial für sehr empfindliches Magnetfeld‑Imaging; derzeit meist Labor-/Forschungsreife für industrielle Flächeninspektion.`

### 6.1 Elektrische und magnetische Felder: Eddy Current / Induktion

- Heading (verbatim): `6.1 Elektrische und magnetische Felder: Eddy Current / Induktion`
- Purpose: argue why ECT/induction is sensitive to grid geometry, and
  bound the verdict by the lift-off and frequency design.
- `block_id`: `detail_assessment_eddy_current`
- Length: 816 characters.
- Verbatim opening (canonical clean example for LINT-FILLER-OPENING):
  `Induktive Verfahren regen Wirbelströme in leitfähigen Strukturen an und messen die resultierende Änderung des Magnetfelds bzw. der Sondenimpedanz.`
- Verbatim verdict close-out:
  `Erfolgsaussichten (qualitativ): sehr hoch für Gitterabbildung und Gitterdefekte; mittel für reine Delamination ohne elektrische Auswirkung. Wesentliche Randbedingung ist der Sensorabstand (Lift-off) und die genaue Auslegung von Frequenz und Sondengeometrie.`

### 6.2 Induktions-Thermografie (Eddy-Current Heating + IR-Kamera)

- Heading (verbatim): `6.2 Induktions-Thermografie (Eddy-Current Heating + IR-Kamera)`
- Purpose: explain why coupling induction heating with an IR full-field
  sensor is the most attractive single-shot screening method.
- `block_id`: `detail_assessment_induction_thermography`
- Length: 1 035 characters.
- Verbatim opening:
  `Bei der Induktions-Thermografie wird die Blitzschutzlage induktiv erwärmt; eine IR-Kamera erfasst die Temperaturentwicklung flächig.`
- Verbatim figure caption:
  `Abbildung 4: Prinzip der Induktions-Thermografie (eigene Darstellung). In Anlehnung an grundlegende ECPT/Induktions-Thermografie-Prinzipien, u.a. Oswald‑Tranta (2025) sowie Feng et al. (2020).`
- Verbatim verdict close-out:
  `Erfolgsaussichten (qualitativ): hoch bis sehr hoch für Gitter und Defekte, hoch für Delamination nahe der Blitzschutzlage. Risiken: Überlagerung durch andere leitfähige Schichten (z.B. zusätzliche Folie) und Wärmeleitung in Decklagen; erfordert geeignete Anregungsfrequenz und Spulengeometrie.`

### 6.3 Terahertz-Imaging

- Heading (verbatim): `6.3 Terahertz-Imaging`
- Purpose: localise THz's strengths (dielectric layer transparency,
  time-domain depth) and its hard physical limit (closed metal foils).
- `block_id`: `detail_assessment_terahertz`
- Length: 747 characters.
- Verbatim opening:
  `Terahertz-Verfahren (THz) können viele dielektrische Schichten (Lacke, Harze, Glasfaserlagen) durchdringen und liefern im Reflexionsmodus Kontrast an Grenzflächen.`
- Verbatim verdict close-out:
  `Erfolgsaussichten (qualitativ): hoch für die Lokalisierung der ersten Metallschicht; mittel bis hoch für die Abbildung einer Gitterstruktur, sofern keine geschlossene Folie als erste Metallschicht wirkt.`

### 6.4 Hyperspektral (VIS/NIR/SWIR) – Einordnung

- Heading (verbatim): `6.4 Hyperspektral (VIS/NIR/SWIR) – Einordnung`
- Purpose: ratchet HSI down to a coating-health module rather than a
  primary grid sensor.
- `block_id`: `detail_assessment_hyperspectral`
- Length: 633 characters.
- Verbatim opening:
  `Hyperspektrale Kameras sind primär Oberflächen- und Beschichtungssensoren. Unter typischen Luftfahrtlacksystemen ist die optische Eindringtiefe begrenzt; eine direkte geometrische Abbildung eines Kupfergitters unter Lack/Primer ist daher nur in Sonderfällen zu erwarten (z.B. sehr dünne, teiltransparente Schichten oder starke sekundäre Oberflächeneffekte).`
- Verbatim role close-out:
  `Als Ergänzung kann Hyperspektral wertvoll sein, um Beschichtungszustand, Alterung, Feuchte oder Korrosions-/Unterwanderungsindikatoren kontaktlos zu bewerten – also als „Coating Health“-Modul in einer mehrstufigen Inspektionskette.`
- Note: this chapter does NOT close with an explicit
  `Erfolgsaussichten (qualitativ):` line — its verdict is woven into the
  body. The verdict-pattern catalogue below leaves this one out
  accordingly; it is a structural exception worth preserving.

### 6.5 Mikrowelle / mmWave – Alternative mit größerem Standoff

- Heading (verbatim): `6.5 Mikrowelle / mmWave – Alternative mit größerem Standoff`
- Purpose: name microwave/mmWave as a robustness-driven fallback at
  reduced resolution.
- `block_id`: `detail_assessment_microwave`
- Length: 442 characters.
- Verbatim full body:
  `Mikrowellen- und Millimeterwellenverfahren können größere Sensorabstände erlauben und sind hardwareseitig oft robuster als THz. Die erreichbare Auflösung ist jedoch geringer; zudem ist CFK im RF-/MW-Bereich anisotrop leitfähig und verlustbehaftet, was die Auswertung erschweren kann. Als Fallback kann mmWave für großskalige Fehlstellen oder zur schnellen Vordetektion sinnvoll sein.`
- Note: like 6.4, no explicit `Erfolgsaussichten (qualitativ):` close-out
  line.

### 7. Empfohlenes Systemkonzept und Versuchsdesign — intro

- Heading (verbatim): `7. Empfohlenes Systemkonzept und Versuchsdesign`
- Purpose: summarise the multi-stage inspection concept that pairs an
  area screen with a focused characterisation step.
- `block_id`: `system_concept_intro`
- Length: 326 characters (intro paragraph + figure caption).
- Verbatim excerpt:
  `Basierend auf der Bewertung wird ein mehrstufiges Prüfkonzept empfohlen, das ein flächiges Screening mit einer gezielten Charakterisierung kombiniert (Abbildung 5).`
- Verbatim figure caption:
  `Abbildung 5: Empfohlenes mehrstufiges Prüfszenario (eigene Darstellung).`

### 7.1 Phase 0: Klärung des tatsächlichen Schichtaufbaus

- Heading (verbatim): `7.1 Phase 0: Klärung des tatsächlichen Schichtaufbaus`
- Purpose: insist that the real layup must be verified before any
  sensor decisions.
- `block_id`: `phase_0_layup_check`
- Length: 406 characters.
- Verbatim full body:
  `Vor einer sensortechnischen Festlegung sollte der reale Schichtaufbau verifiziert werden (insbesondere: Gitter vs. EMF, zusätzliche metallische Folie, Lage und Dicke der leitfähigen Lagen). Empfohlen: Schliff/Querschliff an repräsentativen Coupons oder eine einfache elektrische Durchgangs- bzw. Flächenwiderstandsmessung als erster Plausibilitätscheck.`

### 7.2 Phase 1: Coupon-Studie (Machbarkeit & Parameterraum) + Defektkatalog

- Heading (verbatim): `7.2 Phase 1: Coupon-Studie (Machbarkeit & Parameterraum)`
- Purpose: state the coupon-study goal and lay out the D1–D6 defect
  catalogue.
- `block_id`: `phase_1_coupon_study`
- Length: 868 characters (intro + table + Messgrößen sentence).
- Verbatim intro:
  `Ziel ist eine schnelle, quantitative Aussage zur Detektierbarkeit relevanter Defekte. Empfohlen wird ein Satz repräsentativer Coupons mit definierten Defektklassen und Ground Truth.`
- Verbatim defect-catalogue rows (verbatim from the document table,
  `ID` → `Beschreibung`):
  - `D1` — `Unterbrechung einzelner Stege (Schnitt/Bruch)`
  - `D2` — `Fehlender Gitterbereich / Fehlstelle (z.B. Auslassung)`
  - `D3` — `Lokaler Abbrand/Überhitzung (simuliert, z.B. durch definierte Erwärmung/Materialentfernung)`
  - `D4` — `Disbond/Delamination nahe Blitzschutzlage (eingebrachte Trennlage)`
  - `D5` — `Variierende Deckschichtdicke (Lack/Surfacer)`
  - `D6` — `Zusätzliche leitfähige Schicht (Folie) – Referenzfall zur Abschirmungsbewertung`
- Verbatim Messgrößen sentence:
  `Messgrößen (Beispiele): Detektionswahrscheinlichkeit (POD), Falschalarme, Lokalisierungsfehler, minimale detektierbare Defektgröße, Durchsatz (m²/h) und Robustheit gegenüber Abstand/Neigung.`

### 7.3 Phase 2: Prototypische Systemintegration

- Heading (verbatim): `7.3 Phase 2: Prototypische Systemintegration`
- Purpose: define the prototype-integration scope and the realistic
  test geometries.
- `block_id`: `phase_2_prototype_integration`
- Length: 434 characters.
- Verbatim full body:
  `Auf Basis der Coupon-Ergebnisse werden 1–2 Verfahren priorisiert (z.B. Induktions-Thermografie + Eddy-Current-Array oder Induktions-Thermografie + THz) und in einem prototypischen Prüfaufbau zusammengeführt. Ziel: Demonstration auf repräsentativen Bauteilgeometrien (Krümmung, Kanten, Stringer-Nähe) und Bewertung der industriellen Handhabbarkeit (Standoff, Geschwindigkeit, Bedienkonzept).`

### 8. Risiken, Abhängigkeiten und Mitigation

- Heading (verbatim): `8. Risiken, Abhängigkeiten und Mitigation`
- Purpose: name the five top risks (R1–R5) with cause and concrete
  mitigation lever each.
- `block_id`: `risk_register`
- Length: 1 090 characters.
- See `## Risk-pattern catalogue` below for the verbatim R1–R5 entries.

### 9. Fazit und Empfehlung

- Heading (verbatim): `9. Fazit und Empfehlung`
- Purpose: settle on the recommended priority list for the next project
  phase.
- `block_id`: `recommendation`
- Length: 910 characters.
- Verbatim opening:
  `Unter der Randbedingung „kontaktlos“ sind elektrische/magnetische Verfahren (Eddy Current/Induktion) und insbesondere Induktions-Thermografie die aussichtsreichsten Kandidaten für eine robuste Detektion von Gitteranomalien unter Beschichtung. THz-Imaging ist eine sehr interessante Ergänzung für Schicht-/Grenzflächeninformation, sofern der Schichtaufbau keine geschlossene Metallfolie als erste reflektierende Schicht enthält.`
- Verbatim recommendation bullets:
  - `Induktions-Thermografie (EC+IR) als flächiges Screening (Single-Shot-tauglich).`
  - `Eddy-Current-Array bzw. B-Feld-Imaging zur präzisen Abbildung von Leiterdefekten.`
  - `THz (Reflexion/TDS) als Ergänzung, abhängig vom realen Schichtaufbau.`
  - `HSI als Coating-Health-Sensor (optional).`

### Anhang A: Quellen und Literatur (Auswahl)

- Heading (verbatim): `Anhang A: Quellen und Literatur (Auswahl)`
- Purpose: list the 16 sources that anchor the study; this is the
  smoke-test list for `public_research`'s Crossref resolution.
- `block_id`: `evidence_register_appendix`
- Length: 3 481 characters.
- See `## Citation register` below for the 16 entries verbatim.

## Verdict-pattern catalogue

Verbatim transcript of every `Erfolgsaussichten (qualitativ): …` line that
appears in the document body. This is the goldreferenz for how
feasibility-study verdicts are phrased — which level words (`hoch`,
`sehr hoch`, `mittel`, `mittel–hoch`, `niedrig`) get paired with which
qualifying clauses.

- Kap. 6.1 (Eddy Current / Induktion):
  `Erfolgsaussichten (qualitativ): sehr hoch für Gitterabbildung und Gitterdefekte; mittel für reine Delamination ohne elektrische Auswirkung.`
- Kap. 6.2 (Induktions-Thermografie):
  `Erfolgsaussichten (qualitativ): hoch bis sehr hoch für Gitter und Defekte, hoch für Delamination nahe der Blitzschutzlage.`
- Kap. 6.3 (Terahertz-Imaging):
  `Erfolgsaussichten (qualitativ): hoch für die Lokalisierung der ersten Metallschicht; mittel bis hoch für die Abbildung einer Gitterstruktur, sofern keine geschlossene Folie als erste Metallschicht wirkt.`

Note (structural): chapters 6.4 (Hyperspektral) and 6.5 (Mikrowelle/
mmWave) intentionally do not close with the formal
`Erfolgsaussichten (qualitativ):` template — their verdict is woven into
the body text. A robust LINT-VERDICT-MISMATCH implementation must accept
the absence of that line as long as the matrix and the body conclusion
agree (i.e. it can only fire when the verdict line is present and
contradicts the matrix).

Pattern observations (used by the asset pack):

- The verdict line ALWAYS uses the exact rubric vocabulary
  (`niedrig`, `mittel`, `hoch`, `sehr hoch`) plus the bridge labels
  (`mittel bis hoch`, `niedrig–mittel`, `mittel–hoch`).
- The level word is ALWAYS qualified by the axis on which it applies
  (`sehr hoch für Gitterabbildung und Gitterdefekte`), never standing
  alone.
- A second clause typically introduces the bounding condition
  (`mittel für reine Delamination ohne elektrische Auswirkung`,
  `sofern keine geschlossene Folie als erste Metallschicht wirkt`).

## Risk-pattern catalogue

Verbatim transcript of the five risk entries from Kap. 8, each with its
mitigation. The structural pattern is `cause → mitigation lever`, with
the lever expressed as a concrete engineering decision rather than as a
generic process step.

- `R1: Abschirmung durch geschlossene Metallfolie: THz/MW und bestimmte induktive Geometrien sehen primär die erste leitfähige Schicht. Mitigation: Schichtaufbau verifizieren; Anregungsfrequenz/Coil-Design anpassen; ggf. auf thermische Signaturen (Induktions-Thermografie) ausweichen.`
  - Pattern: physical-mechanism cause → engineering-design lever
    (frequency/coil) plus a fallback method.

- `R2: Variabler Sensorabstand (Lift-off): ECT-Signale reagieren stark auf Abstand. Mitigation: mechanische Abstandshilfe, Kalibrierung, Mehrfrequenz-Auswertung, oder kamera-basierte Distanzmessung zur Korrektur.`
  - Pattern: signal-physics cause → mechanical + algorithmic mitigation
    stack.

- `R3: CFK-Leitfähigkeit/Anisotropie: Kann EM-Messungen beeinflussen. Mitigation: Fokus auf Kupferdominanz; geeignete Frequenzen; Referenzmessungen in unkritischen Bereichen.`
  - Pattern: material-property cause → measurement-design lever
    (Kupferdominanz, Frequenzwahl, Referenzmessung).

- `R4: Thermische Randbedingungen: Thermografie/Induktion hängt von Konvektion, Emissivität, Umgebungsbedingungen ab. Mitigation: kontrollierte Anregung, kurze Messzeiten, Emissivitätskorrektur, Lock-in-Auswertung, Referenzflächen.`
  - Pattern: environmental cause → controlled excitation + emissivity
    correction + Lock-in.

- `R5: Operative Restriktionen (Röntgen): Strahlenschutz, Logistik, Durchsatz. Mitigation: Röntgen nur als Verifikations-/Stichprobenverfahren in Stufe C vorsehen.`
  - Pattern: operational/regulatory cause → scoping mitigation (use only
    as verification step).

The asset pack's `risk_register` template MUST emit risks in this
`R<n>: <cause-headline>: <mechanism>. Mitigation: <concrete levers>.`
shape; the `cause-headline → mechanism → mitigation levers` triplet is
how the lints distinguish a real risk register from a generic
"there are challenges and we will address them" placeholder.

## Citation register

Verbatim transcript of all 16 entries from Anhang A. Each entry is
annotated with: first-author family name, year, venue, DOI/URL.

1. `[1] Segui, J. Protecting Aircraft Composites from Lightning Strike Damage. COMSOL Blog, 2015. https://www.comsol.com/blogs/protecting-aircraft-composites-from-lightning-strike-damage`
   — Segui · 2015 · COMSOL Blog · URL (no DOI)
2. `[2] Gagné, M.; Therriault, D. Lightning strike protection of composites. Progress in Aerospace Sciences 64 (2014) 1–16. https://doi.org/10.1016/j.paerosci.2013.07.002`
   — Gagné · 2014 · Progress in Aerospace Sciences · 10.1016/j.paerosci.2013.07.002
3. `[3] Towsyfyan, H.; Biguri, A.; Boardman, R.; Blumensath, T. Successes and challenges in non-destructive testing of aircraft composite structures. Chinese Journal of Aeronautics 33(3) (2020) 771–791. https://doi.org/10.1016/j.cja.2019.09.017`
   — Towsyfyan · 2020 · Chinese Journal of Aeronautics · 10.1016/j.cja.2019.09.017
4. `[4] Feng, B.; Pasadas, D.J.; Ribeiro, A.L.; Ramos, H.G. Eddy Current Testing of the Lightning Strike Protection Layer in Aerospace Composite Structures. In: Electromagnetic Non-Destructive Evaluation (XXIII), IOS Press (2020). Open Access. https://doi.org/10.3233/SAEM200004`
   — Feng · 2020 · ENDE XXIII (IOS Press) · 10.3233/SAEM200004
5. `[5] Oswald‑Tranta, B. Inductive thermography – review of a non-destructive inspection technique for surface crack detection. Quantitative InfraRed Thermography Journal (2025). https://doi.org/10.1080/17686733.2024.2448049`
   — Oswald‑Tranta · 2025 · QIRT Journal · 10.1080/17686733.2024.2448049
6. `[6] Liu, R.; Xu, C.; Liu, P.; et al. Eddy current pulsed thermography with an inductive heating layer (ECPT‑IHL) for subsurface defect detection in GFRP materials. Composites Part B: Engineering 290 (2025) 111982. https://doi.org/10.1016/j.compositesb.2024.111982`
   — Liu · 2025 · Composites Part B: Engineering · 10.1016/j.compositesb.2024.111982
7. `[7] Tian, G.Y.; Gao, Y.; Li, K.; Wang, Y.; Gao, B.; He, Y. Eddy Current Pulsed Thermography with Different Excitation Configurations for Metallic Material and Defect Characterization. Sensors 16(6) (2016) 843. https://doi.org/10.3390/s16060843`
   — Tian · 2016 · Sensors · 10.3390/s16060843
8. `[8] Ciampa, F.; Mahmoodi, P.; Pinto, F.; Meo, M. Recent Advances in Active Infrared Thermography for Non-Destructive Testing of Aerospace Components. Sensors 18(2) (2018) 609. https://doi.org/10.3390/s18020609`
   — Ciampa · 2018 · Sensors · 10.3390/s18020609
9. `[9] Li, X.; Li, J.; Li, Y.; et al. High-throughput terahertz imaging: progress and challenges. Light: Science & Applications 12 (2023) 233. https://doi.org/10.1038/s41377-023-01278-0`
   — Li · 2023 · Light: Science & Applications · 10.1038/s41377-023-01278-0
10. `[10] Ellrich, F.; Bauer, M.; Schreiner, N.; et al. Terahertz Quality Inspection for Automotive and Aviation Industries. Journal of Infrared, Millimeter, and Terahertz Waves 41 (2020) 470–489. https://doi.org/10.1007/s10762-019-00639-4`
    — Ellrich · 2020 · J. Infrared, Millimeter, and Terahertz Waves · 10.1007/s10762-019-00639-4
11. `[11] Rahman, M.S. ur; Abou‑Khousa, M.A.; Akbar, M.F. A review on microwave non-destructive testing (NDT) of composites. Engineering Science and Technology, an International Journal 58 (2024) 101848. https://doi.org/10.1016/j.jestch.2024.101848`
    — Rahman · 2024 · Engineering Science and Technology, an International Journal · 10.1016/j.jestch.2024.101848
12. `[12] Brinker, K.; Dvorsky, M.; Al Qaseer, M.T.; Zoughi, R. Review of advances in microwave and millimetre-wave NDT&E: principles and applications. Philosophical Transactions of the Royal Society A 378(2182) (2020) 20190585. https://doi.org/10.1098/rsta.2019.0585`
    — Brinker · 2020 · Philosophical Transactions of the Royal Society A · 10.1098/rsta.2019.0585
13. `[13] Hung, Y.Y. Shearography for non-destructive evaluation of composite structures. Optics and Lasers in Engineering 24 (1996). https://doi.org/10.1016/0143-8166(95)00020-8`
    — Hung · 1996 · Optics and Lasers in Engineering · 10.1016/0143-8166(95)00020-8
14. `[14] Zarei, A.; Pilla, S. Laser ultrasonics for nondestructive testing of composite materials and structures: A review. Ultrasonics 136 (2024) 107163. https://doi.org/10.1016/j.ultras.2023.107163`
    — Zarei · 2024 · Ultrasonics · 10.1016/j.ultras.2023.107163
15. `[15] Gholizadeh, S. A review of non-destructive testing methods of composite materials. Procedia Structural Integrity 1 (2016) 50–57. https://doi.org/10.1016/j.prostr.2016.02.008`
    — Gholizadeh · 2016 · Procedia Structural Integrity · 10.1016/j.prostr.2016.02.008
16. `[16] Hu, T.; Yu, J.; et al. Lightning Performance of Copper‑Mesh Clad Composite Panels: Test and Simulation. Coatings 9(11) (2019) 727. https://doi.org/10.3390/coatings9110727`
    — Hu · 2019 · Coatings · 10.3390/coatings9110727

Smoke-test rule: a deep-research run on the RASCON topic is expected to
resolve all 16 of these entries through Crossref (or the explicit URL,
in the case of [1] which is a blog post). LINT-DOI-NOT-RESOLVED MUST
fire only on entries with `crossref_status` other than `ok` for entries
[2]–[16], and never on [1] (the COMSOL blog has no DOI by design).

## Anti-slop reference points

Three concrete things RASCON does that an AI-generated feasibility study
typically fails. Each item shows a verbatim RASCON excerpt against a
contrasting synthetic AI-slop example.

### 1. Method paragraphs that name a specific physical mechanism

RASCON (Kap. 6.1, opening sentence verbatim):
`Induktive Verfahren regen Wirbelströme in leitfähigen Strukturen an und messen die resultierende Änderung des Magnetfelds bzw. der Sondenimpedanz.`

Synthetic AI slop example (would trigger LINT-FILLER-OPENING and likely
LINT-DEAD-PHRASE):
`Im Rahmen dieser Studie werden induktive Verfahren betrachtet, die im Bereich der zerstörungsfreien Prüfung eine hohe Relevanz aufweisen und vielversprechende Möglichkeiten eröffnen.`

Key contrast: the RASCON sentence names the energy mechanism
(`Wirbelströme`), the measured quantity (`Änderung des Magnetfelds bzw.
der Sondenimpedanz`), and ties them to the medium (`leitfähigen
Strukturen`). The slop sentence wraps abstract framings around a method
name without ever describing what is measured.

### 2. Matrix-cell rationales with axis-specific reasoning

RASCON (Kap. 5.2, the row "Eddy Current (ECT) / Arrays") assigns a
different value to each of the five axes
(`mittel` Fläche, `sehr hoch` Gitterbild, `sehr hoch` Gitterdefekt,
`mittel` Delamination, `hoch` Reifegrad), and Kap. 6.1 explains *why*
the gitter-axes are `sehr hoch` (Kupfergitter-Kopplung,
Stromverteilungsänderungen bei Unterbrechungen) without re-using the
same sentence for any other axis.

Synthetic AI slop example (would trigger LINT-DUPLICATE-RATIONALE):
`Eddy Current — Fläche: hoch, weil sehr robust und industrieerprobt. Gitterbild: hoch, weil sehr robust und industrieerprobt. Gitterdefekt: hoch, weil sehr robust und industrieerprobt. Delamination: hoch, weil sehr robust und industrieerprobt. Reifegrad: hoch, weil sehr robust und industrieerprobt.`

Key contrast: the slop pastes the same rationale into every cell. The
RASCON matrix rotates the rationale per axis (single-shot Fläche,
Leiterabbildung, Defektsensitivität, Wärmefluss-/Stromfluss-Effekte,
TRL).

### 3. Explicit limitations tied to the layup variant

RASCON (Kap. 5.3 footnote, second sentence verbatim):
`Für THz gilt hingegen: Eine geschlossene Metallschicht wirkt i.d.R. als starke Reflexions-/Abschirmbarriere.`

Synthetic AI slop example (would trigger LINT-UNANCHORED-HEDGE):
`Möglicherweise ist Terahertz unter bestimmten Umständen geeignet, hängt aber von verschiedenen Faktoren ab.`

Key contrast: the RASCON sentence binds the limitation to a specific
layup feature (`geschlossene Metallschicht`) and to a specific physical
mechanism (`Reflexions-/Abschirmbarriere`). The slop sentence hedges
without anchor: it would not survive LINT-UNANCHORED-HEDGE and offers
no engineering signal a reader can act on.

## How to use this archetype

This file is the structural goldreferenz for the deep-research skill.
Operator guidance:

- This is the structural template the deep-research skill aims to
  reproduce in the feasibility-study domain. A complete deep-research
  run on a comparable topic (a contactless inspection method for an
  aerospace composite structure) is expected to produce a document with
  a similar overall shape: ~25 chapters / sub-chapters at headings 1–2,
  one Management Summary block of ~1 500–2 000 chars, one main
  evaluation matrix with 5 axes × ~6–8 candidate methods, a scenario
  matrix conditional on layup variants, four to six method-detail
  chapters of ~600–1 100 chars each, a multi-stage system concept
  (Phase 0/1/2), a risk register R1–Rn with cause→mitigation pattern,
  and an evidence appendix of 14–18 sources with at least 90% DOI
  resolution rate.
- This file is NEVER fed directly into a sub-skill prompt. The asset
  pack (`references/asset_pack.json` and the future `style_guide`)
  distills the patterns documented here into block templates, axis
  rubrics, and lint thresholds. Sub-skills only see the asset pack.
- This file is the smoke-test target. When the deep-research skill is
  run end-to-end on a RASCON-comparable topic, the output is graded
  against this archetype: chapter count, length per chapter, matrix
  axis count, verdict-line cadence, citation density, citation
  resolution rate. The deltas surface as findings in the run's mission-
  review record, never inside the produced document itself.
- Excerpts in this file are short and verbatim. Anyone updating this
  file MUST keep every quoted excerpt verbatim and identical to the
  source. Paraphrased excerpts are a regression: they break the
  goldreferenz contract and weaken the lints anchored against it.

## Sibling archetypes (placeholders)

RASCON is the only filled goldreferenz for now. The six other report
types the deep-research skill supports each need their own archetype
when an operator supplies a real reference dossier. Until then, the
following expected-shape notes are the structural anchor lints and
sub-skills fall back on.

### `market_research`

- Expected source: a decision-grade market-analysis dossier from a
  recognised analyst house or operator-supplied sample.
- Expected structure (block sequence): `market_overview` →
  `market_sizing` → `segments` → `customer_jobs` → `value_chain` →
  `demand_drivers` → `barriers_and_risks` → `competitor_landscape` →
  `entry_options` → `recommendation_market`.
- Expected length: ≈ 28 000 chars total.
- Expected citation density: 15–25 distinct sources, evenly
  distributed across segments and competitors. No single source above
  25% of `used_reference_ids[]`.
- Expected register: third-person analyst (no first-person plural);
  named segments and named competitors over generic placeholders;
  every quantitative claim sourced.
- Verdict-line pattern: `Marktattraktivität (qualitativ): <level> für
  <segment>` (vocabulary `niedrig|mittel|hoch`).

### `competitive_analysis`

- Expected source: a competitive-positioning dossier from a strategy
  team or operator-supplied sample.
- Expected structure: `competitor_set` → `capability_axes` →
  `competitor_matrix` → `price_position_map` → `channel_overlap` →
  `moat_assessment` → `gap_to_close` → `recommendation_competitive`.
- Expected length: ≈ 22 000 chars total.
- Expected citation density: 10–18 distinct sources, weighted toward
  named-competitor evidence (annual reports, product-page captures,
  pricing pages, analyst notes).
- Expected register: third-person analyst; competitor names spelled
  consistently and in their official form; capability claims grounded
  in observable evidence (no "we believe").
- Verdict-line pattern: `Wettbewerbsstellung: <level> in <dimension>`
  (vocabulary `weak|moderate|strong|leading`).

### `technology_screening`

- Partial archetype: RASCON Kapitel 5 (the `Technologie-Screening`
  section, the qualitative evaluation matrix, the scenario-conditional
  matrix, and the short assessment of secondary methods) is a clean
  partial archetype. The screening logic and matrix structure
  transfer cleanly.
- The feasibility-specific `detail_assessment_per_option` block does
  **not** transfer to a pure technology-screening run. Screening stops
  at the shortlist; detail assessment belongs in a follow-up
  feasibility study.
- Expected source for a full technology-screening archetype:
  operator-supplied. Until then, RASCON Kap. 5 is referenced from
  the asset pack `reference_archetypes[id="rascon_archetype"].partial_for[]`.
- Expected length: ≈ 14 000 chars total.
- Expected register: same scientific register as feasibility, but
  more compact and method-comparative rather than method-deep.
- Verdict-line pattern: `Eignung (qualitativ): <level> für <use case>`
  (vocabulary `niedrig|mittel|hoch`).

### `whitepaper`

- Expected source: position paper from a recognised industry or
  research org (e.g. an Acatempo / VDI / IEEE position paper).
- Expected structure: `thesis` → `context_and_problem` →
  `argument_section` (repeated 3–6 times) → `counter_arguments` →
  `implications` → `closing_position`.
- Expected length: ≈ 18 000 chars total.
- Expected citation density: 8–15 sources; quantitative claims must
  be sourced, but the thesis itself is a position and is allowed to
  be unsourced (this is the rationale for `LINT-UNCITED-CLAIM` being
  conditional on whitepaper).
- Expected register: position-bearing argumentative; first-person
  plural of the publishing org is allowed where the org is named at
  the top; otherwise third-person.
- Verdict-line pattern: `null`. No quantified verdict line. The
  closing_position block carries the position as prose.

### `literature_review`

- Expected source: a published systematic review (e.g. Annual
  Reviews entry, Cochrane review style, or a peer-reviewed scoping
  review).
- Expected structure: `scope_and_method` → `theme_overview` →
  `theme_section` (repeated 4–8 times) → `synthesis` →
  `gaps_and_open_questions` → `appendix_sources_review`.
- Expected length: ≈ 22 000 chars total.
- Expected citation density: 40–80 sources, distributed evenly
  across themes (LINT-LR-THEME-IMBALANCE guards the lower bound).
- Expected register: third-person academic synthesis; no narrative
  voice intrusion; passive constructions are acceptable when they
  preserve agent-method clarity.
- Verdict-line pattern: `null`. The synthesis block summarises
  consensus and disagreement; no qualitative-level verdict.

### `decision_brief`

- Expected source: a McKinsey/BCG decision memo or a German
  Vorstandsvorlage.
- Expected structure: `decision_at_stake` → `situation` →
  `options_summary` → `criteria` → `evaluation` →
  `recommendation_brief` → `caveats_and_next_steps`.
- Expected length: ≈ 8 000 chars total. Compact by design.
- Expected citation density: 5–10 sources; recommendation must rest
  on cited criteria evaluation, not on persuasion.
- Expected register: compact, action-oriented, position-bearing.
  Recommendation is **front-loaded** (LINT-DB-RECOMMENDATION-BURIED
  fires when buried in the second half of the document).
- Verdict-line pattern: `Empfehlung: <level>` (vocabulary
  `recommend|recommend with caveats|not recommended`).

## Cross-type structural lessons from RASCON

Three things RASCON does well are universal moves; three are
feasibility-specific; three need re-tuning per type. Use these notes
when adapting the archetype patterns to other runs.

### What translates (universal moves)

- **Evidence-anchored claims.** Every method-claim, scenario-claim,
  and risk-claim in RASCON is backed by an explicit source from
  Anhang A. This pattern is universal: it works for market_research
  segment-claims, competitive_analysis capability-claims,
  whitepaper-argument claims, literature_review theme-claims, and
  decision_brief criterion-evaluation claims. The lints
  `LINT-FAB-DOI`, `LINT-CITED-BUT-MISSING`, `LINT-UNCITED-CLAIM`
  carry across all seven types.
- **Named items over generic placeholders.** RASCON names methods
  (`Eddy Current`, `Induktions-Thermografie`, `Terahertz-Imaging`)
  rather than saying "various contactless methods". The same
  discipline applies to market_research (named segments and named
  competitors) and decision_brief (named options). Lint
  `LINT-MR-COMPETITOR-NAMELESS` is the cross-type analogue.
- **Register discipline.** RASCON stays in scientific-engineering
  register throughout. The `style_profiles` differ per report type
  (scientific_engineering_dossier vs market_analyst_dossier vs
  policy_brief_dossier vs academic_review_dossier), but the
  discipline of staying-in-register is universal. Lint
  `LINT-INVERTED-PERSPECTIVE` is the cross-type guard.
- **Prioritised recommendation list.** RASCON Kap. 9 closes with a
  paragraph plus four prioritised bullets (induction-thermography
  primary, eddy-current array supporting, THz conditional, HSI
  optional). This pattern transfers cleanly to market_research
  recommendation_market, competitive_analysis recommendation_competitive,
  decision_brief recommendation_brief.
- **Risk register with cause→mitigation pairs.** RASCON Kap. 8's
  R1–R5 pattern (each with cause + mitigation) translates to
  market_research barriers_and_risks, competitive_analysis
  moat_assessment risks, and decision_brief caveats_and_next_steps.
  Whitepaper and literature_review do not need a risk register.

### What does not translate (feasibility-specific moves)

- **Qualitative-matrix verdict pattern.** "Erfolgsaussichten
  (qualitativ): hoch …" is feasibility-specific. Whitepaper and
  literature_review have no quantified verdict at all
  (`verdict_line_pattern: null`). Decision_brief uses a different
  pattern (`Empfehlung: …`).
- **Scenario-A/B/C structure (Kap. 5.3).** Conditional matrix on
  layup variants is rare outside method-feasibility studies.
  Market_research uses scenarios but typically as segment-as-
  dimension, not as Boolean variants of a physical configuration.
- **Experiment-design phase plan (Phase 0/1/2).** Uniquely
  feasibility / technology-screening territory. The phased validation
  pattern does not translate to whitepaper, market_research, or
  decision_brief. It does translate, partially, to a deep
  technology-screening run that proposes a follow-up feasibility
  study.
- **Defektkatalog (D1–D6 with attributes per defect).** Feasibility
  / NDT-specific catalogue pattern. Other report types catalogue
  different things (segments, competitors, themes, options, criteria),
  but the structural pattern of "coded-id, attributes, narrative
  context" transfers — only the catalogue subject is different.

### What needs adaptation (per-type re-tuning)

- **`section_role_guidance` per type.** A market_research
  `scope_and_method` block has a different role than a feasibility
  `scope_disclaimer` block: scope_disclaimer disclaims limitations,
  scope_and_method declares method (top-down vs bottom-up,
  geographic boundary, time horizon). The asset-pack
  `style_guidance.section_role_guidance[]` lists are per type.
- **Disclaimer language varies by type.** Feasibility disclaimers
  cover "Plausibilitätsannahmen" and "Validierung an
  repräsentativen Proben". Market_research disclaimers cover
  "Methodenbasis" (top-down vs bottom-up) and "Datenstand".
  Literature_review disclaimers cover "Suchstrategie" (databases,
  search dates, exclusion criteria). `LINT-MISSING-DISCLAIMER`
  uses the type-conditional substring set declared in the asset
  pack.
- **Tone variants on the same discipline.** RASCON's "interner
  Feststellungston" (internal observational tone) works for
  technical reports. Market_research uses a "third-person analyst"
  tone — still no first-person plural, but more named-actor framing
  ("Operator X reports … ", "Analyst house Y estimates …").
  Whitepaper allows publisher first-person ("we, the AAA, hold
  that … "). Each is captured in the appropriate `style_profile`'s
  directives.
