use super::*;

impl GameState {
    /// Combat v4 NPC auto-play through collection stages.
    ///
    /// Returns every order the AI successfully applied, in application order.
    /// Harnesses must append these to the save stream so barrier-stage AI
    /// commits are replayed exactly (player-only saves cannot reconstruct them).
    pub fn resolve_v2_npc_actions(&mut self) -> Vec<crate::movement::Order> {
        use crate::movement::Order;

        const STEP_CAP: usize = 8192;
        let mut applied = Vec::new();

        for _ in 0..STEP_CAP {
            if self.status != ScenarioStatus::InProgress {
                break;
            }
            match self.phase {
                Phase::Allocate => {
                    let next = self
                        .ships
                        .iter()
                        .find(|ship| {
                            !ship.destroyed
                                && self.is_v2_ai(ship.id)
                                && !self.staged_allocations.contains_key(&ship.id)
                                && !self.allocated_this_turn.contains(&ship.id)
                        })
                        .map(|ship| ship.id);
                    match next {
                        Some(id) => {
                            let (movement, weapons, shields) = crate::ai::v2_allocation(self, id)
                                .unwrap_or((0, BTreeMap::new(), [0; 6]));
                            let order = Order::Allocate {
                                ship: id,
                                movement,
                                weapons: weapons.clone(),
                                shields,
                                cloak: false,
                                repair: 0,
                                unsquad: false,
                                squad_leader: None,
                            };
                            if self.allocate_v2(id, movement, weapons, shields).is_ok() {
                                applied.push(order);
                            }
                        }
                        None => break,
                    }
                }
                Phase::Movement => {
                    let next = self
                        .ships
                        .iter()
                        .find(|ship| {
                            !ship.destroyed
                                && self.is_v2_ai(ship.id)
                                && !self.has_committed_path(ship.id)
                        })
                        .map(|ship| ship.id);
                    match next {
                        Some(id) => {
                            let actions = crate::ai::v2_path_decision(self, id).unwrap_or_default();
                            let order = Order::CommitPath {
                                ship: id,
                                actions: actions.clone(),
                                evasive: 0,
                                follow: false,
                            };
                            if self.commit_path(id, actions, 0).is_ok() {
                                applied.push(order);
                            }
                        }
                        None => break,
                    }
                }
                Phase::Firing => {
                    let next = self
                        .ships
                        .iter()
                        .find(|ship| {
                            !ship.destroyed
                                && self.is_v2_ai(ship.id)
                                && !self.has_committed_volley(ship.id)
                        })
                        .map(|ship| ship.id);
                    match next {
                        Some(id) => {
                            let shots = crate::ai::v2_volley(self, id);
                            let order = Order::CommitVolley {
                                ship: id,
                                shots: shots.clone(),
                            };
                            if self.commit_volley(id, shots).is_ok() {
                                applied.push(order);
                            }
                        }
                        None => break,
                    }
                }
            }
        }
        applied
    }
}
