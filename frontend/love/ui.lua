-- Shared UI scale, fonts, and clickable regions.

local ui = {}

-- Default 1.0 so allocate controls fit short maximized clients (~380–500px).
-- Ctrl -/= still scales up for large displays.
ui.scale = 1.0
ui.min_scale = 1.0
ui.max_scale = 3.0
ui._fonts = {}
ui._hits = {} -- filled each frame during draw

function ui.set_scale(s)
  ui.scale = math.max(ui.min_scale, math.min(ui.max_scale, s))
  ui._fonts = {}
end

function ui.adjust_scale(delta)
  ui.set_scale(ui.scale + delta)
end

function ui.font(size)
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
  love.graphics.setFont(ui.font(size))
end

function ui.line_h(size)
  return ui.font(size):getHeight() + math.floor(4 * ui.scale)
end

function ui.clear_hits()
  ui._hits = {}
end

--- Register a clickable rect. id is a string; payload optional table.
function ui.hit(id, x, y, w, h, payload)
  ui._hits[#ui._hits + 1] = {
    id = id,
    x = x,
    y = y,
    w = w,
    h = h,
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

function ui.button(label, x, y, w, h, id, payload, active)
  if active then
    love.graphics.setColor(0.25, 0.55, 0.35)
  else
    love.graphics.setColor(0.22, 0.24, 0.28)
  end
  love.graphics.rectangle("fill", x, y, w, h, 4, 4)
  love.graphics.setColor(0.55, 0.6, 0.65)
  love.graphics.rectangle("line", x, y, w, h, 4, 4)
  ui.use(13)
  love.graphics.setColor(1, 1, 1)
  local font = love.graphics.getFont()
  -- Support "Title\nsubtitle" for left-rail step buttons.
  local line1, line2 = label:match("^(.-)\n(.*)$")
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
  ui.hit(id, x, y, w, h, payload)
end

function ui.panel_bg(x, y, w, h)
  love.graphics.setColor(0.1, 0.1, 0.12, 0.94)
  love.graphics.rectangle("fill", x, y, w, h)
end

return ui
