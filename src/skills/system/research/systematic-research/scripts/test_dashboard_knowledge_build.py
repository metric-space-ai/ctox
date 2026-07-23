#!/usr/bin/env python3
"""Regression tests for the deterministic dashboard writeback builder.

Covers the F-003/F-004 repair and Workstream D contract items:

(a) builder ``measured_load_points`` output passes the native field/kind
    contract mirrored from ``src/core/business_os/store.rs``;
(b) the full candidate audit inventory survives with rejection reasons and
    canonical dedup across discovery rounds, and rejected candidates are
    never promoted to ``source_catalog``;
(d) a malformed ENOLA header (missing delimiter between ``THRUST[N]`` and
    ``u_THRUST[N]``) is repaired only via the deterministic, audited parser
    rule — never by inventing measurements.
"""

from __future__ import annotations

import csv
import hashlib
import json
import math
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import dashboard_knowledge_build as builder  # noqa: E402


ENOLA_HEADER = "RPM,THRUST[N],u_THRUST[N],TORQUE[NM],u_TORQUE[NM],CT,CP"
ENOLA_HEADER_MALFORMED = "RPM,THRUST[N]u_THRUST[N],TORQUE[NM],u_TORQUE[NM],CT,CP"
ENOLA_ROWS = (
    "2700,0.5580,0.0110,0.0312,0.0011,0.0123,0.0824\n"
    "2800,0.6010,0.0120,0.0331,0.0012,0.0131,0.0830\n"
    "2900,NaN,0.0125,0.0338,0.0013,0.0138,0.0835\n"
)


def parse_default_member(text: str = ENOLA_HEADER + "\n" + ENOLA_ROWS) -> dict:
    return builder.parse_enola_member_csv(text, "BBDD/APC/APC15x8/results/APC15x8_exp.csv")


def measured_kwargs(parsed: dict) -> dict:
    return {
        "research_run_id": "run-1",
        "research_command_id": "cmd-1",
        "source_id": "SRC-ENOLA",
        "canonical_url": "https://zenodo.org/api/records/20111572/files/Propeller_Database.zip/content",
        "snapshot_hash": "c9e92c5e",
        "propeller_size": "APC15x8",
        "archive_manifest_hash": "a" * 64,
        "member_path": "BBDD/APC/APC15x8/results/APC15x8_exp.csv",
        "member_hash": "b" * 64,
        "parsed": parsed,
    }


class PropellerNotationTests(unittest.TestCase):
    def test_splits_numeric_diameter_and_pitch(self) -> None:
        self.assertEqual(builder.parse_propeller_notation("9x5"), (9.0, 5.0))
        self.assertEqual(builder.parse_propeller_notation("APC15x8"), (15.0, 8.0))
        self.assertEqual(builder.parse_propeller_notation("AeronautCAM9x5"), (9.0, 5.0))

    def test_missing_notation_fails_closed(self) -> None:
        with self.assertRaisesRegex(ValueError, "propeller_notation_missing"):
            builder.parse_propeller_notation("unknown prop")


class EnolaParserTests(unittest.TestCase):
    def test_wellformed_header_needs_no_repair(self) -> None:
        parsed = parse_default_member()
        self.assertFalse(parsed["audit"]["header_repaired"])
        self.assertEqual(parsed["audit"]["repairs"], [])
        self.assertEqual(len(parsed["rows"]), 3)

    def test_malformed_header_repaired_by_audited_rule(self) -> None:
        parsed = parse_default_member(ENOLA_HEADER_MALFORMED + "\n" + ENOLA_ROWS)
        audit = parsed["audit"]
        self.assertTrue(audit["header_repaired"])
        self.assertEqual(len(audit["repairs"]), 1)
        repair = audit["repairs"][0]
        self.assertEqual(repair["rule"], builder.ENOLA_HEADER_REPAIR_RULE)
        self.assertEqual(repair["original_token"], "THRUST[N]u_THRUST[N]")
        self.assertEqual(repair["repaired_tokens"], ["THRUST[N]", "u_THRUST[N]"])
        self.assertEqual(parsed["headers"], ENOLA_HEADER.split(","))
        # The audit binds the repair to the original bytes.
        self.assertEqual(audit["original_header"], ENOLA_HEADER_MALFORMED)
        self.assertEqual(len(audit["original_header_sha256"]), 64)

    def test_data_width_mismatch_after_repair_fails_closed(self) -> None:
        # One data row is missing the uncertainty field; repairing the header
        # must not invent the missing measurement.
        text = ENOLA_HEADER_MALFORMED + "\n" + "2700,0.5580,0.0312,0.0011,0.0123,0.0824\n"
        with self.assertRaisesRegex(ValueError, "enola_row_field_count_mismatch"):
            builder.parse_enola_member_csv(text, "member.csv")

    def test_missing_thrust_column_is_never_invented(self) -> None:
        text = "RPM,CT,CP\n2700,0.0123,0.0824\n"
        with self.assertRaisesRegex(ValueError, "enola_required_column_missing"):
            builder.parse_enola_member_csv(text, "member.csv")

    def test_thrust_without_explicit_newton_unit_fails(self) -> None:
        text = "RPM,THRUST,CT\n2700,0.5580,0.0123\n"
        with self.assertRaisesRegex(ValueError, "enola_required_column_missing|enola_unit_missing"):
            builder.parse_enola_member_csv(text, "member.csv")


