//! Shared read-only protocol-v4 request dispatch.
//!
//! The command-line harness and the network session both use this adapter so
//! preview validation and response shapes cannot drift between clients.

use serde_json::{json, Value};

use crate::game_state::GameState;
use crate::movement::Order;
use crate::path::PathAction;
use crate::protocol::PROTOCOL_VERSION;

pub fn dispatch(game: &GameState, value: &Value) -> Option<Value> {
    let request = value.get("request").and_then(Value::as_str)?;
    let request_id = value.get("request_id").cloned();
    let response = match request {
        "path_preview" => path_preview(game, value),
        "reach_preview" => reach_preview(game, value),
        "fire_preview" => fire_preview(game, value),
        "movement_preview" | "maneuver_options" => Err((
            "retired_request".into(),
            format!("{request} was removed in protocol v4; use path_preview / reach_preview"),
        )),
        other => Err((
            "unknown_request".into(),
            format!("unknown request: {other}"),
        )),
    };
    Some(with_request_id(response, request_id))
}

fn with_request_id(response: Result<Value, (String, String)>, request_id: Option<Value>) -> Value {
    match response {
        Ok(mut body) => {
            if let Some(id) = request_id {
                body["request_id"] = id;
            }
            body
        }
        Err((code, message)) => {
            let mut body = json!({
                "type": "error",
                "protocol_version": PROTOCOL_VERSION,
                "ok": false,
                "code": code,
                "message": message,
            });
            if let Some(id) = request_id {
                body["request_id"] = id;
            }
            body
        }
    }
}

fn ship(value: &Value, request: &str) -> Result<u32, (String, String)> {
    let Some(raw) = value.get("ship").and_then(Value::as_u64) else {
        return Err((
            "preview_invalid".into(),
            format!("{request} requires integer `ship`"),
        ));
    };
    u32::try_from(raw).map_err(|_| {
        (
            "preview_invalid".into(),
            format!("{request} `ship` is out of range"),
        )
    })
}

fn path_preview(game: &GameState, value: &Value) -> Result<Value, (String, String)> {
    let ship = ship(value, "path_preview")?;
    let mut actions = Vec::new();
    if let Some(raw) = value.get("actions") {
        let Some(items) = raw.as_array() else {
            return Err((
                "preview_invalid".into(),
                "path_preview actions must be an array".into(),
            ));
        };
        for item in items {
            let Some(name) = item.as_str() else {
                return Err((
                    "preview_invalid".into(),
                    "path_preview actions must be strings".into(),
                ));
            };
            let Some(action) = PathAction::parse(name) else {
                return Err((
                    "preview_invalid".into(),
                    format!("unknown path action: {name}"),
                ));
            };
            actions.push(action);
        }
    }
    let preview = game
        .path_preview(ship, &actions)
        .map_err(|e| ("preview_invalid".into(), e.to_string()))?;
    Ok(json!({
        "type": "path_preview", "protocol_version": PROTOCOL_VERSION, "ok": true,
        "ship": preview.ship, "cost": preview.cost, "remaining_motion": preview.remaining_motion,
        "final_q": preview.final_q, "final_r": preview.final_r, "final_facing": preview.final_facing,
        "steps": preview.steps, "error_index": preview.error_index, "error": preview.error,
    }))
}

fn reach_preview(game: &GameState, value: &Value) -> Result<Value, (String, String)> {
    let ship = ship(value, "reach_preview")?;
    let budget = value
        .get("budget")
        .and_then(Value::as_u64)
        .map(|v| u32::try_from(v).unwrap_or(u32::MAX));
    let endpoints = game
        .reach_preview(ship, budget)
        .map_err(|e| ("preview_invalid".into(), e.to_string()))?;
    Ok(json!({
        "type": "reach_preview", "protocol_version": PROTOCOL_VERSION, "ok": true,
        "ship": ship, "endpoints": endpoints,
    }))
}

fn fire_preview(game: &GameState, value: &Value) -> Result<Value, (String, String)> {
    let ship = ship(value, "fire_preview")?;
    let Some(weapon) = value.get("weapon").and_then(Value::as_str) else {
        return Err((
            "preview_invalid".into(),
            "fire_preview requires string `weapon`".into(),
        ));
    };
    let Some(raw_target) = value.get("target").and_then(Value::as_u64) else {
        return Err((
            "preview_invalid".into(),
            "fire_preview requires integer `target`".into(),
        ));
    };
    let target = u32::try_from(raw_target).map_err(|_| {
        (
            "preview_invalid".into(),
            "fire_preview `target` is out of range".into(),
        )
    })?;
    let body = match game.fire_decision_preview(ship, weapon, target) {
        Ok(preview) => json!({
            "type": "fire_preview", "protocol_version": PROTOCOL_VERSION, "ok": true, "legal": true,
            "ship": preview.ship, "weapon": preview.weapon, "target": preview.target,
            "range": preview.range, "threshold": preview.threshold, "die_sides": preview.die_sides,
            "hit_percent": preview.hit_percent, "projected_damage": preview.projected_damage,
            "legal_shield_facings": preview.legal_shield_facings,
        }),
        Err(error) => json!({
            "type": "fire_preview", "protocol_version": PROTOCOL_VERSION, "ok": true, "legal": false,
            "ship": ship, "weapon": weapon, "target": target, "reason": error.to_string(),
        }),
    };
    Ok(body)
}

pub fn parse_order(value: &Value) -> Result<Order, String> {
    serde_json::from_value(value.clone()).map_err(|e| e.to_string())
}
