# One pinned release recorder diagnosis

This branch adds a manual mode to the already registered Deployment Upgrade
Proof workflow. Dispatch `deployment-upgrade-proof.yml` on this branch with
`recorder_diagnostic=true`. Only the diagnostic job runs; the normal upgrade
mode remains the default. This is not a required-check bypass or release proof.

The probe downloads artifact10620151316 from run35554698322, validates ZIP,
archive and binary SHA256, and extracts only the regular `./bin/ctox` member.
It applies the independently reviewed verbose/stderr-tail patch in the disposable
checkout, then runs the existing isolated context fixture exactly once. There
is no Cargo build, source dependency compilation, production connection or
deployment. Diagnostic tooling consists of binary Linux perf packages, the
existing pinned npm lock with lifecycle scripts disabled, and runner Chrome.

The job is bounded to15minutes and the single fixture to480seconds. Fixture
children are in an owned process group that is stopped on completion or error.
The retained output includes package/tool/script identities, kernel, perf package
version, full bounded native thread observations, nearest before/after recorder
inventories, verbose stderr, perf data and report, and fixture status. Failed
recordings remain failures; positive sample/report requirements are unchanged.

The release binary has a different build profile from the original debug
full-host failure. A successful probe cannot replace that fixture's acceptance,
the outstanding visual review, or verified native/signed-shell deployment.
Do not retry automatically, dispatch the normal upgrade mode, or merge this
diagnostic branch into the release branch merely to execute the probe.
