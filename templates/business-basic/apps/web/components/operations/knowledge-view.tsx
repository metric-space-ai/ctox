import type { CtoxKnowledgeSkill, CtoxRunbook, CtoxSkillFile, CtoxSkillbook } from "../../lib/ctox-knowledge-store";
import type { ReactNode } from "react";
import { OperationsQueueButton } from "./operations-actions";
import { KnowledgeResizableLayout } from "./knowledge-resizable-layout";
import { OperationsPaneHead, copyLabel } from "./shell";
import { operationsBaseHref, operationsPanelHref, type OperationsSubmoduleViewProps } from "./types";
import { RunbookDocumentViewer } from "./runbook-document-viewer";

type KnowledgeSelection = {
  activeGroup: SkillFolder;
  groupSkills: CtoxKnowledgeSkill[];
  skill: CtoxKnowledgeSkill;
  skillbooks: CtoxSkillbook[];
  selectedSkillbook?: CtoxSkillbook;
  runbooks: CtoxRunbook[];
  selectedFile?: CtoxSkillFile;
  selectedRunbook?: CtoxRunbook;
};

type SkillFolder = {
  id: string;
  kind: CtoxKnowledgeSkill["kind"];
  name: string;
  label: string;
  skills: CtoxKnowledgeSkill[];
};

