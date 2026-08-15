use super::*;

impl GameState {
    /// Commit one complete path for `ship`. Resolves when all living ships commit.
    /// `evasive` spends motion from the same budget as path actions.
    pub fn commit_path(
        &mut self,
        ship_id: u32,
        actions: Vec<PathAction>,
        evasive: u32,
    ) -> Result<(), crate::movement::OrderError> {
        self.commit_path_with_follow(ship_id, actions, evasive, false)
    }

    pub fn commit_path_with_follow(
        &mut self,
        ship_id: u32,
        actions: Vec<PathAction>,
        evasive: u32,
        follow: bool,
    ) -> Result<(), crate::movement::OrderError> {
        if self.phase != Phase::Movement {
            return Err(crate::movement::OrderError::WrongPhase {
                expected: "movement",
                actual: self.phase_name(),
            });
        }
        let ship = self
            .ship(ship_id)
            .ok_or(crate::movement::OrderError::ShipNotFound(ship_id))?;
        if ship.destroyed {
            return Err(crate::movement::OrderError::ShipNotFound(ship_id));
        }
        if self
            .squads
            .values()
            .any(|squad| squad.leader != ship_id && squad.members.contains(&ship_id))
            && (!follow || !actions.is_empty())
        {
            return Err(crate::movement::OrderError::SquaddedFollowerPath { ship: ship_id });
        }
        if self.path_commits.contains_key(&ship_id) {
            return Err(crate::movement::OrderError::AlreadyCommittedPath(ship_id));
        }
        let start = PathState {
            pos: ship.pos,
            facing: ship.facing,
        };
        // Path cost + evasive share the motion budget; reduce budget so
        // trace_path enforces the combined limit.
        let budget = ship.motion_available.saturating_sub(evasive);
        if evasive > ship.motion_available {
            return Err(crate::movement::OrderError::InsufficientMotion {
                ship: ship_id,
                need: evasive,
                have: ship.motion_available,
            });
        }
        path::trace_path(start, &actions, budget, self.hard_bounds()).map_err(|err| {
            crate::movement::OrderError::IllegalPath {
                ship: ship_id,
                reason: err.to_string(),
            }
        })?;
        self.path_commits
            .insert(ship_id, PathCommit { actions, evasive });
        if self.all_living_path_committed() {
            self.resolve_paths_phase();
        }
        Ok(())
    }

    fn all_living_path_committed(&self) -> bool {
        self.ships
            .iter()
            .filter(|s| !s.destroyed)
            .all(|s| self.path_commits.contains_key(&s.id))
    }

    fn resolve_paths_phase(&mut self) {
        let bounds = self.hard_bounds();
        let mut claims = Vec::new();
        let mut evasion_by_ship: BTreeMap<u32, u32> = BTreeMap::new();
        let mut claim_ids = BTreeMap::new();
        for (squad_id, squad) in &self.squads {
            claim_ids.insert(squad.leader, *squad_id);
        }
        for ship in self.ships.iter().filter(|s| !s.destroyed) {
            if self
                .squads
                .values()
                .any(|squad| squad.members.contains(&ship.id) && squad.leader != ship.id)
            {
                evasion_by_ship.insert(
                    ship.id,
                    self.path_commits
                        .get(&ship.id)
                        .map(|c| c.evasive)
                        .unwrap_or(0),
                );
                continue;
            }
            let commit = self.path_commits.get(&ship.id);
            let actions = commit.map(|c| c.actions.clone()).unwrap_or_default();
            let evasive = commit.map(|c| c.evasive).unwrap_or(0);
            evasion_by_ship.insert(ship.id, evasive);
            let start = PathState {
                pos: ship.pos,
                facing: ship.facing,
            };
            let group_motion = claim_ids
                .get(&ship.id)
                .and_then(|squad_id| self.squads.get(squad_id))
                .map(|squad| {
                    squad
                        .members
                        .iter()
                        .filter_map(|member| self.ship(*member).map(|ship| ship.motion_available))
                        .min()
                        .unwrap_or(ship.motion_available)
                })
                .unwrap_or(ship.motion_available);
            let budget = group_motion.saturating_sub(evasive);
            if let Ok(trace) = path::trace_path(start, &actions, budget, bounds) {
                claims.push(PathClaim {
                    ship: ship.id,
                    trace,
                });
            }
        }
        let occupancy = self.ships.clone();
        let results = path_resolve::resolve_paths(&claims, &mut self.prng);
        for result in &results {
            let members = self
                .squads
                .values()
                .find(|squad| squad.leader == result.ship)
                .map(|squad| squad.members.clone())
                .unwrap_or_else(|| vec![result.ship]);
            let leader_side = self.controller_label(result.ship);
            let blocked_by_enemy = occupancy.iter().any(|occupant| {
                occupant.pos == Hex::new(result.final_q, result.final_r)
                    && !members.contains(&occupant.id)
                    && !occupant.destroyed
                    && self.controller_label(occupant.id) != leader_side
            });
            if blocked_by_enemy {
                continue;
            }
            for member in members {
                if let Some(ship) = self.ship_mut(member) {
                    ship.pos = Hex::new(result.final_q, result.final_r);
                    ship.facing = result.final_facing;
                    ship.motion_available = 0;
                }
            }
        }
        // Apply declared evasion for every living ship that committed a path
        // (including pure-jink empty paths that still spent evasive motion).
        for (ship_id, evasive) in &evasion_by_ship {
            if let Some(ship) = self.ship_mut(*ship_id) {
                ship.evasion_committed = *evasive;
                ship.motion_available = 0;
            }
        }
        self.path_results = results;
        self.path_commits.clear();
        self.refresh_status();
        if self.status != ScenarioStatus::InProgress {
            return;
        }
        self.phase = Phase::Firing;
        self.volley_commits.clear();
    }

}
