#!/usr/bin/env python3
"""Deterministic source-review discovery runner.

The deep-research skill uses this script before drafting a source_review. It
turns a topic into a broad query plan, executes many `ctox web deep-research`
calls, persists every raw bundle, registers each query as a report research log,
and writes auditable CSV artifacts for the search protocol and source catalog.
"""

from __future__ import annotations

import argparse
import csv
import json
import re
import subprocess
import sys
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class QuerySpec:
    focus: str
    query: str


def slugify(value: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9]+", "_", value.strip().lower()).strip("_")
    return cleaned[:80] or "source_review"


def default_query_plan(topic: str) -> list[QuerySpec]:
    t = topic.strip()
    q = normalized_topic_for_query(t)
    families: list[tuple[str, list[str]]] = [
        (
            "web",
            [
                f"{q} data sources",
                f"{q} technical data",
                f"{q} measurements dataset",
                f"{q} operating limits",
                f"{q} failure loads vibration wind thermal mechanical",
            ],
        ),
        (
            "scholarly",
            [
                f"{q} review paper",
                f"{q} experimental study data",
                f"{q} model validation dataset",
                f"{q} measurement campaign",
                f"{q} load estimation response identification",
                f"{q} open data DOI",
            ],
        ),
        (
            "agency",
            [
                f"{q} FAA EASA NASA DoD report",
                f"{q} government technical report",
                f"{q} regulatory guidance data",
                f"{q} safety assessment authority",
                f"{q} public agency dataset",
            ],
        ),
        (
            "standards",
            [
                f"{q} ASTM ISO IEC SAE RTCA standard",
                f"{q} MIL STD NATO STANAG standard",
                f"{q} standard test method load vibration environmental",
                f"{q} qualification test standard",
            ],
        ),
        (
            "reports",
            [
                f"{q} technical report PDF",
                f"{q} thesis dissertation data",
                f"{q} DTIC NTRS technical report",
                f"{q} conference proceedings dataset",
                f"{q} benchmark report",
            ],
        ),
        (
            "dataset",
            [
                f"{q} dataset repository",
                f"{q} GitHub data csv",
                f"{q} Zenodo Figshare Dataverse",
                f"{q} telemetry log data",
                f"{q} benchmark database",
            ],
        ),
        (
            "industry",
            [
                f"{q} manufacturer datasheet",
                f"{q} product manual limits",
                f"{q} application note data",
                f"{q} OEM specification payload load vibration",
            ],
        ),
        (
            "patent",
            [
                f"{q} patent load data",
                f"{q} patent technical report",
                f"{q} invention measurement system",
            ],
        ),
    ]
    plan = topic_specific_query_plan(t)
    plan.extend(QuerySpec(focus, query) for focus, queries in families for query in queries)
    return dedupe_query_plan(plan)


def normalized_topic_for_query(topic: str) -> str:
    compact = re.sub(r"\s+", " ", topic).strip()
    compact = re.sub(r"(?i)^research into sources of\s+", "", compact)
    compact = re.sub(r"(?i)^source review for\s+", "", compact)
    compact = re.sub(r"(?i)^source review of\s+", "", compact)
    compact = re.sub(r"(?i)^sources review for\s+", "", compact)
    compact = re.sub(r"(?i)^source compendium for\s+", "", compact)
    compact = re.sub(r"(?i)^sources of\s+", "", compact)
    compact = re.sub(r"(?i)^find sources for\s+", "", compact)
    compact = re.sub(r"(?i)^load data sources for\s+", "load data for ", compact)
    return compact or topic.strip()


def dedupe_query_plan(plan: list[QuerySpec]) -> list[QuerySpec]:
    seen: set[tuple[str, str]] = set()
    out: list[QuerySpec] = []
    for item in plan:
        key = (item.focus.strip().lower(), re.sub(r"\s+", " ", item.query.strip().lower()))
        if key in seen:
            continue
        seen.add(key)
        out.append(item)
    return out


