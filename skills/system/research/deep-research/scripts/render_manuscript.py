#!/usr/bin/env python3
"""Render a Manuscript v1 JSON (read from stdin) to a Word document.

Invoked by `ctox report render --format docx`. The Rust side writes the
manuscript JSON to this script's stdin and reads back a single
`renderer_version=...` line on stdout. Errors go to stderr and surface as the
subprocess error message.

The script is intentionally limited to what python-docx supports natively:
- title, subtitle, version label
- TOC field (Word will populate on first open / "Update Field")
- numbered + bulleted lists (real Word numbering, no Unicode bullets)
- tables with headers
- scope/disclaimer block
- citation register

The script does NOT generate prose. Every paragraph emitted comes from a
specific manuscript field. If the manuscript has no content for a section,
that section is skipped.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

try:
    from docx import Document
    from docx.enum.style import WD_STYLE_TYPE
    from docx.enum.table import WD_ALIGN_VERTICAL
    from docx.enum.text import WD_ALIGN_PARAGRAPH
    from docx.oxml.ns import qn
    from docx.oxml import OxmlElement
    from docx.shared import Pt, Cm, RGBColor
except ImportError as exc:
    sys.stderr.write(
        "render_manuscript.py requires python-docx. Install with:\n"
        "  python3 -m pip install python-docx\n"
        f"underlying error: {exc}\n"
    )
    sys.exit(2)


RENDERER_VERSION = "ctox-report/docx/v1"


def _ensure_styles(doc: Document) -> None:
    styles = doc.styles
    # Set default font
    normal = styles["Normal"]
    normal.font.name = "Arial"
    normal.font.size = Pt(11)
    normal.font.color.rgb = RGBColor(0x00, 0x00, 0x00)
    # Caption-like style
    if "Body Text" in styles:
        bt = styles["Body Text"]
        bt.font.name = "Arial"
    # Heading colours -> black for legibility (Word default is blue)
    for lvl in range(1, 5):
        name = f"Heading {lvl}"
        if name in styles:
            styles[name].font.color.rgb = RGBColor(0x00, 0x00, 0x00)
            styles[name].font.name = "Arial"


def _add_toc(doc: Document) -> None:
    p = doc.add_paragraph()
    p.add_run("Table of Contents").bold = True
    fld = doc.add_paragraph()
    run = fld.add_run()
    fld_char_begin = OxmlElement("w:fldChar")
    fld_char_begin.set(qn("w:fldCharType"), "begin")
    run._element.append(fld_char_begin)
    instr = OxmlElement("w:instrText")
    instr.text = 'TOC \\o "1-3" \\h \\z \\u'
    run._element.append(instr)
    fld_char_sep = OxmlElement("w:fldChar")
    fld_char_sep.set(qn("w:fldCharType"), "separate")
    run._element.append(fld_char_sep)
    placeholder = OxmlElement("w:t")
    placeholder.text = (
        "Right-click and choose 'Update Field' to populate the table of contents."
    )
    run._element.append(placeholder)
    fld_char_end = OxmlElement("w:fldChar")
    fld_char_end.set(qn("w:fldCharType"), "end")
    run._element.append(fld_char_end)


def _add_title_block(doc: Document, manuscript: dict) -> None:
    title = manuscript.get("title", "")
    subtitle = manuscript.get("subtitle")
    version_label = manuscript.get("version_label", "")
    if title:
        p = doc.add_paragraph()
        p.alignment = WD_ALIGN_PARAGRAPH.LEFT
        run = p.add_run(title)
        run.bold = True
        run.font.size = Pt(28)
    if subtitle:
        p = doc.add_paragraph(subtitle)
        for r in p.runs:
            r.font.size = Pt(16)
    if version_label:
        p = doc.add_paragraph(version_label)
        for r in p.runs:
            r.italic = True
            r.font.size = Pt(10)


def _add_scope(doc: Document, manuscript: dict) -> None:
    scope = manuscript.get("scope") or {}
    disclaimer = scope.get("disclaimer_md", "").strip()
    if disclaimer:
        p = doc.add_paragraph()
        p.paragraph_format.left_indent = Cm(0.5)
        run = p.add_run("Scope and Disclaimer")
        run.bold = True
        para = doc.add_paragraph(disclaimer)
        para.paragraph_format.left_indent = Cm(0.5)
    questions = scope.get("leading_questions") or []
    if questions:
        doc.add_paragraph("Leading questions").runs[0].bold = True
        for i, q in enumerate(questions, 1):
            doc.add_paragraph(f"{i}. {q}")
    out_of_scope = scope.get("out_of_scope") or []
    if out_of_scope:
        doc.add_paragraph("Out of scope").runs[0].bold = True
        for s in out_of_scope:
            doc.add_paragraph(s, style="List Bullet")
    assumptions = scope.get("assumptions") or []
    if assumptions:
        doc.add_paragraph("Assumptions").runs[0].bold = True
        for s in assumptions:
            doc.add_paragraph(s, style="List Bullet")


def _cite_suffix(evidence_ids: list, cite_index: dict) -> str:
    nums = [str(cite_index[e]) for e in evidence_ids if e in cite_index]
    if not nums:
        return ""
    return f" [{','.join(nums)}]"


def _add_section(doc: Document, section: dict, cite_index: dict, register: list) -> None:
    heading = section.get("heading", "").strip()
    level = max(1, min(int(section.get("heading_level") or 1), 4))
    if heading:
        doc.add_heading(heading, level=level)
    for block in section.get("blocks") or []:
        _add_block(doc, block, cite_index, register)


def _add_block(doc: Document, block: dict, cite_index: dict, register: list) -> None:
    kind = block.get("kind")
    if kind == "paragraph":
        text = (block.get("text_md") or "").strip()
        if not text:
            return
        suffix = _cite_suffix(block.get("evidence_ids") or [], cite_index)
        doc.add_paragraph(text + suffix)
    elif kind == "bullets":
        for item in block.get("items") or []:
            _add_bullet_item(doc, item, "List Bullet", cite_index)
    elif kind == "numbered":
        for item in block.get("items") or []:
            _add_bullet_item(doc, item, "List Number", cite_index)
    elif kind == "options_table":
        _add_options_table(doc, block.get("options") or [])
    elif kind == "requirements_table":
        _add_requirements_table(doc, block.get("rows") or [])
    elif kind == "matrix_table":
        _add_matrix_table(doc, block, cite_index)
    elif kind == "scenario_block":
        code = block.get("code") or ""
        label = block.get("label") or ""
        desc = block.get("description_md") or ""
        p = doc.add_paragraph()
        run = p.add_run(f"Scenario {code}: {label}")
        run.bold = True
        if desc:
            doc.add_paragraph(desc)
    elif kind == "risk_register":
        _add_risk_register(doc, block.get("rows") or [])
    elif kind == "citation_register":
        _add_citation_register(doc, register)
    elif kind == "note":
        text = (block.get("text_md") or "").strip()
        if text:
            doc.add_paragraph(text)


def _add_bullet_item(doc: Document, item: dict, style: str, cite_index: dict) -> None:
    text = (item.get("text_md") or "").strip()
    if not text:
        return
    suffix = _cite_suffix(item.get("evidence_ids") or [], cite_index)
    if item.get("primary_recommendation"):
        suffix += "  (primary)"
    if item.get("scenario_code"):
        suffix += f"  [scenario {item['scenario_code']}]"
    doc.add_paragraph(text + suffix, style=style)
    asm = (item.get("assumption_note_md") or "").strip()
    if asm:
        p = doc.add_paragraph(f"assumption: {asm}", style=style)
        for r in p.runs:
            r.italic = True


def _add_options_table(doc: Document, options: list) -> None:
    if not options:
        return
    table = doc.add_table(rows=1, cols=3)
    table.style = "Light Grid Accent 1"
    hdr = table.rows[0].cells
    hdr[0].text = "Code"
    hdr[1].text = "Option"
    hdr[2].text = "Summary"
    for opt in options:
        row = table.add_row().cells
        row[0].text = opt.get("code", "")
        row[1].text = opt.get("label", "")
        row[2].text = opt.get("summary_md") or ""


def _add_requirements_table(doc: Document, rows: list) -> None:
    if not rows:
        return
    table = doc.add_table(rows=1, cols=4)
    table.style = "Light Grid Accent 1"
    hdr = table.rows[0].cells
    hdr[0].text = "Code"
    hdr[1].text = "Title"
    hdr[2].text = "Must-have"
    hdr[3].text = "Description"
    for r in rows:
        c = table.add_row().cells
        c[0].text = r.get("code", "")
        c[1].text = r.get("title", "")
        c[2].text = "yes" if r.get("must_have") else "no"
        c[3].text = r.get("description_md") or ""


def _add_matrix_table(doc: Document, block: dict, cite_index: dict) -> None:
    axes = block.get("axes") or []
    rows = block.get("rows") or []
    label = block.get("label") or ""
    if label:
        p = doc.add_paragraph()
        run = p.add_run(label)
        run.bold = True
    if not axes or not rows:
        return
    table = doc.add_table(rows=1, cols=1 + len(axes))
    table.style = "Light Grid Accent 1"
    hdr = table.rows[0].cells
    hdr[0].text = "Option"
    for i, axis in enumerate(axes):
        hdr[i + 1].text = axis.get("label", axis.get("code", ""))
    for row in rows:
        opt_label = row.get("option_label", "")
        opt_code = row.get("option_code", "")
        cells_in_row = row.get("cells") or []
        cells_by_axis = {c.get("axis_code"): c for c in cells_in_row}
        out_row = table.add_row().cells
        out_row[0].text = f"{opt_label} ({opt_code})"
        for i, axis in enumerate(axes):
            cell = cells_by_axis.get(axis.get("code"))
            if cell:
                value = cell.get("value_label", "")
                cite = _cite_suffix(cell.get("evidence_ids") or [], cite_index)
                out_row[i + 1].text = f"{value}{cite}"
            else:
                out_row[i + 1].text = "-"
    # Rationale list under the table.
    for row in rows:
        for c in row.get("cells") or []:
            rationale = (c.get("rationale_md") or "").strip()
            if rationale:
                opt_code = row.get("option_code", "")
                axis_code = c.get("axis_code", "")
                cite = _cite_suffix(c.get("evidence_ids") or [], cite_index)
                p = doc.add_paragraph(style="List Bullet")
                run = p.add_run(f"{opt_code} / {axis_code}: ")
                run.italic = True
                p.add_run(rationale + cite)


def _add_risk_register(doc: Document, rows: list) -> None:
    if not rows:
        return
    table = doc.add_table(rows=1, cols=6)
    table.style = "Light Grid Accent 1"
    hdr = table.rows[0].cells
    hdr[0].text = "Code"
    hdr[1].text = "Risk"
    hdr[2].text = "Likelihood"
    hdr[3].text = "Impact"
    hdr[4].text = "Description"
    hdr[5].text = "Mitigation"
    for r in rows:
        c = table.add_row().cells
        c[0].text = r.get("code", "")
        c[1].text = r.get("title", "")
        c[2].text = r.get("likelihood") or "-"
        c[3].text = r.get("impact") or "-"
        c[4].text = r.get("description_md") or ""
        c[5].text = r.get("mitigation_md") or ""


def _add_citation_register(doc: Document, register: list) -> None:
    for c in register:
        idx = c.get("display_index")
        authors = "; ".join(c.get("authors") or []) or "-"
        title = c.get("title") or "(untitled)"
        venue = c.get("venue") or ""
        year = f" ({c.get('year')})" if c.get("year") else ""
        kind = c.get("citation_kind", "")
        canonical = c.get("canonical_id", "")
        url = c.get("full_text_url") or c.get("landing_url") or ""
        doc.add_paragraph(
            f"[{idx}] {authors}. {title}. {venue}{year}. {kind} {canonical} {url}",
            style="List Number",
        )


def _build_cite_index(register: list) -> dict:
    return {c["evidence_id"]: c["display_index"] for c in register if "evidence_id" in c}


def main() -> int:
    parser = argparse.ArgumentParser(description="Render Manuscript v1 to a Word document.")
    parser.add_argument("--format", required=True, choices=["docx"])
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="Render a hardcoded minimal manuscript and assert it can be re-parsed.",
    )
    args = parser.parse_args()

    if args.self_test:
        manuscript = {
            "schema": "ctox.report.manuscript/v1",
            "run_id": "run_self_test",
            "preset": "feasibility",
            "language": "en",
            "title": "Self-test",
            "subtitle": "Self-test subtitle",
            "version_label": "v0",
            "scope": {
                "leading_questions": ["Q?"],
                "out_of_scope": [],
                "assumptions": [],
                "disclaimer_md": "Self-test disclaimer.",
                "success_criteria": [],
            },
            "sections": [],
            "citation_register": [],
        }
    else:
        raw = sys.stdin.read()
        manuscript = json.loads(raw)
    if manuscript.get("schema") != "ctox.report.manuscript/v1":
        sys.stderr.write(
            f"unsupported manuscript schema: {manuscript.get('schema')!r}\n"
        )
        return 3

    doc = Document()
    _ensure_styles(doc)
    _add_title_block(doc, manuscript)
    _add_toc(doc)
    _add_scope(doc, manuscript)
    register = manuscript.get("citation_register") or []
    cite_index = _build_cite_index(register)
    for section in manuscript.get("sections") or []:
        _add_section(doc, section, cite_index, register)

    args.out.parent.mkdir(parents=True, exist_ok=True)
    doc.save(args.out)
    print(f"renderer_version={RENDERER_VERSION}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
