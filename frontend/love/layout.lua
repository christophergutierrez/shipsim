-- Pure layout math for the Love HUD (FIX-PLAN F1).
-- No love.* APIs — headless-testable under plain luajit.

local layout = {}

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

return layout
