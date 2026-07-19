-- Tutorial mode — step-gated walkthrough (Love2D port of the TUI tutorial).
--
-- Pure Lua: no love.* APIs. Runs under plain luajit so it is testable
-- headless, mirroring selection.lua / events.lua / preview.lua. The
-- controller (main.lua) owns the tutorial state and calls these functions
-- from its key handler (before command_mapping dispatch) and its snapshot
-- apply path; they mutate the passed `t` table in place.
--
-- UPGRADE-PLAN Phase 6: mirrors frontend/tui/src/tutorial.rs (the
-- REAR_ATTACK_STEPS sequence, the gate machine, and the two-phase
-- order-backed commit). Same scenario (scenarios/tutorial_rear_attack.toml,
-- seed 4), same step order, same gate conditions.
--
-- Gate machine summary (see tutorial.rs:96-192):
--   check_action(t, action)      — discrete steps; may advance immediately.
--   validate_action(t, action)   — order-backed steps; validates but does NOT
--                                  advance. The caller advances only after the
--                                  engine returns an accepted snapshot
--                                  (confirm_order, mirroring app.rs:735-744).
--   check_reach_value(t, field, old, new) — ReachValue steps; returns
--                                  (allow, advanced). Allows edits that move
--                                  toward the target; advances only on ==.
--
-- `t` shape (a slice of the Love app table):
--   current     — 1-based step index (Lua convention; nil/0 = not started)
--   error_msg   — string or nil (last gate error, shown in coach panel)
--
-- Step table shape (mirrors TutorialStep, tutorial.rs:43-54):
--   { title=, text=, why=, expected=, hint= }
-- where `expected` is a Lua table tagged by its first key, e.g.
--   { kind="ReachValue", field=0, target=10 }
--   { kind="NavField", field=4 }
--   { kind="TurnTo", facing=3 }
--   { kind="ShieldFacing", facing=3 }
--   { kind="Accel" } / { kind="CommitAllocate" } / ... (unit variants)

local tutorial = {}

-- Allocate cursor field labels (heavy cruiser, ship/TOML order).
-- Mirrors field_label in tutorial.rs:292-306.
local FIELD_LABELS = {
  [0] = "Engine (Movement)",
  [1] = "beam_1",
  [2] = "torp_1",
  [3] = "plasma_1",
  [4] = "shield F (forward)",
  [5] = "shield FR (fwd-right)",
  [6] = "shield RR (rear-right)",
  [7] = "shield R (rear)",
  [8] = "shield RL (rear-left)",
  [9] = "shield FL (fwd-left)",
}

local function field_label(field)
  return FIELD_LABELS[field] or ("field " .. tostring(field))
end

-- Strict variant+payload equality for discrete actions.
-- Mirrors action_matches in tutorial.rs:308-330.
local function action_matches(expected, actual)
  if expected.kind ~= actual.kind then
    return false
  end
  if expected.kind == "NavField" then
    return expected.field == actual.field
  elseif expected.kind == "TurnTo" then
    return expected.facing == actual.facing
  elseif expected.kind == "ShieldFacing" then
    return expected.facing == actual.facing
  elseif expected.kind == "TabWeapon" then
    return expected.weapon == nil or expected.weapon == actual.weapon
  end
  -- Unit variants: CommitAllocate, Accel, Coast, EnterMap, PanMap, ZoomOut,
  -- ZoomIn, RecenterMap, ExitMap, EnterFire, FireWeapon, TabWeapon,
  -- ReadyFire, EndTurn, Dismiss.
  return true
end

