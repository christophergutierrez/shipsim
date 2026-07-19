-- Selection state machine: dead-focus recovery and auto-advance.
--
-- Pure Lua: no love.* APIs. Runs under plain luajit so it is testable
-- headless, mirroring events.lua / fx.lua / preview.lua. The controller
-- (main.lua) owns an app state and calls these functions after every accepted
-- snapshot; they mutate the passed `state` table in place.
--
-- UPGRADE-PLAN Phase 2: mirrors the TUI's dead-focus recovery
-- (frontend/tui/src/app.rs:322-340). A destroyed (or vanished) focus is
-- unrecoverable by normal flow — pending-ship advancement waits for the
-- focused ship to act, and a wreck never acts. Re-focus the first living
-- player ship and drop drafts tied to the old focus.
--
-- `state` shape (a slice of the Love app table):
--   selected_id   — current focused ship id (nil = none)
--   weapon_id     — fire draft weapon (global, not per-ship)
--   target_id     — fire draft target
--   shield_facing — fire draft shield facing
--   alloc         — map ship_id -> alloc draft table (per-ship)
--
-- `snap` shape: { ships = { {id, controller, destroyed, ...} }, phase,
--   ships_committed_this_phase = {id,...} }

local phases = require("phases")

local selection = {}

--- Return sorted list of living player ship ids from a snapshot.
function selection.player_ids(snap)
  local ids = {}
  if not snap then
    return ids
  end
  for _, s in ipairs(snap.ships or {}) do
    if s.controller == "player" and not s.destroyed then
      ids[#ids + 1] = s.id
    end
  end
  table.sort(ids)
  return ids
end

--- Return charged, operational weapons that have not fired or been queued.
function selection.fireable_weapons(snap, ship_id)
  local ship = nil
  for _, candidate in ipairs((snap and snap.ships) or {}) do
    if candidate.id == ship_id then
      ship = candidate
      break
    end
  end
  if not ship then
    return {}
  end

  local committed = {}
  for _, fire in ipairs(snap.fire_commits or {}) do
    if fire.ship == ship_id and fire.weapon then
      committed[fire.weapon] = true
    end
  end

  local weapons = {}
  for _, weapon in ipairs(ship.weapons or {}) do
    if weapon.operational ~= false and (weapon.charge or 0) > 0
        and not weapon.fired and not committed[weapon.id] then
      weapons[#weapons + 1] = weapon.id
    end
  end
  return weapons
end

--- Cycle through currently fireable weapons in ship order.
function selection.cycle_fireable_weapon(snap, ship_id, current, delta)
  local weapons = selection.fireable_weapons(snap, ship_id)
  if #weapons == 0 then
    return nil
  end
  local index = nil
  for i, weapon_id in ipairs(weapons) do
    if weapon_id == current then
      index = i
      break
    end
  end
  if not index then
    return weapons[1]
  end
  local step = (delta or 1) < 0 and -1 or 1
  return weapons[((index - 1 + step) % #weapons) + 1]
end

--- First uncommitted player ship id in movement phase, or nil.
function selection.first_uncommitted(snap, controller)
  if not snap or snap.phase ~= "movement" then
    return nil
  end
  local committed = {}
  for _, id in ipairs(snap.ships_committed_this_phase or {}) do
    committed[id] = true
  end
  for _, s in ipairs(snap.ships or {}) do
    if not s.destroyed and not committed[s.id]
        and (not controller or s.controller == controller) then
      return s.id
    end
  end
  return nil
end

--- Drop drafts tied to a dead focus. Fire drafts are global (weapon/target);
--- alloc drafts are per-ship. Mutates `state` in place.
function selection.clear_drafts_for(state, dead_id)
  if dead_id == nil then
    return
  end
  if state.weapon_id ~= nil or state.target_id ~= nil then
    state.weapon_id = nil
    state.target_id = nil
    state.shield_facing = 0
  end
  if state.alloc then
    state.alloc[dead_id] = nil
  end
end

--- Reconcile focus against a fresh snapshot. Implements dead-focus recovery
--- (reselect first living player ship + clear drafts) and movement auto-advance
--- (follow first uncommitted player ship). Mutates `state` in place.
function selection.ensure(state, snap)
  local ids = selection.player_ids(snap)
  if #ids == 0 then
    state.selected_id = nil
    return
  end
  -- Dead-focus recovery: focused ship destroyed or gone.
  local prev = state.selected_id
  if prev ~= nil then
    local gone = true
    for _, id in ipairs(ids) do
      if id == prev then
        gone = false
        break
      end
    end
    if gone then
      selection.clear_drafts_for(state, prev)
      state.selected_id = ids[1]
      return
    end
  end
  -- Auto-advance: in movement, follow the first uncommitted player ship.
  local active = selection.first_uncommitted(snap, "player")
  if active then
    for _, id in ipairs(ids) do
      if id == active then
        state.selected_id = active
        return
      end
    end
  end
  -- Allocate is also per ship. Keep the one visible allocation form focused on
  -- the next ship that still owes power so fleet forms never stack below the
  -- viewport.
  if snap and snap.phase == phases.ALLOCATE then
    local allocated = {}
    for _, id in ipairs(snap.ships_allocated_this_turn or {}) do
      allocated[id] = true
    end
    for _, id in ipairs(ids) do
      if not allocated[id] then
        state.selected_id = id
        return
      end
    end
  end
  -- Keep current focus if still valid.
  for _, id in ipairs(ids) do
    if id == state.selected_id then
      return
    end
  end
  state.selected_id = ids[1]
end

return selection
