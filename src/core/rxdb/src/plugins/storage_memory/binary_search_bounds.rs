//! Port of `src/plugins/storage-memory/binary-search-bounds.ts`.
//!
//! Everything in this file was copied and adapted from
//! <https://github.com/mikolalysenko/binary-search-bounds>.

use std::cmp::Ordering;

type Compare<T> = dyn Fn(&T, &T) -> Ordering;

// ref: rxdb/src/plugins/storage-memory/binary-search-bounds.ts:13-26
fn ge_inner<T>(a: &[T], y: &T, c: &Compare<T>, mut l: i64, mut h: i64) -> i64 {
    let mut i = h + 1;
    while l <= h {
        let m = ((l as u64 + h as u64) >> 1) as i64;
        let x = &a[m as usize];
        let p = c(x, y);
        if p != Ordering::Less {
            i = m;
            h = m - 1;
        } else {
            l = m + 1;
        }
    }
    i
}

// ref: rxdb/src/plugins/storage-memory/binary-search-bounds.ts:28-41
fn gt_inner<T>(a: &[T], y: &T, c: &Compare<T>, mut l: i64, mut h: i64) -> i64 {
    let mut i = h + 1;
    while l <= h {
        let m = ((l as u64 + h as u64) >> 1) as i64;
        let x = &a[m as usize];
        let p = c(x, y);
        if p == Ordering::Greater {
            i = m;
            h = m - 1;
        } else {
            l = m + 1;
        }
    }
    i
}

// ref: rxdb/src/plugins/storage-memory/binary-search-bounds.ts:43-55
fn lt_inner<T>(a: &[T], y: &T, c: &Compare<T>, mut l: i64, mut h: i64) -> i64 {
    let mut i = l - 1;
    while l <= h {
        let m = ((l as u64 + h as u64) >> 1) as i64;
        let x = &a[m as usize];
        let p = c(x, y);
        if p == Ordering::Less {
            i = m;
            l = m + 1;
        } else {
            h = m - 1;
        }
    }
    i
}

// ref: rxdb/src/plugins/storage-memory/binary-search-bounds.ts:57-69
fn le_inner<T>(a: &[T], y: &T, c: &Compare<T>, mut l: i64, mut h: i64) -> i64 {
    let mut i = l - 1;
    while l <= h {
        let m = ((l as u64 + h as u64) >> 1) as i64;
        let x = &a[m as usize];
        let p = c(x, y);
        if p != Ordering::Greater {
            i = m;
            l = m + 1;
        } else {
            h = m - 1;
        }
    }
    i
}

// ref: rxdb/src/plugins/storage-memory/binary-search-bounds.ts:71-85
fn eq_inner<T>(a: &[T], y: &T, c: &Compare<T>, mut l: i64, mut h: i64) -> i64 {
    while l <= h {
        let m = ((l as u64 + h as u64) >> 1) as i64;
        let x = &a[m as usize];
        let p = c(x, y);
        if p == Ordering::Equal {
            return m;
        }
        if p != Ordering::Greater {
            l = m + 1;
        } else {
            h = m - 1;
        }
    }
    -1
}

// ref: rxdb/src/plugins/storage-memory/binary-search-bounds.ts:87-89
fn norm<T>(
    a: &[T],
    y: &T,
    c: &Compare<T>,
    l: Option<i64>,
    h: Option<i64>,
    f: impl Fn(&[T], &T, &Compare<T>, i64, i64) -> i64,
) -> i64 {
    let l = l.unwrap_or(0);
    let h = h.unwrap_or((a.len() as i64) - 1);
    f(a, y, c, l, h)
}

// ref: rxdb/src/plugins/storage-memory/binary-search-bounds.ts:92-94
pub fn bound_ge<T>(a: &[T], y: &T, c: &Compare<T>, l: Option<i64>, h: Option<i64>) -> i64 {
    norm(a, y, c, l, h, ge_inner)
}

// ref: rxdb/src/plugins/storage-memory/binary-search-bounds.ts:95-97
pub fn bound_gt<T>(a: &[T], y: &T, c: &Compare<T>, l: Option<i64>, h: Option<i64>) -> i64 {
    norm(a, y, c, l, h, gt_inner)
}

// ref: rxdb/src/plugins/storage-memory/binary-search-bounds.ts:98-100
pub fn bound_lt<T>(a: &[T], y: &T, c: &Compare<T>, l: Option<i64>, h: Option<i64>) -> i64 {
    norm(a, y, c, l, h, lt_inner)
}

// ref: rxdb/src/plugins/storage-memory/binary-search-bounds.ts:101-103
pub fn bound_le<T>(a: &[T], y: &T, c: &Compare<T>, l: Option<i64>, h: Option<i64>) -> i64 {
    norm(a, y, c, l, h, le_inner)
}

// ref: rxdb/src/plugins/storage-memory/binary-search-bounds.ts:104-106
pub fn bound_eq<T>(a: &[T], y: &T, c: &Compare<T>, l: Option<i64>, h: Option<i64>) -> i64 {
    norm(a, y, c, l, h, eq_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    fn cmp_int(a: &i32, b: &i32) -> Ordering {
        a.cmp(b)
    }

    #[test]
    fn bound_ge_finds_first_not_less() {
        let a = vec![1, 3, 5, 7, 9];
        // first index with a[i] >= 6
        assert_eq!(bound_ge(&a, &6, &cmp_int, None, None), 3);
        // first index with a[i] >= 3
        assert_eq!(bound_ge(&a, &3, &cmp_int, None, None), 1);
    }

    #[test]
    fn bound_lt_finds_last_less() {
        let a = vec![1, 3, 5, 7, 9];
        // last index with a[i] < 6
        assert_eq!(bound_lt(&a, &6, &cmp_int, None, None), 2);
        // none less than 1
        assert_eq!(bound_lt(&a, &1, &cmp_int, None, None), -1);
    }

    #[test]
    fn bound_eq_finds_match_or_neg_one() {
        let a = vec![1, 3, 5, 7, 9];
        assert_eq!(bound_eq(&a, &5, &cmp_int, None, None), 2);
        assert_eq!(bound_eq(&a, &4, &cmp_int, None, None), -1);
    }
}
