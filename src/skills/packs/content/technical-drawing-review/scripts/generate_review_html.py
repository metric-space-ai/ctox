#!/usr/bin/env python3
"""Generate a standalone interactive HTML review for pinned drawing findings."""

import argparse
import base64
import html
import json
import mimetypes
from pathlib import Path


SEVERITY_ORDER = {"critical": 0, "major": 1, "minor": 2, "info": 3}


def parse_page_image(value: str) -> tuple[int, Path]:
    if "=" not in value:
        raise argparse.ArgumentTypeError("--page-image must use PAGE=PATH")
    page_text, path_text = value.split("=", 1)
    try:
        page = int(page_text)
    except ValueError as exc:
        raise argparse.ArgumentTypeError("PAGE must be an integer") from exc
    if page < 1:
        raise argparse.ArgumentTypeError("PAGE must be >= 1")
    path = Path(path_text)
    if not path.is_file():
        raise argparse.ArgumentTypeError(f"image not found: {path}")
    return page, path


def page_images_from_manifest(path: Path) -> list[tuple[int, Path]]:
    data = json.loads(path.read_text(encoding="utf-8"))
    rows = data.get("page_images")
    if not isinstance(rows, list):
        raise SystemExit("ERROR: manifest must contain a page_images list")
    page_images = []
    for index, row in enumerate(rows, start=1):
        if not isinstance(row, dict):
            raise SystemExit(f"ERROR: manifest page_images[{index}] must be an object")
        page = row.get("page")
        image_path = row.get("path")
        if not isinstance(page, int) or page < 1:
            raise SystemExit(f"ERROR: manifest page_images[{index}].page must be >= 1")
        if not isinstance(image_path, str) or not image_path:
            raise SystemExit(f"ERROR: manifest page_images[{index}].path is required")
        resolved = Path(image_path)
        if not resolved.is_absolute():
            resolved = path.parent / resolved
        if not resolved.is_file():
            raise SystemExit(f"ERROR: manifest image not found: {resolved}")
        page_images.append((page, resolved))
    return page_images


def data_url(path: Path) -> str:
    mime = mimetypes.guess_type(path.name)[0] or "image/png"
    encoded = base64.b64encode(path.read_bytes()).decode("ascii")
    return f"data:{mime};base64,{encoded}"


def esc(value: object) -> str:
    return html.escape("" if value is None else str(value), quote=True)


def load_findings(path: Path) -> dict:
    data = json.loads(path.read_text(encoding="utf-8"))
    findings = data.get("findings")
    if not isinstance(findings, list):
        raise SystemExit("ERROR: findings JSON must contain a findings list")
    return data


def severity_rank(finding: dict) -> int:
    return SEVERITY_ORDER.get(str(finding.get("severity", "info")), 99)


def js_string(value: object) -> str:
    return json.dumps("" if value is None else str(value), ensure_ascii=True)


