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
local slide = require("slide")
local json = require("json")
local tutorial = require("tutorial")
local layout = require("layout")
local status_fmt = require("status_fmt")
local settings = require("settings")
local toast = require("toast")
local camera = require("camera")

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
  slide = slide.new(),
  fx_enabled = true,
  session_log_path = nil,
  reach_debounce = debounce.new(),
  reach = nil,
  threats = nil,
  threats_snap_turn = nil,
  tutorial = nil,
  tutorial_order_pending = false,
  tutorial_order_candidate = nil,
  -- FIX-PLAN F2/F4
  target_previews = {}, -- [target_id] = fire_preview response (cached)
  target_previews_key = nil,
  toast = toast.new(),
  cam_sys = nil, -- filled in love.load
  settings = settings.defaults(),
  prev_phase = nil,
  _need_weapon_pick = false,
}

-- Forward declarations (defined later; needed from sync_phase / request_previews).
local next_fireable_weapon
local apply_legal_shield_facing

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

-- Forward-usable: request_movement_preview and key handlers call this before
-- do_allocate is defined later in the file.
-- Seeds weapon draft from carried charge so Allocate does not try to strip
-- (PROTOCOL: charge carries; cannot lower below current total).
local function alloc_for(ship_id)
  if not app.alloc[ship_id] then
    local weapons = json.object({})
    local snap = app.session and app.session.snapshot
    if snap then
      for _, s in ipairs(snap.ships or {}) do
        if s.id == ship_id then
          for _, w in ipairs(s.weapons or {}) do
            if (w.charge or 0) > 0 then
              weapons[w.id] = w.charge
            end
          end
          break
        end
      end
    end
    app.alloc[ship_id] = {
      movement = 0,
      weapons = weapons,
      shields = { 0, 0, 0, 0, 0, 0 },
    }
  end
  -- Re-tag after any mutation path so empty maps stay JSON objects.
  if not getmetatable(app.alloc[ship_id].weapons) then
    app.alloc[ship_id].weapons = json.object(app.alloc[ship_id].weapons or {})
  end
  return app.alloc[ship_id]
end

local function snap_now()
  return app.session and app.session.snapshot
end

local function set_status(level, message)
  ui_status.set(app.status, level, message)
  status_fmt.stamp(app.status, snap_now())
end

local function clear_phase_overlays(new_phase)
  -- F2 D4: overlays from allocate/movement die outside those phases.
  if new_phase ~= phases.ALLOCATE and new_phase ~= phases.MOVEMENT then
    app.reach = nil
  end
  if new_phase ~= phases.MOVEMENT then
    app.maneuver_options = nil
    app.ghost_path = {}
  end
  if new_phase ~= phases.FIRING then
    app.fire_preview = nil
    app.target_previews = {}
    app.target_previews_key = nil
  end
end

local function sync_phase()
  local snap = snap_now()
  local old_phase = app.phase
  if snap and snap.phase then
    app.phase = snap.phase
  end
  if app.phase ~= old_phase then
    clear_phase_overlays(app.phase)
    status_fmt.clear_if_stale(app.status, snap)
    local label = toast.phase_label(snap, old_phase)
    if label and app.screen == "play" then
      toast.show(app.toast, label)
    end
    app.prev_phase = old_phase
    -- Entering movement: default turn target off current nose (same-facing is illegal).
    if app.phase == phases.MOVEMENT and snap then
      local sid = selection.first_uncommitted(snap, "player") or app.selected_id
      local sh = nil
      if snap.ships then
        for _, s in ipairs(snap.ships) do
          if s.id == sid then sh = s; break end
        end
      end
      if sh then
        local face = sh.facing or 0
        if (app.maneuver_facing or 0) == face then
          app.maneuver_facing = (face + 1) % 6
        end
      end
    end
    -- Entering firing: clear spent weapon selection so request_previews can re-pick.
    if app.phase == phases.FIRING then
      app._need_weapon_pick = true
    end
  else
    status_fmt.clear_if_stale(app.status, snap)
  end
  -- Surface additive snapshot fields onto app state (UPGRADE-PLAN Phase 0).
  if snap then
    app.fire_opportunity = snap.fire_opportunity or nil
    app.translation_results = snap.translation_results or nil
    app.end_turn_warning = snap.end_turn_warning or false
    app.rules_id = snap.rules_id
    app.rules_fingerprint = snap.rules_fingerprint
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
          -- UPGRADE-PLAN Phase 5: fire animation on resolve. When a volley
          -- resolves (new combat_log entries), spawn tracers from the attacker
          -- to the target. beam = instant line flash, torp = moving dot,
          -- plasma = expanding bolt. Skippable via fx_enabled=false. Input
          -- stays live — animations are cosmetic and never block orders.
          if app.fx_enabled and ev.meta.attacker and ship_pos[ev.meta.attacker] then
            local ax, ay = unpack(ship_pos[ev.meta.attacker])
            local kind = "beam"
            local wname = ev.text:match("%S+%s+(%S+)%s")
            if wname and wname:match("^torp") then
              kind = "torp"
            elseif wname and wname:match("^plasma") then
              kind = "plasma"
            end
            local hit = ev.kind ~= "miss"
            fx.tracer(app.fx, ax, ay, px, py, kind, hit)
          end
        end
      end
    end
    -- UPGRADE-PLAN Phase 5: slide interpolation. Feed the snapshot's ship
    -- positions so ships lerp between their previous and current hex over
    -- 0.3s instead of teleporting. hex.to_pixel is geometry-for-pixels.
    slide.feed(app.slide, snap, hex.to_pixel, draw_board.hex_size())
    -- UPGRADE-PLAN Phase 6: detect unexpected game-over mid-lesson (mirrors
    -- TUI tutorial.rs:275-288 state_error). If the game ended but the tutorial
    -- step is not Dismiss, surface the error so the coach panel can show it.
    if app.tutorial then
      local serr = tutorial.state_error(app.tutorial, snap)
      if serr then
        tutorial.set_error(app.tutorial, serr)
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

