-- Protocol-v4 collection stages: allocate, movement, firing, then auto-advance.

local phases = {}

phases.ALLOCATE = "allocate"
phases.MOVEMENT = "movement"
phases.FIRING = "firing"
phases.ORDER = { phases.ALLOCATE, phases.MOVEMENT, phases.FIRING }

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
  elseif action == "commit_path" then
    return p == phases.MOVEMENT
  elseif action == "commit_volley" then
    return p == phases.FIRING
  end
  return false
end

return phases
