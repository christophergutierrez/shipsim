//! M4: Movement boundary rules (ADR-0022) — course changes at most 60 degrees
//! per phase, and a full-speed reversal cannot happen in a single phase.

use std::collections::BTreeMap;
use std::path::PathBuf;

use shipsim_core::game_state::GameState;
use shipsim_core::motion::Maneuver;
use shipsim_core::movement::{apply_order, Order};
use shipsim_core::scenario::load_scenario;

const ALL_SHIPS: [u32; 5] = [1, 2, 3, 4, 5];

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn load() -> GameState {
    load_scenario(&manifest_path("scenarios/m4_movement.toml")).expect("m4_movement loads")
}

fn allocate_all(game: &mut GameState, movement: u32) {
    for ship in ALL_SHIPS {
        apply_order(
            game,
            Order::Allocate {
                ship,
                movement,
                weapons: BTreeMap::new(),
                shields: [0; 6],
            },
        )
        .expect("allocate");
    }
}

/// Commit `maneuver` for `ship` and `Maneuver::Coast` for every other ship,
/// then ready every ship's (empty) fire commitment — resolving the current
/// phase's maneuvers, translation, and fire window (ADR-0022 M5) into the
/// next movement phase.
fn resolve_phase_with(game: &mut GameState, ship: u32, maneuver: Maneuver) {
    for other in ALL_SHIPS {
        let m = if other == ship {
            maneuver
        } else {
            Maneuver::Coast
        };
        apply_order(
            game,
            Order::CommitManeuver {
                ship: other,
                maneuver: m,
            },
        )
        .expect("commit");
    }
    for ship in ALL_SHIPS {
        apply_order(game, Order::ReadyFire { ship }).expect("ready fire");
    }
}

#[test]
fn course_changes_by_at_most_one_step_per_phase() {
    // Ship 4 starts on course 0. A single TurnCourseStarboard/Port commit can
    // only rotate course by one hex direction (60 degrees) — never more —
    // because `Maneuver` offers no multi-step course change and each ship
    // commits exactly one maneuver per phase.
    let mut game = load();
    // Course-turn cost equals current speed (ship 4 starts at velocity 3), so
    // two turns cost 3 thrust each; allocate enough for both.
    allocate_all(&mut game, 8);
    let before = game.ship(4).unwrap().velocity.course;

    resolve_phase_with(&mut game, 4, Maneuver::TurnCourseStarboard);
    let after_one_phase = game.ship(4).unwrap().velocity.course;
    assert_eq!(
        (after_one_phase + 6 - before) % 6,
        5,
        "one commit changes course by exactly one step (60 degrees)"
    );

    resolve_phase_with(&mut game, 4, Maneuver::TurnCourseStarboard);
    let after_two_phases = game.ship(4).unwrap().velocity.course;
    assert_eq!(
        (after_two_phases + 6 - after_one_phase) % 6,
        5,
        "the next phase's commit is a separate one-step change, not a bigger jump"
    );
}

#[test]
fn speed_four_reversal_cannot_occur_in_one_phase() {
    // Ship 5 starts at velocity 4, course 0. Reversing to the opposite course
    // at speed 4 requires decelerating to zero (four Decelerate commits, one
    // per phase since each commit costs one phase) before a stopped
    // Accelerate can select the opposite course — it cannot happen within a
    // single phase's one maneuver commit.
    let mut game = load();
    allocate_all(&mut game, 4);
    assert_eq!(game.ship(5).unwrap().velocity.speed, 4);
    assert_eq!(game.ship(5).unwrap().velocity.course, 0);

    resolve_phase_with(&mut game, 5, Maneuver::Decelerate);
    let ship5 = game.ship(5).unwrap();
    assert_eq!(
        ship5.velocity.speed, 3,
        "a single phase's maneuver only steps speed by one"
    );
    assert_eq!(
        ship5.velocity.course, 0,
        "course is unchanged while still moving; reversal needs a stop first"
    );

    // Reaching a full stop from speed 4 takes four separate Decelerate
    // commits — one per phase — which exhausts this turn's four movement
    // phases entirely (a single phase can never contain the whole reversal).
    for _ in 0..3 {
        resolve_phase_with(&mut game, 5, Maneuver::Decelerate);
    }
    assert_eq!(game.ship(5).unwrap().velocity.speed, 0);
    assert_eq!(
        game.phase_name(),
        "turn_end",
        "decelerating to zero consumed this turn's four movement phases"
    );

    // Only in a fresh turn can the stopped ship select the opposite course
    // and begin accelerating — confirming the reversal spans a turn boundary,
    // not a single phase.
    apply_order(&mut game, Order::EndTurn).expect("end turn");
    allocate_all(&mut game, 4);
    resolve_phase_with(&mut game, 5, Maneuver::Accelerate { course: Some(3) });
    let ship5 = game.ship(5).unwrap();
    assert_eq!(ship5.velocity.speed, 1);
    assert_eq!(ship5.velocity.course, 3, "opposite of course 0");
}
