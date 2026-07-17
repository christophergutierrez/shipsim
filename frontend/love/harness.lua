-- Interactive shipsim harness over a long-lived `--stdin` pipe.
--
-- Spawns `shipsim --scenario <path> --stdin` once and keeps the child's
-- stdin/stdout pipes open for the whole session. One NDJSON line in -> one
-- NDJSON line out. This mirrors frontend/tui/src/harness.rs exactly and is
-- the only transport that supports read-only requests (movement_preview,
-- maneuver_options, fire_preview): a batch `--orders` replay process has no
-- way to receive a mid-game request.
--
-- Engine line protocol (empirically verified):
--   - On spawn: emits exactly 1 initial snapshot (the starting state).
--   - Per order: emits exactly 1 snapshot (or error envelope).
--   - Per read-only request: emits exactly 1 typed response envelope; the
--     request does NOT mutate game state, so the next order still applies.
--
-- Public API (preserved from the batch-replay version so main.lua,
-- scripted_pump.lua, and tests/run_all.lua keep working):
--   harness.new(opts)            -> session
--   harness.load_scenario(s, p)  -> snapshot | nil, err   (spawns child)
--   harness.submit(s, order)     -> snapshot | nil, err   (send order, read 1)
--   harness.request(s, tbl)      -> response | nil, err   (read-only; no log)
--   harness.parse_stream(text)   -> snapshots, errors     (line classifier)
--   harness.kill(s)              (teardown)

local json = require("json")
local paths = require("paths")
local subprocess = require("subprocess")

local harness = {}

function harness.new(opts)
  opts = opts or {}
  local repo = opts.repo_root or paths.find_repo_root()
  local bin = opts.bin or paths.find_shipsim_bin(repo)
  return {
    repo_root = repo,
    bin = bin,
    scenario = nil,
    -- Successful order tables (for save/replay). Read-only requests are
    -- NEVER appended here -- see harness.request.
    orders = {},
    snapshot = nil,
    last_error = nil,
    -- Live child process (set by load_scenario).
    proc = nil,
    -- Kept for back-compat with callers that pass them; no longer used for
    -- batch replay, but still surfaced for save/debug paths.
    orders_path = opts.orders_path or paths.default_orders_path(),
    stderr_path = opts.stderr_path or paths.default_stderr_path(),
  }
end

--- Classify NDJSON lines from a blob into snapshots and errors.
--- Kept for headless tests and line classification. A line with
--- type:"error" is an error; anything else with content is a snapshot.
function harness.parse_stream(text)
  local snapshots = {}
  local errors = {}
  for line in (text or ""):gmatch("[^\r\n]+") do
    if line:match("%S") then
      local ok, obj = pcall(json.decode, line)
      if ok and type(obj) == "table" then
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

--- Decode one NDJSON line into a table, or nil + reason.
local function decode_line(line)
  if not line or line == "" then
    return nil, "empty line"
  end
  local ok, obj = pcall(json.decode, line)
  if not ok or type(obj) ~= "table" then
    return nil, "non-JSON line: " .. tostring(line):sub(1, 80)
  end
  return obj
end

--- Classify a decoded line: snapshot, error, or typed response (preview).
--- Mirrors the TUI's EngineLine disambiguation by the `type` field.
local function classify(obj)
  if obj.type == "error" then
    return "error", obj
  end
  if obj.type == "movement_preview"
     or obj.type == "maneuver_options"
     or obj.type == "fire_preview" then
    return "response", obj
  end
  -- No `type` (or unrecognized) -> snapshot.
  return "snapshot", obj
end

local function absolutize(path)
  if not path or path:sub(1, 1) == "/" then
    return path
  end
  local p = io.popen("pwd")
  local cwd = p and p:read("*l") or "."
  if p then p:close() end
  return cwd .. "/" .. path
end

