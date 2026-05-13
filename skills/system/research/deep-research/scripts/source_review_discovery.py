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
import html
import json
import os
import re
import subprocess
import sys
import time
import tempfile
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
    q = natural_search_phrase(t)
    seeds = seed_query_plan(t)
    families: list[tuple[str, list[str]]] = [
        (
            "web",
            [
                f"{q} sources",
                f"{q} data",
                f"{q} documentation",
                f"{q} technical report",
                f"{q} examples",
            ],
        ),
        (
            "scholarly",
            [
                f"{q} review paper",
                f"{q} experimental study data",
                f"{q} model validation dataset",
                f"{q} measurement campaign",
                f"{q} methods evaluation validation data",
                f"{q} open data DOI",
            ],
        ),
        (
            "agency",
            [
                f"{q} government agency official report",
                f"{q} government technical report",
                f"{q} regulatory guidance data",
                f"{q} safety assessment authority",
                f"{q} public agency dataset",
            ],
        ),
        (
            "standards",
            [
                f"{q} standard specification",
                f"{q} standard test method",
                f"{q} industry standard",
                f"{q} qualification test standard",
            ],
        ),
        (
            "reports",
            [
                f"{q} technical report PDF",
                f"{q} thesis dissertation data",
                f"{q} institutional repository technical report",
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
                f"{q} OEM specification",
            ],
        ),
        (
            "patent",
            [
                f"{q} patent technical data",
                f"{q} patent technical report",
                f"{q} invention measurement system",
            ],
        ),
    ]
    plan = seeds
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
    compact = re.sub(r"(?i)^[a-z0-9 /+-]{0,40}\bdata\s+sources\s+for\s+", "", compact)
    compact = re.sub(r"(?i)^(data|information|evidence|technical data)\s+sources\s+for\s+", "", compact)
    return compact or topic.strip()


def natural_search_phrase(topic: str, limit: int = 8) -> str:
    """Compress an operator prompt into a plain search-engine phrase."""

    terms = query_core_terms(topic, limit=limit)
    if terms:
        return " ".join(terms)
    compact = normalized_topic_for_query(topic)
    compact = re.sub(r"\([^)]{0,120}\)", " ", compact)
    compact = re.sub(r"\b(particularly|especially|according to|relevant for)\b", " ", compact, flags=re.IGNORECASE)
    return re.sub(r"\s+", " ", compact).strip()


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


def seed_query_plan(query: str) -> list[QuerySpec]:
    """Create short, obvious first-pass queries before broad expansion.

    The seed phase is deliberately generic. It uses only operator-provided
    words and universal source containers, so it works for drones, medicine,
    legal topics, markets, materials, software, or any other source review
    without benchmark-specific shortcuts.
    """

    raw = re.sub(r"\s+", " ", query).strip()
    q = natural_search_phrase(raw)
    terms = query_core_terms(raw, limit=6)
    compact = " ".join(terms[:6]) if terms else q
    named = named_entities_from_query(raw)
    source_hints = ["dataset", "database", "documentation", "technical report", "standard", "github"]
    seeds = [
        QuerySpec("seed_obvious", q),
        QuerySpec("seed_obvious", compact),
    ]
    if len(terms) >= 4:
        seeds.append(QuerySpec("seed_obvious", " ".join(terms[:3] + terms[-2:])))
    if len(terms) >= 6:
        seeds.append(QuerySpec("seed_obvious", " ".join(terms[:2] + terms[3:6])))
    for hint in source_hints:
        seeds.append(QuerySpec("seed_obvious", f"{compact} {hint}"))
    for entity in named[:6]:
        seeds.append(QuerySpec("seed_named_entity", entity))
        for hint in source_hints[:3]:
            seeds.append(QuerySpec("seed_named_entity", f"{entity} {hint}"))
    return dedupe_query_plan(seeds)


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


def load_csv_rows(path: Path) -> list[dict[str, str]]:
    if not path.exists():
        return []
    with path.open(newline="", encoding="utf-8") as handle:
        return [dict(row) for row in csv.DictReader(handle)]


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


def http_text(url: str, timeout_sec: int) -> str:
    request = urllib.request.Request(
        url,
        headers={
            "User-Agent": (
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
                "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124 Safari/537.36"
            )
        },
    )
    with urllib.request.urlopen(request, timeout=timeout_sec) as response:
        return response.read().decode("utf-8", errors="ignore")


def strip_html(value: str) -> str:
    value = re.sub(r"<[^>]+>", " ", value)
    value = html.unescape(value)
    return re.sub(r"\s+", " ", value).strip()


def normalize_duckduckgo_url(raw: str) -> str:
    value = html.unescape(raw or "").strip()
    if not value:
        return ""
    if value.startswith("//"):
        value = "https:" + value
    parsed = urllib.parse.urlparse(value)
    if parsed.netloc.endswith("duckduckgo.com") and parsed.path.startswith("/l/"):
        target = urllib.parse.parse_qs(parsed.query).get("uddg", [""])[0]
        if target:
            return target
    return value


QUERY_SOFTENERS = {
    "dataset",
    "report",
    "reports",
    "xlsx",
    "xls",
    "csv",
    "pdf",
    "download",
    "downloads",
    "source",
    "sources",
    "review",
    "public",
    "technical",
    "table",
    "tables",
}


def query_variants(query: str) -> list[str]:
    """Return progressively simpler web-search queries.

    Metadata APIs tolerate long descriptive queries; web search often does not.
    This keeps the first pass general by preserving named entities and core
    nouns, then tries simpler variants when the full query has no parseable
    results. It is intentionally domain-agnostic.
    """

    normalized = re.sub(r"\s+", " ", query).strip()
    tokens = normalized.split()
    variants = [normalized]
    if len(tokens) > 5:
        kept = [token for token in tokens if token.lower().strip(".,;:()[]{}") not in QUERY_SOFTENERS]
        if 2 <= len(kept) < len(tokens):
            variants.append(" ".join(kept))
        variants.append(" ".join(tokens[:5]))
        variants.append(" ".join(tokens[:4]))

    named_indexes = [
        idx
        for idx, token in enumerate(tokens)
        if re.fullmatch(r"[A-Z][A-Za-z0-9+-]{1,}", token) or re.fullmatch(r"[A-Z0-9]{2,}", token)
    ]
    for idx in named_indexes:
        window = tokens[idx : min(len(tokens), idx + 4)]
        if len(window) >= 2:
            variants.append(" ".join(window))
        tail = [
            token
            for token in tokens[idx + 1 :]
            if token.lower().strip(".,;:()[]{}") not in QUERY_SOFTENERS
        ][:4]
        if tail:
            variants.append(" ".join([tokens[idx], *tail]))

    out: list[str] = []
    seen: set[str] = set()
    for variant in variants:
        cleaned = re.sub(r"\s+", " ", variant).strip()
        key = cleaned.lower()
        if cleaned and key not in seen:
            seen.add(key)
            out.append(cleaned)
    return out


