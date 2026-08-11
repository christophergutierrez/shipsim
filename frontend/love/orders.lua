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
--- Optional `evasive` spends motion points from the same budget as path actions.
function orders.commit_path(ship, actions, evasive)
  local order = {
    type = "commit_path",
    ship = ship,
    actions = actions or {},
  }
  if evasive and evasive > 0 then
    order.evasive = evasive
  end
  return versioned(order)
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
