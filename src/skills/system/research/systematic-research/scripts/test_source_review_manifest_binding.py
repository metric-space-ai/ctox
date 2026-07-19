#!/usr/bin/env python3
"""Focused tests for source-review manifest binding and fail-closed inputs."""

from __future__ import annotations

import copy
import csv
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import source_review_reading as reading  # noqa: E402
import source_review_report as report  # noqa: E402
from evidence_guard import SCHEMA_VERSION, lineage_hash  # noqa: E402


class SourceReviewManifestBindingTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.base = Path(self.tmp.name)
        self.snapshot = self.base / "original.txt"
        self.snapshot.write_text(
            "Original full text with a measured thrust of 42 N and test methods. " * 20,
            encoding="utf-8",
        )
        self.manifest = self._manifest("src-1", "ev-1", "snap-1", "https://publisher.example/full-text")
        self.bindings = reading.load_manifest_bindings(self.manifest, self.base)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _manifest(self, source_id: str, evidence_id: str, snapshot_id: str, url: str) -> dict:
        digest = hashlib.sha256(self.snapshot.read_bytes()).hexdigest()
        retrieval = self._artifact(
            f"{evidence_id}-retrieval.json",
            json.dumps({"tool": "ctox_web_read", "url": url}),
        )
        claim = {
            "claim_id": f"claim-{evidence_id}",
            "claim_text": "The source reports a measured thrust.",
            "evidence_quote": "Original full text with a measured thrust of 42 N and test methods.",
            "evidence_id": evidence_id,
            "snapshot_id": snapshot_id,
            "source_id": source_id,
            "canonical_url": url,
        }
        claim["lineage_sha256"] = lineage_hash(claim)
        return {
            "schema_version": SCHEMA_VERSION,
            "run_id": "run-1",
            "research_run_id": "run-1",
            "research_command_id": "command-1",
            "research_attempt_id": "attempt-1",
            "as_of": "2026-07-17",
            "sources": [{"source_id": source_id, "canonical_url": url}],
            "evidence": [{
                "evidence_id": evidence_id,
                "source_id": source_id,
                "canonical_url": url,
                "url_role": "original_content",
                "http_status": 200,
                "retrieved_at": "2026-07-17T08:00:00Z",
                "freshness_status": "current",
                "relevance_score": 9,
                "evidence_status": "eligible",
                "content_scope": "full_text",
                "content_kind": "full_text",
                "snapshot_id": snapshot_id,
                "snapshot_sha256": digest,
                "snapshot": {
                    "snapshot_id": snapshot_id,
                    "path": "original.txt",
                    "sha256": digest,
                    "source_id": source_id,
                    "canonical_url": url,
                },
                "extracted_text": {
                    "path": "original.txt",
                    "sha256": digest,
                    "source_snapshot_sha256": digest,
                },
                "retrieval_receipt": {
                    "tool": "ctox_web_read",
                    "request_url": url,
                    "final_url": url,
                    "http_status": 200,
                    "checked_at": "2026-07-17T08:00:00Z",
                    "body_sha256": digest,
                    "byte_count": self.snapshot.stat().st_size,
                    "receipt_artifact": retrieval,
                },
            }],
            "claims": [claim],
            "data_files": [],
            "knowledge": {"living": False},
        }

    def _artifact(self, name: str, content: str) -> dict[str, str]:
        path = self.base / name
        path.write_text(content, encoding="utf-8")
        return {
            "path": name,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        }

    def _eligible_status_row(self, binding: dict[str, object] | None = None) -> dict[str, str]:
        binding = binding or self.bindings["ev-1"]
        return {
            **reading.binding_output_fields(binding),
            "evidence_eligible": "true",
            "status": "evidence",
            "read_url": str(binding["canonical_url"]),
        }

    def test_normalize_doi_rejects_ordinary_urls(self) -> None:
        self.assertEqual(reading.normalize_doi("https://example.com/article"), "")
        self.assertEqual(report.normalize_doi("https://example.com/article"), "")
        self.assertEqual(reading.normalize_doi("https://doi.org/10.1234/example."), "10.1234/example")
        self.assertEqual(report.normalize_doi("doi:10.1234/example"), "10.1234/example")

    def test_manifest_requires_retrieved_at_and_rehashes_snapshot(self) -> None:
        missing_timestamp = copy.deepcopy(self.manifest)
        missing_timestamp["evidence"][0].pop("retrieved_at")
        with self.assertRaisesRegex(ValueError, "retrieved_at"):
            reading.load_manifest_bindings(missing_timestamp, self.base)

        self.snapshot.write_text("tampered", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "sha256"):
            reading.load_manifest_bindings(self.manifest, self.base)

    def test_metadata_landing_and_doi_urls_are_not_original_evidence(self) -> None:
        for url, role, scope in (
            ("https://doi.org/10.1234/example", "original_content", "full_text"),
            ("https://publisher.example/article", "landing", "full_text"),
            ("https://publisher.example/metadata", "original_content", "metadata"),
        ):
            manifest = copy.deepcopy(self.manifest)
            manifest["sources"][0]["canonical_url"] = url
            item = manifest["evidence"][0]
            item["canonical_url"] = url
            item["url_role"] = role
            item["content_scope"] = scope
            item["snapshot"]["canonical_url"] = url
            with self.subTest(url=url):
                with self.assertRaises(ValueError):
                    reading.load_manifest_bindings(manifest, self.base)

    def test_candidate_requires_exact_manifest_identity(self) -> None:
        binding = self.bindings["ev-1"]
        self.assertIs(reading.binding_for_candidate({"url": binding["canonical_url"]}, self.bindings), binding)
        self.assertIsNone(reading.binding_for_candidate({"url": "https://doi.org/10.1234/example"}, self.bindings))
        self.assertIsNone(reading.binding_for_candidate({"title": "same-looking source", "openalex_id": "W1"}, self.bindings))

    def test_valid_lineage_passes_and_unrelated_manifest_fails(self) -> None:
        status = self._eligible_status_row()
        measurement = {
            **reading.binding_output_fields(self.bindings["ev-1"]),
            "source_url": "https://publisher.example/full-text",
            "family": "thrust_force",
            "value": "42",
            "unit": "N",
        }
        eligible, measurements = reading.validate_reading_artifacts([status], [measurement], self.bindings)
        self.assertEqual(eligible, [status])
        self.assertEqual(measurements, [measurement])

        unrelated = self._manifest("src-2", "ev-2", "snap-2", "https://other.example/full-text")
        with self.assertRaisesRegex(ValueError, "reading_manifest_binding"):
            reading.validate_reading_artifacts([status], [], reading.load_manifest_bindings(unrelated, self.base))

    def test_reader_emits_manifest_lineage_on_status_and_measurement_rows(self) -> None:
        discovery_dir = self.base / "discovery"
        reading_dir = self.base / "reading"
        discovery_dir.mkdir()
        with (discovery_dir / "candidate_sources.csv").open("w", newline="", encoding="utf-8") as handle:
            writer = csv.DictWriter(
                handle,
                fieldnames=["focus", "query", "title", "url", "doi", "openalex_id", "snippet", "relevance_score", "acceptance_reason"],
            )
            writer.writeheader()
            writer.writerow({
                "title": "Authoritative source",
                "url": "https://publisher.example/full-text",
                "relevance_score": "9",
                "acceptance_reason": "primary source",
            })
        manifest_path = self.base / "manifest.json"
        manifest_path.write_text(json.dumps(self.manifest), encoding="utf-8")

        reading.main([
            "--discovery-dir", str(discovery_dir),
            "--out-dir", str(reading_dir),
            "--evidence-manifest", str(manifest_path),
        ])

        with (reading_dir / "reading_status.csv").open(newline="", encoding="utf-8") as handle:
            status_rows = list(csv.DictReader(handle))
        with (reading_dir / "extracted_measurements.csv").open(newline="", encoding="utf-8") as handle:
            measurement_rows = list(csv.DictReader(handle))
        self.assertEqual(status_rows[0]["evidence_eligible"], "true")
        self.assertEqual(status_rows[0]["canonical_url"], "https://publisher.example/full-text")
        self.assertEqual(measurement_rows[0]["evidence_id"], "ev-1")
        self.assertEqual(measurement_rows[0]["sha256"], status_rows[0]["sha256"])

    def test_report_rejects_eligible_row_from_unrelated_valid_manifest(self) -> None:
        reading_dir = self.base / "reading"
        discovery_dir = self.base / "discovery"
        reading_dir.mkdir()
        discovery_dir.mkdir()
        with (reading_dir / "reading_status.csv").open("w", newline="", encoding="utf-8") as handle:
            writer = csv.DictWriter(handle, fieldnames=list(self._eligible_status_row()))
            writer.writeheader()
            writer.writerow(self._eligible_status_row())
        with (reading_dir / "extracted_measurements.csv").open("w", newline="", encoding="utf-8") as handle:
            handle.write("source_id,evidence_id,snapshot_id,canonical_url,url_role,content_scope,http_status,retrieved_at,freshness,snapshot_path,sha256,source_url,family,value,unit\n")
        unrelated = self._manifest("src-2", "ev-2", "snap-2", "https://other.example/full-text")
        with self.assertRaisesRegex(ValueError, "reading_manifest_binding"):
            report.build_docx(
                "Test",
                "topic",
                discovery_dir,
                reading_dir,
                self.base / "report.docx",
                manifest=unrelated,
                manifest_base_dir=self.base,
            )


if __name__ == "__main__":
    unittest.main()
