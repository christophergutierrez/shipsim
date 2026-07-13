//! Combat Model v2 weapon mount and shield-facing geometry.
//!
//! Facing 0 is straight ahead. Forward-port and forward-starboard mounts can
//! also bear straight ahead, but no mount reaches past the neighboring face.

use serde::{Deserialize, Serialize};

use crate::hex::Hex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mount {
    Forward,
    ForwardStarboard,
    AftStarboard,
    Aft,
    AftPort,
    ForwardPort,
}

impl Mount {
    pub fn relative_facings(self) -> &'static [u8] {
        match self {
            Mount::Forward => &[0],
            Mount::ForwardStarboard => &[5, 0],
            Mount::AftStarboard => &[3, 4],
            Mount::Aft => &[3],
            Mount::AftPort => &[2, 3],
            Mount::ForwardPort => &[0, 1],
        }
    }
}

pub fn in_arc(mount: Mount, attacker_facing: u8, from: Hex, to: Hex) -> bool {
    let relative = relative_bearing(attacker_facing, from, to);
    mount.relative_facings().contains(&relative)
}

pub fn legal_shield_facings(attacker_pos: Hex, target_pos: Hex, target_facing: u8) -> Vec<u8> {
    let absolute = nearest_bearings(target_pos, attacker_pos);
    absolute
        .into_iter()
        .map(|bearing| (bearing + 6 - target_facing) % 6)
        .fold(Vec::new(), |mut facings, facing| {
            if !facings.contains(&facing) {
                facings.push(facing);
            }
            facings
        })
}

pub fn relative_bearing(origin_facing: u8, from: Hex, to: Hex) -> u8 {
    (bearing_to(from, to) + 6 - origin_facing) % 6
}

pub fn bearing_to(from: Hex, to: Hex) -> u8 {
    nearest_bearings(from, to).into_iter().next().unwrap_or(0)
}

fn nearest_bearings(from: Hex, to: Hex) -> Vec<u8> {
    if from == to {
        return vec![0];
    }

    let neighbors = from.neighbors();
    let best = neighbors
        .iter()
        .map(|neighbor| neighbor.distance(to))
        .min()
        .unwrap_or(0);

    neighbors
        .iter()
        .enumerate()
        .filter_map(|(facing, neighbor)| (neighbor.distance(to) == best).then_some(facing as u8))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_faces_match_v2_arc_contract() {
        assert_eq!(Mount::Forward.relative_facings(), &[0]);
        assert_eq!(Mount::ForwardStarboard.relative_facings(), &[5, 0]);
        assert_eq!(Mount::AftStarboard.relative_facings(), &[3, 4]);
        assert_eq!(Mount::Aft.relative_facings(), &[3]);
        assert_eq!(Mount::AftPort.relative_facings(), &[2, 3]);
        assert_eq!(Mount::ForwardPort.relative_facings(), &[0, 1]);
    }

    #[test]
    fn in_arc_uses_attacker_facing() {
        let from = Hex::new(0, 0);
        assert!(in_arc(Mount::Forward, 0, from, Hex::new(3, 0)));
        assert!(!in_arc(Mount::Forward, 0, from, Hex::new(3, -3)));
        assert!(in_arc(Mount::ForwardPort, 0, from, Hex::new(3, -3)));
        assert!(in_arc(Mount::ForwardStarboard, 0, from, Hex::new(0, 3)));
        assert!(in_arc(Mount::Aft, 0, from, Hex::new(-3, 0)));
    }

    #[test]
    fn legal_shields_are_relative_to_target_facing() {
        let target = Hex::new(0, 0);
        assert_eq!(legal_shield_facings(Hex::new(3, 0), target, 0), vec![0]);
        assert_eq!(legal_shield_facings(Hex::new(3, 0), target, 1), vec![5]);
        assert_eq!(legal_shield_facings(Hex::new(3, -3), target, 0), vec![1]);
    }

    #[test]
    fn legal_shields_include_corner_ties() {
        let facings = legal_shield_facings(Hex::new(2, -1), Hex::new(0, 0), 0);
        assert_eq!(facings, vec![0, 1]);
    }
}
