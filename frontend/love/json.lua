-- Minimal JSON encode/decode for shipsim orders/snapshots (pure Lua).
-- Enough for object/array/string/number/bool/null. Not a full RFC suite.

local json = {}

local function skip_ws(s, i)
  while true do
    local c = s:sub(i, i)
    if c ~= " " and c ~= "\t" and c ~= "\n" and c ~= "\r" then
      return i
    end
    i = i + 1
    if i > #s then
      return i
    end
  end
end

local function parse_value(s, i)
  i = skip_ws(s, i)
  local c = s:sub(i, i)
  if c == '"' then
    return parse_string(s, i)
  elseif c == "{" then
    return parse_object(s, i)
  elseif c == "[" then
    return parse_array(s, i)
  elseif c == "t" and s:sub(i, i + 3) == "true" then
    return true, i + 4
  elseif c == "f" and s:sub(i, i + 4) == "false" then
    return false, i + 5
  elseif c == "n" and s:sub(i, i + 3) == "null" then
    return nil, i + 4
  else
    return parse_number(s, i)
  end
end

function parse_string(s, i)
  i = i + 1
  local out = {}
  while i <= #s do
    local c = s:sub(i, i)
    if c == '"' then
      return table.concat(out), i + 1
    elseif c == "\\" then
      local n = s:sub(i + 1, i + 1)
      local map = { ['"'] = '"', ["\\"] = "\\", ["/"] = "/", b = "\b", f = "\f", n = "\n", r = "\r", t = "\t" }
      if n == "u" then
        local hex = s:sub(i + 2, i + 5)
        out[#out + 1] = utf8_char(tonumber(hex, 16) or 0)
        i = i + 6
      else
        out[#out + 1] = map[n] or n
        i = i + 2
      end
    else
      out[#out + 1] = c
      i = i + 1
    end
  end
  error("unterminated string")
end

function utf8_char(code)
  if code < 0x80 then
    return string.char(code)
  elseif code < 0x800 then
    return string.char(0xC0 + math.floor(code / 0x40), 0x80 + code % 0x40)
  else
    return string.char(
      0xE0 + math.floor(code / 0x1000),
      0x80 + math.floor(code / 0x40) % 0x40,
      0x80 + code % 0x40
    )
  end
end

function parse_number(s, i)
  local j = i
  if s:sub(j, j) == "-" then
    j = j + 1
  end
  while s:sub(j, j):match("%d") do
    j = j + 1
  end
  if s:sub(j, j) == "." then
    j = j + 1
    while s:sub(j, j):match("%d") do
      j = j + 1
    end
  end
  if s:sub(j, j):match("[eE]") then
    j = j + 1
    if s:sub(j, j):match("[+-]") then
      j = j + 1
    end
    while s:sub(j, j):match("%d") do
      j = j + 1
    end
  end
  local n = tonumber(s:sub(i, j - 1))
  if not n then
    error("bad number at " .. i)
  end
  return n, j
end

function parse_array(s, i)
  i = i + 1
  local arr = {}
  i = skip_ws(s, i)
  if s:sub(i, i) == "]" then
    return arr, i + 1
  end
  while true do
    local v
    v, i = parse_value(s, i)
    arr[#arr + 1] = v
    i = skip_ws(s, i)
    local c = s:sub(i, i)
    if c == "]" then
      return arr, i + 1
    elseif c == "," then
      i = i + 1
    else
      error("expected , or ] at " .. i)
    end
  end
end

function parse_object(s, i)
  i = i + 1
  local obj = {}
  i = skip_ws(s, i)
  if s:sub(i, i) == "}" then
    return obj, i + 1
  end
  while true do
    i = skip_ws(s, i)
    if s:sub(i, i) ~= '"' then
      error("expected string key at " .. i)
    end
    local key
    key, i = parse_string(s, i)
    i = skip_ws(s, i)
    if s:sub(i, i) ~= ":" then
      error("expected : at " .. i)
    end
    i = i + 1
    local val
    val, i = parse_value(s, i)
    obj[key] = val
    i = skip_ws(s, i)
    local c = s:sub(i, i)
    if c == "}" then
      return obj, i + 1
    elseif c == "," then
      i = i + 1
    else
      error("expected , or } at " .. i)
    end
  end
end

function json.decode(s)
  local v, i = parse_value(s, 1)
  i = skip_ws(s, i)
  if i <= #s then
    error("trailing junk at " .. i)
  end
  return v
end

local function encode_string(str)
  str = tostring(str)
  str = str:gsub("\\", "\\\\")
  str = str:gsub('"', '\\"')
  str = str:gsub("\n", "\\n")
  str = str:gsub("\r", "\\r")
  str = str:gsub("\t", "\\t")
  return '"' .. str .. '"'
end

local function is_array(t)
  local n = 0
  for k, _ in pairs(t) do
    if type(k) ~= "number" then
      return false
    end
    if k > n then
      n = k
    end
  end
  return n == #t
end

function json.encode(val)
  local t = type(val)
  if val == nil then
    return "null"
  elseif t == "boolean" then
    return val and "true" or "false"
  elseif t == "number" then
    return string.format("%.17g", val)
  elseif t == "string" then
    return encode_string(val)
  elseif t == "table" then
    if is_array(val) then
      local parts = {}
      for i = 1, #val do
        parts[i] = json.encode(val[i])
      end
      return "[" .. table.concat(parts, ",") .. "]"
    else
      local parts = {}
      for k, v in pairs(val) do
        parts[#parts + 1] = encode_string(k) .. ":" .. json.encode(v)
      end
      table.sort(parts)
      return "{" .. table.concat(parts, ",") .. "}"
    end
  end
  error("cannot encode " .. t)
end

return json
