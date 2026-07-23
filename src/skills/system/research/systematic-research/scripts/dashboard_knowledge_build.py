#!/usr/bin/env python3
"""Deterministic builder for native Business OS research writeback tables.

This script replaces model-authored ``dashboard/knowledge/*.csv`` artifacts.
It builds ``source_candidates.csv``, ``source_catalog.csv``,
``measured_load_points.csv`` and ``derived_bearing_loads.csv`` so they match
the native importer contract in ``src/core/business_os/store.rs``
(``validate_systematic_research_csv``) exactly:

- ``measured_load_points`` rows are direct measurements only
  (``measurement_kind`` in {measured, direct, experimental},
  ``is_derived=false``), with positive RPM, an explicit newton axial-force
  channel, numeric propeller diameter/pitch columns, and source-row lineage.
- Any conversion (CT/CP -> force/torque, lbf -> N, thrust -> radial bearing
  load) is derived and belongs in ``derived_bearing_loads`` with formula,
  constants, assumptions, units and source-row lineage. Measured axial thrust
  is never mixed with an inferred radial bearing load.
- ``source_candidates`` accumulates every discovery candidate from every
  round with canonical dedup (URL / DOI / stable id / content hash) and an
  explicit per-candidate rejection reason. Rejected candidates are never
  promoted into ``source_catalog``.
- Source row-count reconciliation is fail-closed: every parsed source row is
  either emitted or dropped with an explicit machine-readable reason, and the
  reconciliation counts must add up or the build aborts.

ENOLA member CSVs with malformed headers (for example a missing delimiter
between ``THRUST[N]`` and ``u_THRUST[N]``) are repaired only through the
deterministic, audited parser rule below. The rule splits concatenated
bracketed column labels; it never invents, interpolates, or estimates a
measurement value.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import re
import sys
from pathlib import Path
from typing import Any, Iterable

SCHEMA_NOTE = "native contract mirror: src/core/business_os/store.rs validate_systematic_research_csv"

MEASUREMENT_KINDS = {"measured", "direct", "experimental"}

MEASURED_LOAD_POINTS_HEADERS = [
    "research_run_id",
    "research_command_id",
    "source_id",
    "canonical_url",
    "snapshot_hash",
    "source_row_ref",
    "measurement_kind",
    "is_derived",
    "rpm",
    "propeller_size",
    "prop_diameter_in",
    "prop_pitch_in",
    "thrust_N",
    "torque_Nm",
    "u_rpm",
    "u_thrust_N",
    "u_torque_Nm",
    "rpm_unit",
    "thrust_unit",
    "torque_unit",
    "CT",
    "CP",
    "archive_manifest_hash",
    "archive_member_path",
    "archive_member_hash",
    "parsing_rule",
]

DERIVED_BEARING_LOADS_HEADERS = [
    "research_run_id",
    "research_command_id",
    "claim_id",
    "evidence_id",
    "source_id",
    "snapshot_id",
    "canonical_url",
    "snapshot_hash",
    "quote",
    "source_row_ref",
    "derivation_method",
    "assumption_text",
    "is_derived",
    "thrust_N",
    "torque_Nm",
    "bearing_radial_load_N",
    "formula",
    "constants",
    "units",
    "archive_manifest_hash",
    "archive_member_path",
    "archive_member_hash",
]

SOURCE_CANDIDATES_HEADERS = [
    "research_run_id",
    "research_command_id",
    "candidate_key",
    "title",
    "url",
    "doi",
    "openalex_id",
    "focus",
    "query",
    "snippet",
    "source_class",
    "verification_state",
    "rejection_reason",
    "discovery_rounds",
    "content_hash",
]

SOURCE_CATALOG_HEADERS = [
    "research_run_id",
    "research_command_id",
    "source_id",
    "canonical_url",
    "snapshot_hash",
    "evidence_id",
    "snapshot_id",
    "relevance_score",
]

ENOLA_HEADER_REPAIR_RULE = "enola_missing_delimiter_between_bracketed_columns"
ENOLA_PARSER_VERSION = "enola-member-csv/1"

DOI_PATTERN = re.compile(r"^10\.\d{4,9}/\S+$", re.IGNORECASE)
PROPELLER_PATTERN = re.compile(r"(\d+(?:\.\d+)?)\s*x\s*(\d+(?:\.\d+)?)", re.IGNORECASE)
# A merged header token: two bracketed labels concatenated without a delimiter,
# e.g. "THRUST[N]u_THRUST[N]". The second label starts right after the "]".
MERGED_HEADER_TOKEN = re.compile(r"^(.+?\[[^\]]*\])([A-Za-z_][A-Za-z0-9_ .\-/]*\[[^\]]*\])$")


class BuildError(ValueError):
    """A writeback build invariant failed; the build must abort fail-closed."""


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def normalize_doi(raw: Any) -> str:
    if not isinstance(raw, str):
        return ""
    value = raw.strip()
    value = re.sub(r"^https?://(dx\.)?doi\.org/", "", value, flags=re.IGNORECASE)
    value = re.sub(r"^doi:\s*", "", value, flags=re.IGNORECASE).rstrip(".,;)")
    return value if DOI_PATTERN.fullmatch(value) else ""


def canonical_url_key(raw: Any) -> str:
    """Canonical URL identity used for candidate dedup (scheme/host case,
    default ports, duplicate slashes, trailing slash)."""

    if not isinstance(raw, str) or not raw.strip():
        return ""
    from urllib.parse import urlparse

    parsed = urlparse(raw.strip())
    host = (parsed.hostname or "").lower().rstrip(".")
    if parsed.scheme.lower() not in {"http", "https"} or not host:
        return ""
    port = parsed.port
    authority = host
    if port is not None and not (
        (parsed.scheme.lower() == "http" and port == 80)
        or (parsed.scheme.lower() == "https" and port == 443)
    ):
        authority = f"{host}:{port}"
    path = re.sub(r"/{2,}", "/", parsed.path or "/")
    if path != "/":
        path = path.rstrip("/")
    query = f"?{parsed.query}" if parsed.query else ""
    return f"{parsed.scheme.lower()}://{authority}{path}{query}"


def candidate_content_hash(row: dict[str, str]) -> str:
    payload = "\n".join(
        [
            re.sub(r"\s+", " ", str(row.get("title") or "")).strip().lower(),
            re.sub(r"\s+", " ", str(row.get("snippet") or "")).strip().lower(),
        ]
    )
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()


def candidate_key(row: dict[str, str]) -> str:
    """Canonical dedup key: DOI, stable id, canonical URL, then content hash."""

    doi = normalize_doi(row.get("doi") or "")
    if doi:
        return f"doi:{doi.lower()}"
    for field in ("openalex_id", "stable_id", "source_id"):
        value = str(row.get(field) or "").strip()
        if value:
            return f"id:{value.lower()}"
    url_key = canonical_url_key(row.get("url") or row.get("canonical_url"))
    if url_key:
        return f"url:{url_key}"
    title = re.sub(r"\s+", " ", str(row.get("title") or "")).strip()
    if title:
        return f"content:{candidate_content_hash(row)}"
    return ""


def parse_propeller_notation(text: str) -> tuple[float, float]:
    """Split propeller notation such as ``9x5`` or ``APC15x8`` into numeric
    (diameter_in, pitch_in). Fail closed when no explicit notation exists."""

    match = PROPELLER_PATTERN.search(str(text or ""))
    if not match:
        raise BuildError(f"propeller_notation_missing:{text!r}")
    diameter = float(match.group(1))
    pitch = float(match.group(2))
    if diameter <= 0 or pitch <= 0:
        raise BuildError(f"propeller_notation_not_positive:{text!r}")
    return diameter, pitch


def format_number(value: float) -> str:
    """Machine-readable decimal storage: dot separator, no thousands grouping."""

    return f"{value:.6f}"


def parse_source_number(raw: str) -> float | None:
    value = str(raw or "").strip()
    if not value or value.lower() in {"nan", "na", "n/a", "null", "none", "-"}:
        return None
    try:
        number = float(value)
    except ValueError:
        return None
    return number if number == number and number not in (float("inf"), float("-inf")) else None


def detect_delimiter(header_line: str) -> str:
    if header_line.count(";") > header_line.count(","):
        return ";"
    return ","


def repair_enola_header_tokens(
    tokens: list[str],
) -> tuple[list[str], list[dict[str, str]]]:
    """Deterministic repair for concatenated bracketed header labels.

    Rule ``enola_missing_delimiter_between_bracketed_columns``: when one header
    token contains exactly two bracketed column labels concatenated without a
    delimiter (``THRUST[N]u_THRUST[N]``), split it into the two labels. The
    rule only splits labels; it never creates or alters a data value. Every
    application is recorded in the audit trail.
    """

    repaired: list[str] = []
    repairs: list[dict[str, str]] = []
    for token in tokens:
        current = token
        while True:
            match = MERGED_HEADER_TOKEN.match(current)
            if not match:
                break
            left, right = match.group(1), match.group(2)
            repairs.append(
                {
                    "rule": ENOLA_HEADER_REPAIR_RULE,
                    "original_token": current,
                    "repaired_tokens": [left, right],
                }
            )
            repaired.append(left)
            current = right
        repaired.append(current)
    return repaired, repairs


def normalized_column_name(raw: str) -> str:
    return re.sub(r"\s+", "", str(raw or "")).upper()


def map_enola_columns(headers: list[str]) -> dict[str, int]:
    """Map ENOLA header labels to canonical channels. Units must be explicit:
    thrust requires ``[N]``, torque a newton-metre bracket. RPM is the bare
    ``RPM`` column. Unknown extra columns are ignored, never guessed."""

    mapping: dict[str, int] = {}
    for index, header in enumerate(headers):
        name = normalized_column_name(header)
        if name == "RPM":
            mapping.setdefault("rpm", index)
        elif name == "THRUST[N]":
            mapping.setdefault("thrust_N", index)
        elif name in {"TORQUE[NM]", "TORQUE[N*M]", "TORQUE[N.M]", "TORQUE[N·M]"}:
            mapping.setdefault("torque_Nm", index)
        elif name == "CT":
            mapping.setdefault("CT", index)
        elif name == "CP":
            mapping.setdefault("CP", index)
        elif name == "U_RPM":
            mapping.setdefault("u_rpm", index)
        elif name == "U_THRUST[N]":
            mapping.setdefault("u_thrust_N", index)
        elif name in {"U_TORQUE[NM]", "U_TORQUE[N*M]", "U_TORQUE[N.M]", "U_TORQUE[N·M]"}:
            mapping.setdefault("u_torque_Nm", index)
    return mapping


def parse_enola_member_csv(
    text: str,
    member_path: str,
) -> dict[str, Any]:
    """Parse one ENOLA archive member CSV with audited header repair.

    Returns headers, the column mapping, parsed data rows and the audit trail.
    Fails closed when a data row does not match the repaired header width —
    the parser never invents missing measurements or units.
    """

    lines = [line for line in text.replace("\r\n", "\n").replace("\r", "\n").split("\n")]
    lines = [line for line in lines if line.strip()]
    if not lines:
        raise BuildError(f"enola_member_empty:{member_path}")
    header_line = lines[0]
    delimiter = detect_delimiter(header_line)
    raw_tokens = [token.strip() for token in header_line.split(delimiter)]
    headers, repairs = repair_enola_header_tokens(raw_tokens)
    mapping = map_enola_columns(headers)
    for required in ("rpm", "thrust_N"):
        if required not in mapping:
            raise BuildError(f"enola_required_column_missing:{member_path}:{required}")
    if "thrust_N" in mapping:
        label = normalized_column_name(headers[mapping["thrust_N"]])
        if not label.endswith("[N]"):
            raise BuildError(f"enola_unit_missing:{member_path}:thrust")
    if "torque_Nm" in mapping:
        label = normalized_column_name(headers[mapping["torque_Nm"]])
        if "[" not in label:
            raise BuildError(f"enola_unit_missing:{member_path}:torque")

    rows: list[dict[str, Any]] = []
    for line_number, line in enumerate(lines[1:], start=2):
        fields = [field.strip() for field in line.split(delimiter)]
        if len(fields) != len(headers):
            raise BuildError(
                f"enola_row_field_count_mismatch:{member_path}:line {line_number} "
                f"has {len(fields)} fields, repaired header has {len(headers)}"
            )
        rows.append({"line_number": line_number, "fields": fields})

    return {
        "headers": headers,
        "mapping": mapping,
        "rows": rows,
        "delimiter": delimiter,
        "audit": {
            "member_path": member_path,
            "parser": ENOLA_PARSER_VERSION,
            "original_header": header_line,
            "original_header_sha256": hashlib.sha256(header_line.encode("utf-8")).hexdigest(),
            "repaired_header": delimiter.join(headers),
            "repairs": repairs,
            "header_repaired": bool(repairs),
        },
    }


def cell(row: dict[str, Any], mapping: dict[str, int], channel: str) -> str:
    index = mapping.get(channel)
    if index is None:
        return ""
    fields = row["fields"]
    return fields[index] if index < len(fields) else ""


def build_measured_load_points(
    *,
    research_run_id: str,
    research_command_id: str,
    source_id: str,
    canonical_url: str,
    snapshot_hash: str,
    propeller_size: str,
    archive_manifest_hash: str,
    member_path: str,
    member_hash: str,
    parsed: dict[str, Any],
) -> tuple[list[dict[str, str]], dict[str, Any]]:
    """Build native-contract measured rows from a parsed ENOLA member.

    Every row is a direct experimental measurement (``measurement_kind=
    experimental``, ``is_derived=false``). Rows without a positive RPM or a
    machine-readable newton thrust are dropped with an explicit reason in the
    reconciliation report; the build aborts if emitted + dropped does not
    equal the source row count (no partial silent import).
    """

    diameter_in, pitch_in = parse_propeller_notation(propeller_size)
    mapping = parsed["mapping"]
    parsing_rule = (
        ENOLA_HEADER_REPAIR_RULE
        if parsed["audit"]["header_repaired"]
        else ENOLA_PARSER_VERSION
    )
    rows: list[dict[str, str]] = []
    dropped: list[dict[str, Any]] = []
    for source_row in parsed["rows"]:
        line_number = source_row["line_number"]
        row_ref = f"{member_path}#row-{line_number}"
        rpm = parse_source_number(cell(source_row, mapping, "rpm"))
        thrust = parse_source_number(cell(source_row, mapping, "thrust_N"))
        if rpm is None or rpm <= 0:
            dropped.append({"source_row_ref": row_ref, "reason": "rpm_missing_or_not_positive"})
            continue
        if thrust is None:
            dropped.append({"source_row_ref": row_ref, "reason": "thrust_N_missing_or_not_numeric"})
            continue
        torque = parse_source_number(cell(source_row, mapping, "torque_Nm"))
        ct = parse_source_number(cell(source_row, mapping, "CT"))
        cp = parse_source_number(cell(source_row, mapping, "CP"))
        u_rpm = parse_source_number(cell(source_row, mapping, "u_rpm"))
        u_thrust = parse_source_number(cell(source_row, mapping, "u_thrust_N"))
        u_torque = parse_source_number(cell(source_row, mapping, "u_torque_Nm"))
        rows.append(
            {
                "research_run_id": research_run_id,
                "research_command_id": research_command_id,
                "source_id": source_id,
                "canonical_url": canonical_url,
                "snapshot_hash": snapshot_hash,
                "source_row_ref": row_ref,
                "measurement_kind": "experimental",
                "is_derived": "false",
                "rpm": format_number(rpm),
                "propeller_size": propeller_size,
                "prop_diameter_in": format_number(diameter_in),
                "prop_pitch_in": format_number(pitch_in),
                "thrust_N": format_number(thrust),
                "torque_Nm": format_number(torque) if torque is not None else "",
                "u_rpm": format_number(u_rpm) if u_rpm is not None else "",
                "u_thrust_N": format_number(u_thrust) if u_thrust is not None else "",
                "u_torque_Nm": format_number(u_torque) if u_torque is not None else "",
                "rpm_unit": "rpm",
                "thrust_unit": "N",
                "torque_unit": "Nm" if "torque_Nm" in mapping else "",
                "CT": format_number(ct) if ct is not None else "",
                "CP": format_number(cp) if cp is not None else "",
                "archive_manifest_hash": archive_manifest_hash,
                "archive_member_path": member_path,
                "archive_member_hash": member_hash,
                "parsing_rule": parsing_rule,
            }
        )
    reconciliation = {
        "member_path": member_path,
        "source_rows": len(parsed["rows"]),
        "emitted_rows": len(rows),
        "dropped_rows": dropped,
    }
    if reconciliation["source_rows"] != len(rows) + len(dropped):
        raise BuildError(f"row_reconciliation_mismatch:{member_path}")
    if not rows:
        raise BuildError(f"no_emittable_measurements:{member_path}")
    return rows, reconciliation


def derive_thrust_from_ct(ct: float, rpm: float, diameter_in: float, rho: float) -> float:
    revs_per_sec = rpm / 60.0
    diameter_m = diameter_in * 0.0254
    return ct * rho * revs_per_sec**2 * diameter_m**4


def derive_torque_from_cp(cp: float, rpm: float, diameter_in: float, rho: float) -> float:
    revs_per_sec = rpm / 60.0
    diameter_m = diameter_in * 0.0254
    return cp * rho * revs_per_sec**2 * diameter_m**5 / (2.0 * math.pi)


def build_derived_bearing_loads(
    *,
    research_run_id: str,
    research_command_id: str,
    claim: dict[str, str],
    propeller_size: str,
    archive_manifest_hash: str,
    member_path: str,
    member_hash: str,
    parsed: dict[str, Any],
    rho_kg_per_m3: float = 1.225,
) -> tuple[list[dict[str, str]], dict[str, Any]]:
    """Convert measured CT/CP coefficients into derived force/torque rows.

    UIUC/ENOLA CT and CP columns are direct dimensionless measurements; the
    force/torque conversion is derived and lives only here, with formula,
    constants, assumptions, units and source-row lineage. Derived rows are
    never mixed into the measured table, and an inferred radial bearing load
    is never derived from or mixed with measured axial thrust.
    """

    diameter_in, _ = parse_propeller_notation(propeller_size)
    mapping = parsed["mapping"]
    if "CT" not in mapping and "CP" not in mapping:
        return [], {
            "member_path": member_path,
            "source_rows": len(parsed["rows"]),
            "emitted_rows": 0,
            "dropped_rows": [],
        }
    rows: list[dict[str, str]] = []
    dropped: list[dict[str, Any]] = []
    for source_row in parsed["rows"]:
        line_number = source_row["line_number"]
        row_ref = f"{member_path}#row-{line_number}"
        rpm = parse_source_number(cell(source_row, mapping, "rpm"))
        ct = parse_source_number(cell(source_row, mapping, "CT"))
        cp = parse_source_number(cell(source_row, mapping, "CP"))
        if rpm is None or rpm <= 0:
            dropped.append({"source_row_ref": row_ref, "reason": "rpm_missing_or_not_positive"})
            continue
        if ct is None and cp is None:
            dropped.append({"source_row_ref": row_ref, "reason": "ct_cp_missing_or_not_numeric"})
            continue
        thrust = derive_thrust_from_ct(ct, rpm, diameter_in, rho_kg_per_m3) if ct is not None else None
        torque = derive_torque_from_cp(cp, rpm, diameter_in, rho_kg_per_m3) if cp is not None else None
        rows.append(
            {
                "research_run_id": research_run_id,
                "research_command_id": research_command_id,
                "claim_id": claim["claim_id"],
                "evidence_id": claim["evidence_id"],
                "source_id": claim["source_id"],
                "snapshot_id": claim["snapshot_id"],
                "canonical_url": claim["canonical_url"],
                "snapshot_hash": claim["snapshot_hash"],
                "quote": claim["quote"],
                "source_row_ref": row_ref,
                "derivation_method": "CT/CP to force/torque conversion",
                "assumption_text": (
                    "Static sea-level ISA air density; rigid propeller; "
                    "axial thrust only, no radial bearing load inferred"
                ),
                "is_derived": "true",
                "thrust_N": format_number(thrust) if thrust is not None else "",
                "torque_Nm": format_number(torque) if torque is not None else "",
                "bearing_radial_load_N": "",
                "formula": "T = CT * rho * (rpm/60)^2 * D^4; Q = CP * rho * (rpm/60)^2 * D^5 / (2*pi)",
                "constants": f"rho={rho_kg_per_m3} kg/m^3; D={diameter_in} in = {diameter_in * 0.0254} m",
                "units": "thrust_N=N; torque_Nm=Nm",
                "archive_manifest_hash": archive_manifest_hash,
                "archive_member_path": member_path,
                "archive_member_hash": member_hash,
            }
        )
    reconciliation = {
        "member_path": member_path,
        "source_rows": len(parsed["rows"]),
        "emitted_rows": len(rows),
        "dropped_rows": dropped,
    }
    if reconciliation["source_rows"] != len(rows) + len(dropped):
        raise BuildError(f"row_reconciliation_mismatch:{member_path}")
    return rows, reconciliation


def load_csv_rows(path: Path) -> list[dict[str, str]]:
    if not path.exists():
        return []
    with path.open(newline="", encoding="utf-8") as handle:
        return [dict(row) for row in csv.DictReader(handle)]


def load_jsonl_rows(path: Path) -> list[dict[str, str]]:
    if not path.exists():
        return []
    rows: list[dict[str, str]] = []
    with path.open(encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, start=1):
            if not line.strip():
                continue
            try:
                value = json.loads(line)
            except json.JSONDecodeError as exc:
                raise BuildError(f"candidate_jsonl_invalid:{path}:line-{line_number}") from exc
            if not isinstance(value, dict):
                raise BuildError(f"candidate_jsonl_row_not_object:{path}:line-{line_number}")
            rows.append({str(key): value for key, value in value.items()})
    return rows


def iter_discovery_rows(discovery_dirs: Iterable[Path]) -> list[dict[str, str]]:
    """Collect every candidate-audit row from every discovery round."""

    rows: list[dict[str, str]] = []
    for index, directory in enumerate(discovery_dirs, start=1):
        for stem, state in (
            ("candidate_sources", "candidate"),
            ("screened_sources", "screened"),
            ("rejected_sources", "rejected"),
        ):
            for loader, suffix in ((load_csv_rows, ".csv"), (load_jsonl_rows, ".jsonl")):
                for row in loader(directory / f"{stem}{suffix}"):
                    row = dict(row)
                    row.setdefault("verification_state", state)
                    row["_discovery_round"] = str(index)
                    rows.append(row)
    return rows


def normalized_sha256(value: Any, field: str) -> str:
    text = str(value or "").strip().lower()
    if text.startswith("sha256:"):
        text = text[7:]
    if not re.fullmatch(r"[0-9a-f]{64}", text):
        raise BuildError(f"{field}_invalid")
    return text


def load_enola_member_binding(path: Path) -> dict[str, str]:
    try:
        binding = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise BuildError(f"enola_member_binding_invalid:{path}") from exc
    if not isinstance(binding, dict):
        raise BuildError(f"enola_member_binding_not_object:{path}")

    required = (
        "csv_path",
        "propeller_size",
        "source_id",
        "canonical_url",
        "archive_sha256",
        "manifest_path",
        "manifest_sha256",
        "member_path",
        "member_sha256",
    )
    for field in required:
        if not str(binding.get(field) or "").strip():
            raise BuildError(f"enola_member_binding_missing:{field}")

    csv_path = Path(str(binding["csv_path"]))
    manifest_path = Path(str(binding["manifest_path"]))
    archive_hash = normalized_sha256(binding["archive_sha256"], "archive_sha256")
    manifest_hash = normalized_sha256(binding["manifest_sha256"], "manifest_sha256")
    member_hash = normalized_sha256(binding["member_sha256"], "member_sha256")
    if sha256_file(manifest_path) != manifest_hash:
        raise BuildError("enola_manifest_sha256_mismatch")
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise BuildError("enola_manifest_invalid") from exc
    if not isinstance(manifest, dict) or manifest.get("schema_version") != "ctox.web.zip-manifest.v2":
        raise BuildError("enola_manifest_schema_invalid")
    if normalized_sha256(manifest.get("archive_sha256"), "archive_sha256") != archive_hash:
        raise BuildError("enola_manifest_archive_sha256_mismatch")
    member_path = str(binding["member_path"]).strip()
    members = manifest.get("members")
    if not isinstance(members, list):
        raise BuildError("enola_manifest_members_invalid")
    member = next(
        (
            item
            for item in members
            if isinstance(item, dict) and str(item.get("path") or "") == member_path
        ),
        None,
    )
    if member is None:
        raise BuildError(f"enola_manifest_member_missing:{member_path}")
    if normalized_sha256(member.get("sha256"), "member_sha256") != member_hash:
        raise BuildError("enola_manifest_member_sha256_mismatch")
    if sha256_file(csv_path) != member_hash:
        raise BuildError("enola_extracted_member_sha256_mismatch")

    return {
        "csv_path": str(csv_path),
        "propeller_size": str(binding["propeller_size"]).strip(),
        "source_id": str(binding["source_id"]).strip(),
        "canonical_url": str(binding["canonical_url"]).strip(),
        "archive_sha256": archive_hash,
        "manifest_sha256": manifest_hash,
        "member_path": member_path,
        "member_sha256": member_hash,
    }


def build_source_candidates(
    *,
    research_run_id: str,
    research_command_id: str,
    discovery_dirs: Iterable[Path],
    admitted_urls: set[str],
) -> list[dict[str, str]]:
    """Accumulate the complete candidate audit inventory.

    Every candidate from every round is persisted exactly once under its
    canonical dedup key (DOI / stable id / canonical URL / content hash) with
    an explicit per-candidate verification state and rejection reason. There
    is no row limit and no silent truncation.
    """

    merged: dict[str, dict[str, str]] = {}
    for row in iter_discovery_rows(discovery_dirs):
        key = candidate_key(row)
        if not key:
            key = f"content:{candidate_content_hash(row)}"
        existing = merged.get(key)
        if existing is None:
            row["candidate_key"] = key
            row["_rounds"] = {row.get("_discovery_round", "")}
            merged[key] = row
        else:
            existing["_rounds"].add(row.get("_discovery_round", ""))
            # Prefer the most informative state: rejected > candidate > screened.
            rank = {"rejected": 2, "candidate": 1, "screened": 0}
            if rank.get(row.get("verification_state", ""), 0) > rank.get(
                existing.get("verification_state", ""), 0
            ):
                row["candidate_key"] = key
                row["_rounds"] = existing["_rounds"]
                merged[key] = row

    out: list[dict[str, str]] = []
    for row in merged.values():
        url_key = canonical_url_key(row.get("url") or "")
        state = row.get("verification_state") or "candidate"
        rejection_reason = (
            row.get("rejection_reason")
            or row.get("screening_reason")
            or ""
        ).strip()
        if url_key and url_key in admitted_urls:
            state = "admitted"
            rejection_reason = ""
        elif state == "rejected" and not rejection_reason:
            rejection_reason = "rejected_by_screening"
        elif state != "rejected":
            state = "not_promoted"
            if not rejection_reason:
                rejection_reason = "candidate_not_promoted_to_evidence"
        out.append(
            {
                "research_run_id": research_run_id,
                "research_command_id": research_command_id,
                "candidate_key": row["candidate_key"],
                "title": str(row.get("title") or "").strip(),
                "url": str(row.get("url") or "").strip(),
                "doi": normalize_doi(row.get("doi") or "") or str(row.get("doi") or "").strip(),
                "openalex_id": str(row.get("openalex_id") or "").strip(),
                "focus": str(row.get("focus") or "").strip(),
                "query": str(row.get("query") or "").strip(),
                "snippet": str(row.get("snippet") or "").strip()[:500],
                "source_class": str(row.get("source_class") or row.get("focus") or "").strip(),
                "verification_state": state,
                "rejection_reason": rejection_reason,
                "discovery_rounds": "|".join(sorted(r for r in row["_rounds"] if r)),
                "content_hash": candidate_content_hash(row),
            }
        )
    out.sort(key=lambda item: item["candidate_key"])
    return out


def build_source_catalog(
    *,
    research_run_id: str,
    research_command_id: str,
    manifest: dict[str, Any],
) -> list[dict[str, str]]:
    """Verified-source registry: only evidence admitted by the guard manifest.

    Rows come exclusively from manifest sources that carry eligible evidence;
    rejected or unread discovery candidates can never appear here.
    """

    evidence_by_source: dict[str, dict[str, Any]] = {}
    for item in manifest.get("evidence", []):
        if not isinstance(item, dict):
            continue
        if item.get("evidence_status") != "eligible":
            continue
        source_id = str(item.get("source_id") or "")
        if source_id:
            evidence_by_source[source_id] = item
    rows: list[dict[str, str]] = []
    for source in manifest.get("sources", []):
        if not isinstance(source, dict):
            continue
        source_id = str(source.get("source_id") or "")
        item = evidence_by_source.get(source_id)
        if not source_id or item is None:
            continue
        rows.append(
            {
                "research_run_id": research_run_id,
                "research_command_id": research_command_id,
                "source_id": source_id,
                "canonical_url": str(source.get("canonical_url") or ""),
                "snapshot_hash": str(item.get("snapshot_sha256") or ""),
                "evidence_id": str(item.get("evidence_id") or ""),
                "snapshot_id": str(item.get("snapshot_id") or ""),
                "relevance_score": str(item.get("relevance_score") or ""),
            }
        )
    rows.sort(key=lambda item: item["source_id"])
    return rows


def finite_number(value: str) -> float | None:
    value = str(value or "").strip()
    if not value:
        return None
    try:
        number = float(value)
    except ValueError:
        return None
    return number if number == number and abs(number) != float("inf") else None


def assert_native_measured_contract(rows: list[dict[str, str]]) -> None:
    """Mirror of the native ``measured_load_points`` row contract in
    ``src/core/business_os/store.rs``. The native schema is the source of
    truth; this check keeps builder output honest before native import."""

    for index, row in enumerate(rows, start=1):
        if row.get("measurement_kind", "").strip().lower() not in MEASUREMENT_KINDS:
            raise BuildError(f"native_contract:row {index}:measurement_kind")
        if row.get("is_derived", "").strip().lower() in {"true", "1", "yes"}:
            raise BuildError(f"native_contract:row {index}:is_derived")
        rpm = finite_number(row.get("rpm", ""))
        if rpm is None or rpm <= 0:
            raise BuildError(f"native_contract:row {index}:rpm")
        if not any(
            finite_number(row.get(field, "")) is not None
            for field in ("thrust_N", "force_N", "axial_load_N")
        ):
            raise BuildError(f"native_contract:row {index}:axial_force")
        diameter = finite_number(row.get("prop_diameter_in", ""))
        pitch = finite_number(row.get("prop_pitch_in", ""))
        if diameter is None or diameter <= 0 or pitch is None or pitch <= 0:
            raise BuildError(f"native_contract:row {index}:propeller")
        if not str(row.get("propeller_size") or "").strip():
            raise BuildError(f"native_contract:row {index}:propeller_size")
        ref = str(row.get("source_row_ref") or "").strip()
        if not ref or ref.lower() in {"unknown", "n/a"}:
            raise BuildError(f"native_contract:row {index}:source_row_ref")
        torque = str(row.get("torque_Nm") or "").strip()
        if torque and finite_number(torque) is None:
            raise BuildError(f"native_contract:row {index}:torque_Nm")
        for field in ("archive_manifest_hash", "archive_member_hash"):
            normalized_sha256(row.get(field), f"native_contract:row {index}:{field}")
        if not str(row.get("archive_member_path") or "").strip():
            raise BuildError(f"native_contract:row {index}:archive_member_path")
        for field in ("research_run_id", "research_command_id", "source_id", "canonical_url", "snapshot_hash"):
            if not str(row.get(field) or "").strip():
                raise BuildError(f"native_contract:row {index}:{field}")


def write_csv(path: Path, headers: list[str], rows: list[dict[str, str]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=headers, extrasaction="ignore")
        writer.writeheader()
        for row in rows:
            writer.writerow(row)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--research-run-id", required=True)
    parser.add_argument("--research-command-id", required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument(
        "--discovery-dir",
        type=Path,
        action="append",
        default=[],
        help="Discovery output directory; repeat for every discovery round.",
    )
    parser.add_argument(
        "--evidence-manifest",
        type=Path,
        help="Guard-validated validation/evidence-manifest.json for source_catalog and admitted URLs.",
    )
    parser.add_argument(
        "--enola-member",
        action="append",
        default=[],
        metavar="LEGACY_SPEC",
        help="Unsupported legacy syntax retained only for a fail-closed migration error.",
    )
    parser.add_argument(
        "--enola-member-binding",
        type=Path,
        action="append",
        default=[],
        metavar="BINDING_JSON",
        help="Verified ENOLA extraction binding with archive, manifest, member, and extracted-file hashes.",
    )
    parser.add_argument(
        "--derived-claim",
        action="append",
        default=[],
        metavar="LEGACY_SPEC",
        help="Unsupported legacy syntax retained only for a fail-closed migration error.",
    )
    parser.add_argument(
        "--derived-claim-binding",
        action="append",
        default=[],
        metavar="BINDING_JSON:CLAIM_ID",
        help="Build derived CT/CP rows from a verified ENOLA binding and validated manifest claim.",
    )
    args = parser.parse_args(argv)

    manifest: dict[str, Any] = {}
    admitted_urls: set[str] = set()
    claims_by_id: dict[str, dict[str, Any]] = {}
    if args.evidence_manifest:
        manifest = json.loads(args.evidence_manifest.read_text(encoding="utf-8"))
        for item in manifest.get("evidence", []):
            if isinstance(item, dict) and item.get("evidence_status") == "eligible":
                key = canonical_url_key(item.get("canonical_url"))
                if key:
                    admitted_urls.add(key)
        for claim in manifest.get("claims", []):
            if isinstance(claim, dict) and claim.get("claim_id"):
                claims_by_id[str(claim["claim_id"])] = claim

    out_dir: Path = args.out_dir
    reconciliations: list[dict[str, Any]] = []
    audits: list[dict[str, Any]] = []

    candidates = build_source_candidates(
        research_run_id=args.research_run_id,
        research_command_id=args.research_command_id,
        discovery_dirs=args.discovery_dir,
        admitted_urls=admitted_urls,
    )
    write_csv(out_dir / "source_candidates.csv", SOURCE_CANDIDATES_HEADERS, candidates)

    if manifest:
        catalog = build_source_catalog(
            research_run_id=args.research_run_id,
            research_command_id=args.research_command_id,
            manifest=manifest,
        )
        write_csv(out_dir / "source_catalog.csv", SOURCE_CATALOG_HEADERS, catalog)

    if args.enola_member:
        raise BuildError(
            "legacy_enola_member_binding_unsupported:"
            "use --enola-member-binding with archive/member hashes"
        )

    measured_rows: list[dict[str, str]] = []
    for binding_path in args.enola_member_binding:
        binding = load_enola_member_binding(binding_path)
        path = Path(binding["csv_path"])
        parsed = parse_enola_member_csv(
            path.read_text(encoding="utf-8"),
            binding["member_path"],
        )
        audits.append(parsed["audit"])
        rows, reconciliation = build_measured_load_points(
            research_run_id=args.research_run_id,
            research_command_id=args.research_command_id,
            source_id=binding["source_id"],
            canonical_url=binding["canonical_url"],
            snapshot_hash=binding["archive_sha256"],
            propeller_size=binding["propeller_size"],
            archive_manifest_hash=binding["manifest_sha256"],
            member_path=binding["member_path"],
            member_hash=binding["member_sha256"],
            parsed=parsed,
        )
        assert_native_measured_contract(rows)
        measured_rows.extend(rows)
        reconciliations.append(reconciliation)
    if measured_rows:
        write_csv(out_dir / "measured_load_points.csv", MEASURED_LOAD_POINTS_HEADERS, measured_rows)

    if args.derived_claim:
        raise BuildError(
            "legacy_derived_claim_binding_unsupported:"
            "use --derived-claim-binding with archive/member hashes"
        )

    derived_rows: list[dict[str, str]] = []
    for spec in args.derived_claim_binding:
        binding_path_text, sep, claim_id = spec.rpartition(":")
        if not sep or not binding_path_text or not claim_id:
            raise BuildError(f"derived_claim_binding_spec_invalid:{spec!r}")
        binding = load_enola_member_binding(Path(binding_path_text))
        claim = claims_by_id.get(claim_id)
        if claim is None:
            raise BuildError(f"derived_claim_not_in_manifest:{claim_id}")
        quote = str(claim.get("evidence_quote") or "")
        if not quote:
            raise BuildError(f"derived_claim_missing_quote:{claim_id}")
        claim_snapshot_hash = normalized_sha256(
            claim.get("snapshot_hash") or claim.get("snapshot_sha256"),
            "derived_claim_snapshot_hash",
        )
        if claim_snapshot_hash != binding["archive_sha256"]:
            raise BuildError(f"derived_claim_archive_binding_mismatch:{claim_id}")
        path = Path(binding["csv_path"])
        parsed = parse_enola_member_csv(
            path.read_text(encoding="utf-8"),
            binding["member_path"],
        )
        audits.append(parsed["audit"])
        rows, reconciliation = build_derived_bearing_loads(
            research_run_id=args.research_run_id,
            research_command_id=args.research_command_id,
            claim={
                "claim_id": claim_id,
                "evidence_id": str(claim.get("evidence_id") or ""),
                "source_id": str(claim.get("source_id") or ""),
                "snapshot_id": str(claim.get("snapshot_id") or ""),
                "canonical_url": str(claim.get("canonical_url") or ""),
                "snapshot_hash": str(claim.get("snapshot_hash") or claim.get("snapshot_sha256") or ""),
                "quote": quote,
            },
            propeller_size=binding["propeller_size"],
            archive_manifest_hash=binding["manifest_sha256"],
            member_path=binding["member_path"],
            member_hash=binding["member_sha256"],
            parsed=parsed,
        )
        derived_rows.extend(rows)
        reconciliations.append(reconciliation)
    if derived_rows:
        write_csv(out_dir / "derived_bearing_loads.csv", DERIVED_BEARING_LOADS_HEADERS, derived_rows)

    report = {
        "research_run_id": args.research_run_id,
        "research_command_id": args.research_command_id,
        "source_candidates": len(candidates),
        "measured_rows": len(measured_rows),
        "derived_rows": len(derived_rows),
        "reconciliations": reconciliations,
        "parser_audits": audits,
    }
    report_path = out_dir / "writeback_reconciliation.json"
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except BuildError as exc:
        print(json.dumps({"ok": False, "error": str(exc)}))
        raise SystemExit(1)
