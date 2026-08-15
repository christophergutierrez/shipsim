use super::*;

impl GameState {
    /// Validate allocation draft without mutation (shared by stage + apply).
    fn validate_allocation_draft(
        &self,
        ship: &Ship,
        ship_id: u32,
        movement: u32,
        weapons: &BTreeMap<String, u32>,
        shields: &[u32; 6],
        options: AllocationOptions,
    ) -> Result<(u32, BTreeMap<String, u32>), crate::movement::OrderError> {
        let mut weapon_increases: u32 = 0;
        let mut merged_charges = ship.weapon_charges.clone();
        for (weapon_id, charge) in weapons {
            let weapon = ship
                .weapon(weapon_id)
                .ok_or_else(|| crate::movement::OrderError::WeaponNotFound(weapon_id.clone()))?;
            if *charge > weapon.max_charge {
                return Err(crate::movement::OrderError::WeaponChargeTooHigh {
                    ship: ship_id,
                    weapon: weapon_id.clone(),
                    charge: *charge,
                    max: weapon.max_charge,
                });
            }
            let have = ship.weapon_charges.get(weapon_id).copied().unwrap_or(0);
            if *charge < have {
                return Err(crate::movement::OrderError::CannotStripWeaponCharge {
                    ship: ship_id,
                    weapon: weapon_id.clone(),
                    have,
                    want: *charge,
                });
            }
            // Reject charge *increases* when the magazine is empty. Listing a
            // dry weapon at its current charge is a no-op and remains legal.
            if *charge > have && ship.weapon_ammo.get(weapon_id).copied().unwrap_or(u32::MAX) == 0 {
                return Err(crate::movement::OrderError::WeaponOutOfAmmo {
                    ship: ship_id,
                    weapon: weapon_id.clone(),
                });
            }
            weapon_increases = weapon_increases.saturating_add(charge - have);
            merged_charges.insert(weapon_id.clone(), *charge);
        }
        for (facing, power) in shields.iter().copied().enumerate() {
            let max = ship.shield_cap(facing);
            if power > max {
                return Err(crate::movement::OrderError::ShieldPowerTooHigh {
                    ship: ship_id,
                    facing: facing as u8,
                    power,
                    max,
                });
            }
        }
        let shield_power: u32 = shields.iter().copied().sum();
        let cloak_power = if options.cloak {
            if !ship
                .systems
                .iter()
                .any(|system| matches!(system, crate::schema::SystemKind::Cloak))
            {
                return Err(crate::movement::OrderError::SystemNotInstalled(
                    "cloak".into(),
                ));
            }
            4u32.saturating_add(ship.size)
        } else {
            0
        };
        let repair_cap = ship.repair_cap();
        if options.repair > repair_cap {
            return Err(crate::movement::OrderError::RepairTooMuch {
                ship: ship_id,
                requested: options.repair,
                max: repair_cap,
            });
        }
        if options.repair > 0
            && !ship
                .systems
                .iter()
                .any(|system| matches!(system, crate::schema::SystemKind::Repair))
        {
            return Err(crate::movement::OrderError::SystemNotInstalled(
                "repair".into(),
            ));
        }
        let total = movement
            .saturating_add(weapon_increases)
            .saturating_add(shield_power)
            .saturating_add(cloak_power)
            .saturating_add(options.repair.saturating_mul(2));
        let available = ship.effective_power();
        if total > available {
            return Err(crate::movement::OrderError::OverAllocated {
                ship: ship_id,
                total,
                available,
            });
        }
        Ok((movement, merged_charges))
    }

    /// Stage one allocation. Applied only when every living ship has staged.
    pub fn allocate_v2(
        &mut self,
        ship_id: u32,
        movement: u32,
        weapons: BTreeMap<String, u32>,
        shields: [u32; 6],
    ) -> Result<(), crate::movement::OrderError> {
        self.allocate_v2_with_systems(
            ship_id,
            movement,
            weapons,
            shields,
            AllocationOptions::default(),
        )
    }

