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
local scripted_pump = require("scripted_pump")
local preview = require("preview")
local events = require("events")
local fx = require("fx")
local selection = require("selection")
local debounce = require("debounce")
local slide = require("slide")
local json = require("json")
local tutorial = require("tutorial")
local input_policy = require("input_policy")
local layout = require("layout")
local status_fmt = require("status_fmt")
local settings = require("settings")
local toast = require("toast")
local camera = require("camera")
local path_editor = require("path_editor")

local app = {
  screen = "picker",
  scenarios = {},
  picker_index = 1,
  picker_first = 1,
  sidebar_scroll = 0,
  sidebar_max_scroll = 0,
  requested_scale = nil,
  effective_scale = 1,
  window_supported = true,
  scale_clamp_notice = false,
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
  tutorial_order_candidate = nil,
  -- FIX-PLAN F2/F4
  target_previews = {}, -- [target_id] = fire_preview response (cached)
  target_previews_key = nil,
  volley_drafts = {}, -- [ship_id] = complete protocol-v4 shot list
  path_drafts = {}, -- [ship_id] = ordered protocol-v4 path action list
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

local function picker_metrics()
  return layout.picker_metrics(love.graphics.getWidth(), love.graphics.getHeight(),
    ui.scale, #app.scenarios)
end

local function ensure_picker_visible()
  local m = picker_metrics()
  app.picker_first = layout.ensure_index_visible(app.picker_first, app.picker_index,
    #app.scenarios, math.max(1, m.capacity))
end

local function reset_sidebar_scroll()
  app.sidebar_scroll = 0
  app.sidebar_max_scroll = 0
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
    reset_sidebar_scroll()
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
    local new_events = events.feed(app.events, snap, pids)
    if #new_events > 0 then
      app.last_event_time = love.timer.getTime()
      -- Spawn floaters + pulses for new combat events.
      local size = draw_board.hex_size()
      local ship_pos = {}
      for _, s in ipairs(snap.ships or {}) do
        ship_pos[s.id] = { hex.to_pixel(s.q, s.r, size) }
      end
      for _, ev in ipairs(new_events) do
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
  local previous = app.selected_id
  selection.ensure(app, snap_now())
  if app.selected_id ~= previous then
    reset_sidebar_scroll()
  end
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
    protocol_version = 4,
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
      protocol_version = 4,
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
  -- Protocol v4 has a path editor, not one-step maneuver options. The HUD
  -- retains its static controls until it is replaced by that editor.
  app.maneuver_options = nil
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
  local numerator = (a.movement or 0) * (ship.thrust_per_power or 1)
  local budget = math.floor(numerator / math.max(1, ship.power_per_thrust or 1))
  budget = math.min(budget, ship.max_maneuver_actions or budget)
  local resp, err = harness.request(app.session, {
    protocol_version = 4,
    request = "reach_preview",
    ship = ship_id,
    budget = budget,
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
            protocol_version = 4,
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
  -- The gate records the candidate before dispatch. Confirm it only after the
  -- engine accepts this exact submission; rejected orders must never advance.
  if app.tutorial and app.tutorial_order_candidate then
    if tutorial.confirm_order(app.tutorial, app.tutorial_order_candidate, not err) then
      reset_sidebar_scroll()
    end
    app.tutorial_order_candidate = nil
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

-- UPGRADE-PLAN Phase 6: tutorial gate. Mirrors the TUI's tutorial_gate
-- (input.rs:254-373) and map_key_to_action (input.rs:946-1026), adapted to
-- the Love2D input model (mouse buttons + keyboard shortcuts). The gate
-- intercepts both handle_ui_hit (mouse) and love.keypressed (keyboard)
-- before the normal dispatch. Returns true if the input is blocked.
--
-- Engine-backed steps validate but do NOT advance until submit() succeeds.
-- Draft-local steps validate before dispatch and advance only after the draft
-- mutation succeeds.

local function allocation_edit(hit)
  local id = hit.id
  local p = hit.payload or {}
  local draft = alloc_for(p.id)
  if id == "alloc_movement_up" then
    return 0, draft.movement, draft.movement + 1
  elseif id == "alloc_movement_dn" then
    return 0, draft.movement, math.max(0, draft.movement - 1)
  elseif id == "alloc_weapon_up" or id == "alloc_weapon_dn" then
    local fields = { beam_1 = 1, torp_1 = 2, plasma_1 = 3 }
    local field = fields[p.weapon]
    local old = draft.weapons[p.weapon] or 0
    local new = id == "alloc_weapon_up"
      and allocation.increment(old, p.max)
      or allocation.decrement(old)
    return field, old, new
  elseif id == "alloc_shield_up" or id == "alloc_shield_dn" then
    local field = 4 + (p.face or 0)
    local old = draft.shields[(p.face or 0) + 1] or 0
    local new = id == "alloc_shield_up"
      and allocation.increment(old, p.max)
      or allocation.decrement(old)
    return field, old, new
  end
  return nil
end

local function advance_local_tutorial(action)
  if not app.tutorial or tutorial.is_complete(app.tutorial) then
    return
  end
  if tutorial.validate_action(app.tutorial, action) then
    tutorial.advance(app.tutorial)
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

  -- ReachValue steps validate the exact value that the click would produce.
  if expected.kind == "ReachValue" then
    local field, old, new = allocation_edit(hit)
    if field ~= nil then
      local allow = tutorial.check_reach_value(app.tutorial, field, old, new)
      return not allow
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

  -- Love has no allocation cursor. Clicking a control selects its field and
  -- applies the edit, so use that click as the NavField action.
  if expected.kind == "NavField" then
    local field = allocation_edit(hit)
    if field ~= nil then
      return not tutorial.check_action(app.tutorial, { kind = "NavField", field = field })
    end
    tutorial.set_error(app.tutorial, ("Expected: %s. %s"):format(step.title, step.hint))
    return true
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

  -- PathAppend: allow path_action buttons (payload.action = move_f / …).
  if expected.kind == "PathAppend" then
    if id == "path_action" then
      local act = p and p.action
      if tutorial.validate_action(app.tutorial, { kind = "PathAppend", action = act }) then
        return false
      end
    end
    tutorial.set_error(app.tutorial, ("Expected: %s. %s"):format(step.title, step.hint))
    return true
  end

  -- CommitPath: allow path_commit.
  if expected.kind == "CommitPath" then
    if id == "path_commit" then
      if tutorial.validate_action(app.tutorial, { kind = "CommitPath" }) then
        app.tutorial_order_candidate = { kind = "CommitPath" }
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
        return false
      end
    end
    tutorial.set_error(app.tutorial, ("Expected: %s. %s"):format(step.title, step.hint))
    return true
  end

  -- CommitVolley: allow ready_fire (submits commit_volley in v4).
  if expected.kind == "CommitVolley" then
    if id == "ready_fire" then
      if tutorial.validate_action(app.tutorial, { kind = "CommitVolley" }) then
        app.tutorial_order_candidate = { kind = "CommitVolley" }
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

  if expected.kind == "TabWeapon" then
    if id == "pick_weapon" then
      return not tutorial.check_action(app.tutorial, {
        kind = "TabWeapon",
        weapon = p.id,
      })
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

  -- Map key → tutorial action (protocol v4 path/volley).
  local action = nil
  if key == "v" then
    action = { kind = "EnterMap" }
  elseif key == "c" and phase ~= phases.MOVEMENT then
    -- In movement, c is not recenter while drafting (path clear elsewhere).
    action = { kind = "RecenterMap" }
  elseif phase == phases.MOVEMENT then
    if key == "w" then action = { kind = "PathAppend", action = "move_f" }
    elseif key == "a" then action = { kind = "PathAppend", action = "move_fl" }
    elseif key == "d" then action = { kind = "PathAppend", action = "move_fr" }
    elseif key == "z" then action = { kind = "PathAppend", action = "turn_left" }
    elseif key == "x" then action = { kind = "PathAppend", action = "turn_right" }
    elseif key == "return" or key == "kpenter" then action = { kind = "CommitPath" }
    elseif key == "c" then action = { kind = "RecenterMap" }
    end
  elseif phase == phases.FIRING then
    if key == "return" or key == "kpenter" then action = { kind = "FireWeapon" }
    elseif key == "r" or key == "space" then action = { kind = "CommitVolley" }
    elseif key == "down" or key == "up" then
      action = {
        kind = "TabWeapon",
        weapon = selection.cycle_fireable_weapon(snap, app.selected_id,
          app.weapon_id, key == "up" and -1 or 1),
      }
    end
  elseif phase == phases.ALLOCATE then
    if key == "return" or key == "kpenter" then action = { kind = "CommitAllocate" }
    end
  end
  -- Map-focus steps can fire in any phase when expected is map-related.
  if expected.kind == "EnterMap" or expected.kind == "ExitMap" then
    if key == "v" then action = { kind = expected.kind } end
  elseif expected.kind == "PanMap" and key == "a" then
    action = { kind = "PanMap" }
  elseif expected.kind == "ZoomOut" and (key == "-" or key == "kp-") then
    action = { kind = "ZoomOut" }
  elseif expected.kind == "ZoomIn" and (key == "=" or key == "+" or key == "kp+") then
    action = { kind = "ZoomIn" }
  elseif expected.kind == "RecenterMap" and key == "c" then
    action = { kind = "RecenterMap" }
  elseif expected.kind == "Dismiss" and (key == "return" or key == "kpenter" or key == "q") then
    action = { kind = "Dismiss" }
  end

  if not action then
    -- Help and app quit always allowed (user must be able to leave).
    if key == "h" or key == "/" or key == "?" or key == "q" then return false end
    tutorial.set_error(app.tutorial, ("Expected: %s. %s"):format(step.title, step.hint))
    return true
  end

  -- Draft-local steps advance in their mutators, after the edit succeeds.
  if expected.kind == "PathAppend" and action.kind == "PathAppend" then
    if tutorial.validate_action(app.tutorial, action) then
      return false
    end
    return true
  end

  if expected.kind == "FireWeapon" and action.kind == "FireWeapon" then
    return not tutorial.validate_action(app.tutorial, action)
  end

  -- Order-backed steps: validate but don't advance until engine accepts.
  local order_backed = (expected.kind == "CommitAllocate"
    or expected.kind == "CommitPath"
    or expected.kind == "CommitVolley")

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
  app.requested_scale = app.settings.ui_scale or layout.default_scale(w, h)
  local max_scale = layout.max_usable_scale(w, h, false)
  app.effective_scale = math.min(app.requested_scale, max_scale)
  ui.set_scale(app.effective_scale)
  app.window_supported = layout.window_supported(w, h)
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
  app.settings.ui_scale = app.requested_scale or ui.scale
  app.settings.auto_follow = app.cam_sys and app.cam_sys.auto or true
  settings.save(paths.local_dir() .. "/settings.json", app.settings, json)
end

local function apply_requested_scale()
  local w = love.graphics.getWidth()
  local h = love.graphics.getHeight()
  local max_scale = layout.max_usable_scale(w, h, app.tutorial ~= nil)
  app.effective_scale = math.min(app.requested_scale or layout.default_scale(w, h), max_scale)
  ui.set_scale(app.effective_scale)
  app.window_supported = layout.window_supported(w, h)
  local clamped = (app.requested_scale or ui.scale) > app.effective_scale + 0.01
  if clamped and not app.scale_clamp_notice then
    ui_status.set(app.status, "warn", string.format("UI scale limited to %.2f for this window", app.effective_scale))
    app.scale_clamp_notice = true
  elseif not clamped then
    app.scale_clamp_notice = false
  end
  ensure_picker_visible()
  app.sidebar_scroll = math.max(0, math.min(app.sidebar_scroll or 0, app.sidebar_max_scroll or 0))
  ui.clear_hits()
end

local function adjust_requested_scale(delta)
  app.requested_scale = math.max(ui.min_scale, math.min(ui.max_scale,
    (app.requested_scale or ui.scale) + delta))
  apply_requested_scale()
end

local function scroll_sidebar(lines)
  app.sidebar_scroll = layout.scroll_clamp((app.sidebar_scroll or 0)
    + lines * ui.line_h(13), (app.sidebar_max_scroll or 0) + 1, 1)
  -- Hitboxes are rebuilt during draw using the new offset. Do not leave the
  -- previous frame's controls clickable between a wheel event and that draw.
  ui.clear_hits()
end

function love.resize()
  apply_requested_scale()
  ensure_picker_visible()
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
  -- Drafts are per-scenario: an abandoned volley/path must not leak into a new
  -- scenario that happens to reuse the same ship ids.
  app.volley_drafts = {}
  app.path_drafts = {}
  app.picker_first = 1
  reset_sidebar_scroll()
  -- UPGRADE-PLAN Phase 6: detect the tutorial scenario by filename (mirrors
  -- the TUI's --tutorial flag, main.rs:32-39). The TUI uses a CLI flag; the
  -- Love2D frontend has no CLI args, so we match the scenario basename.
  -- tutorial.lua is a pure-Lua step-gate machine; nil in free play.
  local basename = entry.path:match("([^/]+)$") or entry.path
  if basename == "tutorial_rear_attack.toml" then
    app.tutorial = tutorial.new()
    app.tutorial_order_candidate = nil
  else
    app.tutorial = nil
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
    ui_status.set(app.status, "info", "Allocate power, then build a path and commit to move.")
  end
  apply_requested_scale()
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
    set_status("info", status_fmt.order_echo(ship_id, "allocate") ..
      string.format(" (move %d)", a.movement))
  end
end

-- Protocol v4 path draft: one ordered commit_path per ship. Draft state and
-- commit intent live in path_editor (shared with headless tests). Engine
-- path_preview is authoritative for legality; client only enforces UX budget.
local function active_mover(snap)
  if not snap or snap.phase ~= phases.MOVEMENT then
    return nil
  end
  local ship_id = first_uncommitted_ship(snap, "player")
  if not ship_id or not is_player_ship(ship_id) then
    return nil
  end
  return ship_id, find_ship_in_snap(snap, ship_id)
end

local function request_path_preview(ship_id)
  if not app.session or not ship_id then
    return
  end
  local actions = path_editor.get(app.path_drafts, ship_id)
  local req = path_editor.preview_request(ship_id, actions)
  local resp = select(1, harness.request(app.session, req))
  app.path_preview = resp
  app.path_preview_ship = ship_id
end

local function do_path_append(action)
  local snap = snap_now()
  if not snap or snap.phase ~= phases.MOVEMENT then
    set_status("warn", "Not movement phase")
    return
  end
  local ship_id, ship = active_mover(snap)
  if not ship_id then
    set_status("warn", "Not your move")
    return
  end
  local cap = path_editor.motion_cap(ship)
  local ok, reason, draft = path_editor.append(app.path_drafts, ship_id, action, cap)
  if not ok then
    if reason == "budget_full" then
      set_status("warn", string.format("Path full (%d/%d motion)", #draft, cap))
    else
      set_status("warn", "Unknown path action")
    end
    return
  end
  set_status("info", string.format("Path +%s (%d/%d)", action, #draft, cap))
  request_path_preview(ship_id)
  advance_local_tutorial({ kind = "PathAppend", action = action })
end

local function do_path_undo()
  local snap = snap_now()
  local ship_id = active_mover(snap)
  if not ship_id then
    return
  end
  local ok, _, draft = path_editor.undo(app.path_drafts, ship_id)
  if ok then
    set_status("info", string.format("Path undo (%d left)", #draft))
    request_path_preview(ship_id)
  end
end

local function do_path_clear()
  local snap = snap_now()
  local ship_id = active_mover(snap)
  if not ship_id then
    return
  end
  path_editor.clear(app.path_drafts, ship_id)
  set_status("info", "Path cleared")
  request_path_preview(ship_id)
end

--- Commit Path (non-empty draft only) or explicit Hold Position (empty path).
--- hold=true is the sole intentional empty-path action (P / Hold Position).
local function do_commit_path(hold)
  local snap = snap_now()
  if not snap or snap.phase ~= phases.MOVEMENT then
    set_status("warn", "Not movement phase")
    return false
  end
  local ship_id = active_mover(snap)
  if not ship_id then
    set_status("warn", "Not your move")
    return false
  end
  local kind, actions, reason
  if hold then
    kind, actions = path_editor.hold(app.path_drafts, ship_id)
  else
    kind, actions, reason = path_editor.try_commit(app.path_drafts, ship_id)
  end
  if kind == "blocked" then
    set_status("warn", "Add a path action or choose Hold Position")
    return false
  end
  local order = path_editor.order(ship_id, actions)
  local _, err = submit(order, true)
  if err then
    -- Keep draft for correction; never auto-convert to Hold Position.
    set_status("warn", "Path rejected — draft retained")
    return false
  end
  path_editor.accept(app.path_drafts, ship_id)
  app.path_preview = nil
  if hold or #(actions or {}) == 0 then
    set_status("info", status_fmt.order_echo(ship_id, "hold_position"))
  else
    set_status("info", status_fmt.order_echo(ship_id, "commit_path"))
  end
  return true
end

--- Next charged weapon not already in this local protocol-v4 volley draft.
next_fireable_weapon = function(snap, ship_id, exclude)
  local ship = find_ship_in_snap(snap, ship_id)
  if not ship then
    return nil
  end
  local committed = {}
  -- Draft shot entries are {weapon,target,shield_facing} (no .ship field); the
  -- draft is already keyed by ship_id, so any entry with a weapon is committed.
  for _, c in ipairs(app.volley_drafts[ship_id] or {}) do
    if c.weapon then
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
  local draft = app.volley_drafts[ship] or {}
  for _, shot in ipairs(draft) do
    if shot.weapon == weapon then
      set_status("warn", weapon .. " is already in this volley")
      return
    end
  end
  draft[#draft + 1] = { weapon = weapon, target = target, shield_facing = app.shield_facing }
  app.volley_drafts[ship] = draft
  advance_local_tutorial({ kind = "FireWeapon" })
  do
    set_status("info", string.format("Queued %s for volley (%d shot%s)", weapon, #draft,
      #draft == 1 and "" or "s"))
    -- Advance to next charged weapon so a full volley is clickable without
    -- re-picking each mount (FIX-PLAN playtest: hard to fire all weapons).
    local snap = snap_now()
    local nxt = app.tutorial and weapon or next_fireable_weapon(snap, ship, weapon)
    app.weapon_id = nxt
    if app.tutorial then
      request_previews()
    elseif nxt then
      request_previews()
      apply_legal_shield_facing()
      set_status("info", string.format("Queued %s · next: %s", weapon, nxt))
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
  local _, err = submit(orders.commit_volley(ship, app.volley_drafts[ship] or {}), true)
  if not err then
    app.volley_drafts[ship] = nil
    set_status("info", status_fmt.order_echo(ship, "commit_volley"))
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
  -- For order-backed steps, the gate validates and records the candidate;
  -- submit() advances only after the engine accepts the order.
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
    local previous = app.selected_id
    app.selected_id = p.id
    if app.selected_id ~= previous then reset_sidebar_scroll() end
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
  if id == "path_action" then
    do_path_append(p.action)
    return true
  end
  if id == "path_undo" then
    do_path_undo()
    return true
  end
  if id == "path_clear" then
    do_path_clear()
    return true
  end
  if id == "path_commit" then
    do_commit_path(false)
    return true
  end
  if id == "path_hold" then
    do_commit_path(true)
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
    draw_hud.draw_toast(app)
  elseif app.screen == "end" then
    draw_hud.draw_game_over(app)
  end
  draw_hud.status_strip(app.status)
  if not app.window_supported then
    ui.clear_hits()
    draw_hud.draw_resize_overlay()
  end
end

local function ctrl_down()
  return love.keyboard.isDown("lctrl", "rctrl")
end

function love.keypressed(key)
  if not app.window_supported then
    if input_policy.resize_key_allowed(key) then
      love.event.quit()
    end
    return
  end
  if ctrl_down() and (key == "=" or key == "kp+" or key == "+") then
    adjust_requested_scale(0.15)
    persist_settings()
    return
  end
  if ctrl_down() and (key == "-" or key == "kp-") then
    adjust_requested_scale(-0.15)
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
    elseif key == "home" then
      app.picker_index = 1
    elseif key == "end" then
      app.picker_index = #app.scenarios
    elseif key == "pageup" then
      local m = picker_metrics()
      app.picker_index = math.max(1, app.picker_index - math.max(1, m.capacity))
    elseif key == "pagedown" then
      local m = picker_metrics()
      app.picker_index = math.min(#app.scenarios, app.picker_index + math.max(1, m.capacity))
    elseif key == "return" or key == "kpenter" then
      local sc = app.scenarios[app.picker_index]
      if sc then
        start_scenario(sc)
      end
    elseif key == "escape" or key == "q" then
      love.event.quit()
    end
    ensure_picker_visible()
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
  local scroll_command = input_policy.sidebar_scroll_command(key)
  if scroll_command == "page_up" then
    scroll_sidebar(-4)
    return
  elseif scroll_command == "page_down" then
    scroll_sidebar(4)
    return
  elseif scroll_command == "top" then
    app.sidebar_scroll = 0
    ui.clear_hits()
    return
  elseif scroll_command == "bottom" then
    app.sidebar_scroll = app.sidebar_max_scroll or 0
    ui.clear_hits()
    return
  end
  -- UPGRADE-PLAN Phase 6: tutorial gate intercepts keys before normal dispatch
  -- (mirrors TUI input.rs:55). Returns true = blocked. For order-backed steps
  -- the gate validates and records tutorial_order_candidate, then returns
  -- false so the normal handler can emit the order.
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
    elseif app.phase == phases.MOVEMENT then
      do_commit_path(false)
    elseif app.phase == phases.FIRING then
      do_commit_fire()
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
  elseif app.phase == phases.MOVEMENT and key == "w" then
    do_path_append("move_f") -- W: step forward
  elseif app.phase == phases.MOVEMENT and key == "a" then
    do_path_append("move_fl") -- A: step forward-left
  elseif app.phase == phases.MOVEMENT and key == "d" then
    do_path_append("move_fr") -- D: step forward-right
  elseif app.phase == phases.MOVEMENT and key == "z" then
    do_path_append("turn_left") -- Z: turn left in place
  elseif app.phase == phases.MOVEMENT and key == "x" then
    do_path_append("turn_right") -- X: turn right in place
  elseif app.phase == phases.MOVEMENT and key == "backspace" then
    do_path_undo()
  elseif app.phase == phases.MOVEMENT and key == "delete" then
    do_path_clear()
  elseif app.phase == phases.MOVEMENT and key == "p" then
    do_commit_path(true) -- P: Hold Position (explicit empty path)
  elseif input_policy.fire_weapon_delta(app.phase, key) then
    app.weapon_id = selection.cycle_fireable_weapon(snap_now(), app.selected_id,
      app.weapon_id, input_policy.fire_weapon_delta(app.phase, key))
    request_previews()
    apply_legal_shield_facing()
  elseif key == "r" then
    do_ready_fire()
  end
end

function love.textinput(t)
  if not app.window_supported then
    return
  end
  if t == "?" then
    app.show_help = not app.show_help
  end
end

function love.mousepressed(x, y, button)
  if not app.window_supported then
    return
  end
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
        local previous = app.selected_id
        app.selected_id = s.id
        if app.selected_id ~= previous then reset_sidebar_scroll() end
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
  if not app.window_supported then
    app.drag = nil
    return
  end
  if app.drag then
    app.cam.x = app.drag.camx + (x - app.drag.x)
    app.cam.y = app.drag.camy + (y - app.drag.y)
    if app.cam_sys then
      camera.user_moved(app.cam_sys)
    end
  end
end

function love.wheelmoved(_, y)
  if not app.window_supported or y == 0 then
    return
  end
  local mx, my = love.mouse.getPosition()
  local W = love.graphics.getWidth()
  local picker = app.screen == "picker" and picker_metrics() or nil
  local sidebar = nil
  if app.screen == "play" then
    sidebar = layout.sidebar_regions(W, love.graphics.getHeight(), ui.scale,
      app.tutorial ~= nil).panel
  end
  local owner = input_policy.wheel_owner(app.screen, mx, my,
    picker and picker.list or nil, sidebar)
  if app.screen == "picker" then
    local m = picker
    if owner == "picker" then
      app.picker_first = layout.ensure_index_visible(app.picker_first,
        app.picker_index + (y > 0 and -1 or 1), #app.scenarios, math.max(1, m.capacity))
      app.picker_index = math.max(1, math.min(#app.scenarios,
        app.picker_index + (y > 0 and -1 or 1)))
      ensure_picker_visible()
      return
    end
  elseif app.screen == "play" then
    if owner == "sidebar" then
      scroll_sidebar(y > 0 and -4 or 4)
      return
    end
  end
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
