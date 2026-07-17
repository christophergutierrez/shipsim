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
--
-- Ship damage pulses: a short red flash on a ship that lost hull. The pulse
-- is keyed by ship id so draw_board can look it up during the ship loop and
-- tint the marker. Pulses expire after PULSE_LIFE seconds.

local fx = {}

local DEFAULT_LIFE = 0.9
local DEFAULT_VY = -22.0 -- world px/s upward (matches SIZE=36 scale)
local PULSE_LIFE = 0.4   -- ship marker red flash duration (UPGRADE-PLAN Phase 3)

--- Create a new effect system.
function fx.new()
  return {
    active = {},   -- array of floater effect tables, oldest first
    pulses = {},   -- map ship_id -> { t, life } damage-pulse timers
    next_id = 1,   -- monotonic id for stable ordering / dedup
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

--- Advance all effects by dt seconds. Removes expired floaters (t >= life)
--- and expired damage pulses. Returns the count of floaters still active.
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
  -- Advance and expire damage pulses.
  if sys.pulses then
    local dead = {}
    for id, p in pairs(sys.pulses) do
      p.t = p.t + dt
      if p.t >= p.life then
        dead[#dead + 1] = id
      end
    end
    for _, id in ipairs(dead) do
      sys.pulses[id] = nil
    end
  end
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
  sys.pulses = {}
end

--- Spawn a damage pulse on a ship id. Re-starts the timer if already pulsing.
function fx.pulse(sys, ship_id, opts)
  if not sys or not sys.pulses or ship_id == nil then
    return
  end
  opts = opts or {}
  sys.pulses[ship_id] = {
    t = 0.0,
    life = opts.life or PULSE_LIFE,
  }
end

--- Pulse intensity for a ship at its current t: 1.0 at spawn, linear fade to 0
--- over life. Returns 0 if no active pulse. Pure function.
function fx.pulse_alpha(sys, ship_id)
  if not sys or not sys.pulses or not ship_id then
    return 0.0
  end
  local p = sys.pulses[ship_id]
  if not p or p.life <= 0 then
    return 0.0
  end
  if p.t >= p.life then
    return 0.0
  end
  return 1.0 - (p.t / p.life)
end

return fx
