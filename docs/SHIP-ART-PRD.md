# PRD: Love2D Ship Art Production and Runtime System

Status: Draft for spec audit
Source: Reviewed synthesis of the completed Ship Image System creation plan

## Problem Statement

shipsim's Love2D board represents ships with controller-colored circles. The markers keep the game playable, but they do not communicate hull identity, scale, variant, or visual character, and the text-only sidebar offers no portrait-level recognition. Producing a complete catalog manually would be slow and inconsistent, while introducing a hard dependency on finished art would make the game unusable during production or whenever an asset is missing.

The sibling NorRust project demonstrates a useful production model: generate transparent art through a reference-guided image API, post-process and validate it, review or repair it in a desktop tool, and let the game fall back to its original geometric markers. shipsim needs an equivalent system adapted to top-down starships, six-direction hex presentation, the Love2D client boundary, and shipsim's canonical ship catalog.

The original plan is directionally correct but cannot be implemented literally. Runtime ship IDs are scenario-instance numbers, while art belongs to canonical ship classes. Current snapshots expose a display class name rather than the canonical class key, including duplicate display names. Love-specific assets and tools also belong inside the Love client tree under the repository's frontend-isolation rules. Finally, the Love client has no TOML runtime parser, and its hex-facing angles do not correspond to a simple `facing × 60°` rotation from an upward-pointing source image.

## Solution

Add a Love2D-owned, fallback-safe ship-art system with four cooperating parts:

1. A structured authoring catalog containing every canonical ship class, its original visual description, aliases, desired states, and generation settings.
2. A Python generator that calls a configurable image-generation provider, uses the top-down image as the visual reference for later states, post-processes generated PNGs, validates them, and writes metadata plus a consolidated JSON runtime manifest.
3. A Python desktop reviewer that browses catalog completeness, edits prompts as structured data, regenerates one state from a chosen reference, and performs reversible image repairs.
4. A Love2D runtime loader that resolves art by canonical class identity, lazily and safely loads images from the Love client, and supplies either an art render specification or the existing geometric fallback to the board and HUD.

The engine remains authoritative for all game rules and never reads image files. It adds only the canonical ship class key to each ship snapshot so presentation clients can identify catalog content without parsing ship-definition TOML. Missing, malformed, unsupported, or partially generated art never prevents scenario loading or play. The existing circle, facing indicator, selection marker, controller identity, damage pulse, and text-only HUD remain the fallback contract.

The initial release uses one static top-down image and one portrait per canonical class. The top-down source has one declared canonical orientation and is rotated at draw time using the Love client's established hex presentation geometry. Destroyed, firing, hit, multi-frame, and per-facing assets are optional later states.

## User Stories

