//! Cost-matched fleet engagements for simulation suites.
//!
//! Builds in-memory scenarios from fleet composition lines and ship `cost`
//! fields so equal-budget matchups can be re-run without one-off scenario files.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::schema::{ScenarioDef, ShipPlacementDef, TerminalDef};
use crate::scenario::{load_ship_def, LoadError};

fn default_count() -> u32 {
    1
}

fn default_width() -> u32 {
    32
}

fn default_height() -> u32 {
    24
}

/// One ship class and how many copies to field.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetLine {
    pub class: String,
    #[serde(default = "default_count")]
    pub count: u32,
    /// Optional design-power override for balance sweeps.
    #[serde(default)]
    pub power: Option<u32>,
    #[serde(default)]
    pub structure: Option<u32>,
    #[serde(default)]
    pub max_shield_per_facing: Option<u32>,
}

impl FleetLine {
    pub fn new(class: impl Into<String>, count: u32) -> Self {
        Self {
            class: class.into(),
            count,
            power: None,
            structure: None,
            max_shield_per_facing: None,
        }
    }
}

/// Named fleet-vs-fleet pairing under a shared budget.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngagementSpec {
    pub name: String,
    pub player: Vec<FleetLine>,
    pub opponent: Vec<FleetLine>,
}

/// Expand one fleet composition across a range of power values on a class.
///
/// Use to find the effective power band: too little → dies (or never works);
/// too much → surplus idle energy. Diagnose *how* it loses with turn count:
/// quick loss → defenses weak; long loss → offense weak.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerSweepSpec {
    /// Prefix for generated engagement names (`{name}_p{value}`).
    pub name: String,
    /// Ship class file id whose `power` is varied (e.g. `titan_line`).
    pub class: String,
    /// `player` or `opponent` fleet side containing `class`.
    pub side: String,
    /// Power values to try.
    pub values: Vec<u32>,
    pub player: Vec<FleetLine>,
    pub opponent: Vec<FleetLine>,
}

impl PowerSweepSpec {
    pub fn expand(&self) -> Result<Vec<EngagementSpec>, FleetError> {
        let side = self.side.to_ascii_lowercase();
        if side != "player" && side != "opponent" {
            return Err(FleetError::BadSweepSide {
                sweep: self.name.clone(),
                side: self.side.clone(),
            });
        }
        if self.values.is_empty() {
            return Err(FleetError::EmptySweep {
                sweep: self.name.clone(),
            });
        }
        let mut out = Vec::with_capacity(self.values.len());
        for &power in &self.values {
            let mut player = self.player.clone();
            let mut opponent = self.opponent.clone();
            let target = if side == "player" {
                &mut player
            } else {
                &mut opponent
            };
            let mut hit = false;
            for line in target.iter_mut() {
                if line.class == self.class {
                    line.power = Some(power);
                    hit = true;
                }
            }
            if !hit {
                return Err(FleetError::SweepClassMissing {
                    sweep: self.name.clone(),
                    class: self.class.clone(),
                    side: self.side.clone(),
                });
            }
            out.push(EngagementSpec {
                name: format!("{}_p{}", self.name, power),
                player,
                opponent,
            });
        }
        Ok(out)
    }
}

/// Map dimensions for generated fleet scenarios.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetMapSpec {
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default)]
    pub map_mode: Option<String>,
}

