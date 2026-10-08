// Origin: CTOX
// License: AGPL-3.0-only

//! Deterministic, nominal EUR proceeds at 60 calendar months. The SaaS adapter
//! follows section 22 of the five-year exit-model concept; its teaching inputs
//! are never runtime defaults. A finite equity grid is a distinct adapter.
use anyhow::{ensure, Context};
use chrono::{Months, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub(super) const CONTRACT: &str = "ctox.workjet.exit_model.v1";
pub(super) const ENGINE: &str = "finite-saas-1";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Source {
    pub id: String,
    pub reference: String,
    pub observed_at: String,
    pub valid_until: String,
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Resources {
    pub hours_per_week: f64,
    pub monthly_budget_eur: f64,
    pub comparison_mode: String,
}
impl Resources {
    pub(super) fn validate(&self) -> anyhow::Result<()> {
        nonnegative("hours_per_week", self.hours_per_week)?;
        nonnegative("monthly_budget_eur", self.monthly_budget_eur)?;
        ensure!(
            self.hours_per_week <= 168.0,
            "hours_per_week exceeds one person's week"
        );
        ensure!(
            matches!(
                self.comparison_mode.as_str(),
                "equal_resources" | "project_specific"
            ),
            "unsupported comparison_mode"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Plan {
    pub version: String,
    pub mode: String,
    pub comparison_mode: String,
    pub confirmed: bool,
    pub hours_per_week: f64,
    pub assumptions: Vec<String>,
    pub source_ids: Vec<String>,
    pub opening_customers: f64,
    pub opening_cash: f64,
    pub paid_marketing: Vec<f64>,
    pub brand_budget: Vec<f64>,
    pub cash_opex: Vec<f64>,
    pub equity_funding: Vec<f64>,
    pub capex: Vec<f64>,
    pub sales_capacity: Vec<f64>,
    pub onboarding_capacity: Vec<f64>,
    pub service_capacity: Vec<f64>,
    pub reserve_months: f64,
    pub tax_proxy: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Scenario {
    pub name: String,
    pub source_ids: Vec<String>,
    pub weight_given_build: f64,
    pub launch_month: u8,
    pub sales_lag: u8,
    pub sam0: f64,
    pub sam_annual_growth: f64,
    pub cpl: f64,
    pub conversion: f64,
    pub organic_leads0: f64,
    pub organic_annual_growth: f64,
    pub logo_churn: f64,
    pub arpa0: f64,
    pub arpa_annual_growth: f64,
    pub gross_margin: f64,
    pub brand0: f64,
    pub brand_ceiling: f64,
    pub brand_speed: f64,
    pub brand_reference_budget: f64,
    pub brand_organic_lift: f64,
    pub brand_price_lift: f64,
    pub multiple_basis: String,
    pub multiple: f64,
    pub sale_probability: f64,
    pub debt_like_exit: f64,
    pub wc_adjustment: f64,
    pub failure_sale_probability: f64,
    pub failure_equity_price: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Outcome {
    pub state: String,
    pub probability: f64,
    pub sale_probability: f64,
    pub equity_price_eur: f64,
    pub source_ids: Vec<String>,
    #[serde(default)]
    pub ev_eur: f64,
    #[serde(default)]
    pub excess_cash_eur: f64,
    #[serde(default)]
    pub funding_eur: f64,
    #[serde(default)]
    pub cash_failure_month: Option<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Inputs {
    pub adapter: String,
    pub sale_perimeter: String,
    pub rights_confirmed: bool,
    pub perimeter_source_ids: Vec<String>,
    pub sources: Vec<Source>,
    pub plan: Plan,
    #[serde(default)]
    pub scenarios: Vec<Scenario>,
    #[serde(default)]
    pub outcomes: Vec<Outcome>,
    pub build_probability: f64,
    pub technical_failure_sale_probability: f64,
    pub technical_failure_equity_price: f64,
    pub probability_source_ids: Vec<String>,
}

pub(super) fn date(value: &str) -> anyhow::Result<NaiveDate> {
    ensure!(
        value.len() == 10 && value.is_ascii(),
        "date must be YYYY-MM-DD"
    );
    let parsed = NaiveDate::parse_from_str(value, "%Y-%m-%d").context("date must be YYYY-MM-DD")?;
    ensure!(
        parsed.to_string() == value,
        "date must use canonical YYYY-MM-DD"
    );
    Ok(parsed)
}
pub(super) fn add_months(value: &str, months: u32) -> anyhow::Result<String> {
    let shifted = date(value)?
        .checked_add_months(Months::new(months))
        .context("calendar horizon overflow")?
        .to_string();
    date(&shifted)?;
    Ok(shifted)
}
fn nonnegative(name: &str, value: f64) -> anyhow::Result<()> {
    ensure!(
        value.is_finite() && value >= 0.0,
        "invalid nonnegative {name}"
    );
    Ok(())
}
fn probability(name: &str, value: f64) -> anyhow::Result<()> {
    nonnegative(name, value)?;
    ensure!(value <= 1.0, "invalid probability {name}");
    Ok(())
}
fn text(name: &str, value: &str, max: usize) -> anyhow::Result<()> {
    ensure!(
        !value.trim().is_empty()
            && value.chars().count() <= max
            && !value.chars().any(char::is_control),
        "invalid {name}"
    );
    Ok(())
}
fn vector(name: &str, values: &[f64]) -> anyhow::Result<()> {
    ensure!(values.len() == 60, "{name} must contain 60 months");
    for value in values {
        nonnegative(name, *value)?;
    }
    Ok(())
}

pub(super) fn validate(inputs: &Inputs, as_of: &str) -> anyhow::Result<Vec<String>> {
    let as_of = date(as_of)?;
    text("plan.version", &inputs.plan.version, 128)?;
    ensure!(
        inputs.sources.len() <= 200 && inputs.scenarios.len() <= 32 && inputs.outcomes.len() <= 64,
        "input count exceeds bounded engine budget"
    );
    ensure!(
        inputs.plan.assumptions.len() <= 100,
        "too many plan assumptions"
    );
    for assumption in &inputs.plan.assumptions {
        text("assumption", assumption, 2048)?;
    }
    for scenario in &inputs.scenarios {
        validate_refs(&scenario.source_ids)?;
    }
    for outcome in &inputs.outcomes {
        validate_refs(&outcome.source_ids)?;
    }
    let mut missing = BTreeSet::new();
    if inputs.plan.mode != "committed_plan" {
        missing.insert("supported_committed_plan".to_owned());
    }
    if !inputs.plan.confirmed {
        missing.insert("confirmed_resource_plan".to_owned());
    }
    if inputs.sale_perimeter != "100_percent_equity_before_fees_and_personal_tax" {
        missing.insert("sale_perimeter".to_owned());
    }
    if !inputs.rights_confirmed {
        missing.insert("transferable_rights".to_owned());
    }
    Resources {
        hours_per_week: inputs.plan.hours_per_week,
        monthly_budget_eur: 0.0,
        comparison_mode: inputs.plan.comparison_mode.clone(),
    }
    .validate()?;
    for (name, value) in [
        ("opening_customers", inputs.plan.opening_customers),
        ("opening_cash", inputs.plan.opening_cash),
        ("reserve_months", inputs.plan.reserve_months),
    ] {
        nonnegative(name, value)?;
    }
    probability("tax_proxy", inputs.plan.tax_proxy)?;
    let expense_total: f64 = inputs
        .plan
        .paid_marketing
        .iter()
        .chain(&inputs.plan.brand_budget)
        .chain(&inputs.plan.cash_opex)
        .chain(&inputs.plan.capex)
        .sum();
    ensure!(expense_total.is_finite(), "resource plan expense overflow");
    for (name, values) in [
        ("paid_marketing", &inputs.plan.paid_marketing),
        ("brand_budget", &inputs.plan.brand_budget),
        ("cash_opex", &inputs.plan.cash_opex),
        ("equity_funding", &inputs.plan.equity_funding),
        ("capex", &inputs.plan.capex),
        ("sales_capacity", &inputs.plan.sales_capacity),
        ("onboarding_capacity", &inputs.plan.onboarding_capacity),
        ("service_capacity", &inputs.plan.service_capacity),
    ] {
        vector(name, values)?;
    }
    let mut source_ids = BTreeSet::new();
    for source in &inputs.sources {
        text("source.id", &source.id, 128)?;
        text("source.reference", &source.reference, 4096)?;
        ensure!(source_ids.insert(source.id.clone()), "duplicate source id");
        ensure!(
            matches!(source.kind.as_str(), "observed" | "derived" | "assumed"),
            "unsupported source kind"
        );
        let observed = date(&source.observed_at)?;
        let until = date(&source.valid_until)?;
        ensure!(observed <= until, "source validity precedes observation");
        if observed > as_of || until < as_of {
            missing.insert(format!("current_source:{}", source.id));
        }
    }
    let mut check_refs = |name: &str, refs: &[String]| -> anyhow::Result<()> {
        validate_refs(refs)?;
        if refs.is_empty() {
            missing.insert(format!("source:{name}"));
        }
        for id in refs {
            if !source_ids.contains(id) {
                missing.insert(format!("source:{id}"));
            }
        }
        Ok(())
    };
    check_refs("plan", &inputs.plan.source_ids)?;
    check_refs("sale_perimeter", &inputs.perimeter_source_ids)?;
    check_refs("probabilities", &inputs.probability_source_ids)?;
    probability("build_probability", inputs.build_probability)?;
    probability(
        "technical_failure_sale_probability",
        inputs.technical_failure_sale_probability,
    )?;
    nonnegative(
        "technical_failure_equity_price",
        inputs.technical_failure_equity_price,
    )?;
    if inputs.adapter == "saas_reference_v1" {
        ensure!(
            inputs.outcomes.is_empty(),
            "SaaS and terminal equity outcomes must not be combined"
        );
        ensure!(
            !inputs.scenarios.is_empty()
                && (inputs
                    .scenarios
                    .iter()
                    .map(|s| s.weight_given_build)
                    .sum::<f64>()
                    - 1.0)
                    .abs()
                    <= 1e-10,
            "conditional scenario weights must sum to 1"
        );
        let mut names = BTreeSet::new();
        for s in &inputs.scenarios {
            text("scenario.name", &s.name, 128)?;
            ensure!(names.insert(&s.name), "duplicate scenario name");
            check_refs(&s.name, &s.source_ids)?;
            for (name, value) in [
                ("weight_given_build", s.weight_given_build),
                ("conversion", s.conversion),
                ("logo_churn", s.logo_churn),
                ("gross_margin", s.gross_margin),
                ("brand0", s.brand0),
                ("brand_ceiling", s.brand_ceiling),
                ("brand_speed", s.brand_speed),
                ("sale_probability", s.sale_probability),
                ("failure_sale_probability", s.failure_sale_probability),
            ] {
                probability(name, value)?;
            }
            for (name, value) in [
                ("sam0", s.sam0),
                ("organic_leads0", s.organic_leads0),
                ("arpa0", s.arpa0),
                ("brand_organic_lift", s.brand_organic_lift),
                ("brand_price_lift", s.brand_price_lift),
                ("multiple", s.multiple),
                ("debt_like_exit", s.debt_like_exit),
                ("failure_equity_price", s.failure_equity_price),
            ] {
                nonnegative(name, value)?;
            }
            for (name, value) in [
                ("cpl", s.cpl),
                ("brand_reference_budget", s.brand_reference_budget),
            ] {
                ensure!(value.is_finite() && value > 0.0, "{name} must be positive");
            }
            for value in [
                s.sam_annual_growth,
                s.organic_annual_growth,
                s.arpa_annual_growth,
            ] {
                ensure!(value.is_finite() && value > -1.0, "invalid annual growth");
            }
            ensure!(
                s.wc_adjustment.is_finite()
                    && (1..=61).contains(&s.launch_month)
                    && s.sales_lag <= 60
                    && s.brand0 <= s.brand_ceiling,
                "invalid scenario timing, cash bridge or brand ceiling"
            );
            ensure!(
                matches!(s.multiple_basis.as_str(), "ARR" | "LTM_REVENUE" | "EBITDA"),
                "unsupported multiple basis"
            );
        }
    } else if inputs.adapter == "terminal_equity_grid_v1" {
        // The grid accepts researched *equity* states at the horizon. It does
        // not calculate game or IP economics or infer prices from SaaS ARR.
        ensure!(
            inputs.scenarios.is_empty(),
            "terminal equity and SaaS adapters cannot be combined"
        );
        ensure!(
            inputs.build_probability == 1.0
                && inputs.technical_failure_sale_probability == 0.0
                && inputs.technical_failure_equity_price == 0.0,
            "terminal grid already includes failure; extra build factors are forbidden"
        );
        validate_outcomes(&inputs.outcomes)?;
        for o in &inputs.outcomes {
            check_refs(&o.state, &o.source_ids)?;
        }
    } else {
        missing.insert("supported_adapter".to_owned());
    }
    Ok(missing.into_iter().collect())
}

fn simulate(p: &Plan, s: &Scenario, joint: f64) -> anyhow::Result<Outcome> {
    let (mut customers, mut cash, mut brand) = (p.opening_customers, p.opening_cash, s.brand0);
    let mut wins = Vec::new();
    let mut revenues = Vec::new();
    let mut ebitdas = Vec::new();
    let mut mrr = 0.0;
    let mut funding = 0.0;
    for i in 0..60 {
        let month = i + 1;
        let sam = s.sam0 * (1.0 + s.sam_annual_growth).powf(month as f64 / 12.0);
        let effort = (p.brand_budget[i] / s.brand_reference_budget).min(1.0);
        brand += s.brand_speed * effort * (s.brand_ceiling - brand).max(0.0);
        let increment = brand - s.brand0;
        let arpa = s.arpa0
            * (1.0 + s.arpa_annual_growth).powf(month as f64 / 12.0)
            * (1.0 + s.brand_price_lift * increment);
        let organic = s.organic_leads0
            * (1.0 + s.organic_annual_growth).powf(month as f64 / 12.0)
            * (1.0 + s.brand_organic_lift * increment);
        wins.push(if month >= s.launch_month as usize {
            (p.paid_marketing[i] / s.cpl + organic) * s.conversion
        } else {
            0.0
        });
        let demand = if month >= s.launch_month as usize && i >= s.sales_lag as usize {
            wins[i - s.sales_lag as usize]
        } else {
            0.0
        };
        let survivors = (customers * (1.0 - s.logo_churn))
            .min(sam)
            .min(p.service_capacity[i]);
        let additions = demand
            .min(p.sales_capacity[i])
            .min(p.onboarding_capacity[i])
            .min((sam - survivors).max(0.0))
            .min((p.service_capacity[i] - survivors).max(0.0));
        customers = survivors + additions;
        mrr = customers * arpa;
        let ebitda =
            mrr * s.gross_margin - p.cash_opex[i] - p.paid_marketing[i] - p.brand_budget[i];
        funding += p.equity_funding[i];
        cash += ebitda - ebitda.max(0.0) * p.tax_proxy - p.capex[i] + p.equity_funding[i];
        ensure!(
            [sam, brand, arpa, organic, customers, mrr, ebitda, cash, funding]
                .iter()
                .all(|v| v.is_finite()),
            "numeric overflow in operational path"
        );
        revenues.push(mrr);
        ebitdas.push(ebitda);
        if cash < -1e-8 {
            return Ok(Outcome {
                state: format!("{}:cash_failure", s.name),
                probability: joint,
                sale_probability: s.failure_sale_probability,
                equity_price_eur: s.failure_equity_price,
                source_ids: s.source_ids.clone(),
                ev_eur: 0.0,
                excess_cash_eur: 0.0,
                funding_eur: funding,
                cash_failure_month: Some(month as u8),
            });
        }
    }
    let revenue_ltm: f64 = revenues[48..].iter().sum();
    let ebitda_ltm: f64 = ebitdas[48..].iter().sum();
    let metric = match s.multiple_basis.as_str() {
        "ARR" => 12.0 * mrr,
        "LTM_REVENUE" => revenue_ltm,
        "EBITDA" => {
            ensure!(
                ebitda_ltm > 0.0,
                "nonpositive EBITDA requires another valuation route"
            );
            ebitda_ltm
        }
        _ => anyhow::bail!("unsupported multiple basis"),
    };
    let ev = metric * s.multiple;
    let reserve = p.reserve_months * (p.cash_opex[59] + p.paid_marketing[59] + p.brand_budget[59]);
    let excess_cash = (cash - reserve).max(0.0);
    let equity = (ev + excess_cash - s.debt_like_exit + s.wc_adjustment).max(0.0);
    ensure!(
        [ev, reserve, excess_cash, equity]
            .iter()
            .all(|v| v.is_finite()),
        "numeric overflow in EV-equity bridge"
    );
    Ok(Outcome {
        state: s.name.clone(),
        probability: joint,
        sale_probability: s.sale_probability,
        equity_price_eur: equity,
        source_ids: s.source_ids.clone(),
        ev_eur: ev,
        excess_cash_eur: excess_cash,
        funding_eur: funding,
        cash_failure_month: None,
    })
}
fn validate_refs(refs: &[String]) -> anyhow::Result<()> {
    ensure!(refs.len() <= 200, "too many source references");
    for id in refs {
        text("source reference id", id, 128)?;
    }
    Ok(())
}
fn validate_outcomes(outcomes: &[Outcome]) -> anyhow::Result<()> {
    ensure!(
        !outcomes.is_empty()
            && (outcomes.iter().map(|o| o.probability).sum::<f64>() - 1.0).abs() <= 1e-10,
        "joint probabilities must sum to 1"
    );
    let mut names = BTreeSet::new();
    for o in outcomes {
        text("state", &o.state, 128)?;
        ensure!(names.insert(&o.state), "duplicate terminal state");
        probability("state probability", o.probability)?;
        probability("sale probability", o.sale_probability)?;
        nonnegative("equity_price_eur", o.equity_price_eur)?;
        nonnegative("ev_eur", o.ev_eur)?;
        nonnegative("excess_cash_eur", o.excess_cash_eur)?;
        nonnegative("funding_eur", o.funding_eur)?;
        ensure!(
            o.cash_failure_month.is_none_or(|m| (1..=60).contains(&m)),
            "cash failure month must be within 1..60"
        );
        validate_refs(&o.source_ids)?;
    }
    Ok(())
}
pub(super) fn aggregate(outcomes: &[Outcome]) -> anyhow::Result<Value> {
    validate_outcomes(outcomes)?;
    let mass: f64 = outcomes.iter().map(|o| o.probability).sum();
    let mut distribution = Vec::new();
    let mut expected = 0.0;
    let mut sale = 0.0;
    for o in outcomes {
        let weight = o.probability / mass;
        let sold = weight * o.sale_probability;
        expected += sold * o.equity_price_eur;
        sale += sold;
        distribution.push((o.equity_price_eur, sold));
        distribution.push((0.0, weight * (1.0 - o.sale_probability)));
    }
    ensure!(expected.is_finite(), "aggregate numeric overflow");
    let sale = sale.clamp(0.0, 1.0);
    let zero: f64 = distribution
        .iter()
        .filter(|(v, _)| *v == 0.0)
        .map(|(_, p)| p)
        .sum();
    let zero = zero.clamp(0.0, 1.0);
    distribution.retain(|(_, p)| *p > 0.0);
    distribution.sort_by(|a, b| a.0.total_cmp(&b.0));
    let quantile = |q: f64| {
        let mut cdf = 0.0;
        for (value, p) in &distribution {
            cdf += p;
            if cdf + 1e-12 >= q {
                return *value;
            }
        }
        distribution.last().expect("validated probability mass").0
    };
    Ok(
        json!({"expected_exit_equity_eur":expected,"sale_probability":sale,"expected_price_given_sale_eur":if sale>0.0 {Some(expected/sale)} else {None},"probability_zero_proceeds":zero,"p10_eur":quantile(0.1),"p50_eur":quantile(0.5),"p90_eur":quantile(0.9)}),
    )
}
pub(super) fn calculate(inputs: &Inputs) -> anyhow::Result<(Value, Vec<Outcome>)> {
    let mut outcomes = if inputs.adapter == "terminal_equity_grid_v1" {
        inputs.outcomes.clone()
    } else {
        let mut states = vec![Outcome {
            state: "technical_failure".into(),
            probability: 1.0 - inputs.build_probability,
            sale_probability: inputs.technical_failure_sale_probability,
            equity_price_eur: inputs.technical_failure_equity_price,
            source_ids: inputs.probability_source_ids.clone(),
            ev_eur: 0.0,
            excess_cash_eur: 0.0,
            funding_eur: 0.0,
            cash_failure_month: None,
        }];
        for s in &inputs.scenarios {
            let joint = inputs.build_probability * s.weight_given_build;
            if joint > 0.0 {
                states.push(simulate(&inputs.plan, s, joint)?);
            }
        }
        states
    };
    validate_outcomes(&outcomes)?;
    let mass: f64 = outcomes.iter().map(|o| o.probability).sum();
    for outcome in &mut outcomes {
        outcome.probability /= mass;
    }
    Ok((aggregate(&outcomes)?, outcomes))
}
pub(super) fn plan_summary(plan: &Plan) -> Value {
    json!({"mode":plan.mode,"comparison_mode":plan.comparison_mode,"confirmed":plan.confirmed,"budget_eur":plan.paid_marketing.iter().chain(&plan.brand_budget).chain(&plan.cash_opex).chain(&plan.capex).sum::<f64>(),"hours_per_week":plan.hours_per_week,"assumptions":plan.assumptions})
}

#[cfg(test)]
pub(super) fn fixture() -> Inputs {
    serde_json::from_value(json!({
        "adapter":"saas_reference_v1","sale_perimeter":"100_percent_equity_before_fees_and_personal_tax","rights_confirmed":true,
        "perimeter_source_ids":["fixture"],"probability_source_ids":["fixture"],
        "sources":[{"id":"fixture","reference":"test fixture only; not a real project","observed_at":"2026-10-08","valid_until":"2026-12-31","kind":"assumed"}],
        "plan":{"version":"test-v1","mode":"committed_plan","comparison_mode":"equal_resources","confirmed":true,"hours_per_week":20,
        "assumptions":["Fictitious test values"],"source_ids":["fixture"],"opening_customers":10,"opening_cash":100,
        "paid_marketing":vec![0.0;60],"brand_budget":vec![0.0;60],"cash_opex":vec![0.0;60],"equity_funding":vec![0.0;60],"capex":vec![0.0;60],
        "sales_capacity":vec![0.0;60],"onboarding_capacity":vec![0.0;60],"service_capacity":vec![100.0;60],"reserve_months":0,"tax_proxy":0},
        "scenarios":[{"name":"constant","source_ids":["fixture"],"weight_given_build":1,"launch_month":1,"sales_lag":0,"sam0":100,
        "sam_annual_growth":0,"cpl":100,"conversion":0,"organic_leads0":0,"organic_annual_growth":0,"logo_churn":0,"arpa0":10,"arpa_annual_growth":0,
        "gross_margin":1,"brand0":0,"brand_ceiling":0,"brand_speed":0,"brand_reference_budget":100,"brand_organic_lift":0,"brand_price_lift":0,
        "multiple_basis":"ARR","multiple":2,"sale_probability":1,"debt_like_exit":500,"wc_adjustment":50,
        "failure_sale_probability":0,"failure_equity_price":0}],
        "outcomes":[],"build_probability":1,"technical_failure_sale_probability":0,"technical_failure_equity_price":0
    })).expect("valid test fixture")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn operational_reference_example_matches_teaching_kernel() -> anyhow::Result<()> {
        let mut input = fixture();
        let yearly = |blocks: [f64; 5]| {
            blocks
                .into_iter()
                .flat_map(|v| std::iter::repeat_n(v, 12))
                .collect()
        };
        input.plan.opening_customers = 0.0;
        input.plan.opening_cash = 600_000.0;
        input.plan.paid_marketing = yearly([4000.0, 5000.0, 6000.0, 7000.0, 8000.0]);
        input.plan.brand_budget = yearly([1000.0, 1250.0, 1500.0, 1750.0, 2000.0]);
        input.plan.cash_opex = yearly([12000.0, 14000.0, 16000.0, 18000.0, 20000.0]);
        input.plan.equity_funding = vec![10000.0; 60];
        input.plan.capex = vec![0.0; 60];
        input.plan.sales_capacity = yearly([15.0, 20.0, 25.0, 30.0, 35.0]);
        input.plan.onboarding_capacity = yearly([12.0, 18.0, 24.0, 30.0, 36.0]);
        input.plan.service_capacity = yearly([300.0, 600.0, 1000.0, 1500.0, 2000.0]);
        input.plan.reserve_months = 3.0;
        input.plan.tax_proxy = 0.25;
        let mut base = input.scenarios.remove(0);
        base.name = "base".into();
        base.weight_given_build = 0.5;
        base.launch_month = 4;
        base.sales_lag = 2;
        base.sam0 = 10000.0;
        base.sam_annual_growth = 0.03;
        base.cpl = 100.0;
        base.conversion = 0.20;
        base.organic_leads0 = 12.0;
        base.organic_annual_growth = 0.15;
        base.logo_churn = 0.015;
        base.arpa0 = 200.0;
        base.arpa_annual_growth = 0.02;
        base.gross_margin = 0.80;
        base.brand0 = 0.10;
        base.brand_ceiling = 0.60;
        base.brand_speed = 0.06;
        base.brand_reference_budget = 2000.0;
        base.brand_organic_lift = 1.0;
        base.brand_price_lift = 0.15;
        base.multiple = 3.0;
        base.sale_probability = 0.75;
        base.debt_like_exit = 0.0;
        base.wc_adjustment = 0.0;
        base.failure_sale_probability = 0.20;
        base.failure_equity_price = 10000.0;
        let mut weak = base.clone();
        weak.name = "weak".into();
        weak.weight_given_build = 0.3;
        weak.launch_month = 10;
        weak.cpl = 160.0;
        weak.conversion = 0.07;
        weak.logo_churn = 0.035;
        weak.arpa0 = 160.0;
        weak.organic_leads0 = 5.0;
        weak.organic_annual_growth = 0.0;
        weak.brand_ceiling = 0.25;
        weak.multiple = 1.5;
        weak.sale_probability = 0.40;
        let mut strong = base.clone();
        strong.name = "strong".into();
        strong.weight_given_build = 0.2;
        strong.launch_month = 2;
        strong.cpl = 70.0;
        strong.conversion = 0.28;
        strong.logo_churn = 0.008;
        strong.arpa0 = 240.0;
        strong.organic_leads0 = 20.0;
        strong.organic_annual_growth = 0.25;
        strong.brand_ceiling = 0.85;
        strong.multiple = 5.0;
        strong.sale_probability = 0.90;
        input.scenarios = vec![weak, base, strong];
        input.build_probability = 0.80;
        input.technical_failure_sale_probability = 0.20;
        input.technical_failure_equity_price = 20000.0;
        assert!(validate(&input, "2026-10-08")?.is_empty());
        let (result, _) = calculate(&input)?;
        assert_eq!(
            result["expected_exit_equity_eur"].as_f64().unwrap().round(),
            6_119_223.0
        );
        assert!((result["sale_probability"].as_f64().unwrap() - 0.58).abs() < 1e-12);
        assert_eq!(result["p50_eur"].as_f64().unwrap().round(), 265_504.0);
        assert_eq!(result["p90_eur"].as_f64().unwrap().round(), 25_468_218.0);
        Ok(())
    }
    #[test]
    fn cash_and_equity_bridge_preserve_funding_and_failure() -> anyhow::Result<()> {
        let mut input = fixture();
        assert!(validate(&input, "2026-10-08")?.is_empty());
        let (result, states) = calculate(&input)?;
        assert_eq!(states[1].ev_eur, 2400.0);
        assert_eq!(states[1].excess_cash_eur, 6100.0);
        assert_eq!(result["expected_exit_equity_eur"], 8050.0);
        input.plan.equity_funding[0] = 200.0;
        let (result, states) = calculate(&input)?;
        assert_eq!(result["expected_exit_equity_eur"], 8250.0);
        assert_eq!(states[1].funding_eur, 200.0);
        input.plan.cash_opex[0] = 1000.0;
        input.scenarios[0].failure_sale_probability = 0.2;
        input.scenarios[0].failure_equity_price = 50.0;
        let (result, states) = calculate(&input)?;
        assert_eq!(states[1].cash_failure_month, Some(1));
        assert_eq!(result["expected_exit_equity_eur"], 10.0);
        Ok(())
    }
    #[test]
    fn missing_sources_plans_and_unsupported_adapters_block() -> anyhow::Result<()> {
        let mut input = fixture();
        input.plan.source_ids = vec![];
        input.rights_confirmed = false;
        input.plan.confirmed = false;
        let missing = validate(&input, "2026-10-08")?;
        assert!(missing.contains(&"source:plan".into()));
        assert!(missing.contains(&"transferable_rights".into()));
        assert!(missing.contains(&"confirmed_resource_plan".into()));
        input.adapter = "game".into();
        assert!(validate(&input, "2026-10-08")?.contains(&"supported_adapter".into()));
        input = fixture();
        input.plan.mode = "funding_dependent_plan".into();
        assert!(validate(&input, "2026-10-08")?.contains(&"supported_committed_plan".into()));
        input = fixture();
        input.sources[0].valid_until = "2026-10-07".into();
        input.sources[0].observed_at = "2026-10-01".into();
        assert!(validate(&input, "2026-10-08")?.contains(&"current_source:fixture".into()));
        input = fixture();
        input.plan.cash_opex.pop();
        assert!(validate(&input, "2026-10-08").is_err());
        Ok(())
    }
    #[test]
    fn cash_and_service_limits_and_invalid_ebitda() -> anyhow::Result<()> {
        let mut input = fixture();
        input.plan.service_capacity = vec![1.0; 60];
        let (_, states) = calculate(&input)?;
        assert_eq!(states[1].ev_eur, 240.0);
        input.scenarios[0].debt_like_exit = 1e9;
        assert_eq!(calculate(&input)?.0["expected_exit_equity_eur"], 0.0);
        input = fixture();
        input.plan.cash_opex = vec![100.0; 60];
        input.scenarios[0].multiple_basis = "EBITDA".into();
        assert!(calculate(&input).is_err());
        Ok(())
    }
    #[test]
    fn exit_diagnostics_refs_and_rounding_are_bounded() -> anyhow::Result<()> {
        let valid = outcome("sold", 1.0, 1.0, 100.0);
        for field in 0..3 {
            for invalid in [-1.0, f64::NAN, f64::INFINITY] {
                let mut state = valid.clone();
                match field {
                    0 => state.ev_eur = invalid,
                    1 => state.excess_cash_eur = invalid,
                    _ => state.funding_eur = invalid,
                }
                assert!(aggregate(&[state]).is_err());
            }
        }
        for month in [0, 61] {
            let mut state = valid.clone();
            state.cash_failure_month = Some(month);
            assert!(aggregate(&[state]).is_err());
        }
        for refs in [vec!["x".repeat(129)], vec!["fixture".into(); 201]] {
            let mut input = fixture();
            input.plan.source_ids = refs.clone();
            assert!(validate(&input, "2026-10-08").is_err());
            input = fixture();
            input.perimeter_source_ids = refs.clone();
            assert!(validate(&input, "2026-10-08").is_err());
            input = fixture();
            input.probability_source_ids = refs.clone();
            assert!(validate(&input, "2026-10-08").is_err());
            input = fixture();
            input.scenarios[0].source_ids = refs.clone();
            assert!(validate(&input, "2026-10-08").is_err());
            let mut state = valid.clone();
            state.source_ids = refs;
            assert!(aggregate(&[state]).is_err());
        }
        let mut input = fixture();
        input.adapter = "terminal_equity_grid_v1".into();
        input.scenarios.clear();
        input.build_probability = 1.0;
        input.technical_failure_sale_probability = 0.0;
        input.technical_failure_equity_price = 0.0;
        input.outcomes = vec![
            outcome("one", 0.5, 1.0, 100.0),
            outcome("two", 0.50000000002, 1.0, 200.0),
        ];
        let (result, states) = calculate(&input)?;
        assert!(result["sale_probability"].as_f64().unwrap() <= 1.0);
        assert!(result["probability_zero_proceeds"].as_f64().unwrap() <= 1.0);
        assert!((states.iter().map(|s| s.probability).sum::<f64>() - 1.0).abs() < 1e-15);
        let contributions: f64 = states
            .iter()
            .map(|s| s.probability * s.sale_probability * s.equity_price_eur)
            .sum();
        assert!(
            (result["expected_exit_equity_eur"].as_f64().unwrap() - contributions).abs() < 1e-12
        );
        for state in &mut input.outcomes {
            state.equity_price_eur = 0.0;
        }
        assert_eq!(calculate(&input)?.0["probability_zero_proceeds"], 1.0);
        Ok(())
    }
    fn outcome(name: &str, p: f64, q: f64, price: f64) -> Outcome {
        Outcome {
            state: name.into(),
            probability: p,
            sale_probability: q,
            equity_price_eur: price,
            source_ids: vec![],
            ev_eur: 0.0,
            excess_cash_eur: 0.0,
            funding_eur: 0.0,
            cash_failure_month: None,
        }
    }
    #[test]
    fn teaching_equity_grid_includes_unsold_mass() -> anyhow::Result<()> {
        let r = aggregate(&[
            outcome("failure", 0.25, 0.2, 20000.0),
            outcome("small", 0.25, 0.5, 700000.0),
            outcome("solid", 0.35, 0.75, 4000000.0),
            outcome("strong", 0.15, 0.9, 15000000.0),
        ])?;
        assert!((r["expected_exit_equity_eur"].as_f64().unwrap() - 3163500.0).abs() < 1e-8);
        assert!((r["sale_probability"].as_f64().unwrap() - 0.5725).abs() < 1e-12);
        assert!((r["probability_zero_proceeds"].as_f64().unwrap() - 0.4275).abs() < 1e-12);
        assert_eq!(r["p10_eur"], 0.0);
        assert_eq!(r["p50_eur"], 700000.0);
        assert_eq!(r["p90_eur"], 15000000.0);
        Ok(())
    }
    #[test]
    fn no_sale_and_zero_price_deal_are_distinct() -> anyhow::Result<()> {
        let r = aggregate(&[outcome("no sale", 1.0, 0.0, 500.0)])?;
        assert_eq!(r["expected_exit_equity_eur"], 0.0);
        assert!(r["expected_price_given_sale_eur"].is_null());
        let r = aggregate(&[outcome("zero deal", 1.0, 1.0, 0.0)])?;
        assert_eq!(r["sale_probability"], 1.0);
        assert_eq!(r["probability_zero_proceeds"], 1.0);
        Ok(())
    }
    #[test]
    fn rejects_invalid_probability_and_nonfinite_money() {
        for o in [
            outcome("x", 1.0, 1.1, 5.0),
            outcome("x", 0.8, 0.4, 5.0),
            outcome("x", 1.0, 0.4, f64::NAN),
            outcome("x", 1.0, -0.1, 5.0),
        ] {
            assert!(aggregate(&[o]).is_err());
        }
    }
    #[test]
    fn calendar_horizon_clamps_month_end() -> anyhow::Result<()> {
        assert_eq!(add_months("2024-02-29", 60)?, "2029-02-28");
        assert_eq!(add_months("2026-01-31", 1)?, "2026-02-28");
        assert!(date("2026-2-1").is_err());
        Ok(())
    }
}