export function OperationsKnowledgeView({
  copy,
  data,
  locale,
  query,
  submoduleId
}: OperationsSubmoduleViewProps) {
  const label = (key: string, fallback: string) => copyLabel(copy, key, fallback);
  const store = data.ctoxKnowledge;
  const systemSkills = store.skills.filter((skill) => skill.kind === "system");
  const skills = store.skills.filter((skill) => skill.kind === "skill");
  const folders = buildSkillFolders(store.skills);
  const selection = resolveSelection(store, query, folders);
  const selectedRunbookItems = selection.selectedRunbook
    ? store.runbookItems.filter((item) => item.runbookId === selection.selectedRunbook?.id)
    : [];
  const selectedFiles = selection.skill.files.slice(0, 80);
  const rightPaneTitle = selection.selectedRunbook?.title ?? selection.skill.title;
  const rightPaneDescription = selection.selectedRunbook
    ? label("runbookDescription", "Formatted runbook document, editable in place without changing the workspace layout.")
    : `${label("skillFile", "Skill file")} · ${selection.skill.sourcePath ?? selection.skill.title}`;
  const newSkillHref = knowledgeCreateHref(query, submoduleId, {
    group: selection.activeGroup.id,
    recordId: "new-ctox-skill",
    skillId: selection.skill.id
  });
  const newFileHref = knowledgeCreateHref(query, submoduleId, {
    group: selection.activeGroup.id,
    recordId: "new-ctox-skill-file",
    skillId: selection.skill.id
  });
  const newRunbookHref = knowledgeCreateHref(query, submoduleId, {
    group: selection.activeGroup.id,
    recordId: "new-ctox-runbook",
    skillId: selection.skill.id
  });

  return (
    <KnowledgeResizableLayout>
      <section className="ops-pane ctox-skill-column" aria-label={label("skills", "Skills")}>
        <OperationsPaneHead
          description={label("knowledgeDescription", "Folder structure for CTOX system skills and reusable skills.")}
          title={label("knowledge", "Knowledge Store")}
        />
        <div className="ctox-knowledge-counts">
          <span><strong>{store.counts.systemSkills}</strong><small>{label("systemSkills", "System")}</small></span>
          <span><strong>{store.counts.skills}</strong><small>{label("skills", "Skills")}</small></span>
        </div>
        <div className="ctox-folder-tree" aria-label="Knowledge folders">
          <FolderRoot
            folders={folders.filter((folder) => folder.kind === "skill")}
            label={label("skills", "Skills")}
            query={query}
            selectedFolderId={selection.activeGroup.id}
            submoduleId={submoduleId}
            total={skills.length}
          />
          <FolderRoot
            folders={folders.filter((folder) => folder.kind === "system")}
            label={label("systemSkills", "System Skills")}
            query={query}
            selectedFolderId={selection.activeGroup.id}
            submoduleId={submoduleId}
            total={systemSkills.length}
          />
        </div>
      </section>

      <section className="ops-pane ctox-skillbook-column" aria-label={label("skillbooks", "Skillbooks")}>
        <OperationsPaneHead
          description={`${selection.activeGroup.kind === "skill" ? label("skills", "Skills") : label("systemSkills", "System Skills")} / ${selection.activeGroup.name}`}
          title={selection.activeGroup.label}
        >
          <a
            className="ctox-text-action"
            aria-label={label("newSkill", "New skill")}
            data-context-action="create"
            data-context-label={label("newSkill", "New skill")}
            data-context-module="operations"
            data-context-record-id="new-ctox-skill"
            data-context-record-type="ctox_skill"
            data-context-submodule={submoduleId}
            href={newSkillHref}
            title={label("newSkill", "New skill")}
          >
            {label("newSkill", "New skill")}
          </a>
          <OperationsQueueButton
            action="sync"
            instruction={`Organize CTOX knowledge group ${selection.activeGroup.label}. Review contained skills, linked skillbooks, runbooks, source bindings, and propose missing hierarchy or asset changes.`}
            payload={{ group: selection.activeGroup, skill: selection.skill, skillbooks: selection.skillbooks, runbooks: selection.runbooks }}
            recordId={selection.activeGroup.id}
            resource="knowledge"
            title={`Organize CTOX knowledge group: ${selection.activeGroup.label}`}
          >
            {label("askCtoxOrganize", "Organize")}
          </OperationsQueueButton>
        </OperationsPaneHead>
        <div className="ctox-skill-meta">
          <span><small>{label("kind", "Kind")}</small><strong>{selection.activeGroup.kind === "skill" ? label("skills", "Skills") : label("systemSkills", "System")}</strong></span>
          <span><small>{label("cluster", "Cluster")}</small><strong>{selection.activeGroup.name}</strong></span>
          <span><small>{label("skills", "Skills")}</small><strong>{selection.groupSkills.length}</strong></span>
        </div>
        <div className="ctox-skill-list ctox-group-skill-list">
          <SkillGroup label={selection.activeGroup.label} query={query} selectedSkillId={selection.skill.id} skills={selection.groupSkills} submoduleId={submoduleId} />
        </div>
        <div className="ctox-document-sections">
          <DocumentGroup
            actionHref={newFileHref}
            actionLabel={label("newFile", "New file")}
            count={selectedFiles.length}
            label={label("files", "Files")}
          >
            {selectedFiles.length > 0 ? selectedFiles.map((file) => (
              <a
                className={`ctox-document-row ${!selection.selectedRunbook && selection.selectedFile?.relativePath === file.relativePath ? "active" : ""}`}
                data-context-item
                data-context-label={file.relativePath}
                data-context-file-path={file.relativePath}
                data-context-group={selection.activeGroup.id}
                data-context-module="operations"
                data-context-record-id={`${selection.skill.id}:${file.relativePath}`}
                data-context-record-type="ctox_skill_file"
                data-context-skill-id={selection.skill.id}
                data-context-source-path={selection.skill.sourcePath}
                data-context-submodule={submoduleId}
                href={knowledgeHref(query, submoduleId, { filePath: file.relativePath, group: selection.activeGroup.id, skillId: selection.skill.id })}
                key={file.relativePath}
              >
                <strong>{file.relativePath}</strong>
                <small>{file.contentText ? `${Math.round(file.contentText.length / 100) / 10}k chars` : label("empty", "empty")}</small>
              </a>
            )) : <span className="ctox-document-empty">{label("noAssets", "No files")}</span>}
          </DocumentGroup>

          {selection.skillbooks.length > 0 ? selection.skillbooks.map((skillbook) => (
            <DocumentGroup count={selection.skillbooks.length} key={skillbook.id} label={label("skillbook", "Skillbook")}>
              <a
                className={`ctox-document-row ${selection.selectedSkillbook?.id === skillbook.id ? "active" : ""}`}
                data-context-item
                data-context-label={skillbook.title}
                data-context-group={selection.activeGroup.id}
                data-context-module="operations"
                data-context-record-id={skillbook.id}
                data-context-record-type="ctox_skillbook"
                data-context-skill-id={selection.skill.id}
                data-context-source-path={selection.skill.sourcePath}
                data-context-submodule={submoduleId}
                href={knowledgeHref(query, submoduleId, { group: selection.activeGroup.id, skillId: selection.skill.id, skillbookId: skillbook.id })}
              >
                <strong>{skillbook.title}</strong>
                <small>{skillbook.status} · v{skillbook.version}</small>
              </a>
            </DocumentGroup>
          )) : null}

          {selection.runbooks.length > 0 ? (
            <DocumentGroup actionHref={newRunbookHref} actionLabel={label("newRunbook", "New runbook")} count={selection.runbooks.length} label={label("runbooks", "Runbooks")}>
              {selection.runbooks.map((runbook) => (
                <a
                  className={`ctox-document-row ${selection.selectedRunbook?.id === runbook.id ? "active" : ""}`}
                  data-context-item
                  data-context-label={runbook.title}
                  data-context-group={selection.activeGroup.id}
                  data-context-module="operations"
                  data-context-record-id={runbook.id}
                  data-context-record-type="ctox_runbook"
                  data-context-skill-id={selection.skill.id}
                  data-context-source-path={selection.skill.sourcePath}
                  data-context-submodule={submoduleId}
                  href={knowledgeHref(query, submoduleId, { group: selection.activeGroup.id, skillId: selection.skill.id, skillbookId: runbook.skillbookId, runbookId: runbook.id })}
                  key={runbook.id}
                >
                  <strong>{runbook.title}</strong>
                  <small>{runbook.status} · {runbook.problemDomain}</small>
                </a>
              ))}
            </DocumentGroup>
          ) : (
            <DocumentGroup actionHref={newRunbookHref} actionLabel={label("newRunbook", "New runbook")} count={0} label={label("runbooks", "Runbooks")}>
              <span className="ctox-document-empty">{label("noRunbooks", "No runbooks linked")}</span>
            </DocumentGroup>
          )}
        </div>
      </section>

      <section className="ops-pane ctox-runbook-column" aria-label={label("runbooks", "Runbooks")}>
        <OperationsPaneHead
          actionContext={{ action: "open-set", label: label("knowledgeStorePages", "Knowledge set"), recordId: "knowledge", recordType: "knowledge_set", submoduleId }}
          actionHref={operationsPanelHref(query, submoduleId, "operations-set", "knowledge", "right")}
          actionLabel={label("knowledgeStorePages", "Knowledge set")}
          description={rightPaneDescription}
          title={rightPaneTitle}
        />
        {selection.runbooks.length > 0 ? (
          <div className="ctox-runbook-tabs">
            {selection.runbooks.map((runbook) => (
              <a
                className={selection.selectedRunbook?.id === runbook.id ? "active" : ""}
                data-context-item
                data-context-label={runbook.title}
                data-context-module="operations"
                data-context-record-id={runbook.id}
                data-context-record-type="ctox_runbook"
                data-context-submodule={submoduleId}
                href={knowledgeHref(query, submoduleId, { group: selection.activeGroup.id, skillId: selection.skill.id, skillbookId: runbook.skillbookId, runbookId: runbook.id })}
                key={runbook.id}
              >
                {runbook.title}
              </a>
            ))}
          </div>
        ) : null}
          <RunbookDocumentViewer
          files={selectedFiles}
          locale={locale}
          runbook={selection.selectedRunbook}
          runbookItems={selectedRunbookItems}
          selectedFile={selection.selectedFile}
          selectedSkill={selection.skill}
          groupId={selection.activeGroup.id}
          skillbook={selection.selectedSkillbook}
        />
      </section>
    </KnowledgeResizableLayout>
  );
}

