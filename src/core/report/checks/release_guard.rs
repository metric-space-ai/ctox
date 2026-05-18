//! Deterministic release-guard check.
//!
//! Implementation of the 34-lint catalogue defined in
//! `skills/system/research/systematic-research/references/release_guard_lints.md`.
//! Each lint is a small zero-sized struct implementing the [`Lint`]
//! trait; the dispatcher consults the applicability matrix encoded in
//! [`Lint::applies`] before running each lint.
//!
//! The output payload is fully deterministic: lints emit issues in
//! `(lint_id, instance_id)` ascending order so identical workspace
//! state produces identical output (turn-ledger replays depend on
//! this).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::report::asset_pack::{AssetPack, DocumentBlueprint, ReportType, StyleGuidance};
use crate::report::checks::{dedupe_keep_order, CheckOutcome};
use crate::report::workspace::{BlockRecord, EvidenceEntry, Workspace};

const CHECK_KIND: &str = "release_guard";

/// Severity classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    Hard,
    Soft,
    Critical,
}

impl LintSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            LintSeverity::Hard => "hard",
            LintSeverity::Soft => "soft",
            LintSeverity::Critical => "critical",
        }
    }
}

/// One lint hit emitted by [`Lint::check`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintIssue {
    pub lint_id: &'static str,
    pub severity: LintSeverity,
    pub instance_ids: Vec<String>,
    pub reason: String,
    pub goal: String,
}

impl Serialize for LintSeverity {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LintSeverity {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "hard" => Ok(LintSeverity::Hard),
            "soft" => Ok(LintSeverity::Soft),
            "critical" => Ok(LintSeverity::Critical),
            other => Err(serde::de::Error::custom(format!(
                "unknown lint severity: {other}"
            ))),
        }
    }
}

/// Compact context handed to every lint.
pub struct LintContext<'a> {
    pub report_type_id: &'a str,
    pub depth_profile_id: &'a str,
    pub report_type: &'a ReportType,
    pub style_guidance: &'a StyleGuidance,
    pub committed_blocks: &'a [BlockRecord],
    pub evidence_register: &'a [EvidenceEntry],
    pub asset_pack: &'a AssetPack,
    pub document_blueprint: &'a DocumentBlueprint,
    pub language: &'a str,
}

/// Lint trait — every catalogue entry implements this.
pub trait Lint {
    fn id(&self) -> &'static str;
    fn applies(&self, ctx: &LintContext) -> bool;
    fn severity(&self) -> LintSeverity;
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue>;
}

// ---- catalogue -----------------------------------------------------------

/// Build the 34-lint catalogue. The order here is informational; the
/// dispatcher sorts emitted issues deterministically by `(lint_id,
/// instance_id)` before assembling the payload.
fn build_catalogue() -> Vec<Box<dyn Lint>> {
    let mut out: Vec<Box<dyn Lint>> = Vec::with_capacity(34);
    // Evidence integrity (9)
    out.push(Box::new(LintFabDoi));
    out.push(Box::new(LintFabArxiv));
    out.push(Box::new(LintFabAuthor));
    out.push(Box::new(LintUncitedClaim));
    out.push(Box::new(LintCitedButMissing));
    out.push(Box::new(LintDoiNotResolved));
    out.push(Box::new(LintEvidenceFloor));
    out.push(Box::new(LintEvidenceConcentration));
    out.push(Box::new(LintStubEvidence));
    // Anti-slop language (6)
    out.push(Box::new(LintDeadPhrase));
    out.push(Box::new(LintMetaPhrase));
    out.push(Box::new(LintConsultantOveruse));
    out.push(Box::new(LintUnanchoredHedge));
    out.push(Box::new(LintFillerOpening));
    out.push(Box::new(LintInvertedPerspective));
    // Matrix integrity (4)
    out.push(Box::new(LintDuplicateRationale));
    out.push(Box::new(LintVerdictMismatch));
    out.push(Box::new(LintAxisCompleteness));
    out.push(Box::new(LintRubricMismatch));
    // Structural integrity (4)
    out.push(Box::new(LintMinChars));
    out.push(Box::new(LintMaxChars));
    out.push(Box::new(LintMissingDisclaimer));
    out.push(Box::new(LintDuplicateSectionOpening));
    // Market-research (4)
    out.push(Box::new(LintMrUnquantifiedMarket));
    out.push(Box::new(LintMrMethodMissing));
    out.push(Box::new(LintMrCompetitorNameless));
    out.push(Box::new(LintMrSegmentWithoutSize));
    // Whitepaper (3)
    out.push(Box::new(LintWpThesisDrift));
    out.push(Box::new(LintWpEvidenceMissingForClaim));
    out.push(Box::new(LintWpFillerOpening));
    // Decision-brief (3)
    out.push(Box::new(LintDbRecommendationBuried));
    out.push(Box::new(LintDbHedgeRecommendation));
    out.push(Box::new(LintDbCriteriaWithoutWeights));
    // Literature-review (2)
    out.push(Box::new(LintLrThemeImbalance));
    out.push(Box::new(LintLrNoGapsSection));
    out
}

/// Public entry point.
pub fn run_release_guard_check(workspace: &Workspace) -> Result<CheckOutcome> {
    let metadata = workspace.run_metadata()?;
    let asset_pack = AssetPack::load()?;
    let report_type = asset_pack.report_type(&metadata.report_type_id)?;
    let document_blueprint = asset_pack
        .document_blueprint(&report_type.document_blueprint_id)
        .unwrap_or(DocumentBlueprint {
            base_docs: Vec::new(),
            sequence: Vec::new(),
        });

    let committed = workspace.committed_blocks()?;
    let evidence = workspace.evidence_register()?;

    if committed.is_empty() {
        let payload = json!({
            "summary": "Keine populated blocks — Release-Guard übersprungen.",
            "check_applicable": false,
            "ready_to_finish": true,
            "needs_revision": false,
            "candidate_instance_ids": Value::Array(Vec::new()),
            "goals": Value::Array(Vec::new()),
            "reasons": Value::Array(Vec::new()),
            "issues": Value::Array(Vec::new()),
        });
        return Ok(CheckOutcome {
            check_kind: CHECK_KIND.to_string(),
            summary: "Keine populated blocks — Release-Guard übersprungen.".to_string(),
            check_applicable: false,
            ready_to_finish: true,
            needs_revision: false,
            candidate_instance_ids: Vec::new(),
            goals: Vec::new(),
            reasons: Vec::new(),
            raw_payload: payload,
        }
        .cap());
    }

    let style_guidance = asset_pack.style_guidance();
    let ctx = LintContext {
        report_type_id: &metadata.report_type_id,
        depth_profile_id: &metadata.depth_profile_id,
        report_type,
        style_guidance,
        committed_blocks: &committed,
        evidence_register: &evidence,
        asset_pack,
        document_blueprint: &document_blueprint,
        language: &metadata.language,
    };

    let catalogue = build_catalogue();
    let mut issues: Vec<LintIssue> = Vec::new();
    for lint in &catalogue {
        if !lint.applies(&ctx) {
            continue;
        }
        for issue in lint.check(&ctx) {
            issues.push(issue);
        }
    }

    // Deterministic ordering: lint_id ascending, then instance_id
    // ascending (joined as a stable secondary key).
    issues.sort_by(|a, b| {
        a.lint_id
            .cmp(b.lint_id)
            .then_with(|| a.instance_ids.join(",").cmp(&b.instance_ids.join(",")))
    });

    let any_hard_or_critical = issues
        .iter()
        .any(|i| matches!(i.severity, LintSeverity::Hard | LintSeverity::Critical));
    let ready_to_finish = !any_hard_or_critical;
    let needs_revision = any_hard_or_critical;

    // Candidate ids: hard+critical first, then soft, deduplicated, capped 6.
    let mut hard_ids: Vec<String> = Vec::new();
    let mut soft_ids: Vec<String> = Vec::new();
    for issue in &issues {
        let bucket = match issue.severity {
            LintSeverity::Hard | LintSeverity::Critical => &mut hard_ids,
            LintSeverity::Soft => &mut soft_ids,
        };
        for id in &issue.instance_ids {
            bucket.push(id.clone());
        }
    }
    let mut candidate: Vec<String> = Vec::new();
    candidate.extend(hard_ids);
    candidate.extend(soft_ids);
    let candidate = dedupe_keep_order(candidate);

    let goals = dedupe_keep_order(issues.iter().map(|i| i.goal.clone()).collect::<Vec<_>>());
    let reasons = dedupe_keep_order(issues.iter().map(|i| i.reason.clone()).collect::<Vec<_>>());

    let summary = if issues.is_empty() {
        "Keine harten Freigabe- oder Stilrisiken gefunden.".to_string()
    } else {
        format!("{} Freigabe- bzw. Stilrisiken erkannt.", issues.len())
    };

    let issues_payload: Vec<Value> = issues
        .iter()
        .map(|i| {
            json!({
                "lint_id": i.lint_id,
                "severity": i.severity.as_str(),
                "instance_ids": i.instance_ids,
                "reason": i.reason,
                "goal": i.goal,
            })
        })
        .collect();

    let payload = json!({
        "summary": summary,
        "check_applicable": true,
        "ready_to_finish": ready_to_finish,
        "needs_revision": needs_revision,
        "candidate_instance_ids": candidate,
        "goals": goals,
        "reasons": reasons,
        "issues": issues_payload,
    });

    Ok(CheckOutcome {
        check_kind: CHECK_KIND.to_string(),
        summary,
        check_applicable: true,
        ready_to_finish,
        needs_revision,
        candidate_instance_ids: candidate,
        goals,
        reasons,
        raw_payload: payload,
    }
    .cap())
}

// ---- shared helpers ------------------------------------------------------

fn report_type_has_matrix(rt: &ReportType) -> bool {
    let needles = [
        "screening_matrix",
        "scenario_matrix",
        "screening_matrix_short",
        "competitor_matrix",
    ];
    rt.block_library_keys
        .iter()
        .any(|k| needles.iter().any(|n| k == n))
}

fn lower_chars(s: &str) -> String {
    s.to_lowercase()
}

fn split_paragraphs(markdown: &str) -> Vec<&str> {
    markdown
        .split("\n\n")
        .map(|p| p.trim_matches(|c: char| c == '\r' || c == '\n'))
        .filter(|p| !p.is_empty())
        .collect()
}

