//! Port of the `array-push-at-sort-position` NPM package (gap-item N8a).
//!
//! Upstream uses an in-house binary-search routine. Rust's standard library
//! exposes the same primitive via `Vec::binary_search_by`. Returns the
//! insertion index (the position the new element occupies after the insert).
//!
//! Source: https://github.com/pubkey/array-push-at-sort-position
//! (version pinned in upstream's `package.json`; the function is
//! single-purpose and stable).

use std::cmp::Ordering;

// ref: array-push-at-sort-position/src/index.ts: pushAtSortPosition
/// Insert `item` into the sorted slice `arr[start_index..]` using `comparator`,
/// preserving order. Returns the index `item` ends up at in `arr`.
///
/// Comparator semantics match upstream: `comparator(a, b) < 0` ⇒ `a` is
/// considered "less than" `b` (so equal/less elements stay before `item`).
pub fn push_at_sort_position<T>(
    arr: &mut Vec<T>,
    item: T,
    comparator: impl Fn(&T, &T) -> Ordering,
    start_index: usize,
) -> usize {
    let slice = &arr[start_index..];
    let rel_pos = slice
        .binary_search_by(|x| comparator(x, &item))
        .unwrap_or_else(|e| e);
    let pos = rel_pos + start_index;
    arr.insert(pos, item);
    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_at_correct_position() {
        let mut v = vec![1, 3, 5, 7];
        let pos = push_at_sort_position(&mut v, 4, |a, b| a.cmp(b), 0);
        assert_eq!(pos, 2);
        assert_eq!(v, vec![1, 3, 4, 5, 7]);
    }

    #[test]
    fn inserts_at_front() {
        let mut v = vec![10, 20, 30];
        let pos = push_at_sort_position(&mut v, 5, |a, b| a.cmp(b), 0);
        assert_eq!(pos, 0);
        assert_eq!(v, vec![5, 10, 20, 30]);
    }

    #[test]
    fn inserts_at_end() {
        let mut v = vec![1, 2, 3];
        let pos = push_at_sort_position(&mut v, 4, |a, b| a.cmp(b), 0);
        assert_eq!(pos, 3);
        assert_eq!(v, vec![1, 2, 3, 4]);
    }

    #[test]
    fn respects_start_index() {
        let mut v = vec![100, 1, 3, 5];
        let pos = push_at_sort_position(&mut v, 4, |a, b| a.cmp(b), 1);
        assert_eq!(pos, 3);
        assert_eq!(v, vec![100, 1, 3, 4, 5]);
    }
}
