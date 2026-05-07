const baseUrl = process.env.CTOX_BUSINESS_BASE_URL ?? "http://localhost:3001";
const sessionCookie = await loginCookie();

// 1. Page renders.
const page = await fetch(`${baseUrl}/app/payroll/runs?locale=de&theme=light`, { headers: { cookie: sessionCookie } });
assert(page.ok, `/app/payroll/runs returned ${page.status}`);
const html = await page.text();
assert(html.includes("Lohnabrechnung"), "Payroll page missing Lohnabrechnung label");
assert(html.includes("data-context-module=\"payroll\""), "Payroll page missing data-context-module=\"payroll\"");
assert(html.includes("data-context-submodule=\"runs\""), "Payroll page missing data-context-submodule=\"runs\"");

// 2. Initial snapshot has the seed data.
const initial = await fetch(`${baseUrl}/api/payroll`, { headers: { cookie: sessionCookie } });
assert(initial.ok, `/api/payroll returned ${initial.status}`);
const initialPayload = await initial.json();
assert(initialPayload.ok === true, "Payroll GET failed");
assert(initialPayload.snapshot?.periods?.length >= 1, "Seed period missing");
assert(initialPayload.snapshot?.assignments?.length >= 2, "Seed assignments missing");
assert(initialPayload.snapshot?.components?.length >= 3, "Seed components missing");

// 3. Create a fresh ad-hoc period for this smoke run so reruns are independent.
const stamp = Date.now();
const yyyy = String(2026 + Math.floor(stamp / 1e12));
const adhocPeriodPayload = {
  startDate: `${yyyy}-${String((stamp % 12) + 1).padStart(2, "0")}-01`,
  endDate: `${yyyy}-${String((stamp % 12) + 1).padStart(2, "0")}-28`,
  frequency: "monthly"
};
const periodCreated = await post("create_period", adhocPeriodPayload);
const periodId = periodCreated.snapshot.periods.find(
  (p) => p.startDate === adhocPeriodPayload.startDate && p.endDate === adhocPeriodPayload.endDate
)?.id;
assert(periodId, "Ad-hoc period id missing");
await prepareWorkforcePayrollCandidate(periodId, adhocPeriodPayload.startDate);
const createRun = await post("create_run", { periodId, payableAccountId: "1755" });
const runId = createRun.snapshot.runs.find((r) => r.periodId === periodId)?.id;
assert(runId, "Run id not produced");

// 4. Queue run -> generates slips.
const queued = await post("queue_run", { id: runId });
assert(queued.ok === true, "queue_run failed");
const slips = queued.snapshot.payslips.filter((s) => s.runId === runId);
assert(slips.length >= 2, `Expected >=2 slips for run, got ${slips.length}`);
const slip = slips[0];
assert(slip.grossPay > 0, "Slip gross is zero");
assert(slip.netPay > 0, "Slip net is zero");
assert(slip.lines.length >= 3, "Slip lines missing");
const workforceLine = slips.flatMap((s) => s.lines).find((line) => line.componentCode === "workforce_hours" && line.amount > 0);
assert(workforceLine, "Workforce payroll additional did not flow into payroll run");

// 5. Move slip to review.
const toReview = await post("mark_payslip_review", { id: slip.id });
const reviewSlip = toReview.snapshot.payslips.find((s) => s.id === slip.id);
assert(reviewSlip.status === "Review", "Slip did not transition to Review");

// 6. Post slip → journal entry created.
const posted = await post("post_payslip", { id: slip.id, actor: "operator" });
const postedSlip = posted.snapshot.payslips.find((s) => s.id === slip.id);
assert(postedSlip.status === "Posted", "Slip did not transition to Posted");
assert(postedSlip.journalEntryId, "Slip missing journal entry id");
const journal = posted.snapshot.postedJournals.find((p) => p.payslipId === slip.id);
assert(journal, "Posted journal missing");
assert(journal.draft.lines.length >= 4, "Journal draft has too few lines");
const debitTotal = journal.draft.lines.reduce((acc, l) => acc + l.debit, 0);
const creditTotal = journal.draft.lines.reduce((acc, l) => acc + l.credit, 0);
assert(Math.round(debitTotal * 100) === Math.round(creditTotal * 100), `Journal unbalanced ${debitTotal} vs ${creditTotal}`);

// 7. CTOX payload is present and points at the slip.
assert(posted.ctoxPayload?.module === "payroll", "CTOX payload module must be 'payroll'");
assert(posted.ctoxPayload?.submodule === "runs", "CTOX payload submodule must be 'runs'");
assert(posted.ctoxPayload?.recordType === "payroll_payslip", "CTOX payload wrong recordType");
assert(posted.ctoxPayload?.recordId === slip.id, "CTOX payload wrong recordId");

