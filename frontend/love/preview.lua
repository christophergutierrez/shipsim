-- Engine-authoritative preview formatting (UPGRADE-PLAN Phase 1).
-- Pure logic: no Love APIs. Runs under plain luajit for headless testing.
-- The controller (main.lua) issues fire_preview / maneuver_options requests
-- via harness.request and stores the response on app state; this module turns
-- those responses into display strings. Legality is never computed here — it
-- comes straight from the engine response.

local preview = {}

local SHIELD_FACE = { "F", "FR", "RR", "R", "RL", "FL" }

-- Callsign for a ship id, mirroring the TUI's callsign() (protocol.rs).
-- Player -> "A"+id, ai -> "B"+id, other -> "C"+id. Falls back to "#"+id.
function preview.callsign(ship)
  if not ship then
    return "?"
  end
  local prefix = "C"
  if ship.controller == "player" then
    prefix = "A"
  elseif ship.controller == "ai" then
    prefix = "B"
  end
  return prefix .. tostring(ship.id)
end

local function find_ship(snap, id)
  if not snap or not id then
    return nil
  end
  for _, s in ipairs(snap.ships or {}) do
    if s.id == id then
      return s
    end
  end
  return nil
end

-- Is the weapon destroyed (operational == false) on this ship?
local function weapon_destroyed(ship, weapon_id)
  if not ship or not weapon_id then
    return false
  end
  for _, w in ipairs(ship.weapons or {}) do
    if w.id == weapon_id then
      return w.operational == false
    end
  end
  return false
end

-- Format a fire_preview response into a display line.
-- Returns { text, color } where color is "green" (legal), "red" (illegal),
-- or "gray" (no preview). Mirrors TUI fire_preview_line (ui.rs:1279).
-- app = { fire_preview = <response or nil>, selected_id, weapon_id, target_id,
--         shield_facing, session = { snapshot = snap } }
function preview.fire_line(app)
  local fp = app.fire_preview
  if not fp then
    return nil
  end
  local snap = app.session and app.session.snapshot
  local attacker_ship = find_ship(snap, fp.ship)
  local target_ship = find_ship(snap, fp.target)
  local attacker = preview.callsign(attacker_ship)
  local target = preview.callsign(target_ship)

  if not fp.legal then
    -- A destroyed weapon comes back from the engine as a lookup failure
    -- ("weapon X was not found") — say what actually happened.
    local reason
    if weapon_destroyed(attacker_ship, fp.weapon) then
      reason = fp.weapon .. " is destroyed and cannot fire"
    else
      reason = fp.reason or "illegal shot"
    end
    return {
      text = string.format("%s %s->%s: %s", attacker, fp.weapon, target, reason),
      color = "red",
    }
  end

  local face = app.shield_facing or 0
  local face_ok = false
  for _, f in ipairs(fp.legal_shield_facings or {}) do
    if f == face then
      face_ok = true
      break
    end
  end
  local valid_faces = {}
  for _, f in ipairs(fp.legal_shield_facings or {}) do
    valid_faces[#valid_faces + 1] = SHIELD_FACE[f + 1] or "?"
  end
  local valid_str = table.concat(valid_faces, "/")
  local face_label = SHIELD_FACE[face + 1] or "?"
  local face_suffix
  if face_ok then
    face_suffix = "ok"
  else
    face_suffix = "INVALID; use " .. valid_str
  end

  local text = string.format("%s %s->%s d%d: %d%% (d%d<=%d) dmg~%d  face %s %s",
    attacker, fp.weapon, target,
    fp.range or 0, fp.hit_percent or 0,
    fp.die_sides or 0, fp.threshold or 0,
    fp.projected_damage or 0,
    face_label, face_suffix)
  return {
    text = text,
    color = face_ok and "green" or "red",
  }
end

--- Summarize a path_preview response for the path panel (protocol v4).
--- Returns a short status string; does not invent legality.
function preview.path_line(path_preview)
  if not path_preview then
    return "…"
  end
  if path_preview.error then
    return "illegal: " .. tostring(path_preview.error)
  end
  local cost = path_preview.cost or 0
  local rem = path_preview.remaining_motion
  if rem ~= nil then
    return string.format("cost %d · %d left · end face %s",
      cost, rem, tostring(path_preview.final_facing or "?"))
  end
  return string.format("cost %d · end face %s", cost, tostring(path_preview.final_facing or "?"))
end

return preview
