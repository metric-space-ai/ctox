//! Sliding ring buffer of captured target features.
//!
//! During each verify step the target forward captures its post-layer
//! hidden states at a small set of layer indices (see
//! [`crate::models::dflash_draft::capture::FeatureCapture`]). The
//! draft's next step consumes the last `ctx_len` of these stacked
//! feature vectors as its cross-attention KV context — it does NOT
//! need the full history, only a recent window.
//!
//! The reference implementation caps this at 4096 slots (one slot per
//! committed target token) — same 128K-context-via-sliding-ring trick
//! described in `dflash/README.md`. For our first CTOX port we use
//! the same cap and the same wrap semantics (ring indexed by
//! `position % capacity`) to stay drop-in compatible with the trained
//! draft.

use candle_core::{Device, Result, Tensor};

/// Default capacity of the feature ring, in committed tokens. Matches
/// `DFLASH27B_TARGET_FEAT_RING_CAP` in the reference's `internal.h`.
pub const DEFAULT_RING_CAP: usize = 4096;

/// A sliding ring of captured target features.
///
/// Each slot holds ONE token's stack of per-layer hidden states,
/// concatenated along the feature dim. For Qwen3.5-27B with
/// `target_layer_ids.len() == 5` and `hidden_size == 5120` that's a
/// `[5 * 5120] = [25600]`-element vector per slot.
///
/// The ring is device-resident (lives on the target's CUDA device) so
/// the draft can read it without H→D copies every step. We pre-
/// allocate one contiguous tensor of shape
/// `[capacity, target_stack_count * hidden]` and update it slot by
/// slot via `scatter_add`-style writes.
///
/// Commits land in writer-position order: each committed target token
/// advances the write head by 1; draft reads the last `ctx_len` slots
/// starting from the tail backwards.
pub struct TargetFeatureRing {
    /// Ring storage: `[capacity, fused_feature_dim]` on the target
    /// device, same dtype as the captured tensors (BF16 in practice).
    storage: Tensor,
    /// Number of slots allocated. Must be a power of two for the
    /// `pos & (cap - 1)` masking used at read time (we also fall back
    /// to modulo if this invariant is relaxed in the future; it's not
    /// a correctness requirement, just a micro-optimisation).
    capacity: usize,
    /// Concatenated feature dim per slot — captured together so we
    /// can shape-check on every write.
    fused_feature_dim: usize,
    /// Number of tokens that have been committed so far. Wraps around
    /// the ring when `pos >= capacity`; never decreases.
    commit_pos: usize,
}

impl TargetFeatureRing {
    /// Allocate a new ring on `device` with `capacity` slots and per-
    /// slot feature dim `fused_feature_dim`.
    pub fn new(
        device: &Device,
        capacity: usize,
        fused_feature_dim: usize,
        dtype: candle_core::DType,
    ) -> Result<Self> {
        if capacity == 0 {
            candle_core::bail!("TargetFeatureRing: capacity must be > 0");
        }
        if fused_feature_dim == 0 {
            candle_core::bail!("TargetFeatureRing: fused_feature_dim must be > 0");
        }
        let storage = Tensor::zeros((capacity, fused_feature_dim), dtype, device)?;
        Ok(Self {
            storage,
            capacity,
            fused_feature_dim,
            commit_pos: 0,
        })
    }

    /// Number of slots physically allocated.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Per-slot feature dimension.
    pub fn fused_feature_dim(&self) -> usize {
        self.fused_feature_dim
    }

    /// Total number of tokens committed to the ring so far (monotonic;
    /// can exceed `capacity` — the modular write head does the wrap).
    pub fn commit_pos(&self) -> usize {
        self.commit_pos
    }

    /// Current number of valid tokens in the ring. Either
    /// `commit_pos` (if the ring hasn't wrapped yet) or `capacity`.
    pub fn len(&self) -> usize {
        self.commit_pos.min(self.capacity)
    }

    pub fn is_empty(&self) -> bool {
        self.commit_pos == 0
    }

