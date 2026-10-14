#!/bin/bash
# Origin: CTOX
# License: AGPL-3.0-only
set -eu
crate_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/cliproxyapi-module-scope-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT HUP INT TERM
fixture="$scratch/crate"
mkdir -p "$fixture/scripts/tests" "$scratch/bin" "$scratch/cache"
for f in run_strict_umbrella_gate.sh check_strict_umbrella_receipt.sh strict_go_module_scope.jq write_pinned_go_module_scope.sh; do
    cp "$crate_dir/scripts/$f" "$fixture/scripts/$f"
done
validator="$fixture/scripts/check_strict_umbrella_receipt.sh"
driver="$fixture/scripts/run_strict_umbrella_gate.sh"
printf '// isolated orchestration fixture\n' > "$fixture/fixture.rs"
printf '[package]\nname="fixture"\nversion="0.0.0"\n' > "$fixture/Cargo.toml"
printf '{}\n' > "$fixture/strict-credit-audit.json"
cat > "$fixture/scripts/run_cargo_test_gate.sh" <<'STUB'
#!/bin/bash
printf 'Fixture Rust test command; no compiler acceptance claimed\n'
STUB
cat > "$fixture/scripts/run_with_macos_cargo_metadata_guard.sh" <<'STUB'
#!/bin/bash
exec "$@"
STUB
cat > "$fixture/scripts/check_tracking.sh" <<'STUB'
#!/bin/bash
printf 'Fixture tracking command\n'
STUB
cat > "$fixture/scripts/build_dashboard.sh" <<'STUB'
#!/bin/bash
printf 'fixture dashboard\n' > "$1"
STUB
cat > "$scratch/bin/go" <<'STUB'
#!/bin/bash
set -eu
case "$1" in
    version) printf 'go version go1.26.0 fixture\n' ;;
    env) printf '%s\n' "$MODULE_SCOPE_FIXTURE_CACHE" ;;
    test)
        printf 'Fixture Go command in %s; no reference acceptance claimed\n' "$PWD"
        if [ -n "${MODULE_SCOPE_FIXTURE_MUTATE:-}" ]; then
            printf ' ' >> "$MODULE_SCOPE_FIXTURE_MUTATE"
        fi
        ;;
    *) exit 1 ;;
esac
STUB
for f in cargo rustc node; do
    printf '#!/bin/bash\nprintf "Fixture executable: %%s\\n" "$0 $*"\n' > "$scratch/bin/$f"
done
chmod +x "$fixture/scripts/"*.sh "$scratch/bin/"*
export MODULE_SCOPE_FIXTURE_CACHE="$scratch/cache"
export PATH="$scratch/bin:$PATH"
printf 'Orchestration fixtures use stub compilers; these are not real Go/Rust reference gates.\n'

hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}';
    else shasum -a 256 "$1" | awk '{print $1}'; fi
}

run_fixture() {
    count=$1
    repo="$scratch/repo-$count"
    out="$scratch/out-$count"
    mkdir -p "$repo"
    git -C "$repo" init -q
    git -C "$repo" config user.name "Go module scope fixture"
    git -C "$repo" config user.email "module-scope@invalid.example"
    git -C "$repo" config commit.gpgSign false
    git -C "$repo" config core.hooksPath /dev/null
    printf 'module invalid.example/root\n' > "$repo/go.mod"
    i=1
    while [ "$i" -lt "$count" ]; do
        directory="examples/reference-$i"
        if [ "$i" -eq 1 ]; then directory='examples/Grüße module'; fi
        mkdir -p "$repo/$directory"
        printf 'module invalid.example/reference-%s\n' "$i" > "$repo/$directory/go.mod"
        i=$((i + 1))
    done
    git -C "$repo" add .
    git -C "$repo" commit -qm fixture
    pin=$(git -C "$repo" rev-parse HEAD)
    jq -n --arg pin "$pin" '{base_commit:$pin}' > "$fixture/upstream-lock.json"
    if ! "$driver" "$repo" "$out" "$scratch/target-$count" > "$scratch/run-$count.log" 2>&1; then
        cat "$scratch/run-$count.log" >&2
        exit 1
    fi
    jq -e --argjson count "$count" '
      .schema == "ctox.cliproxyapi.strict-umbrella-receipt.v3" and
      (.go_modules | length) == $count and (.go_hydration | length) == $count
    ' "$out/strict-umbrella-receipt.json" >/dev/null
    "$validator" "$out/strict-umbrella-receipt.json" "$pin" >/dev/null
    printf 'PASS generated exact %s-module receipt\n' "$count"
}

