# aria2-rust

Clean-room reimplementation of [aria2](https://github.com/aria2/aria2) in **safe Rust** (`#![forbid(unsafe_code)]`).

This is **not** feature-complete. After a sandbox wipe the previous ~97-turn tree was lost (never pushed). This repository is the new source of truth — every working slice is committed here.

## What works now

- HTTP GET, including segmented `Range` downloads
- Dest-match tests (`cargo test`)
- JSON-RPC: `addUri`, `tellStatus`, `tellActive`/`Waiting`/`Stopped`, `getVersion`, `getGlobalStat`, `remove`, `shutdown`, `system.listMethods` / `multicall`
- CLI: `--dir` `--out` `--split` `--piece-length` `--continue` `--enable-rpc` …

## Not yet (honest)

FTP, SFTP, BitTorrent, Metalink, DHT, most of the 214 C++ options. See `FEATURE_MATRIX.md`. Measure is C++ `prefs.h` + `RpcMethodFactory.cc` (250 boxes). Tick only production paths covered by tests.

## Build

```
CARGO_TARGET_DIR=/tmp/aria2-rust-target cargo test
CARGO_TARGET_DIR=/tmp/aria2-rust-target cargo build --release
```

License: GPL-2.0-or-later (same as aria2).

Library consumers that only need HTTP can set `default-features = false` to omit the optional `sftp` feature and its SSH dependencies. Default builds retain SFTP; an SFTP request in a build without that feature fails explicitly instead of falling back to another protocol.

Windows uses offset-based file reads/writes and Tokio's vectored socket writer.
POSIX-only `rlimit-nofile`, DSCP socket options and `file-allocation=falloc`
fail explicitly when requested on Windows. Default options leave these settings
unchanged. `file-allocation=prealloc` remains available.