    pub fn allocate_v2_with_systems(
        &mut self,
        ship_id: u32,
        movement: u32,
        weapons: BTreeMap<String, u32>,
        shields: [u32; 6],
        options: AllocationOptions,
    ) -> Result<(), crate::movement::OrderError> {
        if self.phase != Phase::Allocate {
            return Err(crate::movement::OrderError::WrongPhase {
                expected: "allocate",
                actual: self.phase_name(),
            });
        }
        if self.staged_allocations.contains_key(&ship_id)
            || self.allocated_this_turn.contains(&ship_id)
        {
            return Err(crate::movement::OrderError::AlreadyAllocated(ship_id));
        }

        let ship = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        if ship.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(ship_id));
        }
        self.validate_allocation_draft(ship, ship_id, movement, &weapons, &shields, options)?;
        // Stage only — do not mutate public ship state yet.
        self.staged_allocations.insert(
            ship_id,
            StagedAllocation {
                movement,
                weapons,
                shields,
                cloak: options.cloak,
                repair: options.repair,
                unsquad: options.unsquad,
                squad_leader: options.squad_leader,
            },
        );

        if self.all_living_staged_allocation() {
            self.apply_all_allocations();
        }
        Ok(())
    }

    fn all_living_staged_allocation(&self) -> bool {
        self.ships
            .iter()
            .filter(|s| !s.destroyed)
            .all(|s| self.staged_allocations.contains_key(&s.id))
    }

    fn apply_all_allocations(&mut self) {
        let staged: Vec<(u32, StagedAllocation)> = self
            .staged_allocations
            .iter()
            .map(|(id, a)| (*id, a.clone()))
            .collect();
        for (ship_id, alloc) in &staged {
            let Some(ship) = self.ship(*ship_id).cloned() else {
                continue;
            };
            let Ok((_, merged_charges)) = self.validate_allocation_draft(
                &ship,
                *ship_id,
                alloc.movement,
                &alloc.weapons,
                &alloc.shields,
                AllocationOptions {
                    cloak: alloc.cloak,
                    repair: alloc.repair,
                    unsquad: alloc.unsquad,
                    squad_leader: alloc.squad_leader,
                },
            ) else {
                continue;
            };
            let Some(ship) = self.ship_mut(*ship_id) else {
                continue;
            };
            ship.movement_allocated = alloc.movement;
            let (converted, _rem) = ship.thrust_conversion.convert(alloc.movement);
            let cap = ship.effective_max_maneuver_actions();
            ship.motion_available = path::usable_motion(converted, cap);
            ship.weapon_charges = merged_charges;
            ship.shields_powered = alloc.shields;
            ship.shields_remaining = alloc.shields;
            ship.cloaked = alloc.cloak;
            if alloc.repair > 0 {
                ship.ssd.hull = ship
                    .ssd
                    .hull
                    .saturating_add(alloc.repair)
                    .min(ship.ssd.hull_max);
                ship.destroyed = ship.ssd.is_destroyed();
            }
            self.allocated_this_turn.insert(*ship_id);
        }
        self.apply_squad_allocation_changes(&staged);
        self.staged_allocations.clear();
        self.phase = Phase::Movement;
        self.path_commits.clear();
        self.path_results.clear();
    }

    fn apply_squad_allocation_changes(&mut self, staged: &[(u32, StagedAllocation)]) {
        for (ship_id, alloc) in staged {
            if alloc.unsquad {
                let squad_id = self
                    .squads
                    .iter()
                    .find(|(_, squad)| squad.members.contains(ship_id))
                    .map(|(id, _)| *id);
                if let Some(squad_id) = squad_id {
                    if let Some(squad) = self.squads.get_mut(&squad_id) {
                        squad.members.retain(|member| member != ship_id);
                        if squad.members.is_empty() {
                            self.squads.remove(&squad_id);
                        } else if squad.leader == *ship_id {
                            squad.leader = *squad.members.iter().min().unwrap();
                        }
                    }
                }
            }
            if let Some(requested) = alloc.squad_leader {
                if let Some(squad) = self.squads.values_mut().find(|squad| {
                    squad.members.contains(ship_id) && squad.members.contains(&requested)
                }) {
                    squad.leader = requested;
                }
            }
        }
    }
}
