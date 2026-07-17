-- Run shipsim via orders-file replay (no long-lived stdin pipe).

local json = require("json")
local paths = require("paths")

local harness = {}

local function shell_quote(s)
  return "'" .. tostring(s):gsub("'", "'\\''") .. "'"
end

function harness.new(opts)
  opts = opts or {}
  local repo = opts.repo_root or paths.find_repo_root()
  local bin = opts.bin or paths.find_shipsim_bin(repo)
  return {
    repo_root = repo,
    bin = bin,
    scenario = opts.scenario, -- relative to repo, e.g. scenarios/combat.toml
    orders = {}, -- successful order tables
    snapshot = nil,
    last_error = nil,
    -- Scratch stays under frontend/love/local/ (never repo /tmp or system /tmp).
    orders_path = opts.orders_path or paths.default_orders_path(),
    stderr_path = opts.stderr_path or paths.default_stderr_path(),
  }
end

function harness.parse_stream(text)
  local snapshots = {}
  local errors = {}
  for line in (text or ""):gmatch("[^\r\n]+") do
    if line:match("%S") then
      local ok, obj = pcall(json.decode, line)
      if ok and type(obj) == "table" then
        -- Classify by the `type` field, matching the TUI (harness.rs).
        -- The engine enforces protocol_version on orders; the client must not
        -- version-gate snapshots (they carry the engine's current version).
        if obj.type == "error" then
          errors[#errors + 1] = obj
        else
          snapshots[#snapshots + 1] = obj
        end
      end
    end
  end
  return snapshots, errors
end

function harness.write_orders(session)
  local f = assert(io.open(session.orders_path, "w"))
  for _, o in ipairs(session.orders) do
    f:write(json.encode(o))
    f:write("\n")
  end
  f:close()
end

local function absolutize(path)
  if not path or path:sub(1, 1) == "/" then
    return path
  end
  -- Resolve relative paths against the current process cwd *before* the
  -- harness `cd`s into the repo root (Love local/ scratch is not under root).
  local p = io.popen("pwd")
  local cwd = p and p:read("*l") or "."
  if p then p:close() end
  return cwd .. "/" .. path
end

function harness.run(session)
  assert(session.scenario, "scenario required")
  session.orders_path = absolutize(session.orders_path or paths.default_orders_path())
  session.bin = absolutize(session.bin)
  session.repo_root = absolutize(session.repo_root)
  harness.write_orders(session)
  local stderr_path = absolutize(session.stderr_path or paths.default_stderr_path())
  session.stderr_path = stderr_path
  local cmd = string.format(
    "cd %s && %s --scenario %s --orders %s 2>%s",
    shell_quote(session.repo_root),
    shell_quote(session.bin),
    shell_quote(session.scenario),
    shell_quote(session.orders_path),
    shell_quote(stderr_path)
  )
  local p = io.popen(cmd, "r")
  if not p then
    session.last_error = {
      type = "error",
      ok = false,
      code = "spawn_failed",
      message = "failed to spawn shipsim",
      source = "client",
    }
    return nil, session.last_error
  end
  local out = p:read("*a") or ""
  local ok_close, _, code = p:close()
  local snapshots, errors = harness.parse_stream(out)
  if #snapshots > 0 then
    session.snapshot = snapshots[#snapshots]
  end
  if errors[#errors] then
    session.last_error = errors[#errors]
  else
    session.last_error = nil
  end
  if not ok_close and #snapshots == 0 then
    local errf = io.open(stderr_path, "r")
    local stderr = errf and errf:read("*a") or ""
    if errf then
      errf:close()
    end
    session.last_error = {
      type = "error",
      ok = false,
      code = "spawn_failed",
      message = "shipsim failed: " .. tostring(code) .. " " .. stderr,
      source = "client",
    }
    return nil, session.last_error
  end
  return session.snapshot, session.last_error
end

function harness.load_scenario(session, scenario_rel)
  session.scenario = scenario_rel
  session.orders = {}
  session.last_error = nil
  return harness.run(session)
end

--- Append order if accepted; on order_illegal for last line, drop it.
function harness.submit(session, order)
  session.orders[#session.orders + 1] = order
  local snap, err = harness.run(session)
  if err and (err.code == "order_illegal" or err.code == "parse_error") then
    -- Drop failed order so replay stays clean.
    table.remove(session.orders)
    harness.run(session)
    return session.snapshot, err
  end
  return snap, err
end

return harness
