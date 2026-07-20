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
    /// Relative target silhouette. Canonical tiers are 1..=7 in
    /// `data/sizes.toml` (Fighter … Titan). Size 2 is the neutral to-hit
    /// baseline (`combat.accuracy.baseline_target_size` in
    /// `data/rules/default.toml`); smaller ships are harder to hit and larger
    /// ships easier.
    pub size: u32,
    /// Design maximum path actions per turn (hull cap).
    pub max_maneuver_actions: u8,
    /// Energy per turn.
    pub power: u32,
    pub max_shield_per_facing: u32,
    #[serde(default)]
    pub structure: u32,
    /// SSD power-system boxes.
    pub power_sys: u32,
    /// SSD engine boxes.
    pub engine_boxes: u32,
    /// SSD boxes per weapon. Defaults to one box per weapon.
    #[serde(default = "default_weapon_boxes")]
    pub weapon_boxes: u32,
    /// Additional fire-control accuracy bonus for this hull. Defaults to zero.
    #[serde(default)]
    pub attack_accuracy_bonus: u8,
    #[serde(default)]
    pub weapons: Vec<WeaponDef>,
    /// Motion produced per unit of engine power. Defaults to 1.
    #[serde(default = "default_thrust_per_power")]
    pub thrust_per_power: u32,
    /// Engine power required per unit of motion. Defaults to 1.
    #[serde(default = "default_power_per_thrust")]
    pub power_per_thrust: u32,
    /// Construction / fleet-budget cost (catalog). Not consumed by combat rules yet.
    #[serde(default)]
    pub cost: u32,
}

fn default_thrust_per_power() -> u32 {
    1
}

fn default_power_per_thrust() -> u32 {
    1
}

fn default_weapon_boxes() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioDef {
    pub width: u32,
    pub height: u32,
    #[serde(default = "default_seed")]
    pub seed: u64,
    /// `"unbounded"` (default), `"hard"`, or `"floating"` (D4).
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
    /// Override design power for this placement (balance sweeps / scenarios).
    #[serde(default)]
    pub power: Option<u32>,
    /// Override hull structure boxes for this placement.
    #[serde(default)]
    pub structure: Option<u32>,
    /// Override per-facing shield cap for this placement.
    #[serde(default)]
    pub max_shield_per_facing: Option<u32>,
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
    pub max_charge: u32,
}