def build_html(data: dict, images: list[tuple[int, Path]], title: str, language: str) -> str:
    image_payload = [
        {"page": page, "name": path.name, "src": data_url(path)}
        for page, path in sorted(images, key=lambda item: item[0])
    ]
    findings = sorted(data.get("findings", []), key=lambda item: (severity_rank(item), item.get("id", "")))
    summary = data.get("summary", {})
    drawing = data.get("drawing", {})
    image_json = json.dumps(image_payload, ensure_ascii=True)
    findings_json = json.dumps(findings, ensure_ascii=True)
    is_de = language.lower().startswith("de")
    labels = {
        "panel_title": "KI-Pruefung" if is_de else "AI Review",
        "status_open": "Abgeschlossen" if is_de else "Completed",
        "shared": "Gefundene Probleme" if is_de else "Found Issues",
        "confidence": "Konfidenz" if is_de else "Confidence",
        "evidence": "Befund" if is_de else "Evidence",
        "risk": "Risiko" if is_de else "Risk",
        "recommendation": "Empfehlung" if is_de else "Recommendation",
        "pin": "Pin" if is_de else "Pin",
        "no_findings": "Keine aktionsrelevanten Befunde." if is_de else "No actionable findings.",
    }

    return f"""<!doctype html>
<html lang="{esc(language)}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{esc(title)}</title>
<style>
:root {{
  --bg: #f7f8f6;
  --panel: #ffffff;
  --ink: #24292f;
  --muted: #737b82;
  --line: #dde3e2;
  --critical: #b42318;
  --major: #c2410c;
  --minor: #b7791f;
  --info: #2f83c5;
  --active: #2f83c5;
  --accent: #c5cd43;
}}
* {{ box-sizing: border-box; }}
body {{
  margin: 0;
  color: var(--ink);
  background: var(--bg);
  font: 14px/1.45 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}}
header {{
  height: 52px;
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 16px;
  padding: 0 22px;
  border-bottom: 1px solid var(--line);
  background: var(--panel);
}}
h1 {{
  margin: 0;
  font-size: 15px;
  font-weight: 700;
}}
.meta {{
  margin-left: auto;
  color: var(--muted);
  font-size: 12px;
  white-space: nowrap;
}}
.shell {{
  display: grid;
  grid-template-columns: minmax(0, 1fr) 382px;
  height: calc(100vh - 52px);
}}
.viewer {{
  overflow: auto;
  padding: 22px;
  background: #f4f5f2;
}}
.page {{
  position: relative;
  width: min(100%, 1320px);
  margin: 0 auto 20px;
  background: white;
  border: 1px solid var(--line);
  box-shadow: 0 8px 28px rgba(25, 35, 45, 0.08);
}}
.page img {{
  display: block;
  width: 100%;
  height: auto;
}}
.pin {{
  position: absolute;
  width: 34px;
  height: 34px;
  transform: translate(-50%, -50%);
  border: 3px solid rgba(255, 255, 255, 0.92);
  border-radius: 50%;
  color: white;
  font-weight: 800;
  font-size: 14px;
  display: grid;
  place-items: center;
  cursor: pointer;
  box-shadow: 0 10px 26px rgba(47, 131, 197, 0.3);
}}
.pin::after {{
  content: "";
  position: absolute;
  left: 50%;
  top: 100%;
  transform: translateX(-50%);
  border-left: 7px solid transparent;
  border-right: 7px solid transparent;
  border-top: 11px solid currentColor;
}}
.pin.critical {{ background: var(--info); color: var(--info); }}
.pin.major {{ background: var(--info); color: var(--info); }}
.pin.minor {{ background: var(--info); color: var(--info); }}
.pin.info {{ background: var(--info); color: var(--info); }}
.pin span {{
  color: white;
  position: relative;
  z-index: 1;
}}
.pin.active {{
  outline: 4px solid rgba(197, 205, 67, 0.45);
  background: var(--active);
}}
aside {{
  overflow: auto;
  border-left: 1px solid var(--line);
  background: #fbfbfa;
}}
.panel-head {{
  display: flex;
  gap: 10px;
  align-items: center;
  padding: 18px 20px 10px;
}}
.mark {{
  width: 34px;
  height: 34px;
  border-radius: 10px;
  background: linear-gradient(135deg, #c5cd43 0%, #38a1d5 52%, #5b6ed6 100%);
}}
.panel-title {{
  font-weight: 750;
  line-height: 1.15;
}}
.panel-subtitle {{
  color: var(--muted);
  font-size: 12px;
  margin-top: 2px;
}}
.summary {{
  padding: 10px 20px 16px;
  border-bottom: 1px solid var(--line);
}}
.summary strong {{
  font-size: 13px;
}}
.summary-row {{
  display: flex;
  justify-content: space-between;
  gap: 12px;
  margin-top: 6px;
  color: var(--muted);
  font-size: 12px;
}}
.finding {{
  position: relative;
  margin: 14px 16px;
  border: 1px solid #edf0ef;
  border-left: 5px solid var(--accent);
  border-radius: 8px;
  padding: 14px 14px 14px 58px;
  cursor: pointer;
  background: linear-gradient(110deg, #ffffff 0%, #f4fbff 100%);
  box-shadow: 0 6px 18px rgba(30, 40, 50, 0.05);
}}
.finding.active {{
  border-color: rgba(47, 131, 197, 0.45);
  box-shadow: 0 8px 24px rgba(47, 131, 197, 0.14);
}}
.finding h2 {{
  margin: 2px 0 10px;
  font-size: 14px;
  line-height: 1.3;
}}
.finding-number {{
  position: absolute;
  left: 18px;
  top: 18px;
  width: 28px;
  height: 28px;
  display: grid;
  place-items: center;
  border-radius: 50%;
  background: var(--info);
  color: white;
  font-weight: 800;
  box-shadow: 0 5px 16px rgba(47, 131, 197, 0.25);
}}
.badges {{
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}}
.badge {{
  border: 1px solid var(--line);
  border-radius: 999px;
  padding: 2px 8px;
  font-size: 11px;
  color: var(--muted);
  background: white;
}}
.badge.critical {{ color: var(--critical); border-color: #f1b5b0; }}
.badge.major {{ color: var(--major); border-color: #fed7aa; }}
.badge.minor {{ color: var(--minor); border-color: #fde68a; }}
.badge.info {{ color: var(--info); border-color: #bfdbfe; }}
.field {{
  margin-top: 8px;
  color: #3c454c;
}}
.label {{
  display: block;
  color: var(--muted);
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}}
.empty {{
  padding: 24px 20px;
  color: var(--muted);
}}
@media (max-width: 900px) {{
  header {{ height: auto; align-items: flex-start; flex-direction: column; padding: 12px 14px; }}
  .shell {{ display: block; height: auto; }}
  .viewer {{ padding: 12px; }}
  aside {{ border-left: 0; border-top: 1px solid var(--line); }}
}}
</style>
</head>
<body>
<header>
  <h1>{esc(title)}</h1>
  <div class="meta">{esc(drawing.get("source", ""))}</div>
</header>
<main class="shell">
  <section class="viewer" id="viewer"></section>
  <aside>
    <section class="panel-head">
      <div class="mark" aria-hidden="true"></div>
      <div>
        <div class="panel-title">{esc(labels["panel_title"])}</div>
        <div class="panel-subtitle">{esc(labels["status_open"])}</div>
      </div>
    </section>
    <section class="summary">
      <strong>{esc(labels["shared"])}, {esc(summary.get("finding_count", len(findings)))}</strong>
      <div class="summary-row"><span>Status</span><span>{esc(summary.get("overall_status", "review"))}</span></div>
      <div class="summary-row"><span>Highest severity</span><span>{esc(summary.get("highest_severity", "n/a"))}</span></div>
      <div class="summary-row"><span>Pages</span><span>{esc(", ".join(str(item["page"]) for item in image_payload))}</span></div>
    </section>
    <section id="findings"></section>
  </aside>
</main>
<script>
const pages = {image_json};
const findings = {findings_json};
const labels = {{
  confidence: {js_string(labels["confidence"])},
  evidence: {js_string(labels["evidence"])},
  risk: {js_string(labels["risk"])},
  recommendation: {js_string(labels["recommendation"])},
  pin: {js_string(labels["pin"])},
  noFindings: {js_string(labels["no_findings"])}
}};

const viewer = document.getElementById("viewer");
const findingsPanel = document.getElementById("findings");

function text(value) {{
  return value === undefined || value === null ? "" : String(value);
}}

function renderPages() {{
  pages.forEach(page => {{
    const pageEl = document.createElement("div");
    pageEl.className = "page";
    pageEl.id = `page-${{page.page}}`;
    const image = document.createElement("img");
    image.src = page.src;
    image.alt = `Drawing page ${{page.page}}`;
    pageEl.appendChild(image);
    findings.filter(f => f.pin && f.pin.page === page.page).forEach((finding, index) => {{
      const pin = document.createElement("button");
      pin.className = `pin ${{text(finding.severity)}}`;
      pin.id = `pin-${{finding.id}}`;
      pin.style.left = `${{Math.max(0, Math.min(1, Number(finding.pin.x))) * 100}}%`;
      pin.style.top = `${{Math.max(0, Math.min(1, Number(finding.pin.y))) * 100}}%`;
      const pinNumber = document.createElement("span");
      pinNumber.textContent = text(finding.id).replace(/^TD-0*/, "") || String(index + 1);
      pin.appendChild(pinNumber);
      pin.title = `${{finding.id}}: ${{finding.title}}`;
      pin.addEventListener("click", () => activateFinding(finding.id, true));
      pageEl.appendChild(pin);
    }});
    viewer.appendChild(pageEl);
  }});
}}

function renderFindings() {{
  if (!findings.length) {{
    findingsPanel.innerHTML = `<div class="empty">${{labels.noFindings}}</div>`;
    return;
  }}
  findings.forEach(finding => {{
    const item = document.createElement("article");
    item.className = "finding";
    item.id = `finding-${{finding.id}}`;
    const number = text(finding.id).replace(/^TD-0*/, "") || "";
    item.innerHTML = `
      <div class="finding-number">${{number}}</div>
      <div class="badges">
        <span class="badge ${{text(finding.severity)}}">${{text(finding.severity)}}</span>
        <span class="badge">${{text(finding.category)}}</span>
        <span class="badge">${{labels.confidence}} ${{Math.round(Number(finding.confidence || 0) * 100)}}%</span>
        <span class="badge">${{text(finding.status || "open")}}</span>
      </div>
      <h2>${{text(finding.title)}}</h2>
      <div class="field"><span class="label">${{labels.evidence}}</span>${{text(finding.evidence)}}</div>
      <div class="field"><span class="label">${{labels.risk}}</span>${{text(finding.risk)}}</div>
      <div class="field"><span class="label">${{labels.recommendation}}</span>${{text(finding.recommendation)}}</div>
      <div class="field"><span class="label">${{labels.pin}}</span>Page ${{finding.pin?.page || "?"}}, ${{text(finding.pin?.anchor)}}</div>
    `;
    item.addEventListener("click", () => activateFinding(finding.id, true));
    findingsPanel.appendChild(item);
  }});
}}

function activateFinding(id, scroll) {{
  document.querySelectorAll(".finding.active, .pin.active").forEach(el => el.classList.remove("active"));
  const findingEl = document.getElementById(`finding-${{id}}`);
  const pinEl = document.getElementById(`pin-${{id}}`);
  if (findingEl) findingEl.classList.add("active");
  if (pinEl) pinEl.classList.add("active");
  if (scroll && pinEl) pinEl.scrollIntoView({{ behavior: "smooth", block: "center", inline: "center" }});
  if (scroll && findingEl) findingEl.scrollIntoView({{ behavior: "smooth", block: "nearest" }});
}}

renderPages();
renderFindings();
if (findings[0]) activateFinding(findings[0].id, false);
</script>
</body>
</html>
"""


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--findings", required=True, type=Path, help="Path to findings JSON")
    parser.add_argument("--manifest", type=Path, help="Manifest from prepare_review_inputs.py")
    parser.add_argument("--page-image", action="append", type=parse_page_image, help="Page image mapping as PAGE=PATH")
    parser.add_argument("--output", required=True, type=Path, help="Output HTML path")
    parser.add_argument("--title", default="Technical Drawing Review", help="HTML title")
    parser.add_argument("--language", default="de", help="UI language code, for example de or en")
    args = parser.parse_args()

    data = load_findings(args.findings)
    page_images = []
    if args.manifest:
        page_images.extend(page_images_from_manifest(args.manifest))
    if args.page_image:
        page_images.extend(args.page_image)
    if not page_images:
        raise SystemExit("ERROR: provide --manifest or at least one --page-image PAGE=PATH")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(build_html(data, page_images, args.title, args.language), encoding="utf-8")
    print(f"Wrote {args.output}")


if __name__ == "__main__":
    main()
