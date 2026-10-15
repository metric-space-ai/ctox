# Upstream v8.0.28 dependency and release inputs

This is a partial input review, not a promoted baseline or a completed semantic
review. The accepted pin remains `a88197f845c979132c8978ea223c6af05cc81536`.
The reviewed candidate is `0d18d3b365be000eeaee7de4615f9da596b2059d`.
Both immutable trees were fetched explicitly for development review.

## Dependency changes

The complete `go.mod` difference is:

| Input | Candidate change | Rust-port consequence |
| --- | --- | --- |
| Root module | `github.com/router-for-me/CLIProxyAPI/v7` becomes `/v8` | Classify actual import-token migrations conservatively; retain all other source differences for semantic review. |
| Go version | Remains `1.26.0` | No toolchain version bump in this delta. Reference tests still require the declared version. |
| `github.com/libp2p/zeroconf/v2` | New direct dependency, `v2.2.0` | Used by `internal/discovery/zeroconf.go`. Native discovery behavior, bounds and lifecycle still need their module review/port; no compatibility credit here. |
| `github.com/miekg/dns` | New indirect dependency, `v1.1.43` | Review with the discovery dependency; do not import a Go networking runtime into CTOX. |
| `golang.org/x/text` | Same `v0.40.0`, changes from indirect to direct | Imported by `internal/api/handlers/management/plugin_quota.go`; its Rust semantic comparison remains open. |

`go.sum` adds 15 lines and removes none: checksums for the two new modules and
historical transitive `go.mod` inputs. Those extra checksum entries do not
replace the selected versions in `go.mod`. Rust dependencies are not changed by
this review; their audit and full-build gates remain required.

The candidate has **24 Go modules**, compared with 23 at the accepted pin. The
new module is `examples/realtime-openai-go/go.mod`. The full reference gate must
cover it when the pin advances; the existing fixed count of 23 is not sufficient
for that candidate. This review does not mark those tests passed.

## Release and licence boundaries

The release-workflow delta changes only the FreeBSD CGO build: it replaces the
`go-cross/cgo-actions` step with explicit clang/lld compilation against a
downloaded FreeBSD sysroot, with bounded mirror fallbacks and cleanup. CTOX's
Rust release pipeline remains the product build path. This Go-only release
change does not establish a new CTOX FreeBSD target or add runtime downloads.

The root MIT `LICENSE` blob is unchanged:
`e3305a12a6147f3eb16d4bf0057e89819892d626`, SHA-256
`879792e89cf1bdd6a8d446033ec87e30496f97dcafc4656dc53f641509b346a6`.
Licences and attribution of newly ported source/dependencies still require
review in their production modules.

## Reproducible source identities

| File | Accepted blob | Candidate blob |
| --- | --- | --- |
| `go.mod` | `1f5d12fb2b4bc274f0a02ed49a6513299dfb2618` | `145d668e2a7a23d5a99fd640ad52ee393f18b1b2` |
| `go.sum` | `3d3458d6060b7dab39eee87f09ac7ccf0d568a73` | `5b69fa6240a650b7ca28859ebc8b05609a26d5f6` |
| `.github/workflows/release.yaml` | `92941ebac6fba9b27375babfad207935d56c0508` | `7407a19d3e72fd35b0ba9fff172da002cb8576ea` |

Use `git diff <accepted> <candidate> -- go.mod go.sum .github/workflows/release.yaml`
and `git ls-tree -r --name-only <commit>` to reproduce the inputs and module list.
The separate import classifier described in `UPSTREAM.md` produces candidates
for review; it never accepts dispositions, moves anchors or grants gate credit.
