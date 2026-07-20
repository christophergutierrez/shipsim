#!/usr/bin/env python3
"""Ship art catalog, sidecar schema, and runtime manifest library.

This module is the single source of truth for the ship-art authoring catalog,
per-class sidecar metadata, and the Love runtime manifest.  It provides:

  * Schema definitions for catalog entries, sidecars, and manifest records.
  * Deterministic manifest generation from catalog + valid sidecars.
  * Audit rules that enforce the Phase 2 exit gates.
  * A CLI with ``--audit``, ``--write-manifest``, and ``--check-manifest``.

The engine never reads art.  Love reads only the runtime manifest.  Per-class
``sprite.toml`` sidecars are authoring/provenance sidecars; Love does not parse
them at runtime — only this tool does, to build the manifest.

Phase 2 of ``docs/SHIP-ART-IMPLEMENTATION-PLAN.md``.
"""

from __future__ import annotations

import hashlib
import json
import math
import os
import re
import sys
import tomllib
from contextlib import contextmanager
from dataclasses import dataclass, field, replace
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------

# This file lives at frontend/love/tools/ship_art_catalog.py.
_TOOLS_DIR = Path(__file__).resolve().parent
_LOVE_DIR = _TOOLS_DIR.parent
_ASSETS_DIR = _LOVE_DIR / "assets" / "ship_art"
_SHIPS_DIR = _LOVE_DIR.parent.parent / "data" / "ships"
_SIZES_FILE = _LOVE_DIR.parent.parent / "data" / "sizes.toml"

CATALOG_PATH = _ASSETS_DIR / "catalog.json"
MANIFEST_PATH = _ASSETS_DIR / "manifest.json"

# P0 states: top-down sprite + portrait.
P0_STATES = ("top_down", "portrait")
REVIEW_STATUSES = ("unreviewed", "accepted", "rejected")
_IDENTIFIER_RE = re.compile(r"^[a-z][a-z0-9_]*$")


def is_safe_identifier(value: Any) -> bool:
    """Return whether a class/state identifier is safe in TOML and paths."""
    return isinstance(value, str) and _IDENTIFIER_RE.fullmatch(value) is not None

# ---------------------------------------------------------------------------
# Schema
# ---------------------------------------------------------------------------


@dataclass
class CatalogEntry:
    """One record in the authoring catalog.

    Primary entries own their art.  Alias entries borrow another class's art.
    """

    class_id: str
    display_name: str
    kind: str  # "primary" or "alias"
    alias_target: str | None = None
    size_tier: int = 0
    size_name: str = ""
    variant: str = ""  # "heavy", "light", "line", "", etc.
    visual_description: str = ""
    desired_states: list[str] = field(default_factory=lambda: list(P0_STATES))
    special: str = ""  # "tutorial", "immobile", "pilot", ...

    def to_dict(self) -> dict[str, Any]:
        d: dict[str, Any] = {
            "class_id": self.class_id,
            "display_name": self.display_name,
            "kind": self.kind,
            "size_tier": self.size_tier,
            "size_name": self.size_name,
            "variant": self.variant,
            "visual_description": self.visual_description,
            "desired_states": list(self.desired_states),
            "special": self.special,
        }
        if self.kind == "alias":
            d["alias_target"] = self.alias_target
        return d