fn split_sentences(text: &str) -> Vec<String> {
    // Split on period, exclamation, question mark followed by whitespace
    // or end-of-string.
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        cur.push(chars[i]);
        if matches!(chars[i], '.' | '!' | '?') {
            // Look ahead: end-of-string or whitespace -> break.
            let next_is_ws_or_end = i + 1 >= chars.len() || chars[i + 1].is_whitespace();
            if next_is_ws_or_end {
                let trimmed = cur.trim().to_string();
                if !trimmed.is_empty() {
                    out.push(trimmed);
                }
                cur.clear();
            }
        }
        i += 1;
    }
    let leftover = cur.trim().to_string();
    if !leftover.is_empty() {
        out.push(leftover);
    }
    out
}

fn snippet_60(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i >= 60 {
            break;
        }
        out.push(c);
    }
    out
}

fn cap_per_block(seen: &mut HashMap<String, usize>, instance_id: &str, max: usize) -> bool {
    let entry = seen.entry(instance_id.to_string()).or_insert(0);
    if *entry >= max {
        return false;
    }
    *entry += 1;
    true
}

fn shingles_3(words: &[String]) -> HashSet<String> {
    let mut out: HashSet<String> = HashSet::new();
    if words.len() < 3 {
        if !words.is_empty() {
            out.insert(words.join(" "));
        }
        return out;
    }
    for w in words.windows(3) {
        out.insert(w.join(" "));
    }
    out
}

fn jaccard_similarity(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let inter: usize = a.intersection(b).count();
    let union: usize = a.union(b).count();
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

fn tokenise_words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect()
}

fn extract_capitalised_ngrams(text: &str, max_n: usize) -> Vec<String> {
    let tokens: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':' | '(' | ')'))
        .filter(|t| !t.is_empty())
        .collect();
    let is_cap = |s: &str| {
        let first = s.chars().next();
        matches!(first, Some(c) if c.is_uppercase())
    };
    let mut out: Vec<String> = Vec::new();
    let n = tokens.len();
    let mut i = 0;
    while i < n {
        if !is_cap(tokens[i]) {
            i += 1;
            continue;
        }
        let mut run: Vec<&str> = vec![tokens[i]];
        let mut j = i + 1;
        while j < n && is_cap(tokens[j]) {
            run.push(tokens[j]);
            j += 1;
            if run.len() >= max_n {
                break;
            }
        }
        // Emit one canonical span (full run) — keeps the space small.
        let span: Vec<String> = run
            .iter()
            .map(|s| s.trim_matches('.').to_string())
            .collect();
        if !span.is_empty() {
            let valid: Vec<String> = span
                .iter()
                .filter(|s| s.chars().count() >= 3)
                .cloned()
                .collect();
            if !valid.is_empty() {
                out.push(valid.join(" "));
            }
        }
        i = if j > i { j } else { i + 1 };
    }
    out
}

fn block_used_reference_ids_set(block: &BlockRecord) -> HashSet<String> {
    block.used_reference_ids.iter().cloned().collect()
}

// ---- DOI / arXiv helpers -------------------------------------------------

fn doi_regex() -> &'static Regex {
    static R: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| Regex::new(r"10\.\d{4,9}/[-._;()/:A-Za-z0-9]+").expect("doi regex compiles"))
}

fn arxiv_regex() -> &'static Regex {
    static R: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"(?i)arXiv:?\s*(\d{4}\.\d{4,5})(v\d+)?").expect("arxiv regex compiles")
    })
}

fn author_year_regex() -> &'static Regex {
    static R: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| {
        // First author + optional et-al/und-form + year.
        Regex::new(
            r"([A-ZÄÖÜ][a-zäöüß]+)(?:\s+(?:et\s+al\.|und|and|&)\s+([A-ZÄÖÜ][a-zäöüß]+))?\s*(?:\(|,\s*)?((?:19|20)\d{2})\b",
        )
        .expect("author-year regex compiles")
    })
}

fn extract_dois(text: &str) -> Vec<String> {
    doi_regex()
        .find_iter(text)
        .filter_map(|m| normalize_doi_fragment(m.as_str()))
        .collect()
}

fn normalize_doi_fragment(raw: &str) -> Option<String> {
    let mut doi = raw
        .trim()
        .trim_start_matches("doi:")
        .trim_start_matches("DOI:")
        .to_ascii_lowercase();
    while matches!(
        doi.chars().last(),
        Some('.' | ',' | ';' | ':' | ')' | ']' | '}' | '>')
    ) {
        doi.pop();
    }
    if doi.starts_with("10.") {
        Some(doi)
    } else {
        None
    }
}

fn extract_arxiv_ids(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for cap in arxiv_regex().captures_iter(text) {
        if let Some(m) = cap.get(1) {
            out.push(m.as_str().to_string());
        }
    }
    out
}

fn evidence_dois(register: &[EvidenceEntry]) -> HashSet<String> {
    let mut out = HashSet::new();
    for entry in register {
        if entry.kind.eq_ignore_ascii_case("doi") {
            if let Some(id) = entry
                .canonical_id
                .as_deref()
                .and_then(normalize_doi_fragment)
            {
                out.insert(id);
            }
        }
        for value in [
            entry.canonical_id.as_deref(),
            entry.url_canonical.as_deref(),
            entry.url_full_text.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            out.extend(extract_dois(value));
        }
    }
    out
}

fn evidence_arxiv_ids(register: &[EvidenceEntry]) -> HashSet<String> {
    register
        .iter()
        .filter(|e| e.kind.eq_ignore_ascii_case("arxiv"))
        .filter_map(|e| e.canonical_id.as_ref())
        .map(|id| {
            let s = id.trim().to_string();
            // Strip leading arXiv: and trailing vN.
            let lower = s.to_lowercase();
            let stripped = lower
                .trim_start_matches("arxiv:")
                .trim_start_matches("arxiv ")
                .trim();
            // strip vN suffix
            let mut tail = stripped.to_string();
            if let Some(idx) = tail.rfind('v') {
                if tail[idx + 1..].chars().all(|c| c.is_ascii_digit()) {
                    tail.truncate(idx);
                }
            }
            tail
        })
        .collect()
}

fn evidence_first_author_year(register: &[EvidenceEntry]) -> HashSet<(String, i64)> {
    let mut out: HashSet<(String, i64)> = HashSet::new();
    for e in register {
        if let (Some(first), Some(year)) = (e.authors.first(), e.year) {
            // `authors[0]` may be "Family, Given" or "Family"; take
            // the leading substring up to comma/space as the family
            // candidate.
            let family = first
                .split(|c: char| c == ',' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .to_lowercase();
            if !family.is_empty() {
                out.insert((family, year));
            }
        }
    }
    out
}

// ---- Lint implementations ------------------------------------------------

// ============ Evidence integrity (8) ============

struct LintFabDoi;

impl Lint for LintFabDoi {
    fn id(&self) -> &'static str {
        "LINT-FAB-DOI"
    }
    fn applies(&self, _ctx: &LintContext) -> bool {
        true
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Critical
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let registered = evidence_dois(ctx.evidence_register);
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let mut seen: HashSet<String> = HashSet::new();
            for doi in extract_dois(&block.markdown) {
                if registered.contains(&doi) || !seen.insert(doi.clone()) {
                    continue;
                }
                out.push(LintIssue {
                    lint_id: self.id(),
                    severity: self.severity(),
                    instance_ids: vec![block.instance_id.clone()],
                    reason: format!(
                        "Block {title} zitiert eine DOI ({doi}), die nicht im Evidence-Register dieses Runs auftaucht.",
                        title = block.title,
                        doi = doi,
                    ),
                    goal: format!(
                        "Entferne in {title} die DOI {doi} oder belege sie zuerst über public_research und ergänze den Eintrag im Evidence-Register.",
                        title = block.title,
                        doi = doi,
                    ),
                });
            }
        }
        out
    }
}

struct LintFabArxiv;

impl Lint for LintFabArxiv {
    fn id(&self) -> &'static str {
        "LINT-FAB-ARXIV"
    }
    fn applies(&self, _ctx: &LintContext) -> bool {
        true
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let registered = evidence_arxiv_ids(ctx.evidence_register);
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let mut seen: HashSet<String> = HashSet::new();
            for raw in extract_arxiv_ids(&block.markdown) {
                let id = raw.to_lowercase();
                if registered.contains(&id) || !seen.insert(id.clone()) {
                    continue;
                }
                out.push(LintIssue {
                    lint_id: self.id(),
                    severity: self.severity(),
                    instance_ids: vec![block.instance_id.clone()],
                    reason: format!(
                        "Block {title} verweist auf arXiv {arxiv}, ohne dass dieser Eintrag im Evidence-Register existiert.",
                        title = block.title,
                        arxiv = raw,
                    ),
                    goal: format!(
                        "Belege {arxiv} über public_research bevor er in {title} stehenbleibt; ansonsten entfernen.",
                        title = block.title,
                        arxiv = raw,
                    ),
                });
            }
        }
        out
    }
}

struct LintFabAuthor;

impl Lint for LintFabAuthor {
    fn id(&self) -> &'static str {
        "LINT-FAB-AUTHOR"
    }
    fn applies(&self, _ctx: &LintContext) -> bool {
        true
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Critical
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let registered = evidence_first_author_year(ctx.evidence_register);
        let deny_leading = [
            "tabelle",
            "abbildung",
            "stand",
            "ausgabe",
            "version",
            "im",
            "am",
            "kapitel",
        ];
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let mut seen: HashSet<String> = HashSet::new();
            let mut emitted = 0usize;
            for cap in author_year_regex().captures_iter(&block.markdown) {
                if emitted >= 5 {
                    break;
                }
                let family = cap
                    .get(1)
                    .map(|m| m.as_str().to_lowercase())
                    .unwrap_or_default();
                if family.is_empty() || deny_leading.contains(&family.as_str()) {
                    continue;
                }
                let year_str = cap.get(3).map(|m| m.as_str()).unwrap_or("");
                let year: i64 = match year_str.parse() {
                    Ok(y) => y,
                    Err(_) => continue,
                };
                let key = format!("{family}__{year}");
                if !seen.insert(key) {
                    continue;
                }
                let matches_register = registered
                    .iter()
                    .any(|(fam, y)| family.contains(fam) || fam.contains(&family) && *y == year);
                let matches_register = matches_register
                    || registered
                        .iter()
                        .any(|(fam, y)| *y == year && fam == &family);
                if matches_register {
                    continue;
                }
                let citation = cap.get(0).map(|m| m.as_str()).unwrap_or("").to_string();
                out.push(LintIssue {
                    lint_id: self.id(),
                    severity: self.severity(),
                    instance_ids: vec![block.instance_id.clone()],
                    reason: format!(
                        "In {title} wirkt {citation} wie ein Autor-Jahr-Verweis, hat aber keinen Treffer im Evidence-Register.",
                        title = block.title,
                        citation = citation,
                    ),
                    goal: format!(
                        "Hinterlege {citation} im Evidence-Register oder entferne den Verweis aus {title}.",
                        title = block.title,
                        citation = citation,
                    ),
                });
                emitted += 1;
            }
        }
        out
    }
}

