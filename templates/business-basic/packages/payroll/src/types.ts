export type PayrollFrequency = "monthly" | "bi-weekly" | "weekly";
export type PayrollComponentType = "earning" | "deduction";

export type PayrollFormulaKind = "fix" | "percent_of" | "formula";

export type PayrollComponent = {
  id: string;
  code: string;
  label: string;
  type: PayrollComponentType;
  taxable: boolean;
  dependsOnPaymentDays: boolean;
  accountId: string;
  formulaKind: PayrollFormulaKind;
  formulaAmount?: number;
  formulaBase?: string;
  formulaPercent?: number;
  formulaExpression?: string;
  sequence: number;
  disabled: boolean;
};

export type PayrollStructure = {
  id: string;
  companyId: string;
  label: string;
  frequency: PayrollFrequency;
  currency: string;
  isActive: boolean;
  modeOfPayment: "bank" | "cash" | "manual";
  componentIds: string[];
};

export type PayrollStructureAssignment = {
  id: string;
  employeeId: string;
  structureId: string;
  baseSalary: number;
  currency: string;
  fromDate: string;
  toDate?: string;
  createdAt: string;
  createdBy: string;
};

export type PayrollPeriod = {
  id: string;
  companyId: string;
  frequency: PayrollFrequency;
  startDate: string;
  endDate: string;
  locked: boolean;
  createdAt: string;
};

export type PayrollRunStatus = "Draft" | "Queued" | "Running" | "Submitted" | "Failed" | "Cancelled";
export type PayrollPayslipStatus = "Draft" | "Review" | "Posted" | "Withheld" | "Cancelled";

export type PayrollAdditional = {
  id: string;
  employeeId: string;
  periodId: string;
  componentId: string;
  amount: number;
  note?: string;
  appliedToPayslipId?: string;
};

export type PayrollPayslipLine = {
  id: string;
  componentId: string;
  componentCode: string;
  componentLabel: string;
  sequence: number;
  type: PayrollComponentType;
  qty: number;
  rate: number;
  amount: number;
};

export type PayrollPayslip = {
  id: string;
  runId: string;
  employeeId: string;
  employeeName: string;
  assignmentId: string;
  periodId: string;
  startDate: string;
  endDate: string;
  paymentDays: number;
  lwpDays: number;
  absentDays: number;
  currency: string;
  grossPay: number;
  totalDeduction: number;
  netPay: number;
  status: PayrollPayslipStatus;
  journalEntryId?: string;
  postedAt?: string;
  postedBy?: string;
  notes?: string;
  lines: PayrollPayslipLine[];
};

export type PayrollRun = {
  id: string;
  companyId: string;
  periodId: string;
  frequency: PayrollFrequency;
  status: PayrollRunStatus;
  selectedEmployeeIds: string[];
  payableAccountId: string;
  postingDate: string;
  error?: string;
  createdBy: string;
  createdAt: string;
  submittedAt?: string;
};

export type PayrollAuditEntry = {
  id: string;
  entityType: "payroll_run" | "payroll_payslip";
  entityId: string;
  fromStatus: string;
  toStatus: string;
  actor: string;
  at: string;
  note?: string;
};

export type PayrollEmployee = {
  id: string;
  displayName: string;
  taxId?: string;
  bankAccountIban?: string;
  contractType?: "fulltime" | "parttime" | "marginal";
};