impl Default for FleetMapSpec {
    fn default() -> Self {
        Self {
            width: default_width(),
            height: default_height(),
            map_mode: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FleetCosts {
    pub player: u32,
    pub opponent: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum FleetError {
    #[error(transparent)]
    Load(#[from] LoadError),
    #[error("engagement {engagement:?}: fleet line count must be >= 1 (class {class:?})")]
    ZeroCount { engagement: String, class: String },
    #[error(
        "engagement {engagement:?}: player cost {player_cost} and opponent cost {opponent_cost} \
         differ by more than tolerance {tolerance}"
    )]
    CostImbalance {
        engagement: String,
        player_cost: u32,
        opponent_cost: u32,
        tolerance: u32,
    },
    #[error(
        "engagement {engagement:?}: side cost {side_cost} is outside budget {budget} ± {tolerance}"
    )]
    OffBudget {
        engagement: String,
        side_cost: u32,
        budget: u32,
        tolerance: u32,
    },
    #[error(
        "engagement {engagement:?}: need {needed} hexes for placement but map is {width}x{height}"
    )]
    MapTooSmall {
        engagement: String,
        needed: u32,
        width: u32,
        height: u32,
    },
    #[error("power sweep {sweep:?}: side must be \"player\" or \"opponent\" (got {side:?})")]
    BadSweepSide { sweep: String, side: String },
    #[error("power sweep {sweep:?}: values list is empty")]
    EmptySweep { sweep: String },
    #[error("power sweep {sweep:?}: class {class:?} not present on {side} fleet")]
    SweepClassMissing {
        sweep: String,
        class: String,
        side: String,
    },
}

/// Sum `cost × count` for a fleet using ship class TOMLs under `data_root`.
pub fn fleet_cost(data_root: &Path, lines: &[FleetLine]) -> Result<u32, FleetError> {
    let mut total = 0u32;
    for line in lines {
        if line.count == 0 {
            return Err(FleetError::ZeroCount {
                engagement: String::new(),
                class: line.class.clone(),
            });
        }
        let def = load_ship_def(data_root, &line.class)?;
        total = total.saturating_add(def.cost.saturating_mul(line.count));
    }
    Ok(total)
}

pub fn engagement_costs(
    data_root: &Path,
    engagement: &EngagementSpec,
) -> Result<FleetCosts, FleetError> {
    Ok(FleetCosts {
        player: fleet_cost(data_root, &engagement.player).map_err(|e| annotate(e, &engagement.name))?,
        opponent: fleet_cost(data_root, &engagement.opponent)
            .map_err(|e| annotate(e, &engagement.name))?,
    })
}

fn annotate(err: FleetError, engagement: &str) -> FleetError {
    match err {
        FleetError::ZeroCount { class, .. } => FleetError::ZeroCount {
            engagement: engagement.to_string(),
            class,
        },
        other => other,
    }
}

/// Validate both sides are within `tolerance` of each other and optional budget.
pub fn validate_engagement_costs(
    costs: &FleetCosts,
    engagement: &str,
    budget: Option<u32>,
    tolerance: u32,
) -> Result<(), FleetError> {
    let delta = costs.player.abs_diff(costs.opponent);
    if delta > tolerance {
        return Err(FleetError::CostImbalance {
            engagement: engagement.to_string(),
            player_cost: costs.player,
            opponent_cost: costs.opponent,
            tolerance,
        });
    }
    if let Some(budget) = budget {
        for side_cost in [costs.player, costs.opponent] {
            if side_cost.abs_diff(budget) > tolerance {
                return Err(FleetError::OffBudget {
                    engagement: engagement.to_string(),
                    side_cost,
                    budget,
                    tolerance,
                });
            }
        }
    }
    Ok(())
}

/// Build a scenario: player fleet west facing east, opponent east facing west.
/// Terminal is `annihilation` (all enemy ships destroyed).
pub fn build_engagement_scenario(
    engagement: &EngagementSpec,
    map: &FleetMapSpec,
    seed: u64,
) -> Result<ScenarioDef, FleetError> {
    let player_n: u32 = engagement.player.iter().map(|l| l.count).sum();
    let opponent_n: u32 = engagement.opponent.iter().map(|l| l.count).sum();
    let needed = player_n.max(opponent_n).max(1);
    if map.height < needed + 2 || map.width < 10 {
        return Err(FleetError::MapTooSmall {
            engagement: engagement.name.clone(),
            needed: needed + 2,
            width: map.width,
            height: map.height,
        });
    }

    let mut ships = Vec::new();
    let mut next_id = 1u32;

    place_side(
        &mut ships,
        &mut next_id,
        &engagement.player,
        /*controller*/ "player",
        /*facing*/ 0,
        /*q*/ 3,
        map.height,
    )?;
    place_side(
        &mut ships,
        &mut next_id,
        &engagement.opponent,
        "scripted",
        3,
        map.width as i32 - 4,
        map.height,
    )?;

    Ok(ScenarioDef {
        width: map.width,
        height: map.height,
        seed,
        map_mode: map.map_mode.clone(),
        objective: None,
        terminal: Some(TerminalDef {
            terminal_type: "annihilation".into(),
            target: None,
        }),
        ships,
    })
}