struct LintUncitedClaim;

impl Lint for LintUncitedClaim {
    fn id(&self) -> &'static str {
        "LINT-UNCITED-CLAIM"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        // Universal except "conditional" for whitepaper.
        if ctx.report_type_id == "whitepaper" {
            // Conditional on argument blocks existing — treat as
            // applicable only when an `argument_section` template id
            // is in the report-type's block_library_keys[].
            return ctx
                .report_type
                .block_library_keys
                .iter()
                .any(|k| k == "argument_section");
        }
        true
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Critical
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let target_templates: HashSet<&str> =
            ["management_summary", "detail_assessment", "recommendation"]
                .into_iter()
                .collect();
        let unit_re = Regex::new(
            r"(?i)\d+(?:[.,]\d+)?\s*(?:%|°C|K|GHz|THz|MHz|kHz|µm|um|nm|mm|cm|m|kV|kW|kA|A|V|s|ms|µs|us|min|h|m²/h|m\^2/h)",
        )
        .expect("unit regex compiles");
        let method_re = Regex::new(
            r"(?i)(?:POD|Auflösung|Aufloesung|Sensitivität|Sensitivitaet|Durchsatz|Frequenz|Wellenlänge|Wellenlaenge|TRL)\s*(?:von|bei|≥|>=|>|<|≤|<=|=|:)\s*[\w\d.,]+",
        )
        .expect("method regex compiles");

        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let template_id = block
                .block_template_id
                .as_deref()
                .unwrap_or(&block.block_id);
            if !target_templates.contains(template_id) {
                continue;
            }
            // Per-paragraph check; we approximate the per-paragraph
            // `used_reference_ids[]` mapping by treating the block-
            // level array as the upper bound: if the block array is
            // empty, every paragraph is uncited.
            let block_refs_empty = block.used_reference_ids.is_empty();
            for paragraph in split_paragraphs(&block.markdown) {
                if !block_refs_empty {
                    continue;
                }
                let has_unit = unit_re.is_match(paragraph);
                let has_method = method_re.is_match(paragraph);
                if !has_unit && !has_method {
                    continue;
                }
                let snippet = snippet_60(paragraph);
                out.push(LintIssue {
                    lint_id: self.id(),
                    severity: self.severity(),
                    instance_ids: vec![block.instance_id.clone()],
                    reason: format!(
                        "Quantitative Aussage in {title} („{snippet}…“) ist nicht über used_reference_ids[] belegt.",
                        title = block.title,
                        snippet = snippet,
                    ),
                    goal: format!(
                        "Verknüpfe in {title} den Satz „{snippet}…“ mit der zugehörigen Quelle aus dem Evidence-Register oder ersetze die Zahl durch eine qualitative Einordnung.",
                        title = block.title,
                        snippet = snippet,
                    ),
                });
                break; // one issue per block is enough
            }
        }
        out
    }
}

struct LintCitedButMissing;

impl Lint for LintCitedButMissing {
    fn id(&self) -> &'static str {
        "LINT-CITED-BUT-MISSING"
    }
    fn applies(&self, _ctx: &LintContext) -> bool {
        true
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let known_ids: HashSet<&str> = ctx
            .evidence_register
            .iter()
            .map(|e| e.evidence_id.as_str())
            .collect();
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let mut seen: HashSet<String> = HashSet::new();
            for ref_id in &block.used_reference_ids {
                if known_ids.contains(ref_id.as_str()) {
                    continue;
                }
                if !seen.insert(ref_id.clone()) {
                    continue;
                }
                out.push(LintIssue {
                    lint_id: self.id(),
                    severity: self.severity(),
                    instance_ids: vec![block.instance_id.clone()],
                    reason: format!(
                        "Block {title} verweist auf Reference-ID {ref_id}, die nicht im Evidence-Register vorhanden ist.",
                        title = block.title,
                        ref_id = ref_id,
                    ),
                    goal: format!(
                        "Lege {ref_id} im Evidence-Register an oder entferne die Verknüpfung aus {title}.",
                        ref_id = ref_id,
                        title = block.title,
                    ),
                });
            }
        }
        out
    }
}

struct LintDoiNotResolved;

impl Lint for LintDoiNotResolved {
    fn id(&self) -> &'static str {
        "LINT-DOI-NOT-RESOLVED"
    }
    fn applies(&self, _ctx: &LintContext) -> bool {
        true
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        // The `EvidenceEntry` carries `resolver_used` but no explicit
        // `crossref_status`. We treat a registered DOI without an
        // `integrity_hash` AND with `resolver_used` flagged
        // unresolved/timeout/error/not_found as failed.
        let unresolved = ["not_found", "timeout", "error", "unresolved"];
        let mut out: Vec<LintIssue> = Vec::new();
        for entry in ctx.evidence_register {
            if !entry.kind.eq_ignore_ascii_case("doi") {
                continue;
            }
            let resolver = entry
                .resolver_used
                .as_deref()
                .map(|s| s.to_lowercase())
                .unwrap_or_default();
            let bad = unresolved.iter().any(|u| resolver.contains(u));
            if !bad {
                continue;
            }
            // Reverse-lookup blocks that cite this evidence id.
            let ref_id = entry.evidence_id.as_str();
            let mut instance_ids: Vec<String> = Vec::new();
            for block in ctx.committed_blocks {
                if block
                    .used_reference_ids
                    .iter()
                    .any(|r| r.as_str() == ref_id)
                {
                    instance_ids.push(block.instance_id.clone());
                }
            }
            let doi = entry.canonical_id.clone().unwrap_or_default();
            out.push(LintIssue {
                lint_id: self.id(),
                severity: self.severity(),
                instance_ids,
                reason: format!(
                    "Quelle {ref_id} (DOI {doi}) wurde von Crossref mit Status {status} zurückgewiesen.",
                    ref_id = ref_id,
                    doi = doi,
                    status = resolver,
                ),
                goal: format!(
                    "Ersetze {ref_id} durch eine Quelle mit auflösbarer DOI oder hinterlege eine bestätigte Alternative.",
                    ref_id = ref_id,
                ),
            });
        }
        out
    }
}

/// LINT-STUB-EVIDENCE — cited evidence must carry real source content
/// (abstract_md or snippet_md, ≥200 chars combined).
///
/// Catches the failure mode where the agent registered evidence rows
/// from titles only, then cited them. Without real source content the
/// agent cannot have *read* anything, so any prose claim attached to
/// such an evidence_id is necessarily fabricated. The CLI guard in
/// `add-evidence` already rejects new stub rows; this lint also catches
/// pre-existing stubs imported from older runs.
struct LintStubEvidence;

impl Lint for LintStubEvidence {
    fn id(&self) -> &'static str {
        "LINT-STUB-EVIDENCE"
    }
    fn applies(&self, _ctx: &LintContext) -> bool {
        true
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        // Build a quick lookup of evidence_id → content_chars.
        let mut content_chars: HashMap<&str, usize> = HashMap::new();
        for entry in ctx.evidence_register {
            let abs_len = entry
                .abstract_md
                .as_deref()
                .map(|s| s.chars().count())
                .unwrap_or(0);
            let snip_len = entry
                .snippet_md
                .as_deref()
                .map(|s| s.chars().count())
                .unwrap_or(0);
            content_chars.insert(entry.evidence_id.as_str(), abs_len + snip_len);
        }

        let mut out: Vec<LintIssue> = Vec::new();
        let mut already_flagged: HashSet<String> = HashSet::new();
        for block in ctx.committed_blocks {
            for ref_id in &block.used_reference_ids {
                let chars = content_chars.get(ref_id.as_str()).copied().unwrap_or(0);
                if chars >= 200 {
                    continue;
                }
                if !already_flagged.insert(ref_id.clone()) {
                    continue;
                }
                out.push(LintIssue {
                    lint_id: self.id(),
                    severity: self.severity(),
                    instance_ids: vec![block.instance_id.clone()],
                    reason: format!(
                        "Block {title} zitiert {ref_id}, deren Eintrag im Evidence-Register weniger als 200 Zeichen Quellinhalt trägt ({chars} chars).",
                        title = block.title,
                        ref_id = ref_id,
                        chars = chars,
                    ),
                    goal: format!(
                        "Lade die Quelle für {ref_id} mit `ctox web read` und re-registriere via `ctox report add-evidence --abstract-file`, oder ersetze die Zitation in {title} durch eine Evidenz mit echtem Inhalt.",
                        ref_id = ref_id,
                        title = block.title,
                    ),
                });
            }
        }
        out
    }
}

struct LintEvidenceFloor;

impl Lint for LintEvidenceFloor {
    fn id(&self) -> &'static str {
        "LINT-EVIDENCE-FLOOR"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        // Conditional on decision_brief: only when the run actually
        // populates an evidence register at all (ie. > 0 entries
        // counts as applicable; the depth_profile then dictates the
        // required floor).
        if ctx.report_type_id == "decision_brief" {
            return !ctx.evidence_register.is_empty();
        }
        true
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let actual = ctx.evidence_register.len() as u32;
        let required = ctx
            .asset_pack
            .depth_profile(ctx.depth_profile_id)
            .ok()
            .and_then(|depth| {
                depth.min_evidence_count.or_else(|| {
                    depth
                        .evidence_floor
                        .get("min_sources")
                        .and_then(Value::as_u64)
                        .map(|min| min as u32)
                })
            })
            .unwrap_or(0);
        if required == 0 {
            return Vec::new();
        }
        if actual >= required {
            return Vec::new();
        }
        vec![LintIssue {
            lint_id: self.id(),
            severity: self.severity(),
            instance_ids: Vec::new(),
            reason: format!(
                "Das Evidence-Register hält {actual} Quellen bereit; das Tiefenprofil verlangt mindestens {required}.",
            ),
            goal: format!(
                "Erweitere das Evidence-Register mit public_research auf mindestens {required} valide Quellen, bevor das Paket freigegeben wird.",
            ),
        }]
    }
}

struct LintEvidenceConcentration;

