// Default skillbooks — verbatim from store_outbound_commands.rs
// outbound_default_message_drafting_skillbook / _reply_handling_ / _scheduling_ (ref :4692-4848).

export function messageDraftingSkillbook() {
  return {
        "id": "business-os.outbound.message_drafting.v1",
        "skillbook_id": "business-os.outbound.message_drafting.v1",
        "title": "Initial- und Follow-up-Drafts vorbereiten",
        "mission": "Personalisierte Erst- und Follow-up-Anschreiben fuer eine Outbound-Campaign entwerfen, die an die ICP-Beschreibung, das konkrete Unternehmen und die Zielperson anknuepfen und ausschliesslich als freigabepflichtige Drafts entstehen.",
        "non_negotiable_rules": [
            "Keine Nachricht ohne explizite Freigabe versenden",
            "Keinen externen Dienst und keine HTTP-Schnittstelle aufrufen; ausschliesslich ueber den CTOX-Command-Bus arbeiten",
            "Suppression-, Bounce- und Opt-out-Listen jederzeit respektieren",
            "Keine Fakten erfinden; nur belegte Rechercheergebnisse und vom Operator gepflegte Angaben verwenden",
            "Sprache und Anrede an die Zielperson anpassen (Standard: Deutsch, Sie-Form)",
        ],
        "workflow_backbone": [
            {
                "step": "context_intake",
                "title": "Kontext aufnehmen",
                "detail": "drafting_request lesen: ICP-/Produktbeschreibung, CTA, Signatur, Landingpage-Checkliste, Prompt-Vorlagen sowie Unternehmens- und Personendaten.",
            },
            {
                "step": "anchor_selection",
                "title": "Anknuepfungspunkt waehlen",
                "detail": "Aus Homepage-Summary und Personendaten den staerksten, belegbaren Anknuepfungspunkt zwischen Angebot und Empfaenger ableiten.",
            },
            {
                "step": "initial_draft",
                "title": "Erstanschreiben entwerfen",
                "detail": "Betreff plus knappen Body mit einem klaren CTA schreiben; Signatur anhaengen; keine erfundenen Aussagen.",
            },
            {
                "step": "followups",
                "title": "Zwei Follow-ups entwerfen",
                "detail": "Zwei kurze, eskalationsarme Follow-ups verfassen, die ohne Antwort hoeflich nachfassen und sich auf das Erstanschreiben beziehen.",
            },
            {
                "step": "writeback",
                "title": "Ergebnis zurueckschreiben",
                "detail": "message_mail_subject, message_mail_body, message_followup_1 und message_followup_2 ausschliesslich ueber den Command outbound.pipeline.write_outreach_draft persistieren.",
            },
        ],
        "routing_taxonomy": [
            { "intent": "initial", "route": "Erstanschreiben entwerfen", "stop": "Draft als awaiting_approval ablegen" },
            { "intent": "followup", "route": "Naechstes Follow-up aus der Sequenz entwerfen", "stop": "stop on reply" },
            { "intent": "reply_received", "route": "An reply_handling.v1 uebergeben", "stop": "Keine weitere Sequenznachricht senden" },
        ],
        "stop_rules": [
            "stop on reply",
            "stop on bounce",
            "stop on opt-out",
            "stop on suppression match",
        ],
        "version_number": 1,
    };
}