@dataclass(frozen=True)
class StateAsset:
    """One complete sidecar state and its review lifecycle.

    This is the sole schema owner for persisted state metadata. Sidecars never
    persist partial dictionaries; malformed legacy/manual TOML fails loading
    and is reported by the audit.
    """

    image_path: str
    width: int
    height: int
    anchor_x: float
    anchor_y: float
    source_angle: float
    scale: float
    provider: str
    model: str
    prompt_hash: str
    reference_state: str
    processing_version: str
    review_status: str

    @classmethod
    def from_dict(cls, data: dict[str, Any], state: str = "state") -> "StateAsset":
        required = tuple(cls.__dataclass_fields__)
        for key in required:
            if key not in data:
                raise ValueError(f"state '{state}' missing metadata '{key}'")
        asset = cls(**{key: data[key] for key in required})
        asset.validate_schema(state)
        return asset

    def validate_schema(self, state: str = "state") -> None:
        if (
            not isinstance(self.image_path, str)
            or not self.image_path
            or not is_safe_relative_path(self.image_path, Path("."))
        ):
            raise ValueError(f"state '{state}' has invalid image_path")
        for key in ("width", "height"):
            value = getattr(self, key)
            if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
                raise ValueError(f"state '{state}' has invalid {key}")
        for key in ("anchor_x", "anchor_y"):
            value = getattr(self, key)
            if (
                not isinstance(value, (int, float))
                or isinstance(value, bool)
                or not math.isfinite(value)
                or not 0.0 <= value <= 1.0
            ):
                raise ValueError(f"state '{state}' has invalid {key}")
        if (
            not isinstance(self.source_angle, (int, float))
            or isinstance(self.source_angle, bool)
            or not math.isfinite(self.source_angle)
        ):
            raise ValueError(f"state '{state}' has invalid source_angle")
        if (
            not isinstance(self.scale, (int, float))
            or isinstance(self.scale, bool)
            or not math.isfinite(self.scale)
            or not 0.0 < self.scale <= 1.0
        ):
            raise ValueError(f"state '{state}' has invalid scale")
        for key in ("provider", "model", "prompt_hash", "processing_version"):
            value = getattr(self, key)
            if not isinstance(value, str) or not value.strip():
                raise ValueError(f"state '{state}' has invalid {key}")
        if not isinstance(self.reference_state, str):
            raise ValueError(f"state '{state}' has invalid reference_state")
        if self.review_status not in REVIEW_STATUSES:
            raise ValueError(
                f"state '{state}' has unknown review_status '{self.review_status}'"
            )

    def to_dict(self) -> dict[str, Any]:
        return {
            key: getattr(self, key)
            for key in self.__dataclass_fields__
        }

    def transition_review(self, review_status: str) -> "StateAsset":
        if review_status not in REVIEW_STATUSES:
            raise ValueError(f"unknown review status '{review_status}'")
        return replace(self, review_status=review_status)

    def after_pixel_change(self, width: int, height: int) -> "StateAsset":
        changed = replace(
            self,
            width=width,
            height=height,
            review_status="unreviewed",
        )
        changed.validate_schema()
        return changed

    def asset_exists(self, assets_dir: Path) -> bool:
        return is_safe_relative_path(self.image_path, assets_dir) and (
            assets_dir / self.image_path
        ).is_file()

    def asset_valid(self, assets_dir: Path) -> bool:
        if not self.asset_exists(assets_dir):
            return False
        try:
            from PIL import Image

            with Image.open(assets_dir / self.image_path) as image:
                actual_size = image.size
                image.verify()
        except (OSError, ValueError):
            return False
        return actual_size == (self.width, self.height)

    def is_accepted(self, assets_dir: Path) -> bool:
        return self.review_status == "accepted" and self.asset_valid(assets_dir)

    def to_manifest_record(self, class_id: str, state: str) -> "ManifestRecord":
        return ManifestRecord(
            class_id=class_id,
            state=state,
            image_path=Path(self.image_path).as_posix(),
            width=self.width,
            height=self.height,
            anchor_x=self.anchor_x,
            anchor_y=self.anchor_y,
            source_angle=self.source_angle,
            scale=self.scale,
        )


@dataclass
class Sidecar:
    """Per-class authoring/provenance metadata (sprite.toml).

    Only present once an asset exists.  Love does not parse this at runtime;
    this tool reads it to build the manifest.
    """

    class_id: str
    states: dict[str, StateAsset] = field(default_factory=dict)
    display_name: str = ""

    @classmethod
    def from_toml(cls, path: Path) -> "Sidecar":
        with open(path, "rb") as f:
            data = tomllib.load(f)
        class_id = data.get("class_id", path.parent.name)
        states: dict[str, StateAsset] = {}
        raw_states = data.get("states", {})
        if isinstance(raw_states, dict):
            for state_name, state_data in raw_states.items():
                if not is_safe_identifier(state_name):
                    raise ValueError(f"invalid state identifier '{state_name}'")
                if not isinstance(state_data, dict):
                    raise ValueError(f"state '{state_name}' must be a table")
                states[state_name] = StateAsset.from_dict(state_data, state_name)
        return cls(class_id=class_id, states=states, display_name=data.get("display_name", ""))


