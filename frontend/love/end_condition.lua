-- Battle outcome from the core status.

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
  if snapshot.status == "Lost" then
    return "defeat"
  end
  return nil
end

return end_condition
