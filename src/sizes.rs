use serde::Deserialize;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Clone, Deserialize)]
pub struct SizeTable {
    pub sizes: Vec<HullSize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HullSize {
    pub id: u32,
    pub key: String,
    pub name: String,
    pub space: u32,
    pub frame_cost: u32,
    pub base_structure: u32,
    /// MOO natural defense (yard label). Combat to-hit still uses size / 2.
    #[serde(default)]
    pub defense: i8,
    pub max_maneuver_actions: u8,
    pub thrust_per_power: u32,
    pub power_per_thrust: u32,
    pub power_sys: u32,
    pub engine_boxes: u32,
}

#[derive(Debug, Error)]
pub enum SizeError {
    #[error("cannot read size table {path:?}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("cannot parse size table {path:?}: {source}")]
    Parse {
        path: String,
        source: toml::de::Error,
    },
    #[error("unknown hull size {0}")]
    Unknown(u32),
}

pub fn load(root: &Path) -> Result<SizeTable, SizeError> {
    let path = root.join("data/sizes.toml");
    let text = std::fs::read_to_string(&path).map_err(|source| SizeError::Read {
        path: path.display().to_string(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| SizeError::Parse {
        path: path.display().to_string(),
        source,
    })
}

impl SizeTable {
    pub fn get(&self, id: u32) -> Result<&HullSize, SizeError> {
        self.sizes
            .iter()
            .find(|size| size.id == id)
            .ok_or(SizeError::Unknown(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn moo_decades_span_seven_hulls() {
        let table = load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert_eq!(table.sizes.len(), 7);
        let small = table.get(1).unwrap();
        let medium = table.get(3).unwrap();
        let large = table.get(5).unwrap();
        let huge = table.get(7).unwrap();
        assert_eq!(small.space, 20);
        assert_eq!(medium.space, 200);
        assert_eq!(large.space, 2000);
        assert_eq!(huge.space, 20000);
        assert_eq!(small.base_structure, 2);
        assert_eq!(huge.base_structure, 2000);
        assert_eq!(small.defense, 2);
        assert_eq!(medium.defense, 1);
        assert_eq!(large.defense, 0);
        assert_eq!(huge.defense, -1);
        let mid = table.get(2).unwrap();
        assert_eq!(mid.base_structure, 6);
        assert!(mid.space > small.space && mid.space < medium.space);
    }
}
