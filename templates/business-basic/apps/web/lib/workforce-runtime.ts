import { randomUUID } from "node:crypto";
import { businessDeepLink } from "@ctox-business/ui";
import { loadRuntimeJsonStore, saveRuntimeJsonStore } from "./runtime-json-store";

export type WorkforcePerson = {
  id: string;
  number: string;
  name: string;
  role: string;
  team: string;
  active: boolean;
  locationId: string;
  payrollEmployeeId?: string;
  skills: string[];
  weeklyHours: number;
};

export type WorkforceShiftType = {
  id: string;
  name: string;
  startTime: string;
  endTime: string;
  role: string;
  color: string;
  billable: boolean;
};

export type WorkforceLocationSlot = {
  id: string;
  name: string;
  zone: string;
  capacity: number;
};

export type WorkforceAssignmentStatus =
  | "draft"
  | "planned"
  | "in_progress"
  | "needs_time"
  | "needs_review"
  | "approved"
  | "blocked"
  | "invoice_ready"
  | "archived";

export type WorkforceAssignment = {
  id: string;
  title: string;
  personId: string;
  shiftTypeId: string;
  locationSlotId: string;
  date: string;
  startTime: string;
  endTime: string;
  customer?: string;
  project?: string;
  status: WorkforceAssignmentStatus;
  notes?: string;
  blocker?: string;
  createdAt: string;
  updatedAt: string;
};

export type WorkforceTimeEntryStatus = "draft" | "submitted" | "approved" | "correction_requested";

export type WorkforceTimeEntry = {
  id: string;
  assignmentId: string;
  personId: string;
  date: string;
  startTime: string;
  endTime: string;
  breakMinutes: number;
  status: WorkforceTimeEntryStatus;
  evidence?: string;
  note?: string;
  approvedAt?: string;
  approvedBy?: string;
};

export type WorkforceAbsenceStatus = "requested" | "approved" | "cancelled";

export type WorkforceAbsence = {
  id: string;
  personId: string;
  startDate: string;
  endDate: string;
  type: "vacation" | "sick" | "training" | "unavailable";
  status: WorkforceAbsenceStatus;
  note?: string;
  createdAt: string;
  updatedAt: string;
};

export type WorkforceRecurringShiftPattern = {
  id: string;
  title: string;
  personId: string;
  shiftTypeId: string;
  locationSlotId: string;
  weekday: number;
  startDate: string;
  endDate?: string;
  customer?: string;
  project?: string;
  active: boolean;
  createdAt: string;
  updatedAt: string;
};

export type WorkforceScoreCheck = {
  id: string;
  bucket: "basis" | "leistung" | "bonus";
  label: string;
  ok: boolean;
  severity: "ok" | "warning" | "blocker" | "info";
  detail: string;
};

export type WorkforceScore = {
  assignmentId: string;
  percent: number;
  basis: number;
  leistung: number;
  bonus: number;
  checks: WorkforceScoreCheck[];
};

export type WorkforceEvent = {
  id: string;
  at: string;
  command: WorkforceCommand;
  entityType: "person" | "shift_type" | "location_slot" | "assignment" | "time_entry" | "absence" | "recurring_pattern" | "handoff";
  entityId: string;
  message: string;
};

export type WorkforceCtoxPayload = {
  module: "operations";
  submodule: "workforce";
  recordId: string;
  recordType: "workforce_person" | "workforce_shift_type" | "workforce_assignment" | "workforce_time_entry" | "workforce_absence" | "workforce_recurring_pattern" | "workforce_handoff";
  selectedFields: Record<string, unknown>;
  allowedActions: string[];
};

export type WorkforceInvoiceCandidate = {
  id: string;
  assignmentId: string;
  timeEntryId: string;
  customer: string;
  project: string;
  hours: number;
  preparedAt: string;
  status: "prepared";
};

export type WorkforcePayrollCandidate = {
  id: string;
  assignmentId: string;
  timeEntryId: string;
  employeeId: string;
  periodId: string;
  componentId: string;
  hours: number;
  hourlyRate: number;
  amount: number;
  preparedAt: string;
  status: "prepared" | "applied";
};

export type WorkforceInvoiceDraft = {
  id: string;
  invoiceCandidateId: string;
  assignmentId: string;
  customer: string;
  project: string;
  hours: number;
  hourlyRate: number;
  amount: number;
  currency: string;
  status: "draft";
  businessRecordId: string;
  deepLink: string;
  createdAt: string;
};

export type WorkforceSnapshot = {
  source: "database" | "file" | "seed";
  companyId: string;
  weekStart: string;
  people: WorkforcePerson[];
  shiftTypes: WorkforceShiftType[];
  locationSlots: WorkforceLocationSlot[];
  absences: WorkforceAbsence[];
  recurringPatterns: WorkforceRecurringShiftPattern[];
  assignments: WorkforceAssignment[];
  timeEntries: WorkforceTimeEntry[];
  invoiceCandidates: WorkforceInvoiceCandidate[];
  payrollCandidates: WorkforcePayrollCandidate[];
  invoiceDrafts: WorkforceInvoiceDraft[];
  events: WorkforceEvent[];
  ctoxPayloads: WorkforceCtoxPayload[];
  scores: WorkforceScore[];
};

export type WorkforceCommand =
  | "create_person"
  | "update_person"
  | "toggle_person_active"
  | "create_shift_type"
  | "rename_shift_type"
  | "create_location_slot"
  | "rename_location_slot"
  | "create_assignment"
  | "update_assignment"
  | "move_assignment"
  | "duplicate_assignment"
  | "archive_assignment"
  | "resolve_blocker"
  | "create_time_entry"
  | "update_time_entry"
  | "approve_time_entry"
  | "request_correction"
  | "create_absence"
  | "approve_absence"
  | "cancel_absence"
  | "create_recurring_shift_pattern"
  | "materialize_recurring_shift_pattern"
  | "prepare_payroll_candidate"
  | "prepare_invoice_candidate"
  | "create_invoice_draft";

export type WorkforceMutationRequest = {
  command: WorkforceCommand;
  actor?: string;
  idempotencyKey?: string;
  payload?: Record<string, unknown>;
};

export type WorkforceMutationResult = {
  ok: true;
  command: WorkforceCommand;
  event: WorkforceEvent;
  ctoxPayload?: WorkforceCtoxPayload;
  snapshot: WorkforceSnapshot;
};

export async function getWorkforceSnapshot(): Promise<WorkforceSnapshot> {
  const seed = buildWorkforceSeed();

  const database = await loadRuntimeJsonStore<Partial<WorkforceSnapshot>>("workforce");
  if (database) {
    return normalizeSnapshot({ ...seed, ...database, source: "database" });
  }

  await persistWorkforceSnapshot(seed);
  return seed;
}