impl Lint for LintEvidenceConcentration {
    fn id(&self) -> &'static str {
        "LINT-EVIDENCE-CONCENTRATION"
    }
    fn applies(&self, _ctx: &LintContext) -> bool {
        true
    }
    fn severity(&self) -> LintSeverity {
        // Severity is computed dynamically: > 80% = Critical, 60–80%
        // = Soft. We default to Soft and let `check` override per
        // issue.
        LintSeverity::Soft
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        let mut total: usize = 0;
        for block in ctx.committed_blocks {
            for ref_id in &block.used_reference_ids {
                *counts.entry(ref_id.clone()).or_insert(0) += 1;
                total += 1;
            }
        }
        if total == 0 {
            return Vec::new();
        }
        let mut out: Vec<LintIssue> = Vec::new();
        for (ref_id, count) in &counts {
            let share = (*count as f64) / (total as f64);
            if share <= 0.60 {
                continue;
            }
            let percent = (share * 100.0).round() as i64;
            // Find blocks where the dominant ref_id appears.
            let mut instance_ids: Vec<String> = Vec::new();
            for block in ctx.committed_blocks {
                if block.used_reference_ids.iter().any(|r| r == ref_id) {
                    instance_ids.push(block.instance_id.clone());
                }
            }
            let instance_ids = dedupe_keep_order(instance_ids);
            let short_title = ctx
                .evidence_register
                .iter()
                .find(|e| &e.evidence_id == ref_id)
                .and_then(|e| e.title.clone())
                .unwrap_or_else(|| ref_id.clone());
            let severity = if share > 0.80 {
                LintSeverity::Critical
            } else {
                LintSeverity::Soft
            };
            out.push(LintIssue {
                lint_id: self.id(),
                severity,
                instance_ids,
                reason: format!(
                    "Die Belegkette stützt sich zu {percent}% auf {ref_id} ({short_title}); das wirkt wie eine Monoquelle.",
                ),
                goal: "Streue die Belege in den Detailkapiteln über mehrere Quellen aus dem Register; ergänze ggf. weitere via public_research.".to_string(),
            });
        }
        out
    }
}

// ============ Anti-slop language (6) ============

struct LintDeadPhrase;

impl Lint for LintDeadPhrase {
    fn id(&self) -> &'static str {
        "LINT-DEAD-PHRASE"
    }
    fn applies(&self, _ctx: &LintContext) -> bool {
        true
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let mut out: Vec<LintIssue> = Vec::new();
        let phrases: Vec<String> = ctx
            .style_guidance
            .dead_phrases_to_avoid
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        if phrases.is_empty() {
            return out;
        }
        for block in ctx.committed_blocks {
            let lower = block.markdown.to_lowercase();
            let mut hits: Vec<String> = Vec::new();
            for phrase in &phrases {
                if lower.contains(phrase) {
                    hits.push(phrase.clone());
                    if hits.len() >= 3 {
                        break;
                    }
                }
            }
            for phrase in hits {
                out.push(LintIssue {
                    lint_id: self.id(),
                    severity: self.severity(),
                    instance_ids: vec![block.instance_id.clone()],
                    reason: format!(
                        "Block {title} enthält tote Wendung „{phrase}“.",
                        title = block.title,
                        phrase = phrase,
                    ),
                    goal: format!(
                        "Ersetze in {title} „{phrase}“ durch eine konkrete Aussage über Mechanik, Wirkung oder Beleg.",
                        title = block.title,
                        phrase = phrase,
                    ),
                });
            }
        }
        out
    }
}

struct LintMetaPhrase;

impl Lint for LintMetaPhrase {
    fn id(&self) -> &'static str {
        "LINT-META-PHRASE"
    }
    fn applies(&self, _ctx: &LintContext) -> bool {
        true
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let phrases: Vec<String> = ctx
            .style_guidance
            .forbidden_meta_phrases
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        let mut out: Vec<LintIssue> = Vec::new();
        if phrases.is_empty() {
            return out;
        }
        for block in ctx.committed_blocks {
            let lower = block.markdown.to_lowercase();
            for phrase in &phrases {
                if lower.contains(phrase) {
                    out.push(LintIssue {
                        lint_id: self.id(),
                        severity: self.severity(),
                        instance_ids: vec![block.instance_id.clone()],
                        reason: format!(
                            "Verbotene Meta-Formel in {title}: „{phrase}“.",
                            title = block.title,
                            phrase = phrase,
                        ),
                        goal: format!(
                            "Formuliere {title} aus interner Feststellungsperspektive neu und entferne Gutachter- oder Aktenformeln.",
                            title = block.title,
                        ),
                    });
                    break;
                }
            }
        }
        out
    }
}

struct LintConsultantOveruse;

impl Lint for LintConsultantOveruse {
    fn id(&self) -> &'static str {
        "LINT-CONSULTANT-OVERUSE"
    }
    fn applies(&self, _ctx: &LintContext) -> bool {
        true
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Soft
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let phrases: Vec<String> = ctx
            .style_guidance
            .consultant_phrases_to_soften
            .iter()
            .cloned()
            .collect();
        let mut out: Vec<LintIssue> = Vec::new();
        if phrases.is_empty() {
            return out;
        }
        for block in ctx.committed_blocks {
            let lower = block.markdown.to_lowercase();
            for phrase in &phrases {
                let needle = phrase.to_lowercase();
                if needle.is_empty() {
                    continue;
                }
                let count = lower.matches(needle.as_str()).count();
                if count > 2 {
                    out.push(LintIssue {
                        lint_id: self.id(),
                        severity: self.severity(),
                        instance_ids: vec![block.instance_id.clone()],
                        reason: format!(
                            "Beraterhaft glatte Formulierungen in {title}: {phrase} (×{count}).",
                            title = block.title,
                            phrase = phrase,
                            count = count,
                        ),
                        goal: format!(
                            "Ersetze in {title} glatte Beraterwörter durch konkretere fachliche Sprache.",
                            title = block.title,
                        ),
                    });
                }
            }
        }
        out
    }
}

struct LintUnanchoredHedge;

impl Lint for LintUnanchoredHedge {
    fn id(&self) -> &'static str {
        "LINT-UNANCHORED-HEDGE"
    }
    fn applies(&self, _ctx: &LintContext) -> bool {
        true
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let hedges = [
            "möglicherweise",
            "moeglicherweise",
            "in bestimmten fällen",
            "in bestimmten faellen",
            "tendenziell",
            "vielleicht",
            "could potentially",
            "in some cases",
            "may be able to",
        ];
        let anchors = [
            "unter der randbedingung",
            "unter der annahme",
            "bei",
            "vorausgesetzt",
            "wenn",
            "sofern",
            "quelle",
            "[",
            "abbildung",
            "tabelle",
            "szenario",
            "provided that",
            "under the assumption",
            "assumption",
        ];
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let mut emitted = false;
            for sentence in split_sentences(&block.markdown) {
                let lower = sentence.to_lowercase();
                let has_hedge = hedges.iter().any(|h| lower.contains(h));
                if !has_hedge {
                    continue;
                }
                let has_anchor = anchors.iter().any(|a| lower.contains(a));
                if has_anchor {
                    continue;
                }
                let snippet = snippet_60(&sentence);
                out.push(LintIssue {
                    lint_id: self.id(),
                    severity: self.severity(),
                    instance_ids: vec![block.instance_id.clone()],
                    reason: format!(
                        "Hedging in {title} ohne erkennbaren Anker: „{snippet}…“.",
                        title = block.title,
                        snippet = snippet,
                    ),
                    goal: format!(
                        "Binde {title} den Hedge an Annahme, Szenario oder Quelle, oder streiche ihn.",
                        title = block.title,
                    ),
                });
                emitted = true;
                break;
            }
            let _ = emitted;
        }
        out
    }
}

struct LintFillerOpening;

impl Lint for LintFillerOpening {
    fn id(&self) -> &'static str {
        "LINT-FILLER-OPENING"
    }
    fn applies(&self, _ctx: &LintContext) -> bool {
        true
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let openers = [
            "im folgenden",
            "im rahmen dieser",
            "vor diesem hintergrund",
            "es ist anzumerken",
            "die folgenden abschnitte",
        ];
        let strip_markers = |line: &str| -> String {
            let mut s = line.trim_start().to_string();
            // Strip leading list markers and heading hashes.
            for prefix in ["#", "- ", "* ", "> ", "1. ", "2. ", "3. ", "4. ", "5. "] {
                while s.starts_with(prefix) {
                    s = s[prefix.len()..].trim_start().to_string();
                }
            }
            s
        };
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            for paragraph in split_paragraphs(&block.markdown) {
                // Take the first non-empty line.
                let line = paragraph
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("");
                let stripped = strip_markers(line);
                let lower = stripped.to_lowercase();
                for opener in &openers {
                    if lower.starts_with(opener) {
                        out.push(LintIssue {
                            lint_id: self.id(),
                            severity: self.severity(),
                            instance_ids: vec![block.instance_id.clone()],
                            reason: format!(
                                "Absatz in {title} beginnt mit Füllformel „{opener}“.",
                                title = block.title,
                                opener = opener,
                            ),
                            goal: format!(
                                "Eröffne den Absatz in {title} mit einem konkreten Anker (Verfahren, Schichtaufbau, Defekt) statt mit „{opener}“.",
                                title = block.title,
                                opener = opener,
                            ),
                        });
                        break;
                    }
                }
            }
        }
        out
    }
}

struct LintInvertedPerspective;

impl Lint for LintInvertedPerspective {
    fn id(&self) -> &'static str {
        "LINT-INVERTED-PERSPECTIVE"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        if ctx.report_type_id == "whitepaper" {
            // Conditional: only when management_summary or recommendation
            // is present (i.e. the dossier has a third-person register).
            return ctx
                .report_type
                .block_library_keys
                .iter()
                .any(|k| k == "management_summary" || k == "recommendation");
        }
        true
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let target_templates: HashSet<&str> = [
            "management_summary",
            "detail_assessment",
            "risk_register",
            "scope_disclaimer",
            "recommendation",
        ]
        .into_iter()
        .collect();
        let pronoun = Regex::new(
            r"\b(?:Wir|wir|Unser|unser|unsere|unserem|unseren|unserer|unseres|we|our|We|Our)\b",
        )
        .expect("pronoun regex compiles");

        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let template_id = block
                .block_template_id
                .as_deref()
                .unwrap_or(&block.block_id);
            if !target_templates.contains(template_id) {
                continue;
            }
            // Skip fenced-code lines (cheap heuristic).
            let mut found = false;
            for line in block.markdown.lines() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("```") || trimmed.starts_with(">") {
                    continue;
                }
                if pronoun.is_match(line) {
                    found = true;
                    break;
                }
            }
            if found {
                out.push(LintIssue {
                    lint_id: self.id(),
                    severity: self.severity(),
                    instance_ids: vec![block.instance_id.clone()],
                    reason: format!(
                        "Block {title} schreibt in Wir-Form, obwohl der Block in dritter Person geführt wird.",
                        title = block.title,
                    ),
                    goal: format!(
                        "Stelle {title} auf dritte Person bzw. Sachregister um; halte die Perspektive im gesamten Paket einheitlich.",
                        title = block.title,
                    ),
                });
            }
        }
        out
    }
}

