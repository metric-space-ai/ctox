import { notFound } from "next/navigation";
import { cookies } from "next/headers";
import { findBusinessModule, findBusinessSubmodule, WorkSurface, type BusinessModuleId, type WorkSurfacePanelState } from "@ctox-business/ui";
import { AppShell } from "../../../../components/app-shell";
import { BusinessPanel, BusinessWorkspace } from "../../../../components/business-workspace";
import { CompetitiveAnalysisDashboard, CompetitiveAnalysisPanel } from "../../../../components/competitive-analysis-dashboard";
import { CtoxPanel, CtoxWorkspace } from "../../../../components/ctox-workspace";
import { businessOsName, companyNameCookieName, normalizeCompanyName } from "../../../../lib/company-settings";
import { MarketingPanel, MarketingWorkspace } from "../../../../components/marketing-workspace";
import { OperationsPanel, OperationsWorkspace } from "../../../../components/operations-workspace";
import { SalesPanel, SalesWorkspace } from "../../../../components/sales-workspace";

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
  const module = findBusinessModule(moduleId);
  const submodule = findBusinessSubmodule(moduleId, submoduleId);

  if (!module || !submodule) notFound();
  const isBusiness = module.id === "business";
  const isCompetitiveAnalysis = module.id === "marketing" && submodule.id === "competitive-analysis";
  const isCtox = module.id === "ctox";
  const isMarketing = module.id === "marketing" && !isCompetitiveAnalysis;
  const isOperations = module.id === "operations";
  const isSales = module.id === "sales";
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
        description={isCompetitiveAnalysis ? "Market monitor workspace" : isOperations ? "Operations workspace" : `${module.label} workspace`}
        panelState={panelState}
        panelContent={
          isSales
            ? <SalesPanel panelState={panelState} query={viewQuery} submoduleId={submodule.id} />
            : isCompetitiveAnalysis
              ? <CompetitiveAnalysisPanel panelState={panelState} query={viewQuery} />
              : isMarketing
                ? <MarketingPanel panelState={panelState} query={viewQuery} submoduleId={submodule.id} />
                : isOperations
                  ? <OperationsPanel panelState={panelState} query={viewQuery} submoduleId={submodule.id} />
                  : isBusiness
                    ? <BusinessPanel panelState={panelState} query={viewQuery} submoduleId={submodule.id} />
                    : isCtox
                      ? <CtoxPanel panelState={panelState} query={viewQuery} submoduleId={submodule.id} />
                      : undefined
        }
        hideHeader={isSales || isCompetitiveAnalysis || isMarketing || isOperations || isBusiness || isCtox}
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
          {isSales ? <SalesWorkspace query={viewQuery} submoduleId={submodule.id} /> : null}
          {isCompetitiveAnalysis ? <CompetitiveAnalysisDashboard query={viewQuery} /> : null}
          {isMarketing ? <MarketingWorkspace query={viewQuery} submoduleId={submodule.id} /> : null}
          {isOperations ? <OperationsWorkspace query={viewQuery} submoduleId={submodule.id} /> : null}
          {isBusiness ? <BusinessWorkspace query={viewQuery} submoduleId={submodule.id} /> : null}
          {isCtox ? <CtoxWorkspace companyName={companyName} query={viewQuery} submoduleId={submodule.id} /> : null}
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
