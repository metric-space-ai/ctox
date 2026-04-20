//! MoE expert cache with LFU eviction and static pre-allocated tiered storage.
//!
//! The cache lets CTOX run MoE models whose total expert weights exceed the
//! GPU VRAM budget. Only `capacity` experts are kept resident on-device; the
//! rest live in a **statically pre-allocated** backing pool (RAM slab or
//! memory-mapped SSD file) and are swapped in on demand, evicting the
//! least-frequently-used resident expert.
//!
//! # Performance model
//!
//! The pool is allocated **once** at construction:
//!
//! * A single `Vec<u8>` of `num_experts * slot_size` bytes for the warm tier,
//!   OR a single `fd`+`mmap` pair of the same size for the cold tier.
//! * Each expert has a fixed slot at `offset = expert_idx * slot_size`; the
//!   slot is large enough for the model's maximum-sized expert, with per-slot
//!   metadata tracking the actual gate/up/down byte lengths.
//! * No per-eviction heap allocation, no per-eviction file creation, no per-
//!   eviction `fsync`. Tier transitions are a single `memcpy` (warm) or a
//!   single `pwrite_at` into an already-open, already-sized file (cold).
//! * Reads from the cold tier are zero-copy: `ReplicatedLayer::deserialize`
//!   is called on a borrowed slice of the `mmap`. The OS page cache absorbs
//!   repeated accesses.
//! * LFU counters use `AtomicU32` with periodic exponential decay so they
//!   stay meaningful over long inference runs.
//! * Hot-path hits take only a read lock + a pair of atomic increments — no
//!   writes to the shared state.
//!
//! # Topologies
//!
//! * [`Topology::Unified`] — Apple Silicon et al. GPU and CPU share one
//!   physical memory, so a separate "CPU RAM" tier would just double-count
//!   the same bytes. The pool is backed by SSD (or by RAM for tests /
//!   small models when no cold path is configured).
//! * [`Topology::Discrete`] — NVIDIA / AMD with dedicated VRAM. The pool
//!   prefers CPU RAM (fast) when it fits the model, else falls back to SSD.
//! * [`Topology::CpuOnly`] — no GPU backend compiled in. Cache still works:
//!   experts materialize on `Device::Cpu`, swap goes through the cold tier
//!   (SSD) or stays RAM-resident. Useful on memory-constrained hosts where
//!   even the full-resident model wouldn't fit.

use std::borrow::Cow;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use candle_core::{Device, Result};
use engine_quant::{Comm, QuantMethod, QuantizeOntoGuard, QuantizedSerde, ReplicatedLayer};
use memmap2::{MmapMut, MmapOptions};
use parking_lot::RwLock;

/// How often (in cache accesses) to decay the LFU counters. A decay halves
/// every counter, which prevents the early-active experts from accumulating
/// uncatchable leads over experts that only become hot later in the run.
const LFU_DECAY_INTERVAL: u32 = 4096;

/// Hardware topology relevant for cache tier planning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topology {
    /// Unified memory (Apple Silicon, Grace Hopper, etc.): VRAM and RAM are
    /// the same physical bytes. No distinct warm tier; non-resident experts
    /// go directly to the cold (SSD) tier or stay in RAM as a test fallback.
    Unified,
    /// Discrete GPU (NVIDIA / AMD): separate VRAM. The pool backing lives in
    /// CPU RAM by default (fast warm tier) or on SSD when memory is tight.
    Discrete,
    /// CPU-only execution. Cache is a pass-through; all experts resident.
    CpuOnly,
}

impl Topology {
    /// Detect topology from a candle device handle.
    pub fn detect(device: &Device) -> Self {
        match device {
            Device::Metal(_) => Self::Unified,
            Device::Cuda(_) => Self::Discrete,
            Device::Cpu => Self::CpuOnly,
        }
    }

    pub fn has_warm_tier(self) -> bool {
        matches!(self, Self::Discrete)
    }

    pub fn has_cold_tier(self) -> bool {
        !matches!(self, Self::CpuOnly)
    }
}

/// Three quantized linear layers making up one MoE expert: gate, up, down.
#[derive(Clone)]
pub struct ExpertTriple {
    pub gate: Arc<dyn QuantMethod>,
    pub up: Arc<dyn QuantMethod>,
    pub down: Arc<dyn QuantMethod>,
}

/// Per-expert metadata kept alongside the static pool. Records the actual
/// serialized length of each of the three layers so we know how much of the
/// fixed-size slot holds meaningful bytes.
#[derive(Debug, Clone, Copy)]
struct SlotMetadata {
    gate_len: u32,
    up_len: u32,
    down_len: u32,
    /// `true` once the slot has been populated at least once. Resident slots
    /// that have never been evicted start with this `false` and skip the
    /// pool read on their first eviction round.
    staged: bool,
}

impl SlotMetadata {
    const fn empty() -> Self {
        Self {
            gate_len: 0,
            up_len: 0,
            down_len: 0,
            staged: false,
        }
    }

    fn total_len(self) -> u64 {
        u64::from(self.gate_len) + u64::from(self.up_len) + u64::from(self.down_len)
    }
}

/// Backing storage for one expert's slot content.
///
/// There's exactly one `PoolBacking` per cache — either a RAM slab (warm
/// tier, fast) or a memory-mapped SSD file (cold tier, larger). Both are
/// fixed-size at construction and never reallocated.
enum PoolBacking {
    /// RAM slab: a single `Vec<u8>` of `slot_size * num_experts` bytes.
    /// Fastest materialization path. Used on discrete GPU with enough
    /// host RAM, or when no SSD path is configured.
    Ram(Vec<u8>),
    /// SSD-mapped slab: single file, sized once via `set_len`, with a
    /// writable mmap covering the whole range. Writes are in-place
    /// (`mmap[...].copy_from_slice`) with lazy flushing; reads are zero-
    /// copy slices of the mmap (the OS page cache serves them).
    Ssd {
        #[allow(dead_code)]
        file: File,
        mmap: MmapMut,
        #[allow(dead_code)]
        path: PathBuf,
    },
}

impl PoolBacking {
    fn slice(&self) -> &[u8] {
        match self {
            Self::Ram(v) => v,
            Self::Ssd { mmap, .. } => &mmap[..],
        }
    }

    fn slice_mut(&mut self) -> &mut [u8] {
        match self {
            Self::Ram(v) => v,
            Self::Ssd { mmap, .. } => &mut mmap[..],
        }
    }

    fn is_ssd(&self) -> bool {
        matches!(self, Self::Ssd { .. })
    }

