// ref: sdk/cliproxy/auth/scheduler.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: instance-owned flat eligibility views replace Go's mutable incremental indexes while preserving strategies
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::internal::thinking::parse_suffix;

use super::cooldown_state::CooldownStateRecord;
use super::selector::{AccountCandidate, AccountSelectionError};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SchedulerStrategy {
    #[default]
    RoundRobin,
    FillFirst,
    WeightedRoundRobin,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchedulerPickOptions {
    pub pinned_auth_id: Option<String>,
    pub prefer_websocket: bool,
    pub tried_auth_ids: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledAccount {
    pub provider: String,
    pub candidate: AccountCandidate,
}

#[derive(Default)]
struct SmoothWeightedState {
    current: HashMap<String, i64>,
    weights: HashMap<String, i64>,
}

#[derive(Default)]
struct SchedulerState {
    cursors: HashMap<String, usize>,
    weighted: HashMap<String, SmoothWeightedState>,
}

pub(crate) struct StandardSelectionState<'a> {
    pub cooldowns: &'a [CooldownStateRecord],
    pub options: &'a SchedulerPickOptions,
    pub expiries: &'a std::collections::BTreeMap<String, i64>,
}

pub struct AuthScheduler {
    strategy: SchedulerStrategy,
    state: Mutex<SchedulerState>,
}

impl AuthScheduler {
    pub fn new(strategy: SchedulerStrategy) -> Self {
        Self {
            strategy,
            state: Mutex::new(SchedulerState::default()),
        }
    }

    pub fn strategy(&self) -> SchedulerStrategy {
        self.strategy
    }

    pub fn pick_single(
        &self,
        provider: &str,
        model: Option<&str>,
        now_ms: i64,
        candidates: &[AccountCandidate],
        cooldowns: &[CooldownStateRecord],
        options: &SchedulerPickOptions,
    ) -> Result<AccountCandidate, AccountSelectionError> {
        let provider = provider.trim().to_ascii_lowercase();
        if provider.is_empty() {
            return Err(AccountSelectionError::NotFound);
        }
        let mut available = available_candidates(
            std::slice::from_ref(&provider),
            model,
            now_ms,
            candidates,
            cooldowns,
            options,
            self.strategy,
        )?;
        if options.pinned_auth_id.is_none()
            && options.prefer_websocket
            && matches!(provider.as_str(), "codex" | "xai")
            && available
                .iter()
                .any(|candidate| candidate.websocket_enabled)
        {
            available.retain(|candidate| candidate.websocket_enabled);
        }
        retain_highest_priority(&mut available);
        available.sort_by(|left, right| left.auth_id.cmp(&right.auth_id));
        let key = format!("{provider}:{}", canonical_model_key(model.unwrap_or("")));
        self.pick_from_available(&key, &available)
    }

    /// Fixed CTOX setup. Eligibility/cooldowns use the same canonical filter;
    /// fresh requests consume the earliest reported usable window, then the
    /// stable account ID. Portable manual strategies are unchanged.
    pub(crate) fn pick_standard(
        &self,
        provider: &str,
        model: Option<&str>,
        now_ms: i64,
        candidates: &[AccountCandidate],
        state: StandardSelectionState<'_>,
    ) -> Result<AccountCandidate, AccountSelectionError> {
        let StandardSelectionState { cooldowns, options, expiries } = state;
        let provider = provider.trim().to_ascii_lowercase();
        if provider.is_empty() {
            return Err(AccountSelectionError::NotFound);
        }
        let mut available = available_candidates(
            std::slice::from_ref(&provider),
            model,
            now_ms,
            candidates,
            cooldowns,
            options,
            SchedulerStrategy::FillFirst,
        )?;
        if options.pinned_auth_id.is_none()
            && options.prefer_websocket
            && matches!(provider.as_str(), "codex" | "xai")
            && available
                .iter()
                .any(|candidate| candidate.websocket_enabled)
        {
            available.retain(|candidate| candidate.websocket_enabled);
        }
        available.sort_by(|a, b| {
            let rank = |id: &str| {
                expiries
                    .get(id)
                    .copied()
                    .filter(|reset| *reset > now_ms)
                    .map_or((true, 0), |reset| (false, reset))
            };
            rank(&a.auth_id)
                .cmp(&rank(&b.auth_id))
                .then_with(|| a.auth_id.cmp(&b.auth_id))
        });
        available
            .into_iter()
            .next()
            .ok_or(AccountSelectionError::Unavailable)
    }

    pub fn pick_mixed(
        &self,
        providers: &[String],
        model: Option<&str>,
        now_ms: i64,
        candidates: &[AccountCandidate],
        cooldowns: &[CooldownStateRecord],
        options: &SchedulerPickOptions,
    ) -> Result<ScheduledAccount, AccountSelectionError> {
        let providers = normalize_provider_keys(providers);
        if providers.is_empty() {
            return Err(AccountSelectionError::NotFound);
        }
        if providers.len() == 1 {
            let provider = providers[0].clone();
            let candidate =
                self.pick_single(&provider, model, now_ms, candidates, cooldowns, options)?;
            return Ok(ScheduledAccount {
                provider,
                candidate,
            });
        }
        if let Some(pinned) = options
            .pinned_auth_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let candidate = candidates
                .iter()
                .find(|candidate| candidate.auth_id.trim() == pinned)
                .ok_or(AccountSelectionError::NotFound)?;
            if !providers
                .iter()
                .any(|provider| candidate.provider.trim().eq_ignore_ascii_case(provider))
            {
                return Err(AccountSelectionError::NotFound);
            }
        }

        let mut available = available_candidates(
            &providers,
            model,
            now_ms,
            candidates,
            cooldowns,
            options,
            self.strategy,
        )?;
        retain_highest_priority(&mut available);
        if self.strategy == SchedulerStrategy::FillFirst {
            available.sort_by(|left, right| {
                provider_index(&providers, &left.provider)
                    .cmp(&provider_index(&providers, &right.provider))
                    .then_with(|| left.auth_id.cmp(&right.auth_id))
            });
        } else if self.strategy == SchedulerStrategy::WeightedRoundRobin {
            available.sort_by(|left, right| left.auth_id.cmp(&right.auth_id));
        } else {
            available.sort_by(|left, right| {
                provider_index(&providers, &left.provider)
                    .cmp(&provider_index(&providers, &right.provider))
                    .then_with(|| left.auth_id.cmp(&right.auth_id))
            });
        }
        let key = format!(
            "{}:{}",
            providers.join(","),
            canonical_model_key(model.unwrap_or(""))
        );
        let candidate = self.pick_from_available(&key, &available)?;
        Ok(ScheduledAccount {
            provider: candidate.provider.trim().to_ascii_lowercase(),
            candidate,
        })
    }

    fn pick_from_available(
        &self,
        key: &str,
        available: &[AccountCandidate],
    ) -> Result<AccountCandidate, AccountSelectionError> {
        let first = available
            .first()
            .ok_or(AccountSelectionError::Unavailable)?;
        if self.strategy == SchedulerStrategy::FillFirst {
            return Ok(first.clone());
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| AccountSelectionError::State)?;
        if self.strategy == SchedulerStrategy::WeightedRoundRobin {
            return pick_smooth_weighted(key, available, &mut state);
        }
        let cursor = state.cursors.entry(key.to_owned()).or_default();
        if *cursor >= 2_147_483_640 {
            *cursor = 0;
        }
        let selected = available[*cursor % available.len()].clone();
        *cursor += 1;
        Ok(selected)
    }
}