def _toml_string(value: str) -> str:
    """Encode a string for the small, deterministic sidecar TOML schema."""
    return json.dumps(value, ensure_ascii=False)


def sidecar_to_toml(sidecar: Sidecar) -> str:
    """Serialize authoring metadata deterministically without a TOML dependency."""
    lines = [f"class_id = {_toml_string(sidecar.class_id)}"]
    if sidecar.display_name:
        lines.append(f"display_name = {_toml_string(sidecar.display_name)}")
    for state, asset in sorted(sidecar.states.items()):
        metadata = asset.to_dict()
        lines.extend(("", f"[states.{state}]"))
        preferred = (
            "image_path", "width", "height", "anchor_x", "anchor_y",
            "source_angle", "scale", "provider", "model", "prompt_hash",
            "reference_state", "processing_version", "review_status",
        )
        for key in preferred:
            value = metadata[key]
            if isinstance(value, str):
                encoded = _toml_string(value)
            elif isinstance(value, bool):
                encoded = "true" if value else "false"
            elif isinstance(value, (int, float)) and not isinstance(value, bool):
                encoded = str(value).lower()
            else:
                raise ValueError(f"unsupported sidecar value for {state}.{key}")
            lines.append(f"{key} = {encoded}")
    return "\n".join(lines) + "\n"


def write_sidecar_state(
    path: Path,
    *,
    class_id: str,
    display_name: str,
    state: str,
    metadata: StateAsset | dict[str, Any],
) -> None:
    """Atomically insert or replace one state while preserving other states."""
    if not is_safe_identifier(class_id) or not is_safe_identifier(state):
        raise ValueError("class_id and state must be lowercase identifiers")
    sidecar = Sidecar(class_id=class_id, display_name=display_name)
    if path.is_file():
        sidecar = Sidecar.from_toml(path)
        if sidecar.class_id != class_id:
            raise ValueError(
                f"sidecar class_id '{sidecar.class_id}' does not match '{class_id}'"
            )
        sidecar.display_name = display_name or sidecar.display_name
    asset = (
        metadata
        if isinstance(metadata, StateAsset)
        else StateAsset.from_dict(metadata, state)
    )
    asset.validate_schema(state)
    sidecar.states[state] = asset
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(sidecar_to_toml(sidecar))
    os.replace(temporary, path)


def set_review_status(
    path: Path,
    *,
    class_id: str,
    state: str,
    review_status: str,
) -> None:
    """Atomically update the explicit reviewer decision for one existing state."""
    if review_status not in REVIEW_STATUSES:
        raise ValueError(f"unknown review status '{review_status}'")
    if not path.is_file():
        raise FileNotFoundError(path)
    sidecar = Sidecar.from_toml(path)
    if sidecar.class_id != class_id:
        raise ValueError(
            f"sidecar class_id '{sidecar.class_id}' does not match '{class_id}'"
        )
    asset = sidecar.states.get(state)
    if asset is None:
        raise ValueError(f"sidecar has no state '{state}'")
    write_sidecar_state(
        path,
        class_id=class_id,
        display_name=sidecar.display_name,
        state=state,
        metadata=asset.transition_review(review_status),
    )


@dataclass
class ManifestRecord:
    """One runtime manifest entry for a single class + state."""

    class_id: str
    state: str
    image_path: str  # client-relative, normalized
    width: int = 0
    height: int = 0
    anchor_x: float = 0.5
    anchor_y: float = 0.5
    source_angle: float = 0.0  # degrees; 0 = pointing up
    scale: float = 1.0

    def to_dict(self) -> dict[str, Any]:
        return {
            "class_id": self.class_id,
            "state": self.state,
            "image_path": self.image_path,
            "width": self.width,
            "height": self.height,
            "anchor_x": self.anchor_x,
            "anchor_y": self.anchor_y,
            "source_angle": self.source_angle,
            "scale": self.scale,
        }


# ---------------------------------------------------------------------------
# Audit result
# ---------------------------------------------------------------------------