    /// Return the ring's current valid context as a contiguous tensor
    /// `[ctx_len, fused_feature_dim]`, ordered oldest → newest.
    ///
    /// If the ring hasn't wrapped yet this is just a narrow into the
    /// storage. After it wraps we return `[tail_slice; head_slice]`
    /// concatenated so the caller gets a contiguous time-ordered view.
    ///
    /// `ctx_len` caps at [`Self::len`] — passing a larger value
    /// returns only what's available. Pass 0 to get an empty tensor
    /// with the right feature dim and dtype.
    pub fn window(&self, ctx_len: usize) -> Result<Tensor> {
        let valid = self.len();
        let ctx_len = ctx_len.min(valid);
        if ctx_len == 0 {
            return Tensor::zeros(
                (0, self.fused_feature_dim),
                self.storage.dtype(),
                self.storage.device(),
            );
        }
        if self.commit_pos <= self.capacity {
            // Ring not yet wrapped: oldest valid slot is 0.
            let start = self.commit_pos - ctx_len;
            return self.storage.narrow(0, start, ctx_len);
        }
        // Ring has wrapped. Write head is at `commit_pos % capacity`;
        // oldest valid slot is at `commit_pos % capacity` (i.e. the
        // next slot to be overwritten). We want the LAST `ctx_len`
        // entries, so walk back from the write head.
        let head = self.commit_pos % self.capacity;
        if ctx_len <= head {
            // All `ctx_len` entries live before the head, contiguous.
            return self.storage.narrow(0, head - ctx_len, ctx_len);
        }
        // Spans the wrap. Stitch [tail_prefix; head_prefix].
        let head_part = ctx_len - head;
        let tail_start = self.capacity - head_part;
        let tail = self.storage.narrow(0, tail_start, head_part)?;
        let head_slice = self.storage.narrow(0, 0, head)?;
        Tensor::cat(&[&tail, &head_slice], 0)
    }

    /// Append `n` new committed-token feature rows to the ring.
    ///
    /// `features` must be shape `[n, fused_feature_dim]`, same device
    /// and dtype as the ring. We write slot-by-slot (modulo
    /// `capacity`); if `n >= capacity` we retain only the last
    /// `capacity` rows and fast-forward `commit_pos` accordingly.
    ///
    /// The caller is expected to have already concatenated the
    /// per-layer captured tensors (one per entry in the draft's
    /// `target_layer_ids`) along the feature dim before calling this.
    pub fn append(&mut self, features: &Tensor) -> Result<()> {
        let (n, d) = features.dims2()?;
        if d != self.fused_feature_dim {
            candle_core::bail!(
                "TargetFeatureRing::append: got feature dim {d}, expected {}",
                self.fused_feature_dim
            );
        }
        if !features.device().same_device(self.storage.device()) {
            candle_core::bail!(
                "TargetFeatureRing::append: features device mismatch (got {:?}, ring is {:?})",
                features.device().location(),
                self.storage.device().location()
            );
        }
        if features.dtype() != self.storage.dtype() {
            candle_core::bail!(
                "TargetFeatureRing::append: dtype mismatch (got {:?}, ring is {:?})",
                features.dtype(),
                self.storage.dtype()
            );
        }
        if n == 0 {
            return Ok(());
        }
        // If the incoming chunk is larger than the ring we only keep
        // its tail — earlier rows would be overwritten anyway. This is
        // rare (prefill of >ring_cap tokens on the first step) but we
        // handle it explicitly to avoid a scatter that silently drops
        // the wrap.
        let (features, skip) = if n > self.capacity {
            let keep = self.capacity;
            (features.narrow(0, n - keep, keep)?, n - keep)
        } else {
            (features.clone(), 0)
        };
        let n = features.dim(0)?;
        let head = self.commit_pos % self.capacity;
        if head + n <= self.capacity {
            // Contiguous write — one slice_assign call.
            self.storage = self.storage.slice_assign(&[head..head + n, 0..self.fused_feature_dim], &features)?;
        } else {
            // Wrap: write [head..cap] then [0..tail_len].
            let first = self.capacity - head;
            let f_first = features.narrow(0, 0, first)?;
            let f_second = features.narrow(0, first, n - first)?;
            self.storage = self.storage.slice_assign(
                &[head..self.capacity, 0..self.fused_feature_dim],
                &f_first,
            )?;
            self.storage = self.storage.slice_assign(
                &[0..n - first, 0..self.fused_feature_dim],
                &f_second,
            )?;
        }
        self.commit_pos += skip + n;
        Ok(())
    }