// ============ Matrix integrity (4) ============

struct LintDuplicateRationale;

impl Lint for LintDuplicateRationale {
    fn id(&self) -> &'static str {
        "LINT-DUPLICATE-RATIONALE"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        report_type_has_matrix(ctx.report_type)
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Critical
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        // Heuristic: a matrix block lives in a markdown table with
        // ≥ 3 columns and ≥ 2 rows. Each row's non-header cells are
        // candidate rationales. We compare every pair of cells in
        // the same row.
        let matrix_templates = [
            "screening_matrix",
            "scenario_matrix",
            "screening_matrix_short",
            "competitor_matrix",
        ];
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let template_id = block
                .block_template_id
                .as_deref()
                .unwrap_or(&block.block_id);
            if !matrix_templates.contains(&template_id) {
                continue;
            }
            // Parse pipe table.
            let rows: Vec<Vec<String>> = block
                .markdown
                .lines()
                .filter_map(|line| {
                    let trimmed = line.trim();
                    if !trimmed.starts_with('|') {
                        return None;
                    }
                    let cells: Vec<String> = trimmed
                        .trim_matches('|')
                        .split('|')
                        .map(|c| c.trim().to_string())
                        .collect();
                    Some(cells)
                })
                .collect();
            if rows.len() < 3 {
                continue;
            }
            // Skip header (row 0) and separator (row 1, contains
            // dashes).
            let mut headers: Vec<String> = Vec::new();
            if let Some(h) = rows.first() {
                headers = h.clone();
            }
            for row in rows.iter().skip(2) {
                if row.len() < 3 {
                    continue;
                }
                let option = row.first().cloned().unwrap_or_default();
                // Compare each pair of axes.
                for i in 1..row.len() {
                    for j in (i + 1)..row.len() {
                        let a = &row[i];
                        let b = &row[j];
                        let a_words = tokenise_words(a);
                        let b_words = tokenise_words(b);
                        if a_words.len() < 3 || b_words.len() < 3 {
                            continue;
                        }
                        let sa = shingles_3(&a_words);
                        let sb = shingles_3(&b_words);
                        let sim = jaccard_similarity(&sa, &sb);
                        if sim >= 0.7 {
                            let axis_a = headers.get(i).cloned().unwrap_or_default();
                            let axis_b = headers.get(j).cloned().unwrap_or_default();
                            out.push(LintIssue {
                                lint_id: self.id(),
                                severity: self.severity(),
                                instance_ids: vec![block.instance_id.clone()],
                                reason: format!(
                                    "Bewertungsmatrix für {option}: Begründungen in den Achsen {axis_a} und {axis_b} sind nahezu identisch.",
                                ),
                                goal: format!(
                                    "Schreibe die Achsen-Begründungen für {option} so, dass jede Achse ihre eigene fachliche Logik trägt (z.B. Single-Shot vs. Defektsensitivität vs. Reifegrad).",
                                ),
                            });
                        }
                    }
                }
            }
        }
        out
    }
}

struct LintVerdictMismatch;

impl Lint for LintVerdictMismatch {
    fn id(&self) -> &'static str {
        "LINT-VERDICT-MISMATCH"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        ctx.report_type.verdict_line_pattern.is_some()
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Critical
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let verdict_re = Regex::new(
            r"(?i)Erfolgsaussichten\s*\(qualitativ\)\s*:?\s*(sehr\s+hoch|hoch|mittel(?:[\s\-–—]*hoch)?|niedrig(?:[\s\-–—]*mittel)?|niedrig)",
        )
        .expect("verdict regex compiles");
        let mut out: Vec<LintIssue> = Vec::new();
        // Without a structured matrix we cannot resolve `matrix_value`
        // for the same `(option, axis)`. We fire only when the verdict
        // line is malformed or absent in a `detail_assessment` block —
        // which is the irreducible structural mismatch the lint can
        // detect deterministically without the matrix asset.
        for block in ctx.committed_blocks {
            let template_id = block
                .block_template_id
                .as_deref()
                .unwrap_or(&block.block_id);
            if template_id != "detail_assessment" {
                continue;
            }
            // Look at the last 200 chars of the block — verdicts live
            // at the end.
            let chars: Vec<char> = block.markdown.chars().collect();
            let tail_start = chars.len().saturating_sub(400);
            let tail: String = chars[tail_start..].iter().collect();
            let cap = verdict_re.captures(&tail);
            if cap.is_none() {
                out.push(LintIssue {
                    lint_id: self.id(),
                    severity: self.severity(),
                    instance_ids: vec![block.instance_id.clone()],
                    reason: format!(
                        "Verdict in {title} („—“) passt nicht zur Matrixzelle —/— („—“).",
                        title = block.title,
                    ),
                    goal: format!(
                        "Synchronisiere Detail-Verdict und Matrixzelle für {title} und passe ggf. die Matrix an, falls die Detailbegründung tragfähiger ist.",
                        title = block.title,
                    ),
                });
            }
        }
        out
    }
}

struct LintAxisCompleteness;

impl Lint for LintAxisCompleteness {
    fn id(&self) -> &'static str {
        "LINT-AXIS-COMPLETENESS"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        report_type_has_matrix(ctx.report_type)
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let matrix_templates = [
            "screening_matrix",
            "scenario_matrix",
            "screening_matrix_short",
            "competitor_matrix",
        ];
        let empties = ["", "-", "–", "—", "tbd", "TBD", "n/a", "N/A"];
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let template_id = block
                .block_template_id
                .as_deref()
                .unwrap_or(&block.block_id);
            if !matrix_templates.contains(&template_id) {
                continue;
            }
            let rows: Vec<Vec<String>> = block
                .markdown
                .lines()
                .filter_map(|line| {
                    let trimmed = line.trim();
                    if !trimmed.starts_with('|') {
                        return None;
                    }
                    let cells: Vec<String> = trimmed
                        .trim_matches('|')
                        .split('|')
                        .map(|c| c.trim().to_string())
                        .collect();
                    Some(cells)
                })
                .collect();
            if rows.len() < 3 {
                continue;
            }
            let headers = rows.first().cloned().unwrap_or_default();
            for row in rows.iter().skip(2) {
                if row.is_empty() {
                    continue;
                }
                let option = row.first().cloned().unwrap_or_default();
                for (i, cell) in row.iter().enumerate().skip(1) {
                    let normalised = cell.trim();
                    if empties.contains(&normalised) {
                        let axis = headers.get(i).cloned().unwrap_or_default();
                        out.push(LintIssue {
                            lint_id: self.id(),
                            severity: self.severity(),
                            instance_ids: vec![block.instance_id.clone()],
                            reason: format!(
                                "Bewertungsmatrix: Zelle {option}/{axis} ist nicht ausgefüllt.",
                            ),
                            goal: format!(
                                "Trage in {option}/{axis} eine qualitative Bewertung im erlaubten Rubrik-Vokabular ein oder begründe ihren Ausschluss explizit.",
                            ),
                        });
                    }
                }
            }
        }
        out
    }
}

struct LintRubricMismatch;

impl Lint for LintRubricMismatch {
    fn id(&self) -> &'static str {
        "LINT-RUBRIC-MISMATCH"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        report_type_has_matrix(ctx.report_type)
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        // The asset pack does not yet expose per-axis rubric vocabulary
        // as a structured field. We fall back to the canonical
        // RASCON-style rubric.
        let rubric: HashSet<String> = [
            "niedrig",
            "mittel",
            "hoch",
            "sehr hoch",
            "niedrig–mittel",
            "niedrig-mittel",
            "mittel–hoch",
            "mittel-hoch",
        ]
        .iter()
        .map(|s| s.to_lowercase())
        .collect();
        let allowed = "niedrig | mittel | hoch | sehr hoch | niedrig–mittel | mittel–hoch";
        let matrix_templates = [
            "screening_matrix",
            "scenario_matrix",
            "screening_matrix_short",
            "competitor_matrix",
        ];
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let template_id = block
                .block_template_id
                .as_deref()
                .unwrap_or(&block.block_id);
            if !matrix_templates.contains(&template_id) {
                continue;
            }
            let rows: Vec<Vec<String>> = block
                .markdown
                .lines()
                .filter_map(|line| {
                    let trimmed = line.trim();
                    if !trimmed.starts_with('|') {
                        return None;
                    }
                    let cells: Vec<String> = trimmed
                        .trim_matches('|')
                        .split('|')
                        .map(|c| c.trim().to_string())
                        .collect();
                    Some(cells)
                })
                .collect();
            if rows.len() < 3 {
                continue;
            }
            let headers = rows.first().cloned().unwrap_or_default();
            for row in rows.iter().skip(2) {
                if row.is_empty() {
                    continue;
                }
                let option = row.first().cloned().unwrap_or_default();
                for (i, cell) in row.iter().enumerate().skip(1) {
                    let normalised = cell.trim().to_lowercase();
                    if normalised.is_empty() {
                        continue;
                    }
                    if rubric.contains(&normalised) {
                        continue;
                    }
                    // Accept compound-cell rationales by treating the
                    // first ' — ' or ' - ' separator as the value.
                    let lead = normalised
                        .split([' ', '—', '-'])
                        .next()
                        .unwrap_or(&normalised)
                        .to_string();
                    if rubric.contains(&lead) {
                        continue;
                    }
                    let axis = headers.get(i).cloned().unwrap_or_default();
                    out.push(LintIssue {
                        lint_id: self.id(),
                        severity: self.severity(),
                        instance_ids: vec![block.instance_id.clone()],
                        reason: format!(
                            "Bewertungsmatrix: {option}/{axis} = „{value}“ entspricht keiner Stufe der Rubrik ({allowed}).",
                            option = option,
                            axis = axis,
                            value = cell,
                            allowed = allowed,
                        ),
                        goal: format!(
                            "Wähle in {option}/{axis} eine zulässige Rubrikstufe ({allowed}) und passe ggf. die Begründung an.",
                            option = option,
                            axis = axis,
                            allowed = allowed,
                        ),
                    });
                }
            }
        }
        out
    }
}

// ============ Structural integrity (4) ============

struct LintMinChars;

