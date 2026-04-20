# Speculative Decoding vs Qwen3.6-35B-A3B — Status

Wiring is complete (TOML parse, loader wrap, both models load; verified via
`Wrapping first model's loader in SpeculativeLoader`). End-to-end
inference now runs without panicking but produces degenerate output
(see below).

## Fixed this session (committed)

1. **Downcast failure in VisionPipeline::forward_inputs** — `SpeculativePipeline::get_processor` returned the default `BasicProcessor` (text-only). Fix (f032d83): delegate to `target.get_processor()`.

2. **`other_config` missing in process_inputs calls** — all four call sites in `SpeculativePipeline::step` hard-coded `None`. Fix (b6c69cf + c97cdd8): pipe `self.get_input_processor_config()` through.

3. **paged-kv context-len underflow panic** — in the draft's per-iteration completion path. Fix (f10efd5): cap `num_blocks` at `ceil(context_len/block_size)` in `make_completion_chunk`'s paged-kv builder. This stops the panic; caller bug is still present (see #4).

## Open — known root cause, needs targeted debug session

4. **KV cache state desync in verify path — output degenerates**.

    Observable: first few tokens look plausible
    (`"The ocean is a vast and mysterious expanse of salt water..."`),
    then tokens start repeating
    (`"... covering covering covering covering ..."`). Throughput is also
    **slower** than the non-spec baseline (9.99 tok/s vs. 25.67 tok/s) —
    acceptance rate is near-zero and the draft forward is pure overhead.

    Suspected root causes (need to be walked with a debugger):

    - **Draft/target block-table divergence**: the draft's KV manager
      reserves a block range for the gamma chunk before `seq.add_tmp_tok`
      commits each token; when a draft token is rejected, the reserved
      tail must be released and the KV entries invalidated. The current
      code in `SpeculativePipeline::step` (around the `for i in 0..gamma`
      loop and the post-verify rejection branch) looks complete for
      Normal cache but may have gaps for the Hybrid cache path.

    - **Hybrid-cache recurrent-state rollback**: Qwen3.6 has 30 GatedDeltaNet
      layers with per-sequence `recurrent_state` that must be snapshotted
      before the target verify pass and restored on rejection. `SpeculativePipeline::step`
      does this (`target_recurrent_snapshot`), but the draft's own
      recurrent state (`draft_recurrent_slots` map) might not be rewound
      symmetrically.

    - **Position-ids / seqlen_offsets drift**: when spec rolls back N
      draft tokens, `seq.len()` should rewind and the next forward's
      position encoding must reflect that. Any off-by-one in the roll-
      back math → wrong RoPE → wrong logits → degenerate sampling.

    - **Tokenizer vocab parity vs router concentration**: even if all
      state is correct, a 0.8B dense Qwen3.5 draft predicting token
      distributions for a 35B MoE Qwen3.6 target will agree rarely enough
      that acceptance rates are low. Proper draft is either Qwen3.6's
      own smaller sibling (doesn't exist yet publicly) or a
      continuous-batching multi-request setup where target throughput
      wins on its own.

## Reproduction recipe

On the RTX A6000 host:

    # target + draft already downloaded to ~/.cache/huggingface
    bash /tmp/remote-spec.sh
    # uses /tmp/qwen_spec_test.toml with gamma=5

Current output snippet (for ctox commit f10efd5):

    The ocean is a vast and mysterious expanse of salt water that covers
    approximately  the Earth's surface, acting as the the Earth's life
    life, covering covering covering covering covering covering covering ...

Engine logs the `Wrapping` and `All models loaded` info lines; the
defensive cap in `make_completion_chunk` now logs a warn on first trigger.

## What would actually close this

1. Add per-step debug logging around `SpeculativePipeline::step` for
   each sequence: `seq.len()`, `block_table.len()`, `recurrent_state` slot
   version, `accepted_count`, `rejected_count`.
2. Run the repro with `RUST_LOG=engine_core::pipeline::speculative=trace`
   and capture the first 3 steps.
3. Compare to the ground-truth output of running the target alone —
   every token until divergence MUST match bit-exactly if spec decoding
   is implemented correctly.
4. The first token of divergence points at which cache (KV blocks,
   recurrent state, seqlen_offsets) is out of sync.

Estimated effort: one focused debugging session, probably 2–4 hours
with the right log trail.
