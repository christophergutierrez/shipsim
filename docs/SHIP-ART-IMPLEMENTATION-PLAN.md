# Ship Art System — Phased Implementation Plan

Status: Draft implementation handoff
PRD: `docs/SHIP-ART-PRD.md`
Baseline: `1f76ae5` (`Directional fix`)

## Outcome

Ship art is complete when the Love2D client can resolve a ship's canonical class to validated top-down and portrait art, render that art at the correct six hex facings without obscuring gameplay overlays, and fall back to the current geometric presentation for every missing or invalid asset. Generation and review remain offline tools from the game's perspective: no API key, network connection, art file, or art metadata is required to build the engine or play through another client.

## Fixed Scope Decisions

- All art assets, authoring data, tools, tests, documentation, and scratch belong to `frontend/love/`.
- The engine exposes canonical `class_id` as an additive snapshot field but never reads art.
- Runtime lookup uses `class_id`, never numeric scenario ship ID or display class name.
- The authoring catalog and runtime manifest are structured JSON. Per-class `sprite.toml` files remain authoring/provenance sidecars; Love does not parse them.
- Tutorial classes initially alias their base visuals:
  - `tutorial_escort` → `escort`
  - `tutorial_heavy_cruiser` → `heavy_cruiser`
- The current 28 ship definitions therefore resolve to 26 primary P0 art sets and 2 explicit aliases. P0 calls are 52 minimum: one top-down and one portrait for each primary set, before retries.
- P0 uses static images. Destroyed, firing, hit, animation, and per-facing source images are later work.
- The first provider adapter uses the Gemini reference-image flow from NorRust, but provider endpoint and model are configuration, not permanent schema.
- No network or paid generation occurs before the Phase 4 checkpoint.

## Milestone Summary

| Phase | Milestone | Quantitative exit gate |
|---|---|---|
| 0 | Baseline and contract lock | 28 definitions inventoried; 26 primary records + 2 aliases specified; all existing suites green |
| 1 | Canonical class identity | Every ship snapshot contains `class_id`; duplicate display names resolve to distinct canonical IDs; protocol remains v4 |
| 2 | Catalog, sidecar, and manifest foundation | 28/28 definitions resolve; alias graph is acyclic; committed manifest regenerates byte-for-byte |
| 3 | Offline generator and reviewer | All provider-free tool tests pass; dry-run reports 52 P0 calls; malformed and spacecraft-specific fixtures classify correctly |
| 4 | Runtime fallback and render seam | Zero/partial/corrupt catalogs pass headless tests; all six rotations align with board geometry; no network required |
| 5 | Three-ship pilot | Six accepted pilot images; three hull scales reviewed at six facings; one mixed-art UI game passes |
| 6 | Full P0 catalog | 28/28 definitions resolve to accepted art; 26 top-down + 26 portraits committed; zero blocking audit failures |
| 7 | Release and documentation | Full engine/client/tool regression matrix green; offline asset audit reproducible; live UI acceptance recorded |
| 8 | Optional states | Each optional state has its own approved scope and fallback; P0 remains independently releasable |

## Phase 0 — Baseline and Contract Lock

### Work

1. Preserve baseline test evidence at commit `1f76ae5`.
2. Inventory ship definitions using Python `tomllib`; record canonical ID, display name, size tier, variant, and special-class status.
3. Create the initial catalog decision table with 26 primary records and the two tutorial aliases above.
4. Freeze these runtime contracts before implementation:
   - canonical identity is `class_id`;
   - source top-down art points upward;
   - runtime angle comes from `geom.facing_angle` plus the source-orientation offset;
   - board footprint does not exceed the existing marker footprint in P0;
   - controller color remains an underlay or outline;
   - portraits disappear before controls are clipped at the minimum window size;
   - invalid art always falls back and emits at most one diagnostic per asset.
5. Record provisional asset limits as pilot-tunable values rather than copying NorRust's 30 KB threshold as a release requirement.
6. Verify the selected provider/model, expected quota, terms, and estimated six-call pilot cost immediately before Phase 5, not earlier.

### Measurable exit gate

- Inventory reports exactly 28 current ship definitions.
- Catalog design reports exactly 26 primary entries and 2 aliases, with no unknown IDs.
- Duplicate display names (`Escort`, `Heavy Cruiser`) are documented and never used as runtime keys.
- Love, LuaJIT, Pillow, and tkinter availability is recorded.
- Root Rust, TUI, REPL, and Love baseline suites pass.

### Commands

