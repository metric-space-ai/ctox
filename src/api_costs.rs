use anyhow::{Context, Result};
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct ApiTokenUsage {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
}

impl ApiTokenUsage {
    pub fn is_zero(self) -> bool {
        self.input_tokens <= 0
            && self.cached_input_tokens <= 0
            && self.output_tokens <= 0
            && self.reasoning_output_tokens <= 0
            && self.total_tokens <= 0
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiCostSummary {
    pub period: String,
    pub label: String,
    pub start_day: String,
    pub end_day: String,
    pub day: String,
    pub events: u64,
    pub provider_count: u64,
    pub model_count: u64,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
    pub priced_events: u64,
    pub unpriced_events: u64,
    pub total_cost_microusd: i64,
    pub by_model: Vec<ApiCostModelSummary>,
}

impl ApiCostSummary {
    pub fn empty_period(period: String, label: String, start_day: String, end_day: String) -> Self {
        Self {
            period,
            label,
            day: start_day.clone(),
            start_day,
            end_day,
            events: 0,
            provider_count: 0,
            model_count: 0,
            input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: 0,
            priced_events: 0,
            unpriced_events: 0,
            total_cost_microusd: 0,
            by_model: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiCostModelSummary {
    pub provider: String,
    pub model: String,
    pub events: u64,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
    pub priced_events: u64,
    pub unpriced_events: u64,
    pub total_cost_microusd: i64,
    pub pricing_source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiPriceRate {
    pub provider: String,
    pub model: String,
    pub input_usd_per_million: f64,
    pub cached_input_usd_per_million: Option<f64>,
    pub output_usd_per_million: f64,
    pub effective_from_day: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy)]
struct PriceRate {
    input_usd_per_million: f64,
    cached_input_usd_per_million: Option<f64>,
    output_usd_per_million: f64,
}

#[derive(Debug, Clone, Copy)]
struct CostEstimate {
    total_microusd: i64,
}

pub fn today_day() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub fn record_api_model_usage(
    root: &Path,
    provider: &str,
    model: &str,
    turn_id: Option<&str>,
    usage: ApiTokenUsage,
) -> Result<()> {
    if provider.trim().is_empty() || model.trim().is_empty() || usage.is_zero() {
        return Ok(());
    }
    let conn = open_cost_db(root)?;
    ensure_schema(&conn)?;
    let now = chrono::Local::now();
    let created_at = now.to_rfc3339();
    let day = now.format("%Y-%m-%d").to_string();
    let provider = normalize_provider(provider);
    conn.execute(
        "INSERT INTO api_model_cost_events (
             created_at, day, provider, model, source, turn_id,
             input_tokens, cached_input_tokens, output_tokens,
             reasoning_output_tokens, total_tokens
         )
         VALUES (?1, ?2, ?3, ?4, 'direct_session', ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            created_at,
            day,
            provider,
            model.trim(),
            turn_id,
            clamp_i64(usage.input_tokens),
            clamp_i64(usage.cached_input_tokens),
            clamp_i64(usage.output_tokens),
            clamp_i64(usage.reasoning_output_tokens),
            clamp_i64(usage.total_tokens),
        ],
    )
    .context("failed to record API model cost event")?;
    Ok(())
}

pub fn summary_for_day(root: &Path, day: &str) -> Result<ApiCostSummary> {
    validate_day(day)?;
    summary_for_range(root, "day", day, day, day)
}

pub fn summary_for_current_week(root: &Path) -> Result<ApiCostSummary> {
    let today = chrono::Local::now().date_naive();
    let (start, end, label) = week_range_for_date(today);
    summary_for_range(root, "week", &label, &format_day(start), &format_day(end))
}

pub fn summary_for_week(root: &Path, week: &str) -> Result<ApiCostSummary> {
    let (start, end, label) = parse_week(week)?;
    summary_for_range(root, "week", &label, &format_day(start), &format_day(end))
}

pub fn summary_for_current_month(root: &Path) -> Result<ApiCostSummary> {
    let today = chrono::Local::now().date_naive();
    let (start, end, label) = month_range_for_date(today)?;
    summary_for_range(root, "month", &label, &format_day(start), &format_day(end))
}

pub fn summary_for_month(root: &Path, month: &str) -> Result<ApiCostSummary> {
    let (start, end, label) = parse_month(month)?;
    summary_for_range(root, "month", &label, &format_day(start), &format_day(end))
}

pub fn summary_for_range(
    root: &Path,
    period: &str,
    label: &str,
    start_day: &str,
    end_day: &str,
) -> Result<ApiCostSummary> {
    validate_day(start_day)?;
    validate_day(end_day)?;
    let conn = open_cost_db(root)?;
    ensure_schema(&conn)?;
    let mut stmt = conn.prepare(
        "SELECT day, provider, model,
                input_tokens, cached_input_tokens, output_tokens,
                reasoning_output_tokens, total_tokens
         FROM api_model_cost_events
         WHERE day >= ?1 AND day <= ?2
         ORDER BY day, provider, model, id",
    )?;
    let rows = stmt.query_map(params![start_day, end_day], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            ApiTokenUsage {
                input_tokens: row.get(3)?,
                cached_input_tokens: row.get(4)?,
                output_tokens: row.get(5)?,
                reasoning_output_tokens: row.get(6)?,
                total_tokens: row.get(7)?,
            },
        ))
    })?;

    let mut summary = ApiCostSummary::empty_period(
        period.to_string(),
        label.to_string(),
        start_day.to_string(),
        end_day.to_string(),
    );
    let mut by_model: BTreeMap<(String, String), ApiCostModelSummary> = BTreeMap::new();
    let mut providers = BTreeMap::<String, ()>::new();
    let mut models = BTreeMap::<String, ()>::new();

    for row in rows {
        let (event_day, provider, model, usage) = row?;
        let provider = normalize_provider(&provider);
        providers.insert(provider.clone(), ());
        models.insert(format!("{provider}/{model}").to_ascii_lowercase(), ());

        let (estimate, pricing_source) =
            estimate_event_cost(&conn, &provider, &model, &event_day, usage)?;
        add_usage_to_summary(
            &mut summary.input_tokens,
            &mut summary.cached_input_tokens,
            &mut summary.output_tokens,
            &mut summary.reasoning_output_tokens,
            &mut summary.total_tokens,
            usage,
        );
        summary.events += 1;
        match estimate {
            Some(cost) => {
                summary.priced_events += 1;
                summary.total_cost_microusd += cost.total_microusd;
            }
            None => summary.unpriced_events += 1,
        }

        let entry = by_model
            .entry((provider.clone(), model.clone()))
            .or_insert_with(|| ApiCostModelSummary {
                provider: provider.clone(),
                model: model.clone(),
                events: 0,
                input_tokens: 0,
                cached_input_tokens: 0,
                output_tokens: 0,
                reasoning_output_tokens: 0,
                total_tokens: 0,
                priced_events: 0,
                unpriced_events: 0,
                total_cost_microusd: 0,
                pricing_source: None,
            });
        add_usage_to_summary(
            &mut entry.input_tokens,
            &mut entry.cached_input_tokens,
            &mut entry.output_tokens,
            &mut entry.reasoning_output_tokens,
            &mut entry.total_tokens,
            usage,
        );
        entry.events += 1;
        match estimate {
            Some(cost) => {
                entry.priced_events += 1;
                entry.total_cost_microusd += cost.total_microusd;
                entry.pricing_source = pricing_source;
            }
            None => entry.unpriced_events += 1,
        }
    }

    summary.provider_count = providers.len() as u64;
    summary.model_count = models.len() as u64;
    summary.by_model = by_model.into_values().collect();
    Ok(summary)
}

pub fn summaries_for_recent_days(root: &Path, days: usize) -> Result<Vec<ApiCostSummary>> {
    let conn = open_cost_db(root)?;
    ensure_schema(&conn)?;
    let mut stmt =
        conn.prepare("SELECT DISTINCT day FROM api_model_cost_events ORDER BY day DESC LIMIT ?1")?;
    let rows = stmt.query_map([days.max(1) as i64], |row| row.get::<_, String>(0))?;
    let mut summaries = Vec::new();
    for row in rows {
        summaries.push(summary_for_day(root, &row?)?);
    }
    Ok(summaries)
}

pub fn summaries_for_recent_weeks(root: &Path, weeks: usize) -> Result<Vec<ApiCostSummary>> {
    let today = chrono::Local::now().date_naive();
    let (current_start, _, _) = week_range_for_date(today);
    let mut summaries = Vec::new();
    for offset in 0..weeks.max(1) {
        let start = current_start - Duration::days((offset * 7) as i64);
        let end = start + Duration::days(6);
        let label = week_label_for_date(start);
        summaries.push(summary_for_range(
            root,
            "week",
            &label,
            &format_day(start),
            &format_day(end),
        )?);
    }
    Ok(summaries)
}

pub fn summaries_for_recent_months(root: &Path, months: usize) -> Result<Vec<ApiCostSummary>> {
    let today = chrono::Local::now().date_naive();
    let mut year = today.year();
    let mut month = today.month();
    let mut summaries = Vec::new();
    for _ in 0..months.max(1) {
        let (start, end, label) = month_range_for_year_month(year, month)?;
        summaries.push(summary_for_range(
            root,
            "month",
            &label,
            &format_day(start),
            &format_day(end),
        )?);
        if month == 1 {
            year -= 1;
            month = 12;
        } else {
            month -= 1;
        }
    }
    Ok(summaries)
}

pub fn list_prices(root: &Path) -> Result<Vec<ApiPriceRate>> {
    let conn = open_cost_db(root)?;
    ensure_schema(&conn)?;
    let mut stmt = conn.prepare(
        "SELECT provider, model, input_usd_per_million,
                cached_input_usd_per_million, output_usd_per_million,
                effective_from_day, updated_at
         FROM api_model_price_rates
         ORDER BY provider, model, effective_from_day DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ApiPriceRate {
            provider: row.get(0)?,
            model: row.get(1)?,
            input_usd_per_million: row.get(2)?,
            cached_input_usd_per_million: row.get(3)?,
            output_usd_per_million: row.get(4)?,
            effective_from_day: row.get(5)?,
            updated_at: row.get(6)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to list API model price rates")
}

pub fn set_price_rate(
    root: &Path,
    provider: &str,
    model: &str,
    input_usd_per_million: f64,
    cached_input_usd_per_million: Option<f64>,
    output_usd_per_million: f64,
    effective_from_day: Option<&str>,
) -> Result<()> {
    anyhow::ensure!(input_usd_per_million >= 0.0, "input price must be >= 0");
    anyhow::ensure!(output_usd_per_million >= 0.0, "output price must be >= 0");
    if let Some(value) = cached_input_usd_per_million {
        anyhow::ensure!(value >= 0.0, "cached input price must be >= 0");
    }
    let provider = normalize_provider(provider);
    let model = model.trim();
    anyhow::ensure!(!provider.is_empty(), "provider is required");
    anyhow::ensure!(!model.is_empty(), "model is required");
    let day = effective_from_day
        .map(ToOwned::to_owned)
        .unwrap_or_else(today_day);
    validate_day(&day)?;
    let conn = open_cost_db(root)?;
    ensure_schema(&conn)?;
    let now = chrono::Local::now().to_rfc3339();
    conn.execute(
        "INSERT INTO api_model_price_rates (
             provider, model, input_usd_per_million,
             cached_input_usd_per_million, output_usd_per_million,
             effective_from_day, updated_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(provider, model, effective_from_day) DO UPDATE SET
             input_usd_per_million = excluded.input_usd_per_million,
             cached_input_usd_per_million = excluded.cached_input_usd_per_million,
             output_usd_per_million = excluded.output_usd_per_million,
             updated_at = excluded.updated_at",
        params![
            provider,
            model,
            input_usd_per_million,
            cached_input_usd_per_million,
            output_usd_per_million,
            day,
            now,
        ],
    )
    .context("failed to set API model price rate")?;
    Ok(())
}

pub fn handle_cost_command(root: &Path, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        None | Some("today") => {
            let json = has_flag(args, "--json");
            let day = find_flag_value(args, "--day").unwrap_or_else(|| today_day());
            validate_day(&day)?;
            let summary = summary_for_day(root, &day)?;
            print_summary(&summary, json)
        }
        Some("daily") => {
            let json = has_flag(args, "--json");
            let days = find_flag_value(args, "--days")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(14);
            let summaries = summaries_for_recent_days(root, days)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&summaries)?);
            } else if summaries.is_empty() {
                println!("No API model cost events recorded yet.");
            } else {
                for summary in summaries {
                    print_summary(&summary, false)?;
                }
            }
            Ok(())
        }
        Some("week") | Some("this-week") => {
            let json = has_flag(args, "--json");
            let summary = match find_flag_value(args, "--week") {
                Some(week) => summary_for_week(root, &week)?,
                None => summary_for_current_week(root)?,
            };
            print_summary(&summary, json)
        }
        Some("weekly") => {
            let json = has_flag(args, "--json");
            let weeks = find_flag_value(args, "--weeks")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(8);
            let summaries = summaries_for_recent_weeks(root, weeks)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&summaries)?);
            } else if summaries.iter().all(|summary| summary.events == 0) {
                println!("No API model cost events recorded in the selected weeks.");
            } else {
                for summary in summaries.iter().filter(|summary| summary.events > 0) {
                    print_summary(summary, false)?;
                }
            }
            Ok(())
        }
        Some("month") | Some("this-month") => {
            let json = has_flag(args, "--json");
            let summary = match find_flag_value(args, "--month") {
                Some(month) => summary_for_month(root, &month)?,
                None => summary_for_current_month(root)?,
            };
            print_summary(&summary, json)
        }
        Some("monthly") => {
            let json = has_flag(args, "--json");
            let months = find_flag_value(args, "--months")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(12);
            let summaries = summaries_for_recent_months(root, months)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&summaries)?);
            } else if summaries.iter().all(|summary| summary.events == 0) {
                println!("No API model cost events recorded in the selected months.");
            } else {
                for summary in summaries.iter().filter(|summary| summary.events > 0) {
                    print_summary(summary, false)?;
                }
            }
            Ok(())
        }
        Some("prices") => {
            let json = has_flag(args, "--json");
            let prices = list_prices(root)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&prices)?);
            } else if prices.is_empty() {
                println!("No API model prices configured.");
                println!("Set one with: ctox cost set-price --provider <provider> --model <model> --input <usd_per_1m> --output <usd_per_1m> [--cached-input <usd_per_1m>]");
            } else {
                for price in prices {
                    println!(
                        "{} {} from {}: input ${:.6}/1M cached {} output ${:.6}/1M",
                        price.provider,
                        price.model,
                        price.effective_from_day,
                        price.input_usd_per_million,
                        price
                            .cached_input_usd_per_million
                            .map(|value| format!("${value:.6}/1M"))
                            .unwrap_or_else(|| "same-as-input".to_string()),
                        price.output_usd_per_million
                    );
                }
            }
            Ok(())
        }
        Some("set-price") => {
            let provider = find_flag_value(args, "--provider")
                .context("usage: ctox cost set-price --provider <provider> --model <model> --input <usd_per_1m> --output <usd_per_1m> [--cached-input <usd_per_1m>] [--effective-from <YYYY-MM-DD>]")?;
            let model = find_flag_value(args, "--model")
                .context("usage: ctox cost set-price --provider <provider> --model <model> --input <usd_per_1m> --output <usd_per_1m> [--cached-input <usd_per_1m>] [--effective-from <YYYY-MM-DD>]")?;
            let input = find_flag_value(args, "--input")
                .context("missing --input <usd_per_1m>")?
                .parse::<f64>()
                .context("failed to parse --input price")?;
            let output = find_flag_value(args, "--output")
                .context("missing --output <usd_per_1m>")?
                .parse::<f64>()
                .context("failed to parse --output price")?;
            let cached_input = find_flag_value(args, "--cached-input")
                .map(|value| value.parse::<f64>().context("failed to parse --cached-input price"))
                .transpose()?;
            let effective_from = find_flag_value(args, "--effective-from");
            set_price_rate(
                root,
                &provider,
                &model,
                input,
                cached_input,
                output,
                effective_from.as_deref(),
            )?;
            println!(
                "Set API model price for {} {} effective {}.",
                normalize_provider(&provider),
                model.trim(),
                effective_from.unwrap_or_else(today_day)
            );
            Ok(())
        }
        _ => anyhow::bail!(
            "usage:\n  ctox cost today [--day <YYYY-MM-DD>] [--json]\n  ctox cost daily [--days <n>] [--json]\n  ctox cost week [--week <YYYY-Www>] [--json]\n  ctox cost weekly [--weeks <n>] [--json]\n  ctox cost month [--month <YYYY-MM>] [--json]\n  ctox cost monthly [--months <n>] [--json]\n  ctox cost prices [--json]\n  ctox cost set-price --provider <provider> --model <model> --input <usd_per_1m> --output <usd_per_1m> [--cached-input <usd_per_1m>] [--effective-from <YYYY-MM-DD>]"
        ),
    }
}

