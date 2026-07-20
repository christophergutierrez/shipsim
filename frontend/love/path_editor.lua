-- Protocol-v4 path draft controller (client-only).
--
-- Pure Lua: no love.* APIs. Owns draft mutation and commit *intent* only.
-- The engine remains authoritative for legality and cost via path_preview.
--
-- Production main.lua and headless tests must use this same seam so keyboard
-- and button coverage cannot diverge from the submitted NDJSON order.

local path_editor = {}
local orders = require("orders")

path_editor.ACTIONS = {
  move_f = true,
  move_fr = true,
  move_fl = true,
  turn_left = true,
  turn_right = true,
}

--- Motion points the ship may still spend (snapshot UX cap only).
function path_editor.motion_cap(ship)
  if not ship then
    return 0
  end
  local avail = ship.motion_available or 0
  local cap = ship.max_maneuver_actions
  if cap and cap < avail then
    avail = cap
  end
  return avail
end

function path_editor.get(drafts, ship_id)
  drafts = drafts or {}
  local d = drafts[ship_id]
  if not d then
    return {}
  end
  local copy = {}
  for i, a in ipairs(d) do
    copy[i] = a
  end
  return copy
end

local function ensure(drafts, ship_id)
  if not drafts[ship_id] then
    drafts[ship_id] = {}
  end
  return drafts[ship_id]
end

--- Append one legal action. Returns ok, reason, draft_copy.
function path_editor.append(drafts, ship_id, action, cap)
  if not path_editor.ACTIONS[action] then
    return false, "unknown_action", path_editor.get(drafts, ship_id)
  end
  cap = cap or 0
  local draft = ensure(drafts, ship_id)
  if #draft >= cap then
    return false, "budget_full", path_editor.get(drafts, ship_id)
  end
  draft[#draft + 1] = action
  return true, nil, path_editor.get(drafts, ship_id)
end

function path_editor.undo(drafts, ship_id)
  local draft = drafts[ship_id]
  if not draft or #draft == 0 then
    return false, "empty", path_editor.get(drafts, ship_id)
  end
  table.remove(draft)
  return true, nil, path_editor.get(drafts, ship_id)
end

function path_editor.clear(drafts, ship_id)
  drafts[ship_id] = {}
  return true, nil, {}
end

--- Normal Commit Path: requires at least one action.
--- Returns kind ("submit"|"blocked"), actions_or_nil, reason.
function path_editor.try_commit(drafts, ship_id)
  local actions = path_editor.get(drafts, ship_id)
  if #actions == 0 then
    return "blocked", nil, "empty_draft"
  end
  return "submit", actions, nil
end

--- Explicit Hold Position: always submits empty actions array.
function path_editor.hold(drafts, ship_id)
  local _ = drafts
  local _id = ship_id
  return "submit", {}, nil
end

--- Clear draft after engine accepts the order.
function path_editor.accept(drafts, ship_id)
  drafts[ship_id] = nil
end

--- Build NDJSON order for a successful commit intent.
function path_editor.order(ship_id, actions)
  return orders.commit_path(ship_id, actions)
end

--- Read-only path_preview request body (protocol v4).
function path_editor.preview_request(ship_id, actions)
  return {
    protocol_version = 4,
    request = "path_preview",
    ship = ship_id,
    actions = actions or {},
  }
end

--- Keyboard / button dispatch → action token (nil if not a path control).
function path_editor.key_to_action(key)
  if key == "w" then
    return "move_f"
  elseif key == "a" then
    return "move_fl"
  elseif key == "d" then
    return "move_fr"
  elseif key == "z" then
    return "turn_left"
  elseif key == "x" then
    return "turn_right"
  end
  return nil
end

return path_editor
