-- M6 production command mapping. Directional maneuver controls are deferred to M8.
local commands = {}
local orders = require("orders")

function commands.movement_order(action, ship_id)
  if action == "coast" then
    return orders.coast(ship_id)
  end
  return nil
end

return commands