pub fn format_usd_micros(microusd: i64) -> String {
    let sign = if microusd < 0 { "-" } else { "" };
    let value = microusd.abs() as f64 / 1_000_000.0;
    format!("{sign}${value:.6}")
}

fn print_summary(summary: &ApiCostSummary, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(summary)?);
        return Ok(());
    }
    println!(
        "API model costs for {}: {} across {} event(s), {} token(s)",
        summary_period_label(summary),
        format_usd_micros(summary.total_cost_microusd),
        summary.events,
        summary.total_tokens
    );
    if summary.unpriced_events > 0 {
        println!(
            "Unpriced events: {}. Configure rates with `ctox cost set-price`.",
            summary.unpriced_events
        );
    }
    for model in &summary.by_model {
        println!(
            "  {} {}: {} event(s), {} in ({} cached), {} out, {} total, {}{}",
            model.provider,
            model.model,
            model.events,
            model.input_tokens,
            model.cached_input_tokens,
            model.output_tokens,
            model.total_tokens,
            format_usd_micros(model.total_cost_microusd),
            if model.unpriced_events > 0 {
                " + unpriced"
            } else {
                ""
            }
        );
    }
    Ok(())
}

fn summary_period_label(summary: &ApiCostSummary) -> String {
    if summary.period == "day" {
        summary.label.clone()
    } else {
        format!(
            "{} {} ({}..{})",
            summary.period, summary.label, summary.start_day, summary.end_day
        )
    }
}

