# Ship art assets (Love2D runtime)

Authoring / production rationale: [`PRD.md`](PRD.md). This file is the
runtime contract.

## How a ship TOML gets an image

The engine never loads art. Association is by **catalog key**:

| Layer | Field / path |
|---|---|
| Ship definition | `data/ships/<class_id>.toml` (`id` must match the file stem) |
| Snapshot (play) | `ship.class_id` (same string) |
| Runtime index | `manifest.json` records keyed by `class_id` + state |
| Files on disk | `assets/ship_art/<class_id>/…` |

Love reads **only** `manifest.json` at play time. Empty or missing records fall
back to the geometric marker.

## Drop images and register (no generator)

1. Put board art (source facing **up**) and optional portrait under the class id:

```text
frontend/love/assets/ship_art/
  destroyer_line/
    top_down.png      # board sprite (required for board art)
    portrait.png      # HUD thumbnail (optional)
  escort/
    top_down.png
  …
```

Class ids match `data/ships/*.toml` stems (`destroyer_line`, `escort`, …).
Tutorial aliases (`tutorial_escort` → `escort`) are defined in `catalog.json`
and do not need their own folders.

2. Register accepted images and rebuild the runtime manifest:

```bash
python3 frontend/love/tools/ship_art_catalog.py --register-images
```

3. Play Love2D from the repo (or `frontend/love`):

```bash
cargo build -q
./frontend/love/play.sh
# or: love frontend/love
```

Ships whose `class_id` has a `top_down` manifest record use that sprite;
everyone else still uses the circle marker.

## Rebuild manifest only

If sidecars already exist and you only changed review status:

```bash
python3 frontend/love/tools/ship_art_catalog.py --write-manifest
python3 frontend/love/tools/ship_art_catalog.py --check-manifest
```

## Files

| Path | Role |
|---|---|
| `catalog.json` | Authoring catalog (primaries + aliases); not read by Love at runtime |
| `manifest.json` | **Runtime** index Love loads |
| `<class_id>/sprite.toml` | Sidecar metadata used to build the manifest |
| `<class_id>/*.png` | Image pixels |

AI generation / the tkinter reviewer are optional; this register path is enough
to associate existing images with ship classes for play.
