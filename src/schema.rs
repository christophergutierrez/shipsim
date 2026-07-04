use serde::Deserialize;

fn default_turn_mode() -> u32 {
    0
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShipDef {
    pub id: String,
    pub name: String,
    pub speed_max: u32,
    #[serde(default = "default_turn_mode")]
    pub turn_mode: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioDef {
    pub width: u32,
    pub height: u32,
    pub objective: HexDef,
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
