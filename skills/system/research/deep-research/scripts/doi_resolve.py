#!/usr/bin/env python3
"""Resolve a list of DOIs against Crossref and emit a JSON array suitable for
`ctox report evidence add --from-file`.

Usage:
    python3 doi_resolve.py 10.1016/j.compositesb.2024.111982 10.3390/s16060843 ...
    cat dois.txt | python3 doi_resolve.py -
"""

from __future__ import annotations

import argparse
import json
import sys
import urllib.request

CROSSREF = "https://api.crossref.org/works/"
USER_AGENT = "ctox-report/1 (mailto:contact@metric-space-ai.github.io)"
TIMEOUT = 10.0


def fetch(doi: str) -> dict | None:
    req = urllib.request.Request(
        CROSSREF + doi.strip(),
        headers={"User-Agent": USER_AGENT, "Accept": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
            payload = json.loads(resp.read().decode("utf-8"))
            return payload.get("message")
    except Exception as exc:
        sys.stderr.write(f"crossref fetch failed for {doi}: {exc}\n")
        return None


def to_evidence_input(doi: str, message: dict | None) -> dict:
    if not message:
        return {
            "citation_kind": "doi",
            "canonical_id": doi,
            "title": None,
            "authors": [],
            "venue": None,
            "year": None,
            "publisher": None,
            "landing_url": f"https://doi.org/{doi}",
            "full_text_url": None,
            "abstract_md": None,
            "snippet_md": None,
            "license": None,
            "resolver": "manual_unresolved",
        }
    title = (message.get("title") or [None])[0]
    venue = (message.get("container-title") or [None])[0]
    publisher = message.get("publisher")
    year = None
    issued = message.get("issued") or {}
    parts = issued.get("date-parts") or []
    if parts and parts[0]:
        year = parts[0][0]
    authors = []
    for a in message.get("author", []) or []:
        family = a.get("family")
        given = a.get("given")
        if family and given:
            authors.append(f"{family}, {given}")
        elif family:
            authors.append(family)
        elif given:
            authors.append(given)
    license_url = None
    licenses = message.get("license") or []
    if licenses:
        license_url = licenses[0].get("URL")
    return {
        "citation_kind": "doi",
        "canonical_id": doi,
        "title": title,
        "authors": authors,
        "venue": venue,
        "year": year,
        "publisher": publisher,
        "landing_url": message.get("URL") or f"https://doi.org/{doi}",
        "full_text_url": None,
        "abstract_md": message.get("abstract"),
        "snippet_md": message.get("abstract"),
        "license": license_url,
        "resolver": "crossref",
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "dois",
        nargs="*",
        help="DOIs to resolve. Use '-' to read newline-separated from stdin.",
    )
    args = parser.parse_args()
    inputs = []
    for token in args.dois:
        if token == "-":
            inputs.extend([line.strip() for line in sys.stdin if line.strip()])
        else:
            inputs.append(token.strip())
    out = []
    for doi in inputs:
        if not doi:
            continue
        msg = fetch(doi)
        out.append(to_evidence_input(doi, msg))
    json.dump(out, sys.stdout, indent=2, ensure_ascii=False)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
