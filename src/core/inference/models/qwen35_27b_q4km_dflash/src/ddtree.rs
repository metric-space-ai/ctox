//! 1:1 Rust port of the DDTree helpers in
//! `lucebox/dflash/test/test_dflash.cpp`.
//!
//! Covers:
//!
//!   * [`extract_draft_topk`]   — test_dflash.cpp:128-192
//!   * [`DDTree`]                — test_dflash.cpp:203-210
//!   * [`build_ddtree`]          — test_dflash.cpp:212-375
//!   * [`follow_verified_tree`]  — test_dflash.cpp:377-400
//!   * [`build_tree_mask`]       — test_dflash.cpp:408-427
//!
//! Tree-mode target graph building + the DDTree branch of the run loop
//! live in `driver.rs` because they touch the graph builders.
//!
//! # Algorithm summary
//!
//! DDTree wraps a best-first heap walk of per-position top-K
//! distributions into a DFS-flattened tree. Slot 0 is the tree root
//! (= previous iter's `last_tok`); slots 1..n_nodes are DFS-ordered
//! tree nodes. `parents[i]` gives each node's parent index in the
//! flat array (`parents[0] = -1`). `visibility[i * N + j]` (ancestor-
//! only mask) is true iff `j` is an ancestor of `i` in the tree
//! (including `j == i`), used to build the attention mask.

use std::collections::{BinaryHeap, HashMap};

use rayon::prelude::*;

// ─── extract_draft_topk ─────────────────────────────────────────
//
// ref: test_dflash.cpp:128-192

/// Per-position top-K log-prob + token-id extraction.
///
/// `logits`: `[n_positions * vocab]` row-major.
/// `out_log_probs`: `[n_positions * K]` row-major, sorted DESCENDING
///                  (largest logit first).
/// `out_token_ids`: `[n_positions * K]` matching token ids.
/// `temperature`:   softmax temperature. `< 1` sharpens (used to
///                  compensate for Q4_K_M draft's flattened softmax).
///
/// Implementation mirrors the reference exactly — online log-sum-exp
/// with running max + min-heap top-K maintenance in a single pass over
/// the vocab. Not parallelized here (the reference uses `#pragma omp
/// parallel for` over positions); the gain would be ~0.1 ms at our
/// sizes so we leave it serial for port fidelity.
///
/// ref: test_dflash.cpp:128-192
pub fn extract_draft_topk(
    logits: &[f32],
    n_positions: usize,
    vocab: usize,
    k: usize,
    out_log_probs: &mut [f32],
    out_token_ids: &mut [i32],
    temperature: f32,
) {
    #[derive(Debug, Clone, Copy)]
    struct Entry {
        logit: f32,
        id: i32,
    }
    // Min-heap by `logit` — we want to kick out the smallest when we
    // see a larger candidate. Rust's BinaryHeap is a MAX-heap so we
    // wrap with a Reverse-ish ordering. Keep explicit Ord with floats
    // (using total_cmp to sidestep NaN complaints).
    impl PartialEq for Entry {
        fn eq(&self, other: &Self) -> bool {
            self.logit == other.logit
        }
    }
    impl Eq for Entry {}
    impl PartialOrd for Entry {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Entry {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            // Reverse: smaller logit = "greater" in heap → pops first.
            other.logit.total_cmp(&self.logit)
        }
    }

    // Temperature scaling: dividing logits by T<1 sharpens the softmax
    // (widens gap between top-1 and lower ranks). Compensates for
    // Q4_K_M quantization.
    let inv_t = 1.0_f32 / temperature.max(1e-3);

    // Parallelize over positions — matches the reference's
    // `#pragma omp parallel for schedule(static)`.
    //
    // Each position is independent: it scans the full vocab once and
    // writes k entries into its own stripe of out_log_probs /
    // out_token_ids. Use rayon `par_chunks_mut` to split the output
    // slices into disjoint per-position chunks each worker gets a
    // safe mutable view into.
    out_log_probs
        .par_chunks_mut(k)
        .zip(out_token_ids.par_chunks_mut(k))
        .enumerate()
        .take(n_positions)
        .for_each(|(i, (lp_out, id_out))| {
            let li = &logits[i * vocab..(i + 1) * vocab];
            let mut heap: BinaryHeap<Entry> = BinaryHeap::with_capacity(k);

            let mut running_max = f32::NEG_INFINITY;
            let mut running_sum_exp = 0.0_f32;
            for (j, &lj) in li.iter().enumerate() {
                let l = lj * inv_t;

                // Online logsumexp.
                if l > running_max {
                    if running_max > f32::NEG_INFINITY {
                        running_sum_exp *= (running_max - l).exp();
                    }
                    running_sum_exp += 1.0;
                    running_max = l;
                } else {
                    running_sum_exp += (l - running_max).exp();
                }

                // Top-K maintenance.
                if heap.len() < k {
                    heap.push(Entry {
                        logit: l,
                        id: j as i32,
                    });
                } else {
                    let top = heap.peek().expect("heap non-empty");
                    if l > top.logit {
                        heap.pop();
                        heap.push(Entry {
                            logit: l,
                            id: j as i32,
                        });
                    }
                }
            }
            let log_z = running_max + running_sum_exp.ln();

            let sorted: Vec<Entry> = heap.into_sorted_vec();
            for (rank, e) in sorted.iter().enumerate() {
                lp_out[rank] = e.logit - log_z;
                id_out[rank] = e.id;
            }
        });
}

