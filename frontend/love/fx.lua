-- Transient animation system for damage floaters and other short-lived effects.
--
-- Pure Lua: no love.* APIs. Runs under plain luajit so it is testable
-- headless, mirroring events.lua. The UI layer (main.lua) owns an instance,
-- ticks it in love.update(dt), and draws active effects in love.draw.
--
-- Effect shape: { x, y, text, color, t, life, vy }
--   x, y    — world-space anchor (caller converts hex->pixel before spawning)
--   text    — string to render
--   color   — {r, g, b, a} 0..1 (nil => caller default)
--   t       — elapsed seconds since spawn (advanced by update)
--   life    — total seconds before expiry
--   vy      — world-pixels/second upward drift (negative = rises)
--
-- Phase 3 (UPGRADE-PLAN): damage floaters attach to the target ship's cx, cy
-- (draw_board.lua ship loop) and rise + fade over ~0.9s.

local fx = {}

local DEFAULT_LIFE = 0.9
local DEFAULT_VY = -22.0 -- world px/s upward (matches SIZE=36 scale)

--- Create a new effect system.
function fx.new()
  return {
    active = {},  -- array of effect tables, oldest first
    next_id = 1,  -- monotonic id for stable ordering / dedup
  }
end

--- Spawn a floating-text effect. `opts` may override life, vy, color.
--- Returns the spawned effect table (or nil if x/y missing).
function fx.spawn(sys, x, y, text, opts)
  if x == nil or y == nil then
    return nil
  end
  opts = opts or {}
  local e = {
    id = sys.next_id,
    x = x,
    y = y,
    text = tostring(text),
    color = opts.color,
    t = 0.0,
    life = opts.life or DEFAULT_LIFE,
    vy = opts.vy or DEFAULT_VY,
  }
  sys.next_id = sys.next_id + 1
  sys.active[#sys.active + 1] = e
  return e
end

--- Advance all effects by dt seconds. Removes expired ones (t >= life).
--- Returns the count of effects still active after the tick.
function fx.update(sys, dt)
  if not sys or dt <= 0 then
    return #sys.active
  end
  local keep = {}
  for _, e in ipairs(sys.active) do
    e.t = e.t + dt
    e.y = e.y + e.vy * dt
    if e.t < e.life then
      keep[#keep + 1] = e
    end
  end
  sys.active = keep
  return #sys.active
end

--- Alpha for an effect at its current t: 1.0 for the first half, then a
--- linear fade to 0 over the second half. Pure function (no sys mutation).
function fx.alpha(e)
  if not e or e.life <= 0 then
    return 0.0
  end
  local half = e.life * 0.5
  if e.t < half then
    return 1.0
  end
  local remaining = e.life - e.t
  if remaining <= 0 then
    return 0.0
  end
  return remaining / half
end

--- Return a copy of the active effects, oldest first. The caller draws these
--- (main.lua / draw_board.lua) inside the camera transform so world-space
--- x/y land on the right hex.
function fx.active(sys)
  local copy = {}
  for i, e in ipairs(sys.active) do
    copy[i] = e
  end
  return copy
end

--- Return the count of active effects.
function fx.count(sys)
  return #sys.active
end

--- Remove all active effects (e.g. on scenario reload).
function fx.clear(sys)
  sys.active = {}
end

return fx
