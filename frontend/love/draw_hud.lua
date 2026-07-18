-- v2 HUD (Combat Model v2, ADR-0020).
-- Phase-aware panels: Allocate, Movement, Firing, TurnEnd.

local phases = require("phases")
local hex = require("hex")
local ui = require("ui")
local preview = require("preview")
local events = require("events")
local tutorial = require("tutorial")
local layout = require("layout")
local status_fmt = require("status_fmt")
local allocation = require("allocation")

local draw_hud = {}

local SHIELD_FACE = { "F", "FR", "RR", "R", "RL", "FL" }

-- UPGRADE-PLAN Phase 3: event-kind → display color. Pure function (no Love
-- APIs) so it is testable headless. Mirrors draw_board.EVENT_COLORS but lives
-- here so draw_hud is self-contained for the ticker.
local EVENT_COLOR = {
  hit_dealt = { 0.4, 0.9, 0.5 },
  hit_taken = { 0.95, 0.35, 0.35 },
  miss      = { 0.7, 0.7, 0.75 },
  blocked   = { 0.95, 0.8, 0.3 },
  info      = { 0.9, 0.85, 0.4 },
}

--- Return the {r,g,b} color for an event kind. Pure function (no Love APIs).
function draw_hud.event_color(kind)
  return EVENT_COLOR[kind] or EVENT_COLOR.info
end

--- Ticker fade alpha: 1.0 while recent, fading to 0.35 after TICKER_FADE
--- seconds of no new events. `now` and `last_event_time` are seconds (e.g.
--- love.timer.getTime()). Pure function (no Love APIs) so it is testable.
local TICKER_FADE = 5.0
local TICKER_FADED_ALPHA = 0.35
function draw_hud.ticker_alpha(now, last_event_time)
  if not last_event_time or not now then
    return 1.0
  end
  local elapsed = now - last_event_time
  if elapsed >= TICKER_FADE then
    return TICKER_FADED_ALPHA
  end
  -- Linear fade from 1.0 to TICKER_FADED_ALPHA over the fade window.
  local t = elapsed / TICKER_FADE
  return 1.0 - (1.0 - TICKER_FADED_ALPHA) * t
end

function draw_hud.panel_width()
  return math.floor(300 * ui.scale)
end

function draw_hud.left_rail_width()
  return 0
end

function draw_hud.top_h()
  return math.floor(34 * ui.scale)
end

function draw_hud.bottom_h()
  return math.floor(30 * ui.scale)
end

local function line(s, x, color)
  love.graphics.setColor(color or { 0.95, 0.95, 0.95 })
  love.graphics.print(s, x)
end

local function section(title, x)
  love.graphics.setColor(0.35, 0.9, 0.55)
  love.graphics.print(title, x)
end

local function ship_label(s)
  local ctrl = s.controller == "player" and "" or (" [" .. s.controller .. "]")
  return string.format("#%d %s%s", s.id, s.class or "?", ctrl)
end

local function find_ship(snap, id)
  if not snap or not id then
    return nil
  end
  for _, s in ipairs(snap.ships or {}) do
    if s.id == id then
      return s
    end
  end
  return nil
end

-- Callsign mirroring the TUI (protocol.rs callsign): A# for player, B# for ai,
-- C# otherwise. Used by the call-to-action banner so the Love2D header names
-- ships the same way the TUI does, instead of raw #id.
local function callsign(ship)
  if not ship then
    return "Ship"
  end
  local prefix = "C"
  if ship.controller == "player" then
    prefix = "A"
  elseif ship.controller == "ai" then
    prefix = "B"
  end
  return string.format("%s%d", prefix, ship.id)
end

-- Phase-specific call-to-action mirroring the TUI's phase_call_to_action
-- (ui.rs:2080). Returns "" when there is no next action to advertise. Pure
-- function (no Love APIs) so it can be unit-tested under plain luajit.
--
-- `selected_id` is the Love2D equivalent of the TUI's focused ship.
function draw_hud.phase_call_to_action(snap, selected_id)
  if not snap then
    return ""
  end
  -- A finished game has no next action.
  if snap.status == "Won" or snap.status == "Lost" then
    return "Game over"
  end

  local function completed_set(list)
    local set = {}
    for _, id in ipairs(list or {}) do
      set[id] = true
    end
    return set
  end

  -- pending_cta: name the selected ship if it still owes an action; otherwise
  -- name the first pending fleetmate with a Tab hint. Mirrors the TUI closure
  -- at ui.rs:2091-2106.
  local function pending_cta(completed, verb)
    local sel = find_ship(snap, selected_id)
    if sel and sel.controller == "player" and not sel.destroyed
        and not completed[sel.id] then
      return string.format("%s %s", callsign(sel), verb)
    end
    for _, s in ipairs(snap.ships or {}) do
      if s.controller == "player" and not s.destroyed and not completed[s.id] then
        return string.format("%s %s — Tab to switch", callsign(s), verb)
      end
    end
    return ""
  end

  local phase = snap.phase
  if phase == phases.ALLOCATE then
    local sel = find_ship(snap, selected_id)
    if sel and sel.controller == "player" and not sel.destroyed
        and (sel.power_available or 0) == 0
        and not completed_set(snap.ships_allocated_this_turn)[sel.id] then
      return string.format("%s disabled; Space passes", callsign(sel))
    end
    return pending_cta(completed_set(snap.ships_allocated_this_turn), "needs power allocation")
  elseif phase == phases.MOVEMENT then
    local sel = find_ship(snap, selected_id)
    if sel and sel.controller == "player" and not sel.destroyed
        and (sel.thrust_remaining or 0) == 0
        and not completed_set(snap.ships_committed_this_phase)[sel.id] then
      return string.format("%s no thrust; Space coasts", callsign(sel))
    end
    return pending_cta(completed_set(snap.ships_committed_this_phase), "needs a maneuver")
  elseif phase == phases.FIRING then
    local ready = {}
    for _, id in ipairs(snap.ships_ready_fire or {}) do
      ready[id] = true
    end
    local focused_ready = selected_id ~= nil and ready[selected_id] or false
    if focused_ready then
      local sel = find_ship(snap, selected_id)
      local cs = callsign(sel)
      local opp = snap.fire_opportunity
      if opp and selected_id ~= opp.ship then
        local other = callsign(find_ship(snap, opp.ship))
        local tgt = callsign(find_ship(snap, opp.target))
        return string.format("%s ready; Tab>%s %s>%s", cs, other, opp.weapon or "?", tgt)
      end
      return string.format("%s ready", cs)
    end
    local queued = 0
    for _, c in ipairs(snap.fire_commits or {}) do
      if c.ship == selected_id then
        queued = queued + 1
      end
    end
    if queued > 0 then
      return string.format("%d queued; Space fires", queued)
    elseif snap.fire_opportunity then
      local opp = snap.fire_opportunity
      local attacker = callsign(find_ship(snap, opp.ship))
      local tgt = callsign(find_ship(snap, opp.target))
      if selected_id == opp.ship then
        return string.format("%s %s>%s available", attacker, opp.weapon or "?", tgt)
      else
        local active = callsign(find_ship(snap, selected_id))
        return string.format("%s active; Tab>%s %s>%s", active, attacker, opp.weapon or "?", tgt)
      end
    else
      return "No legal shot; Space passes fire"
    end
  elseif phase == phases.TURN_END then
    return "Turn complete; e"
  end
  return ""