def parse_duckduckgo_results(page: str, max_sources: int) -> list[dict[str, Any]]:
    """Parse DuckDuckGo's HTML endpoint into source records.

    This deliberately avoids scholarly-only APIs. Source reviews need to find
    datasets, public databases, vendor performance pages, technical docs and
    repositories; many of those never appear as DOI/OpenAlex records.
    """

    results: list[dict[str, Any]] = []
    blocks = re.split(r'<div[^>]+class="[^"]*result[^"]*"[^>]*>', page)
    for block in blocks[1:]:
        link_match = re.search(
            r'<a[^>]+class="[^"]*result__a[^"]*"[^>]+href="([^"]+)"[^>]*>(.*?)</a>',
            block,
            flags=re.IGNORECASE | re.DOTALL,
        )
        if not link_match:
            link_match = re.search(r'<a[^>]+href="([^"]+)"[^>]*>(.*?)</a>', block, flags=re.IGNORECASE | re.DOTALL)
        if not link_match:
            continue
        url = normalize_duckduckgo_url(link_match.group(1))
        title = strip_html(link_match.group(2))
        snippet_match = re.search(
            r'<a[^>]+class="[^"]*result__snippet[^"]*"[^>]*>(.*?)</a>|'
            r'<div[^>]+class="[^"]*result__snippet[^"]*"[^>]*>(.*?)</div>',
            block,
            flags=re.IGNORECASE | re.DOTALL,
        )
        snippet = ""
        if snippet_match:
            snippet = strip_html(next(group for group in snippet_match.groups() if group))
        if not title or not url.startswith(("http://", "https://")):
            continue
        results.append(
            {
                "title": title,
                "url": url,
                "snippet": snippet,
                "source_kind": "web",
                "resolver": "duckduckgo-html",
            }
        )
        if len(results) >= max_sources:
            break
    return results


def js_string_unescape(value: str) -> str:
    try:
        return json.loads(f'"{value}"')
    except Exception:
        return html.unescape(value.replace(r"\/", "/"))


def is_search_noise_url(url: str) -> bool:
    try:
        parsed = urllib.parse.urlparse(url)
    except Exception:
        return True
    host = parsed.netloc.lower()
    if not host:
        return True
    noise_hosts = {
        "search.brave.com",
        "cdn.search.brave.com",
        "www.google.com",
        "www.bing.com",
        "search.yahoo.com",
        "duckduckgo.com",
        "html.duckduckgo.com",
        "lite.duckduckgo.com",
    }
    return host in noise_hosts or host.endswith(".search.brave.com")


def parse_brave_results(page: str, max_sources: int) -> list[dict[str, Any]]:
    """Parse Brave Search's inline result data.

    Brave's server-rendered page contains result objects in a JavaScript data
    blob. The fields are stable enough for source discovery and avoid relying on
    one brittle search provider. We only extract external result URLs and keep
    the raw title/snippet as evidence for later screening.
    """

    results: list[dict[str, Any]] = []
    seen: set[str] = set()
    pattern = re.compile(
        r'title:"(?P<title>(?:\\.|[^"\\])*)",url:"(?P<url>https?://(?:\\.|[^"\\])*)".{0,1200}?description:(?:"(?P<description>(?:\\.|[^"\\])*)"|void 0)',
        flags=re.DOTALL,
    )
    for match in pattern.finditer(page):
        url = js_string_unescape(match.group("url"))
        if is_search_noise_url(url) or url in seen:
            continue
        seen.add(url)
        title = strip_html(js_string_unescape(match.group("title")))
        description = strip_html(js_string_unescape(match.group("description") or ""))
        if not title:
            title = urllib.parse.urlparse(url).netloc
        results.append(
            {
                "title": title,
                "url": url,
                "snippet": description,
                "source_kind": "web",
                "resolver": "brave-html",
            }
        )
        if len(results) >= max_sources:
            return results

    for raw_url in re.findall(r"https?://[^\"<>\\\s]+", page):
        url = js_string_unescape(raw_url).rstrip("),.;")
        if is_search_noise_url(url) or url in seen:
            continue
        seen.add(url)
        results.append(
            {
                "title": urllib.parse.urlparse(url).netloc,
                "url": url,
                "snippet": "",
                "source_kind": "web",
                "resolver": "brave-html-url-extract",
            }
        )
        if len(results) >= max_sources:
            break
    return results


def parse_bing_rss_results(page: str, max_sources: int) -> list[dict[str, Any]]:
    results: list[dict[str, Any]] = []
    for item in re.findall(r"<item>(.*?)</item>", page, flags=re.IGNORECASE | re.DOTALL):
        title_match = re.search(r"<title>(.*?)</title>", item, flags=re.IGNORECASE | re.DOTALL)
        link_match = re.search(r"<link>(.*?)</link>", item, flags=re.IGNORECASE | re.DOTALL)
        description_match = re.search(r"<description>(.*?)</description>", item, flags=re.IGNORECASE | re.DOTALL)
        title = strip_html(title_match.group(1)) if title_match else ""
        url = html.unescape(strip_html(link_match.group(1))) if link_match else ""
        snippet = strip_html(description_match.group(1)) if description_match else ""
        if not title or not url.startswith(("http://", "https://")) or is_search_noise_url(url):
            continue
        results.append(
            {
                "title": title,
                "url": url,
                "snippet": snippet,
                "source_kind": "web",
                "resolver": "bing-rss",
            }
        )
        if len(results) >= max_sources:
            break
    return results


def significant_query_terms(query: str) -> list[str]:
    terms: list[str] = []
    for raw in re.findall(r"[A-Za-z0-9][A-Za-z0-9+.-]{1,}", query.lower()):
        term = raw.strip(".-")
        if len(term) < 3 or term in QUERY_SOFTENERS or term in TOPIC_STOPWORDS:
            continue
        terms.append(term)
    return list(dict.fromkeys(terms))