export async function executeWorkforceCommand(request: WorkforceMutationRequest): Promise<WorkforceMutationResult> {
  const snapshot = await getWorkforceSnapshot();
  const payload = request.payload ?? {};
  const actor = stringValue(payload.actor) ?? request.actor ?? "operator";
  const at = nowIso();
  let event: WorkforceEvent;
  let ctoxPayload: WorkforceCtoxPayload | undefined;

  switch (request.command) {
    case "create_person": {
      const person: WorkforcePerson = {
        id: stringValue(payload.id) ?? `wp_${shortId()}`,
        number: stringValue(payload.number) ?? `MA-${snapshot.people.length + 1}`.padStart(5, "0"),
        name: requireString(payload.name, "name"),
        role: stringValue(payload.role) ?? "Service",
        team: stringValue(payload.team) ?? "Team A",
        active: booleanValue(payload.active, true),
        locationId: stringValue(payload.locationId) ?? snapshot.locationSlots[0]?.id ?? "slot_floor",
        payrollEmployeeId: stringValue(payload.payrollEmployeeId),
        skills: stringArray(payload.skills),
        weeklyHours: numberValue(payload.weeklyHours) ?? 40
      };
      snapshot.people = upsert(snapshot.people, person, (p) => p.id);
      event = makeEvent(request.command, "person", person.id, `Mitarbeiter angelegt: ${person.name}`);
      ctoxPayload = payloadFor("workforce_person", person.id, { person }, ["update_person", "toggle_person_active"]);
      break;
    }
    case "update_person": {
      const person = requireEntity(snapshot.people, requireString(payload.id, "id"), "person_not_found");
      person.name = stringValue(payload.name) ?? person.name;
      person.role = stringValue(payload.role) ?? person.role;
      person.team = stringValue(payload.team) ?? person.team;
      person.locationId = stringValue(payload.locationId) ?? person.locationId;
      person.payrollEmployeeId = payload.payrollEmployeeId === null ? undefined : stringValue(payload.payrollEmployeeId) ?? person.payrollEmployeeId;
      person.weeklyHours = numberValue(payload.weeklyHours) ?? person.weeklyHours;
      if (Array.isArray(payload.skills)) person.skills = stringArray(payload.skills);
      event = makeEvent(request.command, "person", person.id, `Mitarbeiter gespeichert: ${person.name}`);
      ctoxPayload = payloadFor("workforce_person", person.id, { person }, ["update_person", "toggle_person_active"]);
      break;
    }
    case "toggle_person_active": {
      const person = requireEntity(snapshot.people, requireString(payload.id, "id"), "person_not_found");
      person.active = booleanValue(payload.active, !person.active);
      event = makeEvent(request.command, "person", person.id, `${person.name}: ${person.active ? "aktiv" : "inaktiv"}`);
      ctoxPayload = payloadFor("workforce_person", person.id, { person }, ["toggle_person_active"]);
      break;
    }
    case "create_shift_type": {
      const shiftType: WorkforceShiftType = {
        id: stringValue(payload.id) ?? `st_${shortId()}`,
        name: requireString(payload.name, "name"),
        startTime: stringValue(payload.startTime) ?? "08:00",
        endTime: stringValue(payload.endTime) ?? "16:00",
        role: stringValue(payload.role) ?? "Service",
        color: stringValue(payload.color) ?? "#7dd3fc",
        billable: booleanValue(payload.billable, true)
      };
      assertTimeOrder(shiftType.startTime, shiftType.endTime);
      snapshot.shiftTypes = upsert(snapshot.shiftTypes, shiftType, (s) => s.id);
      event = makeEvent(request.command, "shift_type", shiftType.id, `Schichttyp angelegt: ${shiftType.name}`);
      ctoxPayload = payloadFor("workforce_shift_type", shiftType.id, { shiftType }, ["rename_shift_type", "create_assignment"]);
      break;
    }
    case "rename_shift_type": {
      const shiftType = requireEntity(snapshot.shiftTypes, requireString(payload.id, "id"), "shift_type_not_found");
      shiftType.name = requireString(payload.name, "name");
      shiftType.startTime = stringValue(payload.startTime) ?? shiftType.startTime;
      shiftType.endTime = stringValue(payload.endTime) ?? shiftType.endTime;
      shiftType.role = stringValue(payload.role) ?? shiftType.role;
      shiftType.billable = booleanValue(payload.billable, shiftType.billable);
      assertTimeOrder(shiftType.startTime, shiftType.endTime);
      event = makeEvent(request.command, "shift_type", shiftType.id, `Schichttyp umbenannt: ${shiftType.name}`);
      ctoxPayload = payloadFor("workforce_shift_type", shiftType.id, { shiftType }, ["rename_shift_type"]);
      break;
    }
    case "create_location_slot": {
      const slot: WorkforceLocationSlot = {
        id: stringValue(payload.id) ?? `slot_${shortId()}`,
        name: requireString(payload.name, "name"),
        zone: stringValue(payload.zone) ?? "Betrieb",
        capacity: numberValue(payload.capacity) ?? 1
      };
      snapshot.locationSlots = upsert(snapshot.locationSlots, slot, (s) => s.id);
      event = makeEvent(request.command, "location_slot", slot.id, `Arbeitsplatz angelegt: ${slot.name}`);
      break;
    }
    case "rename_location_slot": {
      const slot = requireEntity(snapshot.locationSlots, requireString(payload.id, "id"), "location_slot_not_found");
      slot.name = requireString(payload.name, "name");
      slot.zone = stringValue(payload.zone) ?? slot.zone;
      slot.capacity = numberValue(payload.capacity) ?? slot.capacity;
      event = makeEvent(request.command, "location_slot", slot.id, `Arbeitsplatz gespeichert: ${slot.name}`);
      break;
    }
    case "create_assignment": {
      const shiftType = requireEntity(snapshot.shiftTypes, requireString(payload.shiftTypeId, "shiftTypeId"), "shift_type_not_found");
      const assignment: WorkforceAssignment = {
        id: stringValue(payload.id) ?? `wa_${shortId()}`,
        title: stringValue(payload.title) ?? shiftType.name,
        personId: requireString(payload.personId, "personId"),
        shiftTypeId: shiftType.id,
        locationSlotId: stringValue(payload.locationSlotId) ?? snapshot.locationSlots[0]?.id ?? "slot_floor",
        date: requireString(payload.date, "date"),
        startTime: stringValue(payload.startTime) ?? shiftType.startTime,
        endTime: stringValue(payload.endTime) ?? shiftType.endTime,
        customer: stringValue(payload.customer),
        project: stringValue(payload.project),
        status: "planned",
        notes: stringValue(payload.notes),
        createdAt: at,
        updatedAt: at
      };
      assertAssignment(snapshot, assignment);
      snapshot.assignments = upsert(snapshot.assignments, assignment, (a) => a.id);
      event = makeEvent(request.command, "assignment", assignment.id, `Einsatz angelegt: ${assignment.title}`);
      ctoxPayload = payloadFor("workforce_assignment", assignment.id, { assignment }, assignmentActions(assignment));
      break;
    }
    case "update_assignment":
    case "move_assignment": {
      const assignment = requireEntity(snapshot.assignments, requireString(payload.id, "id"), "assignment_not_found");
      const candidate: WorkforceAssignment = {
        ...assignment,
        title: stringValue(payload.title) ?? assignment.title,
        personId: stringValue(payload.personId) ?? assignment.personId,
        shiftTypeId: stringValue(payload.shiftTypeId) ?? assignment.shiftTypeId,
        locationSlotId: stringValue(payload.locationSlotId) ?? assignment.locationSlotId,
        date: stringValue(payload.date) ?? assignment.date,
        startTime: stringValue(payload.startTime) ?? assignment.startTime,
        endTime: stringValue(payload.endTime) ?? assignment.endTime,
        customer: payload.customer === null ? undefined : stringValue(payload.customer) ?? assignment.customer,
        project: payload.project === null ? undefined : stringValue(payload.project) ?? assignment.project,
        notes: payload.notes === null ? undefined : stringValue(payload.notes) ?? assignment.notes,
        status: assignmentStatus(payload.status) ?? assignment.status,
        updatedAt: at
      };
      assertAssignment(snapshot, candidate, assignment.id);
      Object.assign(assignment, candidate);
      event = makeEvent(request.command, "assignment", assignment.id, request.command === "move_assignment" ? `Einsatz verschoben: ${assignment.date} ${assignment.startTime}` : `Einsatz gespeichert: ${assignment.title}`);
      ctoxPayload = payloadFor("workforce_assignment", assignment.id, { assignment }, assignmentActions(assignment));
      break;
    }
    case "duplicate_assignment": {
      const source = requireEntity(snapshot.assignments, requireString(payload.id, "id"), "assignment_not_found");
      const next: WorkforceAssignment = {
        ...source,
        id: stringValue(payload.newId) ?? `wa_${shortId()}`,
        date: stringValue(payload.date) ?? source.date,
        personId: stringValue(payload.personId) ?? source.personId,
        startTime: stringValue(payload.startTime) ?? source.startTime,
        endTime: stringValue(payload.endTime) ?? source.endTime,
        status: "planned",
        notes: source.notes ? `${source.notes}\nDuplikat aus ${source.id}` : `Duplikat aus ${source.id}`,
        blocker: undefined,
        createdAt: at,
        updatedAt: at
      };
      assertAssignment(snapshot, next);
      snapshot.assignments = upsert(snapshot.assignments, next, (a) => a.id);
      event = makeEvent(request.command, "assignment", next.id, `Einsatz dupliziert: ${next.title}`);
      ctoxPayload = payloadFor("workforce_assignment", next.id, { assignment: next }, assignmentActions(next));
      break;
    }
    case "archive_assignment": {
      const assignment = requireEntity(snapshot.assignments, requireString(payload.id, "id"), "assignment_not_found");
      assignment.status = "archived";
      assignment.updatedAt = at;
      event = makeEvent(request.command, "assignment", assignment.id, `Einsatz archiviert: ${assignment.title}`);
      ctoxPayload = payloadFor("workforce_assignment", assignment.id, { assignment }, ["duplicate_assignment"]);
      break;
    }
    case "resolve_blocker": {
      const assignment = requireEntity(snapshot.assignments, requireString(payload.id, "id"), "assignment_not_found");
      assignment.blocker = undefined;
      assignment.status = assignment.status === "blocked" ? "planned" : assignment.status;
      assignment.updatedAt = at;
      event = makeEvent(request.command, "assignment", assignment.id, `Blocker geloest: ${assignment.title}`);
      ctoxPayload = payloadFor("workforce_assignment", assignment.id, { assignment }, assignmentActions(assignment));
      break;
    }
    case "create_time_entry": {
      const assignment = requireEntity(snapshot.assignments, requireString(payload.assignmentId, "assignmentId"), "assignment_not_found");
      const entry: WorkforceTimeEntry = {
        id: stringValue(payload.id) ?? `wte_${shortId()}`,
        assignmentId: assignment.id,
        personId: stringValue(payload.personId) ?? assignment.personId,
        date: stringValue(payload.date) ?? assignment.date,
        startTime: stringValue(payload.startTime) ?? assignment.startTime,
        endTime: stringValue(payload.endTime) ?? assignment.endTime,
        breakMinutes: numberValue(payload.breakMinutes) ?? 0,
        status: "submitted",
        evidence: stringValue(payload.evidence),
        note: stringValue(payload.note)
      };
      assertTimeEntry(snapshot, entry);
      snapshot.timeEntries = upsert(snapshot.timeEntries, entry, (t) => t.id);
      assignment.status = "needs_review";
      assignment.updatedAt = at;
      event = makeEvent(request.command, "time_entry", entry.id, `Zeitnachweis eingereicht: ${assignment.title}`);
      ctoxPayload = payloadFor("workforce_time_entry", entry.id, { entry, assignment }, ["approve_time_entry", "request_correction", "update_time_entry"]);
      break;
    }
    case "update_time_entry": {
      const entry = requireEntity(snapshot.timeEntries, requireString(payload.id, "id"), "time_entry_not_found");
      const candidate: WorkforceTimeEntry = {
        ...entry,
        date: stringValue(payload.date) ?? entry.date,
        startTime: stringValue(payload.startTime) ?? entry.startTime,
        endTime: stringValue(payload.endTime) ?? entry.endTime,
        breakMinutes: numberValue(payload.breakMinutes) ?? entry.breakMinutes,
        evidence: payload.evidence === null ? undefined : stringValue(payload.evidence) ?? entry.evidence,
        note: payload.note === null ? undefined : stringValue(payload.note) ?? entry.note,
        status: timeEntryStatus(payload.status) ?? entry.status
      };
      assertTimeEntry(snapshot, candidate, entry.id);
      Object.assign(entry, candidate);
      event = makeEvent(request.command, "time_entry", entry.id, `Zeitnachweis gespeichert`);
      ctoxPayload = payloadFor("workforce_time_entry", entry.id, { entry }, ["approve_time_entry", "request_correction", "update_time_entry"]);
      break;
    }
    case "approve_time_entry": {
      const entry = requireEntity(snapshot.timeEntries, requireString(payload.id, "id"), "time_entry_not_found");
      entry.status = "approved";
      entry.approvedAt = at;
      entry.approvedBy = actor;
      const assignment = snapshot.assignments.find((item) => item.id === entry.assignmentId);
      if (assignment) {
        assignment.status = "approved";
        assignment.updatedAt = at;
      }
      event = makeEvent(request.command, "time_entry", entry.id, `Zeitnachweis freigegeben`);
      ctoxPayload = payloadFor("workforce_time_entry", entry.id, { entry, assignment }, ["prepare_payroll_candidate", "prepare_invoice_candidate"]);
      break;
    }
    case "request_correction": {
      const entry = requireEntity(snapshot.timeEntries, requireString(payload.id, "id"), "time_entry_not_found");
      entry.status = "correction_requested";
      entry.note = stringValue(payload.note) ?? entry.note ?? "Korrektur angefordert";
      const assignment = snapshot.assignments.find((item) => item.id === entry.assignmentId);
      if (assignment) {
        assignment.status = "blocked";
        assignment.blocker = entry.note;
        assignment.updatedAt = at;
      }
      event = makeEvent(request.command, "time_entry", entry.id, `Korrektur angefordert`);
      ctoxPayload = payloadFor("workforce_time_entry", entry.id, { entry, assignment }, ["update_time_entry"]);
      break;
    }
    case "create_absence": {
      const startDate = requireString(payload.startDate, "startDate");
      const absence: WorkforceAbsence = {
        id: stringValue(payload.id) ?? `wabs_${shortId()}`,
        personId: requireString(payload.personId, "personId"),
        startDate,
        endDate: stringValue(payload.endDate) ?? startDate,
        type: absenceType(payload.type),
        status: absenceStatus(payload.status) ?? "requested",
        note: stringValue(payload.note),
        createdAt: at,
        updatedAt: at
      };
      requireEntity(snapshot.people, absence.personId, "person_not_found");
      assertDateOrder(absence.startDate, absence.endDate);
      snapshot.absences = upsert(snapshot.absences, absence, (item) => item.id);
      event = makeEvent(request.command, "absence", absence.id, `Abwesenheit erfasst: ${personNameById(snapshot, absence.personId)}`);
      ctoxPayload = payloadFor("workforce_absence", absence.id, { absence }, ["approve_absence", "cancel_absence"]);
      break;
    }
    case "approve_absence": {
      const absence = requireEntity(snapshot.absences, requireString(payload.id, "id"), "absence_not_found");
      absence.status = "approved";
      absence.updatedAt = at;
      for (const assignment of snapshot.assignments.filter((item) =>
        item.status !== "archived" &&
        item.personId === absence.personId &&
        dateRangesOverlap(item.date, item.date, absence.startDate, absence.endDate)
      )) {
        assignment.status = "blocked";
        assignment.blocker = `Abwesenheit ${absence.startDate}-${absence.endDate}`;
        assignment.updatedAt = at;
      }
      event = makeEvent(request.command, "absence", absence.id, `Abwesenheit freigegeben: ${personNameById(snapshot, absence.personId)}`);
      ctoxPayload = payloadFor("workforce_absence", absence.id, { absence }, ["cancel_absence"]);
      break;
    }
    case "cancel_absence": {
      const absence = requireEntity(snapshot.absences, requireString(payload.id, "id"), "absence_not_found");
      absence.status = "cancelled";
      absence.updatedAt = at;
      event = makeEvent(request.command, "absence", absence.id, `Abwesenheit storniert: ${personNameById(snapshot, absence.personId)}`);
      ctoxPayload = payloadFor("workforce_absence", absence.id, { absence }, ["create_absence"]);
      break;
    }
    case "create_recurring_shift_pattern": {
      const shiftType = requireEntity(snapshot.shiftTypes, requireString(payload.shiftTypeId, "shiftTypeId"), "shift_type_not_found");
      const pattern: WorkforceRecurringShiftPattern = {
        id: stringValue(payload.id) ?? `wrp_${shortId()}`,
        title: stringValue(payload.title) ?? shiftType.name,
        personId: requireString(payload.personId, "personId"),
        shiftTypeId: shiftType.id,
        locationSlotId: stringValue(payload.locationSlotId) ?? snapshot.locationSlots[0]?.id ?? "slot_floor",
        weekday: clampInt(numberValue(payload.weekday) ?? 1, 1, 7),
        startDate: requireString(payload.startDate, "startDate"),
        endDate: stringValue(payload.endDate),
        customer: stringValue(payload.customer),
        project: stringValue(payload.project),
        active: booleanValue(payload.active, true),
        createdAt: at,
        updatedAt: at
      };
      requireEntity(snapshot.people, pattern.personId, "person_not_found");
      requireEntity(snapshot.locationSlots, pattern.locationSlotId, "location_slot_not_found");
      if (pattern.endDate) assertDateOrder(pattern.startDate, pattern.endDate);
      snapshot.recurringPatterns = upsert(snapshot.recurringPatterns, pattern, (item) => item.id);
      event = makeEvent(request.command, "recurring_pattern", pattern.id, `Schichtmuster angelegt: ${pattern.title}`);
      ctoxPayload = payloadFor("workforce_recurring_pattern", pattern.id, { pattern }, ["materialize_recurring_shift_pattern"]);
      break;
    }
    case "materialize_recurring_shift_pattern": {
      const pattern = requireEntity(snapshot.recurringPatterns, requireString(payload.id, "id"), "recurring_pattern_not_found");
      const shiftType = requireEntity(snapshot.shiftTypes, pattern.shiftTypeId, "shift_type_not_found");
      if (!pattern.active) throw new Error("recurring_pattern_inactive");
      const from = stringValue(payload.fromDate) ?? pattern.startDate;
      const to = stringValue(payload.toDate) ?? pattern.endDate ?? addDays(from, 28);
      assertDateOrder(from, to);
      let created = 0;
      for (const date of eachDate(from, to)) {
        if (weekdayNumber(date) !== pattern.weekday) continue;
        const exists = snapshot.assignments.some((assignment) =>
          assignment.status !== "archived" &&
          assignment.personId === pattern.personId &&
          assignment.date === date &&
          rangesOverlap(assignment.startTime, assignment.endTime, shiftType.startTime, shiftType.endTime)
        );
        if (exists) continue;
        const assignment: WorkforceAssignment = {
          id: `wa_${shortId()}`,
          title: pattern.title,
          personId: pattern.personId,
          shiftTypeId: pattern.shiftTypeId,
          locationSlotId: pattern.locationSlotId,
          date,
          startTime: shiftType.startTime,
          endTime: shiftType.endTime,
          customer: pattern.customer,
          project: pattern.project,
          status: "planned",
          notes: `Aus Muster ${pattern.id}`,
          createdAt: at,
          updatedAt: at
        };
        assertAssignment(snapshot, assignment);
        snapshot.assignments.push(assignment);
        created += 1;
      }
      event = makeEvent(request.command, "recurring_pattern", pattern.id, `Schichtmuster materialisiert: ${created} Einsaetze`);
      ctoxPayload = payloadFor("workforce_recurring_pattern", pattern.id, { pattern, created }, ["materialize_recurring_shift_pattern"]);
      break;
    }
    case "prepare_payroll_candidate": {
      const assignment = requireEntity(snapshot.assignments, requireString(payload.assignmentId, "assignmentId"), "assignment_not_found");
      const entry = snapshot.timeEntries.find((item) => item.assignmentId === assignment.id && item.status === "approved");
      if (!entry) throw new Error("approved_time_entry_required");
      const person = requireEntity(snapshot.people, entry.personId, "person_not_found");
      const hourlyRate = numberValue(payload.hourlyRate) ?? 32;
      const hours = durationHours(entry.startTime, entry.endTime, entry.breakMinutes);
      const candidate: WorkforcePayrollCandidate = {
        id: stringValue(payload.id) ?? `wpc_${shortId()}`,
        assignmentId: assignment.id,
        timeEntryId: entry.id,
        employeeId: stringValue(payload.employeeId) ?? person.payrollEmployeeId ?? person.id,
        periodId: stringValue(payload.periodId) ?? payrollPeriodIdForDate(entry.date),
        componentId: stringValue(payload.componentId) ?? "pc-workforce-hours",
        hours,
        hourlyRate,
        amount: round2(hours * hourlyRate),
        preparedAt: at,
        status: "prepared"
      };
      snapshot.payrollCandidates = upsert(snapshot.payrollCandidates, candidate, (item) => item.id);
      event = makeEvent(request.command, "handoff", candidate.id, `Lohnposition vorbereitet: ${person.name} ${candidate.hours.toFixed(2)}h`);
      ctoxPayload = payloadFor("workforce_handoff", candidate.id, { candidate, assignment }, ["payroll_queue_run"]);
      break;
    }
    case "prepare_invoice_candidate": {
      const assignment = requireEntity(snapshot.assignments, requireString(payload.assignmentId, "assignmentId"), "assignment_not_found");
      const entry = snapshot.timeEntries.find((item) => item.assignmentId === assignment.id && item.status === "approved");
      if (!entry) throw new Error("approved_time_entry_required");
      const shiftType = snapshot.shiftTypes.find((item) => item.id === assignment.shiftTypeId);
      if (!shiftType?.billable) throw new Error("assignment_not_billable");
      const candidate: WorkforceInvoiceCandidate = {
        id: stringValue(payload.id) ?? `wic_${shortId()}`,
        assignmentId: assignment.id,
        timeEntryId: entry.id,
        customer: assignment.customer ?? "Interner Kunde",
        project: assignment.project ?? assignment.title,
        hours: durationHours(entry.startTime, entry.endTime, entry.breakMinutes),
        preparedAt: at,
        status: "prepared"
      };
      snapshot.invoiceCandidates = upsert(snapshot.invoiceCandidates, candidate, (item) => item.id);
      assignment.status = "invoice_ready";
      assignment.updatedAt = at;
      event = makeEvent(request.command, "handoff", candidate.id, `Abrechnungsposition vorbereitet: ${candidate.project}`);
      ctoxPayload = payloadFor("workforce_handoff", candidate.id, { candidate, assignment }, ["create_invoice_draft"]);
      break;
    }
    case "create_invoice_draft": {
      const candidate = requireEntity(snapshot.invoiceCandidates, requireString(payload.invoiceCandidateId, "invoiceCandidateId"), "invoice_candidate_not_found");
      const assignment = requireEntity(snapshot.assignments, candidate.assignmentId, "assignment_not_found");
      const hourlyRate = numberValue(payload.hourlyRate) ?? 85;
      const draftId = stringValue(payload.id) ?? `wid_${shortId()}`;
      const businessRecordId = stringValue(payload.businessRecordId) ?? `inv-workforce-${draftId}`;
      const link = businessDeepLink({
        module: "business",
        submodule: "invoices",
        recordId: businessRecordId,
        panel: "invoice",
        drawer: "left-bottom",
        locale: "de",
        theme: "light"
      });
      const draft: WorkforceInvoiceDraft = {
        id: draftId,
        invoiceCandidateId: candidate.id,
        assignmentId: assignment.id,
        customer: candidate.customer,
        project: candidate.project,
        hours: candidate.hours,
        hourlyRate,
        amount: round2(candidate.hours * hourlyRate),
        currency: "EUR",
        status: "draft",
        businessRecordId,
        deepLink: link?.url ?? link?.href ?? `/app/business/invoices?recordId=${businessRecordId}`,
        createdAt: at
      };
      snapshot.invoiceDrafts = upsert(snapshot.invoiceDrafts, draft, (item) => item.id);
      event = makeEvent(request.command, "handoff", draft.id, `Rechnungsdraft erstellt: ${draft.project}`);
      ctoxPayload = payloadFor("workforce_handoff", draft.id, { draft, assignment }, ["business_invoice_sync"]);
      break;
    }
    default:
      throw new Error("unknown_workforce_command");
  }

  snapshot.events = [event, ...snapshot.events].slice(0, 80);
  if (ctoxPayload) {
    snapshot.ctoxPayloads = [ctoxPayload, ...snapshot.ctoxPayloads.filter((payload) =>
      payload.recordType !== ctoxPayload!.recordType || payload.recordId !== ctoxPayload!.recordId
    )].slice(0, 40);
  }
  const next = normalizeSnapshot({ ...snapshot, source: "database" });
  await persistWorkforceSnapshot(next);
  return { ok: true, command: request.command, event, ctoxPayload, snapshot: next };
}

