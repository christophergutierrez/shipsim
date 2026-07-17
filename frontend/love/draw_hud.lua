-- v2 HUD (Combat Model v2, ADR-0020).
-- Phase-aware panels: Allocate, Movement, Firing, TurnEnd.

local phases = require("phases")
local hex = require("hex")
local ui = require("ui")

local draw_hud = {}

local SHIELD_FACE = { "F", "FR", "RR", "R", "RL", "FL" }

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

-- Projected beam damage for a charged weapon (duplicates the core
-- combat_tables formula for UI preview; core is authoritative).
local function projected_damage(ship, weapon_id)
  if not ship or not weapon_id then
    return nil
  end
  local w = nil
  for _, ww in ipairs(ship.weapons or {}) do
    if ww.id == weapon_id then
      w = ww
      break
    end
  end
  if not w then
    return nil
  end
  local charge = w.charge or w.level or 1
  local dice = w.damage_dice or 2
  local per = w.damage_per_die or 4
  return dice * per * charge, charge
end

function draw_hud.draw(app)
  local snap = app.session and app.session.snapshot
  local W = love.graphics.getWidth()
  local H = love.graphics.getHeight()
  local pw = draw_hud.panel_width()
  local px = W - pw
  local pad = math.floor(10 * ui.scale)
  local content_w = pw - 2 * pad

  love.graphics.setColor(0.1, 0.11, 0.14, 0.96)
  love.graphics.rectangle("fill", 0, 0, W, draw_hud.top_h())
  ui.use(14)
  love.graphics.setColor(1, 1, 1)
  local turn = (snap and snap.turn) or 1
  local phase = app.phase or (snap and snap.phase) or phases.ALLOCATE
  local active = snap and first_uncommitted_ship(snap, "player")
  local mp = ""
  if phase == phases.MOVEMENT and snap and snap.movement_phase then
    mp = string.format(" %d/4", snap.movement_phase)
  end
  love.graphics.print(
    string.format("Turn %d  %s%s  Active #%s", turn, phase, mp, tostring(active)),
    pad, (draw_hud.top_h() - ui.font(14):getHeight()) / 2
  )

  love.graphics.setColor(0.08, 0.09, 0.12, 0.97)
  love.graphics.rectangle("fill", px, draw_hud.top_h(), pw, H - draw_hud.top_h() - draw_hud.bottom_h())
  local y = draw_hud.top_h() + pad
  ui.use(13)

  if not snap then
    section("No scenario loaded", px + pad)
    return
  end

  section("Ships", px + pad)
  y = y + ui.line_h(13)
  for _, s in ipairs(snap.ships or {}) do
    local mark = (s.id == app.selected_id) and ">" or " "
    local dest = s.destroyed and " (destroyed)" or ""
    love.graphics.setColor(s.controller == "player" and { 0.7, 0.9, 1 } or { 1, 0.6, 0.6 })
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

  ui.button("Scenarios", px + pad, H - draw_hud.bottom_h() - math.floor(32 * ui.scale), content_w, math.floor(26 * ui.scale), "menu", nil, false)

  if app.show_help then
    draw_hud.draw_help_overlay()
  end
end