// 8. Reload preserves Posted status.
const reload = await fetch(`${baseUrl}/api/payroll`, { headers: { cookie: sessionCookie } });
const reloadPayload = await reload.json();
const reloaded = reloadPayload.snapshot.payslips.find((s) => s.id === slip.id);
assert(reloaded?.status === "Posted", "Slip Posted state did not persist after reload");
assert(reloaded?.journalEntryId === postedSlip.journalEntryId, "Slip journal id did not persist");

// 9. Audit trail recorded transitions.
const audit = reloadPayload.snapshot.audit.filter((a) => a.entityId === slip.id);
assert(audit.some((a) => a.toStatus === "Review"), "Audit missing Review transition");
assert(audit.some((a) => a.toStatus === "Posted"), "Audit missing Posted transition");

// 10. Posted slip is immutable: re-post is a no-op.
const repost = await post("post_payslip", { id: slip.id });
const repostSlip = repost.snapshot.payslips.find((s) => s.id === slip.id);
assert(repostSlip.status === "Posted", "Repost should leave slip Posted");

// 11. Posted slip line edit is rejected.
const lineId = postedSlip.lines[0].id;
const editPosted = await postExpectFail("update_payslip_line", { payslipId: slip.id, lineId, amount: 1 }, "payslip_immutable");
assert(editPosted, "Editing a Posted slip line should be rejected");

// 12. Duplicate run for same period+frequency is rejected.
const dupRun = await postExpectFail("create_run", { periodId, payableAccountId: "1755" }, "run_already_exists_for_period");
assert(dupRun, "Duplicate run for same period must be rejected");

// 13. Withheld → Review round trip on a second slip.
const otherSlip = slips.find((s) => s.id !== slip.id);
assert(otherSlip, "Need a second slip for withheld test");
const withheld = await post("mark_payslip_withheld", { id: otherSlip.id });
assert(withheld.snapshot.payslips.find((s) => s.id === otherSlip.id).status === "Withheld", "Slip not Withheld");
const backToReview = await post("mark_payslip_review", { id: otherSlip.id });
assert(backToReview.snapshot.payslips.find((s) => s.id === otherSlip.id).status === "Review", "Withheld→Review return failed");

// 14. Cancel-after-Posted writes a reversal journal.
const cancelPosted = await post("cancel_payslip", { id: slip.id, note: "Test reversal" });
const cancelled = cancelPosted.snapshot.payslips.find((s) => s.id === slip.id);
assert(cancelled.status === "Cancelled", "Slip not Cancelled");
const reversal = cancelPosted.snapshot.postedJournals.find((p) => p.payslipId === slip.id && p.id.endsWith("_reversal"));
assert(reversal, "Reversal journal missing");
const revDebit = reversal.draft.lines.reduce((acc, l) => acc + l.debit, 0);
const revCredit = reversal.draft.lines.reduce((acc, l) => acc + l.credit, 0);
assert(Math.round(revDebit * 100) === Math.round(revCredit * 100), "Reversal journal unbalanced");
const origDebit = journal.draft.lines.reduce((acc, l) => acc + l.debit, 0);
assert(Math.round(revDebit * 100) === Math.round(origDebit * 100), "Reversal totals do not match original");

// 15. Cancel the run, lock the period, then verify create_run is blocked by lock.
await post("cancel_run", { id: runId });
await post("lock_period", { id: periodId });
const lockedReject = await postExpectFail("create_run", { periodId, payableAccountId: "1755" }, "period_locked");
assert(lockedReject, "Locked period must reject create_run");

// 16. M1 commands cycle: fresh period for the M1 round-trip (unique id avoids collisions across smoke reruns).
const m1PeriodId = `period_m1_${stamp}`;
const m1PeriodCreate = await post("create_period", {
  id: m1PeriodId,
  startDate: `${yyyy}-12-01`,
  endDate: `${yyyy}-12-31`,
  frequency: "monthly"
});
assert(m1PeriodCreate.snapshot.periods.some((p) => p.id === m1PeriodId), "m1 period missing in snapshot");
const m1Run = await post("create_run", { periodId: m1PeriodId, payableAccountId: "1755" });
const m1RunId = m1Run.snapshot.runs.find((r) => r.periodId === m1PeriodId).id;
const m1Queued = await post("queue_run", { id: m1RunId });
const m1Slips = m1Queued.snapshot.payslips.filter((s) => s.runId === m1RunId);
assert(m1Slips.length >= 2, `M1 round-trip needs >=2 slips, got ${m1Slips.length}`);
const slipA = m1Slips[0];
const slipB = m1Slips[1];