impl Lint for LintMinChars {
    fn id(&self) -> &'static str {
        "LINT-MIN-CHARS"
    }
    fn applies(&self, _ctx: &LintContext) -> bool {
        true
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let mut out: Vec<LintIssue> = Vec::new();
        // Build a map of block_id -> required + min_chars from the
        // blueprint sequence.
        let mut required_min: HashMap<String, (bool, u32)> = HashMap::new();
        for entry in &ctx.document_blueprint.sequence {
            let library = match ctx.asset_pack.block_library_entry(&entry.block_id) {
                Ok(e) => e,
                Err(_) => continue,
            };
            required_min.insert(
                format!("{}__{}", entry.doc_id, entry.block_id),
                (entry.required, library.min_chars),
            );
        }
        for block in ctx.committed_blocks {
            let key = block.instance_id.clone();
            let (required, min_chars) = match required_min.get(&key) {
                Some(t) => *t,
                None => continue,
            };
            if !required || min_chars == 0 {
                continue;
            }
            let actual = block.markdown.trim().chars().count() as u32;
            let floor = ((min_chars as f64) * 0.65).ceil() as u32;
            if actual >= floor {
                continue;
            }
            out.push(LintIssue {
                lint_id: self.id(),
                severity: self.severity(),
                instance_ids: vec![block.instance_id.clone()],
                reason: format!(
                    "Block {title} ist mit {actual} Zeichen deutlich kürzer als das Sollmaß ({min_chars}).",
                    title = block.title,
                    actual = actual,
                    min_chars = min_chars,
                ),
                goal: format!(
                    "Verdichte {title} auf mindestens das Sollmaß; orientiere dich am Detailgrad eines vergleichbaren RASCON-Kapitels.",
                    title = block.title,
                ),
            });
        }
        out
    }
}

struct LintMaxChars;

impl Lint for LintMaxChars {
    fn id(&self) -> &'static str {
        "LINT-MAX-CHARS"
    }
    fn applies(&self, _ctx: &LintContext) -> bool {
        true
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Soft
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let library = match ctx.asset_pack.block_library_entry(&block.block_id) {
                Ok(e) => e,
                Err(_) => continue,
            };
            if library.min_chars == 0 {
                continue;
            }
            let actual = block.markdown.trim().chars().count() as u32;
            let ceiling = library.min_chars.saturating_mul(2);
            if actual <= ceiling {
                continue;
            }
            out.push(LintIssue {
                lint_id: self.id(),
                severity: self.severity(),
                instance_ids: vec![block.instance_id.clone()],
                reason: format!(
                    "Block {title} ist mit {actual} Zeichen über doppelt so lang wie das Sollmaß ({min_chars}).",
                    title = block.title,
                    actual = actual,
                    min_chars = library.min_chars,
                ),
                goal: format!(
                    "Kürze {title} auf das Sollmaß; verschiebe Zusatzdetails in die zugehörigen Detailkapitel oder einen Anhang.",
                    title = block.title,
                ),
            });
        }
        out
    }
}

struct LintMissingDisclaimer;

impl Lint for LintMissingDisclaimer {
    fn id(&self) -> &'static str {
        "LINT-MISSING-DISCLAIMER"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        matches!(
            ctx.report_type_id,
            "feasibility_study" | "technology_screening" | "market_research"
        )
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let assumption = [
            "annahme",
            "plausibilitätsannahme",
            "annahmen",
            "assumption",
            "assumptions",
        ];
        let validation = [
            "validierung",
            "validiert",
            "repräsentative proben",
            "validation",
            "validated",
        ];
        let limitation = ["grenze", "einschränkung", "nicht", "limit", "limitation"];
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let template_id = block
                .block_template_id
                .as_deref()
                .unwrap_or(&block.block_id);
            if template_id != "scope_disclaimer" {
                continue;
            }
            let lower = block.markdown.to_lowercase();
            let mut missing: Vec<&'static str> = Vec::new();
            if !assumption.iter().any(|a| lower.contains(a)) {
                missing.push("Annahme");
            }
            if !validation.iter().any(|a| lower.contains(a)) {
                missing.push("Validierung");
            }
            if !limitation.iter().any(|a| lower.contains(a)) {
                missing.push("Grenze");
            }
            for cluster in missing {
                out.push(LintIssue {
                    lint_id: self.id(),
                    severity: self.severity(),
                    instance_ids: vec![block.instance_id.clone()],
                    reason: format!(
                        "Scope-Disclaimer fehlt eine erforderliche Klausel ({cluster}).",
                    ),
                    goal: format!(
                        "Ergänze im Scope-Disclaimer eine Aussage zu {cluster}; orientiere dich an dem Hinweisblock einer RASCON-Studie.",
                    ),
                });
            }
        }
        out
    }
}

struct LintDuplicateSectionOpening;

impl Lint for LintDuplicateSectionOpening {
    fn id(&self) -> &'static str {
        "LINT-DUPLICATE-SECTION-OPENING"
    }
    fn applies(&self, _ctx: &LintContext) -> bool {
        true
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Soft
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let stop_words: HashSet<&'static str> = [
            "der", "die", "das", "und", "oder", "ist", "sind", "mit", "für", "the", "is", "are",
            "with", "for", "of", "to", "ein", "eine", "einen", "einer", "an", "in",
        ]
        .into_iter()
        .collect();
        let first_sentence_words = |block: &BlockRecord| -> HashSet<String> {
            let sentences = split_sentences(&block.markdown);
            let first = sentences.first().cloned().unwrap_or_default();
            tokenise_words(&first)
                .into_iter()
                .filter(|w| !stop_words.contains(w.as_str()))
                .collect()
        };
        let mut out: Vec<LintIssue> = Vec::new();
        let blocks: Vec<&BlockRecord> = ctx.committed_blocks.iter().collect();
        for i in 0..blocks.len() {
            for j in (i + 1)..blocks.len() {
                let a = first_sentence_words(blocks[i]);
                let b = first_sentence_words(blocks[j]);
                if a.is_empty() || b.is_empty() {
                    continue;
                }
                let inter = a.intersection(&b).count() as f64;
                let smaller = a.len().min(b.len()) as f64;
                if smaller == 0.0 {
                    continue;
                }
                let overlap = inter / smaller;
                if overlap >= 0.7 {
                    out.push(LintIssue {
                        lint_id: self.id(),
                        severity: self.severity(),
                        instance_ids: vec![
                            blocks[i].instance_id.clone(),
                            blocks[j].instance_id.clone(),
                        ],
                        reason: format!(
                            "Die Blöcke {title_a} und {title_b} steigen mit fast identischer Einleitung ein.",
                            title_a = blocks[i].title,
                            title_b = blocks[j].title,
                        ),
                        goal: format!(
                            "Bau in {title_b} eine Brücke zum vorherigen Gedanken statt das Vorhaben erneut bei null vorzustellen.",
                            title_b = blocks[j].title,
                        ),
                    });
                }
            }
        }
        out
    }
}

// ============ Market-research (4) ============

struct LintMrUnquantifiedMarket;

impl Lint for LintMrUnquantifiedMarket {
    fn id(&self) -> &'static str {
        "LINT-MR-UNQUANTIFIED-MARKET"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        match ctx.report_type_id {
            "market_research" => true,
            "competitive_analysis" | "decision_brief" => ctx
                .report_type
                .block_library_keys
                .iter()
                .any(|k| k == "market_overview" || k == "demand_drivers"),
            _ => false,
        }
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Soft
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let target = [
            "market_overview",
            "market_sizing",
            "demand_drivers",
            "segments",
        ];
        let growth = Regex::new(
            r"(?i)\b(wächst|hochdynamisch|deutlicher\s+wachstumstrend|rapidly\s+growing|double[-\s]digit\s+growth|expanding\s+fast|stark\s+wachsend)\b",
        )
        .expect("growth regex compiles");
        let number_unit = Regex::new(r"\d+(?:[.,]\d+)?\s*(%|Mio\.?|Mrd\.?|bn|m\b|million|billion)")
            .expect("number-unit regex compiles");
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let template_id = block
                .block_template_id
                .as_deref()
                .unwrap_or(&block.block_id);
            if !target.contains(&template_id) {
                continue;
            }
            for cap in growth.find_iter(&block.markdown) {
                let phrase = cap.as_str().to_string();
                let start = cap.start();
                let end = cap.end();
                let pre = block.markdown[..start]
                    .chars()
                    .rev()
                    .take(200)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<String>();
                let post: String = block.markdown[end..].chars().take(200).collect();
                let window = format!("{pre}{post}");
                let has_number = number_unit.is_match(&window);
                let has_refs = !block.used_reference_ids.is_empty();
                if has_number && has_refs {
                    continue;
                }
                out.push(LintIssue {
                    lint_id: self.id(),
                    severity: self.severity(),
                    instance_ids: vec![block.instance_id.clone()],
                    reason: format!(
                        "Wachstumsbehauptung in {title} ohne belegte Zahl oder Quelle (gefundene Phrase: \"{phrase}\").",
                        title = block.title,
                        phrase = phrase,
                    ),
                    goal: format!(
                        "Belege die Wachstumsaussage in {title} mit einer datierten Marktzahl und einer registrierten Quelle, oder streiche sie.",
                        title = block.title,
                    ),
                });
                break;
            }
        }
        out
    }
}

struct LintMrMethodMissing;

impl Lint for LintMrMethodMissing {
    fn id(&self) -> &'static str {
        "LINT-MR-METHOD-MISSING"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        match ctx.report_type_id {
            "market_research" => true,
            "competitive_analysis" | "decision_brief" => ctx
                .report_type
                .block_library_keys
                .iter()
                .any(|k| k == "market_sizing"),
            _ => false,
        }
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Soft
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let tam_re = Regex::new(r"\b(TAM|SAM|SOM)\b").expect("tam regex compiles");
        let method_terms = [
            "top-down",
            "bottom-up",
            "methode",
            "method",
            "annahmen",
            "assumption",
        ];
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            for cap in tam_re.find_iter(&block.markdown) {
                let start = cap.start();
                let end = cap.end();
                let pre = block.markdown[..start]
                    .chars()
                    .rev()
                    .take(200)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<String>();
                let post: String = block.markdown[end..].chars().take(200).collect();
                let window = format!("{pre}{post}").to_lowercase();
                let has_method = method_terms.iter().any(|t| window.contains(t));
                if has_method {
                    continue;
                }
                out.push(LintIssue {
                    lint_id: self.id(),
                    severity: self.severity(),
                    instance_ids: vec![block.instance_id.clone()],
                    reason: format!(
                        "{title} nennt TAM/SAM/SOM ohne Hinweis auf die Berechnungsmethode.",
                        title = block.title,
                    ),
                    goal: format!(
                        "Ergänze in {title} eine kurze Methodenangabe (top-down vs bottom-up, Bezugsjahr, geographischer Geltungsbereich) für die TAM/SAM/SOM-Werte.",
                        title = block.title,
                    ),
                });
                break;
            }
        }
        out
    }
}