-- The rear-attack sequence. Mirrors REAR_ATTACK_STEPS (tutorial.rs:338-577).
-- 26 steps; same titles, text, why, hint, and gate conditions as the TUI.
local REAR_ATTACK_STEPS = {
  -- ── Turn 1 allocate ────────────────────────────────────────────────
  {
    title = "Engine power (Movement)",
    text = "Each turn you split a power pool. Movement is not distance — it buys a thrust pool for this turn only. You will spend thrust later to accel and turn. Velocity (speed/course) persists after end-turn; thrust does not.\n\nYellow bar shows why + keys. ▶ marks the selected allocate field. Set Movement to 10 so we can race past the escort.",
    why = "Engine = thrust this turn (not permanent speed)",
    hint = "→ until Movement = 10, or type 10",
    expected = { kind = "ReachValue", field = 0, target = 10 },
  },
  {
    title = "Charge the beam",
    text = "Weapons are separate power sinks. beam_1 is your main gun: multi-charge, solid damage, long range. Charge carries across turns if you don't fire — we load it now and hold for the stern shot.\n\nThe form auto-selects beam_1 (▶). ↓/↑ move between fields; →/← set the value. Charge to 4 (max) — more charge = more beam damage. We will not shoot until we are behind the escort.",
    why = "beam_1 charge = damage budget for later volley",
    hint = "→ until beam charge = 4, or type 4",
    expected = { kind = "ReachValue", field = 1, target = 4 },
  },
  {
    title = "Charge torpedo",
    text = "torp_1 is a single-charge, fixed-damage shot. It fires in the same volley as beam and plasma. Weapon rows follow ship order (same list you will see in fire mode). Charge to 1 (max) and leave it loaded for the rear volley.",
    why = "Arm torp for the same volley as beam + plasma",
    hint = "→ once, or type 1",
    expected = { kind = "ReachValue", field = 2, target = 1 },
  },
  {
    title = "Charge plasma",
    text = "plasma_1 is a short-range hammer (max charge 1). Huge damage at close range — the finisher of the rear-arc dump. One point arms it; the charge stays until you fire.",
    why = "Arm plasma for the close rear-arc dump",
    hint = "→ once, or type 1",
    expected = { kind = "ReachValue", field = 3, target = 1 },
  },
  {
    title = "Select forward shield",
    text = "Shields are six faces around the ship (F, FR, RR, R, RL, FL). They always start at 0 each allocate — no leftover armor. Power on a face absorbs hits that land there.\n\nF (forward) faces your nose. The escort will shoot your bow while you close, so we armor F first.",
    why = "Select shield F — nose armor vs their approach fire",
    hint = "↓ to shield F",
    expected = { kind = "NavField", field = 4 },
  },
  {
    title = "Power forward shield",
    text = "Put 6 on F (max per face). Hits on your forward arc spend this before hull. Budget: 10 engine + 4+1+1 weapons + 6 F = 22 (full pool).",
    why = "Shield F=6 so bow hits soak before hull",
    hint = "→ until F = 6, or type 6",
    expected = { kind = "ReachValue", field = 4, target = 6 },
  },
  {
    title = "Commit allocate",
    text = "Nothing is spent in the engine until you commit. Enter sends the allocate order and opens movement cycle 1 of 4.",
    why = "Commit power plan — draft becomes real",
    hint = "Enter",
    expected = { kind = "CommitAllocate" },
  },
  -- ── Turn 1 movement ────────────────────────────────────────────────
  {
    title = "Accel — leave the pier",
    text = "t = accel: spend 1 thrust along your facing (nose). From a stop, that sets course = facing and speed 1, then you slide 1 hex on course. Each cycle you slide `speed` hexes.",
    why = "Build eastbound speed — race past the escort",
    hint = "t",
    expected = { kind = "Accel" },
  },
  {
    title = "Hold fire — cycle 1",
    text = "You can bear on them, but you would hit their forward shields. Space = ready fire: leave the fire window without spending weapon charge. (e would end the whole turn — wrong here.)",
    why = "Don't waste charged weapons on their bow",
    hint = "Space",
    expected = { kind = "ReadyFire" },
  },
  {
    title = "Accel — speed 2",
    text = "Accel again: speed 2, slide 2 hexes east. Range collapses fast.",
    why = "More speed = longer slides east each cycle",
    hint = "t",
    expected = { kind = "Accel" },
  },
  {
    title = "Hold fire — cycle 2",
    text = "Still not a stern shot. Hold charge.",
    why = "Still wrong geometry — hold the volley",
    hint = "Space",
    expected = { kind = "ReadyFire" },
  },
  {
    title = "Turn nose west",
    text = "Facing and course are different. Turn only changes facing (guns/nose); course/speed keep you sliding east. Face 3 = west. Cost is hex-ring distance (0→3 costs 3 thrust). Inertia carries you past the escort while your guns turn onto its stern.",
    why = "Point guns west while still flying east (stern shot)",
    hint = "3",
    expected = { kind = "TurnTo", facing = 3 },
  },
  {
    title = "Focus the tactical map",
    text = "You crossed the escort and now have its unshielded stern in front of your guns. Before firing, press v to focus the map. Map focus is read-only: it never spends thrust or advances the phase.",
    why = "Inspect the pass without issuing an order",
    hint = "v",
    expected = { kind = "EnterMap" },
  },
  {
    title = "Pan toward the escort",
    text = "WASD pans the camera. The escort is west (left) of you, so press a. Panning changes only what you can see; ships keep their coordinates.",
    why = "Move the camera west to inspect the target",
    hint = "a",
    expected = { kind = "PanMap" },
  },
  {
    title = "Zoom out",
    text = "- zooms out to cover more space. Use it when contacts or projected movement spread beyond the current view.",
    why = "Fit more of the battle into the map",
    hint = "-",
    expected = { kind = "ZoomOut" },
  },
  {
    title = "Zoom in",
    text = "+ zooms back in for readable local geometry. Zoom and pan remain manual until you ask the camera to auto-fit again.",
    why = "Return to a closer tactical view",
    hint = "+",
    expected = { kind = "ZoomIn" },
  },
  {
    title = "Auto-fit contacts",
    text = "c clears manual pan and zoom. The map automatically frames all living ships and, during allocation, your movement preview.",
    why = "Let the camera frame the battle again",
    hint = "c",
    expected = { kind = "RecenterMap" },
  },
  {
    title = "Return to fire controls",
    text = "Press v again to leave map focus. Your firing window is still waiting exactly where you left it.",
    why = "Return without changing the game state",
    hint = "v",
    expected = { kind = "ExitMap" },
  },
  {
    title = "Aim at the rear shield face",
    text = "Shots must name the target face they enter. The escort faces west, so your attack from its east side hits face 3: R (rear). Press → until the fire panel shows target shield R. The engine validates this against the actual geometry.",
    why = "Aim the volley through the unshielded rear face",
    hint = "→ until target shield = 3:R",
    expected = { kind = "ShieldFacing", facing = 3 },
  },
  {
    title = "Fire the beam",
    text = "Fire mode is open. Enter queues beam_1 at the escort (does not resolve yet). Charge drops when everyone readies.",
    why = "Queue beam into their unshielded stern",
    hint = "Enter",
    expected = { kind = "FireWeapon" },
  },
  {
    title = "Select torpedo",
    text = "↓ cycles the selected weapon to torp_1 (ship order: beam, torp, plasma).",
    why = "Select torp for the same volley",
    hint = "↓",
    expected = { kind = "TabWeapon", weapon = "torp_1" },
  },
  {
    title = "Fire the torpedo",
    text = "Queue torp_1. It does not resolve until every living ship readies.",
    why = "Queue torp into the simultaneous volley",
    hint = "Enter",
    expected = { kind = "FireWeapon" },
  },
  {
    title = "Select plasma",
    text = "↓ to plasma_1 — the short-range finisher.",
    why = "Select plasma finisher",
    hint = "↓",
    expected = { kind = "TabWeapon", weapon = "plasma_1" },
  },
  {
    title = "Fire the plasma",
    text = "Queue plasma. All three resolve together when you press Space.",
    why = "Queue plasma — complete the full volley",
    hint = "Enter",
    expected = { kind = "FireWeapon" },
  },
  {
    title = "Resolve the kill",
    text = "Space marks you ready. Hits/misses resolve; escort should die (Won).",
    why = "Resolve the triple volley",
    hint = "Space",
    expected = { kind = "ReadyFire" },
  },
  {
    title = "Victory",
    text = "Turn-one rear-arc volley complete. Yellow bar can rest.",
    why = "Won — Enter dismisses or q quits",
    hint = "Enter or q",
    expected = { kind = "Dismiss" },
  },
}

