#!/usr/bin/env python3
"""Validate static quantized metalpack payload metadata.

This is a structural gate only. It does not read or dequantize weights; it
checks that manifest fields imply the packed byte counts that the kernels will
use for address arithmetic.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path


def round_up(value: int, multiple: int) -> int:
    if multiple <= 0:
        return value
    return math.ceil(value / multiple) * multiple


def expected_row_tiled_bytes(entry: dict) -> int | None:
    shape = entry.get("source_shape")
    if not isinstance(shape, list) or len(shape) != 2:
        return None
    row_tile = int(entry.get("row_tile", 0))
    col_tile = int(entry.get("col_tile", 0))
    if row_tile <= 0 or col_tile <= 0:
        return None
    rows = int(shape[0])
    cols = int(shape[1])
    return round_up(rows, row_tile) * round_up(cols, col_tile) * 2


def expected_quant_bytes(entry: dict) -> tuple[int | None, list[str]]:
    errors: list[str] = []
    shape = entry.get("source_shape")
    if not isinstance(shape, list) or len(shape) != 2:
        return None, ["quantized entry must have 2D source_shape"]

    try:
        rows = int(shape[0])
        cols = int(shape[1])
        row_tile = int(entry.get("row_tile", 0))
        col_tile = int(entry.get("col_tile", 0))
        group = int(entry.get("quant_group_size", 0))
        bits = int(entry.get("quant_value_bits", 0))
    except (TypeError, ValueError):
        return None, ["quantized entry has non-integer tile/group/bits metadata"]

    if row_tile <= 0:
        errors.append("row_tile must be > 0")
    if col_tile <= 0:
        errors.append("col_tile must be > 0")
    if group <= 0:
        errors.append("quant_group_size must be > 0")
    if bits not in (4, 8):
        errors.append("quant_value_bits must be 4 or 8")
    if errors:
        return None, errors

    if group > col_tile:
        errors.append(f"quant_group_size {group} exceeds col_tile {col_tile}")
    if col_tile % group != 0:
        errors.append(f"quant_group_size {group} must divide col_tile {col_tile}")
    if bits == 4 and group % 2 != 0:
        errors.append(f"int4 quant_group_size {group} must be even")
    if errors:
        return None, errors

    padded_rows = round_up(rows, row_tile)
    padded_cols = round_up(cols, col_tile)
    col_tiles = padded_cols // col_tile
    groups_per_col_tile = col_tile // group
    value_bytes = group if bits == 8 else group // 2
    group_bytes = 2 + value_bytes
    return padded_rows * col_tiles * groups_per_col_tile * group_bytes, []


def validate_manifest(path: Path, strict: bool) -> tuple[int, int, list[str]]:
    manifest = json.loads(path.read_text())
    errors: list[str] = []
    entries = manifest.get("entries")
    if not isinstance(entries, list):
        return 0, 0, ["manifest entries must be a list"]

    quant_entries = 0
    checked_entries = 0
    expected_total = 0
    previous_end = 0

    for idx, entry in enumerate(entries):
        name = str(entry.get("tensor", f"<entry {idx}>"))
        layout = str(entry.get("layout", ""))
        scheme = str(entry.get("quant_scheme", "none"))
        packed_offset = int(entry.get("packed_offset", -1))
        packed_bytes = int(entry.get("packed_bytes", -1))

        if packed_offset != previous_end:
            errors.append(
                f"{name}: packed_offset {packed_offset} does not match previous end {previous_end}"
            )
        previous_end = packed_offset + packed_bytes
        expected_total += packed_bytes

        if layout == "fp16_row_tiled":
            expected = expected_row_tiled_bytes(entry)
            if expected is not None and expected != packed_bytes:
                errors.append(
                    f"{name}: fp16_row_tiled packed_bytes {packed_bytes} != expected {expected}"
                )
            checked_entries += 1
            continue

        if scheme == "none":
            if strict and layout in ("int8_row_tiled", "int4_groupwise_row_tiled"):
                errors.append(f"{name}: quantized layout has quant_scheme=none")
            continue

        quant_entries += 1
        checked_entries += 1
        expected, entry_errors = expected_quant_bytes(entry)
        for error in entry_errors:
            errors.append(f"{name}: {error}")
        if expected is not None and expected != packed_bytes:
            errors.append(
                f"{name}: quant packed_bytes {packed_bytes} != expected {expected}"
            )

        expected_layout = {
            "int8_symmetric": "int8_row_tiled",
            "int4_groupwise_symmetric": "int4_groupwise_row_tiled",
        }.get(scheme)
        if expected_layout is None:
            errors.append(f"{name}: unknown quant_scheme {scheme}")
        elif layout != expected_layout:
            errors.append(
                f"{name}: layout {layout} does not match quant_scheme {scheme}"
            )

    manifest_total = int(manifest.get("packed_bytes", -1))
    if manifest_total != expected_total:
        errors.append(
            f"manifest packed_bytes {manifest_total} != entry sum {expected_total}"
        )
    if previous_end != expected_total:
        errors.append(f"final offset {previous_end} != entry sum {expected_total}")
    if strict and quant_entries == 0:
        errors.append("strict mode requires at least one quantized entry")

    return checked_entries, quant_entries, errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("metalpack", type=Path, help="metalpack directory or manifest.json")
    parser.add_argument("--strict", action="store_true")
    args = parser.parse_args()

    manifest_path = args.metalpack
    if manifest_path.is_dir():
        manifest_path = manifest_path / "manifest.json"
    if not manifest_path.exists():
        print(f"validation: FAIL\nmissing manifest: {manifest_path}", file=sys.stderr)
        return 2

    checked, quant, errors = validate_manifest(manifest_path, args.strict)
    if errors:
        print("validation: FAIL")
        print(f"manifest: {manifest_path}")
        print(f"checked_entries: {checked}")
        print(f"quant_entries: {quant}")
        for error in errors:
            print(f"error: {error}")
        return 1

    print("validation: PASS")
    print(f"manifest: {manifest_path}")
    print(f"checked_entries: {checked}")
    print(f"quant_entries: {quant}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
