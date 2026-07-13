# Game UI Issues Found During Testing

## Issue 1: Invalid Command "play"
- **Phase**: movement/fire
- **Context**: After committing power allocation, game enters movement phase
- **Problem**: Trying to use "play" command returns "unknown command 'play'. Type help for commands; try status, move, fire, or quit."
- **Impact**: No obvious way to auto-advance through movement cycles or automatically proceed to fire phase
- **Severity**: Medium - prevents smooth flow through game

## Issue 2: Shield Allocation Context Confusion
- **Phase**: allocate (weapons subsection)
- **Context**: After entering weapons allocation with "w" command
- **Problem**: Trying to type "sh" to move to shields group results in error: "need a weapon id first (e.g. b1 2), not a bare number"
- **Expected**: Should recognize "sh" as a navigation command to switch to shields allocation
- **Actual**: Interprets "sh" as an incomplete weapon allocation command
- **Impact**: Confusing UX - not clear how to exit weapons subsection to allocate shields
- **Severity**: High - core allocation workflow is blocked/unclear

## Issue 3: Bare Number Handling in Groups
- **Phase**: allocate (various subsections)
- **Context**: When in a subsection (weapons, shields), navigation between groups is unclear
- **Problem**: Instructions say "done leaves group" but unclear how to invoke this, and bare numbers are rejected
- **Expected**: Simple commands like "done" or even just hitting enter should exit current group
- **Actual**: Bare numbers give error "need a weapon id first (e.g. b1 2), not a bare number"
- **Severity**: Medium - prevents natural command flow

## Testing Environment
- Scenario: scenarios/ai.toml
- Commands attempted: engine 6 → w → b1 2 → sh → 0 3 → commit
- Game reached: Movement phase after successful power allocation

