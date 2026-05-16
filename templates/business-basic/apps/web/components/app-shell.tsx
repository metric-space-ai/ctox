import Link from "next/link";
import { WorkspaceShell, type BusinessModuleId } from "@ctox-business/ui";
import { BugReportWidget } from "./bug-report-widget";
import { ClientNavigationBridge } from "./client-navigation-bridge";
import { ContextMenuBridge } from "./context-menu-bridge";
import { readBusinessOsNavigationState } from "../lib/ctox-core-bridge";

export async function AppShell({
  children,
  currentHref,
  brandName,
  moduleId,
  submoduleId,
  locale,
  theme
}: {
  children: React.ReactNode;
  currentHref?: string;
  brandName?: string;
  moduleId?: BusinessModuleId;
  submoduleId?: string;
  locale?: string;
  theme?: string;
}) {
  const navigation = await readBusinessOsNavigationState();

  return (
    <WorkspaceShell
      currentHref={currentHref}
      brandName={brandName}
      moduleId={moduleId}
      submoduleId={submoduleId}
      locale={locale}
      theme={theme}
      moduleIds={navigation.enabledModules}
      LinkComponent={({ href, className, children }) => (
        <Link className={className} href={href}>{children}</Link>
      )}
    >
      <ClientNavigationBridge />
      <ContextMenuBridge locale={locale} />
      {children}
      <BugReportWidget locale={locale} moduleId={moduleId} submoduleId={submoduleId} />
    </WorkspaceShell>
  );
}
