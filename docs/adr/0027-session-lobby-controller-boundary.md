# ADR-0027: Separate session lobbies from game rules and controller execution

Status: Accepted
Date: 2026-08-16

## Context

The game protocol v4 assumes an already loaded match. Network clients need a
pre-match contract for version negotiation, scenario selection, seat
assignment, invitations, controller selection, and waiting state. Folding
those concerns into protocol v4 would mix match orchestration with combat
rules and make local subprocess clients depend on a network lobby.

Controller identity also cannot be inferred from side, socket order, or a
ship's presentation `controller` label. A side may be directed by a person, a
server-owned deterministic policy, or an external model-backed participant.
Only the assigned seat determines who may submit orders for that side.

Model-backed participants introduce a credential boundary. Provider keys,
provider authorization, prompts, and conversation history are not game state
and must not enter the engine, session server, lobby messages, snapshots,
saves, or ordinary UI logs.

## Decision

Define session protocol v1 as a separate NDJSON framing contract identified by
`session_protocol_version: 1`. Game orders, previews, snapshots, and saves keep
`protocol_version: 4`. After a seat is assigned and a match starts, game-v4
messages remain unwrapped on the same connection.

Use the domain terms in root `CONTEXT.md`: a Match has sides A and B; a Seat is
the authority to submit orders for one side; a Controller directs a seat; and
the Lobby is pre-match state. Controller wire values are exactly:

- `human` — a Human controller using any compatible client;
- `bot` — a server-owned Bot controller with a stable policy identifier;
- `llm_agent` — an external LLM agent controller using the same seat contract.

The first client that successfully negotiates versions in an unconfigured
lobby receives Match host permission. This permission allows lobby
configuration; it does not itself grant side A or ownership of any ships.
Authorization after assignment is always based on the connection's assigned
Side/Seat.

Bot policies execute under the session server's ownership but produce normal
game-v4 orders that pass through the engine's ordinary validation path. LLM
agents remain external API clients. The agent process owns its provider
profile, credentials, prompts, conversation, retries, and provider calls. The
lobby records only the controller kind; it has no credential, provider URL,
authorization-header, or prompt fields.

Keep the TUI's existing local subprocess mode. Network play is additive. Local
mode remains the lowest-friction single-player and development path, while the
session protocol supplies multiplayer and external-controller orchestration.

Scenario selection uses a server-owned catalog. Clients submit stable catalog
identifiers and receive only identifiers and display names, never filesystem
paths. The server resolves enabled entries beneath its configured scenario
directory and rejects unsafe, unknown, disabled, or escaping entries.

## Consequences

- Lobby evolution can version independently of combat and save compatibility.
- REPL, TUI, Love2D, bots, and LLM agents can eventually occupy seats through
  the same public game API without controller-specific combat rules.
- A host is a permission-bearing participant, not a privileged side.
- Provider credentials cannot be supplied through `create_match`; deployment
  must configure them in the external agent process.
- Session servers must project state and errors per assigned side.
- Local TUI behavior does not need a lobby server and remains backward
  compatible.

## Rejected alternatives

- **Extend `protocol_version: 4` with lobby messages:** couples orchestration
  changes to combat clients and saves.
- **Treat side B as the bot:** conflates team identity with execution and
  prevents symmetric human play.
- **Run LLM providers in `shipsim-session`:** gives the authoritative server
  unnecessary credential and conversation ownership.
- **Replace local TUI mode with TCP:** adds setup and failure modes to existing
  single-player play without an engine benefit.

## Contract

The normative message shapes, state sequence, stable error codes, and traces
are in `docs/PROTOCOL.md`. Canonical examples are locked under
`tests/fixtures/session-v1/`. Phase 1 freezes this contract but intentionally
does not change `shipsim-session` or TUI runtime behavior.