// ─── DDTree struct ──────────────────────────────────────────────
//
// ref: test_dflash.cpp:203-210

/// A flat DFS-ordered tree built from the draft's top-K softmax
/// distributions.
pub struct DDTree {
    /// excludes root
    pub n_nodes: i32,
    /// size `n_nodes`
    pub token_ids: Vec<i32>,
    /// size `n_nodes` (1..L)
    pub depths: Vec<i32>,
    /// size `n_nodes + 1`, `parents[0] = -1`
    pub parents: Vec<i32>,
    /// size `n_nodes + 1` — per-node map from token_id → child flat index
    pub child_maps: Vec<HashMap<i32, i32>>,
    /// `(1 + n_nodes)^2` row-major. `visibility[i*N + j]` iff `j` is
    /// an ancestor of `i` (including j == i).
    pub visibility: Vec<u8>,
}

impl Default for DDTree {
    fn default() -> Self {
        Self {
            n_nodes: 0,
            token_ids: Vec::new(),
            depths: Vec::new(),
            parents: Vec::new(),
            child_maps: Vec::new(),
            visibility: Vec::new(),
        }
    }
}

// ─── build_ddtree ───────────────────────────────────────────────
//
// ref: test_dflash.cpp:212-375

/// Port of `build_ddtree_tree()` from ddtree.py. Runs a best-first
/// heap over prefixes of the per-position top-K distributions, pops
/// until `budget` nodes are accumulated. Populates the flat
/// DFS-ordered tree structure.
///
///   * `top_log_probs`: `[L * K]` drafter's per-position top-K log-probs
///   * `top_token_ids`: `[L * K]` matching token ids, rank 0 = argmax
///   * `l`:             max tree depth (usually `q_len - 1`)
///   * `k`:             top-K per position
///   * `budget`:        maximum number of non-root tree nodes
///   * `chain_seed`:    if `true`, pre-seed top-1 chain (defensive,
///                     guarantees AL >= chain mode). If `false`, pure
///                     best-first from root.
pub fn build_ddtree(
    top_log_probs: &[f32],
    top_token_ids: &[i32],
    l: i32,
    k: i32,
    budget: i32,
    chain_seed: bool,
) -> DDTree {
    let mut tree = DDTree::default();
    if budget <= 0 || l <= 0 {
        tree.parents.push(-1);
        tree.child_maps.push(HashMap::new());
        tree.visibility = vec![1];
        return tree;
    }

    #[derive(Debug, Clone)]
    struct HeapEntry {
        neg_logw: f32, // smaller neg_logw = higher logw → pop first
        parent_index: i32,
        depth: i32,
        rank: i32,
        logw: f32,
    }
    // BinaryHeap is max-heap. We want smallest neg_logw at the top ⇒
    // compare by Reverse(neg_logw).
    impl PartialEq for HeapEntry {
        fn eq(&self, o: &Self) -> bool {
            self.neg_logw == o.neg_logw
        }
    }
    impl Eq for HeapEntry {}
    impl PartialOrd for HeapEntry {
        fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(o))
        }
    }
    impl Ord for HeapEntry {
        fn cmp(&self, o: &Self) -> std::cmp::Ordering {
            // REVERSE so smallest neg_logw sits on top of the max-heap.
            o.neg_logw.total_cmp(&self.neg_logw)
        }
    }
    let mut heap: BinaryHeap<HeapEntry> = BinaryHeap::new();

    tree.token_ids.reserve(budget as usize);
    tree.depths.reserve(budget as usize);
    tree.parents.reserve((budget + 1) as usize);
    tree.parents.push(-1); // root
    tree.child_maps.push(HashMap::new()); // root's children

    let k_us = k as usize;
    let lp = |d: i32, r: i32| top_log_probs[((d - 1) as usize) * k_us + r as usize];
    let tid = |d: i32, r: i32| top_token_ids[((d - 1) as usize) * k_us + r as usize];

    if chain_seed {
        // Pre-seed full top-1 chain.
        let chain_depth = std::cmp::min(l, budget);
        let mut cum_logw = 0.0_f32;
        let mut prev_idx: i32 = 0;
        for d in 1..=chain_depth {
            let tok_id = tid(d, 0);
            cum_logw += lp(d, 0);

            let cur_idx = tree.n_nodes + 1;
            tree.token_ids.push(tok_id);
            tree.depths.push(d);
            tree.parents.push(prev_idx);
            tree.child_maps.push(HashMap::new());
            tree.child_maps[prev_idx as usize].insert(tok_id, cur_idx);
            tree.n_nodes += 1;

            if k > 1 {
                let sibling_logw = cum_logw - lp(d, 0) + lp(d, 1);
                heap.push(HeapEntry {
                    neg_logw: -sibling_logw,
                    parent_index: prev_idx,
                    depth: d,
                    rank: 1,
                    logw: sibling_logw,
                });
            }
            prev_idx = cur_idx;
        }
    } else {
        // Paper-style pure best-first: seed heap with depth-1 top-1.
        let root_logw = lp(1, 0);
        heap.push(HeapEntry {
            neg_logw: -root_logw,
            parent_index: 0,
            depth: 1,
            rank: 0,
            logw: root_logw,
        });
    }

    while tree.n_nodes < budget {
        let Some(top) = heap.pop() else {
            break;
        };

        let depth = top.depth;
        let rank = top.rank;
        let token_id = tid(depth, rank);

        let current_index = tree.n_nodes + 1;
        tree.token_ids.push(token_id);
        tree.depths.push(depth);
        tree.parents.push(top.parent_index);
        tree.child_maps.push(HashMap::new());
        tree.child_maps[top.parent_index as usize].insert(token_id, current_index);
        tree.n_nodes += 1;

        // Push next sibling.
        if rank + 1 < k {
            let sibling_logw = top.logw - lp(depth, rank) + lp(depth, rank + 1);
            heap.push(HeapEntry {
                neg_logw: -sibling_logw,
                parent_index: top.parent_index,
                depth,
                rank: rank + 1,
                logw: sibling_logw,
            });
        }

        // Push first child.
        if depth < l {
            let child_logw = top.logw + lp(depth + 1, 0);
            heap.push(HeapEntry {
                neg_logw: -child_logw,
                parent_index: current_index,
                depth: depth + 1,
                rank: 0,
                logw: child_logw,
            });
        }
    }

    // Build ancestor-only visibility mask (flat row-major, (1+n)²).
    let n = (1 + tree.n_nodes) as usize;
    tree.visibility = vec![0_u8; n * n];
    tree.visibility[0 * n + 0] = 1; // root sees itself
    for i in 1..n {
        let p = tree.parents[i] as usize;
        // Inherit parent's row up to column i-1, then mark self at col i.
        for j in 0..i {
            tree.visibility[i * n + j] = tree.visibility[p * n + j];
        }
        tree.visibility[i * n + i] = 1;
    }

    tree
}