function assertAssignment(snapshot: WorkforceSnapshot, assignment: WorkforceAssignment, currentId?: string) {
  requireEntity(snapshot.people, assignment.personId, "person_not_found");
  requireEntity(snapshot.shiftTypes, assignment.shiftTypeId, "shift_type_not_found");
  requireEntity(snapshot.locationSlots, assignment.locationSlotId, "location_slot_not_found");
  assertTimeOrder(assignment.startTime, assignment.endTime);
  const absence = activeAbsenceFor(snapshot, assignment.personId, assignment.date);
  if (absence) throw new Error(`absence_conflict:${absence.id}`);
  const overlap = snapshot.assignments.find((item) =>
    item.id !== currentId &&
    item.status !== "archived" &&
    item.personId === assignment.personId &&
    item.date === assignment.date &&
    rangesOverlap(item.startTime, item.endTime, assignment.startTime, assignment.endTime)
  );
  if (overlap) throw new Error(`assignment_overlap:${overlap.id}`);
  const policy = workingTimePolicyFindings(snapshot, assignment, currentId);
  const blocker = policy.find((item) => !item.ok && item.severity === "blocker");
  if (blocker) throw new Error(`working_time_policy:${blocker.id}:${blocker.detail}`);
}

function assertTimeEntry(snapshot: WorkforceSnapshot, entry: WorkforceTimeEntry, currentId?: string) {
  requireEntity(snapshot.people, entry.personId, "person_not_found");
  requireEntity(snapshot.assignments, entry.assignmentId, "assignment_not_found");
  assertTimeOrder(entry.startTime, entry.endTime);
  const overlap = snapshot.timeEntries.find((item) =>
    item.id !== currentId &&
    item.personId === entry.personId &&
    item.date === entry.date &&
    item.status !== "correction_requested" &&
    rangesOverlap(item.startTime, item.endTime, entry.startTime, entry.endTime)
  );
  if (overlap) throw new Error(`time_entry_overlap:${overlap.id}`);
}

