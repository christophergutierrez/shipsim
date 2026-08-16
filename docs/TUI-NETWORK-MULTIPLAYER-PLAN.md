# TUI network multiplayer and LLM-agent implementation plan

Status: proposed  
Scope: six implementation phases; no REPL or Love2D network adapter in this plan  
Primary scenario: `scenarios/shipyard_assault.toml`

## Outcome

Turn `shipsim-session` into a one-match lobby server and make the TUI a real
network client. A person can connect a TUI, choose a scenario, and play against:

1. a selectable built-in bot;
2. another human using a second TUI; or
3. an external LLM agent using either a local OpenAI-compatible endpoint or
   Fireworks AI.

Every participant must submit ordinary engine orders. The server remains the
only authority for rules, legality, hidden staged decisions, and victory.
Credentials and model conversations never enter `shipsim_core`, the session
wire protocol, snapshots, saves, or TUI logs.

Implementation is complete after all automated phase gates pass. Release
acceptance follows with the manual TUI playtests for phases 4, 5, and 6 near the
end of this document.

## Locked decisions

These decisions make the plan executable without another design round:

- A match always has sides A and B. “One player” means one human-controlled
  seat; it is not a different combat mode.
- Controller vocabulary is **Human**, **Bot**, and **LLM agent**. Do not use
  `AI` as a protocol value because the current code uses it ambiguously.
- The first version-negotiated TUI in an unconfigured lobby is the **match host**.
  Hosting is a permission, not ownership of side A.
- The host selects the scenario and side-B opponent. The initial TUI occupies
  side A as a Human controller for this slice; arbitrary controller-vs-controller
  lobby setup remains a compatible future extension.
- A Human side B waits for a second client. A Bot side B is filled in-process by
  the session server. An LLM-agent side B waits for, or is filled by, an
  external `shipsim-agent` process.
- The server runs one match and exits after the match or a terminal disconnect.
  Multiple lobbies, discovery, matchmaking, spectators, reconnection, and
  persistence across server restarts are deferred.
- Session/lobby framing has its own `session_protocol_version = 1`. Combat
  remains game protocol v4. Do not overload `protocol_version` with lobby
  semantics.
- Once a seat is assigned, unwrapped protocol-v4 orders and preview requests
  continue over the same NDJSON connection. Session messages always carry
  `session_protocol_version`.
- The TUI retains its existing local-subprocess mode. Network play is additive
  through `--connect HOST:PORT`.
- Built-in bots generate normal `Order` values and pass through
  `apply_order`; they receive no privileged mutation path.
- LLM agents are API clients. The model provider adapter lives outside the
  engine and session server.
- The agent keeps its conversation locally for one match. It uses bounded
  history and stateless chat-completion requests; no provider-hosted
  conversation persistence is required.
- Provider keys are named indirectly by profile fields such as
  `api_key_env = "FIREWORKS_API_KEY"`. Raw keys are never accepted in lobby
  messages, command-line arguments, checked-in files, or logs.
- Blocking play remains the initial policy. The UI must show which seat or
  phase commitment is pending so waiting is legible rather than appearing
  hung.

## Existing seams and known blockers

- `shipsim-session` selects a scenario on its command line, accepts exactly
  two sockets, and sends no lobby messages.
- It does not route `path_preview`, `fire_preview`, or other read-only
  requests, so a thin network UI cannot currently use the full v4 contract.
- The TUI `Harness` only owns subprocess pipes and performs blocking reads.
- TUI ownership, target selection, pending-ship selection, map colors, and
  endgame statistics frequently infer allegiance from
  `controller == "player"`. Network play must use the viewer’s assigned
  `SideId`.
- The live NPC implementation has Greedy/Scripted labels. Simulation already
  has Random, Greedy, Aggressive, Defensive, and Mobility policies, but those
  policies also contain controller-based opponent assumptions.
- Purchased ships must inherit their seat’s controller behavior without
  encoding “side B means bot.”

## Target architecture

```text
TUI host ───────────────┐
second TUI ─────────────┤
shipsim-agent ──────────┼── shipsim-session ── shipsim_core::GameState
server-owned bot policy ┘          │
                                   └── side-relative snapshots and previews

shipsim-agent ── provider profile ── local endpoint or Fireworks
```