--- Spawn the engine for a scenario and read the initial snapshot.
function harness.load_scenario(session, scenario_rel)
  if session.proc and session.scenario == scenario_rel then
    return session.snapshot, nil
  end
  harness.kill(session)
  session.scenario = scenario_rel
  session.orders = {}
  session.last_error = nil
  session.snapshot = nil

  local bin = absolutize(session.bin)
  local repo = absolutize(session.repo_root)
  -- stderr goes to a scratch file (kept for debug, like the old harness).
  local stderr_path = absolutize(session.stderr_path or paths.default_stderr_path())
  -- chdir into repo root so scenario paths and data/ resolve. We do this by
  -- spawning a shell that cds then execs the binary, so the child's cwd is
  -- the repo root (matches the TUI, which spawns from the repo root).
  local argv = {
    "sh", "-c",
    string.format(
      "cd '%s' && '%s' --scenario '%s' --stdin 2>'%s'",
      repo:gsub("'", "'\\''"),
      bin:gsub("'", "'\\''"),
      session.scenario:gsub("'", "'\\''"),
      stderr_path:gsub("'", "'\\''")
    ),
  }
  local proc, err = subprocess.spawn(argv)
  if not proc then
    session.last_error = {
      type = "error", ok = false, code = "spawn_failed",
      message = "failed to spawn shipsim --stdin: " .. tostring(err),
      source = "client",
    }
    return nil, session.last_error
  end
  session.proc = proc

  -- Read the initial snapshot (1 line on spawn).
  local line = subprocess.read_line(proc)
  if not line then
    session.last_error = {
      type = "error", ok = false, code = "spawn_failed",
      message = "engine closed stdout before initial snapshot",
      source = "client",
    }
    harness.kill(session)
    return nil, session.last_error
  end
  local obj, reason = decode_line(line)
  if not obj then
    session.last_error = {
      type = "error", ok = false, code = "spawn_failed",
      message = "initial snapshot not JSON: " .. reason,
      source = "client",
    }
    harness.kill(session)
    return nil, session.last_error
  end
  local kind, classified = classify(obj)
  if kind == "error" then
    session.last_error = classified
    session.snapshot = nil
    return nil, classified
  end
  -- kind == "snapshot"
  session.snapshot = classified
  session.last_error = nil
  return session.snapshot, nil
end

--- Send an order (one JSON line) and read the engine's response.
--- On success, updates session.snapshot and appends to session.orders.
--- On order_illegal/parse_error, returns the error (order not appended).
function harness.submit(session, order)
  if not session.proc then
    return nil, {
      type = "error", ok = false, code = "not_loaded",
      message = "no engine loaded; call load_scenario first",
      source = "client",
    }
  end
  local json_line = json.encode(order)
  local ok, err = subprocess.write_line(session.proc, json_line)
  if not ok then
    return nil, {
      type = "error", ok = false, code = "write_failed",
      message = err, source = "client",
    }
  end
  local line = subprocess.read_line(session.proc)
  if not line then
    return nil, {
      type = "error", ok = false, code = "engine_closed",
      message = "engine closed stdout after order", source = "client",
    }
  end
  local obj, reason = decode_line(line)
  if not obj then
    return nil, {
      type = "error", ok = false, code = "parse_error",
      message = "non-JSON response: " .. reason, source = "client",
    }
  end
  local kind, classified = classify(obj)
  if kind == "error" then
    session.last_error = classified
    return nil, classified
  end
  -- kind == "snapshot" -- order accepted.
  session.orders[#session.orders + 1] = order
  session.snapshot = classified
  session.last_error = nil
  return session.snapshot, nil
end

--- Send a read-only request (movement_preview / maneuver_options /
--- fire_preview). Returns the typed response envelope. Does NOT mutate game
--- state and is NEVER appended to session.orders. See docs/PROTOCOL.md
--- "Read-only requests".
function harness.request(session, tbl)
  if not session.proc then
    return nil, {
      type = "error", ok = false, code = "not_loaded",
      message = "no engine loaded; call load_scenario first",
      source = "client",
    }
  end
  local json_line = json.encode(tbl)
  local ok, err = subprocess.write_line(session.proc, json_line)
  if not ok then
    return nil, {
      type = "error", ok = false, code = "write_failed",
      message = err, source = "client",
    }
  end
  local line = subprocess.read_line(session.proc)
  if not line then
    return nil, {
      type = "error", ok = false, code = "engine_closed",
      message = "engine closed stdout after request", source = "client",
    }
  end
  local obj, reason = decode_line(line)
  if not obj then
    return nil, {
      type = "error", ok = false, code = "parse_error",
      message = "non-JSON response: " .. reason, source = "client",
    }
  end
  local kind, classified = classify(obj)
  if kind == "error" then
    session.last_error = classified
    return nil, classified
  end
  -- kind == "response" (movement_preview / maneuver_options / fire_preview)
  session.last_error = nil
  return classified, nil
end

--- Tear down the engine subprocess.
function harness.kill(session)
  if session.proc then
    subprocess.kill(session.proc)
    session.proc = nil
  end
end

return harness