tutorial.STEPS = REAR_ATTACK_STEPS
tutorial.NAME = "rear-attack"
tutorial.OBJECTIVE = "Race past the escort, inspect the map, and destroy it from behind with all weapons."

--- Create a fresh tutorial controller state (1-based current index).
function tutorial.new()
  return {
    name = tutorial.NAME,
    objective = tutorial.OBJECTIVE,
    steps = REAR_ATTACK_STEPS,
    current = 1,
    error_msg = nil,
  }
end

--- Number of steps in the sequence.
function tutorial.step_count(t)
  return #t.steps
end

--- The current step table, or nil if complete.
function tutorial.current_step(t)
  if t.current > #t.steps then
    return nil
  end
  return t.steps[t.current]
end

--- True when all steps are done.
function tutorial.is_complete(t)
  return t.current > #t.steps
end

--- Advance to the next step and clear the error.
function tutorial.advance(t)
  t.current = t.current + 1
  t.error_msg = nil
end

--- Set a gate error message.
function tutorial.set_error(t, msg)
  t.error_msg = msg
end

--- Check a discrete action. May advance immediately for non-order-backed steps.
--- Returns true if the action was accepted (and possibly advanced).
--- Mirrors check_action in tutorial.rs:96-132.
function tutorial.check_action(t, action)
  local step = tutorial.current_step(t)
  if not step then
    return false
  end
  local expected = step.expected
  -- NavField: allow stepping toward the target via ↓; advance only on ==.
  if expected.kind == "NavField" then
    if action.kind == "NavField" then
      if action.field <= expected.field then
        t.error_msg = nil
        if action.field == expected.field then
          tutorial.advance(t)
        end
        return true
      end
      t.error_msg = ("Go to field %d (↓). %s"):format(expected.field, step.hint)
      return false
    end
  end
  -- ShieldFacing: allow stepping toward the target via →; advance only on ==.
  if expected.kind == "ShieldFacing" then
    if action.kind == "ShieldFacing" then
      if action.facing <= expected.facing then
        t.error_msg = nil
        if action.facing == expected.facing then
          tutorial.advance(t)
        end
        return true
      end
      t.error_msg = ("Select shield face %d with →. %s"):format(expected.facing, step.hint)
      return false
    end
  end
  -- All other discrete variants: strict match.
  if action_matches(expected, action) then
    tutorial.advance(t)
    return true
  end
  t.error_msg = ("Expected: %s. %s"):format(step.title, step.hint)
  return false