fn add_usage_to_summary(
    input_tokens: &mut u64,
    cached_input_tokens: &mut u64,
    output_tokens: &mut u64,
    reasoning_output_tokens: &mut u64,
    total_tokens: &mut u64,
    usage: ApiTokenUsage,
) {
    *input_tokens = input_tokens.saturating_add(clamp_i64(usage.input_tokens) as u64);
    *cached_input_tokens =
        cached_input_tokens.saturating_add(clamp_i64(usage.cached_input_tokens) as u64);
    *output_tokens = output_tokens.saturating_add(clamp_i64(usage.output_tokens) as u64);
    *reasoning_output_tokens =
        reasoning_output_tokens.saturating_add(clamp_i64(usage.reasoning_output_tokens) as u64);
    *total_tokens = total_tokens.saturating_add(clamp_i64(usage.total_tokens) as u64);
}

fn estimate_event_cost(
    conn: &Connection,
    provider: &str,
    model: &str,
    day: &str,
    usage: ApiTokenUsage,
) -> Result<(Option<CostEstimate>, Option<String>)> {
    let Some(rate) = price_for_event(conn, provider, model, day)? else {
        return Ok((None, None));
    };
    let cached = clamp_i64(usage.cached_input_tokens);
    let input = clamp_i64(usage.input_tokens);
    let non_cached_input = input.saturating_sub(cached);
    let cached_price = rate
        .cached_input_usd_per_million
        .unwrap_or(rate.input_usd_per_million);
    let cost = tokens_to_microusd(non_cached_input, rate.input_usd_per_million)
        + tokens_to_microusd(cached, cached_price)
        + tokens_to_microusd(clamp_i64(usage.output_tokens), rate.output_usd_per_million);
    Ok((
        Some(CostEstimate {
            total_microusd: cost,
        }),
        Some("configured".to_string()),
    ))
}

