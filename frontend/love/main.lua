-- Combat Model v2 frontend (ADR-0020).
-- Phase machine: Allocate -> Movement -> Firing -> TurnEnd, driven by snap.phase.

local harness = require("harness")
local paths = require("paths")
local phases = require("phases")
local orders = require("orders")
local end_condition = require("end_condition")
local ui_status = require("ui_status")
local draw_board = require("draw_board")
local draw_hud = require("draw_hud")
local hex = require("hex")
local ui = require("ui")

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
  cam = { x = 200, y = 120, zoom = 1.2 },
  status = ui_status.new(),
  end_result = nil,
  drag = nil,
  show_help = false,
  ghost_path = {},
  alloc = {},
  show_end_warning = false,
}

local function player_ids(snap)
  local ids = {}
  if not snap then
    return ids
  end
  for _, s in ipairs(snap.ships or {}) do
    if s.controller == "player" and not s.destroyed then
      ids[#ids + 1] = s.id
    end
  end
  table.sort(ids)
  return ids
end

local function snap_now()
  return app.session and app.session.snapshot
end

local function sync_phase()
  local snap = snap_now()
  if snap and snap.phase then
    app.phase = snap.phase
  end
end

local function ensure_selection()
  local snap = snap_now()
  local ids = player_ids(snap)
  if #ids == 0 then
    app.selected_id = nil
    return
  end
  local active = snap and snap.active_ship
  if active then
    for _, id in ipairs(ids) do
      if id == active then
        app.selected_id = active
        return
      end
    end
  end
  for _, id in ipairs(ids) do
    if id == app.selected_id then
      return
    end
  end
  app.selected_id = ids[1]
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
  app.alloc = {}
  app.show_end_warning = false
  sync_phase()
  ensure_selection()
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

local function do_move(mode)
  local snap = snap_now()
  local ship = snap and snap.active_ship
  if not ship or not is_player_ship(ship) then
    ui_status.set(app.status, "warn", "Not your move — active is #" .. tostring(ship))
    return
  end
  local _, err = submit(orders.move(ship, mode), true)
  if not err then
    ui_status.set(app.status, "info", string.format("Ship #%d moved (%s)", ship, mode))
  end
end

local function do_pass_move()
  local snap = snap_now()
  local ship = snap and snap.active_ship
  if not ship or not is_player_ship(ship) then
    ui_status.set(app.status, "warn", "Not your move — active is #" .. tostring(ship))
    return
  end
  local _, err = submit(orders.pass_move(ship), true)
  if not err then
    ui_status.set(app.status, "info", string.format("Ship #%d passed", ship))
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
    return true
  end
  if id == "alloc_movement_up" then
    local a = alloc_for(p.id)
    a.movement = a.movement + 1
    return true
  end
  if id == "alloc_movement_dn" then
    local a = alloc_for(p.id)
    a.movement = math.max(0, a.movement - 1)
    return true
  end
  if id == "alloc_weapon_up" then
    local a = alloc_for(p.id)
    a.weapons[p.weapon] = math.min(3, (a.weapons[p.weapon] or 0) + 1)
    return true
  end
  if id == "alloc_weapon_dn" then
    local a = alloc_for(p.id)
    a.weapons[p.weapon] = math.max(0, (a.weapons[p.weapon] or 0) - 1)
    return true
  end
  if id == "alloc_shield_up" then
    local a = alloc_for(p.id)
    a.shields[p.face + 1] = (a.shields[p.face + 1] or 0) + 1
    return true
  end
  if id == "alloc_shield_dn" then
    local a = alloc_for(p.id)
    a.shields[p.face + 1] = math.max(0, (a.shields[p.face + 1] or 0) - 1)
    return true
  end
  if id == "alloc_confirm" then
    do_allocate(p.id)
    return true
  end
  if id == "move_forward" then
    do_move("forward")
    return true
  end
  if id == "move_port" then
    do_move("turn_port")
    return true
  end
  if id == "move_starboard" then
    do_move("turn_starboard")
    return true
  end
  if id == "pass_move" then
    do_pass_move()
    return true
  end
  if id == "pick_weapon" then
    app.weapon_id = p.id
    return true
  end
  if id == "pick_target" then
    app.target_id = p.id
    return true
  end
  if id == "pick_shield_facing" then
    app.shield_facing = p.face
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
    })
    draw_hud.draw(app)
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
      app.screen = "picker"
      app.session = nil
    end
    return
  end
  if key == "escape" then
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
  elseif key == "w" then
    do_move("forward")
  elseif key == "p" then
    do_pass_move()
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
      else
        app.target_id = s.id
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