end

--- Validate an order-backed action WITHOUT advancing. The caller advances only
--- after the engine returns an accepted snapshot (confirm_order).
--- Mirrors validate_action in tutorial.rs:136-144.
function tutorial.validate_action(t, action)
  local step = tutorial.current_step(t)
  if not step then
    return false
  end
  if action_matches(step.expected, action) then
    return true
  end
  t.error_msg = ("Expected: %s. %s"):format(step.title, step.hint)
  return false
end

--- Advance an order-backed step only after the engine accepts its candidate.
function tutorial.confirm_order(t, candidate, accepted)
  if not candidate or not accepted then
    return false
  end
  tutorial.advance(t)
  return true
end

--- Check a ReachValue edit. Returns (allow, advanced).
--- allow=true permits the draft edit; advanced=true means the step completed.
--- Mirrors check_reach_value in tutorial.rs:147-192.
function tutorial.check_reach_value(t, field, old_value, new_value)
  local step = tutorial.current_step(t)
  if not step then
    return true, false
  end
  local expected = step.expected
  if expected.kind ~= "ReachValue" then
    t.error_msg = ("Expected: %s. %s"):format(step.title, step.hint)
    return false, false
  end
  local exp_field, target = expected.field, expected.target
  if field ~= exp_field then
    t.error_msg = ("Wrong field (▶ slot %s; need %s). Press ↓/↑. %s"):format(
      tostring(field), field_label(exp_field), step.hint)
    return false, false
  end
  if new_value == old_value then
    t.error_msg = ("Value is %d; need %d. Use → / ← (or digits)."):format(old_value, target)
    return false, false
  end
  if new_value == target then
    tutorial.advance(t)
    return true, true
  end
  if new_value > target then
    t.error_msg = ("Too high (%d > %d). Press ← to come back down."):format(new_value, target)
  else
    t.error_msg = ("Now %d / need %d. Press → to raise (← lowers)."):format(new_value, target)
  end
  return true, false
end