```bash
cargo test
cargo test --manifest-path frontend/tui/Cargo.toml
(cd frontend/repl && python3 -m unittest discover -s tests)
luajit frontend/love/tests/run_all.lua
python3 -c 'import PIL, tkinter, tomllib; print(PIL.__version__)'
```

### Stop/go rule

Do not begin schema or art work with a red baseline. If a pre-existing failure is discovered, either fix it in a separate reviewed change or record an approved exception with an exact failing test and owner.

## Phase 1 — Add Canonical Class Identity to the Protocol

### Work

1. Preserve the catalog key used to load each ship definition on the runtime ship model.
2. Serialize that key as `class_id` on every ship snapshot while preserving the existing human-readable `class` field.
3. Keep `protocol_version = 4`; the field is additive and does not change order semantics.
4. Update real snapshot fixtures and any strict client snapshot types.
5. Validate shipped definitions whose internal `id` is present against the catalog key used to load them.
6. Add a regression fixture containing both tutorial and base classes with duplicate display names.

### Measurable exit gate

- Every ship in every emitted snapshot has a non-empty `class_id`.
- `class_id` equals the scenario's catalog class key.
- Numeric `id`, display `class`, and canonical `class_id` are independently asserted.
- `escort` and `tutorial_escort` may both display `Escort` but emit different `class_id` values.
- Existing orders and snapshots remain protocol v4.
- Rust, REPL, TUI, and Love suites remain green without any art files present.

### Focused verification

```bash
cargo test
cargo test --test harness
cargo test --manifest-path frontend/tui/Cargo.toml
(cd frontend/repl && python3 -m unittest discover -s tests)
luajit frontend/love/tests/run_all.lua
```

### Stop/go rule

If canonical identity cannot be exposed additively without breaking an existing client, stop and resolve the protocol compatibility issue. Do not fall back to normalizing display names or reading ship TOMLs from Love.

## Phase 2 — Build the Authoring Catalog, Sidecar Schema, and Runtime Manifest

### Deliverables

- `frontend/love/assets/ship_art/catalog.json` — structured prompt and alias catalog.
- `frontend/love/assets/ship_art/manifest.json` — generated Love runtime index.
- `frontend/love/assets/ship_art/<class_id>/sprite.toml` — per-primary authoring metadata once an asset exists.
- `frontend/love/tools/ship_art_catalog.py` — catalog/schema/audit library.
- `frontend/love/tools/tests/` — Python unit tests and small image fixtures.
- ignored raw output and backups below `frontend/love/local/`.

### Work

1. Define catalog fields for canonical ID, display name, original visual description, size/variant cues, desired states, alias target, and optional state-specific validation overrides.
2. Define sidecar fields for canonical ID, image descriptors, dimensions, anchors, source angle, generation provider/model, prompt revision or hash, reference state, processing version, and review status.
3. Define the consolidated runtime JSON manifest with only client-relative normalized paths and render metadata Love needs.
4. Implement deterministic manifest generation from catalog plus valid sidecars.
5. Implement catalog and manifest audit rules:
   - every authoritative definition resolves to one primary or alias;
   - no unknown catalog IDs;
   - no self-aliases or cycles;
   - alias target exists;
   - paths are relative, normalized, and remain inside the Love client;
   - required P0 descriptors are complete before a primary is marked complete;
   - manifest output is stable across repeated generation.
6. Keep incomplete primary entries in the authoring catalog while omitting unavailable states from the runtime manifest so partial production remains playable.

### Measurable exit gate

- Audit reports: `definitions=28`, `primary=26`, `aliases=2`, `unknown=0`, `cycles=0`.
- Rebuilding the runtime manifest twice produces identical SHA-256 output.
- Path traversal and absolute-path fixtures fail validation.
- An empty asset catalog generates a valid fallback-only manifest.
- A single complete fixture generates exactly one usable primary manifest entry plus any aliases targeting it.
- Python tests pass without network access.

### Commands

```bash
python3 -m unittest discover -s frontend/love/tools/tests
python3 frontend/love/tools/ship_art_catalog.py --audit
python3 frontend/love/tools/ship_art_catalog.py --write-manifest
python3 frontend/love/tools/ship_art_catalog.py --check-manifest
```

## Phase 3 — Implement Provider-Free Generation and Review Tooling

### Work

1. Port the reusable NorRust image primitives behind shipsim-specific interfaces:
   - base64 reference loading;
   - bounded provider request/retry;
   - chroma-background removal;
   - transparent-content bounds and centering;
   - edge, file-size, empty-mask, and connected-component checks;
   - portrait processing;
   - resize, flop, trim, and reversible backup operations.
