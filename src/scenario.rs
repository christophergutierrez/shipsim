use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc as SharedArc;

use thiserror::Error;

use crate::arc::Mount;
use crate::board::{Board, MapMode};
use crate::combat::{Arc, Weapon};
use crate::combat_tables::WeaponKind;
use crate::game_state::{GameState, NpcController, Terminal};
use crate::hex::Hex;
use crate::rules::{RulesError, Ruleset};
use crate::schema::{ScenarioDef, ShipDef, WeaponDef};
use crate::ship::Ship;

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("cannot read {path:?}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot parse {path:?}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("cannot load rules: {0}")]
    Rules(#[from] RulesError),
    #[error("ship class {class:?} is missing at {path:?}")]
    MissingShipClass { class: String, path: PathBuf },
    #[error("hex ({q},{r}) is outside the scenario map")]
    OffBoard { q: i32, r: i32 },
    #[error("facing {facing} is not in 0..=5")]
    InvalidFacing { facing: u8 },
    #[error("ship class {class:?} has size 0; size must be at least 1")]
    InvalidShipSize { class: String },
    #[error("ship class {class:?} has engine_boxes 0; engine_boxes must be at least 1")]
    InvalidEngineBoxes { class: String },
    #[error("ship class {class:?} has power_sys 0; power_sys must be at least 1")]
    InvalidPowerSys { class: String },
    #[error("ship class {class:?} has weapon_boxes 0; weapon_boxes must be at least 1")]
    InvalidWeaponBoxes { class: String },
    #[error("ships {a} and {b} both placed on hex ({q},{r})")]
    OverlappingPlacement { a: u32, b: u32, q: i32, r: i32 },
    #[error("scenario defines conflicting terminals (objective / destruction / annihilation)")]
    ConflictingTerminals,
    #[error("destruction terminal missing target")]
    DestructionTargetMissing,
    #[error("unknown terminal type {0:?}")]
    UnknownTerminal(String),
    #[error("unknown weapon kind {kind:?} on weapon {weapon_id}")]
    UnknownWeaponKind { kind: String, weapon_id: String },
    #[error("unknown weapon arc {arc:?} on weapon {weapon_id}")]
    UnknownWeaponArc { arc: String, weapon_id: String },
    #[error(
        "ship class {class:?} weapon {weapon:?} max_range {configured} exceeds rules maximum {supported}"
    )]
    WeaponRangeExceedsRules {
        class: String,
        weapon: String,
        configured: u32,
        supported: u32,
    },
    #[error("ship class {class:?} weapon {weapon:?} must have max_range greater than zero")]
    InvalidWeaponRange { class: String, weapon: String },
    #[error("ship class {class:?} has invalid thrust conversion: {source}")]
    InvalidThrustConversion {
        class: String,
        source: crate::thrust::ConversionError,
    },
    #[error(
        "ship class {class:?} cannot buy one motion point with its design power {power} (conversion {thrust_per_power}:{power_per_thrust})"
    )]
    MobileHullCannotBuyThrust {
        class: String,
        power: u32,
        thrust_per_power: u32,
        power_per_thrust: u32,
    },
}

pub fn load_scenario(path: &Path) -> Result<GameState, LoadError> {
    let text = read_to_string(path)?;
    let def: ScenarioDef = parse_toml(path, &text)?;
    let data_root = path
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."));
    let rules = Ruleset::load(data_root)?;
    load_scenario_def_with_rules(&def, data_root, rules)
}

/// Load a scenario from an in-memory definition.
/// `data_root` is the repository root containing `data/ships/`.
pub fn load_scenario_def(def: &ScenarioDef, data_root: &Path) -> Result<GameState, LoadError> {
    let rules = Ruleset::load(data_root)?;
    load_scenario_def_with_rules(def, data_root, rules)
}

