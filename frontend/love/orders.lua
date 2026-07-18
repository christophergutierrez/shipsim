-- v3 order builders (Combat Model v2, ADR-0020 + ADR-0022 M4/M6 motion).
-- JSON shape must match src/movement.rs::Order (serde snake_case tags).
-- Maneuver variants match src/motion.rs::Maneuver: coast, accel, turn{facing}, turn_accel{facing}.

local orders = {}
local json = require("json")
local PROTOCOL_VERSION = 3

local function versioned(order)
  order.protocol_version = PROTOCOL_VERSION
  return order
end

--- Allocate power: movement points, weapon charges, shield facings.
--- weapons: map weapon_id -> charge level (must be a JSON object, never []).
--- shields: array of 6 facings [F, FR, RR, R, RL, FL] -> power.
function orders.allocate(ship, movement, weapons, shields)
  -- Engine deserializes weapons as BTreeMap — empty Lua {} encodes as []
  -- unless tagged as a JSON object (see json.object).
  return versioned({
    type = "allocate",
    ship = ship,
    movement = movement,
    weapons = json.object(weapons or {}),
    shields = shields or { 0, 0, 0, 0, 0, 0 },
  })
end

--- Commit a maneuver for a ship during the current movement phase (ADR-0022 M4/M6).
--- maneuver: { type = "coast" } | { type = "accel" } |
---            { type = "turn", facing = N } | { type = "turn_accel", facing = N }
--- (facing is 0..5; matches src/motion.rs::Maneuver, serde snake_case).
function orders.commit_maneuver(ship, maneuver)
  return versioned({
    type = "commit_maneuver",
    ship = ship,
    maneuver = maneuver,
  })
end

--- Coast is the common case: commit Maneuver::Coast (no thrust spent).
function orders.coast(ship)
  return orders.commit_maneuver(ship, { type = "coast" })
end

--- Accel: thrust along current facing (Maneuver::Accel).
function orders.accel(ship)
  return orders.commit_maneuver(ship, { type = "accel" })
end

--- Turn hull to absolute facing 0..5 (Maneuver::Turn). Cost = ring distance.
function orders.turn(ship, facing)
  return orders.commit_maneuver(ship, { type = "turn", facing = facing })
end

--- Turn to facing then accel from the new facing (Maneuver::TurnAccel).
function orders.turn_accel(ship, facing)
  return orders.commit_maneuver(ship, { type = "turn_accel", facing = facing })
end

--- Commit a charged weapon to fire at a target through a shield facing.
--- shield_facing: 0..5 hex facing the shot strikes.
function orders.commit_fire(ship, weapon, target, shield_facing)
  return versioned({
    type = "commit_fire",
    ship = ship,
    weapon = weapon,
    target = target,
    shield_facing = shield_facing,
  })
end

--- Mark a ship as having fired all its committed weapons this turn.
function orders.ready_fire(ship)
  return versioned({ type = "ready_fire", ship = ship })
end

--- End the turn. Legal after Allocate; core owns the warning flag.
function orders.end_turn()
  return versioned({ type = "end_turn" })
end

return orders