--- F4.1: one fire_preview per enemy for selected weapon; cached per weapon+snap.
local function request_target_previews()
  local snap = snap_now()
  if not snap or snap.phase ~= phases.FIRING then
    app.target_previews = {}
    app.target_previews_key = nil
    return
  end
  local ship_id = app.selected_id
  local weapon_id = app.weapon_id
  if not ship_id or not weapon_id then
    app.target_previews = {}
    app.target_previews_key = nil
    return
  end
  local key = string.format("%s:%s:%s:%s", snap.turn or 0, snap.phase or "", ship_id, weapon_id)
  if app.target_previews_key == key then
    return
  end
  app.target_previews = {}
  app.target_previews_key = key
  local enemies = layout.enemy_targets(snap, ship_id)
  for _, s in ipairs(enemies) do
    local resp = harness.request(app.session, {
      protocol_version = 3,
      request = "fire_preview",
      ship = ship_id,
      weapon = weapon_id,
      target = s.id,
    })
    if resp and resp.ok then
      app.target_previews[s.id] = resp
    end
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
  -- Auto-pick first charged weapon when entering fire or after a volley shot.
  if app.phase == phases.FIRING and app.selected_id then
    local snap = snap_now()
    if snap and (app._need_weapon_pick or not app.weapon_id) then
      app.weapon_id = next_fireable_weapon(snap, app.selected_id, nil)
      app._need_weapon_pick = false
    end
  end
  request_fire_preview()
  request_target_previews()
  request_maneuver_options()
  if app.phase == phases.FIRING then
    apply_legal_shield_facing()
  end
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
    -- Must be JSON object {} not [] (engine BTreeMap) — see json.object.
    weapons = json.object(a.weapons or {}),
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
  -- UPGRADE-PLAN Phase 6: confirm order-backed tutorial steps only after
  -- the engine accepted the order (mirrors app.rs:735-744 confirm_tutorial_order).
  if app.tutorial and app.tutorial_order_pending then
    tutorial.advance(app.tutorial)
    app.tutorial_order_pending = false
  end
  ensure_selection()
  scripted_pump.run(app.session, function(err) ui_status.from_error(app.status, err) end)
  sync_phase()
  ensure_selection()
  request_previews()
  check_end()
  return snap, err
end

-- UPGRADE-PLAN Phase 6: tutorial gate. Mirrors the TUI's tutorial_gate
-- (input.rs:254-373) and map_key_to_action (input.rs:946-1026), adapted to
-- the Love2D input model (mouse buttons + keyboard shortcuts). The gate
-- intercepts both handle_ui_hit (mouse) and love.keypressed (keyboard)
-- before the normal dispatch. Returns true if the input is blocked.
--
-- Order-backed steps (CommitAllocate, Accel, TurnTo, Coast, FireWeapon,
-- ReadyFire, EndTurn) validate but do NOT advance — the caller advances only
-- after submit() returns an accepted snapshot (confirm_tutorial_order above).
-- Discrete steps (NavField, ShieldFacing, EnterMap, PanMap, etc.) advance
-- immediately via check_action.

