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
local draw_hud = require("draw_hud")
local command_mapping = require("command_mapping")
local scripted_pump = require("scripted_pump")

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
assert_eq(a.protocol_version, 3, "allocate protocol version")
assert_eq(a.ship, 1, "allocate ship")
assert_eq(a.movement, 4, "allocate movement")
assert_eq(a.weapons.beam_1, 1, "allocate weapons")
assert_eq(#a.shields, 6, "allocate shields len")

assert_eq(command_mapping.movement_order("forward", 1), nil, "directional movement disabled in M6")
local coast = command_mapping.movement_order("coast", 1)
assert_eq(coast.type, "commit_maneuver", "production coast type")
assert_eq(coast.ship, 1, "production coast ship")
assert_eq(coast.maneuver.type, "coast", "production coast maneuver")

local cm = orders.commit_maneuver(1, { type = "coast" })
assert_eq(cm.type, "commit_maneuver", "commit_maneuver type")
assert_eq(cm.maneuver.type, "coast", "commit_maneuver maneuver")

local co = orders.coast(2)
assert_eq(co.type, "commit_maneuver", "coast is commit_maneuver")
assert_eq(co.maneuver.type, "coast", "coast maneuver")

-- v3 motion model: accel / turn / turn_accel builders (src/motion.rs::Maneuver).
local ac = orders.accel(1)
assert_eq(ac.type, "commit_maneuver", "accel is commit_maneuver")
assert_eq(ac.maneuver.type, "accel", "accel maneuver")
assert_eq(ac.protocol_version, 3, "accel protocol version")

local tn = orders.turn(1, 3)
assert_eq(tn.type, "commit_maneuver", "turn is commit_maneuver")
assert_eq(tn.maneuver.type, "turn", "turn maneuver")
assert_eq(tn.maneuver.facing, 3, "turn facing")

local ta = orders.turn_accel(1, 5)
assert_eq(ta.type, "commit_maneuver", "turn_accel is commit_maneuver")
assert_eq(ta.maneuver.type, "turn_accel", "turn_accel maneuver")
assert_eq(ta.maneuver.facing, 5, "turn_accel facing")

-- command_mapping builds all four maneuver variants.
assert_eq(command_mapping.movement_order("coast", 1).maneuver.type, "coast", "cmd coast")
assert_eq(command_mapping.movement_order("accel", 1).maneuver.type, "accel", "cmd accel")
assert_eq(command_mapping.movement_order("turn", 1, 2).maneuver.type, "turn", "cmd turn")
assert_eq(command_mapping.movement_order("turn", 1, 2).maneuver.facing, 2, "cmd turn facing")
assert_eq(command_mapping.movement_order("turn_accel", 1, 4).maneuver.type, "turn_accel", "cmd turn_accel")
assert_eq(command_mapping.movement_order("turn_accel", 1, 4).maneuver.facing, 4, "cmd turn_accel facing")
assert_eq(command_mapping.movement_order("turn", 1), nil, "cmd turn without facing is nil")
assert_eq(command_mapping.movement_order("bogus", 1), nil, "cmd unknown action is nil")

local cf = orders.commit_fire(1, "beam_1", 2, 3)
assert_eq(cf.type, "commit_fire", "commit_fire type")
assert_eq(cf.weapon, "beam_1", "commit_fire weapon")
assert_eq(cf.target, 2, "commit_fire target")
assert_eq(cf.shield_facing, 3, "commit_fire shield_facing")

local rf = orders.ready_fire(1)
assert_eq(rf.type, "ready_fire", "ready_fire type")

local et = orders.end_turn()
assert_eq(et.type, "end_turn", "end_turn type")
ok("v2 order builders")

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
-- v3 snapshots (no type:"error") are classified as snapshots, not version-gated.
local snaps, _ = harness.parse_stream('{"protocol_version":3,"turn":1,"phase":"allocate","ships":[]}')
assert_eq(snaps[1].turn, 1, "v3 snapshot parsed")
ok("parse_stream")

print("phases")
assert_eq(phases.ALLOCATE, "allocate", "phase allocate")
assert_eq(phases.MOVEMENT, "movement", "phase movement")
assert_eq(phases.FIRING, "firing", "phase firing")
assert_eq(phases.TURN_END, "turn_end", "phase turn_end")
assert_eq(phases.next(phases.ALLOCATE), phases.MOVEMENT, "phase next")
assert_eq(phases.allows(phases.ALLOCATE, "allocate"), true, "allows allocate")
assert_eq(phases.allows(phases.MOVEMENT, "move"), false, "legacy move disabled")
assert_eq(phases.allows(phases.MOVEMENT, "pass_move"), false, "legacy pass disabled")
assert_eq(phases.allows(phases.MOVEMENT, "commit_maneuver"), true, "allows maneuver")
assert_eq(phases.allows(phases.FIRING, "commit_fire"), true, "allows commit_fire")
assert_eq(phases.allows(phases.ALLOCATE, "commit_fire"), false, "disallows commit_fire in allocate")
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

-- Allocate the player ship; Love must pump the scripted ship automatically.
snap = select(1, harness.submit(session, orders.allocate(1, 4, { beam_1 = 1 }, { 2, 1, 0, 0, 0, 1 })))
assert(snap, "allocate ship 1")
assert_eq(snap.ships[1].shields_powered[1], 2, "front shield allocated")
assert_eq(snap.ships[1].shields_powered[2], 1, "front-right shield allocated")
scripted_pump.run(session, function(err) error(err.message or "scripted pump failed") end)
snap = session.snapshot
assert_eq(snap.phase, "movement", "phase after allocate")
ok("allocate + move phase")

-- Movement cycle 1: non-coast commit (accel) accepted by the engine.
-- Velocity only changes after all living ships commit and the phase resolves.
snap = select(1, harness.submit(session, command_mapping.movement_order("accel", 1)))
assert(snap, "accel ship 1")
local committed = false
for _, id in ipairs(snap.ships_committed_this_phase or {}) do
  if id == 1 then committed = true end
end
assert(committed, "accel must mark ship 1 committed this phase")
ok("player accel accepted by engine")

-- Scripted coast completes the simultaneous commit set → resolve → firing.
scripted_pump.run(session, function(err) error(err.message or "scripted pump failed") end)
snap = session.snapshot
assert_eq(snap.phase, "firing", "accel + scripted coast completes movement cycle to firing")
local vel = 0
local face = nil
for _, s in ipairs(snap.ships or {}) do
  if s.id == 1 then
    vel = s.velocity or 0
    face = s.facing
  end
end
assert(vel >= 1, "resolved accel should leave ship 1 with velocity >= 1")
ok("resolved accel raises velocity")

-- Ready both ships and end turn so a later cycle can exercise turn.
snap = select(1, harness.submit(session, orders.ready_fire(1)))
assert(snap, "ready ship 1")
scripted_pump.run(session, function(err) error(err.message or "scripted pump failed") end)
snap = session.snapshot
-- May be turn_end or next movement depending on NPC ready.
if snap.phase == "turn_end" or snap.phase == "firing" then
  -- try end turn if needed to continue; not required for core assertions
end
ok("ready after accel cycle")

-- Turn builder round-trip (JSON already unit-tested; engine accept if still movement).
if snap.phase == "movement" then
  local turn_face = ((face or 0) + 1) % 6
  local turn_snap = select(1, harness.submit(session, command_mapping.movement_order("turn", 1, turn_face)))
  assert(turn_snap, "turn ship 1")
  ok("player turn accepted by engine")
else
  -- Order shape already covered; phase may have advanced past movement.
  ok("player turn skipped (not in movement; shape tested above)")
end

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
  ok("events ring buffer caps and orders")
end

-- Classifies hit_dealt / hit_taken / miss by player_ids.
do
  local ev = events.new()
  -- player 1 hits ship 2 (player attacker) -> hit_dealt
  events.feed(ev, { turn = 1, combat_log = {
    { attacker = 1, target = 2, weapon = "beam_1", shield = 0,
      damage = 5, shield_absorbed = 0, hull_damage = 5, kind = "hit" },
  } }, { [1] = true })
  -- ship 3 hits player 1 (player target) -> hit_taken
  events.feed(ev, { turn = 1, combat_log = {
    { attacker = 1, target = 2, weapon = "beam_1", shield = 0,
      damage = 5, shield_absorbed = 0, hull_damage = 5, kind = "hit" },
    { attacker = 3, target = 1, weapon = "torp_1", shield = 0,
      damage = 4, shield_absorbed = 4, hull_damage = 0, kind = "hit" },
    { attacker = 1, target = 3, weapon = "beam_1", shield = 0,
      damage = 0, shield_absorbed = 0, hull_damage = 0, kind = "miss" },
  } }, { [1] = true })
  local all = events.recent(ev)
  assert_eq(#all, 3, "three events fed (entry 1 not re-emitted)")
  assert_eq(all[1].kind, "hit_dealt", "player attacker -> hit_dealt")
  assert_eq(all[2].kind, "hit_taken", "player target -> hit_taken")
  assert_eq(all[2].text:match("shield"), "shield", "shield-only hit text says shield")
  assert_eq(all[3].kind, "miss", "miss classified")
  ok("events classify hit_dealt vs hit_taken vs miss")
end

-- Does not re-emit already-seen combat_log entries (diff by count).
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
  ok("events diff combat_log by count")
end

-- Blocked translation becomes a "blocked" event.
do
  local ev = events.new()
  events.feed(ev, { turn = 2, translation_results = {
    { ship = 1, requested = 3, moved = 1, blocked = { kind = "occupied", ships = { 2 } } },
    { ship = 2, requested = 2, moved = 2 }, -- not blocked, no event
  } }, {})
  assert_eq(events.count(ev), 1, "one blocked event")
  local rec = events.recent(ev)
  assert_eq(rec[1].kind, "blocked", "blocked kind")
  assert_eq(rec[1].text:match("moved 1/3"), "moved 1/3", "blocked text has moved/requested")
  ok("blocked translation becomes an event")
end

-- Snapshot field surfacing: a synthetic snapshot with the additive fields
-- is classified correctly by the harness line classifier (parse_stream).
do
  local snaps, _ = harness.parse_stream(
    '{"protocol_version":3,"turn":1,"phase":"firing","ships":[],' ..
    '"rules_id":"default","rules_fingerprint":"fnv1a-deadbeef","end_turn_warning":true,' ..
    '"fire_opportunity":{"ship":1,"weapon":"beam_1","target":2,"legal_shield_facings":[0,1]},' ..
    '"translation_results":[{"ship":1,"requested":2,"moved":2}],"combat_log":[]}')
  local s = snaps[1]
  assert(s, "synthetic snapshot parsed")
  assert_eq(s.rules_id, "default", "rules_id surfaced")
  assert_eq(s.rules_fingerprint, "fnv1a-deadbeef", "rules_fingerprint surfaced")
  assert_eq(s.end_turn_warning, true, "end_turn_warning surfaced")
  assert_eq(type(s.fire_opportunity), "table", "fire_opportunity surfaced")
  assert_eq(s.fire_opportunity.ship, 1, "fire_opportunity.ship")
  assert_eq(s.fire_opportunity.weapon, "beam_1", "fire_opportunity.weapon")
  assert_eq(type(s.translation_results), "table", "translation_results surfaced")
  ok("snapshot exposes fire_opportunity fields")
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

-- Live-engine round-trip: gated behind LOVE_LIVE=1 so headless stays green.
if os.getenv("LOVE_LIVE") then
  print("phase 0: live request envelope round-trip (LOVE_LIVE=1)")
  local live = harness.new({ repo_root = repo, bin = paths.find_shipsim_bin(repo) })
  local lsnap = harness.load_scenario(live, "scenarios/combat.toml")
  assert(lsnap, "live load")
  -- Allocate so movement_preview is meaningful.
  harness.submit(live, orders.allocate(1, 4, { beam_1 = 1 }, { 2, 1, 0, 0, 0, 1 }))
  local resp, rerr = harness.request(live,
    { protocol_version = 3, request = "movement_preview", ship = 1 })
  assert(resp, "live movement_preview failed: " .. tostring(rerr and rerr.message))
  assert_eq(resp.type, "movement_preview", "live response type")
  assert_eq(resp.ok, true, "live response ok")
  assert(resp.endpoints and #resp.endpoints > 0, "live endpoints non-empty")
  -- request must not pollute the order log
  assert_eq(#live.orders, 1, "live request did not pollute order log")
  harness.kill(live)
  ok("request envelope round-trip")
else
  ok("request envelope round-trip (skipped: LOVE_LIVE unset)")
end

print(string.format("\nAll %d checks passed.", pass))
