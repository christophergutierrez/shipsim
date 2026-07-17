local hex = require("hex")
local fx = require("fx")

local draw_board = {}

local SIZE = 36

-- UPGRADE-PLAN Phase 3: color map for event kinds, used by the ticker and
-- floater spawning. Pure data (no Love APIs) so it is testable headless.
draw_board.EVENT_COLORS = {
  hit_dealt = { 0.4, 0.9, 0.5, 1 },
  hit_taken = { 0.95, 0.35, 0.35, 1 },
  miss      = { 0.7, 0.7, 0.75, 1 },
  blocked   = { 0.95, 0.8, 0.3, 1 },
  info      = { 0.9, 0.85, 0.4, 1 },
}

-- Display only — shot validity comes from fire_preview (UPGRADE-PLAN Phase 1).
-- bearing_to / relative_bearing / arc_ok below are used ONLY to tint hexes
-- and mark enemies for the player's eye. They never decide whether a shot is
-- allowed; the engine's fire_preview response is the sole authority on that.
-- Phase 4's arc-fan rendering may replace these shading helpers entirely.

function draw_board.hex_size()
  return SIZE
end

local function weapon_def(ship, weapon_id)
  if not ship or not weapon_id then
    return nil
  end
  for _, w in ipairs(ship.weapons or {}) do
    if w.id == weapon_id then
      return w
    end
  end
  return nil
end

--- Absolute facing 0..5 toward target (same idea as core bearing_to).
local function bearing_to(from_q, from_r, to_q, to_r)
  local best_i, best_key = 0, 1e9
  for i = 0, 5 do
    local d = hex.DIRS[i + 1]
    local nq, nr = from_q + d[1], from_r + d[2]
    local dist = hex.distance(nq, nr, to_q, to_r)
    local key = dist * 10 + i
    if key < best_key then
      best_key = key
      best_i = i
    end
  end
  return best_i
end

--- Relative bearing 0..5 from ship facing to target hex (core relative_bearing).
local function relative_bearing(facing, from_q, from_r, to_q, to_r)
  local abs = bearing_to(from_q, from_r, to_q, to_r)
  return (abs - (facing or 0)) % 6
end

local function arc_ok(arc, rel)
  arc = arc or "Forward"
  if arc == "Forward" then
    return rel == 0
  elseif arc == "Rear" then
    return rel == 3
  elseif arc == "Left" then
    return rel == 1 or rel == 2
  elseif arc == "Right" then
    return rel == 4 or rel == 5
  elseif arc == "All" then
    return true
  end
  return rel == 0
end

