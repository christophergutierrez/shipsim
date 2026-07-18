-- Ship position slide interpolation (UPGRADE-PLAN Phase 5).
--
-- Pure Lua: no love.* APIs. Runs under plain luajit so it is testable
-- headless, mirroring events.lua / fx.lua. The UI layer (main.lua) owns an
-- instance, feeds it each accepted snapshot's ship positions, ticks it in
-- love.update(dt), and queries interpolated positions in love.draw.
--
-- Ships lerp between their previous and current hex positions over SLIDE_DUR
-- seconds instead of teleporting. When a snapshot arrives, each ship's current
-- pixel position becomes the "from" and the new position becomes the "to".
-- Ships that are new (not seen before) snap instantly (t=1). Ships that
-- vanished (destroyed/removed) are dropped from the slide table.

local slide = {}

local SLIDE_DUR = 0.3 -- UPGRADE-PLAN: 0.3s slide

--- Create a new slide interpolation system.
function slide.new()
  return {
    -- map ship_id -> { from_x, from_y, to_x, to_y, t, life }
    ships = {},
  }
end

--- Feed a snapshot to update slide targets. `hex_to_pixel` is a function
--- (q, r, size) -> x, y (typically hex.to_pixel). `size` is the hex size.
--- Each ship's current interpolated position becomes the new "from"; the
--- snapshot position becomes the new "to". New ships snap (t=life).
function slide.feed(sys, snap, hex_to_pixel, size)
  if not sys or not snap or not hex_to_pixel then
    return
  end
  local seen = {}
  for _, s in ipairs(snap.ships or {}) do
    seen[s.id] = true
    local tx, ty = hex_to_pixel(s.q, s.r, size)
    local prev = sys.ships[s.id]
    if prev then
      -- Continue from the current interpolated position.
      local cur_x, cur_y = slide.position(sys, s.id)
      sys.ships[s.id] = {
        from_x = cur_x, from_y = cur_y,
        to_x = tx, to_y = ty,
        t = 0.0, life = SLIDE_DUR,
      }
    else
      -- New ship: snap instantly.
      sys.ships[s.id] = {
        from_x = tx, from_y = ty,
        to_x = tx, to_y = ty,
        t = SLIDE_DUR, life = SLIDE_DUR,
      }
    end
  end
  -- Drop ships that vanished.
  for id in pairs(sys.ships) do
    if not seen[id] then
      sys.ships[id] = nil
    end
  end
end

--- Advance all slides by dt seconds. Returns nothing.
function slide.update(sys, dt)
  if not sys or not sys.ships or dt <= 0 then
    return
  end
  for _, s in pairs(sys.ships) do
    s.t = s.t + dt
    if s.t > s.life then
      s.t = s.life
    end
  end
end

--- Eased progress 0..1 for a slide entry. Uses ease-out-cubic for a
--- natural deceleration. Pure function.
local function progress(s)
  if not s or s.life <= 0 then
    return 1.0
  end
  if s.t >= s.life then
    return 1.0
  end
  local raw = s.t / s.life
  -- ease-out-cubic: 1 - (1 - raw)^3
  return 1.0 - (1.0 - raw) ^ 3
end

--- Return the interpolated {x, y} for a ship id, or nil if unknown.
--- Pure function (does not mutate sys).
function slide.position(sys, ship_id)
  if not sys or not sys.ships then
    return nil
  end
  local s = sys.ships[ship_id]
  if not s then
    return nil
  end
  local p = progress(s)
  return s.from_x + (s.to_x - s.from_x) * p,
         s.from_y + (s.to_y - s.from_y) * p
end

--- Return true if all ships have finished sliding (t >= life).
function slide.settled(sys)
  if not sys or not sys.ships then
    return true
  end
  for _, s in pairs(sys.ships) do
    if s.t < s.life then
      return false
    end
  end
  return true
end

--- Remove all slide state (e.g. on scenario reload).
function slide.clear(sys)
  if sys then
    sys.ships = {}
  end
end

return slide
