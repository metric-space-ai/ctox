# Bench: `chess-best-move`

Terminal-Bench-2 task. Reads a chess board image (`chess_board.png`),
white to move. Agent must write the best move for white to `/app/move.txt`
in `[src][dst]` format (e.g. `e2e4`).

- Image: `alexgshaw/chess-best-move:20251031`
- Task timeout: `900s` (run with `--agent-timeout-multiplier 3` → 2700s)
- Verifier: 1 test (file contains the canonical winning move)

## Results

| Model | Reward | Turns | tok_in/out | Reply | Pass? |
|---|---|---|---|---|---|
| gpt-5.4-mini  | **1.0** | 1 | 65 / 11   | `Done — the move file contains:\ne2e4\ng2g4` | ✅ |
| gpt-5.4-nano  | 0.0 | 2 | 245 / 1   | `d2f4` (wrong move) | model fail |
| MiniMax-M2.7  | 0.0 | 2 | 268 / 374 | `c1g5` (Bg5 — pinning Black's knight) | model fail |

## CTOX integration assessment

**No CTOX bug on this task.** All three runs:

- ran end-to-end without crashes / hangs / orchestration errors
- produced a `trajectory.json` with proper ATIF
- counted `n_errors=0` (Harbor saw no exceptions)
- the agent actually wrote `/app/move.txt` (the verifier wouldn't have
  found a file to read otherwise)

The two failures are **model-side**: nano and M2.7 either misread the
position or chose suboptimal moves. M2.7 in particular did real chess
analysis ("pinning Black's knight to the king and attacking their most
developed piece") and wrote a plausible move that wasn't the verifier's
canonical answer.

## Mid-work fix (commit `f062a63`) validated here

Both nano and M2.7 ran **two turns** rather than the previous-buggy
single turn. The mid-work continuation kicked in after the first turn
ended in an intent statement, and the second turn produced the actual
file write. Without this fix nano would have terminated at `tok_out=1`
with no output and counted as a CTOX failure, not a model failure.

## Verdict per user's bench-acceptance rules

- gpt-5.4-mini: passed (no need to run gpt-5.4 quality)
- gpt-5.4-nano: legitimate model fail (wrong move written, no CTOX issue)
- MiniMax-M2.7: legitimate model fail (wrong move written, no CTOX issue)

No CTOX fixes needed for this task.

## Notes on M2.7 vision

MiniMax docs say "Images / documents are not supported on either
endpoint" — but M2.7 here clearly engaged with the position (described
piece development) and produced a chess-shaped move. Likely the agent
relied on tool calls (Python/PIL) inside the container to inspect the
PNG and feed back text descriptions, rather than the model itself
ingesting image bytes. CTOX' tool path is doing its job.