    /// Flush pending writes to the underlying backing. For RAM this is a
    /// no-op; for SSD it flushes the mmap pages.
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Ram(_) => Ok(()),
            Self::Ssd { mmap, .. } => mmap.flush_async(),
        }
    }

    /// Hint the kernel to prefetch the given byte range. Only meaningful on
    /// SSD backing; RAM is already resident. Best-effort — ignore failures
    /// because prefetching is a performance hint, not a correctness
    /// requirement.
    fn advise_willneed(&self, offset: usize, len: usize) {
        if let Self::Ssd { mmap, .. } = self {
            let end = offset.saturating_add(len).min(mmap.len());
            if end > offset {
                let _ = mmap.advise_range(memmap2::Advice::WillNeed, offset, end - offset);
            }
        }
    }

    /// Hint the kernel that the given range is no longer needed in the page
    /// cache. The OS is free to reclaim those pages; a subsequent access
    /// will cold-load them again from disk.
    ///
    /// Used after promotion (we've pulled bytes out of the pool onto the
    /// device, so the CPU-side page cache copy is now redundant) to keep
    /// the page cache targeted at experts that are actively hot — prevents
    /// a 30-GB pool from squatting on all the available RAM for stale
    /// slots that may never be touched again this run.
    ///
    /// # Safety
    /// `MADV_DONTNEED` on a writable mmap can discard *unsaved* writes for
    /// the range. In this cache the invariant is that every write to a slot
    /// completes (and is flushed) *before* any reader observes the slot
    /// state change to `InPool`. Callers only invoke this on slots that are
    /// `InPool` and have been fully staged — no writer is active on the
    /// range, so no data is at risk. Wrapped in `unsafe` here because
    /// `memmap2` forces the caller to acknowledge the contract.
    fn advise_dontneed(&self, offset: usize, len: usize) {
        if let Self::Ssd { mmap, .. } = self {
            let end = offset.saturating_add(len).min(mmap.len());
            if end > offset {
                // SAFETY: slot is fully staged and not being written; see
                // function-level comment for the invariant.
                unsafe {
                    let _ = mmap.unchecked_advise_range(
                        memmap2::UncheckedAdvice::DontNeed,
                        offset,
                        end - offset,
                    );
                }
            }
        }
    }
}

/// Static pool: backing storage + per-expert metadata.
///
/// All fields are initialized once at [`MoEExpertCache::new`] and never
/// resized. Slot contents are mutated in place via `copy_from_slice`; slot
/// metadata updates happen under the cache's write lock.
pub struct StaticPool {
    slot_size: u64,
    num_experts: usize,
    metadata: Vec<SlotMetadata>,
    backing: PoolBacking,
}

impl StaticPool {
    fn new_ram(slot_size: u64, num_experts: usize) -> Self {
        let len = slot_size.saturating_mul(num_experts as u64) as usize;
        Self {
            slot_size,
            num_experts,
            metadata: vec![SlotMetadata::empty(); num_experts],
            backing: PoolBacking::Ram(vec![0u8; len]),
        }
    }

    fn new_ssd(slot_size: u64, num_experts: usize, path: PathBuf) -> Result<Self> {
        // `path` from the caller is the user-facing cold-tier *directory*
        // (e.g. `/tmp/moe_cache_ssd`). Each `MoEExpertCache` needs its own
        // file inside it so 40+ concurrent layer-load constructions don't
        // collide on one pool file. If the path isn't a directory yet, treat
        // it as the target file and use its parent for probing.
        let (backing_dir, file_path): (PathBuf, PathBuf) = if path.is_dir() || !path.exists() {
            std::fs::create_dir_all(&path).map_err(candle_core::Error::wrap)?;
            use std::sync::atomic::{AtomicUsize, Ordering};
            static POOL_COUNTER: AtomicUsize = AtomicUsize::new(0);
            let idx = POOL_COUNTER.fetch_add(1, Ordering::Relaxed);
            let fname = format!("pool-{}-{idx:03}.bin", std::process::id());
            (path.clone(), path.join(fname))
        } else {
            let parent = path.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
            (parent, path.clone())
        };
        std::fs::create_dir_all(&backing_dir).map_err(candle_core::Error::wrap)?;
        let total_len = slot_size.saturating_mul(num_experts as u64);
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&file_path)
            .map_err(candle_core::Error::wrap)?;
        file.set_len(total_len).map_err(candle_core::Error::wrap)?;

        let mmap = unsafe {
            MmapOptions::new()
                .len(total_len as usize)
                .map_mut(&file)
                .map_err(candle_core::Error::wrap)?
        };
        // Default the base advice to `Random`: MoE routing drives accesses in
        // a data-dependent, non-sequential pattern, so encouraging the OS to
        // stream-read adjacent pages would pollute the page cache with
        // experts that won't be touched. Per-access `WillNeed` hints (see
        // `prefetch_many`) take over the prefetch job explicitly.
        //
        // On Linux this maps to `madvise(MADV_RANDOM)` — disables read-ahead
        // on the range. On macOS it maps to `madvise(MADV_RANDOM)` with the
        // same effect. On both OSes, subsequent `WillNeed` on a specific
        // slot re-enables readahead for that slot's byte range only.
        let _ = mmap.advise(memmap2::Advice::Random);

        Ok(Self {
            slot_size,
            num_experts,
            metadata: vec![SlotMetadata::empty(); num_experts],
            backing: PoolBacking::Ssd {
                file,
                mmap,
                path: file_path,
            },
        })
    }

    fn slot_offset(&self, idx: usize) -> usize {
        (idx as u64 * self.slot_size) as usize
    }

    /// Stage an expert's serialized bytes into its fixed slot. Uses
    /// `copy_from_slice` (memcpy) — no allocation. Updates metadata.
    fn write_slot(&mut self, idx: usize, gate: &[u8], up: &[u8], down: &[u8]) -> Result<()> {
        let total = gate.len() + up.len() + down.len();
        if total as u64 > self.slot_size {
            candle_core::bail!(
                "static pool: expert {} serialized to {} bytes, exceeds slot_size {}",
                idx,
                total,
                self.slot_size
            );
        }
        let base = self.slot_offset(idx);
        let backing = self.backing.slice_mut();
        backing[base..base + gate.len()].copy_from_slice(gate);
        let up_start = base + gate.len();
        backing[up_start..up_start + up.len()].copy_from_slice(up);
        let down_start = up_start + up.len();
        backing[down_start..down_start + down.len()].copy_from_slice(down);

        self.metadata[idx] = SlotMetadata {
            gate_len: gate.len() as u32,
            up_len: up.len() as u32,
            down_len: down.len() as u32,
            staged: true,
        };
        Ok(())
    }

    /// Borrow the three layer byte ranges for expert `idx` from the pool.
    /// Zero-copy: returns slices into the RAM slab or mmap.
    fn read_slot(&self, idx: usize) -> Result<(&[u8], &[u8], &[u8])> {
        let meta = self.metadata[idx];
        if !meta.staged {
            candle_core::bail!("static pool: expert {} was never staged to the pool", idx);
        }
        let base = self.slot_offset(idx);
        let backing = self.backing.slice();
        let gate = &backing[base..base + meta.gate_len as usize];
        let up_start = base + meta.gate_len as usize;
        let up = &backing[up_start..up_start + meta.up_len as usize];
        let down_start = up_start + meta.up_len as usize;
        let down = &backing[down_start..down_start + meta.down_len as usize];
        Ok((gate, up, down))
    }
}