end

-- ADR-0022 M4: simultaneous commits — HUD "Active" is the first living ship
-- still owing a commitment this phase. Pass controller="player" for player input.
local function first_uncommitted_ship(snap, controller)
  if not snap or snap.phase ~= "movement" then
    return nil
  end
  local committed = {}
  for _, id in ipairs(snap.ships_committed_this_phase or {}) do
    committed[id] = true
  end
  for _, s in ipairs(snap.ships or {}) do
    if not s.destroyed and not committed[s.id]
        and (not controller or s.controller == controller) then
      return s.id
    end
  end
  return nil
end

--- Build the status header without Love APIs so nil focus and ownership stay
--- testable. "Active" is meaningful only in the simultaneous movement phase.
--- Never emits raw "#nil" (FIX-PLAN D8 / F2.4).
function draw_hud.header_text(snap, app_phase, selected_id)
  local turn = (snap and snap.turn) or 1
  local phase = app_phase or (snap and snap.phase) or phases.ALLOCATE
  local mp = ""
  if phase == phases.MOVEMENT and snap and snap.movement_phase then
    mp = string.format(" %d/4", snap.movement_phase)
  end
  local header = string.format("Turn %d  %s%s", turn, phase, mp)
  if phase == phases.MOVEMENT then
    local active = first_uncommitted_ship(snap, "player")
    if active then
      header = header .. status_fmt.header_active(active, function(id)
        return callsign(find_ship(snap, id))
      end)
    end
  end
  local cta = draw_hud.phase_call_to_action(snap, selected_id)
  if cta and cta ~= "" then
    header = header .. "  │ " .. cta
  end
  -- Guard: never show the literal "nil" in the header string.
  header = header:gsub("#nil", ""):gsub("%snil%s", " "):gsub("%s+", " ")
  return header
end

--- Center a board inside the space not occupied by HUD chrome.
function draw_hud.board_camera_origin(width, height, panel_w, top_h, bottom_h, map_x, map_y, zoom)
  return layout.board_camera_origin(width, height, panel_w, top_h, bottom_h, map_x, map_y, zoom)
end