function normalizeSnapshot(snapshot: WorkforceSnapshot): WorkforceSnapshot {
  const assignments = Array.isArray(snapshot.assignments) ? snapshot.assignments : [];
  const people = Array.isArray(snapshot.people) ? snapshot.people.map(normalizePerson) : [];
  const absences = Array.isArray(snapshot.absences) ? snapshot.absences : [];
  const recurringPatterns = Array.isArray(snapshot.recurringPatterns) ? snapshot.recurringPatterns : [];
  const timeEntries = Array.isArray(snapshot.timeEntries) ? snapshot.timeEntries : [];
  const invoiceCandidates = Array.isArray(snapshot.invoiceCandidates) ? snapshot.invoiceCandidates : [];
  const payrollCandidates = Array.isArray(snapshot.payrollCandidates) ? snapshot.payrollCandidates : [];
  const invoiceDrafts = Array.isArray(snapshot.invoiceDrafts) ? snapshot.invoiceDrafts : [];
  const normalizedForScore = { ...snapshot, people, assignments, absences, recurringPatterns, timeEntries, invoiceCandidates, payrollCandidates, invoiceDrafts } as WorkforceSnapshot;
  return {
    source: snapshot.source ?? "seed",
    companyId: snapshot.companyId ?? "ctox",
    weekStart: snapshot.weekStart ?? "2026-05-11",
    people,
    shiftTypes: Array.isArray(snapshot.shiftTypes) ? snapshot.shiftTypes : [],
    locationSlots: Array.isArray(snapshot.locationSlots) ? snapshot.locationSlots : [],
    absences,
    recurringPatterns,
    assignments,
    timeEntries,
    invoiceCandidates,
    payrollCandidates,
    invoiceDrafts,
    events: Array.isArray(snapshot.events) ? snapshot.events : [],
    ctoxPayloads: dedupeCtoxPayloads(Array.isArray(snapshot.ctoxPayloads) ? snapshot.ctoxPayloads : []),
    scores: assignments.map((assignment) => scoreAssignment(normalizedForScore, assignment))
  };
}