export function replyHandlingSkillbook() {
  return {
        "id": "business-os.outbound.reply_handling.v1",
        "skillbook_id": "business-os.outbound.reply_handling.v1",
        "title": "Antworten klassifizieren und Reply-Drafts vorbereiten",
        "mission": "Eingehende Antworten klassifizieren, die laufende Sequenz anhalten und einen passenden, freigabepflichtigen Reply-Draft vorbereiten.",
        "non_negotiable_rules": [
            "Bei jeder Antwort die automatische Sequenz anhalten, bevor weiter entworfen wird",
            "Opt-out und Unsubscribe sofort in die Suppression-Liste ueberfuehren",
            "Keine Antwort ohne explizite Freigabe versenden",
            "Keinen externen Dienst aufrufen; nur ueber den CTOX-Command-Bus arbeiten",
        ],
        "workflow_backbone": [
            {
                "step": "classify",
                "title": "Antwort klassifizieren",
                "detail": "Antwort in positive_reply, question, objection, not_interested, out_of_office oder unsubscribe einsortieren.",
            },
            {
                "step": "halt_sequence",
                "title": "Sequenz anhalten",
                "detail": "Bei jeder echten Antwort die laufende Follow-up-Sequenz pausieren, damit keine widerspruechliche Nachricht hinausgeht.",
            },
            {
                "step": "route",
                "title": "Folgeaktion routen",
                "detail": "Auf Basis der Klassifikation Reply-Draft, Terminfindung, Suppression oder manuelle Eskalation auswaehlen.",
            },
            {
                "step": "draft_reply",
                "title": "Reply-Draft vorbereiten",
                "detail": "Knappe, kontextbezogene Antwort als freigabepflichtigen Draft ablegen.",
            },
        ],
        "routing_taxonomy": [
            { "intent": "positive_reply", "route": "Reply-Draft oder Terminfindung vorbereiten", "stop": "Sequenz beendet" },
            { "intent": "question", "route": "Antwort mit Klaerung entwerfen", "stop": "Sequenz pausiert" },
            { "intent": "objection", "route": "Einwand-Antwort entwerfen", "stop": "Sequenz pausiert" },
            { "intent": "not_interested", "route": "Hoeflich schliessen", "stop": "Engagement schliessen" },
            { "intent": "out_of_office", "route": "Follow-up nach OOO-Datum neu planen", "stop": "stop until ooo_until" },
            { "intent": "unsubscribe", "route": "In Suppression-Liste ueberfuehren", "stop": "stop on opt-out" },
        ],
        "stop_rules": [
            "stop on opt-out",
            "stop on unsubscribe",
            "stop sequence on any human reply",
        ],
        "version_number": 1,
    };
}

export function schedulingSkillbook() {
  return {
        "id": "business-os.outbound.scheduling.v1",
        "skillbook_id": "business-os.outbound.scheduling.v1",
        "title": "Terminfindung vorbereiten",
        "mission": "Auf Terminwunsch passende Slots vorschlagen, gegen Kalenderkonflikte und Arbeitszeiten pruefen und das Meeting erst nach Freigabe buchen.",
        "non_negotiable_rules": [
            "Slots nur innerhalb der konfigurierten Arbeitszeiten und Limits vorschlagen",
            "Kein Meeting ohne explizite Freigabe buchen",
            "Bei Kalenderkonflikt einen Alternativslot anbieten statt zu ueberbuchen",
        ],
        "workflow_backbone": [
            {
                "step": "collect_constraints",
                "title": "Rahmen sammeln",
                "detail": "Dauer, Zeitzone, bevorzugte Fenster und Arbeitszeiten der Campaign ermitteln.",
            },
            {
                "step": "propose_slots",
                "title": "Slots vorschlagen",
                "detail": "Zwei bis drei konkrete Slots vorschlagen, die innerhalb der Limits liegen.",
            },
            {
                "step": "check_conflicts",
                "title": "Konflikte pruefen",
                "detail": "Vorgeschlagene Slots gegen Kalender und bestehende Buchungen pruefen und Konflikte aussortieren.",
            },
            {
                "step": "book_on_approval",
                "title": "Nach Freigabe buchen",
                "detail": "Erst nach Operator-Freigabe per outbound.scheduling.mark_booked buchen.",
            },
        ],
        "routing_taxonomy": [
            { "intent": "slot_request", "route": "Slots vorschlagen", "stop": "Auf Empfaengerwahl warten" },
            { "intent": "slot_confirmed", "route": "Buchung nach Freigabe vorbereiten", "stop": "stop until approved" },
            { "intent": "conflict", "route": "Alternativslot vorschlagen", "stop": "Nicht ueberbuchen" },
        ],
        "stop_rules": [
            "stop on reply",
            "stop until approved before booking",
            "stop on calendar conflict",
        ],
        "version_number": 1,
    };
}
