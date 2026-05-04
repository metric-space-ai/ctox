import Link from "next/link";
import { WorkspaceShell, type BusinessModuleId } from "@ctox-business/ui";
import { BugReportWidget } from "./bug-report-widget";
import { ClientNavigationBridge } from "./client-navigation-bridge";
import { ContextMenuBridge } from "./context-menu-bridge";

export function AppShell({
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
  return (
    <WorkspaceShell
      currentHref={currentHref}
      brandName={brandName}
      moduleId={moduleId}
      submoduleId={submoduleId}
      locale={locale}
      theme={theme}
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
