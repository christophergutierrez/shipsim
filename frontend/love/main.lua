-- Combat Model v2 frontend (ADR-0020).
-- Phase machine: Allocate -> Movement -> Firing -> TurnEnd, driven by snap.phase.

local harness = require("harness")
local paths = require("paths")
local phases = require("phases")
local orders = require("orders")
local allocation = require("allocation")
local end_condition = require("end_condition")
local ui_status = require("ui_status")
local draw_board = require("draw_board")
local draw_hud = require("draw_hud")
local hex = require("hex")
local ui = require("ui")
local command_mapping = require("command_mapping")
local scripted_pump = require("scripted_pump")
local preview = require("preview")
local events = require("events")
local fx = require("fx")
local selection = require("selection")
local debounce = require("debounce")

local app = {
  screen = "picker",
  scenarios = {},
  picker_index = 1,
  session = nil,
  phase = phases.ALLOCATE,
  selected_id = nil,
  weapon_id = nil,
  target_id = nil,
  shield_facing = 0,
  maneuver_facing = 0,
  cam = { x = 200, y = 120, zoom = 1.2 },
  status = ui_status.new(),
  end_result = nil,
  drag = nil,
  show_help = false,
  ghost_path = {},
  alloc = {},
  show_end_warning = false,
  fire_preview = nil,
  maneuver_options = nil,
  events = events.new(),
  fx = fx.new(),
  -- UPGRADE-PLAN Phase 4: reach-preview debounce + threat cache.
  -- reach_debounce coalesces alloc-draft bursts into ≤5 movement_preview
  -- requests/s. reach holds the last response; threats holds bearing lines
  -- cached per snapshot (never recomputed per frame).
  reach_debounce = debounce.new(),
  reach = nil,
  threats = nil,
  threats_snap_turn = nil,
}

-- ADR-0022 M6: commitments are simultaneous. Player selection skips scripted
-- ships; pump_scripted() below advances those ships until a player owes input.
-- Logic lives in the requireable `selection` module (testable headless); these
-- are thin wrappers so the rest of main.lua keeps its local names.
local function first_uncommitted_ship(snap, controller)
  return selection.first_uncommitted(snap, controller)
end

local function player_ids(snap)
  return selection.player_ids(snap)
end

local function snap_now()
  return app.session and app.session.snapshot
end

local function sync_phase()
  local snap = snap_now()
  if snap and snap.phase then
    app.phase = snap.phase
  end
  -- Surface additive snapshot fields onto app state (UPGRADE-PLAN Phase 0).
  -- These come straight from the engine; the client never computes them.
  if snap then
    app.fire_opportunity = snap.fire_opportunity or nil
    app.translation_results = snap.translation_results or nil
    app.end_turn_warning = snap.end_turn_warning or false
    app.rules_id = snap.rules_id
    app.rules_fingerprint = snap.rules_fingerprint
    -- per-ship attack_accuracy_bonus (absent = 0)
    app.attack_accuracy = {}
    for _, s in ipairs(snap.ships or {}) do
      app.attack_accuracy[s.id] = s.attack_accuracy_bonus or 0
    end
    -- Feed the event ring buffer (player_ids set for hit classification).
    -- UPGRADE-PLAN Phase 3: after feeding, inspect new combat events to spawn
    -- damage floaters (at the target ship's hex) and damage pulses (on hull
    -- loss). The event meta carries target_id + hull_damage so we don't
    -- re-parse the text. hex→pixel is geometry-for-pixels, not legality.
    local pids = {}
    for _, s in ipairs(snap.ships or {}) do
      if s.controller == "player" then pids[s.id] = true end
    end
    local prev_count = events.count(app.events)
    events.feed(app.events, snap, pids)
    local new_count = events.count(app.events)
    if new_count > prev_count then
      app.last_event_time = love.timer.getTime()
      -- Spawn floaters + pulses for new combat events.
      local size = draw_board.hex_size()
      local ship_pos = {}
      for _, s in ipairs(snap.ships or {}) do
        ship_pos[s.id] = { hex.to_pixel(s.q, s.r, size) }
      end
      local rec = events.recent(app.events, new_count - prev_count)
      for _, ev in ipairs(rec) do
        if ev.meta and ev.meta.target_id and ship_pos[ev.meta.target_id] then
          local px, py = unpack(ship_pos[ev.meta.target_id])
          -- Floater text + color by event kind.
          local text, color
          if ev.kind == "miss" then
            text = "MISS"
            color = { 0.7, 0.7, 0.75, 1 }
          elseif ev.kind == "hit_dealt" then
            text = ev.text:match("(-?%d+)$") or "-?"
            text = "-" .. text
            color = { 0.4, 0.9, 0.5, 1 }
          elseif ev.kind == "hit_taken" then
            text = ev.text:match("(-?%d+)$") or "-?"
            text = "-" .. text
            color = { 0.95, 0.35, 0.35, 1 }
          else
            text = ev.text
            color = { 0.9, 0.85, 0.4, 1 }
          end
          fx.spawn(app.fx, px, py - size * 0.3, text, { color = color })
          -- Hull damage pulse on the target ship.
          if (ev.meta.hull_damage or 0) > 0 then
            fx.pulse(app.fx, ev.meta.target_id)
          end
        end
      end
    end
  end
