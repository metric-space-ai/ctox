// Origin: CTOX
// License: AGPL-3.0-only
//! Essential KPI: expected sale price of a project's business at month 60.
//!
//! Pure deterministic core. No I/O, no clock, no randomness. The Supervisor
//! supplies a versioned parameter register; this module only computes. Missing
//! required inputs block the result instead of being estimated.

const HORIZON_MONTHS: u32 = 60;
const TOLERANCE: f64 = 1e-9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamStatus {
    Observed,
    Derived,
    Estimated,
    Assumed,
    Unknown,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Param {
    pub value: f64,
    pub status: ParamStatus,
}

impl Param {
    pub fn is_unknown(&self) -> bool {
        self.status == ParamStatus::Unknown
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scenario {
    /// Probability that the operating state `s` is reached.
    pub probability: f64,
    /// Probability that a sale succeeds within state `s`.
    pub sale_probability: f64,
    pub new_customers_per_month: f64,
    pub monthly_churn: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Inputs {
    pub customers_now: Param,
    pub arpa_monthly: Param,
    pub multiple_arr: Param,
    /// Excess cash minus debt at the horizon. Negative values are allowed.
    pub net_cash_at_horizon: Param,
    pub scenarios: Vec<Scenario>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Ready,
    /// At least one required input is only assumed.
    Provisional,
    /// A required input is unknown. The value is withheld, not estimated.
    Blocked { missing: &'static str },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Result {
    pub status: Status,
    /// Probability-weighted equity value at the horizon, EUR nominal.
    pub e5: Option<f64>,
    /// Sum of `p * q`. Zero means no sale in any state.
    pub p_sale: Option<f64>,
    /// Expected equity price conditional on a sale. `None` when no sale is possible.
    pub price_if_sold: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputError {
    NonFinite(&'static str),
    OutOfRange(&'static str),
    ProbabilitiesNotNormalized,
    NoScenarios,
}

/// Validates the register before any arithmetic. Unknown values are skipped
/// because they carry no number; everything else must be a usable value.
pub fn validate(inputs: &Inputs) -> std::result::Result<(), InputError> {
    for (name, param) in [
        ("customers_now", inputs.customers_now),
        ("arpa_monthly", inputs.arpa_monthly),
        ("multiple_arr", inputs.multiple_arr),
        ("net_cash_at_horizon", inputs.net_cash_at_horizon),
    ] {
        if param.is_unknown() {
            continue;
        }
        if !param.value.is_finite() {
            return Err(InputError::NonFinite(name));
        }
    }
    let known_non_negative = [
        ("customers_now", inputs.customers_now),
        ("arpa_monthly", inputs.arpa_monthly),
        ("multiple_arr", inputs.multiple_arr),
    ];
    for (name, param) in known_non_negative {
        if !param.is_unknown() && param.value < 0.0 {
            return Err(InputError::OutOfRange(name));
        }
    }
    if inputs.scenarios.is_empty() {
        return Err(InputError::NoScenarios);
    }
    let mut total = 0.0;
    for scenario in &inputs.scenarios {
        let values = [
            scenario.probability,
            scenario.sale_probability,
            scenario.new_customers_per_month,
            scenario.monthly_churn,
        ];
        if values.iter().any(|v| !v.is_finite()) {
            return Err(InputError::NonFinite("scenario"));
        }
        if !(0.0..=1.0).contains(&scenario.probability)
            || !(0.0..=1.0).contains(&scenario.sale_probability)
            || !(0.0..=1.0).contains(&scenario.monthly_churn)
        {
            return Err(InputError::OutOfRange("scenario"));
        }
        if scenario.new_customers_per_month < 0.0 {
            return Err(InputError::OutOfRange("new_customers_per_month"));
        }
        total += scenario.probability;
    }
    if (total - 1.0).abs() > TOLERANCE {
        return Err(InputError::ProbabilitiesNotNormalized);
    }
    Ok(())
}

/// Customers after 60 monthly steps: `k' = k * (1 - churn) + new`.
fn customers_at_horizon(start: f64, scenario: &Scenario) -> f64 {
    (0..HORIZON_MONTHS).fold(start, |customers, _| {
        customers * (1.0 - scenario.monthly_churn) + scenario.new_customers_per_month
    })
}

pub fn compute(inputs: &Inputs) -> std::result::Result<Result, InputError> {
    validate(inputs)?;

    let required = [
        ("customers_now", inputs.customers_now),
        ("arpa_monthly", inputs.arpa_monthly),
        ("multiple_arr", inputs.multiple_arr),
        ("net_cash_at_horizon", inputs.net_cash_at_horizon),
    ];
    if let Some((missing, _)) = required.iter().find(|(_, p)| p.is_unknown()) {
        return Ok(Result {
            status: Status::Blocked { missing },
            e5: None,
            p_sale: None,
            price_if_sold: None,
        });
    }

    let provisional = required
        .iter()
        .any(|(_, p)| p.status == ParamStatus::Assumed);
    let [customers_now, arpa, multiple, net_cash] = required.map(|(_, p)| p.value);

    let mut e5 = 0.0;
    let mut p_sale = 0.0;
    for scenario in &inputs.scenarios {
        let customers = customers_at_horizon(customers_now, scenario);
        let arr = customers * arpa * 12.0;
        let enterprise_value = multiple * arr;
        let equity = (enterprise_value + net_cash).max(0.0);
        let weight = scenario.probability * scenario.sale_probability;
        e5 += weight * equity;
        p_sale += weight;
    }

    let price_if_sold = (p_sale > TOLERANCE).then(|| e5 / p_sale);
    Ok(Result {
        status: if provisional {
            Status::Provisional
        } else {
            Status::Ready
        },
        e5: Some(e5),
        p_sale: Some(p_sale),
        price_if_sold,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(value: f64, status: ParamStatus) -> Param {
        Param { value, status }
    }

    fn base(sale_probability: f64) -> Inputs {
        Inputs {
            customers_now: known(10.0, ParamStatus::Observed),
            arpa_monthly: known(100.0, ParamStatus::Estimated),
            multiple_arr: known(3.0, ParamStatus::Estimated),
            net_cash_at_horizon: known(0.0, ParamStatus::Assumed),
            scenarios: vec![Scenario {
                probability: 1.0,
                sale_probability,
                new_customers_per_month: 2.0,
                monthly_churn: 0.0,
            }],
        }
    }

    #[test]
    fn hand_calculation_matches() {
        // 10 + 60 * 2 = 130 customers; ARR = 130 * 100 * 12 = 156_000;
        // EV = 3 * 156_000 = 468_000; E5 = 0.5 * 468_000 = 234_000.
        let mut inputs = base(0.5);
        inputs.net_cash_at_horizon = known(0.0, ParamStatus::Derived);
        let result = compute(&inputs).unwrap();
        assert_eq!(result.status, Status::Ready);
        assert!((result.e5.unwrap() - 234_000.0).abs() < 1e-6);
        assert!((result.p_sale.unwrap() - 0.5).abs() < 1e-12);
        assert!((result.price_if_sold.unwrap() - 468_000.0).abs() < 1e-6);
    }

    #[test]
    fn no_sale_gives_zero_and_no_conditional_price() {
        let result = compute(&base(0.0)).unwrap();
        assert_eq!(result.e5, Some(0.0));
        assert_eq!(result.p_sale, Some(0.0));
        assert_eq!(result.price_if_sold, None);
    }

    #[test]
    fn observed_zero_revenue_is_a_value_not_unknown() {
        let mut inputs = base(1.0);
        inputs.customers_now = known(0.0, ParamStatus::Observed);
        inputs.arpa_monthly = known(100.0, ParamStatus::Derived);
        inputs.multiple_arr = known(3.0, ParamStatus::Derived);
        inputs.net_cash_at_horizon = known(0.0, ParamStatus::Derived);
        inputs.scenarios[0].new_customers_per_month = 0.0;
        let result = compute(&inputs).unwrap();
        assert_eq!(result.status, Status::Ready);
        assert_eq!(result.e5, Some(0.0));
    }

    #[test]
    fn unknown_required_input_blocks_without_a_number() {
        let mut inputs = base(1.0);
        inputs.multiple_arr = known(0.0, ParamStatus::Unknown);
        let result = compute(&inputs).unwrap();
        assert_eq!(
            result.status,
            Status::Blocked {
                missing: "multiple_arr"
            }
        );
        assert_eq!(result.e5, None);
        assert_eq!(result.p_sale, None);
    }

    #[test]
    fn assumed_required_input_is_provisional() {
        let mut inputs = base(1.0);
        inputs.arpa_monthly = known(100.0, ParamStatus::Assumed);
        let result = compute(&inputs).unwrap();
        assert_eq!(result.status, Status::Provisional);
        assert!(result.e5.is_some());
    }

    #[test]
    fn negative_equity_is_clamped_to_zero() {
        let mut inputs = base(1.0);
        inputs.net_cash_at_horizon = known(-1_000_000.0, ParamStatus::Derived);
        let result = compute(&inputs).unwrap();
        assert_eq!(result.e5, Some(0.0));
    }

    #[test]
    fn churn_reduces_customers_over_the_horizon() {
        let scenario = Scenario {
            probability: 1.0,
            sale_probability: 1.0,
            new_customers_per_month: 0.0,
            monthly_churn: 0.5,
        };
        let customers = customers_at_horizon(8.0, &scenario);
        assert!((customers - 8.0 * 0.5f64.powi(60)).abs() < 1e-30);
    }

    #[test]
    fn unnormalized_probabilities_are_rejected() {
        let mut inputs = base(1.0);
        inputs.scenarios[0].probability = 0.9;
        assert_eq!(
            compute(&inputs),
            Err(InputError::ProbabilitiesNotNormalized)
        );
    }

    #[test]
    fn out_of_range_and_non_finite_inputs_are_rejected() {
        let mut inputs = base(1.5);
        assert_eq!(compute(&inputs), Err(InputError::OutOfRange("scenario")));
        inputs = base(1.0);
        inputs.arpa_monthly = known(f64::NAN, ParamStatus::Estimated);
        assert_eq!(compute(&inputs), Err(InputError::NonFinite("arpa_monthly")));
        inputs = base(1.0);
        inputs.scenarios.clear();
        assert_eq!(compute(&inputs), Err(InputError::NoScenarios));
    }

    #[test]
    fn same_inputs_give_same_result() {
        let inputs = base(0.3);
        assert_eq!(compute(&inputs), compute(&inputs));
    }
}
