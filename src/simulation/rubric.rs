use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::metrics::AggregateMetrics;
use crate::game_state::ScenarioStatus;

/// Win-rate summary for a single (player_policy, opponent_policy) matchup.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MatchupMetrics {
    pub player_policy: String,
    pub opponent_policy: String,
    pub matches: u64,
    pub wins: u64,
    pub win_rate: f64,
}

/// Per-matchup win-rate data derived from the match results.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MatchupBreakdown {
    pub matchups: Vec<MatchupMetrics>,
    /// Largest policy-dominance gap between the two directions of a paired
    /// matchup. For a pair (A as player vs B) and (B as player vs A),
    /// dominance = |win_rate_AB - win_rate_BA|. If A always beats B the rates
    /// are 1.0 and 0.0, so dominance is 1.0. A value near 0 means neither
    /// policy dominates the other.
    pub max_dominance: f64,
    /// Largest side-bias gap between the two directions of a paired matchup.
    /// For a pair (A as player vs B) and (B as player vs A),
    /// side_bias = |win_rate_AB - (1 - win_rate_BA)|. This is high when
    /// player/opponent side assignment changes the outcome rather than policy
    /// quality. Distinct from `max_dominance`, which measures policy strength.
    pub max_side_bias: f64,
}

impl MatchupBreakdown {
    /// Compute per-matchup win rates and the maximum paired dominance from
    /// raw match results. Each result contributes (player_policy, opponent_policy, status).
    pub fn from_results(
        results: impl IntoIterator<Item = (String, String, ScenarioStatus)>,
    ) -> Self {
        let mut counts: BTreeMap<(String, String), (u64, u64)> = BTreeMap::new();
        for (player_policy, opponent_policy, status) in results {
            let entry = counts
                .entry((player_policy, opponent_policy))
                .or_insert((0, 0));
            entry.0 += 1;
            if matches!(status, ScenarioStatus::Won) {
                entry.1 += 1;
            }
        }
        let mut matchups = Vec::new();
        for ((player_policy, opponent_policy), (matches, wins)) in &counts {
            let win_rate = if *matches > 0 {
                *wins as f64 / *matches as f64
            } else {
                0.0
            };
            matchups.push(MatchupMetrics {
                player_policy: player_policy.clone(),
                opponent_policy: opponent_policy.clone(),
                matches: *matches,
                wins: *wins,
                win_rate,
            });
        }
        matchups.sort_by(|a, b| {
            a.player_policy
                .cmp(&b.player_policy)
                .then_with(|| a.opponent_policy.cmp(&b.opponent_policy))
        });

        // Compute max dominance and max side bias across paired directions.
        // For a pair (A as player vs B) and (B as player vs A):
        //   wr_AB = A's win rate as player vs B
        //   wr_BA = B's win rate as player vs A
        // Dominance measures policy strength: |wr_AB - wr_BA|. If A always
        // beats B, wr_AB = 1.0 and wr_BA = 0.0, so dominance = 1.0.
        // Side bias measures whether side assignment determines the outcome:
        //   |wr_AB - (1 - wr_BA)|. This is 0 when the same policy wins
        //   regardless of side, and high when the player side always wins.
        let mut max_dominance = 0.0f64;
        let mut max_side_bias = 0.0f64;
        for m in &matchups {
            let mirror = matchups.iter().find(|other| {
                other.player_policy == m.opponent_policy && other.opponent_policy == m.player_policy
            });
            if let Some(mirror) = mirror {
                let dominance = (m.win_rate - mirror.win_rate).abs();
                if dominance > max_dominance {
                    max_dominance = dominance;
                }
                let side_bias = (m.win_rate - (1.0 - mirror.win_rate)).abs();
                if side_bias > max_side_bias {
                    max_side_bias = side_bias;
                }
            }
        }

        MatchupBreakdown {
            matchups,
            max_dominance,
            max_side_bias,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EngagementMetrics {
    pub engagement: String,
    pub matches: u64,
    pub wins: u64,
    pub win_rate: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EngagementBreakdown {
    pub engagements: Vec<EngagementMetrics>,
}

impl EngagementBreakdown {
    pub fn from_results(
        results: impl IntoIterator<Item = (Option<String>, ScenarioStatus)>,
    ) -> Self {
        let mut counts: BTreeMap<String, (u64, u64)> = BTreeMap::new();
        for (engagement, status) in results {
            let Some(engagement) = engagement else {
                continue;
            };
            let entry = counts.entry(engagement).or_default();
            entry.0 += 1;
            if matches!(status, ScenarioStatus::Won) {
                entry.1 += 1;
            }
        }
        Self {
            engagements: counts
                .into_iter()
                .map(|(engagement, (matches, wins))| EngagementMetrics {
                    engagement,
                    matches,
                    wins,
                    win_rate: if matches == 0 {
                        0.0
                    } else {
                        wins as f64 / matches as f64
                    },
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngagementWinRateSpec {
    pub engagement: String,
    #[serde(default)]
    pub min_win_rate: Option<f64>,
    #[serde(default)]
    pub max_win_rate: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
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
    /// No single matchup's player-side win rate may fall below this.
    #[serde(default)]
    pub min_win_rate_per_matchup: Option<f64>,
    /// No single matchup's player-side win rate may exceed this.
    #[serde(default)]
    pub max_win_rate_per_matchup: Option<f64>,
    /// Maximum allowed policy-dominance gap between the two directions of a
    /// paired matchup. Detects one policy consistently beating another in a
    /// way that aggregate win rate hides.
    #[serde(default)]
    pub max_matchup_dominance: Option<f64>,
    /// Maximum allowed side-bias gap between the two directions of a paired
    /// matchup. Detects player/opponent side assignment determining the
    /// outcome rather than policy quality.
    #[serde(default)]
    pub max_matchup_side_bias: Option<f64>,
    /// Advisory rubrics are reported but do not fail the simulation CLI.
    #[serde(default)]
    pub advisory: bool,
    #[serde(default)]
    pub max_blocked_translation_rate: Option<f64>,
    #[serde(default)]
    pub max_zero_translation_rate: Option<f64>,
    #[serde(default)]
    pub min_coasting_distance: Option<f64>,
    #[serde(default)]
    pub hull_class: Option<String>,
    #[serde(default)]
    pub max_hull_zero_translation_rate: Option<f64>,
    #[serde(default)]
    pub max_hull_zero_velocity_rate: Option<f64>,
    #[serde(default)]
    pub max_hull_broad_resource_rate: Option<f64>,
    /// Every hull class with allocation observations must keep power utilization
    /// (spent / available) at or above this. Catches power-dead reactors (e.g.
    /// titan with power_sys zeroed by DAC).
    #[serde(default)]
    pub min_class_power_utilization: Option<f64>,
    /// Win-rate bounds for named fleet engagements.
    #[serde(default)]
    pub engagement_win_rates: Vec<EngagementWinRateSpec>,
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
    pub blocking: bool,
    pub checks: Vec<RubricCheck>,
}

pub fn evaluate_rubric(
    spec: &RubricSpec,
    metrics: &AggregateMetrics,
    breakdown: &MatchupBreakdown,
    engagement_breakdown: &EngagementBreakdown,
) -> RubricResult {
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
    // Per-matchup bounds: no single matchup may fall outside the band.
    if let (Some(min), Some(max)) = (spec.min_win_rate_per_matchup, spec.max_win_rate_per_matchup) {
        for m in &breakdown.matchups {
            checks.push(minimum(
                &format!("win_rate[{}/{}]", m.player_policy, m.opponent_policy),
                m.win_rate,
                min,
            ));
            checks.push(maximum(
                &format!("win_rate[{}/{}]", m.player_policy, m.opponent_policy),
                m.win_rate,
                max,
            ));
        }
    }
    // Maximum dominance: no paired matchup may have a policy-strength gap above this.
    if let Some(expected) = spec.max_matchup_dominance {
        checks.push(maximum(
            "max_matchup_dominance",
            breakdown.max_dominance,
            expected,
        ));
    }
    // Maximum side bias: no paired matchup may have a side-assignment gap above this.
    if let Some(expected) = spec.max_matchup_side_bias {
        checks.push(maximum(
            "max_matchup_side_bias",
            breakdown.max_side_bias,
            expected,
        ));
    }
    if let Some(expected) = spec.max_blocked_translation_rate {
        // Protocol v4: blocked path rate (legacy rubric field name retained).
        let actual = rate(metrics.blocked_paths, metrics.scheduled_paths);
        checks.push(maximum("blocked_translation_rate", actual, expected));
    }
    if let Some(expected) = spec.max_zero_translation_rate {
        let actual = rate(
            metrics.zero_translation_observations,
            metrics.scheduled_paths,
        );
        checks.push(maximum("zero_translation_rate", actual, expected));
    }
    if let Some(expected) = spec.min_coasting_distance {
        // Protocol v4: total hexes translated along submitted paths.
        checks.push(minimum(
            "path_translated_steps",
            metrics.path_translated_steps as f64,
            expected,
        ));
    }
    if let Some(class) = &spec.hull_class {
        let values = metrics.hull_efficiency.get(class);
        if values.is_none()
            && (spec.max_hull_zero_translation_rate.is_some()
                || spec.max_hull_broad_resource_rate.is_some()
                || spec.max_hull_zero_velocity_rate.is_some())
        {
            checks.push(RubricCheck {
                metric: format!("hull_class_present[{class}]"),
                actual: 0.0,
                expectation: "class has observations".into(),
                passed: false,
            });
        }
        if let Some(expected) = spec.max_hull_zero_translation_rate {
            let actual = values
                .map(|v| rate(v.zero_translation_observations, v.scheduled_paths))
                .unwrap_or(0.0);
            checks.push(maximum(
                &format!("hull_zero_translation_rate[{class}]"),
                actual,
                expected,
            ));
        }
        if let Some(expected) = spec.max_hull_broad_resource_rate {
            let actual = values
                .map(|v| rate(v.broad_resource_allocations, v.allocation_observations))
                .unwrap_or(0.0);
            checks.push(maximum(
                &format!("hull_broad_resource_rate[{class}]"),
                actual,
                expected,
            ));
        }
        if let Some(expected) = spec.max_hull_zero_velocity_rate {
            // Legacy name: zero-motion path rate (path cost 0 with motion available).
            let actual = values
                .map(|v| rate(v.zero_motion_path_observations, v.path_observations))
                .unwrap_or(0.0);
            checks.push(maximum(
                &format!("hull_zero_motion_path_rate[{class}]"),
                actual,
                expected,
            ));
        }
    }
    if let Some(expected) = spec.min_class_power_utilization {
        for (class, values) in &metrics.hull_efficiency {
            if values.allocation_observations == 0 {
                continue;
            }
            checks.push(minimum(
                &format!("power_utilization[{class}]"),
                values.power_utilization(),
                expected,
            ));
        }
    }
    for expected in &spec.engagement_win_rates {
        let observed = engagement_breakdown
            .engagements
            .iter()
            .find(|metrics| metrics.engagement == expected.engagement);
        let Some(observed) = observed else {
            checks.push(RubricCheck {
                metric: format!("engagement_present[{}]", expected.engagement),
                actual: 0.0,
                expectation: "engagement has observations".into(),
                passed: false,
            });
            continue;
        };
        if let Some(min) = expected.min_win_rate {
            checks.push(minimum(
                &format!("win_rate[{}]", expected.engagement),
                observed.win_rate,
                min,
            ));
        }
        if let Some(max) = expected.max_win_rate {
            checks.push(maximum(
                &format!("win_rate[{}]", expected.engagement),
                observed.win_rate,
                max,
            ));
        }
    }
    RubricResult {
        id: spec.id.clone(),
        description: spec.description.clone(),
        passed: checks.iter().all(|check| check.passed),
        blocking: !spec.advisory,
        checks,
    }
}

fn rate(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::ScenarioStatus;

    /// Build a breakdown from a list of (player, opponent, won?) tuples.
    fn breakdown(results: &[(&str, &str, bool)]) -> MatchupBreakdown {
        MatchupBreakdown::from_results(
            results
                .iter()
                .map(|(p, o, won)| {
                    (
                        (*p).to_string(),
                        (*o).to_string(),
                        if *won {
                            ScenarioStatus::Won
                        } else {
                            ScenarioStatus::Lost
                        },
                    )
                })
                .collect::<Vec<_>>(),
        )
    }

    /// Case: A dominates B in both directions.
    /// A as player beats B (wr_AB = 1.0); B as player loses to A (wr_BA = 0.0).
    /// Dominance should be 1.0; side bias should be 0.0.
    #[test]
    fn dominance_detects_policy_dominating_both_directions() {
        let b = breakdown(&[
            ("A", "B", true),
            ("A", "B", true),
            ("B", "A", false),
            ("B", "A", false),
        ]);
        assert!(
            (b.max_dominance - 1.0).abs() < 1e-9,
            "dominance {}",
            b.max_dominance
        );
        assert!(
            (b.max_side_bias - 0.0).abs() < 1e-9,
            "side_bias {}",
            b.max_side_bias
        );
    }

    /// Case: player side always wins regardless of policy.
    /// wr_AB = 1.0 and wr_BA = 1.0. Dominance should be 0.0; side bias 1.0.
    #[test]
    fn side_bias_detects_player_side_always_winning() {
        let b = breakdown(&[
            ("A", "B", true),
            ("A", "B", true),
            ("B", "A", true),
            ("B", "A", true),
        ]);
        assert!(
            (b.max_dominance - 0.0).abs() < 1e-9,
            "dominance {}",
            b.max_dominance
        );
        assert!(
            (b.max_side_bias - 1.0).abs() < 1e-9,
            "side_bias {}",
            b.max_side_bias
        );
    }

    /// Case: perfectly balanced policies. Each side wins half the time.
    /// Dominance and side bias should both be 0.0.
    #[test]
    fn balanced_policies_yield_zero_dominance_and_side_bias() {
        let b = breakdown(&[
            ("A", "B", true),
            ("A", "B", false),
            ("B", "A", true),
            ("B", "A", false),
        ]);
        assert!(
            (b.max_dominance - 0.0).abs() < 1e-9,
            "dominance {}",
            b.max_dominance
        );
        assert!(
            (b.max_side_bias - 0.0).abs() < 1e-9,
            "side_bias {}",
            b.max_side_bias
        );
    }

    /// Case: missing reverse matchup. Only (A vs B) exists, no (B vs A).
    /// With no pair, dominance and side bias stay at 0.0.
    #[test]
    fn missing_reverse_matchup_yields_zero_dominance() {
        let b = breakdown(&[("A", "B", true), ("A", "B", false)]);
        assert!((b.max_dominance - 0.0).abs() < 1e-9);
        assert!((b.max_side_bias - 0.0).abs() < 1e-9);
        assert_eq!(b.matchups.len(), 1);
    }

    /// Case: per-matchup threshold failure.
    /// A dominates B (wr_AB = 1.0) which exceeds max_win_rate_per_matchup = 0.75.
    #[test]
    fn per_matchup_threshold_fails_on_dominant_policy() {
        let b = breakdown(&[
            ("A", "B", true),
            ("A", "B", true),
            ("B", "A", false),
            ("B", "A", false),
        ]);
        let spec = RubricSpec {
            id: "test".to_string(),
            description: "test".to_string(),
            min_win_rate_per_matchup: Some(0.25),
            max_win_rate_per_matchup: Some(0.75),
            ..Default::default()
        };
        let metrics = AggregateMetrics::default();
        let result = evaluate_rubric(&spec, &metrics, &b, &EngagementBreakdown::default());
        assert!(!result.passed, "should fail: A/B win rate 1.0 exceeds 0.75");
        let ab_max = result
            .checks
            .iter()
            .find(|c| c.metric == "win_rate[A/B]" && c.expectation.starts_with("<="))
            .expect("A/B max check present");
        assert!(!ab_max.passed);
    }

    #[test]
    fn large_hull_zero_translation_negative_rubric_is_visible() {
        let metrics = AggregateMetrics {
            hull_efficiency: [(
                "Huge".into(),
                crate::simulation::metrics::HullEfficiencyMetrics {
                    scheduled_paths: 10,
                    zero_translation_observations: 8,
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        let result = evaluate_rubric(
            &RubricSpec {
                id: "large_motion".into(),
                description: "large hull must translate".into(),
                hull_class: Some("Huge".into()),
                max_hull_zero_translation_rate: Some(0.5),
                ..Default::default()
            },
            &metrics,
            &MatchupBreakdown::default(),
            &EngagementBreakdown::default(),
        );
        assert!(!result.passed);
        assert!(result.blocking);
        assert_eq!(result.checks[0].actual, 0.8);
    }

    #[test]
    fn tiny_broad_resource_negative_rubric_is_advisory_and_visible() {
        let mut metrics = AggregateMetrics::default();
        metrics.hull_efficiency.insert(
            "Escort".into(),
            crate::simulation::metrics::HullEfficiencyMetrics {
                allocation_observations: 4,
                broad_resource_allocations: 4,
                ..Default::default()
            },
        );
        let result = evaluate_rubric(
            &RubricSpec {
                id: "tiny_balance".into(),
                description: "tiny hull resource concentration".into(),
                advisory: true,
                hull_class: Some("Escort".into()),
                max_hull_broad_resource_rate: Some(0.5),
                ..Default::default()
            },
            &metrics,
            &MatchupBreakdown::default(),
            &EngagementBreakdown::default(),
        );
        assert!(!result.passed);
        assert!(!result.blocking);
    }

    /// Side-bias threshold fails when player side always wins.
    #[test]
    fn side_bias_threshold_fails_on_player_side_advantage() {
        let b = breakdown(&[
            ("A", "B", true),
            ("A", "B", true),
            ("B", "A", true),
            ("B", "A", true),
        ]);
        let spec = RubricSpec {
            id: "test".to_string(),
            description: "test".to_string(),
            max_matchup_side_bias: Some(0.50),
            ..Default::default()
        };
        let metrics = AggregateMetrics::default();
        let result = evaluate_rubric(&spec, &metrics, &b, &EngagementBreakdown::default());
        assert!(!result.passed, "should fail: side bias 1.0 exceeds 0.50");
    }

    /// Dominance threshold fails when one policy dominates.
    #[test]
    fn dominance_threshold_fails_on_policy_dominance() {
        let b = breakdown(&[
            ("A", "B", true),
            ("A", "B", true),
            ("B", "A", false),
            ("B", "A", false),
        ]);
        let spec = RubricSpec {
            id: "test".to_string(),
            description: "test".to_string(),
            max_matchup_dominance: Some(0.50),
            ..Default::default()
        };
        let metrics = AggregateMetrics::default();
        let result = evaluate_rubric(&spec, &metrics, &b, &EngagementBreakdown::default());
        assert!(!result.passed, "should fail: dominance 1.0 exceeds 0.50");
    }

    #[test]
    fn engagement_win_rate_bounds_are_enforced() {
        let engagements = EngagementBreakdown::from_results([
            (Some("claim_a".into()), ScenarioStatus::Won),
            (Some("claim_a".into()), ScenarioStatus::Lost),
            (Some("claim_b".into()), ScenarioStatus::Won),
        ]);
        let spec = RubricSpec {
            id: "claims".into(),
            description: "engagement bands".into(),
            engagement_win_rates: vec![
                EngagementWinRateSpec {
                    engagement: "claim_a".into(),
                    min_win_rate: Some(0.4),
                    max_win_rate: Some(0.6),
                },
                EngagementWinRateSpec {
                    engagement: "claim_b".into(),
                    min_win_rate: None,
                    max_win_rate: Some(0.9),
                },
            ],
            ..Default::default()
        };
        let result = evaluate_rubric(
            &spec,
            &AggregateMetrics::default(),
            &MatchupBreakdown::default(),
            &engagements,
        );
        assert!(!result.passed);
        assert!(result
            .checks
            .iter()
            .any(|check| { check.metric == "win_rate[claim_b]" && !check.passed }));
    }

    #[test]
    fn missing_engagement_fails_named_band() {
        let spec = RubricSpec {
            id: "claims".into(),
            description: "engagement bands".into(),
            engagement_win_rates: vec![EngagementWinRateSpec {
                engagement: "missing".into(),
                min_win_rate: Some(0.4),
                max_win_rate: Some(0.6),
            }],
            ..Default::default()
        };
        let result = evaluate_rubric(
            &spec,
            &AggregateMetrics::default(),
            &MatchupBreakdown::default(),
            &EngagementBreakdown::default(),
        );
        assert!(!result.passed);
        assert_eq!(result.checks[0].metric, "engagement_present[missing]");
    }
}
