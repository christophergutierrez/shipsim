//! Pure endpoint conflict resolver for protocol v4 paths (ADR-0025).
//!
//! Operates on precomputed path traces only. No `GameState` / commit storage.
//!
//! Algorithm: stationary ships permanently reserve their starts. Movers claim
//! destinations by descending path depth (primary, then reverse translated
//! history, then start). Non-stationary occupants of a claimed hex are
//! *evictable*: they re-enter the pending set and cascade down their own
//! chains. Stationary occupants never move. This guarantees termination with
//! a unique final hex per living ship.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::hex::Hex;
use crate::path::PathTrace;
use crate::prng::Prng;

/// Structured outcome for one ship after simultaneous path resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathResult {
    pub ship: u32,
    pub submitted_cost: u32,
    /// Hexes actually traveled from start to final (0 if final == start).
    pub translated_steps: u32,
    pub final_q: i32,
    pub final_r: i32,
    pub final_facing: u8,
    /// How many times this ship lost a claim and stepped back along its chain.
    pub fallback_steps: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_kind: Option<String>,
    pub conflicting_ships: Vec<u32>,
}

/// Input for one living ship.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathClaim {
    pub ship: u32,
    pub trace: PathTrace,
}

/// Preferred destination sequence: primary final, reverse unique translated
/// history, then the mover's unique starting hex as the guaranteed terminal.
/// The origin is always last exactly once — even when the path ends on or
/// loops through the start hex — so a looping mover can never lose its own
/// start as a last-resort claim.
fn fallback_chain(c: &PathClaim) -> Vec<Hex> {
    let start = c.trace.start.pos;
    let final_pos = c.trace.final_state.pos;
    let mut chain = Vec::new();
    let mut seen = BTreeSet::new();
    // Primary final first, but never treat origin as a mid-chain entry.
    if final_pos != start {
        chain.push(final_pos);
        seen.insert(final_pos);
    }
    for hex in c.trace.translated_positions.iter().rev() {
        if *hex == start {
            continue; // origin is the terminal candidate, not a mid-chain step
        }
        if seen.insert(*hex) {
            chain.push(*hex);
        }
    }
    // Always terminal, exactly once.
    chain.push(start);
    chain
}

/// Count actual path hexes between start and final along the submitted
/// translated history (not the requested length when fallback shortens travel).
fn actual_translated_steps(trace: &PathTrace, final_pos: Hex) -> u32 {
    if final_pos == trace.start.pos {
        return 0;
    }
    // Index of final_pos at the latest occurrence on the forward path. The
    // fallback chain walks history in reverse, so repeated positions resolve
    // to the latest visit, not the first one.
    for (i, hex) in trace.translated_positions.iter().enumerate().rev() {
        if *hex == final_pos {
            return (i + 1) as u32;
        }
    }
    // Final is not on the forward path (should not happen if final is on chain).
    // Count as distance along chain depth equivalent: 0 if start else submitted.
    if final_pos == trace.final_state.pos {
        trace.translated_positions.len() as u32
    } else {
        0
    }
}

