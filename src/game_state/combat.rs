use super::*;

impl GameState {
    fn mark_weapon_fired(&mut self, ship: u32, weapon: &str) {
        self.fired_weapons_this_turn
            .insert((ship, weapon.to_string()));
    }

    /// Commit one complete volley. Resolves when all living ships commit.
    pub fn commit_volley(
        &mut self,
        ship_id: u32,
        shots: Vec<VolleyShot>,
    ) -> Result<(), crate::movement::OrderError> {
        if self.phase != Phase::Firing {
            return Err(crate::movement::OrderError::WrongPhase {
                expected: "firing",
                actual: self.phase_name(),
            });
        }
        let ship = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        if ship.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(ship_id));
        }
        if self.volley_commits.contains_key(&ship_id) {
            return Err(crate::movement::OrderError::AlreadyCommittedVolley(ship_id));
        }

        // Validate against current (pre-fire) geometry; each weapon at most once.
        let mut seen = HashSet::new();
        for shot in &shots {
            if !seen.insert(shot.weapon.clone()) {
                return Err(crate::movement::OrderError::WeaponAlreadyInVolley {
                    ship: ship_id,
                    weapon: shot.weapon.clone(),
                });
            }
            self.validate_shot(ship_id, shot)?;
        }

        self.volley_commits.insert(ship_id, shots);
        if self.all_living_volley_committed() {
            self.resolve_volley_phase()?;
        }
        Ok(())
    }

    fn all_living_volley_committed(&self) -> bool {
        self.ships
            .iter()
            .filter(|s| !s.destroyed)
            .all(|s| self.volley_commits.contains_key(&s.id))
    }

    pub(super) fn validate_shot(
        &self,
        ship_id: u32,
        shot: &VolleyShot,
    ) -> Result<(), crate::movement::OrderError> {
        let attacker = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        if attacker.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(ship_id));
        }
        let target = self
            .ship(shot.target)
            .ok_or(crate::movement::OrderError::TargetNotFound(shot.target))?;
        if target.destroyed {
            return Err(crate::movement::OrderError::TargetNotFound(shot.target));
        }
        if ship_id == shot.target {
            return Err(crate::movement::OrderError::FireAtSelf(shot.target));
        }
        let weapon = attacker
            .weapon(&shot.weapon)
            .ok_or_else(|| crate::movement::OrderError::WeaponNotFound(shot.weapon.clone()))?;
        if self.weapon_fired_this_turn(ship_id, &shot.weapon) {
            return Err(crate::movement::OrderError::WeaponAlreadyFired {
                ship: ship_id,
                weapon: shot.weapon.clone(),
            });
        }
        let charge = attacker
            .weapon_charges
            .get(&shot.weapon)
            .copied()
            .unwrap_or(0);
        if charge == 0 {
            return Err(crate::movement::OrderError::WeaponNotCharged {
                ship: ship_id,
                weapon: shot.weapon.clone(),
            });
        }
        if attacker
            .weapon_ammo
            .get(&shot.weapon)
            .copied()
            .unwrap_or(u32::MAX)
            == 0
        {
            return Err(crate::movement::OrderError::WeaponOutOfAmmo {
                ship: ship_id,
                weapon: shot.weapon.clone(),
            });
        }
        let kind = weapon.kind;
        let range = attacker.pos.distance(target.pos);
        if range == 0 {
            return Err(crate::movement::OrderError::TooClose {
                weapon: shot.weapon.clone(),
                range,
                min_range: 1,
            });
        }
        let max_range = self.effective_weapon_max_range(weapon);
        if range > max_range {
            return Err(crate::movement::OrderError::OutOfRange {
                weapon: shot.weapon.clone(),
                range,
                max_range,
            });
        }
        if kind == crate::combat_tables::WeaponKind::Beam
            && crate::combat_tables::beam_damage(self.rules.combat(), charge, range).is_none()
        {
            return Err(crate::movement::OrderError::NoDamage {
                weapon: shot.weapon.clone(),
                range,
                charge,
            });
        }
        if let Some(mount) = weapon.mount {
            if !crate::arc::in_arc(mount, attacker.facing, attacker.pos, target.pos) {
                return Err(crate::movement::OrderError::OutOfArc {
                    weapon: shot.weapon.clone(),
                    target: shot.target,
                });
            }
        }
        let legal_facings =
            crate::arc::legal_shield_facings(attacker.pos, target.pos, target.facing);
        if !legal_facings.contains(&shot.shield_facing) {
            return Err(crate::movement::OrderError::IllegalShieldFacing {
                requested: shot.shield_facing,
                legal: legal_facings,
            });
        }
        Ok(())
    }

    fn resolve_volley_phase(&mut self) -> Result<(), crate::movement::OrderError> {
        // Clear previous turn's combat log only when a new volley resolves so
        // the post-resolution allocate snapshot still exposes those events.
        self.combat_log.clear();

        // Freeze participants and geometry at fire start.
        let snapshot = self.ships.clone();
        let alive_at_start: HashSet<u32> = snapshot
            .iter()
            .filter(|s| !s.destroyed)
            .map(|s| s.id)
            .collect();

        // Flatten volleys into canonical (attacker, weapon, target) order.
        let mut shots: Vec<(u32, VolleyShot)> = Vec::new();
        for (ship_id, volley) in &self.volley_commits {
            if !alive_at_start.contains(ship_id) {
                continue;
            }
            for shot in volley {
                shots.push((*ship_id, shot.clone()));
            }
        }
        shots.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.weapon.cmp(&b.1.weapon))
                .then_with(|| a.1.target.cmp(&b.1.target))
        });

        let intercepted = self.resolve_point_defense(&snapshot, &shots);

        let mut results = Vec::new();
        for (attacker_id, shot) in &shots {
            if intercepted.contains(&(*attacker_id, shot.weapon.clone(), shot.target)) {
                continue;
            }
            // Attacker must have been alive at start; still fire even if destroyed mid-phase.
            if !alive_at_start.contains(attacker_id) {
                continue;
            }
            // Target must have been alive at start; overkill still executes.
            if !alive_at_start.contains(&shot.target) {
                continue;
            }
            let attacker = snapshot
                .iter()
                .find(|s| s.id == *attacker_id)
                .ok_or(crate::movement::OrderError::ShipNotFound(*attacker_id))?;
            let target = snapshot
                .iter()
                .find(|s| s.id == shot.target)
                .ok_or(crate::movement::OrderError::TargetNotFound(shot.target))?;
            // Re-validate geometry from frozen snapshot (not live destroyed state).
            let weapon = attacker
                .weapon(&shot.weapon)
                .ok_or_else(|| crate::movement::OrderError::WeaponNotFound(shot.weapon.clone()))?;
            let kind = weapon.kind;
            let range = attacker.pos.distance(target.pos);
            // Defensive: reject firing a dry magazine even if charge somehow remains.
            if attacker
                .weapon_ammo
                .get(&shot.weapon)
                .copied()
                .unwrap_or(u32::MAX)
                == 0
            {
                return Err(crate::movement::OrderError::WeaponOutOfAmmo {
                    ship: *attacker_id,
                    weapon: shot.weapon.clone(),
                });
            }
            if kind == crate::combat_tables::WeaponKind::Graviton {
                let victims: Vec<_> = snapshot
                    .iter()
                    .filter(|victim| {
                        victim.id != *attacker_id
                            && alive_at_start.contains(&victim.id)
                            && victim.pos == target.pos
                    })
                    .map(|victim| (victim.id, attacker.size.saturating_sub(victim.size)))
                    .collect();
                if let Some(att) = self.ship_mut(*attacker_id) {
                    att.weapon_charges.insert(shot.weapon.clone(), 0);
                    if let Some(ammo) = att.weapon_ammo.get_mut(&shot.weapon) {
                        *ammo = ammo.saturating_sub(1);
                    }
                }
                self.mark_weapon_fired(*attacker_id, &shot.weapon);
                for (victim, damage) in victims {
                    let (_, hull_damage) = self.apply_v2_pierce_damage(victim, damage);
                    self.combat_log.push(CombatLogEvent {
                        attacker: *attacker_id,
                        target: victim,
                        weapon: shot.weapon.clone(),
                        shield: 0,
                        damage,
                        shield_absorbed: 0,
                        hull_damage,
                        kind: "graviton".into(),
                        packet: None,
                        vs_weapon: None,
                    });
                }
                continue;
            }
            let threshold = crate::combat_tables::final_to_hit_threshold_with_modifiers(
                self.rules.combat(),
                kind,
                range,
                target.size,
                to_hit_modifiers(
                    attacker.attack_accuracy_bonus,
                    weapon.accuracy_bonus,
                    &attacker.systems,
                    target.cloaked,
                    target.evasion_committed,
                    &target.systems,
                    kind,
                ),
            )
            .ok_or_else(|| crate::movement::OrderError::OutOfRange {
                weapon: shot.weapon.clone(),
                range,
                max_range: self.effective_weapon_max_range(weapon),
            })?;
            let packet_count = if weapon.repeat {
                attacker
                    .weapon_charges
                    .get(&shot.weapon)
                    .copied()
                    .unwrap_or(0)
            } else {
                1
            };
            for packet in 0..packet_count {
                let roll = self.prng.roll(u32::from(self.rules.combat().die_sides()));
                let hit = roll <= threshold as u32;
                let damage = if hit {
                    self.v2_packet_damage(
                        attacker,
                        weapon,
                        range,
                        if weapon.repeat {
                            1
                        } else {
                            attacker
                                .weapon_charges
                                .get(&shot.weapon)
                                .copied()
                                .unwrap_or(0)
                        },
                    )?
                } else {
                    0
                };
                results.push((
                    *attacker_id,
                    shot.weapon.clone(),
                    shot.target,
                    shot.shield_facing,
                    hit,
                    damage,
                    weapon.repeat.then_some(packet as u8),
                ));
            }
        }

        for (attacker_id, weapon, target, shield_facing, hit, damage, packet) in results {
            if let Some(att) = self.ship_mut(attacker_id) {
                att.weapon_charges.insert(weapon.clone(), 0);
                if let Some(ammo) = att.weapon_ammo.get_mut(&weapon) {
                    *ammo = ammo.saturating_sub(1);
                }
            }
            self.mark_weapon_fired(attacker_id, &weapon);
            let (shield_absorbed, hull_damage) = if hit && damage > 0 {
                let pierce = snapshot
                    .iter()
                    .find(|ship| ship.id == attacker_id)
                    .and_then(|ship| ship.weapon(&weapon))
                    .is_some_and(|weapon| weapon.pierce);
                if pierce {
                    self.apply_v2_pierce_damage(target, damage)
                } else {
                    self.apply_v2_damage(target, shield_facing, damage)
                }
            } else {
                (0, 0)
            };
            self.combat_log.push(CombatLogEvent {
                attacker: attacker_id,
                target,
                weapon,
                shield: shield_facing as usize,
                damage,
                shield_absorbed,
                hull_damage,
                kind: if hit { "hit".into() } else { "miss".into() },
                packet,
                vs_weapon: None,
            });
        }

        self.volley_commits.clear();
        self.refresh_status();
        if self.status == ScenarioStatus::InProgress {
            // Automatic turn advance to next allocate.
            self.advance_turn_counter();
            self.reset_all_power();
            self.refresh_status();
        }
        Ok(())
    }

    fn resolve_point_defense(
        &mut self,
        snapshot: &[Ship],
        shots: &[(u32, VolleyShot)],
    ) -> HashSet<(u32, String, u32)> {
        let mut intercepted = HashSet::new();
        let mut used_pd = HashSet::<(u32, String)>::new();
        let ordinance: Vec<_> = shots
            .iter()
            .filter_map(|(attacker_id, shot)| {
                let attacker = snapshot.iter().find(|ship| ship.id == *attacker_id)?;
                let weapon = attacker.weapon(&shot.weapon)?;
                matches!(
                    weapon.kind,
                    crate::combat_tables::WeaponKind::Torp
                        | crate::combat_tables::WeaponKind::Missile
                )
                .then_some((*attacker_id, shot.clone()))
            })
            .collect();

        for (incoming_id, shot) in ordinance {
            let Some(incoming) = snapshot.iter().find(|ship| ship.id == incoming_id) else {
                continue;
            };
            let Some(incoming_weapon) = incoming.weapon(&shot.weapon) else {
                continue;
            };
            let Some(defender) = snapshot.iter().find(|ship| ship.id == shot.target) else {
                continue;
            };
            let mut pd_ids: Vec<_> = defender
                .weapons
                .iter()
                .filter(|weapon| weapon.kind == crate::combat_tables::WeaponKind::Pd)
                .filter(|weapon| {
                    defender
                        .weapon_charges
                        .get(&weapon.id)
                        .copied()
                        .unwrap_or(0)
                        >= 1
                })
                .filter(|weapon| {
                    weapon.mount.is_some_and(|mount| {
                        crate::arc::in_arc(mount, defender.facing, defender.pos, incoming.pos)
                    })
                })
                .map(|weapon| weapon.id.clone())
                .collect();
            pd_ids.sort();

            for pd_id in pd_ids {
                if used_pd.contains(&(defender.id, pd_id.clone())) {
                    continue;
                }
                used_pd.insert((defender.id, pd_id.clone()));
                let computer = computer_bonus(&defender.systems);
                let threshold = self
                    .rules
                    .combat()
                    .pd()
                    .threshold(incoming_weapon.kind)
                    .unwrap_or(0)
                    .saturating_add(computer)
                    .min(self.rules.combat().die_sides().saturating_sub(1));
                let roll = self.prng.roll(u32::from(self.rules.combat().die_sides()));
                if let Some(ship) = self.ship_mut(defender.id) {
                    ship.weapon_charges.insert(pd_id.clone(), 0);
                }
                let hit = roll <= u32::from(threshold);
                self.combat_log.push(CombatLogEvent {
                    attacker: defender.id,
                    target: incoming_id,
                    weapon: pd_id,
                    shield: 0,
                    damage: 0,
                    shield_absorbed: 0,
                    hull_damage: 0,
                    kind: if hit { "pd_hit" } else { "pd_miss" }.into(),
                    packet: None,
                    vs_weapon: Some(shot.weapon.clone()),
                });
                if hit {
                    intercepted.insert((incoming_id, shot.weapon.clone(), shot.target));
                    break;
                }
            }
        }
        intercepted
    }
}
