#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[4]


def canonical_onboarding_dedupe_key(system: str) -> str:
    return f"onboarding-guide:{system}"


def load_env_file(path: Path) -> dict[str, str]:
    env: dict[str, str] = {}
    if not path.exists():
        return env
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        env[key.strip()] = value.strip().strip('"').strip("'")
    return env


def run_json(command: list[str], env: dict[str, str]) -> dict[str, Any]:
    completed = subprocess.run(command, cwd=REPO_ROOT, env=env, check=True, capture_output=True, text=True)
    stdout = completed.stdout.strip()
    return json.loads(stdout) if stdout else {}


def run_plain(command: list[str], env: dict[str, str]) -> None:
    subprocess.run(command, cwd=REPO_ROOT, env=env, check=True)


def current_state(ctox_bin: str, system: str, env: dict[str, str]) -> dict[str, Any]:
    source_skills = run_json([ctox_bin, "ticket", "source-skills", "--system", system], env)
    bindings = source_skills.get("source_skills", []) if isinstance(source_skills, dict) else []
    active_binding = next((b for b in bindings if b.get("status") == "active"), None)

    self_work = run_json([ctox_bin, "ticket", "self-work-list", "--system", system, "--limit", "200"], env)
    items = self_work.get("items", []) if isinstance(self_work, dict) else []
    onboarding_item = next(
        (
            item
            for item in items
            if item.get("kind") == "ticket-system-onboarding"
            and isinstance(item.get("metadata"), dict)
            and item["metadata"].get("dedupe_key") == canonical_onboarding_dedupe_key(system)
            and item.get("state") not in {"closed", "rejected"}
        ),
        None,
    )
    def belongs_to_source(item: dict[str, Any]) -> bool:
        metadata = item.get("metadata")
        if not isinstance(metadata, dict):
            return False
        ticket_key = str(metadata.get("ticket_key") or "").strip()
        return ticket_key.startswith(f"{system}:")

    validation_count = sum(
        1 for item in items if item.get("kind") == "knowledge-validation" and belongs_to_source(item)
    )
    execution_gap_item = next(
        (
            item
            for item in items
            if item.get("kind") == "execution-enrichment-review" and item.get("state") not in {"closed", "rejected"}
        ),
        None,
    )
    desk_refinement_item = next(
        (
            item
            for item in items
            if item.get("kind") == "desk-skill-refinement-review" and item.get("state") not in {"closed", "rejected"}
        ),
        None,
    )
    validation_expansion_item = next(
        (
            item
            for item in items
            if item.get("kind") == "validation-expansion-review" and item.get("state") not in {"closed", "rejected"}
        ),
        None,
    )
    return {
        "active_binding": active_binding,
        "onboarding_item": onboarding_item,
        "validation_count": validation_count,
        "execution_gap_item": execution_gap_item,
        "desk_refinement_item": desk_refinement_item,
        "validation_expansion_item": validation_expansion_item,
    }


def ensure_guide(ctox_bin: str, system: str, env: dict[str, str], publish: bool) -> None:
    guide_script = REPO_ROOT / "skills/system/ticket-system-onboarding/scripts/upsert_onboarding_guide.py"
    command = [
        sys.executable,
        str(guide_script),
        "--ctox-bin",
        ctox_bin,
        "--system",
        system,
        "--assign-self",
    ]
    if publish:
        command.extend(["--publish", "--env-file", env.get("CTOX_ONBOARDING_ENV_FILE", "")])
    subprocess.run(command, cwd=REPO_ROOT, env=env, check=True)