struct LintMrCompetitorNameless;

impl Lint for LintMrCompetitorNameless {
    fn id(&self) -> &'static str {
        "LINT-MR-COMPETITOR-NAMELESS"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        match ctx.report_type_id {
            "market_research" | "competitive_analysis" => true,
            "decision_brief" => ctx
                .report_type
                .block_library_keys
                .iter()
                .any(|k| k == "options_summary"),
            _ => false,
        }
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let target = [
            "competitor_landscape",
            "competitor_set",
            "channel_overlap",
            "gap_to_close",
        ];
        let competitor_re = Regex::new(
            r"(?i)\b(wettbewerber|competitors?|players?|anbieter|marktteilnehmer|various\s+(?:players|firms))\b",
        )
        .expect("competitor regex compiles");
        let stop_caps: HashSet<&'static str> = [
            "Wettbewerber",
            "Markt",
            "Industry",
            "Sektor",
            "Unternehmen",
            "Anbieter",
            "Players",
            "Player",
        ]
        .into_iter()
        .collect();
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let template_id = block
                .block_template_id
                .as_deref()
                .unwrap_or(&block.block_id);
            if !target.contains(&template_id) {
                continue;
            }
            if !competitor_re.is_match(&block.markdown) {
                continue;
            }
            let candidates = extract_capitalised_ngrams(&block.markdown, 4);
            let unique: HashSet<String> = candidates
                .into_iter()
                .filter(|c| !stop_caps.contains(c.as_str()))
                .collect();
            if unique.len() >= 3 {
                continue;
            }
            out.push(LintIssue {
                lint_id: self.id(),
                severity: self.severity(),
                instance_ids: vec![block.instance_id.clone()],
                reason: format!(
                    "{title} spricht über Wettbewerber, ohne mindestens drei konkret zu benennen.",
                    title = block.title,
                ),
                goal: format!(
                    "Nenne in {title} mindestens drei Wettbewerber namentlich (z. B. Marktführer, Herausforderer, Nischenanbieter) und verlinke sie zu Evidenz-Einträgen.",
                    title = block.title,
                ),
            });
        }
        out
    }
}

struct LintMrSegmentWithoutSize;

impl Lint for LintMrSegmentWithoutSize {
    fn id(&self) -> &'static str {
        "LINT-MR-SEGMENT-WITHOUT-SIZE"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        match ctx.report_type_id {
            "market_research" => true,
            "competitive_analysis" => ctx
                .report_type
                .block_library_keys
                .iter()
                .any(|k| k == "segments"),
            _ => false,
        }
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Soft
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let bullet_re = Regex::new(r"(?m)^\s*[-*]\s+").expect("bullet regex compiles");
        let size_re = Regex::new(r"\d+(?:[.,]\d+)?\s*(%|Mio\.?|Mrd\.?|bn|M\s*units|EUR|USD)")
            .expect("size regex compiles");
        let method_terms = ["methode", "method", "annahmen", "assumption"];
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let template_id = block
                .block_template_id
                .as_deref()
                .unwrap_or(&block.block_id);
            if template_id != "segments" {
                continue;
            }
            let bullets = bullet_re.find_iter(&block.markdown).count();
            if bullets < 3 {
                continue;
            }
            let has_size = size_re.is_match(&block.markdown);
            let lower = block.markdown.to_lowercase();
            let has_method = method_terms.iter().any(|t| lower.contains(t));
            if has_size || has_method {
                continue;
            }
            out.push(LintIssue {
                lint_id: self.id(),
                severity: self.severity(),
                instance_ids: vec![block.instance_id.clone()],
                reason: format!(
                    "Segmentliste in {title} ohne adressierbare Grösse oder Methodenangabe.",
                    title = block.title,
                ),
                goal: format!(
                    "Ergänze in {title} pro Segment entweder eine Grösseneinschätzung mit Bezugsjahr oder eine Sammelmethodenangabe.",
                    title = block.title,
                ),
            });
        }
        out
    }
}

// ============ Whitepaper (3) ============

struct LintWpThesisDrift;

impl Lint for LintWpThesisDrift {
    fn id(&self) -> &'static str {
        "LINT-WP-THESIS-DRIFT"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        ctx.report_type_id == "whitepaper"
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let mut thesis_block: Option<&BlockRecord> = None;
        let mut counter_block: Option<&BlockRecord> = None;
        for block in ctx.committed_blocks {
            let tid = block
                .block_template_id
                .as_deref()
                .unwrap_or(&block.block_id);
            if tid == "thesis" {
                thesis_block = Some(block);
            }
            if tid == "counter_arguments" {
                counter_block = Some(block);
            }
        }
        let mut out: Vec<LintIssue> = Vec::new();
        if let Some(thesis) = thesis_block {
            let sentences = split_sentences(&thesis.markdown);
            // Case 1: more than one declarative position with disjoint
            // head-noun sets.
            if sentences.len() >= 2 {
                let head_sets: Vec<HashSet<String>> = sentences
                    .iter()
                    .map(|s| {
                        extract_capitalised_ngrams(s, 1)
                            .into_iter()
                            .map(|w| w.to_lowercase())
                            .collect::<HashSet<String>>()
                    })
                    .collect();
                let mut disjoint = false;
                for i in 0..head_sets.len() {
                    for j in (i + 1)..head_sets.len() {
                        if head_sets[i].is_disjoint(&head_sets[j])
                            && !head_sets[i].is_empty()
                            && !head_sets[j].is_empty()
                        {
                            disjoint = true;
                        }
                    }
                }
                if disjoint {
                    out.push(LintIssue {
                        lint_id: self.id(),
                        severity: self.severity(),
                        instance_ids: vec![thesis.instance_id.clone()],
                        reason: format!(
                            "Thesisblock {title} enthält mehrere konkurrierende Positionen ohne erkennbaren Hauptanker.",
                            title = thesis.title,
                        ),
                        goal: format!(
                            "Verdichte {title} auf eine einzige Hauptposition; verlagere konkurrierende Aussagen in argument_section.",
                            title = thesis.title,
                        ),
                    });
                }
            }
            // Case 2: counter-arguments overlap with thesis < 30%.
            if let Some(counter) = counter_block {
                let thesis_phrases: HashSet<String> =
                    extract_capitalised_ngrams(&thesis.markdown, 2)
                        .into_iter()
                        .map(|w| w.to_lowercase())
                        .collect();
                let counter_phrases: HashSet<String> =
                    extract_capitalised_ngrams(&counter.markdown, 2)
                        .into_iter()
                        .map(|w| w.to_lowercase())
                        .collect();
                if !thesis_phrases.is_empty() {
                    let inter = thesis_phrases.intersection(&counter_phrases).count() as f64;
                    let share = inter / thesis_phrases.len() as f64;
                    if share < 0.30 {
                        out.push(LintIssue {
                            lint_id: self.id(),
                            severity: self.severity(),
                            instance_ids: vec![counter.instance_id.clone()],
                            reason: format!(
                                "Gegenargumente {title} adressieren die These offenbar nicht — Begriffsüberschneidung unter 30 %.",
                                title = counter.title,
                            ),
                            goal: format!(
                                "Schreibe {title} so um, dass mindestens drei Schlüsselbegriffe aus dem Thesisblock direkt aufgegriffen und entkräftet bzw. abgegrenzt werden.",
                                title = counter.title,
                            ),
                        });
                    }
                }
            }
        }
        out
    }
}

struct LintWpEvidenceMissingForClaim;

impl Lint for LintWpEvidenceMissingForClaim {
    fn id(&self) -> &'static str {
        "LINT-WP-EVIDENCE-MISSING-FOR-CLAIM"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        ctx.report_type_id == "whitepaper"
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let claim_re = Regex::new(
            r"\d+(?:[.,]\d+)?\s*(%|Mio\.?|Mrd\.?|bn|x|fold|×)|\b(POD|TRL|signal-to-noise|SNR)\b\s*\d",
        )
        .expect("claim regex compiles");
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let tid = block
                .block_template_id
                .as_deref()
                .unwrap_or(&block.block_id);
            if tid != "argument_section" {
                continue;
            }
            if !claim_re.is_match(&block.markdown) {
                continue;
            }
            if !block.used_reference_ids.is_empty() {
                continue;
            }
            out.push(LintIssue {
                lint_id: self.id(),
                severity: self.severity(),
                instance_ids: vec![block.instance_id.clone()],
                reason: format!(
                    "{title} stellt eine quantitative oder methodenspezifische Behauptung auf, ohne registrierte Quelle.",
                    title = block.title,
                ),
                goal: format!(
                    "Verlinke die Aussage in {title} mit einem Evidenz-Eintrag (oder streiche die Quantifizierung).",
                    title = block.title,
                ),
            });
        }
        out
    }
}

struct LintWpFillerOpening;

impl Lint for LintWpFillerOpening {
    fn id(&self) -> &'static str {
        "LINT-WP-FILLER-OPENING"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        ctx.report_type_id == "whitepaper"
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Soft
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let patterns = [
            r"(?i)^in\s+today's\b",
            r"(?i)^in\s+der\s+heutigen\b",
            r"(?i)^paradigm\s+shift\b",
            r"(?i)^(next|cutting)[-\s]edge\b",
            r"(?i)^state[-\s]of[-\s]the[-\s]art\b",
            r"(?i)^im\s+zeitalter\s+der\b",
            r"(?i)^wir\s+leben\s+in\b",
        ];
        let regs: Vec<Regex> = patterns
            .iter()
            .map(|p| Regex::new(p).expect("filler regex compiles"))
            .collect();
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            // First non-blank, non-heading line.
            let first_line = block
                .markdown
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty() && !l.starts_with('#'))
                .unwrap_or("");
            if first_line.is_empty() {
                continue;
            }
            for (i, re) in regs.iter().enumerate() {
                if re.is_match(first_line) {
                    out.push(LintIssue {
                        lint_id: self.id(),
                        severity: self.severity(),
                        instance_ids: vec![block.instance_id.clone()],
                        reason: format!(
                            "{title} öffnet mit einer Whitepaper-Floskel (\"{phrase}\").",
                            title = block.title,
                            phrase = patterns[i],
                        ),
                        goal: format!(
                            "Beginne {title} mit einer konkreten Aussage zum Untersuchungsgegenstand statt mit einer Zeitgeist-Floskel.",
                            title = block.title,
                        ),
                    });
                    break;
                }
            }
        }
        out
    }
}