-- Mark a validated order-backed action as emitted (mirrors app.rs:746-750).
local function mark_tutorial_order_emitted()
  if app.tutorial and app.tutorial_order_candidate ~= nil then
    app.tutorial_order_candidate = nil
    app.tutorial_order_pending = true
  end
end

-- Gate a mouse/UI hit. Returns true if blocked.
local function tutorial_gate_ui(hit)
  if not app.tutorial or tutorial.is_complete(app.tutorial) then
    return false
  end
  if not hit then
    return false
  end
  local step = tutorial.current_step(app.tutorial)
  if not step then
    return false
  end
  local expected = step.expected
  local id = hit.id
  local p = hit.payload or {}

  -- Always allow leaving the session / quitting the app.
  if id == "menu" or id == "quit" or id == "toggle_help" then
    return false
  end

  -- ReachValue steps: allow the up/down button for the correct field only.
  if expected.kind == "ReachValue" then
    local need_field = expected.field
    if id == "alloc_movement_up" or id == "alloc_movement_dn" then
      if need_field == 0 then return false end
      tutorial.set_error(app.tutorial, ("Wrong field (need %s). %s"):format(
        ({[0]="Movement",[1]="beam_1",[2]="torp_1",[3]="plasma_1",
          [4]="shield F"})[need_field] or ("field "..need_field), step.hint))
      return true
    end
    if id == "alloc_weapon_up" or id == "alloc_weapon_dn" then
      local wmap = { beam_1 = 1, torp_1 = 2, plasma_1 = 3 }
      if wmap[p.weapon] == need_field then return false end
      tutorial.set_error(app.tutorial, ("Wrong field (need %s). %s"):format(
        ({[0]="Movement",[1]="beam_1",[2]="torp_1",[3]="plasma_1",
          [4]="shield F"})[need_field] or ("field "..need_field), step.hint))
      return true
    end
    if id == "alloc_shield_up" or id == "alloc_shield_dn" then
      if 4 + (p.face or 0) == need_field then return false end
      tutorial.set_error(app.tutorial, ("Wrong shield face (need field %d). %s"):format(need_field, step.hint))
      return true
    end
    if id == "alloc_confirm" then
      tutorial.set_error(app.tutorial, ("Set %s to %d first. %s"):format(
        ({[0]="Movement",[1]="beam_1",[2]="torp_1",[3]="plasma_1",
          [4]="shield F"})[expected.field] or ("field "..expected.field),
        expected.target, step.hint))
      return true
    end
    tutorial.set_error(app.tutorial, ("Expected: %s. %s"):format(step.title, step.hint))
    return true
  end

  -- NavField: allow alloc buttons (check_action handles advance on ==).
  if expected.kind == "NavField" then
    return false
  end

  -- CommitAllocate: allow the confirm button.
  if expected.kind == "CommitAllocate" then
    if id == "alloc_confirm" then
      if tutorial.validate_action(app.tutorial, { kind = "CommitAllocate" }) then
        app.tutorial_order_candidate = { kind = "CommitAllocate" }
        return false
      end
    end
    tutorial.set_error(app.tutorial, ("Expected: %s. %s"):format(step.title, step.hint))
    return true
  end

  -- Accel: allow the accel button.
  if expected.kind == "Accel" then
    if id == "accel" then
      if tutorial.validate_action(app.tutorial, { kind = "Accel" }) then
        app.tutorial_order_candidate = { kind = "Accel" }
        return false
      end
    end
    tutorial.set_error(app.tutorial, ("Expected: %s. %s"):format(step.title, step.hint))
    return true
  end

  -- FireWeapon: allow the fire_confirm button.
  if expected.kind == "FireWeapon" then
    if id == "fire_confirm" then
      if tutorial.validate_action(app.tutorial, { kind = "FireWeapon" }) then
        app.tutorial_order_candidate = { kind = "FireWeapon" }
        return false
      end
    end
    tutorial.set_error(app.tutorial, ("Expected: %s. %s"):format(step.title, step.hint))
    return true
  end

  -- ReadyFire: allow the ready_fire button.
  if expected.kind == "ReadyFire" then
    if id == "ready_fire" then
      if tutorial.validate_action(app.tutorial, { kind = "ReadyFire" }) then
        app.tutorial_order_candidate = { kind = "ReadyFire" }
        return false
      end
    end
    tutorial.set_error(app.tutorial, ("Expected: %s. %s"):format(step.title, step.hint))
    return true
  end

  -- ShieldFacing: allow pick_shield_facing (check_action advances on ==).
  if expected.kind == "ShieldFacing" then
    if id == "pick_shield_facing" then
      return not tutorial.check_action(app.tutorial, { kind = "ShieldFacing", facing = p.face })
    end
    tutorial.set_error(app.tutorial, ("Expected: %s. %s"):format(step.title, step.hint))
    return true
  end

  -- Default: block unrecognized hits during tutorial.
  tutorial.set_error(app.tutorial, ("Expected: %s. %s"):format(step.title, step.hint))
  return true