def ensure_validation_expansion_work(
    ctox_bin: str,
    system: str,
    env: dict[str, str],
    publish: bool,
) -> str | None:
    metadata = {
        "skill": "ticket-system-onboarding",
        "kind": "validation-expansion-review",
        "goal": "collect more validated runbook applications and correction evidence",
        "dedupe_key": f"validation-expansion:{system}",
    }
    result = run_json(
        [
            ctox_bin,
            "ticket",
            "self-work-put",
            "--system",
            system,
            "--kind",
            "validation-expansion-review",
            "--title",
            f"CTOX: weitere bestaetigte Runbook-Anwendungen fuer {system} sammeln",
            "--body",
            (
                f"CTOX hat fuer {system} den ersten bestaetigten Ausfuehrungspfad. "
                "Naechster Schritt ist, weitere bestaetigte Anwendungen und mindestens einen sichtbaren Gegenfall zu sammeln, bevor der Leitfaden in bounded autonomy uebergeht."
            ),
            "--skill",
            "ticket-system-onboarding",
            "--metadata-json",
            json.dumps(metadata, ensure_ascii=False),
            *(["--publish"] if publish else []),
        ],
        env,
    )
    work_id = result.get("item", {}).get("work_id")
    if work_id:
        try:
            run_json(
                [ctox_bin, "ticket", "self-work-assign", "--work-id", work_id, "--assignee", "self", "--assigned-by", "ctox"],
                env,
            )
        except subprocess.CalledProcessError:
            pass
        run_json(
            [
                ctox_bin,
                "ticket",
                "self-work-note",
                "--work-id",
                work_id,
                "--body",
                "Der erste bestaetigte Runbook-Pfad ist da. Jetzt fehlen weitere bestaetigte Anwendungen und sichtbare Gegenfaelle, damit CTOX die supervised Phase sauber verlaesst.",
                "--authored-by",
                "ctox",
                "--visibility",
                "internal",
            ],
            env,
        )
    return work_id


def ensure_execution_gap_work(ctox_bin: str, system: str, env: dict[str, str], publish: bool) -> None:
    metadata = {
        "skill": "ticket-system-onboarding",
        "kind": "execution-enrichment-review",
        "goal": "build first source-specific execution supplement",
        "dedupe_key": f"execution-gap:{system}",
    }
    command = [
        ctox_bin,
        "ticket",
        "self-work-put",
        "--system",
        system,
        "--kind",
        "execution-enrichment-review",
        "--title",
        f"CTOX: erste Execution-Quelle fuer {system} anreichern",
        "--body",
        (
            f"CTOX hat fuer {system} bereits den ersten Desk-Pfad, aber noch keine belastbare Execution-Quelle. "
            "Naechster Schritt ist genau eine erste family-spezifische Execution-Ergaenzung mit klaren Tool-Aktionen, Verifikation und Writeback-Grenze."
        ),
        "--skill",
        "ticket-system-onboarding",
        "--metadata-json",
        json.dumps(metadata, ensure_ascii=False),
    ]
    if publish:
        command.append("--publish")
    result = run_json(command, env)
    work_id = result.get("item", {}).get("work_id")
    if work_id:
        try:
            run_json([ctox_bin, "ticket", "self-work-assign", "--work-id", work_id, "--assignee", "self", "--assigned-by", "ctox"], env)
        except subprocess.CalledProcessError:
            pass
        run_json(
            [
                ctox_bin,
                "ticket",
                "self-work-note",
                "--work-id",
                work_id,
                "--body",
                "Der Desk-Pfad ist vorhanden. Es fehlt jetzt genau eine echte Execution-Quelle fuer die erste source-spezifische Familie.",
                "--authored-by",
                "ctox",
                "--visibility",
                "internal",
            ],
            env,
        )


def ensure_desk_skill_refinement_work(
    ctox_bin: str,
    system: str,
    env: dict[str, str],
    publish: bool,
    blocker_summary: str,
) -> str | None:
    metadata = {
        "skill": "ticket-system-onboarding",
        "kind": "desk-skill-refinement-review",
        "goal": "raise first desk skill to activation quality",
        "dedupe_key": f"desk-skill-refinement:{system}",
    }
    result = run_json(
        [
            ctox_bin,
            "ticket",
            "self-work-put",
            "--system",
            system,
            "--kind",
            "desk-skill-refinement-review",
            "--title",
            f"CTOX: ersten Desk-Skill fuer {system} aktivierbar machen",
            "--body",
            (
                f"Der erste Desk-Skill fuer {system} ist noch nicht aktivierbar. "
                f"Blocker: {blocker_summary}"
            ),
            "--skill",
            "ticket-system-onboarding",
            "--metadata-json",
            json.dumps(metadata, ensure_ascii=False),
            *(["--publish"] if publish else []),
        ],
        env,
    )
    work_id = result.get("item", {}).get("work_id")
    if work_id:
        try:
            run_json([ctox_bin, "ticket", "self-work-assign", "--work-id", work_id, "--assignee", "self", "--assigned-by", "ctox"], env)
        except subprocess.CalledProcessError:
            pass
        run_json(
            [
                ctox_bin,
                "ticket",
                "self-work-note",
                "--work-id",
                work_id,
                "--body",
                blocker_summary,
                "--authored-by",
                "ctox",
                "--visibility",
                "internal",
            ],
            env,
        )
    return work_id