function draw_hud.draw(app)
  local snap = app.session and app.session.snapshot
  local W = love.graphics.getWidth()
  local H = love.graphics.getHeight()
  local pw = draw_hud.panel_width()
  local px = W - pw
  local pad = math.floor(10 * ui.scale)
  local content_w = pw - 2 * pad
  local slots = layout.header_slots(W, ui.scale)
  local phase = app.phase or (snap and snap.phase) or phases.ALLOCATE

  -- F1 D1: three fixed header slots — left text, End Turn button, rules.
  love.graphics.setColor(0.1, 0.11, 0.14, 0.96)
  love.graphics.rectangle("fill", 0, 0, W, slots.top_h)
  ui.use(14)
  love.graphics.setColor(1, 1, 1)
  local header = draw_hud.header_text(snap, phase, app.selected_id)
  local font14 = ui.font(14)
  local measure = function(s) return font14:getWidth(s) end
  local left_text = layout.ellipsize(header, slots.left.w, measure)
  love.graphics.setScissor(slots.left.x, slots.left.y, slots.left.w, slots.left.h)
  love.graphics.print(left_text, slots.left.x,
    (slots.top_h - font14:getHeight()) / 2)
  love.graphics.setScissor()

  -- End Turn always owns the center-right header slot (discoverable; D1).
  if app.screen == "play" and snap then
    local et = slots.end_turn
    ui.button("End Turn", et.x, et.y, et.w, et.h, "end_turn", nil, false)
  end

  love.graphics.setColor(0.08, 0.09, 0.12, 0.97)
  love.graphics.rectangle("fill", px, slots.top_h, pw, H - slots.top_h - draw_hud.bottom_h())
  local y = slots.top_h + pad
  ui.use(13)

  if not snap then
    section("No scenario loaded", px + pad)
    return
  end

  -- Fixed roster: all ships every phase (dead dimmed) so panel y is stable (D5).
  section("Ships", px + pad)
  y = y + ui.line_h(13)
  local roster = layout.roster_ships(snap)
  for _, s in ipairs(roster) do
    local mark = (s.id == app.selected_id) and ">" or " "
    local dest = s.destroyed and " wreck" or ""
    if s.destroyed then
      love.graphics.setColor(0.45, 0.45, 0.5)
    elseif s.controller == "player" then
      love.graphics.setColor(0.7, 0.9, 1)
    else
      love.graphics.setColor(1, 0.6, 0.6)
    end
    love.graphics.print(string.format("%s %s pwr %d%s", mark, ship_label(s), s.power or 0, dest), px + pad, y)
    y = y + ui.line_h(13)
  end

  y = y + 4
  section("Phase: " .. phase, px + pad)
  y = y + ui.line_h(13)

  if phase == phases.ALLOCATE then
    y = draw_hud.draw_allocate_panel(app, snap, px, pad, y, content_w)
  elseif phase == phases.MOVEMENT then
    y = draw_hud.draw_movement_panel(app, snap, px, pad, y, content_w)
  elseif phase == phases.FIRING then
    y = draw_hud.draw_firing_panel(app, snap, px, pad, y, content_w)
  elseif phase == phases.TURN_END then
    y = draw_hud.draw_turn_end_panel(app, snap, px, pad, y, content_w)
  end

  y = y + 6
  -- UPGRADE-PLAN Phase 3: recent-events ticker. Last ~6 events from the
  -- events.lua ring buffer, color-coded by kind, fading after ~5s of no
  -- change. Sits above the combat log so fresh damage is visible without
  -- scrolling. The fade alpha is a pure function (ticker_alpha) tested
  -- headless; only the draw call touches Love APIs.
  if app.events then
    local rec = events.recent(app.events, 6)
    if #rec > 0 then
      local now = love.timer.getTime()
      local fade = draw_hud.ticker_alpha(now, app.last_event_time)
      for i = #rec, 1, -1 do
        local ev = rec[i]
        local c = draw_hud.event_color(ev.kind)
        love.graphics.setColor(c[1], c[2], c[3], fade)
        love.graphics.print(ev.text, px + pad, y)
        y = y + ui.line_h(13)
      end
      y = y + 4
    end
  end

  section("Combat log", px + pad)
  y = y + ui.line_h(13)
  local log = snap.combat_log or {}
  if #log == 0 then
    love.graphics.setColor(0.6, 0.6, 0.65)
    love.graphics.print("(empty)", px + pad, y)
    y = y + ui.line_h(13)
  else
    local start = math.max(1, #log - 5)
    for i = start, #log do
      local e = log[i]
      local face = SHIELD_FACE[(e.shield or 0) + 1] or "?"
      if e.kind == "miss" or (e.damage or 0) == 0 then
        love.graphics.setColor(0.85, 0.75, 0.4)
        love.graphics.print(string.format("%d->%d MISS (face %s)", e.attacker, e.target, face), px + pad, y)
      else
        love.graphics.setColor(0.95, 0.95, 0.95)
        love.graphics.print(string.format("%d->%d HIT %s dmg %d", e.attacker, e.target, face, e.damage), px + pad, y)
      end
      y = y + ui.line_h(13)
    end
  end

  -- UPGRADE-PLAN Phase 6: tutorial coach panel. Mirrors the TUI's
  -- render_tutorial_panel (ui.rs:2009-2077): yellow-bordered box with a
  -- prompt strip (do_now_line) and narration body. Sits at the bottom of
  -- the right panel, above the Scenarios button, only when app.tutorial is
  -- active. The prompt/narration are pure functions on the tutorial state;
  -- only the draw calls here touch Love APIs.
  if app.tutorial then
    y = draw_hud.draw_tutorial_panel(app, snap, px, pad, y, content_w,
      H - draw_hud.bottom_h() - math.floor(32 * ui.scale))
  end

  local nav_h = math.floor(26 * ui.scale)
  local nav_y = H - draw_hud.bottom_h() - nav_h - math.floor(6 * ui.scale)
  local gap = math.floor(6 * ui.scale)
  local half = math.floor((content_w - gap) / 2)
  ui.button("Scenarios", px + pad, nav_y, half, nav_h, "menu", nil, false)
  ui.button("Exit", px + pad + half + gap, nav_y, half, nav_h, "quit", nil, false)

  if app.show_help then
    draw_hud.draw_help_overlay()
  end
end

-- UPGRADE-PLAN Phase 6: tutorial coach panel. Mirrors the TUI's
-- render_tutorial_panel (ui.rs:2009-2077). Renders a yellow-bordered box with:
--   1. A title line: "Coach · Turn N · Phase · step/total" (or "Tutorial complete")
--   2. A yellow prompt strip (do_now_line — the immediate next action)
--   3. The narration body (step text + any error_msg prefix)
-- The prompt and narration are pure functions on the tutorial state (tutorial.lua);
-- only the draw calls here touch Love APIs. `y_bot` is the lower bound (Scenarios
-- button top) — the panel is clamped to fit above it.
function draw_hud.draw_tutorial_panel(app, snap, px, pad, y, content_w, y_bot)
  local t = app.tutorial
  if not t then
    return y
  end

  local lh = ui.line_h(13)
  local turn = (snap and snap.turn) or 0
  local phase = app.phase or (snap and snap.phase) or ""
  local mp = ""
  if phase == "movement" and snap and snap.movement_phase then
    mp = string.format(" %d/4", snap.movement_phase)
  elseif phase == "firing" and snap and snap.movement_phase then
    mp = string.format(" %d/4", snap.movement_phase)
  end
  local phase_str = phase .. mp
  if phase_str == "" then
    phase_str = "Starting"
  end

  local complete = tutorial.is_complete(t)
  local step_idx = tutorial.step_count(t) > 0
    and (t.current + 1) or 1
  local total = tutorial.step_count(t)
  local title
  if complete then
    title = "Tutorial complete"
  else
    title = string.format("Coach · Turn %d · %s · %d/%d", turn, phase_str, step_idx, total)
  end

  -- Derive cursor / field_value for do_now_line from the selected ship's alloc
  -- draft (the Love2D client is mouse-driven, so there is no keyboard cursor;
  -- we pass nil when not in allocate or no draft, and do_now_line handles nil).
  local cursor, field_value = nil, nil
  if not complete and snap and app.selected_id then
    local a = app.alloc[app.selected_id]
    if a and (phase == "allocate") then
      -- The Love2D alloc draft has no cursor; pass nil so do_now_line shows
      -- the "↓/↑ until ▶ is on <field>" guidance rather than a live value.
      -- field_value stays nil — the prompt is still useful without it.
    end
  end

  local prompt = tutorial.do_now_line(t, cursor, field_value)
  local narration = tutorial.narration(t)

  -- Clamp: reserve space for title (1 line) + prompt (1-2 lines) + narration.
  -- If we're too close to y_bot, skip the panel rather than overlap the button.
  local min_h = lh * 4
  if y + min_h > y_bot then
    return y
  end

  -- Yellow border box.
  local box_h = math.min(y_bot - y, lh * 10)
  love.graphics.setColor(0.7, 0.65, 0.15, 0.25)
  love.graphics.rectangle("fill", px, y, content_w + 2 * pad, box_h, 4, 4)
  love.graphics.setColor(0.85, 0.8, 0.25)
  love.graphics.rectangle("line", px, y, content_w + 2 * pad, box_h, 4, 4)

  local iy = y + math.floor(4 * ui.scale)

  -- Title line (yellow, bold-ish).
  ui.use(13)
  love.graphics.setColor(0.9, 0.85, 0.3)
  love.graphics.print(title, px + pad, iy)
  iy = iy + lh

  -- Prompt strip: black on yellow (mirrors the TUI's styled prompt).
  if prompt and prompt ~= "" then
    local font = ui.font(13)
    local prompt_h = lh
    love.graphics.setColor(0.9, 0.8, 0.15)
    love.graphics.rectangle("fill", px + pad, iy, content_w, prompt_h, 2, 2)
    love.graphics.setColor(0.08, 0.07, 0.05)
    love.graphics.print(" " .. prompt, px + pad, iy)
    iy = iy + prompt_h + 2
  end

  -- Narration body (white, wrapped crudely by splitting on \n).
  love.graphics.setColor(0.88, 0.88, 0.9)
  for line_text in narration:gmatch("[^\n]+") do
    if iy + lh > y + box_h then
      break
    end
    love.graphics.print(line_text, px + pad, iy)
    iy = iy + lh
  end

  return y + box_h + math.floor(6 * ui.scale)
end

function draw_hud.draw_allocate_panel(app, snap, px, pad, y, content_w)
  local bh = math.max(math.floor(24 * ui.scale), layout.MIN_HIT)
  local step = math.max(math.floor(22 * ui.scale), layout.MIN_HIT)
  local row_h = math.max(ui.line_h(13) + 2, step + 4)
  local allocated = {}
  for _, id in ipairs(snap.ships_allocated_this_turn or {}) do
    allocated[id] = true
  end
  local s = find_ship(snap, app.selected_id)
  if not s or s.controller ~= "player" or s.destroyed or allocated[s.id] then
    for _, candidate in ipairs(snap.ships or {}) do
      if candidate.controller == "player" and not candidate.destroyed and not allocated[candidate.id] then
        s = candidate
        break
      end
    end
  end
  if s and s.controller == "player" and not s.destroyed and not allocated[s.id] then
      love.graphics.setColor(0.8, 0.85, 0.9)
      love.graphics.print(ship_label(s), px + pad, y)
      y = y + ui.line_h(13)
      local a = app.alloc[s.id] or { movement = 0, weapons = {}, shields = { 0, 0, 0, 0, 0, 0 } }
      -- Quick-set allocation (F3.4).
      local qh = math.max(math.floor(22 * ui.scale), layout.MIN_HIT)
      local qw = math.floor((content_w - 9) / 4)
      ui.button("Max wpn", px + pad, y, qw, qh, "alloc_quick_max_weapons", { id = s.id }, false)
      ui.button("Bal sh", px + pad + qw + 3, y, qw, qh, "alloc_quick_balance_shields", { id = s.id }, false)
      ui.button("Engine", px + pad + 2 * (qw + 3), y, qw, qh, "alloc_quick_all_engine", { id = s.id }, false)
      ui.button("Clear", px + pad + 3 * (qw + 3), y, qw, qh, "alloc_quick_clear", { id = s.id }, false)
      y = y + qh + 4
      love.graphics.setColor(0.7, 0.75, 0.8)
      love.graphics.print(string.format("move %d  (+/- keys)", a.movement), px + pad, y)
      ui.button("-", px + pad + content_w - step * 2 - 4, y - 2, step, step, "alloc_movement_dn", { id = s.id }, false)
      ui.button("+", px + pad + content_w - step, y - 2, step, step, "alloc_movement_up", { id = s.id }, false)
      y = y + row_h
      -- Power bar: click sets movement fraction (F3.4).
      local bar_h = math.floor(12 * ui.scale)
      local bar_hit_h = math.max(bar_h, layout.MIN_HIT)
      local power = s.power_available or s.power or 0
      local spent = allocation.power_spent(s, a)
      love.graphics.setColor(0.15, 0.16, 0.2)
      love.graphics.rectangle("fill", px + pad, y, content_w, bar_h, 2, 2)
      local raw_frac = power > 0 and spent / power or 0
      love.graphics.setColor(raw_frac > 1 and 0.9 or 0.35, raw_frac > 1 and 0.3 or 0.7, 0.4)
      love.graphics.rectangle("fill", px + pad, y, content_w * math.min(1, raw_frac), bar_h, 2, 2)
      ui.hit("alloc_power_bar", px + pad, y, content_w, bar_hit_h, { id = s.id, power = power })
      y = y + bar_hit_h + 4
      for _, w in ipairs(s.weapons or {}) do
        local ch = a.weapons[w.id] or 0
        love.graphics.setColor(0.7, 0.75, 0.8)
        love.graphics.print(string.format("%s ch %d", w.id, ch), px + pad, y)
        ui.button("-", px + pad + content_w - step * 2 - 4, y - 2, step, step, "alloc_weapon_dn", { id = s.id, weapon = w.id }, false)
        ui.button("+", px + pad + content_w - step, y - 2, step, step, "alloc_weapon_up", {
          id = s.id,
          weapon = w.id,
          max = w.max_charge or 0,
        }, false)
        y = y + row_h
      end
      love.graphics.setColor(0.7, 0.75, 0.8)
      love.graphics.print("shields", px + pad, y)
      y = y + ui.line_h(13)
      for face = 0, 5 do
        local value = a.shields[face + 1] or 0
        love.graphics.setColor(0.7, 0.75, 0.8)
        love.graphics.print(string.format("%s %d", SHIELD_FACE[face + 1], value), px + pad, y)
        ui.button("-", px + pad + content_w - step * 2 - 4, y - 2, step, step, "alloc_shield_dn", { id = s.id, face = face }, false)
        ui.button("+", px + pad + content_w - step, y - 2, step, step, "alloc_shield_up", {
          id = s.id,
          face = face,
          max = s.max_shield_per_facing or 0,
        }, false)
        y = y + row_h
      end
      love.graphics.setColor(spent > power and { 0.95, 0.4, 0.4 } or { 0.7, 0.75, 0.8 })
      love.graphics.print(string.format("power %d / %d", spent, power), px + pad, y)
      y = y + ui.line_h(13) + 2
      ui.button("Allocate (Enter)", px + pad, y, content_w, bh, "alloc_confirm", { id = s.id }, false)
      y = y + bh + 6
  else
    love.graphics.setColor(0.7, 0.75, 0.8)
    love.graphics.print("All player ships allocated", px + pad, y)
    y = y + ui.line_h(13)
  end
  return y
end

function draw_hud.draw_movement_panel(app, snap, px, pad, y, content_w)
  local bh = math.max(math.floor(28 * ui.scale), layout.MIN_HIT)
  local active = first_uncommitted_ship(snap, "player")
  local ship = find_ship(snap, active)
  if not ship then
    love.graphics.setColor(0.7, 0.75, 0.8)
    love.graphics.print("No player mover pending", px + pad, y)
    y = y + ui.line_h(13)
    return y
  end
  love.graphics.setColor(0.8, 0.85, 0.9)
  local cycle = snap.movement_phase or 0
  love.graphics.print(
    string.format("Moving #%d (%s) — cycle %d/4", ship.id, ship.class or "?", cycle),
    px + pad, y)
  y = y + ui.line_h(13) + 2
  love.graphics.setColor(0.7, 0.75, 0.8)
  love.graphics.print(
    string.format("face %d  course %d  vel %d  thrust %d",
      ship.facing or 0, ship.course or 0, ship.velocity or 0, ship.thrust_remaining or 0),
    px + pad, y)
  y = y + ui.line_h(13) + 4
  -- Cost column inside button (F1 D2) — not a second line under the border.
  local opts = app.maneuver_options and app.maneuver_options.options or nil
  ui.button_split("Coast (P)", preview.maneuver_cost_label(opts, { type = "coast" }),
    px + pad, y, content_w, bh, "coast", nil, false)
  y = y + bh + 4
  ui.button_split("Accel (T)", preview.maneuver_cost_label(opts, { type = "accel" }),
    px + pad, y, content_w, bh, "accel", nil, false)
  y = y + bh + 4
  love.graphics.setColor(0.7, 0.75, 0.8)
  love.graphics.print(string.format("Turn to facing: %d  (keys 0-5)", app.maneuver_facing or 0), px + pad, y)
  y = y + ui.line_h(13) + 1
  local fw = math.floor((content_w - 5 * 3) / 6)
  for i = 0, 5 do
    local sel = ((app.maneuver_facing or 0) == i)
    ui.button(tostring(i), px + pad + i * (fw + 3), y, fw, bh, "pick_maneuver_facing", { face = i }, sel)
  end
  y = y + bh + 4
  ui.button_split("Turn", preview.maneuver_cost_label(opts, { type = "turn", facing = app.maneuver_facing or 0 }),
    px + pad, y, content_w, bh, "turn", nil, false)
  y = y + bh + 4
  ui.button_split("Turn+Accel", preview.maneuver_cost_label(opts, { type = "turn_accel", facing = app.maneuver_facing or 0 }),
    px + pad, y, content_w, bh, "turn_accel", nil, false)
  y = y + bh + 6
  return y
end

function draw_hud.draw_firing_panel(app, snap, px, pad, y, content_w)
  local bh = math.max(math.floor(24 * ui.scale), layout.MIN_HIT)
  local ship = find_ship(snap, app.selected_id)
  if not ship or ship.controller ~= "player" then
    love.graphics.setColor(0.7, 0.75, 0.8)
    love.graphics.print("Select one of your ships", px + pad, y)
    y = y + ui.line_h(13)
    return y
  end
  love.graphics.setColor(0.8, 0.85, 0.9)
  love.graphics.print("Weapon:", px + pad, y)
  y = y + ui.line_h(13) + 1
  for _, w in ipairs(ship.weapons or {}) do
    local sel = (app.weapon_id == w.id)
    ui.button(w.id, px + pad, y, content_w, bh, "pick_weapon", { id = w.id }, sel)
    y = y + bh + 2
  end
  y = y + 2
  love.graphics.setColor(0.8, 0.85, 0.9)
  love.graphics.print("Target: (enemies only)", px + pad, y)
  y = y + ui.line_h(13) + 1
  -- F2 D6 + F4.1: enemies only; show cached hit% when available.
  local enemies = layout.enemy_targets(snap, ship.id)
  for _, s in ipairs(enemies) do
    local sel = (app.target_id == s.id)
    local label = string.format("#%d %s", s.id, s.class or "?")
    local cache = app.target_previews and app.target_previews[s.id]
    if cache and cache.legal then
      label = string.format("#%d %s · %d%% ≈%ddmg", s.id, s.class or "?",
        cache.hit_percent or 0, cache.projected_damage or 0)
    elseif cache and not cache.legal then
      label = string.format("#%d %s · no shot", s.id, s.class or "?")
    end
    ui.button(label, px + pad, y, content_w, bh, "pick_target", { id = s.id }, sel)
    y = y + bh + 2
  end
  if #enemies == 0 then
    love.graphics.setColor(0.6, 0.6, 0.65)
    love.graphics.print("(no enemies)", px + pad, y)
    y = y + ui.line_h(13)
  end
  y = y + 2
  love.graphics.setColor(0.8, 0.85, 0.9)
  love.graphics.print(string.format("Shield face: %s", SHIELD_FACE[(app.shield_facing or 0) + 1] or "?"), px + pad, y)
  y = y + ui.line_h(13) + 1
  local fw = math.floor((content_w - 5 * 3) / 6)
  for i = 0, 5 do
    local sel = (app.shield_facing == i)
    ui.button(SHIELD_FACE[i + 1], px + pad + i * (fw + 3), y, fw, bh, "pick_shield_facing", { face = i }, sel)
  end
  y = y + bh + 4
  -- Fixed 2-line preview slot so Commit Fire y never jumps (F1 D3/D5).
  local preview_h = ui.line_h(13) * 2 + 4
  local fline = preview.fire_line(app)
  local font = ui.font(12)
  local measure = function(s) return font:getWidth(s) end
  if fline then
    if fline.color == "green" then
      love.graphics.setColor(0.4, 0.85, 0.5)
    elseif fline.color == "red" then
      love.graphics.setColor(0.95, 0.4, 0.4)
    else
      love.graphics.setColor(0.6, 0.6, 0.65)
    end
    local lines = layout.wrap_text(fline.text, content_w, measure, 2)
    ui.use(12)
    local ly = y
    for _, ln in ipairs(lines) do
      love.graphics.print(ln, px + pad, ly)
      ly = ly + ui.line_h(12)
    end
  end
  y = y + preview_h
  ui.button("Commit Fire (Enter)", px + pad, y, content_w, bh, "fire_confirm", nil, false)
  y = y + bh + 3
  ui.button("Ready (R)", px + pad, y, content_w, bh, "ready_fire", nil, false)
  y = y + bh + 6
  return y
end

function draw_hud.draw_turn_end_panel(app, snap, px, pad, y, content_w)
  local bh = math.max(math.floor(24 * ui.scale), layout.MIN_HIT)
  love.graphics.setColor(0.7, 0.75, 0.8)
  love.graphics.print("End of turn", px + pad, y)
  y = y + ui.line_h(13) + 4
  if snap.end_turn_warning then
    love.graphics.setColor(0.9, 0.75, 0.3)
    love.graphics.print("WARNING: unresolved fire", px + pad, y)
    y = y + ui.line_h(13) + 3
  end
  ui.button("End Turn (E)", px + pad, y, content_w, bh, "end_turn", nil, false)
  y = y + bh + 6
  return y
end

function draw_hud.draw_end_warning(app)
  local W = love.graphics.getWidth()
  local H = love.graphics.getHeight()
  love.graphics.setColor(0, 0, 0, 0.72)
  love.graphics.rectangle("fill", 0, 0, W, H)
  local box_w = math.min(W - 80, math.floor(480 * ui.scale))
  local box_h = math.floor(200 * ui.scale)
  local bx = (W - box_w) / 2
  local by = (H - box_h) / 2
  love.graphics.setColor(0.18, 0.14, 0.08, 0.98)
  love.graphics.rectangle("fill", bx, by, box_w, box_h, 6, 6)
  love.graphics.setColor(0.95, 0.8, 0.3)
  ui.use(18)
  love.graphics.print("End turn anyway?", bx + 16, by + 14)
  ui.use(13)
  love.graphics.setColor(0.9, 0.9, 0.92)
  -- F4.3: name the forfeited shot when fire_opportunity is present.
  local detail = "There is unresolved fire or unspent power."
  local snap = app.session and app.session.snapshot
  if snap and snap.fire_opportunity then
    local opp = snap.fire_opportunity
    detail = string.format("%s %s → ship #%s in range — forfeit?",
      callsign(find_ship(snap, opp.ship)),
      opp.weapon or "weapon",
      tostring(opp.target or "?"))
  end
  local font = ui.font(13)
  local lines = layout.wrap_text(detail, box_w - 32, function(s) return font:getWidth(s) end, 3)
  local ly = by + 48
  for _, ln in ipairs(lines) do
    love.graphics.print(ln, bx + 16, ly)
    ly = ly + ui.line_h(13)
  end
  local bw = math.floor(160 * ui.scale)
  ui.button("Confirm", bx + 16, by + box_h - 44, bw, 30, "end_warning_confirm", nil, false)
  ui.button("Cancel", bx + 16 + bw + 12, by + box_h - 44, bw, 30, "end_warning_cancel", nil, false)
end

--- Phase toast overlay (F4.4).
function draw_hud.draw_toast(app)
  local toast_mod = require("toast")
  if not app.toast or not toast_mod.active(app.toast) then
    return
  end
  local W = love.graphics.getWidth()
  local H = love.graphics.getHeight()
  local a = toast_mod.alpha(app.toast)
  local text = app.toast.text
  ui.use(22)
  local font = ui.font(22)
  local tw = font:getWidth(text)
  local th = font:getHeight()
  local bx = (W - tw) / 2 - 20
  local by = H * 0.28
  love.graphics.setColor(0.08, 0.09, 0.12, 0.85 * a)
  love.graphics.rectangle("fill", bx, by, tw + 40, th + 20, 6, 6)
  love.graphics.setColor(0.95, 0.9, 0.45, a)
  love.graphics.print(text, bx + 20, by + 10)
end

function draw_hud.draw_help_overlay()
  local W = love.graphics.getWidth()
  local H = love.graphics.getHeight()
  love.graphics.setColor(0, 0, 0, 0.72)
  love.graphics.rectangle("fill", 0, 0, W, H)
  local box_w = math.min(W - 80, math.floor(560 * ui.scale))
  local box_h = math.floor(360 * ui.scale)
  local bx = (W - box_w) / 2
  local by = (H - box_h) / 2
  love.graphics.setColor(0.12, 0.13, 0.16, 0.98)
  love.graphics.rectangle("fill", bx, by, box_w, box_h, 6, 6)
  love.graphics.setColor(0.4, 0.85, 0.55)
  ui.use(18)
  love.graphics.print("Help — Combat v2 turns", bx + 16, by + 12)
  ui.use(13)
  love.graphics.setColor(0.9, 0.9, 0.92)
  local lines = {
    "Allocate: +/− steppers (hold to repeat) or +/− keys for movement.",
    "  Quick: Max wpn / Bal sh / Engine / Clear. Power bar sets move fraction.",
    "  Enter or Allocate commits. Weapon charge carries (cannot strip).",
    "Movement: P=coast, T=accel, 0-5=turn, Shift+0-5=turn+accel.",
    "  End Turn button lives in the header (also E).",
    "Firing: enemies only; rows show hit% when available.",
    "  Enter=Commit Fire, R=Ready. Board-click sets target.",
    "Right-drag pan, wheel zoom, C=auto-fit, F=toggle follow, Ctrl -/= UI scale.",
    "?/H help. Esc=scenarios. Exit/Q=quit.",
  }
  local y = by + 48
  for _, s in ipairs(lines) do
    love.graphics.print(s, bx + 16, y)
    y = y + ui.line_h(13)
  end
  ui.button("Close", bx + 16, by + box_h - 40, math.floor(120 * ui.scale), 28, "toggle_help", nil, false)
end

function draw_hud.draw_picker(app)
  local W = love.graphics.getWidth()
  local H = love.graphics.getHeight()
  love.graphics.setColor(0.08, 0.09, 0.11)
  love.graphics.rectangle("fill", 0, 0, W, H)
  local pad = math.floor(20 * ui.scale)
  ui.use(22)
  love.graphics.setColor(0.4, 0.85, 0.55)
  love.graphics.print("Combat v2 — choose a scenario", pad, pad)
  ui.use(14)
  love.graphics.setColor(0.7, 0.75, 0.8)
  love.graphics.print("Up/Down to select, Enter to start.  Esc or Q quits.", pad, pad + math.floor(34 * ui.scale))
  local y = pad + math.floor(70 * ui.scale)
  local bw = math.min(W - 2 * pad, math.floor(460 * ui.scale))
  local bh = math.floor(30 * ui.scale)
  local exit_h = math.floor(30 * ui.scale)
  local list_bot = H - pad - exit_h - math.floor(12 * ui.scale)
  for i, sc in ipairs(app.scenarios) do
    if y + bh > list_bot then
      break
    end
    local sel = (i == app.picker_index)
    ui.button(sc.name, pad, y, bw, bh, "pick_scenario", { index = i }, sel)
    y = y + bh + 4
  end
  if #app.scenarios == 0 then
    love.graphics.setColor(0.9, 0.6, 0.4)
    love.graphics.print("No scenarios found. Check repo_root.", pad, y)
  end
  local exit_w = math.min(bw, math.floor(160 * ui.scale))
  ui.button("Exit", pad, H - pad - exit_h, exit_w, exit_h, "quit", nil, false)
end

function draw_hud.status_strip(st)
  -- ui_status uses message/level; accept legacy msg/kind for safety.
  if not st then
    return
  end
  local msg = st.message or st.msg
  if not msg or msg == "" then
    return
  end
  local kind = st.level or st.kind or "info"
  local W = love.graphics.getWidth()
  local H = love.graphics.getHeight()
  local h = draw_hud.bottom_h()
  local color = { 0.6, 0.6, 0.65 }
  if kind == "error" then
    color = { 0.95, 0.4, 0.4 }
  elseif kind == "warn" then
    color = { 0.95, 0.75, 0.3 }
  elseif kind == "info" then
    color = { 0.5, 0.8, 0.6 }
  end
  love.graphics.setColor(0.06, 0.07, 0.09, 0.95)
  love.graphics.rectangle("fill", 0, H - h, W, h)
  love.graphics.setColor(color)
  ui.use(13)
  love.graphics.print(msg, math.floor(10 * ui.scale), H - h + (h - ui.font(13):getHeight()) / 2)
end

-- Persistent rules-provenance label (UPGRADE-PLAN Phase 0 task 4).
-- Drawn in a screen corner, always visible during play. This is distinct from
-- the transient status strip: provenance is metadata, not an event message.
-- Format mirrors the TUI header: "rules: <id> <fp12>".

-- Pure label formatter (no Love APIs) so it is testable under plain luajit.
function draw_hud.rules_label(app)
  if not app or not app.rules_id then
    return nil
  end
  local fp = app.rules_fingerprint or ""
  return string.format("rules: %s %s", app.rules_id, fp:sub(1, 12))
end

function draw_hud.rules_provenance(app)
  local label = draw_hud.rules_label(app)
  if not label then
    return
  end
  local W = love.graphics.getWidth()
  local slots = layout.header_slots(W, ui.scale)
  ui.use(11)
  local font = ui.font(11)
  local measure = function(s) return font:getWidth(s) end
  local text = layout.ellipsize(label, slots.right.w - 8, measure)
  local tw = font:getWidth(text)
  local x = slots.right.x + slots.right.w - tw - 4
  local y = math.floor((slots.top_h - font:getHeight()) / 2)
  love.graphics.setScissor(slots.right.x, slots.right.y, slots.right.w, slots.right.h)
  love.graphics.setColor(0.5, 0.5, 0.55)
  love.graphics.print(text, x, y)
  love.graphics.setScissor()
end

-- UPGRADE-PLAN Phase 5: game-over panel. Mirrors the TUI's
-- render_game_over_summary (ui.rs:2194): VICTORY/DEFEAT banner, turns, player
-- shots/hits, internal damage dealt/taken — all computed from the events.lua
-- history (structured — never log string parsing). Plus a quit button and the
-- session log path (printed on exit like the TUI does).
function draw_hud.draw_game_over(app)
  local W = love.graphics.getWidth()
  local H = love.graphics.getHeight()
  love.graphics.setColor(0.06, 0.07, 0.09)
  love.graphics.rectangle("fill", 0, 0, W, H)

  local snap = app.session and app.session.snapshot
  local status = app.end_result or (snap and snap.status) or ""
  local banner = "GAME OVER"
  local bcolor = { 0.9, 0.85, 0.4 }
  if status == "won" or status == "Won" then
    banner = " VICTORY"
    bcolor = { 0.4, 0.9, 0.5 }
  elseif status == "defeat" or status == "Lost" then
    banner = " DEFEAT"
    bcolor = { 0.95, 0.35, 0.35 }
  end

  local pad = math.floor(20 * ui.scale)
  local box_w = math.min(W - 2 * pad, math.floor(440 * ui.scale))
  local box_h = math.floor(300 * ui.scale)
  local bx = (W - box_w) / 2
  local by = (H - box_h) / 2
  love.graphics.setColor(0.1, 0.11, 0.14, 0.98)
  love.graphics.rectangle("fill", bx, by, box_w, box_h, 8, 8)

  local y = by + math.floor(20 * ui.scale)
  ui.use(28)
  love.graphics.setColor(bcolor[1], bcolor[2], bcolor[3])
  love.graphics.print(banner, bx + pad, y)
  y = y + math.floor(40 * ui.scale)

  ui.use(15)
  local turn = (snap and snap.turn) or 0
  love.graphics.setColor(0.9, 0.9, 0.92)
  love.graphics.print(string.format(" Turns: %d", turn), bx + pad, y)
  y = y + ui.line_h(15)

  -- Stats from the structured event history (events.stats — pure function).
  local st = { shots = 0, hits = 0, int_dealt = 0, int_taken = 0 }
  if app.events then
    st = events.stats(app.events)
  end
  love.graphics.print(string.format(" Player shots: %d  hits: %d", st.shots, st.hits), bx + pad, y)
  y = y + ui.line_h(15)
  love.graphics.print(string.format(" Internal damage dealt: %d", st.int_dealt), bx + pad, y)
  y = y + ui.line_h(15)
  love.graphics.print(string.format(" Internal damage taken: %d", st.int_taken), bx + pad, y)
  y = y + ui.line_h(15) + 4

  -- Session log path (written on quit by main.lua love.quit).
  love.graphics.setColor(0.6, 0.6, 0.65)
  if app.session_log_path then
    love.graphics.print(" Session log: " .. app.session_log_path, bx + pad, y)
  else
    love.graphics.print(" Session log written on exit", bx + pad, y)
  end
  y = y + ui.line_h(15) + 8

  -- Return to picker or exit the app (session log written in love.quit).
  local bh = math.floor(28 * ui.scale)
  local bw = math.floor(140 * ui.scale)
  local by_btn = by + box_h - bh - math.floor(16 * ui.scale)
  ui.button("Scenarios", bx + pad, by_btn, bw, bh, "menu", nil, false)
  ui.button("Exit", bx + pad + bw + math.floor(12 * ui.scale), by_btn, bw, bh, "quit", nil, false)
  love.graphics.setColor(0.6, 0.6, 0.65)
  ui.use(13)
  love.graphics.print("Enter/Esc → picker · Q → exit", bx + pad, by_btn - ui.line_h(13) - 2)
end

return draw_hud
