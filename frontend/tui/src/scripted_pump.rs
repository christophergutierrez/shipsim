//! Passive orders for `scripted`-controller ships (protocol v4).
//!
//! Mirrors `frontend/repl/repl.py::plan_scripted_orders`: only advance when the
//! current collection stage is blocked solely on scripted ships. Never drives
//! `player` or `ai` controllers.

use crate::protocol::{Order, Snapshot};

/// Passive orders for scripted ships when the current stage is blocked ONLY on
/// scripted ships. Returns empty if any living player ship is still pending in
/// the stage, if the game is over, or if there is nothing to do. Never returns
/// orders for `player` or `ai` controllers.
pub fn plan_scripted_orders(snap: &Snapshot) -> Vec<Order> {
    if snap.is_over() {
        return Vec::new();
    }
    let living = || snap.ships.iter().filter(|s| !s.destroyed);
    let done: &[i64] = match snap.phase.as_str() {
        "allocate" => &snap.ships_allocated_this_turn,
        "movement" => &snap.ships_committed_path,
        "firing" => &snap.ships_committed_volley,
        _ => return Vec::new(),
    };
    // Barrier: do nothing while any living player ship is still pending.
    let player_pending = living().any(|s| s.controller == "player" && !done.contains(&s.id));
    if player_pending {
        return Vec::new();
    }
    living()
        .filter(|s| s.controller == "scripted" && !done.contains(&s.id))
        .map(|s| match snap.phase.as_str() {
            "allocate" => Order::passive_allocate(s.id),
            "movement" => Order::commit_path(s.id, Vec::new()),
            _ => Order::hold_fire(s.id), // firing
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::OrderBody;

    fn snap_from(json: &str) -> Snapshot {
        serde_json::from_str(json).expect("test snapshot")
    }

    #[test]
    fn plan_returns_empty_while_player_pending() {
        let snap = snap_from(
            r#"{
            "protocol_version": 4,
            "turn": 1,
            "status": "InProgress",
            "phase": "allocate",
            "ships_allocated_this_turn": [],
            "ships_committed_path": [],
            "ships_committed_volley": [],
            "map": {"width": 4, "height": 4, "mode": "hard"},
            "ships": [
                {"id":1,"class":"Heavy Cruiser","size":2,"controller":"player",
                 "q":1,"r":0,"facing":0,"power":10,"power_available":10,
                 "structure":10,"destroyed":false,"weapons":[]},
                {"id":2,"class":"Escort","size":1,"controller":"scripted",
                 "q":0,"r":0,"facing":0,"power":8,"power_available":8,
                 "structure":8,"destroyed":false,"weapons":[]}
            ],
            "combat_log": []
        }"#,
        );
        assert!(plan_scripted_orders(&snap).is_empty());
    }

    #[test]
    fn plan_allocate_omits_weapons() {
        let snap = snap_from(
            r#"{
            "protocol_version": 4,
            "turn": 1,
            "status": "InProgress",
            "phase": "allocate",
            "ships_allocated_this_turn": [1],
            "ships_committed_path": [],
            "ships_committed_volley": [],
            "map": {"width": 4, "height": 4, "mode": "hard"},
            "ships": [
                {"id":1,"class":"Heavy Cruiser","size":2,"controller":"player",
                 "q":1,"r":0,"facing":0,"power":10,"power_available":10,
                 "structure":10,"destroyed":false,"weapons":[]},
                {"id":2,"class":"Escort","size":1,"controller":"scripted",
                 "q":0,"r":0,"facing":0,"power":8,"power_available":8,
                 "structure":8,"destroyed":false,"weapons":[]}
            ],
            "combat_log": []
        }"#,
        );
        let orders = plan_scripted_orders(&snap);
        assert_eq!(orders.len(), 1);
        let json = orders[0].to_json();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "allocate");
        assert_eq!(v["ship"], 2);
        assert_eq!(v["movement"], 0);
        assert!(
            v.get("weapons").is_none(),
            "weapons key must be omitted: {json}"
        );
        assert_eq!(v["shields"], serde_json::json!([0, 0, 0, 0, 0, 0]));
    }

    #[test]
    fn plan_movement_and_firing_are_empty_orders() {
        let move_snap = snap_from(
            r#"{
            "protocol_version": 4,
            "turn": 1,
            "status": "InProgress",
            "phase": "movement",
            "ships_allocated_this_turn": [1, 2],
            "ships_committed_path": [1],
            "ships_committed_volley": [],
            "map": {"width": 4, "height": 4, "mode": "hard"},
            "ships": [
                {"id":1,"class":"Heavy Cruiser","size":2,"controller":"player",
                 "q":1,"r":0,"facing":0,"power":10,"power_available":10,
                 "structure":10,"destroyed":false,"weapons":[]},
                {"id":2,"class":"Escort","size":1,"controller":"scripted",
                 "q":0,"r":0,"facing":0,"power":8,"power_available":8,
                 "structure":8,"destroyed":false,"weapons":[]}
            ],
            "combat_log": []
        }"#,
        );
        let orders = plan_scripted_orders(&move_snap);
        assert_eq!(orders.len(), 1);
        match &orders[0].body {
            OrderBody::CommitPath { ship, actions } => {
                assert_eq!(*ship, 2);
                assert!(actions.is_empty());
            }
            other => panic!("expected empty commit_path, got {other:?}"),
        }

        let fire_snap = snap_from(
            r#"{
            "protocol_version": 4,
            "turn": 1,
            "status": "InProgress",
            "phase": "firing",
            "ships_allocated_this_turn": [1, 2],
            "ships_committed_path": [1, 2],
            "ships_committed_volley": [1],
            "map": {"width": 4, "height": 4, "mode": "hard"},
            "ships": [
                {"id":1,"class":"Heavy Cruiser","size":2,"controller":"player",
                 "q":1,"r":0,"facing":0,"power":10,"power_available":10,
                 "structure":10,"destroyed":false,"weapons":[]},
                {"id":2,"class":"Escort","size":1,"controller":"scripted",
                 "q":0,"r":0,"facing":0,"power":8,"power_available":8,
                 "structure":8,"destroyed":false,"weapons":[]}
            ],
            "combat_log": []
        }"#,
        );
        let orders = plan_scripted_orders(&fire_snap);
        assert_eq!(orders.len(), 1);
        match &orders[0].body {
            OrderBody::CommitVolley { ship, shots } => {
                assert_eq!(*ship, 2);
                assert!(shots.is_empty());
            }
            other => panic!("expected empty commit_volley, got {other:?}"),
        }
    }

    #[test]
    fn plan_never_selects_ai() {
        let snap = snap_from(
            r#"{
            "protocol_version": 4,
            "turn": 1,
            "status": "InProgress",
            "phase": "allocate",
            "ships_allocated_this_turn": [1],
            "ships_committed_path": [],
            "ships_committed_volley": [],
            "map": {"width": 4, "height": 4, "mode": "hard"},
            "ships": [
                {"id":1,"class":"Heavy Cruiser","size":2,"controller":"player",
                 "q":1,"r":0,"facing":0,"power":10,"power_available":10,
                 "structure":10,"destroyed":false,"weapons":[]},
                {"id":2,"class":"Escort","size":1,"controller":"ai",
                 "q":0,"r":0,"facing":0,"power":8,"power_available":8,
                 "structure":8,"destroyed":false,"weapons":[]},
                {"id":3,"class":"Escort","size":1,"controller":"scripted",
                 "q":2,"r":0,"facing":0,"power":8,"power_available":8,
                 "structure":8,"destroyed":false,"weapons":[]}
            ],
            "combat_log": []
        }"#,
        );
        let orders = plan_scripted_orders(&snap);
        assert_eq!(orders.len(), 1);
        match &orders[0].body {
            OrderBody::Allocate { ship, .. } => assert_eq!(*ship, 3),
            other => panic!("expected allocate for scripted only, got {other:?}"),
        }
    }
}
