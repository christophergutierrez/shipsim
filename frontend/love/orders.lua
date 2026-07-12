-- v2 order builders (Combat Model v2, ADR-0020).
-- JSON shape must match src/movement.rs::Order (serde snake_case tags).

local orders = {}
local PROTOCOL_VERSION = 2

local function versioned(order)
  order.protocol_version = PROTOCOL_VERSION
  return order
end

--- Allocate power: movement points, weapon charges, shield facings.
--- weapons: map weapon_id -> charge level (1..3).
--- shields: array of 6 facings [F, FR, RR, R, RL, FL] -> power.
function orders.allocate(ship, movement, weapons, shields)
  return versioned({
    type = "allocate",
    ship = ship,
    movement = movement,
    weapons = weapons or {},
    shields = shields or { 0, 0, 0, 0, 0, 0 },
  })
end

--- Commit a maneuver for a ship during the current movement phase (ADR-0022 M6).
--- maneuver: a table like { type = "coast" }, { type = "accelerate", course = N }, etc.
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
