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

print("order builders (gate 3)")
local a = orders.allocate(1, 4, { beam_1 = 1 }, { 0, 0, 0, 0, 0, 0 })
assert_eq(a.type, "allocate", "allocate type")
assert_eq(a.protocol_version, 1, "allocate protocol version")
assert_eq(a.ship, 1, "allocate ship")
assert_eq(a.movement, 4, "allocate movement")
assert_eq(a.weapons.beam_1, 1, "allocate weapons")
assert_eq(#a.shields, 6, "allocate shields len")

local m = orders.move(1, "forward")
assert_eq(m.type, "move", "move type")
assert_eq(m.mode, "forward", "move mode")

local pm = orders.pass_move(1)
assert_eq(pm.type, "pass_move", "pass_move type")

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

local _, version_errors = harness.parse_stream('{"protocol_version":2,"turn":1,"ships":[]}')
assert_eq(version_errors[1].code, "unsupported_protocol", "client rejects protocol version")

print("phases")
assert_eq(phases.ALLOCATE, "allocate", "phase allocate")
assert_eq(phases.MOVEMENT, "movement", "phase movement")
assert_eq(phases.FIRING, "firing", "phase firing")
assert_eq(phases.TURN_END, "turn_end", "phase turn_end")
assert_eq(phases.next(phases.ALLOCATE), phases.MOVEMENT, "phase next")
assert_eq(phases.allows(phases.ALLOCATE, "allocate"), true, "allows allocate")
assert_eq(phases.allows(phases.MOVEMENT, "move"), true, "allows move")
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
  orders_path = repo .. "/tmp/love_test_orders.jsonl",
})
local snap = harness.load_scenario(session, "scenarios/combat.toml")
assert(snap, "load")
assert_eq(snap.turn, 1, "turn")
assert_eq(snap.phase, "allocate", "phase")
ok("load")

-- Allocate both ships, then pass-move ship 1.
snap = select(1, harness.submit(session, orders.allocate(1, 4, { beam_1 = 1 }, { 2, 1, 0, 0, 0, 1 })))
assert(snap, "allocate ship 1")
assert_eq(snap.ships[1].shields_powered[1], 2, "front shield allocated")
assert_eq(snap.ships[1].shields_powered[2], 1, "front-right shield allocated")
snap = select(1, harness.submit(session, orders.allocate(2, 2, { beam_1 = 1 }, { 0, 0, 0, 0, 0, 0 })))
assert(snap, "allocate ship 2")
assert_eq(snap.phase, "movement", "phase after allocate")
ok("allocate + move phase")

snap = select(1, harness.submit(session, orders.pass_move(1)))
assert(snap, "pass_move ship 1")
ok("move order")

print(string.format("\nAll %d checks passed.", pass))
