-- Debounce / coalesce timer for read-only preview requests.
--
-- Pure Lua: no love.* APIs. Runs under plain luajit so it is testable
-- headless. The controller (main.lua) owns an instance, calls `poke(dt)`
-- each frame in love.update, and `trip()` whenever a draft changes. When the
-- quiet period elapses with no new trips, `due()` returns true and the
-- controller issues the (single) coalesced request, then calls `consume()`.
--
-- Protocol v4: reach_preview (allocate motion cloud) is built for live
-- slider drags, but harness.request is synchronous (blocks on a read). Issuing
-- it on every slider tick would stall the frame. This debounce coalesces a
-- burst of draft changes into at most one request per QUIET window.

local debounce = {}

local QUIET = 0.2   -- seconds of quiet before a coalesced request fires (≤5/s)

--- Create a new debounce timer.
function debounce.new(quiet)
  return {
    quiet = quiet or QUIET,
    armed = false,    -- true after a trip(), waiting for quiet to elapse
    t = 0.0,          -- seconds since the last trip
  }
end

--- Signal that a draft changed (e.g. allocation slider moved). Arms the timer.
function debounce.trip(d)
  if not d then return end
  d.armed = true
  d.t = 0.0
end

--- Advance the timer by dt seconds. Call every frame from love.update.
function debounce.poke(d, dt)
  if not d or not d.armed or dt <= 0 then
    return
  end
  d.t = d.t + dt
end

--- True when the quiet period has elapsed since the last trip. The controller
--- issues the coalesced request and calls consume() to disarm.
function debounce.due(d)
  if not d or not d.armed then
    return false
  end
  return d.t >= d.quiet
end

--- Disarm the timer after the controller has issued the coalesced request.
function debounce.consume(d)
  if not d then return end
  d.armed = false
  d.t = 0.0
end

--- True when the timer is armed (a trip happened but the request hasn't fired).
function debounce.armed(d)
  return d and d.armed or false
end

return debounce
