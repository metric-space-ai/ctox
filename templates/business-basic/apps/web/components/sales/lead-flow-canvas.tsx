"use client";

import { useEffect, useMemo, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { createPortal } from "react-dom";
import { SalesQueueButton } from "./actions";

export type LeadFlowNodeState = "done" | "running" | "planned" | "blocked";
export type LeadFlowActor = "Agent" | "User" | "System";

export type LeadFlowNode = {
  actor: LeadFlowActor;
  compact?: boolean;
  detail: string;
  evidence: string;
  id: string;
  state: LeadFlowNodeState;
  time: string;
  title: string;
  type: "source" | "research" | "message" | "meeting" | "demo" | "offer" | "wait";
  x: number;
  y: number;
};

export type LeadFlowLink = {
  from: string;
  id: string;
  state: LeadFlowNodeState;
  to: string;
};

export type LeadFlow = {
  links: LeadFlowLink[];
  nodes: LeadFlowNode[];
  patterns: Array<{ label: string; signal: string; state: LeadFlowNodeState }>;
};

type DragState = {
  id: string;
  originX: number;
  originY: number;
  pointerX: number;
  pointerY: number;
};

type StoredFlow = {
  compactIds?: string[];
  customLinks?: LeadFlowLink[];
  customNodes?: LeadFlowNode[];
  edits?: Record<string, Partial<Pick<LeadFlowNode, "detail" | "evidence" | "state" | "time" | "title" | "type">>>;
  positions?: Record<string, { x: number; y: number }>;
};

const CANVAS_MIN_WIDTH = 1240;
const CANVAS_MIN_HEIGHT = 760;
const NODE_WIDTH = 210;
const NODE_HEIGHT = 160;
const COMPACT_NODE_WIDTH = 132;
const COMPACT_NODE_HEIGHT = 60;

export function LeadFlowCanvas({
  createLeadHref,
  flow,
  locale,
  storageKey
}: {
  createLeadHref: string;
  flow: LeadFlow;
  locale: "en" | "de";
  storageKey: string;
}) {
  const canvasRef = useRef<HTMLDivElement | null>(null);
  const draggedNodeRef = useRef(false);
  const [isMounted, setIsMounted] = useState(false);
  const [nodes, setNodes] = useState(flow.nodes);
  const [links, setLinks] = useState(flow.links);
  const [dragging, setDragging] = useState<DragState | null>(null);
  const [automationNode, setAutomationNode] = useState<LeadFlowNode | null>(null);
  const [automationPrompt, setAutomationPrompt] = useState("");
  const [automationTrigger, setAutomationTrigger] = useState("manual-approval");
  const [creationPoint, setCreationPoint] = useState<{ x: number; y: number } | null>(null);
  const [isSheetExpanded, setIsSheetExpanded] = useState(true);
  const nodeIds = useMemo(() => flow.nodes.map((node) => node.id).join("|"), [flow.nodes]);
  const activeAutomationNode = automationNode ? nodes.find((node) => node.id === automationNode.id) ?? automationNode : null;
  const canvasSize = useMemo(() => ({
    height: Math.max(CANVAS_MIN_HEIGHT, ...nodes.map((node) => node.y + nodeVisualHeight(node) + 260)),
    width: Math.max(CANVAS_MIN_WIDTH, ...nodes.map((node) => node.x + nodeVisualWidth(node) + 360))
  }), [nodes]);
  const openCreationMenu = (clientX: number, clientY: number) => {
    const bounds = canvasRef.current?.getBoundingClientRect();
    setAutomationNode(null);
    setCreationPoint({
      x: Math.max(0, clientX - (bounds?.left ?? 0) + (canvasRef.current?.parentElement?.scrollLeft ?? 0)),
      y: Math.max(0, clientY - (bounds?.top ?? 0) + (canvasRef.current?.parentElement?.scrollTop ?? 0))
    });
  };
  const openNodeSheet = (node: LeadFlowNode) => {
    setCreationPoint(null);
    if (activeAutomationNode?.id === node.id) {
      setIsSheetExpanded((expanded) => !expanded);
      return;
    }
    setAutomationNode(node);
    setAutomationPrompt(defaultAutomationPrompt(node, locale));
    setIsSheetExpanded(true);
  };
  const appendProposal = (parent: LeadFlowNode) => {
    const proposal = nextProposalNode(parent, nodes, locale);
    setNodes((current) => [...current, proposal]);
    setLinks((current) => [...current, {
      from: parent.id,
      id: `proposal-link-${parent.id}-${proposal.id}`,
      state: "planned",
      to: proposal.id
    }]);
    setAutomationNode(proposal);
    setAutomationPrompt(defaultAutomationPrompt(proposal, locale));
    setIsSheetExpanded(true);
    window.setTimeout(() => {
      canvasRef.current?.querySelector(`[data-node-id="${proposal.id}"]`)?.scrollIntoView({
        behavior: "smooth",
        block: "nearest",
        inline: "nearest"
      });
    }, 50);
  };

  useEffect(() => {
    setIsMounted(true);
  }, []);

  useEffect(() => {
    const stored = window.localStorage.getItem(`lead-flow:${storageKey}`);
    if (!stored) {
      setNodes(flow.nodes);
      setLinks(flow.links);
      return;
    }

    try {
      const parsed = JSON.parse(stored) as StoredFlow;
      const positions = parsed.positions ?? {};
      const edits = parsed.edits ?? {};
      const compactIds = new Set(parsed.compactIds ?? []);
      const baseNodes = flow.nodes.map((node) => ({
        ...node,
        ...edits[node.id],
        compact: compactIds.has(node.id),
        x: positions[node.id]?.x ?? node.x,
        y: positions[node.id]?.y ?? node.y
      }));
      const customNodes = (parsed.customNodes ?? []).map((node) => ({
        ...node,
        ...edits[node.id],
        compact: compactIds.has(node.id) || node.compact,
        x: positions[node.id]?.x ?? node.x,
        y: positions[node.id]?.y ?? node.y
      }));
      setNodes([...baseNodes, ...customNodes]);
      setLinks([...flow.links, ...(parsed.customLinks ?? [])]);
    } catch {
      setNodes(flow.nodes);
      setLinks(flow.links);
    }
  }, [flow.links, flow.nodes, nodeIds, storageKey]);

  useEffect(() => {
    if (!nodes.length) return;
    const positions = Object.fromEntries(nodes.map((node) => [node.id, { x: node.x, y: node.y }]));
    const customNodes = nodes.filter((node) => node.id.startsWith("proposal-"));
    const customLinks = links.filter((link) => link.id.startsWith("proposal-link-"));
    const compactIds = nodes.filter((node) => node.compact).map((node) => node.id);
    const edits: StoredFlow["edits"] = {};
    nodes.forEach((node) => {
      const base = flow.nodes.find((item) => item.id === node.id);
      if (!base || base.title !== node.title || base.detail !== node.detail || base.evidence !== node.evidence || base.time !== node.time || base.state !== node.state || base.type !== node.type) {
        edits[node.id] = { detail: node.detail, evidence: node.evidence, state: node.state, time: node.time, title: node.title, type: node.type };
      }
    });
    const stored: StoredFlow = { compactIds, customLinks, customNodes, edits, positions };
    window.localStorage.setItem(`lead-flow:${storageKey}`, JSON.stringify(stored));
  }, [flow.nodes, links, nodes, storageKey]);

  const automationSheet = activeAutomationNode ? (
    <section className={`lead-flow-bottom-sheet ${isSheetExpanded ? "" : "is-collapsed"}`} aria-label={locale === "de" ? "CTOX Automatisierung" : "CTOX automation"}>
      <header>
        <div>
          <span>{locale === "de" ? "Klick: Details und Automation" : "Click: details and automation"}</span>
          <h3>{activeAutomationNode.title}</h3>
        </div>
        <div className="lead-flow-sheet-controls">
          <button onClick={() => setAutomationNode(null)} type="button">{locale === "de" ? "Schliessen" : "Close"}</button>
        </div>
      </header>
      {isSheetExpanded ? (
        <>
          <div className="lead-flow-sheet-body">
            <section>
              <h4>{locale === "de" ? "Plan-Block" : "Plan block"}</h4>
              <label>
                <span>{locale === "de" ? "Titel" : "Title"}</span>
                <input onChange={(event) => updateNode(activeAutomationNode.id, { title: event.target.value }, setNodes)} value={activeAutomationNode.title} />
              </label>
              <label>
                <span>{locale === "de" ? "Datum" : "Date"}</span>
                <input onChange={(event) => updateNode(activeAutomationNode.id, { time: event.target.value }, setNodes)} value={activeAutomationNode.time} />
              </label>
              <div className="lead-flow-sheet-grid">
                <label>
                  <span>{locale === "de" ? "Typ" : "Type"}</span>
                  <select onChange={(event) => updateNode(activeAutomationNode.id, { type: event.target.value as LeadFlowNode["type"] }, setNodes)} value={activeAutomationNode.type}>
                    <option value="source">Source</option>
                    <option value="research">Research</option>
                    <option value="message">E-Mail</option>
                    <option value="wait">{locale === "de" ? "Warten" : "Wait"}</option>
                    <option value="meeting">{locale === "de" ? "Meeting" : "Meeting"}</option>
                    <option value="demo">Demo</option>
                    <option value="offer">{locale === "de" ? "Angebotsreife" : "Offer readiness"}</option>
                  </select>
                </label>
                <label>
                  <span>Status</span>
                  <select onChange={(event) => updateNode(activeAutomationNode.id, { state: event.target.value as LeadFlowNodeState }, setNodes)} value={activeAutomationNode.state}>
                    <option value="done">{locale === "de" ? "Event erledigt" : "Event done"}</option>
                    <option value="running">{locale === "de" ? "Task laeuft" : "Task running"}</option>
                    <option value="planned">{locale === "de" ? "Plan" : "Plan"}</option>
                    <option value="blocked">{locale === "de" ? "Blockiert" : "Blocked"}</option>
                  </select>
                </label>
              </div>
              <label>
                <span>{locale === "de" ? "Beschreibung" : "Description"}</span>
                <textarea onChange={(event) => updateNode(activeAutomationNode.id, { detail: event.target.value }, setNodes)} value={activeAutomationNode.detail} />
              </label>
              <label>
                <span>{locale === "de" ? "Evidenz / Trigger" : "Evidence / trigger"}</span>
                <textarea onChange={(event) => updateNode(activeAutomationNode.id, { evidence: event.target.value }, setNodes)} value={activeAutomationNode.evidence} />
              </label>
            </section>
            <section>
              <h4>{locale === "de" ? "Automatisierung" : "Automation"}</h4>
              <label>
                <span>{locale === "de" ? "Ausloeser" : "Trigger"}</span>
                <select value={automationTrigger} onChange={(event) => setAutomationTrigger(event.target.value)}>
                  <option value="manual-approval">{locale === "de" ? "Manuelle Freigabe" : "Manual approval"}</option>
                  <option value="no-reply">{locale === "de" ? "Keine Antwort nach Wartezeit" : "No reply after wait"}</option>
                  <option value="reply-detected">{locale === "de" ? "Antwort erkannt" : "Reply detected"}</option>
                  <option value="scheduled">{locale === "de" ? "Zum geplanten Zeitpunkt" : "Scheduled time"}</option>
                </select>
              </label>
              <label>
                <span>CTOX Prompt</span>
                <textarea value={automationPrompt} onChange={(event) => setAutomationPrompt(event.target.value)} />
              </label>
            </section>
          </div>
          <footer>
            <button className="campaign-secondary" onClick={() => appendProposal(activeAutomationNode)} type="button">
              {locale === "de" ? "Naechsten Plan-Schritt anhaengen" : "Attach next plan step"}
            </button>
            {isDeletablePlan(activeAutomationNode) ? (
              <button className="campaign-secondary is-danger" onClick={() => removeNode(activeAutomationNode.id, setNodes, setLinks, setAutomationNode)} type="button">
                {locale === "de" ? "Plan Item loeschen" : "Delete plan item"}
              </button>
            ) : null}
            <button className="campaign-secondary" onClick={() => shrinkBranch(activeAutomationNode.id, links, setNodes)} type="button">
              {locale === "de" ? "Zweig verkuemmern" : "Miniaturize branch"}
            </button>
            {activeAutomationNode.compact ? (
              <button className="campaign-secondary" onClick={() => restoreBranch(activeAutomationNode.id, links, setNodes)} type="button">
                {locale === "de" ? "Zweig wieder anzeigen" : "Restore branch"}
              </button>
            ) : null}
            <SalesQueueButton
              action="sync"
              className="drawer-primary"
              instruction={automationPrompt}
              payload={{ links, node: activeAutomationNode, nodes, prompt: automationPrompt, trigger: automationTrigger }}
              recordId={`lead-flow-automation-${storageKey}-${activeAutomationNode.id}`}
              resource="sales_activity"
              title={`CTOX lead flow automation: ${activeAutomationNode.title}`}
            >
              {locale === "de" ? "Automation starten" : "Start automation"}
            </SalesQueueButton>
          </footer>
        </>
      ) : null}
    </section>
  ) : null;

  const createSheet = creationPoint ? (
    <section className="lead-flow-bottom-sheet lead-flow-create-sheet" aria-label={locale === "de" ? "Neues Flow Element" : "New flow element"}>
      <header>
        <div>
          <span>{locale === "de" ? "Rechtsklick: Neu" : "Right click: New"}</span>
          <h3>{locale === "de" ? "Flow erweitern" : "Extend flow"}</h3>
        </div>
        <button onClick={() => setCreationPoint(null)} type="button">{locale === "de" ? "Schliessen" : "Close"}</button>
      </header>
      <div className="lead-flow-sheet-body">
        <section>
          <h4>{locale === "de" ? "An dieser Stelle" : "At this point"}</h4>
          <p>{locale === "de" ? "Lege einen neuen Plan-Block direkt auf der Arbeitsflaeche an oder starte einen neuen Lead." : "Add a new plan block directly on the canvas or start a new lead."}</p>
        </section>
        <section>
          <h4>{locale === "de" ? "Aktionen" : "Actions"}</h4>
          <div className="lead-flow-create-actions">
            <button
              className="drawer-primary"
              onClick={() => {
                const node = newCanvasNode(creationPoint, locale);
                setNodes((current) => [...current, node]);
                setAutomationNode(node);
                setAutomationPrompt(defaultAutomationPrompt(node, locale));
                setIsSheetExpanded(true);
                setCreationPoint(null);
              }}
              type="button"
            >
              {locale === "de" ? "Plan-Item hier anlegen" : "Create plan item here"}
            </button>
            <a className="campaign-secondary" href={createLeadHref}>{locale === "de" ? "Neuen Lead anlegen" : "Create new lead"}</a>
          </div>
        </section>
      </div>
    </section>
  ) : null;

  return (
    <>
      <div
        className="lead-flow-canvas"
        ref={canvasRef}
        role="application"
        aria-label={locale === "de" ? "Verschiebbare Lead Flow Map" : "Draggable lead flow map"}
        onPointerDown={(event) => {
          if (event.button !== 2 || isFlowMenuTarget(event.target)) return;
          event.preventDefault();
          openCreationMenu(event.clientX, event.clientY);
        }}
        onContextMenu={(event) => {
          if (isFlowMenuTarget(event.target)) return;
          event.preventDefault();
          openCreationMenu(event.clientX, event.clientY);
        }}
        style={{ height: canvasSize.height, width: canvasSize.width }}
      >
        <button
          className="lead-flow-reset"
          onClick={() => {
            window.localStorage.removeItem(`lead-flow:${storageKey}`);
            setAutomationNode(null);
            setLinks(flow.links);
            setNodes(flow.nodes);
          }}
          type="button"
        >
          {locale === "de" ? "Layout reset" : "Reset layout"}
        </button>
        <svg className="lead-flow-links" viewBox={`0 0 ${canvasSize.width} ${canvasSize.height}`} aria-hidden="true" style={{ height: canvasSize.height, width: canvasSize.width }}>
          <defs>
            <marker id="lead-flow-arrow" markerHeight="8" markerWidth="8" orient="auto" refX="7" refY="4">
              <path d="M0,0 L8,4 L0,8 Z" />
            </marker>
          </defs>
          {links.map((link) => (
            <path className={`lead-flow-link is-${link.state}`} d={leadFlowLinkPath(nodes, link)} key={link.id} markerEnd="url(#lead-flow-arrow)" />
          ))}
        </svg>
        {nodes.map((node) => (
          <article
          className={`lead-flow-node type-${node.type} is-${node.state} ${node.compact ? "is-compact" : ""} ${dragging?.id === node.id ? "is-dragging" : ""}`}
          key={node.id}
          onPointerDown={(event) => {
            if (event.button !== 0) return;
            draggedNodeRef.current = false;
            event.currentTarget.setPointerCapture(event.pointerId);
            setDragging({
              id: node.id,
              originX: node.x,
              originY: node.y,
              pointerX: event.clientX,
              pointerY: event.clientY
            });
          }}
          onPointerMove={(event) => {
            if (!dragging || dragging.id !== node.id) return;
            if (Math.abs(event.clientX - dragging.pointerX) > 5 || Math.abs(event.clientY - dragging.pointerY) > 5) {
              draggedNodeRef.current = true;
            }
            const nextX = clamp(dragging.originX + event.clientX - dragging.pointerX, 0, canvasSize.width + 520 - nodeVisualWidth(node));
            const nextY = clamp(dragging.originY + event.clientY - dragging.pointerY, 0, canvasSize.height + 360 - nodeVisualHeight(node));
            setNodes((current) => current.map((item) => item.id === node.id ? { ...item, x: nextX, y: nextY } : item));
          }}
          onPointerUp={(event) => {
            event.currentTarget.releasePointerCapture(event.pointerId);
            setDragging(null);
          }}
          onClick={(event) => {
            if (isInteractiveFlowTarget(event.target) || draggedNodeRef.current) return;
            openNodeSheet(node);
          }}
          data-context-item
          data-context-label={node.title}
          data-context-module="sales"
          data-context-record-id={node.id}
          data-context-record-type={`lead_flow_${node.state}`}
          data-context-submodule="leads"
          data-node-id={node.id}
          style={{ left: node.x, top: node.y }}
          tabIndex={0}
        >
          <time className="lead-flow-date">{node.time}</time>
          {node.compact ? (
            <>
              <strong>{node.title}</strong>
              <em>{locale === "de" ? "verkuemmert" : "mini branch"}</em>
            </>
          ) : (
            <>
              <strong>{node.title}</strong>
              <p>{node.detail}</p>
              <small>{node.evidence}</small>
              <em>{leadFlowStateLabel(node.state, locale)}</em>
            </>
          )}
          <button
            aria-label={locale === "de" ? "Naechsten Plan-Schritt anhaengen" : "Attach next plan step"}
            className="lead-flow-add-branch"
            onClick={(event) => {
              event.stopPropagation();
              appendProposal(node);
            }}
            onPointerDown={(event) => event.stopPropagation()}
            type="button"
          >
            +
          </button>
          <div className="lead-flow-node-actions" onPointerDown={(event) => event.stopPropagation()}>
              {isDeletablePlan(node) ? (
                <button
                  aria-label={locale === "de" ? "Zweig verkleinern" : "Miniaturize branch"}
                  onClick={() => shrinkBranch(node.id, links, setNodes)}
                  type="button"
                >
                  Mini
                </button>
              ) : null}
              {node.compact ? (
                <button
                  aria-label={locale === "de" ? "Zweig wieder anzeigen" : "Restore branch"}
                  onClick={() => restoreBranch(node.id, links, setNodes)}
                  type="button"
                >
                  {locale === "de" ? "Auf" : "Open"}
                </button>
              ) : null}
              {isDeletablePlan(node) ? (
                <button
                  aria-label={locale === "de" ? "Plan Item loeschen" : "Delete plan item"}
                  className="is-danger"
                  onClick={() => removeNode(node.id, setNodes, setLinks, setAutomationNode)}
                  type="button"
                >
                  {locale === "de" ? "Loeschen" : "Delete"}
                </button>
              ) : null}
          </div>
          </article>
        ))}
      </div>
      {isMounted && automationSheet ? createPortal(automationSheet, document.body) : null}
      {isMounted && createSheet ? createPortal(createSheet, document.body) : null}
    </>
  );
}

function updateNode(id: string, patch: Partial<LeadFlowNode>, setNodes: Dispatch<SetStateAction<LeadFlowNode[]>>) {
  setNodes((current) => current.map((node) => node.id === id ? { ...node, ...patch } : node));
}

function isFlowMenuTarget(target: EventTarget | null) {
  return target instanceof Element &&
    !!target.closest(".lead-flow-node, .lead-flow-bottom-sheet, button, input, textarea, select, a");
}

function isInteractiveFlowTarget(target: EventTarget | null) {
  return target instanceof Element &&
    !!target.closest("button, input, textarea, select, a");
}

function removeNode(
  id: string,
  setNodes: Dispatch<SetStateAction<LeadFlowNode[]>>,
  setLinks: Dispatch<SetStateAction<LeadFlowLink[]>>,
  setAutomationNode: Dispatch<SetStateAction<LeadFlowNode | null>>
) {
  setNodes((current) => current.filter((node) => node.id !== id));
  setLinks((current) => current.filter((link) => link.from !== id && link.to !== id));
  setAutomationNode((current) => current?.id === id ? null : current);
}

function shrinkBranch(id: string, links: LeadFlowLink[], setNodes: Dispatch<SetStateAction<LeadFlowNode[]>>) {
  const ids = branchIds(id, links);
  setNodes((current) => current.map((node) => ids.has(node.id) ? { ...node, compact: true } : node));
}

function restoreBranch(id: string, links: LeadFlowLink[], setNodes: Dispatch<SetStateAction<LeadFlowNode[]>>) {
  const ids = branchIds(id, links);
  setNodes((current) => current.map((node) => ids.has(node.id) ? { ...node, compact: false } : node));
}

function branchIds(rootId: string, links: LeadFlowLink[]) {
  const ids = new Set([rootId]);
  let changed = true;
  while (changed) {
    changed = false;
    links.forEach((link) => {
      if (ids.has(link.from) && !ids.has(link.to)) {
        ids.add(link.to);
        changed = true;
      }
    });
  }
  return ids;
}

function isDeletablePlan(node: LeadFlowNode) {
  return node.state === "planned" || node.id.startsWith("proposal-");
}

function nextProposalNode(parent: LeadFlowNode, nodes: LeadFlowNode[], locale: "en" | "de"): LeadFlowNode {
  const suggestion = proposalCopy(parent, locale);
  const position = nextOpenPosition(parent, nodes);
  return {
    actor: "Agent",
    detail: suggestion.detail,
    evidence: suggestion.evidence,
    id: `proposal-${parent.id}-${Date.now()}`,
    state: "planned",
    time: locale === "de" ? "Vorschlag" : "Proposal",
    title: suggestion.title,
    type: suggestion.type,
    x: position.x,
    y: position.y
  };
}

function newCanvasNode(point: { x: number; y: number }, locale: "en" | "de"): LeadFlowNode {
  return {
    actor: "User",
    detail: locale === "de" ? "Neuen Plan-Block beschreiben und bei Bedarf mit CTOX automatisieren." : "Describe the new plan block and automate it with CTOX if needed.",
    evidence: locale === "de" ? "Manuell angelegt" : "Manually created",
    id: `proposal-free-${Date.now()}`,
    state: "planned",
    time: locale === "de" ? "Plan" : "Plan",
    title: locale === "de" ? "Neues Plan-Item" : "New plan item",
    type: "message",
    x: Math.max(0, point.x),
    y: Math.max(0, point.y)
  };
}

function nextOpenPosition(parent: LeadFlowNode, nodes: LeadFlowNode[]) {
  const columns = [
    parent.x + 260,
    parent.x + 520,
    parent.x + 780,
    parent.x + 120
  ].map((x) => Math.max(0, x));
  const rows = [560, 24, 250, 430, parent.y, parent.y + 150, parent.y - 150]
    .map((y) => Math.max(0, y));

  for (const x of columns) {
    for (const y of rows) {
      if (!nodes.some((node) => rectanglesOverlap({ x, y }, node))) return { x, y };
    }
  }

  return {
    x: Math.max(0, parent.x + 260),
    y: Math.max(0, parent.y + 120)
  };
}

function rectanglesOverlap(left: { x: number; y: number }, right: LeadFlowNode) {
  const gap = 18;
  return left.x < right.x + nodeVisualWidth(right) + gap &&
    left.x + NODE_WIDTH + gap > right.x &&
    left.y < right.y + nodeVisualHeight(right) + gap &&
    left.y + NODE_HEIGHT + gap > right.y;
}

function proposalCopy(parent: LeadFlowNode, locale: "en" | "de"): Pick<LeadFlowNode, "detail" | "evidence" | "title" | "type"> {
  const lowerTitle = parent.title.toLowerCase();
  if (lowerTitle.includes("e-mail") || lowerTitle.includes("mail")) {
    return locale === "de"
      ? { detail: "Keine Sofortreaktion erzwingen. Antwortfenster beobachten und nur bei Signal weiter eskalieren.", evidence: "Trigger: Antwort, Oeffnung, Klick oder 2 Werktage ohne Rueckmeldung.", title: "Warten auf Antwort", type: "wait" }
      : { detail: "Do not force an immediate reaction. Watch the reply window and only escalate on a signal.", evidence: "Trigger: reply, open, click, or 2 business days without response.", title: "Wait for reply", type: "wait" };
  }
  if (parent.type === "wait") {
    return locale === "de"
      ? { detail: "Wenn keine Antwort kommt, kurzes Follow-up mit neuem Beleg. Wenn Antwort kommt, Terminfindung starten.", evidence: "Branch: keine Antwort -> Follow-up, Antwort -> Terminfindung.", title: "Follow-up / Terminfindung", type: "message" }
      : { detail: "If there is no reply, send a short follow-up with new evidence. If they reply, start scheduling.", evidence: "Branch: no reply -> follow-up, reply -> scheduling.", title: "Follow-up / scheduling", type: "message" };
  }
  if (lowerTitle.includes("terminfindung") || lowerTitle.includes("scheduling")) {
    return locale === "de"
      ? { detail: "Konkrete Zeitfenster, Teilnehmer und Ziel des Termins festlegen.", evidence: "Output: Kalendereinladung mit Agenda und Owner.", title: "Termin planen", type: "meeting" }
      : { detail: "Set concrete time windows, participants, and meeting objective.", evidence: "Output: calendar invite with agenda and owner.", title: "Plan meeting", type: "meeting" };
  }
  if (lowerTitle.includes("termin planen") || lowerTitle.includes("plan meeting")) {
    return locale === "de"
      ? { detail: "Briefing, Fragen, Einwaende und Demo-Ausschnitt fuer genau diesen Lead vorbereiten.", evidence: "Output: Terminbriefing fuer User und Agent.", title: "Termin vorbereiten", type: "demo" }
      : { detail: "Prepare briefing, questions, objections, and demo slice for this lead.", evidence: "Output: meeting brief for user and agent.", title: "Prepare meeting", type: "demo" };
  }
  return locale === "de"
    ? { detail: "CTOX recherchiert den naechsten sinnvollen Schritt und haengt ihn als editierbare Planungsbox an.", evidence: "Vorschlag: erst pruefen, dann senden oder terminieren.", title: "E-Mail vorbereiten", type: "message" }
    : { detail: "CTOX researches the next useful step and attaches it as an editable planning box.", evidence: "Proposal: check first, then send or schedule.", title: "Prepare email", type: "message" };
}

function defaultAutomationPrompt(node: LeadFlowNode, locale: "en" | "de") {
  if (locale === "de") {
    return `Analysiere diese Lead-Flow-Box und schlage den naechsten konkreten Plan-Item vor. Arbeite nur auf Sicht: maximal ein bis zwei naechste Schritte, keine vollstaendige Eventualitaeten-Matrix. Box: ${node.title}. Kontext: ${node.detail}. Evidenz/Trigger: ${node.evidence}.`;
  }
  return `Analyze this lead-flow box and suggest the next concrete plan item. Work only within the immediate horizon: at most one or two next steps, no complete eventuality matrix. Box: ${node.title}. Context: ${node.detail}. Evidence/trigger: ${node.evidence}.`;
}

function leadFlowLinkPath(nodes: LeadFlowNode[], link: LeadFlowLink) {
  const from = nodes.find((node) => node.id === link.from);
  const to = nodes.find((node) => node.id === link.to);
  if (!from || !to) return "";
  const startX = from.x + nodeVisualWidth(from);
  const startY = from.y + Math.min(58, nodeVisualHeight(from) / 2);
  const endX = to.x;
  const endY = to.y + Math.min(58, nodeVisualHeight(to) / 2);
  const curve = Math.max(70, Math.abs(endX - startX) * 0.48);
  return `M ${startX} ${startY} C ${startX + curve} ${startY}, ${endX - curve} ${endY}, ${endX} ${endY}`;
}

function nodeVisualWidth(node: LeadFlowNode) {
  return node.compact ? COMPACT_NODE_WIDTH : NODE_WIDTH;
}

function nodeVisualHeight(node: LeadFlowNode) {
  return node.compact ? COMPACT_NODE_HEIGHT : NODE_HEIGHT;
}

function leadFlowStateLabel(state: LeadFlowNodeState, locale: "en" | "de") {
  const labels = locale === "de"
    ? { blocked: "blockiert", done: "Event erledigt", planned: "Plan", running: "Task laeuft" }
    : { blocked: "blocked", done: "event done", planned: "plan", running: "task running" };
  return labels[state];
}

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max);
}
