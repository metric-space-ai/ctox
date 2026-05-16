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
import html
import json
import re
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from io import BytesIO
from pathlib import Path
from typing import Any


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
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=timeout_sec) as response:
        return json.loads(response.read().decode("utf-8"))


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
    seen.add(value)
    out.append(ReadCandidate(kind, value))


def resolve_read_candidates(row: dict[str, str], work: dict[str, Any] | None) -> list[ReadCandidate]:
    out: list[ReadCandidate] = []
    seen: set[str] = set()

    if work:
        best = work.get("best_oa_location") if isinstance(work.get("best_oa_location"), dict) else {}
        primary = work.get("primary_location") if isinstance(work.get("primary_location"), dict) else {}
        open_access = work.get("open_access") if isinstance(work.get("open_access"), dict) else {}
        add_url(out, seen, "openalex_best_pdf", best.get("pdf_url"))
        add_url(out, seen, "openalex_oa_url", open_access.get("oa_url"))
        add_url(out, seen, "openalex_best_landing", best.get("landing_page_url"))
        add_url(out, seen, "openalex_primary_pdf", primary.get("pdf_url"))
        add_url(out, seen, "openalex_primary_landing", primary.get("landing_page_url"))
        locations = work.get("locations")
        if isinstance(locations, list):
            for loc in locations[:10]:
                if not isinstance(loc, dict):
                    continue
                add_url(out, seen, "openalex_location_pdf", loc.get("pdf_url"))
                add_url(out, seen, "openalex_location_landing", loc.get("landing_page_url"))

    add_url(out, seen, "candidate_url", row.get("url"))
    doi = normalize_doi(row.get("doi") or row.get("url"))
    if doi:
        add_url(out, seen, "doi", f"https://doi.org/{doi}")
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


def run_ctox_web_read(url: str, timeout_sec: int) -> tuple[bool, str, str]:
    try:
        proc = subprocess.run(
            ["ctox", "web", "read", "--url", url],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_sec,
            check=False,
        )
    except Exception as exc:
        return False, "", f"ctox web read failed: {exc}"
    text = proc.stdout.strip()
    if proc.returncode == 0 and len(text) >= 500:
        return True, text[:MAX_TEXT_CHARS], ""
    return False, text[:2000], (proc.stderr.strip() or f"ctox web read returned {proc.returncode}")[:2000]


def strip_html(value: str) -> str:
    value = re.sub(r"(?is)<(script|style|noscript).*?</\1>", " ", value)
    value = re.sub(r"(?s)<[^>]+>", " ", value)
    value = html.unescape(value)
    return re.sub(r"\s+", " ", value).strip()


def direct_fetch_text(url: str, timeout_sec: int) -> tuple[bool, str, str]:
    request = urllib.request.Request(
        url,
        headers={
            "User-Agent": USER_AGENT,
            "Accept": "text/html,application/pdf,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout_sec) as response:
            data = response.read(12_000_000)
            content_type = response.headers.get("content-type", "").lower()
    except urllib.error.HTTPError as exc:
        return False, "", f"http {exc.code}"
    except Exception as exc:
        return False, "", f"fetch failed: {exc}"

    if "pdf" in content_type or url.lower().endswith(".pdf"):
        try:
            from pypdf import PdfReader  # type: ignore

            reader = PdfReader(BytesIO(data))
            pages = []
            for page in reader.pages[:40]:
                pages.append(page.extract_text() or "")
            text = re.sub(r"\s+", " ", "\n".join(pages)).strip()
            return len(text) >= 500, text[:MAX_TEXT_CHARS], "" if len(text) >= 500 else "pdf text too short"
        except Exception as exc:
            return False, "", f"pdf parse failed: {exc}"

    for encoding in ("utf-8", "latin-1"):
        try:
            decoded = data.decode(encoding, errors="ignore")
            text = strip_html(decoded)
            return len(text) >= 500, text[:MAX_TEXT_CHARS], "" if len(text) >= 500 else "html text too short"
        except Exception:
            continue
    return False, "", "decode failed"


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

    candidates = list(csv.DictReader(candidate_path.open(encoding="utf-8")))
    candidates.sort(key=readable_priority, reverse=True)
    selected = candidates[: args.limit]

    status_rows: list[dict[str, Any]] = []
    measurement_rows: list[dict[str, str]] = []
    graph_nodes: list[dict[str, Any]] = []
    graph_edges: list[dict[str, Any]] = []

    for index, row in enumerate(selected, start=1):
        work = openalex_work(row, args.read_timeout_sec)
        abstract = compact_abstract(work.get("abstract_inverted_index")) if work else ""
        urls = resolve_read_candidates(row, work)
        tried: list[str] = []
        read_text = ""
        read_url = ""
        read_method = ""
        last_error = ""

        for candidate in urls[: args.max_urls_per_source]:
            tried.append(f"{candidate.kind}:{candidate.url}")
            ok, text, error = direct_fetch_text(candidate.url, args.read_timeout_sec)
            if ok:
                read_text = text
                read_url = candidate.url
                read_method = f"direct:{candidate.kind}"
                break
            last_error = error

            ok, text, error = run_ctox_web_read(candidate.url, args.read_timeout_sec)
            if ok:
                read_text = text
                read_url = candidate.url
                read_method = f"ctox_web_read:{candidate.kind}"
                break
            last_error = error

        if not read_text and abstract:
            read_text = abstract
            read_url = row.get("url", "")
            read_method = "metadata_abstract"

        source_measurements: list[dict[str, str]] = []
        text_path = ""
        if read_text:
            text_path = str(text_dir / f"{index:03d}_{slugify(row.get('title', 'source'))}.txt")
            Path(text_path).write_text(read_text, encoding="utf-8")
            source_measurements = extract_measurements(row, read_url, read_text)
            measurement_rows.extend(source_measurements)

        if not read_text:
            status = "blocked"
        elif read_method == "metadata_abstract":
            status = "metadata_only"
        elif source_measurements:
            status = "extracted"
        else:
            status = "readable_no_measurements"

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
                "tried_urls": " | ".join(tried),
                "last_error": last_error,
                "text_path": text_path,
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
            "tried_urls",
            "last_error",
            "text_path",
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
        "readable_sources": sum(1 for row in status_rows if row["status"] in {"extracted", "readable_no_measurements"}),
        "metadata_only_sources": sum(1 for row in status_rows if row["status"] == "metadata_only"),
        "blocked_sources": sum(1 for row in status_rows if row["status"] == "blocked"),
        "extracted_sources": sum(1 for row in status_rows if row["status"] == "extracted"),
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