2. Add a provider adapter interface and a fake provider used by all automated tests.
3. Implement generator CLI modes: list, audit, dry run, one class, one state, missing-only, all P0, redo with reference, maximum calls, and non-interactive confirmation override.
4. Make batch planning print primary asset count, requested states, minimum calls, retry cap, model, output location, and whether any call would overwrite accepted art.
5. Implement atomic writes: generate into local scratch, process and validate, then replace accepted output only after success.
6. Classify validation outcomes as blocking error, warning requiring review, or pass.
7. Adapt multi-component validation for spacecraft:
   - empty and duplicate full silhouettes block;
   - small detached glow/details warn;
   - destroyed-state debris may use state-specific allowances;
   - fixture evidence, not copied humanoid thresholds, determines defaults.
8. Implement the tkinter reviewer with searchable completeness, state previews, zoom, structured prompt editing, base-to-target regeneration, validation display, repair actions, undo, and worker-thread generation.
9. Marshal all tkinter mutations back to the UI thread.
10. Ensure API keys are read only from the environment and never written to logs or provenance.

### Measurable exit gate

- `--dry-run --all-p0` reports 26 primary sets, 2 aliases, and 52 minimum calls.
- `--max-calls 5 --all-p0` refuses before making a request.
- Fake-provider tests cover success, timeout, malformed JSON, missing image payload, bounded retry, and validation retry.
- Image fixtures cover transparent success, chroma removal, empty mask, clipped edge, oversize warning/failure, true duplicate subject, and valid multi-component spacecraft.
- A failed generation leaves the previously accepted image byte-for-byte unchanged.
- Prompt edits survive reviewer restart and do not modify executable Python source.
- Reviewer regeneration runs without freezing the main UI in a manual local test.
- No default test reads `GEMINI_API_KEY` or accesses the network.

### Commands

```bash
python3 -m unittest discover -s frontend/love/tools/tests
python3 frontend/love/tools/generate_ship_art.py --list
python3 frontend/love/tools/generate_ship_art.py --dry-run --all-p0
python3 frontend/love/tools/generate_ship_art.py --dry-run --all-p0 --max-calls 5
python3 frontend/love/tools/review_ship_art.py --missing
```

### Stop/go rule

Do not call the live provider in this phase. The fake-provider and fixture suite must prove retries, atomic output, validation, and call caps first.

## Phase 4 — Implement the Love Runtime Loader and Exact Fallback

### Work

1. Add a pure render-decision module that accepts snapshot ship data plus a manifest entry and returns:
   - art or fallback choice;
   - top-down asset key;
   - portrait availability;
   - sprite scale and anchor;
   - six-facing rotation;
   - controller cue;
   - destroyed-state choice.
2. Add a Love image adapter that lazily loads client-relative images, caches successes, and negatively caches failures.
3. Emit one diagnostic per failed asset and never throw from the board or HUD draw loop.
4. Use `class_id` to resolve manifest records and aliases.
5. Derive rotation from `geom.facing_angle(facing)` and the declared upward source angle.
6. Integrate the decision module into the existing board marker block while retaining the original circle block as a named fallback.
7. Preserve instance number, facing tick/chevron, selection ring, target ring, range cues, damage pulse, and controller identity above or around art.
8. Add portrait rendering to the HUD as optional decoration that yields space to command controls.
9. Use fixture PNGs only; live generated pilots are not needed to complete this phase.

### Measurable exit gate

- Zero-art fixture selects geometric fallback for 100% of ships.
- Partial fixture renders one sprite while all other ships use fallback.
- Missing, malformed, corrupt, unsupported-frame, and escaped-path fixtures use fallback without exceptions.
- Repeated missing-image lookup invokes the image adapter exactly once because the negative cache is active.
- All six facing vectors match `hex.to_pixel(hex.neighbor(...))` within a small floating-point tolerance.
- Sprite bounds remain within the agreed P0 marker footprint.
- Destroyed ship without a destroyed asset renders the existing gray marker.
- Portrait absence produces the current text-only HUD.
- At 1024×720, all existing actionable controls remain visible with a portrait present.
- Love headless tests remain provider-free and pass.

### Commands

```bash
luajit frontend/love/tests/run_all.lua
LOVE_LIVE=1 luajit frontend/love/tests/run_all.lua
cargo test
```

### Rollback property

