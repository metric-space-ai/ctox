"use client";

import { type ReactNode, useEffect, useState } from "react";

export type ContextMenuItemContext = {
  action?: string;
  currentUrl?: string;
  filePath?: string;
  group?: string;
  moduleId?: string;
  submoduleId?: string;
  recordType?: string;
  recordId?: string;
  label?: string;
  href?: string;
  selectedText?: string;
  skillId?: string;
  sourcePath?: string;
};

type MenuState = {
  x: number;
  y: number;
  items: ContextMenuItemContext[];
};

type PromptState = {
  items: ContextMenuItemContext[];
  status: "idle" | "submitting" | "queued" | "error";
  taskId?: string;
  initialInstruction?: string;
};

export function ContextMenuScope({ children }: { children: ReactNode }) {
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [prompt, setPrompt] = useState<PromptState | null>(null);
  const capabilities = menu ? menuCapabilities(menu.items) : null;

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    window.addEventListener("click", close);
    window.addEventListener("keydown", close);
    window.addEventListener("scroll", close, true);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("keydown", close);
      window.removeEventListener("scroll", close, true);
    };
  }, [menu]);

  return (
    <div
      className="context-menu-scope"
      onContextMenu={(event) => {
        const target = event.target instanceof Element ? event.target.closest("[data-context-item]") : null;
        if (!target) return;
        event.preventDefault();
        setMenu({
          x: event.clientX,
          y: event.clientY,
          items: collectContextItems(target)
        });
      }}
    >
      {children}
      {menu ? (
        <div
          className="context-menu"
          role="menu"
          style={{ left: menu.x, top: menu.y }}
        >
          <button
            className="context-menu-primary"
            role="menuitem"
            type="button"
            onClick={() => {
              setPrompt({ items: menu.items, status: "idle", initialInstruction: presetInstruction("prompt", menu.items) });
              setMenu(null);
            }}
          >
            Prompt CTOX
          </button>
          {capabilities?.details ? (
            <button
              role="menuitem"
              type="button"
              onClick={() => {
                openDetails(menu.items);
                setMenu(null);
              }}
            >
              Open details
            </button>
          ) : null}
          {capabilities?.new ? (
            <button
              role="menuitem"
              type="button"
              onClick={() => {
                openNewItem(menu.items);
                setMenu(null);
              }}
            >
              New
            </button>
          ) : null}
          <button
            role="menuitem"
            type="button"
            onClick={() => {
              setPrompt({ items: menu.items, status: "idle", initialInstruction: presetInstruction("edit", menu.items) });
              setMenu(null);
            }}
          >
            Edit
          </button>
          {capabilities?.assign ? (
            <button
              role="menuitem"
              type="button"
              onClick={() => {
                setPrompt({ items: menu.items, status: "idle", initialInstruction: presetInstruction("assign", menu.items) });
                setMenu(null);
              }}
            >
              Assign
            </button>
          ) : null}
          {capabilities?.archive ? (
            <button
              role="menuitem"
              type="button"
              onClick={async () => {
                const items = menu.items;
                setPrompt({ items, status: "submitting", initialInstruction: presetInstruction("archive", items) });
                setMenu(null);
                const result = await promptCtox(items, presetInstruction("archive", items));
                setPrompt({ items, status: result.taskId ? "queued" : "error", taskId: result.taskId, initialInstruction: presetInstruction("archive", items) });
              }}
            >
              Archive
            </button>
          ) : null}
        </div>
      ) : null}
      {prompt ? (
        <CtoxPromptPanel
          prompt={prompt}
          setPrompt={setPrompt}
        />
      ) : null}
    </div>
  );
}

function CtoxPromptPanel({
  prompt,
  setPrompt
}: {
  prompt: PromptState;
  setPrompt: (state: PromptState | null) => void;
}) {
  const [instruction, setInstruction] = useState(prompt.initialInstruction ?? "");

  useEffect(() => {
    setInstruction(prompt.initialInstruction ?? "");
  }, [prompt.initialInstruction]);

  return (
    <section className="ctox-prompt-panel" aria-label="Prompt CTOX">
      <header>
        <div>
          <strong>Prompt CTOX</strong>
          <span>{contextSummary(prompt.items)}</span>
        </div>
        <button type="button" onClick={() => setPrompt(null)}>Close</button>
      </header>
      <div className="ctox-prompt-context">
        {prompt.items.map((item, index) => (
          <span key={`${item.moduleId}-${item.submoduleId}-${item.recordType}-${item.recordId}-${index}`}>
            {item.label ?? item.recordId ?? item.recordType ?? "Selected item"}
          </span>
        ))}
      </div>
      <textarea
        autoFocus
        onChange={(event) => setInstruction(event.target.value)}
        placeholder="Tell CTOX what to do with this context..."
        value={instruction}
      />
      <footer>
        {prompt.status === "queued" ? <span>Queued{prompt.taskId ? `: ${prompt.taskId}` : ""}</span> : prompt.status === "error" ? <span>Queue failed</span> : <span />}
        <button
          disabled={!instruction.trim() || prompt.status === "submitting"}
          onClick={async () => {
            setPrompt({ ...prompt, status: "submitting" });
            const result = await promptCtox(prompt.items, instruction);
            setPrompt({ ...prompt, status: "queued", taskId: result.taskId, initialInstruction: undefined });
            setInstruction("");
          }}
          type="button"
        >
          Queue task
        </button>
      </footer>
    </section>
  );
}