def web_sources_quality_ok(query: str, sources: list[dict[str, Any]]) -> bool:
    if not sources:
        return False
    terms = significant_query_terms(query)
    if len(terms) < 2:
        return True
    scored = 0
    for source in sources[: min(5, len(sources))]:
        text = source_text(source)
        hit_count = sum(1 for term in terms if term in text)
        if hit_count >= 2:
            scored += 1
    return scored > 0


def fetch_search_page_with_retry(url: str, query: str, timeout_sec: int, errors: list[dict[str, str]]) -> str:
    for attempt in range(2):
        page = http_text(url, timeout_sec)
        if "anomaly-modal" not in page and "Unfortunately, bots use DuckDuckGo too" not in page:
            return page
        errors.append(
            {
                "backend": "duckduckgo-html",
                "error": "rate_limited_or_anomaly",
                "url": url,
                "query": query,
                "attempt": str(attempt + 1),
            }
        )
        time.sleep(1.0)
    return page


SEARCH_PROVIDER_COOLDOWN_UNTIL: dict[str, float] = {}


def search_provider_sequence() -> list[str]:
    raw = os.environ.get("CTOX_SOURCE_REVIEW_WEB_PROVIDERS", "ctox-web-search,brave-html,duckduckgo-html,bing-rss")
    providers: list[str] = []
    for item in raw.split(","):
        provider = item.strip()
        if provider and provider not in providers:
            providers.append(provider)
    if os.environ.get("CTOX_SOURCE_REVIEW_DDG_FALLBACK", "1") == "0":
        providers = [provider for provider in providers if provider != "duckduckgo-html"]
    return providers or ["brave-html"]


def search_provider_url(provider: str, encoded_query: str) -> str:
    if provider == "ctox-web-search":
        return f"ctox web search:{encoded_query}"
    if provider == "brave-html":
        return f"https://search.brave.com/search?q={encoded_query}"
    if provider == "bing-rss":
        return f"https://www.bing.com/search?format=rss&q={encoded_query}"
    if provider == "duckduckgo-html":
        return f"https://html.duckduckgo.com/html/?q={encoded_query}"
    raise ValueError(f"unsupported search provider: {provider}")


def parse_provider_sources(provider: str, page: str, max_sources: int) -> list[dict[str, Any]]:
    if provider == "ctox-web-search":
        return parse_ctox_web_search_results(page, max_sources)
    if provider == "brave-html":
        return parse_brave_results(page, max_sources)
    if provider == "bing-rss":
        return parse_bing_rss_results(page, max_sources)
    if provider == "duckduckgo-html":
        return parse_duckduckgo_results(page, max_sources)
    raise ValueError(f"unsupported search provider: {provider}")


def parse_ctox_web_search_results(payload_text: str, max_sources: int) -> list[dict[str, Any]]:
    payload = json.loads(payload_text)
    out: list[dict[str, Any]] = []
    for item in payload.get("results") or []:
        if not isinstance(item, dict):
            continue
        url = str(item.get("url") or "").strip()
        title = str(item.get("title") or "").strip()
        if not url or is_search_noise_url(url):
            continue
        out.append(
            {
                "title": title or urllib.parse.urlparse(url).netloc,
                "url": url,
                "snippet": str(item.get("snippet") or item.get("summary") or "").strip(),
                "source_kind": "web",
                "resolver": str(payload.get("provider") or "ctox-web-search"),
            }
        )
        if len(out) >= max_sources:
            break
    return out


def run_ctox_web_search_provider(query: str, timeout_sec: int) -> str:
    proc = subprocess.run(
        ["ctox", "web", "search", "--query", query, "--include-sources"],
        check=True,
        text=True,
        capture_output=True,
        timeout=timeout_sec,
    )
    return proc.stdout


