import { execFile } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

export type CtoxKnowledgeSkill = {
  id: string;
  name: string;
  title: string;
  kind: "system" | "skill";
  className: string;
  state: string;
  cluster: string;
  sourcePath?: string;
  executionModel: string;
  linkedSkillbookIds: string[];
  linkedRunbookIds: string[];
  fileCount: number;
  files: CtoxSkillFile[];
};

export type CtoxMainSkill = {
  id: string;
  title: string;
  primaryChannel: string;
  entryAction: string;
  resolverContract: string;
  executionContract: string;
  resolveFlow: string[];
  writebackFlow: string[];
  linkedSkillbookIds: string[];
  linkedRunbookIds: string[];
  updatedAt: string;
};

export type CtoxSkillbook = {
  id: string;
  title: string;
  version: string;
  status: string;
  summary: string;
  mission: string;
  runtimePolicy: string;
  answerContract: string;
  workflowBackbone: string[];
  routingTaxonomy: string[];
  linkedRunbookIds: string[];
  updatedAt: string;
};

export type CtoxRunbook = {
  id: string;
  skillbookId: string;
  title: string;
  version: string;
  status: string;
  summary: string;
  problemDomain: string;
  itemLabels: string[];
  updatedAt: string;
};

export type CtoxRunbookItem = {
  id: string;
  runbookId: string;
  skillbookId: string;
  label: string;
  title: string;
  problemClass: string;
  chunkText: string;
  structured: Record<string, unknown>;
  status: string;
  version: string;
  updatedAt: string;
};

export type CtoxSkillFile = {
  skillId: string;
  relativePath: string;
  contentText?: string;
};

export type CtoxKnowledgeStore = {
  source: "sqlite" | "seed";
  sqlitePath?: string;
  skills: CtoxKnowledgeSkill[];
  mainSkills: CtoxMainSkill[];
  skillbooks: CtoxSkillbook[];
  runbooks: CtoxRunbook[];
  runbookItems: CtoxRunbookItem[];
  sourceBindings: Array<{
    sourceSystem: string;
    skillName: string;
    archetype: string;
    status: string;
    origin: string;
    artifactPath?: string;
  }>;
  counts: {
    systemSkills: number;
    skills: number;
    mainSkills: number;
    skillbooks: number;
    runbooks: number;
    runbookItems: number;
  };
};

type SkillBundleRow = {
  skill_id: string;
  skill_name: string;
  class: string;
  state: string;
  cluster: string;
  source_path: string | null;
  file_count?: number;
};

type MainSkillRow = {
  main_skill_id: string;
  title: string;
  primary_channel: string;
  entry_action: string;
  resolver_contract_json: string;
  execution_contract_json: string;
  resolve_flow_json: string;
  writeback_flow_json: string;
  linked_skillbooks_json: string;
  linked_runbooks_json: string;
  updated_at: string;
};

type SkillbookRow = {
  skillbook_id: string;
  title: string;
  version: string;
  status: string;
  summary: string;
  mission: string;
  runtime_policy: string;
  answer_contract: string;
  workflow_backbone_json: string;
  routing_taxonomy_json: string;
  linked_runbooks_json: string;
  updated_at: string;
};

type RunbookRow = {
  runbook_id: string;
  skillbook_id: string;
  title: string;
  version: string;
  status: string;
  summary: string;
  problem_domain: string;
  item_labels_json: string;
  updated_at: string;
};

type RunbookItemRow = {
  item_id: string;
  runbook_id: string;
  skillbook_id: string;
  label: string;
  title: string;
  problem_class: string;
  chunk_text: string;
  structured_json: string;
  status: string;
  version: string;
  updated_at: string;
};

type SkillFileRow = {
  skill_id: string;
  relative_path: string;
  content_text?: string | null;
};

type SourceBindingRow = {
  source_system: string;
  skill_name: string;
  archetype: string;
  status: string;
  origin: string;
  artifact_path: string | null;
};