@dataclass
class AuditResult:
    """Result of catalog + manifest audit."""

    definitions: int = 0
    primary: int = 0
    aliases: int = 0
    unknown: int = 0
    cycles: int = 0
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        return len(self.errors) == 0

    def to_dict(self) -> dict[str, Any]:
        return {
            "definitions": self.definitions,
            "primary": self.primary,
            "aliases": self.aliases,
            "unknown": self.unknown,
            "cycles": self.cycles,
            "errors": list(self.errors),
            "warnings": list(self.warnings),
            "ok": self.ok,
        }


# ---------------------------------------------------------------------------
# Ship definition loading (from data/ships/*.toml)
# ---------------------------------------------------------------------------


def load_ship_definitions(ships_dir: Path = _SHIPS_DIR) -> dict[str, dict[str, Any]]:
    """Load all ship definition TOMLs keyed by file stem (catalog key)."""
    defs: dict[str, dict[str, Any]] = {}
    if not ships_dir.is_dir():
        return defs
    for p in sorted(ships_dir.glob("*.toml")):
        with open(p, "rb") as f:
            data = tomllib.load(f)
        key = p.stem
        defs[key] = data
    return defs


def load_size_names(sizes_file: Path = _SIZES_FILE) -> dict[int, str]:
    """Load size tier id -> name mapping from data/sizes.toml."""
    names: dict[int, str] = {}
    if not sizes_file.is_file():
        return names
    with open(sizes_file, "rb") as f:
        data = tomllib.load(f)
    for entry in data.get("sizes", []):
        sid = entry.get("id")
        name = entry.get("name", "")
        if sid is not None:
            names[sid] = name
    return names


def _extract_variant(class_id: str, display_name: str) -> str:
    """Extract variant cue from class_id or display name."""
    # Check parenthetical in display name: "Battleship (Heavy)" -> "heavy"
    if "(" in display_name and ")" in display_name:
        inside = display_name[display_name.index("(") + 1 : display_name.rindex(")")]
        return inside.strip().lower()
    # Check class_id suffix: _heavy, _light, _line, _double
    for suffix in ("_heavy", "_light", "_line", "_double"):
        if class_id.endswith(suffix):
            return suffix[1:]
    return ""


def _build_visual_description(display_name: str, size_name: str, variant: str, special: str) -> str:
    """Build a default visual description for prompt authoring."""
    parts = []
    if special == "tutorial":
        parts.append("tutorial variant of")
    if variant:
        parts.append(f"{variant} variant of the")
    else:
        parts.append("a")
    parts.append(size_name.lower() if size_name else "starship")
    desc = " ".join(parts)
    desc += f" ({display_name}), top-down view, pointing upward, clean silhouette on transparent background"
    return desc


# ---------------------------------------------------------------------------
# Catalog building
# ---------------------------------------------------------------------------

# Fixed alias mappings per the plan.
ALIASES = {
    "tutorial_escort": "escort",
    "tutorial_heavy_cruiser": "heavy_cruiser",
}

# Pilot hulls per Phase 5.
PILOT_HULLS = {"escort", "heavy_cruiser", "huge"}


def build_catalog(
    ships_dir: Path = _SHIPS_DIR,
    sizes_file: Path = _SIZES_FILE,
) -> list[CatalogEntry]:
    """Build the authoring catalog from ship definitions."""
    defs = load_ship_definitions(ships_dir)
    size_names = load_size_names(sizes_file)
    entries: list[CatalogEntry] = []

    for class_id in sorted(defs.keys()):
        d = defs[class_id]
        display_name = d.get("name", class_id)
        size_tier = d.get("size", 0)
        size_name = size_names.get(size_tier, "")
        variant = _extract_variant(class_id, display_name)

        special = ""
        if class_id.startswith("tutorial_"):
            special = "tutorial"
        if class_id == "starbase":
            special = "immobile"
        if class_id in PILOT_HULLS:
            special = "pilot" if not special else f"{special},pilot"

        if class_id in ALIASES:
            entry = CatalogEntry(
                class_id=class_id,
                display_name=display_name,
                kind="alias",
                alias_target=ALIASES[class_id],
                size_tier=size_tier,
                size_name=size_name,
                variant=variant,
                visual_description=_build_visual_description(
                    display_name, size_name, variant, "tutorial"
                ),
                desired_states=list(P0_STATES),
                special=special,
            )
        else:
            entry = CatalogEntry(
                class_id=class_id,
                display_name=display_name,
                kind="primary",
                alias_target=None,
                size_tier=size_tier,
                size_name=size_name,
                variant=variant,
                visual_description=_build_visual_description(
                    display_name, size_name, variant, special
                ),
                desired_states=list(P0_STATES),
                special=special,
            )
        entries.append(entry)

    return entries