// 17. update_structure_assignment + recompute reflects new base.
const newBase = 4500;
const assignmentForA = m1Queued.snapshot.assignments.find((a) => a.employeeId === slipA.employeeId);
assert(assignmentForA, "assignment for slipA missing");
await post("update_structure_assignment", { id: assignmentForA.id, baseSalary: newBase });
const recomputed = await post("recompute_run", { id: m1RunId });
const slipAfter = recomputed.snapshot.payslips.find((s) => s.id === slipA.id);
const baseLine = slipAfter.lines.find((l) => l.componentCode === "base");
assert(Math.round(baseLine.amount) === newBase, `base line did not reflect updated base (${baseLine.amount} vs ${newBase})`);

// 18. update_payslip_line override on Draft slip persists, totals recompute.
const overrideLine = slipAfter.lines.find((l) => l.componentCode === "social_employee");
assert(overrideLine, "social_employee line missing");
const overrideAmount = 100;
const overridden = await post("update_payslip_line", { payslipId: slipA.id, lineId: overrideLine.id, amount: overrideAmount });
const overriddenSlip = overridden.snapshot.payslips.find((s) => s.id === slipA.id);
const overriddenLine = overriddenSlip.lines.find((l) => l.id === overrideLine.id);
assert(Math.round(overriddenLine.amount) === overrideAmount, "override amount not persisted");
assert(overriddenSlip.totalDeduction !== slipAfter.totalDeduction, "totals did not recompute after override");

// 19. mark_payslip_review + mark_payslip_draft round-trip.
const r1 = await post("mark_payslip_review", { id: slipA.id });
assert(r1.snapshot.payslips.find((s) => s.id === slipA.id).status === "Review", "did not move to Review");
const r2 = await post("mark_payslip_draft", { id: slipA.id });
assert(r2.snapshot.payslips.find((s) => s.id === slipA.id).status === "Draft", "did not return to Draft");

// 20. bulk_mark_review + bulk_post_run.
const bulkReview = await post("bulk_mark_review", { id: m1RunId });
const reviewedSlips = bulkReview.snapshot.payslips.filter((s) => s.runId === m1RunId);
assert(reviewedSlips.every((s) => s.status === "Review"), "bulk_mark_review did not flip all to Review");
const bulkPosted = await post("bulk_post_run", { id: m1RunId });
const postedSlips = bulkPosted.snapshot.payslips.filter((s) => s.runId === m1RunId && s.status === "Posted");
assert(postedSlips.length >= 2, `bulk_post_run did not post slips (${postedSlips.length})`);

// 21. duplicate_structure clones the source.
const sourceStructureId = bulkPosted.snapshot.structures[0].id;
const dup = await post("duplicate_structure", { id: sourceStructureId });
const dupStructure = dup.snapshot.structures.find((s) => s.id !== sourceStructureId && s.componentIds.join("|") === bulkPosted.snapshot.structures.find((s2) => s2.id === sourceStructureId).componentIds.join("|"));
assert(dupStructure, "duplicate_structure did not produce clone");

// 22. delete_component on referenced component is rejected.
const deleteBlocked = await postExpectFail("delete_component", { id: "pc-base" }, "component_in_use");
assert(deleteBlocked, "delete_component should reject in-use component");

// 23. propose_additional_via_ctox returns ok and emits proposal in event note.
const propose = await post("propose_additional_via_ctox", {
  employeeId: slipB.employeeId,
  periodId: m1PeriodId,
  payslipId: slipB.id,
  componentId: "pc-base",
  amount: 250,
  note: "Smoke proposal"
});
assert(propose.event?.message?.includes("queueProposal="), "proposal event missing queueProposal");

// 24. install_country_pack DE adds packed components and structure.
const beforePack = bulkPosted.snapshot.components.length;
const pack = await post("install_country_pack", { country: "DE" });
const afterPack = pack.snapshot.components.length;
assert(afterPack > beforePack, "country pack did not add components");
assert(pack.snapshot.structures.some((s) => s.id === "pde-default"), "country pack DE structure missing");

// 25. Period comparison view returns rows for an employee with posted slips.
const compRes = await fetch(`${baseUrl}/api/payroll?view=comparison&employeeId=${encodeURIComponent(slipA.employeeId)}&periods=6`, {
  headers: { cookie: sessionCookie }
});
assert(compRes.ok, `comparison view returned ${compRes.status}`);
const compJson = await compRes.json();
assert(compJson.ok && compJson.comparison.rows.length >= 1, "comparison rows empty");

