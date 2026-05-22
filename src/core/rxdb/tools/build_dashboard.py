#!/usr/bin/env python3
"""Regenerate dashboard.html from PORTING.md.

Single source of truth is PORTING.md. Subagents edit that file; this script
re-renders the dashboard from it.

Run: python3 src/core/rxdb/tools/build_dashboard.py
"""
import os
import re
import sys
import datetime
import html

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
PORTING = os.path.join(ROOT, "PORTING.md")
OUT = os.path.join(ROOT, "dashboard.html")


def parse_porting(text):
    """Return {section_name: [row_dict, ...]} for every tracked table section."""
    sections = {}
    cur_section = None
    cur_rows = []
    cur_headers = []
    in_table = False
    for raw in text.splitlines():
        line = raw.rstrip()
        m = re.match(r"^##\s+(phase-\d|skip|unassigned)\s*$", line)
        if m:
            if cur_section and cur_rows:
                sections[cur_section] = cur_rows
            cur_section = m.group(1)
            cur_rows = []
            cur_headers = []
            in_table = False
            continue
        if line.startswith("## New code"):
            if cur_section and cur_rows:
                sections[cur_section] = cur_rows
            cur_section = "new-code"
            cur_rows = []
            cur_headers = []
            in_table = False
            continue
        if line.startswith("## CTOX Integration Layer"):
            if cur_section and cur_rows:
                sections[cur_section] = cur_rows
            cur_section = "integration"
            cur_rows = []
            cur_headers = []
            in_table = False
            continue
        if line.startswith("## "):
            if cur_section and cur_rows:
                sections[cur_section] = cur_rows
            cur_section = None
            cur_rows = []
            in_table = False
            continue
        if cur_section is None:
            continue
        if line.startswith("|") and "|---" in line.replace(" ", ""):
            in_table = True
            continue
        if line.startswith("|") and not cur_headers:
            cur_headers = [c.strip() for c in line.strip("|").split("|")]
            continue
        if in_table and line.startswith("|"):
            cells = [c.strip() for c in line.strip("|").split("|")]
            if len(cells) < len(cur_headers):
                continue
            row = dict(zip(cur_headers, cells))
            cur_rows.append(row)
        elif in_table and not line.startswith("|"):
            in_table = False
    if cur_section and cur_rows:
        sections[cur_section] = cur_rows
    return sections


def strip_backticks(s):
    return re.sub(r"`([^`]*)`", r"\1", s)


def clean_row(row):
    out = {}
    for k, v in row.items():
        out[k] = strip_backticks(v).strip()
    return out


