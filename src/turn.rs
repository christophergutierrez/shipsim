//! Turn orchestration: 32-impulse simultaneous movement, scripted auto-plot, deferred fire.
//!
//! Depends downward on `GameState` storage, pure `impulse` schedule, `movement` plot validation,
//! and pure `combat` legality (damage applied via GameState::apply_fire).

use std::collections::{HashMap, HashSet};

use crate::game_state::GameState;
use crate::hex::Hex;
use crate::impulse::{max_plot_steps, moves_on_impulse};
use crate::movement;

/// Resolve a full turn: scripted plots, 32 impulses (move then impulse-gated fire), advance turn.
pub fn run_turn(game: &mut GameState) {
    ensure_scripted_plots(game);

    for impulse in 1u8..=32 {
        game.set_impulse(impulse);
        resolve_impulse_movement(game, impulse);
        resolve_fires_on_impulse(game, impulse);
    }

    game.set_impulse(0);
    // Any fire that never matched a window is dropped (all shipped weapons hit impulse 32).
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
    // D2-fire: all ready shots this impulse compute from a frozen pre-fire state, then apply.
    // Tie-break / PRNG order: ascending ship id (then weapon, target).
    game.resolve_simultaneous_fires(ready);
}

fn ensure_scripted_plots(game: &mut GameState) {
    let scripted_ids = game.scripted_ship_ids();
    for ship_id in scripted_ids {
        if game.has_plot(ship_id) {
            continue;
        }
        let path = generate_scripted_plot(game, ship_id);
        game.store_plot(ship_id, path);
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
    let mut path: Vec<Hex> = Vec::new();
    let mut pos = ship.pos;
    let mut waypoints_advanced = 0usize;

    while (path.len() as u32) < max_steps {
        let target = {
            let Some((next_wp, waypoints)) = game.scripted_waypoint_view(ship_id) else {
                break;
            };
            let mut idx = next_wp + waypoints_advanced;
            while waypoints.get(idx).is_some_and(|w| *w == pos) {
                waypoints_advanced += 1;
                idx = next_wp + waypoints_advanced;
            }
            match waypoints.get(idx).copied() {
                Some(t) => t,
                None => break,
            }
        };

        if pos.distance(target) == 0 {
            break;
        }

        let next = if pos.distance(target) == 1
            && game.board().contains(target)
            && !game.is_occupied_by_other(ship_id, target)
            && !path.contains(&target)
        {
            Some(target)
        } else {
            let mut best: Option<(u8, Hex)> = None;
            for facing in 0u8..=5 {
                let Some(delta) = Hex::direction(facing) else {
                    continue;
                };
                let candidate = pos + delta;
                if !game.board().contains(candidate) {
                    continue;
                }
                if game.is_occupied_by_other(ship_id, candidate) {
                    continue;
                }
                if path.contains(&candidate) {
                    continue;
                }
                if candidate.distance(target) >= pos.distance(target) {
                    continue;
                }
                match best {
                    None => best = Some((facing, candidate)),
                    Some((bf, _)) if facing < bf => best = Some((facing, candidate)),
                    _ => {}
                }
            }
            best.map(|(_, hex)| hex)
        };

        let Some(step) = next else {
            break;
        };
        path.push(step);
        if step == target {
            waypoints_advanced += 1;
        }
        pos = step;
    }

    while !path.is_empty() {
        if movement::validate_plot(game, ship_id, &path).is_ok() {
            break;
        }
        path.pop();
    }

    let _ = waypoints_advanced;
    path
}
