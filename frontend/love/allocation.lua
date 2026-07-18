local allocation = {}

function allocation.increment(value, maximum)
  return math.min(maximum or 0, (value or 0) + 1)
end

function allocation.decrement(value)
  return math.max(0, (value or 0) - 1)
end

--- Power the engine would spend for this desired allocation draft.
--- Weapon entries are desired charge totals; only charge above the carried
--- snapshot amount costs power this turn.
function allocation.power_spent(ship, draft)
  local total = (draft and draft.movement) or 0
  local carried = {}
  for _, weapon in ipairs((ship and ship.weapons) or {}) do
    carried[weapon.id] = weapon.charge or 0
  end
  for weapon_id, desired in pairs((draft and draft.weapons) or {}) do
    total = total + math.max(0, (desired or 0) - (carried[weapon_id] or 0))
  end
  for _, shield in ipairs((draft and draft.shields) or {}) do
    total = total + (shield or 0)
  end
  return total
end

--- Remaining affordable movement after the other draft buckets are held fixed.
function allocation.available_for_movement(ship, draft)
  local movement = (draft and draft.movement) or 0
  local power = (ship and ship.power_available) or (ship and ship.power) or 0
  return math.max(0, power - (allocation.power_spent(ship, draft) - movement))
end

local function power_available(ship)
  return (ship and ship.power_available) or (ship and ship.power) or 0
end

--- Fill weapon charge in display order without spending power already drafted
--- for movement or shields. The resulting draft is always affordable.
function allocation.maximize_weapons(ship, draft)
  local shield_cost = 0
  for _, shield in ipairs(draft.shields or {}) do
    shield_cost = shield_cost + shield
  end
  local remaining = math.max(0, power_available(ship) - (draft.movement or 0) - shield_cost)
  draft.weapons = {}
  for _, weapon in ipairs((ship and ship.weapons) or {}) do
    local carried = weapon.charge or 0
    local add = math.min(math.max(0, (weapon.max_charge or 0) - carried), remaining)
    draft.weapons[weapon.id] = carried + add
    remaining = remaining - add
  end
end

--- Spread the power left after movement and weapons evenly over shield faces.
function allocation.balance_shields(ship, draft)
  local without_shields = allocation.power_spent(ship, {
    movement = draft.movement,
    weapons = draft.weapons,
    shields = { 0, 0, 0, 0, 0, 0 },
  })
  local per = math.floor(math.max(0, power_available(ship) - without_shields) / 6)
  per = math.min(per, (ship and ship.max_shield_per_facing) or per)
  draft.shields = { per, per, per, per, per, per }
end

--- Reserve all remaining power for movement, clearing shields first.
function allocation.all_engine(ship, draft)
  draft.shields = { 0, 0, 0, 0, 0, 0 }
  draft.movement = 0
  draft.movement = allocation.available_for_movement(ship, draft)
end

--- Set movement to a fraction of the power left after weapons and shields.
function allocation.set_movement_fraction(ship, draft, fraction)
  draft.movement = 0
  local available = allocation.available_for_movement(ship, draft)
  draft.movement = math.floor(available * math.max(0, math.min(1, fraction or 0)) + 0.5)
end

return allocation
