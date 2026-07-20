#!/usr/bin/env python3

"""generate_ship_art.py — Generate ship art one class/state at a time.

Generator CLI modes: list, audit, dry run, one class, one state,
missing-only, all P0, redo with reference, maximum calls, and
non-interactive confirmation override.

Batch planning prints primary asset count, requested states, minimum calls,
retry cap, model, output location, and whether any call would overwrite
accepted art.

Atomic writes: generate into local scratch, process and validate, then
replace accepted output only after success.

No default test reads GEMINI_API_KEY or accesses the network.

Phase 3 of ``docs/SHIP-ART-IMPLEMENTATION-PLAN.md``.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

# Make sibling modules importable.
_TOOLS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_TOOLS_DIR))

import ship_art_catalog as sac
import ship_art_image as sai
from ship_art_provider import (
    FakeProvider,
    FakeProviderConfig,
    ProviderAdapter,
    ProviderRequest,
    create_provider,
)

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------

_LOVE_DIR = _TOOLS_DIR.parent
_ASSETS_DIR = _LOVE_DIR / "assets" / "ship_art"
_LOCAL_DIR = _LOVE_DIR / "local"
_SCRATCH_DIR = _LOCAL_DIR / "scratch"
_BACKUP_DIR = _LOCAL_DIR / "backups"

# ---------------------------------------------------------------------------
# Batch planning
# ---------------------------------------------------------------------------


@dataclass
class BatchPlan:
    """Plan for a batch generation run."""

    classes: list[str] = field(default_factory=list)
    states: list[str] = field(default_factory=list)
    jobs: list[tuple[str, str]] = field(default_factory=list)
    primary_count: int = 0
    alias_count: int = 0
    min_calls: int = 0
    retry_cap: int = 3
    model: str = ""
    output_location: str = ""
    overwrites: list[str] = field(default_factory=list)
    would_overwrite: bool = False

    def to_dict(self) -> dict[str, Any]:
        return {
            "classes": list(self.classes),
            "states": list(self.states),
            "jobs": [list(job) for job in self.jobs],
            "primary_count": self.primary_count,
            "alias_count": self.alias_count,
            "min_calls": self.min_calls,
            "retry_cap": self.retry_cap,
            "model": self.model,
            "output_location": self.output_location,
            "overwrites": list(self.overwrites),
            "would_overwrite": self.would_overwrite,
        }


def requested_states(state_arg: str | None, redo_state: str | None) -> list[str] | None:
    """Resolve normal state selection and the documented one-state redo mode."""
    if redo_state is not None:
        if not sac.is_safe_identifier(redo_state):
            raise ValueError("--redo must name one lowercase state")
        if state_arg is not None:
            raise ValueError("--redo and --state cannot be combined")
        return [redo_state]
    return state_arg.split(",") if state_arg else None


def accepted_top_down_reference(class_id: str, assets_dir: Path = _ASSETS_DIR) -> Path | None:
    """Return the reviewed top-down reference for redo/reference generation."""
    sidecar = sac.load_sidecars(assets_dir).get(class_id)
    metadata = sidecar.states.get("top_down") if sidecar is not None else None
    if metadata is None or not metadata.is_accepted(assets_dir):
        return None
    return assets_dir / metadata.image_path


def plan_batch(
    catalog: list[sac.CatalogEntry],
    ship_ids: list[str] | None = None,
    states: list[str] | None = None,
    missing_only: bool = False,
    all_p0: bool = False,
    retry_cap: int = 3,
    model: str = "gemini-2.5-flash-image",
    assets_dir: Path = _ASSETS_DIR,
) -> BatchPlan:
    """Plan a batch generation run.

    Returns a BatchPlan with primary count, states, min calls, etc.
    """
    by_id = {e.class_id: e for e in catalog}
    primaries = [e for e in catalog if e.kind == "primary"]
    aliases = [e for e in catalog if e.kind == "alias"]

    # Determine which classes to generate.
    if ship_ids:
        target_primaries = []
        for sid in ship_ids:
            entry = by_id.get(sid)
            if entry is None:
                continue
            if entry.kind == "alias":
                # Resolve alias to primary.
                target = sac._resolve_alias_chain(sid, catalog)
                entry = by_id.get(target)
                if entry is None or entry.kind != "primary":
                    continue
            if entry.kind == "primary":
                target_primaries.append(entry)
    elif all_p0:
        target_primaries = primaries
    else:
        target_primaries = primaries

    # Deduplicate.
    seen = set()
    unique_primaries = []
    for e in target_primaries:
        if e.class_id not in seen:
            seen.add(e.class_id)
            unique_primaries.append(e)
    target_primaries = unique_primaries

    # Determine states.
    if states is None:
        states = list(sac.P0_STATES)

    # Plan exact class/state jobs. This keeps a resumable missing-only run from
    # regenerating already accepted states in a partially complete class.
    sidecars = sac.load_sidecars(assets_dir)
    jobs: list[tuple[str, str]] = []
    overwrites = []
    for entry in target_primaries:
        sc = sidecars.get(entry.class_id)
        for state in states:
            metadata = sc.states.get(state) if sc is not None else None
            metadata_for_check = metadata if metadata is not None else {}
            exists = sac.state_asset_exists(metadata_for_check, assets_dir)
            valid = sac.state_asset_valid(metadata_for_check, assets_dir)
            # Pending review output is intentionally not published, but it is
            # also not regenerated by --missing (and billed again). Rejected
            # output is eligible for a fresh missing-only attempt.
            pending_review = valid and metadata.review_status == "unreviewed"
            accepted = valid and metadata.review_status == "accepted"
            if missing_only and (accepted or pending_review):
                continue
            jobs.append((entry.class_id, state))
            if exists:
                overwrites.append(f"{entry.class_id}/{state}")

    job_classes = list(dict.fromkeys(class_id for class_id, _ in jobs))

    return BatchPlan(
        classes=job_classes,
        states=list(states),
        jobs=jobs,
        primary_count=len(job_classes),
        alias_count=len(aliases),
        min_calls=len(jobs),
        retry_cap=retry_cap,
        model=model,
        output_location=str(assets_dir),
        overwrites=overwrites,
        would_overwrite=len(overwrites) > 0,
    )


# ---------------------------------------------------------------------------
# Generation
# ---------------------------------------------------------------------------


@dataclass
class GenerationResult:
    """Result of generating one class/state."""

    class_id: str
    state: str
    success: bool
    outcome: str = "pass"  # pass, warning, blocking
    issues: list[str] = field(default_factory=list)
    file_bytes: int = 0
    attempts: int = 0
    error: str = ""

    def to_dict(self) -> dict[str, Any]:
        return {
            "class_id": self.class_id,
            "state": self.state,
            "success": self.success,
            "outcome": self.outcome,
            "issues": list(self.issues),
            "file_bytes": self.file_bytes,
            "attempts": self.attempts,
            "error": self.error,
        }


def build_prompt(entry: sac.CatalogEntry, state: str, has_reference: bool = False) -> str:
    """Build the generation prompt for a class + state."""
    style = (
        "Style: clean top-down spaceship sprite, dark outline, "
        "even flat studio lighting, solid uniform #FF00FF (magenta) background, "
        "centered on the mask."
    )
    desc = entry.visual_description

    if state == "top_down":
        pose = "top-down view, pointing upward, single ship centered."
    elif state == "portrait":
        pose = (
            "painterly close-up portrait, face and upper hull only, "
            "dramatic lighting, oil painting style, solid black background."
        )
    else:
        pose = f"{state} view."

    if has_reference:
        ref_note = (
            "This is a reference image of the ship. "
            "Generate the SAME ship. Same colors, same proportions, same style. "
        )
    else:
        ref_note = ""

    return (
        f"{ref_note}{style}\n\n"
        f"Make a {desc}.\n\n"
        f"A single ship: {pose}\n\n"
        f"CRITICAL: Only ONE ship. No duplicates. No multiple views. "
        f"Just one ship, centered on the background."
    )


def generate_one(
    provider: ProviderAdapter,
    entry: sac.CatalogEntry,
    state: str,
    assets_dir: Path = _ASSETS_DIR,
    scratch_dir: Path = _SCRATCH_DIR,
    backup_dir: Path = _BACKUP_DIR,
    retry_cap: int = 3,
    reference_image_path: str | Path | None = None,
    model: str = "gemini-2.5-flash-image",
    cancel_event: Any | None = None,
    catalog: list[sac.CatalogEntry] | None = None,
) -> GenerationResult:
    """Generate one class/state image with atomic write.

    Generates into scratch, processes and validates, then replaces accepted
    output only after success.
    """
    if not sac.is_safe_identifier(entry.class_id) or not sac.is_safe_identifier(state):
        return GenerationResult(
            class_id=entry.class_id,
            state=state,
            success=False,
            error="class_id and state must be lowercase identifiers",
        )
    class_dir = assets_dir / entry.class_id
    class_dir.mkdir(parents=True, exist_ok=True)
    scratch_dir.mkdir(parents=True, exist_ok=True)
    backup_dir.mkdir(parents=True, exist_ok=True)

    accepted_path = class_dir / f"{state}.png"
    scratch_path = scratch_dir / f"{entry.class_id}__{state}.png"

    if cancel_event is not None and cancel_event.is_set():
        return GenerationResult(
            class_id=entry.class_id, state=state, success=False,
            outcome="cancelled", error="generation cancelled",
        )

    # Later states default to the accepted top-down image. An explicit
    # reference (including --redo) takes precedence.
    reference_state = ""
    if reference_image_path is None and state != "top_down":
        sidecar = sac.load_sidecars(assets_dir).get(entry.class_id)
        top_down = sidecar.states.get("top_down") if sidecar is not None else None
        if top_down is not None and sac.state_is_accepted(top_down, assets_dir):
            reference_image_path = assets_dir / top_down.image_path
            reference_state = "top_down"

    # Load reference image if provided.
    ref_b64 = None
    if reference_image_path:
        ref_b64 = sai.load_image_base64(reference_image_path)
        if not reference_state:
            reference_state = Path(reference_image_path).stem

    prompt = build_prompt(entry, state, has_reference=ref_b64 is not None)
    request = ProviderRequest(prompt=prompt, reference_image_b64=ref_b64, model=model)

    result = provider.generate(request, retries=retry_cap)

    if not result.success:
        return GenerationResult(
            class_id=entry.class_id,
            state=state,
            success=False,
            error=result.error,
            attempts=result.attempts,
        )

    if cancel_event is not None and cancel_event.is_set():
        return GenerationResult(
            class_id=entry.class_id, state=state, success=False,
            outcome="cancelled", error="generation cancelled",
            attempts=result.attempts,
        )

    img_data = result.image_data or b""

    # Process into scratch.
    is_portrait = state == "portrait"
    try:
        if is_portrait:
            ok, file_bytes = sai.process_portrait(img_data, scratch_path)
        else:
            ok, pad_top, pad_bot = sai.process_top_down(img_data, scratch_path)
            file_bytes = os.path.getsize(scratch_path)
    except Exception as e:
        return GenerationResult(
            class_id=entry.class_id,
            state=state,
            success=False,
            error=f"processing failed: {e}",
            attempts=result.attempts,
        )

    # Validate.
    validation = sai.validate_image(scratch_path, is_portrait=is_portrait)

    if validation.outcome == sai.Outcome.BLOCKING:
        return GenerationResult(
            class_id=entry.class_id,
            state=state,
            success=False,
            outcome="blocking",
            issues=validation.issues,
            file_bytes=validation.file_bytes,
            attempts=result.attempts,
        )

    if cancel_event is not None and cancel_event.is_set():
        scratch_path.unlink(missing_ok=True)
        return GenerationResult(
            class_id=entry.class_id,
            state=state,
            success=False,
            outcome="cancelled",
            error="generation cancelled",
            attempts=result.attempts,
        )

    # Success — backup existing, then atomically replace.
    if accepted_path.is_file():
        sai.reversible_backup(accepted_path, backup_dir / entry.class_id)

    # Move scratch to accepted, update its sidecar, and remove the now-
    # unreviewed state from the runtime manifest as one publication unit.
    # This matters when regenerating an already accepted stable path: a stale
    # manifest must never expose the replacement bytes before review.
    publication_catalog = catalog if catalog is not None else sac.load_catalog()
    sidecar_path = class_dir / "sprite.toml"
    try:
        with sac.asset_publication(
            publication_catalog,
            assets_dir,
            [accepted_path, sidecar_path],
        ):
            from PIL import Image

            with Image.open(scratch_path) as accepted:
                width, height = accepted.size
            metadata = sac.StateAsset(
                image_path=f"{entry.class_id}/{state}.png",
                width=width,
                height=height,
                anchor_x=0.5,
                anchor_y=0.5,
                source_angle=0.0,
                scale=1.0,
                provider=provider.name,
                model=model,
                prompt_hash=hashlib.sha256(prompt.encode()).hexdigest(),
                reference_state=reference_state,
                processing_version="1",
                review_status="unreviewed",
            )
            sac.write_sidecar_state(
                sidecar_path,
                class_id=entry.class_id,
                display_name=entry.display_name,
                state=state,
                metadata=metadata,
            )
            # Remove any prior accepted record before replacing a stable image
            # path. If the process stops after either subsequent operation,
            # the on-disk manifest cannot expose the unreviewed bytes.
            sac.write_manifest(
                sac.generate_manifest(
                    publication_catalog,
                    assets_dir=assets_dir,
                ),
                path=assets_dir / "manifest.json",
            )
            os.replace(scratch_path, accepted_path)
    except Exception as error:
        return GenerationResult(
            class_id=entry.class_id,
            state=state,
            success=False,
            error=f"metadata update failed: {error}",
            attempts=result.attempts,
        )

    return GenerationResult(
        class_id=entry.class_id,
        state=state,
        success=True,
        outcome=validation.outcome.value,
        issues=validation.issues,
        file_bytes=validation.file_bytes,
        attempts=result.attempts,
    )


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def _cmd_list(args: argparse.Namespace) -> int:
    """List all catalog classes."""
    catalog = sac.load_catalog()
    for entry in sorted(catalog, key=lambda e: e.class_id):
        kind = entry.kind
        alias = f" -> {entry.alias_target}" if entry.kind == "alias" else ""
        special = f" [{entry.special}]" if entry.special else ""
        print(f"  {entry.class_id:30s} {kind:8s}{alias}{special}")
    print(f"\n  {len(catalog)} entries ({sum(1 for e in catalog if e.kind == 'primary')} primary, "
          f"{sum(1 for e in catalog if e.kind == 'alias')} aliases)")
    return 0


def _cmd_audit(args: argparse.Namespace) -> int:
    """Run catalog audit."""
    result = sac.audit()
    print(json.dumps(result.to_dict(), indent=2, sort_keys=True))
    return 0 if result.ok else 1


def _cmd_dry_run(args: argparse.Namespace) -> int:
    """Print batch plan without making any calls."""
    catalog = sac.load_catalog()
    try:
        states = requested_states(args.state, args.redo)
    except ValueError as error:
        print(f"REFUSED: {error}")
        return 1
    ship_ids = args.ship if args.ship else None

    plan = plan_batch(
        catalog,
        ship_ids=ship_ids,
        states=states,
        missing_only=args.missing,
        all_p0=args.all_p0,
        retry_cap=args.retry_cap,
        model=args.model,
    )

    print("Batch Plan (dry run — no calls will be made):")
    print(json.dumps(plan.to_dict(), indent=2, sort_keys=True))

    # Check max-calls constraint.
    if args.max_calls is not None and plan.min_calls > args.max_calls:
        print(f"\nREFUSED: {plan.min_calls} minimum calls exceeds --max-calls {args.max_calls}")
        return 1

    if plan.would_overwrite:
        print(f"\nWARNING: {len(plan.overwrites)} accepted assets would be overwritten:")
        for ow in plan.overwrites:
            print(f"  {ow}")

    return 0


def _cmd_generate(args: argparse.Namespace) -> int:
    """Generate ship art (requires API key unless --provider fake)."""
    catalog = sac.load_catalog()
    by_id = {e.class_id: e for e in catalog}
    try:
        states = requested_states(args.state, args.redo) or list(sac.P0_STATES)
    except ValueError as error:
        print(f"REFUSED: {error}")
        return 1
    ship_ids = args.ship if args.ship else None

    plan = plan_batch(
        catalog,
        ship_ids=ship_ids,
        states=states,
        missing_only=args.missing,
        all_p0=args.all_p0,
        retry_cap=args.retry_cap,
        model=args.model,
    )

    # Check max-calls constraint before any request.
    if args.max_calls is not None and plan.min_calls > args.max_calls:
        print(f"REFUSED: {plan.min_calls} minimum calls exceeds --max-calls {args.max_calls}")
        return 1

    # Confirm overwrites unless --yes.
    if plan.would_overwrite and not args.yes:
        print(f"WARNING: {len(plan.overwrites)} accepted assets would be overwritten:")
        for ow in plan.overwrites:
            print(f"  {ow}")
        print("\nUse --yes to confirm overwrites.")
        return 1

    redo_references: dict[str, Path] = {}
    if args.redo:
        for class_id, _ in plan.jobs:
            reference = accepted_top_down_reference(class_id)
            if reference is None:
                print(f"REFUSED: {class_id}/top_down has no accepted reference for --redo")
                return 1
            redo_references[class_id] = reference

    # Create provider.
    if args.provider == "fake":
        provider = FakeProvider(FakeProviderConfig())
    else:
        provider = create_provider(args.provider, model=args.model)

    print(f"Generating: {plan.primary_count} primaries, {plan.min_calls} state calls")
    print(f"Provider: {provider.name}, Model: {args.model}")
    print(f"Retry cap: {plan.retry_cap}")
    print(f"Output: {plan.output_location}")
    print()

    results: list[GenerationResult] = []
    for class_id, state in plan.jobs:
        entry = by_id[class_id]
        print(f"  Generating {class_id}/{state}...", end=" ", flush=True)
        result = generate_one(
            provider, entry, state,
            retry_cap=args.retry_cap,
            reference_image_path=redo_references.get(class_id),
            model=args.model,
            catalog=catalog,
        )
        results.append(result)
        status = "OK" if result.success else "FAIL"
        print(f"{status}")
        if result.issues:
            print(f"    issues: {', '.join(result.issues)}")

    # Summary.
    passed = sum(1 for r in results if r.success)
    failed = sum(1 for r in results if not r.success)
    print(f"\nSummary: {passed}/{len(results)} passed, {failed} failed")

    # A successful zero-job resume must still repair a missing or stale
    # generated manifest. Each non-empty job has already published its image,
    # sidecar, and manifest transactionally.
    if failed == 0:
        print("Regenerating manifest...")
        records = sac.generate_manifest(catalog)
        sac.write_manifest(records)
        print(f"Manifest: {len(records)} records")

    return 0 if failed == 0 else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Generate ship art one class/state at a time"
    )
    parser.add_argument("--list", action="store_true", help="List all catalog classes")
    parser.add_argument("--audit", action="store_true", help="Run catalog audit")
    parser.add_argument("--dry-run", action="store_true", help="Print batch plan without making calls")
    parser.add_argument("--ship", action="append", help="Ship class ID (can repeat)")
    parser.add_argument("--state", help="Comma-separated states (default: all P0)")
    parser.add_argument("--missing", action="store_true", help="Only generate missing assets")
    parser.add_argument("--all-p0", "--p0", action="store_true", help="Generate all P0 primaries")
    parser.add_argument("--redo", metavar="STATE", help="Regenerate one state using accepted top-down as reference")
    parser.add_argument("--max-calls", type=int, help="Refuse if minimum calls exceed this")
    parser.add_argument("--retry-cap", type=int, default=3, help="Max retry attempts per call")
    parser.add_argument("--model", default="gemini-2.5-flash-image", help="Provider model")
    parser.add_argument("--provider", default="gemini", help="Provider name (gemini or fake)")
    parser.add_argument("--yes", action="store_true", help="Non-interactive confirmation override")
    args = parser.parse_args(argv)

    if args.list:
        return _cmd_list(args)
    elif args.audit:
        return _cmd_audit(args)
    elif args.dry_run:
        return _cmd_dry_run(args)
    else:
        return _cmd_generate(args)


if __name__ == "__main__":
    sys.exit(main())
