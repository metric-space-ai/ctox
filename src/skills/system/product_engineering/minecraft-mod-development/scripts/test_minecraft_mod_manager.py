#!/usr/bin/env python3
from __future__ import annotations

import json
import subprocess
import tempfile
import zipfile
from pathlib import Path


SCRIPT = Path(__file__).with_name("minecraft_mod_manager.py")


def run(*args: str) -> dict:
    result = subprocess.run(["python3", str(SCRIPT), *args], text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert result.returncode == 0, result.stderr
    return json.loads(result.stdout)


def make_fabric_jar(path: Path, mod_id: str, version: str) -> None:
    with zipfile.ZipFile(path, "w") as jar:
        jar.writestr("fabric.mod.json", json.dumps({"schemaVersion": 1, "id": mod_id, "version": version, "name": mod_id.title()}))


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        jar_a = root / "controller-support-1.0.0.jar"
        jar_b = root / "controller-support-1.1.0.jar"
        make_fabric_jar(jar_a, "controller-support", "1.0.0")
        make_fabric_jar(jar_b, "controller-support", "1.1.0")

        inspected = run("inspect", "--jar", str(jar_a))
        assert inspected["loader"] == "fabric"
        assert inspected["mod_id"] == "controller-support"

        mc_dir = root / "minecraft"
        installed = run("install", "--jar", str(jar_a), "--minecraft-dir", str(mc_dir))
        assert Path(installed["installed"]["installed_path"]).exists()
        manifest = json.loads((mc_dir / ".ctox" / "minecraft-mods" / "manifest.json").read_text())
        assert manifest["installed"][0]["mod_id"] == "controller-support"

        source_a = root / "mods-a"
        source_b = root / "mods-b"
        source_a.mkdir()
        source_b.mkdir()
        jar_a.replace(source_a / jar_a.name)
        jar_b.replace(source_b / jar_b.name)
        merged = run("merge", "--source-dir", str(source_a), "--source-dir", str(source_b), "--target-dir", str(root / "merged"))
        assert merged["selected"][0]["version"] == "1.1.0"
        assert (root / "merged" / jar_b.name).exists()

        scaffolded = run("new", "--target", str(root / "new-mod"), "--loader", "fabric", "--mod-id", "ctox-test-mod", "--name", "CTOX Test Mod")
        assert "src/main/resources/fabric.mod.json" in scaffolded["files"]

    print("minecraft mod manager tests OK")


if __name__ == "__main__":
    main()