function collectContextItems(target: Element): ContextMenuItemContext[] {
  const scope = target.closest(".context-menu-scope");
  const selectedItems = scope
    ? Array.from(scope.querySelectorAll("[data-context-item][data-selected='true']"))
    : [];
  const elements = selectedItems.includes(target) ? selectedItems : [target];

  const selectedText = window.getSelection()?.toString().trim().slice(0, 4000);
  const currentUrl = window.location.href;

  return elements.map((element) => {
    const dataset = (element as HTMLElement).dataset;
    const href = element.closest("a")?.getAttribute("href") ?? undefined;
    return {
      action: dataset.contextAction,
      currentUrl,
      filePath: dataset.contextFilePath,
      group: dataset.contextGroup,
      moduleId: dataset.contextModule,
      submoduleId: dataset.contextSubmodule,
      recordType: dataset.contextRecordType,
      recordId: dataset.contextRecordId,
      label: dataset.contextLabel ?? element.textContent?.trim().replace(/\s+/g, " ").slice(0, 120),
      href,
      selectedText: selectedText || undefined,
      skillId: dataset.contextSkillId,
      sourcePath: dataset.contextSourcePath
    };
  });
}

async function promptCtox(items: ContextMenuItemContext[], instruction: string) {
  const response = await fetch("/api/ctox/queue-tasks", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      instruction: instruction.trim(),
      context: {
        source: "context-menu",
        currentUrl: window.location.href,
        items
      }
    })
  });
  const payload = await response.json().catch(() => null) as { task?: { id?: string } } | null;
  return { taskId: payload?.task?.id };
}

function openNewItem(items: ContextMenuItemContext[]) {
  const item = items[0];
  const moduleId = item?.moduleId;
  const submoduleId = item?.submoduleId;
  if (!moduleId || !submoduleId) return;

  const current = new URL(window.location.href);
  const params = new URLSearchParams();
  const locale = current.searchParams.get("locale");
  const theme = current.searchParams.get("theme");
  if (locale) params.set("locale", locale);
  if (theme) params.set("theme", theme);

  params.set("panel", "new");
  params.set("drawer", "left-bottom");

  if (isKnowledgeSkillContext(item)) {
    params.set("recordId", knowledgeNewRecordId(item));
    if (item.group) params.set("group", item.group);
    if (item.skillId) params.set("skillId", item.skillId);
    if (item.filePath) params.set("filePath", item.filePath);
  } else if (item.recordType === "scoring_criterion") {
    params.set("panel", "criterion");
    params.set("recordId", "new");
  } else if (item.recordType === "market_opportunity" || item.recordType === "market_risk") {
    params.set("panel", "draft");
    params.set("recordId", "new-note");
    params.set("drawer", "right");
  } else if (moduleId === "sales") {
    params.set("recordId", newRecordId("sales", submoduleId, item.recordType));
  } else if (moduleId === "marketing") {
    params.set("recordId", marketingNewResource(submoduleId));
  } else if (moduleId === "business") {
    params.set("recordId", newRecordId("business", submoduleId, item.recordType));
  } else if (moduleId === "ctox") {
    params.set("recordId", newRecordId("ctox", submoduleId, item.recordType));
  } else {
    params.set("recordId", item.recordType ? `new-${item.recordType}` : "new-item");
  }

  navigateInApp(`/app/${moduleId}/${submoduleId}?${params.toString()}`);
}

function openDetails(items: ContextMenuItemContext[]) {
  const item = items[0];
  if (!item) return;
  if (item.href) {
    navigateInApp(item.href);
    return;
  }

  const moduleId = item.moduleId;
  const submoduleId = item.submoduleId;
  if (!moduleId || !submoduleId || !item.recordId) return;

  const current = new URL(window.location.href);
  const params = new URLSearchParams();
  const locale = current.searchParams.get("locale");
  const theme = current.searchParams.get("theme");
  if (locale) params.set("locale", locale);
  if (theme) params.set("theme", theme);
  params.set("panel", detailPanelFor(item));
  params.set("recordId", item.recordId);
  params.set("drawer", "right");
  navigateInApp(`/app/${moduleId}/${submoduleId}?${params.toString()}`);
}

function navigateInApp(href: string) {
  const event = new CustomEvent("ctox:navigate", {
    cancelable: true,
    detail: { href }
  });
  if (!window.dispatchEvent(event)) return;
  window.location.href = href;
}

