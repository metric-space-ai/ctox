#!/usr/bin/env python3
"""Fail-closed validator for a systematic-research evidence manifest.

The manifest is a build receipt, not a substitute for the source.  Discovery
records may be present, but only entries that pass this validator may feed a
claim, Knowledge version, or report version.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import re
import sys
import zipfile
from pathlib import Path
from typing import Any
from urllib.parse import urlparse


SCHEMA_VERSION = "ctox.research.evidence.v2"
MAX_DATA_EXCERPT_BYTES = 64 * 1024 * 1024
MAX_NESTED_ARCHIVE_BYTES = 512 * 1024 * 1024
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
    "wikipedia.org",
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


def normalized_sha256(value: Any, label: str) -> str:
    if not isinstance(value, str):
        raise GuardError(f"{label}_invalid")
    digest = value.strip().lower().removeprefix("sha256:")
    if not re.fullmatch(r"[0-9a-f]{64}", digest):
        raise GuardError(f"{label}_invalid")
    return digest


def canonical_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def lineage_hash(claim: dict[str, Any]) -> str:
    payload = {
        "claim_id": claim.get("claim_id"),
        "claim_text": claim.get("claim_text"),
        "evidence_quote": claim.get("evidence_quote"),
        "evidence_id": claim.get("evidence_id"),
        "snapshot_id": claim.get("snapshot_id"),
        "source_id": claim.get("source_id"),
        "canonical_url": claim.get("canonical_url"),
    }
    if claim.get("data_excerpt") is not None:
        payload["data_excerpt"] = claim.get("data_excerpt")
    return hashlib.sha256(canonical_json(payload).encode("utf-8")).hexdigest()


def normalize_evidence_text(value: str) -> str:
    value = re.sub(r"<[^>]+>", " ", value)
    return re.sub(r"\s+", " ", value).strip().casefold()


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
    expected_hash = normalized_sha256(receipt.get("sha256"), f"{label}_receipt_sha256")
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
    query = parsed.query.lower()
    if "cookies_not_supported" in query or "cookie_not_supported" in query:
        raise GuardError("canonical_url_is_cookie_interstitial")


def canonical_url_identity(url: str) -> str:
    parsed = urlparse(url)
    host = (parsed.hostname or "").lower().rstrip(".")
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


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def read_zip_member(
    archive: zipfile.ZipFile,
    member_path: str,
    max_bytes: int,
) -> bytes:
    try:
        info = archive.getinfo(member_path)
    except KeyError as exc:
        raise GuardError("claim_data_excerpt_member_unreadable") from exc
    if info.flag_bits & 0x1:
        raise GuardError("claim_data_excerpt_member_encrypted")
    if info.file_size > max_bytes:
        raise GuardError("claim_data_excerpt_member_too_large")
    try:
        with archive.open(info) as handle:
            content = handle.read(max_bytes + 1)
    except (OSError, RuntimeError, zipfile.BadZipFile) as exc:
        raise GuardError("claim_data_excerpt_member_unreadable") from exc
    if len(content) > max_bytes:
        raise GuardError("claim_data_excerpt_member_too_large")
    return content


def data_claim_text(
    claim: dict[str, Any],
    item: dict[str, Any],
    snapshot_path: Path,
) -> str:
    excerpt = require_dict(claim.get("data_excerpt"), "claim_data_excerpt")
    if normalized_sha256(
        excerpt.get("source_snapshot_sha256"), "claim_data_excerpt_source_snapshot_sha256"
    ) != normalized_sha256(item.get("snapshot_sha256"), "evidence_snapshot_sha256"):
        raise GuardError("claim_data_excerpt_snapshot_binding_mismatch")
    encoding = require_string(excerpt, "encoding", "claim_data_excerpt").lower()
    if encoding not in {"ascii", "utf-8"}:
        raise GuardError("claim_data_excerpt_encoding_unsupported")
    extraction = require_string(excerpt, "extraction", "claim_data_excerpt")
    if extraction == "snapshot_text":
        if excerpt.get("member_chain") not in (None, []):
            raise GuardError("claim_data_excerpt_member_chain_unexpected")
        if snapshot_path.stat().st_size > MAX_DATA_EXCERPT_BYTES:
            raise GuardError("claim_data_excerpt_snapshot_too_large")
        content = snapshot_path.read_bytes()
    elif extraction == "zip_member_chain":
        chain = excerpt.get("member_chain")
        if not isinstance(chain, list) or not 1 <= len(chain) <= 4:
            raise GuardError("claim_data_excerpt_member_chain_invalid")
        archive_input: Path | io.BytesIO = snapshot_path
        for index, raw_member in enumerate(chain):
            member = require_dict(raw_member, "claim_data_excerpt_member")
            member_path = require_string(
                member, "path", "claim_data_excerpt_member"
            )
            if (
                member_path.startswith(("/", "\\"))
                or ".." in Path(member_path).parts
            ):
                raise GuardError("claim_data_excerpt_member_path_unsafe")
            max_bytes = (
                MAX_NESTED_ARCHIVE_BYTES
                if index < len(chain) - 1
                else MAX_DATA_EXCERPT_BYTES
            )
            try:
                with zipfile.ZipFile(archive_input) as archive:
                    content = read_zip_member(archive, member_path, max_bytes)
            except (OSError, zipfile.BadZipFile) as exc:
                raise GuardError("claim_data_excerpt_member_unreadable") from exc
            member_hash = require_string(
                member, "sha256", "claim_data_excerpt_member"
            )
            if sha256_bytes(content) != normalized_sha256(
                member_hash, "claim_data_excerpt_member_sha256_mismatch"
            ):
                raise GuardError("claim_data_excerpt_member_sha256_mismatch")
            if index < len(chain) - 1:
                archive_input = io.BytesIO(content)
                if not zipfile.is_zipfile(archive_input):
                    raise GuardError("claim_data_excerpt_intermediate_not_zip")
                archive_input.seek(0)
    else:
        raise GuardError("claim_data_excerpt_extraction_unsupported")
    try:
        return normalize_evidence_text(content.decode(encoding, errors="strict"))
    except UnicodeDecodeError as exc:
        raise GuardError("claim_data_excerpt_decode_failed") from exc


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
    if not isinstance(sources, list) or not isinstance(evidence, list) or not isinstance(claims, list):
        raise GuardError("sources_evidence_claims_must_be_arrays")
    if not sources or not evidence:
        raise GuardError("at_least_one_verified_source_and_evidence_required")

    source_by_id: dict[str, dict[str, Any]] = {}
    source_by_url: dict[str, str] = {}
    for source in sources:
        source = require_dict(source, "source")
        source_id = require_string(source, "source_id", "source")
        if source_id in source_by_id:
            raise GuardError("source_id_not_unique")
        canonical_url = require_string(source, "canonical_url", "source")
        identity = canonical_url_identity(canonical_url)
        if identity in source_by_url:
            raise GuardError("source_canonical_url_not_unique")
        source_by_url[identity] = source_id
        source_by_id[source_id] = source

    evidence_by_id: dict[str, dict[str, Any]] = {}
    evidence_text_by_id: dict[str, str] = {}
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
        if (
            not isinstance(item.get("relevance_score"), int)
            or isinstance(item.get("relevance_score"), bool)
            or not 8 <= item["relevance_score"] <= 10
        ):
            raise GuardError("evidence_relevance_not_exact_ctox_web_read_score")
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
        if actual_hash != normalized_sha256(
            snapshot.get("sha256"), "snapshot_sha256"
        ) or actual_hash != normalized_sha256(
            item.get("snapshot_sha256"), "evidence_snapshot_sha256"
        ):
            raise GuardError("snapshot_sha256_mismatch")
        if snapshot.get("source_id") != source_id or snapshot.get("canonical_url") != url:
            raise GuardError("snapshot_source_lineage_mismatch")
        if item.get("snapshot_id") != snapshot_id:
            raise GuardError("evidence_snapshot_id_mismatch")
        retrieval = require_dict(item.get("retrieval_receipt"), "retrieval_receipt")
        if retrieval.get("tool") != "ctox_web_read":
            raise GuardError("evidence_requires_ctox_web_stack_receipt")
        request_url = require_string(retrieval, "request_url", "retrieval_receipt")
        request_parsed = urlparse(request_url)
        if request_parsed.scheme not in {"http", "https"} or not request_parsed.hostname:
            raise GuardError("retrieval_receipt_request_url_invalid")
        if retrieval.get("final_url") != url:
            raise GuardError("retrieval_receipt_url_mismatch")
        if retrieval.get("http_status") != item.get("http_status"):
            raise GuardError("retrieval_receipt_status_mismatch")
        if normalized_sha256(
            retrieval.get("body_sha256"), "retrieval_receipt_body_hash"
        ) != actual_hash:
            raise GuardError("retrieval_receipt_body_hash_mismatch")
        if retrieval.get("byte_count") != snapshot_path.stat().st_size:
            raise GuardError("retrieval_receipt_byte_count_mismatch")
        require_string(retrieval, "checked_at", "retrieval_receipt")
        checked_at_epoch = retrieval.get("checked_at_epoch")
        if not isinstance(checked_at_epoch, int) or checked_at_epoch <= 0:
            raise GuardError("retrieval_receipt_missing_checked_at_epoch")
        receipt_path = validate_artifact_receipt(
            retrieval.get("receipt_artifact"), base_dir, "retrieval"
        )
        try:
            persisted_receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            raise GuardError("retrieval_receipt_artifact_invalid_json") from exc
        persisted_receipt = require_dict(
            persisted_receipt, "retrieval_receipt_artifact"
        )
        if persisted_receipt.get("schema_version") != "ctox.web-read.workspace-evidence.v3":
            raise GuardError("retrieval_receipt_artifact_schema_mismatch")
        immutable_fields = {
            "requested_url": request_url,
            "final_url": url,
            "status": item.get("http_status"),
            "checked_at_epoch": checked_at_epoch,
            "byte_count": item.get("retrieval_receipt", {}).get("byte_count"),
            "snapshot_sha256": item.get("retrieval_receipt", {}).get("body_sha256"),
            "content_kind": item.get("retrieval_receipt", {}).get("content_kind"),
            "evidence_relevance_score": item.get("relevance_score"),
            "evidence_eligible": True,
        }
        for field, expected in immutable_fields.items():
            actual = persisted_receipt.get(field)
            if field == "snapshot_sha256":
                actual = normalized_sha256(actual, "receipt_snapshot_sha256")
                expected = normalized_sha256(expected, "manifest_snapshot_sha256")
            if actual != expected:
                raise GuardError(f"retrieval_receipt_artifact_{field}_mismatch")
        if item.get("content_kind") != "data_file":
            extracted_text = require_dict(item.get("extracted_text"), "extracted_text")
            text_path = validate_artifact_receipt(
                extracted_text, base_dir, "extracted_text"
            )
            if normalized_sha256(
                extracted_text.get("source_snapshot_sha256"),
                "extracted_text_source_snapshot_sha256",
            ) != actual_hash:
                raise GuardError("extracted_text_snapshot_binding_mismatch")
            text = text_path.read_text(encoding="utf-8", errors="strict")
            normalized_text = normalize_evidence_text(text)
            if len(normalized_text) < 100:
                raise GuardError("extracted_full_text_missing")
            evidence_text_by_id[evidence_id] = normalized_text

    evidenced_source_ids = {
        require_string(item, "source_id", "evidence") for item in evidence_by_id.values()
    }
    if evidenced_source_ids != set(source_by_id):
        raise GuardError("manifest_source_without_verified_evidence")

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
        if normalized_sha256(
            check.get("sha256"), "deterministic_data_hash"
        ) != normalized_sha256(item.get("snapshot_sha256"), "evidence_snapshot_sha256"):
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
        if not data_path.is_file() or sha256_file(data_path) != normalized_sha256(
            item.get("snapshot_sha256"), "evidence_snapshot_sha256"
        ):
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
        quote = require_string(claim, "evidence_quote", "claim")
        normalized_quote = normalize_evidence_text(quote)
        if len(normalized_quote) < 40 or len(normalized_quote.split()) < 6:
            raise GuardError("claim_evidence_quote_too_short")
        if item.get("content_kind") == "data_file":
            evidence_text = data_claim_text(
                claim,
                item,
                resolve_path(base_dir, item["snapshot"]["path"]),
            )
            if normalized_quote not in evidence_text:
                raise GuardError("claim_evidence_quote_not_in_data_excerpt")
        elif normalized_quote not in evidence_text_by_id.get(evidence_id, ""):
            raise GuardError("claim_evidence_quote_not_in_extracted_text")
        if claim.get("lineage_sha256") != lineage_hash(claim):
            raise GuardError("claim_lineage_hash_mismatch")

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
