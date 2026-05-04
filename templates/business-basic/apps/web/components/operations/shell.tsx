import type { ReactNode } from "react";

type OperationsContext = {
  action?: string;
  label: string;
  module?: string;
  recordId: string;
  recordType: string;
  submoduleId: string;
};

export function OperationsPaneHead({
  actionContext,
  actionHref,
  actionLabel,
  children,
  description,
  title
}: {
  actionContext?: OperationsContext;
  actionHref?: string;
  actionLabel?: string;
  children?: ReactNode;
  description?: string;
  title: string;
}) {
  const contextProps = actionContext ? contextAttributes(actionContext) : {};

  return (
    <div className="ops-pane-head">
      <div>
        <h2>{title}</h2>
        {description ? <p>{description}</p> : null}
      </div>
      <div className="ops-pane-actions">
        {children}
        {actionHref ? <a aria-label={actionLabel ?? "Add"} href={actionHref} {...contextProps}>{actionLabel ?? "New"}</a> : null}
      </div>
    </div>
  );
}

export function OperationsSignal({
  context,
  href,
  label,
  value
}: {
  context?: OperationsContext;
  href?: string;
  label: string;
  value: string;
}) {
  const content = (
    <>
      <span>{label}</span>
      <strong>{value}</strong>
    </>
  );
  const contextProps = context ? contextAttributes(context) : {};

  return href ? <a className="ops-signal" href={href} {...contextProps}>{content}</a> : <div className="ops-signal" {...contextProps}>{content}</div>;
}

export function copyLabel(copy: Record<string, string>, key: string, fallback: string) {
  return copy[key] ?? fallback;
}

function contextAttributes(context: OperationsContext) {
  return {
    "data-context-action": context.action,
    "data-context-item": true,
    "data-context-label": context.label,
    "data-context-module": context.module ?? "operations",
    "data-context-record-id": context.recordId,
    "data-context-record-type": context.recordType,
    "data-context-submodule": context.submoduleId
  };
}