1. As a Love2D player, I want each ship class to have a recognizable silhouette, so that I can identify ships without reading every label.
2. As a Love2D player, I want light, line, and heavy variants to look meaningfully different, so that fleet composition is visible at a glance.
3. As a Love2D player, I want small and large hull tiers to communicate different visual mass, so that a fighter does not look interchangeable with a titan.
4. As a Love2D player, I want a selected ship's portrait in the sidebar, so that the command panel has a strong visual identity.
5. As a Love2D player, I want every sprite to point in the same direction as the existing facing indicator, so that movement and firing orientation remain unambiguous.
6. As a Love2D player, I want controller ownership to remain visible when sprites replace colored circles, so that I can distinguish friendly, enemy, and scripted ships immediately.
7. As a Love2D player, I want selection, target, range, damage, and facing overlays to remain legible over ship art, so that decoration never obscures decisions.
8. As a Love2D player, I want ships without finished art to render exactly through the current geometric fallback, so that incomplete art never blocks play.
9. As a Love2D player, I want a missing portrait to retain the current text-only panel, so that sidebar controls remain usable.
10. As a Love2D player, I want a missing wreck image to retain the current destroyed marker, so that destroyed ships remain understandable.
11. As a Love2D player, I want corrupt or unsupported assets to degrade safely, so that one bad PNG cannot crash a match.
12. As a player at the minimum supported window size, I want portraits to fit without hiding controls, so that visual polish does not reduce usability.
13. As a keyboard player, I want art integration to leave all controls and hit targets unchanged, so that the feature does not alter gameplay interaction.
14. As a content author, I want every canonical ship class listed in one structured catalog, so that art coverage and descriptions are auditable.
15. As a content author, I want tutorial and special hull classes represented explicitly, so that non-grid catalog entries are not forgotten.
16. As a content author, I want intentional art aliases to be declared, so that two classes may share art without relying on duplicate display names.
17. As a content author, I want prompts stored as data rather than rewritten inside executable source, so that edits are safe, reviewable, and deterministic.
18. As a content author, I want a rich per-class visual description, so that generated ships preserve tier and variant distinctions.
19. As a content author, I want an original shipsim art direction, so that generated assets do not imitate a named entertainment franchise.
20. As a content author, I want one top-down pilot generated before the rest of a class's states, so that it can anchor visual consistency.
21. As a content author, I want later states generated with the accepted top-down image as a reference, so that portraits and effects depict the same ship.
22. As a content author, I want to generate one ship, one state, missing states, or the full catalog, so that API use matches the task at hand.
23. As a content author, I want a dry run and a planned-call count before network generation, so that I understand the scope and likely spend.
24. As a content author, I want batch call limits and explicit confirmation, so that an accidental command cannot generate the full catalog.
25. As a content author, I want failed API calls retried with bounded backoff, so that transient failures do not lose a batch.
26. As a content author, I want failed validation retried separately from transport errors, so that output quality and provider availability are distinguishable.
27. As a content author, I want raw provider output retained only in ignored local scratch, so that I can diagnose generation without bloating the repository.
28. As a content author, I want accepted processed PNGs versioned, so that builds and reviews do not depend on a live API.
29. As a content author, I want generation provenance recorded, so that I can trace the model, prompt revision, reference asset, and processing settings used for an image.
30. As a content author, I want API credentials read only from the environment, so that secrets never enter committed files or command logs.
31. As an art reviewer, I want a searchable completeness list, so that I can focus on missing or failed classes.
32. As an art reviewer, I want all states for a ship visible together, so that I can judge identity consistency.
33. As an art reviewer, I want adjustable zoom and transparent-background presentation, so that I can inspect edges and small artifacts.
34. As an art reviewer, I want to edit a prompt and save it atomically to the authoring catalog, so that generation instructions remain valid if the application closes.
35. As an art reviewer, I want to select an existing base state and regenerate one target state, so that a good ship identity can be preserved while repairing one image.
36. As an art reviewer, I want generation to run away from the UI thread, so that the review tool remains responsive.
37. As an art reviewer, I want flop, resize, edge-trim, and undo tools, so that mechanical issues do not require another API call.
38. As an art reviewer, I want repairs to create local backups, so that destructive edits are recoverable.
39. As an art reviewer, I want automatic checks to identify clipping, oversize files, empty masks, and likely duplicate subjects, so that common failures are caught early.
40. As an art reviewer, I want disconnected-component checks tuned for spacecraft, so that valid detached nacelles, engine glows, or multi-hull designs are not rejected as duplicate characters.
41. As an art reviewer, I want warnings to be distinguishable from blocking failures, so that unusual but intentional silhouettes can be accepted consciously.
42. As a maintainer, I want all art code, tools, tests, assets, and scratch owned by the Love client, so that deleting the client does not leave presentation debris elsewhere.
43. As a maintainer, I want the Rust core to remain independent of PNGs and art metadata, so that engine builds and tests remain headless.
44. As a maintainer, I want canonical class identity in snapshots, so that frontends can resolve data-driven presentation without guessing from display names.
45. As a maintainer, I want class identity to be distinct from the numeric scenario ship ID, so that multiple instances of one class share the correct art.
46. As a maintainer, I want duplicate display names to be harmless, so that tutorial definitions cannot select the wrong asset.
47. As a maintainer, I want the runtime loader to consume the client's existing JSON capability rather than add a TOML parser, so that the runtime surface stays small.
48. As a maintainer, I want the runtime manifest generated and validated from authoring metadata, so that the loader does not scan directories or infer schema at startup.
49. As a maintainer, I want image loading to be lazy and cached, so that unused optional states do not consume startup time and memory.
50. As a maintainer, I want failed image lookups negatively cached with diagnostics, so that a missing file is not retried every frame.
51. As a maintainer, I want a development reload action or restart-safe workflow, so that reviewed art can be inspected without changing game logic.
52. As a maintainer, I want the geometric fallback decision testable without a Love window, so that regressions are agent-verifiable.
53. As a maintainer, I want all six sprite rotations checked against established board geometry, so that coordinate conventions cannot silently drift.
54. As a maintainer, I want an offline asset audit, so that CI can validate committed images and manifests without API credentials or network access.
55. As a maintainer, I want catalog parity checked against ship definitions, so that new ship classes create a visible completeness failure.
56. As a maintainer, I want fallback tests for zero, partial, malformed, and complete catalogs, so that staged production remains safe.
57. As a maintainer, I want the existing Love headless suite to cover loader and render-selection behavior, so that a separate test harness is unnecessary.
58. As a maintainer, I want one live Love UI play with mixed complete and missing art, so that the highest presentation seam is verified.
59. As a maintainer, I want existing REPL, TUI, harness, and simulation behavior unchanged, so that a graphical feature cannot alter game outcomes.
60. As a maintainer, I want provider and model names configurable, so that a retired model does not require rewriting the pipeline.
61. As a maintainer, I want costs and quotas treated as runtime constraints rather than assumed negligible, so that batch generation remains deliberate.
62. As a maintainer, I want generated-art provenance and review status documented, so that future contributors know which assets are accepted and reproducible.

