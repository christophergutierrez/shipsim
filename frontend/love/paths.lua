-- Repo root and shipsim binary discovery.

local paths = {}

local function file_exists(p)
  local f = io.open(p, "r")
  if f then
    f:close()
    return true
  end
  return false
end

--- Repo root: directory containing Cargo.toml and scenarios/.
function paths.find_repo_root()
  local env = os.getenv("SHIPSIM_ROOT")
  if env and file_exists(env .. "/Cargo.toml") then
    return env
  end
  -- love.filesystem source may be frontend/love; walk up.
  local candidates = {
    ".",
    "..",
    "../..",
    "../../..",
  }
  if love and love.filesystem then
    local src = love.filesystem.getSource()
    if src then
      table.insert(candidates, 1, src .. "/../..")
      table.insert(candidates, 1, src .. "/..")
      table.insert(candidates, 1, src)
    end
  end
  for _, c in ipairs(candidates) do
    if file_exists(c .. "/Cargo.toml") and file_exists(c .. "/scenarios") then
      return c
    end
  end
  return "."
end

function paths.find_shipsim_bin(repo_root)
  local env = os.getenv("SHIPSIM_BIN")
  if env and file_exists(env) then
    return env
  end
  local root = repo_root or paths.find_repo_root()
  local candidates = {
    root .. "/target/debug/shipsim",
    root .. "/target/release/shipsim",
  }
  for _, c in ipairs(candidates) do
    if file_exists(c) then
      return c
    end
  end
  return "shipsim"
end

function paths.list_scenarios(repo_root)
  local root = repo_root or paths.find_repo_root()
  local dir = root .. "/scenarios"
  local list = {}
  local p = io.popen('ls -1 "' .. dir .. '"/*.toml 2>/dev/null')
  if not p then
    return list
  end
  for line in p:lines() do
    local name = line:match("([^/]+)%.toml$")
    if name then
      list[#list + 1] = { name = name, path = "scenarios/" .. name .. ".toml", abs = line }
    end
  end
  p:close()
  table.sort(list, function(a, b)
    return a.name < b.name
  end)
  return list
end

return paths