The session server owns lobby state, seat authorization, controller scheduling,
and the authoritative `GameState`. The TUI owns presentation and input. The
agent owns prompts, conversation history, provider calls, credentials, retries,
and conversion of model output into ordinary orders.

## Session protocol v1

The exact Rust/Python shapes may evolve during phase 1, but these capabilities
and transitions are required:

```text
Disconnected
  → Connected
  → HelloNegotiated
  → LobbyUnconfigured
  → LobbyConfigured
  → WaitingForSeats
  → Running
  → Finished
```

Required client messages:

- `hello`: client kind, display name, supported session versions, supported
  game-protocol versions.
- `create_match`: scenario identifier and controller specification for each
  seat.
- `join_match`: one-time seat token and display name.
- `participant_status`: `ready`, `thinking`, or `error`; advisory only
  and never an order.
- Ordinary protocol-v4 orders and preview requests after assignment.

Required server messages:

- `welcome`: negotiated versions, connection identity, server capabilities,
  and whether the client may configure the lobby.
- `scenario_catalog`: server-controlled scenario identifiers and display
  names; clients never submit arbitrary filesystem paths.
- `lobby_state`: configuration, seat occupancy/readiness, bot policy choices,
  and actionable waiting reason.
- `seat_assigned`: side, match id, and viewer identity.
- `seat_invitation`: a redacted display code plus a one-time join token. The
  full token is shown only to the host and is not written to ordinary logs.
- Side-relative game snapshots, preview responses, and private errors.

Required error codes include:

- `unsupported_session_version`
- `unsupported_game_protocol`
- `host_required`
- `lobby_already_configured`
- `unknown_scenario`
- `unknown_controller`
- `unknown_bot_policy`
- `seat_not_ready`
- `invalid_join_token`
- `seat_already_occupied`
- `ownership`
- `request_rejected`

## Cross-phase invariants

Every phase must preserve these:

1. Only `shipsim_core` validates and mutates combat state.
2. Authorization is by assigned seat/side, never connection order after lobby
   configuration and never the ship’s presentation `controller` label.
3. A client receives only its own private errors and side-relative terminal
   status.
4. Opponent staged allocation, path, and volley payloads remain hidden.
5. Preview requests are read-only, side-authorized, and answered only to the
   requester.
6. A purchased ship is controlled by the controller assigned to its side.
7. Bot and LLM orders pass through exactly the same validation path as human
   orders.
8. Match seed plus bot policy completely determines bot decisions.
9. No provider credential appears in process arguments, protocol messages,
   snapshots, save files, committed fixtures, or logs.
10. Existing local TUI, REPL, Love2D, simulation, save/replay, and yard behavior
    remain green.

## Phase 1 — Domain model and session protocol

### Goal

Freeze the language and wire contract before changing server or TUI behavior.

### Deliverables

- Keep root `CONTEXT.md` as the implementation-neutral glossary for Match,
  Side, Seat, Controller, Human controller, Bot controller, LLM agent
  controller, Lobby, and Match host.
- Add an ADR for the lobby/controller boundary. It must record:
  - session protocol separate from game protocol;
  - first-client host permission;
  - side-based authorization;
  - server-owned bots versus external LLM agents;
  - credential ownership by the agent process;
  - why the TUI keeps local mode.
- Add `src/session_protocol.rs` containing serialized session-v1 messages,
  controller specifications, lobby state, stable error codes, and version
  negotiation.
- Add `data/scenario_catalog.toml` as the server-owned allowlist of stable
  identifiers, display names, paths, and network-enabled flags. Add a
  scenario-catalog projection from that file. Reject `..`, absolute paths,
  slashes, unknown IDs, disabled entries, and symlink escape.
- Document session v1 in `docs/PROTOCOL.md`, including complete example traces
  for Human-vs-Bot, Human-vs-Human, and Human-vs-LLM-agent lobbies.
- Add golden NDJSON fixtures under `tests/fixtures/session-v1/`.

### Automated tests

- Round-trip every session message and reject unknown/unsupported versions.
- Golden fixtures deserialize and reserialize byte-identically.
- Scenario identifiers reject traversal and cannot escape the configured
  scenario directory.