run_fixture 24
out24="$out"
pin24="$pin"
cp "$out24/strict-umbrella-receipt.json" "$scratch/good.json"
cp "$out24/pinned-go-modules.json" "$scratch/good-scope.json"
cp "$out24/hydrated-go-manifest.tsv" "$scratch/good-hydrated.tsv"
receipt="$out24/strict-umbrella-receipt.json"

reset_case() {
    cp "$scratch/good.json" "$receipt"
    cp "$scratch/good-scope.json" "$out24/pinned-go-modules.json"
    cp "$scratch/good-hydrated.tsv" "$out24/hydrated-go-manifest.tsv"
}
mutate_receipt() { jq "$1" "$receipt" > "$scratch/next.json"; mv "$scratch/next.json" "$receipt"; }
expect_rejected() {
    name=$1
    diagnostic=${2:-}
    if "$validator" "$receipt" "$pin24" > "$scratch/unexpected.log" 2>&1; then
        printf 'FAIL accepted %s\n' "$name" >&2; exit 1
    fi
    if [ -n "$diagnostic" ]; then grep -F "$diagnostic" "$scratch/unexpected.log" >/dev/null; fi
    printf 'PASS rejected %s\n' "$name"
}
reset_case
mutate_receipt '.go_modules = .go_modules[1:]'
expect_rejected 'missing offline module'
reset_case
mutate_receipt '.go_hydration = .go_hydration[1:]'
expect_rejected 'missing hydration module'
reset_case
mutate_receipt '.go_modules[0] = .go_modules[1]'
expect_rejected 'duplicate replacing another module'
reset_case
mutate_receipt '.go_modules[0].module = "unregistered/module" | .go_hydration[0].module = "unregistered/module"'
expect_rejected 'same-count identity substitution'
reset_case
mutate_receipt '.schema = "ctox.cliproxyapi.strict-umbrella-receipt.v2" | .go_modules = .go_modules[1:] | .go_hydration = .go_hydration[1:]'
expect_rejected 'schema downgrade hiding the 24th module' 'hydrated Go module scope mismatch'
reset_case
printf ' ' >> "$out24/pinned-go-modules.json"
expect_rejected 'pinned inventory hash tampering'
reset_case
jq '.upstream_commit = "0000000000000000000000000000000000000000"' "$scratch/good-scope.json" > "$out24/pinned-go-modules.json"
sha=$(hash_file "$out24/pinned-go-modules.json")
mutate_receipt ".go_module_scope_manifest_sha256 = \"$sha\""
expect_rejected 'inventory bound to a different pin'
reset_case
jq '.modules += [.modules[0]]' "$scratch/good-scope.json" > "$out24/pinned-go-modules.json"
sha=$(hash_file "$out24/pinned-go-modules.json")
mutate_receipt ".go_module_scope_manifest_sha256 = \"$sha\""
expect_rejected 'duplicate inventory module'
reset_case
printf '%064d\textra/untested/go.mod\n' 0 >> "$out24/hydrated-go-manifest.tsv"
sha=$(hash_file "$out24/hydrated-go-manifest.tsv")
mutate_receipt ".hydrated_go_manifest_sha256 = \"$sha\""
expect_rejected 'untested module in hydrated snapshot'
reset_case
printf '%064d\t../escape/go.mod\n' 0 >> "$out24/hydrated-go-manifest.tsv"
sha=$(hash_file "$out24/hydrated-go-manifest.tsv")
mutate_receipt ".hydrated_go_manifest_sha256 = \"$sha\""
expect_rejected 'unsafe hydrated module path'
reset_case
cat "$scratch/good-hydrated.tsv" >> "$out24/hydrated-go-manifest.tsv"
sha=$(hash_file "$out24/hydrated-go-manifest.tsv")
mutate_receipt ".hydrated_go_manifest_sha256 = \"$sha\""
expect_rejected 'duplicate hydrated module path'
reset_case
mutate_receipt 'del(.go_module_scope_manifest)'
expect_rejected 'missing v3 inventory'
reset_case
log=$(jq -r '.go_modules[0].log' "$receipt")
cp "$out24/$log" "$scratch/good-log"
printf 'tampered fixture log\n' > "$out24/$log"
expect_rejected 'test log hash tampering'
cp "$scratch/good-log" "$out24/$log"
reset_case