An empty runtime manifest disables all art without code changes. This is the release rollback and must remain tested.

## Phase 5 — Generate and Review the Three-Ship Pilot

### Pilot set

- `escort` — small, compact combatant.
- `heavy_cruiser` — mid-sized primary player hull used by many shipped scenarios.
- `huge` — capital-scale silhouette.

This set can be exercised together in `scenarios/m3_thrust.toml`; leaving another class or state deliberately absent provides fallback coverage where needed.

### Pre-call checkpoint

Before any network request:

- verify the configured provider model is available;
- record estimated six-call base cost, retry cap, and quota;
- inspect the exact prompts and references printed by dry run;
- confirm no accepted output will be overwritten;
- set `--max-calls` to the approved pilot ceiling;
- receive explicit authorization to perform the external calls.

### Work

1. Generate top-down plus portrait for the three pilots: six successful base outputs before retries.
2. Review every output mechanically and visually.
3. Confirm the art is original and does not resemble a named franchise design.
4. Inspect each top-down sprite at all six runtime rotations.
5. Tune chroma threshold, connected-component policy, edge padding, file-size limits, portrait treatment, and board scale from pilot evidence.
6. Record accepted provenance and review status; commit processed assets only.
7. Launch Love through the UI and play a mixed-art scenario through at least one complete turn.

### Measurable exit gate

- Exactly 6 pilot P0 images are accepted: 3 top-down and 3 portraits.
- 18 top-down orientation views (3 ships × 6 facings) pass visual review.
- All pilot assets pass the offline audit with zero blocking failures.
- At least one pilot exercises a multi-component silhouette without a false duplicate-subject failure.
- Controller cues remain distinguishable for player, AI, and scripted ownership.
- One live Love game reaches the next turn with art enabled.
- A deliberately absent or invalid asset falls back during the same acceptance session.
- Minimum-size HUD remains playable with each pilot portrait.
- Final validation thresholds and total-asset budget are recorded before Phase 6.

### Commands

```bash
python3 frontend/love/tools/generate_ship_art.py --dry-run \
  --ship escort --ship heavy_cruiser --ship huge --p0
python3 frontend/love/tools/generate_ship_art.py \
  --ship escort --ship heavy_cruiser --ship huge --p0 --max-calls <approved-limit>
python3 frontend/love/tools/review_ship_art.py
python3 frontend/love/tools/ship_art_catalog.py --audit --offline
cargo build -q
./frontend/love/play.sh
```

### Stop/go rule

Do not generate the remaining catalog unless all six pilot images are accepted and all 18 rotations pass. Prompt, processing, scale, or validation changes after this gate require regenerating or re-reviewing affected pilots first.

## Phase 6 — Generate and Accept the Full P0 Catalog

### Work

1. Recompute the missing-only plan from the accepted pilot baseline.
2. Generate remaining primary top-down images first, review them, then use each accepted top-down as the reference for its portrait.
3. Keep each batch below the approved call cap and preserve resumability.
4. Review failures by class/state; do not lower global validation thresholds to accommodate one unusual hull when a scoped override is sufficient.
5. Regenerate the runtime manifest only from accepted assets.
6. Run complete offline parity and provenance audits after each batch.

### Measurable exit gate

- Audit reports `definitions=28`, `primary=26`, `aliases=2`, `resolved_complete=28`.
- Exactly 26 accepted top-down images and 26 accepted portraits are represented, unless a documented primary-to-primary art alias reduces the count through spec amendment.
- Every accepted top-down image is reviewed at all six facings: 156 orientation checks total.
- Every primary sidecar records provider/model, prompt revision, reference lineage, processing version, and review status.
- Runtime manifest regeneration is deterministic and clean in Git.
- Blocking validation failures: 0.
- Unknown catalog IDs: 0.
- Missing primary P0 states: 0.
- Unreviewed warning states: 0.
- Total committed asset bytes remain within the budget established by the pilot gate.

### Commands

```bash
python3 frontend/love/tools/generate_ship_art.py --dry-run --missing --p0
python3 frontend/love/tools/generate_ship_art.py --missing --p0 --max-calls <batch-limit>
python3 frontend/love/tools/review_ship_art.py --missing
python3 frontend/love/tools/ship_art_catalog.py --audit --offline
python3 frontend/love/tools/ship_art_catalog.py --write-manifest
python3 frontend/love/tools/ship_art_catalog.py --check-manifest
```

### Stop/go rule