end

-- UPGRADE-PLAN Phase 2: dead-focus recovery (mirrors TUI app.rs:322-340).
-- A destroyed (or vanished) focus is unrecoverable by normal flow: pending-ship
-- advancement waits for the focused ship to act, and a wreck never acts —
-- allocate/fire would keep drafting orders for the dead ship and the engine
-- would reject every one. Re-focus the first living player ship and drop drafts
-- tied to the old focus. Logic lives in the requireable `selection` module
-- (testable headless); these wrappers pass an app-state slice.
local function clear_drafts_for(dead_id)
  selection.clear_drafts_for(app, dead_id)
end

local function ensure_selection()
  selection.ensure(app, snap_now())
end

-- UPGRADE-PLAN Phase 1: engine-authoritative previews.
-- Requests live in the controller (main.lua), never in draw_*.
-- The view reads app.fire_preview / app.maneuver_options; the controller
-- refreshes them when selection or phase changes. Guards mirror the TUI
-- (app.rs request_fire_preview / request_maneuver_options).

local function find_ship_in_snap(snap, id)
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

local function request_fire_preview()
  local snap = snap_now()
  if not snap or snap.phase ~= phases.FIRING then
    app.fire_preview = nil
    return
  end
  local ship_id = app.selected_id
  if not ship_id then
    app.fire_preview = nil
    return
  end
  local ship = find_ship_in_snap(snap, ship_id)
  if not ship or ship.destroyed or ship.controller ~= "player" then
    app.fire_preview = nil
    return
  end
  local weapon_id = app.weapon_id
  if not weapon_id then
    app.fire_preview = nil
    return
  end
  -- Target: explicit selection, else auto-pick first non-player living ship.
  local target = app.target_id
  if not target then
    for _, s in ipairs(snap.ships or {}) do
      if s.controller ~= "player" and not s.destroyed then
        target = s.id
        break
      end
    end
  end
  if not target then
    app.fire_preview = nil
    return
  end
  app.fire_preview = nil
  local resp, err = harness.request(app.session, {
    protocol_version = 3,
    request = "fire_preview",
    ship = ship_id,
    weapon = weapon_id,
    target = target,
  })
  if resp and resp.ok and resp.ship == ship_id and resp.weapon == weapon_id then
    app.fire_preview = resp
  end
end

local function request_maneuver_options()
  local snap = snap_now()
  if not snap or snap.phase ~= phases.MOVEMENT then
    app.maneuver_options = nil
    return
  end
  local ship_id = app.selected_id
  if not ship_id then
    app.maneuver_options = nil
    return
  end
  local ship = find_ship_in_snap(snap, ship_id)
  if not ship or ship.destroyed or ship.controller ~= "player" then
    app.maneuver_options = nil
    return
  end
  local resp, err = harness.request(app.session, {
    protocol_version = 3,
    request = "maneuver_options",
    ship = ship_id,
  })
  if resp and resp.ok and resp.ship == ship_id then
    app.maneuver_options = resp
  end