function draw_hud.draw_allocate_panel(app, snap, px, pad, y, content_w)
  local bh = math.floor(24 * ui.scale)
  for _, s in ipairs(snap.ships or {}) do
    if s.controller == "player" and not s.destroyed then
      love.graphics.setColor(0.8, 0.85, 0.9)
      love.graphics.print(ship_label(s), px + pad, y)
      y = y + ui.line_h(13)
      local a = app.alloc[s.id] or { movement = 0, weapons = {}, shields = { 0, 0, 0, 0, 0, 0 } }
      love.graphics.setColor(0.7, 0.75, 0.8)
      love.graphics.print(string.format("move %d", a.movement), px + pad, y)
      ui.button("-", px + pad + 60, y - 2, 20, bh, "alloc_movement_dn", { id = s.id }, false)
      ui.button("+", px + pad + 84, y - 2, 20, bh, "alloc_movement_up", { id = s.id }, false)
      y = y + ui.line_h(13) + 2
      for _, w in ipairs(s.weapons or {}) do
        local ch = a.weapons[w.id] or 0
        love.graphics.setColor(0.7, 0.75, 0.8)
        love.graphics.print(string.format("%s ch %d", w.id, ch), px + pad, y)
        ui.button("-", px + pad + 100, y - 2, 20, bh, "alloc_weapon_dn", { id = s.id, weapon = w.id }, false)
        ui.button("+", px + pad + 124, y - 2, 20, bh, "alloc_weapon_up", {
          id = s.id,
          weapon = w.id,
          max = w.max_charge or 0,
        }, false)
        y = y + ui.line_h(13) + 1
      end
      love.graphics.setColor(0.7, 0.75, 0.8)
      love.graphics.print("shields", px + pad, y)
      y = y + ui.line_h(13)
      for face = 0, 5 do
        local value = a.shields[face + 1] or 0
        love.graphics.setColor(0.7, 0.75, 0.8)
        love.graphics.print(string.format("%s %d", SHIELD_FACE[face + 1], value), px + pad, y)
        ui.button("-", px + pad + 60, y - 2, 20, bh, "alloc_shield_dn", { id = s.id, face = face }, false)
        ui.button("+", px + pad + 84, y - 2, 20, bh, "alloc_shield_up", {
          id = s.id,
          face = face,
          max = s.max_shield_per_facing or 0,
        }, false)
        y = y + ui.line_h(13) + 1
      end
      local spent = a.movement
      for _, charge in pairs(a.weapons) do spent = spent + charge end
      for _, shield in ipairs(a.shields) do spent = spent + shield end
      love.graphics.setColor(spent > (s.power or 0) and { 0.95, 0.4, 0.4 } or { 0.7, 0.75, 0.8 })
      love.graphics.print(string.format("power %d / %d", spent, s.power or 0), px + pad, y)
      y = y + ui.line_h(13) + 2
      ui.button("Allocate", px + pad, y, content_w, bh, "alloc_confirm", { id = s.id }, false)
      y = y + bh + 6
    end
  end
  return y
end

function draw_hud.draw_movement_panel(app, snap, px, pad, y, content_w)
  local bh = math.floor(24 * ui.scale)
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
  -- v3 motion model (ADR-0022 M4/M6): coast / accel / turn{facing} / turn_accel{facing}.
  ui.button("Coast (P)", px + pad, y, content_w, bh, "coast", nil, false)
  y = y + bh + 4
  ui.button("Accel (T)", px + pad, y, content_w, bh, "accel", nil, false)
  y = y + bh + 4
  love.graphics.setColor(0.7, 0.75, 0.8)
  love.graphics.print(string.format("Turn to facing: %d", app.maneuver_facing or 0), px + pad, y)
  y = y + ui.line_h(13) + 1
  local fw = math.floor((content_w - 5 * 3) / 6)
  for i = 0, 5 do
    local sel = ((app.maneuver_facing or 0) == i)
    ui.button(tostring(i), px + pad + i * (fw + 3), y, fw, bh, "pick_maneuver_facing", { face = i }, sel)
  end
  y = y + bh + 4
  ui.button("Turn", px + pad, y, content_w, bh, "turn", nil, false)
  y = y + bh + 4
  ui.button("Turn+Accel (Shift+0-5)", px + pad, y, content_w, bh, "turn_accel", nil, false)
  y = y + bh + 6
  return y
end