/// Load a scenario with an already validated ruleset.
pub fn load_scenario_def_with_rules(
    def: &ScenarioDef,
    data_root: &Path,
    rules: SharedArc<Ruleset>,
) -> Result<GameState, LoadError> {
    let mode = def
        .map_mode
        .as_deref()
        .map(MapMode::parse)
        .unwrap_or_default();
    let board = Board::new(def.width, def.height).with_mode(mode);

    let has_objective = def.objective.is_some();
    let terminal_kind = def.terminal.as_ref().map(|t| t.terminal_type.as_str());
    let has_destruction = terminal_kind == Some("destruction");
    let has_annihilation = terminal_kind == Some("annihilation");
    let terminal_count =
        usize::from(has_objective) + usize::from(has_destruction) + usize::from(has_annihilation);
    if terminal_count > 1 {
        return Err(LoadError::ConflictingTerminals);
    }

    let terminal = if let Some(obj) = def.objective.as_ref() {
        let hex = Hex::new(obj.q, obj.r);
        validate_on_board(&board, hex)?;
        Some(Terminal::ReachHex(hex))
    } else if let Some(term) = def.terminal.as_ref() {
        match term.terminal_type.as_str() {
            "destruction" => {
                let target = term.target.ok_or(LoadError::DestructionTargetMissing)?;
                Some(Terminal::DestroyShip(target))
            }
            "annihilation" => Some(Terminal::AnnihilateEnemies),
            other => return Err(LoadError::UnknownTerminal(other.to_string())),
        }
    } else {
        None
    };

    let seed = if def.seed == 0 { 1 } else { def.seed };

    let mut ships = Vec::with_capacity(def.ships.len());
    let mut npcs: BTreeMap<u32, NpcController> = BTreeMap::new();
    let mut occupied: BTreeMap<(i32, i32), u32> = BTreeMap::new();

    for placement in &def.ships {
        if !Hex::is_valid_facing(placement.facing) {
            return Err(LoadError::InvalidFacing {
                facing: placement.facing,
            });
        }

        let pos = Hex::new(placement.q, placement.r);
        validate_on_board(&board, pos)?;
        if let Some(other) = occupied.insert((pos.q, pos.r), placement.id) {
            return Err(LoadError::OverlappingPlacement {
                a: other,
                b: placement.id,
                q: pos.q,
                r: pos.r,
            });
        }

        let ship_def = load_ship_def(data_root, &placement.class)?;
        if ship_def.size == 0 {
            return Err(LoadError::InvalidShipSize {
                class: placement.class.clone(),
            });
        }
        let ctrl = placement.controller.to_ascii_lowercase();
        let is_ai = matches!(ctrl.as_str(), "ai" | "greedy");
        let is_scripted = !is_ai && ctrl == "scripted";

        let power = placement.power.unwrap_or(ship_def.power);
        let structure = placement.structure.unwrap_or(ship_def.structure);
        let max_shield_per_facing = placement
            .max_shield_per_facing
            .unwrap_or(ship_def.max_shield_per_facing);
        let weapons: Vec<_> = ship_def
            .weapons
            .into_iter()
            .map(|weapon_def| {
                let weapon_id = weapon_def.id.clone();
                let weapon = parse_weapon(weapon_def)?;
                if weapon.max_range == 0 {
                    return Err(LoadError::InvalidWeaponRange {
                        class: placement.class.clone(),
                        weapon: weapon_id,
                    });
                }
                let supported = rules.max_range(weapon.kind);
                if weapon.max_range > supported {
                    return Err(LoadError::WeaponRangeExceedsRules {
                        class: placement.class.clone(),
                        weapon: weapon_id,
                        configured: weapon.max_range,
                        supported,
                    });
                }
                Ok(weapon)
            })
            .collect::<Result<Vec<_>, LoadError>>()?;
        if ship_def.engine_boxes == 0 {
            return Err(LoadError::InvalidEngineBoxes {
                class: placement.class.clone(),
            });
        }
        if ship_def.power_sys == 0 {
            return Err(LoadError::InvalidPowerSys {
                class: placement.class.clone(),
            });
        }
        if ship_def.weapon_boxes == 0 {
            return Err(LoadError::InvalidWeaponBoxes {
                class: placement.class.clone(),
            });
        }
        let ssd = crate::ssd::Ssd::with_weapon_boxes(
            structure,
            ship_def.engine_boxes,
            ship_def.power_sys,
            weapons.len(),
            ship_def.weapon_boxes,
        );

        let max_maneuver_actions = ship_def.max_maneuver_actions;
        let thrust_conversion = crate::thrust::ThrustConversion::new(
            ship_def.thrust_per_power,
            ship_def.power_per_thrust,
            max_maneuver_actions,
        )
        .map_err(|source| LoadError::InvalidThrustConversion {
            class: placement.class.clone(),
            source,
        })?;

        if max_maneuver_actions > 0 {
            let (motion_at_full_power, _remainder) = thrust_conversion.convert(power);
            if motion_at_full_power < 1 {
                return Err(LoadError::MobileHullCannotBuyThrust {
                    class: placement.class.clone(),
                    power,
                    thrust_per_power: ship_def.thrust_per_power,
                    power_per_thrust: ship_def.power_per_thrust,
                });
            }
        }

        ships.push(Ship {
            id: placement.id,
            class: ship_def.name,
            size: ship_def.size,
            pos,
            facing: placement.facing,
            power,
            attack_accuracy_bonus: ship_def.attack_accuracy_bonus,
            weapons,
            shields_powered: [0; 6],
            shields_remaining: [0; 6],
            max_shield_per_facing,
            movement_allocated: 0,
            weapon_charges: BTreeMap::new(),
            ssd,
            destroyed: false,
            max_maneuver_actions,
            thrust_conversion,
            motion_available: 0,
        });
        if is_ai {
            npcs.insert(placement.id, NpcController::GreedySeek);
        } else if is_scripted {
            npcs.insert(placement.id, NpcController::Scripted);
        }
    }

    Ok(GameState::new_with_options(
        board, ships, terminal, npcs, seed, rules,
    ))
}