fn price_for_event(
    conn: &Connection,
    provider: &str,
    model: &str,
    day: &str,
) -> Result<Option<PriceRate>> {
    conn.query_row(
        "SELECT input_usd_per_million, cached_input_usd_per_million,
                output_usd_per_million
         FROM api_model_price_rates
         WHERE provider = ?1
           AND lower(model) = lower(?2)
           AND effective_from_day <= ?3
         ORDER BY effective_from_day DESC
         LIMIT 1",
        params![normalize_provider(provider), model.trim(), day],
        |row| {
            Ok(PriceRate {
                input_usd_per_million: row.get(0)?,
                cached_input_usd_per_million: row.get(1)?,
                output_usd_per_million: row.get(2)?,
            })
        },
    )
    .optional()
    .context("failed to load API model price")
}

fn tokens_to_microusd(tokens: i64, usd_per_million: f64) -> i64 {
    ((tokens.max(0) as f64) * usd_per_million).round() as i64
}

fn open_cost_db(root: &Path) -> Result<Connection> {
    let path = crate::paths::core_db(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open cost database {}", path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    Ok(conn)
}

fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS api_model_cost_events (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             created_at TEXT NOT NULL,
             day TEXT NOT NULL,
             provider TEXT NOT NULL,
             model TEXT NOT NULL,
             source TEXT NOT NULL,
             turn_id TEXT,
             input_tokens INTEGER NOT NULL DEFAULT 0,
             cached_input_tokens INTEGER NOT NULL DEFAULT 0,
             output_tokens INTEGER NOT NULL DEFAULT 0,
             reasoning_output_tokens INTEGER NOT NULL DEFAULT 0,
             total_tokens INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_api_model_cost_events_day
             ON api_model_cost_events(day);
         CREATE INDEX IF NOT EXISTS idx_api_model_cost_events_model_day
             ON api_model_cost_events(provider, model, day);

         CREATE TABLE IF NOT EXISTS api_model_price_rates (
             provider TEXT NOT NULL,
             model TEXT NOT NULL,
             input_usd_per_million REAL NOT NULL,
             cached_input_usd_per_million REAL,
             output_usd_per_million REAL NOT NULL,
             effective_from_day TEXT NOT NULL,
             updated_at TEXT NOT NULL,
             PRIMARY KEY(provider, model, effective_from_day)
         );",
    )
    .context("failed to initialize API model cost schema")
}

