# Changelog

All notable changes to CTOX are documented in this file, following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) conventions.
Security-relevant changes are always listed under a **Security** heading so
planned hardening is distinguishable from feature work.

## Versioning policy

- CTOX is pre-1.0: minor/patch tags (`v0.3.x`) may contain breaking changes;
  breaking changes are called out per release below.
- **Pin a tagged release.** `main` moves continuously and is not a supported
  deployment target; production and pilot installations should pin an exact
  tag and upgrade deliberately.
- Only the latest tagged release receives security fixes (see
  [SECURITY.md](SECURITY.md)).
- `1.0` will be declared when the stable-release criteria in
  [docs/business-adoption-readiness-plan.md](docs/business-adoption-readiness-plan.md)
  (P1-M1) are met — not before, and not for optics.

## [Unreleased]

### Fixed

- Service event stream no longer wedges permanently after a chat client
  disconnects mid-turn ([#21](https://github.com/metric-space-ai/ctox/issues/21)):
  turn-completion events are buffered instead of blocking the request path,
  `turn/interrupt` now reliably reaches a running turn, sessions tear down
  gracefully, and `ctox chat --wait` judges completion per conversation
  against the durable assistant outcome — empty replies and failure outcomes
  exit non-zero instead of reporting false success.
- Hardening pass on the #21 fix after adversarial review: request responses
  can no longer deadlock behind a saturated event buffer (runaway buffers
  fail the session explicitly), a timed-out `turn/start` is never retried
  (prevents duplicate turn execution), stale events from an interrupted turn
  can no longer surface as the next turn's reply, and `chat --wait` re-reads
  the durable outcome after the worker finishes so late failures decide the
  exit code.

### Added

- `SECURITY.md`: private vulnerability reporting channel, response targets,
  supported versions, and a summary of the security model.
- This changelog.

## Releases up to v0.3.31

Releases before this changelog was introduced are documented by their
[GitHub release notes](https://github.com/metric-space-ai/ctox/releases) and
the git history. From the next tagged release onward, every release gets an
entry here.
