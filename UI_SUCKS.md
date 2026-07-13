# UI Sucks - User Confusion Log

## Game: shipsim REPL (scenarios/ai.toml)

### Round 4: After Fixes Applied ✓

**MAJOR FIXES CONFIRMED**:
1. ✓ **Engine terminology fixed** - Now clearly shows "engine=8 power → thrust=8" 
2. ✓ **Thrust auto-spend clarified** - No longer asks for "thrust N" input; auto-spent by coast/accel/etc
3. ✓ **To-hit info now displayed!** - Shows "to-hit d20≤7 (35%), damage≈3" before firing
4. ✓ **Weapon menu clearer** - Now shows "Enter weapon number (0), weapon name (b1), or -1 when done:" (accepts both formats)
5. ✓ **Weapon menu notation fixed** - Changed from confusing "(0-0 or -1)" to clear "(0), weapon name (b1), or -1"

**Game is now much more playable!**

### Issue 1: Firing Phase Command Format ✓ FIXED
- **Status**: ✓ RESOLVED - Weapon menu now shows "Enter weapon number (0), weapon name (b1), or -1 when done:"
- **Improvement**: Now accepts both numeric and name-based weapon input

### Issue 3: Confusing Phase Transitions
- **What I tried**: `f` command in firing phase
- **What happened**: Enters weapon selection menu, but the UI doesn't clearly say "you're now selecting weapons to fire"
- **The confusion**: After firing, I got sent back to movement phase without clear indication that the fire action was committed
- **Expected**: Clear message showing which turn phase I'm in and what action just completed

### Issue 4: Movement Commands Are Clear ✓
- **Status**: ✓ GOOD - "choose one: coast | accel [course] | decel | course port/starboard | rotate port/starboard" is clear
- **Added value discovered**: Prompts also show "after this maneuver: fire window → next cycle (or end turn after 4/4)" which explains sequence

### Issue 5: Weapon Charge/Ready Status
- **The confusion**: Weapons show "[##..] CHG 2/4" or similar, but it's not obvious what "CHG" means
- **Expected**: Full spelling like "CHARGING" or clear indicator that 2/4 means "2 out of 4 power allocated" vs "ready after 2 more turns"

### Issue 6: Shield Direction Abbreviations
- **Status**: Pretty clear once you figure out "0:F = shield 0, Forward-facing" but...
- **The confusion**: On first glance, "0:F" looks like a ratio or coordinate, not a shield designation
- **Related**: The legend says "0:F=forward=ship's facing arrow" but that's buried in small text

## Turn 1 Summary (What Worked)
1. `engine 6` - Allocated 6 power to engine ✓
2. `w b1 2` - Allocated 2 power to beam_1 weapon ✓
3. `sh 0 3` - Allocated 3 power to shield 0:F ✓
4. `commit` - Committed allocation ✓
5. `accel 0` - Accelerated in direction 0 ✓
6. `commit` - Committed movement ✓
7. Firing phase - Got to weapon menu but couldn't figure out selection syntax

### Issue 7: ✓ REMOVED - False Issue
- **What I thought**: Movement commands weren't persisting across cycles
- **What actually happened**: I was misunderstanding the turn structure (4 movement/fire cycles)
- **Resolution**: The UI now clearly shows "movement/fire cycle N/4" which makes the structure obvious

## Turn 1 Summary (Actual Play)
1. `engine 6` → engine set to 6 ✓
2. `w b1 2` → beam_1 power set to 2 ✓
3. `sh 0 3` → shield 0:F power set to 3 ✓
4. `commit` → allocation committed ✓
5. `accel 0` → accelerated in direction 0 ✓
6. `commit` → movement committed ✓
7. `f` → entered firing mode
8. `0` → selected weapon 0 (beam_1) (after some parsing errors)
9. Targeting was auto-selected to B2 Escort
10. `r` → marked ready to fire
11. Turn 1 resolved:
    - My beam hit but **MISSED** (why? insufficient power? accuracy mechanic?)
    - Enemy beam hit me for 1 damage (shields absorbed damage and depleted)
    - Moved to turn 2 allocate phase
    - **BUG**: Tried to continue movement but got empty draft error and reset

### Issue 8: d20 Roll Display ⭐ PARTIALLY FIXED
- **What was broken**: No info about to-hit chance or d20 mechanics shown to player
- **What's fixed**: ✓ Now shows "to-hit d20≤7 (35%), damage≈3" when selecting target!
  - Players can now see what they need to roll to hit
  - Estimated damage is shown upfront
  - Much better transparency than before
- **What still needs work**: 
  - Fire resolution still doesn't show the actual d20 roll
  - Currently shows: "FIRED MISS dmg=0 sh=0"
  - Should show: "FIRED MISS [rolled 9]" or similar
  - Helps players understand if they lost to RNG vs poor tactics

