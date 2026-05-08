/**
 * payroll-de — Country pack with simplified German 2026 components.
 *
 * Tax math here is intentionally simplified (Lohnsteuer is a flat-bracket simulation,
 * not the full ESt-Tarif). The full Lohnsteuer 2026 bracket math needs DSL extensions
 * (see rfcs/0007_payroll-de-dsl.md). This pack ships the social-insurance components
 * with realistic 2026 percentages plus a placeholder Lohnsteuer that can be replaced
 * once the DSL gains a polynomial operator.
 */

import type { PayrollComponent, PayrollStructure } from "@ctox-business/payroll";

export const PAYROLL_DE_VERSION = "2026.1";

export const payrollDeComponents: PayrollComponent[] = [
  // Earnings (DE-namespaced codes; do not collide with seed `base` / `overtime`).
  {
    id: "pde-grundgehalt",
    code: "de_grundgehalt",
    label: "Grundgehalt (DE 2026)",
    type: "earning",
    taxable: true,
    dependsOnPaymentDays: true,
    accountId: "6020",
    formulaKind: "percent_of",
    formulaBase: "base_salary",
    formulaPercent: 100,
    sequence: 10,
    disabled: false
  },
  {
    id: "pde-overtime",
    code: "de_overtime",
    label: "Mehrarbeit (DE 2026)",
    type: "earning",
    taxable: true,
    dependsOnPaymentDays: false,
    accountId: "6022",
    formulaKind: "fix",
    formulaAmount: 0,
    sequence: 12,
    disabled: false
  },

  // Employee social-insurance deductions (AN, 2026 simplified)
  {
    id: "pde-kv-an",
    code: "kv_an",
    label: "Krankenversicherung AN (7,3 % + 0,85 % ZB)",
    type: "deduction",
    taxable: false,
    dependsOnPaymentDays: false,
    accountId: "1742",
    formulaKind: "percent_of",
    formulaBase: "de_grundgehalt",
    formulaPercent: 8.15,
    sequence: 30,
    disabled: false
  },
  {
    id: "pde-rv-an",
    code: "rv_an",
    label: "Rentenversicherung AN (9,3 %)",
    type: "deduction",
    taxable: false,
    dependsOnPaymentDays: false,
    accountId: "1742",
    formulaKind: "percent_of",
    formulaBase: "de_grundgehalt",
    formulaPercent: 9.3,
    sequence: 31,
    disabled: false
  },
  {
    id: "pde-av-an",
    code: "av_an",
    label: "Arbeitslosenversicherung AN (1,3 %)",
    type: "deduction",
    taxable: false,
    dependsOnPaymentDays: false,
    accountId: "1742",
    formulaKind: "percent_of",
    formulaBase: "de_grundgehalt",
    formulaPercent: 1.3,
    sequence: 32,
    disabled: false
  },
  {
    id: "pde-pv-an",
    code: "pv_an",
    label: "Pflegeversicherung AN (2,3 %)",
    type: "deduction",
    taxable: false,
    dependsOnPaymentDays: false,
    accountId: "1742",
    formulaKind: "percent_of",
    formulaBase: "de_grundgehalt",
    formulaPercent: 2.3,
    sequence: 33,
    disabled: false
  },

  // Wage tax (simplified flat 18 %, see RFC 0007)
  {
    id: "pde-lohnsteuer",
    code: "lohnsteuer",
    label: "Lohnsteuer (vereinfacht 18 %)",
    type: "deduction",
    taxable: false,
    dependsOnPaymentDays: false,
    accountId: "1741",
    formulaKind: "percent_of",
    formulaBase: "de_grundgehalt",
    formulaPercent: 18,
    sequence: 40,
    disabled: false
  },
  {
    id: "pde-soli",
    code: "soli",
    label: "Solidaritätszuschlag (5,5 % auf Lohnsteuer)",
    type: "deduction",
    taxable: false,
    dependsOnPaymentDays: false,
    accountId: "1743",
    formulaKind: "percent_of",
    formulaBase: "lohnsteuer",
    formulaPercent: 5.5,
    sequence: 41,
    disabled: false
  },

  // Employer side (statistical, do not affect net but are tracked)
  {
    id: "pde-kv-ag",
    code: "kv_ag",
    label: "Krankenversicherung AG (7,3 %)",
    type: "earning",
    taxable: false,
    dependsOnPaymentDays: false,
    accountId: "6110",
    formulaKind: "percent_of",
    formulaBase: "de_grundgehalt",
    formulaPercent: 7.3,
    sequence: 90,
    disabled: true
  },
  {
    id: "pde-rv-ag",
    code: "rv_ag",
    label: "Rentenversicherung AG (9,3 %)",
    type: "earning",
    taxable: false,
    dependsOnPaymentDays: false,
    accountId: "6111",
    formulaKind: "percent_of",
    formulaBase: "de_grundgehalt",
    formulaPercent: 9.3,
    sequence: 91,
    disabled: true
  }
];

export const payrollDeStructures: PayrollStructure[] = [
  {
    id: "pde-default",
    companyId: "ctox-business",
    label: "DE Standard Vollzeit (2026)",
    frequency: "monthly",
    currency: "EUR",
    isActive: true,
    modeOfPayment: "bank",
    componentIds: [
      "pde-grundgehalt",
      "pde-overtime",
      "pde-kv-an",
      "pde-rv-an",
      "pde-av-an",
      "pde-pv-an",
      "pde-lohnsteuer",
      "pde-soli"
    ]
  }
];

type InstallableSnapshot = {
  components: PayrollComponent[];
  structures: PayrollStructure[];
  events?: Array<{ id: string; at: string; command: string; entityType: string; entityId: string; message: string }>;
};

export type InstallResult = {
  componentsAdded: number;
  structuresAdded: number;
  version: string;
};

export function installIntoSnapshot(
  snapshot: InstallableSnapshot,
  options: { actor: string; at: string }
): InstallResult {
  let componentsAdded = 0;
  for (const component of payrollDeComponents) {
    if (!snapshot.components.some((c) => c.id === component.id)) {
      snapshot.components = [...snapshot.components, component];
      componentsAdded += 1;
    }
  }
  let structuresAdded = 0;
  for (const structure of payrollDeStructures) {
    if (!snapshot.structures.some((s) => s.id === structure.id)) {
      snapshot.structures = [...snapshot.structures, structure];
      structuresAdded += 1;
    }
  }
  if (snapshot.events) {
    snapshot.events.unshift({
      id: `pde_${options.at}`,
      at: options.at,
      command: "install_country_pack",
      entityType: "payroll_component",
      entityId: "payroll-de",
      message: `Country pack DE ${PAYROLL_DE_VERSION}: +${componentsAdded} Komponenten, +${structuresAdded} Strukturen`
    });
  }
  return { componentsAdded, structuresAdded, version: PAYROLL_DE_VERSION };
}