function detailPanelFor(item: ContextMenuItemContext) {
  const recordType = item.recordType ?? "record";
  if (recordType === "work_item" || recordType === "ticket") return "work-item";
  if (recordType === "wiki_page" || recordType === "document" || recordType === "runbook") return "knowledge";
  if (recordType === "queue_item") return "queue";
  if (recordType === "bug_report") return "bug";
  if (recordType === "agent_run") return "run";
  if (recordType === "knowledge_record") return "knowledge";
  if (recordType === "sync_event") return "sync";
  return recordType.replace(/_/g, "-");
}

function menuCapabilities(items: ContextMenuItemContext[]) {
  const first = items[0];
  const knowledge = isKnowledgeSkillContext(first);
  return {
    archive: Boolean(first?.recordId || first?.skillId || first?.filePath),
    assign: !knowledge,
    details: Boolean(first?.href || (!knowledge && first?.moduleId && first?.submoduleId && first?.recordId)),
    new: Boolean(first?.moduleId && first?.submoduleId)
  };
}

function presetInstruction(action: "prompt" | "edit" | "assign" | "archive", items: ContextMenuItemContext[]) {
  const summary = contextSummary(items);
  const first = items[0];
  if ((action === "prompt" || action === "edit") && isKnowledgeSkillContext(first)) {
    const target = first?.filePath
      ? `file ${first.filePath} in skill ${first.skillId ?? first.label ?? first.recordId}`
      : `skill ${first?.skillId ?? first?.label ?? first?.recordId}`;
    const selectedText = first?.selectedText ? " Use the selected text as the exact edit target." : "";
    return `Change ${target}. Preserve the CTOX skill hierarchy, update the materialized skill file/runbook through CTOX, and return the concrete patch or queued edit plan.${selectedText}`;
  }
  if (action === "archive" && isKnowledgeSkillContext(first)) {
    const target = first?.filePath
      ? `file ${first.filePath} in skill ${first.skillId ?? first.label ?? first.recordId}`
      : `skill context ${first?.skillId ?? first?.label ?? first?.recordId}`;
    return `Queue an archive request for ${target}. Do not delete immediately. Check linked skillbooks, runbooks, source files, and CTOX SQLite materialization first, then return the exact archive patch or dependency warning.`;
  }
  if (action === "prompt") return `Work on ${summary}. Use the attached module, record, route, and selected text context.`;
  if (action === "edit") return `Edit or update ${summary}. Keep the change inside the current Business OS module and preserve CTOX synchronization context.`;
  if (action === "assign") return `Assign or reassign ${summary}. Include the responsible owner, due date, and any follow-up work CTOX should queue.`;
  return `Prepare an archive request for ${summary}. Check dependencies, linked records, and whether this should be hidden, closed, or kept for audit.`;
}

function knowledgeNewRecordId(item: ContextMenuItemContext) {
  if (item.recordType === "ctox_skillbook" || item.recordType === "ctox_runbook") return "new-ctox-runbook";
  if (item.recordType === "ctox_skill" || item.recordType === "ctox_skill_file") return "new-ctox-skill-file";
  return "new-ctox-skill";
}

function isKnowledgeSkillContext(item?: ContextMenuItemContext) {
  if (!item) return false;
  return item.submoduleId === "knowledge" && (
    item.recordType === "ctox_skill" ||
    item.recordType === "ctox_skill_file" ||
    item.recordType === "ctox_runbook" ||
    item.recordType === "ctox_skillbook"
  );
}

function newRecordId(moduleId: string, submoduleId: string, recordType?: string) {
  if (recordType && !recordType.includes("set") && !recordType.includes("signal")) return recordType.replace(/_/g, "-");
  if (moduleId === "sales") {
    if (submoduleId === "accounts") return "account";
    if (submoduleId === "contacts") return "contact";
    if (submoduleId === "leads") return "lead";
    if (submoduleId === "offers") return "offer";
    if (submoduleId === "tasks") return "task";
    return "opportunity";
  }
  if (moduleId === "business") {
    if (submoduleId === "products") return "product";
    if (submoduleId === "invoices") return "invoice";
    if (submoduleId === "bookkeeping") return "export";
    if (submoduleId === "reports") return "report";
    return "customer";
  }
  if (moduleId === "ctox") return submoduleId.slice(0, -1) || "task";
  return "item";
}

function marketingNewResource(submoduleId: string) {
  if (submoduleId === "assets") return "assets";
  if (submoduleId === "campaigns") return "campaigns";
  if (submoduleId === "research") return "research";
  if (submoduleId === "commerce") return "commerce";
  return "website";
}

function contextSummary(items: ContextMenuItemContext[]) {
  const first = items[0];
  if (!first) return "No context selected";
  const target = [first.moduleId, first.submoduleId, first.recordType].filter(Boolean).join(" / ");
  const suffix = items.length > 1 ? ` + ${items.length - 1} more` : "";
  return `${target || "Context"}${suffix}`;
}