### FIXED: Turn Progression Confusion
- **Initial observation**: Thought turns were resetting when they weren't
- **Actual mechanic**: After firing phase, return to movement phase (movement/fire cycle)
- **Root cause of confusion**: Didn't understand there are 4 movement/fire cycles per turn
- **Resolution**: Clarified by reading docs/PLAY-V2.md

## Gameplay Sessions

### Round 1 (Initial exploration)
- Got confused by d20 mechanics being hidden
- Assumed game was broken when shots missed
- Discovered turn structure through reading docs, not UI

### Round 2 (Refined understanding) 
- Learned d20 to-hit is range-dependent but invisible
- Discovered terminology "movement/fire cycle N/4" is actually clear
- Found weapon status shows "(ready to fire)" which is helpful

### Round 3 (Testing weapons and UI clarity)
- Attempted torpedo and beam weapons
- Confirmed some UI messages are good ("phase complete: firing → movement")
- Discovered weapon selection menu has input parsing issues
- Current state: Still haven't successfully completed a full turn due to input validation

### Issue 9: Command Context Validation is Confusing
- **What I tried**: After entering firing phase and firing, I continued typing movement commands like `coast` and `commit`
- **What happened**: Got errors "unknown command 'commit'. Did you mean 'coast'?" and "coast only in movement phase (now firing)"
- **The confusion**: 
  - The error message for "commit" suggests "coast" as an alternative, but coast is ALSO invalid in firing phase
  - Error messages don't clearly state "type 'r' for ready" or "type 'nofire' to skip"
  - It's not clear what the valid commands are in each phase
- **Expected**: Clear error messages like "Invalid command in firing phase. Use: f (fire), r (ready), nofire (skip)"

### Issue 10: Turn Progression - ACTUALLY CLEAR NOW ✓
- **REVISION**: Prompts show "movement/fire cycle 1/4" through "4/4" which is explicit
- **Good messaging**: "after this maneuver: fire window → next cycle (or end turn after 4/4)"
- **Shows phases**: "phase complete: firing → movement" marks transitions
- **Still could improve**: No explicit "END OF TURN 1 / START OF TURN 2" message between allocate phases

## Gameplay Progress Summary
### Turn 1 Final State
- My ships position: @(0,4) facing right, moved closer to enemy
- Enemy: @(7,4) facing left (range reduced from 8 to 7)
- My beam_1 fired with 3 power allocated → **MISSED**
- Enemy beam fired → **HIT** (I took 2 hull damage, hull 12→10)
- Movement completed through 4 movement phases
- **MAJOR DISCOVERY**: Even with weapon power allocated, beams appear to have very low hit chance
  - My: 0% success rate so far (0/2 shots)
  - Enemy: 100% hit rate (2/2 shots)
  - This is either a critical game balance issue or I'm fundamentally misunderstanding the mechanics

## CRITICAL ISSUES FOR DEV TEAM (After Round 4 Fixes)

### ✓ FIXED Issues (User Applied)
1. ✓ **Engine terminology** - Changed "mov" → "engine" with "(→ thrust for movement phase)" notation
2. ✓ **Thrust input error** - Fixed confusing "thrust=N" command that looked like valid input
3. ✓ **To-hit transparency** - NOW SHOWS: "to-hit d20≤7 (35%), damage≈3" before firing! ⭐⭐⭐
4. ✓ **Weapon menu format** - Changed "(0-0 or -1)" to "weapon number (0), weapon name (b1), or -1"
5. ✓ **Weapon menu accepts both formats** - Can input "0" or "b1" (numeric or name)

### Remaining Issues

**HIGH PRIORITY:**
1. **d20 roll not shown in outcome** - Still just says "FIRED MISS" without showing the roll
   - Currently: "FIRED MISS dmg=0 sh=0"
   - Should be: "FIRED MISS [rolled 9 vs needed ≤7] dmg=0 sh=0"
   - Helps players understand whether they got unlucky or made a tactical mistake

2. **Damage estimation timing** - To-hit info shown at target selection, but not again before committing
   - Good: Shows when selecting target
   - Could be better: Show again when reviewing queued shots in PENDING FIRE panel

**MEDIUM PRIORITY:**
3. **CHG abbreviation** - Weapons show "CHARGED 3/4" but still a bit cryptic
   - Consider: "READY 3/4 (need 4)" to make max requirement clearer
   
4. **No d20 roll randomness feedback** - Players see high % miss rate (rare) and think it's broken
   - Solution: Show roll result in output so they understand it's luck, not design

