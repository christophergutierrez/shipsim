# ADR-0024 — Versioned Rules Data With Per-Game Ownership

Status: Accepted
Date: 2026-07-16
Related: ADR-0020 (Combat Model v2), ADR-0022 (persistent velocity)

## Context

Combat balance tables and the SSD damage-allocation chart were embedded in
Rust code. That made balance changes harder to review and allowed the shipped
documentation, catalog ranges, and engine behavior to drift. Weapon instances
already expose `max_range`, but the engine previously enforced only the
weapon-kind table length.

## Decisions

1. The canonical ruleset is `data/rules/default.toml`. It is one atomic,
   schema-versioned document; selectable rules variants are deferred.
2. TOML supplies validated values and named algorithm choices. Formula
   implementation remains typed Rust code; rules files cannot contain code or
   free-form expressions.
3. Scenario construction loads one immutable `Arc<Ruleset>` and attaches it to
   `GameState`. There is no process-global mutable rules singleton, so tests
   and simulations can inject isolated rulesets.
4. Combat tables, accuracy knobs, die sides, damage values, and the SSD DAC are
   authoritative in the rules document. Hex geometry, mount tables, protocol
   version, movement structure, and weapon-kind enums remain code rules.
5. Effective weapon range is the instance `max_range`, bounded by the rules
   table's supported range. Invalid zero or over-table ranges fail content
   loading; the engine does not silently truncate them.
6. Rules fingerprints are semantic hashes of the parsed rules document. Saves
   created now record the fingerprint and reject replay against a different
   ruleset. Older saves without the optional field retain compatibility.

## Consequences

- Balance changes are reviewable as data diffs and simulation reports identify
  the rules data used.
- Frontends remain thin clients: they receive snapshots and submit orders; they
  do not load or interpret rules files for legality.
- Changing `default.toml` is a behavior change and requires fast/pooled
  simulation review. The reserved sign-off seed range must remain untouched
  until certification.
- Hull properties such as subsystem box counts and shield caps are not moved
  into the global rules file. Shipped hull TOML must state them explicitly.