/// Load a ship class TOML from `{data_root}/data/ships/{class}.toml`.
pub fn load_ship_def(data_root: &Path, class: &str) -> Result<ShipDef, LoadError> {
    let ship_path = data_root
        .join("data")
        .join("ships")
        .join(format!("{class}.toml"));
    if !ship_path.exists() {
        return Err(LoadError::MissingShipClass {
            class: class.to_string(),
            path: ship_path,
        });
    }

    let text = read_to_string(&ship_path)?;
    let ship_def: ShipDef = parse_toml(&ship_path, &text)?;
    Ok(ship_def)
}

fn read_to_string(path: &Path) -> Result<String, LoadError> {
    std::fs::read_to_string(path).map_err(|source| LoadError::Read {
        path: path.to_path_buf(),
        source,
    })
}

fn parse_toml<T: serde::de::DeserializeOwned>(path: &Path, text: &str) -> Result<T, LoadError> {
    toml::from_str(text).map_err(|source| LoadError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

fn validate_on_board(board: &Board, hex: Hex) -> Result<(), LoadError> {
    if board.mode == MapMode::Unbounded || board.contains(hex) {
        Ok(())
    } else {
        Err(LoadError::OffBoard { q: hex.q, r: hex.r })
    }
}

fn parse_weapon(def: WeaponDef) -> Result<Weapon, LoadError> {
    let kind = match def.kind.as_str() {
        "plasma" => WeaponKind::Plasma,
        "beam" => WeaponKind::Beam,
        "torp" => WeaponKind::Torp,
        other => {
            return Err(LoadError::UnknownWeaponKind {
                kind: other.to_string(),
                weapon_id: def.id.clone(),
            });
        }
    };
    let arc = match def.arc.as_str() {
        "forward" => Arc::Forward,
        "rear" => Arc::Rear,
        "left" => Arc::Left,
        "right" => Arc::Right,
        "all" => Arc::All,
        other => {
            return Err(LoadError::UnknownWeaponArc {
                arc: other.to_string(),
                weapon_id: def.id.clone(),
            });
        }
    };
    let mount = match def.mount.as_deref().unwrap_or(def.arc.as_str()) {
        "forward" => Some(Mount::Forward),
        "forward_starboard" => Some(Mount::ForwardStarboard),
        "aft_starboard" => Some(Mount::AftStarboard),
        "aft" | "rear" => Some(Mount::Aft),
        "aft_port" => Some(Mount::AftPort),
        "forward_port" => Some(Mount::ForwardPort),
        "left" | "right" | "all" => None,
        other => {
            return Err(LoadError::UnknownWeaponArc {
                arc: other.to_string(),
                weapon_id: def.id.clone(),
            });
        }
    };
    Ok(Weapon {
        id: def.id,
        kind,
        arc,
        mount,
        max_range: def.max_range,
        max_charge: def.max_charge,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_catalog_requires_explicit_combat_fields() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let entries = std::fs::read_dir(root.join("data/ships")).expect("ship catalog");
        let mut count = 0;
        for entry in entries {
            let path = entry.expect("catalog entry").path();
            if path.extension().and_then(|value| value.to_str()) != Some("toml") {
                continue;
            }
            let class = path.file_stem().unwrap().to_str().unwrap();
            let ship = load_ship_def(root, class).expect("strict ship definition");
            assert!(ship.power_sys > 0, "{class} power_sys");
            assert!(ship.engine_boxes > 0, "{class} engine_boxes");
            assert!(ship.max_shield_per_facing > 0, "{class} shield cap");
            for weapon in &ship.weapons {
                assert!(weapon.max_range > 0, "{class}/{} max_range", weapon.id);
                assert!(weapon.max_charge > 0, "{class}/{} max_charge", weapon.id);
            }
            count += 1;
        }
        assert!(count > 0);
    }

    fn write_ship(dir: &std::path::Path, class: &str, body: &str) {
        let ships_dir = dir.join("data").join("ships");
        std::fs::create_dir_all(&ships_dir).expect("create data/ships");
        std::fs::write(ships_dir.join(format!("{class}.toml")), body).expect("write ship def");
    }

    fn one_ship_scenario(class: &str) -> ScenarioDef {
        toml::from_str(&format!(
            r#"
width = 6
height = 6
seed = 1

[terminal]
type = "destruction"
target = 999

[[ships]]
id = 1
class = "{class}"
q = 0
r = 0
facing = 0
controller = "player"
"#
        ))
        .expect("scenario parses")
    }

    const MIN_SHIP: &str = r#"
name = "Test"
size = 2
max_maneuver_actions = 4
power = 8
max_shield_per_facing = 1
structure = 1
power_sys = 1
engine_boxes = 1
"#;

    #[test]
    fn zero_engine_boxes_is_rejected_at_load_time() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_ship(
            dir.path(),
            "bad_engine",
            &format!("{MIN_SHIP}\nengine_boxes = 0\n"),
        );
        // Override: rewrite cleanly
        write_ship(
            dir.path(),
            "bad_engine",
            r#"
name = "Bad Engine"
size = 2
max_maneuver_actions = 1
power = 4
max_shield_per_facing = 1
structure = 1
power_sys = 1
engine_boxes = 0
"#,
        );
        let def = one_ship_scenario("bad_engine");
        let error = load_scenario_def_with_rules(&def, dir.path(), Ruleset::builtin())
            .expect_err("zero engine_boxes must be rejected");
        assert!(
            matches!(error, LoadError::InvalidEngineBoxes { .. }),
            "{error}"
        );
    }

    #[test]
    fn zero_power_sys_is_rejected_at_load_time() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_ship(
            dir.path(),
            "bad_power",
            r#"
name = "Bad Power"
size = 2
max_maneuver_actions = 1
power = 4
max_shield_per_facing = 1
structure = 1
power_sys = 0
engine_boxes = 1
"#,
        );
        let def = one_ship_scenario("bad_power");
        let error = load_scenario_def_with_rules(&def, dir.path(), Ruleset::builtin())
            .expect_err("zero power_sys must be rejected");
        assert!(
            matches!(error, LoadError::InvalidPowerSys { .. }),
            "{error}"
        );
    }

    #[test]
    fn zero_explicit_weapon_boxes_is_rejected_at_load_time() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_ship(
            dir.path(),
            "bad_weapon_boxes",
            r#"
name = "Bad Weapon Boxes"
size = 2
max_maneuver_actions = 1
power = 4
max_shield_per_facing = 1
structure = 1
power_sys = 1
engine_boxes = 1
weapon_boxes = 0
"#,
        );
        let def = one_ship_scenario("bad_weapon_boxes");
        let error = load_scenario_def_with_rules(&def, dir.path(), Ruleset::builtin())
            .expect_err("zero weapon_boxes must be rejected");
        assert!(
            matches!(error, LoadError::InvalidWeaponBoxes { .. }),
            "{error}"
        );
    }

    #[test]
    fn omitted_weapon_boxes_still_defaults_to_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_ship(
            dir.path(),
            "default_weapon_boxes",
            r#"
name = "Default Weapon Boxes"
size = 2
max_maneuver_actions = 1
power = 4
max_shield_per_facing = 1
structure = 1
power_sys = 1
engine_boxes = 1
"#,
        );
        let def = one_ship_scenario("default_weapon_boxes");
        load_scenario_def_with_rules(&def, dir.path(), Ruleset::builtin())
            .expect("omitted weapon_boxes should default to one, not be rejected");
    }

    #[test]
    fn missing_size_fails_ship_deserialization() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_ship(
            dir.path(),
            "no_size",
            r#"
name = "No Size"
max_maneuver_actions = 1
power = 4
max_shield_per_facing = 1
structure = 1
power_sys = 1
engine_boxes = 1
"#,
        );
        let error =
            load_ship_def(dir.path(), "no_size").expect_err("missing size must fail to parse");
        assert!(matches!(error, LoadError::Parse { .. }), "{error}");
    }
}
