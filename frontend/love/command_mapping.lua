-- Protocol-v4 path mapping. Every action costs one allocated motion point.
local commands = {}
local orders = require("orders")

local function turn_actions(from, to)
  local right = (to - from) % 6
  local left = (from - to) % 6
  local action = right <= left and "turn_right" or "turn_left"
  local count = math.min(right, left)
  local actions = {}
  for _ = 1, count do actions[#actions + 1] = action end
  return actions
end

--- Map legacy button labels to one complete protocol-v4 path.
function commands.movement_order(action, ship_id, facing, current_facing)
  if action == "coast" then
    return orders.commit_path(ship_id, {})
  elseif action == "accel" then
    return orders.commit_path(ship_id, { "move_f" })
  elseif action == "turn" then
    if not facing then return nil end
    return orders.commit_path(ship_id, turn_actions(current_facing or 0, facing))
  elseif action == "turn_accel" then
    if not facing then return nil end
    local actions = turn_actions(current_facing or 0, facing)
    actions[#actions + 1] = "move_f"
    return orders.commit_path(ship_id, actions)
  end
  return nil
end

return commands
