-- Structured event ring buffer built by diffing consecutive snapshots.
--
-- Pure Lua: no love.* APIs. Runs under plain luajit so it is testable
-- headless. The UI layer (main.lua / draw_hud.lua) owns an instance and
-- feeds it each accepted snapshot; events.lua owns the diffing logic.
--
-- Event shape: { turn = <n>, kind = <str>, text = <str> }
--   kind ∈ "hit_dealt", "hit_taken", "miss", "blocked", "info"
--
-- Sources (per docs/PROTOCOL.md additive fields):
--   - combat_log: new entries only (diffed by count). Each entry is
--     {attacker, target, weapon, shield, damage, shield_absorbed,
--      hull_damage, kind} where kind is "hit" or "miss".
--   - translation_results: per-ship {ship, requested, moved, blocked?}.
--     A blocked translation becomes a "blocked" event.

local events = {}

local CAP = 50

--- Create a new event ring buffer.
function events.new()
  return {
    buf = {},            -- array of event tables, oldest first
    seen_turn = nil,     -- last turn seen
    seen_log = 0,        -- count of combat_log entries already consumed on seen_turn
  }
end

--- Format a combat_log entry into an event. `player_ids` is a set
--- (id -> true) of player-controlled ship ids, used to classify hit_dealt
--- vs hit_taken. Returns the event table.
local function combat_event(entry, turn, player_ids)
  local is_player_attacker = player_ids[entry.attacker] == true
  local is_player_target = player_ids[entry.target] == true
  if entry.kind == "miss" then
    return {
      turn = turn,
      kind = "miss",
      text = string.format("%d %s → %d: MISS",
        entry.attacker, entry.weapon, entry.target),
    }
  end
  -- hit
  local kind = "info"
  if is_player_attacker then
    kind = "hit_dealt"
  elseif is_player_target then
    kind = "hit_taken"
  end
  local dmg = entry.damage or 0
  local shield_abs = entry.shield_absorbed or 0
  local hull = entry.hull_damage or 0
  local text
  if shield_abs > 0 and hull == 0 then
    text = string.format("%d %s → %d: shield %d", entry.attacker, entry.weapon,
      entry.target, shield_abs)
  else
    text = string.format("%d %s → %d: -%d", entry.attacker, entry.weapon,
      entry.target, dmg)
  end
  return { turn = turn, kind = kind, text = text }
end

--- Format a blocked translation into an event.
local function blocked_event(tr, turn)
  local moved = tr.moved or 0
  local requested = tr.requested or 0
  local bkind = (tr.blocked and tr.blocked.kind) or "unknown"
  return {
    turn = turn,
    kind = "blocked",
    text = string.format("ship %d moved %d/%d; blocked (%s)",
      tr.ship, moved, requested, bkind),
  }
end

--- Push an event into the ring buffer, evicting the oldest if at capacity.
local function push(ev, event)
  ev.buf[#ev.buf + 1] = event
  if #ev.buf > CAP then
    table.remove(ev.buf, 1)
  end
end

--- Feed a snapshot to the ring buffer. Diffs combat_log (new entries only)
--- and scans translation_results for blocks. `player_ids` is a set
--- (id -> true) of player-controlled ship ids for hit_dealt/hit_taken
--- classification.
function events.feed(ev, snap, player_ids)
  if not snap then
    return
  end
  player_ids = player_ids or {}
  local turn = snap.turn or 0

  -- New combat_log entries only. The engine clears combat_log at the start
  -- of each turn (game_state.rs reset), so the log is cumulative within a
  -- turn but resets across turns. Diff by (turn, count): on a new turn,
  -- emit all entries; on the same turn, emit only the new tail.
  local log = snap.combat_log or {}
  local prev
  if ev.seen_turn == turn then
    prev = ev.seen_log
  else
    prev = 0
  end
  ev.seen_turn = turn
  ev.seen_log = #log
  if #log > prev then
    for i = prev + 1, #log do
      push(ev, combat_event(log[i], turn, player_ids))
    end
  end

  -- translation_results: a block on any ship becomes a "blocked" event.
  local trs = snap.translation_results
  if type(trs) == "table" then
    for _, tr in ipairs(trs) do
      if tr.blocked then
        push(ev, blocked_event(tr, turn))
      end
    end
  end
end

--- Return the last n events (oldest first), or all if n is nil.
function events.recent(ev, n)
  local nbuf = #ev.buf
  if not n or n >= nbuf then
    local copy = {}
    for i, e in ipairs(ev.buf) do copy[i] = e end
    return copy
  end
  local copy = {}
  for i = nbuf - n + 1, nbuf do
    copy[#copy + 1] = ev.buf[i]
  end
  return copy
end

--- Return the count of events currently buffered.
function events.count(ev)
  return #ev.buf
end

return events