- A `create_match` message cannot contain a model API key, provider
  authorization header, arbitrary provider URL, or prompt.
- State-machine table tests cover every allowed transition and reject every
  out-of-order message without mutation.
- Existing protocol-v4 fixtures remain byte-identical.

### Exit gate

- The ADR, glossary, protocol document, and fixture examples use the same terms
  and enum values.
- `cargo test --all-targets` and root clippy pass.
- No server or TUI behavior changes in this phase.
- Commit: `docs(protocol): define session lobby and controller model`.

## Phase 2 — One-match lobby server

### Goal

Make `shipsim-session` start empty, negotiate clients, configure one match,
assign seats, and route the complete game API.

### Deliverables

- Refactor the binary into testable server components instead of placing the
  accept loop and state machine in `main`.
- Default launch:

  ```bash
  cargo run --bin shipsim-session -- --listen 127.0.0.1:4100
  ```

- Keep `--scenario ID` only as a documented preconfigured-lobby convenience;
  it must still use session-v1 hello and seat assignment.
- Bind tests to `127.0.0.1:0`; report the selected address through a testable
  startup channel.
- Negotiate session/game versions before accepting configuration or orders.
- Grant match-host permission to the first negotiated client in an
  unconfigured lobby.
- Load `GameState` only after a valid `create_match`.
- Generate cryptographically unpredictable, single-use seat tokens from the OS
  random source. Store only a non-reversible token digest server-side.
- Start immediately when all required external seats are ready. Bot seats do
  not consume connections.
- Replace writer-index assumptions with connection/seat records.
- Extract protocol-v4 read-only request dispatch from `src/bin/shipsim.rs`
  into a reusable engine adapter, then route network previews through that
  exact implementation. Do not copy preview logic into the session server. A
  request response is private.
- Broadcast accepted-order snapshots through
  `StateSnapshot::from_game_state_for_side`.
- Keep errors private and preserve the intentional no-timeout policy.
- Broadcast explicit waiting state: pending seat before play, then pending
  side/ship commitments during each collection stage.
- Define disconnect behavior:
  - before start: release the seat; host disconnect cancels the lobby;
  - during play: end the match with `participant_disconnected`;
  - after finish: close cleanly after final snapshots.
- Shut down reader threads and sockets deterministically; no orphan host
  process after tests.

### Automated tests

Use real localhost sockets, fragmented writes, and multiple messages in one
packet:

- First compatible client becomes host and receives the scenario catalog.
- Non-host cannot create or reconfigure a match.
- Human-vs-Bot requires one external seat; Human-vs-Human and
  Human-vs-LLM-agent require two.
- A valid token assigns the intended side exactly once; wrong, reused, or
  already-occupied tokens fail privately.
- Third-seat connection cannot observe a game snapshot.
- Side A and side B can submit the same order kinds for their own ships and are
  rejected for the other side.
- `purchase` and `purchase_custom` use connection ownership.
- `path_preview` and `fire_preview` succeed over TCP, do not mutate state,
  and do not leak to the opponent.
- A slow participant leaves the server alive and produces a visible waiting
  state; no test relies on an unbounded wait.
- Disconnect tests terminate within a bounded duration and leave no worker
  thread running.
- Mutual destruction reports `Draw` to both seats.

### Exit gate

- A headless integration test configures a match, connects required seats,
  advances one zero-action turn, and exits the server cleanly.
- Invalid JSON, invalid lobby messages, rejected orders, and disconnects never
  panic or kill an otherwise viable lobby.
- Full root suites and clippy pass.
- Commit: `feat(session): add negotiated one-match lobby`.

## Phase 3 — TUI TCP transport and lobby UX

### Goal

Let a person connect through the TUI, configure or join a lobby, and reach the
ordinary battle screen without typing JSON.

### Deliverables

- Introduce a TUI transport interface shared by:
  - existing local subprocess transport;
  - new TCP session transport;
  - deterministic fake transport used by UI tests.
- Use a background reader and channel for network input. The ratatui event loop
  must continue rendering while the server or opponent is quiet.
- Add launch forms:

  ```bash
  # Existing local play remains valid
  cargo run --manifest-path frontend/tui/Cargo.toml -- scenarios/battle.toml

  # Connect to a waiting server
  cargo run --manifest-path frontend/tui/Cargo.toml -- --connect 127.0.0.1:4100
  ```