end

-- Gate a keyboard input. Returns true if blocked.
local function tutorial_gate_key(key)
  if not app.tutorial or tutorial.is_complete(app.tutorial) then
    return false
  end
  local snap = snap_now()
  if not snap then return false end
  local step = tutorial.current_step(app.tutorial)
  if not step then return false end
  local expected = step.expected
  local phase = app.phase or snap.phase

  -- Map key → action (mirrors map_key_to_action, input.rs:946-1026).
  local action = nil
  if key == "v" then
    action = { kind = "EnterMap" }
  elseif key == "c" then
    action = { kind = "RecenterMap" }
  elseif key == "e" then
    action = { kind = "EndTurn" }
  elseif phase == phases.MOVEMENT then
    if key == "t" then action = { kind = "Accel" }
    elseif key == "p" then action = { kind = "Coast" }
    elseif key:match("^[0-5]$") then action = { kind = "TurnTo", facing = tonumber(key) }
    end
  elseif phase == phases.FIRING then
    if key == "return" or key == "kpenter" then action = { kind = "FireWeapon" }
    elseif key == "r" or key == "space" then action = { kind = "ReadyFire" }
    end
  elseif phase == phases.TURN_END then
    if key == "return" or key == "kpenter" or key == "e" then action = { kind = "EndTurn" } end
  end

  if not action then
    -- Help and app quit always allowed (user must be able to leave).
    if key == "h" or key == "/" or key == "?" or key == "q" then return false end
    tutorial.set_error(app.tutorial, ("Expected: %s. %s"):format(step.title, step.hint))
    return true
  end

  -- Order-backed steps: validate but don't advance.
  local order_backed = (expected.kind == "CommitAllocate"
    or expected.kind == "Accel" or expected.kind == "TurnTo"
    or expected.kind == "Coast" or expected.kind == "FireWeapon"
    or expected.kind == "ReadyFire" or expected.kind == "EndTurn")

  if order_backed then
    if tutorial.validate_action(app.tutorial, action) then
      app.tutorial_order_candidate = action
      return false
    end
    return true
  end

  -- Discrete steps: check and advance immediately.
  return not tutorial.check_action(app.tutorial, action)
end

local function center_camera()
  local snap = app.session and app.session.snapshot
  if not snap or not snap.map then
    return
  end
  local cx, cy = hex.to_pixel((snap.map.width or 1) / 2, (snap.map.height or 1) / 2, draw_board.hex_size())
  local pw = draw_hud.panel_width()
  app.cam.x, app.cam.y = draw_hud.board_camera_origin(
    love.graphics.getWidth(),
    love.graphics.getHeight(),
    pw,
    draw_hud.top_h(),
    draw_hud.bottom_h(),
    cx, cy, app.cam.zoom
  )
end

