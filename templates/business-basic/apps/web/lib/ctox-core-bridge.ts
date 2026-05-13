import { execFile } from "node:child_process";
import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

export type CtoxCoreTaskRequest = {
  title: string;
  prompt: string;
  source: string;
  context?: Record<string, unknown>;
  priority?: "urgent" | "high" | "normal" | "low";
  skill?: string;
  threadKey?: string;
  workspaceRoot?: string;
};

export type CtoxCoreEventRequest = {
  type: string;
  module: string;
  recordType: string;
  recordId: string;
  payload: Record<string, unknown>;
};

export type CtoxDeepResearchRequest = {
  query: string;
  focus?: string;
  depth?: "quick" | "standard" | "exhaustive";
  maxSources?: number;
  workspace?: string;
};

export type CtoxSourceReviewDiscoveryRequest = {
  topic: string;
  runId: string;
  title?: string;
  queries?: Array<{ focus: string; query: string }>;
  targetAdditionalSources?: number;
  workspace?: string;
  existingDiscoveryDir?: string;
  databaseUrl?: string;
  openaiApiKey?: string;
  storeKey?: string;
};

type CtoxQueueTask = {
  message_key?: string;
  title?: string;
  thread_key?: string;
  route_status?: string;
};

export type CtoxCoreTaskDedupeOptions = {
  threadKey: string;
  statuses?: Array<"pending" | "leased" | "blocked" | "failed">;
  updatePrompt?: string;
  updateTitle?: string;
};

export type CtoxHarnessFlow = {
  schema_version: number;
  source: {
    message_key?: string | null;
    work_id?: string | null;
    source_kind: string;
  };
  ledger_events: Array<{
    event_id: string;
    chain_key: string;
    event_kind: string;
    title: string;
    body_text: string;
    message_key?: string | null;
    work_id?: string | null;
    ticket_key?: string | null;
    attempt_index?: number | null;
    metadata_json: string;
    created_at: string;
  }>;
  blocks: CtoxHarnessBlock[];
};

export type CtoxHarnessBlock = {
  kind: "task" | "attempt" | "finish" | "empty";
  title: string;
  lines: string[];
  branches: CtoxHarnessBranch[];
};

export type CtoxHarnessBranch = {
  kind:
    | "queue_pickup"
    | "context"
    | "knowledge"
    | "review"
    | "ticket_backlog"
    | "ticket_source"
    | "queue_reload"
    | "guard"
    | "state_machine"
    | "verification"
    | "process_mining"
    | "harness_ledger";
  title: string;
  lines: string[];
  returns_to_spine: boolean;
};

export type CtoxHarnessFlowResult = {
  ok: boolean;
  mode: "bridge_http" | "ctox_cli" | "fallback";
  flow: CtoxHarnessFlow;
  ascii: string;
  error?: string;
};

export async function createCtoxCoreTask(request: CtoxCoreTaskRequest) {
  const bridgeUrl = process.env.CTOX_BRIDGE_URL?.replace(/\/$/, "");
  const token = process.env.CTOX_BRIDGE_TOKEN;

  if (bridgeUrl) {
    const response = await fetch(`${bridgeUrl}/queue-tasks`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        ...(token ? { authorization: `Bearer ${token}` } : {})
      },
      body: JSON.stringify({
        instruction: request.prompt,
        context: {
          source: request.source,
          title: request.title,
          priority: request.priority ?? "normal",
          skill: request.skill,
          threadKey: request.threadKey,
          workspaceRoot: request.workspaceRoot,
          ...request.context
        }
      })
    });

    if (response.ok) {
      const payload = await response.json().catch(() => null) as { task?: { id?: string; message_key?: string } } | null;
      return {
        ok: true,
        mode: "bridge_http",
        task: payload?.task ?? null,
        taskId: payload?.task?.id ?? payload?.task?.message_key ?? null
      };
    }
  }

  const command = buildQueueAddCommand(request);
  if (shouldExecuteCoreQueue()) {
    const [binary, ...args] = command;
    try {
      const { stdout } = await execFileAsync(binary, args, { cwd: process.env.CTOX_ROOT });
      const payload = JSON.parse(stdout) as { task?: { message_key?: string; title?: string } };
      return {
        ok: true,
        mode: "ctox_cli",
        task: payload.task ?? null,
        taskId: payload.task?.message_key ?? null
      };
    } catch (error) {
      return plannedTask(request, command, error instanceof Error ? error.message : String(error));
    }
  }

  return plannedTask(request, command);
}

