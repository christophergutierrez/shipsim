use crate::hex::Hex;

#[derive(Debug, Clone)]
pub struct Board {
    pub width: u32,
    pub height: u32,
}

impl Board {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn contains(&self, hex: Hex) -> bool {
        hex.q >= 0 && hex.r >= 0 && hex.q < self.width as i32 && hex.r < self.height as i32
    }
}
