const baseUrl = process.env.CTOX_BUSINESS_BASE_URL ?? "http://localhost:3001";
const sessionCookie = await loginCookie();

const page = await fetch(`${baseUrl}/app/operations/workforce?locale=de&theme=light`, { headers: { cookie: sessionCookie } });
assert(page.ok, `/app/operations/workforce returned ${page.status}`);
const html = await page.text();
assert(html.includes("Einsatzplanung"), "Workforce page missing label");
assert(html.includes("data-context-submodule=\"workforce\""), "Workforce page missing context records");

const initial = await fetch(`${baseUrl}/api/operations/workforce`, { headers: { cookie: sessionCookie } });
assert(initial.ok, `/api/operations/workforce returned ${initial.status}`);
const initialPayload = await initial.json();
assert(initialPayload.ok === true, "Workforce GET failed");
assert(initialPayload.snapshot?.people?.length >= 3, "Seed people missing");
assert(initialPayload.snapshot?.shiftTypes?.length >= 3, "Seed shift types missing");
assert(initialPayload.snapshot?.assignments?.length >= 3, "Seed assignments missing");

for (const assignment of initialPayload.snapshot.assignments.filter((item) =>
  (String(item.title ?? "").startsWith("Smoke Einsatz") ||
    String(item.title ?? "").startsWith("Smoke Muster") ||
    String(item.title ?? "").startsWith("Payroll Bridge")) &&
  item.status !== "archived"
)) {
  await post("archive_assignment", { id: assignment.id });
}

const personId = initialPayload.snapshot.people.find((p) => p.active)?.id;
const secondPersonId = initialPayload.snapshot.people.find((p) => p.active && p.id !== personId)?.id;
const shiftTypeId = initialPayload.snapshot.shiftTypes[0].id;
const locationSlotId = initialPayload.snapshot.locationSlots[0].id;
const smokeTitle = `Smoke Einsatz ${Date.now()}`;
const runOffset = Date.now() % 20;
const workDate = isoDate(addDays(new Date(Date.UTC(2035, 6, 1)), runOffset));
const duplicateDate = isoDate(addDays(new Date(`${workDate}T00:00:00.000Z`), 1));

const created = await post("create_assignment", {
  title: smokeTitle,
  personId,
  shiftTypeId,
  locationSlotId,
  date: workDate,
  startTime: "18:00",
  endTime: "19:00",
  customer: "Smoke Kunde",
  project: "Smoke Projekt"
});
const assignment = created.snapshot.assignments.find((item) => item.title === smokeTitle);
assert(assignment, "create_assignment did not produce assignment");

const moved = await post("move_assignment", {
  id: assignment.id,
  personId: secondPersonId,
  date: workDate
});
const movedAssignment = moved.snapshot.assignments.find((item) => item.id === assignment.id);
assert(movedAssignment?.personId === secondPersonId, "move_assignment did not update person");
assert(movedAssignment?.date === workDate, "move_assignment did not update date");

const duplicated = await post("duplicate_assignment", { id: assignment.id, date: duplicateDate });
const duplicate = duplicated.snapshot.assignments.find((item) => item.id !== assignment.id && item.notes?.includes(assignment.id));
assert(duplicate, "duplicate_assignment did not create copy");

const timed = await post("create_time_entry", {
  assignmentId: assignment.id,
  personId: secondPersonId,
  date: workDate,
  startTime: "18:05",
  endTime: "18:55",
  breakMinutes: 0,
  evidence: "Smoke proof"
});
const entry = timed.snapshot.timeEntries.find((item) => item.assignmentId === assignment.id);
assert(entry?.status === "submitted", "create_time_entry did not submit entry");

const approved = await post("approve_time_entry", { id: entry.id });
const approvedEntry = approved.snapshot.timeEntries.find((item) => item.id === entry.id);
assert(approvedEntry?.status === "approved", "approve_time_entry did not approve");
const approvedAssignment = approved.snapshot.assignments.find((item) => item.id === assignment.id);
assert(approvedAssignment?.status === "approved", "Assignment did not become approved");

const handoff = await post("prepare_invoice_candidate", { assignmentId: assignment.id });
const invoice = handoff.snapshot.invoiceCandidates.find((item) => item.assignmentId === assignment.id);
assert(invoice, "prepare_invoice_candidate did not create handoff");
assert(handoff.ctoxPayload?.submodule === "workforce", "CTOX payload missing workforce submodule");

const payroll = await post("prepare_payroll_candidate", { assignmentId: assignment.id, periodId: "period_2026_07", hourlyRate: 31 });
const payrollCandidate = payroll.snapshot.payrollCandidates.find((item) => item.assignmentId === assignment.id);
assert(payrollCandidate?.amount > 0, "prepare_payroll_candidate did not create amount");