def topic_specific_query_plan(topic: str) -> list[QuerySpec]:
    """Add deterministic query families for high-ambiguity technical data topics."""

    text = topic.lower()
    if not any(term in text for term in ("drone", "uas", "uav", "suas", "unmanned aerial")):
        return []
    if not any(term in text for term in ("load", "payload", "takeoff", "mtow", "weight", "thrust")):
        return []

    scopes = [
        "drone UAS UAV sUAS up to 25 kg",
        "DoD Group 1 Group 2 UAS classification",
        "small unmanned aircraft MTOW payload capacity",
    ]
    variables = [
        "payload capacity MTOW AUW datasheet",
        "maximum gross takeoff weight payload small UAS table",
        "thrust stand load cell force moment dataset",
        "UAV propulsion database thrust torque current voltage propeller",
        "flight log telemetry current draw motor output dataset",
        "rotor propeller aerodynamic loads experimental data",
        "small UAV wind tunnel force moment data",
        "airframe structural loads small UAV technical report",
    ]
    repositories = [
        "NASA NTRS small UAV load data",
        "NASA NTRS UAS payload MTOW technical report",
        "DTIC small UAS payload load technical report",
        "DTIC Group 1 Group 2 UAS payload maximum gross weight",
        "FAA EASA small UAS weight payload data",
        "PX4 ArduPilot drone flight log dataset payload",
        "Zenodo Figshare GitHub UAV thrust dataset",
        "UIUC propeller database UAV thrust coefficient",
        "Tyto Robotics thrust stand CSV drone motor propeller",
    ]
    oems = [
        "small UAV manufacturer datasheet payload MTOW",
        "multirotor drone payload capacity technical specifications",
        "fixed wing UAV payload endurance MTOW datasheet",
        "Group 2 UAS datasheet payload MTOW endurance",
    ]
    standards = [
        "ASTM small UAS standard payload weight",
        "NATO STANAG UAS classification Group 1 Group 2",
        "DoD UAS groups maximum gross takeoff weight",
    ]
    families = [
        ("scope_terms", scopes),
        ("measured_variables", variables),
        ("datasets_repositories", repositories),
        ("oem_specs", oems),
        ("classification_standards", standards),
    ]
    return [QuerySpec(focus, query) for focus, queries in families for query in queries]


def load_query_plan(path: Path) -> list[QuerySpec]:
    if path.suffix.lower() == ".json":
        raw = json.loads(path.read_text(encoding="utf-8"))
        return [QuerySpec(str(item["focus"]), str(item["query"])) for item in raw]
    rows: list[QuerySpec] = []
    with path.open(newline="", encoding="utf-8") as handle:
        reader = csv.DictReader(handle)
        for row in reader:
            rows.append(QuerySpec(row["focus"], row["query"]))
    return rows


