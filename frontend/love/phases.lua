-- v2 phase machine (Combat Model v2, ADR-0020).
-- Matches the core Phase enum (serde snake_case): allocate, movement, firing, turn_end.

local phases = {}

phases.ALLOCATE = "allocate"
phases.MOVEMENT = "movement"
phases.FIRING = "firing"
phases.TURN_END = "turn_end"

phases.ORDER = { phases.ALLOCATE, phases.MOVEMENT, phases.FIRING, phases.TURN_END }

function phases.next(p)
  for i, name in ipairs(phases.ORDER) do
    if name == p then
      return phases.ORDER[i + 1] or phases.ALLOCATE
    end
  end
  return phases.ALLOCATE
end

--- Which orders are legal in a phase (UI gating; core is authoritative).
function phases.allows(p, action)
  if action == "allocate" then
    return p == phases.ALLOCATE
  elseif action == "commit_maneuver" then
    return p == phases.MOVEMENT
  elseif action == "commit_fire" or action == "ready_fire" then
    return p == phases.FIRING
  elseif action == "end_turn" then
    return true
  end
  return false
end

return phases