function love.update(dt)
  -- Tick transient effects (damage floaters, Phase 5 resolution theater).
  -- fx is pure Lua; safe to tick every frame even when no effects are active.
  if app.fx then
    fx.update(app.fx, dt)
  end
  -- UPGRADE-PLAN Phase 5: slide interpolation. Advance all ship slides by dt
  -- so ships lerp between snapshot positions over 0.3s instead of teleporting.
  -- slide is pure Lua; safe to tick every frame.
  if app.slide then
    slide.update(app.slide, dt)
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
  compute_threats()
  -- F3 hold-to-repeat steppers.
  if love.mouse.isDown(1) then
    local mx, my = love.mouse.getPosition()
    local rep = ui.press_tick(dt, true, mx, my)
    if rep and ui.is_repeatable(rep.id) then
      handle_ui_hit(rep)
    end
  else
    ui.press_end()
  end
  -- F4 toast + auto-follow camera.
  toast.update(app.toast, dt)
  if app.screen == "play" and app.cam_sys then
    local snap = snap_now()
    local living = {}
    if snap then
      for _, s in ipairs(snap.ships or {}) do
        if not s.destroyed then
          living[#living + 1] = s
        end
      end
    end
    local W = love.graphics.getWidth()
    local H = love.graphics.getHeight()
    local pw = draw_hud.panel_width()
    camera.update(app.cam_sys, dt, living, hex.to_pixel, draw_board.hex_size(), {
      x = 0, y = draw_hud.top_h(), w = W - pw, h = H - draw_hud.top_h() - draw_hud.bottom_h(),
    })
  end
end

function love.load()
  -- Do not call love.window.setMode here: it recreates the X window after
  -- launch and undoes i3 floating applied by frontend/love/play.sh.
  app.repo_root = paths.find_repo_root()
  app.scenarios = paths.list_scenarios(app.repo_root)
  app.cam_sys = camera.new(app.cam)
  -- F3: DPI default scale + restore settings.
  local settings_path = paths.local_dir() .. "/settings.json"
  app.settings = settings.load(settings_path, json)
  local w = love.graphics.getWidth()
  local h = love.graphics.getHeight()
  if app.settings.ui_scale then
    ui.set_scale(app.settings.ui_scale)
  else
    ui.set_scale(layout.default_scale(w, h))
  end
  if app.settings.auto_follow == false then
    camera.set_auto(app.cam_sys, false)
  end
  if #app.scenarios == 0 then
    set_status("error", "No scenarios. repo=" .. tostring(app.repo_root))
  else
    set_status("info", "v2: Allocate, Move, Fire, End turn. ? help · Exit/Q quits.")
  end
end

local function persist_settings()
  app.settings.ui_scale = ui.scale
  app.settings.auto_follow = app.cam_sys and app.cam_sys.auto or true
  settings.save(paths.local_dir() .. "/settings.json", app.settings, json)
end

function love.resize()
  if app.screen == "play" then
    center_camera()
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
  -- UPGRADE-PLAN Phase 6: detect the tutorial scenario by filename (mirrors
  -- the TUI's --tutorial flag, main.rs:32-39). The TUI uses a CLI flag; the
  -- Love2D frontend has no CLI args, so we match the scenario basename.
  -- tutorial.lua is a pure-Lua step-gate machine; nil in free play.
  local basename = entry.path:match("([^/]+)$") or entry.path
  if basename == "tutorial_rear_attack.toml" then
    app.tutorial = tutorial.new()
    app.tutorial_order_pending = false
    app.tutorial_order_candidate = nil
  else
    app.tutorial = nil
    app.tutorial_order_pending = false
    app.tutorial_order_candidate = nil
  end
  scripted_pump.run(app.session, function(err) ui_status.from_error(app.status, err) end)
  sync_phase()
  ensure_selection()
  request_previews()
  center_camera()
  if app.tutorial then
    ui_status.set(app.status, "info", "Tutorial: " .. tutorial.OBJECTIVE)
  else
    ui_status.set(app.status, "info", "Allocate power for your ships, then End turn to move.")
  end
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

local function do_allocate(ship_id)
  if not is_player_ship(ship_id) then
    set_status("warn", "Not your ship")
    return
  end
  local a = alloc_for(ship_id)
  local _, err = submit(orders.allocate(ship_id, a.movement, a.weapons, a.shields), true)
  if not err then
    mark_tutorial_order_emitted()
    set_status("info", status_fmt.order_echo(ship_id, "allocate") ..
      string.format(" (move %d)", a.movement))
  end
end

local function do_movement(action, facing)
  local snap = snap_now()
  if not snap or snap.phase ~= phases.MOVEMENT then
    set_status("warn", "Not movement phase")
    return
  end
  local ship_id = first_uncommitted_ship(snap, "player")
  if not ship_id or not is_player_ship(ship_id) then
    set_status("warn", "Not your move")
    return
  end
  local ship = find_ship_in_snap(snap, ship_id)
  -- Engine rejects turn/turn_accel to the current facing as a no-op.
  if (action == "turn" or action == "turn_accel") and ship
      and facing ~= nil and facing == (ship.facing or 0) then
    set_status("warn", string.format(
      "Already facing %d — pick another facing, or Coast (P)", facing))
    return
  end
  local order = command_mapping.movement_order(action, ship_id, facing)
  if not order then
    set_status("warn", "Unknown maneuver")
    return
  end
  local _, err = submit(order, true)
  if not err then
    mark_tutorial_order_emitted()
    set_status("info", status_fmt.order_echo(ship_id, action, facing))
  end
end

--- Next charged weapon not already in fire_commits for this ship (multi-volley).
next_fireable_weapon = function(snap, ship_id, exclude)
  local ship = find_ship_in_snap(snap, ship_id)
  if not ship then
    return nil
  end
  local committed = {}
  for _, c in ipairs(snap.fire_commits or {}) do
    if c.ship == ship_id and c.weapon then
      committed[c.weapon] = true
    end
  end
  if exclude then
    committed[exclude] = true
  end
  for _, w in ipairs(ship.weapons or {}) do
    if w.operational ~= false and (w.charge or 0) > 0 and not committed[w.id]
        and not w.fired then
      return w.id
    end
  end
  return nil
end

--- Prefer first legal shield facing from fire_preview (engine authority).
apply_legal_shield_facing = function()
  local fp = app.fire_preview
  if not fp or not fp.legal_shield_facings or #fp.legal_shield_facings == 0 then
    return
  end
  local cur = app.shield_facing or 0
  for _, f in ipairs(fp.legal_shield_facings) do
    if f == cur then
      return -- current is legal
    end
  end
  app.shield_facing = fp.legal_shield_facings[1]
end

local function do_commit_fire()
  local ship = app.selected_id
  if not ship or not is_player_ship(ship) then
    set_status("warn", "Select one of your ships")
    return
  end
  if not (app.weapon_id and app.target_id) then
    set_status("warn", "Pick weapon and target")
    return
  end
  -- Auto-fix illegal shield face before send (common multi-weapon trap).
  apply_legal_shield_facing()
  local weapon = app.weapon_id
  local target = app.target_id
  local snap2, err = submit(orders.commit_fire(ship, weapon, target, app.shield_facing), true)
  if not err then
    mark_tutorial_order_emitted()
    set_status("info", fire_result_message(snap2 or snap_now(), weapon, target))
    -- Advance to next charged weapon so a full volley is clickable without
    -- re-picking each mount (FIX-PLAN playtest: hard to fire all weapons).
    local snap = snap2 or snap_now()
    local nxt = next_fireable_weapon(snap, ship, weapon)
    app.weapon_id = nxt
    if nxt then
      request_previews()
      apply_legal_shield_facing()
      set_status("info", fire_result_message(snap, weapon, target)
        .. string.format(" · next: %s", nxt))
    else
      request_previews()
    end
  end
end

local function do_ready_fire()
  local ship = app.selected_id
  if not ship or not is_player_ship(ship) then
    set_status("warn", "Select one of your ships")
    return
  end
  local _, err = submit(orders.ready_fire(ship), true)
  if not err then
    mark_tutorial_order_emitted()
    set_status("info", status_fmt.order_echo(ship, "ready_fire"))
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
    mark_tutorial_order_emitted()
    app.alloc = {}
    set_status("info", status_fmt.order_echo(nil, "end_turn"))
  end
end

local function apply_quick_alloc(kind, ship_id)
  if not is_player_ship(ship_id) then
    return
  end
  local snap = snap_now()
  local ship = find_ship_in_snap(snap, ship_id)
  if not ship then
    return
  end
  local a = alloc_for(ship_id)
  if kind == "clear" then
    a.movement = 0
    a.weapons = {}
    for _, w in ipairs(ship.weapons or {}) do
      if (w.charge or 0) > 0 then
        a.weapons[w.id] = w.charge
      end
    end
    a.shields = { 0, 0, 0, 0, 0, 0 }
  elseif kind == "max_weapons" then
    allocation.maximize_weapons(ship, a)
  elseif kind == "balance_shields" then
    allocation.balance_shields(ship, a)
  elseif kind == "all_engine" then
    allocation.all_engine(ship, a)
  end
  -- Engine maps must encode as {} not [] when empty.
  a.weapons = json.object(a.weapons or {})
  debounce.trip(app.reach_debounce)
end

local function handle_ui_hit(hit)
  if not hit then
    return false
  end
  -- UPGRADE-PLAN Phase 6: tutorial gate intercepts mouse hits before normal
  -- dispatch (mirrors TUI input.rs:55 tutorial_gate). Returns true = blocked.
  -- For order-backed steps, the gate validates and sets tutorial_order_candidate;
  -- it returns false (allow) so the normal handler below emits the order, then
  -- calls mark_tutorial_order_emitted() to move candidate → pending.
  if tutorial_gate_ui(hit) then
    return true
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
  if id == "alloc_quick_max_weapons" then
    apply_quick_alloc("max_weapons", p.id)
    return true
  end
  if id == "alloc_quick_balance_shields" then
    apply_quick_alloc("balance_shields", p.id)
    return true
  end
  if id == "alloc_quick_all_engine" then
    apply_quick_alloc("all_engine", p.id)
    return true
  end
  if id == "alloc_quick_clear" then
    apply_quick_alloc("clear", p.id)
    return true
  end
  if id == "alloc_power_bar" then
    -- Click fraction of bar → movement points (F3.4).
    local mx = love.mouse.getX()
    local hits = ui.hits()
    local bar = nil
    for i = #hits, 1, -1 do
      if hits[i].id == "alloc_power_bar" and hits[i].payload and hits[i].payload.id == p.id then
        bar = hits[i]
        break
      end
    end
    if bar then
      local frac = (mx - bar.x) / math.max(1, bar.w)
      frac = math.max(0, math.min(1, frac))
      local a = alloc_for(p.id)
      local ship = find_ship_in_snap(snap_now(), p.id)
      if ship then
        allocation.set_movement_fraction(ship, a, frac)
      end
      debounce.trip(app.reach_debounce)
    end
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
    request_previews()
    apply_legal_shield_facing()
    return true
  end
  if id == "pick_target" then
    app.target_id = p.id
    request_previews()
    apply_legal_shield_facing()
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
  if id == "quit" then
    -- love.quit writes the session log and kills the harness.
    love.event.quit()
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
      slide = app.slide,
    })
    -- Draw transient effects (damage floaters + Phase 5 tracers) inside the
    -- camera transform so world-space x/y land on the right hex. draw_board.draw
    -- pops its own transform, so we re-apply the camera here.
    if app.fx then
      love.graphics.push()
      love.graphics.translate(app.cam.x, app.cam.y)
      love.graphics.scale(app.cam.zoom, app.cam.zoom)
      -- Phase 5: resolution theater tracers (beam/torp/plasma + spark/puff).
      -- Drawn before floaters so impact effects sit on top of the beam line.
      if app.fx_enabled then
        for _, t in ipairs(fx.tracers_active(app.fx)) do
          local a = fx.tracer_alpha(t)
          if a > 0 then
            if t.kind == "beam" then
              love.graphics.setColor(1.0, 0.9, 0.3, a * 0.9)
              love.graphics.setLineWidth(2)
              love.graphics.line(t.x1, t.y1, t.x2, t.y2)
              love.graphics.setLineWidth(1)
            elseif t.kind == "torp" then
              local p = fx.torp_progress(t)
              local px = t.x1 + (t.x2 - t.x1) * p
              local py = t.y1 + (t.y2 - t.y1) * p
              love.graphics.setColor(1.0, 0.5, 0.2, a)
              love.graphics.circle("fill", px, py, 4)
            elseif t.kind == "plasma" then
              local dist = math.sqrt((t.x2 - t.x1) ^ 2 + (t.y2 - t.y1) ^ 2)
              local r = fx.plasma_radius(t, dist)
              love.graphics.setColor(0.4, 0.7, 1.0, a * 0.6)
              love.graphics.circle("fill", t.x1, t.y1, r)
            elseif t.kind == "spark" then
              love.graphics.setColor(1.0, 0.8, 0.3, a)
              love.graphics.circle("fill", t.x1, t.y1, 6 * a)
            elseif t.kind == "puff" then
              love.graphics.setColor(0.7, 0.7, 0.75, a * 0.5)
              love.graphics.circle("line", t.x1, t.y1, 8 + 6 * (1 - a))
            end
          end
        end
      end
      -- Damage floaters.
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
    draw_hud.draw_toast(app)
  elseif app.screen == "end" then
    draw_hud.draw_game_over(app)
  end
  draw_hud.status_strip(app.status)
