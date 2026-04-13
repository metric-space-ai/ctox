#!/usr/bin/env python3
import argparse
import json


LAYER_ORDER = [
    "service_process",
    "listener",
    "http",
    "authenticated_api",
    "admin_identity",
    "mutating_smoke",
    "persistence",
]

LAYER_INDEX = {layer: index for index, layer in enumerate(LAYER_ORDER)}

PROFILE_MINIMUM_LAYER = {
    "process_only": "service_process",
    "network_service": "listener",
    "read_only_service": "http",
    "operator_managed": "authenticated_api",
    "admin_managed": "admin_identity",
    "safe_mutation": "mutating_smoke",
    "durable_mutation": "persistence",
}


def load_checks(path: str) -> list[dict]:
    with open(path, "r", encoding="utf-8") as handle:
        payload = json.load(handle)
    if not isinstance(payload, list):
        raise SystemExit("checks json must be a list")
    return payload


def resolve_minimum_layer(required_profiles: list[str], explicit_minimum_layer: str | None) -> str | None:
    requested = []
    for profile in required_profiles:
        minimum = PROFILE_MINIMUM_LAYER.get(profile)
        if minimum is None:
            raise SystemExit(f"unknown required profile: {profile}")
        requested.append(minimum)
    if explicit_minimum_layer:
        if explicit_minimum_layer not in LAYER_INDEX:
            raise SystemExit(f"unknown minimum layer: {explicit_minimum_layer}")
        requested.append(explicit_minimum_layer)
    if not requested:
        return None
    return max(requested, key=lambda layer: LAYER_INDEX[layer])


def summarize(
    checks: list[dict],
    *,
    required_profiles: list[str] | None = None,
    minimum_layer: str | None = None,
) -> dict:
    required_profiles = required_profiles or []
    required_minimum_layer = resolve_minimum_layer(required_profiles, minimum_layer)
    passed = []
    failed = None
    normalized = {item.get("layer"): item for item in checks if item.get("layer")}
    for layer in LAYER_ORDER:
        item = normalized.get(layer)
        if item is None:
            continue
        ok = bool(item.get("ok"))
        if ok and failed is None:
            passed.append(layer)
            continue
        if not ok and failed is None:
            failed = {
                "layer": layer,
                "cause": item.get("cause", "unknown"),
                "detail": item.get("detail", ""),
            }
            break
    highest_passed_layer = passed[-1] if passed else None
    if failed is None and required_minimum_layer is not None:
        highest_index = LAYER_INDEX[highest_passed_layer] if highest_passed_layer else -1
        required_index = LAYER_INDEX[required_minimum_layer]
        if highest_index < required_index:
            failed = {
                "layer": required_minimum_layer,
                "cause": "verification_incomplete",
                "detail": (
                    f"required minimum layer {required_minimum_layer} not proven; "
                    f"highest passed layer was {highest_passed_layer or 'none'}"
                ),
            }
    if failed is None:
        return {
            "state": "executed",
            "passed_layers": passed,
            "highest_passed_layer": highest_passed_layer,
            "required_minimum_layer": required_minimum_layer,
            "failed_layer": None,
        }
    return {
        "state": "needs_repair" if passed else "blocked",
        "passed_layers": passed,
        "highest_passed_layer": highest_passed_layer,
        "required_minimum_layer": required_minimum_layer,
        "failed_layer": failed,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Summarize layered service acceptance checks.")
    parser.add_argument("--checks-json", required=True)
    parser.add_argument("--required-profile", action="append", default=[])
    parser.add_argument("--minimum-layer")
    args = parser.parse_args()
    print(
        json.dumps(
            summarize(
                load_checks(args.checks_json),
                required_profiles=args.required_profile,
                minimum_layer=args.minimum_layer,
            ),
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