def save_query_plan(path: Path, plan: list[QuerySpec]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.writer(handle)
        writer.writerow(["focus", "query"])
        for item in plan:
            writer.writerow([item.focus, item.query])


def http_json(url: str, timeout_sec: int) -> dict[str, Any]:
    request = urllib.request.Request(url, headers={"User-Agent": "ctox-source-review/1.0"})
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
    return " ".join(token for _, token in sorted(positions))[:1000]


def normalize_doi(raw: Any) -> str:
    if not isinstance(raw, str):
        return ""
    value = raw.strip()
    value = re.sub(r"^https?://(dx\.)?doi\.org/", "", value, flags=re.IGNORECASE)
    return value


def normalize_openalex_work_id(raw: str) -> str:
    value = str(raw or "").strip()
    if not value:
        return ""
    if value.startswith("https://openalex.org/"):
        return value.rsplit("/", 1)[-1]
    return value


def openalex_work_to_source(item: dict[str, Any]) -> dict[str, Any]:
    location = item.get("primary_location") if isinstance(item.get("primary_location"), dict) else {}
    landing = location.get("landing_page_url") or item.get("doi") or item.get("id")
    authors = []
    for author in item.get("authorships", []) if isinstance(item.get("authorships"), list) else []:
        author_obj = author.get("author") if isinstance(author, dict) else {}
        if isinstance(author_obj, dict) and author_obj.get("display_name"):
            authors.append(str(author_obj["display_name"]))
    return {
        "title": item.get("title") or item.get("display_name") or "",
        "url": landing or "",
        "openalex_id": item.get("id") or "",
        "doi": normalize_doi(item.get("doi")),
        "snippet": compact_abstract(item.get("abstract_inverted_index")),
        "year": item.get("publication_year"),
        "venue": (location.get("source") or {}).get("display_name")
        if isinstance(location.get("source"), dict)
        else "",
        "authors": authors[:8],
        "source_kind": "openalex",
        "type": item.get("type") or "",
        "cited_by_count": item.get("cited_by_count") or 0,
    }


def openalex_select_fields() -> str:
    return (
        "id,doi,title,display_name,publication_year,publication_date,"
        "primary_location,authorships,abstract_inverted_index,type,cited_by_count"
    )


def run_openalex_snowball_search(query: QuerySpec, max_sources: int, out_path: Path, timeout_sec: int) -> dict[str, Any] | None:
    if not query.query.startswith(("openalex_refs:", "openalex_cited_by:", "openalex_related:")):
        return None

    kind, raw_id = query.query.split(":", 1)
    work_id = normalize_openalex_work_id(raw_id)
    sources: list[dict[str, Any]] = []
    errors: list[dict[str, str]] = []
    try:
        if kind in {"openalex_refs", "openalex_related"}:
            work_url = f"https://api.openalex.org/works/{urllib.parse.quote(work_id)}"
            work = http_json(work_url, timeout_sec)
            key = "referenced_works" if kind == "openalex_refs" else "related_works"
            refs = [
                normalize_openalex_work_id(value)
                for value in work.get(key, [])
                if isinstance(value, str) and normalize_openalex_work_id(value)
            ][:max_sources]
            for ref in refs:
                try:
                    item = http_json(f"https://api.openalex.org/works/{urllib.parse.quote(ref)}", timeout_sec)
                    sources.append(openalex_work_to_source(item))
                except Exception as exc:  # noqa: BLE001 - persisted as discovery evidence.
                    errors.append({"backend": "openalex_snowball_work", "error": str(exc)[:500]})
        else:
            cited_url = (
                "https://api.openalex.org/works"
                f"?filter=cites:{urllib.parse.quote(work_id)}"
                f"&per-page={min(max_sources, 200)}"
                f"&select={openalex_select_fields()}"
            )
            data = http_json(cited_url, timeout_sec)
            for item in data.get("results", []):
                if isinstance(item, dict):
                    sources.append(openalex_work_to_source(item))
    except Exception as exc:  # noqa: BLE001 - persisted as discovery evidence.
        errors.append({"backend": "openalex_snowball", "error": str(exc)[:500]})

    payload = {
        "sources": sources[:max_sources],
        "query": query.query,
        "focus": query.focus,
        "resolver": "openalex-snowball",
        "errors": errors,
    }
    out_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    return payload


def run_open_metadata_search(
    query: QuerySpec,
    max_sources: int,
    out_path: Path,
    timeout_sec: int,
) -> dict[str, Any]:
    snowball_payload = run_openalex_snowball_search(query, max_sources, out_path, timeout_sec)
    if snowball_payload is not None:
        return snowball_payload

    encoded = urllib.parse.quote(query.query)
    per_backend = max(1, max_sources // 2)
    sources: list[dict[str, Any]] = []
    errors: list[dict[str, str]] = []

    openalex_url = (
        "https://api.openalex.org/works"
        f"?search={encoded}&per-page={min(per_backend, 200)}"
        f"&select={openalex_select_fields()}"
    )
    try:
        data = http_json(openalex_url, timeout_sec)
        for item in data.get("results", []):
            if not isinstance(item, dict):
                continue
            sources.append(openalex_work_to_source(item))
    except Exception as exc:  # noqa: BLE001 - persisted as discovery evidence.
        errors.append({"backend": "openalex", "error": str(exc)[:500]})

    crossref_url = f"https://api.crossref.org/works?query={encoded}&rows={min(per_backend, 100)}"
    try:
        data = http_json(crossref_url, timeout_sec)
        for item in (data.get("message") or {}).get("items", []):
            if not isinstance(item, dict):
                continue
            title = " ".join(item.get("title") or [])
            published = item.get("published-print") or item.get("published-online") or item.get("issued") or {}
            date_parts = published.get("date-parts") if isinstance(published, dict) else []
            year = date_parts[0][0] if date_parts and date_parts[0] else None
            authors = [
                " ".join(str(author.get(part, "")).strip() for part in ("given", "family")).strip()
                for author in item.get("author", [])
                if isinstance(author, dict)
            ]
            abstract = re.sub(r"<[^>]+>", " ", str(item.get("abstract") or ""))
            url = item.get("URL") or (f"https://doi.org/{item.get('DOI')}" if item.get("DOI") else "")
            sources.append(
                {
                    "title": title,
                    "url": url,
                    "openalex_id": "",
                    "doi": normalize_doi(item.get("DOI")),
                    "snippet": re.sub(r"\s+", " ", abstract).strip()[:1000],
                    "year": year,
                    "venue": " ".join(item.get("container-title") or []),
                    "authors": authors[:8],
                    "source_kind": "crossref",
                    "type": item.get("type") or "",
                    "cited_by_count": item.get("is-referenced-by-count") or 0,
                }
            )
    except Exception as exc:  # noqa: BLE001 - persisted as discovery evidence.
        errors.append({"backend": "crossref", "error": str(exc)[:500]})

    payload = {
        "sources": sources[:max_sources],
        "query": query.query,
        "focus": query.focus,
        "resolver": "openalex+crossref",
        "errors": errors,
    }
    out_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    return payload


def run_deep_research(
    query: QuerySpec,
    max_sources: int,
    out_path: Path,
    timeout_sec: int,
    backend: str,
) -> dict[str, Any]:
    if backend == "open-metadata":
        return run_open_metadata_search(query, max_sources, out_path, timeout_sec)

    cmd = [
        "ctox",
        "web",
        "deep-research",
        "--query",
        query.query,
        "--focus",
        query.focus,
        "--depth",
        "standard",
        "--max-sources",
        str(max_sources),
    ]
    try:
        proc = subprocess.run(
            cmd,
            check=True,
            text=True,
            capture_output=True,
            timeout=timeout_sec,
        )
    except subprocess.TimeoutExpired as exc:
        payload = {
            "sources": [],
            "error": "query_timeout",
            "timeout_sec": timeout_sec,
            "query": query.query,
            "focus": query.focus,
            "stderr": (exc.stderr or "")[-1000:] if isinstance(exc.stderr, str) else "",
        }
        out_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")
        return payload
    out_path.write_text(proc.stdout, encoding="utf-8")
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"ctox web deep-research did not return JSON for {query.query!r}") from exc


def source_records(payload: dict[str, Any]) -> list[dict[str, Any]]:
    for key in ("sources", "results", "items", "hits", "records", "papers"):
        value = payload.get(key)
        if isinstance(value, list):
            return [item for item in value if isinstance(item, dict)]
    return []


def source_key(record: dict[str, Any]) -> str:
    for key in ("doi", "DOI", "url", "canonical_url", "link", "id"):
        value = record.get(key)
        if isinstance(value, str) and value.strip():
            return f"{key.lower()}:{value.strip().lower()}"
    title = str(record.get("title") or "").strip().lower()
    return "title:" + re.sub(r"\s+", " ", title)


def extract_url(record: dict[str, Any]) -> str:
    for key in ("url", "canonical_url", "link", "source_url"):
        value = record.get(key)
        if isinstance(value, str) and value.strip():
            return value.strip()
    return ""


def extract_doi(record: dict[str, Any]) -> str:
    for key in ("doi", "DOI"):
        value = record.get(key)
        if isinstance(value, str) and value.strip():
            return value.strip()
    text = " ".join(str(record.get(k) or "") for k in ("url", "title", "snippet"))
    match = re.search(r"\b10\.\d{4,9}/[-._;()/:A-Za-z0-9]+\b", text)
    return match.group(0) if match else ""


DRONE_DOMAIN_TERMS = (
    "drone",
    "drones",
    "uas",
    "uav",
    "suas",
    "rpas",
    "remotely piloted aircraft",
    "unmanned aircraft",
    "unmanned aerial",
    "unmanned aerial vehicle",
    "unmanned aircraft system",
    "multirotor",
    "multi-rotor",
    "quadrotor",
    "fixed-wing unmanned",
    "micro air vehicle",
    "mav",
)

LOAD_DATA_TERMS = (
    "load",
    "loads",
    "payload",
    "mtow",
    "takeoff weight",
    "take-off weight",
    "gross weight",
    "maximum gross",
    "empty weight",
    "thrust",
    "torque",
    "force",
    "moment",
    "force/moment",
    "force and moment",
    "load cell",
    "propeller",
    "rotor",
    "rpm",
    "current",
    "voltage",
    "power coefficient",
    "thrust coefficient",
    "wind tunnel",
    "aerodynamic",
    "structural",
    "airframe",
    "flight log",
    "telemetry",
    "ardupilot",
    "px4",
    "classification",
    "group 1",
    "group 2",
)

REJECT_HINT_TERMS = (
    "african invertebrates",
    "taxonomy",
    "coleoptera",
    "carabidae",
    "rfigshare",
    "cran.package",
    "moodle",
    "neuroblastoma",
    "tumor",
    "cancer",
    "fetal",
    "parasite",
    "covid",
    "pneumonia",
    "diarrhea",
)


def source_text(record: dict[str, Any]) -> str:
    fields = (
        record.get("title"),
        record.get("name"),
        record.get("snippet"),
        record.get("summary"),
        record.get("abstract"),
        record.get("url"),
        record.get("canonical_url"),
        record.get("link"),
        record.get("source_url"),
        record.get("doi"),
        record.get("DOI"),
    )
    return re.sub(r"\s+", " ", " ".join(str(value or "") for value in fields)).strip().lower()


def source_acceptance(record: dict[str, Any], topic: str) -> tuple[bool, str]:
    """Gate a screened source before it enters the usable candidate catalog.

    Query text is intentionally excluded. A hit is not accepted just because the
    search query contained "UAV" or "payload"; the returned source itself must
    carry both a drone/UAS signal and a load-data signal.
    """

    text = source_text(record)
    if not text:
        return False, "empty_source_metadata"
    if any(term in text for term in REJECT_HINT_TERMS):
        return False, "known_off_topic_metadata_hit"

    domain_hits = [term for term in DRONE_DOMAIN_TERMS if term in text]
    load_hits = [term for term in LOAD_DATA_TERMS if term in text]
    if not domain_hits:
        return False, "missing_drone_uas_context_in_source"
    if not load_hits:
        return False, "missing_load_payload_thrust_aero_context_in_source"

    return True, f"accepted:domain={domain_hits[0]};load={load_hits[0]}"


def source_relevance_score(record: dict[str, Any]) -> int:
    """Return a deterministic 0-100 score for accepted source triage."""

    text = source_text(record)
    if not text:
        return 0
    direct_measurement_terms = (
        "thrust stand",
        "load cell",
        "force moment",
        "force/moment",
        "force and moment",
        "wind tunnel",
        "telemetry",
        "flight log",
        "rpm",
        "current",
        "voltage",
        "torque",
        "propeller",
        "rotor",
        "aerodynamic",
        "structural loads",
        "airframe loads",
    )
    mass_scope_terms = (
        "payload",
        "mtow",
        "takeoff weight",
        "take-off weight",
        "gross weight",
        "maximum gross",
        "group 1",
        "group 2",
        "classification",
    )
    weak_context_terms = (
        "review",
        "survey",
        "applications",
        "monitoring",
        "mapping",
        "precision agriculture",
        "hydrology",
        "medical",
        "surgical",
    )

    score = 0
    score += min(15, 3 * sum(1 for term in DRONE_DOMAIN_TERMS if term in text))
    score += min(45, 9 * sum(1 for term in direct_measurement_terms if term in text))
    score += min(25, 5 * sum(1 for term in mass_scope_terms if term in text))
    if extract_doi(record):
        score += 5
    if any(term in text for term in ("dataset", "database", "csv", "download", "repository", "github", "zenodo", "figshare")):
        score += 10
    if any(term in text for term in weak_context_terms) and not any(term in text for term in direct_measurement_terms):
        score -= 25
    if any(term in text for term in REJECT_HINT_TERMS):
        score -= 50
    return max(0, min(100, score))


def write_summary(path: Path, spec: QuerySpec, records: list[dict[str, Any]]) -> None:
    lines = [f"Query: {spec.query}", f"Focus: {spec.focus}", f"Sources returned: {len(records)}", ""]
    for idx, record in enumerate(records[:25], start=1):
        title = str(record.get("title") or record.get("name") or "(untitled)").strip()
        url = extract_url(record)
        lines.append(f"{idx}. {title} {url}".strip())
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def register_research_log(
    run_id: str,
    spec: QuerySpec,
    count: int,
    summary: Path,
    raw: Path,
    resolver: str,
) -> str:
    cmd = [
        "ctox",
        "report",
        "research-log-add",
        "--run-id",
        run_id,
        "--question",
        spec.query,
        "--focus",
        spec.focus,
        "--resolver",
        resolver,
        "--sources-count",
        str(count),
        "--summary-file",
        str(summary),
        "--raw-payload-file",
        str(raw),
    ]
    proc = subprocess.run(cmd, check=True, text=True, capture_output=True)
    match = re.search(r"research_id:\s+(\S+)", proc.stdout)
    return match.group(1) if match else ""


def build_snowball_queries(topic: str, candidates: list[dict[str, str]], limit: int) -> list[QuerySpec]:
    out: list[QuerySpec] = []
    ranked = sorted(candidates, key=lambda row: int(row.get("relevance_score") or 0), reverse=True)
    for row in ranked:
        title = row.get("title", "")
        doi = row.get("doi", "")
        openalex_id = row.get("openalex_id", "")
        if openalex_id:
            work_id = normalize_openalex_work_id(openalex_id)
            out.append(QuerySpec("snowball_openalex_refs", f"openalex_refs:{work_id}"))
            out.append(QuerySpec("snowball_openalex_cited_by", f"openalex_cited_by:{work_id}"))
            out.append(QuerySpec("snowball_openalex_related", f"openalex_related:{work_id}"))
        elif doi:
            out.append(QuerySpec("snowball_doi_references", f"{doi} references cited by related work {topic}"))
            out.append(QuerySpec("snowball_doi_cited_by", f"{doi} cited by related work {topic}"))
        elif title:
            compact = " ".join(title.split()[:10])
            out.append(QuerySpec("snowball_title", f"{compact} references cited by related work {topic}"))
        if len(out) >= limit:
            break
    return out


def write_outputs(
    out_dir: Path,
    protocol_rows: list[dict[str, Any]],
    candidates: list[dict[str, str]],
    screened_sources: list[dict[str, str]],
    rejected_sources: list[dict[str, str]],
    query_plan: list[QuerySpec],
    research_ids: list[str],
) -> None:
    candidates = sorted(candidates, key=lambda row: int(row.get("relevance_score") or 0), reverse=True)
    save_query_plan(out_dir / "query_plan.csv", query_plan)
    with (out_dir / "search_protocol.csv").open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=[
                "focus",
                "query",
                "reviewed_results",
                "unique_new_sources",
                "excluded_or_duplicate",
                "research_id",
                "raw_payload",
            ],
        )
        writer.writeheader()
        writer.writerows(protocol_rows)
    with (out_dir / "candidate_sources.csv").open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=[
                "focus",
                "query",
                "title",
                "url",
                "doi",
                "openalex_id",
                "snippet",
                "relevance_score",
                "acceptance_reason",
            ],
        )
        writer.writeheader()
        writer.writerows(candidates)
    with (out_dir / "screened_sources.csv").open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=[
                "focus",
                "query",
                "title",
                "url",
                "doi",
                "openalex_id",
                "snippet",
                "screening_status",
                "screening_reason",
            ],
        )
        writer.writeheader()
        writer.writerows(screened_sources)
    with (out_dir / "rejected_sources.csv").open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=[
                "focus",
                "query",
                "title",
                "url",
                "doi",
                "openalex_id",
                "snippet",
                "screening_status",
                "screening_reason",
            ],
        )
        writer.writeheader()
        writer.writerows(rejected_sources)
    (out_dir / "research_ids.txt").write_text("\n".join(r for r in research_ids if r) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--topic", required=True)
    parser.add_argument("--run-id")
    parser.add_argument("--out-dir", required=True)
    parser.add_argument("--queries-file")
    parser.add_argument("--max-sources-per-query", type=int, default=80)
    parser.add_argument("--target-reviewed", type=int, default=1000)
    parser.add_argument("--query-timeout-sec", type=int, default=240)
    parser.add_argument(
        "--discovery-backend",
        choices=["ctox-deep-research", "open-metadata"],
        default="ctox-deep-research",
    )
    parser.add_argument("--snowball-rounds", type=int, default=1)
    parser.add_argument("--snowball-limit", type=int, default=12)
    parser.add_argument("--plan-only", action="store_true")
    args = parser.parse_args()

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    query_plan = load_query_plan(Path(args.queries_file)) if args.queries_file else default_query_plan(args.topic)
    save_query_plan(out_dir / "query_plan.csv", query_plan)
    if args.plan_only:
        print(out_dir / "query_plan.csv")
        return 0

    seen: set[str] = set()
    candidates: list[dict[str, str]] = []
    screened_sources: list[dict[str, str]] = []
    rejected_sources: list[dict[str, str]] = []
    protocol_rows: list[dict[str, Any]] = []
    research_ids: list[str] = []
    queue = list(query_plan)
    rounds_remaining = max(0, args.snowball_rounds)
    reviewed_total = 0

    idx = 0
    while queue:
        idx += 1
        spec = queue.pop(0)
        raw_path = out_dir / "raw" / f"{idx:03d}_{slugify(spec.focus)}.json"
        summary_path = out_dir / "summaries" / f"{idx:03d}_{slugify(spec.focus)}.md"
        raw_path.parent.mkdir(parents=True, exist_ok=True)
        summary_path.parent.mkdir(parents=True, exist_ok=True)
        payload = run_deep_research(
            spec,
            args.max_sources_per_query,
            raw_path,
            args.query_timeout_sec,
            args.discovery_backend,
        )
        records = source_records(payload)
        reviewed_total += len(records)
        write_summary(summary_path, spec, records)

        new_count = 0
        for record in records:
            key = source_key(record)
            if not key or key in seen:
                continue
            seen.add(key)
            new_count += 1
            row = {
                "focus": spec.focus,
                "query": spec.query,
                "title": str(record.get("title") or record.get("name") or "").strip(),
                "url": extract_url(record),
                "doi": extract_doi(record),
                "openalex_id": str(record.get("openalex_id") or "").strip(),
                "snippet": str(record.get("snippet") or record.get("summary") or "").strip()[:500],
            }
            accepted, reason = source_acceptance(record, args.topic)
            screened_row = {
                **row,
                "screening_status": "accepted" if accepted else "rejected",
                "screening_reason": reason,
            }
            screened_sources.append(screened_row)
            if accepted:
                candidates.append({**row, "relevance_score": str(source_relevance_score(record)), "acceptance_reason": reason})
            else:
                rejected_sources.append(screened_row)

        research_id = ""
        if args.run_id:
            resolver = "openalex+crossref" if args.discovery_backend == "open-metadata" else "ctox web deep-research"
            research_id = register_research_log(args.run_id, spec, len(records), summary_path, raw_path, resolver)
            research_ids.append(research_id)
        protocol_rows.append(
            {
                "focus": spec.focus,
                "query": spec.query,
                "reviewed_results": len(records),
                "unique_new_sources": new_count,
                "excluded_or_duplicate": max(0, len(records) - new_count),
                "research_id": research_id,
                "raw_payload": str(raw_path),
            }
        )

        if not queue and rounds_remaining > 0:
            rounds_remaining -= 1
            queue.extend(build_snowball_queries(args.topic, candidates, args.snowball_limit))

    write_outputs(out_dir, protocol_rows, candidates, screened_sources, rejected_sources, query_plan, research_ids)
    summary_obj = {
        "out_dir": str(out_dir),
        "queries_run": len(protocol_rows),
        "reviewed_results": reviewed_total,
        "unique_sources": len(candidates),
        "screened_unique_sources": len(screened_sources),
        "rejected_sources": len(rejected_sources),
        "research_logs": len([r for r in research_ids if r]),
        "research_ids_file": str(out_dir / "research_ids.txt"),
        "search_protocol_csv": str(out_dir / "search_protocol.csv"),
        "candidate_sources_csv": str(out_dir / "candidate_sources.csv"),
        "screened_sources_csv": str(out_dir / "screened_sources.csv"),
        "rejected_sources_csv": str(out_dir / "rejected_sources.csv"),
    }
    (out_dir / "summary.json").write_text(json.dumps(summary_obj, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary_obj, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
