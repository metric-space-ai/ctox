"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { usePathname } from "next/navigation";

type ContextItem = {
  action?: string;
  currentUrl: string;
  label: string;
  moduleId: string;
  recordId: string;
  recordType: string;
  selectedText?: string;
  submoduleId: string;
};

type MenuState = {
  href?: string;
  item: ContextItem;
  actions: ContextMenuAction[];
  x: number;
  y: number;
};

type ContextMenuAction = {
  description: string;
  direct?: boolean;
  id: string;
  label: string;
  prompt?: (item: ContextItem) => string;
};

type QueueResponse = {
  ok?: boolean;
  core?: { mode?: string; taskId?: string | null };
  error?: string;
};

const copy = {
  de: {
    close: "Schliessen",
    context: "Kontext",
    defaultPrompt: (item: ContextItem) => {
      const selected = item.selectedText ? `\nAuswahl: ${item.selectedText}` : "";
      return `Bearbeite diesen Kontext in CTOX: ${item.label}. Modul: ${item.moduleId}/${item.submoduleId}. Datensatz: ${item.recordType} ${item.recordId}.${selected}`;
    },
    open: "Details oeffnen",
    prompt: "CTOX Prompt",
    done: "OK",
    failed: "Aktion fehlgeschlagen",
    submit: "An CTOX uebergeben"
  },
  en: {
    close: "Close",
    context: "Context",
    defaultPrompt: (item: ContextItem) => {
      const selected = item.selectedText ? `\nSelection: ${item.selectedText}` : "";
      return `Work on this context in CTOX: ${item.label}. Module: ${item.moduleId}/${item.submoduleId}. Record: ${item.recordType} ${item.recordId}.${selected}`;
    },
    open: "Open details",
    prompt: "CTOX Prompt",
    done: "OK",
    failed: "Action failed",
    submit: "Send to CTOX"
  }
};