def render(sections):
    """Build the HTML."""
    # Aggregate
    to_port_phases = [s for s in sections if s.startswith("phase-")]
    all_rows = []
    for ph in to_port_phases:
        for r in sections[ph]:
            r = clean_row(r)
            r["_phase"] = ph
            all_rows.append(r)

    addition_rows = []
    for r in sections.get("new-code", []):
        r = clean_row(r)
        # Context rows such as N-OOC are documented but not part of the crate
        # completion metric.
        if r.get("Status") not in ("pending", "claimed", "wip", "done"):
            continue
        r["_phase"] = "new-code"
        r["Upstream"] = r.get("Item", "")
        r["Rust target"] = r.get("Where", "")
        r["Bytes"] = "—"
        addition_rows.append(r)

    skip_rows = sections.get("skip", [])
    integration_rows = []
    for r in sections.get("integration", []):
        r = clean_row(r)
        r["_phase"] = "integration"
        r["Upstream"] = r.get("Area", "")
        r["Rust target"] = r.get("Files", "")
        r["Tier"] = "—"
        r["Owner"] = "main"
        integration_rows.append(r)

    by_status = {"pending": 0, "claimed": 0, "wip": 0, "done": 0, "skipped": 0}
    by_tier = {"T1": {"pending": 0, "claimed": 0, "wip": 0, "done": 0},
               "T2": {"pending": 0, "claimed": 0, "wip": 0, "done": 0},
               "T3": {"pending": 0, "claimed": 0, "wip": 0, "done": 0}}
    by_phase = {}
    in_progress = []
    scope_rows = all_rows + addition_rows
    for r in scope_rows:
        st = r.get("Status", "pending")
        tier = r.get("Tier", "—")
        ph = r["_phase"]
        if st in by_status:
            by_status[st] += 1
        if tier in by_tier and st in by_tier[tier]:
            by_tier[tier][st] += 1
        by_phase.setdefault(ph, {"total": 0, "done": 0, "claimed": 0, "wip": 0, "pending": 0})
        by_phase[ph]["total"] += 1
        if st in by_phase[ph]:
            by_phase[ph][st] += 1
        if st in ("claimed", "wip"):
            in_progress.append(r)

    total_to_port = len(scope_rows)
    total_done = by_status["done"]
    pct = (total_done / total_to_port * 100.0) if total_to_port else 0.0
    upstream_done = sum(1 for r in all_rows if r.get("Status") == "done")
    upstream_total = len(all_rows)
    additions_done = sum(1 for r in addition_rows if r.get("Status") == "done")
    additions_total = len(addition_rows)
    activity_rows = []
    latest_wave = None
    for r in scope_rows:
        notes = r.get("Notes", "")
        waves = [int(m.group(1)) for m in re.finditer(r"\bwave-(\d+)\b", notes)]
        if not waves:
            continue
        r = dict(r)
        r["_latest_wave"] = max(waves)
        r["_wave_count"] = len(waves)
        latest_wave = max(latest_wave or 0, r["_latest_wave"])
        activity_rows.append(r)
    for r in integration_rows:
        try:
            wave = int(r.get("Wave", "0"))
        except ValueError:
            continue
        if wave <= 0:
            continue
        r = dict(r)
        r["_latest_wave"] = wave
        r["_wave_count"] = 1
        latest_wave = max(latest_wave or 0, wave)
        activity_rows.append(r)
    activity_rows.sort(
        key=lambda r: (
            -r["_latest_wave"],
            r.get("Status", ""),
            r.get("Upstream", r.get("Item", "")),
        )
    )

    def row_activity_score(r):
        """A secondary progress signal for long-running T1 rows.

        File-row completion remains the canonical scope metric. This score
        gives partial credit to active WIP rows so repeated porting waves move
        the dashboard instead of staying visually stuck until an entire source
        file flips to `done`.
        """
        status = r.get("Status", "pending")
        if status == "done":
            return 1.0
        if status == "claimed":
            return 0.10
        if status == "wip":
            wave_count = len(re.findall(r"\bwave-\d+\b", r.get("Notes", "")))
            return min(0.95, 0.25 + (0.07 * wave_count))
        return 0.0

    activity_score = sum(row_activity_score(r) for r in scope_rows)

    now = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")

    css = """
*{box-sizing:border-box}
html,body{margin:0;padding:0}
body{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",ui-sans-serif,sans-serif;color:#111;background:#fafaf8;line-height:1.45;font-size:14px}
.wrap{max-width:1180px;margin:0 auto;padding:24px}
header{border-bottom:1px solid #ddd;padding-bottom:12px;margin-bottom:20px}
header h1{margin:0 0 4px 0;font-size:20px;font-weight:600}
header .meta{color:#666;font-size:12px}
section{margin-bottom:28px}
section h2{margin:0 0 10px 0;font-size:15px;font-weight:600;color:#333;border-bottom:1px solid #e3e3e0;padding-bottom:4px}
section h3{margin:14px 0 8px 0;font-size:13px;font-weight:600;color:#444}
.mono{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:12.5px}
.progress{position:relative;width:100%;background:#e8e8e3;height:14px;border-radius:3px;overflow:hidden;margin:4px 0}
.progress-bar{background:#3d7c47;height:100%;transition:width 0.2s}
.progress-bar.t1{background:#7a5b9a}
.progress-bar.t2{background:#5b7a9a}
.progress-bar.t3{background:#3d7c47}
.progress-label{font-size:12px;color:#555;margin-top:2px}
.stat-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(240px,1fr));gap:14px}
.stat{background:#fff;border:1px solid #ddd;border-radius:4px;padding:12px 14px}
.stat .num{font-size:22px;font-weight:600;color:#222}
.stat .lbl{font-size:11px;color:#777;text-transform:uppercase;letter-spacing:0.04em;margin-bottom:4px}
table{width:100%;border-collapse:collapse;background:#fff;border:1px solid #ddd;font-size:12.5px}
th,td{text-align:left;padding:6px 9px;border-bottom:1px solid #ececec;vertical-align:top}
th{background:#f4f4ef;font-weight:600;color:#444;font-size:11.5px;text-transform:uppercase;letter-spacing:0.03em}
tr:last-child td{border-bottom:none}
td.num,th.num{text-align:right;font-variant-numeric:tabular-nums;font-family:ui-monospace,monospace}
.status{display:inline-block;font-size:11px;font-weight:600;padding:1px 7px;border-radius:3px;font-family:ui-monospace,monospace;letter-spacing:0.03em}
.status-pending{background:#f0f0ed;color:#888}
.status-claimed{background:#fdf3d8;color:#8a6608}
.status-wip{background:#d8eef2;color:#0d6976}
.status-done{background:#d9ecda;color:#205c2a}
.status-skipped{background:#f0f0ed;color:#aaa;text-decoration:line-through}
.tier{display:inline-block;font-size:11px;font-weight:600;padding:1px 6px;border-radius:3px;font-family:ui-monospace,monospace}
.tier-T1{background:#ece4f4;color:#5d3e85}
.tier-T2{background:#e0eaf4;color:#2c4f76}
.tier-T3{background:#e3efe1;color:#2a5b35}
.path{font-family:ui-monospace,monospace;font-size:12px;color:#222}
.bytes{color:#888;font-family:ui-monospace,monospace;font-size:11.5px}
.owner{font-family:ui-monospace,monospace;font-size:12px;color:#444}
.owner-none{color:#bbb}
.notes{color:#666;font-size:11.5px;max-width:540px}
.empty{padding:18px;color:#999;text-align:center;background:#fafafa;border:1px dashed #ddd;border-radius:4px;font-size:12.5px}
.legend{display:inline-block;margin-right:14px;font-size:11.5px;color:#666}
.legend .status{margin-right:5px}
footer{margin-top:30px;padding-top:14px;border-top:1px solid #ddd;color:#888;font-size:11.5px;text-align:center}
"""

    def status_chip(st):
        return f'<span class="status status-{st}">{st}</span>'

    def tier_chip(t):
        return f'<span class="tier tier-{t}">{t}</span>' if t in ("T1", "T2", "T3") else t

    def owner_cell(o):
        if not o or o == "—":
            return '<span class="owner owner-none">—</span>'
        return f'<span class="owner">{html.escape(o)}</span>'

    def progress_bar(done, total, cls=""):
        p = (done / total * 100.0) if total else 0.0
        return (
            f'<div class="progress"><div class="progress-bar {cls}" style="width:{p:.1f}%"></div></div>'
            f'<div class="progress-label">{done} / {total} ({p:.1f}%)</div>'
        )

    def progress_bar_float(done, total, cls="", label=""):
        p = (done / total * 100.0) if total else 0.0
        text = label or f"{done:.1f} / {total} ({p:.1f}%)"
        return (
            f'<div class="progress"><div class="progress-bar {cls}" style="width:{p:.1f}%"></div></div>'
            f'<div class="progress-label">{html.escape(text)}</div>'
        )

    def bytes_sort_value(r):
        try:
            return int(r.get("Bytes", "0") or "0")
        except ValueError:
            return 0

    parts = []
    parts.append("<!doctype html>")
    parts.append('<html lang="en"><head><meta charset="utf-8">')
    parts.append("<title>rxdb-rs port progress</title>")
    parts.append(f"<style>{css}</style>")
    parts.append("</head><body><div class=\"wrap\">")

    parts.append("<header>")
    parts.append("<h1>rxdb-rs — port progress</h1>")
    parts.append(
        f'<div class="meta">Upstream: <span class="mono">pubkey/rxdb@16.20.0</span> · '
        f'Generated: <span class="mono">{now}</span> · '
        f'Source: <span class="mono">PORTING.md</span></div>'
    )
    parts.append("</header>")

    # Overall
    parts.append("<section>")
    parts.append("<h2>Overall scope</h2>")
    parts.append(progress_bar_float(
        activity_score,
        total_to_port,
        label=f"wave-adjusted: {activity_score:.1f} / {total_to_port} ({(activity_score / total_to_port * 100.0) if total_to_port else 0.0:.1f}%) · file-row completion: {total_done} / {total_to_port} ({pct:.1f}%)"
    ))
    parts.append('<div class="stat-grid" style="margin-top:14px">')
    parts.append(f'<div class="stat"><div class="lbl">file-row completion</div><div class="num">{pct:.1f}%</div></div>')
    parts.append(f'<div class="stat"><div class="lbl">upstream files</div><div class="num">{upstream_done}/{upstream_total}</div></div>')
    parts.append(f'<div class="stat"><div class="lbl">new code items</div><div class="num">{additions_done}/{additions_total}</div></div>')
    parts.append(f'<div class="stat"><div class="lbl">CTOX integration waves</div><div class="num">{len(integration_rows)}</div></div>')
    parts.append(f'<div class="stat"><div class="lbl">latest wave</div><div class="num">{latest_wave or "—"}</div></div>')
    parts.append(f'<div class="stat"><div class="lbl">rows with wave notes</div><div class="num">{len(activity_rows)}</div></div>')
    for st, cnt in [("pending", by_status["pending"]), ("claimed", by_status["claimed"]),
                    ("wip", by_status["wip"]), ("done", by_status["done"])]:
        parts.append(f'<div class="stat"><div class="lbl">{st}</div><div class="num">{cnt}</div></div>')
    parts.append(f'<div class="stat"><div class="lbl">skip (out-of-scope)</div><div class="num">{len(skip_rows)}</div></div>')
    parts.append("</div>")
    parts.append("</section>")

    # By tier
    parts.append("<section>")
    parts.append("<h2>By tier</h2>")
    parts.append('<table><thead><tr><th>Tier</th><th class="num">Done</th><th class="num">Total</th><th style="width:50%">Progress</th><th>Delegation</th></tr></thead><tbody>')
    tier_labels = {
        "T1": "main agent only — sequential",
        "T2": "parallel subagents with PORT_STYLE.md",
        "T3": "parallel mechanical subagents",
    }
    for t in ("T1", "T2", "T3"):
        tot = sum(by_tier[t].values())
        dn = by_tier[t]["done"]
        parts.append(
            f'<tr><td>{tier_chip(t)}</td><td class="num">{dn}</td><td class="num">{tot}</td>'
            f'<td>{progress_bar(dn, tot, "t"+t[1])}</td>'
            f'<td style="color:#666;font-size:12px">{tier_labels[t]}</td></tr>'
        )
    parts.append("</tbody></table>")
    parts.append("</section>")

    # Recent porting activity
    parts.append("<section>")
    parts.append("<h2>Recent Porting Waves</h2>")
    if not activity_rows:
        parts.append('<div class="empty">No wave markers found in PORTING.md notes.</div>')
    else:
        parts.append('<table><thead><tr><th class="num">Wave</th><th>Status</th><th>Tier</th><th>Phase</th><th>Item</th><th>Rust target</th><th>Latest note</th></tr></thead><tbody>')
        for r in activity_rows[:12]:
            notes = r.get("Notes", "")
            marker = f'wave-{r["_latest_wave"]:03d}'
            idx = notes.find(marker)
            latest_note = notes[idx:] if idx >= 0 else notes
            # Keep this section scan-friendly; full notes remain in the tier tables.
            latest_note = latest_note.split(". ")[0].strip()
            parts.append(
                f'<tr><td class="num mono">{r["_latest_wave"]:03d}</td>'
                f'<td>{status_chip(r.get("Status","pending"))}</td>'
                f'<td>{tier_chip(r.get("Tier","—"))}</td>'
                f'<td class="mono">{html.escape(r["_phase"])}</td>'
                f'<td class="path">{html.escape(r.get("Upstream", r.get("Item","")))}</td>'
                f'<td class="path">{html.escape(r.get("Rust target", r.get("Where","")))}</td>'
                f'<td class="notes">{html.escape(latest_note)}</td></tr>'
            )
        parts.append("</tbody></table>")
    parts.append("</section>")

    # By phase
    parts.append("<section>")
    parts.append("<h2>By phase</h2>")
    parts.append('<table><thead><tr><th>Phase</th><th class="num">Done</th><th class="num">Claimed</th><th class="num">Wip</th><th class="num">Pending</th><th class="num">Total</th><th style="width:40%">Progress</th></tr></thead><tbody>')
    for ph in sorted(by_phase.keys(), key=lambda p: (p != "new-code", p)):
        v = by_phase[ph]
        parts.append(
            f'<tr><td class="mono">{ph}</td>'
            f'<td class="num">{v["done"]}</td>'
            f'<td class="num">{v.get("claimed",0)}</td>'
            f'<td class="num">{v.get("wip",0)}</td>'
            f'<td class="num">{v.get("pending",0)}</td>'
            f'<td class="num">{v["total"]}</td>'
            f'<td>{progress_bar(v["done"], v["total"])}</td></tr>'
        )
    parts.append("</tbody></table>")
    parts.append("</section>")

    # New code additions
    if addition_rows:
        parts.append("<section>")
        parts.append("<h2>New code additions</h2>")
        parts.append('<table><thead><tr><th>Status</th><th>Tier</th><th>Item</th><th>Rust target</th><th>Owner</th><th>Notes</th></tr></thead><tbody>')
        for r in addition_rows:
            parts.append(
                f'<tr><td>{status_chip(r.get("Status","pending"))}</td>'
                f'<td>{tier_chip(r.get("Tier","—"))}</td>'
                f'<td>{html.escape(r.get("Item",""))}</td>'
                f'<td class="path">{html.escape(r.get("Where",""))}</td>'
                f'<td>{owner_cell(r.get("Owner"))}</td>'
                f'<td class="notes">{html.escape(r.get("Notes",""))}</td></tr>'
            )
        parts.append("</tbody></table>")
        parts.append("</section>")

    # CTOX integration waves
    if integration_rows:
        parts.append("<section>")
        parts.append("<h2>CTOX integration waves</h2>")
        parts.append('<table><thead><tr><th class="num">Wave</th><th>Status</th><th>Area</th><th>Files</th><th>Notes</th></tr></thead><tbody>')
        for r in sorted(integration_rows, key=lambda row: int(row.get("Wave", "0") or "0"), reverse=True):
            parts.append(
                f'<tr><td class="num mono">{html.escape(r.get("Wave",""))}</td>'
                f'<td>{status_chip(r.get("Status","pending"))}</td>'
                f'<td class="path">{html.escape(r.get("Area",""))}</td>'
                f'<td class="path">{html.escape(r.get("Files",""))}</td>'
                f'<td class="notes">{html.escape(r.get("Notes",""))}</td></tr>'
            )
        parts.append("</tbody></table>")
        parts.append("</section>")

    # Currently in progress
    parts.append("<section>")
    parts.append("<h2>Currently in progress</h2>")
    if not in_progress:
        parts.append('<div class="empty">No subagent currently claimed a row.</div>')
    else:
        parts.append('<table><thead><tr><th>Owner</th><th>Status</th><th>Tier</th><th>Phase</th><th>Upstream</th><th>Rust target</th></tr></thead><tbody>')
        for r in sorted(in_progress, key=lambda x: (x.get("Owner",""), x.get("Upstream",""))):
            parts.append(
                f'<tr><td>{owner_cell(r.get("Owner"))}</td>'
                f'<td>{status_chip(r.get("Status","pending"))}</td>'
                f'<td>{tier_chip(r.get("Tier","—"))}</td>'
                f'<td class="mono">{html.escape(r["_phase"])}</td>'
                f'<td class="path">{html.escape(r.get("Upstream",""))}</td>'
                f'<td class="path">{html.escape(r.get("Rust target",""))}</td></tr>'
            )
        parts.append("</tbody></table>")
    parts.append("</section>")

    # Three tier sections
    for t in ("T1", "T2", "T3"):
        tier_rows = [r for r in scope_rows if r.get("Tier") == t]
        if not tier_rows:
            continue
        # Sort: pending → claimed → wip → done; within group by phase, then path
        status_rank = {"pending": 0, "claimed": 1, "wip": 2, "done": 3}
        tier_rows.sort(key=lambda r: (status_rank.get(r.get("Status","pending"), 9),
                                       r["_phase"],
                                       bytes_sort_value(r)))
        parts.append("<section>")
        parts.append(f'<h2>Tier {t[1]} <span style="color:#888;font-weight:400">— {tier_labels[t]}</span></h2>')
        done_t = by_tier[t]["done"]
        tot_t = sum(by_tier[t].values())
        parts.append(progress_bar(done_t, tot_t, "t"+t[1]))
        parts.append('<table><thead><tr><th>Status</th><th>Phase</th><th>Upstream</th><th class="num">Bytes</th><th>Rust target</th><th>Owner</th><th>Notes</th></tr></thead><tbody>')
        for r in tier_rows:
            parts.append(
                f'<tr><td>{status_chip(r.get("Status","pending"))}</td>'
                f'<td class="mono">{html.escape(r["_phase"])}</td>'
                f'<td class="path">{html.escape(r.get("Upstream",""))}</td>'
                f'<td class="bytes num">{html.escape(r.get("Bytes",""))}</td>'
                f'<td class="path">{html.escape(r.get("Rust target",""))}</td>'
                f'<td>{owner_cell(r.get("Owner"))}</td>'
                f'<td class="notes">{html.escape(r.get("Notes",""))}</td></tr>'
            )
        parts.append("</tbody></table>")
        parts.append("</section>")

    parts.append('<footer>Regenerate: <span class="mono">python3 src/core/rxdb/tools/build_dashboard.py</span></footer>')
    parts.append("</div></body></html>")
    return "\n".join(parts)


def main():
    if not os.path.exists(PORTING):
        sys.exit(f"PORTING.md not found at {PORTING}")
    text = open(PORTING).read()
    sections = parse_porting(text)
    sec_names = [s for s in sections if s.startswith("phase-")]
    if not sec_names:
        sys.exit("no phase sections parsed — PORTING.md format change?")
    html_out = render(sections)
    with open(OUT, "w") as f:
        f.write(html_out)
        f.write("\n")
    rows = sum(len(sections[s]) for s in sec_names)
    additions = [
        r for r in sections.get("new-code", [])
        if clean_row(r).get("Status") in ("pending", "claimed", "wip", "done")
    ]
    skip = len(sections.get("skip", []))
    print(f"wrote {OUT}")
    print(f"  to-port rows parsed: {rows}")
    print(f"  new-code rows parsed: {len(additions)}")
    print(f"  skip rows parsed: {skip}")


if __name__ == "__main__":
    main()