end

local function request_previews()
  request_fire_preview()
  request_maneuver_options()
end

-- UPGRADE-PLAN Phase 4: reachable-endpoint cloud. During allocate, as the
-- movement slider/keys change, issue the reach preview with clamp:true (built
-- for live drags) and store the response on app.reach for draw_board to render.
-- The debounce coalesces a burst of draft changes into ≤5 requests/s; this
-- function is the actual issuer, called from love.update when debounce.due().
-- Guards mirror the TUI: only during allocate/movement, only for a living
-- player ship. The request fields mirror `allocate` exactly (PROTOCOL).
local function request_movement_preview()
  local snap = snap_now()
  if not snap then
    app.reach = nil
    return
  end
  if snap.phase ~= phases.ALLOCATE and snap.phase ~= phases.MOVEMENT then
    app.reach = nil
    return
  end
  local ship_id = app.selected_id
  if not ship_id then
    app.reach = nil
    return
  end
  local ship = find_ship_in_snap(snap, ship_id)
  if not ship or ship.destroyed or ship.controller ~= "player" then
    app.reach = nil
    return
  end
  local a = alloc_for(ship_id)
  local resp, err = harness.request(app.session, {
    protocol_version = 3,
    request = "movement_preview",
    ship = ship_id,
    movement = a.movement or 0,
    weapons = a.weapons or {},
    shields = a.shields or { 0, 0, 0, 0, 0, 0 },
    clamp = true,
  })
  if resp and resp.ok and resp.ship == ship_id then
    app.reach = resp
  else
    app.reach = nil
  end
end

-- UPGRADE-PLAN Phase 4: threat bearing lines. For each enemy with a charged
-- weapon that the engine says can reach the selected ship, draw a thin red
-- bearing line. We reuse fire_preview with roles reversed (enemy fires at the
-- selected player ship). Cached per snapshot turn so it never runs per frame.
-- The result is an array of {from_q, from_r, to_q, to_r} for draw_board.
local function compute_threats()
  local snap = snap_now()
  if not snap or snap.phase ~= phases.FIRING then
    app.threats = nil
    app.threats_snap_turn = nil
    return
  end
  local turn_key = (snap.turn or 0) .. ":" .. (app.selected_id or "")
  if app.threats and app.threats_snap_turn == turn_key then
    return -- cache hit
  end
  app.threats_snap_turn = turn_key
  app.threats = nil
  local target_id = app.selected_id
  if not target_id then
    return
  end
  local target = find_ship_in_snap(snap, target_id)
  if not target or target.destroyed then
    return
  end
  local threats = {}
  for _, enemy in ipairs(snap.ships or {}) do
    if enemy.id ~= target_id and not enemy.destroyed
       and enemy.controller ~= "player" then
      for _, w in ipairs(enemy.weapons or {}) do
        if w.operational and (w.charge or 0) >= (w.max_charge or 1)
           and (w.max_range or 0) > 0 then
          local resp = harness.request(app.session, {
            protocol_version = 3,
            request = "fire_preview",
            ship = enemy.id,
            weapon = w.id,
            target = target_id,
          })
          if resp and resp.ok and resp.legal then
            threats[#threats + 1] = {
              from_q = enemy.q, from_r = enemy.r,
              to_q = target.q, to_r = target.r,
            }
            break -- one threat line per enemy is enough
          end
        end
      end
    end
  end
  app.threats = threats
end

local function is_player_ship(id)
  local snap = snap_now()
  if not snap or not id then
    return false
  end
  for _, s in ipairs(snap.ships or {}) do
    if s.id == id and s.controller == "player" and not s.destroyed then
      return true
    end
  end
  return false
end

local function check_end()
  local result = end_condition.evaluate(snap_now())
  if result then
    app.end_result = result
    app.screen = "end"
  end