export async function upsertCtoxCoreTask(request: CtoxCoreTaskRequest, options: CtoxCoreTaskDedupeOptions) {
  const bridgeUrl = process.env.CTOX_BRIDGE_URL?.replace(/\/$/, "");
  if (bridgeUrl) {
    return createCtoxCoreTask({
      ...request,
      context: {
        dedupeThreadKey: options.threadKey,
        ...request.context
      }
    });
  }

  const addCommand = buildQueueAddCommand(request);
  if (!shouldExecuteCoreQueue()) return plannedTask(request, addCommand);

  try {
    const existing = await findCtoxQueueTask(options.threadKey, options.statuses ?? ["pending", "leased", "blocked", "failed"]);
    if (existing?.message_key) {
      const command = buildQueueEditCommand(
        existing.message_key,
        request,
        options.updatePrompt ?? request.prompt,
        options.updateTitle ?? request.title
      );
      const [binary, ...args] = command;
      const { stdout } = await execFileAsync(binary, args, { cwd: process.env.CTOX_ROOT, maxBuffer: 32 * 1024 * 1024 });
      const payload = JSON.parse(stdout) as { task?: { message_key?: string; title?: string } };
      return {
        ok: true,
        mode: "ctox_cli_existing",
        task: payload.task ?? existing,
        taskId: payload.task?.message_key ?? existing.message_key
      };
    }
  } catch (error) {
    return plannedTask(request, addCommand, error instanceof Error ? error.message : String(error));
  }

  return createCtoxCoreTask(request);
}

export async function emitCtoxCoreEvent(request: CtoxCoreEventRequest) {
  const bridgeUrl = process.env.CTOX_BRIDGE_URL?.replace(/\/$/, "");
  const token = process.env.CTOX_BRIDGE_TOKEN;
  const event = {
    id: crypto.randomUUID(),
    type: request.type,
    module: request.module,
    recordType: request.recordType,
    recordId: request.recordId,
    occurredAt: new Date().toISOString(),
    payload: request.payload
  };

  if (bridgeUrl) {
    const response = await fetch(`${bridgeUrl}/events`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        ...(token ? { authorization: `Bearer ${token}` } : {})
      },
      body: JSON.stringify(event)
    });

    if (response.ok) return { ok: true, mode: "bridge_http", event };
  }

  return { ok: true, mode: "local_event", event };
}

export async function runCtoxDeepResearch(request: CtoxDeepResearchRequest) {
  const bridgeUrl = process.env.CTOX_BRIDGE_URL?.replace(/\/$/, "");
  const token = process.env.CTOX_BRIDGE_TOKEN;

  if (bridgeUrl) {
    const response = await fetch(`${bridgeUrl}/deep-research`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        ...(token ? { authorization: `Bearer ${token}` } : {})
      },
      body: JSON.stringify(request)
    });

    if (!response.ok) {
      const payload = await response.json().catch(() => null) as { error?: string } | null;
      throw new Error(payload?.error ?? `deep_research_bridge_failed_${response.status}`);
    }

    return await response.json() as Record<string, unknown>;
  }

  if (!shouldExecuteCoreQueue()) throw new Error("deep_research_bridge_not_configured");

  const args = ["web", "deep-research", "--query", request.query];
  if (request.focus) args.push("--focus", request.focus);
  if (request.depth) args.push("--depth", request.depth);
  if (request.maxSources) args.push("--max-sources", String(request.maxSources));
  if (request.workspace) args.push("--workspace", request.workspace);
  const { stdout } = await execFileAsync(resolveCtoxBinary(), args, {
    cwd: process.env.CTOX_ROOT,
    maxBuffer: 64 * 1024 * 1024
  });
  return JSON.parse(stdout) as Record<string, unknown>;
}

