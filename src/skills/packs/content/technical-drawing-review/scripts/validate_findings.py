#!/usr/bin/env python3
"""Validate basic technical drawing review finding JSON."""

import json
import sys
from pathlib import Path


SEVERITIES = {"critical", "major", "minor", "info"}
CATEGORIES = {
    "metadata",
    "dimensioning",
    "tolerance",
    "gd_and_t",
    "material_finish",
    "manufacturing",
    "inspection",
    "consistency",
    "standards",
    "needs_context",
}
STATUSES = {"open", "needs_context", "resolved", "false_positive"}


def fail(message: str) -> None:
    print(f"ERROR: {message}", file=sys.stderr)
    raise SystemExit(1)


def require(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def validate_pin(pin: object, finding_id: str) -> None:
    require(isinstance(pin, dict), f"{finding_id}: pin must be an object")
    require(isinstance(pin.get("page"), int) and pin["page"] >= 1, f"{finding_id}: pin.page must be >= 1")
    for axis in ("x", "y"):
        value = pin.get(axis)
        require(isinstance(value, (int, float)), f"{finding_id}: pin.{axis} must be numeric")
        require(0 <= float(value) <= 1, f"{finding_id}: pin.{axis} must be between 0 and 1")
    require(isinstance(pin.get("anchor"), str) and pin["anchor"].strip(), f"{finding_id}: pin.anchor is required")


def validate(path: Path) -> None:
    data = json.loads(path.read_text(encoding="utf-8"))
    require(isinstance(data, dict), "root must be an object")
    findings = data.get("findings")
    require(isinstance(findings, list), "root.findings must be a list")

    seen = set()
    for index, finding in enumerate(findings, start=1):
        require(isinstance(finding, dict), f"finding {index}: must be an object")
        finding_id = finding.get("id")
        require(isinstance(finding_id, str) and finding_id.strip(), f"finding {index}: id is required")
        require(finding_id not in seen, f"{finding_id}: duplicate id")
        seen.add(finding_id)

        require(finding.get("severity") in SEVERITIES, f"{finding_id}: invalid severity")
        require(finding.get("category") in CATEGORIES, f"{finding_id}: invalid category")
        require(finding.get("status", "open") in STATUSES, f"{finding_id}: invalid status")
        for field in ("title", "evidence", "risk", "recommendation"):
            require(isinstance(finding.get(field), str) and finding[field].strip(), f"{finding_id}: {field} is required")

        confidence = finding.get("confidence")
        require(isinstance(confidence, (int, float)), f"{finding_id}: confidence must be numeric")
        require(0 <= float(confidence) <= 1, f"{finding_id}: confidence must be between 0 and 1")
        validate_pin(finding.get("pin"), finding_id)


def main() -> None:
    if len(sys.argv) != 2:
        fail("usage: validate_findings.py findings.json")
    validate(Path(sys.argv[1]))
    print("Technical drawing review JSON is valid.")


if __name__ == "__main__":
    main()
