import type { ReactNode } from "react";
import { ContextMenuScope } from "./context-menu";

export type WorkSurfacePanelState = {
  panel?: string;
  recordId?: string;
  drawer?: "left-bottom" | "bottom" | "right";
};

export function WorkSurface({
  moduleId,
  submoduleId,
  title,
  description,
  panelState,
  panelContent,
  hideHeader,
  children
}: {
  moduleId: string;
  submoduleId: string;
  title: string;
  description?: string;
  panelState?: WorkSurfacePanelState;
  panelContent?: ReactNode;
  hideHeader?: boolean;
  children?: ReactNode;
}) {
  const hasPanel = Boolean(panelState?.panel || panelState?.recordId);
  const drawer = panelState?.drawer ?? "right";

  return (
    <section className="work-surface" data-drawer={hasPanel ? drawer : "none"} data-header={hideHeader ? "hidden" : "visible"}>
      {hideHeader ? null : (
        <header className="work-surface-header">
          <div>
            <h1>{title}</h1>
            {description ? <p>{description}</p> : null}
          </div>
          <div className="work-surface-actions" aria-label="View actions">
            <button type="button">Filter</button>
            <button type="button">New</button>
          </div>
        </header>
      )}
      <ContextMenuScope>
        <div className="work-surface-body">
          <div className="work-view">
            {children ?? <DefaultWorkView moduleId={moduleId} submoduleId={submoduleId} />}
          </div>
          {hasPanel ? (
            <aside className="work-panel" aria-label="Context panel">
              {panelContent ?? (
                <>
                  <strong>{panelState?.panel ?? "record"}</strong>
                  {panelState?.recordId ? <p>Record: {panelState.recordId}</p> : null}
                </>
              )}
            </aside>
          ) : null}
        </div>
      </ContextMenuScope>
    </section>
  );
}

function DefaultWorkView({
  moduleId,
  submoduleId
}: {
  moduleId: string;
  submoduleId: string;
}) {
  return (
    <div className="os-lane-board" aria-label="Working view">
      <section
        data-context-item
        data-context-module={moduleId}
        data-context-submodule={submoduleId}
        data-context-record-type="work_bucket"
        data-context-record-id="now"
        data-context-label="Now"
      >
        <h2>Active</h2>
        <p>Current records, decisions, and queue-relevant work for this area.</p>
      </section>
      <section
        data-context-item
        data-context-module={moduleId}
        data-context-submodule={submoduleId}
        data-context-record-type="work_bucket"
        data-context-record-id="next"
        data-context-label="Next"
      >
        <h2>Queued</h2>
        <p>Upcoming work that CTOX can inspect, route, or turn into a task.</p>
      </section>
      <section
        data-context-item
        data-context-module={moduleId}
        data-context-submodule={submoduleId}
        data-context-record-type="work_bucket"
        data-context-record-id="done"
        data-context-label="Done"
      >
        <h2>Closed</h2>
        <p>Completed records remain available for review, audit, and knowledge sync.</p>
      </section>
    </div>
  );
}