function scoreAssignment(snapshot: WorkforceSnapshot, assignment: WorkforceAssignment): WorkforceScore {
  const person = snapshot.people.find((item) => item.id === assignment.personId);
  const shiftType = snapshot.shiftTypes.find((item) => item.id === assignment.shiftTypeId);
  const slot = snapshot.locationSlots.find((item) => item.id === assignment.locationSlotId);
  const entry = snapshot.timeEntries.find((item) => item.assignmentId === assignment.id);
  const invoice = snapshot.invoiceCandidates.find((item) => item.assignmentId === assignment.id);
  const payroll = snapshot.payrollCandidates.find((item) => item.assignmentId === assignment.id);
  const absence = activeAbsenceFor(snapshot, assignment.personId, assignment.date);
  const policyFindings = workingTimePolicyFindings(snapshot, assignment, assignment.id);
  const overlap = snapshot.assignments.find((item) =>
    item.id !== assignment.id &&
    item.status !== "archived" &&
    item.personId === assignment.personId &&
    item.date === assignment.date &&
    rangesOverlap(item.startTime, item.endTime, assignment.startTime, assignment.endTime)
  );
  const checks: WorkforceScoreCheck[] = [
    check("basis_person", "Mitarbeiter zugewiesen", "basis", !!person, "blocker", person?.name ?? "Keine Person"),
    check("basis_active", "Mitarbeiter aktiv", "basis", !!person?.active, "blocker", person?.active ? "aktiv" : "inaktiv oder fehlt"),
    check("basis_shift", "Schichttyp gesetzt", "basis", !!shiftType, "blocker", shiftType?.name ?? "Kein Schichttyp"),
    check("basis_slot", "Arbeitsplatz gesetzt", "basis", !!slot, "blocker", slot?.name ?? "Kein Arbeitsplatz"),
    check("basis_time", "Zeitfenster gueltig", "basis", timeToMinutes(assignment.endTime) > timeToMinutes(assignment.startTime), "blocker", `${assignment.startTime}-${assignment.endTime}`),
    check("basis_overlap", "Keine Plan-Ueberschneidung", "basis", !overlap, "blocker", overlap ? `Konflikt mit ${overlap.title}` : "frei"),
    check("basis_absence", "Keine Abwesenheit", "basis", !absence, "blocker", absence ? `${absence.type} ${absence.startDate}-${absence.endDate}` : "frei"),
    ...policyFindings.map((finding) => check(finding.id, finding.label, "basis", finding.ok, finding.severity, finding.detail)),
    check("leistung_entry", "Zeitnachweis vorhanden", "leistung", !!entry, "warning", entry ? `${entry.startTime}-${entry.endTime}` : "fehlt"),
    check("leistung_variance", "Abweichung im Rahmen", "leistung", !entry || Math.abs(durationHours(entry.startTime, entry.endTime, entry.breakMinutes) - durationHours(assignment.startTime, assignment.endTime, 0)) <= 0.5, "warning", entry ? `${durationHours(entry.startTime, entry.endTime, entry.breakMinutes)}h Ist` : "noch offen"),
    check("leistung_approved", "Zeit freigegeben", "leistung", entry?.status === "approved", "warning", entry?.status ?? "kein Nachweis"),
    check("bonus_project", "Projekt/Kunde hinterlegt", "bonus", !!assignment.project && !!assignment.customer, "info", assignment.project ?? "nicht gesetzt"),
    check("bonus_evidence", "Nachweis/Notiz vorhanden", "bonus", !!entry?.evidence || !!assignment.notes, "info", entry?.evidence ?? assignment.notes ?? "nicht gesetzt"),
    check("bonus_payroll", "Payroll vorbereitet", "bonus", !!payroll, "info", payroll?.id ?? "noch nicht"),
    check("bonus_handoff", "Rechnung vorbereitet", "bonus", !!invoice || assignment.status === "invoice_ready", "info", invoice?.id ?? "noch nicht")
  ];
  const basis = bucketPercent(checks, "basis");
  const leistung = bucketPercent(checks, "leistung");
  const bonus = bucketPercent(checks, "bonus");
  const percent = Math.round((basis * 0.55) + (leistung * 0.35) + (bonus * 0.1));
  return { assignmentId: assignment.id, percent, basis, leistung, bonus, checks };
}

