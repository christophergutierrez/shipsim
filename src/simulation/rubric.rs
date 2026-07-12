use serde::{Deserialize, Serialize};

use super::metrics::AggregateMetrics;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RubricSpec {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub min_termination_rate: Option<f64>,
    #[serde(default)]
    pub max_stalemate_rate: Option<f64>,
    #[serde(default)]
    pub min_win_rate: Option<f64>,
    #[serde(default)]
    pub max_win_rate: Option<f64>,
    #[serde(default)]
    pub min_average_turns: Option<f64>,
    #[serde(default)]
    pub max_average_turns: Option<f64>,
    #[serde(default)]
    pub max_rejected_orders: Option<u64>,
    #[serde(default)]
    pub min_average_damage: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RubricCheck {
    pub metric: String,
    pub actual: f64,
    pub expectation: String,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RubricResult {
    pub id: String,
    pub description: String,
    pub passed: bool,
    pub checks: Vec<RubricCheck>,
}

pub fn evaluate_rubric(spec: &RubricSpec, metrics: &AggregateMetrics) -> RubricResult {
    let mut checks = Vec::new();
    if let Some(expected) = spec.min_termination_rate {
        checks.push(minimum(
            "termination_rate",
            metrics.termination_rate,
            expected,
        ));
    }
    if let Some(expected) = spec.max_stalemate_rate {
        let actual = if metrics.matches == 0 {
            0.0
        } else {
            metrics.stalemates as f64 / metrics.matches as f64
        };
        checks.push(maximum("stalemate_rate", actual, expected));
    }
    if let Some(expected) = spec.min_win_rate {
        checks.push(minimum("win_rate", metrics.win_rate, expected));
    }
    if let Some(expected) = spec.max_win_rate {
        checks.push(maximum("win_rate", metrics.win_rate, expected));
    }
    if let Some(expected) = spec.min_average_turns {
        checks.push(minimum("average_turns", metrics.average_turns, expected));
    }
    if let Some(expected) = spec.max_average_turns {
        checks.push(maximum("average_turns", metrics.average_turns, expected));
    }
    if let Some(expected) = spec.max_rejected_orders {
        checks.push(maximum(
            "rejected_orders",
            metrics.rejected_orders as f64,
            expected as f64,
        ));
    }
    if let Some(expected) = spec.min_average_damage {
        checks.push(minimum("average_damage", metrics.average_damage, expected));
    }
    RubricResult {
        id: spec.id.clone(),
        description: spec.description.clone(),
        passed: checks.iter().all(|check| check.passed),
        checks,
    }
}

fn minimum(metric: &str, actual: f64, expected: f64) -> RubricCheck {
    RubricCheck {
        metric: metric.to_string(),
        actual,
        expectation: format!(">= {expected}"),
        passed: actual >= expected,
    }
}

fn maximum(metric: &str, actual: f64, expected: f64) -> RubricCheck {
    RubricCheck {
        metric: metric.to_string(),
        actual,
        expectation: format!("<= {expected}"),
        passed: actual <= expected,
    }
}
