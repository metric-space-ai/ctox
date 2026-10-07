# SMB dependency compatibility patch

The `smb/` directory contains the published `smb` 0.12.1 crate from
https://crates.io/api/v1/crates/smb/0.12.1/download.
Archive SHA-256: `b60b8c4d8566c3cf19d17837958e67acc0534e68f331e898a642f4f57e9ecc4c`.
Upstream commit: `7c7d4a60c930e3780e95208967aa13b6346eaa25`, `crates/smb`,
https://github.com/afiffon/smb-rs.

The only change to the published crate is in normalized `Cargo.toml`:
`sspi = "=0.21.3"` becomes `sspi = "=0.23.0"`. Original `Cargo.toml.orig` and
all Rust sources remain unchanged. `LICENSE.md` is additionally copied verbatim
from that upstream commit because it was omitted from the published archive.
The MIT license and original copyright are retained in source and binary bundles.

SSPI 0.21.3 pins crypto prereleases that cannot coexist with current patched
russh. SSPI 0.23.0 and russh 0.64.1 share the stable crypto versions. This avoids
both the libssh2/OpenSSL versus BoringSSL collision and a downgrade to an affected
russh-cryptovec. The immutable aria2-rust snapshot is separate and unchanged.
Replace this local crate with an upstream registry release when it supports the
same compatible SSPI line. Validate both encrypted SMB3 and pinned SFTP fixtures
and the native daemon link when changing the dependency pair.