fn normalize_provider(provider: &str) -> String {
    crate::inference::runtime_state::normalize_api_provider(provider).to_string()
}

fn clamp_i64(value: i64) -> i64 {
    value.max(0)
}

fn validate_day(day: &str) -> Result<()> {
    chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .with_context(|| format!("invalid day {day}; expected YYYY-MM-DD"))?;
    Ok(())
}

fn parse_week(week: &str) -> Result<(NaiveDate, NaiveDate, String)> {
    let (year, week_number) = week
        .split_once("-W")
        .with_context(|| format!("invalid week {week}; expected YYYY-Www"))?;
    let year = year
        .parse::<i32>()
        .with_context(|| format!("invalid week year in {week}"))?;
    let week_number = week_number
        .parse::<u32>()
        .with_context(|| format!("invalid week number in {week}"))?;
    let start = NaiveDate::from_isoywd_opt(year, week_number, Weekday::Mon)
        .with_context(|| format!("invalid ISO week {week}"))?;
    let end = start + Duration::days(6);
    Ok((start, end, format!("{year}-W{week_number:02}")))
}

fn parse_month(month: &str) -> Result<(NaiveDate, NaiveDate, String)> {
    let (year, month_number) = month
        .split_once('-')
        .with_context(|| format!("invalid month {month}; expected YYYY-MM"))?;
    let year = year
        .parse::<i32>()
        .with_context(|| format!("invalid month year in {month}"))?;
    let month_number = month_number
        .parse::<u32>()
        .with_context(|| format!("invalid month number in {month}"))?;
    month_range_for_year_month(year, month_number)
}

