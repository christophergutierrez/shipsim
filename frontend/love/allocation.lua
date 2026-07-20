local allocation = {}

function allocation.increment(value, maximum)
  return math.min(maximum or 0, (value or 0) + 1)
end

function allocation.decrement(value)
  return math.max(0, (value or 0) - 1)
end

local function power_available(ship)
  return (ship and ship.power_available) or (ship and ship.power) or 0
end

--- Live carried charge from the engine snapshot (not the local draft).
function allocation.carried_charge(ship, weapon_id)
  for _, weapon in ipairs((ship and ship.weapons) or {}) do
    if weapon.id == weapon_id then
      return weapon.charge or 0
    end
  end
  return 0
end

--- Seed/refresh draft weapon totals from snapshot charge.
--- Desired totals must start at carried so Allocate never tries to strip,
--- and so the HUD never shows 0 while the ship still banks charge.
function allocation.seed_weapons(ship, draft)
  draft.weapons = draft.weapons or {}
  for _, weapon in ipairs((ship and ship.weapons) or {}) do
    local carried = weapon.charge or 0
    local current = draft.weapons[weapon.id]
    -- Raise a stale-low draft up to carried; never invent a lower total.
    if current == nil or current < carried then
      draft.weapons[weapon.id] = carried
    end
  end
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
  local power = power_available(ship)
  return math.max(0, power - (allocation.power_spent(ship, draft) - movement))
end

--- Raise one weapon desired-total by 1, floored at carried, capped by max_charge
--- and remaining power (only top-ups spend the pool).
function allocation.weapon_up(ship, draft, weapon_id, max_charge)
  allocation.seed_weapons(ship, draft)
  local carried = allocation.carried_charge(ship, weapon_id)
  local current = draft.weapons[weapon_id] or carried
  if current < carried then
    current = carried
  end
  local spent = allocation.power_spent(ship, draft)
  local free = math.max(0, power_available(ship) - spent)
  local cap = math.min(max_charge or current, current + free)
  -- At least allow staying at current; never below carried.
  cap = math.max(cap, current)
  draft.weapons[weapon_id] = allocation.increment(current, cap)
  return draft.weapons[weapon_id]
end

--- Lower one weapon desired-total by 1, but never below carried charge.
function allocation.weapon_down(ship, draft, weapon_id)
  allocation.seed_weapons(ship, draft)
  local carried = allocation.carried_charge(ship, weapon_id)
  local current = draft.weapons[weapon_id] or carried
  draft.weapons[weapon_id] = math.max(carried, (current or 0) - 1)
  return draft.weapons[weapon_id]
end

--- Fill weapon charge in display order without spending power already drafted
--- for movement or shields. The resulting draft is always affordable.
function allocation.maximize_weapons(ship, draft)
  local shield_cost = 0
  for _, shield in ipairs(draft.shields or {}) do
    shield_cost = shield_cost + shield
  end
  local remaining = math.max(0, power_available(ship) - (draft.movement or 0) - shield_cost)
  -- Prefer filling max charge (play intent of Max wpn); clamp by remaining budget.
  draft.weapons = {}
  for _, weapon in ipairs((ship and ship.weapons) or {}) do
    local carried = weapon.charge or 0
    local want = math.max(carried, weapon.max_charge or 0)
    local add = math.min(want - carried, remaining)
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
