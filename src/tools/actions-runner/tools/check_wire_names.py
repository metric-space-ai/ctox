"""Sweep: every hand-transcribed wire name against bollard's generated model.

The `#[serde(rename = "...")]` attributes in `docker_api`, `docker_opts`,
`docker_opts_types` and `docker_opts_mounts` were written by hand from moby's
struct tags. A wrong one is invisible — serde drops an unknown key and the
daemon applies a default — so this compares each of them against the OpenAPI
-derived model in `bollard-stubs` and prints every key the daemon would not
recognise.

`bollard-stubs` is the authority rather than moby's Go source because it is
generated from the same document the daemon is built from; where the two
disagree, the daemon is what matters.

Run from the crate root:

    python3 tools/check_wire_names.py
"""

import pathlib
import re
import sys

STUB = pathlib.Path(
    "/Users/michaelwelsch/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f"
    "/bollard-stubs-1.53.1-rc.29.3.1/src/models.rs"
)

# The crate's struct -> the stub struct that models the same JSON object. In Go
# `Config` is inlined into the create body and `Resources` into `HostConfig`, so
# those two have no model of their own under their own name.
MAP = {
    "Config": "ContainerCreateBody",
    "Resources": "HostConfig",       # flattened into HostConfig
    "DeviceMapping": "DeviceMapping",
    "WeightDevice": "ResourcesBlkioWeightDevice",
    "ThrottleDevice": "ThrottleDevice",
    "LogConfig": "HostConfigLogConfig",
    "Ulimit": "ResourcesUlimits",
    "PortBinding": "PortBinding",
    "HealthConfig": "HealthConfig",
    "RestartPolicy": "RestartPolicy",
    "DeviceRequest": "DeviceRequest",
    # The mount module's option families, which the generator prefixes with
    # `Mount` and this crate's `mod mount` does not.
    "BindOptions": "MountBindOptions",
    "VolumeOptions": "MountVolumeOptions",
    "ImageOptions": "MountImageOptions",
    "TmpfsOptions": "MountTmpfsOptions",
    "Driver": "MountVolumeOptionsDriverConfig",
}

SOURCES = [
    "src/container/docker_api.rs",
    "src/container/docker_opts.rs",
    "src/container/docker_opts_types.rs",
    "src/container/docker_opts_mounts.rs",
]

RENAMED = re.compile(
    r'#\[serde\(rename = "(?P<wire>[^"]+)"\)\]\n'
    r'(?:[ \t]*#\[serde\([^\)]*\)\]\n)*'
    r'[ \t]*pub (?P<rust>\w+):'
)
STRUCT = re.compile(r'^(?P<indent>[ \t]*)pub struct (?P<name>\w+)\s*\{', re.M)


def struct_body(text: str, match: re.Match) -> str:
    """The braces of one struct, honouring its indentation.

    `text.find("\\n}")` is wrong for the types inside `mod mount`, which close
    with an indented brace — and a wrong end silently runs one struct's field
    names into the next one's, which reads as dozens of false mismatches.
    """
    close = re.compile(rf'^{re.escape(match.group("indent"))}\}}', re.M).search(text, match.end())
    if close is None:
        return ""
    return text[match.end():close.start()]


def renames(text: str) -> dict:
    """Every `pub struct` in `text` with its hand-written wire names."""
    found = {}
    for match in STRUCT.finditer(text):
        names = {m.group("rust"): m.group("wire") for m in RENAMED.finditer(struct_body(text, match))}
        if names:
            found[match.group("name")] = names
    return found


def stub_names(stub: str, struct: str) -> dict | None:
    marker = f"pub struct {struct} "
    if marker not in stub:
        marker = f"pub struct {struct}\n"
        if marker not in stub:
            return None
    start = stub.index(marker)
    end = stub.index("\n}\n", start)
    out = {}
    # The generated models put a doc comment between the attributes and the
    # field, which is what a naive attribute-adjacency match trips over.
    pattern = re.compile(
        r'#\[serde\(rename = "([^"]+)"\)\][^\n]*\n'
        r'(?:[ \t]*(?:#\[|///|pub )[^\n]*\n)*?[ \t]*pub (\w+):'
    )
    for match in pattern.finditer(stub[start:end]):
        out[match.group(1)] = match.group(2)
    return out


# Names this crate writes that bollard's generated model has no field for, with
# the reason each is harmless. Anything added here is a claim that has to stay
# true, so each one says why the daemon cannot tell the difference.
ACCEPTED = {
    # `mount.ClusterOptions` is an intentionally empty struct upstream, so the
    # daemon reads `{}` and an absent key the same way. The OpenAPI generator
    # dropped the field entirely. `MountOpt::set` has no `cluster-` key either
    # — `--mount type=cluster,cluster=x` fails client-side with `unknown option
    # 'cluster'` — so the value is never populated on this path at all.
    ("Mount", "cluster_options"),
}


def main() -> int:
    stub = STUB.read_text()
    mine = {}
    for path in SOURCES:
        for struct, names in renames(pathlib.Path(path).read_text()).items():
            mine.setdefault(struct, {}).update(names)

    bad = 0
    skipped = 0
    accepted = 0
    for struct, fields in sorted(mine.items()):
        target = MAP.get(struct, struct)
        theirs = stub_names(stub, target)
        if not theirs:
            print(f"  ? {struct}: no model in bollard-stubs; {len(fields)} names unchecked")
            skipped += 1
            continue
        for rust, wire in sorted(fields.items()):
            if wire in theirs:
                continue
            if (struct, rust) in ACCEPTED:
                accepted += 1
                continue
            print(f"  MISMATCH {struct}.{rust}: writes {wire!r}, {target} has no such key")
            bad += 1
    print(
        f"\n{len(mine)} structs checked, {bad} mismatches, "
        f"{accepted} accepted with a documented reason, {skipped} without a model"
    )
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