const draft = await post("create_invoice_draft", { invoiceCandidateId: invoice.id, hourlyRate: 91 });
const invoiceDraft = draft.snapshot.invoiceDrafts.find((item) => item.invoiceCandidateId === invoice.id);
assert(invoiceDraft?.amount > 0, "create_invoice_draft did not create amount");
assert(String(invoiceDraft?.deepLink ?? "").includes("/app/business/invoices"), "invoice draft deepLink missing business invoices route");

const absenceDate = isoDate(addDays(new Date(`${workDate}T00:00:00.000Z`), 3));
const absenceCreated = await post("create_absence", {
  personId: secondPersonId,
  startDate: absenceDate,
  endDate: absenceDate,
  type: "vacation",
  status: "approved",
  note: "Smoke absence"
});
const absence = absenceCreated.snapshot.absences.find((item) => item.personId === secondPersonId && item.startDate === absenceDate);
assert(absence, "create_absence did not persist absence");
const absenceReject = await postExpectFail("create_assignment", {
  title: "Absence must fail",
  personId: secondPersonId,
  shiftTypeId,
  locationSlotId,
  date: absenceDate,
  startTime: "08:00",
  endTime: "09:00"
}, "absence_conflict");
assert(absenceReject, "Assignment during absence should be rejected");

const patternStart = isoDate(addDays(new Date(`${workDate}T00:00:00.000Z`), 7));
const pattern = await post("create_recurring_shift_pattern", {
  title: `Smoke Muster ${Date.now()}`,
  personId,
  shiftTypeId,
  locationSlotId,
  weekday: weekdayNumber(patternStart),
  startDate: patternStart,
  endDate: isoDate(addDays(new Date(`${patternStart}T00:00:00.000Z`), 14)),
  customer: "Smoke Kunde",
  project: "Smoke Muster"
});
const patternRecord = pattern.snapshot.recurringPatterns.find((item) => item.title.startsWith("Smoke Muster"));
assert(patternRecord, "create_recurring_shift_pattern did not persist");
const materialized = await post("materialize_recurring_shift_pattern", { id: patternRecord.id, fromDate: patternRecord.startDate, toDate: patternRecord.endDate });
const materializedCount = materialized.snapshot.assignments.filter((item) => item.notes?.includes(patternRecord.id)).length;
assert(materializedCount >= 2, "materialize_recurring_shift_pattern did not create expected assignments");

const overlap = await fetch(`${baseUrl}/api/operations/workforce`, {
  method: "POST",
  headers: { "content-type": "application/json", cookie: sessionCookie },
  body: JSON.stringify({
    command: "create_assignment",
    payload: {
      title: "Overlap must fail",
      personId: secondPersonId,
      shiftTypeId,
      locationSlotId,
      date: workDate,
      startTime: "18:15",
      endTime: "18:45"
    }
  })
});
assert(overlap.status === 400, "Overlap create should be rejected");
const overlapPayload = await overlap.json();
assert(String(overlapPayload.error ?? "").includes("assignment_overlap"), "Overlap error missing assignment_overlap");

const reload = await fetch(`${baseUrl}/api/operations/workforce`, { headers: { cookie: sessionCookie } });
const reloadPayload = await reload.json();
const reloadedAssignment = reloadPayload.snapshot.assignments.find((item) => item.id === assignment.id);
assert(reloadedAssignment?.status === "invoice_ready", "Reload did not preserve invoice_ready status");
assert(reloadPayload.snapshot.scores.some((score) => score.assignmentId === assignment.id && score.percent >= 70), "Score missing after reload");
assert(reloadPayload.snapshot.scores.some((score) => score.assignmentId === assignment.id && score.checks.some((check) => check.id === "policy_daily_hours")), "Working-time score checks missing");

console.log(`Workforce smoke passed against ${baseUrl}`);

async function post(command, payload) {
  const res = await fetch(`${baseUrl}/api/operations/workforce`, {
    method: "POST",
    headers: { "content-type": "application/json", cookie: sessionCookie },
    body: JSON.stringify({ command, payload })
  });
  const json = await res.json();
  assert(res.ok, `${command} returned ${res.status}: ${json.error ?? "no_error"}`);
  assert(json.ok === true, `${command} failed: ${json.error}`);
  return json;
}

async function postExpectFail(command, payload, expectedError) {
  const res = await fetch(`${baseUrl}/api/operations/workforce`, {
    method: "POST",
    headers: { "content-type": "application/json", cookie: sessionCookie },
    body: JSON.stringify({ command, payload })
  });
  if (res.ok) return false;
  const json = await res.json();
  if (json.ok === true) return false;
  if (expectedError && !String(json.error ?? "").includes(expectedError)) {
    throw new Error(`Expected error containing '${expectedError}' but got '${json.error}'`);
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

function addDays(date, days) {
  const copy = new Date(date.getTime());
  copy.setUTCDate(copy.getUTCDate() + days);
  return copy;
}

function isoDate(date) {
  return date.toISOString().slice(0, 10);
}

function weekdayNumber(date) {
  const day = new Date(`${date}T00:00:00.000Z`).getUTCDay();
  return day === 0 ? 7 : day;
}
