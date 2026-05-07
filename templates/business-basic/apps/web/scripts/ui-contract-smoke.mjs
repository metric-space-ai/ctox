import { readFile } from "node:fs/promises";

const css = await readFile(new URL("../app/globals.css", import.meta.url), "utf8");
const submodulePage = await readFile(new URL("../app/app/[module]/[submodule]/page.tsx", import.meta.url), "utf8");
const appShell = await readFile(new URL("../components/app-shell.tsx", import.meta.url), "utf8");
const clientNavigationBridge = await readFile(new URL("../components/client-navigation-bridge.tsx", import.meta.url), "utf8");
const contextMenu = await readFile(new URL("../../../packages/ui/src/components/context-menu.tsx", import.meta.url), "utf8");
const workspaceShell = await readFile(new URL("../../../packages/ui/src/components/workspace-shell.tsx", import.meta.url), "utf8");
const operationsShell = await readFile(new URL("../components/operations/shell.tsx", import.meta.url), "utf8");
const operationsProjects = await readFile(new URL("../components/operations/projects-view.tsx", import.meta.url), "utf8");
const operationsWorkItems = await readFile(new URL("../components/operations/work-items-view.tsx", import.meta.url), "utf8");
const operationsBoards = await readFile(new URL("../components/operations/boards-view.tsx", import.meta.url), "utf8");
const operationsPortedTools = await readFile(new URL("../components/operations/ported-tools.tsx", import.meta.url), "utf8");
const operationsPlanning = await readFile(new URL("../components/operations/planning-view.tsx", import.meta.url), "utf8");
const operationsKnowledge = await readFile(new URL("../components/operations/knowledge-view.tsx", import.meta.url), "utf8");
const operationsMeetings = await readFile(new URL("../components/operations/meetings-view.tsx", import.meta.url), "utf8");
const salesWorkspace = await readFile(new URL("../components/sales-workspace.tsx", import.meta.url), "utf8");
const businessWorkspace = await readFile(new URL("../components/business-workspace.tsx", import.meta.url), "utf8");
const competitiveDashboard = await readFile(new URL("../components/competitive-analysis-dashboard.tsx", import.meta.url), "utf8");
const portfolioMap = await readFile(new URL("../components/portfolio-map.tsx", import.meta.url), "utf8");

assert(countSelector(css, ".work-panel") === 1, "Expected one canonical .work-panel block");
assert(countSelector(css, '[data-drawer="right"] .work-panel') === 1, "Expected one right drawer block");
assert(countSelector(css, '[data-drawer="left-bottom"] .work-panel') === 1, "Expected one left-bottom drawer block");
assert(countSelector(css, '[data-drawer="bottom"] .work-panel') === 1, "Expected one bottom drawer block");
assert(css.includes("@keyframes drawer-in-right"), "Missing right drawer animation");
assert(css.includes("@keyframes drawer-in-left-bottom"), "Missing left-bottom drawer animation");
assert(css.includes("@keyframes drawer-in-bottom"), "Missing bottom drawer animation");

assert(contextMenu.includes("function openDetails"), "Context menu must implement Open details");
assert(contextMenu.includes("navigateInApp(item.href)") && contextMenu.includes("ctox:navigate"), "Context menu detail navigation must use the client navigation bridge");
assert(contextMenu.includes("function presetInstruction"), "Context menu must prefill CTOX action prompts");
assert(contextMenu.includes("function newRecordId"), "Context menu must resolve module-specific new-record targets");
assert(contextMenu.includes("promptCtox(prompt.items, instruction)"), "Context menu must queue CTOX prompts");
assert(appShell.includes("<ClientNavigationBridge />"), "App shell must install the client navigation bridge");
assert(clientNavigationBridge.includes("router.push(target, { scroll: false })"), "Client navigation must use soft app-router navigation without scroll reset");
assert(clientNavigationBridge.includes("window.addEventListener(\"click\", onClick, true)"), "Client navigation must intercept internal app anchor clicks");
assert(submodulePage.includes("currentHref={currentHref(module.id, submodule.id, query)}"), "Submodule page must pass full current href to shell");
assert(workspaceShell.includes("currentHref ?? activeSubmodule?.href"), "Workspace shell must preserve panel context for locale/theme switching");
assert(operationsShell.includes("type OperationsContext"), "Operations shell must define reusable context metadata");
assert(operationsShell.includes("contextAttributes"), "Operations shell must apply context metadata to shared signals/actions");
assert(operationsProjects.includes("recordType: \"project_set\""), "Operations project signals must expose project-set context");
assert(operationsProjects.includes("data-context-record-id=\"project\"") && operationsProjects.includes("data-context-record-id=\"work-item\""), "Operations project create controls must expose context");
assert(operationsWorkItems.includes("data-context-item") && operationsWorkItems.includes("open-work-summary"), "Operations work filters must be right-click context items");
assert(operationsWorkItems.includes("data-context-record-id=\"work-item\""), "Operations work create control must expose context");
assert(operationsBoards.includes("OperationsKanbanTool") && operationsPortedTools.includes("draggable") && operationsPortedTools.includes("onDrop"), "Operations boards must use the transplanted drag/drop kanban tool");
assert(operationsPortedTools.includes("OperationsProjectTreeTool") && operationsPortedTools.includes("role=\"tree\""), "Operations projects must expose the transplanted collapsible project tree");
assert(operationsPortedTools.includes("OperationsAgendaTool") && operationsPortedTools.includes("Move up") && operationsPortedTools.includes("New agenda item"), "Operations meetings must expose the transplanted agenda reorder/add tool");
assert(operationsPlanning.includes("actionContext") && operationsPlanning.includes("milestones-at-risk"), "Operations planning must expose contextual create and risk-set controls");
assert(operationsKnowledge.includes("actionContext") && operationsKnowledge.includes("recordType: \"knowledge_set\""), "Operations knowledge signals must expose set context");
assert(operationsMeetings.includes("actionContext") && operationsMeetings.includes("recordType: \"meeting_set\""), "Operations meetings signals must expose set context");
assert(salesWorkspace.includes("contextFromHref") && salesWorkspace.includes("\"data-context-item\": true"), "Sales KPI signals and create actions must expose right-click context");
assert(salesWorkspace.includes("query.panel === panel && query.recordId === recordId"), "Sales drawer links must toggle closed on repeated item clicks");
assert(businessWorkspace.includes("businessContextFromHref") && businessWorkspace.includes("data-context-item"), "Business KPI signals and create actions must expose right-click context");
assert(businessWorkspace.includes("WarehouseView") && businessWorkspace.includes("warehouse-replay"), "Business warehouse workspace must expose M0/M1 replay context");
assert(competitiveDashboard.includes("data-context-record-type=\"criteria\"") && competitiveDashboard.includes("data-context-record-id=\"new-note\""), "Competitive analysis toolbar actions must expose right-click context");
assert(portfolioMap.includes("data-context-record-type=\"watchlist\""), "Portfolio map create action must expose right-click context");

console.log("UI contract smoke passed");

function countSelector(source, selector) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return [...source.matchAll(new RegExp(`(^|\\n)${escaped}\\s*\\{`, "g"))].length;
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
