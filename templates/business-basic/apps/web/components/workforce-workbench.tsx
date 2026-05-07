"use client";

import { useEffect, useMemo, useRef, useState, type CSSProperties, type DragEvent, type FormEvent, type MouseEvent, type PointerEvent as ReactPointerEvent } from "react";
import type {
  WorkforceAssignment,
  WorkforceAssignmentStatus,
  WorkforceCommand,
  WorkforceMutationResult,
  WorkforcePerson,
  WorkforceScore,
  WorkforceShiftType,
  WorkforceSnapshot,
  WorkforceTimeEntry
} from "../lib/workforce-runtime";

type WorkforceWorkbenchProps = {
  query: {
    recordId?: string;
    theme?: string;
    locale?: string;
  };
  snapshot: WorkforceSnapshot;
};

type MutationState = "idle" | "saving" | "saved" | "error";

type ContextMenuState =
  | { kind: "assignment"; assignmentId: string; x: number; y: number }
  | { kind: "cell"; personId: string; date: string; x: number; y: number }
  | null;

type DrawerState = "setup" | "handoff" | null;

const dayLabels = ["Mo", "Di", "Mi", "Do", "Fr"];
const statusLabels: Record<WorkforceAssignmentStatus, string> = {
  draft: "Entwurf",
  planned: "Geplant",
  in_progress: "Laeuft",
  needs_time: "Zeit fehlt",
  needs_review: "Pruefung",
  approved: "Freigegeben",
  blocked: "Blockiert",
  invoice_ready: "Uebergabe",
  archived: "Archiv"
};

