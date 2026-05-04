import type { SupportedLocale } from "../../lib/operations-seed";
import type { OperationsBundle } from "../../lib/operations-store";

export type OperationsQueryState = {
  locale?: string;
  theme?: string;
  panel?: string;
  recordId?: string;
  selectedId?: string;
  drawer?: string;
  group?: string;
  filePath?: string;
  skillId?: string;
  skillbookId?: string;
  runbookId?: string;
};

export type OperationsCopy = Record<string, string>;

export type OperationsSubmoduleViewProps = {
  copy: OperationsCopy;
  data: OperationsBundle;
  locale: SupportedLocale;
  query: OperationsQueryState;
  submoduleId: string;
};

export function operationsPanelHref(
  query: OperationsQueryState,
  submoduleId: string,
  panel: string,
  recordId: string,
  drawer: "left-bottom" | "bottom" | "right"
) {
  if (query.panel === panel && query.recordId === recordId) {
    return operationsBaseHref(query, submoduleId);
  }

  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  params.set("panel", panel);
  params.set("recordId", recordId);
  params.set("drawer", drawer);
  return `/app/operations/${submoduleId}?${params.toString()}`;
}

export function operationsSelectionHref(
  query: OperationsQueryState,
  submoduleId: string,
  selectedId: string
) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  params.set("selectedId", selectedId);
  return `/app/operations/${submoduleId}?${params.toString()}`;
}

export function operationsBaseHref(query: OperationsQueryState, submoduleId: string) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  const queryString = params.toString();
  return queryString ? `/app/operations/${submoduleId}?${queryString}` : `/app/operations/${submoduleId}`;
}