end

local function ctrl_down()
  return love.keyboard.isDown("lctrl", "rctrl")
end

function love.keypressed(key)
  if ctrl_down() and (key == "=" or key == "kp+" or key == "+") then
    ui.adjust_scale(0.15)
    persist_settings()
    return
  end
  if ctrl_down() and (key == "-" or key == "kp-") then
    ui.adjust_scale(-0.15)
    persist_settings()
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
    elseif key == "escape" or key == "q" then
      love.event.quit()
    end
    return
  end
  if app.screen == "end" then
    if key == "return" or key == "escape" then
      if app.session then harness.kill(app.session) end
      app.screen = "picker"
      app.session = nil
    elseif key == "q" then
      love.event.quit()
    end
    return
  end
  if key == "q" then
    love.event.quit()
    return
  end
  if key == "escape" then
    if app.session then harness.kill(app.session) end
    app.screen = "picker"
    app.session = nil
    return
  end
  -- UPGRADE-PLAN Phase 6: tutorial gate intercepts keys before normal dispatch
  -- (mirrors TUI input.rs:55). Returns true = blocked. For order-backed steps
  -- the gate validates and sets tutorial_order_candidate, then returns false
  -- (allow) so the normal handler below emits the order and calls
  -- mark_tutorial_order_emitted() to move candidate → pending.
  if tutorial_gate_key(key) then
    return
  end
  if key == "return" or key == "kpenter" then
    if app.phase == phases.ALLOCATE then
      -- Commit allocate for the selected player ship (or first living player).
      local ship = app.selected_id
      if not ship or not is_player_ship(ship) then
        local ids = player_ids(snap_now())
        ship = ids[1]
      end
      if ship then
        do_allocate(ship)
      end
    elseif app.phase == phases.FIRING then
      do_commit_fire()
    elseif app.phase == phases.TURN_END then
      do_end_turn()
    end
  elseif key == "f" then
    if app.cam_sys then
      camera.set_auto(app.cam_sys, not app.cam_sys.auto)
      persist_settings()
      set_status("info", app.cam_sys.auto and "Auto-fit camera on" or "Auto-fit camera off")
    end
  elseif key == "c" then
    if app.cam_sys then
      camera.set_auto(app.cam_sys, true)
      set_status("info", "Auto-fit camera")
    end
  elseif key == "e" then
    do_end_turn()
  elseif app.phase == phases.ALLOCATE and (key == "=" or key == "kp+" or key == "+") then
    -- Keyboard allocate draft: + / - nudge movement for selected ship.
    local ship = app.selected_id
    if not ship or not is_player_ship(ship) then
      local ids = player_ids(snap_now())
      ship = ids[1]
    end
    if ship and is_player_ship(ship) then
      local a = alloc_for(ship)
      a.movement = a.movement + 1
      debounce.trip(app.reach_debounce)
    end
  elseif app.phase == phases.ALLOCATE and (key == "-" or key == "kp-") then
    local ship = app.selected_id
    if not ship or not is_player_ship(ship) then
      local ids = player_ids(snap_now())
      ship = ids[1]
    end
    if ship and is_player_ship(ship) then
      local a = alloc_for(ship)
      a.movement = math.max(0, a.movement - 1)
      debounce.trip(app.reach_debounce)
    end
  elseif key == "p" and app.phase == phases.MOVEMENT then
    do_movement("coast")
  elseif key == "t" and app.phase == phases.MOVEMENT then
    do_movement("accel")
  elseif key:match("^[0-5]$") and app.phase == phases.MOVEMENT then
    local face = tonumber(key)
    app.maneuver_facing = face
    -- Digit commits turn (help: "0-5=turn"). Same-facing is blocked in do_movement.
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
    ui.press_begin(hit)
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
  if button == 1 then
    ui.press_end()
  end
  if button == 2 or button == 3 then
    app.drag = nil
  end