export function WorkforceWorkbench({ query, snapshot }: WorkforceWorkbenchProps) {
  const [snap, setSnap] = useState<WorkforceSnapshot>(snapshot);
  const [selectedAssignmentId, setSelectedAssignmentId] = useState<string | undefined>(query.recordId ?? snapshot.assignments.find((item) => item.status !== "archived")?.id);
  const [state, setState] = useState<MutationState>("idle");
  const [error, setError] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<ContextMenuState>(null);
  const [drawer, setDrawer] = useState<DrawerState>(null);
  const pointerDragRef = useRef<{ assignmentId: string; x: number; y: number } | null>(null);

  const days = useMemo(() => weekDays(snap.weekStart), [snap.weekStart]);
  const visibleAssignments = useMemo(
    () => snap.assignments.filter((assignment) => assignment.status !== "archived"),
    [snap.assignments]
  );
  const selectedAssignment = useMemo(
    () => snap.assignments.find((assignment) => assignment.id === selectedAssignmentId),
    [selectedAssignmentId, snap.assignments]
  );
  const selectedScore = selectedAssignment ? scoreFor(snap, selectedAssignment.id) : undefined;
  const submittedEntries = snap.timeEntries.filter((entry) => entry.status === "submitted");
  const blockedAssignments = visibleAssignments.filter((assignment) => assignment.status === "blocked");
  const openDemand = visibleAssignments.filter((assignment) => assignment.status === "needs_time" || assignment.status === "blocked" || assignment.status === "needs_review");
  const payrollCandidates = snap.payrollCandidates ?? [];
  const invoiceDrafts = snap.invoiceDrafts ?? [];

  async function dispatch(command: WorkforceCommand, payload: Record<string, unknown> = {}) {
    setState("saving");
    setError(null);
    try {
      const res = await fetch("/api/operations/workforce", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ command, payload })
      });
      const data = await res.json();
      if (!data.ok) throw new Error(data.error ?? "workforce_command_failed");
      const result = data as WorkforceMutationResult;
      setSnap(result.snapshot);
      if ("id" in payload && typeof payload.id === "string") setSelectedAssignmentId(payload.id);
      if ("assignmentId" in payload && typeof payload.assignmentId === "string") setSelectedAssignmentId(payload.assignmentId);
      setState("saved");
      return result.snapshot;
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setState("error");
      return null;
    }
  }

  function assignmentEntry(assignmentId: string) {
    return snap.timeEntries.find((entry) => entry.assignmentId === assignmentId);
  }

  function onAssignmentContext(event: MouseEvent, assignment: WorkforceAssignment) {
    event.preventDefault();
    event.stopPropagation();
    setSelectedAssignmentId(assignment.id);
    setContextMenu({ kind: "assignment", assignmentId: assignment.id, x: event.clientX, y: event.clientY });
  }

  function onCellContext(event: MouseEvent, personId: string, date: string) {
    event.preventDefault();
    event.stopPropagation();
    setContextMenu({ kind: "cell", personId, date, x: event.clientX, y: event.clientY });
  }

  function onDragStart(event: DragEvent, assignmentId: string) {
    event.dataTransfer.setData("text/plain", assignmentId);
    event.dataTransfer.effectAllowed = "move";
  }

  function onPointerDragStart(event: ReactPointerEvent, assignmentId: string) {
    if (event.button !== 0) return;
    pointerDragRef.current = { assignmentId, x: event.clientX, y: event.clientY };
  }

  async function onDropCell(event: DragEvent, personId: string, date: string) {
    event.preventDefault();
    const assignmentId = event.dataTransfer.getData("text/plain");
    const assignment = snap.assignments.find((item) => item.id === assignmentId);
    if (!assignment) return;
    await dispatch("move_assignment", { id: assignment.id, personId, date });
  }

  async function createDefaultAssignment(personId: string, date: string) {
    const shift = snap.shiftTypes[0];
    const person = snap.people.find((item) => item.id === personId);
    if (!shift || !person) return;
    const next = await dispatch("create_assignment", {
      title: shift.name,
      personId,
      shiftTypeId: shift.id,
      locationSlotId: person.locationId,
      date,
      startTime: shift.startTime,
      endTime: shift.endTime,
      customer: "Interner Bedarf",
      project: "Tagesplanung"
    });
    const created = next?.assignments.at(-1);
    if (created) setSelectedAssignmentId(created.id);
  }

  async function createTimeForAssignment(assignment: WorkforceAssignment) {
    const existing = assignmentEntry(assignment.id);
    if (existing) {
      setSelectedAssignmentId(assignment.id);
      return;
    }
    await dispatch("create_time_entry", {
      assignmentId: assignment.id,
      personId: assignment.personId,
      date: assignment.date,
      startTime: assignment.startTime,
      endTime: assignment.endTime,
      breakMinutes: 30,
      evidence: "Direkt in Einsatzplanung erfasst"
    });
  }

  async function duplicateNextDay(assignment: WorkforceAssignment) {
    await dispatch("duplicate_assignment", {
      id: assignment.id,
      date: addDays(assignment.date, 1)
    });
  }

  useEffect(() => {
    const onPointerUp = (event: PointerEvent) => {
      const drag = pointerDragRef.current;
      pointerDragRef.current = null;
      if (!drag) return;
      const moved = Math.abs(event.clientX - drag.x) + Math.abs(event.clientY - drag.y);
      if (moved < 12) return;
      const target = document
        .elementFromPoint(event.clientX, event.clientY)
        ?.closest<HTMLElement>("[data-wf-person-id][data-wf-date]");
      if (!target) return;
      const personId = target.dataset.wfPersonId;
      const date = target.dataset.wfDate;
      if (!personId || !date) return;
      dispatch("move_assignment", { id: drag.assignmentId, personId, date });
    };
    window.addEventListener("pointerup", onPointerUp);
    return () => window.removeEventListener("pointerup", onPointerUp);
  });

  useEffect(() => {
    const onContextAction = (event: Event) => {
      const detail = (event as CustomEvent<{ actionId?: string; item?: { recordId?: string; recordType?: string } }>).detail;
      if (!detail?.actionId || !detail.item?.recordId) return;
      if (!String(detail.item.recordType ?? "").startsWith("workforce_")) return;
      const assignment = snap.assignments.find((item) => item.id === detail.item?.recordId);
      const entry = snap.timeEntries.find((item) => item.id === detail.item?.recordId || item.assignmentId === detail.item?.recordId);
      if (detail.actionId === "workforce-assignment-edit" && assignment) {
        setSelectedAssignmentId(assignment.id);
      } else if (detail.actionId === "workforce-assignment-duplicate" && assignment) {
        duplicateNextDay(assignment);
      } else if (detail.actionId === "workforce-assignment-time" && assignment) {
        createTimeForAssignment(assignment);
      } else if (detail.actionId === "workforce-assignment-payroll" && assignment) {
        dispatch("prepare_payroll_candidate", { assignmentId: assignment.id });
      } else if (detail.actionId === "workforce-assignment-invoice-draft" && assignment) {
        prepareInvoiceDraft(assignment);
      } else if (detail.actionId === "workforce-time-approve" && entry) {
        dispatch("approve_time_entry", { id: entry.id });
      } else if (detail.actionId === "workforce-time-correction" && entry) {
        dispatch("request_correction", { id: entry.id, note: "Korrektur per Kontextmenue angefordert" });
      } else if (detail.actionId === "workforce-person-edit") {
        setDrawer("setup");
      }
    };
    window.addEventListener("ctox:context-action", onContextAction);
    return () => window.removeEventListener("ctox:context-action", onContextAction);
  });

  async function handleCreateAssignment(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    await dispatch("create_assignment", {
      title: String(form.get("title") ?? ""),
      personId: String(form.get("personId") ?? ""),
      shiftTypeId: String(form.get("shiftTypeId") ?? ""),
      locationSlotId: String(form.get("locationSlotId") ?? ""),
      date: String(form.get("date") ?? ""),
      startTime: String(form.get("startTime") ?? ""),
      endTime: String(form.get("endTime") ?? ""),
      customer: String(form.get("customer") ?? ""),
      project: String(form.get("project") ?? "")
    });
    event.currentTarget.reset();
  }

  async function handleUpdateAssignment(event: FormEvent<HTMLFormElement>, assignment: WorkforceAssignment) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    await dispatch("update_assignment", {
      id: assignment.id,
      title: String(form.get("title") ?? assignment.title),
      personId: String(form.get("personId") ?? assignment.personId),
      shiftTypeId: String(form.get("shiftTypeId") ?? assignment.shiftTypeId),
      locationSlotId: String(form.get("locationSlotId") ?? assignment.locationSlotId),
      date: String(form.get("date") ?? assignment.date),
      startTime: String(form.get("startTime") ?? assignment.startTime),
      endTime: String(form.get("endTime") ?? assignment.endTime),
      customer: String(form.get("customer") ?? ""),
      project: String(form.get("project") ?? ""),
      notes: String(form.get("notes") ?? "")
    });
  }

  async function handleCreateShiftType(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    await dispatch("create_shift_type", {
      name: String(form.get("name") ?? ""),
      role: String(form.get("role") ?? ""),
      startTime: String(form.get("startTime") ?? "08:00"),
      endTime: String(form.get("endTime") ?? "16:00"),
      color: String(form.get("color") ?? "#7dd3fc"),
      billable: form.get("billable") === "on"
    });
    event.currentTarget.reset();
  }

  async function handleCreatePerson(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    await dispatch("create_person", {
      name: String(form.get("name") ?? ""),
      role: String(form.get("role") ?? ""),
      team: String(form.get("team") ?? ""),
      locationId: String(form.get("locationId") ?? ""),
      weeklyHours: Number(form.get("weeklyHours") ?? 40)
    });
    event.currentTarget.reset();
  }

  async function handleCreateAbsence(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    await dispatch("create_absence", {
      personId: String(form.get("personId") ?? ""),
      startDate: String(form.get("startDate") ?? ""),
      endDate: String(form.get("endDate") ?? ""),
      type: String(form.get("type") ?? "vacation"),
      status: form.get("approved") === "on" ? "approved" : "requested",
      note: String(form.get("note") ?? "")
    });
    event.currentTarget.reset();
  }

  async function handleCreatePattern(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const result = await dispatch("create_recurring_shift_pattern", {
      title: String(form.get("title") ?? ""),
      personId: String(form.get("personId") ?? ""),
      shiftTypeId: String(form.get("shiftTypeId") ?? ""),
      locationSlotId: String(form.get("locationSlotId") ?? ""),
      weekday: Number(form.get("weekday") ?? 1),
      startDate: String(form.get("startDate") ?? ""),
      endDate: String(form.get("endDate") ?? ""),
      customer: String(form.get("customer") ?? ""),
      project: String(form.get("project") ?? "")
    });
    const created = result?.recurringPatterns.at(-1);
    if (created) {
      await dispatch("materialize_recurring_shift_pattern", { id: created.id, fromDate: created.startDate, toDate: created.endDate ?? addDays(created.startDate, 14) });
    }
    event.currentTarget.reset();
  }

  async function prepareInvoiceDraft(assignment: WorkforceAssignment) {
    const withCandidate = snap.invoiceCandidates.find((candidate) => candidate.assignmentId === assignment.id)
      ? snap
      : await dispatch("prepare_invoice_candidate", { assignmentId: assignment.id });
    const candidate = (withCandidate ?? snap).invoiceCandidates.find((item) => item.assignmentId === assignment.id);
    if (candidate) await dispatch("create_invoice_draft", { invoiceCandidateId: candidate.id });
  }

  return (
    <div className="wf2-shell" onClick={() => setContextMenu(null)}>
      <aside className="wf2-rail wf2-rail-left">
        <header className="wf2-rail-header">
          <div>
            <span>Bedarf</span>
            <h2>Eingang & Personal</h2>
          </div>
          <button type="button" onClick={() => setDrawer("setup")}>Verwalten</button>
        </header>

        <section className="wf2-panel">
          <h3>Neuer Einsatz</h3>
          <form className="wf2-form" onSubmit={handleCreateAssignment}>
            <input name="title" placeholder="Aufgabe oder Auftrag" required />
            <div className="wf2-form-grid">
              <select name="personId" defaultValue={snap.people[0]?.id ?? ""} required>
                {snap.people.map((person) => <option key={person.id} value={person.id}>{person.name}</option>)}
              </select>
              <select name="shiftTypeId" defaultValue={snap.shiftTypes[0]?.id ?? ""} required>
                {snap.shiftTypes.map((shift) => <option key={shift.id} value={shift.id}>{shift.name}</option>)}
              </select>
            </div>
            <div className="wf2-form-grid">
              <input name="date" type="date" defaultValue={days[0]} required />
              <select name="locationSlotId" defaultValue={snap.locationSlots[0]?.id ?? ""}>
                {snap.locationSlots.map((slot) => <option key={slot.id} value={slot.id}>{slot.name}</option>)}
              </select>
            </div>
            <div className="wf2-form-grid">
              <input name="startTime" type="time" defaultValue={snap.shiftTypes[0]?.startTime ?? "08:00"} required />
              <input name="endTime" type="time" defaultValue={snap.shiftTypes[0]?.endTime ?? "16:00"} required />
            </div>
            <input name="customer" placeholder="Kunde optional" />
            <input name="project" placeholder="Auftrag/Projekt optional" />
            <button type="submit" disabled={state === "saving"}>Einsatz anlegen</button>
          </form>
        </section>

        <section className="wf2-panel">
          <h3>Offene Arbeit</h3>
          <div className="wf2-stack">
            {openDemand.map((assignment) => (
              <button
                className="wf2-demand"
                data-testid={`wf-demand-${assignment.id}`}
                key={assignment.id}
                onClick={() => setSelectedAssignmentId(assignment.id)}
                onContextMenu={(event) => onAssignmentContext(event, assignment)}
                type="button"
              >
                <strong>{assignment.title}</strong>
                <span>{personName(snap, assignment.personId)} · {statusLabels[assignment.status]}</span>
              </button>
            ))}
            {!openDemand.length && <p className="wf2-muted">Keine offenen Pruefungen.</p>}
          </div>
        </section>

        <section className="wf2-panel wf2-people">
          <h3>Teams</h3>
          {snap.people.map((person) => (
            <button
              className={person.id === selectedAssignment?.personId ? "is-selected" : ""}
              key={person.id}
              onClick={() => setDrawer("setup")}
              type="button"
            >
              <span>{initials(person.name)}</span>
              <strong>{person.name}</strong>
              <small>{person.team} · {person.active ? "aktiv" : "inaktiv"}</small>
            </button>
          ))}
        </section>
      </aside>

      <main className="wf2-board">
        <header className="wf2-board-header">
          <div>
            <span>Einsatzplanung</span>
            <h1>Wochenplan {formatDate(days[0])} bis {formatDate(days[days.length - 1])}</h1>
          </div>
          <div className="wf2-status-row">
            {state === "saved" && <span className="wf2-state ok">Gespeichert</span>}
            {state === "error" && <span className="wf2-state err">Fehler: {error}</span>}
            {state === "saving" && <span className="wf2-state">Speichert</span>}
            <button type="button" onClick={() => selectedAssignment && createTimeForAssignment(selectedAssignment)} disabled={!selectedAssignment}>Zeit erfassen</button>
            <button type="button" onClick={() => setDrawer("handoff")}>Uebergaben</button>
          </div>
        </header>

        <section className="wf2-roster" data-testid="wf-roster">
          <div className="wf2-roster-head">
            <div>Person</div>
            {days.map((day, index) => <div key={day}>{dayLabels[index]} <span>{formatDate(day)}</span></div>)}
          </div>
          {snap.people.map((person) => (
            <div className="wf2-row" key={person.id}>
              <div className="wf2-person-cell">
                <span>{initials(person.name)}</span>
                <strong>{person.name}</strong>
                <small>{person.role} · {person.weeklyHours}h</small>
              </div>
              {days.map((day) => {
                const cellAssignments = visibleAssignments.filter((assignment) => assignment.personId === person.id && assignment.date === day);
                return (
                  <div
                    className="wf2-cell"
                    data-testid={`wf-cell-${person.id}-${day}`}
                    key={day}
                    data-wf-date={day}
                    data-wf-person-id={person.id}
                    onContextMenu={(event) => onCellContext(event, person.id, day)}
                    onDragOver={(event) => event.preventDefault()}
                    onDrop={(event) => onDropCell(event, person.id, day)}
                  >
                    {cellAssignments.map((assignment) => (
                      <AssignmentCard
                        assignment={assignment}
                        entry={assignmentEntry(assignment.id)}
                        key={assignment.id}
                        onClick={() => setSelectedAssignmentId(assignment.id)}
                        onContextMenu={(event) => onAssignmentContext(event, assignment)}
                        onDragStart={(event) => onDragStart(event, assignment.id)}
                        onPointerDown={(event) => onPointerDragStart(event, assignment.id)}
                        score={scoreFor(snap, assignment.id)}
                        selected={selectedAssignmentId === assignment.id}
                        shiftType={snap.shiftTypes.find((item) => item.id === assignment.shiftTypeId)}
                      />
                    ))}
                    {!cellAssignments.length && <button className="wf2-empty-slot" onClick={() => createDefaultAssignment(person.id, day)} type="button">+</button>}
                  </div>
                );
              })}
            </div>
          ))}
        </section>
      </main>

      <aside className="wf2-rail wf2-rail-right">
        <header className="wf2-rail-header">
          <div>
            <span>Ausgang</span>
            <h2>Pruefung & Uebergabe</h2>
          </div>
          <button type="button" onClick={() => setDrawer("handoff")}>Oeffnen</button>
        </header>

        <section className="wf2-panel">
          <h3>Zeitpruefung</h3>
          <div className="wf2-stack">
            {submittedEntries.map((entry) => {
              const assignment = snap.assignments.find((item) => item.id === entry.assignmentId);
              return assignment ? (
                <article className="wf2-review" key={entry.id}>
                  <strong>{assignment.title}</strong>
                  <span>{personName(snap, entry.personId)} · {entry.startTime}-{entry.endTime}</span>
                  <div>
                    <button type="button" onClick={() => dispatch("approve_time_entry", { id: entry.id })}>Freigeben</button>
                    <button type="button" onClick={() => dispatch("request_correction", { id: entry.id, note: "Bitte Zeitnachweis pruefen" })}>Korrektur</button>
                  </div>
                </article>
              ) : null;
            })}
            {!submittedEntries.length && <p className="wf2-muted">Keine eingereichten Zeiten.</p>}
          </div>
        </section>

        <section className="wf2-panel">
          <h3>Blocker</h3>
          <div className="wf2-stack">
            {blockedAssignments.map((assignment) => (
              <article className="wf2-review is-blocked" key={assignment.id} onClick={() => setSelectedAssignmentId(assignment.id)}>
                <strong>{assignment.title}</strong>
                <span>{assignment.blocker ?? "Blockiert"}</span>
                <button type="button" onClick={(event) => { event.stopPropagation(); dispatch("resolve_blocker", { id: assignment.id }); }}>Loesen</button>
              </article>
            ))}
            {!blockedAssignments.length && <p className="wf2-muted">Keine Blocker.</p>}
          </div>
        </section>

        <section className="wf2-panel">
          <h3>Bereit fuer Rechnung</h3>
          <div className="wf2-stack">
            {snap.invoiceCandidates.map((candidate) => (
              <article className="wf2-review" key={candidate.id}>
                <strong>{candidate.project}</strong>
                <span>{candidate.customer} · {candidate.hours.toFixed(2)}h</span>
              </article>
            ))}
            {!snap.invoiceCandidates.length && <p className="wf2-muted">Noch keine vorbereitete Position.</p>}
          </div>
        </section>

        <section className="wf2-panel">
          <h3>Payroll & Drafts</h3>
          <div className="wf2-stack">
            {payrollCandidates.slice(0, 3).map((candidate) => (
              <article className="wf2-review" key={candidate.id}>
                <strong>{candidate.employeeId}</strong>
                <span>{candidate.hours.toFixed(2)}h · {candidate.amount.toFixed(2)} EUR · {candidate.periodId}</span>
              </article>
            ))}
            {invoiceDrafts.slice(0, 3).map((draft) => (
              <article className="wf2-review" key={draft.id}>
                <strong>{draft.project}</strong>
                <span>{draft.amount.toFixed(2)} {draft.currency} · Draft {draft.businessRecordId}</span>
              </article>
            ))}
            {!payrollCandidates.length && !invoiceDrafts.length && <p className="wf2-muted">Noch keine Payroll- oder Rechnungsdrafts.</p>}
          </div>
        </section>
      </aside>

      {selectedAssignment && selectedScore && (
        <BottomAssignmentDrawer
          assignment={selectedAssignment}
          entry={assignmentEntry(selectedAssignment.id)}
          onApprove={(entry) => dispatch("approve_time_entry", { id: entry.id })}
          onArchive={() => dispatch("archive_assignment", { id: selectedAssignment.id })}
          onClose={() => setSelectedAssignmentId(undefined)}
          onCreateTime={() => createTimeForAssignment(selectedAssignment)}
          onDuplicate={() => duplicateNextDay(selectedAssignment)}
          onPrepareInvoice={() => dispatch("prepare_invoice_candidate", { assignmentId: selectedAssignment.id })}
          onPrepareInvoiceDraft={() => prepareInvoiceDraft(selectedAssignment)}
          onPreparePayroll={() => dispatch("prepare_payroll_candidate", { assignmentId: selectedAssignment.id })}
          onSubmit={(event) => handleUpdateAssignment(event, selectedAssignment)}
          people={snap.people}
          score={selectedScore}
          shiftTypes={snap.shiftTypes}
          slots={snap.locationSlots}
        />
      )}

      {drawer === "setup" && (
        <aside className="wf2-side-drawer left" data-testid="wf-setup-drawer">
          <header>
            <h2>Stammdaten</h2>
            <button type="button" onClick={() => setDrawer(null)}>Schliessen</button>
          </header>
          <section>
            <h3>Schichttyp anlegen</h3>
            <form className="wf2-form" onSubmit={handleCreateShiftType}>
              <input name="name" placeholder="Name" required />
              <div className="wf2-form-grid">
                <input name="role" placeholder="Rolle" />
                <input name="color" type="color" defaultValue="#7dd3fc" />
              </div>
              <div className="wf2-form-grid">
                <input name="startTime" type="time" defaultValue="08:00" />
                <input name="endTime" type="time" defaultValue="16:00" />
              </div>
              <label className="wf2-check"><input name="billable" type="checkbox" defaultChecked /> abrechenbar</label>
              <button type="submit">Schichttyp speichern</button>
            </form>
          </section>
          <section>
            <h3>Mitarbeiter anlegen</h3>
            <form className="wf2-form" onSubmit={handleCreatePerson}>
              <input name="name" placeholder="Name" required />
              <div className="wf2-form-grid">
                <input name="role" placeholder="Rolle" />
                <input name="team" placeholder="Team" />
              </div>
              <div className="wf2-form-grid">
                <select name="locationId" defaultValue={snap.locationSlots[0]?.id ?? ""}>
                  {snap.locationSlots.map((slot) => <option key={slot.id} value={slot.id}>{slot.name}</option>)}
                </select>
                <input name="weeklyHours" type="number" defaultValue={40} min={1} max={60} />
              </div>
              <button type="submit">Mitarbeiter speichern</button>
            </form>
          </section>
          <section>
            <h3>Abwesenheit</h3>
            <form className="wf2-form" onSubmit={handleCreateAbsence}>
              <select name="personId" defaultValue={snap.people[0]?.id ?? ""}>
                {snap.people.map((person) => <option key={person.id} value={person.id}>{person.name}</option>)}
              </select>
              <div className="wf2-form-grid">
                <input name="startDate" type="date" defaultValue={days[0]} required />
                <input name="endDate" type="date" defaultValue={days[0]} required />
              </div>
              <select name="type" defaultValue="vacation">
                <option value="vacation">Urlaub</option>
                <option value="sick">Krank</option>
                <option value="training">Training</option>
                <option value="unavailable">Nicht verfuegbar</option>
              </select>
              <input name="note" placeholder="Notiz" />
              <label className="wf2-check"><input name="approved" type="checkbox" /> direkt freigeben</label>
              <button type="submit">Abwesenheit speichern</button>
            </form>
          </section>
          <section>
            <h3>Wiederkehrende Schicht</h3>
            <form className="wf2-form" onSubmit={handleCreatePattern}>
              <input name="title" placeholder="Mustername" required />
              <div className="wf2-form-grid">
                <select name="personId" defaultValue={snap.people[0]?.id ?? ""}>
                  {snap.people.map((person) => <option key={person.id} value={person.id}>{person.name}</option>)}
                </select>
                <select name="shiftTypeId" defaultValue={snap.shiftTypes[0]?.id ?? ""}>
                  {snap.shiftTypes.map((shift) => <option key={shift.id} value={shift.id}>{shift.name}</option>)}
                </select>
              </div>
              <div className="wf2-form-grid">
                <select name="locationSlotId" defaultValue={snap.locationSlots[0]?.id ?? ""}>
                  {snap.locationSlots.map((slot) => <option key={slot.id} value={slot.id}>{slot.name}</option>)}
                </select>
                <select name="weekday" defaultValue="1">
                  <option value="1">Montag</option>
                  <option value="2">Dienstag</option>
                  <option value="3">Mittwoch</option>
                  <option value="4">Donnerstag</option>
                  <option value="5">Freitag</option>
                </select>
              </div>
              <div className="wf2-form-grid">
                <input name="startDate" type="date" defaultValue={days[0]} required />
                <input name="endDate" type="date" defaultValue={addDays(days[0], 14)} />
              </div>
              <div className="wf2-form-grid">
                <input name="customer" placeholder="Kunde" />
                <input name="project" placeholder="Projekt" />
              </div>
              <button type="submit">Muster anlegen + planen</button>
            </form>
          </section>
          <section>
            <h3>Abwesenheiten & Muster</h3>
            <div className="wf2-stack">
              {snap.absences.slice(0, 4).map((absence) => (
                <article className="wf2-master-row" key={absence.id}>
                  <span />
                  <strong>{personName(snap, absence.personId)}</strong>
                  <small>{absence.type} · {absence.startDate}-{absence.endDate} · {absence.status}</small>
                </article>
              ))}
              {snap.recurringPatterns.slice(0, 4).map((pattern) => (
                <article className="wf2-master-row" key={pattern.id}>
                  <span />
                  <strong>{pattern.title}</strong>
                  <small>{personName(snap, pattern.personId)} · Wochentag {pattern.weekday}</small>
                </article>
              ))}
            </div>
          </section>
          <section>
            <h3>Aktive Schichttypen</h3>
            <div className="wf2-stack">
              {snap.shiftTypes.map((shift) => (
                <article className="wf2-master-row" key={shift.id}>
                  <span style={{ backgroundColor: shift.color }} />
                  <strong>{shift.name}</strong>
                  <small>{shift.startTime}-{shift.endTime} · {shift.role}</small>
                </article>
              ))}
            </div>
          </section>
        </aside>
      )}

      {drawer === "handoff" && (
        <aside className="wf2-side-drawer right" data-testid="wf-handoff-drawer">
          <header>
            <h2>Uebergaben</h2>
            <button type="button" onClick={() => setDrawer(null)}>Schliessen</button>
          </header>
          <section>
            <h3>CTOX Payloads</h3>
            <div className="wf2-stack">
              {snap.ctoxPayloads.map((payload) => (
                <article className="wf2-master-row" key={`${payload.recordType}-${payload.recordId}`}>
                  <strong>{payload.recordType}</strong>
                  <small>{payload.recordId} · {payload.allowedActions.join(", ")}</small>
                </article>
              ))}
              {!snap.ctoxPayloads.length && <p className="wf2-muted">Noch keine Payloads erzeugt.</p>}
            </div>
          </section>
          <section>
            <h3>Payroll-Kandidaten</h3>
            <div className="wf2-stack">
              {payrollCandidates.map((candidate) => (
                <article className="wf2-master-row" key={candidate.id}>
                  <span />
                  <strong>{candidate.employeeId}</strong>
                  <small>{candidate.periodId} · {candidate.hours.toFixed(2)}h · {candidate.amount.toFixed(2)} EUR</small>
                </article>
              ))}
              {!payrollCandidates.length && <p className="wf2-muted">Noch keine Payroll-Kandidaten.</p>}
            </div>
          </section>
          <section>
            <h3>Rechnungsdrafts</h3>
            <div className="wf2-stack">
              {invoiceDrafts.map((draft) => (
                <article className="wf2-master-row" key={draft.id}>
                  <span />
                  <strong>{draft.project}</strong>
                  <small>{draft.customer} · {draft.amount.toFixed(2)} {draft.currency}</small>
                </article>
              ))}
              {!invoiceDrafts.length && <p className="wf2-muted">Noch keine Rechnungsdrafts.</p>}
            </div>
          </section>
          <section>
            <h3>Ereignisse</h3>
            <div className="wf2-stack">
              {snap.events.slice(0, 12).map((event) => (
                <article className="wf2-master-row" key={event.id}>
                  <strong>{event.message}</strong>
                  <small>{event.command} · {new Date(event.at).toLocaleString("de-DE")}</small>
                </article>
              ))}
            </div>
          </section>
        </aside>
      )}

      {contextMenu && (
        <ContextMenu
          context={contextMenu}
          onArchive={(assignment) => dispatch("archive_assignment", { id: assignment.id })}
          onApprove={(entry) => dispatch("approve_time_entry", { id: entry.id })}
          onClose={() => setContextMenu(null)}
          onCreateCell={(personId, date) => createDefaultAssignment(personId, date)}
          onCreateTime={(assignment) => createTimeForAssignment(assignment)}
          onDuplicate={(assignment) => duplicateNextDay(assignment)}
          onPrepareInvoice={(assignment) => dispatch("prepare_invoice_candidate", { assignmentId: assignment.id })}
          onPrepareInvoiceDraft={(assignment) => prepareInvoiceDraft(assignment)}
          onPreparePayroll={(assignment) => dispatch("prepare_payroll_candidate", { assignmentId: assignment.id })}
          onSelect={(assignment) => setSelectedAssignmentId(assignment.id)}
          snap={snap}
        />
      )}
    </div>
  );
}

