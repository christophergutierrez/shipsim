-- Love-backed owner for the shared ship-art presentation lifecycle.
--
-- Both board and HUD ask this module for presentation decisions. Neither
-- renderer reads manifests, authoring catalogs, cache state, or another
-- renderer's globals. The generated manifest is the sole runtime authority.

local ship_art = require("ship_art")

local ship_art_runtime = {}

local presenter = nil
local attempted = false

local function initialize()
  if attempted then return presenter end
  attempted = true
  if not love or not love.filesystem then return nil end

  local ok, payload = pcall(
    love.filesystem.read,
    "assets/ship_art/manifest.json"
  )
  if not ok or not payload or payload == "" then return nil end

  local json = require("json")
  local decoded, manifest = pcall(json.decode, payload)
  if not decoded or not manifest then return nil end

  presenter = ship_art.new_presenter(manifest, function(path)
    if not love.graphics or not love.graphics.newImage then return nil end
    local loaded, image = pcall(love.graphics.newImage, path)
    if loaded then return image end
    return nil
  end)
  return presenter
end

function ship_art_runtime.board_decision(ship, marker_radius)
  local current = initialize()
  if not current then
    return { fallback = true, state = ship and ship.destroyed and "destroyed" or "top_down" }
  end
  return current:board_decision(ship, marker_radius)
end

function ship_art_runtime.portrait_decision(class_id, max_pixels)
  local current = initialize()
  if not current then
    return { fallback = true, class_id = class_id, state = "portrait" }
  end
  return current:portrait_decision(class_id, max_pixels)
end

return ship_art_runtime