end

local function submit(order, keep_status)
  local snap, err = harness.submit(app.session, order)
  if err then
    ui_status.from_error(app.status, err)
  elseif not keep_status then
    ui_status.clear(app.status)
  end
  sync_phase()
  ensure_selection()
  scripted_pump.run(app.session, function(err) ui_status.from_error(app.status, err) end)
  sync_phase()
  ensure_selection()
  request_previews()
  check_end()
  return snap, err
end

local function center_camera()
  local snap = app.session and app.session.snapshot
  if not snap or not snap.map then
    return
  end
  local cx, cy = hex.to_pixel((snap.map.width or 1) / 2, (snap.map.height or 1) / 2, draw_board.hex_size())
  local pw = draw_hud.panel_width()
  local board_w = love.graphics.getWidth() - pw
  app.cam.x = board_w / 2 - cx * app.cam.zoom
  app.cam.y = (love.graphics.getHeight() + draw_hud.top_h()) / 2 - cy * app.cam.zoom
end

function love.update(dt)
  -- Tick transient effects (damage floaters, Phase 5 resolution theater).
  -- fx is pure Lua; safe to tick every frame even when no effects are active.
  if app.fx then
    fx.update(app.fx, dt)
  end
  -- UPGRADE-PLAN Phase 4: coalesce movement_preview requests. The debounce is
  -- tripped whenever an alloc draft changes (handle_ui_hit alloc_* handlers).
  -- Each frame we advance the timer; when the quiet window elapses, fire the
  -- single coalesced request and disarm. This keeps live slider drags to ≤5
  -- requests/s without stalling the frame on a synchronous harness.request.
  if app.reach_debounce then
    debounce.poke(app.reach_debounce, dt)
    if debounce.due(app.reach_debounce) then
      request_movement_preview()
      debounce.consume(app.reach_debounce)
    end
  end
  -- UPGRADE-PLAN Phase 4: threat bearing lines are cached per snapshot turn
  -- (compute_threats checks its own cache). Calling it here every frame is
  -- cheap — the cache hit returns immediately — and keeps the view fresh when
  -- the selected ship changes without coupling to selection events.
  compute_threats()
end

function love.load()
  app.repo_root = paths.find_repo_root()
  app.scenarios = paths.list_scenarios(app.repo_root)
  if #app.scenarios == 0 then
    ui_status.set(app.status, "error", "No scenarios. repo=" .. tostring(app.repo_root))
  else
    ui_status.set(app.status, "info", "v2: Allocate power, Move, Commit fire, Ready, End turn. ? for help.")
  end
end

local function start_scenario(entry)
  app.session = harness.new({ repo_root = app.repo_root })
  local snap, err = harness.load_scenario(app.session, entry.path)
  if err and not snap then
    ui_status.from_error(app.status, err)
    return
  end
  app.screen = "play"
  app.weapon_id = nil
  app.target_id = nil
  app.shield_facing = 0
  app.maneuver_facing = 0
  app.alloc = {}
  app.show_end_warning = false
  scripted_pump.run(app.session, function(err) ui_status.from_error(app.status, err) end)
  sync_phase()
  ensure_selection()
  request_previews()
  center_camera()
  ui_status.set(app.status, "info", "Allocate power for your ships, then End turn to move.")
end

local SHIELD_FACE = { "F", "FR", "RR", "R", "RL", "FL" }

