-- Ship Art runtime loader and fallback (UPGRADE-PLAN / SHIP-ART Phase 4).
--
-- Pure Lua core: no love.* APIs. Runs under plain luajit so it is testable
-- headless, matching the convention established by geom.lua and hex.lua. The
-- only Love touchpoint (image loading) is injected via the `load_image` seam so
-- the loader can be unit-tested with a stub.
--
-- Responsibilities (frozen contracts, PHASE0-INVENTORY.md §4):
--   * Canonical identity is `class_id` (catalog key / file stem), never the
--     numeric instance `id` and never the display `class` name.
--   * Alias resolution: tutorial_* class_ids borrow a base class's art.
--   * Invalid art always falls back to the geometric marker and emits at most
--     one diagnostic per asset.
--   * Board footprint does not exceed the existing circle-marker footprint.
--
-- This module does NOT draw. It resolves "which art, if any, for this ship?"
-- and returns a descriptor the board renderer consumes. The renderer decides
-- whether to draw a sprite or fall back to the circle.

local geom = require("geom")

local ship_art = {}

-- Source orientation: authored top-down art points UP (canonical). The runtime
-- rotation must map the source's "up" onto geom.facing_angle(facing). "Up" on
-- screen is -Y, i.e. angle -pi/2. The offset is applied by the caller via
-- geom.facing_angle; this constant documents the contract and is exposed for
-- tests that verify the rotation math.
ship_art.SOURCE_UP_ANGLE = -math.pi / 2

--- Load and validate a manifest blob (already-decoded JSON table).
--- Returns a normalized loader state or nil, err.
---   manifest = { version = 1, records = { {class_id=..., state=..., ...}, ... } }
--- A record with no/empty `image_path` is treated as "not yet generated" and
--- resolves to fallback (not an error). A record whose class_id is missing is
--- a schema error and is skipped with a diagnostic.
local function safe_image_path(path)
  if type(path) ~= "string" then
    return false
  end
  if path == "" then
    return true
  end
  -- Manifest paths are relative to assets/ship_art. Reject absolute paths,
  -- Windows separators/drives, control characters, and non-normalized path
  -- components before a value can reach love.graphics.newImage.
  if path:sub(1, 1) == "/" or path:match("^%a:") or path:find("\\", 1, true) then
    return false
  end
  if path:find("%c") or path:find("//", 1, true) or path:sub(-1) == "/" then
    return false
  end
  for part in path:gmatch("[^/]+") do
    if part == "." or part == ".." then
      return false
    end
  end
  return true
end

local function finite_number(value)
  return type(value) == "number" and value == value
    and value ~= math.huge and value ~= -math.huge
end

local function valid_presentation(rec)
  if not finite_number(rec.width) or rec.width <= 0
      or not finite_number(rec.height) or rec.height <= 0 then
    return false
  end
  if not finite_number(rec.anchor_x) or rec.anchor_x < 0 or rec.anchor_x > 1
      or not finite_number(rec.anchor_y) or rec.anchor_y < 0 or rec.anchor_y > 1 then
    return false
  end
  if not finite_number(rec.source_angle) then
    return false
  end
  -- Scale is a fraction of the frozen marker footprint, never an expansion.
  if not finite_number(rec.scale) or rec.scale <= 0 or rec.scale > 1 then
    return false
  end
  return true
end

