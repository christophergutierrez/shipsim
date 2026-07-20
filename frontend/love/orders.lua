-- Protocol-v4 order builders. Paths and volleys are complete simultaneous
-- commitments; there is no per-cycle maneuver, ready_fire, or end_turn.

local orders = {}
local json = require("json")
local PROTOCOL_VERSION = 4

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

--- Commit one complete path during movement.
function orders.commit_path(ship, actions)
  return versioned({
    type = "commit_path",
    ship = ship,
    actions = actions or {},
  })
end

--- Commit the locally drafted simultaneous volley.
function orders.commit_volley(ship, shots)
  return versioned({
    type = "commit_volley",
    ship = ship,
    shots = shots or {},
  })
end

return orders
