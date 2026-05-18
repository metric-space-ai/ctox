
#!/usr/bin/env python3
import json
import math
import re
from copy import deepcopy
from pathlib import Path

ROOT = Path(__file__).resolve().parent
FIXTURES = ROOT / "fixtures"

NUMERICISH_RE = re.compile(r"^(?:[*+-]?[\d,.\s]+[%]?$|[*]?-?[\d,.\s]+$|[ZN]/A$|[Z-]$)")
NUMERIC_PATTERN_RE = re.compile(r"^[$]?-?[\d,]+\.?\d*%?$")
MARGIN_LINE_NUMBER_RE = re.compile(r"^\d{1,2}[O]?$")

def clean_ocr_table_artifacts(text: str) -> str:
    trimmed = text.strip()
    if not trimmed:
        return ""
    stripped = trimmed.lstrip("|[](){}").rstrip("|[](){}").strip()
    if NUMERICISH_RE.match(stripped):
        return stripped
    return trimmed

SUBSCRIPT = str.maketrans({
    "0": "₀","1": "₁","2": "₂","3": "₃","4": "₄","5": "₅","6": "₆","7": "₇","8": "₈","9": "₉",
    "+": "₊","-": "₋","=": "₌","(": "₍",")": "₎","a": "ₐ","e": "ₑ","h": "ₕ","i": "ᵢ","j": "ⱼ",
    "k": "ₖ","l": "ₗ","m": "ₘ","n": "ₙ","o": "ₒ","p": "ₚ","r": "ᵣ","s": "ₛ","t": "ₜ","u": "ᵤ",
    "v": "ᵥ","x": "ₓ",
})
SUPERSCRIPT = str.maketrans({
    "0": "⁰","1": "¹","2": "²","3": "³","4": "⁴","5": "⁵","6": "⁶","7": "⁷","8": "⁸","9": "⁹",
    "+": "⁺","-": "⁻","=": "⁼","(": "⁽",")": "⁾","a": "ᵃ","b": "ᵇ","c": "ᶜ","d": "ᵈ","e": "ᵉ",
    "f": "ᶠ","g": "ᵍ","h": "ʰ","i": "ⁱ","j": "ʲ","k": "ᵏ","l": "ˡ","m": "ᵐ","n": "ⁿ","o": "ᵒ",
    "p": "ᵖ","r": "ʳ","s": "ˢ","t": "ᵗ","u": "ᵘ","v": "ᵛ","w": "ʷ","x": "ˣ","y": "ʸ","z": "ᶻ",
})

def to_subscript_string(text: str) -> str:
    return text.translate(SUBSCRIPT)

def to_superscript_string(text: str) -> str:
    return text.translate(SUPERSCRIPT)

def merge_page_bbox(a, b):
    if a and b:
        x = min(a["x"], b["x"])
        y = min(a["y"], b["y"])
        x2 = max(a["x"] + a["w"], b["x"] + b["w"])
        y2 = max(a["y"] + a["h"], b["y"] + b["h"])
        return {"x": x, "y": y, "w": x2 - x, "h": y2 - y}
    return a or b

def effective_page_bbox(box):
    return box.get("pageBbox") or {"x": box["x"], "y": box["y"], "w": box["w"], "h": box["h"]}

def can_merge_markup(a, b):
    return a == b

def approx_eq(a, b, tolerance):
    return abs(a - b) <= tolerance

def handle_rotation_reading_order(boxes):
    groups = {}
    for box in boxes:
        groups.setdefault(box.get("r", 0) or 0, []).append(deepcopy(box))
    out = []
    for rotation in sorted(groups):
        group = sorted(groups[rotation], key=lambda b: b["x"])
        rot = rotation % 360
        if rot == 90:
            for bbox in group:
                old_x = bbox["x"]
                bbox["x"] = round(bbox["y"])
                bbox["y"] = old_x
                bbox["w"], bbox["h"] = bbox["h"], bbox["w"]
                bbox["r"] = 0
                bbox["rotated"] = True
                out.append(bbox)
        elif rot == 180:
            for bbox in group:
                bbox["x"] = round(bbox.get("ry", bbox["y"]))
                bbox["y"] = bbox.get("rx", bbox["x"])
                bbox["r"] = 0
                bbox["rotated"] = True
                out.append(bbox)
        elif rot == 270:
            max_y = max(b["y"] + b["h"] for b in group)
            for bbox in group:
                old_x = bbox["x"]
                bbox["x"] = round(max_y - bbox["y"] - bbox["h"])
                bbox["y"] = old_x
                bbox["w"], bbox["h"] = bbox["h"], bbox["w"]
                bbox["r"] = 0
                bbox["rotated"] = True
                out.append(bbox)
        else:
            out.extend(group)
    return sorted(out, key=lambda b: (b["y"], b["x"]))