export async function runCtoxSourceReviewDiscovery(request: CtoxSourceReviewDiscoveryRequest) {
  const bridgeUrl = process.env.CTOX_BRIDGE_URL?.replace(/\/$/, "");
  const token = process.env.CTOX_BRIDGE_TOKEN;

  if (bridgeUrl) {
    const response = await fetch(`${bridgeUrl}/source-review-discovery`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        ...(token ? { authorization: `Bearer ${token}` } : {})
      },
      body: JSON.stringify(request)
    });

    if (!response.ok) {
      const payload = await response.json().catch(() => null) as { error?: string } | null;
      throw new Error(payload?.error ?? `source_review_discovery_bridge_failed_${response.status}`);
    }

    return await response.json() as Record<string, unknown>;
  }

  if (!shouldExecuteCoreQueue()) throw new Error("source_review_discovery_bridge_not_configured");

  const outDir = request.workspace ?? `/tmp/ctox-business-source-review-${request.runId}`;
  let queriesFile = "";
  if (request.queries?.length) {
    const dir = await mkdtemp(join(tmpdir(), "ctox-source-review-"));
    queriesFile = join(dir, "queries.csv");
    const rows = ["focus,query", ...request.queries.map((item) => `${csvCell(item.focus)},${csvCell(item.query)}`)];
    await writeFile(queriesFile, `${rows.join("\n")}\n`, "utf8");
  }
  const args = [
    "skills/system/research/deep-research/scripts/source_review_discovery.py",
    "--topic",
    request.topic,
    "--out-dir",
    outDir,
    "--max-sources-per-query",
    "50",
    "--target-reviewed",
    String(Math.max(50, request.targetAdditionalSources ?? 50)),
    "--discovery-backend",
    "hybrid",
    "--query-timeout-sec",
    "45",
    "--web-query-delay-sec",
    "1",
    "--snowball-rounds",
    "1"
  ];
  if (queriesFile) args.push("--queries-file", queriesFile);
  else args.push("--allow-auto-query-plan");
  args.push("--llm-screening");
  if (request.existingDiscoveryDir) args.push("--existing-discovery-dir", request.existingDiscoveryDir);
  if (request.targetAdditionalSources) args.push("--target-additional-candidates", String(request.targetAdditionalSources));
  if (request.databaseUrl) {
    args.push(
      "--business-writeback",
      "--business-database-url",
      request.databaseUrl,
      "--business-research-run-id",
      request.runId,
      "--business-research-title",
      request.title ?? request.topic.slice(0, 80),
      "--business-store-key",
      request.storeKey ?? "marketing/research/runs"
    );
  }
  const { stdout } = await execFileAsync("python3", args, {
    cwd: process.env.CTOX_ROOT,
    maxBuffer: 64 * 1024 * 1024
  });
  return JSON.parse(stdout) as Record<string, unknown>;
}

function csvCell(value: string) {
  return `"${value.replace(/"/g, '""')}"`;
}