run_fixture 23
legacy="$out/strict-umbrella-receipt.json"
jq '.schema = "ctox.cliproxyapi.strict-umbrella-receipt.v2" | del(.go_module_scope_manifest,.go_module_scope_manifest_sha256)' "$legacy" > "$out/legacy-v2.json"
"$validator" "$out/legacy-v2.json" "$pin" >/dev/null
printf 'PASS legacy v2 23-module contract\n'
printf 'Fixture historical outer-CTOX command\n' > "$out/outer-fixture.log"
sha=$(hash_file "$out/outer-fixture.log")
jq --arg sha "$sha" '.schema = "ctox.cliproxyapi.strict-umbrella-receipt.v1" |
 .gates += [{name:"outer_ctox_check",status:"passed",command:["fixture","outer-ctox"],log:"outer-fixture.log",output_sha256:$sha}]' "$out/legacy-v2.json" > "$out/legacy-v1.json"
"$validator" "$out/legacy-v1.json" "$pin" >/dev/null
printf 'PASS legacy v1 23-module and outer-CTOX contract\n'

scope_hash=$(hash_file "$out/pinned-go-modules.json")
if bash "$fixture/scripts/write_pinned_go_module_scope.sh" "$repo" "$pin" "$out/pinned-go-modules.json" > "$scratch/overwrite.log" 2>&1; then
    echo 'FAIL existing inventory overwritten' >&2; exit 1
fi
test "$scope_hash" = "$(hash_file "$out/pinned-go-modules.json")"
printf 'PASS inventory output cannot overwrite an existing receipt input\n'
if bash "$fixture/scripts/write_pinned_go_module_scope.sh" "$repo" HEAD "$scratch/mutable.json" > "$scratch/mutable.log" 2>&1; then
    echo 'FAIL mutable Git ref accepted' >&2; exit 1
fi
printf 'PASS immutable pin required\n'

export MODULE_SCOPE_FIXTURE_MUTATE="$scratch/out-mutated/pinned-go-modules.json"
if "$driver" "$repo" "$scratch/out-mutated" "$scratch/target-mutated" > "$scratch/mutated-run.log" 2>&1; then
    echo 'FAIL inventory mutated after pin capture was accepted' >&2; exit 1
fi
grep -F 'pinned Go module manifest hash mismatch' "$scratch/mutated-run.log" >/dev/null
printf 'PASS inventory remains hash-bound across gate execution\n'

unsafe=$'examples/unsafe\nmodule'
mkdir -p "$repo/$unsafe"
printf 'module invalid.example/unsafe\n' > "$repo/$unsafe/go.mod"
git -C "$repo" add .
git -C "$repo" commit -qm 'unsupported control character fixture'
unsafe_pin=$(git -C "$repo" rev-parse HEAD)
if bash "$fixture/scripts/write_pinned_go_module_scope.sh" "$repo" "$unsafe_pin" "$scratch/unsafe.json" > "$scratch/unsafe.log" 2>&1; then
    echo 'FAIL control-character module silently admitted' >&2; exit 1
fi
printf 'PASS unsupported path is rejected instead of omitted\n'
printf 'strict Go module scope orchestration tests passed\n'
