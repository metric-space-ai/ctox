#!/usr/bin/env python3
import argparse
import json
import secrets
from pathlib import Path


def parse_kv(items: list[str]) -> dict[str, str]:
    values = {}
    for item in items:
        key, sep, value = item.partition("=")
        if not sep or not key:
            raise SystemExit(f"invalid KEY=VALUE item: {item}")
        values[key] = value
    return values


def write_env(path: Path, values: dict[str, str]) -> None:
    existing = {}
    if path.exists():
        for line in path.read_text(encoding="utf-8").splitlines():
            if "=" in line and not line.lstrip().startswith("#"):
                key, value = line.split("=", 1)
                existing[key] = value
    existing.update(values)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        "".join(f"{key}={value}\n" for key, value in sorted(existing.items())),
        encoding="utf-8",
    )
    path.chmod(0o600)


def describe(path: Path) -> dict:
    keys = []
    if path.exists():
        for line in path.read_text(encoding="utf-8").splitlines():
            if "=" in line and not line.lstrip().startswith("#"):
                keys.append(line.split("=", 1)[0])
    return {"path": str(path), "exists": path.exists(), "keys": sorted(keys)}


def read_metadata(path: Path) -> list[dict]:
    if not path.exists():
        return []
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, list):
        raise SystemExit("metadata json must be a list")
    return payload


def write_metadata(path: Path, records: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(records, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    path.chmod(0o600)


def upsert_metadata(
    path: Path,
    secret_key: str,
    kind: str,
    status: str,
    reply_path: str,
    material_path: str | None,
    bindings: list[str],
) -> dict:
    records = read_metadata(path)
    kept = [record for record in records if record.get("secret_key") != secret_key]
    record = {
        "secret_key": secret_key,
        "kind": kind,
        "status": status,
        "reply_path": reply_path,
        "material_path": material_path or "",
        "bindings": sorted(binding for binding in bindings if binding.strip()),
    }
    kept.append(record)
    kept.sort(key=lambda item: item.get("secret_key", ""))
    write_metadata(path, kept)
    return record


def main() -> int:
    parser = argparse.ArgumentParser(description="Open helper for local secret material references.")
    sub = parser.add_subparsers(dest="command", required=True)
    gen = sub.add_parser("generate-password")
    gen.add_argument("--length", type=int, default=32)
    upsert = sub.add_parser("upsert-env")
    upsert.add_argument("--path", required=True)
    upsert.add_argument("--set", action="append", default=[])
    meta = sub.add_parser("upsert-metadata")
    meta.add_argument("--path", required=True)
    meta.add_argument("--secret-key", required=True)
    meta.add_argument("--kind", required=True)
    meta.add_argument("--status", required=True)
    meta.add_argument("--reply-path", required=True)
    meta.add_argument("--material-path")
    meta.add_argument("--binding", action="append", default=[])
    desc = sub.add_parser("describe")
    desc.add_argument("--path", required=True)
    desc_meta = sub.add_parser("describe-metadata")
    desc_meta.add_argument("--path", required=True)
    args = parser.parse_args()
    if args.command == "generate-password":
        print(json.dumps({"password": secrets.token_hex(max(1, args.length // 2))}, indent=2))
        return 0
    if args.command == "upsert-env":
        path = Path(args.path)
        values = parse_kv(args.set)
        write_env(path, values)
        print(json.dumps(describe(path), indent=2))
        return 0
    if args.command == "upsert-metadata":
        record = upsert_metadata(
            Path(args.path),
            secret_key=args.secret_key,
            kind=args.kind,
            status=args.status,
            reply_path=args.reply_path,
            material_path=args.material_path,
            bindings=args.binding,
        )
        print(json.dumps(record, indent=2, sort_keys=True))
        return 0
    if args.command == "describe-metadata":
        print(json.dumps(read_metadata(Path(args.path)), indent=2, sort_keys=True))
        return 0
    print(json.dumps(describe(Path(args.path)), indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