## Implementation Decisions

- The Love2D client owns the complete feature: processed art, runtime metadata, Python authoring tools, tool tests, documentation, and ignored generation scratch. Shared engine data remains free of presentation assets.
- The Rust engine never opens, validates, packages, or depends on image files. Headless engine operation remains possible with the entire Love client absent.
- The canonical scenario catalog class key becomes an additive field on each ship snapshot. It is separate from the numeric scenario-instance ID and the human-readable class name. All clients may ignore it; the Love art loader uses it as its primary key.
- The catalog class key used to load a ship definition is authoritative even if the definition's internal ID is absent in older fixtures. Catalog validation checks that shipped definition IDs match their keys where they are present.
- Duplicate display names are not used for lookup. Tutorial or special classes may explicitly alias another class's art in the authoring catalog, but aliases are keyed by canonical class identity.
- The authoring catalog is structured data shared by the generator and reviewer. The reviewer updates that data atomically and never edits executable Python source through string replacement.
- Every shipped ship definition must have exactly one authoring-catalog record or an explicit alias. Catalog parity is derived from the authoritative ship-definition catalog rather than a hard-coded count.
- The initial art direction is original, neutral retro-futurist naval science fiction with clear light/line/heavy variant cues. Named-franchise imitation and franchise-specific terminology are excluded from prompts.
- P0 completeness consists of a static top-down image and a portrait for each non-aliased canonical class. These assets are required for catalog-complete status but remain optional at runtime because fallback is mandatory.
- P1 may add a destroyed image. P2 may add firing and hit images. Per-facing source images and animation are deferred until the static rotated pilots prove insufficient.
- The top-down image is authored pointing in one declared canonical source direction. Runtime rotation is computed from the existing Love hex presentation-angle function plus that source orientation. It is not computed as an unqualified `facing × 60°`.
- A headless geometry test compares all six rendered forward vectors with the pixel vectors produced by the board's neighbor conversion. This is the orientation contract.
- P0 sprites occupy approximately the existing board-marker footprint regardless of hull tier. Tier and variant scale are expressed primarily through silhouette and detail so larger images do not obscure neighboring hexes or controls.
- Controller ownership remains visible through the existing controller-colored underlay or outline. Full-sprite tinting is avoided by default because it destroys authored color, while selection, target, facing, damage, and range overlays remain above the sprite.
- Destroyed ships use a destroyed image only when it is valid and available. Otherwise they use the existing gray geometric marker, not a live top-down image that could be mistaken for an operational ship.
- Portraits are decorative and must fit within the existing sidebar allocation. They may shrink or disappear before any actionable control is clipped at the minimum supported window size.
- Each accepted asset directory has a sidecar containing canonical class identity, display metadata, state file descriptors, anchors, dimensions, declared source orientation, and generation provenance.
- The generator consolidates validated sidecars into a committed JSON runtime manifest. Love uses its existing JSON parser; it does not parse TOML or scan arbitrary directories at runtime.
- Runtime manifest entries contain only client-relative, normalized asset paths. Absolute paths, parent traversal, and files outside the Love client are rejected during generation and ignored defensively at runtime.
- The runtime loader lazily loads images through an injectable image-loading adapter, caches successes, and negatively caches failures. A failed image load records one diagnostic and returns fallback instead of throwing from the draw loop.
- The board and HUD consume presentation decisions from the loader. They do not read metadata files, perform catalog discovery, or contain generation logic.
- Geometric rendering remains a first-class named fallback path rather than an incidental `else` branch. With an empty manifest, board and sidebar output remain behaviorally equivalent to the current client.
- Image generation uses an adapter around the provider call. The initial adapter follows the proven Gemini reference-image request pattern, but provider URL and model are configurable and the selected model must be verified before a batch.
- API credentials are accepted only through environment variables and are never serialized into metadata, prompts, logs, backups, or error reports.
- Batch commands support listing, dry-run planning, one class, one state, missing-only generation, bounded retries, a maximum call count, and explicit confirmation before multi-class network work.
- Network-generated raw responses and repair backups are local scratch. Accepted processed images, sidecars, catalog data, and the runtime manifest are versioned.
- Top-down generation uses a uniform chroma background and reference feedback. Portrait generation may use a dark presentation background, but its identity must be reviewed against the accepted top-down source.
- Background removal, centering, file-size, and edge-clipping primitives may be adapted from NorRust. They are not assumed correct verbatim until pilot fixtures pass.
- Connected-component validation is spacecraft-aware. Empty masks and likely duplicate full subjects are blocking; small detached engine glows, antennae, nacelles, debris intended for a destroyed state, and similar components may be warnings or class/state-specific allowances.
- Validation results distinguish transport failure, decode failure, processing failure, blocking quality failure, and warning-only review. Automated retry applies only to configured retryable categories.
- The review tool performs generation on a worker and applies all UI changes on the UI thread. It uses recoverable backups for image mutations and atomic replacement for catalog edits.
- The feature does not add rendering decisions to the engine, change movement or combat rules, alter controller semantics, or make generated art necessary for API, REPL, TUI, or simulation play.