def run_web_search(query: QuerySpec, max_sources: int, out_path: Path, timeout_sec: int, delay_sec: float = 3.0) -> dict[str, Any]:
    errors: list[dict[str, str]] = []
    sources: list[dict[str, Any]] = []
    attempted: list[str] = []
    matched_query = ""
    web_timeout_sec = max(6, min(timeout_sec, 12))
    max_variants = max(1, int(os.environ.get("CTOX_SOURCE_REVIEW_MAX_WEB_QUERY_VARIANTS", "2")))
    providers = search_provider_sequence()
    for variant_index, variant in enumerate(query_variants(query.query)[:max_variants]):
        if variant_index:
            time.sleep(max(1.0, delay_sec))
        encoded = urllib.parse.quote(variant)
        attempted.append(variant)
        for resolver in providers:
            url = search_provider_url(resolver, encoded)
            cooldown_until = SEARCH_PROVIDER_COOLDOWN_UNTIL.get(resolver, 0.0)
            now = time.time()
            if cooldown_until > now:
                errors.append(
                    {
                        "backend": resolver,
                        "error": "provider_cooldown_after_rate_limit",
                        "url": url,
                        "query": variant,
                        "cooldown_remaining_sec": str(int(cooldown_until - now)),
                    }
                )
                continue
            try:
                if resolver == "ctox-web-search":
                    page = run_ctox_web_search_provider(variant, max(web_timeout_sec, 12))
                else:
                    page = fetch_search_page_with_retry(url, variant, web_timeout_sec, errors)
                sources = parse_provider_sources(resolver, page, max_sources)
                if sources:
                    if not web_sources_quality_ok(variant, sources):
                        errors.append({"backend": resolver, "error": "low_relevance_results_rejected", "url": url, "query": variant})
                        sources = []
                        continue
                    matched_query = variant
                    break
                errors.append({"backend": resolver, "error": "no_results_parsed", "url": url, "query": variant})
            except Exception as exc:  # noqa: BLE001 - persisted for audit.
                message = str(exc)[:500]
                errors.append({"backend": resolver, "error": message, "url": url, "query": variant})
                if "HTTP Error 429" in message or "Too Many Requests" in message:
                    SEARCH_PROVIDER_COOLDOWN_UNTIL[resolver] = time.time() + max(60.0, delay_sec * 8)
                    break
        if sources:
            break
    payload = {
        "sources": sources[:max_sources],
        "query": query.query,
        "matched_query": matched_query,
        "attempted_queries": attempted,
        "focus": query.focus,
        "resolver": "+".join(providers),
        "errors": errors,
    }
    out_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    return payload


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
    web_query_delay_sec: float = 3.0,
) -> dict[str, Any]:
    if backend == "open-metadata":
        return run_open_metadata_search(query, max_sources, out_path, timeout_sec)
    if backend == "web-search":
        return run_web_search(query, max_sources, out_path, timeout_sec, web_query_delay_sec)
    if backend == "hybrid":
        if query.query.startswith(("openalex_refs:", "openalex_cited_by:", "openalex_related:")):
            return run_open_metadata_search(query, max_sources, out_path, timeout_sec)
        if query.focus in {"scholarly", "snowball_openalex_refs", "snowball_openalex_cited_by", "snowball_openalex_related"}:
            metadata_payload = run_open_metadata_search(query, max_sources // 2 or 1, out_path, timeout_sec)
            web_path = out_path.with_name(out_path.stem + "_web.json")
            web_payload = run_web_search(
                query,
                max_sources - len(source_records(metadata_payload)),
                web_path,
                timeout_sec,
                web_query_delay_sec,
            )
            merged = {
                "sources": source_records(metadata_payload) + source_records(web_payload),
                "query": query.query,
                "focus": query.focus,
                "resolver": "hybrid:openalex+crossref+duckduckgo-html",
                "errors": (metadata_payload.get("errors") or []) + (web_payload.get("errors") or []),
                "payloads": [str(out_path), str(web_path)],
            }
            out_path.write_text(json.dumps(merged, indent=2), encoding="utf-8")
            return merged
        return run_web_search(query, max_sources, out_path, timeout_sec, web_query_delay_sec)

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


SOURCE_EVIDENCE_TERMS = (
    "dataset",
    "database",
    "data set",
    "data",
    "measurements",
    "measured",
    "experimental",
    "experiment",
    "benchmark",
    "repository",
    "download",
    "csv",
    "xlsx",
    "doi",
    "technical report",
    "report",
    "datasheet",
    "specification",
    "manual",
    "standard",
    "guideline",
    "documentation",
    "docs",
    "spec",
    "table",
    "paper",
    "article",
    "preprint",
    "thesis",
    "dissertation",
    "white paper",
    "case study",
    "benchmark",
    "api",
    "portal",
    "catalog",
    "catalogue",
)

TOPIC_STOPWORDS = {
    "research",
    "source",
    "sources",
    "review",
    "especially",
    "those",
    "range",
    "according",
    "classification",
    "class",
    "classes",
    "with",
    "from",
    "into",
    "for",
    "and",
    "the",
    "those",
    "that",
    "about",
    "into",
    "using",
    "include",
    "including",
    "especially",
    "particularly",
    "relevant",
    "scope",
    "department",
    "u.s",
    "us",
    "usa",
    "review",
    "find",
    "all",
    "possible",
}

SOURCE_AUTHORITY_HOSTS = (
    ".gov",
    ".edu",
    ".ac.",
    "github.com",
    "gitlab.com",
    "zenodo.org",
    "figshare.com",
    "dataverse",
    "osf.io",
    "openalex.org",
    "doi.org",
    "crossref.org",
    "arxiv.org",
    "nasa.gov",
    "nist.gov",
    "europa.eu",
    "who.int",
    "oecd.org",
    "worldbank.org",
)


def query_core_terms(topic: str, limit: int = 12) -> list[str]:
    terms: list[str] = []
    for raw in re.findall(r"[A-Za-z0-9][A-Za-z0-9+./-]{1,}", normalized_topic_for_query(topic)):
        term = raw.strip(".,;:()[]{}").lower()
        if len(term) < 3 or term in TOPIC_STOPWORDS or term in QUERY_SOFTENERS:
            continue
        terms.append(term)
    return list(dict.fromkeys(terms))[:limit]


def named_entities_from_query(topic: str) -> list[str]:
    entities: list[str] = []
    tokens = normalized_topic_for_query(topic).split()
    for size in (4, 3, 2, 1):
        for idx in range(0, max(0, len(tokens) - size + 1)):
            window = tokens[idx : idx + size]
            while window and window[0].strip(".,;:()[]{}").lower() in TOPIC_STOPWORDS:
                window = window[1:]
            while window and window[-1].strip(".,;:()[]{}").lower() in TOPIC_STOPWORDS:
                window = window[:-1]
            text = " ".join(window).strip(" ,.;:")
            if not text:
                continue
            if any(re.search(r"[A-Z]", token) and re.fullmatch(r"[A-Z0-9][A-Z0-9+./-]{1,}", token.strip(".,;:()[]{}")) for token in window):
                entities.append(text)
            elif size > 1 and sum(1 for token in window if token[:1].isupper()) >= 2:
                entities.append(text)
    out: list[str] = []
    seen: set[str] = set()
    for entity in entities:
        key = entity.lower()
        if key not in seen:
            seen.add(key)
            out.append(entity)
    return out


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


def contains_term(text: str, term: str) -> bool:
    needle = re.sub(r"\s+", " ", term.lower()).strip()
    if not needle:
        return False
    if re.fullmatch(r"[a-z0-9][a-z0-9+./-]*", needle):
        return re.search(rf"(?<![a-z0-9]){re.escape(needle)}(?![a-z0-9])", text) is not None
    return needle in text


def matching_terms(text: str, terms: tuple[str, ...] | list[str]) -> list[str]:
    return [term for term in terms if contains_term(text, term)]


def topic_evidence_terms(topic: str) -> list[str]:
    terms = query_core_terms(topic, limit=16)
    for entity in named_entities_from_query(topic):
        cleaned = re.sub(r"\s+", " ", entity.lower()).strip()
        if cleaned and cleaned not in terms:
            terms.append(cleaned)
    return terms


def source_acceptance(record: dict[str, Any], topic: str, query: str = "") -> tuple[bool, str]:
    """Gate a screened source before it enters the usable candidate catalog.

    Query text is intentionally excluded. A hit is accepted only when the
    returned source metadata itself contains topic evidence and technical-data
    evidence. This keeps benchmark-specific source names out of the gate.
    """

    text = source_text(record)
    if not text:
        return False, "empty_source_metadata"

    prompt_terms = topic_evidence_terms(topic)
    query_terms = query_core_terms(query, limit=12)
    topic_hits = matching_terms(text, list(dict.fromkeys([*prompt_terms, *query_terms])))
    evidence_hits = matching_terms(text, SOURCE_EVIDENCE_TERMS)
    url = extract_url(record).lower()
    source_kind = str(record.get("source_kind") or "").lower()
    authority_hit = any(host in url for host in SOURCE_AUTHORITY_HOSTS)
    title = str(record.get("title") or record.get("name") or "").lower()
    strong_title_hit = bool(topic_hits and any(contains_term(title, hit) for hit in topic_hits[:6]))

    if not topic_hits:
        return False, "missing_topic_context_in_source"
    if not evidence_hits and not authority_hit and source_kind not in {"openalex", "crossref"}:
        return False, "missing_source_evidence_context"

    reason_bits = [f"topic={topic_hits[0]}"]
    if evidence_hits:
        reason_bits.append(f"evidence={evidence_hits[0]}")
    if authority_hit:
        reason_bits.append("authority_host")
    if strong_title_hit:
        reason_bits.append("title_match")
    return True, "accepted:" + ";".join(reason_bits)


def source_relevance_score(record: dict[str, Any], topic: str = "") -> int:
    """Return a deterministic 0-100 score for accepted source triage."""

    text = source_text(record)
    if not text:
        return 0
    topic_terms = topic_evidence_terms(topic) if topic else []
    source_record_terms = (
        "dataset",
        "database",
        "repository",
        "github",
        "zenodo",
        "figshare",
        "dataverse",
        "download",
        "csv",
        "xlsx",
        "api",
        "portal",
    )
    primary_source_terms = (
        "official",
        "standard",
        "manual",
        "datasheet",
        "specification",
        "technical report",
        "government",
        "agency",
        "manufacturer",
        "documentation",
    )
    evidence_detail_terms = (
        "measurement",
        "measurements",
        "measured",
        "experimental",
        "experiment",
        "benchmark",
        "table",
        "method",
        "results",
        "validation",
        "case study",
    )
    weak_context_terms = (
        "review",
        "survey",
        "overview",
        "introduction",
        "news",
        "press release",
    )
    url = extract_url(record).lower()

    score = 0
    score += min(35, 7 * len(matching_terms(text, topic_terms)))
    score += min(25, 5 * len(matching_terms(text, source_record_terms)))
    score += min(20, 4 * len(matching_terms(text, primary_source_terms)))
    score += min(15, 3 * len(matching_terms(text, evidence_detail_terms)))
    if extract_doi(record):
        score += 5
    if any(host in url for host in SOURCE_AUTHORITY_HOSTS):
        score += 10
    if matching_terms(text, weak_context_terms) and not matching_terms(text, source_record_terms + primary_source_terms + evidence_detail_terms):
        score -= 15
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
            out.append(QuerySpec("snowball_doi_references", f"{doi} references"))
            out.append(QuerySpec("snowball_doi_cited_by", f"{doi} cited by"))
        elif title:
            compact = " ".join(title.split()[:10])
            out.append(QuerySpec("snowball_title", f"{compact} references"))
        if len(out) >= limit:
            break
    return out


def build_source_graph_queries(topic: str, candidates: list[dict[str, str]], limit: int) -> list[QuerySpec]:
    """Expand from discovered sources using only source-visible entities.

    This is the generic discovery-graph behavior: find obvious sources first,
    then follow names, repositories, dataset/report titles and host-specific
    paths surfaced by those sources. It avoids topic hard-coding while still
    correcting overly abstract initial prompts.
    """

    out: list[QuerySpec] = []
    topic_terms = query_core_terms(topic, limit=5)
    topic_prefix = " ".join(topic_terms[:3])
    ranked = sorted(candidates, key=lambda row: int(row.get("relevance_score") or 0), reverse=True)
    source_hints = ("dataset", "database", "documentation", "technical report", "github", "csv", "pdf")
    for row in ranked[: max(3, limit)]:
        title = re.sub(r"\s+", " ", row.get("title", "")).strip()
        url = row.get("url", "")
        if title:
            title_core = " ".join(query_core_terms(title, limit=7)) or " ".join(title.split()[:8])
            if title_core:
                out.append(QuerySpec("source_graph_title", title_core))
                for hint in source_hints[:3]:
                    out.append(QuerySpec("source_graph_title", f"{title_core} {hint}"))
        host = ""
        try:
            host = urllib.parse.urlparse(url).netloc.lower().removeprefix("www.")
        except Exception:
            host = ""
        if host and topic_prefix:
            out.append(QuerySpec("source_graph_host", f"site:{host} {topic_prefix}"))
            for hint in source_hints[:2]:
                out.append(QuerySpec("source_graph_host", f"site:{host} {topic_prefix} {hint}"))
        if len(out) >= limit:
            break
    return dedupe_query_plan(out)[:limit]


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


def write_discovery_graph(
    out_dir: Path,
    protocol_rows: list[dict[str, Any]],
    candidates: list[dict[str, str]],
) -> None:
    accepted_keys = {source_key(row) for row in candidates}
    candidate_by_key = {source_key(row): row for row in candidates}
    nodes: dict[str, dict[str, Any]] = {}
    edges: list[dict[str, Any]] = []

    def graph_id(raw: str) -> str:
        return normalize_openalex_work_id(raw) or raw.strip()

    def node_label_for_openalex_id(openalex_id: str) -> str:
        needle = graph_id(openalex_id)
        for row in candidates:
            if graph_id(row.get("openalex_id", "")) == needle:
                return row.get("title") or needle
        return needle

    for row in protocol_rows:
        focus = str(row.get("focus") or "")
        query = str(row.get("query") or "")
        raw_payload = str(row.get("raw_payload") or "")
        query_id = "query:" + slugify(f"{focus}_{query}")[:96]
        nodes.setdefault(query_id, {"id": query_id, "kind": "query", "label": query, "focus": focus})
        raw_path_for_query = Path(raw_payload) if raw_payload else None
        if raw_path_for_query and raw_path_for_query.exists():
            try:
                payload = json.loads(raw_path_for_query.read_text(encoding="utf-8"))
            except json.JSONDecodeError:
                payload = {}
            for record in source_records(payload):
                key = source_key(record)
                if key not in accepted_keys:
                    continue
                accepted_row = candidate_by_key.get(key, {})
                child_id = graph_id(str(record.get("openalex_id") or "")) or key
                nodes.setdefault(
                    child_id,
                    {
                        "id": child_id,
                        "kind": "accepted_source",
                        "label": str(record.get("title") or record.get("name") or child_id).strip(),
                        "accepted": True,
                        "score": accepted_row.get("relevance_score") if accepted_row else None,
                        "doi": extract_doi(record),
                        "url": extract_url(record),
                    },
                )
                edges.append({"from": query_id, "to": child_id, "relation": "search_result", "accepted": True})
        if not focus.startswith("snowball_openalex_") or ":" not in query or not raw_payload:
            continue
        relation = focus.replace("snowball_openalex_", "")
        _, raw_seed_id = query.split(":", 1)
        seed_id = graph_id(raw_seed_id)
        if not seed_id:
            continue
        nodes.setdefault(
            seed_id,
            {"id": seed_id, "kind": "seed", "label": node_label_for_openalex_id(seed_id), "accepted": True},
        )
        raw_path = Path(raw_payload)
        if not raw_path.exists():
            continue
        try:
            payload = json.loads(raw_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            continue
        for record in source_records(payload):
            child_id = graph_id(str(record.get("openalex_id") or "")) or source_key(record)
            if not child_id:
                continue
            key = source_key(record)
            accepted = key in accepted_keys
            accepted_row = candidate_by_key.get(key, {})
            nodes.setdefault(
                child_id,
                {
                    "id": child_id,
                    "kind": "accepted_child" if accepted else "screened_child",
                    "label": str(record.get("title") or record.get("name") or child_id).strip(),
                    "accepted": accepted,
                    "score": accepted_row.get("relevance_score") if accepted_row else None,
                    "doi": extract_doi(record),
                    "url": extract_url(record),
                },
            )
            if accepted:
                nodes[child_id]["kind"] = "accepted_child"
                nodes[child_id]["accepted"] = True
                nodes[child_id]["score"] = accepted_row.get("relevance_score")
            edges.append({"from": seed_id, "to": child_id, "relation": relation, "accepted": accepted})

    relation_counts: dict[str, int] = {}
    accepted_relation_counts: dict[str, int] = {}
    for edge in edges:
        relation = str(edge["relation"])
        relation_counts[relation] = relation_counts.get(relation, 0) + 1
        if edge["accepted"]:
            accepted_relation_counts[relation] = accepted_relation_counts.get(relation, 0) + 1

    graph = {
        "summary": {
            "seed_papers": len([node for node in nodes.values() if node["kind"] == "seed"]),
            "nodes": len(nodes),
            "edges": len(edges),
            "accepted_child_nodes": len([node for node in nodes.values() if node["kind"] == "accepted_child"]),
            "accepted_edges": len([edge for edge in edges if edge["accepted"]]),
            "relations": relation_counts,
            "accepted_relations": accepted_relation_counts,
            "limitation": "metadata/abstract-level citation graph; full-text/PDF/table reading is a separate follow-up stage",
        },
        "nodes": list(nodes.values()),
        "edges": edges,
    }
    (out_dir / "discovery_graph.json").write_text(json.dumps(graph, indent=2) + "\n", encoding="utf-8")


def score_letter(score: int) -> str:
    if score >= 78:
        return "A"
    if score >= 58:
        return "B"
    if score >= 40:
        return "C"
    return "D"


def infer_source_type(row: dict[str, str]) -> str:
    text = " ".join(str(row.get(key) or "") for key in ("title", "url", "snippet")).lower()
    if "dataset" in text or "zenodo" in text or "figshare" in text or "dataverse" in text:
        return "dataset"
    if "github" in text or "gitlab" in text or "repository" in text:
        return "repository"
    if "standard" in text:
        return "standard"
    if "manual" in text or "documentation" in text or "docs" in text:
        return "documentation"
    if "report" in text or "pdf" in text:
        return "technical report"
    if "database" in text:
        return "database"
    return "source"


def source_group_label(row: dict[str, str]) -> str:
    focus = str(row.get("focus") or "").replace("_", " ").strip()
    if focus:
        return focus[:1].upper() + focus[1:]
    return infer_source_type(row).title()


def candidate_to_business_source(row: dict[str, str]) -> dict[str, Any]:
    title = str(row.get("title") or "Untitled source").strip()
    url = str(row.get("url") or "").strip()
    score = int(str(row.get("relevance_score") or "0") or 0)
    source_type = infer_source_type(row)
    publisher = urllib.parse.urlparse(url).netloc.removeprefix("www.") if url else "unknown"
    return {
        "id": slugify(url or title),
        "title": title,
        "group": source_group_label(row),
        "type": source_type,
        "publisher": publisher or "unknown",
        "year": "current",
        "score": score_letter(score),
        "scoreValue": score,
        "contribution": str(row.get("snippet") or row.get("acceptance_reason") or "Source candidate from discovery.").strip(),
        "access": "public" if url else "metadata",
        "url": url,
        "tags": [source_type, source_group_label(row).lower()],
        "fields": str(row.get("snippet") or "").strip(),
        "use": str(row.get("acceptance_reason") or "Candidate for review.").strip(),
        "missing": "Needs reading/extraction pass.",
        "links": [{"label": "Quelle öffnen", "url": url}] if url else [],
    }


def psql_json(database_url: str, sql_text: str) -> str:
    proc = subprocess.run(
        ["psql", database_url, "--tuples-only", "--no-align", "-v", "ON_ERROR_STOP=1", "-c", sql_text],
        check=True,
        text=True,
        capture_output=True,
    )
    return proc.stdout.strip()


def load_business_runs(database_url: str, store_key: str) -> list[dict[str, Any]]:
    ensure_sql = """
CREATE TABLE IF NOT EXISTS business_runtime_stores (
  store_key text PRIMARY KEY,
  payload_json text NOT NULL DEFAULT '{}',
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);
"""
    psql_json(database_url, ensure_sql)
    payload = psql_json(database_url, f"SELECT payload_json FROM business_runtime_stores WHERE store_key = '{store_key}' LIMIT 1;")
    if not payload:
        return []
    try:
        value = json.loads(payload)
        return value if isinstance(value, list) else []
    except json.JSONDecodeError:
        return []


def save_business_runs(database_url: str, store_key: str, runs: list[dict[str, Any]]) -> None:
    payload = json.dumps(runs, ensure_ascii=False)
    marker = "ctox_json_payload"
    while marker in payload:
        marker += "_x"
    sql_text = f"""
CREATE TABLE IF NOT EXISTS business_runtime_stores (
  store_key text PRIMARY KEY,
  payload_json text NOT NULL DEFAULT '{{}}',
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);
INSERT INTO business_runtime_stores (store_key, payload_json, updated_at)
VALUES ('{store_key}', ${marker}${payload}${marker}$, now())
ON CONFLICT (store_key)
DO UPDATE SET payload_json = EXCLUDED.payload_json, updated_at = now();
"""
    with tempfile.NamedTemporaryFile("w", suffix=".sql", delete=False, encoding="utf-8") as handle:
        handle.write(sql_text)
        temp_path = handle.name
    try:
        subprocess.run(["psql", database_url, "-v", "ON_ERROR_STOP=1", "-f", temp_path], check=True, text=True, capture_output=True)
    finally:
        try:
            os.unlink(temp_path)
        except OSError:
            pass


def upsert_business_progress(
    database_url: str,
    store_key: str,
    run_id: str,
    title: str,
    topic: str,
    progress: dict[str, Any],
) -> None:
    runs = load_business_runs(database_url, store_key)
    now_date = time.strftime("%Y-%m-%d")
    run = next((item for item in runs if item.get("id") == run_id), None)
    if not run:
        run = {
            "id": run_id,
            "title": title or topic[:80] or run_id,
            "status": "collecting",
            "updated": now_date,
            "prompt": topic,
            "criteria": "",
            "queryCount": 0,
            "screenedCount": 0,
            "acceptedCount": 0,
            "summary": {"en": topic, "de": topic},
            "sources": [],
            "graph": {"nodes": [], "edges": []},
            "expansionRequests": [],
        }
        runs.insert(0, run)
    run["status"] = "collecting" if progress.get("status") != "done" else "synthesized"
    run["updated"] = now_date
    run["researchProgress"] = progress
    save_business_runs(database_url, store_key, runs)


def sync_business_research_run(
    database_url: str,
    store_key: str,
    run_id: str,
    title: str,
    topic: str,
    protocol_rows: list[dict[str, Any]],
    candidates: list[dict[str, str]],
    screened_sources: list[dict[str, str]],
) -> None:
    runs = load_business_runs(database_url, store_key)
    now_date = time.strftime("%Y-%m-%d")
    run = next((item for item in runs if item.get("id") == run_id), None)
    if not run:
        run = {
            "id": run_id,
            "title": title or topic[:80] or run_id,
            "status": "collecting",
            "updated": now_date,
            "prompt": topic,
            "criteria": "",
            "queryCount": 0,
            "screenedCount": 0,
            "acceptedCount": 0,
            "summary": {"en": topic, "de": topic},
            "sources": [],
            "graph": {"nodes": [], "edges": []},
            "expansionRequests": [],
        }
        runs.insert(0, run)

    existing_sources = run.get("sources") if isinstance(run.get("sources"), list) else []
    source_by_key = {source_key({"url": item.get("url"), "title": item.get("title")}): item for item in existing_sources if isinstance(item, dict)}
    for row in candidates:
        item = candidate_to_business_source(row)
        key = source_key({"url": item.get("url"), "title": item.get("title")})
        source_by_key[key] = {**source_by_key.get(key, {}), **item}
    sources = sorted(source_by_key.values(), key=lambda item: int(item.get("scoreValue") or 0), reverse=True)

    graph_nodes: dict[str, dict[str, Any]] = {}
    graph_edges: dict[str, dict[str, str]] = {}
    for row in protocol_rows:
        query = str(row.get("query") or "")
        qid = "q-" + slugify(query)[:60]
        graph_nodes[qid] = {"id": qid, "label": query, "kind": "query"}
    for source in sources:
        sid = str(source.get("id") or slugify(source.get("title", "")))
        group = str(source.get("group") or "Sources")
        gid = "g-" + slugify(group)[:60]
        graph_nodes[sid] = {"id": sid, "label": source.get("title", sid), "kind": "source", "score": source.get("score")}
        graph_nodes[gid] = {"id": gid, "label": group, "kind": "group"}
        graph_edges[f"{sid}:{gid}:group"] = {"source": sid, "target": gid, "relation": "group"}
    for row in candidates:
        query = str(row.get("query") or "")
        if not query:
            continue
        qid = "q-" + slugify(query)[:60]
        sid = slugify(str(row.get("url") or row.get("title") or "source"))
        graph_edges[f"{qid}:{sid}:found"] = {"source": qid, "target": sid, "relation": "found"}

    run.update({
        "status": "synthesized",
        "updated": now_date,
        "queryCount": len(protocol_rows),
        "screenedCount": len(screened_sources),
        "acceptedCount": len(candidates),
        "sources": sources,
        "graph": {"nodes": list(graph_nodes.values()), "edges": list(graph_edges.values())},
        "researchProgress": {
            "status": "done",
            "currentStep": "Recherche abgeschlossen",
            "currentQuery": "",
            "targetAdditionalSources": 0,
            "identifiedDelta": len(screened_sources),
            "readDelta": len(candidates),
            "usedDelta": len(sources),
            "updatedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        },
    })
    save_business_runs(database_url, store_key, runs)


def existing_candidate_rows(path: Path | None, discovery_dir: Path | None) -> list[dict[str, str]]:
    rows: list[dict[str, str]] = []
    if discovery_dir:
        rows.extend(load_csv_rows(discovery_dir / "candidate_sources.csv"))
    if path:
        rows.extend(load_csv_rows(path))
    deduped: list[dict[str, str]] = []
    seen: set[str] = set()
    for row in rows:
        key = source_key(row)
        if key in seen:
            continue
        seen.add(key)
        deduped.append(row)
    return deduped


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--topic", required=True)
    parser.add_argument("--run-id")
    parser.add_argument("--out-dir", required=True)
    parser.add_argument("--queries-file")
    parser.add_argument(
        "--allow-auto-query-plan",
        action="store_true",
        help="Allow the built-in generic query-plan fallback. Intended for smoke tests only; real source reviews should pass --queries-file.",
    )
    parser.add_argument("--max-sources-per-query", type=int, default=80)
    parser.add_argument("--target-reviewed", type=int, default=1000)
    parser.add_argument("--query-timeout-sec", type=int, default=240)
    parser.add_argument("--web-query-delay-sec", type=float, default=3.0)
    parser.add_argument(
        "--discovery-backend",
        choices=["ctox-deep-research", "open-metadata", "web-search", "hybrid"],
        default="hybrid",
    )
    parser.add_argument("--snowball-rounds", type=int, default=1)
    parser.add_argument("--snowball-limit", type=int, default=12)
    parser.add_argument("--existing-candidates-csv")
    parser.add_argument("--existing-discovery-dir")
    parser.add_argument(
        "--target-additional-candidates",
        type=int,
        default=0,
        help="Continuation mode: stop after this many new accepted candidates have been added to the existing corpus.",
    )
    parser.add_argument("--business-database-url", default=os.environ.get("DATABASE_URL", ""))
    parser.add_argument("--business-store-key", default="marketing/research/runs")
    parser.add_argument("--business-research-run-id")
    parser.add_argument("--business-research-title")
    parser.add_argument("--business-writeback", action="store_true")
    parser.add_argument("--plan-only", action="store_true")
    args = parser.parse_args()

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    if not args.queries_file and not args.allow_auto_query_plan:
        parser.error(
            "--queries-file is required for source-review discovery. "
            "The harness LLM must write natural raw search queries as CSV; "
            "use --allow-auto-query-plan only for smoke tests."
        )
    existing_candidates = existing_candidate_rows(
        Path(args.existing_candidates_csv) if args.existing_candidates_csv else None,
        Path(args.existing_discovery_dir) if args.existing_discovery_dir else None,
    )
    query_plan = load_query_plan(Path(args.queries_file)) if args.queries_file else default_query_plan(args.topic)
    if existing_candidates and args.snowball_limit > 0:
        continuation_queries = build_source_graph_queries(args.topic, existing_candidates, args.snowball_limit)
        continuation_queries.extend(build_snowball_queries(args.topic, existing_candidates, args.snowball_limit))
        query_plan = dedupe_query_plan([*continuation_queries, *query_plan])
    save_query_plan(out_dir / "query_plan.csv", query_plan)
    if args.plan_only:
        print(out_dir / "query_plan.csv")
        return 0

    seen: set[str] = {source_key(row) for row in existing_candidates}
    candidates: list[dict[str, str]] = list(existing_candidates)
    initial_candidate_count = len(candidates)
    screened_sources: list[dict[str, str]] = []
    rejected_sources: list[dict[str, str]] = []
    protocol_rows: list[dict[str, Any]] = []
    research_ids: list[str] = []
    queue = list(query_plan)
    rounds_remaining = max(0, args.snowball_rounds)
    reviewed_total = 0
    business_writeback = bool(args.business_writeback and args.business_database_url and args.business_research_run_id)

    if business_writeback:
        try:
            upsert_business_progress(
                args.business_database_url,
                args.business_store_key,
                args.business_research_run_id,
                args.business_research_title or args.topic[:80],
                args.topic,
                {
                    "status": "running",
                    "currentStep": "Recherche gestartet",
                    "currentQuery": queue[0].query if queue else "",
                    "targetAdditionalSources": args.target_additional_candidates or 0,
                    "identifiedDelta": 0,
                    "readDelta": 0,
                    "usedDelta": len(existing_candidates),
                    "updatedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                },
            )
        except Exception as exc:  # noqa: BLE001 - writeback must not kill discovery.
            print(f"business_writeback_start_failed: {exc}", file=sys.stderr)

    idx = 0
    while queue:
        idx += 1
        spec = queue.pop(0)
        if idx > 1 and args.discovery_backend in {"web-search", "hybrid"}:
            time.sleep(max(0.0, args.web_query_delay_sec))
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
            args.web_query_delay_sec,
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
            accepted, reason = source_acceptance(record, args.topic, spec.query)
            relevance_score = source_relevance_score(record, args.topic)
            screened_row = {
                **row,
                "screening_status": "accepted" if accepted else "rejected",
                "screening_reason": reason,
            }
            screened_sources.append(screened_row)
            if accepted:
                candidates.append({**row, "relevance_score": str(relevance_score), "acceptance_reason": reason})
            else:
                rejected_sources.append(screened_row)

        research_id = ""
        if args.run_id:
            resolver = {
                "open-metadata": "openalex+crossref",
                "web-search": "duckduckgo-html",
                "hybrid": "hybrid:openalex+crossref+duckduckgo-html",
                "ctox-deep-research": "ctox web deep-research",
            }[args.discovery_backend]
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

        if business_writeback:
            try:
                upsert_business_progress(
                    args.business_database_url,
                    args.business_store_key,
                    args.business_research_run_id,
                    args.business_research_title or args.topic[:80],
                    args.topic,
                    {
                        "status": "running",
                        "currentStep": f"Suchrichtung {idx} von {len(query_plan)}",
                        "currentQuery": spec.query,
                        "targetAdditionalSources": args.target_additional_candidates or 0,
                        "identifiedDelta": len(screened_sources),
                        "readDelta": len(candidates) - initial_candidate_count,
                        "usedDelta": len(candidates),
                        "updatedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                    },
                )
            except Exception as exc:  # noqa: BLE001
                print(f"business_writeback_progress_failed: {exc}", file=sys.stderr)

        additional_candidates = len(candidates) - initial_candidate_count
        if args.target_additional_candidates > 0 and additional_candidates >= args.target_additional_candidates:
            break
        if reviewed_total >= args.target_reviewed:
            break

        if not queue and rounds_remaining > 0:
            rounds_remaining -= 1
            expanded = build_source_graph_queries(args.topic, candidates, args.snowball_limit)
            expanded.extend(build_snowball_queries(args.topic, candidates, args.snowball_limit))
            queue.extend(dedupe_query_plan(expanded))

    write_outputs(out_dir, protocol_rows, candidates, screened_sources, rejected_sources, query_plan, research_ids)
    write_discovery_graph(out_dir, protocol_rows, candidates)
    if business_writeback:
        try:
            sync_business_research_run(
                args.business_database_url,
                args.business_store_key,
                args.business_research_run_id,
                args.business_research_title or args.topic[:80],
                args.topic,
                protocol_rows,
                candidates,
                screened_sources,
            )
        except Exception as exc:  # noqa: BLE001
            print(f"business_writeback_final_failed: {exc}", file=sys.stderr)
    summary_obj = {
        "out_dir": str(out_dir),
        "queries_run": len(protocol_rows),
        "reviewed_results": reviewed_total,
        "unique_sources": len(candidates),
        "initial_unique_sources": initial_candidate_count,
        "additional_unique_sources": len(candidates) - initial_candidate_count,
        "screened_unique_sources": len(screened_sources),
        "rejected_sources": len(rejected_sources),
        "research_logs": len([r for r in research_ids if r]),
        "research_ids_file": str(out_dir / "research_ids.txt"),
        "search_protocol_csv": str(out_dir / "search_protocol.csv"),
        "candidate_sources_csv": str(out_dir / "candidate_sources.csv"),
        "screened_sources_csv": str(out_dir / "screened_sources.csv"),
        "rejected_sources_csv": str(out_dir / "rejected_sources.csv"),
        "discovery_graph_json": str(out_dir / "discovery_graph.json"),
    }
    (out_dir / "summary.json").write_text(json.dumps(summary_obj, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary_obj, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
