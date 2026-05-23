#!/usr/bin/env python3
"""CTOX helper for inspecting, building, merging, and installing Minecraft mods."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
import zipfile
from pathlib import Path


IGNORED_JAR_SUFFIXES = ("-sources.jar", "-javadoc.jar", "-dev.jar", "-all-dev.jar")


class ModError(RuntimeError):
    pass


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_json_from_zip(jar: Path, member: str) -> dict | None:
    try:
        with zipfile.ZipFile(jar) as archive:
            if member not in archive.namelist():
                return None
            with archive.open(member) as handle:
                return json.loads(handle.read().decode("utf-8"))
    except (zipfile.BadZipFile, json.JSONDecodeError, UnicodeDecodeError) as exc:
        raise ModError(f"cannot read {member} from {jar}: {exc}") from exc


def read_text_from_zip(jar: Path, member: str) -> str | None:
    try:
        with zipfile.ZipFile(jar) as archive:
            if member not in archive.namelist():
                return None
            with archive.open(member) as handle:
                return handle.read().decode("utf-8", errors="replace")
    except zipfile.BadZipFile as exc:
        raise ModError(f"cannot read {member} from {jar}: {exc}") from exc


def inspect_jar(jar: Path) -> dict:
    jar = jar.expanduser().resolve()
    if not jar.is_file():
        raise ModError(f"jar not found: {jar}")
    if jar.suffix.lower() != ".jar":
        raise ModError(f"expected .jar file: {jar}")

    fabric = read_json_from_zip(jar, "fabric.mod.json")
    if fabric:
        return normalize_metadata(jar, "fabric", fabric.get("id"), fabric.get("version"), fabric.get("name"), fabric)

    quilt = read_json_from_zip(jar, "quilt.mod.json")
    if quilt:
        quilt_meta = quilt.get("quilt_loader") or quilt
        return normalize_metadata(
            jar,
            "quilt",
            quilt_meta.get("id") or quilt_meta.get("group"),
            quilt_meta.get("version"),
            quilt_meta.get("metadata", {}).get("name") or quilt_meta.get("name"),
            quilt,
        )

    neoforge = read_text_from_zip(jar, "META-INF/neoforge.mods.toml")
    if neoforge:
        return normalize_metadata(jar, "neoforge", toml_value(neoforge, "modId"), toml_value(neoforge, "version"), toml_value(neoforge, "displayName"), {"toml": neoforge})

    forge = read_text_from_zip(jar, "META-INF/mods.toml")
    if forge:
        return normalize_metadata(jar, "forge", toml_value(forge, "modId"), toml_value(forge, "version"), toml_value(forge, "displayName"), {"toml": forge})

    raise ModError(f"no recognized Minecraft mod metadata in {jar}")


def normalize_metadata(jar: Path, loader: str, mod_id: str | None, version: str | None, name: str | None, raw: dict) -> dict:
    if not mod_id:
        raise ModError(f"{jar} has {loader} metadata but no mod id")
    version = str(version or "unknown").replace("${version}", "unknown")
    return {
        "path": str(jar),
        "filename": jar.name,
        "loader": loader,
        "mod_id": str(mod_id),
        "name": str(name or mod_id),
        "version": version,
        "sha256": sha256_file(jar),
        "size_bytes": jar.stat().st_size,
        "raw": raw,
    }


def toml_value(text: str, key: str) -> str | None:
    match = re.search(rf"^\s*{re.escape(key)}\s*=\s*[\"']([^\"']+)[\"']", text, re.MULTILINE)
    return match.group(1) if match else None


def release_jars(project: Path) -> list[Path]:
    libs = project / "build" / "libs"
    if not libs.is_dir():
        return []
    jars = sorted(path for path in libs.glob("*.jar") if not path.name.endswith(IGNORED_JAR_SUFFIXES))
    return jars


def build_project(project: Path, task: str) -> dict:
    project = project.expanduser().resolve()
    if not project.is_dir():
        raise ModError(f"project not found: {project}")
    gradlew = project / ("gradlew.bat" if os.name == "nt" else "gradlew")
    command = [str(gradlew), task] if gradlew.exists() else ["gradle", task]
    result = subprocess.run(command, cwd=project, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if result.returncode != 0:
        raise ModError(f"build failed with exit {result.returncode}\n{result.stdout[-4000:]}")
    jars = release_jars(project)
    inspected = [inspect_jar(jar) for jar in jars]
    return {"project": str(project), "command": command, "jars": inspected, "output": result.stdout[-4000:]}


def manifest_path(minecraft_dir: Path) -> Path:
    return minecraft_dir / ".ctox" / "minecraft-mods" / "manifest.json"


def load_manifest(minecraft_dir: Path) -> dict:
    path = manifest_path(minecraft_dir)
    if not path.exists():
        return {"version": 1, "installed": []}
    return json.loads(path.read_text(encoding="utf-8"))


def save_manifest(minecraft_dir: Path, manifest: dict) -> None:
    path = manifest_path(minecraft_dir)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def install_jar(jar: Path, minecraft_dir: Path, profile: str, dry_run: bool) -> dict:
    minecraft_dir = minecraft_dir.expanduser().resolve()
    mods_dir = minecraft_dir / "mods"
    metadata = inspect_jar(jar)
    manifest = load_manifest(minecraft_dir)
    installed = list(manifest.get("installed") or [])
    key = f"{metadata['loader']}:{metadata['mod_id']}"
    destination = mods_dir / metadata["filename"]
    backup_path = None

    for old in installed:
        if old.get("filename") == metadata["filename"] and f"{old.get('loader')}:{old.get('mod_id')}" != key:
            raise ModError(f"filename conflict: {metadata['filename']} is already tracked as {old.get('loader')}:{old.get('mod_id')}")

    if destination.exists():
        current = next((old for old in installed if old.get("filename") == destination.name), None)
        if current and f"{current.get('loader')}:{current.get('mod_id')}" != key:
            raise ModError(f"existing jar conflict: {destination} is tracked as {current.get('loader')}:{current.get('mod_id')}")
        backup_path = minecraft_dir / ".ctox" / "minecraft-mods" / "backups" / f"{int(time.time())}-{destination.name}"

    entry = {
        **{k: metadata[k] for k in ("filename", "loader", "mod_id", "name", "version", "sha256", "size_bytes")},
        "profile": profile,
        "source_path": metadata["path"],
        "installed_path": str(destination),
        "installed_at_ms": int(time.time() * 1000),
    }
    next_installed = [old for old in installed if f"{old.get('loader')}:{old.get('mod_id')}" != key]
    next_installed.append(entry)

    if not dry_run:
        mods_dir.mkdir(parents=True, exist_ok=True)
        if backup_path:
            backup_path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(destination, backup_path)
        shutil.copy2(jar, destination)
        manifest["installed"] = sorted(next_installed, key=lambda item: (item.get("loader", ""), item.get("mod_id", "")))
        save_manifest(minecraft_dir, manifest)

    return {"dry_run": dry_run, "installed": entry, "backup_path": str(backup_path) if backup_path else None}


def semver_key(version: str) -> tuple:
    parts = re.findall(r"\d+", version or "")
    return tuple(int(part) for part in parts[:4]) if parts else (-1,)


def merge_mod_dirs(source_dirs: list[Path], target_dir: Path, dry_run: bool) -> dict:
    selected: dict[str, dict] = {}
    conflicts = []
    for source in source_dirs:
        source = source.expanduser().resolve()
        if not source.is_dir():
            raise ModError(f"source dir not found: {source}")
        for jar in sorted(source.glob("*.jar")):
            metadata = inspect_jar(jar)
            key = f"{metadata['loader']}:{metadata['mod_id']}"
            previous = selected.get(key)
            if not previous or semver_key(metadata["version"]) > semver_key(previous["version"]):
                if previous:
                    conflicts.append({"key": key, "kept": metadata, "replaced": previous})
                selected[key] = metadata
            else:
                conflicts.append({"key": key, "kept": previous, "rejected": metadata})

    if not dry_run:
        target_dir.mkdir(parents=True, exist_ok=True)
        for metadata in selected.values():
            shutil.copy2(metadata["path"], target_dir / metadata["filename"])
        (target_dir / ".ctox-merge-manifest.json").write_text(
            json.dumps({"selected": list(selected.values()), "conflicts": conflicts}, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
    return {"dry_run": dry_run, "target_dir": str(target_dir), "selected": list(selected.values()), "conflicts": conflicts}


def scaffold_new(target: Path, loader: str, mod_id: str, name: str) -> dict:
    if loader != "fabric":
        raise ModError("new currently scaffolds Fabric projects only")
    target = target.expanduser().resolve()
    if target.exists() and any(target.iterdir()):
        raise ModError(f"target is not empty: {target}")
    package_name = mod_id.replace("-", "_")
    package_path = Path(*package_name.split("."))
    main_class = "".join(part.capitalize() for part in re.split(r"[-_.]+", mod_id)) + "Mod"
    files = {
        "settings.gradle": f'pluginManagement {{ repositories {{ maven {{ url = "https://maven.fabricmc.net/" }} gradlePluginPortal() }} }}\ndependencyResolutionManagement {{ repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS); repositories {{ mavenCentral(); maven {{ url = "https://maven.fabricmc.net/" }} }} }}\nrootProject.name = "{mod_id}"\n',
        "build.gradle": 'plugins { id "fabric-loom" version "1.10-SNAPSHOT"; id "maven-publish"; id "java" }\nversion = "0.1.0"\ngroup = "ctox.minecraft"\nbase { archivesName = "' + mod_id + '" }\ndependencies { minecraft "com.mojang:minecraft:1.21.5"; mappings "net.fabricmc:yarn:1.21.5+build.1:v2"; modImplementation "net.fabricmc:fabric-loader:0.16.10"; modImplementation "net.fabricmc.fabric-api:fabric-api:0.119.5+1.21.5" }\njava { toolchain { languageVersion = JavaLanguageVersion.of(21) } }\n',
        "src/main/resources/fabric.mod.json": json.dumps({"schemaVersion": 1, "id": mod_id, "version": "0.1.0", "name": name, "entrypoints": {"main": [f"ctox.minecraft.{package_name}.{main_class}"]}, "depends": {"fabricloader": ">=0.16.0", "minecraft": ">=1.21"}}, indent=2) + "\n",
        f"src/main/java/ctox/minecraft/{package_path}/{main_class}.java": f"package ctox.minecraft.{package_name};\n\nimport net.fabricmc.api.ModInitializer;\n\npublic class {main_class} implements ModInitializer {{\n    @Override\n    public void onInitialize() {{\n    }}\n}}\n",
    }
    for rel, content in files.items():
        path = target / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
    return {"target": str(target), "loader": loader, "mod_id": mod_id, "name": name, "files": sorted(files)}


def print_json(value: dict) -> None:
    print(json.dumps(value, indent=2, sort_keys=True))


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="cmd", required=True)

    inspect_cmd = sub.add_parser("inspect")
    inspect_cmd.add_argument("--jar", required=True, type=Path)

    build_cmd = sub.add_parser("build")
    build_cmd.add_argument("--project", required=True, type=Path)
    build_cmd.add_argument("--task", default="build")

    install_cmd = sub.add_parser("install")
    install_cmd.add_argument("--jar", required=True, type=Path)
    install_cmd.add_argument("--minecraft-dir", required=True, type=Path)
    install_cmd.add_argument("--profile", default="default")
    install_cmd.add_argument("--dry-run", action="store_true")

    merge_cmd = sub.add_parser("merge")
    merge_cmd.add_argument("--source-dir", action="append", required=True, type=Path)
    merge_cmd.add_argument("--target-dir", required=True, type=Path)
    merge_cmd.add_argument("--dry-run", action="store_true")

    new_cmd = sub.add_parser("new")
    new_cmd.add_argument("--target", required=True, type=Path)
    new_cmd.add_argument("--loader", required=True, choices=["fabric"])
    new_cmd.add_argument("--mod-id", required=True)
    new_cmd.add_argument("--name", required=True)

    args = parser.parse_args(argv)
    try:
        if args.cmd == "inspect":
            print_json(inspect_jar(args.jar))
        elif args.cmd == "build":
            print_json(build_project(args.project, args.task))
        elif args.cmd == "install":
            print_json(install_jar(args.jar, args.minecraft_dir, args.profile, args.dry_run))
        elif args.cmd == "merge":
            print_json(merge_mod_dirs(args.source_dir, args.target_dir, args.dry_run))
        elif args.cmd == "new":
            print_json(scaffold_new(args.target, args.loader, args.mod_id, args.name))
        return 0
    except ModError as exc:
        print(f"minecraft_mod_manager: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