// ─── follow_verified_tree ───────────────────────────────────────
//
// ref: test_dflash.cpp:377-400

/// Walk the verified tree following the target's argmax (posterior)
/// at each node. Returns the list of flat-tree indices that make up
/// the accepted path (starting at root), plus the next "bonus" token
/// (target's argmax at the deepest accepted node, which didn't match
/// any of that node's children).
pub fn follow_verified_tree(tree: &DDTree, posterior: &[i32]) -> (Vec<i32>, i32) {
    let mut accepted: Vec<i32> = Vec::with_capacity((tree.n_nodes + 1) as usize);
    accepted.push(0);

    let mut current_index: i32 = 0;
    let mut next_token = posterior[current_index as usize];
    loop {
        let children = &tree.child_maps[current_index as usize];
        let Some(&next_idx) = children.get(&next_token) else {
            break;
        };
        current_index = next_idx;
        accepted.push(current_index);
        next_token = posterior[current_index as usize];
    }
    (accepted, next_token)
}

// ─── build_tree_mask ────────────────────────────────────────────
//
// ref: test_dflash.cpp:408-427

/// F16 helper constants — match the driver's.
const F16_NEG_INF: u16 = 0xFC00;
const F16_ZERO: u16 = 0x0000;

const KQ_MASK_PAD: i32 = 64;
const G_KQ_STRIDE_PAD: i32 = 1;

fn align_up(n: i32, k: i32) -> i32 {
    ((n + k - 1) / k) * k
}

/// Build an f16 ancestor-only attention mask for tree verify:
///   `mask[q=i][k<past_length]           = 0`    (past KV cache)
///   `mask[q=i][k=past_length+j]         = 0 iff j is an ancestor of i
///                                               (including j == i)`
///                                        `= -inf otherwise`
/// Shape matches `flash_attn_ext`: `[kv_pad, q_pad]` f16.
pub fn build_tree_mask(tree: &DDTree, past_length: i32, out_mask: &mut Vec<u16>) {
    let n = 1 + tree.n_nodes;
    let kv_len = past_length + n;
    let kv_pad = align_up(kv_len, G_KQ_STRIDE_PAD);
    let q_pad = align_up(n, KQ_MASK_PAD);
    out_mask.clear();
    out_mask.resize((kv_pad as usize) * (q_pad as usize), F16_NEG_INF);
    let n_us = n as usize;
    let kv_pad_us = kv_pad as usize;
    let past_len_us = past_length as usize;
    for q in 0..n_us {
        // Past KV always visible.
        for k in 0..past_len_us {
            out_mask[q * kv_pad_us + k] = F16_ZERO;
        }
        // Tree region: ancestors-only.
        for j in 0..n_us {
            if tree.visibility[q * n_us + j] != 0 {
                out_mask[q * kv_pad_us + (past_len_us + j)] = F16_ZERO;
            }
        }
    }
}