export async function getCtoxHarnessFlow(): Promise<CtoxHarnessFlowResult> {
  const bridgeUrl = process.env.CTOX_BRIDGE_URL?.replace(/\/$/, "");
  const token = process.env.CTOX_BRIDGE_TOKEN;

  if (bridgeUrl) {
    try {
      const response = await fetch(`${bridgeUrl}/harness-flow/latest`, {
        headers: {
          accept: "application/json",
          ...(token ? { authorization: `Bearer ${token}` } : {})
        },
        cache: "no-store"
      });
      if (response.ok) {
        const payload = await response.json().catch(() => null) as Partial<CtoxHarnessFlowResult> & { flow?: CtoxHarnessFlow } | null;
        const flow = payload?.flow;
        if (flow?.blocks?.length) {
          return {
            ok: true,
            mode: "bridge_http",
            flow,
            ascii: payload?.ascii ?? renderHarnessAscii(flow)
          };
        }
      }
    } catch {
      // Fall through to the local CLI/fallback path.
    }
  }

  if (shouldExecuteCoreQueue()) {
    const command = [resolveCtoxBinary(), "harness-flow", "--latest", "--json"];
    const [binary, ...args] = command;
    try {
      const { stdout } = await execFileAsync(binary, args, {
        cwd: process.env.CTOX_ROOT,
        maxBuffer: 2 * 1024 * 1024
      });
      const flow = JSON.parse(stdout) as CtoxHarnessFlow;
      return {
        ok: true,
        mode: "ctox_cli",
        flow,
        ascii: renderHarnessAscii(flow)
      };
    } catch (error) {
      const flow = fallbackHarnessFlow(error instanceof Error ? error.message : String(error));
      return {
        ok: true,
        mode: "fallback",
        flow,
        ascii: renderHarnessAscii(flow),
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  const flow = fallbackHarnessFlow();
  return {
    ok: true,
    mode: "fallback",
    flow,
    ascii: renderHarnessAscii(flow)
  };
}

function buildQueueAddCommand(request: CtoxCoreTaskRequest) {
  const prompt = [
    request.prompt,
    "",
    "Business stack context:",
    JSON.stringify(request.context ?? {}, null, 2)
  ].join("\n");

  const command = [
    resolveCtoxBinary(),
    "queue",
    "add",
    "--title",
    request.title,
    "--prompt",
    prompt,
    "--thread-key",
    request.threadKey ?? `business/${request.source}`,
    "--priority",
    request.priority ?? "normal"
  ];

  if (request.skill) command.push("--skill", request.skill);
  if (request.workspaceRoot) command.push("--workspace-root", request.workspaceRoot);

  return command;
}

function buildQueueEditCommand(messageKey: string, request: CtoxCoreTaskRequest, prompt: string, title: string) {
  const command = [
    resolveCtoxBinary(),
    "queue",
    "edit",
    "--message-key",
    messageKey,
    "--title",
    title,
    "--prompt",
    buildQueuePrompt({ ...request, prompt }),
    "--thread-key",
    request.threadKey ?? `business/${request.source}`,
    "--priority",
    request.priority ?? "normal"
  ];

  if (request.skill) command.push("--skill", request.skill);
  if (request.workspaceRoot) command.push("--workspace-root", request.workspaceRoot);

  return command;
}

function buildQueuePrompt(request: CtoxCoreTaskRequest) {
  return [
    request.prompt,
    "",
    "Business stack context:",
    JSON.stringify(request.context ?? {}, null, 2)
  ].join("\n");
}

async function findCtoxQueueTask(threadKey: string, statuses: string[]) {
  const command = [resolveCtoxBinary(), "queue", "list", "--limit", "2000", "--json"];
  statuses.forEach((status) => command.push("--status", status));
  const [binary, ...args] = command;
  const { stdout } = await execFileAsync(binary, args, { cwd: process.env.CTOX_ROOT, maxBuffer: 32 * 1024 * 1024 });
  const payload = JSON.parse(stdout) as { tasks?: CtoxQueueTask[] };
  const matches = payload.tasks?.filter((task) => task.thread_key === threadKey) ?? [];
  return matches.find((task) => task.title?.startsWith("Bug inbox:")) ?? matches[0] ?? null;
}

function resolveCtoxBinary() {
  return process.env.CTOX_BIN ?? process.env.CTOX_WEB_BIN ?? "ctox";
}

function shouldExecuteCoreQueue() {
  return process.env.CTOX_BUSINESS_QUEUE_MODE !== "planned";
}

function plannedTask(request: CtoxCoreTaskRequest, command: string[], error?: string) {
  return {
    ok: true,
    mode: "planned",
    task: {
      id: crypto.randomUUID(),
      title: request.title,
      status: "queued",
      source: request.source
    },
    taskId: null,
    command,
    error
  };
}

function fallbackHarnessFlow(error?: string): CtoxHarnessFlow {
  return {
    schema_version: 1,
    source: {
      message_key: null,
      work_id: null,
      source_kind: "fallback"
    },
    ledger_events: [],
    blocks: [
      {
        kind: "task",
        title: "TASK",
        lines: [
          "Latest CTOX runtime flow is unavailable in this Business OS environment.",
          error ? `Bridge detail: ${clip(error, 96)}` : "Connect CTOX_BRIDGE_URL or CTOX_BIN to show live runtime state."
        ],
        branches: [
          {
            kind: "queue_pickup",
            title: "QUEUE PICKUP",
            lines: ["Queue state is read from CTOX core when the bridge is connected."],
            returns_to_spine: true
          },
          {
            kind: "context",
            title: "CONTEXT",
            lines: ["Mission, continuity, and knowledge state stay attached to the same work item."],
            returns_to_spine: true
          }
        ]
      },
      {
        kind: "attempt",
        title: "AGENT RUN",
        lines: [
          "The main agent performs the work and creates required durable artifacts itself.",
          "Review feedback is incorporated into this same work item whenever possible."
        ],
        branches: [
          {
            kind: "review",
            title: "REVIEW GATE",
            lines: [
              "Quality checkpoint only.",
              "It can reject wording, substance, or stale state, but it does not perform the task."
            ],
            returns_to_spine: true
          }
        ]
      },
      {
        kind: "finish",
        title: "FINISH / CURRENT STATE",
        lines: ["Terminal completion requires kernel evidence, not a natural-language claim."],
        branches: [
          {
            kind: "state_machine",
            title: "HARNESS STATE MACHINE",
            lines: [
              "Outcome witness: expected artifact refs must match delivered artifact refs.",
              "Spawn discipline: every child task needs a modeled parent edge and bounded budget.",
              "Rejected kernel proofs resume the original work item with clear corrective feedback."
            ],
            returns_to_spine: true
          },
          {
            kind: "guard",
            title: "SEND / CLOSE GUARD",
            lines: [
              "Outbound email is complete only after the accepted outbound artifact exists.",
              "Provider failures, rate limits, or missing artifacts keep the work open for retry."
            ],
            returns_to_spine: true
          },
          {
            kind: "process_mining",
            title: "PROCESS MINING",
            lines: [
              "Forensics reads core proofs, outcome refs, spawn edges, stuck cases, and conformance.",
              "Use process-mining proofs, spawn-edges, and harness-mining multiperspective for live checks."
            ],
            returns_to_spine: true
          }
        ]
      }
    ]
  };
}

function renderHarnessAscii(flow: CtoxHarnessFlow) {
  const lines: string[] = [];
  flow.blocks.forEach((block, index) => {
    lines.push(...renderAsciiBox(block.title, block.lines, 70));
    block.branches.forEach((branch) => {
      lines.push("                                  │");
      renderAsciiBox(branch.title, branch.lines, 58).forEach((line, branchIndex) => {
        lines.push(`                                  ${branchIndex === 0 ? "├──►" : "│   "}${line}`);
      });
      if (branch.returns_to_spine) lines.push("                                  │");
    });
    if (index + 1 < flow.blocks.length) {
      lines.push("                                  │");
      lines.push("                                  ▼");
    }
  });
  return lines.join("\n");
}

function renderAsciiBox(title: string, content: string[], width: number) {
  const inner = width - 2;
  const rows = [`┌${"─".repeat(inner)}┐`, asciiBoxLine(title, inner)];
  content.forEach((line) => {
    wrapWords(line, inner - 2).forEach((wrapped) => rows.push(asciiBoxLine(`  ${wrapped}`, inner)));
  });
  rows.push(`└${"─".repeat(inner)}┘`);
  return rows;
}

function asciiBoxLine(text: string, width: number) {
  const clipped = clip(text, width);
  return `│${clipped}${" ".repeat(Math.max(0, width - [...clipped].length))}│`;
}

function wrapWords(text: string, width: number) {
  if ([...text].length <= width) return [text];
  const rows: string[] = [];
  let current = "";
  text.split(/\s+/).forEach((word) => {
    const next = current ? `${current} ${word}` : word;
    if ([...next].length > width && current) {
      rows.push(current);
      current = word;
    } else {
      current = next;
    }
  });
  if (current) rows.push(current);
  return rows.length ? rows : [clip(text, width)];
}

function clip(value: string, max: number) {
  const cleaned = value.replace(/\s+/g, " ").trim();
  if ([...cleaned].length <= max) return cleaned;
  return `${[...cleaned].slice(0, Math.max(0, max - 3)).join("")}...`;
}