def close_work_item(
    ctox_bin: str,
    env: dict[str, str],
    work_id: str,
    note: str,
) -> None:
    run_json(
        [
            ctox_bin,
            "ticket",
            "self-work-transition",
            "--work-id",
            work_id,
            "--state",
            "closed",
            "--transitioned-by",
            "ctox",
            "--note",
            note,
            "--visibility",
            "internal",
        ],
        env,
    )


def main() -> None:
    parser = argparse.ArgumentParser(description="Execute the deterministic ticket-system onboarding plan stage by stage.")
    parser.add_argument("--ctox-bin", default="ctox")
    parser.add_argument("--system", required=True)
    parser.add_argument("--env-file")
    parser.add_argument("--dataset-label")
    parser.add_argument("--goal", default="handle tickets in the historically observed desk style")
    parser.add_argument("--skill-name")
    parser.add_argument("--analysis-dir")
    parser.add_argument("--publish", action="store_true")
    parser.add_argument("--stop-after-stage", type=int)
    args = parser.parse_args()

    env = os.environ.copy()
    if args.env_file:
        env["CTOX_ONBOARDING_ENV_FILE"] = args.env_file
        env.update(load_env_file(Path(args.env_file)))

    skill_name = args.skill_name or f"{args.system}-desk-operator"
    analysis_dir = args.analysis_dir or f"runtime/output/{args.system}_desk_skill"
    dataset_label = args.dataset_label or f"{args.system} mirrored ticket history"

    completed_stages: list[int] = []

    run_plain([args.ctox_bin, "ticket", "test", "--system", args.system], env)
    run_json([args.ctox_bin, "ticket", "capabilities", "--system", args.system], env)
    run_json([args.ctox_bin, "ticket", "sync", "--system", args.system], env)
    ensure_guide(args.ctox_bin, args.system, env, args.publish)
    completed_stages.append(1)
    if args.stop_after_stage == 1:
        print(json.dumps({"ok": True, "completed_stages": completed_stages}, ensure_ascii=False, indent=2))
        return

    run_json([args.ctox_bin, "ticket", "knowledge-list", "--system", args.system, "--limit", "20"], env)
    run_json([args.ctox_bin, "ticket", "self-work-list", "--system", args.system, "--limit", "50"], env)
    completed_stages.append(2)
    if args.stop_after_stage == 2:
        print(json.dumps({"ok": True, "completed_stages": completed_stages}, ensure_ascii=False, indent=2))
        return

    state = current_state(args.ctox_bin, args.system, env)
    if state["active_binding"] is None:
        bootstrap_command = [
            sys.executable,
            str(REPO_ROOT / "skills/system/ticket-system-onboarding/scripts/bootstrap_ticket_source_skill.py"),
            "--system",
            args.system,
            "--skill-name",
            skill_name,
            "--dataset-label",
            dataset_label,
            "--goal",
            args.goal,
            "--analysis-dir",
            analysis_dir,
            "--ctox-bin",
            args.ctox_bin,
        ]
        bootstrap_result = subprocess.run(
            bootstrap_command,
            cwd=REPO_ROOT,
            env=env,
            check=False,
            capture_output=True,
            text=True,
        )
        if bootstrap_result.returncode != 0:
            state = current_state(args.ctox_bin, args.system, env)
            blocker_summary = bootstrap_result.stderr.strip().splitlines()[-1] if bootstrap_result.stderr.strip() else "Desk-Skill-Aktivierung fehlgeschlagen."
            onboarding_item = state.get("onboarding_item")
            if onboarding_item:
                run_json(
                    [
                        args.ctox_bin,
                        "ticket",
                        "self-work-note",
                        "--work-id",
                        onboarding_item["work_id"],
                        "--body",
                        f"Stage 3 blockiert. {blocker_summary}",
                        "--authored-by",
                        "ctox",
                        "--visibility",
                        "internal",
                    ],
                    env,
                )
            refinement_work_id = ensure_desk_skill_refinement_work(
                args.ctox_bin,
                args.system,
                env,
                args.publish,
                blocker_summary,
            )
            print(
                json.dumps(
                    {
                        "ok": False,
                        "blocked_stage": 3,
                        "completed_stages": completed_stages,
                        "blocker": blocker_summary,
                        "refinement_work_id": refinement_work_id,
                    },
                    ensure_ascii=False,
                    indent=2,
                )
            )
            raise SystemExit(0)
        state = current_state(args.ctox_bin, args.system, env)
        if state["active_binding"] is None:
            blocker_summary = "Desk-Skill-Build lief durch, aber es wurde keine aktive Source-Skill-Bindung sichtbar."
            onboarding_item = state.get("onboarding_item")
            if onboarding_item:
                run_json(
                    [
                        args.ctox_bin,
                        "ticket",
                        "self-work-note",
                        "--work-id",
                        onboarding_item["work_id"],
                        "--body",
                        f"Stage 3 blockiert. {blocker_summary}",
                        "--authored-by",
                        "ctox",
                        "--visibility",
                        "internal",
                    ],
                    env,
                )
            refinement_work_id = ensure_desk_skill_refinement_work(
                args.ctox_bin,
                args.system,
                env,
                args.publish,
                blocker_summary,
            )
            print(
                json.dumps(
                    {
                        "ok": False,
                        "blocked_stage": 3,
                        "completed_stages": completed_stages,
                        "blocker": blocker_summary,
                        "refinement_work_id": refinement_work_id,
                    },
                    ensure_ascii=False,
                    indent=2,
                )
            )
            raise SystemExit(0)
    ensure_guide(args.ctox_bin, args.system, env, args.publish)
    completed_stages.append(3)
    state = current_state(args.ctox_bin, args.system, env)
    if state["desk_refinement_item"] is not None:
        close_work_item(
            args.ctox_bin,
            env,
            state["desk_refinement_item"]["work_id"],
            "Der erste Desk-Skill ist jetzt aktiv. Dieser fruehere Stage-3-Blocker ist erledigt und wird durch den aktiven Source-Skill abgeloest.",
        )
    if args.stop_after_stage == 3:
        print(json.dumps({"ok": True, "completed_stages": completed_stages}, ensure_ascii=False, indent=2))
        return

    state = current_state(args.ctox_bin, args.system, env)
    if state["validation_count"] == 0 and state["execution_gap_item"] is None:
        ensure_execution_gap_work(args.ctox_bin, args.system, env, args.publish)
    ensure_guide(args.ctox_bin, args.system, env, args.publish)
    completed_stages.append(4)
    if args.stop_after_stage == 4:
        print(json.dumps({"ok": True, "completed_stages": completed_stages}, ensure_ascii=False, indent=2))
        return

    state = current_state(args.ctox_bin, args.system, env)
    if state["validation_count"] > 0:
        if state["execution_gap_item"] is not None:
            close_work_item(
                args.ctox_bin,
                env,
                state["execution_gap_item"]["work_id"],
                "Die erste source-spezifische Execution-Ergaenzung ist bestaetigt. Dieser fruehe Execution-Gap-Arbeitspunkt ist damit erledigt.",
            )
            state = current_state(args.ctox_bin, args.system, env)
        if state["validation_count"] < 3 and state["validation_expansion_item"] is None:
            ensure_validation_expansion_work(args.ctox_bin, args.system, env, args.publish)
        ensure_guide(args.ctox_bin, args.system, env, args.publish)
        completed_stages.append(5)
        state = current_state(args.ctox_bin, args.system, env)
        if state["validation_count"] >= 3:
            completed_stages.append(6)
    print(
        json.dumps(
            {
                "ok": True,
                "completed_stages": completed_stages,
                "active_source_skill": state["active_binding"]["skill_name"] if state["active_binding"] else None,
                "validation_count": state["validation_count"],
                "execution_gap_item": state["execution_gap_item"]["work_id"] if state["execution_gap_item"] else None,
            },
            ensure_ascii=False,
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
