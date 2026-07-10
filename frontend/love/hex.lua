-- Flat-top axial hex layout (frontend only; core is orientation-agnostic).

local hex = {}

--- Pixel center of axial (q,r). size = distance center to vertex.
function hex.to_pixel(q, r, size)
  local x = size * (3 / 2) * q
  local y = size * math.sqrt(3) * (r + q / 2)
  return x, y
end

--- Inverse of to_pixel (round to nearest hex).
function hex.from_pixel(x, y, size)
  local q = (2 / 3) * x / size
  local r = (-1 / 3 * x + math.sqrt(3) / 3 * y) / size
  return hex.round(q, r)
end

function hex.round(qf, rf)
  local xf, zf = qf, rf
  local yf = -xf - zf
  local rx, ry, rz = math.floor(xf + 0.5), math.floor(yf + 0.5), math.floor(zf + 0.5)
  local x_diff, y_diff, z_diff = math.abs(rx - xf), math.abs(ry - yf), math.abs(rz - zf)
  if x_diff > y_diff and x_diff > z_diff then
    rx = -ry - rz
  elseif y_diff > z_diff then
    ry = -rx - rz
  else
    rz = -rx - ry
  end
  return rx, rz
end

--- Flat-top vertex offsets (6 points) relative to center.
function hex.corners(cx, cy, size)
  local pts = {}
  for i = 0, 5 do
    local angle = math.pi / 180 * (60 * i)
    pts[#pts + 1] = cx + size * math.cos(angle)
    pts[#pts + 1] = cy + size * math.sin(angle)
  end
  return pts
end

--- Neighbor axial offsets for flat-top (same cube neighbors as core).
hex.DIRS = {
  { 1, 0 },
  { 1, -1 },
  { 0, -1 },
  { -1, 0 },
  { -1, 1 },
  { 0, 1 },
}

function hex.neighbor(q, r, facing)
  local d = hex.DIRS[(facing % 6) + 1]
  return q + d[1], r + d[2]
end

function hex.distance(aq, ar, bq, br)
  return (math.abs(aq - bq) + math.abs(aq + ar - bq - br) + math.abs(ar - br)) / 2
end

return hex
