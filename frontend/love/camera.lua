-- Auto-follow camera (FIX-PLAN F4). Pure state; draw applies cam table.

local camera = {}

local PAUSE_S = 5.0
local LERP = 4.0

function camera.new(cam)
  return {
    auto = true,
    pause_t = 0,
    cam = cam or { x = 200, y = 120, zoom = 1.2 },
  }
end

function camera.user_moved(sys)
  if not sys then
    return
  end
  sys.pause_t = PAUSE_S
end

function camera.set_auto(sys, on)
  if not sys then
    return
  end
  sys.auto = on and true or false
  if on then
    sys.pause_t = 0
  end
end

function camera.update(sys, dt, living_ships, hex_to_pixel, size, board_rect)
  if not sys or not sys.auto then
    return
  end
  if (sys.pause_t or 0) > 0 then
    sys.pause_t = sys.pause_t - (dt or 0)
    return
  end
  if not living_ships or #living_ships == 0 or not board_rect then
    return
  end
  local minx, miny = 1e9, 1e9
  local maxx, maxy = -1e9, -1e9
  for _, s in ipairs(living_ships) do
    local px, py = hex_to_pixel(s.q, s.r, size)
    if px < minx then minx = px end
    if py < miny then miny = py end
    if px > maxx then maxx = px end
    if py > maxy then maxy = py end
  end
  local cx = (minx + maxx) / 2
  local cy = (miny + maxy) / 2
  local target_x = board_rect.x + board_rect.w / 2 - cx * sys.cam.zoom
  local target_y = board_rect.y + board_rect.h / 2 - cy * sys.cam.zoom
  local a = math.min(1, (dt or 0) * LERP)
  sys.cam.x = sys.cam.x + (target_x - sys.cam.x) * a
  sys.cam.y = sys.cam.y + (target_y - sys.cam.y) * a
end

return camera