def bbox_to_lines(text_bbox, median_width, median_height, page_width=None):
    if not text_bbox:
        return []
    y_sort_tolerance = max(median_height * 0.5, 5.0)
    sorted_boxes = deepcopy(text_bbox)

    if page_width is not None:
        midpoint = page_width / 2.0
        left_zone = midpoint - 25.0
        right_zone = midpoint + 25.0
        for bbox in sorted_boxes:
            trimmed = bbox["str"].strip()
            if MARGIN_LINE_NUMBER_RE.match(trimmed) and bbox["w"] < 15 and left_zone <= bbox["x"] <= right_zone:
                bbox["isMarginLineNumber"] = True

    sorted_boxes.sort(key=lambda b: (round(b["y"] / y_sort_tolerance), b["y"], b["x"]))

    merged = []
    for bbox in sorted_boxes:
        if merged:
            prev = merged[-1]
            x_delta = bbox["x"] - prev["x"] - prev["w"]
            same_y = approx_eq(prev["y"], bbox["y"], y_sort_tolerance)
            same_h = approx_eq(prev["h"], bbox["h"], max(median_height, 2.0))
            can_merge = same_y and same_h and ((-0.5 < x_delta < 0) or (0 <= x_delta < 0.1)) and can_merge_markup(prev.get("markup"), bbox.get("markup"))
            if can_merge:
                prev["str"] += bbox["str"]
                prev["w"] = (bbox["x"] + bbox["w"]) - prev["x"]
                prev["h"] = max(prev["h"], bbox["h"])
                prev["strLength"] += bbox["strLength"]
                prev["pageBbox"] = merge_page_bbox(effective_page_bbox(prev), effective_page_bbox(bbox))
                continue
        merged.append(bbox)

    x_overlap_tolerance = max(median_width / 3.0, 5.0)
    lines = []

    for bbox in merged:
        placed = False
        for line in lines:
            margin_mismatch = bool(line[0].get("isMarginLineNumber", False)) != bool(bbox.get("isMarginLineNumber", False))
            y_tolerance = max(median_height * 2.0, 20.0) if bbox.get("rotated") else y_sort_tolerance
            line_min_y = min(b["y"] for b in line)
            line_max_y = max(b["y"] + b["h"] for b in line)
            line_collides = any(max(0.0, min(existing["x"] + existing["w"], bbox["x"] + bbox["w"]) - max(existing["x"], bbox["x"])) > x_overlap_tolerance for existing in line)
            bbox_center_y = bbox["y"] + bbox["h"] / 2.0
            y_close = any(approx_eq(existing["y"], bbox["y"], y_tolerance) for existing in line) or (line_min_y <= bbox_center_y <= line_max_y) or (line_min_y <= bbox["y"] <= line_max_y)
            if not line_collides and not margin_mismatch and y_close:
                line.append(bbox)
                placed = True
                break
        if not placed:
            lines.append([bbox])

    for line in lines:
        line.sort(key=lambda b: b["x"])
    lines.sort(key=lambda line: line[0]["y"] if line else 0)

    for idx, line in enumerate(lines):
        compact = []
        for current in line:
            if compact:
                previous = compact[-1]
                markup_ok = can_merge_markup(previous.get("markup"), current.get("markup"))
                gap = current["x"] - previous["x"] - previous["w"]
                prev_trim = previous["str"].strip()
                curr_trim = current["str"].strip()
                both_numbers = len(prev_trim) >= 2 and NUMERIC_PATTERN_RE.match(prev_trim) and len(curr_trim) >= 2 and NUMERIC_PATTERN_RE.match(curr_trim)

                if markup_ok and not both_numbers and gap <= 1.0:
                    if current["h"] != 0 and current["h"] < previous["h"] * 0.7:
                        if current["str"].startswith(" "):
                            compact.append(current)
                            continue
                        if current["y"] > previous["y"] + previous["h"] * 0.2:
                            previous["str"] += to_subscript_string(current["str"])
                        else:
                            previous["str"] += to_superscript_string(current["str"])
                    else:
                        previous["str"] += current["str"]
                    previous["w"] = (current["x"] + current["w"]) - previous["x"]
                    previous["h"] = max(previous["h"], current["h"])
                    previous["strLength"] += current["strLength"]
                    previous["pageBbox"] = merge_page_bbox(effective_page_bbox(previous), effective_page_bbox(current))
                    continue

                adaptive_gap = previous["w"] / previous["strLength"] if previous["strLength"] else 0.0
                if markup_ok and not both_numbers and gap < adaptive_gap:
                    if not previous["str"].endswith(" ") and not current["str"].startswith(" "):
                        previous["str"] += " "
                    previous["str"] += current["str"].lstrip()
                    previous["w"] = (current["x"] + current["w"]) - previous["x"]
                    previous["h"] = max(previous["h"], current["h"])
                    previous["strLength"] = len(previous["str"])
                    previous["pageBbox"] = merge_page_bbox(effective_page_bbox(previous), effective_page_bbox(current))
                    continue
            compact.append(current)
        lines[idx] = compact

    i = 1
    while i + 1 < len(lines):
        prev = lines[i-1]
        curr = lines[i]
        if prev and curr:
            prev_min_y = min(b["y"] for b in prev)
            prev_max_y = max(b["y"] + b["h"] for b in prev)
            curr_min_y = min(b["y"] for b in curr)
            curr_max_y = max(b["y"] + b["h"] for b in curr)
            overlaps_y = curr_min_y <= prev_max_y and curr_max_y >= prev_min_y
            overlaps_x = any(max(0.0, min(l["x"] + l["w"], r["x"] + r["w"]) - max(l["x"], r["x"])) > 0 for l in prev for r in curr)
            if overlaps_y and not overlaps_x:
                prev.extend(curr)
                prev.sort(key=lambda b: b["x"])
                del lines[i]
                continue
        i += 1

    with_blanks = []
    for idx, line in enumerate(lines):
        if idx > 0 and with_blanks and with_blanks[-1] and line:
            prev_first = with_blanks[-1][0]
            curr_first = line[0]
            y_delta = curr_first["y"] - prev_first["y"] - prev_first["h"]
            if y_delta > median_height:
                blanks = max(1, min(10, round(y_delta / median_height) - 1))
                with_blanks.extend([[] for _ in range(blanks)])
        with_blanks.append(line)
    return with_blanks

