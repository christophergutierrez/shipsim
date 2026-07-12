-- Advance scripted ships through v2 phases until a player action is required.
local orders = require("orders")

local pump = {}

local function first_pending(snap, controller)
  local committed = {}
  for _, id in ipairs(snap.ships_committed_this_phase or {}) do
    committed[id] = true
  end
  for _, ship in ipairs(snap.ships or {}) do
    if ship.controller == controller and not ship.destroyed and not committed[ship.id] then
      return ship.id
    end
  end
  return nil
end

local function first_unallocated(snap, controller)
  local allocated = {}
  for _, id in ipairs(snap.ships_allocated_this_turn or {}) do allocated[id] = true end
  for _, ship in ipairs(snap.ships or {}) do
    if ship.controller == controller and not ship.destroyed and not allocated[ship.id] then
      return ship.id
    end
  end
  return nil
end

function pump.run(session, on_error)
  for _ = 1, 256 do
    local snap = session and session.snapshot
    if not snap or snap.status == "Won" or snap.status == "Lost" then
      return
    end
    local order
    if snap.phase == "allocate" then
      if first_unallocated(snap, "player") then return end
      local ship = first_unallocated(snap, "scripted")
      if ship then
        local weapons = {}
        for _, weapon in ipairs(snap.ships or {}) do
          if weapon.id == ship then
            for _, mounted in ipairs(weapon.weapons or {}) do
              weapons[mounted.id] = 0
            end
          end
        end
        order = orders.allocate(ship, 0, weapons, { 0, 0, 0, 0, 0, 0 })
      end
    elseif snap.phase == "movement" then
      if first_pending(snap, "player") then return end
      local ship = first_pending(snap, "scripted")
      if ship then order = orders.coast(ship) end
    elseif snap.phase == "firing" then
      local ready = {}
      for _, id in ipairs(snap.ships_ready_fire or {}) do ready[id] = true end
      for _, ship in ipairs(snap.ships or {}) do
        if ship.controller == "player" and not ship.destroyed and not ready[ship.id] then
          return
        end
      end
      for _, ship in ipairs(snap.ships or {}) do
        if ship.controller == "scripted" and not ship.destroyed and not ready[ship.id] then
          order = orders.ready_fire(ship.id)
          break
        end
      end
    else
      return
    end
    if not order then return end
    local _, err = require("harness").submit(session, order)
    if err then
      if on_error then on_error(err) end
      return
    end
  end
  if on_error then on_error({ message = "scripted action pump reached its safety limit" }) end
end

return pump
