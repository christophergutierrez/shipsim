use crate::combat::Weapon;
use crate::hex::Hex;

#[derive(Debug, Clone)]
pub struct Ship {
    pub id: u32,
    pub class: String,
    pub pos: Hex,
    pub facing: u8,
    pub speed_max: u32,
    pub turn_mode: u32,
    pub weapons: Vec<Weapon>,
    pub shields: [u32; 6],
    pub structure: u32,
    pub destroyed: bool,
}
