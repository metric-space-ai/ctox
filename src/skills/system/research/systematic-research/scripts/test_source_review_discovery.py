#!/usr/bin/env python3
"""Regression tests for source-review discovery safety boundaries."""

from __future__ import annotations

import csv
import importlib.util
import json
import os
import shlex
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("source_review_discovery.py")

_spec = importlib.util.spec_from_file_location("source_review_discovery", SCRIPT)
discovery = importlib.util.module_from_spec(_spec)
sys.modules["source_review_discovery"] = discovery
_spec.loader.exec_module(discovery)


def write_fake_ctox(root: Path, payloads: dict[str, dict]) -> Path:
    """Install a fake ``ctox`` binary that answers deep-research queries from
    a fixed query->payload map and logs its argv for exclude-list assertions."""
    lines = [
        "#!/bin/sh",
        f'printf \'%s\\n\' "$@" >> "{root}/argv.log"',
        'query=""',
        'prev=""',
        'for arg in "$@"; do',
        '  if [ "$prev" = "--query" ]; then query="$arg"; fi',
        '  prev="$arg"',
        'done',
        'case "$query" in',
    ]
    for query, payload in payloads.items():
        lines.append(f"  {shlex.quote(query)}) echo {shlex.quote(json.dumps(payload))} ;;")
    lines.append('  *) echo \'{"sources": []}\' ;;')
    lines.append("esac")
    path = root / "ctox"
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    path.chmod(0o755)
    return path


def run_discovery(root: Path, queries: list[tuple[str, str]], extra_args: list[str] | None = None) -> subprocess.CompletedProcess:
    queries_csv = root / "queries.csv"
    with queries_csv.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.writer(handle)
        writer.writerow(["focus", "query"])
        writer.writerows(queries)
    env = os.environ.copy()
    env["PATH"] = f"{root}{os.pathsep}{env.get('PATH', '')}"
    cmd = [
        sys.executable,
        str(SCRIPT),
        "--topic",
        "fixture topic",
        "--out-dir",
        str(root / "out"),
        "--queries-file",
        str(queries_csv),
        "--web-query-delay-sec",
        "0",
    ]
    cmd.extend(extra_args or [])
    return subprocess.run(cmd, env=env, capture_output=True, text=True, check=False)


class CanonicalDedupTests(unittest.TestCase):
    def test_doi_and_doi_org_url_share_one_identity(self) -> None:
        self.assertEqual(
            discovery.source_key({"doi": "10.1234/ABC.Def"}),
            discovery.source_key({"url": "https://doi.org/10.1234/abc.def"}),
        )
        self.assertEqual(
            discovery.source_key({"DOI": "doi:10.1234/abc.def"}),
            "doi:10.1234/abc.def",
        )

    def test_decorated_urls_collapse_to_canonical_identity(self) -> None:
        decorated = {"url": "https://www.Example.com/data/?utm_source=nl&gclid=xyz&id=42#frag"}
        canonical = {"url": "https://example.com/data?id=42"}
        self.assertEqual(discovery.source_key(decorated), discovery.source_key(canonical))
        self.assertEqual(
            discovery.source_key(canonical),
            "url:https://example.com/data?id=42",
        )

    def test_content_hash_alias_dedupes_distinct_urls(self) -> None:
        digest = "a" * 64
        left = discovery.source_identity_keys(
            {"snapshot_hash": digest, "url": "https://a.example/x"}
        )
        right = discovery.source_identity_keys(
            {"content_hash": f"sha256:{digest}", "url": "https://b.example/y"}
        )
        self.assertIn(f"sha256:{digest}", left & right)
        self.assertEqual(
            discovery.source_key({"snapshot_hash": "not-a-hash", "url": "https://a.example/x"}),
            "url:https://a.example/x",
        )

    def test_distinct_sources_keep_distinct_keys(self) -> None:
        self.assertNotEqual(
            discovery.source_key({"url": "https://a.example/one"}),
            discovery.source_key({"url": "https://a.example/two"}),
        )
        self.assertEqual(
            discovery.source_key({"title": "  Some   Paper "}),
            "title:some paper",
        )

    def test_openalex_id_is_a_stable_identity(self) -> None:
        self.assertEqual(
            discovery.source_key({"openalex_id": "https://openalex.org/W123"}),
            discovery.source_key({"openalex_id": "W123"}),
        )

    def test_candidate_exclude_urls_dedupes_and_preserves_order(self) -> None:
        rows = [
            {"url": "https://a.example/x"},
            {"url": "https://b.example/y"},
            {"url": "https://a.example/x"},
            {"url": ""},
        ]
        self.assertEqual(
            discovery.candidate_exclude_urls(rows),
            ["https://a.example/x", "https://b.example/y"],
        )


