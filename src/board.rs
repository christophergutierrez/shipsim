use crate::hex::Hex;

/// How the play area treats edges (D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MapMode {
    /// Off-map is illegal (plots rejected).
    #[default]
    Hard,
    /// Formation may drift; board recenters after movement.
    Floating,
}

impl MapMode {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "floating" | "float" => MapMode::Floating,
            _ => MapMode::Hard,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Board {
    pub width: u32,
    pub height: u32,
    pub mode: MapMode,
}

impl Board {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            mode: MapMode::Hard,
        }
    }

    pub fn with_mode(mut self, mode: MapMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn contains(&self, hex: Hex) -> bool {
        hex.q >= 0 && hex.r >= 0 && hex.q < self.width as i32 && hex.r < self.height as i32
    }

    /// Translate a set of positions so the bounding box fits on the board when possible.
    /// Returns the delta applied (dq, dr).
    pub fn float_delta(positions: &[Hex], width: u32, height: u32) -> (i32, i32) {
        if positions.is_empty() {
            return (0, 0);
        }
        let min_q = positions.iter().map(|h| h.q).min().unwrap();
        let max_q = positions.iter().map(|h| h.q).max().unwrap();
        let min_r = positions.iter().map(|h| h.r).min().unwrap();
        let max_r = positions.iter().map(|h| h.r).max().unwrap();
        let span_q = max_q - min_q;
        let span_r = max_r - min_r;
        let w = width as i32;
        let h = height as i32;

        // Prefer centering the formation in the board.
        let target_min_q = if span_q < w { (w - span_q) / 2 } else { 0 };
        let target_min_r = if span_r < h { (h - span_r) / 2 } else { 0 };
        (target_min_q - min_q, target_min_r - min_r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_float_delta_centers() {
        let pos = [Hex::new(10, 10), Hex::new(11, 10)];
        let (dq, dr) = Board::float_delta(&pos, 8, 8);
        let shifted: Vec<Hex> = pos.iter().map(|h| Hex::new(h.q + dq, h.r + dr)).collect();
        let board = Board::new(8, 8);
        for h in &shifted {
            assert!(board.contains(*h), "shifted {h:?} should be on 8x8 board");
        }
        // Relative separation preserved.
        assert_eq!(shifted[1].q - shifted[0].q, 1);
    }
}