/// Configuration for constructing a cache.
pub struct CacheConfig {
    /// K: max resident experts on GPU.
    pub capacity: usize,
    /// Detected topology; controls which backing the pool uses by default.
    pub topology: Topology,
    /// GPU device to materialize resident experts onto.
    pub device: Device,
    /// Optional CPU-RAM budget for the warm tier. When `Some(b)` and the
    /// estimated pool exceeds `b`, construction falls through to the cold
    /// tier (if `cold_tier_path` is set) or fails.
    pub warm_tier_budget_bytes: Option<usize>,
    /// Optional SSD file path. When `Some`, the pool backs onto a single
    /// pre-sized file at this location rather than CPU RAM. Preferred when
    /// the serialized model is larger than the warm budget.
    pub cold_tier_path: Option<PathBuf>,
    /// Minimum sustained sequential-read floor (MiB/s) required from the
    /// cold-tier SSD. If the startup probe falls below this, construction
    /// fails rather than risk thrashing.
    pub cold_tier_min_mbps: u32,
}

#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub promotions: u64,
    pub decays: u64,
}

/// Atomic-backed stats so the hot-path hit does not need to take a lock.
#[derive(Debug, Default)]
struct AtomicStats {
    hits: AtomicU64,
    misses: AtomicU64,
    evictions: AtomicU64,
    promotions: AtomicU64,
    decays: AtomicU64,
}

impl AtomicStats {
    fn snapshot(&self) -> CacheStats {
        CacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
            promotions: self.promotions.load(Ordering::Relaxed),
            decays: self.decays.load(Ordering::Relaxed),
        }
    }
}

/// Current residency state of a slot. Resident slots hold the live on-device
/// triple; `InPool` slots' bytes live at `pool.slot_offset(idx)` and are
/// materialized on demand from there.
enum SlotState {
    Resident(ExpertTriple),
    InPool,
}

pub struct MoEExpertCache {
    num_experts: usize,
    cfg: CacheConfig,
    comm: Arc<Comm>,
    guard: QuantizeOntoGuard,

    /// Residency state + pool are protected by one `RwLock`. Cache hits take
    /// a read lock; misses upgrade to write for eviction + pool mutation.
    inner: RwLock<CacheInner>,

    /// LFU + recency counters are `Atomic`, so hit-path increments don't
    /// require any lock beyond the read guard.
    lfu_counts: Vec<AtomicU32>,
    last_access: Vec<AtomicU32>,
    clock: AtomicU32,
    accesses_since_decay: AtomicU32,

    /// Aggregated stats — all counters are atomic so the hit path never
    /// touches a lock. Snapshot via [`MoEExpertCache::stats`].
    stats: AtomicStats,
}

struct CacheInner {
    slots: Vec<SlotState>,
    pool: StaticPool,
}

