#[derive(Debug, Clone)]
pub struct Prng {
    state: u64,
}

impl Prng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn from_state(state: u64) -> Self {
        Self { state }
    }

    pub fn state(&self) -> u64 {
        self.state
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D049BB133111EB);
        value ^ (value >> 31)
    }

    pub fn roll(&mut self, sides: u32) -> u32 {
        debug_assert!(sides > 0);
        (self.next_u64() % sides as u64) as u32 + 1
    }
}