function ship_art.load_manifest(manifest)
  if type(manifest) ~= "table" then
    return nil, "manifest must be a table"
  end
  local version = manifest.version or 1
  if version ~= 1 then
    return nil, "unsupported manifest version: " .. tostring(version)
  end
  local records = manifest.records or {}
  if type(records) ~= "table" then
    return nil, "manifest.records must be a table"
  end

  -- Index records by class_id -> { [state] = record }. Last write wins is fine;
  -- the catalog is authoritative and duplicates would be a generation bug.
  local by_class = {}
  local diagnostics = {}
  for _, rec in ipairs(records) do
    if type(rec) ~= "table" then
      diagnostics[#diagnostics + 1] = "manifest record must be a table, skipped"
    elseif type(rec.class_id) ~= "string" or rec.class_id == "" then
      diagnostics[#diagnostics + 1] = "record has invalid class_id, skipped"
    elseif type(rec.state) ~= "string" or rec.state == "" then
      diagnostics[#diagnostics + 1] = "record has invalid state, skipped"
    elseif rec.image_path ~= nil and not safe_image_path(rec.image_path) then
      diagnostics[#diagnostics + 1] = "record has invalid image_path, skipped"
    elseif rec.image_path ~= nil and rec.image_path ~= "" and not valid_presentation(rec) then
      diagnostics[#diagnostics + 1] = "record has invalid presentation metadata, skipped"
    else
      local cid = rec.class_id
      by_class[cid] = by_class[cid] or {}
      by_class[cid][rec.state] = rec
    end
  end

  return {
    version = version,
    by_class = by_class,
    diagnostics = diagnostics,
    -- Set of class_ids we have already warned about, so invalid art emits at
    -- most one diagnostic per asset (contract §4).
    warned = {},
  }, nil
end

--- Look up art for a ship. Returns a descriptor table:
---   { found = bool, class_id = <class>, state = <state>, record = <rec|nil>, fallback = bool }
--- `fallback = true` means the renderer must draw the geometric marker.
--- `fallback = false` means a usable record exists (caller still loads the image).
--- Emits at most one diagnostic per (class_id, state) into state.diagnostics.
---
---   state    — loader state from load_manifest
---   class_id — the ship's class_id (canonical identity)
---   want_state — "top_down" (board) or "portrait" (hud); defaults to "top_down"
function ship_art.lookup(state, class_id, want_state)
  if not state then
    return { found = false, class_id = class_id, state = want_state or "top_down", record = nil, fallback = true }
  end
  want_state = want_state or "top_down"
  local recs = state.by_class[class_id]
  if not recs then
    -- No record at all for this class: silent fallback (common in P0 where the
    -- manifest is empty). Not a diagnostic-worthy condition.
    return { found = false, class_id = class_id, state = want_state, record = nil, fallback = true }
  end
  local rec = recs[want_state]
  if not rec then
    -- State semantics are distinct. A board sprite is not a portrait, and a
    -- live top-down ship must not replace the gray destroyed marker.
    return { found = false, class_id = class_id, state = want_state, record = nil, fallback = true }
  end
  -- A record with no image path is "registered but not generated" -> fallback.
  if not rec.image_path or rec.image_path == "" then
    return { found = false, class_id = class_id, state = want_state, record = rec, fallback = true }
  end
  return { found = true, class_id = class_id, state = want_state, record = rec, fallback = false }
end

--- Record a one-shot diagnostic for an asset that failed to load at runtime
--- (e.g. image file missing on disk). Honors the "at most one per asset"
--- contract by keying on (class_id, state). No-op after the first call.
function ship_art.diagnostic(state, class_id, state_name, message)
  if not state then return end
  local key = (class_id or "?") .. ":" .. (state_name or "?")
  if state.warned[key] then
    return
  end
  state.warned[key] = true
  state.diagnostics[#state.diagnostics + 1] = message
end

--- Build a runtime art cache backed by an injected image loader.
---   load_image(path) -> image_handle | nil, err
--- The loader is the only Love touchpoint (love.graphics.newImage in prod).
--- Returns a cache object with:
---   cache:get(state, class_id, want_state) -> {fallback=..., image=...}
--- On any load failure, records a diagnostic and returns fallback=true so the
--- renderer never crashes on a bad asset.
function ship_art.new_cache(loader_state, load_image)
  local cache = {}
  local img_cache = {} -- path -> image_handle | false (false = failed)

  function cache:get(class_id, want_state)
    local desc = ship_art.lookup(loader_state, class_id, want_state)
    if desc.fallback or not desc.record then
      return desc
    end
    local path = "assets/ship_art/" .. desc.record.image_path
    if img_cache[path] == nil then
      local ok = true
      local img
      local err
      if load_image then
        img, err = load_image(path)
        if not img then
          ok = false
        end
      else
        ok = false
      end
      if ok then
        img_cache[path] = img
      else
        img_cache[path] = false
        ship_art.diagnostic(loader_state, desc.class_id, desc.state,
          "failed to load image for " .. tostring(desc.class_id) .. " (" .. tostring(path) .. ")")
        desc.fallback = true
        desc.image = nil
        return desc
      end
    elseif img_cache[path] == false then
      desc.fallback = true
      desc.image = nil
      return desc
    end
    desc.image = img_cache[path]
    return desc
  end

  return cache
end

local function image_dimensions(desc)
  if desc.fallback or not desc.image or not desc.image.getDimensions then
    return nil, nil
  end
  local ok, width, height = pcall(desc.image.getDimensions, desc.image)
  if not ok or not finite_number(width) or width <= 0
      or not finite_number(height) or height <= 0 then
    return nil, nil
  end
  return width, height
end

--- Build the presentation seam shared by the board and HUD. The manifest is
--- the sole runtime identity/alias authority: aliases are materialized by the
--- Python publisher, so renderers never load the authoring catalog.
function ship_art.new_presenter(manifest, load_image)
  local loader_state, err = ship_art.load_manifest(manifest)
  if not loader_state then
    return nil, err
  end
  local cache = ship_art.new_cache(loader_state, load_image)
  local presenter = {}

  function presenter:board_decision(ship, marker_radius)
    local class_id = ship and ship.class_id or ""
    local want_state = ship and ship.destroyed and "destroyed" or "top_down"
    local desc = cache:get(class_id, want_state)
    local width, height = image_dimensions(desc)
    if not width then
      return { fallback = true, class_id = class_id, state = want_state }
    end
    local record = desc.record
    local origin_x = width * record.anchor_x
    local origin_y = height * record.anchor_y
    local far_x = math.max(origin_x, width - origin_x)
    local far_y = math.max(origin_y, height - origin_y)
    local farthest_corner = math.sqrt(far_x * far_x + far_y * far_y)
    if not finite_number(marker_radius) or marker_radius <= 0 or farthest_corner <= 0 then
      return { fallback = true, class_id = class_id, state = want_state }
    end
    local scale = marker_radius / farthest_corner * record.scale
    return {
      fallback = false,
      class_id = class_id,
      state = want_state,
      image = desc.image,
      scale = scale,
      angle = geom.facing_angle(ship.facing or 0) - ship_art.SOURCE_UP_ANGLE
        + math.rad(record.source_angle),
      origin_x = origin_x,
      origin_y = origin_y,
      outer_radius = farthest_corner * scale,
    }
  end

  function presenter:portrait_decision(class_id, max_pixels)
    local desc = cache:get(class_id or "", "portrait")
    local width, height = image_dimensions(desc)
    if not width or not finite_number(max_pixels) or max_pixels <= 0 then
      return { fallback = true, class_id = class_id, state = "portrait" }
    end
    local scale = max_pixels / math.max(width, height) * desc.record.scale
    return {
      fallback = false,
      class_id = class_id,
      state = "portrait",
      image = desc.image,
      scale = scale,
      width = width * scale,
      height = height * scale,
    }
  end

  function presenter:diagnostics()
    return loader_state.diagnostics
  end

  return presenter, nil
end

return ship_art
