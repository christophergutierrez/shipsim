//! Turn orchestration: NPC orders, 32-impulse move + impulse-gated simultaneous fire.

use std::collections::{HashMap, HashSet};

use crate::ai;
use crate::game_state::{GameState, NpcController};
use crate::hex::Hex;
use crate::impulse::{max_plot_steps, moves_on_impulse};
use crate::movement::{self, Order};

/// Resolve a full turn: NPC plot/fire, 32 impulses, terminals, advance turn.
pub fn run_turn(game: &mut GameState) {
    ensure_npc_orders(game);

    for impulse in 1u8..=32 {
        game.set_impulse(impulse);
        resolve_impulse_movement(game, impulse);
        game.advance_seeking_munitions();
        resolve_fires_on_impulse(game, impulse);
    }

    game.set_impulse(0);
    game.discard_pending_fires();
    game.sync_scripted_waypoints();
    game.clear_turn_ephemera();
    game.reset_all_turn_energy();
    game.refresh_status();
    game.advance_turn_counter();
}

fn resolve_impulse_movement(game: &mut GameState, impulse: u8) {
    let mut intents: Vec<(u32, Hex, u8)> = Vec::new();

    for ship_id in game.ship_ids() {
        let Some(ship) = game.ship(ship_id) else {
            continue;
        };
        if ship.destroyed {
            continue;
        }
        let speed = ship.turn_speed.min(31) as u8;
        if !moves_on_impulse(speed, impulse) {
            continue;
        }
        let Some((next, facing)) = game.plot_next_step(ship_id) else {
            continue;
        };
        intents.push((ship_id, next, facing));
    }

    let mut post = game.alive_positions();
    for (ship_id, next, _) in &intents {
        post.insert(*ship_id, *next);
    }

    let mut occupancy: HashMap<Hex, u32> = HashMap::new();
    for hex in post.values() {
        *occupancy.entry(*hex).or_insert(0) += 1;
    }

    let mut failed: HashSet<u32> = HashSet::new();
    for (ship_id, next, _) in &intents {
        if occupancy.get(next).copied().unwrap_or(0) > 1 {
            failed.insert(*ship_id);
        }
    }

    for (ship_id, next, facing) in intents {
        if failed.contains(&ship_id) {
            game.abort_plot(ship_id);
            continue;
        }
        game.apply_ship_step(ship_id, next, facing);
        game.advance_plot_cursor(ship_id);
    }
}

fn resolve_fires_on_impulse(game: &mut GameState, impulse: u8) {
    let ready = game.drain_fires_for_impulse(impulse);
    game.resolve_simultaneous_fires(ready);
}

fn ensure_npc_orders(game: &mut GameState) {
    let npc_ids = game.npc_ids();
    for ship_id in npc_ids {
        if game.ship(ship_id).is_some_and(|s| s.destroyed) {
            continue;
        }
        // Auto-plot if the player did not submit one for this NPC.
        if !game.has_plot(ship_id) {
            let path = match game.npc(ship_id) {
                Some(NpcController::Scripted(_)) => generate_scripted_plot(game, ship_id),
                Some(NpcController::GreedySeek) => ai::greedy_plot(game, ship_id),
                None => Vec::new(),
            };
            game.store_plot(ship_id, path);
        }
        // AI opportunistic fire (scripted ships stay silent unless later extended).
        if matches!(game.npc(ship_id), Some(NpcController::GreedySeek)) {
            if let Some((weapon, target)) = ai::choose_fire(game, ship_id) {
                let _ = movement::apply_order(
                    game,
                    Order::Fire {
                        ship: ship_id,
                        weapon,
                        target,
                    },
                );
            }
        }
    }
}

fn generate_scripted_plot(game: &mut GameState, ship_id: u32) -> Vec<Hex> {
    let Some(ship) = game.ship(ship_id).cloned() else {
        return Vec::new();
    };
    if ship.destroyed {
        return Vec::new();
    }
    let max_steps = max_plot_steps(ship.turn_speed);
    let Some((next_wp, waypoints)) = game.scripted_waypoint_view(ship_id) else {
        return Vec::new();
    };
    // Resolve current waypoint target (clone waypoints for path builder).
    let waypoints = waypoints.to_vec();
    let mut idx = next_wp;
    let pos = ship.pos;
    while waypoints.get(idx).is_some_and(|w| *w == pos) {
        idx += 1;
    }
    let Some(goal) = waypoints.get(idx).copied() else {
        return Vec::new();
    };
    ai::build_path_toward(game, ship_id, goal, max_steps)
}
