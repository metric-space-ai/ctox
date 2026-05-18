#!/usr/bin/env python3
"""
doi_resolve.py — manual DOI resolver for deep-research diagnostics.

Used as a backup for the Rust resolver and to verify that outbound HTTPS
to Crossref / OpenAlex works from the host the operator is on.

Looks up each DOI in Crossref first; on 404 or non-200, falls back to
OpenAlex. Emits one normalized JSON object per DOI on stdout (a JSON
array overall).

CLI:

    python3 doi_resolve.py 10.3390/coatings9110727 10.1080/17686733.2024.2448049
    # or read from stdin if no DOIs are passed:
    echo "10.3390/coatings9110727" | python3 doi_resolve.py

Stdlib-only. No requests dependency. Per-request timeout 10 seconds.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from typing import Any, Dict, List, Optional

USER_AGENT = "ctox-deep-research/dev (mailto:contact@metric-space-ai.github.io)"
TIMEOUT_SECONDS = 10
CROSSREF_ENDPOINT = "https://api.crossref.org/works/"
OPENALEX_ENDPOINT = "https://api.openalex.org/works/doi:"

DOI_RE = re.compile(r"^10\.\d{4,9}/[-._;()/:A-Za-z0-9]+$", re.IGNORECASE)


def _normalize_doi(raw: str) -> str:
    """Strip URL prefix, leading 'doi:' marker, surrounding whitespace.

    Returns the normalized DOI or empty string if it does not look like
    one.
    """
    s = (raw or "").strip()
    if not s:
        return ""
    s = re.sub(r"^https?://(?:dx\.)?doi\.org/", "", s, flags=re.IGNORECASE)
    s = re.sub(r"^doi:\s*", "", s, flags=re.IGNORECASE)
    s = s.strip()
    if not DOI_RE.match(s):
        return ""
    return s


def _http_get_json(url: str) -> Optional[Dict[str, Any]]:
    """GET the URL with our User-Agent, parse JSON. None on any failure."""
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT, "Accept": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT_SECONDS) as resp:
            status = getattr(resp, "status", 200)
            if status != 200:
                return None
            data = resp.read()
    except urllib.error.HTTPError:
        return None
    except urllib.error.URLError:
        return None
    except (TimeoutError, ConnectionError):
        return None
    except Exception:  # pragma: no cover — be defensive in CLI tools
        return None
    try:
        return json.loads(data.decode("utf-8", errors="replace"))
    except (ValueError, UnicodeDecodeError):
        return None


def _http_status_get(url: str) -> int:
    """Issue the GET, return the HTTP status code (or 0 on transport
    error). Used so callers can distinguish 404 (fall back) from
    transport failure (mark error).
    """
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT, "Accept": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT_SECONDS) as resp:
            return getattr(resp, "status", 200)
    except urllib.error.HTTPError as exc:
        return exc.code
    except urllib.error.URLError:
        return 0
    except (TimeoutError, ConnectionError):
        return 0
    except Exception:  # pragma: no cover
        return 0


def _crossref_resolve(doi: str) -> Optional[Dict[str, Any]]:
    url = CROSSREF_ENDPOINT + urllib.parse.quote(doi, safe="/-._;()/:")
    payload = _http_get_json(url)
    if not payload:
        return None
    msg = payload.get("message")
    if not isinstance(msg, dict):
        return None
    title_list = msg.get("title") or []
    title = title_list[0] if isinstance(title_list, list) and title_list else ""
    authors_raw = msg.get("author") or []
    authors: List[str] = []
    for a in authors_raw:
        if not isinstance(a, dict):
            continue
        family = a.get("family") or ""
        given = a.get("given") or ""
        if family and given:
            authors.append(f"{family}, {given}")
        elif family:
            authors.append(family)
        elif given:
            authors.append(given)
    venue_list = msg.get("container-title") or []
    venue = venue_list[0] if isinstance(venue_list, list) and venue_list else ""
    issued = msg.get("issued") or {}
    parts = issued.get("date-parts") or []
    year: Optional[int] = None
    if isinstance(parts, list) and parts and isinstance(parts[0], list) and parts[0]:
        try:
            year = int(parts[0][0])
        except (TypeError, ValueError):
            year = None
    publisher = msg.get("publisher") or ""
    abstract = msg.get("abstract") or ""
    if abstract and isinstance(abstract, str):
        abstract = re.sub(r"<[^>]+>", "", abstract).strip()
    license_field = ""
    licenses = msg.get("license") or []
    if isinstance(licenses, list) and licenses:
        first = licenses[0]
        if isinstance(first, dict):
            license_field = first.get("URL") or ""
    url_canonical = "https://doi.org/" + doi
    url_full_text = ""
    primary_url = msg.get("URL")
    if isinstance(primary_url, str):
        url_full_text = primary_url
    return {
        "title": title,
        "authors": authors,
        "venue": venue,
        "year": year,
        "publisher": publisher,
        "url_canonical": url_canonical,
        "url_full_text": url_full_text,
        "license": license_field,
        "abstract": abstract,
    }


def _openalex_resolve(doi: str) -> Optional[Dict[str, Any]]:
    url = OPENALEX_ENDPOINT + urllib.parse.quote(doi, safe="/-._;()/:")
    payload = _http_get_json(url)
    if not payload:
        return None
    title = payload.get("title") or payload.get("display_name") or ""
    authors: List[str] = []
    for entry in payload.get("authorships", []) or []:
        author = entry.get("author") if isinstance(entry, dict) else None
        if isinstance(author, dict):
            name = author.get("display_name")
            if isinstance(name, str) and name:
                authors.append(name)
    host = payload.get("primary_location") or {}
    source = host.get("source") if isinstance(host, dict) else None
    venue = ""
    publisher = ""
    if isinstance(source, dict):
        venue = source.get("display_name") or ""
        publisher = source.get("publisher") or ""
    year = payload.get("publication_year")
    try:
        year_int: Optional[int] = int(year) if year is not None else None
    except (TypeError, ValueError):
        year_int = None
    abstract = ""
    inv = payload.get("abstract_inverted_index")
    if isinstance(inv, dict) and inv:
        # Reconstruct rough abstract text from OpenAlex's inverted index.
        positions: List = []
        for word, idx_list in inv.items():
            if not isinstance(idx_list, list):
                continue
            for idx in idx_list:
                if isinstance(idx, int):
                    positions.append((idx, word))
        positions.sort()
        abstract = " ".join(word for _, word in positions)
    url_canonical = "https://doi.org/" + doi
    url_full_text = ""
    if isinstance(host, dict):
        loc = host.get("landing_page_url")
        if isinstance(loc, str):
            url_full_text = loc
    license_field = ""
    if isinstance(host, dict):
        lic = host.get("license")
        if isinstance(lic, str):
            license_field = lic
    return {
        "title": title,
        "authors": authors,
        "venue": venue,
        "year": year_int,
        "publisher": publisher,
        "url_canonical": url_canonical,
        "url_full_text": url_full_text,
        "license": license_field,
        "abstract": abstract,
    }


def resolve(doi_input: str) -> Dict[str, Any]:
    norm = _normalize_doi(doi_input)
    if not norm:
        return {
            "doi": doi_input,
            "kind": "doi",
            "canonical_id": doi_input,
            "title": "",
            "authors": [],
            "venue": "",
            "year": None,
            "publisher": "",
            "url_canonical": "",
            "url_full_text": "",
            "license": "",
            "abstract": "",
            "resolver_used": "none",
            "ok": False,
            "error": "invalid_doi",
        }

    base = {
        "doi": norm,
        "kind": "doi",
        "canonical_id": norm,
        "title": "",
        "authors": [],
        "venue": "",
        "year": None,
        "publisher": "",
        "url_canonical": "https://doi.org/" + norm,
        "url_full_text": "",
        "license": "",
        "abstract": "",
        "resolver_used": "none",
        "ok": False,
        "error": None,
    }

    record = _crossref_resolve(norm)
    if record:
        base.update(record)
        base["resolver_used"] = "crossref"
        base["ok"] = True
        return base

    # Inspect Crossref status to give a more specific error before fallback.
    cr_status = _http_status_get(
        CROSSREF_ENDPOINT + urllib.parse.quote(norm, safe="/-._;()/:")
    )

    record = _openalex_resolve(norm)
    if record:
        base.update(record)
        base["resolver_used"] = "openalex"
        base["ok"] = True
        return base

    if cr_status == 0:
        base["error"] = "crossref_unreachable"
    elif cr_status >= 500:
        base["error"] = f"crossref_{cr_status}"
    elif cr_status == 404:
        base["error"] = "not_found"
    else:
        base["error"] = f"crossref_{cr_status}"
    return base


def _read_dois_from_stdin() -> List[str]:
    if sys.stdin.isatty():
        return []
    text = sys.stdin.read()
    return [line.strip() for line in text.splitlines() if line.strip()]


def main(argv: List[str]) -> int:
    parser = argparse.ArgumentParser(
        prog="doi_resolve.py",
        description="Resolve DOIs via Crossref (Crossref-first, OpenAlex fallback).",
    )
    parser.add_argument(
        "dois",
        nargs="*",
        help="One or more DOIs. If omitted, DOIs are read one-per-line from stdin.",
    )
    args = parser.parse_args(argv)

    dois = args.dois or _read_dois_from_stdin()
    if not dois:
        sys.stderr.write(
            "doi_resolve.py: no DOIs supplied. Pass them as arguments or pipe via stdin.\n"
        )
        return 2

    results = [resolve(d) for d in dois]
    sys.stdout.write(json.dumps(results, ensure_ascii=False, indent=2))
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
