#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import subprocess
from pathlib import Path
from typing import Any


def load_env_file(path: Path) -> dict[str, str]:
    env: dict[str, str] = {}
    if not path.exists():
        return env
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip().strip('"').strip("'")
        if key:
            env[key] = value
    return env


def run_ctox(ctox_bin: str, args: list[str], env_overrides: dict[str, str] | None = None) -> dict[str, Any]:
    env = os.environ.copy()
    if env_overrides:
        env.update(env_overrides)
    completed = subprocess.run(
        [ctox_bin, *args],
        check=True,
        capture_output=True,
        text=True,
        env=env,
    )
    stdout = completed.stdout.strip()
    if not stdout:
        return {}
    return json.loads(stdout)


def canonical_dedupe_key(system: str) -> str:
    return f"onboarding-guide:{system}"


def close_legacy_guide(
    ctox_bin: str,
    legacy_work_id: str,
    canonical_work_id: str,
    env_overrides: dict[str, str] | None,
) -> None:
    run_ctox(
        ctox_bin,
        [
            "ticket",
            "self-work-transition",
            "--work-id",
            legacy_work_id,
            "--state",
            "closed",
            "--transitioned-by",
            "ctox",
            "--note",
            (
                "Dieser fruehere Leitfaden ist ersetzt. "
                f"Der kanonische Onboarding-Leitfaden laeuft jetzt unter {canonical_work_id} weiter."
            ),
            "--visibility",
            "internal",
        ],
        env_overrides,
    )


def derive_phase(active_skill: str | None, assigned_work_count: int, validation_count: int) -> tuple[str, str, str]:
    if not active_skill:
        return (
            "observe",
            "scripted",
            "Erste aktive Desk-Skill-Bindung herstellen, bevor CTOX echte Ticketarbeit uebernimmt.",
        )
    if validation_count == 0:
        return (
            "desk-guided",
            "scripted-with-review",
            "Erste bestaetigte Runbook-Anwendung erzeugen, damit CTOX ueber reine Desk-Fuehrung hinausgeht.",
        )
    if validation_count < 3:
        return (
            "controlled-execution",
            "supervised",
            "Mehrere bestaetigte Runbook-Anwendungen sammeln und Gegenfaelle sichtbar korrigieren.",
        )
    return (
        "bounded-autonomy",
        "bounded-autonomy",
        "Den Guide nur noch fuer Korrektur, Erweiterung und Ausnahmefaelle offen halten; normale Arbeit laeuft jetzt ueber den gebundenen Source-Skill.",
    )


def phase_summary(phase: str) -> str:
    if phase == "observe":
        return "CTOX beobachtet den Desk noch gefuehrt und arbeitet vor allem an Spiegelung, Desk-Verstehen und den ersten Skill-/Runbook-Kandidaten."
    if phase == "desk-guided":
        return "CTOX fuehrt Ticketarbeit jetzt entlang eines aktiven Desk-Skills, bleibt fuer echte Ausfuehrung aber noch eng am Onboarding-Leitfaden."
    if phase == "controlled-execution":
        return "CTOX arbeitet erste reale Faelle in kontrollierten Schleifen ab und bestaetigt oder korrigiert Runbooks an echter Anwendung."
    return "CTOX arbeitet fuer diesen Source bereits weitgehend ueber den normalen Ticketpfad; der Onboarding-Leitfaden bleibt nur noch als sichtbare Sicherheits- und Korrekturspur offen."