## Testing Decisions

- Tests assert external behavior: generated artifacts, manifest contents, loader choices, fallback choices, orientation vectors, visible control preservation, and unchanged protocol/game results. They do not assert private helper call counts or copy-specific implementation structure.
- The highest automated seam is the existing Love headless suite with an injected image loader and fixture manifest. It verifies that a snapshot class resolves to top-down and portrait render specifications, while missing or invalid entries resolve to the geometric fallback.
- A small pure presentation-decision seam returns sprite-or-fallback state, rotation, scale, controller cue, and portrait availability without calling Love graphics APIs. The board and HUD use this same seam.
- The canonical class snapshot field is covered at the real NDJSON harness seam. A shipped scenario must emit both the catalog class key and the existing display class name, including a case whose display name is duplicated by another definition.
- Existing client parsers are run against the additive snapshot field. No client may require an art catalog to deserialize or play the scenario.
- Generator tests use a fake provider adapter and small committed image fixtures. Default tests never require credentials, network access, or paid API calls.
- Generator CLI tests cover catalog listing, one-class planning, one-state planning, missing-only planning, dry-run call totals, call-limit enforcement, retry classification, and nonzero exit codes for invalid class/state/model configuration.
- Processing tests cover chroma removal, transparency, centering, empty output, edge clipping, size warnings/failures, a true duplicate-subject fixture, and a valid multi-component spacecraft fixture.
- Metadata tests cover schema-required fields, normalized relative paths, canonical identity, source orientation, dimensions, aliases, provenance, and rejection of traversal or files outside the client.
- Manifest tests regenerate the consolidated JSON from sidecars and compare it with the committed manifest. A stale or hand-edited manifest fails the audit.
- Catalog parity tests enumerate authoritative ship definitions and require either an art record or explicit alias. They also reject unknown catalog IDs and alias cycles.
- Loader tests cover an empty manifest, missing top-down image, missing portrait, missing destroyed state, corrupt PNG, malformed entry, unsupported frame count, duplicate lookup, and a complete entry.
- Loader tests prove repeated missing lookups return fallback without repeatedly invoking the image adapter.
- Rotation tests cover all six facings and compare the sprite's declared forward direction with the existing board neighbor pixel direction.
- Board presentation tests verify art and fallback markers preserve instance labels, selection rings, target rings, facing indicators, controller cues, and damage pulses.
- HUD presentation tests verify that portraits appear only when valid and never displace the required action controls at the supported layout floor.
- A mixed-catalog live UI play is required: at least one art-equipped player ship, one art-equipped opponent, one deliberately art-less class, and one destroyed fallback are observed in a real Love scenario.
- The pilot visual gate reviews three materially different hulls—a small hull, a line warship, and a capital hull—at all six facings before full-catalog generation is authorized.
- Full-catalog generation is not an automated test. Accepted art requires human visual review in the reviewer in addition to mechanical validation.
- The offline asset audit runs without a Gemini key and reports counts for complete, aliased, partial, warning, invalid, and missing entries.
- Root engine tests, the real harness suite, the Love headless suite, the REPL suite, and the independent TUI suite remain regression gates. Art integration cannot be declared complete while any existing gate is red.
- UI play follows the repository's UI-play procedure. API or headless tests cannot be reported as visual coverage.