class MeasuredLoadPointsContractTests(unittest.TestCase):
    def test_builder_output_passes_native_field_and_kind_contract(self) -> None:
        parsed = parse_default_member()
        rows, reconciliation = builder.build_measured_load_points(**measured_kwargs(parsed))
        # Native-contract mirror must accept every emitted row.
        builder.assert_native_measured_contract(rows)
        self.assertEqual(len(rows), 2)  # NaN-thrust row dropped with reason
        row = rows[0]
        self.assertEqual(row["measurement_kind"], "experimental")
        self.assertEqual(row["is_derived"], "false")
        self.assertGreater(float(row["rpm"]), 0.0)
        self.assertEqual(row["thrust_unit"], "N")
        self.assertEqual(row["propeller_size"], "APC15x8")
        self.assertEqual(float(row["prop_diameter_in"]), 15.0)
        self.assertEqual(float(row["prop_pitch_in"]), 8.0)
        self.assertTrue(row["source_row_ref"].endswith("#row-2"))
        self.assertEqual(row["u_thrust_N"], builder.format_number(0.0110))
        # Uncertainty and units are explicit and machine-readable.
        self.assertNotIn(",", row["rpm"].split(".")[0][1:])
        # Reconciliation: no partial silent import.
        self.assertEqual(reconciliation["source_rows"], 3)
        self.assertEqual(reconciliation["emitted_rows"], 2)
        self.assertEqual(
            reconciliation["dropped_rows"],
            [
                {
                    "source_row_ref": "BBDD/APC/APC15x8/results/APC15x8_exp.csv#row-4",
                    "reason": "thrust_N_missing_or_not_numeric",
                }
            ],
        )

    def test_native_contract_mirror_rejects_legacy_bad_artifact_shape(self) -> None:
        parsed = parse_default_member()
        rows, _ = builder.build_measured_load_points(**measured_kwargs(parsed))
        bad = dict(rows[0])
        bad["measurement_kind"] = "static_thrust_CT_CP"
        with self.assertRaisesRegex(ValueError, "measurement_kind"):
            builder.assert_native_measured_contract([bad])
        bad = dict(rows[0])
        del bad["thrust_N"]
        bad["axial_force_N"] = bad.get("rpm", "")
        with self.assertRaisesRegex(ValueError, "axial_force"):
            builder.assert_native_measured_contract([bad])

    def test_zero_rpm_rows_are_dropped_with_reason_not_silently(self) -> None:
        text = ENOLA_HEADER + "\n" + "0,0.5580,0.0110,0.0312,0.0011,0.0123,0.0824\n2700,0.6,0.01,0.03,0.001,0.013,0.083\n"
        parsed = builder.parse_enola_member_csv(text, "member.csv")
        rows, reconciliation = builder.build_measured_load_points(**measured_kwargs(parsed))
        self.assertEqual(len(rows), 1)
        self.assertEqual(reconciliation["dropped_rows"][0]["reason"], "rpm_missing_or_not_positive")

    def test_all_rows_dropped_fails_closed(self) -> None:
        text = ENOLA_HEADER + "\n" + "0,0.5580,0.0110,0.0312,0.0011,0.0123,0.0824\n"
        parsed = builder.parse_enola_member_csv(text, "member.csv")
        with self.assertRaisesRegex(ValueError, "no_emittable_measurements"):
            builder.build_measured_load_points(**measured_kwargs(parsed))