class ExcludeListPropagationTests(unittest.TestCase):
    def test_existing_candidates_are_passed_as_exclude_urls_every_round(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_fake_ctox(root, {"q1": {"sources": []}, "q2": {"sources": []}})
            existing = root / "existing.csv"
            with existing.open("w", newline="", encoding="utf-8") as handle:
                writer = csv.writer(handle)
                writer.writerow(["focus", "query", "title", "url", "doi", "openalex_id", "snippet", "review_status"])
                writer.writerow(["f", "q", "Known One", "https://known-one.example/a", "", "", "", "agent_review_required"])
                writer.writerow(["f", "q", "Known Two", "https://known-two.example/b", "", "", "", "agent_review_required"])

            result = run_discovery(
                root,
                [("f1", "q1"), ("f2", "q2")],
                ["--existing-candidates-csv", str(existing), "--snowball-limit", "0"],
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            argv_lines = (root / "argv.log").read_text(encoding="utf-8").splitlines()
            exclude_values = [
                argv_lines[index + 1]
                for index, line in enumerate(argv_lines)
                if line == "--exclude-url"
            ]
            self.assertEqual(
                exclude_values,
                [
                    "https://known-one.example/a",
                    "https://known-two.example/b",
                    "https://known-one.example/a",
                    "https://known-two.example/b",
                ],
                "every discovery round must carry the complete canonical exclude list",
            )


class TwoRoundSaturationTests(unittest.TestCase):
    def test_discovery_stops_only_after_two_consecutive_dry_rounds(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_fake_ctox(
                root,
                {
                    "q1": {"sources": [{"title": "A", "url": "https://a.example/x"}]},
                },
            )

            result = run_discovery(
                root,
                [
                    ("facet_a", "q1"),
                    ("facet_b", "q2"),
                    ("facet_a", "q3"),
                    ("facet_b", "q4"),
                    ("facet_a", "q5"),
                    ("facet_b", "q6"),
                    ("facet_a", "q7"),
                ],
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            summary = json.loads((root / "out" / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["queries_run"], 6, "q7 must never run after two complete dry rounds")
            self.assertEqual(summary["completed_query_rounds"], 3)
            self.assertTrue(summary["saturated"])
            self.assertEqual(summary["consecutive_rounds_without_new"], 2)
            self.assertEqual(summary["unique_sources"], 1)

    def test_new_candidate_resets_the_saturation_counter(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_fake_ctox(
                root,
                {
                    "q1": {"sources": [{"title": "A", "url": "https://a.example/x"}]},
                    "q3": {"sources": [{"title": "B", "url": "https://b.example/y"}]},
                },
            )

            result = run_discovery(
                root,
                [
                    ("facet_a", "q1"),
                    ("facet_b", "q2"),
                    ("facet_a", "q3"),
                    ("facet_b", "q4"),
                    ("facet_a", "q5"),
                    ("facet_b", "q6"),
                    ("facet_a", "q7"),
                    ("facet_b", "q8"),
                    ("facet_a", "q9"),
                ],
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            summary = json.loads((root / "out" / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(
                summary["queries_run"],
                8,
                "discovery continues for two complete dry rounds after the last new source (q3)",
            )
            self.assertEqual(summary["unique_sources"], 2)
            self.assertTrue(summary["saturated"])

    def test_failed_round_is_inconclusive_not_a_dry_round(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_fake_ctox(
                root,
                {
                    "q1": {"sources": [{"title": "A", "url": "https://a.example/x"}]},
                    "q3": {"sources": [], "errors": [{"backend": "google", "error": "captcha"}]},
                },
            )

            result = run_discovery(
                root,
                [
                    ("facet_a", "q1"),
                    ("facet_b", "q2"),
                    ("facet_a", "q3"),
                    ("facet_b", "q4"),
                    ("facet_a", "q5"),
                    ("facet_b", "q6"),
                    ("facet_a", "q7"),
                    ("facet_b", "q8"),
                ],
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            summary = json.loads((root / "out" / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(
                summary["queries_run"],
                8,
                "the incomplete q3/q4 round must not advance saturation; two later complete rounds are required",
            )
            self.assertEqual(summary["unique_sources"], 1)
            self.assertTrue(summary["saturated"])

    def test_previously_seen_candidates_never_count_as_new_again(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_fake_ctox(
                root,
                {
                    "q1": {"sources": [{"title": "A", "url": "https://a.example/x?utm_source=nl"}]},
                    "q2": {"sources": [{"title": "A again", "url": "https://www.a.example/x/"}]},
                },
            )

            result = run_discovery(root, [("f1", "q1"), ("f2", "q2"), ("f3", "q3")])

            self.assertEqual(result.returncode, 0, result.stderr)
            summary = json.loads((root / "out" / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(
                summary["unique_sources"],
                1,
                "a decorated re-discovery of the same canonical URL counts once",
            )


class SourceReviewDiscoveryTests(unittest.TestCase):
    def test_ctox_deep_research_is_the_default_backend(self) -> None:
        source = SCRIPT.read_text(encoding="utf-8")
        self.assertIn('default="ctox-deep-research"', source)
        self.assertNotIn('default="hybrid"', source)

    def test_business_writeback_fails_closed_without_invoking_psql(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            marker = root / "psql-invoked"
            fake_psql = root / "psql"
            fake_psql.write_text(f"#!/bin/sh\ntouch {marker}\n", encoding="utf-8")
            fake_psql.chmod(0o755)
            out_dir = root / "out"

            env = os.environ.copy()
            env["PATH"] = f"{root}{os.pathsep}{env.get('PATH', '')}"
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--topic",
                    "example topic",
                    "--out-dir",
                    str(out_dir),
                    "--allow-auto-query-plan",
                    "--business-writeback",
                    "--business-database-url",
                    "postgresql://unsafe.example/research",
                    "--business-research-run-id",
                    "run-1",
                ],
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("--business-writeback is retired and disabled", result.stderr)
            self.assertIn("never writes PostgreSQL or Business OS state", result.stderr)
            self.assertFalse(marker.exists(), "fail-closed option must not invoke psql")
            self.assertFalse(out_dir.exists(), "fail-closed option must not start discovery")

    def test_plan_only_discovery_still_writes_query_plan(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            queries = root / "queries.csv"
            queries.write_text("focus,query\nseed,example topic sources\n", encoding="utf-8")
            out_dir = root / "out"
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--topic",
                    "example topic",
                    "--out-dir",
                    str(out_dir),
                    "--queries-file",
                    str(queries),
                    "--plan-only",
                ],
                capture_output=True,
                text=True,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                (out_dir / "query_plan.csv").read_text(encoding="utf-8"),
                "focus,query\nseed,example topic sources\n",
            )


if __name__ == "__main__":
    unittest.main()