### Issue 11: Turn End Mechanics Confusing
- **What I tried**: Used `e` (end turn) command thinking it would exit firing phase
- **What happened**: Got a warning saying "end_turn ends the WHOLE turn, not the fire phase" and asked me to confirm
- **The confusion**: 
  - User should use `coast` in movement, `ready`/`nofire` in firing
  - `e` for "end" is too ambiguous (E for Exit? End? Either?)
  - Not obvious that there are more movement phases after firing phase
- **Expected**: Help text should say "Use 'coast' to finish movement phases, 'ready'/'nofire' to finish firing, or 'e' to end whole turn"
- **GOOD NEWS**: The confirmation dialog is actually helpful UX!

### Issue 12: Weapon Selection Menu ✓ FIXED
- **What was broken**: Menu showed "(0-0 or -1)" which was confusing notation
- **What's fixed**: ✓ Now shows "Enter weapon number (0), weapon name (b1), or -1 when done:"
- **Improvement**: Clear format + accepts both numeric and name-based input
- **Note**: Previous parsing issues may have been related to command batching in my tests

### REMOVED: Issue about Phase Loop (Clarified)
- **Resolution**: The UI now clearly shows "movement/fire cycle N/4" which is explicit and understandable
- **Good element**: Prompts explain what happens next after each maneuver

## What Actually Works Well ✓ (Round 4 Update)
✓ **Engine terminology**: Now shows "engine=8 power → thrust=8" (much clearer!)
✓ **Movement/fire cycle messaging**: "movement/fire cycle N/4" is explicit and clear
✓ **Phase transitions**: "phase complete: X → Y" clearly shows what's happening
✓ **Next-action hints**: "after this maneuver: fire window → next cycle" explains sequence
✓ **To-hit information**: ⭐ NOW SHOWS "to-hit d20≤7 (35%), damage≈3" before firing
✓ **Weapon menu clarity**: Now accepts both numeric (0) and name-based (b1) input
✓ **Weapon readiness indicator**: "(ready to fire)" shown next to weapon charge
✓ **Weapon status**: "QUEUED (see PENDING FIRE)" is clearer than before
✓ **Ship status display**: Hull bars, shield faces, weapon status all readable
✓ **Shield allocation**: By shield number (0-5), syntax is learnable
✓ **Movement commands**: Clear menu with all valid options listed
✓ **Map display**: Clear positions, facings (arrows), range info
✓ **End-turn confirmation**: Good UX that prevents accidents
✓ **Allocation confirmation**: Shows exactly what was allocated

## What Still Needs Work ✗
✗ **d20 roll result not shown**: Fire outcome says "FIRED MISS" but doesn't show roll value
   - Players can't see if they lost to RNG or made a tactical mistake
   - Suggested: "FIRED MISS [rolled 9]" format
✗ **CHG abbreviation**: "CHARGED 3/4" is a bit cryptic for new players
✗ **Damage shown pre-fire only**: To-hit/damage info shown at target select but not in PENDING FIRE review
✗ **No turn separators**: No explicit message between Turn 1→2 in allocate phase

---

## Round 5: COLD PLAY WITHOUT READING DIRECTIONS (2026-07-12)

**What I did**: Ran `python3 frontend/repl/repl.py scenarios/ai.toml` and tried to play without reading docs or help text

**Initial observation**: Game started in allocate phase with clear state display. Ship A1 vs Escort B2 at range 8.

### Successful Actions
✓ `engine 8` - allocated 8 power to engine
✓ `w b1 4` - allocated 4 power to beam_1
✓ `sh 0 4` - allocated 4 power to shield facing 0:F
✓ `commit` - allocation committed successfully
✓ `coast` - moved through all 4 movement phases successfully
✓ Movement cycles showed clear "movement/fire cycle N/4" messaging

### Issue 13: Fire Phase Command Sequencing (FRICTION)
- **What I tried**: `f` then `0` then `2` then `ready`
- **What happened**:
  1. Typed `f` → weapon picker opened: "Enter weapon number (0), weapon name (b1), or -1 when done:"
  2. Typed `0` → weapon selected, target auto-selected to B2, shield facing auto-selected
  3. Message: "A1 shot queued — resolves once every ship readies"
  4. Typed `2` → **ERROR**: "focus → B2 Escort (ai) (observer: cannot order)"
  5. System switched focus to ship #2 (enemy), next input `ready` gave: "cannot ready ship #2: not a living player ship"
- **Root cause**: After weapon selection completes, bare numbers are interpreted as ship focus commands, not weapon/target input
- **Expected behavior**: After typing `0` to select weapon, system should either:
  - Wait for more target info (not auto-complete), or
  - Reject invalid input gracefully instead of interpreting `2` as a focus command
- **Discovery**: The AUDIT.md mentions "fire b1 #2" syntax for direct fire, but I wasn't aware that was available during weapon selection
- **Confusion level**: HIGH - spent 5+ inputs trying to figure out valid commands after weapon selection