def filter_dot_garbage(boxes):
    dot_count = sum(1 for b in boxes if b["str"].strip(".") == "" and "." in b["str"])
    if dot_count >= 100:
        return [b for b in boxes if not (b["str"].strip(".") == "" and "." in b["str"])]
    return boxes

def infer_left_anchors(boxes, existing):
    if existing:
        return dict(existing)
    xs = sorted({str(int(round(b["x"]))) for b in boxes})
    out = {}
    for idx, x in enumerate(xs):
        out[x] = 1 if idx == 0 else 1 + idx * 10
    return out

def render_lines_minimal(lines):
    rendered = []
    for line in lines:
        if not line:
            rendered.append("")
            continue
        parts = [b["str"].strip() for b in line if b["str"].strip()]
        rendered.append("" if not parts else " " + " ".join(parts))
    return "\n".join(rendered)

def project_to_grid(page, projection_boxes, prev_anchors, total_pages):
    projection_boxes = filter_dot_garbage(deepcopy(projection_boxes))
    projection_boxes = handle_rotation_reading_order(projection_boxes)
    widths = sorted(b["w"] for b in projection_boxes if b["w"] > 0)
    heights = sorted(b["h"] for b in projection_boxes if b["h"] > 0)
    median_width = widths[len(widths)//2] if widths else 10.0
    median_height = heights[len(heights)//2] if heights else 12.0
    lines = bbox_to_lines(projection_boxes, median_width, median_height, page.get("width"))
    return {
        "text": render_lines_minimal(lines),
        "prevAnchors": {
            "forwardAnchorLeft": infer_left_anchors(projection_boxes, prev_anchors["forwardAnchorLeft"]),
            "forwardAnchorRight": dict(prev_anchors["forwardAnchorRight"]),
            "forwardAnchorCenter": dict(prev_anchors["forwardAnchorCenter"]),
        },
    }

def build_bounding_boxes(text_items):
    out = []
    for item in text_items:
        if item["str"].strip():
            w = item.get("w", item.get("width", 0))
            h = item.get("h", item.get("height", 0))
            out.append({
                "x1": item["x"],
                "y1": item["y"],
                "x2": item["x"] + w,
                "y2": item["y"] + h,
            })
    return out

def detect_and_remove_margin_on_page(page):
    lines = page["text"].splitlines()
    if not lines:
        page["text"] = ""
        return
    min_x = None
    min_y = None
    max_y = None
    for idx, line in enumerate(lines):
        positions = [i for i, ch in enumerate(line) if not ch.isspace()]
        if positions:
            pos = positions[0]
            min_x = pos if min_x is None else min(min_x, pos)
            min_y = idx if min_y is None else min(min_y, idx)
            max_y = idx if max_y is None else max(max_y, idx)
    if min_x is None or min_y is None or max_y is None:
        page["text"] = ""
        return
    kept = lines[min_y:max_y+1]
    normalized = [(line[min_x:] if len(line) > min_x else "").rstrip() for line in kept]
    page["text"] = "\n".join(normalized)

def clean_raw_text(pages):
    for page in pages:
        detect_and_remove_margin_on_page(page)
        page["text"] = page["text"].replace("\x00", " ")

def normalize_numbers(obj):
    if isinstance(obj, float):
        if abs(obj - round(obj)) < 1e-9:
            return int(round(obj))
        return round(obj, 6)
    if isinstance(obj, list):
        return [normalize_numbers(x) for x in obj]
    if isinstance(obj, dict):
        return {k: normalize_numbers(v) for k, v in obj.items()}
    return obj

def prune_output_bbox(lines):
    # reduce output to the fields used by fixtures
    out = []
    for line in lines:
        row = []
        for box in line:
            item = {k: box[k] for k in list(box.keys()) if k in {"str","x","y","w","h","strLength","markup","pageBbox"}}
            row.append(normalize_numbers(item))
        out.append(row)
    return out

def main():
    results = []
    passed = 0
    failed = 0

    # bbox fixtures
    data = json.loads((FIXTURES / "bbox_to_line.json").read_text())
    for case in data["cases"]:
        actual = prune_output_bbox(
            bbox_to_lines(case["input"], case["medianWidth"], case["medianHeight"], case.get("pageWidth"))
        )
        expected = normalize_numbers(case["expected"])
        ok = actual == expected
        results.append(("bbox_to_line", case["name"], ok, actual, expected))
        passed += int(ok)
        failed += int(not ok)

    # project fixtures
    data = json.loads((FIXTURES / "project_to_grid.json").read_text())
    for case in data["cases"]:
        actual = normalize_numbers(project_to_grid(case["page"], case["projectionBoxes"], case["prevAnchors"], case["totalPages"]))
        expected = normalize_numbers(case["expected"])
        ok = actual == expected
        results.append(("project_to_grid", case["name"], ok, actual, expected))
        passed += int(ok)
        failed += int(not ok)

    # bbox helper
    data = json.loads((FIXTURES / "build_bounding_boxes.json").read_text())
    for case in data["cases"]:
        actual = normalize_numbers(build_bounding_boxes(case["input"]))
        expected = normalize_numbers(case["expected"])
        ok = actual == expected
        results.append(("build_bounding_boxes", case["name"], ok, actual, expected))
        passed += int(ok)
        failed += int(not ok)

    # clean text
    data = json.loads((FIXTURES / "clean_text.json").read_text())
    for case in data["cases"]:
        actual = deepcopy(case["input"])
        clean_raw_text(actual)
        actual = normalize_numbers(actual)
        expected = normalize_numbers(case["expected"])
        ok = actual == expected
        results.append(("clean_text", case["name"], ok, actual, expected))
        passed += int(ok)
        failed += int(not ok)

    # text utils
    data = json.loads((FIXTURES / "text_utils.json").read_text())
    for case in data["cases"]:
        actual = clean_ocr_table_artifacts(case["input"])
        expected = case["expected"]
        ok = actual == expected
        results.append(("text_utils", case["name"], ok, actual, expected))
        passed += int(ok)
        failed += int(not ok)

    lines = []
    for suite, name, ok, actual, expected in results:
        prefix = "PASS" if ok else "FAIL"
        lines.append(f"{prefix} {suite}::{name}")
        if not ok:
            lines.append(f"  actual:   {actual}")
            lines.append(f"  expected: {expected}")

    lines.append("")
    lines.append(f"Summary: {passed} passed, {failed} failed")

    report = "\n".join(lines)
    print(report)
    (ROOT / "PARITY_RESULTS.txt").write_text(report)

    return 0 if failed == 0 else 1

if __name__ == "__main__":
    raise SystemExit(main())
