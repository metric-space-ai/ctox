#!/usr/bin/env python3
import importlib.util
import sqlite3
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("run.py")
SPEC = importlib.util.spec_from_file_location("ctox_web_bench_run", MODULE_PATH)
assert SPEC and SPEC.loader
RUN = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUN)


class SqliteIdentifierTests(unittest.TestCase):
    def test_identifier_payloads_remain_identifiers(self) -> None:
        conn = sqlite3.connect(":memory:")
        conn.execute("CREATE TABLE sentinel (value TEXT)")
        table = 'records"; DROP TABLE sentinel; --'
        column = 'value") VALUES ("injected"); DROP TABLE sentinel; --'
        conn.execute(
            f"CREATE TABLE {RUN.sqlite_quote_identifier(table)} "
            f"({RUN.sqlite_quote_identifier(column)} TEXT)"
        )
        conn.execute(
            f"INSERT INTO {RUN.sqlite_quote_identifier(table)} "
            f"({RUN.sqlite_quote_identifier(column)}) VALUES (?)",
            ("safe",),
        )
        self.assertEqual(
            conn.execute(
                f"SELECT {RUN.sqlite_quote_identifier(column)} "
                f"FROM {RUN.sqlite_quote_identifier(table)}"
            ).fetchone(),
            ("safe",),
        )
        self.assertIsNotNone(
            conn.execute(
                "SELECT name FROM sqlite_master WHERE name = 'sentinel'"
            ).fetchone()
        )


if __name__ == "__main__":
    unittest.main()
