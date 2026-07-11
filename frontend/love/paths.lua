-- Repo root, this client root, and shipsim binary discovery.
-- Isolation: all Love scratch lives under frontend/love/local/ (see frontend/README.md).

local paths = {}

local function file_exists(p)
  local f = io.open(p, "r")
  if f then
    f:close()
    return true
  end
  return false
end

local function shell_quote(s)
  return "'" .. tostring(s):gsub("'", "'\\''") .. "'"
end

--- Absolute-ish path to this Love client tree (frontend/love).
function paths.client_root()
  if love and love.filesystem and love.filesystem.getSource then
    local src = love.filesystem.getSource()
    if src and src ~= "" then
      return src
    end
  end
  -- When required from luajit tests: directory containing this module.
  local info = debug.getinfo(1, "S").source
  if info and info:sub(1, 1) == "@" then
    local dir = info:sub(2):match("(.+)[/\\]")
    if dir and dir ~= "" then
      return dir
    end
  end
  return "."
end

--- Session scratch directory under this client only (created on demand).
function paths.local_dir()
  local dir = paths.client_root() .. "/local"
  os.execute("mkdir -p " .. shell_quote(dir))
  return dir
end

function paths.default_orders_path()
  return paths.local_dir() .. "/orders.jsonl"
end

function paths.default_stderr_path()
  return paths.local_dir() .. "/shipsim_stderr.txt"
end

--- Repo root: directory containing Cargo.toml and scenarios/.
function paths.find_repo_root()
  local env = os.getenv("SHIPSIM_ROOT")
  if env and file_exists(env .. "/Cargo.toml") then
    return env
  end
  -- love.filesystem source may be frontend/love; walk up.
  local candidates = {
    paths.client_root() .. "/../..",
    paths.client_root() .. "/..",
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