/// Resolve all path claims. Mutates `prng` only for equal-cost ties.
///
/// Guarantees:
/// - every living ship gets a unique final hex;
/// - stationary ships keep their start hex;
/// - result is independent of claim insertion order (claims sorted by ship id).
pub fn resolve_paths(claims: &[PathClaim], prng: &mut Prng) -> Vec<PathResult> {
    let mut ordered: Vec<&PathClaim> = claims.iter().collect();
    ordered.sort_by_key(|c| c.ship);

    let by_id: BTreeMap<u32, &PathClaim> = ordered.iter().map(|c| (c.ship, *c)).collect();

    let mut stationary: BTreeSet<u32> = BTreeSet::new();
    let mut reserved: BTreeMap<Hex, u32> = BTreeMap::new();
    let mut assigned: BTreeMap<u32, Hex> = BTreeMap::new();
    let mut depth: HashMap<u32, usize> = HashMap::new();
    let mut fallback_steps: BTreeMap<u32, u32> = BTreeMap::new();
    let mut blocked_kind: BTreeMap<u32, Option<String>> = BTreeMap::new();
    let mut conflicts: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();

    for c in &ordered {
        fallback_steps.insert(c.ship, 0);
        blocked_kind.insert(c.ship, None);
        conflicts.insert(c.ship, BTreeSet::new());
        if c.trace.is_stationary() {
            stationary.insert(c.ship);
            let hex = c.trace.start.pos;
            reserved.insert(hex, c.ship);
            assigned.insert(c.ship, hex);
            depth.insert(c.ship, 0);
        } else {
            depth.insert(c.ship, 0);
        }
    }

    let mut pending: Vec<u32> = ordered
        .iter()
        .filter(|c| !c.trace.is_stationary())
        .map(|c| c.ship)
        .collect();

    let mut safety = 0usize;
    while !pending.is_empty() {
        safety += 1;
        assert!(
            safety < 50_000,
            "path resolution failed to terminate (cascading displacement)"
        );

        // Group pending ships by their current preferred destination.
        let mut groups: BTreeMap<Hex, Vec<u32>> = BTreeMap::new();
        for &id in &pending {
            let c = by_id[&id];
            let chain = fallback_chain(c);
            let d = depth[&id].min(chain.len().saturating_sub(1));
            groups.entry(chain[d]).or_default().push(id);
        }

        let mut dests: Vec<Hex> = groups.keys().copied().collect();
        dests.sort();

        let mut still_pending: Vec<u32> = Vec::new();
        // Hexes whose non-stationary occupant we already decided this pass.
        let mut settled_this_pass: BTreeSet<Hex> = BTreeSet::new();

        for dest in dests {
            let mut claimants = groups.remove(&dest).unwrap_or_default();
            claimants.sort_unstable();

            // Include current non-stationary occupant as a contestant (eviction).
            if let Some(&owner) = reserved.get(&dest) {
                if stationary.contains(&owner) {
                    // Permanent block — all claimants fall back.
                    for id in claimants {
                        record_conflict(&mut conflicts, id, owner);
                        step_back(
                            id,
                            &by_id,
                            &mut depth,
                            &mut fallback_steps,
                            &mut blocked_kind,
                            "occupied",
                        );
                        still_pending.push(id);
                    }
                    continue;
                }
                // Evictable mover: compete with claimants.
                if !claimants.contains(&owner) {
                    claimants.push(owner);
                    claimants.sort_unstable();
                }
            }

            if claimants.is_empty() {
                continue;
            }

            for &a in &claimants {
                for &b in &claimants {
                    if a != b {
                        record_conflict(&mut conflicts, a, b);
                    }
                }
            }

            // A mover that has exhausted every other fallback must retain its
            // own origin. Without this terminal priority, an earlier seeded
            // tie can be reversed by the exhausted-chain recovery below.
            // Starts are unique, so at most one claimant can qualify.
            let origin_fallback = claimants.iter().copied().find(|id| {
                let chain = fallback_chain(by_id[id]);
                depth[id] == chain.len().saturating_sub(1)
                    && chain.last().copied() == Some(by_id[id].trace.start.pos)
                    && dest == by_id[id].trace.start.pos
            });
            let winner = origin_fallback.unwrap_or_else(|| {
                if claimants.len() == 1 {
                    claimants[0]
                } else {
                    pick_winner(&claimants, &by_id, prng)
                }
            });

            // Free previous assignment of winner if relocating.
            if let Some(old) = assigned.get(&winner).copied() {
                if old != dest && reserved.get(&old) == Some(&winner) {
                    reserved.remove(&old);
                }
            }
            // Free previous occupant of dest if they lost.
            if let Some(&prev) = reserved.get(&dest) {
                if prev != winner {
                    reserved.remove(&dest);
                    assigned.remove(&prev);
                }
            }

            reserved.insert(dest, winner);
            assigned.insert(winner, dest);
            settled_this_pass.insert(dest);
            if depth[&winner] > 0 {
                blocked_kind.insert(winner, Some("fallback_win".into()));
            } else {
                blocked_kind.entry(winner).or_insert(None);
            }

            for id in claimants {
                if id == winner {
                    continue;
                }
                // Loser loses this hex; if they had it assigned, clear.
                if assigned.get(&id) == Some(&dest) {
                    assigned.remove(&id);
                }
                if reserved.get(&dest) == Some(&id) {
                    // winner already owns dest
                }
                step_back(
                    id,
                    &by_id,
                    &mut depth,
                    &mut fallback_steps,
                    &mut blocked_kind,
                    "contested",
                );
                still_pending.push(id);
            }
        }

        // Anyone still unassigned stays pending.
        for &id in &pending {
            if !assigned.contains_key(&id) && !still_pending.contains(&id) {
                still_pending.push(id);
            }
        }
        // Also: any non-stationary ship whose assignment was cleared mid-pass.
        for c in &ordered {
            if stationary.contains(&c.ship) {
                continue;
            }
            if !assigned.contains_key(&c.ship) && !still_pending.contains(&c.ship) {
                still_pending.push(c.ship);
            }
        }

        pending = still_pending;
        pending.sort_unstable();
        pending.dedup();

        // Progress check: if a ship is at max depth of chain and still contested
        // only by itself on free hex — assign it. Safety for start hex free.
        pending.retain(|&id| {
            if assigned.contains_key(&id) {
                return false;
            }
            let chain = fallback_chain(by_id[&id]);
            let d = depth[&id];
            if d >= chain.len() {
                // Exhausted formal chain — try any free hex on chain from start.
                for hex in chain.iter().rev() {
                    if let Some(&owner) = reserved.get(hex) {
                        if owner == id {
                            assigned.insert(id, *hex);
                            return false;
                        }
                        if stationary.contains(&owner) {
                            continue;
                        }
                        // Last-resort: compete once more by putting back at that depth
                        continue;
                    }
                    reserved.insert(*hex, id);
                    assigned.insert(id, *hex);
                    blocked_kind.insert(id, Some("exhausted_fallback".into()));
                    return false;
                }
                // Absolute last resort: this should only be reachable for a
                // pathological chain. Normal exhausted-origin claims win in
                // the group above before a seeded tie is drawn.
                let start = by_id[&id].trace.start.pos;
                if let Some(&owner) = reserved.get(&start) {
                    if !stationary.contains(&owner) && owner != id {
                        reserved.remove(&start);
                        assigned.remove(&owner);
                        // owner will re-enter via outer loop
                        depth.insert(owner, depth.get(&owner).copied().unwrap_or(0) + 1);
                        *fallback_steps.entry(owner).or_insert(0) += 1;
                        blocked_kind.insert(owner, Some("displaced".into()));
                        record_conflict(&mut conflicts, owner, id);
                        record_conflict(&mut conflicts, id, owner);
                    } else if stationary.contains(&owner) {
                        // Impossible: stationary on our start while we moved from it.
                        // Keep pending; safety assert will catch true impossibility.
                        return true;
                    }
                }
                reserved.insert(start, id);
                assigned.insert(id, start);
                blocked_kind.insert(id, Some("start_claim".into()));
                return false;
            }
            true
        });

        // Re-queue anyone who was force-displaced above.
        for c in &ordered {
            if !stationary.contains(&c.ship)
                && !assigned.contains_key(&c.ship)
                && !pending.contains(&c.ship)
            {
                pending.push(c.ship);
            }
        }
        pending.sort_unstable();
        pending.dedup();

        let _ = settled_this_pass;
    }

    // Build results in ship-id order.
    let mut results = Vec::with_capacity(ordered.len());
    for c in &ordered {
        let final_pos = assigned[&c.ship];
        let fb = fallback_steps.get(&c.ship).copied().unwrap_or(0);
        let mut conf: Vec<u32> = conflicts
            .get(&c.ship)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default();
        conf.sort_unstable();
        results.push(PathResult {
            ship: c.ship,
            submitted_cost: c.trace.cost,
            translated_steps: actual_translated_steps(&c.trace, final_pos),
            final_q: final_pos.q,
            final_r: final_pos.r,
            final_facing: c.trace.final_state.facing,
            fallback_steps: fb,
            blocked_kind: blocked_kind.get(&c.ship).cloned().flatten(),
            conflicting_ships: conf,
        });
    }

    // Final uniqueness invariant.
    let mut seen = BTreeSet::new();
    for r in &results {
        assert!(
            seen.insert((r.final_q, r.final_r)),
            "duplicate final hex after path resolution"
        );
    }
    results
}