function draw_hud.draw_firing_panel(app, snap, px, pad, y, content_w)
  local bh = math.floor(24 * ui.scale)
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
  love.graphics.print("Target:", px + pad, y)
  y = y + ui.line_h(13) + 1
  for _, s in ipairs(snap.ships or {}) do
    if s.id ~= ship.id and not s.destroyed then
      local sel = (app.target_id == s.id)
      ui.button(string.format("#%d %s", s.id, s.class or "?"), px + pad, y, content_w, bh, "pick_target", { id = s.id }, sel)
      y = y + bh + 2
    end
  end
  y = y + 2
  love.graphics.setColor(0.8, 0.85, 0.9)
  love.graphics.print(string.format("Shield face: %s", SHIELD_FACE[app.shield_facing + 1]), px + pad, y)
  y = y + ui.line_h(13) + 1
  local fw = math.floor((content_w - 5 * 3) / 6)
  for i = 0, 5 do
    local sel = (app.shield_facing == i)
    ui.button(SHIELD_FACE[i + 1], px + pad + i * (fw + 3), y, fw, bh, "pick_shield_facing", { face = i }, sel)
  end
  y = y + bh + 4
  local dmg, charge = projected_damage(ship, app.weapon_id)
  if dmg then
    love.graphics.setColor(0.9, 0.8, 0.4)
    love.graphics.print(string.format("proj dmg ~%d (ch %d)", dmg, charge), px + pad, y)
    y = y + ui.line_h(13) + 3
  end
  ui.button("Commit Fire", px + pad, y, content_w, bh, "fire_confirm", nil, false)
  y = y + bh + 3
  ui.button("Ready (R)", px + pad, y, content_w, bh, "ready_fire", nil, false)
  y = y + bh + 6
  return y
end

function draw_hud.draw_turn_end_panel(app, snap, px, pad, y, content_w)
  local bh = math.floor(24 * ui.scale)
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
  local box_w = math.min(W - 80, math.floor(420 * ui.scale))
  local box_h = math.floor(160 * ui.scale)
  local bx = (W - box_w) / 2
  local by = (H - box_h) / 2
  love.graphics.setColor(0.18, 0.14, 0.08, 0.98)
  love.graphics.rectangle("fill", bx, by, box_w, box_h, 6, 6)
  love.graphics.setColor(0.95, 0.8, 0.3)
  ui.use(18)
  love.graphics.print("End turn anyway?", bx + 16, by + 14)
  ui.use(13)
  love.graphics.setColor(0.9, 0.9, 0.92)
  love.graphics.print("There is unresolved fire or unspent power.", bx + 16, by + 48)
  local bw = math.floor(160 * ui.scale)
  ui.button("Confirm", bx + 16, by + box_h - 44, bw, 30, "end_warning_confirm", nil, false)
  ui.button("Cancel", bx + 16 + bw + 12, by + box_h - 44, bw, 30, "end_warning_cancel", nil, false)
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
    "Allocate: spend power on movement, weapon charges, shields.",
    "  End turn to advance to Movement.",
    "Movement: next uncommitted player ship (header) commits.",
    "  P=coast, T=accel, 0-5=turn, Shift+0-5=turn+accel.",
    "  Buttons: Coast, Accel, Turn, Turn+Accel (pick facing first).",
    "  Cycle N/4 shows which of the four movement windows is active.",
    "Firing: pick weapon + target + shield face, Commit Fire,",
    "  then Ready. Core resolves the shot.",
    "End Turn (E): advances the turn. Warning dialog if",
    "  there is unresolved fire or unspent power.",
    "Right-click drag to pan, wheel to zoom. Ctrl -/= to scale UI.",
    "? or H toggles this help. Esc returns to scenario picker.",
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
  love.graphics.print("Up/Down to select, Enter to start.", pad, pad + math.floor(34 * ui.scale))
  local y = pad + math.floor(70 * ui.scale)
  local bw = math.min(W - 2 * pad, math.floor(460 * ui.scale))
  local bh = math.floor(30 * ui.scale)
  for i, sc in ipairs(app.scenarios) do
    local sel = (i == app.picker_index)
    ui.button(sc.name, pad, y, bw, bh, "pick_scenario", { index = i }, sel)
    y = y + bh + 4
  end
  if #app.scenarios == 0 then
    love.graphics.setColor(0.9, 0.6, 0.4)
    love.graphics.print("No scenarios found. Check repo_root.", pad, y)
  end
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
  ui.use(11)
  local font = ui.font(11)
  local tw = font:getWidth(label)
  local margin = math.floor(8 * ui.scale)
  local x = W - tw - margin
  local y = math.floor(6 * ui.scale)
  love.graphics.setColor(0.06, 0.07, 0.09, 0.8)
  love.graphics.rectangle("fill", x - margin, y, tw + 2 * margin, font:getHeight() + 4)
  love.graphics.setColor(0.5, 0.5, 0.55)
  love.graphics.print(label, x, y + 2)
end

return draw_hud