--- Render the yellow bar line: why first, then the key action / live value.
--- cursor: current allocate cursor index (nil if not in allocate).
--- field_value: current value of the focused allocate field (nil if n/a).
--- Mirrors do_now_line in tutorial.rs:195-249.
function tutorial.do_now_line(t, cursor, field_value)
  if tutorial.is_complete(t) then
    return "Tutorial complete — press q to quit."
  end
  local step = tutorial.current_step(t)
  if not step then
    return "Tutorial complete — press q to quit."
  end
  local why = step.why
  local expected = step.expected
  if expected.kind == "ReachValue" then
    local field, target = expected.field, expected.target
    local cur = field_value or 0
    local on = cursor == field
    local name = field_label(field)
    if not on then
      return ("%s · ↓/↑ until ▶ is on %s, then set to %d"):format(why, name, target)
    elseif cur < target then
      return ("%s · %s %d→%d  (arrows or type %d)"):format(why, name, cur, target, target)
    elseif cur > target then
      return ("%s · %s %d→%d  (← back down · overshot)"):format(why, name, cur, target)
    else
      return ("%s · %s is %d — should advance"):format(why, name, target)
    end
  elseif expected.kind == "NavField" then
    local target = expected.field
    local cur = cursor or 0
    return ("%s · ↓ to ▶ %s  (now on %s)"):format(why, field_label(target), field_label(cur))
  elseif expected.kind == "CommitAllocate" then
    return ("%s · Enter (lock plan, open movement)"):format(why)
  elseif expected.kind == "Accel" then
    return ("%s · t (accel along nose)"):format(why)
  elseif expected.kind == "TurnTo" then
    return ("%s · press %d (face %d only — course unchanged)"):format(why, expected.facing, expected.facing)
  elseif expected.kind == "Coast" then
    return ("%s · c (coast / free slide)"):format(why)
  elseif expected.kind == "EnterMap" then
    return ("%s · v (focus the map)"):format(why)
  elseif expected.kind == "PanMap" then
    return ("%s · a (pan west / left)"):format(why)
  elseif expected.kind == "ZoomOut" then
    return ("%s · - (zoom out)"):format(why)
  elseif expected.kind == "ZoomIn" then
    return ("%s · + (zoom in)"):format(why)
  elseif expected.kind == "RecenterMap" then
    return ("%s · c (auto-fit contacts)"):format(why)
  elseif expected.kind == "ExitMap" then
    return ("%s · v (return to fire controls)"):format(why)
  elseif expected.kind == "EnterFire" then
    return ("%s · f or Enter (fire mode)"):format(why)
  elseif expected.kind == "ShieldFacing" then
    return ("%s · → until target shield face = %d"):format(why, expected.facing)
  elseif expected.kind == "FireWeapon" then
    return ("%s · Enter (queue shot)"):format(why)
  elseif expected.kind == "TabWeapon" then
    return ("%s · ↓ (next weapon)"):format(why)
  elseif expected.kind == "ReadyFire" then
    return ("%s · Space (ready — resolve or skip fire)"):format(why)
  elseif expected.kind == "EndTurn" then
    return ("%s · e (end turn)"):format(why)
  elseif expected.kind == "Dismiss" then
    return ("%s · Enter or q"):format(why)
  end
  return why
end

--- Render the bottom coach panel body. Mirrors narration in tutorial.rs:251-273.
function tutorial.narration(t)
  if tutorial.is_complete(t) then
    return "Tutorial complete! The rear-arc alpha strike secured the win. Press q to quit."
  end
  local step = tutorial.current_step(t)
  if not step then
    return "Tutorial complete!"
  end
  local text = ""
  if t.error_msg then
    text = text .. ("⚠ %s\n"):format(t.error_msg)
  end
  -- Strip Markdown ** and ` markers (plain-text rendering, like the TUI).
  local body = step.text:gsub("%*%*", ""):gsub("`", "")
  text = text .. body
  return text
end

--- Compact pinned guidance. Errors replace the normal prompt so feedback is
--- visible without scrolling to the full narration panel.
function tutorial.pinned_prompt(t, cursor, field_value)
  if t and t.error_msg then
    return "Try again: " .. t.error_msg
  end
  return tutorial.do_now_line(t, cursor, field_value)
end

--- Detect an unexpected game-over mid-lesson. Returns an error string or nil.
--- Mirrors state_error in tutorial.rs:275-288.
function tutorial.state_error(t, snap)
  if tutorial.is_complete(t) then
    return nil
  end
  local step = tutorial.current_step(t)
  if not step then
    return nil
  end
  if not snap then
    return nil
  end
  -- snap.is_over() equivalent: status is Won/Lost.
  local over = (snap.status == "Won" or snap.status == "Lost")
  if over and step.expected.kind ~= "Dismiss" then
    if snap.status == "Won" then
      return nil
    end
    return ("Game ended unexpectedly: %s"):format(snap.status)
  end
  return nil
end

return tutorial