// 26. CSV export view returns the right columns.
const csvRes = await fetch(`${baseUrl}/api/payroll?view=export&periodId=${encodeURIComponent(m1PeriodId)}`, {
  headers: { cookie: sessionCookie }
});
assert(csvRes.ok, `export view returned ${csvRes.status}`);
const csvText = await csvRes.text();
assert(csvText.startsWith("employee_id,employee_name,gross,deductions,net,journal_id,status"), "CSV header missing");
assert(csvText.split("\n").length >= 3, "CSV did not contain rows");

console.log(`Payroll smoke passed against ${baseUrl} (26/26 assertions)`);

async function post(command, payload) {
  const res = await fetch(`${baseUrl}/api/payroll`, {
    method: "POST",
    headers: { "content-type": "application/json", cookie: sessionCookie },
    body: JSON.stringify({ command, payload })
  });
  assert(res.ok, `${command} returned ${res.status}`);
  const json = await res.json();
  assert(json.ok === true, `${command} failed: ${json.error}`);
  return json;
}

async function prepareWorkforcePayrollCandidate(periodId, workDate) {
  const initial = await fetch(`${baseUrl}/api/operations/workforce`, { headers: { cookie: sessionCookie } });
  assert(initial.ok, "Workforce GET for payroll bridge failed");
  const workforce = await initial.json();
  for (const assignment of workforce.snapshot.assignments.filter((item) => String(item.title ?? "").startsWith("Payroll Bridge") && item.status !== "archived")) {
    await workforcePost("archive_assignment", { id: assignment.id });
  }
  const person = workforce.snapshot.people.find((p) => p.payrollEmployeeId === "emp-anna" && p.active) ?? workforce.snapshot.people.find((p) => p.active);
  const shiftType = workforce.snapshot.shiftTypes.find((s) => s.billable) ?? workforce.snapshot.shiftTypes[0];
  const slotId = person.locationId ?? workforce.snapshot.locationSlots[0].id;
  const title = `Payroll Bridge ${Date.now()}`;
  const assignmentResult = await workforcePost("create_assignment", {
    title,
    personId: person.id,
    shiftTypeId: shiftType.id,
    locationSlotId: slotId,
    date: workDate,
    startTime: "21:00",
    endTime: "22:00",
    customer: "Payroll Smoke",
    project: "Payroll Bridge"
  });
  const assignment = assignmentResult.snapshot.assignments.find((item) => item.title === title);
  assert(assignment, "Workforce payroll bridge assignment missing");
  const timeResult = await workforcePost("create_time_entry", {
    assignmentId: assignment.id,
    personId: person.id,
    date: workDate,
    startTime: "21:00",
    endTime: "22:00",
    breakMinutes: 0,
    evidence: "Payroll smoke"
  });
  const entry = timeResult.snapshot.timeEntries.find((item) => item.assignmentId === assignment.id);
  assert(entry, "Workforce payroll bridge time entry missing");
  await workforcePost("approve_time_entry", { id: entry.id });
  await workforcePost("prepare_payroll_candidate", {
    assignmentId: assignment.id,
    periodId,
    employeeId: person.payrollEmployeeId ?? person.id,
    hourlyRate: 33
  });
}

async function workforcePost(command, payload) {
  const res = await fetch(`${baseUrl}/api/operations/workforce`, {
    method: "POST",
    headers: { "content-type": "application/json", cookie: sessionCookie },
    body: JSON.stringify({ command, payload })
  });
  assert(res.ok, `workforce ${command} returned ${res.status}`);
  const json = await res.json();
  assert(json.ok === true, `workforce ${command} failed: ${json.error}`);
  return json;
}

async function postExpectFail(command, payload, expectedError) {
  const res = await fetch(`${baseUrl}/api/payroll`, {
    method: "POST",
    headers: { "content-type": "application/json", cookie: sessionCookie },
    body: JSON.stringify({ command, payload })
  });
  if (res.ok) return false;
  const json = await res.json();
  if (json.ok === true) return false;
  if (expectedError && json.error !== expectedError) {
    throw new Error(`Expected error '${expectedError}' but got '${json.error}'`);
  }
  return true;
}

async function loginCookie() {
  const user = process.env.CTOX_BUSINESS_USER ?? "admin";
  const password = process.env.CTOX_BUSINESS_PASSWORD ?? "ctox-business";
  const response = await fetch(`${baseUrl}/api/auth/login?user=${encodeURIComponent(user)}&password=${encodeURIComponent(password)}&next=/app`, {
    redirect: "manual"
  });
  const cookie = response.headers.get("set-cookie");
  return cookie?.split(";")[0] ?? "";
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