fn place_side(
    ships: &mut Vec<ShipPlacementDef>,
    next_id: &mut u32,
    lines: &[FleetLine],
    controller: &str,
    facing: u8,
    q: i32,
    height: u32,
) -> Result<(), FleetError> {
    let total: u32 = lines.iter().map(|l| l.count).sum();
    if total == 0 {
        return Ok(());
    }
    // Center the column vertically.
    let start_r = ((height.saturating_sub(total)) / 2) as i32;
    let mut index = 0u32;
    for line in lines {
        if line.count == 0 {
            return Err(FleetError::ZeroCount {
                engagement: String::new(),
                class: line.class.clone(),
            });
        }
        for _ in 0..line.count {
            ships.push(ShipPlacementDef {
                id: *next_id,
                class: line.class.clone(),
                q,
                r: start_r + index as i32,
                facing,
                controller: controller.into(),
                velocity: None,
                course: None,
                power: line.power,
                structure: line.structure,
                max_shield_per_facing: line.max_shield_per_facing,
            });
            *next_id += 1;
            index += 1;
        }
    }
    Ok(())
}

/// Synthetic path used in match reports for generated engagements.
pub fn engagement_report_path(suite_dir: &Path, engagement_name: &str) -> PathBuf {
    suite_dir.join(format!("engagement:{engagement_name}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn destroyer_line_cost_is_100() {
        let cost = fleet_cost(
            &root(),
            &[FleetLine::new("destroyer_line", 1)],
        )
        .expect("cost");
        assert_eq!(cost, 100);
    }

    #[test]
    fn eight_destroyers_match_titan_within_tolerance() {
        let eng = EngagementSpec {
            name: "swarm_vs_titan".into(),
            player: vec![FleetLine::new("destroyer_line", 8)],
            opponent: vec![FleetLine::new("titan_line", 1)],
        };
        let costs = engagement_costs(&root(), &eng).expect("costs");
        assert_eq!(costs.player, 800);
        assert_eq!(costs.opponent, 798);
        validate_engagement_costs(&costs, &eng.name, Some(800), 60).expect("balanced");
    }

    #[test]
    fn power_sweep_expands_named_engagements() {
        let sweep = PowerSweepSpec {
            name: "titan_power".into(),
            class: "titan_line".into(),
            side: "opponent".into(),
            values: vec![90, 120],
            player: vec![FleetLine::new("destroyer_line", 8)],
            opponent: vec![FleetLine::new("titan_line", 1)],
        };
        let eng = sweep.expand().expect("expand");
        assert_eq!(eng.len(), 2);
        assert_eq!(eng[0].name, "titan_power_p90");
        assert_eq!(eng[0].opponent[0].power, Some(90));
        assert_eq!(eng[1].opponent[0].power, Some(120));
    }

    #[test]
    fn build_scenario_places_fleets_and_annihilation() {
        let eng = EngagementSpec {
            name: "test".into(),
            player: vec![FleetLine::new("destroyer_line", 2)],
            opponent: vec![FleetLine::new("heavy_cruiser_line", 1)],
        };
        let def = build_engagement_scenario(&eng, &FleetMapSpec::default(), 1).expect("def");
        assert_eq!(def.ships.len(), 3);
        assert_eq!(
            def.terminal.as_ref().map(|t| t.terminal_type.as_str()),
            Some("annihilation")
        );
        assert!(def.ships.iter().any(|s| s.controller == "player"));
        assert!(def.ships.iter().any(|s| s.controller == "scripted"));
    }
}
