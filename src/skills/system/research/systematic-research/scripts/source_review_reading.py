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

from evidence_guard import BLOCKED_CONTENT, LOGIN_INTERSTITIAL, validate_url


USER_AGENT = "ctox-source-review-reading/1.0"
MAX_TEXT_CHARS = 350_000

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
    return value


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

    if "pdf" in content_type or url.lower().endswith(".pdf"):
        try:
            from pypdf import PdfReader  # type: ignore

            reader = PdfReader(BytesIO(data))
            pages = []
            for page in reader.pages[:40]:
                pages.append(page.extract_text() or "")
            text = re.sub(r"\s+", " ", "\n".join(pages)).strip()
            blocked = BLOCKED_CONTENT.search(text) or (len(text) < 1500 and LOGIN_INTERSTITIAL.search(text))
            ok = len(text) >= 500 and not blocked
            return ok, text[:MAX_TEXT_CHARS], "" if ok else "pdf text missing or interstitial", data
        except Exception as exc:
            return False, "", f"pdf parse failed: {exc}", data

    for encoding in ("utf-8", "latin-1"):
        try:
            decoded = data.decode(encoding, errors="ignore")
            text = strip_html(decoded)
            blocked = BLOCKED_CONTENT.search(text) or (len(text) < 1500 and LOGIN_INTERSTITIAL.search(text))
            ok = len(text) >= 500 and not blocked
            return ok, text[:MAX_TEXT_CHARS], "" if ok else "source text missing or interstitial", data
        except Exception:
            continue
    return False, "", "decode failed", data


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


def extract_measurements(row: dict[str, str], source_url: str, text: str, max_rows: int = 80) -> list[dict[str, str]]:
    snippets = snippets_for_terms(text)
    measurement_rows: list[dict[str, str]] = []
    seen: set[tuple[str, str, str, str]] = set()
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
    args = parser.parse_args(argv)

    candidate_path = args.discovery_dir / "candidate_sources.csv"
    if not candidate_path.exists():
        raise SystemExit(f"missing candidate_sources.csv: {candidate_path}")

    args.out_dir.mkdir(parents=True, exist_ok=True)
    text_dir = args.out_dir / "texts"
    text_dir.mkdir(parents=True, exist_ok=True)
    snapshot_dir = args.out_dir / "snapshots"
    snapshot_dir.mkdir(parents=True, exist_ok=True)

    candidates = list(csv.DictReader(candidate_path.open(encoding="utf-8")))
    candidates.sort(key=readable_priority, reverse=True)
    selected = candidates[: args.limit]

    status_rows: list[dict[str, Any]] = []
    measurement_rows: list[dict[str, str]] = []
    graph_nodes: list[dict[str, Any]] = []
    graph_edges: list[dict[str, Any]] = []

    for index, row in enumerate(selected, start=1):
        work = openalex_work(row, args.read_timeout_sec)
        urls = resolve_read_candidates(row, work)
        tried: list[str] = []
        read_text = ""
        read_url = ""
        read_method = ""
        last_error = ""
        raw_data = b""

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
            snapshot_file = snapshot_dir / f"{index:03d}_{slugify(row.get('title', 'source'))}.bin"
            snapshot_file.write_bytes(raw_data)
            snapshot_path = str(snapshot_file)
            snapshot_sha256 = hashlib.sha256(raw_data).hexdigest()
            source_measurements = extract_measurements(row, read_url, read_text)
            measurement_rows.extend(source_measurements)

        try:
            score = float(row.get("relevance_score") or 0)
        except ValueError:
            score = 0
        evidence_eligible = bool(read_text and score >= 8 and snapshot_sha256)
        if evidence_eligible:
            status = "evidence" if source_measurements else "evidence_no_measurements"
        else:
            status = "rejected" if read_text else "blocked"

        status_rows.append(
            {
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
        ],
    )
    write_csv(
        args.out_dir / "extracted_measurements.csv",
        measurement_rows,
        ["title", "doi", "openalex_id", "source_url", "family", "term", "value", "unit", "snippet"],
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
