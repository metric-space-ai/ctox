#!/usr/bin/env python3
"""Normalize technical drawing review inputs into page images and a manifest."""

from __future__ import annotations

import argparse
import json
import mimetypes
import shutil
import subprocess
import sys
import zipfile
from email import policy
from email.parser import BytesParser
from pathlib import Path


IMAGE_EXTENSIONS = {".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp", ".svg"}
TIFF_EXTENSIONS = {".tif", ".tiff"}
PDF_EXTENSIONS = {".pdf"}
EML_EXTENSIONS = {".eml"}
ZIP_EXTENSIONS = {".zip"}


def safe_name(name: str, fallback: str) -> str:
    name = Path(name or fallback).name
    cleaned = "".join(ch if ch.isalnum() or ch in "._- " else "_" for ch in name).strip()
    return cleaned or fallback


def unique_path(directory: Path, name: str) -> Path:
    candidate = directory / name
    if not candidate.exists():
        return candidate
    stem = candidate.stem
    suffix = candidate.suffix
    for index in range(2, 1000):
        next_candidate = directory / f"{stem}-{index}{suffix}"
        if not next_candidate.exists():
            return next_candidate
    raise RuntimeError(f"could not create unique path for {candidate}")


def is_supported_image(path: Path) -> bool:
    return path.suffix.lower() in IMAGE_EXTENSIONS


def is_tiff(path: Path) -> bool:
    return path.suffix.lower() in TIFF_EXTENSIONS


def is_pdf(path: Path) -> bool:
    return path.suffix.lower() in PDF_EXTENSIONS


def is_eml(path: Path) -> bool:
    return path.suffix.lower() in EML_EXTENSIONS


def is_zip(path: Path) -> bool:
    return path.suffix.lower() in ZIP_EXTENSIONS


def discover_directory(path: Path) -> list[Path]:
    return sorted(child for child in path.rglob("*") if child.is_file())


def render_pdf(path: Path, page_start: int, rendered_dir: Path, dpi: int) -> list[dict]:
    if shutil.which("pdftoppm") is None:
        raise SystemExit(
            "ERROR: pdftoppm is required to render PDFs. Install poppler, or provide rendered page images."
        )
    prefix = rendered_dir / path.stem
    subprocess.run(
        ["pdftoppm", "-png", "-r", str(dpi), str(path), str(prefix)],
        check=True,
    )
    rendered = sorted(rendered_dir.glob(f"{path.stem}-*.png"))
    pages = []
    for offset, image in enumerate(rendered):
        pages.append(
            {
                "page": page_start + offset,
                "path": str(image),
                "source": str(path),
                "source_type": "pdf",
            }
        )
    return pages


def render_tiff(path: Path, page_start: int, rendered_dir: Path) -> list[dict]:
    tiffsplit = shutil.which("tiffsplit")
    sips = shutil.which("sips")
    if tiffsplit is None or sips is None:
        raise SystemExit(
            "ERROR: tiffsplit and sips are required to normalize TIFF files on this machine. "
            "Install libtiff tools, or provide PNG/JPEG page images."
        )

    split_dir = rendered_dir / f"{path.stem}-tiff-pages"
    split_dir.mkdir(parents=True, exist_ok=True)
    prefix = split_dir / "page-"
    subprocess.run([tiffsplit, str(path), str(prefix)], check=True)
    split_pages = sorted(split_dir.glob("page-*.tif"))
    pages = []
    for offset, tiff_page in enumerate(split_pages):
        png_path = rendered_dir / f"{path.stem}-{offset + 1}.png"
        subprocess.run(
            [sips, "-s", "format", "png", str(tiff_page), "--out", str(png_path)],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        pages.append(
            {
                "page": page_start + offset,
                "path": str(png_path),
                "source": str(path),
                "source_type": "tiff",
                "source_page": offset + 1,
            }
        )
    return pages


def copy_image(path: Path, page: int, images_dir: Path) -> dict:
    destination = unique_path(images_dir, safe_name(path.name, f"page-{page}{path.suffix.lower()}"))
    shutil.copy2(path, destination)
    return {
        "page": page,
        "path": str(destination),
        "source": str(path),
        "source_type": "image",
    }


def extract_eml(path: Path, attachments_dir: Path) -> list[Path]:
    message = BytesParser(policy=policy.default).parsebytes(path.read_bytes())
    extracted = []
    counter = 1
    for part in message.iter_attachments():
        filename = part.get_filename() or f"attachment-{counter}"
        payload = part.get_payload(decode=True)
        if not payload:
            continue
        content_type = part.get_content_type()
        guessed_ext = mimetypes.guess_extension(content_type) or ""
        name = safe_name(filename, f"attachment-{counter}{guessed_ext}")
        if Path(name).suffix == "" and guessed_ext:
            name = f"{name}{guessed_ext}"
        destination = unique_path(attachments_dir, name)
        destination.write_bytes(payload)
        extracted.append(destination)
        counter += 1
    return extracted


def extract_zip(path: Path, attachments_dir: Path) -> list[Path]:
    extracted = []
    zip_dir = unique_path(attachments_dir, f"{path.stem}-zip")
    zip_dir.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path) as archive:
        for counter, info in enumerate(archive.infolist(), start=1):
            if info.is_dir():
                continue
            raw_name = Path(info.filename).name
            name = safe_name(raw_name, f"zip-attachment-{counter}")
            destination = unique_path(zip_dir, name)
            with archive.open(info) as source, destination.open("wb") as target:
                shutil.copyfileobj(source, target)
            extracted.append(destination)
    return extracted


