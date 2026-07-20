use crate::hex::Hex;

/// How the play area treats edges (D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MapMode {
    /// Off-map is illegal (plots rejected).
    Hard,
    /// Formation may drift. Its nominal dimensions are a client camera hint.
    Floating,
    /// No edges: negative and large coordinates are legal. No recentering,
    /// no clamping. Width/height are metadata only (ADR-0022 unbounded world).
    #[default]
    Unbounded,
}

impl MapMode {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "floating" | "float" => MapMode::Floating,
            "unbounded" | "infinite" => MapMode::Unbounded,
            _ => MapMode::Hard,
        }
    }

    /// True iff off-map destinations are illegal (edge exits blocked).
    /// `Floating` and `Unbounded` both allow movement beyond the nominal box.
    /// Clients, not the engine, choose how to recenter a floating-map camera.
    pub fn blocks_edges(self) -> bool {
        matches!(self, MapMode::Hard)
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
            mode: MapMode::Unbounded,
        }
    }

    pub fn with_mode(mut self, mode: MapMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn contains(&self, hex: Hex) -> bool {
        hex.q >= 0 && hex.r >= 0 && hex.q < self.width as i32 && hex.r < self.height as i32
    }

    /// Calculate a presentation-only camera offset that fits a formation on a
    /// nominal board when possible. Never apply this to game coordinates.
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
    fn float_delta_centers_a_presentation_view() {
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

    #[test]
    fn parse_recognizes_unbounded() {
        assert_eq!(MapMode::parse("unbounded"), MapMode::Unbounded);
        assert_eq!(MapMode::parse("infinite"), MapMode::Unbounded);
        assert_eq!(MapMode::parse("UNBOUNDED"), MapMode::Unbounded);
        // Existing modes still parse.
        assert_eq!(MapMode::parse("hard"), MapMode::Hard);
        assert_eq!(MapMode::parse("floating"), MapMode::Floating);
        // Unknown falls back to Hard (default).
        assert_eq!(MapMode::parse("nonsense"), MapMode::Hard);
    }

    #[test]
    fn blocks_edges_only_for_hard() {
        assert!(MapMode::Hard.blocks_edges());
        assert!(!MapMode::Floating.blocks_edges());
        assert!(!MapMode::Unbounded.blocks_edges());
    }
}