impl fmt::Debug for AuthScheduler {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthScheduler")
            .field("strategy", &self.strategy)
            .finish_non_exhaustive()
    }
}

fn pick_smooth_weighted(
    key: &str,
    available: &[AccountCandidate],
    scheduler: &mut SchedulerState,
) -> Result<AccountCandidate, AccountSelectionError> {
    let weights = available
        .iter()
        .filter(|candidate| candidate.weight > 0)
        .map(|candidate| (candidate.auth_id.clone(), candidate.weight))
        .collect::<HashMap<_, _>>();
    if weights.is_empty() {
        return Err(AccountSelectionError::Unavailable);
    }
    let state = scheduler.weighted.entry(key.to_owned()).or_default();
    if state.weights != weights {
        state.current.clear();
        state.weights = weights;
    }
    let mut picked: Option<&AccountCandidate> = None;
    let mut picked_current = i64::MIN;
    let mut total = 0i64;
    for candidate in available {
        if candidate.weight <= 0 {
            continue;
        }
        let current = state.current.entry(candidate.auth_id.clone()).or_default();
        *current = current.saturating_add(candidate.weight);
        total = total.saturating_add(candidate.weight);
        if picked.is_none() || *current > picked_current {
            picked = Some(candidate);
            picked_current = *current;
        }
    }
    state
        .current
        .retain(|auth_id, _| state.weights.contains_key(auth_id));
    let picked = picked.ok_or(AccountSelectionError::Unavailable)?;
    if let Some(current) = state.current.get_mut(&picked.auth_id) {
        *current = current.saturating_sub(total);
    }
    Ok(picked.clone())
}

