# shipsim

shipsim models deterministic starship matches whose sides may be directed by
people, built-in policies, or external model-backed participants.

## Match participation

**Match**:
A single contest between two opposing sides under one scenario and ruleset.
_Avoid_: Session, game type

**Side**:
One of the two opposing fleets in a match.
_Avoid_: Player, controller

**Seat**:
The right to submit orders for one side in a match.
_Avoid_: Connection, client slot

**Controller**:
The participant responsible for the decisions submitted through a seat.
_Avoid_: Side, player type

**Human controller**:
A person directing a side through any compatible client.
_Avoid_: Player controller

**Bot controller**:
A built-in, seeded policy that directs a side without a person or language
model.
_Avoid_: AI, NPC, algorithm

**LLM agent controller**:
An external model-backed participant that directs a side through the same
contract as any other controller.
_Avoid_: AI, bot

**Lobby**:
The pre-match state in which a scenario and seat controllers are selected and
required participants become ready.
_Avoid_: Game, waiting room

**Match host**:
The human controller permitted to configure an unconfigured lobby.
_Avoid_: Server, side A
