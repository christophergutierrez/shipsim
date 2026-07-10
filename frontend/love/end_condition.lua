-- Win from core status; defeat derived from player ships (ADR-0018).

local end_condition = {}

function end_condition.player_ships(snapshot)
  local out = {}
  if not snapshot or not snapshot.ships then
    return out
  end
  for _, s in ipairs(snapshot.ships) do
    if s.controller == "player" then
      out[#out + 1] = s
    end
  end
  return out
end

--- Returns "won", "defeat", or nil if still playing.
function end_condition.evaluate(snapshot)
  if not snapshot then
    return nil
  end
  if snapshot.status == "Won" then
    return "won"
  end
  local players = end_condition.player_ships(snapshot)
  if #players == 0 then
    return nil
  end
  local any_alive = false
  for _, s in ipairs(players) do
    if not s.destroyed then
      any_alive = true
      break
    end
  end
  if not any_alive then
    return "defeat"
  end
  return nil
end

return end_condition
