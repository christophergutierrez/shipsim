-- Shared UI scale, fonts, and clickable regions.

local layout = require("layout")

local ui = {}

-- Default scale is set at love.load from DPI / settings (FIX-PLAN F3).
ui.scale = 1.0
ui.min_scale = 0.85
ui.max_scale = 3.0
ui._fonts = {}
ui._hits = {} -- filled each frame during draw
ui._press = nil -- { id, payload, t0, last_fire } for hold-to-repeat (F3)

function ui.set_scale(s)
  ui.scale = math.max(ui.min_scale, math.min(ui.max_scale, s))
  ui._fonts = {}
end

function ui.adjust_scale(delta)
  ui.set_scale(ui.scale + delta)
end

function ui.font(size)
  -- Headless tests may lack love.graphics; return a stub with getWidth/getHeight.
  if not love or not love.graphics or not love.graphics.newFont then
    local px = math.floor(size * ui.scale + 0.5)
    if px < 10 then px = 10 end
    return {
      getWidth = function(_, s) return (#(s or "")) * px * 0.55 end,
      getHeight = function() return px end,
    }
  end
  local px = math.floor(size * ui.scale + 0.5)
  if px < 10 then
    px = 10
  end
  if not ui._fonts[px] then
    ui._fonts[px] = love.graphics.newFont(px)
  end
  return ui._fonts[px]
end

function ui.use(size)
  if love and love.graphics and love.graphics.setFont then
    love.graphics.setFont(ui.font(size))
  end
end

function ui.line_h(size)
  return ui.font(size):getHeight() + math.floor(4 * ui.scale)
end

function ui.clear_hits()
  ui._hits = {}
end

--- Register a clickable rect. Hitbox is expanded to layout.MIN_HIT (F3).
function ui.hit(id, x, y, w, h, payload)
  local nx, ny, nw, nh = layout.ensure_hit_size(x, y, w, h, layout.MIN_HIT)
  ui._hits[#ui._hits + 1] = {
    id = id,
    x = nx,
    y = ny,
    w = nw,
    h = nh,
    draw_x = x,
    draw_y = y,
    draw_w = w,
    draw_h = h,
    payload = payload,
  }
end

--- Topmost hit under (mx,my), or nil.
function ui.hit_at(mx, my)
  for i = #ui._hits, 1, -1 do
    local b = ui._hits[i]
    if mx >= b.x and mx <= b.x + b.w and my >= b.y and my <= b.y + b.h then
      return b
    end
  end
  return nil
end

--- All hits (for tests).
function ui.hits()
  return ui._hits
end

local function draw_button_body(x, y, w, h, active)
  if not love or not love.graphics then
    return
  end
  if active then
    love.graphics.setColor(0.25, 0.55, 0.35)
  else
    love.graphics.setColor(0.22, 0.24, 0.28)
  end
  love.graphics.rectangle("fill", x, y, w, h, 4, 4)
  love.graphics.setColor(0.55, 0.6, 0.65)
  love.graphics.rectangle("line", x, y, w, h, 4, 4)
end

function ui.button(label, x, y, w, h, id, payload, active)
  -- Taller default for two-line labels so cost text fits inside (F1 D2).
  local line1, line2 = label:match("^(.-)\n(.*)$")
  if line2 and h < math.floor(36 * ui.scale) then
    h = math.floor(36 * ui.scale)
  end
  draw_button_body(x, y, w, h, active)
  if love and love.graphics then
    ui.use(13)
    love.graphics.setColor(1, 1, 1)
    local font = love.graphics.getFont()
    if line1 then
      local tw1 = font:getWidth(line1)
      love.graphics.print(line1, x + (w - tw1) / 2, y + 4)
      ui.use(11)
      love.graphics.setColor(0.8, 0.85, 0.9)
      local font2 = love.graphics.getFont()
      local tw2 = font2:getWidth(line2)
      love.graphics.print(line2, x + (w - tw2) / 2, y + 4 + font:getHeight())
    else
      local tw = font:getWidth(label)
      local th = font:getHeight()
      love.graphics.print(label, x + (w - tw) / 2, y + (h - th) / 2)
    end
  end
  ui.hit(id, x, y, w, h, payload)
end

--- Single-line button with right-aligned secondary label (maneuver cost).
function ui.button_split(left, right, x, y, w, h, id, payload, active)
  draw_button_body(x, y, w, h, active)
  if love and love.graphics then
    ui.use(13)
    love.graphics.setColor(1, 1, 1)
    local font = love.graphics.getFont()
    local th = font:getHeight()
    local pad = math.floor(8 * ui.scale)
    love.graphics.print(left, x + pad, y + (h - th) / 2)
    ui.use(11)
    love.graphics.setColor(0.75, 0.8, 0.85)
    local font2 = love.graphics.getFont()
    local tw = font2:getWidth(right or "")
    love.graphics.print(right or "", x + w - pad - tw, y + (h - font2:getHeight()) / 2)
  end
  ui.hit(id, x, y, w, h, payload)
end

function ui.panel_bg(x, y, w, h)
  if not love or not love.graphics then
    return
  end
  love.graphics.setColor(0.1, 0.1, 0.12, 0.94)
  love.graphics.rectangle("fill", x, y, w, h)
end

-- Hold-to-repeat: after 350ms, fire at 10/s while mouse still on same hit id.
ui.HOLD_DELAY = 0.35
ui.HOLD_RATE = 0.1

function ui.press_begin(hit)
  if not hit then
    ui._press = nil
    return
  end
  ui._press = {
    id = hit.id,
    payload = hit.payload,
    t0 = 0,
    last_fire = 0,
    fired = false,
  }
end

function ui.press_end()
  ui._press = nil
end

--- Advance hold timer. Returns a synthetic hit to repeat, or nil.
--- Initial click is handled on mousepressed; this only fires after HOLD_DELAY.
function ui.press_tick(dt, mouse_down, mx, my)
  if not ui._press or not mouse_down then
    if not mouse_down then
      ui._press = nil
    end
    return nil
  end
  local p = ui._press
  p.t0 = p.t0 + dt
  local under = ui.hit_at(mx, my)
  if not under or under.id ~= p.id then
    return nil
  end
  if p.t0 < ui.HOLD_DELAY then
    return nil
  end
  if (p.t0 - (p.last_fire or 0)) >= ui.HOLD_RATE then
    p.last_fire = p.t0
    return under
  end
  return nil
end

return ui