function bucketPercent(checks: WorkforceScoreCheck[], bucket: WorkforceScoreCheck["bucket"]) {
  const bucketChecks = checks.filter((item) => item.bucket === bucket);
  if (!bucketChecks.length) return 100;
  return Math.round((bucketChecks.filter((item) => item.ok).length / bucketChecks.length) * 100);
}

function check(id: string, label: string, bucket: WorkforceScoreCheck["bucket"], ok: boolean, failedSeverity: WorkforceScoreCheck["severity"], detail: string): WorkforceScoreCheck {
  return { id, label, bucket, ok, severity: ok ? "ok" : failedSeverity, detail };
}

function normalizePerson(person: WorkforcePerson): WorkforcePerson {
  const payrollMap: Record<string, string> = {
    wp_anna: "emp-anna",
    wp_marc: "emp-ben"
  };
  return {
    ...person,
    payrollEmployeeId: person.payrollEmployeeId ?? payrollMap[person.id]
  };
}

function buildWorkforceSeed(): WorkforceSnapshot {
  const at = "2026-05-07T08:00:00.000Z";
  const people: WorkforcePerson[] = [
    { id: "wp_anna", number: "MA-001", name: "Anna Keller", role: "Service", team: "Team Nord", active: true, locationId: "slot_line_a", payrollEmployeeId: "emp-anna", skills: ["Service", "Kasse", "Kundenauftrag"], weeklyHours: 38 },
    { id: "wp_marc", number: "MA-002", name: "Marc Stoll", role: "Technik", team: "Team Werkstatt", active: true, locationId: "slot_workshop", payrollEmployeeId: "emp-ben", skills: ["Montage", "Pruefung", "Dokumentation"], weeklyHours: 40 },
    { id: "wp_emre", number: "MA-003", name: "Emre Can", role: "Logistik", team: "Team Lager", active: true, locationId: "slot_dispatch", skills: ["Kommission", "Warenausgang"], weeklyHours: 32 },
    { id: "wp_lina", number: "MA-004", name: "Lina Vogt", role: "Service", team: "Team Sued", active: false, locationId: "slot_line_b", skills: ["Service", "Inventur"], weeklyHours: 20 }
  ];
  const shiftTypes: WorkforceShiftType[] = [
    { id: "st_service_early", name: "Fruehservice", startTime: "08:00", endTime: "14:00", role: "Service", color: "#7dd3fc", billable: true },
    { id: "st_build", name: "Auftragsfertigung", startTime: "09:00", endTime: "17:00", role: "Technik", color: "#c4b5fd", billable: true },
    { id: "st_dispatch", name: "Warenausgang", startTime: "12:00", endTime: "18:00", role: "Logistik", color: "#fde68a", billable: false }
  ];
  const locationSlots: WorkforceLocationSlot[] = [
    { id: "slot_line_a", name: "Linie A", zone: "Fertigung", capacity: 1 },
    { id: "slot_line_b", name: "Linie B", zone: "Fertigung", capacity: 1 },
    { id: "slot_workshop", name: "Werkbank 2", zone: "Werkstatt", capacity: 1 },
    { id: "slot_dispatch", name: "Packplatz", zone: "Ausgang", capacity: 2 }
  ];
  const assignments: WorkforceAssignment[] = [
    { id: "wa_1001", title: "Service Schicht", personId: "wp_anna", shiftTypeId: "st_service_early", locationSlotId: "slot_line_a", date: "2026-05-11", startTime: "08:00", endTime: "14:00", customer: "Retail Kunde", project: "Shop-Betrieb", status: "planned", notes: "Kasse und Kundenannahme", createdAt: at, updatedAt: at },
    { id: "wa_1002", title: "Baugruppe fuer Auftrag SO-128", personId: "wp_marc", shiftTypeId: "st_build", locationSlotId: "slot_workshop", date: "2026-05-11", startTime: "09:00", endTime: "17:00", customer: "Muster GmbH", project: "SO-128", status: "needs_review", notes: "Montage, Pruefung, Fotodoku", createdAt: at, updatedAt: at },
    { id: "wa_1003", title: "Ausgang & Versand", personId: "wp_emre", shiftTypeId: "st_dispatch", locationSlotId: "slot_dispatch", date: "2026-05-12", startTime: "12:00", endTime: "18:00", status: "needs_time", notes: "Packliste pruefen", createdAt: at, updatedAt: at },
    { id: "wa_1004", title: "Ersatztermin Service", personId: "wp_lina", shiftTypeId: "st_service_early", locationSlotId: "slot_line_b", date: "2026-05-13", startTime: "08:00", endTime: "14:00", status: "blocked", blocker: "Person inaktiv", createdAt: at, updatedAt: at }
  ];
  const timeEntries: WorkforceTimeEntry[] = [
    { id: "wte_1002", assignmentId: "wa_1002", personId: "wp_marc", date: "2026-05-11", startTime: "09:05", endTime: "16:50", breakMinutes: 30, status: "submitted", evidence: "Foto-Doku in SO-128", note: "Pruefung abgeschlossen" }
  ];
  const seed: WorkforceSnapshot = {
    source: "seed",
    companyId: "ctox",
    weekStart: "2026-05-11",
    people,
    shiftTypes,
    locationSlots,
    assignments,
    timeEntries,
    absences: [],
    recurringPatterns: [],
    invoiceCandidates: [],
    payrollCandidates: [],
    invoiceDrafts: [],
    events: [],
    ctoxPayloads: [],
    scores: []
  };
  return normalizeSnapshot(seed);
}

