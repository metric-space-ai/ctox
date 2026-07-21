#!/usr/bin/env python3
"""Targeted reading and extraction pass for source-review discovery outputs.

The discovery runner intentionally builds a broad source graph first. This
script is the next stage: it resolves readable/open-access locations for the
accepted source catalog, attempts to read the actual source text, extracts
measurement evidence, and records what was readable versus blocked.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import html
import http.client
import json
import re
import sys
import urllib.parse
from dataclasses import dataclass
from io import BytesIO
from pathlib import Path
from typing import Any

from evidence_guard import (
    BLOCKED_CONTENT,
    LOGIN_INTERSTITIAL,
    resolve_path,
    validate_manifest,
    validate_url,
)


USER_AGENT = "ctox-source-review-reading/1.0"
MAX_TEXT_CHARS = 350_000
MANIFEST_LINEAGE_FIELDS = (
    "source_id",
    "evidence_id",
    "snapshot_id",
    "canonical_url",
    "url_role",
    "content_scope",
    "http_status",
    "retrieved_at",
    "freshness",
    "snapshot_path",
    "sha256",
)
DOI_PATTERN = re.compile(r"^10\.\d{4,9}/\S+$", re.IGNORECASE)

MEASUREMENT_FAMILIES: dict[str, list[str]] = {
    "mass_payload": [
        "payload",
        "takeoff weight",
        "take-off weight",
        "gross weight",
        "mtow",
        "maximum takeoff",
        "maximum take-off",
        "all-up weight",
        "auw",
        "mass",
        "weight",
    ],
    "thrust_force": [
        "thrust",
        "force",
        "lift force",
        "load cell",
        "aerodynamic load",
        "normal force",
        "drag force",
    ],
    "torque_moment": [
        "torque",
        "moment",
        "pitching moment",
        "rolling moment",
        "yawing moment",
        "n-m",
        "n·m",
        "newton-meter",
    ],
    "propulsion_power": [
        "rpm",
        "rotational speed",
        "current",
        "voltage",
        "power consumption",
        "propeller",
        "rotor",
        "motor",
    ],
    "flight_environment": [
        "wind tunnel",
        "airspeed",
        "horizontal airflow",
        "flight log",
        "telemetry",
        "gust",
        "m/s",
    ],
    "dataset_table": [
        "dataset",
        "csv",
        "table",
        "supplementary",
        "repository",
        "data availability",
        "appendix",
    ],
}

MEASUREMENT_RE = re.compile(
    r"(?P<value>[+-]?(?:(?:\d{1,3}(?:,\d{3})+|\d+)(?:\.\d+)?|\.\d+)"
    r"(?:\s*[–—−-]\s*(?:(?:\d{1,3}(?:,\d{3})+|\d+)(?:\.\d+)?|\.\d+))?)"
    r"\s*(?P<unit>kg|g|lb|lbs|N|newtons?|N\s*[·.-]?\s*m|Nm|rpm|r/min|A|V|W|kW|m/s|m\s*s-1|%)\b",
    re.IGNORECASE,
)


@dataclass(frozen=True)
class ReadCandidate:
    kind: str
    url: str


def slugify(value: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9]+", "_", value.strip().lower()).strip("_")
    return cleaned[:90] or "source"


def normalize_doi(raw: Any) -> str:
    if not isinstance(raw, str):
        return ""
    value = raw.strip()
    value = re.sub(r"^https?://(dx\.)?doi\.org/", "", value, flags=re.IGNORECASE)
    value = re.sub(r"^doi:\s*", "", value, flags=re.IGNORECASE)
    value = value.rstrip(".,;)")
    return value if DOI_PATTERN.fullmatch(value) else ""


def _required_manifest_string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"manifest_{label}_missing")
    return value.strip()


def _manifest_freshness(item: dict[str, Any], snapshot: dict[str, Any]) -> str:
    values = [
        value.strip()
        for value in (
            item.get("freshness"),
            item.get("freshness_status"),
            snapshot.get("freshness"),
            snapshot.get("freshness_status"),
        )
        if isinstance(value, str) and value.strip()
    ]
    if not values or len(set(values)) != 1:
        raise ValueError("manifest_freshness_missing_or_conflicting")
    freshness = values[0]
    if freshness != "current":
        raise ValueError("manifest_evidence_not_current")
    return freshness


def load_manifest_bindings(manifest: dict[str, Any], base_dir: Path) -> dict[str, dict[str, Any]]:
    """Validate and materialize the manifest's authoritative evidence rows.

    The evidence guard checks the manifest contract. This stricter adapter adds
    the fields needed to bind CSV artifacts and independently re-hashes every
    snapshot immediately before it can authorize a reading row.
    """

    validate_manifest(manifest, base_dir)
    source_by_id = {
        str(source["source_id"]): source
        for source in manifest.get("sources", [])
        if isinstance(source, dict) and source.get("source_id")
    }
    bindings: dict[str, dict[str, Any]] = {}
    for item in manifest.get("evidence", []):
        if not isinstance(item, dict):
            raise ValueError("manifest_evidence_must_be_object")
        evidence_id = _required_manifest_string(item.get("evidence_id"), "evidence_id")
        source_id = _required_manifest_string(item.get("source_id"), "source_id")
        source = source_by_id.get(source_id)
        if source is None:
            raise ValueError("manifest_evidence_source_missing")

        snapshot = item.get("snapshot")
        if not isinstance(snapshot, dict):
            raise ValueError("manifest_snapshot_missing")
        snapshot_id = _required_manifest_string(item.get("snapshot_id"), "snapshot_id")
        if snapshot_id != _required_manifest_string(snapshot.get("snapshot_id"), "snapshot_id"):
            raise ValueError("manifest_snapshot_id_mismatch")
        canonical_url = _required_manifest_string(item.get("canonical_url"), "canonical_url")
        if canonical_url != _required_manifest_string(source.get("canonical_url"), "source_canonical_url"):
            raise ValueError("manifest_source_url_mismatch")
        if canonical_url != _required_manifest_string(snapshot.get("canonical_url"), "snapshot_canonical_url"):
            raise ValueError("manifest_snapshot_url_mismatch")
        url_role = _required_manifest_string(item.get("url_role"), "url_role")
        if url_role not in {"original_content", "original_data"}:
            raise ValueError("manifest_original_evidence_url_role_required")
        content_scope = _required_manifest_string(item.get("content_scope"), "content_scope").lower()
        if content_scope in {"abstract", "cookie_wall", "login", "metadata", "shell", "snippet", "landing"}:
            raise ValueError("manifest_metadata_or_landing_not_original_evidence")
        if str(item.get("content_kind") or "").strip().lower() in {
            "abstract",
            "landing",
            "metadata",
            "shell",
            "snippet",
        }:
            raise ValueError("manifest_metadata_or_landing_not_original_evidence")
        http_status = item.get("http_status")
        if isinstance(http_status, bool) or not isinstance(http_status, int) or not 200 <= http_status < 300:
            raise ValueError("manifest_evidence_requires_current_2xx")
        retrieved_at = item.get("retrieved_at", snapshot.get("retrieved_at"))
        retrieved_at = _required_manifest_string(retrieved_at, "retrieved_at")
        freshness = _manifest_freshness(item, snapshot)
        snapshot_path_raw = _required_manifest_string(snapshot.get("path"), "snapshot_path")
        snapshot_path = resolve_path(base_dir, snapshot_path_raw)
        expected_hash = _required_manifest_string(item.get("snapshot_sha256"), "sha256")
        snapshot_hash = _required_manifest_string(snapshot.get("sha256"), "sha256")
        if expected_hash != snapshot_hash:
            raise ValueError("manifest_snapshot_sha256_mismatch")
        if not snapshot_path.is_file():
            raise ValueError("manifest_snapshot_content_missing")
        actual_hash = hashlib.sha256(snapshot_path.read_bytes()).hexdigest()
        if actual_hash != expected_hash:
            raise ValueError("manifest_snapshot_sha256_mismatch")

        binding = {
            "source_id": source_id,
            "evidence_id": evidence_id,
            "snapshot_id": snapshot_id,
            "canonical_url": canonical_url,
            "url_role": url_role,
            "content_scope": content_scope,
            "http_status": http_status,
            "retrieved_at": retrieved_at,
            "freshness": freshness,
            "snapshot_path": snapshot_path_raw,
            "sha256": actual_hash,
            "freshness_status": freshness,
            "snapshot_sha256": actual_hash,
            "_snapshot_path": snapshot_path,
        }
        if evidence_id in bindings:
            raise ValueError("manifest_evidence_id_not_unique")
        bindings[evidence_id] = binding
    return bindings


def binding_output_fields(binding: dict[str, Any] | None) -> dict[str, str]:
    if not binding:
        return {field: "" for field in MANIFEST_LINEAGE_FIELDS}
    return {field: str(binding[field]) for field in MANIFEST_LINEAGE_FIELDS}


def binding_matches_row(row: dict[str, Any], binding: dict[str, Any]) -> bool:
    for field in MANIFEST_LINEAGE_FIELDS:
        if str(row.get(field, "")).strip() != str(binding.get(field, "")).strip():
            return False
    for alias, field in (("freshness_status", "freshness"), ("snapshot_sha256", "sha256")):
        if row.get(alias) not in (None, "") and str(row.get(alias)).strip() != str(binding[field]).strip():
            return False
    return True


def binding_for_candidate(row: dict[str, str], bindings: dict[str, dict[str, Any]]) -> dict[str, Any] | None:
    """Return a binding only for an exact candidate-to-manifest identity match."""

    if not bindings:
        return None
    candidate_url = (row.get("canonical_url") or row.get("url") or "").strip()
    explicit_ids = {
        key: row.get(key, "").strip()
        for key in ("source_id", "evidence_id", "snapshot_id")
        if row.get(key, "").strip()
    }
    if not candidate_url and not explicit_ids:
        return None
    matches: list[dict[str, Any]] = []
    for binding in bindings.values():
        if candidate_url and candidate_url not in {binding["canonical_url"]}:
            continue
        if any(binding[key] != value for key, value in explicit_ids.items()):
            continue
        if candidate_url or explicit_ids:
            matches.append(binding)
    if len(matches) != 1:
        return None
    binding = matches[0]
    if row.get("url", "").strip() and row["url"].strip() != binding["canonical_url"]:
        return None
    if row.get("canonical_url", "").strip() and row["canonical_url"].strip() != binding["canonical_url"]:
        return None
    return binding


def _binding_for_output_row(row: dict[str, str], bindings: dict[str, dict[str, Any]]) -> dict[str, Any] | None:
    evidence_id = row.get("evidence_id", "").strip()
    if evidence_id:
        binding = bindings.get(evidence_id)
        return binding if binding and binding_matches_row(row, binding) else None
    for binding in bindings.values():
        if binding_matches_row(row, binding):
            return binding
    return None


def validate_reading_artifacts(
    reading_rows: list[dict[str, str]],
    measurement_rows: list[dict[str, str]],
    bindings: dict[str, dict[str, Any]],
) -> tuple[list[dict[str, str]], list[dict[str, str]]]:
    """Return only rows whose complete lineage is bound to the manifest."""

    eligible_rows: list[dict[str, str]] = []
    eligible_keys: set[str] = set()
    verified_hashes: set[str] = set()
    for row in reading_rows:
        if row.get("evidence_eligible", "").strip().lower() != "true":
            continue
        binding = _binding_for_output_row(row, bindings)
        if binding is None or row.get("read_url", "").strip() != binding["canonical_url"]:
            raise ValueError("reading_manifest_binding_mismatch")
        snapshot_path = binding["_snapshot_path"]
        actual_hash = hashlib.sha256(snapshot_path.read_bytes()).hexdigest()
        if actual_hash != binding["sha256"]:
            raise ValueError("reading_snapshot_sha256_mismatch")
        eligible_rows.append(row)
        eligible_keys.add(binding["evidence_id"])
        verified_hashes.add(binding["sha256"])

    valid_measurements: list[dict[str, str]] = []
    for row in measurement_rows:
        binding = _binding_for_output_row(row, bindings)
        if binding is None or binding["evidence_id"] not in eligible_keys:
            raise ValueError("measurement_manifest_binding_mismatch")
        if row.get("source_url", "").strip() != binding["canonical_url"]:
            raise ValueError("measurement_source_url_mismatch")
        if binding["sha256"] not in verified_hashes:
            actual_hash = hashlib.sha256(binding["_snapshot_path"].read_bytes()).hexdigest()
            if actual_hash != binding["sha256"]:
                raise ValueError("measurement_snapshot_sha256_mismatch")
        valid_measurements.append(row)
    return eligible_rows, valid_measurements


def normalize_openalex_id(raw: Any) -> str:
    value = str(raw or "").strip()
    if not value:
        return ""
    if value.startswith("https://openalex.org/"):
        return value.rsplit("/", 1)[-1]
    return value


def http_json(url: str, timeout_sec: int) -> dict[str, Any]:
    status, data, _ = http_get(url, timeout_sec, headers={"User-Agent": USER_AGENT})
    if not (200 <= status < 300):
        raise RuntimeError(f"HTTP {status} for {url}")
    return json.loads(data.decode("utf-8"))


def require_http_url(url: str) -> None:
    parsed = urllib.parse.urlparse(str(url or ""))
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        raise ValueError(f"Refusing non-HTTP(S) source-reading URL: {url!r}")


def http_get(
    url: str,
    timeout_sec: int,
    *,
    headers: dict[str, str] | None = None,
    max_bytes: int | None = None,
) -> tuple[int, bytes, dict[str, str]]:
    require_http_url(url)
    parsed = urllib.parse.urlparse(str(url))
    path = parsed.path or "/"
    if parsed.query:
        path = f"{path}?{parsed.query}"
    connection_cls = http.client.HTTPSConnection if parsed.scheme == "https" else http.client.HTTPConnection
    conn = connection_cls(parsed.hostname, parsed.port, timeout=timeout_sec)
    try:
        conn.request("GET", path, headers=headers or {})
        response = conn.getresponse()
        data = response.read(max_bytes) if max_bytes is not None else response.read()
        response_headers = {key.lower(): value for key, value in response.headers.items()}
        return response.status, data, response_headers
    finally:
        conn.close()


def compact_abstract(value: Any) -> str:
    if isinstance(value, str):
        return re.sub(r"\s+", " ", value).strip()
    if not isinstance(value, dict):
        return ""
    positions: list[tuple[int, str]] = []
    for token, indexes in value.items():
        if not isinstance(indexes, list):
            continue
        for index in indexes:
            if isinstance(index, int):
                positions.append((index, str(token)))
    return " ".join(token for _, token in sorted(positions))


def openalex_work(row: dict[str, str], timeout_sec: int) -> dict[str, Any] | None:
    work_id = normalize_openalex_id(row.get("openalex_id"))
    doi = normalize_doi(row.get("doi") or row.get("url"))
    targets: list[str] = []
    if work_id:
        targets.append(f"https://api.openalex.org/works/{urllib.parse.quote(work_id)}")
    if doi:
        targets.append(f"https://api.openalex.org/works/doi:{urllib.parse.quote(doi, safe='')}")
    for url in targets:
        try:
            return http_json(url, timeout_sec)
        except Exception:
            continue
    return None


def add_url(out: list[ReadCandidate], seen: set[str], kind: str, url: Any) -> None:
    if not isinstance(url, str) or not url.strip():
        return
    value = url.strip()
    if value in seen:
        return
    if not value.startswith(("http://", "https://")):
        return
    try:
        validate_url(value, "original_content")
    except ValueError:
        return
    seen.add(value)
    out.append(ReadCandidate(kind, value))


def resolve_read_candidates(row: dict[str, str], work: dict[str, Any] | None) -> list[ReadCandidate]:
    out: list[ReadCandidate] = []
    seen: set[str] = set()

    if work:
        best = work.get("best_oa_location") if isinstance(work.get("best_oa_location"), dict) else {}
        primary = work.get("primary_location") if isinstance(work.get("primary_location"), dict) else {}
        add_url(out, seen, "openalex_best_pdf", best.get("pdf_url"))
        add_url(out, seen, "openalex_primary_pdf", primary.get("pdf_url"))
        locations = work.get("locations")
        if isinstance(locations, list):
            for loc in locations[:10]:
                if not isinstance(loc, dict):
                    continue
                add_url(out, seen, "openalex_location_pdf", loc.get("pdf_url"))

    add_url(out, seen, "candidate_url", row.get("url"))
    return out


def readable_priority(row: dict[str, str]) -> float:
    score = float(row.get("relevance_score") or 0)
    text = " ".join([row.get("title", ""), row.get("url", ""), row.get("snippet", "")]).lower()
    boost = 0.0
    if any(term in text for term in ("github", "zenodo", "dataverse", "ntrs", "dtic", "uiuc", "csv", "dataset", "repository")):
        boost += 30
    if any(term in text for term in ("supplement", "table", "load cell", "wind tunnel", "telemetry", "flight log")):
        boost += 18
    if "doi.org" in text:
        boost -= 5
    return score + boost


def strip_html(value: str) -> str:
    value = re.sub(r"(?is)<(script|style|noscript).*?</\1>", " ", value)
    value = re.sub(r"(?s)<[^>]+>", " ", value)
    value = html.unescape(value)
    return re.sub(r"\s+", " ", value).strip()


def text_from_bytes(data: bytes, source_hint: str) -> tuple[bool, str, str]:
    hint = source_hint.lower()
    is_pdf = data.startswith(b"%PDF-") or hint.endswith(".pdf") or "pdf" in hint
    if is_pdf:
        try:
            from pypdf import PdfReader  # type: ignore

            reader = PdfReader(BytesIO(data))
            pages = []
            for page in reader.pages[:40]:
                pages.append(page.extract_text() or "")
            text = re.sub(r"\s+", " ", "\n".join(pages)).strip()
            blocked = BLOCKED_CONTENT.search(text) or (len(text) < 1500 and LOGIN_INTERSTITIAL.search(text))
            ok = len(text) >= 500 and not blocked
            return ok, text[:MAX_TEXT_CHARS], "" if ok else "pdf text missing or interstitial"
        except Exception as exc:
            return False, "", f"pdf parse failed: {exc}"

    for encoding in ("utf-8", "latin-1"):
        try:
            decoded = data.decode(encoding, errors="ignore")
            text = strip_html(decoded)
            blocked = BLOCKED_CONTENT.search(text) or (len(text) < 1500 and LOGIN_INTERSTITIAL.search(text))
            ok = len(text) >= 500 and not blocked
            return ok, text[:MAX_TEXT_CHARS], "" if ok else "source text missing or interstitial"
        except Exception:
            continue
    return False, "", "decode failed"


def read_snapshot_text(path: Path) -> tuple[bool, str, str, bytes]:
    data = path.read_bytes()
    ok, text, error = text_from_bytes(data, str(path))
    return ok, text, error, data


def direct_fetch_text(url: str, timeout_sec: int) -> tuple[bool, str, str, bytes]:
    try:
        status, data, response_headers = http_get(
            url,
            timeout_sec,
            headers={
                "User-Agent": USER_AGENT,
                "Accept": "text/html,application/pdf,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            },
            max_bytes=12_000_000,
        )
        if not (200 <= status < 300):
            return False, "", f"http {status}", b""
        content_type = response_headers.get("content-type", "").lower()
    except Exception as exc:
        return False, "", f"fetch failed: {exc}", b""

    ok, text, error = text_from_bytes(data, content_type or url)
    return ok, text, error, data


def snippets_for_terms(text: str) -> list[dict[str, str]]:
    lowered = text.lower()
    hits: list[dict[str, str]] = []
    for family, terms in MEASUREMENT_FAMILIES.items():
        for term in terms:
            start = lowered.find(term.lower())
            if start < 0:
                continue
            left = max(0, start - 180)
            right = min(len(text), start + len(term) + 220)
            snippet = re.sub(r"\s+", " ", text[left:right]).strip()
            hits.append({"family": family, "term": term, "snippet": snippet})
            break
    return hits


def measurement_family_for_unit(raw_unit: str) -> str:
    unit = re.sub(r"\s+", "", raw_unit).lower()
    if raw_unit.strip() == "G":
        return ""
    if unit in {"kg", "g", "lb", "lbs"}:
        return "mass_payload"
    if unit in {"n", "newton", "newtons"}:
        return "thrust_force"
    if unit in {"nm", "n·m", "n-m", "n.m", "newton-meter"}:
        return "torque_moment"
    if unit in {"rpm", "r/min", "a", "v", "w", "kw"}:
        return "propulsion_power"
    if unit in {"m/s", "ms-1", "m*s-1"}:
        return "flight_environment"
    if unit == "%":
        return ""
    return ""


def extract_measurements(
    row: dict[str, str],
    source_url: str,
    text: str,
    max_rows: int = 80,
    binding: dict[str, Any] | None = None,
) -> list[dict[str, str]]:
    snippets = snippets_for_terms(text)
    measurement_rows: list[dict[str, str]] = []
    seen: set[tuple[str, str, str, str]] = set()
    lineage = binding_output_fields(binding)
    lineage["freshness_status"] = str(binding.get("freshness", "")) if binding else ""
    lineage["snapshot_sha256"] = str(binding.get("sha256", "")) if binding else ""
    for hit in snippets:
        for match in MEASUREMENT_RE.finditer(hit["snippet"]):
            family = measurement_family_for_unit(match.group("unit"))
            if not family:
                continue
            key = (row.get("title", ""), family, re.sub(r"\s+", " ", match.group("value")), re.sub(r"\s+", " ", match.group("unit")))
            if key in seen:
                continue
            seen.add(key)
            measurement_rows.append(
                {
                    **lineage,
                    "title": row.get("title", ""),
                    "doi": normalize_doi(row.get("doi") or row.get("url")),
                    "openalex_id": row.get("openalex_id", ""),
                    "source_url": source_url,
                    "family": family,
                    "term": hit["term"],
                    "value": re.sub(r"\s+", " ", match.group("value")),
                    "unit": re.sub(r"\s+", " ", match.group("unit")),
                    "snippet": hit["snippet"][:900],
                }
            )
            if len(measurement_rows) >= max_rows:
                return measurement_rows
    return measurement_rows


def write_csv(path: Path, rows: list[dict[str, Any]], fields: list[str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields, extrasaction="ignore")
        writer.writeheader()
        for row in rows:
            writer.writerow(row)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--discovery-dir", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--limit", type=int, default=60)
    parser.add_argument("--max-urls-per-source", type=int, default=6)
    parser.add_argument("--read-timeout-sec", type=int, default=35)
    parser.add_argument(
        "--evidence-manifest",
        type=Path,
        help="Authoritative manifest. Without it, reads remain non-authoritative and cannot produce evidence rows.",
    )
    args = parser.parse_args(argv)

    candidate_path = args.discovery_dir / "candidate_sources.csv"
    if not candidate_path.exists():
        raise SystemExit(f"missing candidate_sources.csv: {candidate_path}")

    manifest_bindings: dict[str, dict[str, Any]] = {}
    if args.evidence_manifest:
        manifest = json.loads(args.evidence_manifest.read_text(encoding="utf-8"))
        manifest_bindings = load_manifest_bindings(manifest, args.evidence_manifest.parent)

    args.out_dir.mkdir(parents=True, exist_ok=True)
    text_dir = args.out_dir / "texts"
    text_dir.mkdir(parents=True, exist_ok=True)
    snapshot_dir = args.out_dir / "snapshots"
    snapshot_dir.mkdir(parents=True, exist_ok=True)

    with candidate_path.open(encoding="utf-8") as handle:
        candidates = list(csv.DictReader(handle))
    candidates.sort(key=readable_priority, reverse=True)
    selected = candidates[: args.limit]

    status_rows: list[dict[str, Any]] = []
    measurement_rows: list[dict[str, str]] = []
    graph_nodes: list[dict[str, Any]] = []
    graph_edges: list[dict[str, Any]] = []

    for index, row in enumerate(selected, start=1):
        binding = binding_for_candidate(row, manifest_bindings)
        tried: list[str] = []
        read_text = ""
        read_url = ""
        read_method = ""
        last_error = ""
        raw_data = b""

        if args.evidence_manifest:
            if binding is None:
                last_error = "no exact authoritative manifest binding for candidate"
            else:
                tried.append(f"manifest_snapshot:{binding['canonical_url']}")
                try:
                    ok, text, error, downloaded = read_snapshot_text(binding["_snapshot_path"])
                    if ok:
                        read_text = text
                        read_url = binding["canonical_url"]
                        read_method = "manifest_snapshot"
                        raw_data = downloaded
                    else:
                        last_error = error
                except OSError as exc:
                    last_error = f"manifest snapshot read failed: {exc}"
                if raw_data and hashlib.sha256(raw_data).hexdigest() != binding["sha256"]:
                    raise ValueError("manifest_snapshot_sha256_mismatch")
        else:
            work = openalex_work(row, args.read_timeout_sec)
            urls = resolve_read_candidates(row, work)
            for candidate in urls[: args.max_urls_per_source]:
                tried.append(f"{candidate.kind}:{candidate.url}")
                ok, text, error, downloaded = direct_fetch_text(candidate.url, args.read_timeout_sec)
                if ok:
                    read_text = text
                    read_url = candidate.url
                    read_method = f"direct:{candidate.kind}"
                    raw_data = downloaded
                    break
                last_error = error

        source_measurements: list[dict[str, str]] = []
        text_path = ""
        snapshot_path = ""
        snapshot_sha256 = ""
        if read_text:
            text_path = str(text_dir / f"{index:03d}_{slugify(row.get('title', 'source'))}.txt")
            Path(text_path).write_text(read_text, encoding="utf-8")
            snapshot_sha256 = hashlib.sha256(raw_data).hexdigest()
            if binding:
                snapshot_path = binding["snapshot_path"]
                if snapshot_sha256 != binding["sha256"]:
                    raise ValueError("manifest_snapshot_sha256_mismatch")
                source_measurements = extract_measurements(row, read_url, read_text, binding=binding)
                measurement_rows.extend(source_measurements)
            else:
                snapshot_file = snapshot_dir / f"{index:03d}_{slugify(row.get('title', 'source'))}.bin"
                snapshot_file.write_bytes(raw_data)
                snapshot_path = str(snapshot_file)

        try:
            score = float(row.get("relevance_score") or 0)
        except ValueError:
            score = 0
        evidence_eligible = bool(binding and read_text and score >= 8 and snapshot_sha256)
        if evidence_eligible:
            status = "evidence" if source_measurements else "evidence_no_measurements"
        else:
            status = "rejected" if read_text else "blocked"

        status_rows.append(
            {
                **binding_output_fields(binding),
                "freshness_status": str(binding.get("freshness", "")) if binding else "",
                "rank": index,
                "status": status,
                "title": row.get("title", ""),
                "doi": normalize_doi(row.get("doi") or row.get("url")),
                "openalex_id": row.get("openalex_id", ""),
                "relevance_score": row.get("relevance_score", ""),
                "acceptance_reason": row.get("acceptance_reason", ""),
                "read_method": read_method,
                "read_url": read_url,
                "text_chars": len(read_text),
                "measurement_rows": len(source_measurements),
                "evidence_eligible": str(evidence_eligible).lower(),
                "tried_urls": " | ".join(tried),
                "last_error": last_error,
                "text_path": text_path,
                "snapshot_path": snapshot_path,
                "snapshot_sha256": snapshot_sha256,
            }
        )

        node_id = row.get("openalex_id") or row.get("doi") or row.get("url") or f"source-{index}"
        graph_nodes.append(
            {
                "id": node_id,
                "label": row.get("title", "")[:140],
                "kind": "source",
                "status": status,
                "relevance_score": row.get("relevance_score", ""),
                "measurement_rows": len(source_measurements),
                "read_url": read_url,
            }
        )
        for measurement in source_measurements[:20]:
            measurement_id = f"{node_id}::{measurement['family']}::{measurement['value']} {measurement['unit']}"
            graph_nodes.append(
                {
                    "id": measurement_id,
                    "label": f"{measurement['family']}: {measurement['value']} {measurement['unit']}",
                    "kind": "measurement",
                    "family": measurement["family"],
                }
            )
            graph_edges.append({"source": node_id, "target": measurement_id, "relation": "evidence_extract"})

    write_csv(
        args.out_dir / "reading_status.csv",
        status_rows,
        [
            *MANIFEST_LINEAGE_FIELDS,
            "rank",
            "status",
            "title",
            "doi",
            "openalex_id",
            "relevance_score",
            "acceptance_reason",
            "read_method",
            "read_url",
            "text_chars",
            "measurement_rows",
            "evidence_eligible",
            "tried_urls",
            "last_error",
            "text_path",
            "snapshot_path",
            "snapshot_sha256",
            "freshness_status",
        ],
    )
    write_csv(
        args.out_dir / "extracted_measurements.csv",
        measurement_rows,
        [
            *MANIFEST_LINEAGE_FIELDS,
            "title",
            "doi",
            "openalex_id",
            "source_url",
            "family",
            "term",
            "value",
            "unit",
            "snippet",
            "freshness_status",
            "snapshot_sha256",
        ],
    )
    graph = {"nodes": graph_nodes, "edges": graph_edges}
    (args.out_dir / "reading_graph.json").write_text(json.dumps(graph, indent=2, ensure_ascii=False), encoding="utf-8")

    summary = {
        "selected_sources": len(selected),
        "readable_sources": sum(1 for row in status_rows if row["evidence_eligible"] == "true"),
        "metadata_only_sources": 0,
        "blocked_sources": sum(1 for row in status_rows if row["status"] == "blocked"),
        "extracted_sources": sum(1 for row in status_rows if row["status"] in {"evidence", "evidence_no_measurements"}),
        "measurement_rows": len(measurement_rows),
        "manifest_bound_sources": sum(1 for row in status_rows if row["evidence_eligible"] == "true"),
        "outputs": {
            "reading_status": str(args.out_dir / "reading_status.csv"),
            "extracted_measurements": str(args.out_dir / "extracted_measurements.csv"),
            "reading_graph": str(args.out_dir / "reading_graph.json"),
        },
    }
    (args.out_dir / "reading_summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