function SkillGroup({
  label,
  query,
  selectedSkillId,
  skills,
  submoduleId
}: {
  label: string;
  query: OperationsSubmoduleViewProps["query"];
  selectedSkillId: string;
  skills: CtoxKnowledgeSkill[];
  submoduleId: string;
}) {
  if (skills.length === 0) {
    return (
      <div className="ctox-skill-group">
        <div className="ctox-skill-group-head"><span>{label}</span><strong>0</strong></div>
      </div>
    );
  }

  return (
    <div className="ctox-skill-group">
      <div className="ctox-skill-group-head"><span>{label}</span><strong>{skills.length}</strong></div>
      {skills.map((skill) => (
        <a
          className={`ctox-skill-row ${selectedSkillId === skill.id ? "active" : ""}`}
          data-context-item
          data-context-label={skill.title}
          data-context-group={groupIdForSkill(skill)}
          data-context-module="operations"
          data-context-record-id={skill.id}
          data-context-record-type="ctox_skill"
          data-context-skill-id={skill.id}
          data-context-source-path={skill.sourcePath}
          data-context-submodule={submoduleId}
          href={knowledgeHref(query, submoduleId, { group: groupIdForSkill(skill), skillId: skill.id })}
          key={skill.id}
        >
          <strong>{skill.title}</strong>
          <small>{skill.cluster || skill.className} · {skill.state}</small>
        </a>
      ))}
    </div>
  );
}