end

function love.mousemoved(x, y)
  if app.drag then
    app.cam.x = app.drag.camx + (x - app.drag.x)
    app.cam.y = app.drag.camy + (y - app.drag.y)
    if app.cam_sys then
      camera.user_moved(app.cam_sys)
    end
  end
end

function love.wheelmoved(_, y)
  if y > 0 then
    app.cam.zoom = math.min(4, app.cam.zoom * 1.1)
  elseif y < 0 then
    app.cam.zoom = math.max(0.3, app.cam.zoom / 1.1)
  end
  if app.cam_sys then
    camera.user_moved(app.cam_sys)
  end
end

-- UPGRADE-PLAN Phase 5: session log on quit. Mirrors the TUI's
-- write_session_log (main.rs:93): write orders + final snapshot summary to
-- local/ and print the path on exit. The harness already accumulates
-- successful orders in session.orders; we serialize them as JSONL plus a
-- header with the final turn/phase/status. Pure file I/O — no Love APIs
-- beyond what love.quit already touches.
local function write_session_log()
  if not app.session then
    return nil
  end
  local dir = paths.local_dir()
  local stamp = os.time()
  local pid = (app.session.proc and app.session.proc.pid) or 0
  local path = dir .. "/session-" .. stamp .. "-" .. pid .. ".log"
  local f = io.open(path, "w")
  if not f then
    return nil
  end
  f:write("shipsim Love2D session\n")
  local snap = app.session.snapshot
  if snap then
    f:write(string.format("turn=%s phase=%s status=%s\n",
      tostring(snap.turn), tostring(snap.phase), tostring(snap.status)))
  end
  if app.session.last_error then
    f:write("last_error=" .. tostring(app.session.last_error.message or "") .. "\n")
  end
  f:write("\nOrders:\n")
  for _, order in ipairs(app.session.orders or {}) do
    f:write(json.encode(order))
    f:write("\n")
  end
  f:close()
  return path
end

function love.quit()
  -- UPGRADE-PLAN Phase 5: write the session log and print the path, mirroring
  -- the TUI (main.rs:84-88). Only when a session was active.
  if app.session then
    local path = write_session_log()
    if path then
      app.session_log_path = path
      print("Session log: " .. path)
    end
    harness.kill(app.session)
  end
end
