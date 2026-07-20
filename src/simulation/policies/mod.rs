use std::collections::BTreeMap;

use crate::arc::{self, Mount};
use crate::combat_tables::{self, WeaponKind};
use crate::hex::Hex;
use crate::movement::{Order, VolleyShot};
use crate::path::{self, PathAction, PathState};
use crate::rules::Ruleset;
use crate::snapshot::{ShipSnapshot, StateSnapshot, WeaponSnapshot};

use super::policy::{DecisionContext, Policy};

#[derive(Debug, Clone, Copy)]
enum Style {
    Random,
    Greedy,
    Aggressive,
    Defensive,
    Mobility,
}

pub struct BaselinePolicy {
    name: &'static str,
    style: Style,
    random_state: u64,
}

pub fn build_policy(name: &str, seed: u64) -> Option<Box<dyn Policy>> {
    let (canonical, style) = match name {
        "random" => ("random", Style::Random),
        "greedy" => ("greedy", Style::Greedy),
        "aggressive" => ("aggressive", Style::Aggressive),
        "defensive" => ("defensive", Style::Defensive),
        "mobility" | "mobility_first" => ("mobility", Style::Mobility),
        _ => return None,
    };
    Some(Box::new(BaselinePolicy {
        name: canonical,
        style,
        random_state: seed.max(1),
    }))
}

impl BaselinePolicy {
    fn next_random(&mut self) -> u64 {
        let mut value = self.random_state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.random_state = value;
        value
    }

    /// Axial hex distance between two ships (same metric as core hex distance).
    fn range_to(a: &ShipSnapshot, b: &ShipSnapshot) -> i32 {
        let dq = b.q - a.q;
        let dr = b.r - a.r;
        dq.abs().max(dr.abs()).max((dq + dr).abs())
    }

    fn nearest_enemy<'a>(context: &'a DecisionContext<'_>) -> Option<&'a ShipSnapshot> {
        context
            .snapshot
            .ships
            .iter()
            .filter(|ship| {
                !ship.destroyed
                    && ship.id != context.ship.id
                    && ship.controller != context.ship.controller
            })
            .min_by_key(|ship| (Self::range_to(context.ship, ship), ship.id))
    }