impl MoEExpertCache {
    /// Build a cache from a fully-loaded expert set.
    ///
    /// All experts are serialized once via [`QuantMethod::serialize`] to
    /// determine the maximum slot size. A single RAM slab or SSD-backed
    /// file of `num_experts * slot_size` bytes is allocated. Initial
    /// `capacity` experts stay resident; the rest are staged into the pool
    /// and their on-device `Arc`s dropped so the VRAM is actually freed.
    ///
    /// # Errors
    ///
    /// Fails if:
    /// - An expert's [`QuantMethod`] does not implement `serialize`.
    /// - The SSD bandwidth probe falls below `cold_tier_min_mbps`.
    /// - The warm budget is set and would be exceeded, without a cold
    ///   fallback configured.
    pub fn new(experts: Vec<ExpertTriple>, cfg: CacheConfig, comm: Arc<Comm>) -> Result<Self> {
        let t_construct = Instant::now();
        if experts.is_empty() {
            candle_core::bail!("MoEExpertCache: experts vec is empty");
        }
        if cfg.capacity == 0 {
            candle_core::bail!("MoEExpertCache: capacity must be > 0");
        }
        // Previously: CpuOnly forced `capacity >= num_experts` (no swap
        // allowed on CPU). Removed — RAM-pressure on the host can force
        // swap-to-SSD even without a GPU, especially for a 35B-param MoE
        // on a consumer laptop. The cold tier (mmap'd SSD file) is still
        // fully functional on CPU-only topology. If no cold_tier_path is
        // configured and capacity < num_experts, the CpuOnly branch below
        // keeps the surplus experts as in-memory `ColdTier::InMemory` —
        // which doesn't save RAM but preserves correctness.

        let num_experts = experts.len();
        let capacity = cfg.capacity.min(num_experts);

        // Probe the cold tier once, before committing to use it.
        if let Some(cold_path) = &cfg.cold_tier_path {
            if cfg.cold_tier_min_mbps > 0 {
                match probe_ssd_bandwidth_mbps(cold_path) {
                    Some(observed) if observed >= cfg.cold_tier_min_mbps => {
                        tracing::info!(
                            "MoE cache cold-tier probe OK: {} MiB/s >= {} MiB/s required ({})",
                            observed,
                            cfg.cold_tier_min_mbps,
                            cold_path.display()
                        );
                    }
                    Some(observed) => {
                        candle_core::bail!(
                            "MoE cache cold-tier probe too slow: {} MiB/s < {} MiB/s required at {}",
                            observed,
                            cfg.cold_tier_min_mbps,
                            cold_path.display()
                        );
                    }
                    None => {
                        candle_core::bail!(
                            "MoE cache cold-tier probe failed at {} — path not usable",
                            cold_path.display()
                        );
                    }
                }
            }
        }

        // Determine `slot_size` from the *first* expert only. In MoE models
        // every expert is structurally identical (same hidden_size × moe_
        // intermediate_size), so the serialized footprint is the same for
        // every one of them. Serializing all N just to compute max was
        // wasted work — 255 × 3 redundant `serialize()` calls per layer,
        // which at ~100 ms/call scales to ~1 minute of pointless CPU per
        // layer on a 256-expert model.
        //
        // Implementation note: we do NOT keep the serialized bytes from
        // this probe. The second pass below re-serializes each expert just
        // before writing it to the pool, which is a single pass total
        // (vs. the previous two passes). The memory saved by dropping the
        // pre-serialized cache is significant on a 256×3 MoE layer —
        // hundreds of MiB at full-precision weights.
        let probe_triple = experts
            .first()
            .ok_or_else(|| candle_core::Error::msg("cache: empty experts"))?;
        let probe_total = probe_triple.gate.serialize()?.len() as u64
            + probe_triple.up.serialize()?.len() as u64
            + probe_triple.down.serialize()?.len() as u64;
        // 4 KiB page alignment for mmap-friendly slot boundaries.
        let slot_size = (probe_total + 4095) & !4095;

        // Decide the backing. Policy:
        //   - cold_tier_path set AND (Unified OR warm budget wouldn't fit) -> SSD
        //   - else                                                         -> RAM
        let needed_ram_bytes = slot_size as usize * num_experts;
        let use_ssd = cfg.cold_tier_path.is_some()
            && (matches!(cfg.topology, Topology::Unified)
                || matches!(cfg.warm_tier_budget_bytes, Some(b) if b < needed_ram_bytes));

        let mut pool = if use_ssd {
            StaticPool::new_ssd(slot_size, num_experts, cfg.cold_tier_path.clone().unwrap())?
        } else if let Some(b) = cfg.warm_tier_budget_bytes {
            if b < needed_ram_bytes && cfg.cold_tier_path.is_none() {
                candle_core::bail!(
                    "MoEExpertCache: pool would need {} bytes but warm budget is {} and no cold_tier_path set",
                    needed_ram_bytes,
                    b
                );
            }
            StaticPool::new_ram(slot_size, num_experts)
        } else {
            StaticPool::new_ram(slot_size, num_experts)
        };

        // Stage every expert into the pool: serialize once, copy into its
        // fixed-offset pool slot. Resident experts (idx < capacity) keep
        // their device `Arc` alive; pooled experts drop the `Arc` so the
        // VRAM is actually freed.
        //
        // Previously this was a second pass reusing bytes from a
        // pre-serialized cache. The pre-serialize pass was wasteful (it
        // ran on ALL experts just to compute max-slot-size, then all of
        // its bytes were discarded on Resident slots anyway), so we now
        // do a single pass and serialize each expert exactly once.
        // Staging policy — IMPORTANT when `experts` are on CPU (the Cached
        // backend's load path forces Device::Cpu to avoid libcuda's per-context
        // rwlock serializing 30k+ D->H memcpy calls during load):
        //
        //   - serialize each expert from wherever it lives (CPU cheap, CUDA
        //     slow-but-correct) into the pool once;
        //   - for resident slots (idx < capacity), re-materialize onto
        //     `cfg.device` via `deserialize(bytes, cfg.device)`. This is the
        //     only path that puts an expert on CUDA at load time. For a
        //     40-layer Qwen3.6 load with capacity=4 that's 40*4*3 = 480 H->D
        //     memcpys — vs. 40*256*3 = ~30 720 D->H memcpys in the old path;
        //   - for non-resident slots, drop the original Arc (frees whichever
        //     device it was on) and keep only the pool bytes.
        let t_stage = Instant::now();
        let mut slots: Vec<SlotState> = Vec::with_capacity(num_experts);
        let device = cfg.device.clone();
        let guard = QuantizeOntoGuard::new();
        for (idx, triple) in experts.into_iter().enumerate() {
            let gate_bytes = triple.gate.serialize()?;
            let up_bytes = triple.up.serialize()?;
            let down_bytes = triple.down.serialize()?;
            pool.write_slot(idx, &gate_bytes, &up_bytes, &down_bytes)?;
            if idx < capacity {
                // `serialize()` returns a `Cow<[u8]>` that may borrow from the
                // triple's internal tensors, so detach before dropping the
                // source triple, then reconstitute on the target device.
                let gate_owned: Cow<'static, [u8]> = Cow::Owned(gate_bytes.into_owned());
                let up_owned: Cow<'static, [u8]> = Cow::Owned(up_bytes.into_owned());
                let down_owned: Cow<'static, [u8]> = Cow::Owned(down_bytes.into_owned());
                drop(triple);
                let resident = ExpertTriple {
                    gate: ReplicatedLayer::deserialize(gate_owned, &device, &comm, guard.clone())?,
                    up: ReplicatedLayer::deserialize(up_owned, &device, &comm, guard.clone())?,
                    down: ReplicatedLayer::deserialize(down_owned, &device, &comm, guard.clone())?,
                };
                slots.push(SlotState::Resident(resident));
            } else {
                drop(triple);
                slots.push(SlotState::InPool);
            }
        }
        let stage_ms = t_stage.elapsed().as_millis();
        // One flush after all initial writes so the cold tier is durable
        // before the cache accepts queries.
        let _ = pool.backing.flush();
        tracing::info!(
            "MoE cache built: {} experts, capacity={}, slot_size={} KiB, total_construct_ms={}, stage_ms={}",
            num_experts,
            capacity,
            slot_size / 1024,
            t_construct.elapsed().as_millis(),
            stage_ms,
        );

        let lfu_counts = (0..num_experts).map(|_| AtomicU32::new(0)).collect();
        let last_access = (0..num_experts).map(|_| AtomicU32::new(0)).collect();

        Ok(Self {
            num_experts,
            cfg: CacheConfig {
                capacity,
                ..cfg
            },
            comm,
            guard: QuantizeOntoGuard::new(),
            inner: RwLock::new(CacheInner { slots, pool }),
            lfu_counts,
            last_access,
            clock: AtomicU32::new(0),
            accesses_since_decay: AtomicU32::new(0),
            stats: AtomicStats::default(),
        })
    }

    pub fn num_experts(&self) -> usize {
        self.num_experts
    }

    pub fn capacity(&self) -> usize {
        self.cfg.capacity
    }

    pub fn topology(&self) -> Topology {
        self.cfg.topology
    }

