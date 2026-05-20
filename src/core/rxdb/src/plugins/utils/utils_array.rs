//! Array / slice utilities.

use std::future::Future;

use rand::seq::SliceRandom;

// ref: rxdb/src/plugins/utils/utils-array.ts:6-8
pub fn last_of_array<T: Clone>(ar: &[T]) -> Option<T> {
    ar.last().cloned()
}

// ref: rxdb/src/plugins/utils/utils-array.ts:13-15
/// shuffle the given array
pub fn shuffle_array<T: Clone>(arr: &[T]) -> Vec<T> {
    let mut out: Vec<T> = arr.to_vec();
    out.shuffle(&mut rand::thread_rng());
    out
}

// ref: rxdb/src/plugins/utils/utils-array.ts:17-20
pub fn random_of_array<T: Clone>(arr: &[T]) -> T {
    arr.choose(&mut rand::thread_rng())
        .cloned()
        .expect("random_of_array on empty slice")
}

// ref: rxdb/src/plugins/utils/utils-array.ts:23-25
pub fn to_array<T: Clone>(input: Vec<T>) -> Vec<T> {
    input
}

// ref: rxdb/src/plugins/utils/utils-array.ts:31-39
/// Split array with items into smaller arrays with items.
pub fn batch_array<T: Clone>(array: &[T], batch_size: usize) -> Vec<Vec<T>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < array.len() {
        let end = (i + batch_size).min(array.len());
        out.push(array[i..end].to_vec());
        i = end;
    }
    out
}

// ref: rxdb/src/plugins/utils/utils-array.ts:44-55
pub fn remove_one_from_array_if_matches<T: Clone>(
    ar: &[T],
    condition: impl Fn(&T) -> bool,
) -> Vec<T> {
    let mut out: Vec<T> = ar.to_vec();
    for i in (0..out.len()).rev() {
        if condition(&out[i]) {
            out.remove(i);
            break;
        }
    }
    out
}

// ref: rxdb/src/plugins/utils/utils-array.ts:60-69
// `isMaybeReadonlyArray` — TypeScript-specific type-guard. Omitted.

// ref: rxdb/src/plugins/utils/utils-array.ts:73-82
pub fn is_one_item_of_array_in_other_array<T: PartialEq>(ar1: &[T], ar2: &[T]) -> bool {
    ar1.iter().any(|el| ar2.contains(el))
}

// ref: rxdb/src/plugins/utils/utils-array.ts:90-95
// `arrayFilterNotEmpty(value)` — TS type narrowing. In Rust use `Option::flatten` /
// `iter().flatten()`. Omitted.

// ref: rxdb/src/plugins/utils/utils-array.ts:97-113
pub fn count_until_not_matching<T>(ar: &[T], matching_fn: impl Fn(&T, usize) -> bool) -> usize {
    let mut count = 0;
    for (idx, item) in ar.iter().enumerate() {
        if matching_fn(item, idx) {
            count += 1;
        } else {
            break;
        }
    }
    count
}

// ref: rxdb/src/plugins/utils/utils-array.ts:115-121
pub async fn async_filter<T, F, Fut>(array: Vec<T>, predicate: F) -> Vec<T>
where
    T: Clone,
    F: Fn(T, usize) -> Fut,
    Fut: Future<Output = bool>,
{
    let mut filters = Vec::with_capacity(array.len());
    for (i, item) in array.iter().cloned().enumerate() {
        filters.push(predicate(item, i).await);
    }
    array
        .into_iter()
        .enumerate()
        .filter_map(|(i, v)| if filters[i] { Some(v) } else { None })
        .collect()
}

// ref: rxdb/src/plugins/utils/utils-array.ts:126-132
pub fn sum_number_array(array: &[f64]) -> f64 {
    let mut count = 0.0;
    for i in (0..array.len()).rev() {
        count += array[i];
    }
    count
}

// ref: rxdb/src/plugins/utils/utils-array.ts:134-136
pub fn max_of_numbers(arr: &[f64]) -> f64 {
    arr.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
}

// ref: rxdb/src/plugins/utils/utils-array.ts:150-165
/// Appends the given documents to the given array.
/// This will mutate the first given array.
pub fn append_to_array<T: Clone>(ar: &mut Vec<T>, add: &[T]) {
    if add.is_empty() {
        return;
    }
    ar.extend_from_slice(add);
}

// ref: rxdb/src/plugins/utils/utils-array.ts:170-174
pub fn unique_array(arr_arg: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    arr_arg
        .iter()
        .filter(|s| seen.insert((*s).clone()))
        .cloned()
        .collect()
}

// ref: rxdb/src/plugins/utils/utils-array.ts:177-181
/// Returns a comparator that sorts descending by the numeric value at `property`.
pub fn sort_by_number_property<T>(
    extract: impl Fn(&T) -> f64 + Clone,
) -> impl Fn(&T, &T) -> std::cmp::Ordering {
    move |a, b| {
        extract(b)
            .partial_cmp(&extract(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}
