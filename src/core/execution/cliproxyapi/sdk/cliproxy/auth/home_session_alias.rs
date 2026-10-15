// ref: sdk/cliproxy/auth/home_session_alias.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use super::compact_home_session_aliases;

pub const DEFAULT_HOME_SESSION_ALIAS_TTL: Duration = Duration::from_secs(60 * 60);
const HOME_SESSION_ALIAS_CLEANUP_OPS: u64 = 256;
const HOME_SESSION_ALIAS_SOFT_LIMIT: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
struct HomeSessionAliasEntry {
    canonical: String,
    expires_at_ms: i64,
    aliases: Vec<String>,
}

#[derive(Default)]
struct HomeSessionAliasState {
    entries: BTreeMap<String, HomeSessionAliasEntry>,
    groups: BTreeMap<String, HomeSessionAliasEntry>,
    eviction_order: VecDeque<String>,
    ops: u64,
}

/// Reconciles client identifiers for one Home session. CTOX supplies the
/// current time and configured TTL, keeping scheduling authority outside the
/// SDK cache while preserving upstream alias and eviction semantics.
#[derive(Default)]
pub struct HomeSessionAliasCache {
    state: Mutex<HomeSessionAliasState>,
}

impl HomeSessionAliasCache {
    #[must_use]
    pub fn canonical(
        &self,
        primary: &str,
        fallback: &str,
        ttl: Duration,
        now_ms: i64,
    ) -> Option<String> {
        let primary = primary.trim();
        let fallback = fallback.trim();
        if primary.is_empty() {
            return None;
        }
        let ttl = if ttl.is_zero() {
            DEFAULT_HOME_SESSION_ALIAS_TTL
        } else {
            ttl
        };
        let ttl_ms = ttl.as_millis().min(i64::MAX as u128) as i64;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.ops = state.ops.wrapping_add(1);
        if state.ops.is_multiple_of(HOME_SESSION_ALIAS_CLEANUP_OPS) {
            cleanup(&mut state, now_ms);
        }

        let mut canonical = primary.to_owned();
        let mut aliases = merge_aliases([], [primary, fallback]);
        let mut previous = BTreeMap::new();
        let mut primary_found = false;
        let mut from_live_alias = false;

        if let Some(entry) = live_entry(&mut state, primary, now_ms) {
            primary_found = true;
            from_live_alias = true;
            canonical.clone_from(&entry.canonical);
            remember(&mut previous, &entry);
            aliases = merge_aliases(
                aliases.iter().map(String::as_str),
                entry.aliases.iter().map(String::as_str),
            );
        }
        if !fallback.is_empty() && fallback != primary {
            if let Some(entry) = live_entry(&mut state, fallback, now_ms) {
                from_live_alias = true;
                if !primary_found {
                    canonical.clone_from(&entry.canonical);
                }
                remember(&mut previous, &entry);
                aliases = merge_aliases(
                    aliases.iter().map(String::as_str),
                    entry.aliases.iter().map(String::as_str),
                );
            }
        }
        if from_live_alias {
            if let Some(entry) = live_group(&mut state, &canonical, now_ms) {
                remember(&mut previous, &entry);
                aliases = merge_aliases(
                    aliases.iter().map(String::as_str),
                    entry.aliases.iter().map(String::as_str),
                );
            }
        } else if live_group(&mut state, &canonical, now_ms).is_some() {
            return Some(canonical);
        }

        aliases = compact_home_session_aliases(merge_aliases(
            aliases.iter().map(String::as_str),
            [canonical.as_str()],
        ));
        for entry in previous.values() {
            remove_group(&mut state, entry);
        }
        set_group(
            &mut state,
            HomeSessionAliasEntry {
                canonical: canonical.clone(),
                expires_at_ms: now_ms.saturating_add(ttl_ms),
                aliases,
            },
        );
        enforce_limit(&mut state, HOME_SESSION_ALIAS_SOFT_LIMIT);
        Some(canonical)
    }

    pub fn clear(&self) {
        *self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = HomeSessionAliasState::default();
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entries
            .len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn live_entry(
    state: &mut HomeSessionAliasState,
    alias: &str,
    now_ms: i64,
) -> Option<HomeSessionAliasEntry> {
    let entry = state.entries.get(alias)?.clone();
    if now_ms < entry.expires_at_ms {
        return Some(entry);
    }
    if state
        .groups
        .get(&entry.canonical)
        .is_some_and(|group| group == &entry)
    {
        remove_group(state, &entry);
    } else {
        state.entries.remove(alias);
    }
    None
}

fn live_group(
    state: &mut HomeSessionAliasState,
    canonical: &str,
    now_ms: i64,
) -> Option<HomeSessionAliasEntry> {
    let entry = state.groups.get(canonical)?.clone();
    if now_ms < entry.expires_at_ms {
        return Some(entry);
    }
    remove_group(state, &entry);
    None
}

fn remember(previous: &mut BTreeMap<String, HomeSessionAliasEntry>, entry: &HomeSessionAliasEntry) {
    previous.insert(entry.canonical.clone(), entry.clone());
}

fn set_group(state: &mut HomeSessionAliasState, entry: HomeSessionAliasEntry) {
    if let Some(existing) = state.groups.get(&entry.canonical).cloned() {
        remove_group(state, &existing);
    }
    state.groups.insert(entry.canonical.clone(), entry.clone());
    for alias in &entry.aliases {
        state.entries.insert(alias.clone(), entry.clone());
    }
    state.eviction_order.push_back(entry.canonical);
}

fn remove_group(state: &mut HomeSessionAliasState, entry: &HomeSessionAliasEntry) {
    if !state
        .groups
        .get(&entry.canonical)
        .is_some_and(|current| current == entry)
    {
        return;
    }
    for alias in &entry.aliases {
        if state
            .entries
            .get(alias)
            .is_some_and(|mapped| mapped == entry)
        {
            state.entries.remove(alias);
        }
    }
    state.groups.remove(&entry.canonical);
    state
        .eviction_order
        .retain(|canonical| canonical != &entry.canonical);
}

fn enforce_limit(state: &mut HomeSessionAliasState, limit: usize) {
    while state.entries.len() > limit {
        let Some(canonical) = state.eviction_order.pop_front() else {
            return;
        };
        if let Some(entry) = state.groups.get(&canonical).cloned() {
            remove_group(state, &entry);
        }
    }
}

fn cleanup(state: &mut HomeSessionAliasState, now_ms: i64) {
    let expired = state
        .groups
        .values()
        .filter(|entry| now_ms >= entry.expires_at_ms)
        .cloned()
        .collect::<Vec<_>>();
    for entry in expired {
        remove_group(state, &entry);
    }
}

fn merge_aliases<'a>(
    existing: impl IntoIterator<Item = &'a str>,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    existing
        .into_iter()
        .chain(candidates)
        .filter_map(|alias| {
            let alias = alias.trim();
            (!alias.is_empty() && seen.insert(alias.to_owned())).then(|| alias.to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

}
