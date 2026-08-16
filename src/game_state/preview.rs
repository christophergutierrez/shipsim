use super::*;

impl GameState {
    pub fn path_preview(
        &self,
        ship_id: u32,
        actions: &[PathAction],
    ) -> Result<PathPreview, crate::movement::OrderError> {
        let ship = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        if ship.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(ship_id));
        }
        let start = PathState {
            pos: ship.pos,
            facing: ship.facing,
        };
        let budget = ship.motion_available;
        match path::trace_path(start, actions, budget, self.hard_bounds()) {
            Ok(trace) => Ok(PathPreview {
                ship: ship_id,
                cost: trace.cost,
                remaining_motion: budget.saturating_sub(trace.cost),
                final_q: trace.final_state.pos.q,
                final_r: trace.final_state.pos.r,
                final_facing: trace.final_state.facing,
                steps: trace
                    .steps
                    .iter()
                    .map(|s| PathPreviewStep {
                        action: s.action.as_str().to_string(),
                        q: s.pos.q,
                        r: s.pos.r,
                        facing: s.facing,
                    })
                    .collect(),
                error_index: None,
                error: None,
            }),
            Err(err) => {
                let index = match &err {
                    path::PathError::OverBudget { index, .. } => Some(*index),
                    path::PathError::OffBoard { index, .. } => Some(*index),
                    path::PathError::InvalidFacingAt { index, .. } => Some(*index),
                    _ => None,
                };
                Ok(PathPreview {
                    ship: ship_id,
                    cost: actions.len() as u32,
                    remaining_motion: 0,
                    final_q: ship.pos.q,
                    final_r: ship.pos.r,
                    final_facing: ship.facing,
                    steps: Vec::new(),
                    error_index: index,
                    error: Some(err.to_string()),
                })
            }
        }
    }

    /// BFS reachable endpoints up to motion budget (for shading).
    ///
    /// `budget` is clamped to the authoritative usable motion pool for the ship
    /// (converted allocation when available, else effective maneuver cap). This
    /// prevents unbounded-world graph searches from hostile or accidental values.
    pub fn reach_preview(
        &self,
        ship_id: u32,
        budget: Option<u32>,
    ) -> Result<Vec<ReachEndpoint>, crate::movement::OrderError> {
        let ship = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        if ship.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(ship_id));
        }
        // During movement, zero is meaningful: it means the ship bought no
        // motion and must not receive a fabricated reach cloud. During
        // allocation, preview the largest motion purchase possible from the
        // current power pool, capped by the damaged engine and hull limits.
        let max_budget = match self.phase {
            Phase::Movement => ship.motion_available,
            Phase::Allocate => {
                let (converted, _) = ship.thrust_conversion.convert(ship.effective_power());
                converted.min(ship.effective_max_maneuver_actions())
            }
            Phase::Firing => 0,
        };
        let budget = budget.unwrap_or(max_budget).min(max_budget);
        let start = PathState {
            pos: ship.pos,
            facing: ship.facing,
        };
        let bounds = self.hard_bounds();
        let mut best: BTreeMap<(i32, i32, u8), u32> = BTreeMap::new();
        best.insert((start.pos.q, start.pos.r, start.facing), 0);
        let mut frontier = vec![(start, 0u32)];
        while let Some((state, cost)) = frontier.pop() {
            if cost >= budget {
                continue;
            }
            for action in PathAction::ALL {
                let Ok(next) = path::apply_action(state, action) else {
                    continue;
                };
                if let Some(b) = bounds {
                    if action.translates() && !b.contains(next.pos) {
                        continue;
                    }
                }
                let nc = cost + 1;
                let key = (next.pos.q, next.pos.r, next.facing);
                if best.get(&key).is_some_and(|&c| c <= nc) {
                    continue;
                }
                best.insert(key, nc);
                frontier.push((next, nc));
            }
        }
        let mut out: Vec<ReachEndpoint> = best
            .into_iter()
            .map(|((q, r, facing), min_cost)| ReachEndpoint {
                q,
                r,
                facing,
                min_cost,
            })
            .collect();
        out.sort_by_key(|e| (e.min_cost, e.q, e.r, e.facing));
        Ok(out)
    }

    pub fn fire_decision_preview(
        &self,
        ship_id: u32,
        weapon_id: &str,
        target_id: u32,
    ) -> Result<FireDecisionPreview, crate::movement::OrderError> {
        let attacker = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        let target = self
            .ship(target_id)
            .ok_or(crate::movement::OrderError::TargetNotFound(target_id))?;
        let legal_shield_facings =
            crate::arc::legal_shield_facings(attacker.pos, target.pos, target.facing);
        let shield_facing = legal_shield_facings.first().copied().unwrap_or(0);
        let shot = VolleyShot {
            weapon: weapon_id.to_string(),
            target: target_id,
            shield_facing,
        };
        if self.phase == Phase::Firing {
            self.validate_shot(ship_id, &shot)?;
        } else {
            // Soft validation for UI during other phases.
            let _ = shot;
        }

        let weapon = attacker
            .weapon(weapon_id)
            .ok_or_else(|| crate::movement::OrderError::WeaponNotFound(weapon_id.to_string()))?;
        let range = attacker.pos.distance(target.pos);
        let threshold = crate::combat_tables::final_to_hit_threshold_with_modifiers(
            self.rules.combat(),
            weapon.kind,
            range,
            target.size,
            to_hit_modifiers(
                attacker.attack_accuracy_bonus,
                weapon.accuracy_bonus,
                &attacker.systems,
                target.cloaked,
                target.evasion_committed,
                &target.systems,
                weapon.kind,
            ),
        )
        .ok_or_else(|| crate::movement::OrderError::OutOfRange {
            weapon: weapon_id.to_string(),
            range,
            max_range: self.effective_weapon_max_range(weapon),
        })?;
        let die_sides = self.rules.combat().die_sides();
        let projected_damage = if weapon.kind == crate::combat_tables::WeaponKind::Graviton {
            attacker.size.saturating_sub(target.size)
        } else {
            self.v2_projected_damage(attacker, weapon_id, weapon.kind, range)?
        };

        Ok(FireDecisionPreview {
            ship: ship_id,
            weapon: weapon_id.to_string(),
            target: target_id,
            range,
            threshold,
            die_sides,
            hit_percent: u32::from(threshold) * 100 / u32::from(die_sides),
            projected_damage,
            legal_shield_facings,
        })
    }

    pub fn can_any_legal_fire(&self) -> bool {
        self.fire_opportunity().is_some()
    }

    pub fn fire_opportunity(&self) -> Option<FireOpportunity> {
        let mut attackers: Vec<&Ship> = self
            .ships
            .iter()
            .filter(|s| !s.destroyed && self.controller_label(s.id) == "player")
            .filter(|s| !self.volley_commits.contains_key(&s.id))
            .collect();
        attackers.sort_by_key(|s| s.id);
        for attacker in attackers {
            for weapon in &attacker.weapons {
                if !self.weapon_has_legal_shot(attacker, weapon) {
                    continue;
                }
                let mut targets: Vec<&Ship> = self
                    .ships
                    .iter()
                    .filter(|t| !t.destroyed && self.controller_label(t.id) != "player")
                    .collect();
                targets.sort_by_key(|t| t.id);
                for target in targets {
                    let facings = self.v2_legal_shield_facings(attacker, weapon, target);
                    if facings.is_empty() {
                        continue;
                    }
                    return Some(FireOpportunity {
                        ship: attacker.id,
                        weapon: weapon.id.clone(),
                        target: target.id,
                        legal_shield_facings: facings,
                    });
                }
            }
        }
        None
    }

    fn weapon_has_legal_shot(&self, attacker: &Ship, weapon: &combat::Weapon) -> bool {
        self.ships
            .iter()
            .filter(|target| !target.destroyed && self.controller_label(target.id) != "player")
            .any(|target| {
                self.v2_shot_shield_facing(attacker, weapon, target)
                    .is_some()
            })
    }

    fn v2_legal_shield_facings(
        &self,
        attacker: &Ship,
        weapon: &combat::Weapon,
        target: &Ship,
    ) -> Vec<u8> {
        if self
            .v2_shot_shield_facing(attacker, weapon, target)
            .is_none()
        {
            return Vec::new();
        }
        crate::arc::legal_shield_facings(attacker.pos, target.pos, target.facing)
    }

    pub fn v2_shot_shield_facing(
        &self,
        attacker: &Ship,
        weapon: &combat::Weapon,
        target: &Ship,
    ) -> Option<u8> {
        if target.destroyed || target.id == attacker.id {
            return None;
        }
        let kind = weapon.kind;
        attacker.weapon(&weapon.id)?;
        let charge = attacker
            .weapon_charges
            .get(&weapon.id)
            .copied()
            .unwrap_or(0);
        if charge == 0 || self.weapon_fired_this_turn(attacker.id, &weapon.id) {
            return None;
        }
        let range = attacker.pos.distance(target.pos);
        if range == 0 {
            return None;
        }
        if range > self.effective_weapon_max_range(weapon) {
            return None;
        }
        if kind == crate::combat_tables::WeaponKind::Beam
            && crate::combat_tables::beam_damage(self.rules.combat(), charge, range).is_none()
        {
            return None;
        }
        if let Some(mount) = weapon.mount {
            if !crate::arc::in_arc(mount, attacker.facing, attacker.pos, target.pos) {
                return None;
            }
        }
        crate::arc::legal_shield_facings(attacker.pos, target.pos, target.facing)
            .into_iter()
            .next()
    }
}
