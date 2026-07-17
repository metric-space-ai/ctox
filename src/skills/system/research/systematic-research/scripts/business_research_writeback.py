#!/usr/bin/env python3
"""Retired legacy Business OS research writeback entry point.

Research output must be admitted by the native Business OS command path. This
module intentionally has no database, HTTP, process-launch, or file-write code.
Keeping the old command name as a fail-closed tombstone makes stale runbooks
and queued agent instructions harmless while operators migrate to
``ctx.commandBus.dispatch`` and the native RxDB projection.
"""

from __future__ import annotations

import sys


RETIREMENT_MESSAGE = (
    "business_research_writeback.py is retired: direct PostgreSQL/HTTP writeback "
    "is disabled; submit research through the native Business OS command path "
    "after evidence_guard receipt validation"
)


def main() -> int:
    raise SystemExit(RETIREMENT_MESSAGE)


if __name__ == "__main__":
    sys.exit(main())