Any class that remains visually unacceptable after three generation attempts is not silently waived. Keep its geometric fallback, record the incomplete class, and stop catalog-complete release until it is manually repaired, given a scoped prompt/validator override, or explicitly removed from the release target.

## Phase 7 — Documentation, Full Regression, and Release Checkpoint

### Work

1. Publish the authoritative asset specification covering catalog fields, sidecars, runtime manifest, orientation, fallback, validation, provenance, reviewer workflow, and adding a new class.
2. Update the Love README with runtime behavior, tool dependencies, offline audit, generation safety, and reviewer commands.
3. Add the ship-art specification to the repository agent navigation table.
4. Document that raw provider output and backups are ignored local scratch, while processed accepted assets are versioned.
5. Run the complete regression matrix without provider credentials.
6. Perform final UI play using an art-complete scenario and a fallback-forced scenario.
7. Record test counts, asset counts, byte totals, pilot/full generation calls, and any warnings in the release checkpoint.

### Required automated gates

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo fmt --manifest-path frontend/tui/Cargo.toml --check
cargo clippy --manifest-path frontend/tui/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path frontend/tui/Cargo.toml
(cd frontend/repl && python3 -m unittest discover -s tests)
python3 -m unittest discover -s frontend/love/tools/tests
python3 frontend/love/tools/ship_art_catalog.py --audit --offline
luajit frontend/love/tests/run_all.lua
LOVE_LIVE=1 luajit frontend/love/tests/run_all.lua
git diff --check
```

### Final measurable checkpoint

- All required automated commands pass without `GEMINI_API_KEY`.
- Root engine outcomes and simulation determinism are unchanged.
- 28/28 ship definitions resolve to accepted P0 presentation through primary or alias.
- Empty-manifest rollback remains green.
- One art-complete UI play and one forced-fallback UI play are recorded.
- All six facings agree with the existing facing indicator.
- Minimum-size layout retains every action control.
- No absolute paths, secrets, raw provider responses, or reviewer backups are tracked.
- `git status --short` contains only intended implementation, documentation, metadata, and accepted image files.

## Phase 8 — Optional P1/P2 States

Optional states are separate follow-on milestones and do not block the P0 release.

### P1: destroyed state

- Pilot on one small and one capital hull.
- Define debris-aware validation and review criteria.
- Verify destroyed art cannot be mistaken for a living ship.
- Require exact gray-marker fallback when absent.

### P2: firing and hit states

- Decide whether these are replacement sprites, overlays, or effect assets.
- Integrate with the existing resolution theater without changing combat timing.
- Preserve the current tracer and damage-pulse fallback.

### Animation or per-facing images

- Require measured evidence that static rotation is inadequate.
- Specify memory, frame-time, file-size, and metadata impacts before implementation.
- Maintain static-image fallback for every animated state.

## Dependency Graph

```text
Phase 0: baseline and contracts
    → Phase 1: canonical class_id
        → Phase 2: catalog + sidecars + manifest
            ├─→ Phase 3: generator/reviewer tooling
            └─→ Phase 4: Love loader/fallback with fixtures
                    Phase 3 + Phase 4
                        → Phase 5: six-image pilot + UI acceptance
                            → Phase 6: full P0 catalog
                                → Phase 7: release checkpoint
                                    → Phase 8: optional states
```

Phases 3 and 4 may proceed in parallel after Phase 2 because both consume the same frozen metadata contract. No other phase may bypass its predecessor's exit gate.

## Cross-Phase Stop Conditions

- Stop if implementation requires the engine to open art assets or metadata.
- Stop if Love must infer canonical identity from display strings.
- Stop if a missing or corrupt asset can abort scenario load or a draw frame.
- Stop if sprite presentation changes hitboxes, order semantics, or authoritative geometry.
- Stop if accepted art cannot be reproduced or traced to prompt/model/reference metadata.
- Stop network work if the provider model, quota, terms, or expected cost differs materially from the approved checkpoint.
- Stop full generation if pilot rotation, scale, ownership cues, or minimum-size layout is not accepted.
- Stop release if any automated gate requires credentials or network access.

## Definition of Done

The P0 ship-art system is done when all 28 current ship definitions resolve by canonical class identity to one of 26 accepted primary art sets or two explicit aliases; every primary has a reviewed top-down image and portrait; all six rotations align with board geometry; controller and gameplay overlays remain legible; zero, partial, and corrupt catalogs preserve the geometric fallback; the full offline audit and all engine/client/tool suites pass without credentials; and a real Love UI game demonstrates both sprite rendering and forced fallback without changing game behavior.
