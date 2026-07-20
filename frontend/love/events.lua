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
--   - combat_log: one complete volley generation. Each entry is
--     {attacker, target, weapon, shield, damage, shield_absorbed,
--      hull_damage, kind} where kind is "hit" or "miss".
--   - path_results: per-ship path resolution telemetry; fallbacks become
--     "blocked" events (protocol v4).

local events = {}

local CAP = 50

--- Create a new event ring buffer.
function events.new()
  return {
    buf = {},                   -- array of event tables, oldest first
    seen_combat_generation = nil,
    seen_path_fp = nil,         -- last path_results fingerprint (dedupe)
  }
end

--- Format a combat_log entry into an event. `player_ids` is a set
--- (id -> true) of player-controlled ship ids, used to classify hit_dealt
--- vs hit_taken. Returns the event table. Combat events carry a `meta` table
--- with target_id / hull_damage so the controller (main.lua) can spawn damage
--- floaters and pulses at the right ship without re-parsing the text.
local function combat_event(entry, turn, player_ids)
  local is_player_attacker = player_ids[entry.attacker] == true
  local is_player_target = player_ids[entry.target] == true
  if entry.kind == "miss" then
    return {
      turn = turn,
      kind = "miss",
      text = string.format("%d %s → %d: MISS",
        entry.attacker, entry.weapon, entry.target),
      meta = {
        target_id = entry.target,
        attacker = entry.attacker,
        is_player_attack = is_player_attacker,
        hull_damage = 0,
      },
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
  return {
    turn = turn,
    kind = kind,
    text = text,
    meta = {
      target_id = entry.target,
      attacker = entry.attacker,
      is_player_attack = is_player_attacker,
      hull_damage = hull,
    },
  }
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
--- and scans path_results for fallback/blocks. `player_ids` is a set
--- (id -> true) of player-controlled ship ids for hit_dealt/hit_taken
--- classification.
function events.feed(ev, snap, player_ids)
  if not snap then
    return {}
  end
  player_ids = player_ids or {}
  local turn = snap.turn or 0
  local emitted = {}
  local function emit(event)
    push(ev, event)
    emitted[#emitted + 1] = event
  end

  -- combat_log is replaced atomically for each resolved volley and retained
  -- across auto-advance. Key the whole generation, not its length: two
  -- consecutive volleys may contain the same number (or exact shape) of shots.
  local log = snap.combat_log or {}
  local event_turn = turn
  if snap.status == "InProgress" and #log > 0 then
    event_turn = math.max(1, turn - 1)
  end
  local parts = { tostring(event_turn) }
  for _, entry in ipairs(log) do
    parts[#parts + 1] = table.concat({
      tostring(entry.attacker), tostring(entry.target), tostring(entry.weapon),
      tostring(entry.shield), tostring(entry.damage),
      tostring(entry.shield_absorbed), tostring(entry.hull_damage),
      tostring(entry.kind),
    }, ":")
  end
  local generation = #log > 0 and table.concat(parts, "|") or nil
  if generation and generation ~= ev.seen_combat_generation then
    ev.seen_combat_generation = generation
    for _, entry in ipairs(log) do
      emit(combat_event(entry, event_turn, player_ids))
    end
  end

  -- path_results (v4): fallback / contested endpoints become "blocked" events.
  -- Dedupe by fingerprint so the same path_results snapshot does not spam.
  local prs = snap.path_results
  if type(prs) == "table" and #prs > 0 then
    local fp = {}
    for _, pr in ipairs(prs) do
      fp[#fp + 1] = string.format(
        "%s:%s:%s:%s",
        tostring(pr.ship),
        tostring(pr.fallback_steps or 0),
        tostring(pr.blocked_kind or ""),
        tostring(pr.translated_steps or 0)
      )
    end
    local key = tostring(turn) .. "|" .. table.concat(fp, "|")
    if key ~= "" and key ~= ev.seen_path_fp then
      ev.seen_path_fp = key
      for _, pr in ipairs(prs) do
        if (pr.fallback_steps or 0) > 0 or pr.blocked_kind then
          emit({
            kind = "blocked",
            turn = turn,
            ship = pr.ship,
            blocked_kind = pr.blocked_kind or "fallback",
            fallback_steps = pr.fallback_steps or 0,
            translated_steps = pr.translated_steps or 0,
            final_q = pr.final_q,
            final_r = pr.final_r,
            conflicting = pr.conflicting_ships,
            text = string.format(
              "ship %s path fallback x%d (%s) → (%s,%s)",
              tostring(pr.ship),
              pr.fallback_steps or 0,
              pr.blocked_kind or "fallback",
              tostring(pr.final_q),
              tostring(pr.final_r)
            ),
          })
        end
      end
    end
  else
    -- The engine clears path_results between resolutions. Allow an identical
    -- fallback in a later turn to become a new event.
    ev.seen_path_fp = nil
  end
  return emitted
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

--- Compute game-over summary stats from the event history (UPGRADE-PLAN
--- Phase 5). Mirrors the TUI's render_game_over_summary: shots, hits,
--- internal damage dealt/taken — all from structured events, never log
--- string parsing. Pure function (no Love APIs).
---
--- Returns: { shots, hits, int_dealt, int_taken }
---   shots     — player shots fired (hit_dealt + player misses)
---   hits      — player shots that hit (hit_dealt count)
---   int_dealt — hull damage dealt by player (sum of hit_dealt hull_damage)
---   int_taken — hull damage taken by player (sum of hit_taken hull_damage)
function events.stats(ev)
  local shots, hits, int_dealt, int_taken = 0, 0, 0, 0
  for _, e in ipairs(ev.buf) do
    if e.kind == "hit_dealt" then
      shots = shots + 1
      hits = hits + 1
      int_dealt = int_dealt + (e.meta and e.meta.hull_damage or 0)
    elseif e.kind == "hit_taken" then
      int_taken = int_taken + (e.meta and e.meta.hull_damage or 0)
    elseif e.kind == "miss" then
      -- Only count player misses as shots; enemy misses are not player shots.
      if e.meta and e.meta.is_player_attack then
        shots = shots + 1
      end
    end
  end
  return { shots = shots, hits = hits, int_dealt = int_dealt, int_taken = int_taken }
end

return events