function DocumentGroup({
  actionHref,
  actionLabel,
  children,
  count,
  label
}: {
  actionHref?: string;
  actionLabel?: string;
  children: ReactNode;
  count: number;
  label: string;
}) {
  return (
    <div className="ctox-document-group">
      <div className="ctox-skill-group-head">
        <span>{label}</span>
        <div className="ctox-group-head-actions">
          <strong>{count}</strong>
          {actionHref ? <a aria-label={actionLabel ?? label} href={actionHref} title={actionLabel ?? label}>{actionLabel ?? label}</a> : null}
        </div>
      </div>
      <div className="ctox-document-list">{children}</div>
    </div>
  );
}

function FolderRoot({
  folders,
  label,
  query,
  selectedFolderId,
  submoduleId,
  total
}: {
  folders: SkillFolder[];
  label: string;
  query: OperationsSubmoduleViewProps["query"];
  selectedFolderId: string;
  submoduleId: string;
  total: number;
}) {
  return (
    <div className="ctox-folder-root">
      <div className="ctox-folder-row root" aria-current={folders.some((folder) => folder.id === selectedFolderId) ? "true" : undefined}>
        <span className="ctox-folder-caret">v</span>
        <span className="ctox-folder-icon" aria-hidden="true">[]</span>
        <strong>{label}</strong>
        <small>{total}</small>
      </div>
      <div className="ctox-folder-children">
        {folders.map((folder) => (
          <a
            className={`ctox-folder-row leaf ${folder.id === selectedFolderId ? "active" : ""}`}
            href={knowledgeHref(query, submoduleId, { group: folder.id, skillId: folder.skills[0]?.id })}
            key={folder.id}
          >
            <span className="ctox-folder-caret">&gt;</span>
            <span className="ctox-folder-icon" aria-hidden="true">[]</span>
            <strong>{folder.label}</strong>
            <small>{folder.skills.length}</small>
          </a>
        ))}
      </div>
    </div>
  );
}

