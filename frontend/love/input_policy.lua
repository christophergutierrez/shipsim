-- Pure input classification shared by Love callbacks and headless tests.

local layout = require("layout")
local phases = require("phases")

local input_policy = {}

function input_policy.sidebar_scroll_command(key)
  if key == "pageup" then return "page_up" end
  if key == "pagedown" then return "page_down" end
  if key == "home" then return "top" end
  if key == "end" then return "bottom" end
  return nil
end

function input_policy.fire_weapon_delta(phase, key)
  if phase ~= phases.FIRING then return nil end
  if key == "up" then return -1 end
  if key == "down" then return 1 end
  return nil
end

function input_policy.wheel_owner(screen, x, y, picker_list, sidebar_panel)
  if screen == "picker" and layout.point_in_rect(x, y, picker_list) then
    return "picker"
  end
  if screen == "play" and layout.point_in_rect(x, y, sidebar_panel) then
    return "sidebar"
  end
  return "map"
end

function input_policy.resize_key_allowed(key)
  return key == "q" or key == "escape"
end

return input_policy