function AssignmentCard({
  assignment,
  entry,
  onClick,
  onContextMenu,
  onDragStart,
  onPointerDown,
  score,
  selected,
  shiftType
}: {
  assignment: WorkforceAssignment;
  entry?: WorkforceTimeEntry;
  onClick: () => void;
  onContextMenu: (event: MouseEvent) => void;
  onDragStart: (event: DragEvent) => void;
  onPointerDown: (event: ReactPointerEvent) => void;
  score: WorkforceScore;
  selected: boolean;
  shiftType?: WorkforceShiftType;
}) {
  return (
    <article
      className={`wf2-card status-${assignment.status}${selected ? " is-selected" : ""}`}
      data-context-item
      data-context-label={assignment.title}
      data-context-module="operations"
      data-context-record-id={assignment.id}
      data-context-record-type="workforce_assignment"
      data-context-skill="product_engineering/business-basic-module-development"
      data-context-submodule="workforce"
      data-testid={`wf-assignment-${assignment.id}`}
      draggable
      onClick={onClick}
      onContextMenu={onContextMenu}
      onDragStart={onDragStart}
      onPointerDown={onPointerDown}
      style={{ "--wf-card-color": shiftType?.color ?? "#7dd3fc" } as CSSProperties}
    >
      <div className="wf2-card-top">
        <strong>{assignment.title}</strong>
        <span>{score.percent}%</span>
      </div>
      <small>{assignment.startTime}-{assignment.endTime} · {statusLabels[assignment.status]}</small>
      <div className="wf2-card-bottom">
        <span>{entry ? `${entry.status}` : "keine Istzeit"}</span>
        <span>{assignment.project ?? "ohne Auftrag"}</span>
      </div>
    </article>
  );
}