function draw_board.draw(snapshot, cam, selected_id, ghost_path, opts)
  opts = opts or {}
  if not snapshot or not snapshot.map then
    return
  end
  local w = snapshot.map.width or 0
  local h = snapshot.map.height or 0
  local selected = nil
  for _, s in ipairs(snapshot.ships or {}) do
    if s.id == selected_id then
      selected = s
      break
    end
  end
  local wdef = weapon_def(selected, opts.weapon_id)

  love.graphics.push()
  love.graphics.translate(cam.x, cam.y)
  love.graphics.scale(cam.zoom, cam.zoom)

  -- hexes + optional arc/range tint
  for q = 0, w - 1 do
    for r = 0, h - 1 do
      local cx, cy = hex.to_pixel(q, r, SIZE)
      local pts = hex.corners(cx, cy, SIZE - 1)
      local tinted = false
      if selected and wdef and not (q == selected.q and r == selected.r) then
        local dist = hex.distance(selected.q, selected.r, q, r)
        if dist > 0 and dist <= (wdef.max_range or 0) then
          local rel = relative_bearing(selected.facing or 0, selected.q, selected.r, q, r)
          if arc_ok(wdef.arc, rel) then
            love.graphics.setColor(0.15, 0.45, 0.25, 0.45)
            love.graphics.polygon("fill", pts)
            tinted = true
          end
        end
      end
      love.graphics.setColor(0.25, 0.3, 0.35)
      love.graphics.polygon("line", pts)
      if not tinted then
        -- keep lines only
      end
    end
  end

  if snapshot.objective then
    local cx, cy = hex.to_pixel(snapshot.objective.q, snapshot.objective.r, SIZE)
    love.graphics.setColor(0.2, 0.8, 0.3, 0.4)
    love.graphics.circle("fill", cx, cy, SIZE * 0.5)
  end

  if ghost_path then
    love.graphics.setColor(1, 1, 0.3, 0.85)
    for i, step in ipairs(ghost_path) do
      local cx, cy = hex.to_pixel(step.q, step.r, SIZE)
      love.graphics.circle("line", cx, cy, SIZE * 0.35)
      love.graphics.print(tostring(i), cx - 4, cy - 6)
    end
  end

  for _, ship in ipairs(snapshot.ships or {}) do
    local cx, cy = hex.to_pixel(ship.q, ship.r, SIZE)
    if ship.destroyed then
      love.graphics.setColor(0.4, 0.4, 0.4)
    elseif ship.controller == "player" then
      love.graphics.setColor(0.3, 0.6, 1.0)
    elseif ship.controller == "ai" then
      love.graphics.setColor(1.0, 0.35, 0.35)
    else
      love.graphics.setColor(0.95, 0.75, 0.2)
    end
    love.graphics.circle("fill", cx, cy, SIZE * 0.45)

    -- UPGRADE-PLAN Phase 3: damage pulse — hull loss flashes the marker red.
    -- fx.pulse_alpha returns 0 when no pulse is active, so this is a no-op
    -- for ships that haven't been hit recently.
    if opts.fx then
      local pa = fx.pulse_alpha(opts.fx, ship.id)
      if pa > 0 and not ship.destroyed then
        love.graphics.setColor(1.0, 0.15, 0.15, pa * 0.7)
        love.graphics.circle("fill", cx, cy, SIZE * 0.45)
      end
    end

    -- target highlight
    if opts.target_id == ship.id then
      love.graphics.setColor(1, 0.2, 0.2)
      love.graphics.setLineWidth(3)
      love.graphics.circle("line", cx, cy, SIZE * 0.62)
      love.graphics.setLineWidth(1)
    end
    if selected_id == ship.id then
      love.graphics.setColor(1, 1, 1)
      love.graphics.circle("line", cx, cy, SIZE * 0.55)
    end

    -- facing tick
    local nq, nr = hex.neighbor(ship.q, ship.r, ship.facing or 0)
    local fpx, fpy = hex.to_pixel(nq, nr, SIZE)
    local dx, dy = fpx - cx, fpy - cy
    local len = math.sqrt(dx * dx + dy * dy)
    if len > 0 then
      dx, dy = dx / len * SIZE * 0.55, dy / len * SIZE * 0.55
      love.graphics.setColor(1, 1, 1)
      love.graphics.line(cx, cy, cx + dx, cy + dy)
    end
    love.graphics.setColor(1, 1, 1)
    love.graphics.print(tostring(ship.id), cx - 4, cy - 6)

    -- in-range enemy marker when firing
    if selected and wdef and ship.id ~= selected.id and not ship.destroyed then
      local dist = hex.distance(selected.q, selected.r, ship.q, ship.r)
      local rel = relative_bearing(selected.facing or 0, selected.q, selected.r, ship.q, ship.r)
      if dist <= (wdef.max_range or 0) and arc_ok(wdef.arc, rel) then
        love.graphics.setColor(1, 0.4, 0.2, 0.9)
        love.graphics.circle("line", cx, cy, SIZE * 0.7)
      end
    end
  end

  -- UPGRADE-PLAN Phase 3: translation callouts. When translation_results
  -- reports a block for any ship, draw a ⊘ marker on that ship's hex so the
  -- player sees where the slide stopped without reading the log. The marker
  -- is drawn at the ship's current (post-slide) position.
  if snapshot.translation_results then
    for _, tr in ipairs(snapshot.translation_results) do
      if tr.blocked then
        -- Find the ship's current hex by id.
        local bs = nil
        for _, s in ipairs(snapshot.ships or {}) do
          if s.id == tr.ship then
            bs = s
            break
          end
        end
        if bs then
          local bx, by = hex.to_pixel(bs.q, bs.r, SIZE)
          love.graphics.setColor(0.95, 0.8, 0.3, 0.9)
          love.graphics.setLineWidth(2)
          love.graphics.circle("line", bx, by, SIZE * 0.3)
          -- ⊘ = circle with diagonal slash
          local r = SIZE * 0.3
          love.graphics.line(bx - r * 0.7, by - r * 0.7, bx + r * 0.7, by + r * 0.7)
          love.graphics.setLineWidth(1)
        end
      end
    end
  end

  love.graphics.pop()
end

function draw_board.screen_to_hex(mx, my, cam)
  local x = (mx - cam.x) / cam.zoom
  local y = (my - cam.y) / cam.zoom
  local q, r = hex.from_pixel(x, y, SIZE)
  return q, r
end

return draw_board