async function persistWorkforceSnapshot(snapshot: WorkforceSnapshot) {
  const persisted = { ...snapshot, source: "database" as const };
  if (await saveRuntimeJsonStore("workforce", persisted)) return;
  throw new Error("Workforce runtime requires configured Postgres persistence.");
}

function payloadFor(recordType: WorkforceCtoxPayload["recordType"], recordId: string, selectedFields: Record<string, unknown>, allowedActions: string[]): WorkforceCtoxPayload {
  return { module: "operations", submodule: "workforce", recordType, recordId, selectedFields, allowedActions };
}

function dedupeCtoxPayloads(payloads: WorkforceCtoxPayload[]) {
  const seen = new Set<string>();
  return payloads.filter((payload) => {
    const key = `${payload.recordType}:${payload.recordId}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function makeEvent(command: WorkforceCommand, entityType: WorkforceEvent["entityType"], entityId: string, message: string): WorkforceEvent {
  return { id: `wev_${shortId()}`, at: nowIso(), command, entityType, entityId, message };
}

function assignmentActions(assignment: WorkforceAssignment) {
  const base = ["update_assignment", "move_assignment", "duplicate_assignment", "archive_assignment", "create_time_entry"];
  if (assignment.status === "blocked") base.push("resolve_blocker");
  if (assignment.status === "approved") base.push("prepare_invoice_candidate");
  return base;
}

export function workforceDeepLink(recordId?: string) {
  return businessDeepLink({ module: "operations", submodule: "workforce", recordId, panel: "workforce", drawer: "bottom", locale: "de", theme: "light" });
}

function requireEntity<T extends { id: string }>(items: T[], id: string, error: string): T {
  const entity = items.find((item) => item.id === id);
  if (!entity) throw new Error(error);
  return entity;
}

function upsert<T>(items: T[], item: T, key: (item: T) => string) {
  const targetKey = key(item);
  const index = items.findIndex((existing) => key(existing) === targetKey);
  if (index === -1) return [...items, item];
  const next = items.slice();
  next[index] = item;
  return next;
}

function requireString(value: unknown, field: string) {
  const str = stringValue(value);
  if (!str) throw new Error(`${field}_required`);
  return str;
}

function stringValue(value: unknown) {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function stringArray(value: unknown) {
  return Array.isArray(value) ? value.map((item) => String(item).trim()).filter(Boolean) : [];
}

function booleanValue(value: unknown, fallback: boolean) {
  return typeof value === "boolean" ? value : fallback;
}

function numberValue(value: unknown) {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim() && Number.isFinite(Number(value))) return Number(value);
  return undefined;
}

function assignmentStatus(value: unknown): WorkforceAssignmentStatus | undefined {
  const allowed: WorkforceAssignmentStatus[] = ["draft", "planned", "in_progress", "needs_time", "needs_review", "approved", "blocked", "invoice_ready", "archived"];
  return allowed.includes(value as WorkforceAssignmentStatus) ? value as WorkforceAssignmentStatus : undefined;
}

function timeEntryStatus(value: unknown): WorkforceTimeEntryStatus | undefined {
  const allowed: WorkforceTimeEntryStatus[] = ["draft", "submitted", "approved", "correction_requested"];
  return allowed.includes(value as WorkforceTimeEntryStatus) ? value as WorkforceTimeEntryStatus : undefined;
}

function absenceStatus(value: unknown): WorkforceAbsenceStatus | undefined {
  const allowed: WorkforceAbsenceStatus[] = ["requested", "approved", "cancelled"];
  return allowed.includes(value as WorkforceAbsenceStatus) ? value as WorkforceAbsenceStatus : undefined;
}

function absenceType(value: unknown): WorkforceAbsence["type"] {
  if (value === "sick" || value === "training" || value === "unavailable") return value;
  return "vacation";
}

function assertTimeOrder(startTime: string, endTime: string) {
  if (timeToMinutes(endTime) <= timeToMinutes(startTime)) throw new Error("invalid_time_range");
}

function assertDateOrder(startDate: string, endDate: string) {
  if (endDate < startDate) throw new Error("invalid_date_range");
}

function rangesOverlap(startA: string, endA: string, startB: string, endB: string) {
  return timeToMinutes(startA) < timeToMinutes(endB) && timeToMinutes(startB) < timeToMinutes(endA);
}

function dateRangesOverlap(startA: string, endA: string, startB: string, endB: string) {
  return startA <= endB && startB <= endA;
}

function durationHours(startTime: string, endTime: string, breakMinutes: number) {
  return Math.max(0, (timeToMinutes(endTime) - timeToMinutes(startTime) - breakMinutes) / 60);
}

function timeToMinutes(value: string) {
  const [hh, mm] = value.split(":").map((part) => Number(part));
  return (Number.isFinite(hh) ? hh : 0) * 60 + (Number.isFinite(mm) ? mm : 0);
}

function activeAbsenceFor(snapshot: WorkforceSnapshot, personId: string, date: string): WorkforceAbsence | undefined {
  return snapshot.absences.find((absence) =>
    absence.status !== "cancelled" &&
    absence.personId === personId &&
    dateRangesOverlap(date, date, absence.startDate, absence.endDate)
  );
}

function workingTimePolicyFindings(snapshot: WorkforceSnapshot, assignment: WorkforceAssignment, currentId?: string): Array<{ id: string; label: string; ok: boolean; severity: "warning" | "blocker"; detail: string }> {
  const dayAssignments = snapshot.assignments
    .filter((item) => item.id !== currentId && item.status !== "archived" && item.personId === assignment.personId && item.date === assignment.date)
    .concat(assignment);
  const dailyHours = round2(dayAssignments.reduce((acc, item) => acc + durationHours(item.startTime, item.endTime, 0), 0));
  const weekStart = startOfIsoWeek(assignment.date);
  const weekEnd = addDays(weekStart, 6);
  const weekHours = round2(snapshot.assignments
    .filter((item) => item.id !== currentId && item.status !== "archived" && item.personId === assignment.personId && item.date >= weekStart && item.date <= weekEnd)
    .concat(assignment)
    .reduce((acc, item) => acc + durationHours(item.startTime, item.endTime, 0), 0));
  const prev = snapshot.assignments
    .filter((item) => item.id !== currentId && item.status !== "archived" && item.personId === assignment.personId && item.date < assignment.date)
    .sort((a, b) => `${b.date}T${b.endTime}`.localeCompare(`${a.date}T${a.endTime}`))[0];
  const restHours = prev ? hoursBetween(prev.date, prev.endTime, assignment.date, assignment.startTime) : 99;
  return [
    { id: "policy_daily_hours", label: "Tagesgrenze eingehalten", ok: dailyHours <= 10, severity: "blocker", detail: `${dailyHours.toFixed(2)}h geplant` },
    { id: "policy_weekly_hours", label: "Wochenlast plausibel", ok: weekHours <= 48, severity: "warning", detail: `${weekHours.toFixed(2)}h in ISO-Woche` },
    { id: "policy_rest_hours", label: "Ruhezeit plausibel", ok: restHours >= 11, severity: "blocker", detail: prev ? `${round2(restHours).toFixed(2)}h nach ${prev.title}` : "kein Vortag" }
  ];
}

function eachDate(from: string, to: string) {
  const dates: string[] = [];
  let cursor = from;
  while (cursor <= to) {
    dates.push(cursor);
    cursor = addDays(cursor, 1);
  }
  return dates;
}

function addDays(date: string, days: number) {
  const value = new Date(`${date}T00:00:00.000Z`);
  value.setUTCDate(value.getUTCDate() + days);
  return value.toISOString().slice(0, 10);
}

function weekdayNumber(date: string) {
  const day = new Date(`${date}T00:00:00.000Z`).getUTCDay();
  return day === 0 ? 7 : day;
}

function startOfIsoWeek(date: string) {
  const value = new Date(`${date}T00:00:00.000Z`);
  const day = value.getUTCDay() || 7;
  value.setUTCDate(value.getUTCDate() - day + 1);
  return value.toISOString().slice(0, 10);
}

function hoursBetween(dateA: string, timeA: string, dateB: string, timeB: string) {
  const a = Date.parse(`${dateA}T${timeA}:00.000Z`);
  const b = Date.parse(`${dateB}T${timeB}:00.000Z`);
  if (Number.isNaN(a) || Number.isNaN(b)) return 99;
  return (b - a) / 3600000;
}

function payrollPeriodIdForDate(date: string) {
  return `period_${date.slice(0, 7).replace("-", "_")}`;
}

function personNameById(snapshot: WorkforceSnapshot, personId: string) {
  return snapshot.people.find((person) => person.id === personId)?.name ?? personId;
}

function clampInt(value: number, min: number, max: number) {
  return Math.min(Math.max(Math.trunc(value), min), max);
}

function round2(value: number) {
  return Math.round(value * 100) / 100;
}

function nowIso() {
  return new Date().toISOString();
}

function shortId() {
  return randomUUID().slice(0, 8);
}
