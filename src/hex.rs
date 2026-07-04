use serde::{Deserialize, Serialize};
use std::ops::{Add, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hex {
    pub q: i32,
    pub r: i32,
}

const DIRECTIONS: [Hex; 6] = [
    Hex { q: 1, r: 0 },
    Hex { q: 1, r: -1 },
    Hex { q: 0, r: -1 },
    Hex { q: -1, r: 0 },
    Hex { q: -1, r: 1 },
    Hex { q: 0, r: 1 },
];

impl Add for Hex {
    type Output = Hex;

    fn add(self, rhs: Hex) -> Hex {
        Hex::new(self.q + rhs.q, self.r + rhs.r)
    }
}

impl Sub for Hex {
    type Output = Hex;

    fn sub(self, rhs: Hex) -> Hex {
        Hex::new(self.q - rhs.q, self.r - rhs.r)
    }
}

impl Hex {
    pub const ORIGIN: Hex = Hex { q: 0, r: 0 };

    pub const fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    pub fn distance(self, other: Hex) -> u32 {
        let a = self.to_cube();
        let b = other.to_cube();
        ((a.0 - b.0).abs() + (a.1 - b.1).abs() + (a.2 - b.2).abs()) as u32 / 2
    }

    pub fn neighbors(self) -> [Hex; 6] {
        DIRECTIONS.map(|direction| self + direction)
    }

    pub fn direction(facing: u8) -> Option<Hex> {
        DIRECTIONS.get(facing as usize).copied()
    }

    fn to_cube(self) -> (i32, i32, i32) {
        let x = self.q;
        let z = self.r;
        let y = -x - z;
        (x, y, z)
    }
}