class DerivedBearingLoadsTests(unittest.TestCase):
    def claim(self) -> dict[str, str]:
        return {
            "claim_id": "claim-1",
            "evidence_id": "ev-1",
            "source_id": "SRC-ENOLA",
            "snapshot_id": "snap-1",
            "canonical_url": "https://zenodo.org/api/records/20111572/files/Propeller_Database.zip/content",
            "snapshot_hash": "c9e92c5e",
            "quote": "RPM,THRUST[N],u_THRUST[N],TORQUE[NM],u_TORQUE[NM],CT,CP",
        }

    def test_ct_cp_conversion_is_derived_with_full_lineage(self) -> None:
        parsed = parse_default_member()
        rows, _ = builder.build_derived_bearing_loads(
            research_run_id="run-1",
            research_command_id="cmd-1",
            claim=self.claim(),
            propeller_size="APC15x8",
            archive_manifest_hash="a" * 64,
            member_path="BBDD/APC/APC15x8/results/APC15x8_exp.csv",
            member_hash="b" * 64,
            parsed=parsed,
        )
        self.assertEqual(len(rows), 3)  # CT present even where thrust is NaN
        row = rows[0]
        self.assertEqual(row["is_derived"], "true")
        self.assertEqual(row["claim_id"], "claim-1")
        self.assertIn("CT * rho", row["formula"])
        self.assertIn("rho=1.225", row["constants"])
        self.assertIn("N", row["units"])
        self.assertTrue(row["source_row_ref"].endswith("#row-2"))
        expected = 0.0123 * 1.225 * (2700 / 60.0) ** 2 * (15 * 0.0254) ** 4
        self.assertAlmostEqual(float(row["thrust_N"]), expected, places=6)
        expected_torque = (
            0.0824
            * 1.225
            * (2700 / 60.0) ** 2
            * (15 * 0.0254) ** 5
            / (2.0 * math.pi)
        )
        self.assertAlmostEqual(float(row["torque_Nm"]), expected_torque, places=6)
        self.assertIn("/ (2*pi)", row["formula"])
        self.assertEqual(row["bearing_radial_load_N"], "")

    def test_derived_rows_never_mix_measured_axial_with_inferred_radial(self) -> None:
        parsed = parse_default_member()
        measured_rows, _ = builder.build_measured_load_points(**measured_kwargs(parsed))
        derived_rows, _ = builder.build_derived_bearing_loads(
            research_run_id="run-1",
            research_command_id="cmd-1",
            claim=self.claim(),
            propeller_size="APC15x8",
            archive_manifest_hash="a" * 64,
            member_path="BBDD/APC/APC15x8/results/APC15x8_exp.csv",
            member_hash="b" * 64,
            parsed=parsed,
        )
        for row in measured_rows:
            self.assertEqual(row["is_derived"], "false")
            self.assertNotIn("bearing_radial_load_N", row)
        for row in derived_rows:
            self.assertEqual(row["is_derived"], "true")
            self.assertEqual(row["bearing_radial_load_N"], "")


