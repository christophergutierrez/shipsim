-- Component shipyard (Love + luajit). Pure Lua: no love.* APIs.
-- Authoring writes design TOML and shells out to shipsim-yard (not a protocol
-- request). Play writes a scratch scenario and reloads the engine.

local shipyard = {}

shipyard.MOUNTS = {
  "forward",
  "forward_starboard",
  "aft_starboard",
  "aft",
  "aft_port",
  "forward_port",
}

shipyard.ENGINE_KINDS = { "fission", "fusion", "antimatter" }
shipyard.ENGINE_SIZES = { "s", "m", "l", "h" }

shipyard.MATERIALS = {
  "titanium",
  "duralloy",
  "zortrium",
  "andrium",
  "tritanium",
  "adamantium",
  "neutronium",
}

shipyard.WEAPON_SKUS = {
  "beam",
  "beam_compact",
  "beam_potent",
  "beam_precise",
  "torpedo",
  "torpedo_compact",
  "torpedo_potent",
  "torpedo_precise",
  "plasma",
  "plasma_compact",
  "plasma_potent",
  "plasma_precise",
}

local function quote(s)
  return "'" .. tostring(s):gsub("'", "'\\''") .. "'"
end

local function file_exists(path)
  local f = io.open(path, "r")
  if f then
    f:close()
    return true
  end
  return false
end

function shipyard.new_design()
  return {
    id = "yard_custom",
    name = "Yard Custom",
    size = 2,
    material = "titanium",
    engine = "fission",
    engine_size = "m",
    armored = true,
    shields = { 6, 4, 2, 2, 2, 4 },
    weapons = { { component = "beam", mount = "forward" } },
  }
end

function shipyard.to_toml(design)
  local lines = {
    string.format("id = %q", design.id),
    string.format("name = %q", design.name),
    string.format("size = %d", tonumber(design.size) or 2),
    string.format("material = %q", design.material or "titanium"),
    string.format("engine = %q", design.engine or "fission"),
    string.format("engine_size = %q", design.engine_size or "m"),
    string.format("armored = %s", design.armored and "true" or "false"),
    string.format(
      "shields = [%s]",
      table.concat(design.shields or { 6, 4, 2, 2, 2, 4 }, ", ")
    ),
  }
  for _, weapon in ipairs(design.weapons or {}) do
    lines[#lines + 1] = ""
    lines[#lines + 1] = "[[weapons]]"
    lines[#lines + 1] = string.format("component = %q", weapon.component or "beam")
    lines[#lines + 1] = string.format("mount = %q", weapon.mount or "forward")
  end
  return table.concat(lines, "\n") .. "\n"
end

function shipyard.parse_design(text)
  local design = shipyard.new_design()
  design.weapons = {}
  local current
  for raw in (tostring(text) .. "\n"):gmatch("(.-)\n") do
    local line = raw:gsub("%s*#.*", ""):gsub("^%s+", ""):gsub("%s+$", "")
    if line == "[[weapons]]" then
      current = { component = "beam", mount = "forward" }
      design.weapons[#design.weapons + 1] = current
    elseif line:match("^id%s*=") then
      design.id = line:match("=%s*\"([^\"]+)\"") or design.id
    elseif line:match("^name%s*=") then
      design.name = line:match("=%s*\"([^\"]+)\"") or design.name
    elseif line:match("^size%s*=") then
      design.size = tonumber(line:match("=%s*(%d+)")) or design.size
    elseif line:match("^material%s*=") then
      design.material = line:match("=%s*\"([^\"]+)\"") or design.material
    elseif line:match("^engine_size%s*=") then
      design.engine_size = line:match("=%s*\"([^\"]+)\"") or design.engine_size
    elseif line:match("^engine%s*=") then
      design.engine = line:match("=%s*\"([^\"]+)\"") or design.engine
    elseif line:match("^armored%s*=") then
      design.armored = line:match("=%s*(%w+)") == "true"
    elseif line:match("^shields%s*=") then
      local inner = line:match("%[([^%]]*)%]") or ""
      local faces = {}
      for n in inner:gmatch("(%d+)") do
        faces[#faces + 1] = tonumber(n) or 0
      end
      while #faces < 6 do
        faces[#faces + 1] = 0
      end
      design.shields = faces
    elseif current and line:match("^component%s*=") then
      current.component = line:match("=%s*\"([^\"]+)\"") or current.component
    elseif current and line:match("^mount%s*=") then
      current.mount = line:match("=%s*\"([^\"]+)\"") or current.mount
    end
  end
  if #design.weapons == 0 then
    design.weapons = { { component = "beam", mount = "forward" } }
  end
  return design
end

function shipyard.read_design(path)
  local f = io.open(path, "r")
  if not f then
    return nil, "cannot read " .. tostring(path)
  end
  local text = f:read("*a")
  f:close()
  return shipyard.parse_design(text)
end

function shipyard.write_file(path, text)
  local f, err = io.open(path, "w")
  if not f then
    return nil, err or ("cannot write " .. tostring(path))
  end
  f:write(text)
  f:close()
  return true
end

function shipyard.design_path(repo, design)
  return (repo or ".") .. "/data/designs/" .. design.id .. ".toml"
end

function shipyard.play_scenario_toml(class_id)
  return table.concat({
    "width = 24",
    "height = 20",
    "seed = 1",
    "",
    "[terminal]",
    'type = "destruction"',
    "target = 2",
    "",
    "[[ships]]",
    "id = 1",
    string.format("class = %q", class_id),
    "q = 4",
    "r = 10",
    "facing = 0",
    'controller = "player"',
    "",
    "[[ships]]",
    "id = 2",
    string.format("class = %q", class_id),
    "q = 19",
    "r = 10",
    "facing = 3",
    'controller = "ai"',
    "",
  }, "\n")
end

function shipyard.find_yard_bin(repo)
  local root = repo or "."
  for _, rel in ipairs({ "/target/debug/shipsim-yard", "/target/release/shipsim-yard" }) do
    local path = root .. rel
    if file_exists(path) then
      return path
    end
  end
  return nil
end

--- Run shipsim-yard. Returns ok, stdout+stderr text.
function shipyard.run_yard(repo, command, design_rel)
  local bin = shipyard.find_yard_bin(repo)
  if not bin then
    return false, "shipsim-yard not found; run cargo build"
  end
  local cmd = string.format(
    "cd %s && %s %s %s 2>&1",
    quote(repo),
    quote(bin),
    quote(command),
    quote(design_rel)
  )
  local pipe = io.popen(cmd)
  if not pipe then
    return false, "failed to start shipsim-yard"
  end
  local out = pipe:read("*a") or ""
  local ok = pipe:close()
  return ok == true, out:gsub("%s+$", "")
end

function shipyard.cycle(list, current, delta)
  local idx = 1
  for i, value in ipairs(list) do
    if value == current then
      idx = i
      break
    end
  end
  idx = ((idx - 1 + (delta or 1)) % #list) + 1
  return list[idx]
end

function shipyard.nudge(value, delta, minv, maxv)
  local nextv = math.floor(tonumber(value) or 0) + (delta or 0)
  if minv and nextv < minv then
    nextv = minv
  end
  if maxv and nextv > maxv then
    nextv = maxv
  end
  return nextv
end

return shipyard
