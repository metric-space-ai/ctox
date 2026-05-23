---
name: minecraft-mod-development
description: "Use when CTOX must create, build, inspect, merge, install, or troubleshoot Minecraft mods or shader/modpack installs across Fabric, Forge, NeoForge, Quilt, and vanilla Minecraft directories."
metadata:
  short-description: Build, inspect, merge, and install Minecraft mods with reproducible manifests
cluster: product_engineering
---

# Minecraft Mod Development

## Use When

- A Minecraft mod must be created, modified, built, installed, merged, or removed.
- A Minecraft instance, modpack, controller-support setup, shader pack, or loader setup must be audited.
- A user asks whether GPU, Java, controller, or driver prerequisites are suitable for Minecraft mod development.

## Hard Stop Rules

Stop and report the blocker instead of changing files when any of these is true:

```text
the target Minecraft directory or launcher instance is ambiguous
the loader cannot be inferred and the user did not choose Fabric, Forge, NeoForge, Quilt, or vanilla
the build tool is missing and no Gradle wrapper or documented build command exists
the mod jar has no recognizable metadata file
the install would overwrite a different mod id without a manifest backup
the install target is a live running instance and the user did not ask for a hot copy
driver or kernel upgrades require a reboot while GPU workloads are active
```

Do not claim a mod is installed until the target `mods/` directory and CTOX manifest were checked after copy.

## Supported Workflow

Use the helper for deterministic work:

```sh
python3 src/skills/system/product_engineering/minecraft-mod-development/scripts/minecraft_mod_manager.py inspect --jar <mod.jar>
python3 src/skills/system/product_engineering/minecraft-mod-development/scripts/minecraft_mod_manager.py build --project <mod-project>
python3 src/skills/system/product_engineering/minecraft-mod-development/scripts/minecraft_mod_manager.py install --jar <mod.jar> --minecraft-dir <dir>
python3 src/skills/system/product_engineering/minecraft-mod-development/scripts/minecraft_mod_manager.py merge --source-dir <mods-a> --source-dir <mods-b> --target-dir <staging-mods>
python3 src/skills/system/product_engineering/minecraft-mod-development/scripts/minecraft_mod_manager.py new --target <dir> --loader fabric --mod-id <id> --name <name>
```

Prefer `MINECRAFT_DIR` or an explicit launcher instance path. On Linux, use `~/.minecraft` only after checking whether the launcher uses Prism, MultiMC, Modrinth, CurseForge, or Steam/Flatpak-specific paths.

## Build Rules

1. Prefer the project Gradle wrapper: `./gradlew build`.
2. Collect release jars from `build/libs`.
3. Ignore `*-sources.jar`, `*-javadoc.jar`, `*-dev.jar`, and `*-all-dev.jar` unless explicitly requested.
4. Inspect the selected jar before installing it.
5. Record the build command and jar metadata in the installation manifest.

## Install Rules

The helper writes `.ctox/minecraft-mods/manifest.json` under the Minecraft directory. Preserve it. It records installed jar path, SHA-256, loader, mod id, version, source path, and timestamp.

When replacing an installed jar with the same loader and mod id, back up the old jar under `.ctox/minecraft-mods/backups/`. When another jar has the same filename but a different mod id, stop and report a conflict.

## Merge Rules

Merge mod sets by metadata, not filenames. The identity key is:

```text
<loader>:<mod_id>
```

For duplicate keys, prefer the highest parseable semantic version and report non-semver conflicts for human review. Never merge source code automatically unless the source is a git worktree and the normal test/build path succeeds after conflict resolution.

## Driver And Runtime Checks

For Linux gaming workstations, check before changing drivers:

```sh
nvidia-smi --query-gpu=name,driver_version --format=csv,noheader
nvcc --version || true
ubuntu-drivers devices || true
apt-cache policy nvidia-driver-570 nvidia-driver-575 nvidia-driver-580
```

If the installed NVIDIA driver is newer than the configured apt candidate, do not downgrade. If a newer driver requires kernel modules or reboot, report the maintenance window requirement before applying it.