fn available_candidates(
    providers: &[String],
    model: Option<&str>,
    now_ms: i64,
    candidates: &[AccountCandidate],
    cooldowns: &[CooldownStateRecord],
    options: &SchedulerPickOptions,
    strategy: SchedulerStrategy,
) -> Result<Vec<AccountCandidate>, AccountSelectionError> {
    let requested_model = canonical_model_key(model.unwrap_or(""));
    let pinned = options
        .pinned_auth_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut found = 0usize;
    let mut blocked = 0usize;
    let mut earliest_retry_ms = None;
    let mut available = Vec::new();
    for candidate in candidates {
        if candidate.auth_id.trim().is_empty()
            || !providers
                .iter()
                .any(|provider| candidate.provider.trim().eq_ignore_ascii_case(provider))
            || pinned.is_some_and(|pinned| candidate.auth_id.trim() != pinned)
            || options.tried_auth_ids.contains(candidate.auth_id.trim())
            || (!candidate.supported_models.is_empty()
                && !candidate
                    .supported_models
                    .iter()
                    .any(|model| canonical_model_key(model) == requested_model))
        {
            continue;
        }
        if strategy == SchedulerStrategy::WeightedRoundRobin && candidate.weight <= 0 {
            continue;
        }
        found += 1;
        if candidate.disabled {
            blocked += 1;
            continue;
        }
        let mut cooling = false;
        for cooldown in cooldowns.iter().filter(|cooldown| {
            cooldown
                .provider
                .trim()
                .eq_ignore_ascii_case(candidate.provider.trim())
                && cooldown.auth_id.trim() == candidate.auth_id.trim()
                && match cooldown.model.as_deref().map(canonical_model_key) {
                    None => true,
                    Some(state_model) if state_model.is_empty() => true,
                    Some(state_model) => state_model == requested_model,
                }
        }) {
            if !cooldown.is_available_at(now_ms) {
                cooling = true;
                if let Some(next) = cooldown.blocking_until_ms().filter(|next| *next > now_ms) {
                    earliest_retry_ms =
                        Some(earliest_retry_ms.map_or(next, |current: i64| current.min(next)));
                }
            }
        }
        if cooling {
            blocked += 1;
        } else {
            available.push(candidate.clone());
        }
    }
    if !available.is_empty() {
        return Ok(available);
    }
    if found == 0 {
        return Err(AccountSelectionError::NotFound);
    }
    if blocked == found {
        return earliest_retry_ms
            .map_or(Err(AccountSelectionError::Unavailable), |retry_after_ms| {
                Err(AccountSelectionError::Cooldown { retry_after_ms })
            });
    }
    Err(AccountSelectionError::Unavailable)
}

fn retain_highest_priority(candidates: &mut Vec<AccountCandidate>) {
    if let Some(priority) = candidates.iter().map(|candidate| candidate.priority).max() {
        candidates.retain(|candidate| candidate.priority == priority);
    }
}

fn provider_index(providers: &[String], provider: &str) -> usize {
    providers
        .iter()
        .position(|candidate| candidate.eq_ignore_ascii_case(provider.trim()))
        .unwrap_or(usize::MAX)
}

pub fn canonical_model_key(model: &str) -> String {
    let model = model.trim();
    let parsed = parse_suffix(model);
    if parsed.has_suffix && !parsed.model_name.trim().is_empty() {
        parsed.model_name.trim().to_owned()
    } else {
        model.to_owned()
    }
}