// ============ Decision-brief (3) ============

struct LintDbRecommendationBuried;

impl Lint for LintDbRecommendationBuried {
    fn id(&self) -> &'static str {
        "LINT-DB-RECOMMENDATION-BURIED"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        ctx.report_type_id == "decision_brief"
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let mut sorted: Vec<&BlockRecord> = ctx.committed_blocks.iter().collect();
        sorted.sort_by_key(|b| (b.doc_id.clone(), b.ord));
        let total = sorted.len();
        if total == 0 {
            return Vec::new();
        }
        let position = sorted.iter().position(|b| {
            let tid = b.block_template_id.as_deref().unwrap_or(&b.block_id);
            tid == "recommendation_brief"
        });
        let position = match position {
            Some(p) => p,
            None => return Vec::new(),
        };
        let cutoff = ((total as f64) / 3.0).ceil() as usize;
        if position < cutoff {
            return Vec::new();
        }
        let block = sorted[position];
        vec![LintIssue {
            lint_id: self.id(),
            severity: self.severity(),
            instance_ids: vec![block.instance_id.clone()],
            reason: format!(
                "recommendation_brief steht erst an Position {pos}/{total} — nicht im Vorderteil des Dokuments.",
                pos = position + 1,
            ),
            goal: "Verschiebe recommendation_brief in das vordere Drittel; führe situation, options_summary und criteria nach der Empfehlung als Begründung an.".to_string(),
        }]
    }
}

struct LintDbHedgeRecommendation;

impl Lint for LintDbHedgeRecommendation {
    fn id(&self) -> &'static str {
        "LINT-DB-HEDGE-RECOMMENDATION"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        ctx.report_type_id == "decision_brief"
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Critical
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let hedge_re = Regex::new(
            r"(?i)(should\s+consider|may\s+want\s+to|recommend\s+exploring|empfehle\s+weiter\s+zu\s+prüfen|sollte\s+erwogen\s+werden|könnte\s+sinnvoll\s+sein|wäre\s+zu\s+prüfen)",
        )
        .expect("hedge regex compiles");
        let decision_re = Regex::new(
            r"(?i)\b(recommend|not\s+recommended|recommend\s+with\s+caveats|empfohlen|nicht\s+empfohlen|empfohlen\s+mit\s+Auflagen)\b",
        )
        .expect("decision regex compiles");
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let tid = block
                .block_template_id
                .as_deref()
                .unwrap_or(&block.block_id);
            if tid != "recommendation_brief" {
                continue;
            }
            let hedge_match = hedge_re.captures(&block.markdown);
            let has_decision = decision_re.is_match(&block.markdown);
            if let Some(cap) = hedge_match {
                if has_decision {
                    continue;
                }
                let phrase = cap.get(0).map(|m| m.as_str()).unwrap_or("").to_string();
                out.push(LintIssue {
                    lint_id: self.id(),
                    severity: self.severity(),
                    instance_ids: vec![block.instance_id.clone()],
                    reason: format!(
                        "recommendation_brief hedgt (\"{phrase}\") ohne klare Empfehlungsentscheidung.",
                        phrase = phrase,
                    ),
                    goal: format!(
                        "Formuliere {title} als binäre Empfehlung (\"empfohlen\", \"nicht empfohlen\", \"empfohlen mit Auflagen\") und liste Auflagen separat.",
                        title = block.title,
                    ),
                });
            }
        }
        out
    }
}

struct LintDbCriteriaWithoutWeights;

impl Lint for LintDbCriteriaWithoutWeights {
    fn id(&self) -> &'static str {
        "LINT-DB-CRITERIA-WITHOUT-WEIGHTS"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        match ctx.report_type_id {
            "decision_brief" => true,
            "competitive_analysis" => ctx
                .report_type
                .block_library_keys
                .iter()
                .any(|k| k == "capability_axes"),
            _ => false,
        }
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Soft
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let bullet_re = Regex::new(r"(?m)^\s*[-*]\s+").expect("bullet regex compiles");
        let weight_re =
            Regex::new(r"\d+\s*%|\bweight(s|ing)?\b|\bGewicht(ung)?\b|\bpriorit(y|ät)\b")
                .expect("weight regex compiles");
        let order_terms = ["zuerst", "vor allem", "primär", "priorisierend"];
        let mut out: Vec<LintIssue> = Vec::new();
        for block in ctx.committed_blocks {
            let tid = block
                .block_template_id
                .as_deref()
                .unwrap_or(&block.block_id);
            if tid != "criteria" {
                continue;
            }
            let bullets = bullet_re.find_iter(&block.markdown).count();
            if bullets < 3 {
                continue;
            }
            if weight_re.is_match(&block.markdown) {
                continue;
            }
            let lower = block.markdown.to_lowercase();
            if order_terms.iter().any(|t| lower.contains(t)) {
                continue;
            }
            out.push(LintIssue {
                lint_id: self.id(),
                severity: self.severity(),
                instance_ids: vec![block.instance_id.clone()],
                reason: format!(
                    "Kriterienliste in {title} ohne Gewichtung oder explizite Priorisierung.",
                    title = block.title,
                ),
                goal: format!(
                    "Ergänze in {title} entweder Prozent-Gewichte (Summe 100 %) oder eine sichtbare Reihenfolge mit Begründung.",
                    title = block.title,
                ),
            });
        }
        out
    }
}

// ============ Literature-review (2) ============

struct LintLrThemeImbalance;

impl Lint for LintLrThemeImbalance {
    fn id(&self) -> &'static str {
        "LINT-LR-THEME-IMBALANCE"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        ctx.report_type_id == "literature_review"
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Soft
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        let mut theme_blocks: Vec<&BlockRecord> = Vec::new();
        for block in ctx.committed_blocks {
            let tid = block
                .block_template_id
                .as_deref()
                .unwrap_or(&block.block_id);
            if tid == "theme_section" {
                theme_blocks.push(block);
            }
        }
        if theme_blocks.len() < 2 {
            return Vec::new();
        }
        // Distinct evidence ids per theme.
        let mut theme_sets: Vec<(String, BTreeSet<String>)> = theme_blocks
            .iter()
            .map(|b| {
                let set: BTreeSet<String> = b.used_reference_ids.iter().cloned().collect();
                (b.instance_id.clone(), set)
            })
            .collect();
        let mut total_distinct: BTreeSet<String> = BTreeSet::new();
        for (_, s) in &theme_sets {
            for id in s {
                total_distinct.insert(id.clone());
            }
        }
        if total_distinct.is_empty() {
            return Vec::new();
        }
        let total = total_distinct.len() as f64;
        let mut high: Vec<(String, f64, String)> = Vec::new();
        let mut low: Vec<(String, f64, String)> = Vec::new();
        for (id, set) in &theme_sets {
            let share = (set.len() as f64) / total;
            let title = ctx
                .committed_blocks
                .iter()
                .find(|b| b.instance_id == *id)
                .map(|b| b.title.clone())
                .unwrap_or_else(|| id.clone());
            if share > 0.60 {
                high.push((id.clone(), share, title));
            } else if share < 0.10 {
                low.push((id.clone(), share, title));
            }
        }
        if high.is_empty() || low.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<LintIssue> = Vec::new();
        for (id, share, title) in &high {
            let percent = (share * 100.0).round() as i64;
            out.push(LintIssue {
                lint_id: self.id(),
                severity: self.severity(),
                instance_ids: vec![id.clone()],
                reason: format!(
                    "Themenverteilung unausgewogen: {title} hält {percent}% der Quellen, andere unter 10%.",
                ),
                goal: "Bringe Quellen zwischen den Themenblöcken in Balance; verschiebe Belege oder spalte das dominante Thema bei Bedarf in zwei.".to_string(),
            });
        }
        // Cap soft fan-out: 3 highs + 3 lows max.
        let _ = theme_sets.split_off(theme_sets.len().min(theme_sets.len()));
        out
    }
}

struct LintLrNoGapsSection;

impl Lint for LintLrNoGapsSection {
    fn id(&self) -> &'static str {
        "LINT-LR-NO-GAPS-SECTION"
    }
    fn applies(&self, ctx: &LintContext) -> bool {
        ctx.report_type_id == "literature_review"
    }
    fn severity(&self) -> LintSeverity {
        LintSeverity::Hard
    }
    fn check(&self, ctx: &LintContext) -> Vec<LintIssue> {
        // Is the block required by the blueprint?
        let mut required = false;
        let mut min_chars: u32 = 0;
        let mut blueprint_doc_id: Option<String> = None;
        for entry in &ctx.document_blueprint.sequence {
            if entry.block_id == "gaps_and_open_questions" {
                required = required || entry.required;
                if let Ok(library) = ctx.asset_pack.block_library_entry(&entry.block_id) {
                    min_chars = library.min_chars;
                }
                blueprint_doc_id = Some(entry.doc_id.clone());
            }
        }
        if !required {
            return Vec::new();
        }
        // Find the committed block (if any).
        let block = ctx.committed_blocks.iter().find(|b| {
            let tid = b.block_template_id.as_deref().unwrap_or(&b.block_id);
            tid == "gaps_and_open_questions"
        });
        let actual = block
            .map(|b| b.markdown.trim().chars().count() as u32)
            .unwrap_or(0);
        let floor = ((min_chars as f64) * 0.65).ceil() as u32;
        if actual >= floor && actual > 0 {
            return Vec::new();
        }
        let synthetic_id = match block {
            Some(b) => b.instance_id.clone(),
            None => format!(
                "{}__gaps_and_open_questions",
                blueprint_doc_id.unwrap_or_else(|| "literature_review".to_string())
            ),
        };
        vec![LintIssue {
            lint_id: self.id(),
            severity: self.severity(),
            instance_ids: vec![synthetic_id],
            reason: format!(
                "literature_review-Lauf ohne ausgearbeiteten gaps_and_open_questions-Block ({actual} Zeichen, Soll mindestens {min_chars}).",
            ),
            goal: format!(
                "Schreibe einen substantiellen gaps_and_open_questions-Block (mindestens {min_chars} Zeichen), der pro Thema offene Fragen explizit benennt.",
            ),
        }]
    }
}

// ---- silence unused-imports for helpers used only by some lints ---------

#[allow(dead_code)]
fn _silence_unused() {
    let _ = lower_chars("");
    let _ = block_used_reference_ids_set;
    let _ = cap_per_block;
    let _: BTreeMap<String, ()> = BTreeMap::new();
    let _ = LintSeverity::Hard.as_str();
}