## Out of Scope

- Any change to movement, firing, damage, AI, balance, scenario resolution, or simulation policy.
- Loading PNGs, art metadata, prompts, or provider configuration in the Rust core.
- Rendering ship art in the REPL or Ratatui client.
- Replacing the Love2D frontend or introducing another rendering framework.
- Per-controller or per-faction generated variants in P0.
- Six separately generated facing images in P0.
- Multi-frame engine glow, nacelle shimmer, firing animation, or hit animation in P0.
- Procedural ship construction from weapons or SSD state.
- Generating terrain, backgrounds, UI chrome, weapon effects, or campaign art.
- Changing board hitboxes or gameplay selection behavior to match opaque sprite bounds.
- Requiring a live provider call during build, test, packaging, or gameplay.
- Automatically accepting generated images solely because mechanical validation passes.
- Treating API pricing, quotas, or a particular model name as permanently stable.
- Reusing named-franchise visual language or producing deliberately derivative franchise art.

## Further Notes

The reviewed plan's central principle is retained: the game must remain fully playable at every point in art production. The fallback is therefore an acceptance contract, not a temporary migration aid.

The plan correctly identifies the NorRust generator and reviewer as valuable prior art, and its provider request, reference-image flow, chroma processing, repair operations, and tkinter structure are useful starting points. The claim that roughly 850 lines can move verbatim should be treated as an estimate, not a requirement. ships, alpha-separated engine effects, runtime ownership, structured prompt storage, and safe manifest generation require deliberate adaptation.

The runtime identity correction is especially important. A numeric ship ID identifies one vessel in one scenario and cannot key shared class art. A human-readable class name is also insufficient because shipped definitions contain duplicate names. Canonical class identity belongs in the snapshot as general data identity; the engine still remains unaware of art.

Keeping accepted assets and authoring tools within the Love client preserves the established client-isolation rule and makes Love packaging straightforward. Raw generation output and reviewer backups remain local scratch under that same client.

The current NorRust source uses `gemini-2.5-flash-image`, but provider models and commercial terms change. Implementation must verify availability, terms, quota, and expected batch cost before generating pilots or the complete catalog. No PRD acceptance criterion depends on the model retaining that exact name.

## Assumptions and Open Questions

- Assumption: an additive canonical class field in protocol-v4 snapshots is acceptable because it exposes existing data identity, changes no rules, and is safely ignorable by clients.
- Assumption: tutorial classes may either receive distinct art or explicitly alias their corresponding base classes; aliases are a catalog decision, never inferred from duplicate display names.
- Assumption: P0 uses one original neutral visual language across controllers, with controller identity supplied by presentation overlays rather than separate generated fleets.
- Assumption: source top-down images point upward, while runtime rotation uses the board's established presentation angle plus the declared source-angle offset.
- Assumption: processed 256×256 PNGs are an appropriate authoring resolution, but the pilot gate may lower file-size strictness or adjust processing thresholds based on measured Love memory and visual quality.
- Assumption: portraits use a consistent dark-background beauty-shot treatment unless pilot review demonstrates poor sidebar readability.
- Assumption: accepted processed art and provenance metadata may be committed to the repository; provider terms and project licensing must be checked before the full catalog is merged.
- Open question for spec audit: whether the runtime manifest should preload the small P0 catalog at scenario start or remain fully lazy. The PRD prefers lazy loading with caching, but measured startup and draw behavior may justify eager top-down preload.
- Open question for spec audit: whether special classes should have bespoke visual descriptions immediately or initially alias the nearest canonical hull. Either choice must be explicit before full-catalog generation.
- Open question for spec audit: the maximum committed PNG size and total catalog budget. NorRust's character thresholds are prior art, not yet evidence for detailed spacecraft.
- Open question for spec audit: whether generation provenance belongs in each asset sidecar, a catalog-level ledger, or both. The externally required behavior is traceability and offline auditability.