    fn nearest_enemy_for<'a>(
        ship: &ShipSnapshot,
        snapshot: &'a StateSnapshot,
    ) -> Option<&'a ShipSnapshot> {
        snapshot
            .ships
            .iter()
            .filter(|candidate| {
                !candidate.destroyed
                    && candidate.id != ship.id
                    && candidate.controller != ship.controller
            })
            .min_by_key(|candidate| (Self::range_to(ship, candidate), candidate.id))
    }

    fn power_for_motion(ship: &ShipSnapshot, motion: u32) -> u32 {
        if motion == 0 || ship.thrust_per_power == 0 {
            return 0;
        }
        motion
            .saturating_mul(ship.power_per_thrust)
            .saturating_add(ship.thrust_per_power - 1)
            / ship.thrust_per_power
    }

    fn desired_motion(&mut self, ship: &ShipSnapshot, snapshot: Option<&StateSnapshot>) -> u32 {
        let cap = u32::from(ship.max_maneuver_actions);
        if cap == 0 {
            return 0;
        }
        match self.style {
            Style::Random => 1 + (self.next_random() % u64::from(cap)) as u32,
            Style::Mobility => cap,
            Style::Defensive => {
                let Some(snapshot) = snapshot else {
                    return cap.min(1);
                };
                let Some(enemy) = Self::nearest_enemy_for(ship, snapshot) else {
                    return 0;
                };
                let range = Self::range_to(ship, enemy).max(0) as u32;
                let weapon_range = ship
                    .weapons
                    .iter()
                    .filter(|weapon| weapon.operational)
                    .map(|weapon| weapon.max_range)
                    .max()
                    .unwrap_or(0);
                if range > weapon_range {
                    cap.clamp(1, 2)
                } else {
                    cap.min(1)
                }
            }
            Style::Aggressive | Style::Greedy => {
                let Some(snapshot) = snapshot else {
                    return cap;
                };
                let dist = Self::nearest_enemy_for(ship, snapshot)
                    .map(|enemy| Self::range_to(ship, enemy).max(0) as u32)
                    .unwrap_or(0);
                cap.min(dist.max(1))
            }
        }
    }

    fn allocation(
        &mut self,
        ship: &ShipSnapshot,
        snapshot: Option<&StateSnapshot>,
    ) -> (u32, BTreeMap<String, u32>, [u32; 6]) {
        let power = ship.power_available;
        let desired_motion = self.desired_motion(ship, snapshot);
        let movement = Self::power_for_motion(ship, desired_motion).min(power);
        let mut remaining = power - movement;
        let mut weapons = BTreeMap::new();
        let defensive_reserve = if matches!(self.style, Style::Defensive) {
            remaining / 2
        } else {
            0
        };
        let weapon_budget = remaining - defensive_reserve;
        // Protocol 3/4: charge carries. Desired totals must be >= current charge;
        // only *increases* spend from the power pool.
        let mut weapon_remaining = weapon_budget;
        for weapon in ship.weapons.iter().filter(|weapon| weapon.operational) {
            let have = weapon.charge;
            let desired = match self.style {
                Style::Mobility => 1u32.max(have),
                Style::Random => {
                    let roll =
                        1 + (self.next_random() % u64::from(weapon.max_charge.max(1))) as u32;
                    roll.max(have)
                }
                _ => weapon.max_charge.max(have),
            };
            let target = desired.min(weapon.max_charge).max(have);
            let increase = target.saturating_sub(have);
            if increase > weapon_remaining {
                weapons.insert(weapon.id.clone(), have);
                continue;
            }
            weapons.insert(weapon.id.clone(), target);
            weapon_remaining -= increase;
        }
        remaining -= weapon_budget - weapon_remaining;

        let mut shields = [0u32; 6];
        let facing_order = match self.style {
            Style::Defensive => [0, 1, 5, 2, 4, 3],
            _ => [0, 1, 5, 3, 2, 4],
        };
        for facing in facing_order {
            if remaining == 0 {
                break;
            }
            let amount = remaining.min(ship.max_shield_per_facing);
            shields[facing] = amount;
            remaining -= amount;
        }
        (movement, weapons, shields)
    }

    fn preferred_facing(ship: &ShipSnapshot, target: &ShipSnapshot) -> u8 {
        (0..6)
            .min_by_key(|face| {
                let delta = Hex::direction(*face).unwrap_or(Hex::ORIGIN);
                let next = Hex::new(ship.q + delta.q, ship.r + delta.r);
                let tq = target.q - next.q;
                let tr = target.r - next.r;
                (tq.abs().max(tr.abs()).max((tq + tr).abs()), *face)
            })
            .unwrap_or(ship.facing)
    }

    fn facing_cw_steps(from: u8, to: u8) -> u8 {
        (from + 6 - to) % 6
    }

    fn facing_ccw_steps(from: u8, to: u8) -> u8 {
        (to + 6 - from) % 6
    }

    fn map_bounds(snapshot: &StateSnapshot) -> Option<path::MapBounds> {
        if snapshot.map.mode == "hard" {
            Some(path::MapBounds {
                width: snapshot.map.width,
                height: snapshot.map.height,
            })
        } else {
            None
        }
    }

    /// Greedy path planner mirroring `ai::v2_path_decision`, snapshot-only.
    fn plan_path(&mut self, context: &DecisionContext<'_>) -> Vec<PathAction> {
        let ship = context.ship;
        let budget = ship.motion_available;
        if budget == 0 {
            return Vec::new();
        }

        if matches!(self.style, Style::Random) {
            return self.random_path(context, budget);
        }

        let Some(target) = Self::nearest_enemy(context) else {
            return Vec::new();
        };
        let want_face = Self::preferred_facing(ship, target);
        let mut state = PathState {
            pos: Hex::new(ship.q, ship.r),
            facing: ship.facing,
        };
        let mut actions = Vec::new();
        let bounds = Self::map_bounds(context.snapshot);
        let target_pos = Hex::new(target.q, target.r);

        for _ in 0..budget {
            let dist = state.pos.distance(target_pos);
            if dist <= 1 {
                if state.facing != want_face {
                    let action = if Self::facing_cw_steps(state.facing, want_face)
                        <= Self::facing_ccw_steps(state.facing, want_face)
                    {
                        PathAction::TurnRight
                    } else {
                        PathAction::TurnLeft
                    };
                    if let Ok(next) = path::apply_action(state, action) {
                        state = next;
                        actions.push(action);
                        continue;
                    }
                }
                break;
            }

            let candidates = [
                PathAction::MoveF,
                PathAction::MoveFr,
                PathAction::MoveFl,
                PathAction::TurnRight,
                PathAction::TurnLeft,
            ];
            let mut best: Option<(PathAction, PathState, u32, u8)> = None;
            for action in candidates {
                let Ok(next) = path::apply_action(state, action) else {
                    continue;
                };
                if let Some(b) = bounds {
                    if action.translates() && !b.contains(next.pos) {
                        continue;
                    }
                }
                let next_dist = next.pos.distance(target_pos);
                let face_delta = Self::facing_cw_steps(next.facing, want_face)
                    .min(Self::facing_ccw_steps(next.facing, want_face));
                // Defensive bias: prefer turns/sidesteps over pure rush when close.
                let style_bias = match self.style {
                    Style::Defensive if action.translates() && next_dist < dist && dist <= 3 => {
                        1u32
                    }
                    Style::Mobility if action.translates() => 0,
                    Style::Aggressive if action.translates() && next_dist < dist => 0,
                    _ => 0,
                };
                let key = (next_dist + style_bias, face_delta);
                match best {
                    None => best = Some((action, next, key.0, key.1)),
                    Some((_, _, bd, bf)) if key < (bd, bf) => {
                        best = Some((action, next, key.0, key.1))
                    }
                    _ => {}
                }
            }
            if let Some((action, next, ..)) = best {
                state = next;
                actions.push(action);
            } else {
                break;
            }
        }
        actions
    }

    fn random_path(&mut self, context: &DecisionContext<'_>, budget: u32) -> Vec<PathAction> {
        let ship = context.ship;
        let mut state = PathState {
            pos: Hex::new(ship.q, ship.r),
            facing: ship.facing,
        };
        let mut actions = Vec::new();
        let bounds = Self::map_bounds(context.snapshot);
        let length = 1 + (self.next_random() % u64::from(budget.max(1))) as u32;
        let length = length.min(budget);
        for _ in 0..length {
            let mut choices = Vec::new();
            for action in PathAction::ALL {
                let Ok(next) = path::apply_action(state, action) else {
                    continue;
                };
                if let Some(b) = bounds {
                    if action.translates() && !b.contains(next.pos) {
                        continue;
                    }
                }
                choices.push((action, next));
            }
            if choices.is_empty() {
                break;
            }
            let index = (self.next_random() as usize) % choices.len();
            let (action, next) = choices[index];
            state = next;
            actions.push(action);
        }
        actions
    }

    fn parse_mount(name: &str) -> Option<Mount> {
        match name {
            "forward" => Some(Mount::Forward),
            "forward_starboard" => Some(Mount::ForwardStarboard),
            "aft_starboard" => Some(Mount::AftStarboard),
            "aft" => Some(Mount::Aft),
            "aft_port" => Some(Mount::AftPort),
            "forward_port" => Some(Mount::ForwardPort),
            _ => None,
        }
    }

    fn parse_kind(name: &str) -> Option<WeaponKind> {
        match name {
            "Beam" | "beam" => Some(WeaponKind::Beam),
            "Plasma" | "plasma" => Some(WeaponKind::Plasma),
            "Torp" | "torp" | "Torpedo" => Some(WeaponKind::Torp),
            _ => None,
        }
    }

    fn weapon_can_fire(
        ship: &ShipSnapshot,
        weapon: &WeaponSnapshot,
        target: &ShipSnapshot,
        rules: &Ruleset,
    ) -> Option<u8> {
        if !weapon.operational || weapon.fired || weapon.charge == 0 {
            return None;
        }
        let from = Hex::new(ship.q, ship.r);
        let to = Hex::new(target.q, target.r);
        let range = from.distance(to);
        if range == 0 || range > weapon.max_range {
            return None;
        }
        if let Some(kind) = Self::parse_kind(&weapon.kind) {
            if kind == WeaponKind::Beam
                && combat_tables::beam_damage(rules.combat(), weapon.charge, range).is_none()
            {
                return None;
            }
        }
        if let Some(mount_name) = &weapon.mount {
            let mount = Self::parse_mount(mount_name)?;
            if !arc::in_arc(mount, ship.facing, from, to) {
                return None;
            }
        }
        arc::legal_shield_facings(from, to, target.facing)
            .into_iter()
            .next()
    }

    fn plan_volley(&mut self, context: &DecisionContext<'_>) -> Vec<VolleyShot> {
        let Some(target) = Self::nearest_enemy(context) else {
            return Vec::new();
        };
        if matches!(self.style, Style::Random) && self.next_random().is_multiple_of(4) {
            // Occasional hold-fire keeps the random policy from always volleying.
            return Vec::new();
        }
        let mut shots = Vec::new();
        for weapon in &context.ship.weapons {
            let Some(shield_facing) =
                Self::weapon_can_fire(context.ship, weapon, target, context.rules)
            else {
                continue;
            };
            if matches!(self.style, Style::Defensive) && weapon.kind == "Beam" {
                // Defensive: prefer not to empty beam banks at max range noise.
                let range = Self::range_to(context.ship, target).max(0) as u32;
                if range > weapon.max_range.saturating_sub(1).max(1) && weapon.charge < 2 {
                    continue;
                }
            }
            shots.push(VolleyShot {
                weapon: weapon.id.clone(),
                target: target.id,
                shield_facing,
            });
            if matches!(self.style, Style::Random) && self.next_random().is_multiple_of(2) {
                break;
            }
        }
        // Prefer lower-structure targets when multiple enemies exist (aggressive/greedy).
        if matches!(self.style, Style::Aggressive | Style::Greedy) {
            if let Some(weaker) = context
                .snapshot
                .ships
                .iter()
                .filter(|ship| {
                    !ship.destroyed
                        && ship.id != context.ship.id
                        && ship.controller != context.ship.controller
                })
                .filter(|ship| {
                    context.ship.weapons.iter().any(|weapon| {
                        Self::weapon_can_fire(context.ship, weapon, ship, context.rules).is_some()
                    })
                })
                .min_by_key(|ship| (ship.structure, ship.id))
            {
                if weaker.id != target.id {
                    shots.clear();
                    for weapon in &context.ship.weapons {
                        if let Some(shield_facing) =
                            Self::weapon_can_fire(context.ship, weapon, weaker, context.rules)
                        {
                            shots.push(VolleyShot {
                                weapon: weapon.id.clone(),
                                target: weaker.id,
                                shield_facing,
                            });
                        }
                    }
                }
            }
        }
        shots
    }

    fn choose_path(&mut self, context: &DecisionContext<'_>) -> Order {
        Order::CommitPath {
            ship: context.ship.id,
            actions: self.plan_path(context),
        }
    }

    fn choose_volley(&mut self, context: &DecisionContext<'_>) -> Order {
        Order::CommitVolley {
            ship: context.ship.id,
            shots: self.plan_volley(context),
        }
    }
}

impl Policy for BaselinePolicy {
    fn name(&self) -> &str {
        self.name
    }

    fn allocate(&mut self, ship: &ShipSnapshot) -> Order {
        let (movement, weapons, shields) = self.allocation(ship, None);
        Order::Allocate {
            ship: ship.id,
            movement,
            weapons,
            shields,
        }
    }

    fn allocate_with_context(&mut self, ship: &ShipSnapshot, snapshot: &StateSnapshot) -> Order {
        let (movement, weapons, shields) = self.allocation(ship, Some(snapshot));
        Order::Allocate {
            ship: ship.id,
            movement,
            weapons,
            shields,
        }
    }

    fn choose_order(&mut self, context: &DecisionContext<'_>) -> Order {
        match context.snapshot.phase.as_str() {
            "movement" => self.choose_path(context),
            "firing" => self.choose_volley(context),
            _ => context
                .legal_orders
                .first()
                .cloned()
                .unwrap_or(Order::CommitPath {
                    ship: context.ship.id,
                    actions: Vec::new(),
                }),
        }
    }
}
