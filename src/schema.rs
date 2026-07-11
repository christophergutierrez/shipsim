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