def normalize_inputs(inputs: list[Path], output_dir: Path, dpi: int) -> dict:
    output_dir.mkdir(parents=True, exist_ok=True)
    attachments_dir = output_dir / "attachments"
    images_dir = output_dir / "images"
    rendered_dir = output_dir / "rendered"
    attachments_dir.mkdir(exist_ok=True)
    images_dir.mkdir(exist_ok=True)
    rendered_dir.mkdir(exist_ok=True)

    queue = []
    sources = []
    unsupported = []
    for item in inputs:
        if item.is_dir():
            queue.extend(discover_directory(item))
        elif item.is_file():
            queue.append(item)
        else:
            unsupported.append({"path": str(item), "reason": "not_found"})

    page_images = []
    page = 1
    while queue:
        path = queue.pop(0)
        suffix = path.suffix.lower()
        if is_eml(path):
            extracted = extract_eml(path, attachments_dir)
            sources.append({"path": str(path), "type": "eml", "attachments": [str(item) for item in extracted]})
            queue.extend(extracted)
            continue
        if is_zip(path):
            extracted = extract_zip(path, attachments_dir)
            sources.append({"path": str(path), "type": "zip", "attachments": [str(item) for item in extracted]})
            queue.extend(extracted)
            continue
        if is_pdf(path):
            pages = render_pdf(path, page, rendered_dir, dpi)
            page_images.extend(pages)
            page += len(pages)
            sources.append({"path": str(path), "type": "pdf", "pages": len(pages)})
            continue
        if is_tiff(path):
            pages = render_tiff(path, page, rendered_dir)
            page_images.extend(pages)
            page += len(pages)
            sources.append({"path": str(path), "type": "tiff", "pages": len(pages)})
            continue
        if is_supported_image(path):
            page_images.append(copy_image(path, page, images_dir))
            sources.append({"path": str(path), "type": "image", "pages": 1})
            page += 1
            continue
        unsupported.append({"path": str(path), "reason": f"unsupported_extension:{suffix or 'none'}"})

    manifest = {
        "page_images": page_images,
        "sources": sources,
        "unsupported": unsupported,
    }
    (output_dir / "manifest.json").write_text(json.dumps(manifest, indent=2, ensure_ascii=False), encoding="utf-8")
    return manifest


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", action="append", required=True, type=Path, help="PDF, image, TIFF, .eml, .zip, or directory")
    parser.add_argument("--output-dir", required=True, type=Path, help="Directory for normalized review inputs")
    parser.add_argument("--dpi", default=200, type=int, help="PDF render DPI")
    args = parser.parse_args()

    manifest = normalize_inputs(args.input, args.output_dir, args.dpi)
    print(args.output_dir / "manifest.json")
    print(f"page_images={len(manifest['page_images'])}")
    if manifest["unsupported"]:
        print(f"unsupported={len(manifest['unsupported'])}", file=sys.stderr)


if __name__ == "__main__":
    main()
