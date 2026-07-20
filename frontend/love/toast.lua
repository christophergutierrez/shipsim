-- Phase-change toast banner (FIX-PLAN F4). Pure state machine — no love.*.

local toast = {}

local DURATION = 0.6

function toast.new()
  return { text = nil, t = 0, duration = DURATION }
end

function toast.show(sys, text)
  if not sys then
    return
  end
  sys.text = text
  sys.t = 0
  sys.duration = DURATION
end

function toast.update(sys, dt)
  if not sys or not sys.text then
    return
  end
  sys.t = (sys.t or 0) + (dt or 0)
  if sys.t >= (sys.duration or DURATION) then
    sys.text = nil
    sys.t = 0
  end
end

function toast.active(sys)
  return sys and sys.text and sys.text ~= ""
end

function toast.alpha(sys)
  if not toast.active(sys) then
    return 0
  end
  local d = sys.duration or DURATION
  local t = sys.t or 0
  if t < 0.1 then
    return t / 0.1
  end
  if t > d - 0.15 then
    return math.max(0, (d - t) / 0.15)
  end
  return 1
end

--- Human phase banner from snapshot.
function toast.phase_label(snap, prev_phase)
  if not snap then
    return nil
  end
  local phase = snap.phase
  if phase == "movement" then
    return "Plan paths"
  elseif phase == "firing" then
    return "Firing"
  elseif phase == "allocate" then
    if prev_phase and prev_phase ~= "allocate" then
      return "Allocate power"
    end
    return "Allocate power"
  end
  return phase
end

return toast