fn normalize_provider_keys(providers: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for provider in providers {
        let provider = provider.trim().to_ascii_lowercase();
        if !provider.is_empty() && seen.insert(provider.clone()) {
            output.push(provider);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, provider: &str, priority: i32, weight: i64) -> AccountCandidate {
        AccountCandidate {
            auth_id: id.to_owned(),
            provider: provider.to_owned(),
            priority,
            weight,
            websocket_enabled: false,
            supported_models: Vec::new(),
            disabled: false,
        }
    }

    #[test]
    fn smooth_weighted_and_weight_change_match_upstream_shape() {
        let scheduler = AuthScheduler::new(SchedulerStrategy::WeightedRoundRobin);
        let mut candidates = vec![
            candidate("a", "gemini", 0, 5),
            candidate("b", "gemini", 0, 3),
            candidate("c", "gemini", 0, 2),
        ];
        let mut counts = HashMap::new();
        for _ in 0..100 {
            let picked = scheduler
                .pick_single(
                    "gemini",
                    None,
                    0,
                    &candidates,
                    &[],
                    &SchedulerPickOptions::default(),
                )
                .unwrap();
            *counts.entry(picked.auth_id).or_insert(0) += 1;
        }
        assert_eq!(counts.get("a"), Some(&50));
        assert_eq!(counts.get("b"), Some(&30));
        assert_eq!(counts.get("c"), Some(&20));
        candidates[0].weight = 1;
        candidates[1].weight = 1;
        candidates.truncate(2);
        let picks = (0..4)
            .map(|_| {
                scheduler
                    .pick_single(
                        "gemini",
                        None,
                        0,
                        &candidates,
                        &[],
                        &SchedulerPickOptions::default(),
                    )
                    .unwrap()
                    .auth_id
            })
            .collect::<Vec<_>>();
        assert_eq!(picks, ["a", "b", "a", "b"]);
    }

    #[test]
    fn mixed_round_robin_weights_providers_by_ready_account_count() {
        let scheduler = AuthScheduler::new(SchedulerStrategy::RoundRobin);
        let candidates = vec![
            candidate("gemini-a", "gemini", 0, 1),
            candidate("gemini-b", "gemini", 0, 1),
            candidate("claude-a", "claude", 0, 1),
        ];
        let providers = vec!["gemini".to_owned(), "claude".to_owned()];
        let picks = (0..4)
            .map(|_| {
                let picked = scheduler
                    .pick_mixed(
                        &providers,
                        None,
                        0,
                        &candidates,
                        &[],
                        &SchedulerPickOptions::default(),
                    )
                    .unwrap();
                (picked.provider, picked.candidate.auth_id)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            picks,
            [
                ("gemini".to_owned(), "gemini-a".to_owned()),
                ("gemini".to_owned(), "gemini-b".to_owned()),
                ("claude".to_owned(), "claude-a".to_owned()),
                ("gemini".to_owned(), "gemini-a".to_owned())
            ]
        );
    }

    #[test]
    fn websocket_preference_precedes_priority_unless_pinned() {
        let scheduler = AuthScheduler::new(SchedulerStrategy::RoundRobin);
        let mut http = candidate("codex-http", "codex", 10, 1);
        http.websocket_enabled = false;
        let mut websocket = candidate("codex-ws", "codex", 0, 1);
        websocket.websocket_enabled = true;
        let candidates = vec![http, websocket];
        let options = SchedulerPickOptions {
            prefer_websocket: true,
            ..SchedulerPickOptions::default()
        };
        assert_eq!(
            scheduler
                .pick_single("codex", None, 0, &candidates, &[], &options)
                .unwrap()
                .auth_id,
            "codex-ws"
        );
        let pinned = SchedulerPickOptions {
            pinned_auth_id: Some("codex-http".to_owned()),
            prefer_websocket: true,
            ..SchedulerPickOptions::default()
        };
        assert_eq!(
            scheduler
                .pick_single("codex", None, 0, &candidates, &[], &pinned)
                .unwrap()
                .auth_id,
            "codex-http"
        );
    }

    #[test]
    fn suffix_models_share_state_but_prefixed_aliases_do_not() {
        assert_eq!(canonical_model_key(" gpt-5.4(high) "), "gpt-5.4");
        assert_eq!(canonical_model_key("team-a/shared"), "team-a/shared");
        assert_eq!(canonical_model_key("broken(high"), "broken(high");
    }
}