fn week_range_for_date(date: NaiveDate) -> (NaiveDate, NaiveDate, String) {
    let weekday_offset = date.weekday().num_days_from_monday() as i64;
    let start = date - Duration::days(weekday_offset);
    let end = start + Duration::days(6);
    (start, end, week_label_for_date(date))
}

fn week_label_for_date(date: NaiveDate) -> String {
    let week = date.iso_week();
    format!("{}-W{:02}", week.year(), week.week())
}

fn month_range_for_date(date: NaiveDate) -> Result<(NaiveDate, NaiveDate, String)> {
    month_range_for_year_month(date.year(), date.month())
}

fn month_range_for_year_month(year: i32, month: u32) -> Result<(NaiveDate, NaiveDate, String)> {
    let start = NaiveDate::from_ymd_opt(year, month, 1)
        .with_context(|| format!("invalid month {year}-{month:02}; expected YYYY-MM"))?;
    let next_month = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .context("failed to compute next month")?;
    let end = next_month - Duration::days(1);
    Ok((start, end, format!("{year}-{month:02}")))
}

fn format_day(day: NaiveDate) -> String {
    day.format("%Y-%m-%d").to_string()
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn find_flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter().enumerate().find_map(|(idx, arg)| {
        if arg == flag {
            args.get(idx + 1).cloned()
        } else {
            arg.strip_prefix(&format!("{flag}=")).map(ToOwned::to_owned)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn daily_summary_prices_configured_events() -> Result<()> {
        let tmp = TempDir::new()?;
        set_price_rate(
            tmp.path(),
            "openai",
            "gpt-test",
            1.0,
            Some(0.25),
            5.0,
            Some("2026-01-01"),
        )?;
        record_api_model_usage(
            tmp.path(),
            "openai",
            "gpt-test",
            Some("turn-1"),
            ApiTokenUsage {
                input_tokens: 1_000,
                cached_input_tokens: 400,
                output_tokens: 200,
                reasoning_output_tokens: 0,
                total_tokens: 1_200,
            },
        )?;
        let summary = summary_for_day(tmp.path(), &today_day())?;
        assert_eq!(summary.events, 1);
        assert_eq!(summary.unpriced_events, 0);
        assert_eq!(summary.total_cost_microusd, 1_700);
        Ok(())
    }

    #[test]
    fn daily_summary_surfaces_unpriced_events() -> Result<()> {
        let tmp = TempDir::new()?;
        record_api_model_usage(
            tmp.path(),
            "anthropic",
            "claude-test",
            None,
            ApiTokenUsage {
                input_tokens: 10,
                cached_input_tokens: 0,
                output_tokens: 5,
                reasoning_output_tokens: 1,
                total_tokens: 15,
            },
        )?;
        let summary = summary_for_day(tmp.path(), &today_day())?;
        assert_eq!(summary.events, 1);
        assert_eq!(summary.unpriced_events, 1);
        assert_eq!(summary.total_cost_microusd, 0);
        Ok(())
    }

    #[test]
    fn week_and_month_summaries_include_current_day_events() -> Result<()> {
        let tmp = TempDir::new()?;
        set_price_rate(
            tmp.path(),
            "openai",
            "gpt-test",
            1.0,
            None,
            5.0,
            Some("2026-01-01"),
        )?;
        record_api_model_usage(
            tmp.path(),
            "openai",
            "gpt-test",
            Some("turn-1"),
            ApiTokenUsage {
                input_tokens: 1_000,
                cached_input_tokens: 0,
                output_tokens: 200,
                reasoning_output_tokens: 0,
                total_tokens: 1_200,
            },
        )?;

        let week = summary_for_current_week(tmp.path())?;
        let month = summary_for_current_month(tmp.path())?;

        assert_eq!(week.period, "week");
        assert_eq!(week.events, 1);
        assert_eq!(week.total_cost_microusd, 2_000);
        assert_eq!(month.period, "month");
        assert_eq!(month.events, 1);
        assert_eq!(month.total_cost_microusd, 2_000);
        Ok(())
    }
}