    pub fn backing_is_ssd(&self) -> bool {
        self.inner.read().pool.backing.is_ssd()
    }

    pub fn stats(&self) -> CacheStats {
        self.stats.snapshot()
    }

    /// Issue `madvise(WILLNEED)` hints for the given expert indices so the
    /// kernel starts pulling their pool slots into the page cache *before*
    /// the matmul path calls [`Self::ensure_resident`] on them.
    ///
    /// This is a best-effort performance hint: it does nothing on the RAM
    /// backing (bytes are already resident), does nothing for slots that are
    /// currently Resident (they're served from VRAM, not the pool), and
    /// silently ignores range-advise failures.
    ///
    /// The expected usage is that the forward path walks the token's top-k
    /// expert IDs once at the start, calls `prefetch_many` with the full
    /// set, and then iterates experts sequentially for matmul. By the time
    /// a miss actually needs to read its slot, the OS has already started
    /// the disk read — materialization sees warm page cache.
    ///
    /// Safe to call with duplicate indices, out-of-order indices, and
    /// Resident indices; they're all handled.
    pub fn prefetch_many(&self, indices: &[usize]) {
        if indices.is_empty() {
            return;
        }
        let inner = self.inner.read();
        if !inner.pool.backing.is_ssd() {
            return; // RAM backing: no-op.
        }
        for &idx in indices {
            if idx >= self.num_experts {
                continue;
            }
            if matches!(inner.slots[idx], SlotState::Resident(_)) {
                continue;
            }
            let meta = inner.pool.metadata[idx];
            if !meta.staged {
                continue;
            }
            let offset = inner.pool.slot_offset(idx);
            let len = meta.total_len() as usize;
            inner.pool.backing.advise_willneed(offset, len);
        }
    }

    /// Ensure expert `idx` is resident on the GPU; swap out the LFU victim if
    /// needed.
    ///
    /// # Fast path (hit)
    /// Takes only a **read lock** on `self.inner` and two atomic ops:
    /// `lfu_counts[idx].fetch_add(1)` and `last_access[idx].store(clock)`.
    /// Multiple hitting threads don't block each other.
    ///
    /// # Slow path (miss)
    /// Upgrades to a write lock, picks the LFU victim, writes the victim's
    /// serialized bytes to its fixed pool slot (single `memcpy` into the
    /// pre-allocated slab, or a single mmap'd region write for SSD), then
    /// deserializes the incoming expert from its slot onto the device.
    /// No heap allocation in the slot transition itself.
    pub fn ensure_resident(&self, idx: usize) -> Result<ExpertTriple> {
        if idx >= self.num_experts {
            candle_core::bail!(
                "expert idx {} out of bounds (num_experts={})",
                idx,
                self.num_experts
            );
        }

        // Update atomic LFU/clock regardless of hit/miss.
        let tick = self.clock.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        self.lfu_counts[idx].fetch_add(1, Ordering::Relaxed);
        self.last_access[idx].store(tick, Ordering::Relaxed);

        // Periodic LFU decay keeps the counters from saturating the
        // early-active experts over very long runs.
        let since_decay = self
            .accesses_since_decay
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        if since_decay >= LFU_DECAY_INTERVAL {
            self.accesses_since_decay.store(0, Ordering::Relaxed);
            for c in &self.lfu_counts {
                let v = c.load(Ordering::Relaxed);
                c.store(v / 2, Ordering::Relaxed);
            }
            self.stats.decays.fetch_add(1, Ordering::Relaxed);
        }

        // Fast path: take a read lock; if resident, return clone & done.
        // Two atomic increments + one Arc-clone triple — no write lock
        // anywhere, hits scale across threads.
        {
            let inner = self.inner.read();
            if let SlotState::Resident(triple) = &inner.slots[idx] {
                self.stats.hits.fetch_add(1, Ordering::Relaxed);
                return Ok(triple.clone());
            }
        }

        // Slow path: write lock + materialize + demote a victim.
        let mut inner = self.inner.write();
        // Re-check: another writer may have beaten us to it.
        if let SlotState::Resident(triple) = &inner.slots[idx] {
            self.stats.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(triple.clone());
        }

        // Materialize incoming expert from the pool slot.
        let (total_bytes, triple) = {
            let (gate, up, down) = inner.pool.read_slot(idx)?;
            let total_bytes = gate.len() + up.len() + down.len();
            (
                total_bytes,
                ExpertTriple {
                    gate: ReplicatedLayer::deserialize(
                        Cow::Borrowed(gate),
                        &self.cfg.device,
                        &self.comm,
                        self.guard.clone(),
                    )?,
                    up: ReplicatedLayer::deserialize(
                        Cow::Borrowed(up),
                        &self.cfg.device,
                        &self.comm,
                        self.guard.clone(),
                    )?,
                    down: ReplicatedLayer::deserialize(
                        Cow::Borrowed(down),
                        &self.cfg.device,
                        &self.comm,
                        self.guard.clone(),
                    )?,
                },
            )
        };
        let backing_kind = if inner.pool.backing.is_ssd() {
            "ssd"
        } else {
            "ram"
        };

        // Pick eviction victim using atomic snapshots of counters; require
        // the victim be Resident and not `idx`.
        let victim = self.pick_victim(&inner.slots, idx)?;

        // Demote victim — just drop its on-device `Arc`s and mark the slot
        // `InPool`. We do NOT re-serialize the weights: the pool's slot was
        // pre-staged at construction with the canonical bytes, and the cache
        // contract forbids mutating the `Arc<dyn QuantMethod>` post-load
        // (`get_isq_layers` returns empty for the cached backend for exactly
        // this reason). So the pool already holds bit-identical bytes.
        //
        // Skipping serialize+memcpy here saves the full expert size (tens to
        // hundreds of MiB per eviction) of CPU and memory bandwidth on every
        // miss — the single biggest hot-path win after the static-pool
        // refactor itself.
        match &inner.slots[victim] {
            SlotState::Resident(_) => {}
            _ => candle_core::bail!("victim slot {} not resident", victim),
        }
        debug_assert!(
            inner.pool.metadata[victim].staged,
            "invariant violated: victim slot {} not staged in pool",
            victim
        );
        inner.slots[victim] = SlotState::InPool;
        self.stats.evictions.fetch_add(1, Ordering::Relaxed);

        // Tell the kernel we just pulled the incoming slot's bytes onto the
        // device, so the CPU-side page-cache copy is now redundant. Lets the
        // OS reclaim those pages if it's under memory pressure instead of
        // squatting on them for experts that may not be touched again soon.
        // The corresponding `WillNeed` on a future re-activation will cold-
        // load the pages again. No-op on RAM backing.
        let incoming_off = inner.pool.slot_offset(idx);
        let incoming_len = inner.pool.metadata[idx].total_len() as usize;
        inner.pool.backing.advise_dontneed(incoming_off, incoming_len);

        // Promote incoming.
        inner.slots[idx] = SlotState::Resident(triple.clone());
        self.stats.misses.fetch_add(1, Ordering::Relaxed);
        self.stats.promotions.fetch_add(1, Ordering::Relaxed);

        tracing::trace!(
            expert = idx,
            victim = victim,
            tier = backing_kind,
            bytes = total_bytes,
            "moe cache miss + promotion"
        );

        Ok(triple)
    }

