import {
  businessModules,
  findSkillApp,
  getSkillAppsForModule,
  type SkillAppBinding,
  type WorkSurfacePanelState
} from "@ctox-business/ui";
import { readBusinessOsNavigationState } from "../lib/ctox-core-bridge";

type QueryState = {
  drawer?: string;
  locale?: string;
  mode?: string;
  panel?: string;
  recordId?: string;
  selectedId?: string;
  skillId?: string;
  theme?: string;
};

export async function SkillAppWorkspace({
  moduleId,
  submoduleId,
  query
}: {
  moduleId: string;
  submoduleId: string;
  query: QueryState;
}) {
  const module = businessModules.find((entry) => entry.id === moduleId);
  const submodule = module?.submodules.find((entry) => entry.id === submoduleId);
  const activation = await readBusinessOsNavigationState();
  const enabledSkillSet = new Set(activation.enabledSkills);
  const apps = getSkillAppsForModule(moduleId, submoduleId).filter((app) => enabledSkillSet.has(app.skillId));
  const moduleApps = getSkillAppsForModule(moduleId).filter((app) => enabledSkillSet.has(app.skillId));
  const visibleApps = apps.length > 0 ? apps : moduleApps;
  const selected = resolveSelectedSkillApp(query, visibleApps, moduleApps);
  const siblingSubmodules = module?.submodules.map((entry) => ({
    ...entry,
    count: getSkillAppsForModule(moduleId, entry.id).length
  })) ?? [];

  return (
    <div className="skill-app-workspace" data-context-module={moduleId} data-context-submodule={submoduleId}>
      <section className="skill-app-index" aria-label="Skill apps">
        <header>
          <span>{module?.label ?? moduleId}</span>
          <strong>{submodule?.label ?? submoduleId}</strong>
        </header>
        <div className="skill-app-module-list">
          {siblingSubmodules.map((entry) => (
            <a
              className={entry.id === submoduleId ? "is-active" : ""}
              href={entry.href}
              key={entry.id}
            >
              <span>{entry.label}</span>
              <b>{entry.count}</b>
            </a>
          ))}
        </div>
      </section>

      <section className="skill-app-main" aria-label="Mapped skills">
        <header className="skill-app-main-head">
          <div>
            <span>{module?.summary ?? "Skill app workspace"}</span>
            <h1>{submodule?.label ?? submoduleId}</h1>
          </div>
          <strong>{visibleApps.length} skills</strong>
        </header>
        <div className="skill-app-grid">
          {visibleApps.map((app) => (
            <a
              className={selected?.skillId === app.skillId ? "skill-app-card is-selected" : "skill-app-card"}
              data-context-item
              data-context-label={app.title}
              data-context-module={app.moduleId}
              data-context-record-id={app.skillId}
              data-context-record-type="skill_app"
              data-context-submodule={app.submoduleId}
              href={withQuery(app.route, query)}
              key={app.skillId}
            >
              <span>{app.pack}</span>
              <strong>{app.title}</strong>
              <p>{app.description}</p>
              <small>{app.sourcePath}</small>
            </a>
          ))}
          {visibleApps.length === 0 ? (
            <div className="skill-app-empty">
              <strong>No packed skills mapped here yet.</strong>
              <p>This module is reserved for future pack bindings.</p>
            </div>
          ) : null}
        </div>
      </section>

      <aside className="skill-app-rail" aria-label="Selected skill">
        {selected ? <SkillAppDetail app={selected} /> : null}
      </aside>
    </div>
  );
}

export async function SkillAppPanel({
  moduleId,
  submoduleId,
  query
}: {
  moduleId: string;
  submoduleId: string;
  panelState?: WorkSurfacePanelState;
  query: QueryState;
}) {
  const apps = getSkillAppsForModule(moduleId, submoduleId);
  const activation = await readBusinessOsNavigationState();
  const enabledSkillSet = new Set(activation.enabledSkills);
  const enabledApps = apps.filter((app) => enabledSkillSet.has(app.skillId));
  const enabledModuleApps = getSkillAppsForModule(moduleId).filter((app) => enabledSkillSet.has(app.skillId));
  const selected = resolveSelectedSkillApp(query, enabledApps, enabledModuleApps);
  if (!selected) return null;

  return (
    <div className="drawer-content skill-app-drawer">
      <SkillAppDetail app={selected} />
    </div>
  );
}

function SkillAppDetail({ app }: { app: SkillAppBinding }) {
  return (
    <>
      <header className="skill-app-detail-head">
        <span>{app.pack}</span>
        <strong>{app.title}</strong>
      </header>
      <p>{app.description}</p>
      <dl className="drawer-facts">
        <div><dt>Module</dt><dd>{app.moduleId}</dd></div>
        <div><dt>Section</dt><dd>{app.submoduleId}</dd></div>
        <div><dt>Source</dt><dd>{app.sourcePath}</dd></div>
      </dl>
      <section className="skill-app-capabilities">
        {app.capabilities.map((capability) => (
          <span key={capability}>{capability}</span>
        ))}
      </section>
      <a className="skill-app-primary" href={app.route}>Open skill app</a>
    </>
  );
}

function resolveSelectedSkillApp(query: QueryState, apps: SkillAppBinding[], moduleApps: SkillAppBinding[]) {
  if (query.skillId) {
    return findSkillApp(query.skillId) ?? apps[0] ?? moduleApps[0];
  }
  if (query.recordId) {
    return findSkillApp(query.recordId) ?? apps[0] ?? moduleApps[0];
  }
  return apps[0] ?? moduleApps[0];
}

function withQuery(route: string, query: QueryState) {
  const [path, routeQuery] = route.split("?");
  const params = new URLSearchParams(routeQuery);
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  const serialized = params.toString();
  return serialized ? `${path}?${serialized}` : path;
}
