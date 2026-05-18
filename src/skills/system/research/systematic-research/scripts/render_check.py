#!/usr/bin/env python3
"""
render_check.py — quick visual review of a deep-research DOCX.

Steps:
  1. Convert DOCX -> PDF via `soffice --headless --convert-to pdf`.
  2. Convert PDF -> per-page PNGs via `pdftoppm -r 150 -png`.
  3. Print every produced PNG path to stdout, one per line.

Detects missing dependencies and prints install hints.

Exit codes:
   0  ok (PNG paths printed to stdout)
   2  required tools missing
   3  conversion failed

CLI:

    python3 render_check.py --docx /path/to/report.docx --out-dir /tmp/render_check

The script does not download or install anything. It works on macOS dev
machines and on Ubuntu Linux production hosts (Yoda).
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def _print_install_hint() -> None:
    sys.stderr.write(
        "render_check.py: required tools not found.\n"
        "  macOS:  brew install libreoffice poppler\n"
        "  Ubuntu: sudo apt-get install -y libreoffice poppler-utils\n"
    )


def _which(name: str) -> str:
    path = shutil.which(name)
    if not path:
        return ""
    return path


def _convert_docx_to_pdf(soffice: str, docx_path: Path, out_dir: Path) -> Path:
    """Run soffice --headless --convert-to pdf. Use a per-call user
    profile dir so concurrent invocations do not collide on the LO
    config lock.
    """
    user_profile = tempfile.mkdtemp(prefix="lo_profile_")
    try:
        cmd = [
            soffice,
            f"-env:UserInstallation=file://{user_profile}",
            "--headless",
            "--convert-to",
            "pdf",
            "--outdir",
            str(out_dir),
            str(docx_path),
        ]
        proc = subprocess.run(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=180,
        )
        if proc.returncode != 0:
            sys.stderr.write(
                "render_check.py: soffice exited with "
                f"code {proc.returncode}\n"
                f"stdout: {proc.stdout}\n"
                f"stderr: {proc.stderr}\n"
            )
            sys.exit(3)
        pdf_path = out_dir / (docx_path.stem + ".pdf")
        if not pdf_path.exists():
            sys.stderr.write(
                f"render_check.py: expected PDF not produced at {pdf_path}\n"
            )
            sys.exit(3)
        return pdf_path
    finally:
        # LibreOffice profile is single-use; clean it up so we don't
        # accumulate cruft in /tmp.
        shutil.rmtree(user_profile, ignore_errors=True)


def _convert_pdf_to_pngs(pdftoppm: str, pdf_path: Path, out_dir: Path) -> list:
    """Run pdftoppm -r 150 -png. Return paths to produced PNGs sorted
    by page number.
    """
    prefix = out_dir / pdf_path.stem
    cmd = [pdftoppm, "-r", "150", "-png", str(pdf_path), str(prefix)]
    proc = subprocess.run(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        timeout=120,
    )
    if proc.returncode != 0:
        sys.stderr.write(
            "render_check.py: pdftoppm exited with "
            f"code {proc.returncode}\n"
            f"stdout: {proc.stdout}\n"
            f"stderr: {proc.stderr}\n"
        )
        sys.exit(3)
    pages = sorted(
        out_dir.glob(f"{pdf_path.stem}-*.png")
    )
    if not pages:
        # pdftoppm sometimes emits a single PNG named without -1
        single = out_dir / f"{pdf_path.stem}.png"
        if single.exists():
            return [single]
        sys.stderr.write(
            "render_check.py: pdftoppm produced no PNGs\n"
        )
        sys.exit(3)
    return pages


def main(argv: list) -> int:
    parser = argparse.ArgumentParser(
        prog="render_check.py",
        description="Visual review helper: DOCX -> PDF -> per-page PNGs.",
    )
    parser.add_argument("--docx", required=True, help="Path to input DOCX")
    parser.add_argument(
        "--out-dir",
        default=None,
        help="Output directory (default: a fresh temp dir under /tmp)",
    )
    args = parser.parse_args(argv)

    docx_path = Path(args.docx).expanduser().resolve()
    if not docx_path.exists():
        sys.stderr.write(f"render_check.py: DOCX not found: {docx_path}\n")
        return 3
    if docx_path.suffix.lower() != ".docx":
        sys.stderr.write(
            f"render_check.py: input is not a .docx file: {docx_path}\n"
        )
        return 3

    soffice = _which("soffice") or _which("libreoffice")
    pdftoppm = _which("pdftoppm")
    if not soffice or not pdftoppm:
        _print_install_hint()
        return 2

    if args.out_dir:
        out_dir = Path(args.out_dir).expanduser().resolve()
        out_dir.mkdir(parents=True, exist_ok=True)
    else:
        out_dir = Path(tempfile.mkdtemp(prefix="render_check_"))

    pdf_path = _convert_docx_to_pdf(soffice, docx_path, out_dir)
    pngs = _convert_pdf_to_pngs(pdftoppm, pdf_path, out_dir)

    for png in pngs:
        sys.stdout.write(str(png) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