    /// Pick the LFU victim among currently-resident slots, excluding `exclude`.
    /// Tie-break by oldest `last_access`.
    fn pick_victim(&self, slots: &[SlotState], exclude: usize) -> Result<usize> {
        let mut best: Option<(usize, u32, u32)> = None;
        for (i, slot) in slots.iter().enumerate() {
            if i == exclude {
                continue;
            }
            if matches!(slot, SlotState::Resident(_)) {
                let lfu = self.lfu_counts[i].load(Ordering::Relaxed);
                let la = self.last_access[i].load(Ordering::Relaxed);
                match best {
                    None => best = Some((i, lfu, la)),
                    Some((_, blfu, bla)) => {
                        if lfu < blfu || (lfu == blfu && la < bla) {
                            best = Some((i, lfu, la));
                        }
                    }
                }
            }
        }
        best.map(|(i, _, _)| i).ok_or_else(|| {
            candle_core::Error::msg("no eviction victim found (cache state inconsistent?)")
        })
    }
}

/// Process-wide cache of the SSD probe result. Parallel layer-loads on
/// Qwen3.6-scale MoE models run 40+ `MoEExpertCache::new` calls at once;
/// each one calling `probe_ssd_bandwidth_mbps` would hit the same tmp
/// file concurrently and race. We probe once per process per path and
/// reuse the result — the answer doesn't change within a process
/// lifetime anyway.
static PROBE_RESULTS: std::sync::OnceLock<parking_lot::Mutex<hashbrown::HashMap<PathBuf, Option<u32>>>> =
    std::sync::OnceLock::new();

/// Measure sustained sequential-read throughput from a candidate SSD path.
///
/// Writes a 64 MiB probe file to a PID+thread-unique filename to avoid
/// races when multiple MoE layers construct their caches in parallel,
/// then flushes and times a read-to-completion. Caches the result per
/// directory so repeated calls from sibling cache constructions don't
/// re-run the probe.
///
/// Returns `None` if the probe could not run — callers treat that as
/// "cold tier unusable" and fail the cache construction.
pub fn probe_ssd_bandwidth_mbps(dir: &Path) -> Option<u32> {
    let cache = PROBE_RESULTS.get_or_init(|| parking_lot::Mutex::new(hashbrown::HashMap::new()));
    {
        let guard = cache.lock();
        if let Some(cached) = guard.get(dir) {
            return *cached;
        }
    }

    let result = probe_ssd_bandwidth_mbps_inner(dir);
    cache.lock().insert(dir.to_path_buf(), result);
    result
}

