use crate::arc;
use crate::combat_tables::WeaponKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arc {
    Forward,
    Rear,
    Left,
    Right,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weapon {
    pub id: String,
    pub kind: WeaponKind,
    pub arc: Arc,
    pub mount: Option<arc::Mount>,
    pub max_range: u32,
    pub max_charge: u32,
}