function resolveSelection(
  store: OperationsSubmoduleViewProps["data"]["ctoxKnowledge"],
  query: OperationsSubmoduleViewProps["query"],
  folders: SkillFolder[]
): KnowledgeSelection {
  const selectedSkill =
    store.skills.find((skill) => skill.id === query.skillId) ??
    store.skills.find((skill) => skill.id === query.recordId) ??
    folders.find((folder) => folder.id === query.group)?.skills[0] ??
    folders.find((folder) => folder.kind === "skill")?.skills[0] ??
    folders.find((folder) => folder.kind === "system")?.skills[0] ??
    store.skills[0];

  const fallbackSkill = selectedSkill ?? {
    id: "empty",
    name: "empty",
    title: "No CTOX skills found",
    kind: "system" as const,
    className: "empty",
    state: "empty",
    cluster: "system",
    executionModel: "Connect CTOX SQLite to populate the Knowledge Store.",
    linkedSkillbookIds: [],
    linkedRunbookIds: [],
    fileCount: 0,
    files: []
  };

  const activeGroup =
    folders.find((folder) => folder.id === query.group) ??
    folders.find((folder) => folder.skills.some((skill) => skill.id === fallbackSkill.id)) ??
    folders[0] ??
    { id: "system:empty", kind: "system" as const, name: "empty", label: "empty", skills: [fallbackSkill] };
  const groupSkills = activeGroup.skills.length > 0 ? activeGroup.skills : [fallbackSkill];
  const activeSkill = groupSkills.find((skill) => skill.id === fallbackSkill.id) ?? groupSkills[0] ?? fallbackSkill;
  const linkedSkillbooks = activeSkill.linkedSkillbookIds.length > 0
    ? store.skillbooks.filter((skillbook) => activeSkill.linkedSkillbookIds.includes(skillbook.id))
    : [];
  const skillbooks = linkedSkillbooks.length > 0 ? linkedSkillbooks : store.skillbooks.filter((skillbook) => skillbook.id === query.skillbookId);
  const selectedSkillbook =
    skillbooks.find((skillbook) => skillbook.id === query.skillbookId) ??
    skillbooks[0];
  const linkedRunbookIds = new Set([...(selectedSkillbook?.linkedRunbookIds ?? []), ...activeSkill.linkedRunbookIds]);
  const runbooks = linkedRunbookIds.size > 0
    ? store.runbooks.filter((runbook) => linkedRunbookIds.has(runbook.id) || runbook.skillbookId === selectedSkillbook?.id)
    : store.runbooks.filter((runbook) => runbook.skillbookId === selectedSkillbook?.id);
  const selectedRunbook =
    runbooks.find((runbook) => runbook.id === query.runbookId) ??
    runbooks.find((runbook) => runbook.id === query.recordId) ??
    undefined;
  const selectedFile =
    activeSkill.files.find((file) => file.relativePath === query.filePath) ??
    activeSkill.files.find((file) => file.relativePath.toLowerCase() === "skill.md") ??
    activeSkill.files[0];

  return { activeGroup, groupSkills, skill: activeSkill, skillbooks, runbooks, selectedFile, selectedRunbook, selectedSkillbook };
}

function buildSkillFolders(skills: CtoxKnowledgeSkill[]) {
  const foldersById = new Map<string, SkillFolder>();

  skills.forEach((skill) => {
    const id = groupIdForSkill(skill);
    const name = groupNameForSkill(skill);
    const existing = foldersById.get(id);
    if (existing) {
      existing.skills.push(skill);
      return;
    }
    foldersById.set(id, {
      id,
      kind: skill.kind,
      name,
      label: name,
      skills: [skill]
    });
  });

  return Array.from(foldersById.values()).sort((left, right) => {
    if (left.kind !== right.kind) return left.kind === "skill" ? -1 : 1;
    return left.name.localeCompare(right.name);
  });
}

function groupIdForSkill(skill: CtoxKnowledgeSkill) {
  return `${skill.kind}:${groupNameForSkill(skill)}`;
}

function groupNameForSkill(skill: CtoxKnowledgeSkill) {
  return sanitizeGroupName(skill.cluster || skill.className || skill.kind);
}

function sanitizeGroupName(value: string) {
  return value.replace(/^stable/, "").replace(/^skills[/:]/, "").replace(/^system[/:]/, "").replace(/^packs[/:]/, "") || "uncategorized";
}

function knowledgeHref(
  query: OperationsSubmoduleViewProps["query"],
  submoduleId: string,
  next: { filePath?: string; group?: string; skillId?: string; skillbookId?: string; runbookId?: string }
) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  if (next.group ?? query.group) params.set("group", next.group ?? query.group ?? "");
  if (next.skillId) params.set("skillId", next.skillId);
  if (next.filePath) params.set("filePath", next.filePath);
  if (next.skillbookId) params.set("skillbookId", next.skillbookId);
  if (next.runbookId) params.set("runbookId", next.runbookId);
  const queryString = params.toString();
  return queryString ? `/app/operations/${submoduleId}?${queryString}` : operationsBaseHref(query, submoduleId);
}

function knowledgeCreateHref(
  query: OperationsSubmoduleViewProps["query"],
  submoduleId: string,
  next: { group?: string; recordId: "new-ctox-skill" | "new-ctox-skill-file" | "new-ctox-skillbook" | "new-ctox-runbook"; skillId?: string }
) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  if (next.group ?? query.group) params.set("group", next.group ?? query.group ?? "");
  if (next.skillId ?? query.skillId) params.set("skillId", next.skillId ?? query.skillId ?? "");
  params.set("panel", "new");
  params.set("recordId", next.recordId);
  params.set("drawer", "left-bottom");
  return `/app/operations/${submoduleId}?${params.toString()}`;
}
