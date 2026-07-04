use serde::Deserialize;

fn default_seed() -> u64 {
    1
}

fn default_turn_mode() -> u32 {
    0
}

fn default_shields() -> [u32; 6] {
    [0; 6]
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShipDef {
    pub id: String,
    pub name: String,
    pub speed_max: u32,
    #[serde(default = "default_turn_mode")]
    pub turn_mode: u32,
    #[serde(default = "default_shields")]
    pub shields: [u32; 6],
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
    #[serde(default)]
    pub waypoints: Vec<HexDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TerminalDef {
    #[serde(rename = "type")]
    pub terminal_type: String,
    #[serde(default)]
    pub target: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WeaponDef {
    pub id: String,
    pub kind: String,
    pub arc: String,
    pub max_range: u32,
    #[serde(default = "default_weapon_damage")]
    pub damage: u32,
    #[serde(default)]
    pub phaser_dice_by_range: Vec<u32>,
    #[serde(default)]
    pub to_hit_by_range: Vec<u32>,
}

fn default_weapon_damage() -> u32 {
    1
}
