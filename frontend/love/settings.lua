-- Persist UI settings under frontend/love/local/settings.json (FIX-PLAN F3).
-- Pure file I/O when paths/json available; no love.* required for load/save.

local settings = {}

local DEFAULTS = {
  ui_scale = nil, -- nil = use DPI default
  auto_follow = true,
}

function settings.defaults()
  return {
    ui_scale = DEFAULTS.ui_scale,
    auto_follow = DEFAULTS.auto_follow,
  }
end

function settings.load(path, json_mod)
  local f = io.open(path, "r")
  if not f then
    return settings.defaults()
  end
  local raw = f:read("*a")
  f:close()
  if not raw or raw == "" then
    return settings.defaults()
  end
  local ok, data = pcall(function()
    return json_mod.decode(raw)
  end)
  if not ok or type(data) ~= "table" then
    return settings.defaults()
  end
  local out = settings.defaults()
  if type(data.ui_scale) == "number" then
    out.ui_scale = data.ui_scale
  end
  if type(data.auto_follow) == "boolean" then
    out.auto_follow = data.auto_follow
  end
  return out
end

function settings.save(path, data, json_mod)
  local dir = path:match("(.+)/[^/]+$")
  if dir then
    os.execute("mkdir -p '" .. dir:gsub("'", "'\\''") .. "'")
  end
  local f = io.open(path, "w")
  if not f then
    return false
  end
  f:write(json_mod.encode({
    ui_scale = data.ui_scale,
    auto_follow = data.auto_follow,
  }))
  f:write("\n")
  f:close()
  return true
end

return settings
