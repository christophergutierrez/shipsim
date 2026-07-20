-- Tutorial mode — step-gated walkthrough (Love2D port of the TUI tutorial).
--
-- Pure Lua: no love.* APIs. Runs under plain luajit so it is testable
-- headless, mirroring selection.lua / events.lua / preview.lua. The
-- controller (main.lua) owns the tutorial state and calls these functions
-- from its key handler and its snapshot apply path; they mutate the passed
-- `t` table in place.
--
-- Protocol-v4 rear-attack walkthrough. Shares the TUI scenario
-- (scenarios/tutorial_rear_attack.toml, seed 4) and the same gate machine
-- (check_action / validate_action / check_reach_value). Step *count* and
-- titles intentionally diverge: Love has 28 steps (path drafting is split
-- for the graphical editor); TUI has 24 compound steps.
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
--   { kind="PathAppend" } / { kind="CommitPath" } / { kind="CommitVolley" } / ...

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
  elseif expected.kind == "PathAppend" then
    return expected.action == nil or expected.action == actual.action
  end
  -- Unit variants: CommitAllocate, PathAppend, CommitPath, CommitVolley,
  -- EnterMap, PanMap, ZoomOut, ZoomIn, RecenterMap, ExitMap, EnterFire,
  -- FireWeapon, TabWeapon, Dismiss.
  return true
end

