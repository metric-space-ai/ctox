#!/usr/bin/env python3
"""Regression tests for the fail-closed evidence contract."""

from __future__ import annotations

import hashlib
import json
import copy
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from evidence_guard import SCHEMA_VERSION, lineage_hash, validate_manifest  # noqa: E402


class EvidenceGuardTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.base = Path(self.tmp.name)
        self.content = self.base / "original.txt"
        self.content.write_text(
            "Full original source text with methods, results, tables, units, and conclusions. " * 20,
            encoding="utf-8",
        )
        digest = hashlib.sha256(self.content.read_bytes()).hexdigest()
        self.manifest = {
            "schema_version": SCHEMA_VERSION,
            "run_id": "run-1",
            "as_of": "2026-07-17",
            "sources": [{"source_id": "src-1", "canonical_url": "https://example.edu/paper/full-text"}],
            "evidence": [{
                "evidence_id": "ev-1", "source_id": "src-1",
                "canonical_url": "https://example.edu/paper/full-text",
                "url_role": "original_content", "http_status": 200,
                "freshness_status": "current", "relevance_score": 9,
                "evidence_status": "eligible", "content_scope": "full_text",
                "content_kind": "full_text", "snapshot_id": "snap-1",
                "snapshot_sha256": digest,
                "snapshot": {"snapshot_id": "snap-1", "path": "original.txt", "sha256": digest,
                              "source_id": "src-1", "canonical_url": "https://example.edu/paper/full-text"},
            }],
            "claims": [{
                "claim_id": "c-1",
                "claim_text": "The source contains methods, results, tables, units, and conclusions.",
                "evidence_id": "ev-1",
                "snapshot_id": "snap-1",
                "source_id": "src-1",
                "canonical_url": "https://example.edu/paper/full-text",
            }],
            "data_files": [],
            "reviews": [
                {"review_type": "source", "reviewer_id": "r-source", "status": "pass", "reviewed_ids": ["ev-1"]},
                {"review_type": "data", "reviewer_id": "r-data", "status": "pass", "reviewed_ids": ["ev-1"]},
                {"review_type": "claim", "reviewer_id": "r-claim", "status": "pass", "reviewed_ids": ["c-1"]},
            ],
            "knowledge": {"living": False},
        }
        self.manifest["claims"][0]["lineage_sha256"] = lineage_hash(self.manifest["claims"][0])

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def test_valid_manifest_passes(self) -> None:
        validate_manifest(self.manifest, self.base)

    def test_publisher_doi_fulltext_path_is_allowed(self) -> None:
        url = "https://onlinelibrary.wiley.com/doi/full/10.1002/example"
        self.manifest["sources"][0]["canonical_url"] = url
        self.manifest["evidence"][0]["canonical_url"] = url
        self.manifest["evidence"][0]["snapshot"]["canonical_url"] = url
        self.manifest["claims"][0]["canonical_url"] = url
        self.manifest["claims"][0]["lineage_sha256"] = lineage_hash(self.manifest["claims"][0])
        validate_manifest(self.manifest, self.base)

    def test_discovery_candidate_cannot_be_evidence(self) -> None:
        self.manifest["evidence"][0]["evidence_status"] = "candidate"
        with self.assertRaisesRegex(ValueError, "discovery_candidate"):
            validate_manifest(self.manifest, self.base)

    def test_empty_research_cannot_pass_with_empty_reviews(self) -> None:
        self.manifest["sources"] = []
        self.manifest["evidence"] = []
        self.manifest["claims"] = []
        self.manifest["reviews"] = [
            {"review_type": "source", "reviewer_id": "r-source", "status": "pass", "reviewed_ids": []},
            {"review_type": "data", "reviewer_id": "r-data", "status": "pass", "reviewed_ids": []},
            {"review_type": "claim", "reviewer_id": "r-claim", "status": "pass", "reviewed_ids": []},
        ]
        with self.assertRaisesRegex(ValueError, "at_least_one_verified_source"):
            validate_manifest(self.manifest, self.base)

    def test_rejected_url_classes_fail_closed(self) -> None:
        for url, role, needle in (
            ("https://doi.org/10.1/example", "doi_landing", "doi_landing"),
            ("https://api.openalex.org/works/W1", "metadata", "metadata_or_aggregator"),
            ("https://researchgate.net/publication/1", "aggregator", "metadata_or_aggregator"),
        ):
            with self.subTest(url=url):
                item = self.manifest["evidence"][0]
                item["canonical_url"] = url
                item["url_role"] = role
                self.manifest["sources"][0]["canonical_url"] = url
                self.manifest["evidence"][0]["snapshot"]["canonical_url"] = url
                with self.assertRaisesRegex(ValueError, needle):
                    validate_manifest(self.manifest, self.base)
                item["canonical_url"] = "https://example.edu/paper/full-text"
                item["url_role"] = "original_content"
                self.manifest["sources"][0]["canonical_url"] = "https://example.edu/paper/full-text"
                self.manifest["evidence"][0]["snapshot"]["canonical_url"] = "https://example.edu/paper/full-text"

    def test_interstitial_and_metadata_content_fail_closed(self) -> None:
        for text in ("Please log in to continue", "Accept all cookies", "Title: Only metadata\nDOI: 10.1/x", "Enable JavaScript"):
            self.content.write_text(text, encoding="utf-8")
            with self.subTest(text=text):
                with self.assertRaises(ValueError):
                    validate_manifest(self.manifest, self.base)
            self.content.write_text("Actual full text. " * 40, encoding="utf-8")
            digest = hashlib.sha256(self.content.read_bytes()).hexdigest()
            self.manifest["evidence"][0]["snapshot_sha256"] = digest
            self.manifest["evidence"][0]["snapshot"]["sha256"] = digest

    def test_incidental_login_phrase_in_long_fulltext_is_allowed(self) -> None:
        self.content.write_text("Sign in is discussed as a study limitation. " * 60, encoding="utf-8")
        digest = hashlib.sha256(self.content.read_bytes()).hexdigest()
        self.manifest["evidence"][0]["snapshot_sha256"] = digest
        self.manifest["evidence"][0]["snapshot"]["sha256"] = digest
        validate_manifest(self.manifest, self.base)

    def test_deterministic_data_check_requires_all_proofs(self) -> None:
        data_path = self.base / "data.csv"
        data_path.write_text("value,unit\n1,kg\n2,kg\n", encoding="utf-8")
        digest = hashlib.sha256(data_path.read_bytes()).hexdigest()
        url = "https://example.edu/data/original.csv"
        manifest = copy.deepcopy(self.manifest)
        manifest["sources"][0]["canonical_url"] = url
        item = manifest["evidence"][0]
        item.update({"canonical_url": url, "content_scope": "data_file", "content_kind": "data_file", "snapshot_sha256": digest})
        item["snapshot"].update({"path": "data.csv", "sha256": digest, "canonical_url": url})
        manifest["claims"][0]["canonical_url"] = url
        manifest["claims"][0]["lineage_sha256"] = lineage_hash(manifest["claims"][0])
        manifest["data_files"] = [{
            "data_file_id": "data-1", "evidence_id": "ev-1", "path": "data.csv",
            "downloaded": True, "original_data": True, "generated": False, "quarantine_status": "accepted",
            "deterministic_check": {
                "status": "pass", "sha256": digest, "columns": ["value", "unit"], "row_count": 2,
                "encoding": "utf-8", "delimiter": ",", "units": {"value": "kg"},
                "null_handling": "reject nulls", "tabular": True, "parser": "csv", "parser_version": "1",
            },
        }]
        manifest["reviews"][1]["reviewed_ids"] = ["data-1"]
        validate_manifest(manifest, self.base)
        for field in ("sha256", "columns", "row_count", "encoding", "delimiter", "units", "null_handling"):
            broken = copy.deepcopy(manifest)
            broken["data_files"][0]["deterministic_check"].pop(field)
            with self.subTest(field=field):
                with self.assertRaisesRegex(ValueError, "deterministic_data"):
                    validate_manifest(broken, self.base)

    def test_reviews_must_cover_the_full_target_set(self) -> None:
        self.manifest["reviews"][0]["reviewed_ids"] = []
        with self.assertRaisesRegex(ValueError, "full_target_set"):
            validate_manifest(self.manifest, self.base)

    def test_claim_lineage_hash_is_bound(self) -> None:
        claim = {"claim_id": "c-2", "claim_text": "Measured value is 42.", "evidence_id": "ev-1",
                 "snapshot_id": "snap-1", "source_id": "src-1", "canonical_url": "https://example.edu/paper/full-text"}
        claim["lineage_sha256"] = lineage_hash(claim)
        self.manifest["claims"].append(claim)
        self.manifest["reviews"][2]["reviewed_ids"] = ["c-1", "c-2"]
        validate_manifest(self.manifest, self.base)
        claim["claim_text"] = "Changed without a new lineage hash."
        with self.assertRaisesRegex(ValueError, "lineage_hash"):
            validate_manifest(self.manifest, self.base)


if __name__ == "__main__":
    unittest.main()
