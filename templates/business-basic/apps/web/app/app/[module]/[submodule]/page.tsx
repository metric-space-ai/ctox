import { notFound } from "next/navigation";
import { cookies } from "next/headers";
import type { ReactNode } from "react";
import { findBusinessModule, findBusinessSubmodule, WorkSurface, type BusinessModuleId, type WorkSurfacePanelState } from "@ctox-business/ui";
import { AppShell } from "../../../../components/app-shell";
import { businessOsName, companyNameCookieName, normalizeCompanyName } from "../../../../lib/company-settings";
import { readBusinessOsNavigationState } from "../../../../lib/ctox-core-bridge";

export default async function SubmodulePage({
  params,
  searchParams
}: {
  params: Promise<{ module: string; submodule: string }>;
  searchParams: Promise<{
    drawer?: string;
    filePath?: string;
    group?: string;
    locale?: string;
    mode?: string;
    panel?: string;
    recordId?: string;
    selectedId?: string;
    runbookId?: string;
    skillbookId?: string;
    skillId?: string;
    theme?: string;
    xAxis?: string;
    yAxis?: string;
  }>;
}) {
  const { module: moduleId, submodule: submoduleId } = await params;
  const query = await searchParams;
  const cookieStore = await cookies();
  const companyName = normalizeCompanyName(cookieStore.get(companyNameCookieName)?.value);
  const brandName = businessOsName(companyName);
  const activation = await readBusinessOsNavigationState();
  const module = findBusinessModule(moduleId);
  const submodule = findBusinessSubmodule(moduleId, submoduleId);

  if (!module || !submodule || !activation.enabledModules.includes(module.id)) notFound();
  const isBusiness = module.id === "business";
  const isCompetitiveAnalysis = module.id === "marketing" && submodule.id === "competitive-analysis";
  const isCtox = module.id === "ctox";
  const isMarketing = module.id === "marketing" && !isCompetitiveAnalysis;
  const isOperations = module.id === "operations";
  const isSales = module.id === "sales";
  const isSkillAppModule = !isSales && !isMarketing && !isCompetitiveAnalysis && !isOperations && !isBusiness && !isCtox;
  const drawer: WorkSurfacePanelState["drawer"] =
    query.drawer === "left-bottom" || query.drawer === "bottom" || query.drawer === "right"
      ? query.drawer
      : undefined;
  const panelState: WorkSurfacePanelState = {
    panel: query.panel,
    recordId: query.recordId,
    drawer
  };
  const viewQuery = {
    locale: query.locale,
    mode: query.mode,
    theme: query.theme,
    xAxis: query.xAxis,
    yAxis: query.yAxis,
    panel: query.panel,
    recordId: query.recordId,
    selectedId: query.selectedId,
    drawer: query.drawer,
    filePath: query.filePath,
    group: query.group,
    skillId: query.skillId,
    skillbookId: query.skillbookId,
    runbookId: query.runbookId
  };
  let panelContent: ReactNode;
  let workspaceContent: ReactNode;

  if (isSales) {
    const { SalesPanel, SalesWorkspace } = await import("../../../../components/sales-workspace");
    panelContent = <SalesPanel panelState={panelState} query={viewQuery} submoduleId={submodule.id} />;
    workspaceContent = <SalesWorkspace query={viewQuery} submoduleId={submodule.id} />;
  } else if (isCompetitiveAnalysis) {
    const { CompetitiveAnalysisDashboard, CompetitiveAnalysisPanel } = await import("../../../../components/competitive-analysis-dashboard");
    panelContent = <CompetitiveAnalysisPanel panelState={panelState} query={viewQuery} />;
    workspaceContent = <CompetitiveAnalysisDashboard query={viewQuery} />;
  } else if (isMarketing) {
    const { MarketingPanel, MarketingWorkspace } = await import("../../../../components/marketing-workspace");
    panelContent = <MarketingPanel panelState={panelState} query={viewQuery} submoduleId={submodule.id} />;
    workspaceContent = <MarketingWorkspace query={viewQuery} submoduleId={submodule.id} />;
  } else if (isOperations) {
    const { OperationsPanel, OperationsWorkspace } = await import("../../../../components/operations-workspace");
    panelContent = <OperationsPanel panelState={panelState} query={viewQuery} submoduleId={submodule.id} />;
    workspaceContent = <OperationsWorkspace query={viewQuery} submoduleId={submodule.id} />;
  } else if (isBusiness) {
    const { BusinessPanel, BusinessWorkspace } = await import("../../../../components/business-workspace");
    panelContent = <BusinessPanel panelState={panelState} query={viewQuery} submoduleId={submodule.id} />;
    workspaceContent = <BusinessWorkspace query={viewQuery} submoduleId={submodule.id} />;
  } else if (isCtox) {
    const { CtoxPanel, CtoxWorkspace } = await import("../../../../components/ctox-workspace");
    panelContent = <CtoxPanel panelState={panelState} query={viewQuery} submoduleId={submodule.id} />;
    workspaceContent = <CtoxWorkspace companyName={companyName} query={viewQuery} submoduleId={submodule.id} />;
  } else if (isSkillAppModule) {
    const { SkillAppPanel, SkillAppWorkspace } = await import("../../../../components/skill-app-workspace");
    panelContent = <SkillAppPanel moduleId={module.id} panelState={panelState} query={viewQuery} submoduleId={submodule.id} />;
    workspaceContent = <SkillAppWorkspace moduleId={module.id} query={viewQuery} submoduleId={submodule.id} />;
  }

  return (
    <AppShell
      currentHref={currentHref(module.id, submodule.id, query)}
      brandName={brandName}
      moduleId={module.id as BusinessModuleId}
      submoduleId={submodule.id}
      locale={query.locale}
      theme={query.theme}
    >
      <WorkSurface
        moduleId={module.id}
        submoduleId={submodule.id}
        title={submodule.label}
        description={isCompetitiveAnalysis ? "Market monitor workspace" : isOperations ? "Operations workspace" : isSkillAppModule ? module.summary : `${module.label} workspace`}
        panelState={panelState}
        panelContent={panelContent}
        hideHeader={isSales || isCompetitiveAnalysis || isMarketing || isOperations || isBusiness || isCtox || isSkillAppModule}
      >
        <div
          className="context-menu-scope"
          data-context-scope
          data-context-label={`${module.label}: ${submodule.label}`}
          data-context-module={module.id}
          data-context-record-id={`${module.id}-${submodule.id}`}
          data-context-record-type="workspace"
          data-context-submodule={submodule.id}
        >
          {workspaceContent}
        </div>
      </WorkSurface>
    </AppShell>
  );
}

function currentHref(
  moduleId: string,
  submoduleId: string,
  query: {
    panel?: string;
    recordId?: string;
    selectedId?: string;
    drawer?: string;
    filePath?: string;
    group?: string;
    locale?: string;
    mode?: string;
    runbookId?: string;
    skillbookId?: string;
    skillId?: string;
    theme?: string;
    xAxis?: string;
    yAxis?: string;
  }
) {
  const params = new URLSearchParams();
  Object.entries(query).forEach(([key, value]) => {
    if (value) params.set(key, value);
  });
  const queryString = params.toString();
  const path = `/app/${moduleId}/${submoduleId}`;
  return queryString ? `${path}?${queryString}` : path;
}