- Add TUI states and screens for:
  - connecting and negotiation;
  - host scenario selection;
  - opponent-controller selection;
  - bot-policy selection placeholder;
  - waiting for Human or LLM-agent seat;
  - joining with an address/token;
  - participant disconnect and session errors.
- Provide an in-TUI Join Match form and an automation-only
  `--join-token-stdin` path. Never place a full join token in process
  arguments. Mask it after entry and omit it from session logs.
- Store the assigned viewer side in `App`.
- Replace all gameplay ownership/enemy decisions based on
  `controller == "player"` with `ship.side == viewer_side`. Keep
  `controller` only for labels.
- Ensure side B gets the same allocation, movement, volley, purchase, focus,
  help, and game-over interactions as side A.
- Disable the local scripted-pump path in network mode; the server owns all
  non-human scheduling.
- Display “Waiting for side B / ship #N during <phase>” while blocked.
- Add actionable screens for refused connection, incompatible version, invalid
  token, server EOF, and server error.
- Update TUI README and root launch table.

### Automated tests

- Transport contract tests run the same send/read cases against local, fake,
  and TCP transports.
- TestBackend renders every lobby state at 80×24 and 120×40 without clipping.
- Keyboard-only tests configure each opponent type and complete the Join Match
  form.
- A side-B snapshot focuses side-B ships, targets side A, purchases for B, and
  renders B’s win as Victory.
- Static regression guard: gameplay ownership helpers contain no
  controller-string comparison. Explicit presentation-only uses are allowlisted.
- A silent fake server leaves the TUI responsive to help and quit.
- EOF and protocol incompatibility return the terminal to normal mode and show
  one actionable error.
- Session logs redact token-shaped and authorization-shaped values.

### Human-like automated gate

Add or extend a PTY driver under `tools/` so it can:

1. start a real session server on an ephemeral port;
2. launch the real TUI with `--connect`;
3. select a scenario and opponent using visible keys;
4. capture screens without ANSI fragments; and
5. terminate both processes cleanly.

The gate must assert visible selection, connection state, assigned side, and
the first battle screen. Direct NDJSON injection does not satisfy this gate.

### Exit gate

- Local subprocess TUI behavior remains unchanged.
- Network lobby can be reached and exited repeatedly without a terminal or
  process leak.
- All TUI, root, REPL, and Love2D regression suites pass.
- Commit: `feat(tui): connect to session lobbies`.

## Phase 4 — Human versus selectable bot

### Goal

Complete the first fully playable network mode with one TUI and no second
external client.

### Deliverables

- Move the reusable simulation `Policy` abstraction and baseline policies to
  an engine-owned module usable by both `shipsim-sim` and the session server.
- Make target selection side-based, not controller-label-based.
- Expose these stable policy IDs and labels:
  - `random` — Random;
  - `greedy` — Greedy;
  - `aggressive` — Aggressive;
  - `defensive` — Defensive;
  - `mobility` — Mobility.
- Session lobby advertises policies from the server; the TUI does not hard-code
  the authoritative list.
- The server schedules bot orders through `apply_order` whenever the bot seat
  has pending ships. It must handle newly purchased ships in the same stage.
- Give every bot policy an explicit, seeded purchase decision at the start of
  allocation. Track “purchases complete for this turn” separately from ship
  allocation so a bot cannot buy forever or accidentally skip a newly spawned
  ship.
- Seed bot randomness from match seed, side, and policy identifier so replay
  and simulation remain deterministic.
- Record bot orders in match trace/save data in the same order format as human
  orders.
- Show the selected bot and current waiting/acting state in the TUI.
- Add an in-match TUI purchase panel during allocation. It must show local
  credits, server-projected catalog classes/costs, affordability, spawn
  rejection, and the newly assigned ship id. The order’s side comes from the
  assigned viewer side, never a hard-coded `a`.
- A bot failure becomes a bounded private server error and clean match abort,
  never an infinite barrier wait.

### Automated tests

- Each policy emits legal allocations, paths, volleys, and purchases for both
  sides across all stock ship classes.
- Purchase strategies stop deterministically when unaffordable or blocked and
  allocate every successfully purchased ship in the current stage.
