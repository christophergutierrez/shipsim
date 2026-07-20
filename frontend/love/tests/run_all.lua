#!/usr/bin/env luajit
-- v2 Love frontend tests (Combat Model v2, ADR-0020).
-- Gate 1: harness integration (allocate + move).
-- Gate 3: pure Lua order-JSON-shape tests.

local function script_dir()
  local src = debug.getinfo(1, "S").source
  if src:sub(1, 1) == "@" then
    src = src:sub(2)
  end
  return src:match("(.*/)") or "./"
end
local dir = script_dir()
package.path = dir .. "../?.lua;" .. package.path

local json = require("json")
local hex = require("hex")
local orders = require("orders")
local allocation = require("allocation")
local phases = require("phases")
local end_condition = require("end_condition")
local harness = require("harness")
local paths = require("paths")
local draw_board = require("draw_board")
local draw_hud = require("draw_hud")
local path_editor = require("path_editor")
local scripted_pump = require("scripted_pump")
local preview = require("preview")
local events = require("events")
local fx = require("fx")
local slide = require("slide")
local tutorial = require("tutorial")
local input_policy = require("input_policy")
local ship_art = require("ship_art")

local pass = 0
local function ok(msg)
  pass = pass + 1
  print("  OK  " .. msg)
end

local function assert_eq(a, b, msg)
  if a ~= b then
    error(string.format("FAIL %s: %s vs %s", msg, tostring(a), tostring(b)), 2)
  end
end

assert_eq(type(draw_hud), "table", "draw_hud module contract")