def catalog_to_json(entries: list[CatalogEntry]) -> str:
    """Serialize catalog entries to deterministic JSON."""
    data = {
        "version": 1,
        "p0_states": list(P0_STATES),
        "aliases": dict(sorted(ALIASES.items())),
        "entries": [e.to_dict() for e in sorted(entries, key=lambda e: e.class_id)],
    }
    return json.dumps(data, indent=2, sort_keys=True) + "\n"


def write_catalog(entries: list[CatalogEntry], path: Path = CATALOG_PATH) -> None:
    """Write catalog JSON to disk."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(catalog_to_json(entries))


def load_catalog(path: Path = CATALOG_PATH) -> list[CatalogEntry]:
    """Load catalog entries from JSON."""
    if not path.is_file():
        return []
    data = json.loads(path.read_text())
    entries = []
    for raw in data.get("entries", []):
        entry = CatalogEntry(
            class_id=raw["class_id"],
            display_name=raw["display_name"],
            kind=raw["kind"],
            alias_target=raw.get("alias_target"),
            size_tier=raw.get("size_tier", 0),
            size_name=raw.get("size_name", ""),
            variant=raw.get("variant", ""),
            visual_description=raw.get("visual_description", ""),
            desired_states=raw.get("desired_states", list(P0_STATES)),
            special=raw.get("special", ""),
        )
        entries.append(entry)
    return entries


# ---------------------------------------------------------------------------
# Sidecar loading
# ---------------------------------------------------------------------------


def load_sidecars(
    assets_dir: Path = _ASSETS_DIR,
    errors: list[str] | None = None,
) -> dict[str, Sidecar]:
    """Load all sprite.toml sidecars from <assets_dir>/<class_id>/sprite.toml."""
    sidecars: dict[str, Sidecar] = {}
    if not assets_dir.is_dir():
        return sidecars
    for class_dir in sorted(assets_dir.iterdir()):
        if not class_dir.is_dir():
            continue
        sidecar_path = class_dir / "sprite.toml"
        if sidecar_path.is_file():
            try:
                sc = Sidecar.from_toml(sidecar_path)
            except (OSError, TypeError, ValueError, tomllib.TOMLDecodeError) as error:
                if errors is not None:
                    errors.append(f"invalid sidecar '{class_dir.name}': {error}")
                continue
            # Key by the authoritative directory/catalog identity so a
            # mismatched internal class_id remains visible to the audit.
            sidecars[class_dir.name] = sc
    return sidecars


# ---------------------------------------------------------------------------
# Path validation
# ---------------------------------------------------------------------------


def is_safe_relative_path(path_str: str, base: Path) -> bool:
    """Return True if path_str is relative, normalized, and stays inside base."""
    if not isinstance(path_str, str) or not path_str:
        return False
    # Runtime JSON paths use portable POSIX separators and must already be
    # normalized; accepting a path only after normalization can conceal a
    # traversal or produce platform-dependent manifests.
    if (
        "\\" in path_str
        or os.path.normpath(path_str) != path_str
        or path_str.split("/", 1)[0].endswith(":")
    ):
        return False
    if os.path.isabs(path_str):
        return False
    # Normalize and check for traversal.
    normalized = os.path.normpath(path_str)
    if normalized.startswith(".."):
        return False
    # Resolve against base and check containment.
    try:
        resolved = (base / normalized).resolve()
        base_resolved = base.resolve()
        resolved.relative_to(base_resolved)
    except (ValueError, RuntimeError):
        return False
    return True


def _state_asset(metadata: StateAsset | dict[str, Any]) -> StateAsset | None:
    if isinstance(metadata, StateAsset):
        return metadata
    try:
        return StateAsset.from_dict(metadata)
    except (TypeError, ValueError):
        return None


def state_asset_exists(
    metadata: StateAsset | dict[str, Any],
    assets_dir: Path = _ASSETS_DIR,
) -> bool:
    """Return whether state metadata points at an existing in-tree file."""
    asset = _state_asset(metadata)
    return asset is not None and asset.asset_exists(assets_dir)


def state_asset_valid(
    metadata: StateAsset | dict[str, Any],
    assets_dir: Path = _ASSETS_DIR,
) -> bool:
    """Return whether one state has complete metadata and a valid matching PNG."""
    asset = _state_asset(metadata)
    return asset is not None and asset.asset_valid(assets_dir)


def state_is_accepted(
    metadata: StateAsset | dict[str, Any],
    assets_dir: Path = _ASSETS_DIR,
) -> bool:
    """Return whether a state is explicitly reviewed and publishable."""
    asset = _state_asset(metadata)
    return asset is not None and asset.is_accepted(assets_dir)


# ---------------------------------------------------------------------------
# Manifest generation
# ---------------------------------------------------------------------------


def _resolve_alias_chain(class_id: str, catalog: list[CatalogEntry]) -> str:
    """Follow alias chain to the ultimate primary class_id.

    Raises ValueError if a cycle is detected.
    """
    by_id = {e.class_id: e for e in catalog}
    seen: set[str] = set()
    current = class_id
    while current in by_id:
        entry = by_id[current]
        if entry.kind != "alias":
            return current
        if current in seen:
            raise ValueError(f"alias cycle detected at {current}")
        seen.add(current)
        target = entry.alias_target
        if target is None:
            return current
        current = target
    return current


def generate_manifest(
    catalog: list[CatalogEntry],
    sidecars: dict[str, Sidecar] | None = None,
    assets_dir: Path = _ASSETS_DIR,
) -> list[ManifestRecord]:
    """Generate runtime manifest records from catalog + valid sidecars.

    Only primary entries with complete sidecar state descriptors produce
    manifest records.  Alias entries resolve to their target's records
    (re-keyed to the alias class_id).
    """
    if sidecars is None:
        sidecars = load_sidecars(assets_dir)

    by_id = {e.class_id: e for e in catalog}
    records: list[ManifestRecord] = []

    # First, build records for primaries that have sidecars.
    primary_records: dict[str, list[ManifestRecord]] = {}
    for entry in catalog:
        if entry.kind != "primary":
            continue
        sc = sidecars.get(entry.class_id)
        if sc is None:
            continue
        for state in P0_STATES:
            state_data = sc.states.get(state)
            if state_data is None:
                continue
            if not state_is_accepted(state_data, assets_dir):
                continue
            rec = state_data.to_manifest_record(entry.class_id, state)
            primary_records.setdefault(entry.class_id, []).append(rec)

    # Emit primary records.
    for class_id in sorted(primary_records.keys()):
        records.extend(primary_records[class_id])

    # Emit alias records (re-keyed from target).
    for entry in sorted(catalog, key=lambda e: e.class_id):
        if entry.kind != "alias":
            continue
        try:
            target = _resolve_alias_chain(entry.class_id, catalog)
        except ValueError:
            continue
        target_recs = primary_records.get(target, [])
        for rec in target_recs:
            alias_rec = ManifestRecord(
                class_id=entry.class_id,
                state=rec.state,
                image_path=rec.image_path,
                width=rec.width,
                height=rec.height,
                anchor_x=rec.anchor_x,
                anchor_y=rec.anchor_y,
                source_angle=rec.source_angle,
                scale=rec.scale,
            )
            records.append(alias_rec)

    return records


def manifest_to_json(records: list[ManifestRecord]) -> str:
    """Serialize manifest records to deterministic JSON."""
    data = {
        "version": 1,
        "records": [r.to_dict() for r in sorted(records, key=lambda r: (r.class_id, r.state))],
    }
    return json.dumps(data, indent=2, sort_keys=True) + "\n"


def write_manifest(records: list[ManifestRecord], path: Path = MANIFEST_PATH) -> None:
    """Atomically publish manifest JSON."""
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(manifest_to_json(records))
    os.replace(temporary, path)


def _restore_file(path: Path, contents: bytes | None) -> None:
    """Restore one file snapshot without exposing a partially written file."""
    if contents is None:
        path.unlink(missing_ok=True)
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".rollback")
    temporary.write_bytes(contents)
    os.replace(temporary, path)


@contextmanager
def asset_publication(
    catalog: list[CatalogEntry],
    assets_dir: Path,
    changed_paths: list[Path],
):
    """Publish asset files, sidecars, and the runtime manifest as one unit.

    Callers perform atomic image/sidecar mutations inside the context. On a
    successful exit the manifest is rebuilt immediately from the resulting
    accepted states. Any mutation or manifest failure restores every supplied
    path and the prior manifest, so a stale record cannot expose unreviewed
    pixels that reused an accepted image path.
    """
    assets_dir = Path(assets_dir)
    manifest_path = assets_dir / "manifest.json"
    paths = list(
        dict.fromkeys([Path(path) for path in changed_paths] + [manifest_path])
    )
    snapshots = {
        path: path.read_bytes() if path.is_file() else None
        for path in paths
    }
    try:
        yield
        write_manifest(
            generate_manifest(catalog, assets_dir=assets_dir),
            path=manifest_path,
        )
    except Exception:
        for path, contents in snapshots.items():
            _restore_file(path, contents)
        raise


def publish_review_status(
    catalog: list[CatalogEntry],
    assets_dir: Path,
    *,
    class_id: str,
    state: str,
    review_status: str,
) -> None:
    """Change a reviewer decision and runtime visibility atomically."""
    sidecar_path = Path(assets_dir) / class_id / "sprite.toml"
    with asset_publication(catalog, Path(assets_dir), [sidecar_path]):
        set_review_status(
            sidecar_path,
            class_id=class_id,
            state=state,
            review_status=review_status,
        )


def load_manifest(path: Path = MANIFEST_PATH) -> list[ManifestRecord]:
    """Load manifest records from JSON."""
    if not path.is_file():
        return []
    data = json.loads(path.read_text())
    records = []
    for raw in data.get("records", []):
        rec = ManifestRecord(
            class_id=raw["class_id"],
            state=raw["state"],
            image_path=raw["image_path"],
            width=raw.get("width", 0),
            height=raw.get("height", 0),
            anchor_x=raw.get("anchor_x", 0.5),
            anchor_y=raw.get("anchor_y", 0.5),
            source_angle=raw.get("source_angle", 0.0),
            scale=raw.get("scale", 1.0),
        )
        records.append(rec)
    return records


def manifest_sha256(records: list[ManifestRecord]) -> str:
    """Return SHA-256 of the deterministic manifest JSON."""
    return hashlib.sha256(manifest_to_json(records).encode()).hexdigest()


# ---------------------------------------------------------------------------
# Audit
# ---------------------------------------------------------------------------


def audit(
    catalog: list[CatalogEntry] | None = None,
    ships_dir: Path = _SHIPS_DIR,
    assets_dir: Path = _ASSETS_DIR,
) -> AuditResult:
    """Run all catalog and manifest audit rules."""
    if catalog is None:
        catalog = load_catalog()
    defs = load_ship_definitions(ships_dir)
    result = AuditResult()

    # Count definitions.
    result.definitions = len(defs)

    by_id = {e.class_id: e for e in catalog}

    # Check every ship definition resolves to a catalog entry.
    for def_id in sorted(defs.keys()):
        internal_id = defs[def_id].get("id")
        # Rust treats omitted and explicitly empty ids as "use the
        # authoritative file stem". Keep the offline audit in exact parity.
        if internal_id not in (None, "") and internal_id != def_id:
            result.errors.append(
                f"definition '{def_id}' internal id '{internal_id}' does not match file stem"
            )
        if def_id not in by_id:
            result.errors.append(f"definition '{def_id}' has no catalog entry")
            result.unknown += 1

    # Check for unknown catalog IDs (in catalog but not in definitions).
    for entry in catalog:
        if entry.class_id not in defs:
            result.errors.append(f"catalog entry '{entry.class_id}' has no ship definition")
            result.unknown += 1

    # Count primaries and aliases.
    for entry in catalog:
        if not is_safe_identifier(entry.class_id):
            result.errors.append(f"catalog entry has invalid class_id '{entry.class_id}'")
        if entry.kind == "primary":
            result.primary += 1
        elif entry.kind == "alias":
            result.aliases += 1
        else:
            result.errors.append(f"entry '{entry.class_id}' has unknown kind '{entry.kind}'")

    # Check alias targets exist and no self-aliases.
    for entry in catalog:
        if entry.kind != "alias":
            continue
        if entry.alias_target is None:
            result.errors.append(f"alias '{entry.class_id}' has no alias_target")
            continue
        if entry.alias_target == entry.class_id:
            result.errors.append(f"alias '{entry.class_id}' self-aliases")
            continue
        if entry.alias_target not in by_id:
            result.errors.append(
                f"alias '{entry.class_id}' targets unknown class '{entry.alias_target}'"
            )

    # Check for cycles in alias graph.
    for entry in catalog:
        if entry.kind != "alias":
            continue
        try:
            _resolve_alias_chain(entry.class_id, catalog)
        except ValueError:
            result.errors.append(f"alias cycle detected starting at '{entry.class_id}'")
            result.cycles += 1

    # Validate canonical identity and required state metadata. Partial sidecars
    # are allowed, but every state that is declared must be renderable and
    # traceable.
    sidecars = load_sidecars(assets_dir, result.errors)
    for class_id, sc in sidecars.items():
        if sc.class_id != class_id:
            result.errors.append(
                f"sidecar directory '{class_id}' declares class_id '{sc.class_id}'"
            )
        if class_id not in by_id:
            result.errors.append(f"sidecar '{class_id}' has no catalog entry")
        for state, state_data in sc.states.items():
            image_path = state_data.image_path
            if not is_safe_relative_path(image_path, assets_dir):
                result.errors.append(
                    f"sidecar '{class_id}' state '{state}' has unsafe path '{image_path}'"
                )
            elif not (assets_dir / image_path).is_file():
                result.errors.append(
                    f"sidecar '{class_id}' state '{state}' asset does not exist: '{image_path}'"
                )
            else:
                try:
                    from PIL import Image

                    with Image.open(assets_dir / image_path) as image:
                        actual_size = image.size
                        image.verify()
                    declared_size = (state_data.width, state_data.height)
                    if declared_size != actual_size:
                        result.errors.append(
                            f"sidecar '{class_id}' state '{state}' dimensions "
                            f"{declared_size} do not match asset {actual_size}"
                        )
                except (OSError, ValueError) as error:
                    result.errors.append(
                        f"sidecar '{class_id}' state '{state}' asset is invalid: {error}"
                    )

    return result


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def _cmd_audit() -> int:
    result = audit()
    print(json.dumps(result.to_dict(), indent=2, sort_keys=True))
    return 0 if result.ok else 1


def _cmd_write_manifest() -> int:
    catalog = load_catalog()
    records = generate_manifest(catalog)
    write_manifest(records)
    sha = manifest_sha256(records)
    print(f"manifest written: {len(records)} records, sha256={sha}")
    return 0


def _cmd_check_manifest() -> int:
    """Verify committed manifest matches freshly generated one."""
    catalog = load_catalog()
    fresh = generate_manifest(catalog)
    fresh_json = manifest_to_json(fresh)
    if not MANIFEST_PATH.is_file():
        print("manifest missing — run --write-manifest first")
        return 1
    committed_json = MANIFEST_PATH.read_text()
    if fresh_json == committed_json:
        sha = manifest_sha256(fresh)
        print(f"manifest up to date, sha256={sha}")
        return 0
    print("manifest is stale — run --write-manifest")
    return 1


def main(argv: list[str] | None = None) -> int:
    args = argv if argv is not None else sys.argv[1:]
    if not args:
        print("usage: ship_art_catalog.py --audit|--write-manifest|--check-manifest", file=sys.stderr)
        return 2
    cmd = args[0]
    if cmd == "--audit":
        return _cmd_audit()
    elif cmd == "--write-manifest":
        return _cmd_write_manifest()
    elif cmd == "--check-manifest":
        return _cmd_check_manifest()
    else:
        print(f"unknown command: {cmd}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