- Run 20 seeds for every ordered policy pairing (500 Shipyard Assault matches)
  with zero rejected policy orders and zero hangs. This is a
  legality/liveness gate, not a balance claim.
- The same seed and order stream produce byte-identical traces.
- A bot controls every ship bought for its side and never a human-side ship.
- One real-TCP test advances five turns with one human test driver and one bot.
- PTY test creates Human-vs-Greedy entirely through visible TUI controls,
  purchases a ship, and completes allocate → movement → volley.

### Exit gate

- One TUI can start and play Shipyard Assault against every advertised policy.
- No second socket is required for Bot matches.
- Simulation metrics remain reproducible after policy extraction.
- Full repository verification passes.
- Commit: `feat(session): play TUI matches against selectable bots`.

## Phase 5 — Human versus human TUI

### Goal

Allow two independent TUIs to control opposite sides through the same public
session API.

### Deliverables

- Host TUI creates Human-vs-Human and displays:
  - listen/connect address;
  - human-readable invitation code;
  - copyable full join token outside ordinary logs;
  - waiting/connected status.
- Second TUI joins from its start screen or with
  `--connect HOST:PORT`, then enters the invitation in the Join Match form.
- Both TUIs use identical gameplay code after seat assignment.
- Both sides may buy catalog and custom ships during allocation.
- Add an in-match custom-design picker that reuses the TUI yard’s saved design
  loader and sends the selected `Design` through `purchase_custom`. The
  server remains responsible for projection, cost, collision, spawn, and
  legality.
- Every collection stage clearly names local pending work and whether the
  opponent is still pending, without revealing opponent payloads.
- Rejected orders remain private. Accepted snapshots are projected for each
  viewer.
- Endgame shows Victory/Loss/Draw correctly on both clients.
- Define intentional behavior for mid-match quit: the remaining client sees
  “Opponent disconnected; match ended,” not a false victory.

### Automated tests

- Spawn the real server and two real TCP TUI transports; configure, join, and
  complete at least three turns.
- Exercise side B first in every phase to prove order arrival order is
  irrelevant.
- Both sides purchase a catalog ship; both new ships remain human-controlled.
- Side B submits a custom purchase through the visible TUI picker; side A
  cannot spend B’s credits or replace B’s order.
- Ownership, invalid-token, invalid-preview, and rejected-order errors are
  visible only to the responsible client.
- Before a barrier resolves, neither client snapshot contains the other side’s
  staged payload.
- Simultaneous shipyard destruction renders Draw in both TUIs.
- Abruptly drop each client in turn and assert bounded, clean shutdown.
- PTY dual-client smoke test reaches turn 2 using only keys shown on each
  screen.

### Exit gate

- Two TUI processes can play a complete match without raw JSON or direct engine
  access.
- Side A and side B pass the same capability matrix.
- Waiting states make a deliberately slow second player distinguishable from a
  hung client.
- Full repository verification passes.
- Commit: `feat(tui): support two-human network matches`.

## Phase 6 — External LLM agent, local endpoint, and Fireworks

### Goal

Let the TUI host a match against a model-backed external participant without
giving credentials to the engine, server, or TUI.

### Deliverables

- Add an isolated Python 3.11+ `frontend/agent/` client with its own README,
  tests, `.gitignore`, and executable `frontend/agent/shipsim-agent`.
  Use the standard library (`socket`, `urllib.request`, and `tomllib`) so
  the default client adds no provider SDK or credential-handling dependency.
- Define a provider-neutral adapter with:
  - a deterministic fake provider for tests;
  - an OpenAI-compatible chat-completions provider for local endpoints;
  - a Fireworks profile using
    `https://api.fireworks.ai/inference/v1/chat/completions`.