    /// Rewind the ring by `n` tokens. Used by the pipeline when a
    /// verify step rejects the tail of a block — the draft should not
    /// condition on features that were written but then invalidated.
    ///
    /// This only moves the write head; it does not zero out the
    /// rejected slots because they will be overwritten on the next
    /// `append`. Safe to call with `n == 0`.
    pub fn rewind(&mut self, n: usize) {
        self.commit_pos = self.commit_pos.saturating_sub(n);
    }

    /// Reset the ring to empty. Used by the DFlash pipeline between
    /// requests: without it, each new prefill's prompt features land
    /// on top of the previous request's generation features, and the
    /// draft's cross-attention attends to stale context from the old
    /// conversation — which in practice biases the next response to
    /// continue the previous topic. Cheap: only moves the write head;
    /// the storage tensor is overwritten in place by subsequent
    /// appends.
    pub fn reset(&mut self) {
        self.commit_pos = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::{DType, Device};

    fn make(device: &Device, cap: usize, dim: usize) -> TargetFeatureRing {
        TargetFeatureRing::new(device, cap, dim, DType::F32).unwrap()
    }

    #[test]
    fn empty_window_returns_zero_rows() {
        let dev = Device::Cpu;
        let ring = make(&dev, 8, 4);
        let w = ring.window(3).unwrap();
        assert_eq!(w.dims(), &[0, 4]);
    }

    #[test]
    fn append_within_capacity_no_wrap() {
        let dev = Device::Cpu;
        let mut ring = make(&dev, 8, 4);
        let features = Tensor::arange(0f32, 12.0, &dev).unwrap().reshape((3, 4)).unwrap();
        ring.append(&features).unwrap();
        assert_eq!(ring.len(), 3);
        let w = ring.window(3).unwrap();
        assert_eq!(w.dims(), &[3, 4]);
    }

    #[test]
    fn append_wraps_around() {
        let dev = Device::Cpu;
        let mut ring = make(&dev, 4, 2);
        let features = Tensor::arange(0f32, 12.0, &dev).unwrap().reshape((6, 2)).unwrap();
        ring.append(&features).unwrap();
        // 6 entries into a 4-slot ring: last 4 should be visible.
        assert_eq!(ring.len(), 4);
        assert_eq!(ring.commit_pos(), 6);
        let w = ring.window(4).unwrap();
        assert_eq!(w.dims(), &[4, 2]);
        // Expect values [4..12) contiguous (rows 2..=5 of original input).
        let got: Vec<f32> = w.flatten_all().unwrap().to_vec1().unwrap();
        assert_eq!(got, vec![4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0]);
    }

    #[test]
    fn rewind_reduces_len() {
        let dev = Device::Cpu;
        let mut ring = make(&dev, 8, 2);
        let features = Tensor::arange(0f32, 10.0, &dev).unwrap().reshape((5, 2)).unwrap();
        ring.append(&features).unwrap();
        assert_eq!(ring.len(), 5);
        ring.rewind(2);
        assert_eq!(ring.len(), 3);
        let w = ring.window(3).unwrap();
        assert_eq!(w.dims(), &[3, 2]);
    }

    #[test]
    fn oversize_append_retains_tail() {
        let dev = Device::Cpu;
        let mut ring = make(&dev, 4, 2);
        let features = Tensor::arange(0f32, 20.0, &dev).unwrap().reshape((10, 2)).unwrap();
        ring.append(&features).unwrap();
        assert_eq!(ring.commit_pos(), 10);
        assert_eq!(ring.len(), 4);
        let w = ring.window(4).unwrap();
        let got: Vec<f32> = w.flatten_all().unwrap().to_vec1().unwrap();
        // Last 4 of 10 rows = rows 6..=9 = values [12..20)
        assert_eq!(got, vec![12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 19.0]);
    }
}
