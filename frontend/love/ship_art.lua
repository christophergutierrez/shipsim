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
--- A record with no/empty `image` path is treated as "not yet generated" and
--- resolves to fallback (not an error). A record whose class_id is missing is
--- a schema error and is skipped with a diagnostic.
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
    if type(rec) ~= "table" or not rec.class_id then
      diagnostics[#diagnostics + 1] = "record missing class_id, skipped"
    else
      local cid = rec.class_id
      by_class[cid] = by_class[cid] or {}
      local state = rec.state or "top_down"
      by_class[cid][state] = rec
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

--- Resolve a class_id through the alias map. Returns the canonical class_id
--- that should be used for art lookup. Pure function.
---   aliases = { ["tutorial_escort"] = "escort", ... }
--- Aliases may chain (A -> B -> C) but we cap at a small depth to reject cycles.
function ship_art.resolve_alias(class_id, aliases)
  aliases = aliases or {}
  local seen = {}
  local cur = class_id
  for _ = 1, 8 do
    if not aliases[cur] then
      return cur
    end
    if seen[cur] then
      -- cycle: return the last non-cyclic id we had
      return cur
    end
    seen[cur] = true
    cur = aliases[cur]
  end
  return cur
end

--- Look up art for a ship. Returns a descriptor table:
---   { found = bool, class_id = <canonical>, state = <state>, record = <rec|nil>, fallback = bool }
--- `fallback = true` means the renderer must draw the geometric marker.
--- `fallback = false` means a usable record exists (caller still loads the image).
--- Emits at most one diagnostic per (class_id, state) into state.diagnostics.
---
---   state    — loader state from load_manifest
---   class_id — the ship's class_id (canonical identity)
---   aliases  — alias map (usually from catalog.json); may be nil
---   want_state — "top_down" (board) or "portrait" (hud); defaults to "top_down"
function ship_art.lookup(state, class_id, aliases, want_state)
  if not state then
    return { found = false, class_id = class_id, state = want_state or "top_down", record = nil, fallback = true }
  end
  want_state = want_state or "top_down"
  local canonical = ship_art.resolve_alias(class_id, aliases)
  local recs = state.by_class[canonical]
  if not recs then
    -- No record at all for this class: silent fallback (common in P0 where the
    -- manifest is empty). Not a diagnostic-worthy condition.
    return { found = false, class_id = canonical, state = want_state, record = nil, fallback = true }
  end
  local rec = recs[want_state]
  if not rec then
    -- Record exists for the class but not this state. Try top_down as a
    -- graceful degradation before falling back to geometry.
    rec = recs.top_down
    if not rec then
      return { found = false, class_id = canonical, state = want_state, record = nil, fallback = true }
    end
    want_state = "top_down"
  end
  -- A record with no image path is "registered but not generated" -> fallback.
  if not rec.image or rec.image == "" then
    return { found = false, class_id = canonical, state = want_state, record = rec, fallback = true }
  end
  return { found = true, class_id = canonical, state = want_state, record = rec, fallback = false }
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
---   cache:get(state, class_id, aliases, want_state) -> {fallback=..., image=...}
--- On any load failure, records a diagnostic and returns fallback=true so the
--- renderer never crashes on a bad asset.
function ship_art.new_cache(loader_state, aliases, load_image)
  local cache = {}
  local img_cache = {} -- path -> image_handle | false (false = failed)

  function cache:get(loader_state, class_id, want_state)
    local desc = ship_art.lookup(loader_state, class_id, aliases, want_state)
    if desc.fallback or not desc.record then
      return desc
    end
    local path = desc.record.image
    if img_cache[path] == nil then
      local ok = true
      local img
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

return ship_art
