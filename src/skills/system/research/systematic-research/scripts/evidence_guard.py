#!/usr/bin/env python3
"""Fail-closed validator for a systematic-research evidence manifest.

The manifest is a build receipt, not a substitute for the source.  Discovery
records may be present, but only entries that pass this validator may feed a
claim, Knowledge version, or report version.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any
from urllib.parse import urlparse


SCHEMA_VERSION = "ctox.research.evidence.v1"
FORBIDDEN_HOSTS = (
    "doi.org",
    "dx.doi.org",
    "scholar.google.",
    "researchgate.",
    "academia.edu",
    "semanticscholar.org",
    "openalex.org",
    "crossref.org",
    "api.openalex.org",
    "api.crossref.org",
    "api.semanticscholar.org",
    "api.datacite.org",
)
FORBIDDEN_ROLES = {"aggregator", "doi_landing", "landing", "metadata", "snippet"}
FORBIDDEN_SCOPES = {"abstract", "cookie_wall", "login", "metadata", "shell", "snippet"}
BLOCKED_CONTENT = re.compile(
    r"(?:accept\s+(?:all\s+)?cookies?|cookie\s+preferences?|enable\s+javascript|"
    r"javascript\s+is\s+required|search\s+result\s+snippet|"
    r"^\s*(?:title|authors?|doi|abstract)\s*:\s*)",
    re.IGNORECASE | re.MULTILINE,
)
LOGIN_INTERSTITIAL = re.compile(
    r"^\s*(?:please\s+)?(?:sign\s+in|log\s*in|create\s+an\s+account)\b",
    re.IGNORECASE | re.MULTILINE,
)


class GuardError(ValueError):
    """A manifest or evidence invariant failed."""


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def lineage_hash(claim: dict[str, Any]) -> str:
    payload = {
        "claim_id": claim.get("claim_id"),
        "claim_text": claim.get("claim_text"),
        "evidence_id": claim.get("evidence_id"),
        "snapshot_id": claim.get("snapshot_id"),
        "source_id": claim.get("source_id"),
        "canonical_url": claim.get("canonical_url"),
    }
    return hashlib.sha256(canonical_json(payload).encode("utf-8")).hexdigest()


def resolve_path(base_dir: Path, raw: Any) -> Path:
    if not isinstance(raw, str) or not raw.strip():
        raise GuardError("missing_path")
    path = Path(raw)
    if path.is_absolute():
        raise GuardError("absolute_paths_are_forbidden")
    workspace = base_dir.resolve()
    resolved = (workspace / path).resolve()
    try:
        resolved.relative_to(workspace)
    except ValueError as exc:
        raise GuardError("path_escapes_workspace") from exc
    return resolved


def require_dict(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise GuardError(f"{label}_must_be_object")
    return value


def require_string(obj: dict[str, Any], key: str, label: str) -> str:
    value = obj.get(key)
    if not isinstance(value, str) or not value.strip():
        raise GuardError(f"{label}_missing_{key}")
    return value.strip()


def validate_artifact_receipt(value: Any, base_dir: Path, label: str) -> Path:
    receipt = require_dict(value, f"{label}_receipt")
    path = resolve_path(base_dir, receipt.get("path"))
    expected_hash = require_string(receipt, "sha256", f"{label}_receipt")
    if not path.is_file() or path.stat().st_size == 0:
        raise GuardError(f"{label}_receipt_missing")
    if sha256_file(path) != expected_hash:
        raise GuardError(f"{label}_receipt_sha256_mismatch")
    return path


def validate_url(url: Any, role: Any) -> None:
    if not isinstance(url, str):
        raise GuardError("canonical_url_missing")
    parsed = urlparse(url)
    host = (parsed.hostname or "").lower().rstrip(".")
    if parsed.scheme not in {"http", "https"} or not host or not parsed.path:
        raise GuardError("canonical_url_not_http_original")
    if role == "doi_landing" or host in {"doi.org", "dx.doi.org"}:
        raise GuardError("doi_landing_not_evidence")
    if role in FORBIDDEN_ROLES or any(host == item or host.endswith("." + item) for item in FORBIDDEN_HOSTS):
        raise GuardError("canonical_url_is_metadata_or_aggregator")


def validate_content(path: Path, scope: str) -> None:
    if not path.is_file() or path.stat().st_size == 0:
        raise GuardError("snapshot_content_missing")
    if scope in FORBIDDEN_SCOPES:
        raise GuardError("metadata_or_interstitial_not_evidence")
    sample = path.read_bytes()[:2_000_000]
    text = sample.decode("utf-8", errors="ignore")
    short_interstitial = len(text.strip()) < 1_500 and LOGIN_INTERSTITIAL.search(text) is not None
    if BLOCKED_CONTENT.search(text) or short_interstitial:
        raise GuardError("login_cookie_shell_or_snippet_not_evidence")


def validate_manifest(manifest: dict[str, Any], base_dir: Path) -> None:
    if manifest.get("schema_version") != SCHEMA_VERSION:
        raise GuardError("unsupported_schema_version")
    require_string(manifest, "run_id", "manifest")
    require_string(manifest, "research_run_id", "manifest")
    require_string(manifest, "research_command_id", "manifest")
    require_string(manifest, "research_attempt_id", "manifest")
    require_string(manifest, "as_of", "manifest")
    sources = manifest.get("sources")
    evidence = manifest.get("evidence")
    claims = manifest.get("claims")
    reviews = manifest.get("reviews")
    if not isinstance(sources, list) or not isinstance(evidence, list) or not isinstance(claims, list):
        raise GuardError("sources_evidence_claims_must_be_arrays")
    if not isinstance(reviews, list):
        raise GuardError("independent_reviews_required")
    if not sources or not evidence:
        raise GuardError("at_least_one_verified_source_and_evidence_required")

    source_by_id: dict[str, dict[str, Any]] = {}
    for source in sources:
        source = require_dict(source, "source")
        source_id = require_string(source, "source_id", "source")
        if source_id in source_by_id:
            raise GuardError("source_id_not_unique")
        source_by_id[source_id] = source

    evidence_by_id: dict[str, dict[str, Any]] = {}
    for item in evidence:
        item = require_dict(item, "evidence")
        evidence_id = require_string(item, "evidence_id", "evidence")
        if evidence_id in evidence_by_id:
            raise GuardError("evidence_id_not_unique")
        evidence_by_id[evidence_id] = item
        source_id = require_string(item, "source_id", "evidence")
        source = source_by_id.get(source_id)
        if source is None:
            raise GuardError("evidence_source_missing")
        url = require_string(item, "canonical_url", "evidence")
        if url != source.get("canonical_url"):
            raise GuardError("evidence_source_url_mismatch")
        validate_url(url, item.get("url_role"))
        if not isinstance(item.get("http_status"), int) or not 200 <= item["http_status"] < 204:
            raise GuardError("evidence_requires_current_content_2xx")
        if item.get("freshness_status") != "current":
            raise GuardError("evidence_not_current")
        if not isinstance(item.get("relevance_score"), (int, float)) or item["relevance_score"] < 8:
            raise GuardError("evidence_relevance_below_8")
        if item.get("evidence_status") != "eligible":
            raise GuardError("discovery_candidate_not_evidence")
        scope = require_string(item, "content_scope", "evidence").lower()
        if scope != "full_text" and item.get("content_kind") != "data_file":
            raise GuardError("actual_full_text_required")
        snapshot = require_dict(item.get("snapshot"), "snapshot")
        snapshot_id = require_string(snapshot, "snapshot_id", "snapshot")
        snapshot_path = resolve_path(base_dir, snapshot.get("path"))
        validate_content(snapshot_path, scope)
        actual_hash = sha256_file(snapshot_path)
        if actual_hash != snapshot.get("sha256") or actual_hash != item.get("snapshot_sha256"):
            raise GuardError("snapshot_sha256_mismatch")
        if snapshot.get("source_id") != source_id or snapshot.get("canonical_url") != url:
            raise GuardError("snapshot_source_lineage_mismatch")
        if item.get("snapshot_id") != snapshot_id:
            raise GuardError("evidence_snapshot_id_mismatch")
        retrieval = require_dict(item.get("retrieval_receipt"), "retrieval_receipt")
        if retrieval.get("tool") not in {"ctox_web_read", "ctox_deep_research"}:
            raise GuardError("evidence_requires_ctox_web_stack_receipt")
        request_url = require_string(retrieval, "request_url", "retrieval_receipt")
        request_parsed = urlparse(request_url)
        if request_parsed.scheme not in {"http", "https"} or not request_parsed.hostname:
            raise GuardError("retrieval_receipt_request_url_invalid")
        if retrieval.get("final_url") != url:
            raise GuardError("retrieval_receipt_url_mismatch")
        if retrieval.get("http_status") != item.get("http_status"):
            raise GuardError("retrieval_receipt_status_mismatch")
        if retrieval.get("body_sha256") != actual_hash:
            raise GuardError("retrieval_receipt_body_hash_mismatch")
        if retrieval.get("byte_count") != snapshot_path.stat().st_size:
            raise GuardError("retrieval_receipt_byte_count_mismatch")
        require_string(retrieval, "checked_at", "retrieval_receipt")
        validate_artifact_receipt(
            retrieval.get("receipt_artifact"), base_dir, "retrieval"
        )

    data_files = manifest.get("data_files", [])
    if not isinstance(data_files, list):
        raise GuardError("data_files_must_be_array")
    if not claims and not data_files:
        raise GuardError("at_least_one_claim_or_verified_data_file_required")
    data_by_id: dict[str, dict[str, Any]] = {}
    for data in data_files:
        data = require_dict(data, "data_file")
        data_id = require_string(data, "data_file_id", "data_file")
        if data_id in data_by_id:
            raise GuardError("data_file_id_not_unique")
        data_by_id[data_id] = data
        evidence_id = require_string(data, "evidence_id", "data_file")
        item = evidence_by_id.get(evidence_id)
        if item is None or item.get("content_kind") != "data_file":
            raise GuardError("data_file_evidence_missing")
        if data.get("downloaded") is not True or data.get("original_data") is not True or data.get("generated") is True:
            raise GuardError("original_data_download_required")
        if data.get("quarantine_status") != "accepted":
            raise GuardError("unverified_data_must_be_quarantined")
        check = require_dict(data.get("deterministic_check"), "deterministic_check")
        if check.get("status") != "pass" or not check.get("parser") or not check.get("parser_version"):
            raise GuardError("deterministic_data_check_required")
        if check.get("sha256") != item.get("snapshot_sha256"):
            raise GuardError("deterministic_data_hash_missing")
        if not isinstance(check.get("columns"), list) or not check["columns"]:
            raise GuardError("deterministic_data_schema_missing")
        if not isinstance(check.get("row_count"), int) or check["row_count"] < 0:
            raise GuardError("deterministic_data_row_count_missing")
        if not isinstance(check.get("units"), (dict, list)):
            raise GuardError("deterministic_data_units_missing")
        if not isinstance(check.get("null_handling"), str) or not check["null_handling"].strip():
            raise GuardError("deterministic_data_null_handling_missing")
        if check.get("tabular", True) and (
            not isinstance(check.get("encoding"), str) or not check["encoding"].strip()
            or not isinstance(check.get("delimiter"), str) or not check["delimiter"].strip()
        ):
            raise GuardError("deterministic_data_encoding_delimiter_missing")
        data_path = resolve_path(base_dir, data.get("path"))
        if not data_path.is_file() or sha256_file(data_path) != item.get("snapshot_sha256"):
            raise GuardError("downloaded_data_snapshot_mismatch")

    for claim in claims:
        claim = require_dict(claim, "claim")
        require_string(claim, "claim_id", "claim")
        require_string(claim, "claim_text", "claim")
        evidence_id = require_string(claim, "evidence_id", "claim")
        item = evidence_by_id.get(evidence_id)
        if item is None:
            raise GuardError("claim_evidence_missing")
        if claim.get("snapshot_id") != item.get("snapshot_id") or claim.get("source_id") != item.get("source_id"):
            raise GuardError("claim_lineage_broken")
        if claim.get("canonical_url") != item.get("canonical_url"):
            raise GuardError("claim_source_url_mismatch")
        if claim.get("lineage_sha256") != lineage_hash(claim):
            raise GuardError("claim_lineage_hash_mismatch")

    required_reviews = {"source", "data", "claim"}
    seen_review_types: set[str] = set()
    reviewer_ids: set[str] = set()
    for review in reviews:
        review = require_dict(review, "review")
        review_type = require_string(review, "review_type", "review")
        reviewer_id = require_string(review, "reviewer_id", "review")
        if review_type in seen_review_types or reviewer_id in reviewer_ids:
            raise GuardError("reviews_must_be_independent")
        if review.get("status") != "pass" or not isinstance(review.get("reviewed_ids"), list):
            raise GuardError("source_data_claim_reviews_must_pass")
        if review_type == "source":
            target_ids = set(evidence_by_id)
        elif review_type == "data":
            target_ids = set(data_by_id) or set(evidence_by_id)
        else:
            target_ids = {str(claim.get("claim_id")) for claim in claims} or set(evidence_by_id)
        if set(review["reviewed_ids"]) != target_ids:
            raise GuardError("review_does_not_cover_full_target_set")
        review_path = validate_artifact_receipt(
            review.get("receipt_artifact"), base_dir, f"{review_type}_review"
        )
        try:
            review_receipt = json.loads(review_path.read_text(encoding="utf-8"))
        except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise GuardError(f"{review_type}_review_receipt_invalid_json") from exc
        review_receipt = require_dict(review_receipt, f"{review_type}_review_receipt")
        if (
            review_receipt.get("schema_version") != "ctox.research.review.v1"
            or review_receipt.get("review_type") != review_type
            or review_receipt.get("reviewer_id") != reviewer_id
            or review_receipt.get("status") != "pass"
            or set(review_receipt.get("reviewed_ids", [])) != target_ids
            or review_receipt.get("research_run_id") != manifest.get("research_run_id")
            or review_receipt.get("research_command_id") != manifest.get("research_command_id")
            or review_receipt.get("research_attempt_id") != manifest.get("research_attempt_id")
        ):
            raise GuardError(f"{review_type}_review_receipt_contract_mismatch")
        seen_review_types.add(review_type)
        reviewer_ids.add(reviewer_id)
    if seen_review_types != required_reviews:
        raise GuardError("source_data_claim_reviews_required")

    knowledge = require_dict(manifest.get("knowledge"), "knowledge")
    if knowledge.get("living") is True:
        versions = knowledge.get("versions")
        current = require_string(knowledge, "current_version_id", "knowledge")
        if not isinstance(versions, list):
            raise GuardError("knowledge_version_missing")
        current_versions = [
            v for v in versions
            if isinstance(v, dict)
            and v.get("version_id") == current
            and v.get("status") == "current"
        ]
        if len(current_versions) != 1:
            raise GuardError("knowledge_version_missing")
        current_version = current_versions[0]
        knowledge_path = validate_artifact_receipt(
            current_version.get("artifact"), base_dir, "knowledge_version"
        )
        claim_ids = {str(claim.get("claim_id")) for claim in claims}
        if set(current_version.get("claim_ids", [])) != claim_ids:
            raise GuardError("knowledge_version_claim_lineage_incomplete")
        knowledge_text = knowledge_path.read_text(encoding="utf-8", errors="ignore")
        if any(claim_id not in knowledge_text for claim_id in claim_ids):
            raise GuardError("knowledge_artifact_missing_claim_lineage")
        if knowledge.get("mutable_in_place") is True or not isinstance(knowledge.get("invalidations"), list):
            raise GuardError("knowledge_update_invalidation_required")

    reports = manifest.get("reports")
    if reports is not None:
        reports = require_dict(reports, "reports")
        if reports.get("living") is True:
            report_versions = reports.get("versions")
            if not isinstance(report_versions, list) or not report_versions:
                raise GuardError("report_version_missing")
            claim_ids = {str(claim.get("claim_id")) for claim in claims}
            for index, report_version in enumerate(report_versions):
                report_version = require_dict(report_version, f"report_version_{index}")
                report_path = validate_artifact_receipt(
                    report_version.get("artifact"), base_dir, f"report_version_{index}"
                )
                referenced_claims = set(report_version.get("claim_ids", []))
                if not referenced_claims or not referenced_claims.issubset(claim_ids):
                    raise GuardError("report_version_claim_lineage_invalid")
                report_text = report_path.read_text(encoding="utf-8", errors="ignore")
                if any(claim_id not in report_text for claim_id in referenced_claims):
                    raise GuardError("report_artifact_missing_claim_lineage")
            if reports.get("mutable_in_place") is True or not isinstance(reports.get("invalidations"), list):
                raise GuardError("report_update_invalidation_required")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--base-dir", type=Path)
    args = parser.parse_args(argv)
    try:
        manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
        if not isinstance(manifest, dict):
            raise GuardError("manifest_must_be_object")
        validate_manifest(manifest, args.base_dir or args.manifest.parent)
    except (OSError, json.JSONDecodeError, GuardError) as exc:
        print(json.dumps({"ok": False, "error": str(exc)}))
        return 1
    print(json.dumps({"ok": True, "schema_version": SCHEMA_VERSION}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