def main() -> None:
    parser = argparse.ArgumentParser(description="Create or advance the visible onboarding guide for a ticket source.")
    parser.add_argument("--ctox-bin", default="ctox")
    parser.add_argument("--system", required=True)
    parser.add_argument("--env-file")
    parser.add_argument("--publish", action="store_true")
    parser.add_argument("--assign-self", action="store_true")
    parser.add_argument("--close-when-autonomous", action="store_true")
    args = parser.parse_args()

    env_overrides = load_env_file(Path(args.env_file)) if args.env_file else None
    source_skills = run_ctox(args.ctox_bin, ["ticket", "source-skills", "--system", args.system], env_overrides)
    bindings = source_skills.get("source_skills", []) if isinstance(source_skills, dict) else []
    active_binding = next((b for b in bindings if b.get("status") == "active"), None)
    active_skill = active_binding.get("skill_name") if isinstance(active_binding, dict) else None

    self_work = run_ctox(
        args.ctox_bin,
        ["ticket", "self-work-list", "--system", args.system, "--limit", "200"],
        env_overrides,
    )
    items = self_work.get("items", []) if isinstance(self_work, dict) else []
    onboarding_item = next(
        (
            item for item in items
            if item.get("kind") == "ticket-system-onboarding"
            and isinstance(item.get("metadata"), dict)
            and item["metadata"].get("dedupe_key") == canonical_dedupe_key(args.system)
        ),
        None,
    )
    legacy_onboarding_item = next(
        (
            item for item in items
            if item.get("kind") == "ticket-system-onboarding"
        ),
        None,
    )
    def belongs_to_source(item: dict[str, Any]) -> bool:
        metadata = item.get("metadata")
        if not isinstance(metadata, dict):
            return False
        ticket_key = str(metadata.get("ticket_key") or "").strip()
        return ticket_key.startswith(f"{args.system}:")

    assigned_work_count = sum(
        1
        for item in items
        if item.get("kind") != "ticket-system-onboarding"
        and item.get("assigned_to")
        and item.get("state") not in {"closed", "rejected"}
    )
    validation_count = sum(
        1 for item in items if item.get("kind") == "knowledge-validation" and belongs_to_source(item)
    )

    phase, guide_mode, next_gate = derive_phase(active_skill, assigned_work_count, validation_count)
    title = f"CTOX: Onboarding-Leitfaden fuer {args.system}"
    body = "\n".join(
        [
            f"CTOX arbeitet sich kontrolliert in {args.system} ein.",
            f"Aktuelle Phase: {phase}.",
            phase_summary(phase),
            "",
            "Freigabegrenze:",
            f"- {next_gate}",
        ]
    )
    metadata = {
        "skill": "ticket-system-onboarding",
        "phase": phase,
        "guide_mode": guide_mode,
        "active_source_skill": active_skill,
        "assigned_work_count": assigned_work_count,
        "validation_count": validation_count,
        "dedupe_key": canonical_dedupe_key(args.system),
    }

    if onboarding_item is None:
        created = run_ctox(
            args.ctox_bin,
            [
                "ticket",
                "self-work-put",
                "--system",
                args.system,
                "--kind",
                "ticket-system-onboarding",
                "--title",
                title,
                "--body",
                body,
                "--skill",
                "ticket-system-onboarding",
                "--metadata-json",
                json.dumps(metadata, ensure_ascii=False),
                *(["--publish"] if args.publish else []),
            ],
            env_overrides,
        )
        work_id = created["item"]["work_id"]
        if legacy_onboarding_item and legacy_onboarding_item.get("work_id") != work_id:
            close_legacy_guide(
                args.ctox_bin,
                legacy_onboarding_item["work_id"],
                work_id,
                env_overrides,
            )
        if args.assign_self:
            try:
                run_ctox(
                    args.ctox_bin,
                    ["ticket", "self-work-assign", "--work-id", work_id, "--assignee", "self", "--assigned-by", "ctox"],
                    env_overrides,
                )
            except subprocess.CalledProcessError:
                pass
        run_ctox(
            args.ctox_bin,
            [
                "ticket",
                "self-work-note",
                "--work-id",
                work_id,
                "--body",
                f"Onboarding gestartet. Phase {phase}. Naechste Freigabe: {next_gate}",
                "--authored-by",
                "ctox",
                "--visibility",
                "internal",
            ],
            env_overrides,
        )
        print(json.dumps({"ok": True, "action": "created", "work_id": work_id, "phase": phase, "guide_mode": guide_mode}, ensure_ascii=False, indent=2))
        return

    work_id = onboarding_item["work_id"]
    if legacy_onboarding_item and legacy_onboarding_item.get("work_id") != work_id:
        legacy_state = legacy_onboarding_item.get("state")
        if legacy_state not in {"closed", "rejected"}:
            close_legacy_guide(
                args.ctox_bin,
                legacy_onboarding_item["work_id"],
                work_id,
                env_overrides,
            )
    guide_state = run_ctox(args.ctox_bin, ["ticket", "self-work-show", "--work-id", work_id], env_overrides)
    notes = guide_state.get("notes", []) if isinstance(guide_state, dict) else []
    previous_phase = (
        onboarding_item.get("metadata", {}).get("phase")
        if isinstance(onboarding_item.get("metadata"), dict)
        else None
    )
    note_body = f"Onboarding-Stand aktualisiert. Phase {phase}. Aktiver Source-Skill: {active_skill or 'noch keiner'}. Bestaetigte Runbook-Anwendungen: {validation_count}. Selbst uebernommene Arbeitspunkte: {assigned_work_count}. Naechste Freigabe: {next_gate}"
    if previous_phase != phase:
        note_body = f"Onboarding-Phase gewechselt: {previous_phase or 'unbekannt'} -> {phase}. {next_gate}"
    latest_note_body = notes[-1]["body_text"] if notes else None
    persisted_item = guide_state.get("item", {}) if isinstance(guide_state, dict) else {}
    persisted_metadata = (
        persisted_item.get("metadata") if isinstance(persisted_item.get("metadata"), dict) else {}
    )
    body_unchanged = persisted_item.get("body_text") == body
    metadata_unchanged = persisted_metadata == metadata
    if latest_note_body == note_body and body_unchanged and metadata_unchanged:
        print(json.dumps({"ok": True, "action": "noop", "work_id": work_id, "phase": phase, "guide_mode": guide_mode}, ensure_ascii=False, indent=2))
        return
    run_ctox(
        args.ctox_bin,
        [
            "ticket",
            "self-work-put",
            "--system",
            args.system,
            "--kind",
            "ticket-system-onboarding",
            "--title",
            title,
            "--body",
            body,
            "--skill",
            "ticket-system-onboarding",
            "--metadata-json",
            json.dumps(metadata, ensure_ascii=False),
        ],
        env_overrides,
    )
    run_ctox(
        args.ctox_bin,
        [
            "ticket",
            "self-work-note",
            "--work-id",
            work_id,
            "--body",
            note_body,
            "--authored-by",
            "ctox",
            "--visibility",
            "internal",
        ],
        env_overrides,
    )
    if args.close_when_autonomous and phase == "bounded-autonomy":
        run_ctox(
            args.ctox_bin,
            [
                "ticket",
                "self-work-transition",
                "--work-id",
                work_id,
                "--state",
                "closed",
                "--transitioned-by",
                "ctox",
                "--note",
                "Der Onboarding-Leitfaden wird geschlossen. Der normale Ticketpfad laeuft jetzt ueber den aktiven Source-Skill; der Onboarding-Skill bleibt nur noch fuer Korrektur und Ausnahmefaelle relevant.",
                "--visibility",
                "internal",
            ],
            env_overrides,
        )
        print(json.dumps({"ok": True, "action": "closed", "work_id": work_id, "phase": phase, "guide_mode": guide_mode}, ensure_ascii=False, indent=2))
        return

    print(json.dumps({"ok": True, "action": "updated", "work_id": work_id, "phase": phase, "guide_mode": guide_mode}, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