function BottomAssignmentDrawer({
  assignment,
  entry,
  onApprove,
  onArchive,
  onClose,
  onCreateTime,
  onDuplicate,
  onPrepareInvoice,
  onPrepareInvoiceDraft,
  onPreparePayroll,
  onSubmit,
  people,
  score,
  shiftTypes,
  slots
}: {
  assignment: WorkforceAssignment;
  entry?: WorkforceTimeEntry;
  onApprove: (entry: WorkforceTimeEntry) => void;
  onArchive: () => void;
  onClose: () => void;
  onCreateTime: () => void;
  onDuplicate: () => void;
  onPrepareInvoice: () => void;
  onPrepareInvoiceDraft: () => void;
  onPreparePayroll: () => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  people: WorkforcePerson[];
  score: WorkforceScore;
  shiftTypes: WorkforceShiftType[];
  slots: { id: string; name: string }[];
}) {
  const buckets = [
    { key: "basis", label: "Basis-Anforderungen" },
    { key: "leistung", label: "Leistungsanforderungen" },
    { key: "bonus", label: "Begeisterungsfaktoren" }
  ] as const;
  return (
    <section className="wf2-bottom" data-testid="wf-bottom-drawer">
      <header>
        <div>
          <span>Match-Scoring</span>
          <h2>{assignment.title}</h2>
        </div>
        <strong>{score.percent}%</strong>
        <button type="button" onClick={onClose}>Schliessen</button>
      </header>
      <div className="wf2-bottom-grid">
        <form className="wf2-form" onSubmit={onSubmit}>
          <input name="title" defaultValue={assignment.title} />
          <div className="wf2-form-grid">
            <select name="personId" defaultValue={assignment.personId}>
              {people.map((person) => <option key={person.id} value={person.id}>{person.name}</option>)}
            </select>
            <select name="shiftTypeId" defaultValue={assignment.shiftTypeId}>
              {shiftTypes.map((shift) => <option key={shift.id} value={shift.id}>{shift.name}</option>)}
            </select>
          </div>
          <div className="wf2-form-grid">
            <input name="date" type="date" defaultValue={assignment.date} />
            <select name="locationSlotId" defaultValue={assignment.locationSlotId}>
              {slots.map((slot) => <option key={slot.id} value={slot.id}>{slot.name}</option>)}
            </select>
          </div>
          <div className="wf2-form-grid">
            <input name="startTime" type="time" defaultValue={assignment.startTime} />
            <input name="endTime" type="time" defaultValue={assignment.endTime} />
          </div>
          <div className="wf2-form-grid">
            <input name="customer" defaultValue={assignment.customer ?? ""} placeholder="Kunde" />
            <input name="project" defaultValue={assignment.project ?? ""} placeholder="Auftrag" />
          </div>
          <textarea name="notes" defaultValue={assignment.notes ?? ""} placeholder="Notizen, Handover, Blocker" />
          <button type="submit">Einsatz speichern</button>
        </form>
        <section className="wf2-score-columns">
          {buckets.map((bucket) => (
            <div key={bucket.key}>
              <h3>{bucket.label}</h3>
              {score.checks.filter((check) => check.bucket === bucket.key).map((check) => (
                <article className={`wf2-check-row ${check.ok ? "ok" : check.severity}`} key={check.id}>
                  <strong>{check.ok ? "OK" : "!"}</strong>
                  <span>{check.label}</span>
                  <small>{check.detail}</small>
                </article>
              ))}
            </div>
          ))}
        </section>
        <section className="wf2-bottom-actions">
          <h3>Direktaktionen</h3>
          <button type="button" onClick={onCreateTime}>{entry ? "Zeitnachweis oeffnen" : "Zeitnachweis erstellen"}</button>
          <button type="button" onClick={onDuplicate}>Naechsten Tag duplizieren</button>
          <button type="button" onClick={onArchive}>Archivieren</button>
          <button type="button" onClick={() => entry && onApprove(entry)} disabled={!entry || entry.status === "approved"}>Zeit freigeben</button>
          <button type="button" onClick={onPreparePayroll} disabled={!entry || entry.status !== "approved"}>Payroll vorbereiten</button>
          <button type="button" onClick={onPrepareInvoice} disabled={!entry || entry.status !== "approved"}>Rechnung vorbereiten</button>
          <button type="button" onClick={onPrepareInvoiceDraft} disabled={!entry || entry.status !== "approved"}>Rechnungsdraft erstellen</button>
          <div className="wf2-time-fact">
            <strong>Istzeit</strong>
            <span>{entry ? `${entry.startTime}-${entry.endTime} · ${entry.status}` : "noch nicht erfasst"}</span>
          </div>
        </section>
      </div>
    </section>
  );
}

