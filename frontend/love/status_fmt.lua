-- Status strip formatters and lifecycle (FIX-PLAN F2).
-- Pure Lua — no love.* APIs.

local status_fmt = {}

--- Human wording for a successful order echo (protocol v4).
function status_fmt.order_echo(ship, action, facing)
  local id = tostring(ship or "?")
  if action == "turn" then
    return string.format("Ship #%s turned to facing %s", id, tostring(facing))
  end
  if action == "allocate" then
    return string.format("Ship #%s allocated", id)
  end
  if action == "commit_path" then
    return string.format("Ship #%s committed path", id)
  end
  if action == "hold_position" then
    return string.format("Ship #%s held position", id)
  end
  if action == "commit_volley" then
    return string.format("Ship #%s committed volley", id)
  end
  if action == "ready" or action == "ready_fire" then
    return string.format("Ship #%s committed volley", id)
  end
  if action == "fire" or action == "commit_fire" then
    return string.format("Ship #%s queued fire", id)
  end
  return string.format("Ship #%s %s", id, tostring(action or "acted"))
end

--- Attach birth turn+phase so the strip can expire on phase change.
function status_fmt.stamp(st, snap)
  if not st then
    return
  end
  st.born_turn = snap and snap.turn or nil
  st.born_phase = snap and snap.phase or nil
end

--- Clear strip when phase/turn no longer matches birth stamp.
function status_fmt.clear_if_stale(st, snap)
  if not st or not st.message or st.message == "" then
    return false
  end
  if not snap then
    return false
  end
  if st.born_turn ~= nil and st.born_turn ~= snap.turn then
    st.message = ""
    st.level = "info"
    st.born_turn = nil
    st.born_phase = nil
    return true
  end
  if st.born_phase ~= nil and st.born_phase ~= snap.phase then
    st.message = ""
    st.level = "info"
    st.born_turn = nil
    st.born_phase = nil
    return true
  end
  return false
end

--- Header never prints raw nils.
function status_fmt.header_active(active_id, callsign_fn)
  if active_id == nil then
    return ""
  end
  if callsign_fn then
    return "  Active " .. tostring(callsign_fn(active_id))
  end
  return string.format("  Active #%s", tostring(active_id))
end

return status_fmt