-- The rear-attack sequence (Love-expanded vs TUI REAR_ATTACK_STEPS).
-- Protocol v4: allocate + path draft + volley.
local REAR_ATTACK_STEPS = {
  -- ── Turn 1 allocate ────────────────────────────────────────────────
  {
    title = "Engine power (Movement)",
    text = "Each turn you split a power pool. Movement is not distance — it buys a motion pool for this turn only. You spend motion on path actions (F/FR/FL/turns). Motion does not carry between turns.\n\nYellow bar shows why + keys. ▶ marks the selected allocate field. Set Movement to 10 so we have motion for a long path.",
    why = "Engine = motion points this turn (not permanent speed)",
    hint = "Click + on Movement until it reaches 10",
    expected = { kind = "ReachValue", field = 0, target = 10 },
  },
  {
    title = "Charge the beam",
    text = "Weapons are separate power sinks. beam_1 is your main gun: multi-charge, solid damage, long range. Charge carries across turns if you don't fire — we load it now and hold for the stern shot.\n\nClick + on beam_1 until its charge reaches 4 (max). More charge means more beam damage. We will not shoot until we are behind the escort.",
    why = "beam_1 charge = damage budget for later volley",
    hint = "Click + on beam_1 until charge reaches 4",
    expected = { kind = "ReachValue", field = 1, target = 4 },
  },
  {
    title = "Charge torpedo",
    text = "torp_1 is a single-charge, fixed-damage shot. It fires in the same volley as beam and plasma. Weapon rows follow ship order (same list you will see in fire mode). Charge to 1 (max) and leave it loaded for the rear volley.",
    why = "Arm torp for the same volley as beam + plasma",
    hint = "Click + once on torp_1",
    expected = { kind = "ReachValue", field = 2, target = 1 },
  },
  {
    title = "Charge plasma",
    text = "plasma_1 is a short-range hammer (max charge 1). Huge damage at close range — the finisher of the rear-arc dump. One point arms it; the charge stays until you fire.",
    why = "Arm plasma for the close rear-arc dump",
    hint = "Click + once on plasma_1",
    expected = { kind = "ReachValue", field = 3, target = 1 },
  },
  {
    title = "Select forward shield",
    text = "Shields are six faces around the ship (F, FR, RR, R, RL, FL). They always start at 0 each allocate — no leftover armor. Power on a face absorbs hits that land there.\n\nF (forward) faces your nose. The escort will shoot your bow while you close, so we armor F first.",
    why = "Select shield F — nose armor vs their approach fire",
    hint = "Click + once on shield F",
    expected = { kind = "NavField", field = 4 },
  },
  {
    title = "Power forward shield",
    text = "Put 6 on F (max per face). Hits on your forward arc spend this before hull. Budget: 10 engine + 4+1+1 weapons + 6 F = 22 (full pool).",
    why = "Shield F=6 so bow hits soak before hull",
    hint = "Keep clicking + on shield F until it reaches 6",
    expected = { kind = "ReachValue", field = 4, target = 6 },
  },
  {
    title = "Commit allocate",
    text = "Nothing is spent in the engine until you commit. Enter sends the allocate order and opens the path stage (one path per ship per turn).",
    why = "Commit power plan — draft becomes real",
    hint = "Enter",
    expected = { kind = "CommitAllocate" },
  },
  -- ── Turn 1 path (protocol v4) ──────────────────────────────────────
  {
    title = "Path east — leave the pier",
    text = "Movement is one ordered path per turn. Each action costs 1 motion: F/FR/FL translate; turn left/right rotate in place.\n\nPress W (or the F path button) to append move_f. The draft appears in the path panel; the engine previews legality.",
    why = "Start an eastbound path toward and past the escort",
    hint = "W or path F",
    expected = { kind = "PathAppend", action = "move_f" },
  },
  {
    title = "Path east again",
    text = "Append another move_f. You build a multi-hex path before you commit — no impulse cycles.",
    why = "Extend the path east",
    hint = "W or path F",
    expected = { kind = "PathAppend", action = "move_f" },
  },
  {
    title = "Path east a third time",
    text = "One more move_f so you clear past the escort.",
    why = "Race past the escort before the volley",
    hint = "W or path F",
    expected = { kind = "PathAppend", action = "move_f" },
  },
  {
    title = "Turn nose west (1)",
    text = "Append turn_left (Z or path TL). Facing changes are path actions too — same motion pool.",
    why = "Start rotating guns toward the escort stern",
    hint = "Z",
    expected = { kind = "PathAppend", action = "turn_left" },
  },
  {
    title = "Turn west (2)",
    text = "Second turn_left.",
    why = "Keep rotating toward face 3",
    hint = "Z",
    expected = { kind = "PathAppend", action = "turn_left" },
  },
  {
    title = "Turn west (3)",
    text = "Third turn_left — nose should face west (face 3).",
    why = "Complete the facing change in the draft",
    hint = "Z",
    expected = { kind = "PathAppend", action = "turn_left" },
  },
  {
    title = "Commit path",
    text = "Enter / Commit Path submits one commit_path order (draft must be non-empty). When every living ship has committed, paths resolve simultaneously. Use Hold Position (P) for an intentional empty path.",
    why = "Submit the full path",
    hint = "Enter or Commit Path",
    expected = { kind = "CommitPath" },
  },
  {
    title = "Focus the tactical map",
    text = "You should be past the escort with guns facing its stern. Press v to focus the map (read-only camera).",
    why = "Inspect geometry before the volley",
    hint = "v",
    expected = { kind = "EnterMap" },
  },
  {
    title = "Pan toward the escort",
    text = "WASD pans when the map is focused. Press a to pan west.",
    why = "Move the camera toward the target",
    hint = "a",
    expected = { kind = "PanMap" },
  },
  {
    title = "Zoom out",
    text = "- zooms out.",
    why = "Fit more of the battle",
    hint = "-",
    expected = { kind = "ZoomOut" },
  },
  {
    title = "Zoom in",
    text = "+ zooms back in.",
    why = "Closer tactical view",
    hint = "+",
    expected = { kind = "ZoomIn" },
  },
  {
    title = "Auto-fit contacts",
    text = "c clears manual pan/zoom and auto-fits living ships.",
    why = "Frame the battle again",
    hint = "c",
    expected = { kind = "RecenterMap" },
  },
  {
    title = "Return to fire controls",
    text = "Press v again to leave map focus. Firing still waits for your volley.",
    why = "Return without changing game state",
    hint = "v",
    expected = { kind = "ExitMap" },
  },
  {
    title = "Aim at the rear shield face",
    text = "Shots name the target face. From the escort's stern, aim face 3: R (rear).",
    why = "Aim through the unshielded rear face",
    hint = "Click the R shield-face button",
    expected = { kind = "ShieldFacing", facing = 3 },
  },
  {
    title = "Queue the beam",
    text = "Enter adds beam_1 to your volley draft (does not resolve yet).",
    why = "Queue beam into the volley",
    hint = "Enter",
    expected = { kind = "FireWeapon" },
  },
  {
    title = "Select torpedo",
    text = "↓ cycles to torp_1.",
    why = "Select torp for the same volley",
    hint = "↓",
    expected = { kind = "TabWeapon", weapon = "torp_1" },
  },
  {
    title = "Queue the torpedo",
    text = "Enter adds torp_1 to the volley.",
    why = "Queue torp",
    hint = "Enter",
    expected = { kind = "FireWeapon" },
  },
  {
    title = "Select plasma",
    text = "↓ to plasma_1.",
    why = "Select plasma finisher",
    hint = "↓",
    expected = { kind = "TabWeapon", weapon = "plasma_1" },
  },
  {
    title = "Queue the plasma",
    text = "Enter adds plasma to the volley.",
    why = "Complete the volley draft",
    hint = "Enter",
    expected = { kind = "FireWeapon" },
  },
  {
    title = "Commit volley — resolve the kill",
    text = "R / Ready submits commit_volley with your drafted shots. When every living ship commits, fire resolves simultaneously and the turn advances automatically (no end_turn).",
    why = "Submit the full volley",
    hint = "R or Ready",
    expected = { kind = "CommitVolley" },
  },
  {
    title = "Victory",
    text = "Rear-arc volley complete. Yellow bar can rest.",
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
    t.error_msg = ("Wrong field (%s; need %s). Use that row's +/- buttons. %s"):format(
      tostring(field), field_label(exp_field), step.hint)
    return false, false
  end
  if new_value == old_value then
    t.error_msg = ("Value is %d; need %d. Use the + / - buttons."):format(old_value, target)
    return false, false
  end
  if new_value == target then
    tutorial.advance(t)
    return true, true
  end
  if new_value > target then
    t.error_msg = ("Too high (%d > %d). Click - to come back down."):format(new_value, target)
  else
    t.error_msg = ("Now %d / need %d. Click + to raise (- lowers)."):format(new_value, target)
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
      return ("%s · click + on %s until it reaches %d"):format(why, name, target)
    elseif cur < target then
      return ("%s · %s %d→%d  (click +)"):format(why, name, cur, target)
    elseif cur > target then
      return ("%s · %s %d→%d  (click -)"):format(why, name, cur, target)
    else
      return ("%s · %s is %d — should advance"):format(why, name, target)
    end
  elseif expected.kind == "NavField" then
    local target = expected.field
    return ("%s · click + once on %s"):format(why, field_label(target))
  elseif expected.kind == "CommitAllocate" then
    return ("%s · Enter (lock plan, open path stage)"):format(why)
  elseif expected.kind == "PathAppend" then
    return ("%s · append %s to path draft"):format(why, expected.action or "action")
  elseif expected.kind == "CommitPath" then
    return ("%s · Enter / Commit Path"):format(why)
  elseif expected.kind == "CommitVolley" then
    return ("%s · R / Ready (submit volley)"):format(why)
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
    return ("%s · click target shield face %d"):format(why, expected.facing)
  elseif expected.kind == "FireWeapon" then
    return ("%s · Enter (queue shot)"):format(why)
  elseif expected.kind == "TabWeapon" then
    return ("%s · ↓ (next weapon)"):format(why)
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
  -- snap.is_over() equivalent: status is Won/Lost. A nil status means the
  -- game is still in progress (mirrors the TUI's is_over() == false).
  local status = snap.status
  local over = (status == "Won" or status == "Lost")
  if over and step.expected.kind ~= "Dismiss" then
    if status == "Won" then
      return nil
    end
    return ("Game ended unexpectedly: %s"):format(tostring(status))
  end
  return nil
end

return tutorial