function ContextMenu({
  context,
  onArchive,
  onApprove,
  onClose,
  onCreateCell,
  onCreateTime,
  onDuplicate,
  onPrepareInvoice,
  onPrepareInvoiceDraft,
  onPreparePayroll,
  onSelect,
  snap
}: {
  context: ContextMenuState;
  onArchive: (assignment: WorkforceAssignment) => void;
  onApprove: (entry: WorkforceTimeEntry) => void;
  onClose: () => void;
  onCreateCell: (personId: string, date: string) => void;
  onCreateTime: (assignment: WorkforceAssignment) => void;
  onDuplicate: (assignment: WorkforceAssignment) => void;
  onPrepareInvoice: (assignment: WorkforceAssignment) => void;
  onPrepareInvoiceDraft: (assignment: WorkforceAssignment) => void;
  onPreparePayroll: (assignment: WorkforceAssignment) => void;
  onSelect: (assignment: WorkforceAssignment) => void;
  snap: WorkforceSnapshot;
}) {
  if (!context) return null;
  if (context.kind === "cell") {
    return (
      <div className="wf2-context" data-testid="wf-context-menu" style={{ left: context.x, top: context.y }} onClick={(event) => event.stopPropagation()}>
        <button type="button" onClick={() => { onCreateCell(context.personId, context.date); onClose(); }}>Einsatz hier anlegen</button>
        <button type="button" onClick={onClose}>Schliessen</button>
      </div>
    );
  }
  const assignment = snap.assignments.find((item) => item.id === context.assignmentId);
  if (!assignment) return null;
  const entry = snap.timeEntries.find((item) => item.assignmentId === assignment.id);
  return (
    <div className="wf2-context" data-testid="wf-context-menu" style={{ left: context.x, top: context.y }} onClick={(event) => event.stopPropagation()}>
      <button type="button" onClick={() => { onSelect(assignment); onClose(); }}>Unten bearbeiten</button>
      <button type="button" onClick={() => { onCreateTime(assignment); onClose(); }}>{entry ? "Zeitnachweis oeffnen" : "Zeit erfassen"}</button>
      <button type="button" onClick={() => { onDuplicate(assignment); onClose(); }}>Duplizieren</button>
      <button type="button" onClick={() => { entry && onApprove(entry); onClose(); }} disabled={!entry || entry.status === "approved"}>Zeit freigeben</button>
      <button type="button" onClick={() => { onPreparePayroll(assignment); onClose(); }} disabled={!entry || entry.status !== "approved"}>Payroll vorbereiten</button>
      <button type="button" onClick={() => { onPrepareInvoice(assignment); onClose(); }} disabled={!entry || entry.status !== "approved"}>Rechnung vorbereiten</button>
      <button type="button" onClick={() => { onPrepareInvoiceDraft(assignment); onClose(); }} disabled={!entry || entry.status !== "approved"}>Rechnungsdraft erstellen</button>
      <button type="button" onClick={() => { onArchive(assignment); onClose(); }}>Archivieren</button>
    </div>
  );
}

function scoreFor(snapshot: WorkforceSnapshot, assignmentId: string) {
  return snapshot.scores.find((score) => score.assignmentId === assignmentId) ?? {
    assignmentId,
    percent: 0,
    basis: 0,
    leistung: 0,
    bonus: 0,
    checks: []
  };
}

function weekDays(start: string) {
  return Array.from({ length: 5 }, (_, index) => addDays(start, index));
}

function addDays(date: string, days: number) {
  const value = new Date(`${date}T00:00:00.000Z`);
  value.setUTCDate(value.getUTCDate() + days);
  return value.toISOString().slice(0, 10);
}

function formatDate(date: string) {
  return new Date(`${date}T00:00:00.000Z`).toLocaleDateString("de-DE", { day: "2-digit", month: "2-digit" });
}

function personName(snapshot: WorkforceSnapshot, personId: string) {
  return snapshot.people.find((person) => person.id === personId)?.name ?? "Unbekannt";
}

function initials(name: string) {
  return name.split(/\s+/).map((part) => part[0]).join("").slice(0, 2).toUpperCase();
}