fn probe_ssd_bandwidth_mbps_inner(dir: &Path) -> Option<u32> {
    const PROBE_BYTES: usize = 64 * 1024 * 1024;
    std::fs::create_dir_all(dir).ok()?;
    // Unique-per-process-and-thread probe filename so overlapping probe
    // calls from parallel rayon workers don't truncate each other's file.
    let probe = dir.join(format!(
        ".ctox_moe_cache_probe-{}-{:?}",
        std::process::id(),
        std::thread::current().id(),
    ));
    let payload = vec![0u8; PROBE_BYTES];
    {
        let mut f = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&probe)
            .ok()?;
        f.write_all(&payload).ok()?;
        f.sync_all().ok()?;
    }
    let t0 = Instant::now();
    {
        let mut f = File::open(&probe).ok()?;
        let mut sink = vec![0u8; PROBE_BYTES];
        f.read_exact(&mut sink).ok()?;
    }
    let elapsed = t0.elapsed();
    let _ = std::fs::remove_file(&probe);
    let secs = elapsed.as_secs_f64();
    if secs <= 0.0 {
        return None;
    }
    let mbps = (PROBE_BYTES as f64 / (1024.0 * 1024.0) / secs) as u32;
    Some(mbps)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real_expert_triple(seed: f32) -> ExpertTriple {
        use candle_core::{DType, Tensor};
        use engine_quant::{QuantMethodConfig, UnquantLinear};

        let device = Device::Cpu;
        let make = |bias: f32| -> Arc<dyn QuantMethod> {
            let data = vec![seed + bias, seed - bias, seed * 2.0, seed + 1.0];
            let weight = Tensor::from_slice(&data, (2, 2), &device).unwrap();
            let linear = candle_nn::Linear::new(weight, None);
            Arc::new(UnquantLinear::new(QuantMethodConfig::Unquantized(linear)).unwrap())
        };
        let _ = DType::F32;
        ExpertTriple {
            gate: make(0.1),
            up: make(0.2),
            down: make(0.3),
        }
    }

    fn dummy_comm() -> Arc<engine_quant::Comm> {
        Arc::new(
            engine_quant::Comm::from_device(
                engine_quant::Id::new(),
                &Device::Cpu,
                /*rank=*/ 0,
                /*world_size=*/ 1,
            )
            .unwrap(),
        )
    }

    #[test]
    fn topology_detects_cpu() {
        let dev = Device::Cpu;
        assert_eq!(Topology::detect(&dev), Topology::CpuOnly);
        assert!(!Topology::CpuOnly.has_warm_tier());
        assert!(!Topology::CpuOnly.has_cold_tier());
    }

    #[test]
    fn topology_semantics_unified_vs_discrete() {
        assert!(!Topology::Unified.has_warm_tier());
        assert!(Topology::Unified.has_cold_tier());
        assert!(Topology::Discrete.has_warm_tier());
        assert!(Topology::Discrete.has_cold_tier());
    }

    #[test]
    fn static_pool_ram_roundtrip() {
        let mut pool = StaticPool::new_ram(/*slot_size=*/ 4096, /*num_experts=*/ 3);
        pool.write_slot(0, &[1, 2, 3], &[4, 5, 6, 7], &[8, 9]).unwrap();
        let (g, u, d) = pool.read_slot(0).unwrap();
        assert_eq!(g, &[1, 2, 3]);
        assert_eq!(u, &[4, 5, 6, 7]);
        assert_eq!(d, &[8, 9]);
        // Reading an unstaged slot must fail rather than return zeros.
        assert!(pool.read_slot(1).is_err());
    }

    #[test]
    fn static_pool_ssd_roundtrip() {
        let tmp = std::env::temp_dir().join(format!(
            "ctox_pool_ssd_{}.bin",
            std::process::id()
        ));
        let mut pool = StaticPool::new_ssd(4096, 2, tmp.clone()).unwrap();
        pool.write_slot(1, &[10, 20], &[30, 40, 50], &[60]).unwrap();
        pool.backing.flush().unwrap();
        let (g, u, d) = pool.read_slot(1).unwrap();
        assert_eq!(g, &[10, 20]);
        assert_eq!(u, &[30, 40, 50]);
        assert_eq!(d, &[60]);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn cache_swap_works_on_cpu_only_topology() {
        // Regression guard: `Topology::CpuOnly` no longer forces
        // `capacity >= num_experts`. A memory-constrained CPU-only host
        // (e.g. engine built without Metal feature on an M5) must still
        // be able to swap experts to the cold tier — it's a fallback, not
        // a pass-through.
        let tmp_path = std::env::temp_dir().join(format!(
            "ctox_moe_cache_cpu_only_{}.bin",
            std::process::id()
        ));

        let experts = vec![
            real_expert_triple(0.1),
            real_expert_triple(0.2),
            real_expert_triple(0.3),
            real_expert_triple(0.4),
        ];
        let original_3 = experts[3].down.dequantize_w().unwrap();
        let comm = dummy_comm();

        let cache = MoEExpertCache::new(
            experts,
            CacheConfig {
                capacity: 1,
                topology: Topology::CpuOnly,
                device: Device::Cpu,
                warm_tier_budget_bytes: None,
                cold_tier_path: Some(tmp_path.clone()),
                cold_tier_min_mbps: 0,
            },
            comm,
        )
        .expect("CpuOnly with capacity<num_experts and cold_tier_path must succeed");

        // Force swap cycle through all four experts; expert 3 should return
        // to bit-identical weights after re-materialization.
        let _ = cache.ensure_resident(1).unwrap();
        let _ = cache.ensure_resident(2).unwrap();
        let _ = cache.ensure_resident(0).unwrap();
        let re3 = cache.ensure_resident(3).unwrap();
        let restored = re3.down.dequantize_w().unwrap();
        let diff = (&original_3 - &restored).unwrap().abs().unwrap().sum_all().unwrap();
        assert!(
            diff.to_scalar::<f32>().unwrap() < 1e-6,
            "CpuOnly SSD-swap round-trip corrupted weights"
        );

        let stats = cache.stats();
        assert!(stats.evictions >= 1);
        assert!(stats.misses >= 1);

        // Cleanup: the cold-tier file is expert_*.bin children inside a
        // specific directory OR the single pool file; both cases handled.
        let _ = std::fs::remove_file(&tmp_path);
        let _ = std::fs::remove_dir_all(&tmp_path);
    }

    #[test]
    fn cache_end_to_end_swap_preserves_weights() {
        let experts = vec![
            real_expert_triple(0.1),
            real_expert_triple(0.2),
            real_expert_triple(0.3),
        ];
        let originals: Vec<_> = experts
            .iter()
            .map(|t| t.gate.dequantize_w().unwrap())
            .collect();
        let comm = dummy_comm();

        let cache = MoEExpertCache::new(
            experts,
            CacheConfig {
                capacity: 1,
                topology: Topology::Unified,
                device: Device::Cpu,
                warm_tier_budget_bytes: None,
                cold_tier_path: None,
                cold_tier_min_mbps: 0,
            },
            comm.clone(),
        )
        .unwrap();

        let _ = cache.ensure_resident(1).unwrap();
        let _ = cache.ensure_resident(2).unwrap();
        let re0 = cache.ensure_resident(0).unwrap();
        let restored0 = re0.gate.dequantize_w().unwrap();
        let diff = (&originals[0] - &restored0)
            .unwrap()
            .abs()
            .unwrap()
            .sum_all()
            .unwrap();
        assert!(diff.to_scalar::<f32>().unwrap() < 1e-6);

        let stats = cache.stats();
        assert!(stats.misses >= 1, "expected at least one miss");
        assert!(stats.evictions >= 1, "expected at least one eviction");
    }

    #[test]
    fn cache_end_to_end_swap_with_ssd_backing() {
        let tmp_path = std::env::temp_dir().join(format!(
            "ctox_moe_cache_ssd_{}.bin",
            std::process::id()
        ));

        let experts = vec![
            real_expert_triple(0.4),
            real_expert_triple(0.5),
            real_expert_triple(0.6),
        ];
        let original_0 = experts[0].up.dequantize_w().unwrap();
        let comm = dummy_comm();

        let cache = MoEExpertCache::new(
            experts,
            CacheConfig {
                capacity: 1,
                topology: Topology::Unified,
                device: Device::Cpu,
                warm_tier_budget_bytes: None,
                cold_tier_path: Some(tmp_path.clone()),
                cold_tier_min_mbps: 0,
            },
            comm.clone(),
        )
        .unwrap();

        assert!(cache.backing_is_ssd(), "expected SSD backing");

        let _ = cache.ensure_resident(1).unwrap();
        let _ = cache.ensure_resident(2).unwrap();
        let restored = cache.ensure_resident(0).unwrap();
        let restored_w = restored.up.dequantize_w().unwrap();
        let diff = (&original_0 - &restored_w)
            .unwrap()
            .abs()
            .unwrap()
            .sum_all()
            .unwrap();
        assert!(diff.to_scalar::<f32>().unwrap() < 1e-6);

        let _ = std::fs::remove_file(&tmp_path);
    }

    #[test]
    fn cache_hits_do_not_allocate_pool_bytes() {
        // Build a capacity=3 cache (nothing needs to evict). Repeatedly hit
        // expert 0 and verify stats.hits grows but stats.misses stays 0.
        // This is a smoke test of the lock-free hit path.
        let experts = vec![
            real_expert_triple(1.0),
            real_expert_triple(2.0),
            real_expert_triple(3.0),
        ];
        let comm = dummy_comm();
        let cache = MoEExpertCache::new(
            experts,
            CacheConfig {
                capacity: 3,
                topology: Topology::CpuOnly,
                device: Device::Cpu,
                warm_tier_budget_bytes: None,
                cold_tier_path: None,
                cold_tier_min_mbps: 0,
            },
            comm,
        )
        .unwrap();

        for _ in 0..100 {
            let _ = cache.ensure_resident(0).unwrap();
        }
        let stats = cache.stats();
        assert_eq!(stats.hits, 100, "every ensure_resident with capacity=N should hit");
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.evictions, 0);
    }

    #[test]
    fn lfu_decay_fires_after_threshold_accesses() {
        let experts = vec![
            real_expert_triple(1.0),
            real_expert_triple(2.0),
        ];
        let comm = dummy_comm();
        let cache = MoEExpertCache::new(
            experts,
            CacheConfig {
                capacity: 2,
                topology: Topology::CpuOnly,
                device: Device::Cpu,
                warm_tier_budget_bytes: None,
                cold_tier_path: None,
                cold_tier_min_mbps: 0,
            },
            comm,
        )
        .unwrap();

        // Accumulate enough hits to cross the decay interval at least once.
        for _ in 0..(LFU_DECAY_INTERVAL + 10) {
            let _ = cache.ensure_resident(0).unwrap();
        }
        assert!(cache.stats().decays >= 1);
    }

    #[test]
    fn probe_ssd_bandwidth_returns_positive_on_tmp_dir() {
        let probe = probe_ssd_bandwidth_mbps(&std::env::temp_dir());
        assert!(probe.is_some(), "probe must succeed on a writable tmpdir");
        assert!(probe.unwrap() > 0);
    }

    #[test]
    fn prefetch_many_is_safe_noop_on_ram_backing() {
        // RAM backing: prefetch_many must not touch anything, must not
        // error, must not mutate stats. Smoke-proves the idx-validation
        // and resident-filter short-circuits.
        let experts = vec![real_expert_triple(0.1), real_expert_triple(0.2)];
        let comm = dummy_comm();
        let cache = MoEExpertCache::new(
            experts,
            CacheConfig {
                capacity: 1,
                topology: Topology::Unified,
                device: Device::Cpu,
                warm_tier_budget_bytes: None,
                cold_tier_path: None,
                cold_tier_min_mbps: 0,
            },
            comm,
        )
        .unwrap();
        let stats_before = cache.stats();
        // Include a resident index (0), a pooled index (1), a bogus index
        // (99), and a duplicate (0) — none should error or fault.
        cache.prefetch_many(&[0, 1, 99, 0]);
        let stats_after = cache.stats();
        assert_eq!(stats_before.hits, stats_after.hits);
        assert_eq!(stats_before.misses, stats_after.misses);
    }

    #[test]
    fn concurrent_ensure_resident_is_safe() {
        // Stress the RwLock + AtomicU32 path from multiple threads. We spin
        // up N threads, each hammering ensure_resident on a mix of indices
        // that force evictions; we then verify (a) no thread panicked,
        // (b) stats counts match the total number of calls, and (c) every
        // final materialization produces bit-identical weights.
        use std::thread;

        let num_experts = 6;
        let capacity = 2;
        let originals: Vec<_> = (0..num_experts)
            .map(|i| real_expert_triple(i as f32 * 0.1))
            .collect();
        let expected: Vec<_> = originals
            .iter()
            .map(|t| t.gate.dequantize_w().unwrap())
            .collect();

        let cache = Arc::new(
            MoEExpertCache::new(
                originals,
                CacheConfig {
                    capacity,
                    topology: Topology::Unified,
                    device: Device::Cpu,
                    warm_tier_budget_bytes: None,
                    cold_tier_path: None,
                    cold_tier_min_mbps: 0,
                },
                dummy_comm(),
            )
            .unwrap(),
        );

        let calls_per_thread = 50usize;
        let n_threads = 4usize;
        let mut handles = vec![];
        for tid in 0..n_threads {
            let cache = cache.clone();
            handles.push(thread::spawn(move || {
                for i in 0..calls_per_thread {
                    let idx = ((tid + i) * 7) % num_experts;
                    let _ = cache.ensure_resident(idx).unwrap();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let stats = cache.stats();
        let total = (n_threads * calls_per_thread) as u64;
        assert_eq!(
            stats.hits + stats.misses,
            total,
            "hit+miss total mismatch: hits={} misses={} total={}",
            stats.hits,
            stats.misses,
            total
        );

        // After the barrage, every expert's weights must still round-trip
        // correctly through the cache.
        for (i, expected_w) in expected.iter().enumerate() {
            let triple = cache.ensure_resident(i).unwrap();
            let actual = triple.gate.dequantize_w().unwrap();
            let diff = (expected_w - &actual).unwrap().abs().unwrap().sum_all().unwrap();
            assert!(
                diff.to_scalar::<f32>().unwrap() < 1e-6,
                "expert {} corrupted after concurrent stress",
                i
            );
        }
    }

    #[test]
    fn prefetch_many_on_ssd_backing_runs_without_error() {
        // SSD backing: exercise the madvise code path. We can't directly
        // observe page-cache warmup in a unit test (that would need a
        // profiling hook), so we simply verify the call succeeds and
        // a subsequent ensure_resident still produces correct weights.
        let tmp_path = std::env::temp_dir().join(format!(
            "ctox_prefetch_ssd_{}.bin",
            std::process::id()
        ));
        let experts = vec![
            real_expert_triple(0.4),
            real_expert_triple(0.5),
            real_expert_triple(0.6),
        ];
        let original = experts[2].gate.dequantize_w().unwrap();
        let comm = dummy_comm();
        let cache = MoEExpertCache::new(
            experts,
            CacheConfig {
                capacity: 1,
                topology: Topology::Unified,
                device: Device::Cpu,
                warm_tier_budget_bytes: None,
                cold_tier_path: Some(tmp_path.clone()),
                cold_tier_min_mbps: 0,
            },
            comm,
        )
        .unwrap();
        assert!(cache.backing_is_ssd());

        // Prefetch the full active set, then materialize expert 2. Bytes
        // must still round-trip identically.
        cache.prefetch_many(&[1, 2]);
        let re2 = cache.ensure_resident(2).unwrap();
        let restored = re2.gate.dequantize_w().unwrap();
        let diff = (&original - &restored).unwrap().abs().unwrap().sum_all().unwrap();
        assert!(diff.to_scalar::<f32>().unwrap() < 1e-6);

        let _ = std::fs::remove_file(&tmp_path);
    }
}
