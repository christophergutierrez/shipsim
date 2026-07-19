-- Pure layout math for the Love HUD (FIX-PLAN F1).
-- No love.* APIs — headless-testable under plain luajit.

local layout = {}

layout.MIN_WINDOW_WIDTH = 720
layout.MIN_WINDOW_HEIGHT = 420

--- Three fixed header slots for a window of width W.
--- left: turn/phase/CTA · center: End Turn button · right: rules provenance
function layout.header_slots(W, scale)
  scale = scale or 1
  local pad = math.floor(10 * scale)
  local top_h = math.floor(34 * scale)
  local end_w = math.floor(110 * scale)
  local end_h = math.floor(24 * scale)
  local rules_reserve = math.floor(200 * scale)
  local gap = math.floor(8 * scale)

  local right_x = W - pad - rules_reserve
  local end_x = right_x - gap - end_w
  if end_x < pad + math.floor(200 * scale) then
    -- Narrow window: shrink rules reserve.
    rules_reserve = math.floor(120 * scale)
    right_x = W - pad - rules_reserve
    end_x = right_x - gap - end_w
  end
  local left_w = math.max(0, end_x - pad - gap)
  local end_y = math.floor((top_h - end_h) / 2)

  return {
    left = { x = pad, y = 0, w = left_w, h = top_h },
    end_turn = { x = end_x, y = end_y, w = end_w, h = end_h },
    right = { x = right_x, y = 0, w = rules_reserve, h = top_h },
    top_h = top_h,
  }
end

--- True if two axis-aligned rects overlap (strict: edges may touch).
function layout.rects_overlap(a, b)
  if not a or not b then
    return false
  end
  return a.x < b.x + b.w and a.x + a.w > b.x
      and a.y < b.y + b.h and a.y + a.h > b.y
end

function layout.point_in_rect(x, y, rect)
  return rect ~= nil and x >= rect.x and x <= rect.x + rect.w
    and y >= rect.y and y <= rect.y + rect.h
end

--- Assert header slots do not overlap. Returns true, or false + reason.
function layout.header_slots_ok(W, scale)
  local s = layout.header_slots(W, scale)
  if layout.rects_overlap(s.left, s.end_turn) then
    return false, "left overlaps end_turn"
  end
  if layout.rects_overlap(s.left, s.right) then
    return false, "left overlaps right"
  end
  if layout.rects_overlap(s.end_turn, s.right) then
    return false, "end_turn overlaps right"
  end
  return true
end

--- Board camera origin: center map in the non-panel region.
function layout.board_camera_origin(width, height, panel_w, top_h, bottom_h, map_x, map_y, zoom)
  local board_w = width - panel_w
  local board_h = height - top_h - bottom_h
  return board_w / 2 - map_x * zoom, top_h + board_h / 2 - map_y * zoom
end

