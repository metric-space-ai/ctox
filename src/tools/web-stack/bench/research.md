# External Benchmark Scan

This note captures the external benchmark families used to shape the CTOX web
stack bench and the feature gaps they expose.

## Benchmarks Reviewed

### BrowseComp

Source: <https://openai.com/index/browsecomp/>

Why it matters for CTOX:

- It measures browse-time question answering, not just raw retrieval.
- The closest CTOX analog is `ctox_web_search` plus `ctox_web_read`.
- It pushes us to validate evidence synthesis, not only that a URL was fetched.

### Mind2Web and Online-Mind2Web

Source: <https://github.com/OSU-NLP-Group/Mind2Web>

Signals we should copy:

- broad website and domain coverage
- preserved raw traces, HARs, screenshots, and DOM snapshots
- explicit split between offline benchmark artifacts and live-web evaluation

Implication for CTOX:

- CTOX still lacks a browser-automation bench with trace artifacts.
- `ctox_browser_automation` is now module-owned, but not yet covered by a
  deterministic fixture case.

### WorkArena

Source: <https://servicenow.github.io/WorkArena/>

Signals we should copy:

- multimodal observations
- high-level browser actions
- shared gym-style evaluation across MiniWoB, WebArena, and WorkArena

Implication for CTOX:

- the current CTOX bench can only secure preparation and retrieval surfaces
  deterministically
- it still does not secure action-level browser execution quality

### WebArena-Verified

Source: <https://servicenow.github.io/webarena-verified/v1.2.3/>

Signals we should copy:

- structured agent response JSON
- deterministic offline evaluation from logged artifacts
- checksums and versioned task data for reproducibility

Implication for CTOX:

- our bench runner should emit machine-readable reports
- scrape and browser tasks need structured evaluators instead of ad hoc prose

### VisualWebArena

Source: <https://github.com/web-arena-x/visualwebarena>

Signals we should copy:

- execution-based evaluation for multimodal browser tasks
- explicit setup and test-data generation
- unit-testable environment bootstrapping

Implication for CTOX:

- visual and multimodal browser automation is currently a coverage gap
- browser preparation can be tested now, but browser task execution still lacks
  a dedicated regression tier

## Current CTOX Feature Gaps

The benchmark scan highlights four concrete gaps in the current CTOX web stack:

1. There is no CTOX-native task evaluator for multi-step browser tasks, trace
   replay, or HAR-based verification.
2. `ctox_web_scrape` has deterministic wrapper coverage, but no seeded semantic
   regression corpus for ranking quality.
3. The existing ignored Rust benchmark in
   `/Users/michaelwelsch/Dokumente - MacBook Air von Michael/Dokumente - MacBook Air von Michael/CTOX/tools/web-stack/src/web_search.rs`
   is useful for compatibility work, but too narrow to act as the primary web
   regression harness.
4. Browser automation still lacks deterministic bench coverage even after its
   runtime ownership moved into `tools/web-stack`.

## Bench Design Chosen Here

To address those gaps without reintroducing compile sprawl:

- fixture tier secures deterministic regressions for `search`, `read`, `scrape`
  wrapper behavior, and `browser-prepare`
- live tier checks a narrow set of stable public pages and PDFs
- benchmark cases are declared in JSON and executed by a standalone Python
  runner against a built `ctox` binary
- the runner isolates state through `CTOX_ROOT` so bench runs do not mutate the
  repository runtime state

## Next Gaps After This Bench

The next meaningful extension is a dedicated browser-automation bench with:

- micro-tasks inspired by MiniWoB and BrowserGym
- trace or HAR capture inspired by Mind2Web and WebArena-Verified
- task-level success scoring instead of raw stdout inspection