fn step_back(
    id: u32,
    by_id: &BTreeMap<u32, &PathClaim>,
    depth: &mut HashMap<u32, usize>,
    fallback_steps: &mut BTreeMap<u32, u32>,
    blocked_kind: &mut BTreeMap<u32, Option<String>>,
    kind: &str,
) {
    let chain_len = fallback_chain(by_id[&id]).len();
    let d = depth.entry(id).or_insert(0);
    // Move one position beyond the formal chain when the last candidate is
    // also occupied. The next resolution pass uses the exhausted-chain
    // recovery path to displace a mover and continue the cascade. Clamping at
    // the last index causes an infinite re-contest of the same hex.
    *d = (*d + 1).min(chain_len);
    *fallback_steps.entry(id).or_insert(0) += 1;
    blocked_kind.insert(id, Some(kind.into()));
}

fn record_conflict(map: &mut BTreeMap<u32, BTreeSet<u32>>, ship: u32, other: u32) {
    map.entry(ship).or_default().insert(other);
}

fn pick_winner(claimants: &[u32], by_id: &BTreeMap<u32, &PathClaim>, prng: &mut Prng) -> u32 {
    let mut best_cost = 0u32;
    let mut best: Vec<u32> = Vec::new();
    for &id in claimants {
        let cost = by_id[&id].trace.cost;
        if cost > best_cost {
            best_cost = cost;
            best = vec![id];
        } else if cost == best_cost {
            best.push(id);
        }
    }
    best.sort_unstable();
    if best.len() == 1 {
        return best[0];
    }
    let idx = (prng.next_u64() as usize) % best.len();
    best[idx]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::{trace_path, PathAction, PathState};

    fn claim(ship: u32, start: PathState, actions: &[PathAction], budget: u32) -> PathClaim {
        let trace = trace_path(start, actions, budget, None).unwrap();
        PathClaim { ship, trace }
    }

    #[test]
    fn distinct_endpoints_never_interact() {
        let a = claim(
            1,
            PathState::new(Hex::new(0, 0), 0).unwrap(),
            &[PathAction::MoveF],
            1,
        );
        let b = claim(
            2,
            PathState::new(Hex::new(0, 2), 0).unwrap(),
            &[PathAction::MoveF],
            1,
        );
        let mut prng = Prng::new(1);
        let r = resolve_paths(&[a, b], &mut prng);
        assert_eq!(r[0].final_q, 1);
        assert_eq!(r[1].final_q, 1);
        assert_eq!(r[1].final_r, 2);
    }

    #[test]
    fn swaps_and_crossings_succeed() {
        let a = claim(
            1,
            PathState::new(Hex::new(0, 0), 0).unwrap(),
            &[PathAction::MoveF],
            1,
        );
        let b = claim(
            2,
            PathState::new(Hex::new(1, 0), 3).unwrap(),
            &[PathAction::MoveF],
            1,
        );
        let mut prng = Prng::new(1);
        let r = resolve_paths(&[a, b], &mut prng);
        assert_eq!((r[0].final_q, r[0].final_r), (1, 0));
        assert_eq!((r[1].final_q, r[1].final_r), (0, 0));
    }

    #[test]
    fn turn_only_ship_cannot_be_displaced() {
        let stationary = claim(
            1,
            PathState::new(Hex::new(1, 0), 0).unwrap(),
            &[PathAction::TurnRight, PathAction::TurnRight],
            2,
        );
        let mover = claim(
            2,
            PathState::new(Hex::new(0, 0), 0).unwrap(),
            &[PathAction::MoveF],
            1,
        );
        let mut prng = Prng::new(1);
        let r = resolve_paths(&[stationary, mover], &mut prng);
        assert_eq!((r[0].final_q, r[0].final_r), (1, 0));
        assert_eq!((r[1].final_q, r[1].final_r), (0, 0));
        assert!(r[1].fallback_steps >= 1);
        assert_eq!(r[1].translated_steps, 0);
    }

    #[test]
    fn higher_cost_wins_shared_endpoint() {
        let low = claim(
            1,
            PathState::new(Hex::new(0, 0), 0).unwrap(),
            &[PathAction::MoveF],
            1,
        );
        let high = claim(
            2,
            PathState::new(Hex::new(2, 0), 3).unwrap(),
            &[PathAction::MoveF, PathAction::TurnRight],
            2,
        );
        let mut prng = Prng::new(1);
        let r = resolve_paths(&[low, high], &mut prng);
        let p2 = r.iter().find(|x| x.ship == 2).unwrap();
        assert_eq!((p2.final_q, p2.final_r), (1, 0));
        let p1 = r.iter().find(|x| x.ship == 1).unwrap();
        assert_eq!((p1.final_q, p1.final_r), (0, 0));
        assert_eq!(p1.translated_steps, 0);
    }

    #[test]
    fn cascading_displacement_three_ship_chain() {
        // The panic case from review:
        // Ship 1 at (0,0) face 0: [move_f] → claims (1,0)
        // Ship 2 at (1,0) face 0: [move_f] → claims (2,0)
        // Ship 3 at (0,1) face 0: [move_f, move_fl] → (1,1) then (2,0) face 1
        // Ship 3 beats ship 2 for (2,0); ship 2 falls back to (1,0);
        // ship 1 must cascade to (0,0) so ship 2 can take (1,0) or vice versa.
        let s1 = claim(
            1,
            PathState::new(Hex::new(0, 0), 0).unwrap(),
            &[PathAction::MoveF],
            1,
        );
        let s2 = claim(
            2,
            PathState::new(Hex::new(1, 0), 0).unwrap(),
            &[PathAction::MoveF],
            1,
        );
        let s3 = claim(
            3,
            PathState::new(Hex::new(0, 1), 0).unwrap(),
            &[PathAction::MoveF, PathAction::MoveFl],
            2,
        );
        // Confirm ship 3 ends at (2,0)
        assert_eq!(s3.trace.final_state.pos, Hex::new(2, 0));

        for seed in 0..50u64 {
            let mut prng = Prng::new(seed);
            let r = resolve_paths(
                &[
                    claim(
                        1,
                        PathState::new(Hex::new(0, 0), 0).unwrap(),
                        &[PathAction::MoveF],
                        1,
                    ),
                    claim(
                        2,
                        PathState::new(Hex::new(1, 0), 0).unwrap(),
                        &[PathAction::MoveF],
                        1,
                    ),
                    claim(
                        3,
                        PathState::new(Hex::new(0, 1), 0).unwrap(),
                        &[PathAction::MoveF, PathAction::MoveFl],
                        2,
                    ),
                ],
                &mut prng,
            );
            let mut positions = BTreeSet::new();
            for x in &r {
                assert!(
                    positions.insert((x.final_q, x.final_r)),
                    "duplicate at seed {seed}: {r:?}"
                );
            }
            // Ship 3 always wins (2,0) by higher cost
            let p3 = r.iter().find(|x| x.ship == 3).unwrap();
            assert_eq!(
                (p3.final_q, p3.final_r),
                (2, 0),
                "ship 3 should keep (2,0) seed={seed}"
            );
            // Ship 2 has exhausted its chain at its own start. It retains
            // that hex deterministically rather than winning a random tie
            // that terminal recovery would later undo.
            let p1 = r.iter().find(|x| x.ship == 1).unwrap();
            let p2 = r.iter().find(|x| x.ship == 2).unwrap();
            assert_eq!((p1.final_q, p1.final_r), (0, 0));
            assert_eq!((p2.final_q, p2.final_r), (1, 0));
            assert_ne!(p2.blocked_kind.as_deref(), Some("start_claim"));
        }
        let _ = (s1, s2);
    }

    #[test]
    fn two_way_equal_cost_tie_fair_across_seeds() {
        let mut wins = [0u32; 2];
        for seed in 0..1000u64 {
            let a = claim(
                1,
                PathState::new(Hex::new(0, 0), 0).unwrap(),
                &[PathAction::MoveF],
                1,
            );
            let b = claim(
                2,
                PathState::new(Hex::new(2, 0), 3).unwrap(),
                &[PathAction::MoveF],
                1,
            );
            let mut prng = Prng::new(seed);
            let r = resolve_paths(&[a, b], &mut prng);
            let at_dest: Vec<_> = r
                .iter()
                .filter(|x| x.final_q == 1 && x.final_r == 0)
                .collect();
            assert_eq!(at_dest.len(), 1);
            if at_dest[0].ship == 1 {
                wins[0] += 1;
            } else {
                wins[1] += 1;
            }
        }
        for w in wins {
            assert!((450..=550).contains(&w), "win count {w} outside 45-55%");
        }
    }

    #[test]
    fn fixed_seed_exact_winner() {
        let a = claim(
            1,
            PathState::new(Hex::new(0, 0), 0).unwrap(),
            &[PathAction::MoveF],
            1,
        );
        let b = claim(
            2,
            PathState::new(Hex::new(2, 0), 3).unwrap(),
            &[PathAction::MoveF],
            1,
        );
        let mut prng = Prng::new(42);
        let r = resolve_paths(&[a, b], &mut prng);
        let state = prng.state();
        let winner = r
            .iter()
            .find(|x| x.final_q == 1 && x.final_r == 0)
            .unwrap()
            .ship;
        let a = claim(
            1,
            PathState::new(Hex::new(0, 0), 0).unwrap(),
            &[PathAction::MoveF],
            1,
        );
        let b = claim(
            2,
            PathState::new(Hex::new(2, 0), 3).unwrap(),
            &[PathAction::MoveF],
            1,
        );
        let mut prng2 = Prng::new(42);
        let r2 = resolve_paths(&[a, b], &mut prng2);
        let winner2 = r2
            .iter()
            .find(|x| x.final_q == 1 && x.final_r == 0)
            .unwrap()
            .ship;
        assert_eq!(winner, winner2);
        assert_eq!(prng2.state(), state);
    }

    #[test]
    fn three_way_ties_all_can_win() {
        let mut winners = BTreeSet::new();
        for seed in 0..300u64 {
            let claims = [
                claim(
                    1,
                    PathState::new(Hex::new(0, 0), 0).unwrap(),
                    &[PathAction::MoveF],
                    1,
                ),
                claim(
                    2,
                    PathState::new(Hex::new(2, 0), 3).unwrap(),
                    &[PathAction::MoveF],
                    1,
                ),
                claim(
                    3,
                    PathState::new(Hex::new(1, 1), 2).unwrap(),
                    &[PathAction::MoveF],
                    1,
                ),
            ];
            let mut prng = Prng::new(seed);
            let r = resolve_paths(&claims, &mut prng);
            let w = r
                .iter()
                .find(|x| x.final_q == 1 && x.final_r == 0)
                .unwrap()
                .ship;
            winners.insert(w);
            let mut positions = BTreeSet::new();
            for x in &r {
                assert!(positions.insert((x.final_q, x.final_r)));
            }
        }
        assert_eq!(winners, BTreeSet::from([1, 2, 3]));
    }

    #[test]
    fn insertion_order_independent() {
        let mk = || {
            [
                claim(
                    1,
                    PathState::new(Hex::new(0, 0), 0).unwrap(),
                    &[PathAction::MoveF],
                    1,
                ),
                claim(
                    2,
                    PathState::new(Hex::new(2, 0), 3).unwrap(),
                    &[PathAction::MoveF],
                    1,
                ),
            ]
        };
        let mut prng_a = Prng::new(99);
        let r_a = resolve_paths(&mk(), &mut prng_a);
        let mut prng_b = Prng::new(99);
        let rev = {
            let c = mk();
            vec![c[1].clone(), c[0].clone()]
        };
        let r_b = resolve_paths(&rev, &mut prng_b);
        assert_eq!(r_a, r_b);
        assert_eq!(prng_a.state(), prng_b.state());
    }

    #[test]
    fn looping_path_does_not_hang() {
        let actions = [
            PathAction::MoveF,
            PathAction::TurnRight,
            PathAction::TurnRight,
            PathAction::TurnRight,
            PathAction::MoveF,
        ];
        let a = claim(1, PathState::new(Hex::new(0, 0), 0).unwrap(), &actions, 5);
        let c = claim(
            3,
            PathState::new(Hex::new(0, 0), 1).unwrap(),
            &[PathAction::TurnLeft],
            1,
        );
        let mut prng = Prng::new(1);
        let r = resolve_paths(&[a, c], &mut prng);
        let mut positions = BTreeSet::new();
        for x in &r {
            assert!(positions.insert((x.final_q, x.final_r)));
        }
    }

    #[test]
    fn facings_from_full_path_even_after_fallback() {
        let mover = claim(
            1,
            PathState::new(Hex::new(0, 0), 0).unwrap(),
            &[PathAction::MoveF, PathAction::TurnRight],
            2,
        );
        let block = claim(2, PathState::new(Hex::new(1, 0), 0).unwrap(), &[], 0);
        let mut prng = Prng::new(1);
        let r = resolve_paths(&[mover, block], &mut prng);
        let p1 = r.iter().find(|x| x.ship == 1).unwrap();
        assert_eq!((p1.final_q, p1.final_r), (0, 0));
        assert_eq!(p1.final_facing, 5);
        assert_eq!(p1.translated_steps, 0);
    }

    #[test]
    fn translated_steps_zero_when_fallback_to_start() {
        let low = claim(
            1,
            PathState::new(Hex::new(0, 0), 0).unwrap(),
            &[PathAction::MoveF],
            1,
        );
        let high = claim(
            2,
            PathState::new(Hex::new(2, 0), 3).unwrap(),
            &[PathAction::MoveF, PathAction::TurnRight],
            2,
        );
        let mut prng = Prng::new(1);
        let r = resolve_paths(&[low, high], &mut prng);
        let p1 = r.iter().find(|x| x.ship == 1).unwrap();
        assert_eq!((p1.final_q, p1.final_r), (0, 0));
        assert_eq!(p1.translated_steps, 0);
        let p2 = r.iter().find(|x| x.ship == 2).unwrap();
        assert_eq!(p2.translated_steps, 1);
    }

    #[test]
    fn exhausted_fallback_chain_cascades_without_panicking() {
        // Ship 1 loses both its endpoint and its start to higher-cost movers.
        // Its exhausted chain must force a mover displacement, then allow the
        // displaced ship to fall back to its own free start.
        let claims = [
            claim(
                1,
                PathState::new(Hex::new(0, 0), 0).unwrap(),
                &[PathAction::MoveF],
                1,
            ),
            claim(
                2,
                PathState::new(Hex::new(2, 0), 3).unwrap(),
                &[PathAction::MoveF, PathAction::TurnRight],
                2,
            ),
            claim(
                3,
                PathState::new(Hex::new(-1, 0), 0).unwrap(),
                &[PathAction::MoveF, PathAction::TurnRight],
                2,
            ),
        ];
        let mut prng = Prng::new(1);
        let results = resolve_paths(&claims, &mut prng);
        let positions: BTreeSet<_> = results.iter().map(|r| (r.final_q, r.final_r)).collect();
        assert_eq!(positions.len(), claims.len());
        assert!(results.iter().all(|r| r.fallback_steps < 10));
    }

    #[test]
    fn looping_path_retains_origin_as_terminal_fallback() {
        // Mover A: out and back through origin, then continues to final (1,0).
        // Path: (0,0)→(1,0)→turn to face 3→(0,0 origin)→turn to face 0→(1,0 final).
        let a_actions = [
            PathAction::MoveF,     // (1,0) f0
            PathAction::TurnRight, // f1
            PathAction::TurnRight, // f2
            PathAction::TurnRight, // f3
            PathAction::MoveF,     // (0,0) origin revisit
            PathAction::TurnRight, // f4
            PathAction::TurnRight, // f5
            PathAction::TurnRight, // f0
            PathAction::MoveF,     // (1,0) final
        ];
        let a = claim(
            1,
            PathState::new(Hex::new(0, 0), 0).unwrap(),
            &a_actions,
            a_actions.len() as u32,
        );
        assert!(
            a.trace.translated_positions.contains(&Hex::new(0, 0)),
            "origin must appear mid-path: {:?}",
            a.trace.translated_positions
        );
        assert_ne!(a.trace.final_state.pos, Hex::new(0, 0));
        assert_eq!(a.trace.final_state.pos, Hex::new(1, 0));

        // Origin is terminal candidate exactly once.
        let chain = fallback_chain(&a);
        assert_eq!(
            *chain.last().unwrap(),
            Hex::new(0, 0),
            "origin must be terminal candidate: {chain:?}"
        );
        assert_eq!(
            chain.iter().filter(|h| **h == Hex::new(0, 0)).count(),
            1,
            "origin appears exactly once in chain: {chain:?}"
        );

        // Stationary blocker on A's primary final.
        let b = claim(2, PathState::new(Hex::new(1, 0), 0).unwrap(), &[], 0);
        // Higher-cost mover whose primary claim is A's origin.
        let c = claim(
            3,
            PathState::new(Hex::new(-1, 0), 0).unwrap(),
            &[PathAction::MoveF, PathAction::TurnRight],
            2,
        );
        assert_eq!(c.trace.final_state.pos, Hex::new(0, 0));

        // Also cover final == origin: origin must still be the chain terminal.
        let loop_home = [
            PathAction::MoveF,     // (1,0)
            PathAction::TurnRight, // f1
            PathAction::TurnRight, // f2
            PathAction::TurnRight, // f3
            PathAction::MoveF,     // (0,0) ends at origin
        ];
        let home = claim(
            9,
            PathState::new(Hex::new(0, 0), 0).unwrap(),
            &loop_home,
            loop_home.len() as u32,
        );
        assert_eq!(home.trace.final_state.pos, Hex::new(0, 0));
        let home_chain = fallback_chain(&home);
        assert_eq!(*home_chain.last().unwrap(), Hex::new(0, 0));
        assert_eq!(
            home_chain.iter().filter(|h| **h == Hex::new(0, 0)).count(),
            1
        );

        for seed in 0..20u64 {
            let mut prng = Prng::new(seed);
            let r = resolve_paths(
                &[
                    claim(
                        1,
                        PathState::new(Hex::new(0, 0), 0).unwrap(),
                        &a_actions,
                        a_actions.len() as u32,
                    ),
                    claim(2, PathState::new(Hex::new(1, 0), 0).unwrap(), &[], 0),
                    claim(
                        3,
                        PathState::new(Hex::new(-1, 0), 0).unwrap(),
                        &[PathAction::MoveF, PathAction::TurnRight],
                        2,
                    ),
                ],
                &mut prng,
            );
            let positions: BTreeSet<_> = r.iter().map(|x| (x.final_q, x.final_r)).collect();
            assert_eq!(positions.len(), 3, "unique finals seed={seed}: {r:?}");
            let p2 = r.iter().find(|x| x.ship == 2).unwrap();
            assert_eq!((p2.final_q, p2.final_r), (1, 0));
            let p1 = r.iter().find(|x| x.ship == 1).unwrap();
            // B locks A's primary final; A cost ≫ C, so A always retains origin.
            assert_eq!(
                (p1.final_q, p1.final_r),
                (0, 0),
                "A must retain origin seed={seed}: {r:?}"
            );
            assert_eq!(
                p1.translated_steps, 0,
                "origin landing has no net translation"
            );
            assert!(
                p1.fallback_steps >= 1,
                "A fell back from blocked final seed={seed}"
            );
            let p3 = r.iter().find(|x| x.ship == 3).unwrap();
            // C loses origin to A and falls back to its own start.
            assert_eq!((p3.final_q, p3.final_r), (-1, 0));
        }
        let _ = (a, b, c, home);
    }
}