--- Word-wrap a string to max_width using a measure_fn(s) -> pixel width.
--- measure_fn can be font:getWidth or a stub. Returns array of lines (≤ max_lines).
function layout.wrap_text(text, max_width, measure_fn, max_lines)
  max_lines = max_lines or 99
  if not text or text == "" then
    return {}
  end
  measure_fn = measure_fn or function(s) return #s * 7 end
  local lines = {}
  for paragraph in (text .. "\n"):gmatch("(.-)\n") do
    if paragraph == "" then
      if #lines < max_lines then
        lines[#lines + 1] = ""
      end
    else
      local words = {}
      for w in paragraph:gmatch("%S+") do
        words[#words + 1] = w
      end
      local cur = ""
      for _, w in ipairs(words) do
        local trial = (cur == "") and w or (cur .. " " .. w)
        if measure_fn(trial) <= max_width or cur == "" then
          cur = trial
        else
          if #lines >= max_lines then
            break
          end
          lines[#lines + 1] = cur
          cur = w
        end
      end
      if cur ~= "" and #lines < max_lines then
        lines[#lines + 1] = cur
      elseif cur ~= "" and #lines == max_lines then
        -- Ellipsize last line.
        local last = lines[max_lines]
        while measure_fn(last .. "…") > max_width and #last > 1 do
          last = last:sub(1, -2)
        end
        lines[max_lines] = last .. "…"
      end
    end
    if #lines >= max_lines then
      break
    end
  end
  return lines
end

--- Ellipsize a single line to max_width.
function layout.ellipsize(text, max_width, measure_fn)
  measure_fn = measure_fn or function(s) return #s * 7 end
  if not text or measure_fn(text) <= max_width then
    return text or ""
  end
  local s = text
  while #s > 1 and measure_fn(s .. "…") > max_width do
    s = s:sub(1, -2)
  end
  return s .. "…"
end

--- Fixed roster row count = max(#ships, n_player_alive_or_dead).
--- Always return all ships sorted by id so panel height is stable.
function layout.roster_ships(snap)
  local ships = {}
  if not snap then
    return ships
  end
  for _, s in ipairs(snap.ships or {}) do
    ships[#ships + 1] = s
  end
  table.sort(ships, function(a, b) return (a.id or 0) < (b.id or 0) end)
  return ships
end

--- Enemies only for fire target list (controller ~= player, not destroyed).
function layout.enemy_targets(snap, self_id)
  local out = {}
  if not snap then
    return out
  end
  for _, s in ipairs(snap.ships or {}) do
    if s.id ~= self_id and not s.destroyed and s.controller ~= "player" then
      out[#out + 1] = s
    end
  end
  table.sort(out, function(a, b) return (a.id or 0) < (b.id or 0) end)
  return out
end

--- Minimum hitbox size in screen px (FIX-PLAN F3).
layout.MIN_HIT = 32

--- Expand a rect so w,h are at least min_hit (centered expansion where possible).
--- Only expands axes that are short — wide but short buttons grow in height only
--- so side-by-side quick-set chips do not steal each other's clicks.
function layout.ensure_hit_size(x, y, w, h, min_hit)
  min_hit = min_hit or layout.MIN_HIT
  local nw = (w < min_hit) and min_hit or w
  local nh = (h < min_hit) and min_hit or h
  local nx = x - (nw - w) / 2
  local ny = y - (nh - h) / 2
  return nx, ny, nw, nh
end

--- DPI-aware default scale from window size.
function layout.default_scale(w, h)
  local m = math.min(w or 1280, h or 800)
  return math.max(1, math.floor(m / 720))
end

--- Clamp a vertical scroll offset to the content's valid range.
function layout.scroll_clamp(offset, content_height, viewport_height)
  local max_offset = math.max(0, (content_height or 0) - (viewport_height or 0))
  return math.max(0, math.min(max_offset, offset or 0))
end

--- Keep a discrete picker selection inside its visible row window.
function layout.ensure_index_visible(first, selected, item_count, capacity)
  item_count = math.max(0, item_count or 0)
  capacity = math.max(1, capacity or 1)
  if item_count == 0 then
    return 1
  end
  local max_first = math.max(1, item_count - capacity + 1)
  first = math.max(1, math.min(max_first, first or 1))
  selected = math.max(1, math.min(item_count, selected or 1))
  if selected < first then
    first = selected
  elseif selected >= first + capacity then
    first = selected - capacity + 1
  end
  return math.max(1, math.min(max_first, first))
end

--- Fixed picker regions. The status strip owns the bottom edge of the window.
function layout.picker_metrics(width, height, scale, item_count)
  scale = scale or 1
  local pad = math.floor(20 * scale)
  local row_h = math.floor(30 * scale)
  local row_gap = math.floor(4 * scale)
  local status_h = math.floor(30 * scale)
  local status_y = height - status_h
  local exit_h = math.floor(30 * scale)
  local exit_y = status_y - pad - exit_h
  local list_top = pad + math.floor(70 * scale)
  local list_bottom = exit_y - math.floor(12 * scale)
  local capacity = math.max(0, math.floor((list_bottom - list_top + row_gap) / (row_h + row_gap)))
  local total = item_count or 0
  return {
    list = { x = pad, y = list_top, w = math.min(width - 2 * pad, math.floor(460 * scale)),
      h = math.max(0, list_bottom - list_top) },
    row_h = row_h,
    row_gap = row_gap,
    capacity = math.min(total, capacity),
    exit = { x = pad, y = exit_y, w = math.min(width - 2 * pad, math.floor(160 * scale)), h = exit_h },
    status = { x = 0, y = status_y, w = width, h = status_h },
  }
end

--- Fixed play-sidebar regions. Content scrolls; prompt, navigation, and status
--- remain fixed and disjoint from the scroll viewport.
function layout.sidebar_regions(width, height, scale, tutorial_active)
  scale = scale or 1
  local panel_w = math.floor(300 * scale)
  local top_h = math.floor(34 * scale)
  local status_h = math.floor(30 * scale)
  local nav_h = math.floor(26 * scale)
  local gap = math.floor(6 * scale)
  local status_y = height - status_h
  local nav_y = status_y - nav_h - gap
  local prompt_h = tutorial_active and math.floor(50 * scale) or 0
  local prompt_y = nav_y - prompt_h - gap
  local content_bottom = prompt_y - gap
  local content = { x = width - panel_w, y = top_h,
    w = panel_w, h = math.max(0, content_bottom - top_h) }
  return {
    header = { x = 0, y = 0, w = width, h = top_h },
    content = content,
    prompt = tutorial_active and { x = width - panel_w, y = prompt_y, w = panel_w, h = prompt_h } or nil,
    navigation = { x = width - panel_w, y = nav_y, w = panel_w, h = nav_h },
    status = { x = 0, y = status_y, w = width, h = status_h },
    panel = { x = width - panel_w, y = top_h, w = panel_w, h = status_y - top_h },
  }
end

--- Maximum useful scale while retaining a readable map and scroll viewport.
function layout.max_usable_scale(width, height, tutorial_active)
  local map_min = 420
  local width_limit = ((width or 1280) - map_min) / 300
  local fixed_h = 34 + 30 + 26 + 12 + (tutorial_active and 50 or 0)
  local height_limit = ((height or 800) - 180) / fixed_h
  local help_limit = ((height or 800) - 40) / 360
  return math.max(0.85, math.min(3.0, width_limit, height_limit, help_limit))
end

function layout.window_supported(width, height)
  return (width or 0) >= layout.MIN_WINDOW_WIDTH
    and (height or 0) >= layout.MIN_WINDOW_HEIGHT
end

function layout.help_metrics(width, height, scale)
  local margin = 20
  local box_w = math.min(math.max(0, width - 2 * margin), math.floor(560 * scale))
  local box_h = math.min(math.max(0, height - 2 * margin), math.floor(360 * scale))
  return {
    x = (width - box_w) / 2,
    y = (height - box_h) / 2,
    w = box_w,
    h = box_h,
  }
end

return layout