export async function getCtoxKnowledgeStore(): Promise<CtoxKnowledgeStore> {
  const sqlitePath = resolveCtoxSqlitePath();
  if (!sqlitePath) return seedKnowledgeStore();

  try {
    const bundleRows = await sqliteJson<SkillBundleRow>(sqlitePath, `
        SELECT b.skill_id, b.skill_name, b.class, b.state, b.cluster, b.source_path,
          COALESCE(f.file_count, 0) AS file_count
        FROM ctox_skill_bundles b
        LEFT JOIN (
          SELECT skill_id, COUNT(*) AS file_count FROM ctox_skill_files GROUP BY skill_id
        ) f ON f.skill_id = b.skill_id
        ORDER BY b.class, b.state, b.cluster, b.skill_name
      `);
    const mainRows = await sqliteJson<MainSkillRow>(sqlitePath, "SELECT * FROM knowledge_main_skills ORDER BY updated_at DESC, title");
    const skillbookRows = await sqliteJson<SkillbookRow>(sqlitePath, "SELECT * FROM knowledge_skillbooks ORDER BY updated_at DESC, title");
    const runbookRows = await sqliteJson<RunbookRow>(sqlitePath, "SELECT * FROM knowledge_runbooks ORDER BY updated_at DESC, title");
    const itemRows = await sqliteJson<RunbookItemRow>(sqlitePath, "SELECT * FROM knowledge_runbook_items ORDER BY runbook_id, label, updated_at DESC LIMIT 500");
    const fileRows = await sqliteJson<SkillFileRow>(sqlitePath, "SELECT skill_id, relative_path, substr(CAST(content_blob AS TEXT), 1, 60000) AS content_text FROM ctox_skill_files ORDER BY skill_id, relative_path LIMIT 1200");
    const bindingRows = await sqliteJson<SourceBindingRow>(sqlitePath, "SELECT source_system, skill_name, archetype, status, origin, artifact_path FROM ticket_source_skill_bindings ORDER BY source_system");

    const filesBySkill = new Map<string, CtoxSkillFile[]>();
    for (const row of fileRows) {
      const files = filesBySkill.get(row.skill_id) ?? [];
      files.push({ skillId: row.skill_id, relativePath: row.relative_path, contentText: row.content_text ?? undefined });
      filesBySkill.set(row.skill_id, files);
    }

    const mainSkills = mainRows.map(mapMainSkill);
    const skills: CtoxKnowledgeSkill[] = [
      ...bundleRows.map((row): CtoxKnowledgeSkill => {
        const kind = classifySkillKind(row);
        return {
          id: row.skill_id,
          name: row.skill_name,
          title: row.skill_name,
          kind,
          className: row.class,
          state: row.state,
          cluster: row.cluster || (kind === "skill" ? "skill" : "system"),
          sourcePath: row.source_path ?? undefined,
          executionModel: kind === "skill"
            ? "Skill: can call arbitrary local CLI, files, services, or user-defined tooling outside CTOX CLI primitives."
            : "System skill: routes through CTOX-controlled CLI/tool primitives and stored runtime contracts.",
          linkedSkillbookIds: [],
          linkedRunbookIds: [],
          fileCount: row.file_count ?? 0,
          files: filesBySkill.get(row.skill_id) ?? []
        };
      })
    ];

    const skillbooks = skillbookRows.map(mapSkillbook);
    const runbooks = runbookRows.map(mapRunbook);
    const runbookItems = itemRows.map(mapRunbookItem);
    const skillCount = skills.filter((skill) => skill.kind === "skill").length;
    const systemSkills = skills.filter((skill) => skill.kind === "system").length;

    return {
      source: "sqlite",
      sqlitePath,
      skills,
      mainSkills,
      skillbooks,
      runbooks,
      runbookItems,
      sourceBindings: bindingRows.map((row) => ({
        sourceSystem: row.source_system,
        skillName: row.skill_name,
        archetype: row.archetype,
        status: row.status,
        origin: row.origin,
        artifactPath: row.artifact_path ?? undefined
      })),
      counts: {
        systemSkills,
        skills: skillCount,
        mainSkills: mainSkills.length,
        skillbooks: skillbooks.length,
        runbooks: runbooks.length,
        runbookItems: runbookItems.length
      }
    };
  } catch (error) {
    console.warn("Falling back to seeded CTOX knowledge store.", error);
    return seedKnowledgeStore(sqlitePath);
  }
}

async function sqliteJson<T>(sqlitePath: string, sql: string): Promise<T[]> {
  let lastError: unknown;
  for (let attempt = 0; attempt < 4; attempt += 1) {
    try {
      const { stdout } = await execFileAsync("sqlite3", ["-json", "-cmd", ".timeout 3000", sqlitePath, sql], {
        maxBuffer: 1024 * 1024 * 8
      });
      if (!stdout.trim()) return [];
      return JSON.parse(stdout) as T[];
    } catch (error) {
      lastError = error;
      if (!String(error).includes("database is locked")) throw error;
      await wait(140 * (attempt + 1));
    }
  }
  throw lastError;
}