export function ContextMenuBridge({ locale }: { locale?: string }) {
  const pathname = usePathname();
  const activeLocale = locale === "de" ? "de" : "en";
  const activeLocaleRef = useRef<"de" | "en">(activeLocale);
  const t = copy[activeLocale];
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [prompt, setPrompt] = useState<{ item: ContextItem; text: string } | null>(null);
  const [status, setStatus] = useState<"idle" | "submitting" | "queued" | "error">("idle");
  const [message, setMessage] = useState("");

  useEffect(() => {
    activeLocaleRef.current = activeLocale;
  }, [activeLocale]);

  useEffect(() => {
    const close = () => setMenu(null);
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setMenu(null);
        setPrompt(null);
      }
    };
    window.addEventListener("click", close);
    window.addEventListener("scroll", close, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  useEffect(() => {
    setMenu(null);
    setPrompt(null);
  }, [pathname]);

  useEffect(() => {
    const onContextMenu = (event: MouseEvent) => {
      const source = event.target instanceof Element ? event.target : null;
      if (!source || source.closest(".ops-svar-gantt-shell, .context-menu, .ctox-prompt-panel")) return;
      if (source.closest("input, textarea, select, [contenteditable='true']")) return;

      const target =
        source.closest<HTMLElement>("[data-context-item]") ??
        source.closest<HTMLElement>("[data-context-scope]") ??
        source.closest<HTMLElement>("[data-context-module][data-context-submodule]");
      if (!target) return;

      const item = contextItemFromElement(target);
      if (!item) return;

      event.preventDefault();
      event.stopPropagation();
      const actions = contextMenuActions(item, activeLocaleRef.current);
      setPrompt(null);
      setMenu({
        actions,
        href: target instanceof HTMLAnchorElement ? target.href : target.closest<HTMLAnchorElement>("a")?.href,
        item,
        ...fitMenuPosition(event.clientX, event.clientY, actions.length)
      });
    };

    window.addEventListener("contextmenu", onContextMenu, true);
    return () => window.removeEventListener("contextmenu", onContextMenu, true);
  }, []);

  const contextTokens = useMemo(() => {
    if (!prompt) return [];
    return [
      prompt.item.moduleId,
      prompt.item.submoduleId,
      prompt.item.recordType,
      prompt.item.recordId
    ].filter(Boolean);
  }, [prompt]);
  const menuActions = menu?.actions ?? [];

  return (
    <>
      {menu ? (
        <div className="context-menu" role="menu" style={{ left: menu.x, top: menu.y }}>
          <button
            className="context-menu-primary"
            onClick={(event) => {
              event.stopPropagation();
              setMenu(null);
              setStatus("idle");
              setMessage("");
              setPrompt({ item: menu.item, text: t.defaultPrompt(menu.item) });
            }}
            role="menuitem"
            type="button"
          >
            <span>{t.prompt}</span>
            <small>{menu.item.label}</small>
          </button>
          {menu.href ? (
            <button
              onClick={(event) => {
                event.stopPropagation();
                setMenu(null);
                window.dispatchEvent(new CustomEvent("ctox:navigate", { detail: { href: menu.href } }));
              }}
              role="menuitem"
              type="button"
            >
              <span>{t.open}</span>
              <small>{menu.item.recordType}</small>
            </button>
          ) : null}
          {menuActions.map((action) => (
            <button
              key={action.id}
              onClick={(event) => {
                event.stopPropagation();
                setMenu(null);
                if (action.direct) {
                  window.dispatchEvent(new CustomEvent("ctox:context-action", { detail: { actionId: action.id, item: menu.item } }));
                } else if (action.prompt) {
                  setStatus("idle");
                  setMessage("");
                  setPrompt({ item: menu.item, text: action.prompt(menu.item) });
                }
              }}
              role="menuitem"
              type="button"
            >
              <span>{action.label}</span>
              <small>{action.description}</small>
            </button>
          ))}
        </div>
      ) : null}

      {prompt ? (
        <section className="ctox-prompt-panel" aria-label={t.prompt}>
          <header>
            <div>
              <strong>{t.prompt}</strong>
              <span>{prompt.item.label}</span>
            </div>
            <button onClick={() => setPrompt(null)} type="button">{t.close}</button>
          </header>
          <div className="ctox-prompt-context" aria-label={t.context}>
            {contextTokens.map((token) => <span key={token}>{token}</span>)}
          </div>
          <textarea
            onChange={(event) => setPrompt((current) => current ? { ...current, text: event.target.value } : current)}
            value={prompt.text}
          />
          <footer>
            <button
              disabled={status === "submitting" || prompt.text.trim().length === 0}
              onClick={async () => {
                setStatus("submitting");
                setMessage("");
                const result = await postQueue({
                  instruction: prompt.text,
                  context: {
                    source: "right-click-context",
                    currentUrl: prompt.item.currentUrl,
                    items: [prompt.item]
                  }
                });
                if (result.ok) {
                  setStatus("queued");
                  setMessage(t.done);
                } else {
                  setStatus("error");
                  setMessage(result.error ?? t.failed);
                }
              }}
              type="button"
            >
              {status === "submitting" ? "..." : t.submit}
            </button>
            {message ? <span>{message}</span> : null}
          </footer>
        </section>
      ) : null}
    </>
  );
}

function contextItemFromElement(element: HTMLElement): ContextItem | null {
  const dataset = element.dataset;
  const moduleId = dataset.contextModule;
  const submoduleId = dataset.contextSubmodule;
  const recordType = dataset.contextRecordType ?? "workspace";
  const recordId = dataset.contextRecordId ?? `${moduleId}-${submoduleId}`;
  const label = dataset.contextLabel ?? element.getAttribute("aria-label") ?? element.textContent?.trim() ?? `${moduleId}/${submoduleId}`;
  if (!moduleId || !submoduleId || !recordType || !recordId || !label) return null;

  const selectedText = window.getSelection()?.toString().trim();
  return {
    action: dataset.contextAction,
    currentUrl: window.location.href,
    label,
    moduleId,
    recordId,
    recordType,
    selectedText: selectedText || undefined,
    submoduleId
  };
}

function contextMenuActions(item: ContextItem, locale: "de" | "en"): ContextMenuAction[] {
  const de = locale === "de";
  const actions: ContextMenuAction[] = [];
  const add = (id: string, label: string, description: string, prompt: (item: ContextItem) => string) => {
    actions.push({ id, label, description, prompt });
  };
  const addDirect = (id: string, label: string, description: string) => {
    actions.push({ description, direct: true, id, label });
  };

  if (item.action === "create") {
    add(
      "create",
      de ? "Neu anlegen" : "Create new",
      item.recordType,
      (ctx) => de
        ? `Lege ${ctx.label} im Modul ${ctx.moduleId}/${ctx.submoduleId} an. Nutze den aktuellen Kontext und erstelle sinnvolle Pflichtfelder, naechste Schritte und Verknuepfungen.`
        : `Create ${ctx.label} in ${ctx.moduleId}/${ctx.submoduleId}. Use the current context and prepare required fields, next steps, and links.`
    );
  }

  if (item.moduleId === "sales") {
    if (item.submoduleId === "campaigns") {
      add("campaign-research", de ? "Research starten" : "Start research", de ? "Quelle, Firma, Kontakt, Touchpoint" : "Source, company, contact, touchpoint", (ctx) => de
        ? `Arbeite an dieser Kampagne: ${ctx.label}. Pruefe Importquelle, Firmenstammdaten, Ansprechpartner, Touchpoints, Ansprache und Versandbereitschaft. Gib klare Luecken und naechste Aktionen zurueck.`
        : `Work on this campaign: ${ctx.label}. Check source import, company data, contacts, touchpoints, outreach, and send readiness. Return gaps and next actions.`);
      add("campaign-send", de ? "Versand pruefen" : "Check sending", de ? "Mailkonto, Freigabe, Rate-Limits" : "Account, approval, rate limits", (ctx) => de
        ? `Pruefe den Versandkontext fuer ${ctx.label}. Beruecksichtige Mailkonto, Freigabe, Tageslimit, Ruhezeiten, Bounces, Abmeldungen und ob alle Anschreiben wirklich versandbereit sind.`
        : `Check the sending context for ${ctx.label}. Consider mail account, approval, daily cap, quiet hours, bounces, unsubscribes, and whether all messages are ready.`);
    } else if (item.submoduleId === "pipeline") {
      add("pipeline-transition", de ? "Transition pruefen" : "Check transition", de ? "Gate, Blocker, naechste Stufe" : "Gate, blockers, next stage", (ctx) => de
        ? `Pruefe diesen Pipeline-Kontext: ${ctx.label}. Sind Gate-Voraussetzungen, Blocker, naechste Aktion und Uebergabe in die naechste Stufe klar?`
        : `Check this pipeline context: ${ctx.label}. Are gate requirements, blockers, next action, and handoff to the next stage clear?`);
    } else if (item.submoduleId === "leads") {
      add("lead-plan", de ? "Plan-Schritt vorschlagen" : "Suggest plan step", de ? "Flow, Folgeaktion, Automatisierung" : "Flow, follow-up, automation", (ctx) => de
        ? `Schlage fuer diesen Lead-Flow den naechsten sinnvollen Plan-Schritt vor: ${ctx.label}. Unterscheide erledigte Evidenz von geplanten Aktionen und nenne Trigger fuer die Ausfuehrung.`
        : `Suggest the next useful plan step for this lead flow: ${ctx.label}. Distinguish completed evidence from planned actions and name execution triggers.`);
    } else if (item.submoduleId === "offers") {
      add("offer-review", de ? "Angebot pruefen" : "Review offer", de ? "Positionen, Steuer, Freigabe" : "Lines, tax, approval", (ctx) => de
        ? `Pruefe das Angebot ${ctx.label}: Positionen, Summen, Steuer, Gueltigkeit, Kundentext und Uebergabe nach Annahme.`
        : `Review offer ${ctx.label}: lines, totals, tax, validity, customer text, and handoff after acceptance.`);
    } else if (item.submoduleId === "customers") {
      add("customer-dossier", de ? "Dossier aktualisieren" : "Update dossier", de ? "Buying Center, Akte, Risiken" : "Buying center, file, risks", (ctx) => de
        ? `Aktualisiere das Kundendossier fuer ${ctx.label}. Ergaenze Stammdaten, Buying Center, offene Risiken, Operations-Kontext und naechste sinnvolle Pflegeaktion.`
        : `Update the customer dossier for ${ctx.label}. Add master data, buying center, open risks, operations context, and next maintenance action.`);
    }
  }

  if (item.moduleId === "marketing") {
    add("marketing-research", de ? "Research ableiten" : "Derive research", de ? "Signale, Quellen, Content" : "Signals, sources, content", (ctx) => de
      ? `Leite aus diesem Marketing-Kontext ${ctx.label} sinnvolle Research-, Content- und Kampagnenaktionen ab. Nenne Quellen, Risiken und verwertbare Assets.`
      : `Derive useful research, content, and campaign actions from this marketing context ${ctx.label}. Name sources, risks, and usable assets.`);
  }

  if (item.moduleId === "operations") {
    if (item.submoduleId === "projects") {
      add("project-plan", de ? "Projektplan pruefen" : "Check project plan", de ? "Gantt, Abhaengigkeiten, Risiken" : "Gantt, dependencies, risks", (ctx) => de
        ? `Pruefe den Projektkontext ${ctx.label}. Bewerte Gantt-Zeitraum, Abhaengigkeiten, Risiken, Blocker und naechste Work Items.`
        : `Check project context ${ctx.label}. Assess Gantt timeline, dependencies, risks, blockers, and next work items.`);
    } else if (item.submoduleId === "work-items") {
      add("work-status", de ? "Work Item klaeren" : "Clarify work item", de ? "Status, Owner, Blocker" : "Status, owner, blocker", (ctx) => de
        ? `Klaere dieses Work Item ${ctx.label}: aktueller Status, Owner, Blocker, Faelligkeit und konkreter naechster Schritt.`
        : `Clarify this work item ${ctx.label}: current status, owner, blocker, due date, and concrete next step.`);
    } else if (item.submoduleId === "workforce") {
      if (item.recordType === "workforce_assignment") {
        addDirect("workforce-assignment-edit", de ? "Bearbeiten" : "Edit", de ? "unteres Einsatzmodul" : "bottom assignment module");
        addDirect("workforce-assignment-duplicate", de ? "Duplizieren" : "Duplicate", de ? "naechsten Tag planen" : "plan next day");
        addDirect("workforce-assignment-time", de ? "Zeit erfassen" : "Record time", de ? "Istzeit anlegen" : "create actual time");
        addDirect("workforce-assignment-payroll", de ? "Payroll vorbereiten" : "Prepare payroll", de ? "freigegebene Stunden uebergeben" : "handoff approved hours");
        addDirect("workforce-assignment-invoice-draft", de ? "Rechnungsdraft" : "Invoice draft", de ? "abrechenbare Position erstellen" : "create billable draft");
        add("workforce-assignment-score", de ? "Score pruefen" : "Check score", de ? "Basis, Leistung, Bonus" : "base, performance, bonus", (ctx) => de
          ? `Pruefe diesen Einsatz ${ctx.label}. Bewerte Basis-Anforderungen, Leistungsanforderungen und Begeisterungsfaktoren: Person, Zeitfenster, Ueberschneidungen, Istzeit, Freigabe, Auftrag/Kunde und Uebergabe. Nenne genau die naechste Korrektur.`
          : `Check this assignment ${ctx.label}. Assess base, performance, and bonus criteria: person, time window, overlaps, actual time, approval, project/customer, and handoff. Name the exact next correction.`);
      } else if (item.recordType === "workforce_time_entry") {
        addDirect("workforce-time-approve", de ? "Freigeben" : "Approve", de ? "Zeitnachweis buchen" : "approve entry");
        addDirect("workforce-time-correction", de ? "Korrektur" : "Correction", de ? "Rueckfrage setzen" : "request correction");
      } else if (item.recordType === "workforce_person") {
        addDirect("workforce-person-edit", de ? "Bearbeiten" : "Edit", de ? "Stammdaten links" : "left master data");
        add("workforce-person-plan", de ? "Auslastung pruefen" : "Check utilization", de ? "Woche, Rolle, Konflikte" : "week, role, conflicts", (ctx) => de
          ? `Pruefe die Einsatzlage von ${ctx.label}. Stelle Wochenstunden, Rolle, Konflikte, offene Zeitnachweise und sinnvolle Umplanung dar.`
          : `Check workforce utilization for ${ctx.label}. Show weekly hours, role, conflicts, open time entries, and useful replanning.`);
      }
    } else {
      add("operations-context", de ? "Operations-Kontext" : "Operations context", de ? "Strukturieren und verknuepfen" : "Structure and link", (ctx) => de
        ? `Strukturiere diesen Operations-Kontext ${ctx.label}. Verknuepfe relevante Projekte, Work Items, Wissen und Meetings.`
        : `Structure this operations context ${ctx.label}. Link relevant projects, work items, knowledge, and meetings.`);
    }
  }

  if (item.moduleId === "business") {
    if (item.submoduleId === "warehouse") {
      if (item.recordType === "warehouse_order") {
        add("warehouse-order-readiness", de ? "Auftrag pruefen" : "Check order", de ? "Material, Wertschritte, Versandgate" : "Material, value steps, shipping gate", (ctx) => de
          ? `Pruefe diesen Warehouse-Auftrag ${ctx.label}. Bewerte Materialdeckung, fehlende Einzelpositionen, Wertschritte, QA, Packstatus und ob Versand wirklich freigegeben werden darf.`
          : `Check this warehouse order ${ctx.label}. Assess material coverage, missing order lines, value steps, QA, packing status, and whether shipping can be released.`);
        add("warehouse-order-blockers", de ? "Blocker planen" : "Plan blockers", de ? "Fehlteile und Fertigung" : "Missing parts and production", (ctx) => de
          ? `Plane die Blocker fuer Auftrag ${ctx.label}. Leite konkrete Aktionen fuer fehlende Teile, Umlagerung, Auftragsfertigung, QA und Packfreigabe ab.`
          : `Plan blockers for order ${ctx.label}. Derive concrete actions for missing parts, transfers, order production, QA, and packing approval.`);
      } else if (item.recordType === "warehouse_order_line") {
        add("warehouse-line-source", de ? "Position matchen" : "Match line", de ? "Bestand, Fehlteil, Quelle" : "Stock, shortage, source", (ctx) => de
          ? `Matche diese Auftragsposition ${ctx.label}. Pruefe verfuegbaren Bestand, fehlende Menge, beste Lagerquelle, Umlagerung und Fertigungsbedarf.`
          : `Match this order line ${ctx.label}. Check available stock, missing quantity, best warehouse source, transfer, and production need.`);
        add("warehouse-line-escalate", de ? "Fehlteil klaeren" : "Resolve shortage", de ? "Einkauf, Fertigung, Ersatz" : "Purchase, production, substitute", (ctx) => de
          ? `Klaere Fehlteile fuer ${ctx.label}. Nenne Einkaufs-, Fertigungs-, Ersatzteil- oder Teillieferungsoptionen mit naechstem Schritt.`
          : `Resolve shortages for ${ctx.label}. Name purchasing, production, substitute, or partial delivery options with the next step.`);
      } else if (item.recordType === "warehouse_work_step") {
        add("warehouse-step-gate", de ? "Gate pruefen" : "Check gate", de ? "Owner, Status, Freigabe" : "Owner, status, approval", (ctx) => de
          ? `Pruefe den Wertschritt ${ctx.label}. Klaere Owner, Status, Voraussetzungen, Nachweis und Freigabe fuer den naechsten Schritt.`
          : `Check value step ${ctx.label}. Clarify owner, status, prerequisites, evidence, and approval for the next step.`);
      } else if (item.recordType === "warehouse_slot") {
        addDirect("warehouse-slot-edit", de ? "Bearbeiten" : "Edit", de ? "unteres Slot-Modul" : "lower slot module");
        addDirect("warehouse-slot-duplicate", de ? "Duplizieren" : "Duplicate", de ? "neuen Slot anlegen" : "create new slot");
        addDirect("warehouse-slot-block", de ? "Sperren/Entsperren" : "Block/unblock", de ? "Pickbarkeit umschalten" : "toggle pickability");
        addDirect("warehouse-slot-count", de ? "Inventur" : "Count", de ? "Zaehlmodus oeffnen" : "open count mode");
        addDirect("warehouse-slot-audit", de ? "Audit" : "Audit", de ? "Bewegungen anzeigen" : "show movements");
      } else if (item.recordType === "inventory_item") {
        addDirect("warehouse-item-edit", de ? "Bearbeiten" : "Edit", de ? "SKU, Einheit, Tracking" : "SKU, UOM, tracking");
        addDirect("warehouse-item-duplicate", de ? "Duplizieren" : "Duplicate", de ? "Variante anlegen" : "create variant");
        addDirect("warehouse-item-deactivate", de ? "Deaktivieren" : "Deactivate", de ? "fuer neue Vorgaenge sperren" : "block for new work");
        add("warehouse-item-check", de ? "KI-Pruefung" : "AI check", de ? "Auslauf und Abhaengigkeiten" : "Phase-out and dependencies", (ctx) => de
          ? `Pruefe, ob Warehouse-Artikel ${ctx.label} geaendert oder deaktiviert werden kann. Fasse Bestand, offene Auftraege, Reservierungen, Webshop-Sync und sichere Folgeaktion zusammen.`
          : `Check whether warehouse item ${ctx.label} can be changed or deactivated. Summarize stock, open orders, reservations, shop sync, and safe follow-up.`);
      } else if (item.recordType === "stock_balance" || item.recordType === "warehouse_source_match") {
        addDirect("warehouse-stock-reserve", de ? "In Ausgang" : "Reserve outbound", de ? "1 Einheit reservieren" : "reserve 1 unit");
        addDirect("warehouse-stock-move", de ? "Umlagern" : "Move stock", de ? "Bestandsmodul oeffnen" : "open stock module");
        addDirect("warehouse-stock-audit", de ? "Audit" : "Audit", de ? "Slotbewegungen anzeigen" : "show slot movements");
        add("warehouse-stock-count", de ? "KI-Inventurpruefung" : "AI count check", de ? "Soll/Ist und Differenz" : "Expected/actual and variance", (ctx) => de
          ? `Pruefe diesen Bestand fuer eine Inventur- oder Schnellkorrektur: ${ctx.label}. Stelle Sollbestand, moeglichen Istbestand, Differenz, Grund und Audit-Folgeaktion dar.`
          : `Check this stock for count or quick correction: ${ctx.label}. Show expected stock, possible actual stock, variance, reason, and audit follow-up.`);
      } else if (item.recordType === "warehouse_source") {
        addDirect("warehouse-source-edit", de ? "Bearbeiten" : "Edit", de ? "Lagerstruktur oeffnen" : "open warehouse structure");
        addDirect("warehouse-source-section", de ? "Bereich anlegen" : "Add section", de ? "neue Zone" : "new zone");
        addDirect("warehouse-source-duplicate", de ? "Duplizieren" : "Duplicate", de ? "Lager kopieren" : "copy warehouse");
        addDirect("warehouse-source-toggle", de ? "Aktivieren/Deaktivieren" : "Activate/deactivate", de ? "operativer Status" : "operational status");
      } else if (item.recordType === "warehouse_zone") {
        addDirect("warehouse-zone-edit", de ? "Bearbeiten" : "Edit", de ? "Zone unten oeffnen" : "open zone below");
        addDirect("warehouse-zone-slots", de ? "Slots anlegen" : "Add slots", de ? "+4 Plaetze" : "+4 slots");
        addDirect("warehouse-zone-duplicate", de ? "Duplizieren" : "Duplicate", de ? "Slot-Struktur uebernehmen" : "copy slot structure");
      }
    }

    add("business-check", de ? "Business-Datensatz pruefen" : "Check business record", de ? "Stammdaten, Beleg, Folgeaktion" : "Master data, document, next action", (ctx) => de
      ? `Pruefe diesen Business-Kontext ${ctx.label}. Kontrolliere Stammdaten, Beleg-/Produktbezug, offene Folgeaktionen und Synchronisierung.`
      : `Check this business context ${ctx.label}. Review master data, document/product relation, open follow-ups, and synchronization.`);
  }

  if (item.moduleId === "ctox") {
    add("ctox-run", de ? "CTOX Lauf pruefen" : "Check CTOX run", de ? "Status, Kontext, Ergebnis" : "Status, context, result", (ctx) => de
      ? `Pruefe diesen CTOX-Kontext ${ctx.label}. Fasse Status, Input-Kontext, Ergebnis, Fehler und naechste Aktion zusammen.`
      : `Check this CTOX context ${ctx.label}. Summarize status, input context, result, errors, and next action.`);
  }

  add("summarize", de ? "Kurz zusammenfassen" : "Summarize", de ? "Kontext und naechste Aktion" : "Context and next action", (ctx) => de
    ? `Fasse diesen Kontext kurz zusammen und nenne die naechste sinnvolle Aktion: ${ctx.label}.`
    : `Briefly summarize this context and name the next useful action: ${ctx.label}.`);

  return actions.slice(0, 6);
}

function fitMenuPosition(clientX: number, clientY: number, actionCount = 0) {
  const width = 234;
  const height = 100 + actionCount * 42;
  return {
    x: Math.max(8, Math.min(clientX, window.innerWidth - width - 8)),
    y: Math.max(8, Math.min(clientY, window.innerHeight - height - 8))
  };
}

async function postQueue(body: Record<string, unknown>): Promise<QueueResponse> {
  const response = await fetch("/api/ctox/queue-tasks", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body)
  });
  return response.json().catch(() => ({ ok: false, error: "Invalid response" })) as Promise<QueueResponse>;
}
