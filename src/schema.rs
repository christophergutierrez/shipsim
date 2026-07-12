use serde::Deserialize;

fn default_seed() -> u64 {
    1
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShipDef {
    /// Catalog key from the ship TOML file (kept for data identity; runtime id is placement).
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub speed: u32,
    /// Energy per turn; defaults to `speed` when omitted (full movement available).
    #[serde(default)]
    pub power: Option<u32>,
    #[serde(default = "default_max_shield_per_facing")]
    pub max_shield_per_facing: u32,
    #[serde(default)]
    pub structure: u32,
    #[serde(default)]
    pub weapons: Vec<WeaponDef>,
    /// Design maximum velocity in hexes per turn (ADR-0022 §1). When omitted,
    /// the loader derives it from the legacy `speed` field (so a legacy speed-1
    /// hull becomes max velocity 1, etc.); an immobile hull sets this to 0.
    /// Explicit values override the legacy `speed` derivation.
    #[serde(default)]
    pub max_velocity: Option<u8>,
    /// Thrust produced per unit of engine power (ADR-0022 §5). Defaults to 1
    /// (1:1 cruiser-class conversion). Immobile hulls set this to 0.
    #[serde(default = "default_thrust_per_power")]
    pub thrust_per_power: u32,
    /// Engine power required per unit of thrust (ADR-0022 §5). Defaults to 1.
    #[serde(default = "default_power_per_thrust")]
    pub power_per_thrust: u32,
}

fn default_thrust_per_power() -> u32 {
    1
}

fn default_power_per_thrust() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioDef {
    pub width: u32,
    pub height: u32,
    #[serde(default = "default_seed")]
    pub seed: u64,
    /// `"hard"` (default) or `"floating"` (D4).
    #[serde(default)]
    pub map_mode: Option<String>,
    pub objective: Option<HexDef>,
    #[serde(default)]
    pub terminal: Option<TerminalDef>,
    pub ships: Vec<ShipPlacementDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HexDef {
    pub q: i32,
    pub r: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShipPlacementDef {
    pub id: u32,
    pub class: String,
    pub q: i32,
    pub r: i32,
    pub facing: u8,
    #[serde(default)]
    pub controller: String,
    /// Initial velocity speed for inertial movement (ADR-0022 §1). Defaults to
    /// 0 (stationary). Must not exceed the hull's `max_velocity`.
    #[serde(default)]
    pub velocity: Option<u8>,
    /// Initial course (hex direction 0..=5). Defaults to `facing` when omitted.
    #[serde(default)]
    pub course: Option<u8>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TerminalDef {
    #[serde(rename = "type")]
    pub terminal_type: String,
    #[serde(default)]
    pub target: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponDef {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub mount: Option<String>,
    pub arc: String,
    pub max_range: u32,
    #[serde(default = "default_max_charge")]
    pub max_charge: u32,
}

fn default_max_charge() -> u32 {
    4
}

fn default_max_shield_per_facing() -> u32 {
    6
}
