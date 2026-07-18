-- Arc-fan geometry for weapon arc visualization (UPGRADE-PLAN Phase 4).
--
-- Pure Lua: no love.* APIs. Runs under plain luajit so it is testable
-- headless. Given a ship's pixel position, facing, and a weapon's arc/max_range,
-- returns the polygon vertices of the translucent fan a terminal cannot draw.
--
-- This is geometry-for-pixels (where to draw), never geometry-for-legality
-- (whether a shot is allowed). Shot legality comes from fire_preview.
--
-- Flat-top hex layout (hex.lua): facing 0 = east (angle 0°), each facing is
-- 60° apart, clockwise. hex.DIRS[1]={1,0} (east), [2]={1,-1} (NE), etc.
-- A weapon arc covers a set of relative facings (0..5) from the ship's facing:
--   Forward = {0}            (a 60° wedge centered on the facing)
--   Rear    = {3}            (opposite)
--   Left    = {1, 2}         (120° on the left)
--   Right   = {4, 5}         (120° on the right)
--   All     = {0,1,2,3,4,5}  (full circle)
-- The fan polygon is built by walking the arc's facing edges out to the
-- range radius and back, producing a pie-slice (or full circle for "All").

local geom = {}

--- Map a weapon arc string to the set of relative facings (0..5) it covers.
--- Pure function. Returns a table {facing -> true}.
function geom.arc_facings(arc)
  arc = arc or "Forward"
  if arc == "Forward" then
    return { [0] = true }
  elseif arc == "Rear" then
    return { [3] = true }
  elseif arc == "Left" then
    return { [1] = true, [2] = true }
  elseif arc == "Right" then
    return { [4] = true, [5] = true }
  elseif arc == "All" then
    return { [0] = true, [1] = true, [2] = true, [3] = true, [4] = true, [5] = true }
  end
  return { [0] = true }
end

--- Pixel angle (radians) for a core facing 0..5 on hex.lua's axial board.
--- The board's q-axis is 30° down-right, then each increasing facing turns
--- 60° counter-clockwise on screen. This must match hex.to_pixel(neighbor).
function geom.facing_angle(facing)
  return math.pi / 180 * (30 - 60 * ((facing or 0) % 6))
end

--- Pixel angle for a relative direction such as shield F/FR/RR.
--- Keep the relative offset unwrapped so a two-face fan remains one wedge
--- across the 0/5 boundary.
function geom.relative_facing_angle(facing, relative)
  return geom.facing_angle(facing) - math.pi / 180 * (60 * (relative or 0))
end

--- Build the fan polygon for a weapon arc. Returns a flat table of x,y pairs
--- suitable for love.graphics.polygon. Pure pixel-math (no Love APIs).
---
---   cx, cy      — ship pixel center
---   facing      — ship facing 0..5
---   arc         — weapon arc string ("Forward"/"Rear"/"Left"/"Right"/"All")
---   radius      — fan radius in pixels (max_range_hexes * hex_size)
---
--- The fan is a pie slice: center vertex, then arc-edge points at radius,
--- then back to center. For "All" it's a full circle (no center vertex needed
--- but we include it for a consistent polygon shape).
function geom.fan_polygon(cx, cy, facing, arc, radius)
  local facings = geom.arc_facings(arc)
  -- Collect the covered facings in sorted order.
  local list = {}
  for f = 0, 5 do
    if facings[f] then
      list[#list + 1] = f
    end
  end
  if #list == 0 or radius <= 0 then
    return {}
  end

  -- Full circle: walk all 6 facing angles at radius.
  if #list == 6 then
    local pts = {}
    for i = 0, 5 do
      local a = geom.relative_facing_angle(facing, i)
      pts[#pts + 1] = cx + radius * math.cos(a)
      pts[#pts + 1] = cy + radius * math.sin(a)
    end
    return pts
  end

  -- Pie slice: center, leading edge, trailing edge, back to center.
  -- The wedge spans from the first facing's leading edge to the last facing's
  -- trailing edge. Each facing occupies a 60° wedge centered on its angle.
  -- Screen angles decrease as core-facing indices increase. The first covered
  -- facing therefore starts at +30° and the last ends at -30°.
  local first_f = list[1]
  local last_f = list[#list]
  local a0 = geom.relative_facing_angle(facing, first_f) + math.pi / 6
  local a1 = geom.relative_facing_angle(facing, last_f) - math.pi / 6
  local pts = {}
  pts[#pts + 1] = cx
  pts[#pts + 1] = cy
  pts[#pts + 1] = cx + radius * math.cos(a0)
  pts[#pts + 1] = cy + radius * math.sin(a0)
  pts[#pts + 1] = cx + radius * math.cos(a1)
  pts[#pts + 1] = cy + radius * math.sin(a1)
  return pts
end

--- Count the hexes a weapon arc covers at a given range (for the headless
--- milestone "arc fan geometry spans correct hex count"). Pure function.
--- Returns the count of hexes within range and in-arc for a ship at origin.
function geom.arc_hex_count(arc, max_range)
  if not max_range or max_range <= 0 then
    return 0
  end
  local facings = geom.arc_facings(arc)
  local count = 0
  -- Walk all hexes within max_range of origin (0,0) and count in-arc ones.
  -- This mirrors draw_board's hex loop but without Love APIs.
  for dq = -max_range, max_range do
    for dr = -max_range, max_range do
      if dq == 0 and dr == 0 then
        -- skip origin
      else
        local dist = (math.abs(dq) + math.abs(dq + dr) + math.abs(dr)) / 2
        if dist <= max_range then
          -- Compute absolute bearing from origin (0,0) to (dq,dr).
          -- bearing_to mirrors the engine (arc.rs nearest_bearings): find
          -- which neighbor of the ORIGIN is closest to the target hex, and
          -- return that neighbor's facing index. The neighbor of origin in
          -- direction i is DIRS[i]; we check its distance to (dq,dr).
          local best_i, best_key = 0, 1e9
          local DIRS = {
            { 1, 0 }, { 1, -1 }, { 0, -1 }, { -1, 0 }, { -1, 1 }, { 0, 1 },
          }
          for i = 0, 5 do
            local d = DIRS[i + 1]
            -- neighbor of origin in direction i = (d[1], d[2])
            local nd = (math.abs(d[1] - dq) + math.abs(d[1] + d[2] - dq - dr) + math.abs(d[2] - dr)) / 2
            local key = nd * 10 + i
            if key < best_key then
              best_key = key
              best_i = i
            end
          end
          if facings[best_i] then
            count = count + 1
          end
        end
      end
    end
  end
  return count
end

return geom