### Issue 14: Weapon Selection Menu Doesn't Close Clearly (UX)
- **Observation**: After selecting weapon `0` and seeing "shot queued", the prompt line says:
  ```
  firing cycle 4/4: focus=#1  weapon menu opens if you have charge; f again for more shots | r/nofire when done | ready=[2]
  ```
- **The confusion**:
  - Does "weapon menu opens if you have charge" mean I can type `f` again? ✓ (yes, apparently)
  - Does "r/nofire when done" mean I should type `r` to fire? ✓ (yes, means "ready to fire")
  - What does "ready=[2]" mean? (appears to be metadata about which ships are ready, not a command)
- **Missing**: Clear message like "✓ Shot queued. Type 'r' to ready fire, or 'f' to queue another shot"
- **Current message is accurate but dense** - takes effort to parse

### Issue 15: No Explicit Weapon Selection Confirmation (UX)
- **What I expected**: After typing `0`, see a confirmation like: "✓ Selected weapon 0: beam_1 (4/4 power). Target B2 Escort @(6,4) to-hit d20≤10 (50%), damage≈5. Continue?"
- **What I got**: Weapon queued message appears in the RECENT section, but not immediately after selection
- **Result**: Had to look at "PENDING FIRE" panel to confirm shot was queued
- **Good news**: The PENDING FIRE panel does show the shot details, so the data is there

### Observations (POSITIVE)
✓ Auto-target selection is helpful (no need to manually specify B2 when it's the only valid target)
✓ Auto-shield-facing selection is correct (shields facing the player are automatically identified)
✓ To-hit information ("d20≤10 (50%), damage≈5") is shown before queuing
✓ Enemy shots are clearly displayed in results ("A1 beam_1 → C2 HIT for 8 on shield 0:F")
✓ Map shows positions and range clearly
✓ Ship status panel is well-formatted and readable

### What Wasn't Confusing
- Allocate phase command syntax (engine N, w [weapon] N, sh [face] N all work as expected)
- Movement phase options (coast, accel, etc. are clear)
- Visual feedback (firing shows "FIRED MISS" with clear results)
- Phase transitions (messages like "phase complete: allocate → movement" are explicit)

---

## Round 2 Summary & Lessons Learned

### What I Learned from Reading the Docs (instead of UI)
1. **d20 mechanics exist but are invisible in-game**
   - Had to read combat-v2-tables.md to understand why my 35% accuracy shots were failing
   - The UI just says "MISS" with no explanation
   
2. **Turn structure with 4 movement/fire cycles is not explained in REPL**
   - Assumed single move/fire per turn until I read PLAY-V2.md
   - Getting "movement 2/4" after firing was confusing without context
   
3. **Command semantics by phase require trial and error**
   - `e` can end whole turn (requires confirmation) - good UX
   - `r` / `nofire` / `done` finish firing phase - not documented in hints
   - `coast` / `p` / `accel` are valid in movement but not firing
   
4. **Beam power allocation is about damage, not accuracy**
   - My earlier assumption was wrong
   - High range + low charge = low damage, but not lower accuracy
   - UI should show estimated damage range before committing fire

### Gameplay Success Rate
- **Firing**: 0/4 shots landed (25% expected with 35% to-hit... just unlucky)
- **Damage taken**: 1-2 per turn consistently
- **Current hull status**: 11/12 (one successful hit from enemy despite same 35% accuracy)

### File Impact
- Created: UI_SUCKS.md documenting all UI findings across 3 gameplay sessions

## Final Verdict After 4 Rounds

**The Good** ⭐⭐⭐: 
- Phase messaging is crystal clear ("movement/fire cycle N/4")
- To-hit information now displayed before firing! ("d20≤7 (35%), damage≈3")
- Weapon menu accepts both numeric and name-based input
- Engine terminology fixed (mov → engine)
- Thrust confusion resolved
- Overall game is now quite playable

**The Minor Issues**:
- d20 roll result not shown in outcome (just "MISS" without roll value)
- "CHG" abbreviation could be clearer
- No explicit turn separators between allocate phases
- Damage estimate not shown again in PENDING FIRE review

**What Works**: 
- Allocation phase messaging
- Movement phase options
- Firing phase target selection with to-hit info
- Ship status and map displays
- End-turn confirmation dialog

**UI Quality**: **85%+ polished**. Most critical issues resolved. Game is now genuinely playable without needing to read external docs for basic mechanics (except d20 roll results, which are implicit rather than hidden).

**Recommended Next Fixes** (nice-to-have):
1. Show d20 roll in outcome ("rolled 8 vs needed ≤7")
2. Add turn separators in allocate phase
3. Consider "CHARGED 3/4 [max 4]" format instead of just "CHG"
4. Repeat damage estimate in PENDING FIRE panel for review
