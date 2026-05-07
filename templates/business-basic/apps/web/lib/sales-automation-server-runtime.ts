import type { SalesAutomationRuntime } from "./sales-automation-runtime";

export async function loadSalesAutomationRuntime() {
  const modulePath = "./sales-automation-store";
  return await import(modulePath) as SalesAutomationRuntime;
}