class CandidateInventoryTests(unittest.TestCase):
    def _discovery_dir(self, root: Path, name: str, candidates: list[dict[str, str]], rejected: list[dict[str, str]]) -> Path:
        directory = root / name
        directory.mkdir(parents=True)
        with (directory / "candidate_sources.csv").open("w", newline="", encoding="utf-8") as handle:
            writer = csv.DictWriter(
                handle,
                fieldnames=["focus", "query", "title", "url", "doi", "openalex_id", "snippet", "review_status"],
            )
            writer.writeheader()
            writer.writerows(candidates)
        with (directory / "rejected_sources.csv").open("w", newline="", encoding="utf-8") as handle:
            writer = csv.DictWriter(
                handle,
                fieldnames=["focus", "query", "title", "url", "doi", "openalex_id", "snippet", "screening_status", "screening_reason"],
            )
            writer.writeheader()
            writer.writerows(rejected)
        return directory

    def test_full_candidate_inventory_survives_with_rejection_reasons(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            round_one = [
                {"focus": "web", "query": "q1", "title": f"Source {i}", "url": f"https://example.edu/{i}", "doi": "", "openalex_id": "", "snippet": "s"}
                for i in range(100)
            ]
            round_two = [
                {"focus": "dataset", "query": "q2", "title": f"Paper {i}", "url": "", "doi": f"10.1234/{i}", "openalex_id": "", "snippet": "s"}
                for i in range(80)
            ]
            # Duplicate of a round-one URL under a different round (trailing slash alias).
            round_two.append({"focus": "dataset", "query": "q2", "title": "Source 0 alias", "url": "https://example.edu/0/", "doi": "", "openalex_id": "", "snippet": "s"})
            rejected = [
                {"focus": "web", "query": "q1", "title": "Metadata page", "url": "https://aggregator.example/1", "doi": "", "openalex_id": "", "snippet": "", "screening_status": "rejected", "screening_reason": "metadata_only"},
                {"focus": "web", "query": "q1", "title": "Dead link", "url": "https://example.edu/404", "doi": "", "openalex_id": "", "snippet": "", "screening_status": "rejected", "screening_reason": "http_404"},
                {"focus": "web", "query": "q1", "title": "Off topic", "url": "https://example.edu/offtopic", "doi": "", "openalex_id": "", "snippet": "", "screening_status": "rejected", "screening_reason": "irrelevant"},
            ]
            dir_one = self._discovery_dir(root, "round1", round_one, rejected[:1])
            dir_two = self._discovery_dir(root, "round2", round_two, rejected[1:])

            admitted = {builder.canonical_url_key("https://example.edu/1")}
            rows = builder.build_source_candidates(
                research_run_id="run-1",
                research_command_id="cmd-1",
                discovery_dirs=[dir_one, dir_two],
                admitted_urls=admitted,
            )
            # 100 + 80 unique + 3 rejected = 183; the alias dedups into one.
            self.assertEqual(len(rows), 183)
            by_url = {row["url"].rstrip("/"): row for row in rows if row["url"]}
            self.assertEqual(by_url["https://example.edu/1"]["verification_state"], "admitted")
            self.assertEqual(by_url["https://example.edu/1"]["rejection_reason"], "")
            self.assertEqual(by_url["https://aggregator.example/1"]["verification_state"], "rejected")
            self.assertEqual(by_url["https://aggregator.example/1"]["rejection_reason"], "metadata_only")
            self.assertEqual(by_url["https://example.edu/404"]["rejection_reason"], "http_404")
            pending = by_url["https://example.edu/2"]
            self.assertEqual(pending["verification_state"], "not_promoted")
            self.assertTrue(pending["rejection_reason"])
            # Every row carries run lineage and no inventory was truncated.
            self.assertTrue(all(row["research_run_id"] == "run-1" for row in rows))
            keys = [row["candidate_key"] for row in rows]
            self.assertEqual(len(keys), len(set(keys)))

    def test_rejected_candidates_are_never_promoted_to_source_catalog(self) -> None:
        manifest = {
            "sources": [
                {"source_id": "src-1", "canonical_url": "https://example.edu/admitted"},
                {"source_id": "src-2", "canonical_url": "https://aggregator.example/1"},
            ],
            "evidence": [
                {
                    "evidence_id": "ev-1",
                    "source_id": "src-1",
                    "evidence_status": "eligible",
                    "snapshot_sha256": "abc",
                    "snapshot_id": "snap-1",
                    "relevance_score": 9,
                },
                {
                    "evidence_id": "ev-2",
                    "source_id": "src-2",
                    "evidence_status": "rejected",
                    "snapshot_sha256": "def",
                    "snapshot_id": "snap-2",
                    "relevance_score": 3,
                },
            ],
        }
        catalog = builder.build_source_catalog(
            research_run_id="run-1",
            research_command_id="cmd-1",
            manifest=manifest,
        )
        self.assertEqual([row["source_id"] for row in catalog], ["src-1"])
        self.assertNotIn("aggregator.example", json.dumps(catalog))

    def test_jsonl_inventories_are_loaded_and_deduplicated_with_csv(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            (directory / "candidate_sources.jsonl").write_text(
                "\n".join(
                    [
                        json.dumps(
                            {
                                "title": "Paper",
                                "url": "https://example.edu/paper",
                                "doi": "10.1234/paper",
                            }
                        ),
                        json.dumps(
                            {
                                "title": "Dataset",
                                "url": "https://example.edu/data",
                                "content_hash": "c" * 64,
                            }
                        ),
                    ]
                )
                + "\n",
                encoding="utf-8",
            )
            (directory / "rejected_sources.jsonl").write_text(
                json.dumps(
                    {
                        "title": "Paper duplicate",
                        "url": "https://elsewhere.example/paper",
                        "doi": "https://doi.org/10.1234/paper",
                        "screening_reason": "metadata_only",
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            (directory / "candidate_sources.csv").write_text(
                "title,url,doi\nPaper alias,https://example.edu/alias,10.1234/paper\n",
                encoding="utf-8",
            )
            rows = builder.build_source_candidates(
                research_run_id="run-1",
                research_command_id="cmd-1",
                discovery_dirs=[directory],
                admitted_urls=set(),
            )
            self.assertEqual(len(rows), 2)
            paper = next(row for row in rows if row["candidate_key"] == "doi:10.1234/paper")
            self.assertEqual(paper["verification_state"], "rejected")
            self.assertEqual(paper["rejection_reason"], "metadata_only")

    def test_invalid_jsonl_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            (directory / "candidate_sources.jsonl").write_text("{not-json}\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "candidate_jsonl_invalid"):
                builder.iter_discovery_rows([directory])


class BuilderCliTests(unittest.TestCase):
    def _binding(self, root: Path, member: Path) -> Path:
        member_path = "BBDD/APC/APC15x8/results/APC15x8_exp.csv"
        member_hash = hashlib.sha256(member.read_bytes()).hexdigest()
        manifest = {
            "schema_version": "ctox.web.zip-manifest.v2",
            "archive_sha256": "c" * 64,
            "members": [{"path": member_path, "sha256": f"sha256:{member_hash}"}],
        }
        manifest_path = root / "archive.zip.manifest.json"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        binding = {
            "csv_path": str(member),
            "propeller_size": "APC15x8",
            "source_id": "SRC-ENOLA",
            "canonical_url": "https://zenodo.org/api/records/20111572/files/Propeller_Database.zip/content",
            "archive_sha256": "c" * 64,
            "manifest_path": str(manifest_path),
            "manifest_sha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
            "member_path": member_path,
            "member_sha256": member_hash,
        }
        binding_path = root / "binding.json"
        binding_path.write_text(json.dumps(binding), encoding="utf-8")
        return binding_path

    def test_cli_writes_native_contract_tables_and_reconciliation(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            member = root / "APC15x8_exp.csv"
            member.write_text(ENOLA_HEADER_MALFORMED + "\n" + ENOLA_ROWS, encoding="utf-8")
            binding = self._binding(root, member)
            discovery = root / "discovery"
            discovery.mkdir()
            (discovery / "candidate_sources.csv").write_text(
                "focus,query,title,url,doi,openalex_id,snippet,review_status\n"
                "web,q,Source,https://example.edu/1,,,s,agent_review_required\n",
                encoding="utf-8",
            )
            out_dir = root / "dashboard" / "knowledge"
            result = builder.main([
                "--research-run-id", "run-1",
                "--research-command-id", "cmd-1",
                "--out-dir", str(out_dir),
                "--discovery-dir", str(discovery),
                "--enola-member-binding", str(binding),
            ])
            self.assertEqual(result, 0)
            with (out_dir / "measured_load_points.csv").open(newline="", encoding="utf-8") as handle:
                rows = list(csv.DictReader(handle))
            builder.assert_native_measured_contract(rows)
            self.assertEqual(rows[0]["measurement_kind"], "experimental")
            self.assertEqual(rows[0]["parsing_rule"], builder.ENOLA_HEADER_REPAIR_RULE)
            with (out_dir / "source_candidates.csv").open(newline="", encoding="utf-8") as handle:
                candidates = list(csv.DictReader(handle))
            self.assertEqual(len(candidates), 1)
            self.assertEqual(candidates[0]["verification_state"], "not_promoted")
            report = json.loads((out_dir / "writeback_reconciliation.json").read_text(encoding="utf-8"))
            self.assertEqual(report["measured_rows"], 2)
            self.assertEqual(report["reconciliations"][0]["source_rows"], 3)
            self.assertTrue(report["parser_audits"][0]["header_repaired"])

    def test_enola_binding_rejects_tampered_member_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            member = root / "APC15x8_exp.csv"
            member.write_text(ENOLA_HEADER + "\n" + ENOLA_ROWS, encoding="utf-8")
            binding = self._binding(root, member)
            member.write_text(ENOLA_HEADER + "\n2700,99,0,0,0,0,0\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "extracted_member_sha256_mismatch"):
                builder.load_enola_member_binding(binding)

    def test_legacy_enola_binding_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaisesRegex(ValueError, "legacy_enola_member_binding_unsupported"):
                builder.main(
                    [
                        "--research-run-id",
                        "run-1",
                        "--research-command-id",
                        "cmd-1",
                        "--out-dir",
                        tmp,
                        "--enola-member",
                        "legacy",
                    ]
                )


if __name__ == "__main__":
    unittest.main()