local function fire_result_message(snap, weapon, target_id)
  if not snap or not snap.combat_log or #snap.combat_log == 0 then
    return "Shot resolved (no log entry)"
  end
  local e = snap.combat_log[#snap.combat_log]
  local face = SHIELD_FACE[(e.shield or 0) + 1] or tostring(e.shield)
  if e.kind == "miss" or (e.damage or 0) == 0 then
    return string.format("MISS — %s vs ship #%s (face %s)", weapon, tostring(target_id), face)
  end
  return string.format("HIT — %s vs ship #%s face %s for %d damage", weapon, tostring(e.target or target_id), face, e.damage or 0)
end

local function alloc_for(ship_id)
  if not app.alloc[ship_id] then
    app.alloc[ship_id] = { movement = 0, weapons = {}, shields = { 0, 0, 0, 0, 0, 0 } }
  end
  return app.alloc[ship_id]
end

local function do_allocate(ship_id)
  if not is_player_ship(ship_id) then
    ui_status.set(app.status, "warn", "Not your ship")
    return
  end
  local a = alloc_for(ship_id)
  local _, err = submit(orders.allocate(ship_id, a.movement, a.weapons, a.shields), true)
  if not err then
    ui_status.set(app.status, "info", string.format("Ship #%d allocated (move %d)", ship_id, a.movement))
  end
end

local function do_movement(action, facing)
  local snap = snap_now()
  if not snap or snap.phase ~= phases.MOVEMENT then
    ui_status.set(app.status, "warn", "Not movement phase")
    return
  end
  -- Always pick the next uncommitted *player* ship (never AI/scripted).
  local ship = first_uncommitted_ship(snap, "player")
  if not ship or not is_player_ship(ship) then
    ui_status.set(app.status, "warn", "Not your move — active is #" .. tostring(ship))
    return
  end
  local order = command_mapping.movement_order(action, ship, facing)
  if not order then
    ui_status.set(app.status, "warn", "Unknown maneuver")
    return
  end
  local _, err = submit(order, true)
  if not err then
    local label = action
    if facing then label = label .. " " .. facing end
    ui_status.set(app.status, "info", string.format("Ship #%d %s", ship, label))
  end
end

local function do_commit_fire()
  local snap = snap_now()
  local ship = app.selected_id
  if not ship or not is_player_ship(ship) then
    ui_status.set(app.status, "warn", "Select one of your ships")
    return
  end
  if not (app.weapon_id and app.target_id) then
    ui_status.set(app.status, "warn", "Pick weapon and target")
    return
  end
  local weapon = app.weapon_id
  local target = app.target_id
  local snap2, err = submit(orders.commit_fire(ship, weapon, target, app.shield_facing), true)
  if not err then
    ui_status.set(app.status, "info", fire_result_message(snap2 or snap_now(), weapon, target))
  end
end

local function do_ready_fire()
  local ship = app.selected_id
  if not ship or not is_player_ship(ship) then
    ui_status.set(app.status, "warn", "Select one of your ships")
    return
  end
  local _, err = submit(orders.ready_fire(ship), true)
  if not err then
    ui_status.set(app.status, "info", string.format("Ship #%d readied", ship))
  end
end

local function do_end_turn()
  local snap = snap_now()
  if snap and snap.end_turn_warning and not app.show_end_warning then
    app.show_end_warning = true
    return
  end
  app.show_end_warning = false
  local _, err = submit(orders.end_turn(), true)
  if not err then
    app.alloc = {}
    ui_status.set(app.status, "info", "Turn ended")
  end
end

local function handle_ui_hit(hit)
  if not hit then
    return false
  end
  local id = hit.id
  local p = hit.payload or {}
  if id == "toggle_help" then
    app.show_help = not app.show_help
    return true
  end
  if id == "pick_scenario" then
    app.picker_index = p.index
    local sc = app.scenarios[p.index]
    if sc then
      start_scenario(sc)
    end
    return true
  end
  if id == "select_ship" then
    app.selected_id = p.id
    request_previews()
    return true
  end
  if id == "alloc_movement_up" then
    local a = alloc_for(p.id)
    a.movement = a.movement + 1
    debounce.trip(app.reach_debounce)
    return true
  end
  if id == "alloc_movement_dn" then
    local a = alloc_for(p.id)
    a.movement = math.max(0, a.movement - 1)
    debounce.trip(app.reach_debounce)
    return true
  end
  if id == "alloc_weapon_up" then
    local a = alloc_for(p.id)
    a.weapons[p.weapon] = allocation.increment(a.weapons[p.weapon], p.max)
    debounce.trip(app.reach_debounce)
    return true
  end
  if id == "alloc_weapon_dn" then
    local a = alloc_for(p.id)
    a.weapons[p.weapon] = allocation.decrement(a.weapons[p.weapon])
    debounce.trip(app.reach_debounce)
    return true
  end
  if id == "alloc_shield_up" then
    local a = alloc_for(p.id)
    a.shields[p.face + 1] = allocation.increment(a.shields[p.face + 1], p.max)
    debounce.trip(app.reach_debounce)
    return true
  end
  if id == "alloc_shield_dn" then
    local a = alloc_for(p.id)
    a.shields[p.face + 1] = allocation.decrement(a.shields[p.face + 1])
    debounce.trip(app.reach_debounce)
    return true
  end
  if id == "alloc_confirm" then
    do_allocate(p.id)
    return true
  end
  if id == "coast" then
    do_movement("coast")
    return true
  end
  if id == "accel" then
    do_movement("accel")
    return true
  end
  if id == "turn" then
    do_movement("turn", app.maneuver_facing or 0)
    return true
  end
  if id == "turn_accel" then
    do_movement("turn_accel", app.maneuver_facing or 0)
    return true
  end
  if id == "pick_maneuver_facing" then
    app.maneuver_facing = p.face
    return true
  end
  if id == "pick_weapon" then
    app.weapon_id = p.id
    request_fire_preview()
    return true
  end
  if id == "pick_target" then
    app.target_id = p.id
    request_fire_preview()
    return true
  end
  if id == "pick_shield_facing" then
    app.shield_facing = p.face
    request_fire_preview()
    return true
  end
  if id == "fire_confirm" then
    do_commit_fire()
    return true
  end
  if id == "ready_fire" then
    do_ready_fire()
    return true
  end
  if id == "end_turn" then
    do_end_turn()
    return true
  end
  if id == "end_warning_confirm" then
    app.show_end_warning = false
    submit(orders.end_turn(), true)
    app.alloc = {}
    return true
  end
  if id == "end_warning_cancel" then
    app.show_end_warning = false
    return true
  end
  if id == "select_active" then
    ensure_selection()
    return true
  end
  if id == "menu" then
    if app.session then harness.kill(app.session) end
    app.screen = "picker"
    app.session = nil
    return true
  end
  return false
end

function love.draw()
  ui.clear_hits()
  love.graphics.clear(0.08, 0.09, 0.11)
  if app.screen == "picker" then
    draw_hud.draw_picker(app)
  elseif app.screen == "play" then
    draw_board.draw(app.session and app.session.snapshot, app.cam, app.selected_id, nil, {
      weapon_id = app.weapon_id,
      target_id = app.target_id,
      fx = app.fx,
      reach = app.reach,
      threats = app.threats,
    })
    -- Draw transient effects (damage floaters) inside the camera transform
    -- so world-space x/y land on the right hex. (Phase 5 resolution theater.)
    -- draw_board.draw pops its own transform, so we re-apply the camera here.
    if app.fx then
      love.graphics.push()
      love.graphics.translate(app.cam.x, app.cam.y)
      love.graphics.scale(app.cam.zoom, app.cam.zoom)
      for _, e in ipairs(fx.active(app.fx)) do
        local a = fx.alpha(e)
        if a > 0 then
          local c = e.color or { 1, 0.85, 0.2 }
          love.graphics.setColor(c[1], c[2], c[3], a)
          love.graphics.print(e.text, e.x, e.y)
        end
      end
      love.graphics.setColor(1, 1, 1)
      love.graphics.pop()
    end
    draw_hud.draw(app)
    draw_hud.rules_provenance(app)
    if app.show_end_warning then
      draw_hud.draw_end_warning(app)
    end
  elseif app.screen == "end" then
    ui.use(28)
    love.graphics.setColor(1, 1, 1)
    love.graphics.print(app.end_result == "won" and "VICTORY" or "DEFEAT", 40, 40)
    ui.button("Back", 40, 100, 200, 36, "menu", nil, true)
  end
  draw_hud.status_strip(app.status)
end

local function ctrl_down()
  return love.keyboard.isDown("lctrl", "rctrl")
end

function love.keypressed(key)
  if ctrl_down() and (key == "=" or key == "kp+" or key == "+") then
    ui.adjust_scale(0.15)
    return
  end
  if ctrl_down() and (key == "-" or key == "kp-") then
    ui.adjust_scale(-0.15)
    return
  end
  if key == "h" or key == "/" then
    app.show_help = not app.show_help
    return
  end
  if app.show_help then
    if key == "escape" then
      app.show_help = false
    end
    return
  end
  if app.screen == "picker" then
    if key == "up" then
      app.picker_index = math.max(1, app.picker_index - 1)
    elseif key == "down" then
      app.picker_index = math.min(#app.scenarios, app.picker_index + 1)
    elseif key == "return" or key == "kpenter" then
      local sc = app.scenarios[app.picker_index]
      if sc then
        start_scenario(sc)
      end
    end
    return
  end
  if app.screen == "end" then
    if key == "return" or key == "escape" then
      if app.session then harness.kill(app.session) end
      app.screen = "picker"
      app.session = nil
    end
    return
  end
  if key == "escape" then
    if app.session then harness.kill(app.session) end
    app.screen = "picker"
    app.session = nil
    return
  end
  if key == "return" or key == "kpenter" then
    if app.phase == phases.FIRING then
      do_commit_fire()
    elseif app.phase == phases.TURN_END then
      do_end_turn()
    end
  elseif key == "e" then
    do_end_turn()
  elseif key == "p" and app.phase == phases.MOVEMENT then
    do_movement("coast")
  elseif key == "t" and app.phase == phases.MOVEMENT then
    do_movement("accel")
  elseif key:match("^[0-5]$") and app.phase == phases.MOVEMENT then
    local face = tonumber(key)
    app.maneuver_facing = face
    -- Shift+digit = turn_accel (TUI Alt+digit equivalent); plain digit = turn.
    if love.keyboard.isDown("lshift") or love.keyboard.isDown("rshift") then
      do_movement("turn_accel", face)
    else
      do_movement("turn", face)
    end
  elseif key == "r" then
    do_ready_fire()
  end
end

function love.textinput(t)
  if t == "?" then
    app.show_help = not app.show_help
  end
end

function love.mousepressed(x, y, button)
  local hit = ui.hit_at(x, y)
  if hit and button == 1 then
    if handle_ui_hit(hit) then
      return
    end
  end
  if app.screen ~= "play" then
    return
  end
  if button == 2 or button == 3 then
    app.drag = { x = x, y = y, camx = app.cam.x, camy = app.cam.y }
    return
  end
  if button ~= 1 then
    return
  end
  if y < draw_hud.top_h() or x > love.graphics.getWidth() - draw_hud.panel_width() then
    return
  end
  if y > love.graphics.getHeight() - draw_hud.bottom_h() then
    return
  end
  local q, r = draw_board.screen_to_hex(x, y, app.cam)
  local snap = app.session and app.session.snapshot
  if not snap then
    return
  end
  for _, s in ipairs(snap.ships or {}) do
    if s.q == q and s.r == r and not s.destroyed then
      if s.controller == "player" then
        app.selected_id = s.id
        request_previews()
      else
        app.target_id = s.id
        request_fire_preview()
      end
      return
    end
  end
end

function love.mousereleased(_, _, button)
  if button == 2 or button == 3 then
    app.drag = nil
  end
end

function love.mousemoved(x, y)
  if app.drag then
    app.cam.x = app.drag.camx + (x - app.drag.x)
    app.cam.y = app.drag.camy + (y - app.drag.y)
  end
end

function love.wheelmoved(_, y)
  if y > 0 then
    app.cam.zoom = math.min(4, app.cam.zoom * 1.1)
  elseif y < 0 then
    app.cam.zoom = math.max(0.3, app.cam.zoom / 1.1)
  end
end

function love.quit()
  if app.session then harness.kill(app.session) end
end
