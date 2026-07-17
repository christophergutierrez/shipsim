-- v3 command mapping. Builds all Maneuver variants (src/motion.rs).
local commands = {}
local orders = require("orders")

--- Map a movement action to an order for the given ship.
--- action: "coast" | "accel" | "turn" | "turn_accel"
--- facing: 0..5 (required for "turn" and "turn_accel")
function commands.movement_order(action, ship_id, facing)
  if action == "coast" then
    return orders.coast(ship_id)
  elseif action == "accel" then
    return orders.accel(ship_id)
  elseif action == "turn" then
    if not facing then return nil end
    return orders.turn(ship_id, facing)
  elseif action == "turn_accel" then
    if not facing then return nil end
    return orders.turn_accel(ship_id, facing)
  end
  return nil
end

return commands