- Use `FIREWORKS_API_KEY` by default for Fireworks, matching the
  [official quickstart](https://docs.fireworks.ai/getting-started/quickstart).
  Keep model ID configurable because serverless availability changes.
- Use provider-enforced JSON Schema when supported. Fireworks documents
  `response_format.type = "json_schema"` in its
  [structured-output guide](https://docs.fireworks.ai/structured-responses/structured-response-formatting).
- Add user-owned profiles at
  `~/.config/shipsim/agents.toml`, overridable by
  `SHIPSIM_AGENT_CONFIG`. Commit only a redacted example:

  ```toml
  [profiles.local]
  kind = "openai_compatible"
  base_url = "http://127.0.0.1:11434/v1"
  model = "your-local-model"
  api_key_env = ""
  response_format = "auto"

  [profiles.fireworks]
  kind = "openai_compatible"
  base_url = "https://api.fireworks.ai/inference/v1"
  model = "accounts/fireworks/models/choose-at-test-time"
  api_key_env = "FIREWORKS_API_KEY"
  response_format = "json_schema"
  ```

- The TUI discovers profile names through
  `shipsim-agent profiles --json`; it never parses provider configuration or
  receives secret values.
- When the host selects LLM Agent, the TUI launches `shipsim-agent` with only
  server address and profile name; pass the one-time seat token through a
  private stdin pipe, never argv. Also support launching the agent manually
  with masked token input or `--join-token-stdin` so future UIs are not
  required to spawn it. Resolve the executable from `SHIPSIM_AGENT_BIN` or
  the repository default.
- The agent:
  1. negotiates and joins its seat;
  2. converts side-relative snapshots into a compact, bounded prompt;
  3. asks for a schema-valid batch of orders for currently pending ships;
  4. uses authoritative preview requests for paths and shots;
  5. submits orders one at a time;
  6. feeds private engine/provider errors back into the bounded conversation;
  7. retries at most three times per decision; and
  8. submits a legal phase-specific hold/pass after exhausted retries so a
     malformed model response cannot deadlock the match.
- Conversation history is match-local and capped by message count and serialized
  bytes. Store decision summaries, not repeated full snapshots.
- Add profile controls for request timeout, maximum output tokens, maximum model
  requests per match, maximum serialized prompt bytes, and temperature.
  `response_format` supports `auto`, `json_schema`, and `json_object`
  for endpoint compatibility. Defaults must be conservative; report provider
  usage when returned, but do not estimate money from hard-coded prices.
- Agent/provider status is sent as advisory `thinking`, `ready`, or
  `error`; the TUI renders it without exposing prompts or provider responses.
- Logs contain timings, provider/profile name, token counts when available,
  submitted orders, validation errors, and fallback use. Redact authorization,
  keys, join tokens, and provider response metadata that may contain secrets.
- The server treats the LLM agent exactly like a Human connection for
  authorization. No model/provider code enters `shipsim_core` or
  `shipsim-session`.

### Automated tests

All default tests are offline and use local mock TCP/HTTP servers:

- Profile parsing resolves an environment-variable name without serializing its
  value.
- Missing key, HTTP 401/429/500, timeout, truncated body, malformed JSON,
  schema-invalid output, illegal order, and disconnect all produce bounded,
  redacted behavior.
- A repository-wide test fixture containing a sentinel secret verifies the
  sentinel never appears in protocol captures, logs, panic text, or snapshots.
- Fake provider completes at least five turns against Greedy through real TCP
  with zero unhandled errors.
- Malformed provider responses exhaust retries and produce legal pass orders
  without hanging a barrier.
- Agent controls side B, including purchased ships, and cannot order side A.
- Prompt snapshots omit opponent staged payloads and include enough public state
  to identify phase, owned pending ships, credits, purchase catalog, positions,
  facing, weapons, and objective.
- Custom class/display names are encoded as untrusted JSON data; a fixture with
  prompt-like text in a design name cannot alter the system instruction or tool
  schema.
- Conversation bounds hold over a 50-turn synthetic match.
- The TUI remains responsive and visibly says “LLM agent thinking” during a
  delayed mock response.
- No default test accesses the internet or reads `FIREWORKS_API_KEY`.

### Optional live smoke commands

These are never CI gates:

```bash
export FIREWORKS_API_KEY='set-in-your-shell-only'
cargo run --bin shipsim-session -- --listen 127.0.0.1:4100
cargo run --manifest-path frontend/tui/Cargo.toml -- --connect 127.0.0.1:4100
# Select Shipyard Assault → LLM Agent → fireworks in the TUI.
```

The model identifier is selected from the Fireworks account at test time rather
than frozen in source.

### Exit gate

- Offline fake-provider and mock-provider suites pass deterministically.
- A local OpenAI-compatible profile can complete three turns through the real
  TUI/server/agent stack.
- Fireworks code is fully testable without credentials and its only credential
  input is the named environment variable.
- Full repository verification passes.
- Commit: `feat(agent): add local and Fireworks LLM opponents`.

## Final automated verification

Run after each phase where applicable and in full after phase 6:

```bash
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --manifest-path frontend/tui/Cargo.toml
cargo clippy --manifest-path frontend/tui/Cargo.toml --all-targets -- -D warnings
(cd frontend/repl && python3 -m unittest discover -s tests)
luajit frontend/love/tests/run_all.lua
(cd frontend/agent && python3 -m unittest discover -s tests)
python3 -m compileall -q frontend/agent
cargo run --quiet --bin shipsim-yard -- check-all
git diff --check
```

The agent commands begin in phase 6. Record every command and result in the
final implementation handoff.

## Deferred manual acceptance after all coding

Do not stop implementation between phases 4–6 for these tests. Complete all
automated gates first, then run the following together as human players.

### Manual test 4 — Human versus Bot

1. Start a blank server.
2. Connect one TUI with `--connect`.
3. Configure Shipyard Assault against Greedy.
4. Buy at least one ship, complete three turns, fire a legal volley, and finish
   or intentionally quit.
5. Repeat a shorter match with Defensive and confirm its behavior is visibly
   distinct.
6. Confirm there was no second-client wait, raw JSON, unexplained freeze, or
   ownership confusion.

Pass evidence:

- screen transcript or screenshots of lobby, selected policy, purchase,
  waiting state, and combat;
- session trace with no rejected bot orders;
- subjective notes using `docs/UI-PLAYTEST-PROTOCOL.md` severity language.

### Manual test 5 — Human versus Human

1. Start a blank server and connect the host TUI.
2. Select Human Player 2 and capture the invitation.
3. Join from a second TUI.
4. Have both sides buy a ship and complete at least three turns.
5. Deliberately pause one TUI during each phase; verify the other says what it
   is waiting for.
6. Play to Victory/Loss or use a controlled UI fixture to verify Draw.
7. Disconnect one client and verify the remaining TUI explains termination.

Pass evidence:

- both TUI transcripts;
- side-symmetric capability checklist;
- no opponent staged-payload leakage;
- correct terminal result on both screens.

### Manual test 6 — Human versus LLM agents

Run two matches:

1. **Local endpoint:** choose a local profile, play at least three turns, and
   observe one model retry or explain why none occurred.
2. **Fireworks:** export `FIREWORKS_API_KEY` in the shell, choose the
   Fireworks profile and a currently available model, then play at least three
   turns or to a decisive result.

For each match verify:

- TUI shows connected/thinking/ready states;
- model latency never looks like a frozen TUI;
- the agent buys, allocates, moves, and fires through ordinary orders;
- malformed/illegal decisions recover visibly and do not hang;
- quitting terminates agent and server children;
- API key and join token appear nowhere in TUI/session/agent logs;
- request count and available token/cost telemetry are reported.

The live Fireworks result is a connectivity and playability test, not a
deterministic CI assertion or balance result.

## Final deliverables

- Root `CONTEXT.md` and one accepted ADR.
- Session protocol-v1 schemas, fixtures, and documentation.
- Refactored one-match lobby server.
- TUI local and TCP transports plus lobby/join screens.
- Five shared, selectable, seeded bot policies.
- Two-human TUI play.
- Isolated LLM agent client with fake, local-compatible, and Fireworks profiles.
- PTY/network playtest tooling and reports for manual tests 4–6.
- Updated README, architecture, frontend isolation, protocol, and TUI docs.
- One focused commit per phase and a clean working tree.

## Completion definition

The plan is complete only when:

1. every automated exit gate passes;
2. the TUI can configure and play phases 4, 5, and 6 without raw NDJSON;
3. side A and B use the same game API and UI capabilities;
4. Bot and LLM controllers produce only normal validated orders;
5. slow participants are legible and no tested path deadlocks;
6. credentials and tokens pass the redaction audit;
7. local subprocess play and all existing clients remain green; and
8. manual acceptance findings for tests 4–6 are documented, with every Major
   issue fixed or explicitly accepted before release.