function wait(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function classifySkillKind(row: SkillBundleRow): CtoxKnowledgeSkill["kind"] {
  const sourcePath = row.source_path ?? "";
  if (row.class === "personal") return "skill";
  if (sourcePath.includes("/skills/packs/") || sourcePath.includes("skills/packs/")) return "skill";
  return "system";
}

function mapMainSkill(row: MainSkillRow): CtoxMainSkill {
  return {
    id: row.main_skill_id,
    title: row.title,
    primaryChannel: row.primary_channel,
    entryAction: row.entry_action,
    resolverContract: row.resolver_contract_json,
    executionContract: row.execution_contract_json,
    resolveFlow: parseJson(row.resolve_flow_json, []),
    writebackFlow: parseJson(row.writeback_flow_json, []),
    linkedSkillbookIds: parseJson(row.linked_skillbooks_json, []),
    linkedRunbookIds: parseJson(row.linked_runbooks_json, []),
    updatedAt: row.updated_at
  };
}

function mapSkillbook(row: SkillbookRow): CtoxSkillbook {
  return {
    id: row.skillbook_id,
    title: row.title,
    version: row.version,
    status: row.status,
    summary: row.summary,
    mission: row.mission,
    runtimePolicy: row.runtime_policy,
    answerContract: row.answer_contract,
    workflowBackbone: parseJson(row.workflow_backbone_json, []),
    routingTaxonomy: parseJson(row.routing_taxonomy_json, []),
    linkedRunbookIds: parseJson(row.linked_runbooks_json, []),
    updatedAt: row.updated_at
  };
}

function mapRunbook(row: RunbookRow): CtoxRunbook {
  return {
    id: row.runbook_id,
    skillbookId: row.skillbook_id,
    title: row.title,
    version: row.version,
    status: row.status,
    summary: row.summary,
    problemDomain: row.problem_domain,
    itemLabels: parseJson(row.item_labels_json, []),
    updatedAt: row.updated_at
  };
}

function mapRunbookItem(row: RunbookItemRow): CtoxRunbookItem {
  return {
    id: row.item_id,
    runbookId: row.runbook_id,
    skillbookId: row.skillbook_id,
    label: row.label,
    title: row.title,
    problemClass: row.problem_class,
    chunkText: row.chunk_text,
    structured: parseJson(row.structured_json, {}),
    status: row.status,
    version: row.version,
    updatedAt: row.updated_at
  };
}

function parseJson<T>(value: string, fallback: T): T {
  try {
    return JSON.parse(value) as T;
  } catch {
    return fallback;
  }
}

function resolveCtoxSqlitePath() {
  const configured = process.env.CTOX_SQLITE_PATH;
  if (configured && existsSync(configured)) return configured;
  const root = resolveCtoxRoot();
  const candidate = root ? join(root, "runtime", "ctox.sqlite3") : undefined;
  return candidate && existsSync(candidate) ? candidate : undefined;
}

function resolveCtoxRoot() {
  const configured = process.env.CTOX_ROOT;
  if (configured && existsSync(join(configured, "runtime"))) return configured;

  let current = process.cwd();
  for (let index = 0; index < 8; index += 1) {
    if (existsSync(join(current, "runtime", "ctox.sqlite3")) && existsSync(join(current, "src"))) return current;
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
  return undefined;
}

function seedKnowledgeStore(sqlitePath?: string): CtoxKnowledgeStore {
  const skillbook: CtoxSkillbook = {
    id: "business-basic-skillbook",
    title: "Business Basic Skillbook",
    version: "0.1",
    status: "draft",
    summary: "Fallback skillbook for CTOX Business OS knowledge wiring.",
    mission: "Keep the CTOX Business OS installable, customizable, and linked to CTOX core.",
    runtimePolicy: "Use CTOX queue tasks and preserve customer customizations.",
    answerContract: "Return clear next action, linked records, and verification route.",
    workflowBackbone: ["Resolve module context", "Load linked runbook", "Queue CTOX action"],
    routingTaxonomy: ["business-stack", "operations", "knowledge"],
    linkedRunbookIds: ["business-basic-runbook"],
    updatedAt: new Date().toISOString()
  };
  const runbook: CtoxRunbook = {
    id: "business-basic-runbook",
    skillbookId: skillbook.id,
    title: "CTOX Business OS Knowledge Wiring",
    version: "0.1",
    status: "draft",
    summary: "Fallback runbook shown when CTOX SQLite is unavailable.",
    problemDomain: "business-stack",
    itemLabels: ["BB-01"],
    updatedAt: new Date().toISOString()
  };

  return {
    source: "seed",
    sqlitePath,
    skills: [{
      id: "business-basic-meta",
      name: "business-basic-meta",
      title: "Business Basic System Skill",
      kind: "system",
      className: "fallback",
      state: "draft",
      cluster: "meta",
      executionModel: "Fallback projection; connect CTOX SQLite for live skills.",
      linkedSkillbookIds: [skillbook.id],
      linkedRunbookIds: [runbook.id],
      fileCount: 0,
      files: []
    }],
    mainSkills: [],
    skillbooks: [skillbook],
    runbooks: [runbook],
    runbookItems: [{
      id: "business-basic-runbook.bb-01",
      runbookId: runbook.id,
      skillbookId: skillbook.id,
      label: "BB-01",
      title: "Verify CTOX Knowledge Store",
      problemClass: "business-basic.knowledge",
      chunkText: "Connect runtime/ctox.sqlite3, load skills, then render main skills, skillbooks, runbooks, and runbook items.",
      structured: {
        expected_guidance: "Show the CTOX knowledge hierarchy and queue edits through CTOX.",
        verification: ["Knowledge view renders live SQLite counts."]
      },
      status: "draft",
      version: "0.1",
      updatedAt: new Date().toISOString()
    }],
    sourceBindings: [],
    counts: {
      systemSkills: 0,
      skills: 0,
      mainSkills: 1,
      skillbooks: 1,
      runbooks: 1,
      runbookItems: 1
    }
  };
}