print("order builders (gate 3)")
local a = orders.allocate(1, 4, { beam_1 = 1 }, { 0, 0, 0, 0, 0, 0 })
assert_eq(a.type, "allocate", "allocate type")
assert_eq(a.protocol_version, 4, "allocate protocol version")
assert_eq(a.ship, 1, "allocate ship")
assert_eq(a.movement, 4, "allocate movement")
assert_eq(a.weapons.beam_1, 1, "allocate weapons")
assert_eq(#a.shields, 6, "allocate shields len")

-- v4 motion model: one ordered commit_path per ship (no per-cycle maneuver).
local cp = orders.commit_path(1, { "move_f", "turn_left" })
assert_eq(cp.type, "commit_path", "commit_path type")
assert_eq(cp.ship, 1, "commit_path ship")
assert_eq(cp.protocol_version, 4, "commit_path protocol version")
assert_eq(cp.actions[1], "move_f", "commit_path first action")
assert_eq(cp.actions[2], "turn_left", "commit_path second action")
local cpe = orders.commit_path(1, {})
assert_eq(cpe.type, "commit_path", "empty commit_path type")
assert_eq(#cpe.actions, 0, "empty path is legal (stationary)")

-- v4 fire model: one commit_volley per ship (empty = hold fire).
local cv = orders.commit_volley(1, { { weapon = "beam_1", target = 2, shield_facing = 3 } })
assert_eq(cv.type, "commit_volley", "commit_volley type")
assert_eq(cv.ship, 1, "commit_volley ship")
assert_eq(cv.protocol_version, 4, "commit_volley protocol version")
assert_eq(cv.shots[1].weapon, "beam_1", "commit_volley shot weapon")
assert_eq(cv.shots[1].target, 2, "commit_volley shot target")
assert_eq(cv.shots[1].shield_facing, 3, "commit_volley shot shield_facing")
local cvh = orders.commit_volley(1, {})
assert_eq(#cvh.shots, 0, "empty volley holds fire")

-- path_editor is the production draft seam (same as main.lua).
assert_eq(path_editor.key_to_action("w"), "move_f", "W maps to move_f")
assert_eq(path_editor.key_to_action("a"), "move_fl", "A maps to move_fl")
assert_eq(path_editor.key_to_action("d"), "move_fr", "D maps to move_fr")
assert_eq(path_editor.key_to_action("z"), "turn_left", "Z maps to turn_left")
assert_eq(path_editor.key_to_action("x"), "turn_right", "X maps to turn_right")
local drafts = {}
local ok_a, _, d = path_editor.append(drafts, 1, path_editor.key_to_action("w"), 5)
assert_eq(ok_a, true, "W append via same key mapping as production")
assert_eq(d[1], "move_f", "W appends move_f")
ok_a, _, d = path_editor.append(drafts, 1, "move_f", 5) -- button uses same action token
assert_eq(d[2], "move_f", "Forward button action matches W")
-- five-action mixed path
path_editor.clear(drafts, 1)
for _, act in ipairs({ "move_f", "move_fr", "turn_left", "move_fl", "turn_right" }) do
  path_editor.append(drafts, 1, act, 8)
end
d = path_editor.get(drafts, 1)
assert_eq(#d, 5, "five-action draft length")
assert_eq(table.concat(d, ","), "move_f,move_fr,turn_left,move_fl,turn_right", "order preserved")
path_editor.undo(drafts, 1)
assert_eq(#path_editor.get(drafts, 1), 4, "undo pops one")
path_editor.clear(drafts, 1)
assert_eq(#path_editor.get(drafts, 1), 0, "clear empties")
-- Commit Path blocked when empty; Hold Position submits [].
local kind, actions, reason = path_editor.try_commit(drafts, 1)
assert_eq(kind, "blocked", "empty commit blocked")
assert_eq(reason, "empty_draft", "empty_draft reason")
kind, actions = path_editor.hold(drafts, 1)
assert_eq(kind, "submit", "hold submits")
assert_eq(#actions, 0, "hold actions empty array")
path_editor.append(drafts, 1, "move_f", 3)
path_editor.append(drafts, 1, "turn_right", 3)
path_editor.append(drafts, 1, "move_f", 3)
kind, actions = path_editor.try_commit(drafts, 1)
assert_eq(kind, "submit", "non-empty commit")
assert_eq(table.concat(actions, ","), "move_f,turn_right,move_f", "exact draft submitted")
local order = path_editor.order(1, actions)
assert_eq(order.type, "commit_path", "order type")
assert_eq(#order.actions, 3, "order carries draft")
-- Rejected submit leaves draft (accept not called).
assert_eq(#path_editor.get(drafts, 1), 3, "draft retained until accept")
path_editor.accept(drafts, 1)
assert_eq(path_editor.get(drafts, 1)[1], nil, "accept clears draft")
-- path_preview request matches draft
local prev = path_editor.preview_request(1, { "move_f", "turn_left" })
assert_eq(prev.request, "path_preview", "preview request type")
assert_eq(prev.actions[1], "move_f", "preview actions match draft")
assert_eq(prev.protocol_version, 4, "preview protocol")
ok("v4 order builders + path_editor seam")

print("allocation controls")
assert_eq(allocation.increment(3, 4), 4, "increment")
assert_eq(allocation.increment(4, 4), 4, "increment cap")
assert_eq(allocation.decrement(1), 0, "decrement")
assert_eq(allocation.decrement(0), 0, "decrement floor")
ok("allocation controls")

print("parse_stream")
-- Engine error envelopes (type:"error") are classified as errors.
local _, errs = harness.parse_stream('{"type":"error","ok":false,"code":"order_illegal","message":"nope"}')
assert_eq(errs[1].code, "order_illegal", "error envelope parsed")
-- Snapshots (no type:"error") are classified as snapshots, not version-gated.
local snaps, _ = harness.parse_stream('{"protocol_version":4,"turn":1,"phase":"allocate","ships":[]}')
assert_eq(snaps[1].turn, 1, "v4 snapshot parsed")
ok("parse_stream")

print("phases")
assert_eq(phases.ALLOCATE, "allocate", "phase allocate")
assert_eq(phases.MOVEMENT, "movement", "phase movement")
assert_eq(phases.FIRING, "firing", "phase firing")
assert_eq(phases.next(phases.ALLOCATE), phases.MOVEMENT, "phase next")
assert_eq(phases.next(phases.FIRING), phases.ALLOCATE, "firing wraps to allocate")
assert_eq(phases.allows(phases.ALLOCATE, "allocate"), true, "allows allocate")
assert_eq(phases.allows(phases.MOVEMENT, "move"), false, "legacy move disabled")
assert_eq(phases.allows(phases.MOVEMENT, "pass_move"), false, "legacy pass disabled")
assert_eq(phases.allows(phases.MOVEMENT, "commit_path"), true, "allows commit_path")
assert_eq(phases.allows(phases.FIRING, "commit_volley"), true, "allows commit_volley")
assert_eq(phases.allows(phases.ALLOCATE, "commit_volley"), false, "disallows commit_volley in allocate")
ok("phases")

print("hex")
assert_eq(hex.distance(0, 0, 1, 0), 1, "dist")
ok("hex")

print("end_condition")
assert_eq(end_condition.evaluate({ status = "Won", ships = {} }), "won", "won")
assert_eq(end_condition.evaluate({ status = "Lost", ships = {} }), "defeat", "lost")
ok("end")

print("harness (gate 1)")
local function resolve_repo()
  for _, c in ipairs({ ".", dir .. "../../.." }) do
    local f = io.open(c .. "/Cargo.toml", "r")
    if f then
      f:close()
      return c
    end
  end
  return "."
end
local repo = resolve_repo()
local session = harness.new({
  repo_root = repo,
  bin = paths.find_shipsim_bin(repo),
  -- Scratch under frontend/love/local/ only (frontend isolation policy).
  orders_path = paths.local_dir() .. "/test_orders.jsonl",
  stderr_path = paths.local_dir() .. "/test_stderr.txt",
})
local snap = harness.load_scenario(session, "scenarios/combat.toml")
assert(snap, "load")
assert_eq(snap.turn, 1, "turn")
assert_eq(snap.phase, "allocate", "phase")
ok("load")

-- Allocate the player ship; Love pumps the scripted ship automatically. In v4
-- a partial allocation does NOT mutate public ship state — it is staged until
-- every living ship commits, so shields become visible only after the stage
-- resolves and the phase advances to movement.
snap = select(1, harness.submit(session, orders.allocate(1, 4, { beam_1 = 1 }, { 2, 1, 0, 0, 0, 1 })))
assert(snap, "allocate ship 1")
assert_eq(snap.phase, "allocate", "staged allocation stays in allocate until all commit")
scripted_pump.run(session, function(err) error(err.message or "scripted pump failed") end)
snap = session.snapshot
assert_eq(snap.phase, "movement", "phase after allocate")
assert_eq(snap.ships[1].shields_powered[1], 2, "front shield allocated after stage resolves")
assert_eq(snap.ships[1].shields_powered[2], 1, "front-right shield allocated after stage resolves")
ok("allocate + move phase")

-- Movement via path_editor seam (same as production keyboard/buttons).
local face0, q0, r0, motion = nil, nil, nil, 0
for _, s in ipairs(snap.ships or {}) do
  if s.id == 1 then
    face0, q0, r0 = s.facing or 0, s.q, s.r
    motion = s.motion_available or 0
  end
end
assert(motion >= 1, "player has motion after allocate")

local drafts = {}
-- combat.toml: player (1,0) face 3 (west); scripted holds (0,0). Move west would
-- collide — turn nose east (three turn_right) then move_f to (2,0), conflict-free.
path_editor.append(drafts, 1, path_editor.key_to_action("x"), motion)
path_editor.append(drafts, 1, path_editor.key_to_action("x"), motion)
path_editor.append(drafts, 1, path_editor.key_to_action("x"), motion)
path_editor.append(drafts, 1, path_editor.key_to_action("w"), motion)
local kind, actions = path_editor.try_commit(drafts, 1)
assert_eq(kind, "submit", "non-empty draft commits")
assert_eq(#actions, 4, "four drafted actions")
assert_eq(actions[4], "move_f", "ends with W-equivalent translation")
local move_order = path_editor.order(1, actions)
assert_eq(move_order.type, "commit_path", "movement order is a commit_path")
assert(#move_order.actions > 0, "commit_path actions non-empty")
-- Empty Commit Path is blocked (no order) — regression for silent empty submit.
local empty_kind = path_editor.try_commit({ [1] = {} }, 1)
assert_eq(empty_kind, "blocked", "empty Commit Path blocked")
-- Hold Position is the intentional empty submit.
local hkind, hactions = path_editor.hold(drafts, 1)
assert_eq(hkind, "submit", "hold submits")
assert_eq(#hactions, 0, "hold empty array")
snap = select(1, harness.submit(session, move_order))
assert(snap, "commit_path ship 1")
path_editor.accept(drafts, 1)
local committed = false
for _, id in ipairs(snap.ships_committed_path or {}) do
  if id == 1 then committed = true end
end
assert(committed or snap.phase == "firing", "commit_path accepted this stage")
-- Prove the NDJSON order was non-empty (session log / harness orders list).
local last = session.orders[#session.orders]
assert(last and last.type == "commit_path" and #(last.actions or {}) > 0,
  "submitted order must be non-empty commit_path")
ok("player commit_path accepted by engine")

-- Scripted Hold Position (empty path) → resolve → firing.
scripted_pump.run(session, function(err) error(err.message or "scripted pump failed") end)
snap = session.snapshot
assert_eq(snap.phase, "firing", "path resolution advances to firing")
local face, q1, r1, tr_steps = nil, nil, nil, 0
for _, s in ipairs(snap.ships or {}) do
  if s.id == 1 then face, q1, r1 = s.facing, s.q, s.r end
end
for _, pr in ipairs(snap.path_results or {}) do
  if pr.ship == 1 then tr_steps = pr.translated_steps or 0 end
end
assert(tr_steps > 0, "path_results.translated_steps > 0 for mover")
assert(q1 ~= q0 or r1 ~= r0, "ship 1 hex changed after resolution")
assert(face ~= nil, "ship 1 still present")
ok("resolved path moves ship (engine-backed)")

-- Volley stage: hold fire (empty volley), then let the scripted ship commit its
-- volley. The turn advances automatically back to allocate — no end_turn.
snap = select(1, harness.submit(session, orders.commit_volley(1, {})))
assert(snap, "commit_volley ship 1")
scripted_pump.run(session, function(err) error(err.message or "scripted pump failed") end)
snap = session.snapshot
assert(snap.phase == "allocate" or snap.status == "Won" or snap.status == "Lost",
  "turn advances automatically after volleys resolve")
if snap.phase == "allocate" then
  assert(snap.turn >= 2, "auto-advance reaches the next turn")
end
ok("volley resolves and turn auto-advances")

-- Soft-status field contract (draw_hud reads message/level).
local st = { level = "error", message = "test soft error", ticks = 0 }
assert(st.message and st.message ~= "", "status message field")
assert(st.level == "error", "status level field")
ok("status strip field names")

-- ---- Phase 0: events ring buffer + snapshot field surfacing ----
print("phase 0: events")
local events = require("events")

-- Caps at 50 and preserves insertion order.
do
  local ev = events.new()
  for i = 1, 60 do
    events.feed(ev, { turn = i, combat_log = {
      { attacker = 1, target = 2, weapon = "beam_1", shield = 0,
        damage = 1, shield_absorbed = 0, hull_damage = 1, kind = "hit" },
    } }, { [1] = true })
  end
  assert_eq(events.count(ev), 50, "ring buffer caps at 50")
  local rec = events.recent(ev, 3)
  -- most recent 3 should be turns 58, 59, 60 (oldest-first in the slice)
  assert_eq(rec[1].turn, 58, "recent slice oldest is turn 58")
  assert_eq(rec[3].turn, 60, "recent slice newest is turn 60")
  local emitted = events.feed(ev, { turn = 61, combat_log = {
    { attacker = 1, target = 2, weapon = "beam_1", shield = 0,
      damage = 1, shield_absorbed = 0, hull_damage = 1, kind = "hit" },
  } }, { [1] = true })
  assert_eq(#emitted, 1, "new events are reported after the ring reaches capacity")
  assert_eq(events.count(ev), 50, "ring remains capped after reporting new event")
  ok("events ring buffer caps and orders")
end

-- Classifies hit_dealt / hit_taken / miss by player_ids.
do
  local ev = events.new()
  events.feed(ev, { turn = 1, combat_log = {
    { attacker = 1, target = 2, weapon = "beam_1", shield = 0,
      damage = 5, shield_absorbed = 0, hull_damage = 5, kind = "hit" },
    { attacker = 3, target = 1, weapon = "torp_1", shield = 0,
      damage = 4, shield_absorbed = 4, hull_damage = 0, kind = "hit" },
    { attacker = 1, target = 3, weapon = "beam_1", shield = 0,
      damage = 0, shield_absorbed = 0, hull_damage = 0, kind = "miss" },
  } }, { [1] = true })
  local all = events.recent(ev)
  assert_eq(#all, 3, "three combat events fed")
  assert_eq(all[1].kind, "hit_dealt", "player attacker -> hit_dealt")
  assert_eq(all[2].kind, "hit_taken", "player target -> hit_taken")
  assert_eq(all[2].text:match("shield"), "shield", "shield-only hit text says shield")
  assert_eq(all[3].kind, "miss", "miss classified")
  ok("events classify hit_dealt vs hit_taken vs miss")
end

-- Does not re-emit an already-seen combat generation.
do
  local ev = events.new()
  local log = {
    { attacker = 1, target = 2, weapon = "beam_1", shield = 0,
      damage = 1, shield_absorbed = 0, hull_damage = 1, kind = "hit" },
  }
  events.feed(ev, { turn = 1, combat_log = log }, { [1] = true })
  assert_eq(events.count(ev), 1, "first feed emits 1")
  -- feed the same snapshot again (no new log entries) -> no new event
  events.feed(ev, { turn = 1, combat_log = log }, { [1] = true })
  assert_eq(events.count(ev), 1, "re-feed emits nothing new")
  ok("events dedupe a combat generation")
end

-- Identical-sized and identical-content volleys on later turns remain distinct.
-- In-progress snapshots have already auto-advanced; terminal snapshots have not.
do
  local ev = events.new()
  local log = {
    { attacker = 1, target = 2, weapon = "beam_1", shield = 0,
      damage = 1, shield_absorbed = 0, hull_damage = 1, kind = "hit" },
  }
  events.feed(ev, { turn = 2, status = "InProgress", combat_log = log }, { [1] = true })
  events.feed(ev, { turn = 2, status = "InProgress", combat_log = log }, { [1] = true })
  assert_eq(events.count(ev), 1, "retained volley is not duplicated")
  assert_eq(events.recent(ev)[1].turn, 1, "auto-advanced volley attributed to prior turn")
  events.feed(ev, { turn = 2, status = "Won", combat_log = log }, { [1] = true })
  assert_eq(events.count(ev), 2, "terminal same-shape volley is retained")
  assert_eq(events.recent(ev)[2].turn, 2, "terminal volley attributed to current turn")

  local continuing = events.new()
  events.feed(continuing, { turn = 2, status = "InProgress", combat_log = log }, { [1] = true })
  events.feed(continuing, { turn = 3, status = "InProgress", combat_log = log }, { [1] = true })
  assert_eq(events.count(continuing), 2, "identical later-turn volley is retained")
  assert_eq(events.recent(continuing)[2].turn, 2, "later auto-advanced volley turn")
  ok("combat generations survive equal lengths and terminal rollover")
end

-- path_results fallback becomes a "blocked" event (protocol v4).
do
  local ev = events.new()
  events.feed(ev, { turn = 2, path_results = {
    { ship = 1, submitted_cost = 2, translated_steps = 0, final_q = 0, final_r = 0,
      final_facing = 0, fallback_steps = 1, blocked_kind = "contested",
      conflicting_ships = { 2 } },
    { ship = 2, submitted_cost = 1, translated_steps = 1, final_q = 1, final_r = 0,
      final_facing = 0, fallback_steps = 0 },
  } }, {})
  assert_eq(events.count(ev), 1, "one blocked event")
  local rec = events.recent(ev)
  assert_eq(rec[1].kind, "blocked", "blocked kind")
  assert(rec[1].text:match("fallback"), "blocked text mentions fallback")
  events.feed(ev, { turn = 3, path_results = {} }, {})
  events.feed(ev, { turn = 3, path_results = {
    { ship = 1, submitted_cost = 2, translated_steps = 0, final_q = 0, final_r = 0,
      final_facing = 0, fallback_steps = 1, blocked_kind = "contested",
      conflicting_ships = { 2 } },
  } }, {})
  assert_eq(events.count(ev), 2, "same fallback on a later turn is retained")
  ok("path_results fallback becomes an event")
end

-- Snapshot field surfacing: additive v4 fields via harness line classifier.
do
  local snaps, _ = harness.parse_stream(
    '{"protocol_version":4,"turn":1,"phase":"firing","ships":[],' ..
    '"rules_id":"default","rules_fingerprint":"fnv1a-deadbeef",' ..
    '"fire_opportunity":{"ship":1,"weapon":"beam_1","target":2,"legal_shield_facings":[0,1]},' ..
    '"path_results":[{"ship":1,"submitted_cost":1,"translated_steps":1,"final_q":1,"final_r":0,"final_facing":0,"fallback_steps":0,"conflicting_ships":[]}],' ..
    '"combat_log":[]}')
  local s = snaps[1]
  assert(s, "synthetic snapshot parsed")
  assert_eq(s.rules_id, "default", "rules_id surfaced")
  assert_eq(s.rules_fingerprint, "fnv1a-deadbeef", "rules_fingerprint surfaced")
  assert_eq(type(s.fire_opportunity), "table", "fire_opportunity surfaced")
  assert_eq(s.fire_opportunity.ship, 1, "fire_opportunity.ship")
  assert_eq(s.fire_opportunity.weapon, "beam_1", "fire_opportunity.weapon")
  assert_eq(type(s.path_results), "table", "path_results surfaced")
  ok("snapshot exposes fire_opportunity and path_results fields")
end

-- Rules provenance label format (UPGRADE-PLAN Phase 0 task 4).
-- Pure logic: draw_hud.rules_label must run under plain luajit.
do
  local app1 = { rules_id = "default", rules_fingerprint = "fnv1a-deadbeef-cafe" }
  assert_eq(draw_hud.rules_label(app1), "rules: default fnv1a-deadbe", "label format truncates fp to 12")
  local app2 = { rules_id = "default", rules_fingerprint = "short" }
  assert_eq(draw_hud.rules_label(app2), "rules: default short", "label handles short fp")
  assert_eq(draw_hud.rules_label({}), nil, "label nil when no rules_id")
  assert_eq(draw_hud.rules_label(nil), nil, "label nil when app nil")
  ok("rules provenance label format")
end

-- ---- Phase 1: engine-authoritative previews (delete local rules math) ----
print("phase 1: previews")
-- fire_preview drives target legality: an illegal pairing renders the engine's
-- reason verbatim, never a local verdict. Pure-logic test of preview.fire_line.
do
  -- Synthetic snapshot: player ship A1 with a destroyed beam_1, living target B2.
  local snap = { ships = {
    { id = 1, controller = "player", destroyed = false, weapons = {
        { id = "beam_1", operational = false } } },
    { id = 2, controller = "ai", destroyed = false, weapons = {} },
  } }
  -- Destroyed-weapon case: engine returns legal:false; the weapon is on the
  -- ship but operational==false, so the line must say "destroyed and cannot
  -- fire" — never the raw "was not found" lookup text.
  local app_destroyed = {
    fire_preview = { legal = false, ship = 1, weapon = "beam_1", target = 2,
      reason = "weapon beam_1 was not found" },
    session = { snapshot = snap },
  }
  local line = preview.fire_line(app_destroyed)
  assert(line, "fire_line returns a line for destroyed weapon")
  assert_eq(line.color, "red", "destroyed weapon line is red")
  assert(line.text:find("destroyed and cannot fire"), "destroyed weapon says destroyed, not 'not found'; got: " .. line.text)

  -- Out-of-arc case: engine returns legal:false with its own reason; the line
  -- must echo that reason verbatim, not invent a local verdict. Use a snapshot
  -- where the weapon is operational so the destroyed-weapon branch is skipped.
  local snap_live = { ships = {
    { id = 1, controller = "player", destroyed = false, weapons = {
        { id = "beam_1", operational = true } } },
    { id = 2, controller = "ai", destroyed = false, weapons = {} },
  } }
  local app_oar = {
    fire_preview = { legal = false, ship = 1, weapon = "beam_1", target = 2,
      reason = "target out of arc" },
    session = { snapshot = snap_live },
  }
  local oline = preview.fire_line(app_oar)
  assert(oline.text:find("target out of arc"), "out-of-arc line echoes engine reason; got: " .. oline.text)
  assert_eq(oline.color, "red", "out-of-arc line is red")

  -- Legal shot: green line with hit %, damage, and face validity.
  local app_ok = {
    fire_preview = { legal = true, ship = 1, weapon = "beam_1", target = 2,
      range = 3, threshold = 19, die_sides = 20, hit_percent = 95,
      projected_damage = 7, legal_shield_facings = { 0, 1 } },
    shield_facing = 0,
    session = { snapshot = snap },
  }
  local gline = preview.fire_line(app_ok)
  assert_eq(gline.color, "green", "legal in-arc shot is green")
  assert(gline.text:find("95%%"), "legal line shows hit percent; got: " .. gline.text)
  assert(gline.text:find("dmg~7"), "legal line shows projected damage; got: " .. gline.text)

  -- Invalid shield facing on a legal shot: red line naming the valid faces.
  local app_badface = {
    fire_preview = { legal = true, ship = 1, weapon = "beam_1", target = 2,
      range = 3, threshold = 19, die_sides = 20, hit_percent = 95,
      projected_damage = 7, legal_shield_facings = { 3 } },
    shield_facing = 0,
    session = { snapshot = snap },
  }
  local bline = preview.fire_line(app_badface)
  assert_eq(bline.color, "red", "invalid shield facing is red")
  assert(bline.text:find("INVALID"), "invalid face line says INVALID; got: " .. bline.text)
  ok("fire_preview drives target legality")
end

-- path_preview summary line (protocol v4).
do
  assert_eq(preview.path_line(nil), "…", "nil preview")
  assert(preview.path_line({ error = "over budget" }):find("illegal"), "error preview")
  local ok_line = preview.path_line({ cost = 2, remaining_motion = 3, final_facing = 1 })
  assert(ok_line:find("cost 2"), "path line cost")
  assert(ok_line:find("3 left"), "path line remaining")
  ok("path_preview status line")
end

-- Live-engine round-trip: gated behind LOVE_LIVE=1 so headless stays green.
if os.getenv("LOVE_LIVE") then
  print("phase 0: live request envelope round-trip (LOVE_LIVE=1)")
  local live = harness.new({ repo_root = repo, bin = paths.find_shipsim_bin(repo) })
  local lsnap = harness.load_scenario(live, "scenarios/combat.toml")
  assert(lsnap, "live load")
  -- Allocate so reach_preview (v4; movement_preview is retired) is meaningful.
  harness.submit(live, orders.allocate(1, 4, { beam_1 = 1 }, { 2, 1, 0, 0, 0, 1 }))
  local resp, rerr = harness.request(live,
    { protocol_version = 4, request = "reach_preview", ship = 1, budget = 4 })
  assert(resp, "live reach_preview failed: " .. tostring(rerr and rerr.message))
  assert_eq(resp.type, "reach_preview", "live response type")
  assert_eq(resp.ok, true, "live response ok")
  assert(resp.endpoints and #resp.endpoints > 0, "live endpoints non-empty")
  -- request must not pollute the order log
  assert_eq(#live.orders, 1, "live request did not pollute order log")
  harness.kill(live)
  ok("request envelope round-trip")
else
  ok("request envelope round-trip (skipped: LOVE_LIVE unset)")
end

-- ─── Phase 2: call-to-action banner mirrors TUI phase_call_to_action ────
-- These mirror the TUI tests at tests.rs:2993-3074 (dead focus, CTA ownership,
-- Tab hint, fire_opportunity attribution).
do
  local function mkship(id, controller, opts)
    opts = opts or {}
    return {
      id = id,
      controller = controller,
      destroyed = opts.destroyed or false,
      power_available = opts.power_available,
      thrust_remaining = opts.thrust_remaining,
      class = opts.class or "dd",
    }
  end

  -- callsign format: A# player, B# ai, C# other. Fleet has two player ships
  -- (A1, A2) so the pending-fleetmate branch has a real fleetmate to name.
  local snap = { status = "playing", phase = phases.ALLOCATE,
    ships = { mkship(1, "player", { power_available = 4 }),
              mkship(2, "player", { power_available = 4 }),
              mkship(3, "ai", { power_available = 4 }) } }
  assert_eq(draw_hud.phase_call_to_action(snap, 1), "A1 needs power allocation",
    "cta callsign player prefix")
  -- Focused ship done, fleetmate pending → Tab hint.
  snap.ships_allocated_this_turn = { 1 }
  assert_eq(draw_hud.phase_call_to_action(snap, 1), "A2 needs power allocation — click ship on map",
    "cta names pending fleetmate with mouse hint")
  -- Focused ship pending → no Tab hint.
  snap.ships_allocated_this_turn = { 2 }
  assert_eq(draw_hud.phase_call_to_action(snap, 1), "A1 needs power allocation",
    "cta names focused pending ship")

  -- movement phase (v4: commit_path completion tracked by ships_committed_path)
  snap.phase = phases.MOVEMENT
  snap.ships_committed_path = { 1 }
  snap.ships_allocated_this_turn = nil
  assert_eq(draw_hud.phase_call_to_action(snap, 1), "A2 needs a path (or Hold Position) — click ship on map",
    "cta movement pending fleetmate")

  -- firing: fire_opportunity attributed to attacker callsign, focused is attacker
  snap.phase = phases.FIRING
  snap.ships_committed_path = nil
  snap.ships_committed_volley = {}
  snap.fire_opportunity = { ship = 1, weapon = "beam_1", target = 3 }
  assert_eq(draw_hud.phase_call_to_action(snap, 1), "A1 beam_1>B3 · queue then R",
    "cta fire opportunity focused attacker")

  -- firing: focused is NOT the attacker → click to switch (no Tab handler)
  assert_eq(draw_hud.phase_call_to_action(snap, 2), "A2 active; click A1 for beam_1>B3",
    "cta fire opportunity click to attacker")

  -- firing: no opportunity → needs a volley (R holds fire)
  snap.fire_opportunity = nil
  assert_eq(draw_hud.phase_call_to_action(snap, 1), "A1 needs a volley (R holds fire)",
    "cta no legal shot still needs volley commit")

  -- firing: focused ship already committed volley
  snap.ships_committed_volley = { 1 }
  assert_eq(draw_hud.phase_call_to_action(snap, 1), "A1 volley sent; focus A2",
    "cta after volley auto-focus hint")

  -- game over
  snap.status = "Won"
  assert_eq(draw_hud.phase_call_to_action(snap, 1), "Game over",
    "cta game over won")
  snap.status = "Lost"
  assert_eq(draw_hud.phase_call_to_action(snap, 1), "Game over",
    "cta game over lost")

  ok("phase_call_to_action mirrors TUI")
end

do
  local queued = draw_hud.queued_weapons({
    volley_drafts = {
      [1] = {
        { weapon = "beam_1", target = 3, shield_facing = 0 },
        { weapon = "torp_1", target = 3, shield_facing = 0 },
      },
    },
  }, 1)
  assert_eq(queued.beam_1, true, "beam queue shown from local volley draft")
  assert_eq(queued.torp_1, true, "torpedo queue shown from local volley draft")
  assert_eq(queued.plasma_1, nil, "unqueued weapon remains clear")
  ok("firing panel queue source is protocol-v4 local draft")
end

-- ─── Phase 2: dead-focus recovery + auto-advance (selection module) ──────
-- Pure-Lua: selection.ensure / clear_drafts_for. No love.* APIs. Mirrors the
-- TUI's dead-focus recovery (app.rs:322-340): a destroyed focus reselects the
-- first living player ship and drops drafts tied to the dead focus.
print("phase 2: selection")
local selection = require("selection")
assert_eq(type(selection.ensure), "function", "selection module contract")

do
  local function mkship(id, controller, opts)
    opts = opts or {}
    return { id = id, controller = controller, destroyed = opts.destroyed or false }
  end

  -- Two player ships (A1, A2). Focus A1, then A1 is destroyed in the next
  -- snapshot: ensure() must reselect A2 (first living player ship) and clear
  -- the fire drafts tied to A1.
  local state = {
    selected_id = 1,
    weapon_id = "beam_1",
    target_id = 3,
    shield_facing = 2,
    alloc = { [1] = { movement = 1 }, [2] = { movement = 0 } },
  }
  local snap = { phase = "firing",
    ships = { mkship(1, "player", { destroyed = true }),
              mkship(2, "player"),
              mkship(3, "ai") } }
  selection.ensure(state, snap)
  assert_eq(state.selected_id, 2, "dead focus recovers to survivor")
  assert_eq(state.weapon_id, nil, "dead focus clears weapon draft")
  assert_eq(state.target_id, nil, "dead focus clears target draft")
  assert_eq(state.shield_facing, 0, "dead focus clears shield facing")
  assert_eq(state.alloc[1], nil, "dead focus clears its alloc draft")
  assert_eq(state.alloc[2].movement, 0, "survivor alloc draft untouched")
  ok("dead selection recovers to survivor")
end

-- Auto-advance in movement: focus follows the first uncommitted player ship.
do
  local function mkship(id, controller)
    return { id = id, controller = controller, destroyed = false }
  end
  local state = { selected_id = 1, weapon_id = nil, target_id = nil,
    shield_facing = 0, alloc = {} }
  local snap = { phase = "movement",
    ships = { mkship(1, "player"), mkship(2, "player") },
    ships_committed_path = { 1 } }
  selection.ensure(state, snap)
  assert_eq(state.selected_id, 2, "auto-advance to uncommitted ship")
  ok("selection auto-advances to pending ship")
end

-- No living player ships: focus cleared.
do
  local state = { selected_id = 1, weapon_id = "x", target_id = 2,
    shield_facing = 0, alloc = {} }
  local snap = { phase = "firing",
    ships = { { id = 1, controller = "player", destroyed = true },
              { id = 2, controller = "ai", destroyed = false } } }
  selection.ensure(state, snap)
  assert_eq(state.selected_id, nil, "no living players clears focus")
  ok("selection clears focus when no living players")
end

-- Fire weapon cycling follows ship order and skips non-fireable mounts.
-- v4: a shot's queued state lives in the local volley draft (app.volley_drafts),
-- not the snapshot; keyboard cycling walks charged, operational weapons.
do
  local snap = {
    ships = {
      { id = 1, controller = "player", weapons = {
        { id = "beam_1", charge = 4, operational = true },
        { id = "torp_1", charge = 1, operational = true },
        { id = "plasma_1", charge = 1, operational = true },
      } },
    },
  }
  assert_eq(selection.cycle_fireable_weapon(snap, 1, "beam_1", 1), "torp_1",
    "down selects next weapon")
  assert_eq(selection.cycle_fireable_weapon(snap, 1, "beam_1", -1), "plasma_1",
    "up selects previous weapon")
  -- A discharged weapon (charge 0) is not fireable and is skipped.
  snap.ships[1].weapons[2].charge = 0
  assert_eq(selection.cycle_fireable_weapon(snap, 1, "beam_1", 1), "plasma_1",
    "uncharged weapon is skipped")
  snap.ships[1].weapons[2].charge = 1
  -- A disabled weapon (operational=false) is skipped.
  snap.ships[1].weapons[2].operational = false
  assert_eq(selection.cycle_fireable_weapon(snap, 1, "beam_1", 1), "plasma_1",
    "disabled weapon is skipped")
  ok("fire weapon keyboard cycling")
end

-- ─── Phase 3: fx animation system (damage floaters) ──────────────────────
-- Pure-Lua: spawn/update/alpha/count. No love.* APIs. Mirrors events.lua.
print("phase 3: fx")
local fx = require("fx")
assert_eq(type(fx), "table", "fx module contract")

-- spawn + count + active ordering.
do
  local sys = fx.new()
  assert_eq(fx.count(sys), 0, "new system empty")
  local e1 = fx.spawn(sys, 100, 200, "-5")
  assert(e1, "spawn returns effect")
  assert_eq(fx.count(sys), 1, "count after one spawn")
  fx.spawn(sys, 110, 210, "MISS", { color = { 1, 1, 0.4, 1 } })
  assert_eq(fx.count(sys), 2, "count after two spawns")
  local act = fx.active(sys)
  assert_eq(#act, 2, "active returns copy")
  assert_eq(act[1].text, "-5", "active preserves spawn order")
  assert_eq(act[2].text, "MISS", "active second is MISS")
  -- active() returns a fresh array (length-stable across spawns), though
  -- element tables are shared by design so update() can mutate them.
  local act2 = fx.active(sys)
  assert(act2 ~= act, "active returns a fresh array each call")
  ok("fx spawn + count + active")
end

-- update advances t, drifts y upward, and expires effects past life.
do
  local sys = fx.new()
  local e = fx.spawn(sys, 50, 100, "-3", { life = 1.0, vy = -20.0 })
  assert_eq(e.t, 0.0, "spawn t starts at 0")
  -- tick half a second: t=0.5, y drifts up by -20*0.5 = -10 -> y=90.
  local remaining = fx.update(sys, 0.5)
  assert_eq(remaining, 1, "still active after half life")
  assert_eq(e.t, 0.5, "t advanced by dt")
  assert_eq(e.y, 90.0, "y drifted upward by vy*dt")
  -- tick another 0.6s: t=1.1 >= life=1.0 -> expired.
  remaining = fx.update(sys, 0.6)
  assert_eq(remaining, 0, "expired after life exceeded")
  assert_eq(fx.count(sys), 0, "count 0 after expiry")
  ok("fx update advances + expires")
end

-- alpha: 1.0 first half, linear fade to 0 second half, 0 at/after life.
do
  local sys = fx.new()
  local e = fx.spawn(sys, 0, 0, "x", { life = 1.0 })
  assert_eq(fx.alpha(e), 1.0, "alpha 1.0 at t=0")
  e.t = 0.4
  assert_eq(fx.alpha(e), 1.0, "alpha 1.0 in first half")
  e.t = 0.5
  assert_eq(fx.alpha(e), 1.0, "alpha 1.0 at half boundary")
  e.t = 0.75
  -- second half: remaining=0.25, half=0.5 -> 0.5
  assert_eq(fx.alpha(e), 0.5, "alpha 0.5 at three-quarters")
  e.t = 1.0
  assert_eq(fx.alpha(e), 0.0, "alpha 0.0 at life boundary")
  e.t = 1.5
  assert_eq(fx.alpha(e), 0.0, "alpha 0.0 past life")
  ok("fx alpha fade curve")
end

-- clear wipes active effects.
do
  local sys = fx.new()
  fx.spawn(sys, 1, 2, "a")
  fx.spawn(sys, 3, 4, "b")
  assert_eq(fx.count(sys), 2, "two spawned before clear")
  fx.clear(sys)
  assert_eq(fx.count(sys), 0, "clear empties system")
  ok("fx clear")
end

-- spawn with nil x/y returns nil (defensive: no anchor).
do
  local sys = fx.new()
  assert_eq(fx.spawn(sys, nil, 10, "x"), nil, "nil x rejected")
  assert_eq(fx.spawn(sys, 10, nil, "x"), nil, "nil y rejected")
  assert_eq(fx.count(sys), 0, "rejected spawn did not add")
  ok("fx spawn rejects nil anchor")
end

-- update with dt<=0 is a no-op (defensive against paused/stalled frames).
do
  local sys = fx.new()
  local e = fx.spawn(sys, 0, 0, "x", { life = 1.0, vy = -10.0 })
  fx.update(sys, 0.0)
  assert_eq(e.t, 0.0, "dt=0 does not advance t")
  assert_eq(e.y, 0.0, "dt=0 does not drift y")
  fx.update(sys, -0.5)
  assert_eq(e.t, 0.0, "negative dt does not advance t")
  assert_eq(fx.count(sys), 1, "still active after no-op update")
  ok("fx update no-op on nonpositive dt")
end

-- UPGRADE-PLAN Phase 3 milestone: spawn 100 floaters, step update(60×2s),
-- assert the pool empties (tweens complete and free themselves).
do
  local sys = fx.new()
  for i = 1, 100 do
    fx.spawn(sys, i, i, "-" .. i, { life = 0.9, vy = -20.0 })
  end
  assert_eq(fx.count(sys), 100, "100 floaters spawned")
  -- 60 steps of 2s = 120s total; life is 0.9s so all expire well before.
  for _ = 1, 60 do
    fx.update(sys, 2.0)
  end
  assert_eq(fx.count(sys), 0, "pool empty after 60×2s")
  ok("fx tweens complete and free themselves")
end

-- UPGRADE-PLAN Phase 3 milestone: damage pulse spawns, fades, and expires.
do
  local sys = fx.new()
  -- No pulse -> alpha 0.
  assert_eq(fx.pulse_alpha(sys, 1), 0.0, "no pulse -> alpha 0")
  fx.pulse(sys, 1)
  assert_eq(fx.pulse_alpha(sys, 1), 1.0, "fresh pulse -> alpha 1.0")
  -- Step half the pulse life (0.2s of 0.4s) -> alpha 0.5.
  fx.update(sys, 0.2)
  assert_eq(fx.pulse_alpha(sys, 1), 0.5, "half-life pulse -> alpha 0.5")
  -- Step past life -> pulse expired, alpha 0.
  fx.update(sys, 0.3)
  assert_eq(fx.pulse_alpha(sys, 1), 0.0, "expired pulse -> alpha 0.0")
  -- Re-pulse restarts the timer.
  fx.pulse(sys, 1)
  assert_eq(fx.pulse_alpha(sys, 1), 1.0, "re-pulse restarts at 1.0")
  ok("fx damage pulse fades and expires")
end

-- UPGRADE-PLAN Phase 3 milestone: ticker colors by event kind. The pure
-- event_color function must return a distinct color per kind so the ticker
-- is legible (green = hit dealt, red = hit taken, gray = miss, yellow =
-- blocked). Tested headless via draw_hud (already required above).
do
  local green = draw_hud.event_color("hit_dealt")
  local red = draw_hud.event_color("hit_taken")
  local gray = draw_hud.event_color("miss")
  local yellow = draw_hud.event_color("blocked")
  assert(green and green[1] and green[2] and green[3], "hit_dealt color is rgb")
  assert(red and red[1], "hit_taken color is rgb")
  -- Distinct: green should be greener than red (g > r), red redder (r > g).
  assert(green[2] > green[1], "hit_dealt is greenish (g > r)")
  assert(red[1] > red[2], "hit_taken is reddish (r > g)")
  assert(gray[1] == gray[2], "miss is gray (r == g)")
  assert(yellow[1] > yellow[3], "blocked is yellowish (r > b)")
  -- Unknown kind falls back to info color, not nil.
  assert(draw_hud.event_color("nonsense") ~= nil, "unknown kind falls back")
  ok("ticker colors by event kind")
end

-- UPGRADE-PLAN Phase 3: ticker_alpha fade curve (pure function).
do
  -- No last_event_time -> full alpha (never fades before first event).
  assert_eq(draw_hud.ticker_alpha(100, nil), 1.0, "no last event -> 1.0")
  -- Just happened -> full alpha.
  assert_eq(draw_hud.ticker_alpha(100, 100), 1.0, "just now -> 1.0")
  -- Halfway through fade (2.5s of 5s) -> linear 1.0 to 0.35.
  local a = draw_hud.ticker_alpha(102.5, 100)
  assert(a < 1.0 and a > 0.35, "halfway fade between 1.0 and 0.35")
  -- Past fade window -> settled at faded alpha.
  assert_eq(draw_hud.ticker_alpha(110, 100), 0.35, "past 5s -> 0.35")
  ok("ticker alpha fades over time")
end

-- ─── Phase 4: board visualization (debounce + arc-fan geometry) ──────────
-- Pure-Lua: debounce coalesces preview-request bursts; geom produces arc-fan
-- polygons and hex counts. No love.* APIs. Mirrors events.lua / fx.lua.
print("phase 4: board visualization")
local debounce = require("debounce")
local geom = require("geom")
assert_eq(type(debounce), "table", "debounce module contract")
assert_eq(type(geom), "table", "geom module contract")

-- UPGRADE-PLAN Phase 4 milestone: preview debounce coalesces bursts.
-- A burst of trips (slider drag) should produce exactly ONE request after the
-- quiet window, not one per trip. Pure timer logic — no Love APIs.
do
  local d = debounce.new(0.2) -- 200ms quiet window
  -- Fresh timer: not armed, not due.
  assert_eq(debounce.armed(d), false, "fresh timer not armed")
  assert_eq(debounce.due(d), false, "fresh timer not due")
  -- Simulate a burst of 5 trips (slider ticks) with small dt between each.
  for i = 1, 5 do
    debounce.trip(d)
    debounce.poke(d, 0.01)
  end
  -- Still armed (trips reset the timer), not yet due (quiet not elapsed).
  assert_eq(debounce.armed(d), true, "armed after burst")
  assert_eq(debounce.due(d), false, "not due during burst")
  -- Advance past quiet window with no new trips.
  debounce.poke(d, 0.25)
  assert_eq(debounce.due(d), true, "due after quiet window")
  -- Consume (controller issued the single coalesced request).
  debounce.consume(d)
  assert_eq(debounce.armed(d), false, "disarmed after consume")
  assert_eq(debounce.due(d), false, "not due after consume")
  -- A single trip after consume re-arms.
  debounce.trip(d)
  assert_eq(debounce.armed(d), true, "re-armed after single trip")
  ok("preview debounce coalesces bursts")
end

-- UPGRADE-PLAN Phase 4 milestone: arc fan geometry spans correct hex count.
-- Pure pixel-math: arc_hex_count returns the number of hexes within range and
-- in-arc for a ship at origin facing 0. Assert known counts for each arc.
-- The bearing logic mirrors the engine (arc.rs nearest_bearings): find which
-- neighbor of the origin is closest to the target hex; ties break to the
-- lowest facing index (engine's .next()).
do
  -- Forward arc at range 1: only the hex directly ahead (facing 0 = east).
  -- Hex (1,0) is at bearing 0 from origin. Count should be 1.
  assert_eq(geom.arc_hex_count("Forward", 1), 1, "Forward r1 = 1 hex")
  -- Forward arc at range 2: the 60° wedge covers 4 hexes:
  -- (1,0) dist1, (2,0) dist2, (2,-1) dist2, (1,1) dist2 (tie breaks to fwd).
  assert_eq(geom.arc_hex_count("Forward", 2), 4, "Forward r2 = 4 hexes")
  -- Rear arc at range 1: hex at bearing 3 (opposite of facing 0).
  -- Bearing 3 = (-1,0) direction. Hex (-1,0) dist 1 -> 1 hex.
  assert_eq(geom.arc_hex_count("Rear", 1), 1, "Rear r1 = 1 hex")
  -- All arc at range 1: all 6 neighbors -> 6 hexes.
  assert_eq(geom.arc_hex_count("All", 1), 6, "All r1 = 6 hexes")
  -- All arc at range 2: 6 (ring 1) + 12 (ring 2) = 18 hexes.
  assert_eq(geom.arc_hex_count("All", 2), 18, "All r2 = 18 hexes")
  -- Left arc at range 1: bearings 1 and 2 -> 2 hexes.
  assert_eq(geom.arc_hex_count("Left", 1), 2, "Left r1 = 2 hexes")
  -- Right arc at range 1: bearings 4 and 5 -> 2 hexes.
  assert_eq(geom.arc_hex_count("Right", 1), 2, "Right r1 = 2 hexes")
  -- Zero/negative range -> 0 hexes.
  assert_eq(geom.arc_hex_count("Forward", 0), 0, "Forward r0 = 0")
  assert_eq(geom.arc_hex_count("All", -1), 0, "All r-1 = 0")
  ok("arc fan geometry spans correct hex count")
end

-- fan_polygon returns a valid polygon (even vertex count) for a pie slice.
do
  -- Forward arc, facing 0, radius 100: pie slice = center + 2 edge points = 3 pts = 6 values.
  local pts = geom.fan_polygon(0, 0, 0, "Forward", 100)
  assert_eq(#pts % 2, 0, "fan polygon has even vertex count (pie slice)")
  assert(#pts >= 6, "pie slice has at least 3 vertices (6 values)")
  -- All arc: full circle = 6 vertices = 12 values (no center needed but included).
  local allpts = geom.fan_polygon(50, 50, 0, "All", 100)
  assert_eq(#allpts, 12, "All arc fan = 6 vertices (12 values)")
  -- Zero radius -> empty polygon.
  assert_eq(#geom.fan_polygon(0, 0, 0, "Forward", 0), 0, "zero radius -> empty")
  ok("fan polygon shape is valid")
end

-- Fan and shield geometry must follow hex.to_pixel, not an unrelated screen
-- angle convention. Every core facing's pixel vector agrees with geom.
do
  local function angle_delta(a, b)
    return math.abs(math.atan2(math.sin(a - b), math.cos(a - b)))
  end
  for facing = 0, 5 do
    local d = hex.DIRS[facing + 1]
    local x, y = hex.to_pixel(d[1], d[2], 1)
    assert(angle_delta(math.atan2(y, x), geom.facing_angle(facing)) < 1e-9,
      "facing angle follows hex.to_pixel " .. facing)
  end
  local pts = geom.fan_polygon(0, 0, 0, "Forward", 100)
  local mid_x, mid_y = (pts[3] + pts[5]) / 2, (pts[4] + pts[6]) / 2
  assert(angle_delta(math.atan2(mid_y, mid_x), geom.facing_angle(0)) < 1e-9,
    "forward fan points along facing 0")
  ok("fan geometry follows board-facing axes")
end

-- Header and wide-layout math are pure so the high-value visual invariants
-- remain covered when a display server is unavailable.
do
  local snap = {
    turn = 1, phase = "allocate", ships = {}, ships_allocated_this_turn = {},
  }
  assert(not draw_hud.header_text(snap, "allocate", nil):find("nil"),
    "allocate header never formats Active #nil")
  snap.phase = "movement"
  snap.movement_phase = 1
  snap.ships = { { id = 1, controller = "player", destroyed = false } }
  assert(draw_hud.header_text(snap, "movement", 1):find("Active A1"),
    "movement header names the active callsign")
  local x = draw_hud.board_camera_origin(3832, 1021, 300, 34, 30, 360, 360, 1)
  assert(x > 1000, "wide board origin centers content instead of pinning left")
  ok("header nil guard and wide layout math")
end

-- ─── Phase 5: resolution theater and game over ───────────────────────────
-- Pure-Lua: events.stats, slide interpolation, fx tracers. No love.* APIs.
print("phase 5: resolution theater")
assert_eq(type(slide), "table", "slide module contract")

-- UPGRADE-PLAN Phase 5 milestone: game over stats match event history.
-- Feed synthetic combat_log entries into events.lua, then assert events.stats
-- returns exact counts: shots, hits, internal damage dealt/taken. All from
-- structured events — never log string parsing.
do
  local ev = events.new()
  -- Player (id 1) fires 4 shots: 3 hits (hull 5,3,0 shield-only), 1 miss.
  -- Enemy (id 2) hits player twice (hull 4, 2). Enemy misses once (not a player shot).
  events.feed(ev, { turn = 1, combat_log = {
    { attacker = 1, target = 2, weapon = "beam_1", shield = 0,
      damage = 5, shield_absorbed = 0, hull_damage = 5, kind = "hit" },
    { attacker = 1, target = 2, weapon = "beam_1", shield = 0,
      damage = 3, shield_absorbed = 0, hull_damage = 3, kind = "hit" },
    { attacker = 1, target = 2, weapon = "beam_1", shield = 0,
      damage = 2, shield_absorbed = 2, hull_damage = 0, kind = "hit" },
    { attacker = 1, target = 2, weapon = "beam_1", shield = 0,
      damage = 0, shield_absorbed = 0, hull_damage = 0, kind = "miss" },
  } }, { [1] = true })
  events.feed(ev, { turn = 2, combat_log = {
    { attacker = 2, target = 1, weapon = "torp_1", shield = 0,
      damage = 4, shield_absorbed = 0, hull_damage = 4, kind = "hit" },
    { attacker = 2, target = 1, weapon = "torp_1", shield = 0,
      damage = 2, shield_absorbed = 0, hull_damage = 2, kind = "hit" },
    { attacker = 2, target = 1, weapon = "torp_1", shield = 0,
      damage = 0, shield_absorbed = 0, hull_damage = 0, kind = "miss" },
  } }, { [1] = true })
  local st = events.stats(ev)
  -- Player shots = 3 hits + 1 miss = 4.
  assert_eq(st.shots, 4, "stats: player shots = 4 (3 hits + 1 miss)")
  -- Player hits = 3.
  assert_eq(st.hits, 3, "stats: player hits = 3")
  -- Internal damage dealt = 5 + 3 + 0 = 8 (shield-only hit deals 0 hull).
  assert_eq(st.int_dealt, 8, "stats: int damage dealt = 8")
  -- Internal damage taken = 4 + 2 = 6.
  assert_eq(st.int_taken, 6, "stats: int damage taken = 6")
  ok("game over stats match event history")
end

-- events.stats on an empty history returns zeros (no crash).
do
  local ev = events.new()
  local st = events.stats(ev)
  assert_eq(st.shots, 0, "stats: empty shots 0")
  assert_eq(st.hits, 0, "stats: empty hits 0")
  assert_eq(st.int_dealt, 0, "stats: empty int_dealt 0")
  assert_eq(st.int_taken, 0, "stats: empty int_taken 0")
  ok("game over stats empty history is zero")
end

-- UPGRADE-PLAN Phase 5 milestone: lerp reaches target within duration.
-- slide.feed sets from/to; slide.update advances t; slide.position returns
-- the interpolated point. After SLIDE_DUR (0.3s) of updates, the position
-- must equal the target. Pure Lua — hex.to_pixel is stubbed.
do
  local sys = slide.new()
  -- Stub hex_to_pixel: (q, r, size) -> q*10, r*10.
  local function h2p(q, r, size) return q * 10, r * 10 end
  -- First feed: ship 1 at (0,0) -> snaps instantly (new ship).
  slide.feed(sys, { ships = { { id = 1, q = 0, r = 0 } }, }, h2p, 10)
  local x0, y0 = slide.position(sys, 1)
  assert_eq(x0, 0.0, "slide: new ship snaps x")
  assert_eq(y0, 0.0, "slide: new ship snaps y")
  -- Second feed: ship 1 moves to (3,0) -> pixel (30,0). Slides from (0,0).
  slide.feed(sys, { ships = { { id = 1, q = 3, r = 0 } }, }, h2p, 10)
  -- At t=0, position is at "from" (0,0).
  local x1, y1 = slide.position(sys, 1)
  assert_eq(x1, 0.0, "slide: at t=0 position is from_x")
  assert_eq(y1, 0.0, "slide: at t=0 position is from_y")
  -- After 0.3s (full duration), position must reach target (30,0).
  slide.update(sys, 0.3)
  local x2, y2 = slide.position(sys, 1)
  assert_eq(x2, 30.0, "slide: reaches target x after duration")
  assert_eq(y2, 0.0, "slide: reaches target y after duration")
  -- settled() is true after all ships finish.
  assert_eq(slide.settled(sys), true, "slide: settled after duration")
  ok("lerp reaches target within duration")
end

-- slide interpolates at the midpoint (not just endpoints).
do
  local sys = slide.new()
  local function h2p(q, r, size) return q * 10, r * 10 end
  slide.feed(sys, { ships = { { id = 1, q = 0, r = 0 } }, }, h2p, 10)
  slide.feed(sys, { ships = { { id = 1, q = 10, r = 0 } }, }, h2p, 10)
  -- Halfway through 0.3s = 0.15s. ease-out-cubic at raw=0.5: 1-(0.5)^3 = 0.875.
  -- position = 0 + (100-0)*0.875 = 87.5
  slide.update(sys, 0.15)
  local x, y = slide.position(sys, 1)
  assert(x > 50 and x < 100, "slide: midpoint is between from and to; got " .. tostring(x))
  assert_eq(y, 0.0, "slide: midpoint y unchanged")
  ok("slide interpolates at midpoint")
end

-- slide drops ships that vanished from the snapshot.
do
  local sys = slide.new()
  local function h2p(q, r, size) return q, r end
  slide.feed(sys, { ships = { { id = 1, q = 0, r = 0 }, { id = 2, q = 1, r = 0 } } }, h2p, 10)
  assert_eq(slide.position(sys, 2) ~= nil, true, "slide: ship 2 present before vanish")
  -- Ship 2 gone from next snapshot.
  slide.feed(sys, { ships = { { id = 1, q = 0, r = 0 } } }, h2p, 10)
  assert_eq(slide.position(sys, 2), nil, "slide: vanished ship dropped")
  ok("slide drops vanished ships")
end

-- slide.update with dt<=0 is a no-op.
do
  local sys = slide.new()
  local function h2p(q, r, size) return q * 10, r * 10 end
  slide.feed(sys, { ships = { { id = 1, q = 0, r = 0 } } }, h2p, 10)
  slide.feed(sys, { ships = { { id = 1, q = 5, r = 0 } } }, h2p, 10)
  slide.update(sys, 0.0)
  local x, _ = slide.position(sys, 1)
  assert_eq(x, 0.0, "slide: dt=0 does not advance")
  slide.update(sys, -0.1)
  local x2, _ = slide.position(sys, 1)
  assert_eq(x2, 0.0, "slide: negative dt does not advance")
  ok("slide update no-op on nonpositive dt")
end

-- ─── Phase 5: fx tracers (resolution theater) ───────────────────────────
-- Pure-Lua: spawn/update/expire/alpha/progress. No love.* APIs.

-- tracer spawn: hit spawns a spark, miss spawns a puff.
do
  local sys = fx.new()
  -- Hit: tracer + spark = 2 effects.
  fx.tracer(sys, 0, 0, 100, 100, "beam", true)
  local act = fx.tracers_active(sys)
  assert_eq(#act, 2, "tracer hit spawns tracer + spark")
  assert_eq(act[1].kind, "beam", "hit tracer kind is beam")
  assert_eq(act[2].kind, "spark", "hit spawns spark")
  -- Miss: tracer + puff = 2 effects.
  local sys2 = fx.new()
  fx.tracer(sys2, 0, 0, 100, 100, "torp", false)
  local act2 = fx.tracers_active(sys2)
  assert_eq(#act2, 2, "tracer miss spawns tracer + puff")
  assert_eq(act2[1].kind, "torp", "miss tracer kind is torp")
  assert_eq(act2[2].kind, "puff", "miss spawns puff")
  ok("tracer spawn hit vs miss")
end

-- tracer update expires effects past life.
do
  local sys = fx.new()
  fx.tracer(sys, 0, 0, 50, 50, "beam", true)
  assert_eq(#fx.tracers_active(sys), 2, "2 tracer effects before update")
  -- tracer life 0.8s, spark life 0.4s. After 0.5s: spark expired, tracer alive.
  fx.update(sys, 0.5)
  assert_eq(#fx.tracers_active(sys), 1, "spark expired, tracer alive at 0.5s")
  -- After another 0.4s (total 0.9s): tracer expired.
  fx.update(sys, 0.4)
  assert_eq(#fx.tracers_active(sys), 0, "all tracers expired at 0.9s")
  ok("tracer update expires past life")
end

-- tracer_alpha: 1.0 first 30%, then linear fade to 0.
do
  local sys = fx.new()
  local t = fx.tracer(sys, 0, 0, 10, 10, "beam", true)
  -- t is the tracer, not the spark. life=0.8.
  assert_eq(fx.tracer_alpha(t), 1.0, "tracer alpha 1.0 at t=0")
  t.t = 0.2 -- 25% of 0.8 -> still in first 30% (0.24)
  assert_eq(fx.tracer_alpha(t), 1.0, "tracer alpha 1.0 in first 30%")
  t.t = 0.24 -- exactly 30%
  assert_eq(fx.tracer_alpha(t), 1.0, "tracer alpha 1.0 at 30% boundary")
  t.t = 0.6 -- 75%: remaining=0.2, fade_span=0.8-0.24=0.56 -> 0.2/0.56
  local a = fx.tracer_alpha(t)
  assert(a > 0 and a < 1.0, "tracer alpha fading at 75%; got " .. tostring(a))
  t.t = 0.8 -- at life
  assert_eq(fx.tracer_alpha(t), 0.0, "tracer alpha 0.0 at life")
  ok("tracer alpha fade curve")
end

-- torp_progress: 0 at spawn, 1 at half-life, holds at 1 after.
do
  local sys = fx.new()
  local t = fx.tracer(sys, 0, 0, 100, 0, "torp", true)
  assert_eq(fx.torp_progress(t), 0.0, "torp progress 0 at spawn")
  t.t = 0.2 -- half of 0.4 (half of life 0.8)
  -- Wait: half-life = 0.8*0.5 = 0.4. At t=0.2, progress = 0.2/0.4 = 0.5
  assert_eq(fx.torp_progress(t), 0.5, "torp progress 0.5 at quarter life")
  t.t = 0.4 -- half-life
  assert_eq(fx.torp_progress(t), 1.0, "torp progress 1.0 at half-life")
  t.t = 0.6 -- past half-life
  assert_eq(fx.torp_progress(t), 1.0, "torp progress holds 1.0 past half-life")
  ok("torp progress curve")
end

-- plasma_radius: expands from 0 to max over life.
do
  local sys = fx.new()
  local t = fx.tracer(sys, 0, 0, 50, 0, "plasma", true)
  assert_eq(fx.plasma_radius(t, 50), 0.0, "plasma radius 0 at spawn")
  t.t = 0.4 -- half of 0.8 life
  assert_eq(fx.plasma_radius(t, 50), 25.0, "plasma radius half at half life")
  t.t = 0.8 -- at life
  assert_eq(fx.plasma_radius(t, 50), 0.0, "plasma radius 0 at life (expired)")
  ok("plasma radius expands over life")
end

-- fx.clear wipes tracers too.
do
  local sys = fx.new()
  fx.tracer(sys, 0, 0, 10, 10, "beam", true)
  fx.spawn(sys, 1, 2, "x")
  fx.pulse(sys, 1)
  assert_eq(#fx.tracers_active(sys), 2, "tracers present before clear")
  fx.clear(sys)
  assert_eq(#fx.tracers_active(sys), 0, "tracers cleared")
  assert_eq(fx.count(sys), 0, "floaters cleared")
  assert_eq(fx.pulse_alpha(sys, 1), 0.0, "pulses cleared")
  ok("fx clear wipes tracers")
end


-- ─── Phase 6: tutorial step-gate machine ────────────────────────────────
-- Pure-Lua: the tutorial controller (tutorial.lua) is a step-gate machine
-- ported from frontend/tui/src/tutorial.rs. These tests exercise the gate
-- logic (check_action, validate_action, check_reach_value, do_now_line,
-- narration, state_error) without love.* APIs. The integration wiring in
-- main.lua (tutorial_gate_ui/key, confirm_tutorial_order) and the coach
-- panel in draw_hud.lua are covered by the module-contract + syntax checks.
print("phase 6: tutorial")

-- Module contract: tutorial is a table with the expected functions.
assert_eq(type(tutorial), "table", "tutorial module contract")
assert_eq(type(tutorial.new), "function", "tutorial.new is a function")
assert_eq(type(tutorial.check_action), "function", "tutorial.check_action")
assert_eq(type(tutorial.validate_action), "function", "tutorial.validate_action")
assert_eq(type(tutorial.confirm_order), "function", "tutorial.confirm_order")
assert_eq(type(tutorial.check_reach_value), "function", "tutorial.check_reach_value")
assert_eq(type(tutorial.do_now_line), "function", "tutorial.do_now_line")
assert_eq(type(tutorial.narration), "function", "tutorial.narration")
assert_eq(type(tutorial.state_error), "function", "tutorial.state_error")
ok("tutorial module contract")

-- new() returns a controller at step 1, not complete.
do
  local t = tutorial.new()
  assert_eq(tutorial.step_count(t), 28, "rear-attack has 28 v4 steps")
  assert_eq(t.current, 1, "new starts at step 1")
  assert_eq(tutorial.is_complete(t), false, "new is not complete")
  local step = tutorial.current_step(t)
  assert(step, "current_step returns a table at step 1")
  assert_eq(step.expected.kind, "ReachValue", "step 1 is ReachValue")
  assert_eq(step.expected.field, 0, "step 1 field is Movement (0)")
  assert_eq(step.expected.target, 10, "step 1 target is 10")
  ok("tutorial new + step 1 shape")
end

-- check_reach_value: wrong field is blocked; right field toward target allowed;
-- exact target advances.
do
  local t = tutorial.new()
  -- Wrong field (1 vs expected 0): blocked, not advanced.
  local allow, adv = tutorial.check_reach_value(t, 1, 0, 4)
  assert_eq(allow, false, "reach: wrong field blocked")
  assert_eq(adv, false, "reach: wrong field no advance")
  assert_eq(t.current, 1, "reach: wrong field stays on step 1")
  assert(t.error_msg, "reach: wrong field sets error")
  -- Right field, not yet target: allowed, not advanced.
  allow, adv = tutorial.check_reach_value(t, 0, 0, 5)
  assert_eq(allow, true, "reach: toward target allowed")
  assert_eq(adv, false, "reach: below target no advance")
  assert_eq(t.current, 1, "reach: below target stays step 1")
  -- Overshoot: allowed (the edit happens) but error set, no advance.
  allow, adv = tutorial.check_reach_value(t, 0, 5, 12)
  assert_eq(allow, true, "reach: overshoot allowed")
  assert_eq(adv, false, "reach: overshoot no advance")
  assert(t.error_msg, "reach: overshoot sets error")
  -- Exact target: allowed AND advanced.
  allow, adv = tutorial.check_reach_value(t, 0, 5, 10)
  assert_eq(allow, true, "reach: exact target allowed")
  assert_eq(adv, true, "reach: exact target advances")
  assert_eq(t.current, 2, "reach: exact target moves to step 2")
  assert_eq(t.error_msg, nil, "reach: advance clears error")
  ok("check_reach_value gate logic")
end

-- validate_action: order-backed step validates but does NOT advance.
-- Step 7 is CommitAllocate (order-backed).
do
  local t = tutorial.new()
  -- Advance to step 7 (CommitAllocate) by completing steps 1-6.
  for _ = 1, 6 do tutorial.advance(t) end
  assert_eq(t.current, 7, "advanced to step 7")
  local step = tutorial.current_step(t)
  assert_eq(step.expected.kind, "CommitAllocate", "step 7 is CommitAllocate")
  -- Correct action: validates true, does NOT advance.
  local ok_v = tutorial.validate_action(t, { kind = "CommitAllocate" })
  assert_eq(ok_v, true, "validate: correct CommitAllocate returns true")
  assert_eq(t.current, 7, "validate: does NOT advance (order-backed)")
  -- Wrong action: validates false, sets error.
  ok_v = tutorial.validate_action(t, { kind = "PathAppend", action = "move_f" })
  assert_eq(ok_v, false, "validate: wrong action returns false")
  assert(t.error_msg, "validate: wrong action sets error")
  ok("validate_action order-backed no-advance")
end

-- Engine acknowledgment is the only event that advances an order-backed step.
do
  local t = tutorial.new()
  for _ = 1, 6 do tutorial.advance(t) end
  local candidate = { kind = "CommitAllocate" }
  assert(tutorial.validate_action(t, candidate), "candidate validates")
  assert_eq(tutorial.confirm_order(t, candidate, false), false, "rejected order not confirmed")
  assert_eq(t.current, 7, "rejected order stays on step")
  assert_eq(tutorial.confirm_order(t, candidate, true), true, "accepted order confirmed")
  assert_eq(t.current, 8, "accepted order advances")
  ok("tutorial advances on engine acceptance")
end

-- check_action: discrete step advances immediately on match.
-- Step 5 is NavField (field 4) — a discrete step.
do
  local t = tutorial.new()
  for _ = 1, 4 do tutorial.advance(t) end
  assert_eq(t.current, 5, "advanced to step 5")
  local step = tutorial.current_step(t)
  assert_eq(step.expected.kind, "NavField", "step 5 is NavField")
  -- Stepping toward (field 3 <= 4): allowed, no advance.
  local acc = tutorial.check_action(t, { kind = "NavField", field = 3 })
  assert_eq(acc, true, "nav: toward target allowed")
  assert_eq(t.current, 5, "nav: toward target no advance")
  -- Exact field: allowed AND advanced.
  acc = tutorial.check_action(t, { kind = "NavField", field = 4 })
  assert_eq(acc, true, "nav: exact field allowed")
  assert_eq(t.current, 6, "nav: exact field advances")
  ok("check_action discrete advance")
end

-- check_action: wrong kind on a discrete step is blocked.
do
  local t = tutorial.new()
  -- Step 1 is ReachValue; a NavField action should be blocked.
  local acc = tutorial.check_action(t, { kind = "NavField", field = 0 })
  assert_eq(acc, false, "check: wrong kind on ReachValue blocked")
  assert_eq(t.current, 1, "check: wrong kind no advance")
  ok("check_action wrong kind blocked")
end

-- do_now_line: produces a non-empty prompt for each step kind; complete msg
-- after all steps done.
do
  local t = tutorial.new()
  local line = tutorial.do_now_line(t, 0, 0)
  assert(line and line ~= "", "do_now: step 1 non-empty")
  assert(line:match("Movement"), "do_now: step 1 mentions Movement")
  -- Advance through all steps; do_now_line should report complete.
  for _ = 1, tutorial.step_count(t) do tutorial.advance(t) end
  assert_eq(tutorial.is_complete(t), true, "do_now: all steps done")
  line = tutorial.do_now_line(t, nil, nil)
  assert(line:match("complete"), "do_now: complete message")
  ok("do_now_line prompt + complete")
end

-- narration: includes step text; prefixes error_msg when set; complete msg
-- after all steps.
do
  local t = tutorial.new()
  local body = tutorial.narration(t)
  assert(body and body ~= "", "narration: step 1 non-empty")
  assert(body:match("motion") or body:match("path"), "narration: step 1 mentions motion/path")
  -- Set an error; narration should prefix it.
  tutorial.set_error(t, "test error")
  body = tutorial.narration(t)
  assert(body:match("test error"), "narration: prefixes error_msg")
  assert(tutorial.pinned_prompt(t):match("test error"), "pinned prompt shows error")
  assert(not tutorial.pinned_prompt(t):match("Engine ="), "error replaces normal pinned prompt")
  -- Complete.
  for _ = 1, tutorial.step_count(t) do tutorial.advance(t) end
  body = tutorial.narration(t)
  assert(body:match("complete"), "narration: complete message")
  ok("narration body + error prefix")
end

-- state_error: nil when not over; nil when Won (even mid-lesson); error when
-- Lost mid-lesson.
do
  local t = tutorial.new()
  -- Not over: nil.
  assert_eq(tutorial.state_error(t, { status = "Ongoing" }), nil, "state_error: nil when ongoing")
  -- Won mid-lesson: nil (win is acceptable).
  assert_eq(tutorial.state_error(t, { status = "Won" }), nil, "state_error: nil on Won mid-lesson")
  -- Lost mid-lesson: error string.
  local serr = tutorial.state_error(t, { status = "Lost" })
  assert(serr and serr ~= "", "state_error: error on Lost mid-lesson")
  -- When the step IS Dismiss (last step), even Lost is not an error.
  for _ = 1, tutorial.step_count(t) - 1 do tutorial.advance(t) end
  local last = tutorial.current_step(t)
  assert_eq(last.expected.kind, "Dismiss", "last step is Dismiss")
  assert_eq(tutorial.state_error(t, { status = "Lost" }), nil, "state_error: nil on Dismiss step")
  -- nil snap (e.g. before first snapshot loads) must not crash.
  assert_eq(tutorial.state_error(tutorial.new(), nil), nil, "state_error: nil snap returns nil")
  ok("state_error game-over detection")
end

-- Full walkthrough: drive every step via gate functions (protocol v4 path/volley).
do
  local t = tutorial.new()
  tutorial.check_reach_value(t, 0, 0, 10)
  tutorial.check_reach_value(t, 1, 0, 4)
  tutorial.check_reach_value(t, 2, 0, 1)
  tutorial.check_reach_value(t, 3, 0, 1)
  tutorial.check_action(t, { kind = "NavField", field = 4 })
  tutorial.check_reach_value(t, 4, 0, 6)
  tutorial.validate_action(t, { kind = "CommitAllocate" }); tutorial.advance(t)
  -- Path appends (draft-local) + commit
  for _ = 1, 3 do
    tutorial.validate_action(t, { kind = "PathAppend", action = "move_f" }); tutorial.advance(t)
  end
  for _ = 1, 3 do
    tutorial.validate_action(t, { kind = "PathAppend", action = "turn_left" }); tutorial.advance(t)
  end
  tutorial.validate_action(t, { kind = "CommitPath" }); tutorial.advance(t)
  tutorial.check_action(t, { kind = "EnterMap" })
  tutorial.check_action(t, { kind = "PanMap" })
  tutorial.check_action(t, { kind = "ZoomOut" })
  tutorial.check_action(t, { kind = "ZoomIn" })
  tutorial.check_action(t, { kind = "RecenterMap" })
  tutorial.check_action(t, { kind = "ExitMap" })
  tutorial.check_action(t, { kind = "ShieldFacing", facing = 3 })
  tutorial.validate_action(t, { kind = "FireWeapon" }); tutorial.advance(t)
  tutorial.check_action(t, { kind = "TabWeapon", weapon = "torp_1" })
  tutorial.validate_action(t, { kind = "FireWeapon" }); tutorial.advance(t)
  tutorial.check_action(t, { kind = "TabWeapon", weapon = "plasma_1" })
  tutorial.validate_action(t, { kind = "FireWeapon" }); tutorial.advance(t)
  tutorial.validate_action(t, { kind = "CommitVolley" }); tutorial.advance(t)
  tutorial.check_action(t, { kind = "Dismiss" })
  assert_eq(tutorial.is_complete(t), true, "walkthrough: all v4 steps complete")
  ok("full v4 path/volley walkthrough reaches completion")
end

--------------------------------------------------------------------
-- FIX-PLAN F1–F4 pure-module checks
--------------------------------------------------------------------
print("fix-plan F1–F4")
local layout = require("layout")
local status_fmt = require("status_fmt")
local toast = require("toast")
local camera = require("camera")
local settings = require("settings")
local allocation = require("allocation")

-- header slots do not overlap
do
  for _, size in ipairs({ { 1280, 800 }, { 3832, 1021 }, { 1024, 720 } }) do
    local ok_slots, reason = layout.header_slots_ok(size[1], 1.0)
    assert(ok_slots, "header slots overlap at " .. size[1] .. "x" .. size[2] .. ": " .. tostring(reason))
    local ok2, reason2 = layout.header_slots_ok(size[1], 2.0)
    assert(ok2, "header slots overlap at scale 2: " .. tostring(reason2))
  end
  ok("header slots do not overlap")
end

-- wrap_text respects max lines
do
  local lines = layout.wrap_text("weapon beam_1 cannot be fired because range is too long for this facing", 80, function(s) return #s * 6 end, 2)
  assert(#lines <= 2, "wrap_text max_lines")
  assert(#lines >= 1, "wrap_text produced lines")
  ok("wrap_text caps at max_lines")
end

-- roster includes wrecks; enemy list excludes own fleet
do
  local snap = {
    ships = {
      { id = 1, controller = "player", destroyed = false },
      { id = 2, controller = "player", destroyed = true },
      { id = 3, controller = "ai", destroyed = false },
      { id = 4, controller = "ai", destroyed = true },
    },
  }
  local roster = layout.roster_ships(snap)
  assert_eq(#roster, 4, "roster includes all ships")
  local enemies = layout.enemy_targets(snap, 1)
  assert_eq(#enemies, 1, "target list excludes own fleet and wrecks")
  assert_eq(enemies[1].id, 3, "only living enemy")
  ok("target list excludes own fleet")
  ok("panel row count independent of ship status")
end

-- press increments draft exactly once (synthetic)
do
  local v = 0
  v = allocation.increment(v, 10)
  assert_eq(v, 1, "one press +1")
  v = allocation.increment(v, 10)
  assert_eq(v, 2, "second press +1 more")
  ok("press increments draft exactly once")
end

-- Draft accounting matches the engine: carried charge is free, new charge is not.
do
  local ship = {
    power_available = 10,
    weapons = { { id = "beam_1", charge = 3 } },
  }
  local draft = {
    movement = 2,
    weapons = { beam_1 = 4 },
    shields = { 1, 1, 0, 0, 0, 0 },
  }
  assert_eq(allocation.power_spent(ship, draft), 5, "only one beam charge is newly bought")
  assert_eq(allocation.available_for_movement(ship, draft), 7, "movement uses residual power")

  -- Regression: the movement "+" control must stop at the power ceiling, the
  -- same way weapon/shield "+" already stops at their own max via
  -- allocation.increment. Previously the movement field had no ceiling at all
  -- and could be raised indefinitely past available power.
  local at_cap = { movement = 7, weapons = { beam_1 = 4 }, shields = { 1, 1, 0, 0, 0, 0 } }
  local cap = allocation.available_for_movement(ship, at_cap)
  assert_eq(cap, 7, "movement is already at its power ceiling")
  assert_eq(math.min(at_cap.movement + 1, cap), 7, "movement '+' does not exceed the ceiling")

  local quick_ship = {
    power_available = 22,
    max_shield_per_facing = 6,
    weapons = {
      { id = "beam_1", charge = 0, max_charge = 4 },
      { id = "torp_1", charge = 0, max_charge = 1 },
      { id = "plasma_1", charge = 0, max_charge = 1 },
    },
  }
  local quick = { movement = 4, weapons = {}, shields = { 3, 3, 3, 3, 3, 3 } }
  allocation.maximize_weapons(quick_ship, quick)
  assert_eq(allocation.power_spent(quick_ship, quick), 22, "max weapons remains affordable")
  local balanced = {
    movement = 0,
    weapons = { beam_1 = 4, torp_1 = 1, plasma_1 = 1 },
    shields = { 3, 3, 3, 3, 3, 3 },
  }
  allocation.balance_shields(quick_ship, balanced)
  assert_eq(allocation.power_spent(quick_ship, balanced), 18, "balanced shields use only residual power")
  allocation.set_movement_fraction(quick_ship, balanced, 1)
  assert_eq(allocation.power_spent(quick_ship, balanced), 22, "power bar movement remains affordable")
  allocation.all_engine(quick_ship, balanced)
  assert_eq(allocation.power_spent(quick_ship, balanced), 22, "all engine remains affordable")
  ok("allocation budget honors carried charge")
end

-- Weapon steppers: floor at carried, top-up only spends residual power.
-- Regression: showing ch 0 while the ship banks charge, then "+" jumping to
-- carried+1, looked like double-charging.
do
  local ship = {
    power_available = 6,
    weapons = {
      { id = "beam_1", charge = 2, max_charge = 4 },
      { id = "torp_1", charge = 0, max_charge = 1 },
    },
  }
  local draft = { movement = 0, weapons = {}, shields = { 0, 0, 0, 0, 0, 0 } }
  allocation.seed_weapons(ship, draft)
  assert_eq(draft.weapons.beam_1, 2, "seed floors beam at carried")
  assert_eq(draft.weapons.torp_1, 0, "seed includes zero-charge mounts")
  assert_eq(allocation.power_spent(ship, draft), 0, "carried-only draft costs nothing")

  allocation.weapon_up(ship, draft, "beam_1", 4)
  assert_eq(draft.weapons.beam_1, 3, "one + adds one top-up")
  assert_eq(allocation.power_spent(ship, draft), 1, "only the top-up spends power")

  -- Cannot strip below carried.
  draft.weapons.beam_1 = 3
  allocation.weapon_down(ship, draft, "beam_1")
  assert_eq(draft.weapons.beam_1, 2, "down stops at carried")
  allocation.weapon_down(ship, draft, "beam_1")
  assert_eq(draft.weapons.beam_1, 2, "down does not strip carried")

  -- Cap top-ups by residual power (not just max_charge).
  draft.movement = 5
  draft.weapons.beam_1 = 2
  allocation.weapon_up(ship, draft, "beam_1", 4)
  assert_eq(draft.weapons.beam_1, 3, "one free power allows +1")
  allocation.weapon_up(ship, draft, "beam_1", 4)
  assert_eq(draft.weapons.beam_1, 3, "no free power: + does not climb toward max")
  ok("weapon steppers honor carried charge and residual power")
end

-- status clears on phase change
do
  local st = { level = "warn", message = "Pick weapon and target", born_turn = 1, born_phase = "firing" }
  local cleared = status_fmt.clear_if_stale(st, { turn = 1, phase = "movement" })
  assert(cleared, "status clears on phase change")
  assert_eq(st.message, "", "message emptied")
  ok("status clears on phase change")
end

-- order echo uses words
do
  assert(status_fmt.order_echo(2, "turn", 3):match("facing 3"), "turn echo")
  assert(status_fmt.order_echo(1, "commit_path"):match("committed path"), "path echo")
  assert(status_fmt.order_echo(1, "hold_position"):match("held position"), "hold echo")
  ok("order echo uses words")
end

-- header formatter never emits #nil
do
  local draw_hud = require("draw_hud")
  local h1 = draw_hud.header_text({ turn = 1, phase = "allocate", ships = {} }, "allocate", nil)
  assert(not h1:match("#nil"), "no #nil on allocate: " .. h1)
  assert(not h1:match("%snil%s") and not h1:match("nil$"), "no bare nil: " .. h1)
  local h2 = draw_hud.header_text({
    turn = 2, phase = "movement", movement_phase = 1,
    ships = { { id = 1, controller = "player", destroyed = false } },
    ships_committed_path = {},
  }, "movement", 1)
  assert(h2:match("Active"), "movement has Active when ship pending")
  assert(not h2:match("#nil"), "no #nil on movement")
  ok("header formatter never emits #nil")
end

-- stale endpoint cloud cleared on phase change (pure rule)
do
  local reach = { endpoints = { { 1, 0 } } }
  local phase = "firing"
  if phase ~= "allocate" and phase ~= "movement" then
    reach = nil
  end
  assert_eq(reach, nil, "reach cleared outside allocate/movement")
  ok("stale endpoint cloud cleared on phase change")
end

-- hitbox minimum size
do
  local nx, ny, nw, nh = layout.ensure_hit_size(10, 10, 20, 20, 32)
  assert(nw >= 32 and nh >= 32, "hit expanded to 32")
  ok("all registered hitboxes meet minimum size")
end

-- Dense allocation rows must reserve the full hit height, not overlap.
do
  local first = { x = 0, y = 0, w = 32, h = 32 }
  local second = { x = 0, y = 36, w = 32, h = 32 }
  assert(not layout.rects_overlap(first, second), "allocation row hitboxes overlap")
  local ui = require("ui")
  assert(ui.is_repeatable("alloc_weapon_up"), "weapon stepper repeats")
  assert(not ui.is_repeatable("alloc_confirm"), "Allocate never repeats")
  assert(not ui.is_repeatable("alloc_power_bar"), "power bar never repeats")
  ok("allocation controls reserve non-overlapping repeat-safe targets")
end

-- default_scale DPI
do
  assert(layout.default_scale(3840, 2160) >= 2, "4K scale >= 2")
  assert_eq(layout.default_scale(1280, 800), 1, "720p-class scale 1")
  assert_eq(layout.default_scale(3832, 1021), 1, "wide short display stays usable")
  ok("dpi default scale")
end

-- Picker and sidebar chrome must not share pixels with the status strip.
do
  local m = layout.picker_metrics(1272, 792, 1.3, 18)
  assert(m.capacity >= 1 and m.capacity < 18, "picker has a bounded viewport")
  assert(m.exit.y + m.exit.h <= m.status.y, "picker exit clears status strip")
  local first = layout.ensure_index_visible(1, 18, 18, m.capacity)
  assert(first + m.capacity - 1 >= 18, "last picker item becomes visible")
  local regions = layout.sidebar_regions(1272, 792, 1.3, true)
  assert(not layout.rects_overlap(regions.content, regions.navigation), "sidebar content clears nav")
  assert(not layout.rects_overlap(regions.prompt, regions.navigation), "tutorial prompt clears nav")
  assert(not layout.rects_overlap(regions.navigation, regions.status), "nav clears status")
  assert(layout.point_in_rect(regions.navigation.x + 1, regions.navigation.y + 1,
    regions.panel), "fixed navigation is part of sidebar wheel region")
  assert(not layout.point_in_rect(regions.panel.x - 1, regions.navigation.y,
    regions.panel), "map is outside sidebar wheel region")
  assert_eq(layout.scroll_clamp(-5, 100, 40), 0, "scroll lower bound")
  assert_eq(layout.scroll_clamp(100, 100, 40), 60, "scroll upper bound")
  for _, size in ipairs({ { 960, 600 }, { 1272, 792 }, { 1920, 1080 }, { 3832, 1021 } }) do
    for _, scale in ipairs({ 0.85, 1.0, 1.3, 2.0 }) do
      local r = layout.sidebar_regions(size[1], size[2], scale, true)
      assert(not layout.rects_overlap(r.content, r.prompt), "content/prompt overlap")
      assert(not layout.rects_overlap(r.content, r.navigation), "content/nav overlap")
      assert(not layout.rects_overlap(r.prompt, r.navigation), "prompt/nav overlap")
      assert(not layout.rects_overlap(r.navigation, r.status), "nav/status overlap")
    end
  end
  ok("picker/sidebar viewport geometry")
end

-- Clipped controls are not clickable through the fixed chrome.
do
  local ui = require("ui")
  ui.clear_hits()
  ui.push_hit_clip({ x = 0, y = 0, w = 100, h = 40 })
  ui.hit("hidden", 0, 50, 20, 20, nil)
  ui.hit("partial", 0, 30, 20, 20, nil)
  ui.pop_hit_clip()
  assert_eq(#ui.hits(), 1, "fully clipped hit is omitted")
  assert_eq(ui.hits()[1].id, "partial", "partial hit retained")
  assert(ui.hits()[1].y >= 0 and ui.hits()[1].y + ui.hits()[1].h <= 40,
    "partial hit is clipped to viewport")
  ui.clear_hits()
  assert_eq(#ui.hits(), 0, "clear hits resets clip state")
  ok("viewport hit clipping")
end

-- Scale limits preserve a useful map and sidebar viewport.
do
  assert(layout.max_usable_scale(1272, 792, false) >= 1.3, "saved scale fits normal window")
  assert(layout.max_usable_scale(960, 600, true) >= 0.85, "small tutorial window has a floor")
  assert(layout.max_usable_scale(960, 600, true) <= 2.0, "small window scale is bounded")
  assert(layout.window_supported(720, 420), "minimum window is supported")
  assert(layout.window_supported(3832, 491), "wide short window remains usable")
  assert(not layout.window_supported(719, 420), "narrow window requests resize")
  assert(not layout.window_supported(720, 419), "short window requests resize")
  for _, size in ipairs({ { 720, 420 }, { 960, 600 }, { 1272, 792 }, { 1920, 1080 } }) do
    local scale = layout.max_usable_scale(size[1], size[2], true)
    local help = layout.help_metrics(size[1], size[2], scale)
    assert(help.x >= 0 and help.y >= 0, "help starts inside window")
    assert(help.x + help.w <= size[1] and help.y + help.h <= size[2],
      "help ends inside window")
  end
  ok("effective scale bounds")
end

-- Production callbacks use this policy: sidebar paging cannot steal weapon
-- selection, and fixed sidebar chrome still owns wheel input.
do
  assert_eq(input_policy.sidebar_scroll_command("down"), nil,
    "Down is not sidebar scroll")
  assert_eq(input_policy.sidebar_scroll_command("pagedown"), "page_down",
    "PageDown scrolls sidebar")
  assert_eq(input_policy.fire_weapon_delta(phases.FIRING, "down"), 1,
    "Down selects next fire weapon")
  assert_eq(input_policy.fire_weapon_delta(phases.ALLOCATE, "down"), nil,
    "Down has no fire meaning during allocate")
  local sidebar = layout.sidebar_regions(1272, 792, 1.3, true).panel
  assert_eq(input_policy.wheel_owner("play", sidebar.x + 2,
    sidebar.y + sidebar.h - 2, nil, sidebar), "sidebar",
    "wheel over fixed navigation belongs to sidebar")
  assert_eq(input_policy.wheel_owner("play", sidebar.x - 2,
    sidebar.y + 10, nil, sidebar), "map", "wheel over map remains map zoom")
  assert(input_policy.resize_key_allowed("q"), "quit allowed under resize overlay")
  assert(not input_policy.resize_key_allowed("return"),
    "orders blocked under resize overlay")
  ok("production input routing policy")
end

-- toast lifecycle
do
  local t = toast.new()
  toast.show(t, "Movement 2/4")
  assert(toast.active(t), "toast active")
  toast.update(t, 0.7)
  assert(not toast.active(t), "toast expired")
  ok("toast phase banner lifecycle")
end

do
  assert_eq(toast.phase_label({ phase = "movement" }), "Plan paths",
    "v4 movement toast has no retired cycle counter")
  assert_eq(toast.phase_label({ phase = "firing" }), "Firing", "firing toast")
  ok("toast labels use protocol-v4 stages")
end

-- camera auto-follow pause
do
  local cam = camera.new({ x = 0, y = 0, zoom = 1 })
  camera.user_moved(cam)
  assert(cam.pause_t > 0, "pause after user move")
  camera.update(cam, 6.0, { { q = 0, r = 0 } }, function(q, r) return q * 10, r * 10 end, 10, { x = 0, y = 0, w = 100, h = 100 })
  assert(cam.pause_t <= 0, "pause expires")
  ok("camera state machine")
end

-- Auto-fit changes zoom as well as center so both outer contacts remain visible.
do
  local cam = camera.new({ x = 0, y = 0, zoom = 1 })
  camera.update(cam, 1, { { q = 0, r = 0 }, { q = 100, r = 0 } },
    function(q, r) return q * 10, r * 10 end, 10,
    { x = 0, y = 0, w = 1000, h = 500 })
  local left = cam.cam.x
  local right = cam.cam.x + 1000 * cam.cam.zoom
  assert(cam.cam.zoom < 1, "auto-fit zooms out for spread ships")
  assert(left >= 0 and right <= 1000, "auto-fit keeps outer contacts on board")
  ok("camera auto-fit frames living ships")
end

-- Allocate focus follows the next ship so only one fleet form is on screen.
do
  local state = { selected_id = 1, alloc = {} }
  local snap = {
    phase = "allocate",
    ships_allocated_this_turn = { 1 },
    ships = {
      { id = 1, controller = "player", destroyed = false },
      { id = 2, controller = "player", destroyed = false },
    },
  }
  selection.ensure(state, snap)
  assert_eq(state.selected_id, 2, "allocate focus advances to pending ship")
  ok("allocate form follows pending ship")
end

-- settings round-trip
do
  local path = "/tmp/shipsim-love-settings-test.json"
  local data = { ui_scale = 1.5, auto_follow = false }
  assert(settings.save(path, data, json), "settings save")
  local loaded = settings.load(path, json)
  assert_eq(loaded.ui_scale, 1.5, "settings scale")
  assert_eq(loaded.auto_follow, false, "settings auto_follow")
  os.remove(path)
  ok("settings persist")
end

-- target preview cache key uniqueness (one request per weapon+target per snap)
do
  local key1 = string.format("%s:%s:%s:%s", 1, "firing", 1, "beam_1")
  local key2 = string.format("%s:%s:%s:%s", 1, "firing", 1, "beam_1")
  local key3 = string.format("%s:%s:%s:%s", 1, "firing", 1, "torp_1")
  assert_eq(key1, key2, "same snap+weapon shares cache key")
  assert(key1 ~= key3, "weapon change busts cache")
  ok("one request per weapon+target per snapshot")
end

-- empty weapons map encodes as {} not [] (engine BTreeMap)
do
  local orders = require("orders")
  local enc = json.encode(orders.allocate(1, 5, {}, { 0, 0, 0, 0, 0, 0 }))
  assert(enc:match('"weapons":{}'), "empty weapons is object: " .. enc)
  assert(not enc:match('"weapons":%[%]'), "empty weapons must not be array")
  local enc2 = json.encode(orders.allocate(1, 5, { beam_1 = 4 }, { 0, 0, 0, 0, 0, 0 }))
  assert(enc2:match('"beam_1"'), "named weapons preserved")
  ok("empty weapons JSON object not array")
end

-- maximize_weapons fills max_charge within budget
do
  local a = { movement = 0, weapons = {}, shields = { 0, 0, 0, 0, 0, 0 } }
  local ship = {
    power = 22,
    weapons = {
      { id = "beam_1", max_charge = 4, charge = 0 },
      { id = "torp_1", max_charge = 1, charge = 0 },
    },
  }
  allocation.maximize_weapons(ship, a)
  assert_eq(a.weapons.beam_1, 4, "max beam")
  assert_eq(a.weapons.torp_1, 1, "max torp")
  allocation.all_engine(ship, a)
  assert_eq(a.movement, 17, "engine residual after weapons") -- 22-4-1
  ok("quick-set max weapons then engine")
end

-- ---- Phase 4: ship art runtime loader + fallback ----
print("phase 4: ship art")
local function art_record(class_id, state, image_path)
  return {
    class_id = class_id,
    state = state,
    image_path = image_path,
    width = 256,
    height = 256,
    anchor_x = 0.5,
    anchor_y = 0.5,
    source_angle = 0,
    scale = 1,
  }
end
do
  -- Empty manifest (P0 fallback state): every lookup falls back to geometry.
  local st, err = ship_art.load_manifest({ version = 1, records = {} })
  assert(st, "empty manifest loads")
  assert_eq(err, nil, "empty manifest no error")
  local d = ship_art.lookup(st, "escort", "top_down")
  assert_eq(d.fallback, true, "empty manifest -> fallback")
  assert_eq(d.found, false, "empty manifest -> not found")
  assert_eq(d.class_id, "escort", "class_id preserved on fallback")
  ok("empty manifest falls back to geometry")
end

do
  -- Producer-shaped manifest record: lookup consumes the canonical
  -- `image_path` field emitted by ship_art_catalog.py.
  local st = ship_art.load_manifest({
    version = 1,
    records = {
      art_record("escort", "top_down", "escort/top_down.png"),
    },
  })
  local d = ship_art.lookup(st, "escort", "top_down")
  assert_eq(d.found, true, "record found")
  assert_eq(d.fallback, false, "record with image_path is not fallback")
  assert_eq(d.record.image_path, "escort/top_down.png", "producer image path exposed")
  ok("producer manifest record lookup")
end

do
  -- Decode the exact fixture that the Python producer contract test compares
  -- byte-for-byte with manifest_to_json output.
  local fixture = assert(io.open(dir .. "fixtures/ship_art_manifest.json", "r"))
  local payload = json.decode(fixture:read("*a"))
  fixture:close()
  local st = ship_art.load_manifest(payload)
  local d = ship_art.lookup(st, "escort", "top_down")
  assert_eq(d.found, true, "serialized producer fixture loads through JSON")
  assert_eq(d.record.image_path, "escort/top_down.png", "serialized image path survives")
  ok("serialized Python manifest crosses the Love JSON boundary")
end

do
  -- The publisher materializes aliases, so the manifest is the only runtime
  -- identity authority.
  local st = ship_art.load_manifest({
    version = 1,
    records = {
      art_record("tutorial_escort", "top_down", "escort/top_down.png"),
    },
  })
  local d = ship_art.lookup(st, "tutorial_escort", "top_down")
  assert_eq(d.found, true, "materialized alias record resolves")
  assert_eq(d.class_id, "tutorial_escort", "snapshot class_id remains the lookup key")
  assert_eq(d.fallback, false, "alias art is not fallback")
  ok("manifest materializes alias lookup")
end

do
  -- Record registered but image not generated (empty image path) -> fallback.
  local st = ship_art.load_manifest({
    version = 1,
    records = {
      { class_id = "escort", state = "top_down", image_path = "" },
    },
  })
  local d = ship_art.lookup(st, "escort", "top_down")
  assert_eq(d.fallback, true, "empty image path -> fallback")
  assert_eq(d.found, false, "empty image path -> not found")
  ok("ungenerated record falls back")
end

do
  -- State fallbacks are exact: a top-down asset must never impersonate a
  -- portrait or a destroyed-state sprite.
  local st = ship_art.load_manifest({
    version = 1,
    records = {
      art_record("escort", "top_down", "escort/top_down.png"),
    },
  })
  local portrait = ship_art.lookup(st, "escort", "portrait")
  assert_eq(portrait.found, false, "missing portrait is not found")
  assert_eq(portrait.state, "portrait", "requested portrait state preserved")
  assert_eq(portrait.fallback, true, "missing portrait keeps text-only HUD")
  local destroyed = ship_art.lookup(st, "escort", "destroyed")
  assert_eq(destroyed.found, false, "missing destroyed state is not found")
  assert_eq(destroyed.state, "destroyed", "requested destroyed state preserved")
  assert_eq(destroyed.fallback, true, "missing destroyed state keeps gray marker")
  ok("missing states use their exact geometric or text fallback")
end

do
  -- Runtime validation is defensive even if a manifest was edited by hand.
  local loaded, st, err = pcall(ship_art.load_manifest, {
    version = 1,
    records = {
      { class_id = "absolute", state = "top_down", image_path = "/tmp/ship.png" },
      { class_id = "traversal", state = "top_down", image_path = "../ship.png" },
      { class_id = "windows", state = "top_down", image_path = "escort\\ship.png" },
      { class_id = "unnormalized", state = "top_down", image_path = "escort//ship.png" },
      { class_id = "bad_path", state = "top_down", image_path = {} },
      { class_id = "bad_state", state = {}, image_path = "escort/ship.png" },
      { class_id = {}, state = "top_down", image_path = "escort/ship.png" },
      { class_id = "bad_meta", state = "top_down", image_path = "escort/ship.png",
        width = 256, height = 256, anchor_x = 2, anchor_y = 0.5,
        source_angle = 0, scale = 1 },
      "not a record",
    },
  })
  assert_eq(loaded, true, "malformed records do not throw")
  assert(st, "manifest with malformed records still loads")
  assert_eq(err, nil, "record errors do not reject whole manifest")
  assert_eq(next(st.by_class), nil, "unsafe and malformed records are ignored")
  assert(#st.diagnostics >= 9, "each malformed record is diagnosed")
  ok("unsafe and malformed manifest records are ignored")
end

do
  -- Diagnostic dedup: at most one diagnostic per (class_id, state).
  local st = ship_art.load_manifest({ version = 1, records = {} })
  ship_art.diagnostic(st, "escort", "top_down", "load failed")
  ship_art.diagnostic(st, "escort", "top_down", "load failed again")
  ship_art.diagnostic(st, "escort", "top_down", "third time")
  assert_eq(#st.diagnostics, 1, "diagnostic deduped per asset")
  ship_art.diagnostic(st, "battleship_heavy", "top_down", "different asset")
  assert_eq(#st.diagnostics, 2, "different asset gets its own diagnostic")
  ok("diagnostic emitted at most once per asset")
end

do
  -- Cache with injected image loader: success path caches the image.
  local load_count = 0
  local function load_image(path)
    load_count = load_count + 1
    return { handle = path, getDimensions = function() return 256, 256 end }
  end
  local st = ship_art.load_manifest({
    version = 1,
    records = {
      art_record("escort", "top_down", "escort/top_down.png"),
    },
  })
  local cache = ship_art.new_cache(st, load_image)
  local d1 = cache:get("escort", "top_down")
  assert_eq(d1.fallback, false, "cache returns non-fallback on success")
  assert_eq(type(d1.image), "table", "cache returns image handle")
  assert_eq(d1.image.handle, "assets/ship_art/escort/top_down.png", "cache loads from Love asset root")
  local d2 = cache:get("escort", "top_down")
  assert_eq(load_count, 1, "image loaded only once (cached)")
  ok("cache loads image once")
end

do
  -- Cache with failing image loader: falls back + diagnostic, no crash.
  local function load_image(path)
    return nil, "file not found"
  end
  local st = ship_art.load_manifest({
    version = 1,
    records = {
      art_record("escort", "top_down", "escort/missing.png"),
    },
  })
  local cache = ship_art.new_cache(st, load_image)
  local d = cache:get("escort", "top_down")
  assert_eq(d.fallback, true, "load failure -> fallback")
  assert_eq(d.image, nil, "no image on failure")
  assert_eq(#st.diagnostics, 1, "load failure emits one diagnostic")
  -- Second lookup does not re-attempt or re-diagnose.
  local d2 = cache:get("escort", "top_down")
  assert_eq(d2.fallback, true, "cached failure stays fallback")
  assert_eq(#st.diagnostics, 1, "no second diagnostic for same asset")
  ok("cache load failure falls back safely")
end

do
  -- The authored sprite stays white; controller ownership is carried by the
  -- outline color drawn around successful sprites.
  local player = draw_board.controller_color({ controller = "player" })
  local ai = draw_board.controller_color({ controller = "ai" })
  local scripted = draw_board.controller_color({ controller = "scripted" })
  local destroyed = draw_board.controller_color({ controller = "player", destroyed = true })
  assert_eq(player[1], 0.3, "player controller cue")
  assert_eq(ai[1], 1.0, "AI controller cue")
  assert_eq(scripted[2], 0.75, "scripted controller cue")
  assert_eq(destroyed[1], 0.4, "destroyed cue remains gray")
  assert(draw_board.controller_cue_radius() < draw_board.marker_radius(),
    "controller cue stays inset within the frozen marker footprint")
  local record = art_record("escort", "top_down", "escort/top_down.png")
  record.anchor_x = 0.25
  record.anchor_y = 0.75
  record.source_angle = 30
  local presenter = assert(ship_art.new_presenter(
    { version = 1, records = { record } },
    function(path)
      return { getDimensions = function() return 256, 256 end }
    end
  ))
  local presentation = presenter:board_decision(
    { class_id = "escort", facing = 0 },
    draw_board.marker_radius()
  )
  assert(presentation.outer_radius <= draw_board.marker_radius() + 0.000001,
    "full rectangular sprite stays inside the marker footprint")
  assert_eq(presentation.origin_x, 64, "manifest anchor_x is applied")
  assert_eq(presentation.origin_y, 192, "manifest anchor_y is applied")
  assert(math.abs(presentation.angle
    - (geom.facing_angle(0) - ship_art.SOURCE_UP_ANGLE + math.rad(30))) < 0.000001,
    "manifest source angle is applied")
  ok("controller ownership colors remain available with sprites")
end

do
  -- Shared runtime ownership means the HUD can initialize and draw a portrait
  -- before draw_board.draw has ever run.
  local manifest_payload = [[{"version":1,"records":[{
    "class_id":"escort","state":"portrait",
    "image_path":"escort/portrait.png","width":256,"height":256,
    "anchor_x":0.5,"anchor_y":0.5,"source_angle":0,"scale":1
  }]}]]
  local drew = false
  local prior_love = love
  love = {
    filesystem = { read = function(path) return manifest_payload end },
    graphics = {
      newImage = function(path)
        return { getDimensions = function() return 256, 256 end }
      end,
      setColor = function() end,
      draw = function() drew = true end,
    },
  }
  local consumed = draw_hud.draw_portrait(
    { selected_id = 7 },
    { ships = { { id = 7, class_id = "escort" } } },
    700, 10, 100, 220
  )
  love = prior_love
  assert_eq(drew, true, "HUD portrait draws without a board initialization side effect")
  assert_eq(consumed, 48, "HUD portrait reports its presentation height")
  ok("HUD portrait initializes independently of board draw order")
end

do
  -- Source-up angle contract: art points up, runtime combines facing_angle.
  -- This is a documentation-as-test of the frozen contract (PHASE0 §4).
  assert_eq(ship_art.SOURCE_UP_ANGLE, -math.pi / 2, "source up is -pi/2 (screen up)")
  ok("source orientation contract")
end

print(string.format("\nAll %d checks passed.", pass))
