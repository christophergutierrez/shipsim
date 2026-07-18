local hex = require("hex")
local fx = require("fx")
local geom = require("geom")
local slide = require("slide")

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

  -- UPGRADE-PLAN Phase 4: reachable-endpoint cloud. During allocate, the
  -- controller issues the reach preview (clamp:true) and passes the response
  -- here via opts.reach. We render endpoints as translucent diamonds, coast
  -- as a distinct outline, and occupied endpoints in warning color. No
  -- requests are issued here (grep gate: the preview request string does not
  -- appear in this file — board only draws state passed via opts).
  local mp = opts.reach
  if mp and mp.endpoints then
    local occ = {}
    for _, o in ipairs(mp.occupied or {}) do
      occ[o.q .. "," .. o.r] = true
    end
    for _, ep in ipairs(mp.endpoints) do
      local ex, ey = hex.to_pixel(ep.q, ep.r, SIZE)
      if occ[ep.q .. "," .. ep.r] then
        love.graphics.setColor(0.95, 0.4, 0.2, 0.7)
      else
        love.graphics.setColor(0.4, 0.7, 1.0, 0.45)
      end
      love.graphics.polygon("fill", {
        ex, ey - SIZE * 0.25, ex + SIZE * 0.25, ey,
        ex, ey + SIZE * 0.25, ex - SIZE * 0.25, ey,
      })
    end
    if mp.coast then
      local cex, cey = hex.to_pixel(mp.coast.q, mp.coast.r, SIZE)
      love.graphics.setColor(0.9, 0.85, 0.3, 0.9)
      love.graphics.setLineWidth(2)
      love.graphics.polygon("line", {
        cex, cey - SIZE * 0.3, cex + SIZE * 0.3, cey,
        cex, cey + SIZE * 0.3, cex - SIZE * 0.3, cey,
      })
      love.graphics.setLineWidth(1)
    end
  end

  -- UPGRADE-PLAN Phase 4: weapon arc fans for the selected ship. Each weapon's
  -- arc is drawn as a translucent fan (radius = max_range in hexes), colored by
  -- charge state (charged=green, uncharged=blue, destroyed=gray). This is
  -- display-only geometry; legality comes from fire_preview.
  if selected and not selected.destroyed then
    for _, w in ipairs(selected.weapons or {}) do
      local range = (w.max_range or 0) * SIZE
      if range > 0 then
        local scx, scy = hex.to_pixel(selected.q, selected.r, SIZE)
        local fan = geom.fan_polygon(scx, scy, selected.facing or 0, w.arc, range)
        if #fan >= 6 then
          if not w.operational then
            love.graphics.setColor(0.5, 0.5, 0.5, 0.12)
          elseif (w.charge or 0) >= (w.max_charge or 1) then
            love.graphics.setColor(0.3, 0.8, 0.4, 0.18)
          else
            love.graphics.setColor(0.3, 0.5, 0.9, 0.14)
          end
          love.graphics.polygon("fill", fan)
        end
      end
    end
  end

  -- UPGRADE-PLAN Phase 4: shield ring around the selected ship. Six arc
  -- segments sized/colored by shields_remaining vs max_shield_per_facing —
  -- bare faces are visibly missing. Answers "why did that torp hit sh-0".
  if selected and not selected.destroyed and selected.shields_remaining then
    local scx, scy = hex.to_pixel(selected.q, selected.r, SIZE)
    local maxpf = selected.max_shield_per_facing or 1
    if maxpf > 0 then
      local ring_r = SIZE * 0.75
      for face = 0, 5 do
        local remaining = selected.shields_remaining[face + 1] or 0
        local frac = remaining / maxpf
        if frac > 0 then
          -- Shield faces are relative to the hull's current facing, just like
          -- engine legal_shield_facings. Share the board-angle mapping used by
          -- weapon fans so F follows the nose instead of the screen.
          local a0 = geom.relative_facing_angle(selected.facing or 0, face) + math.pi / 6
          local a1 = geom.relative_facing_angle(selected.facing or 0, face) - math.pi / 6
          local segs = 8
          local pts = {}
          for i = 0, segs do
            local a = a0 + (a1 - a0) * i / segs
            pts[#pts + 1] = scx + ring_r * math.cos(a)
            pts[#pts + 1] = scy + ring_r * math.sin(a)
          end
          love.graphics.setColor(0.3, 0.7, 1.0, 0.3 + 0.4 * frac)
          love.graphics.setLineWidth(2 + 2 * frac)
          for i = 1, #pts - 2, 2 do
            love.graphics.line(pts[i], pts[i + 1], pts[i + 2], pts[i + 3])
          end
          love.graphics.setLineWidth(1)
        end
      end
    end
  end

  -- UPGRADE-PLAN Phase 5: slide interpolation. When a slide system is
  -- available (opts.slide), ships render at their interpolated pixel position
  -- instead of teleporting to the new hex. The slide module is pure Lua and
  -- returns nil for unknown ships; in that case we fall back to the snapshot
  -- hex position. Destroyed ships do not slide (they are gone).
  local slide_sys = opts.slide
  for _, ship in ipairs(snapshot.ships or {}) do
    local cx, cy = hex.to_pixel(ship.q, ship.r, SIZE)
    if slide_sys and not ship.destroyed then
      local sx, sy = slide.position(slide_sys, ship.id)
      if sx and sy then
        cx, cy = sx, sy
      end
    end
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

  -- UPGRADE-PLAN Phase 4: velocity vectors. Arrow from each ship along its
  -- course, length ∝ velocity, so head-on pass-throughs and kiting are legible
  -- pre-slide. Course is a hex direction 0..5 (same as facing).
  for _, ship in ipairs(snapshot.ships or {}) do
    if not ship.destroyed and (ship.velocity or 0) > 0 then
      local cx, cy = hex.to_pixel(ship.q, ship.r, SIZE)
      local cq, cr = hex.neighbor(ship.q, ship.r, ship.course or 0)
      local vpx, vpy = hex.to_pixel(cq, cr, SIZE)
      local dx, dy = vpx - cx, vpy - cy
      local len = math.sqrt(dx * dx + dy * dy)
      if len > 0 then
        local vlen = SIZE * 0.5 * (ship.velocity or 0)
        dx, dy = dx / len * vlen, dy / len * vlen
        love.graphics.setColor(0.8, 0.8, 0.85, 0.7)
        love.graphics.setLineWidth(2)
        love.graphics.line(cx, cy, cx + dx, cy + dy)
        -- arrowhead
        local ax, ay = -dy, dx
        local hl = SIZE * 0.12
        love.graphics.polygon("fill", {
          cx + dx, cy + dy,
          cx + dx - dx * 0.3 + ax / len * hl, cy + dy - dy * 0.3 + ay / len * hl,
          cx + dx - dx * 0.3 - ax / len * hl, cy + dy - dy * 0.3 - ay / len * hl,
        })
        love.graphics.setLineWidth(1)
      end
    end
  end

  -- UPGRADE-PLAN Phase 4: threat bearing lines. For each enemy with a charged
  -- weapon that the engine says can reach the selected ship, draw a thin red
  -- bearing line. The controller computes threats (reuse fire_preview with
  -- roles reversed, cached per snapshot) and passes them via opts.threats as
  -- an array of {from_q, from_r, to_q, to_r}. No requests issued here.
  if opts.threats then
    love.graphics.setColor(1.0, 0.2, 0.2, 0.5)
    love.graphics.setLineWidth(1)
    for _, t in ipairs(opts.threats) do
      local fxp, fyp = hex.to_pixel(t.from_q, t.from_r, SIZE)
      local txp, typ = hex.to_pixel(t.to_q, t.to_r, SIZE)
      love.graphics.line(fxp, fyp, txp, typ)
    end
    love.graphics.setLineWidth(1)
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
