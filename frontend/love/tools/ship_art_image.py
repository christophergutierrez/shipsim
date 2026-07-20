#!/usr/bin/env python3
"""Image processing primitives for ship art generation.

Ports the reusable NorRust image primitives behind shipsim-specific interfaces.
All functions are provider-free and testable with fixture PNGs.

Phase 3 of ``docs/SHIP-ART-IMPLEMENTATION-PLAN.md``.
"""

from __future__ import annotations

import base64
import io
import math
import os
import shutil
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

FRAME_SIZE = 256
PORTRAIT_SIZE = 128

# Default validation thresholds (pilot-tunable).
DEFAULT_CHROMA_THRESHOLD = 100
DEFAULT_PORTRAIT_THRESHOLD = 60
DEFAULT_SIZE_HARD_LIMIT = 30720  # 30 KB
DEFAULT_SIZE_WARN_LIMIT = 20480  # 20 KB
DEFAULT_EDGE_BORDER = 2
DEFAULT_PORTRAIT_SIZE_LIMIT = 102400  # 100 KB


# ---------------------------------------------------------------------------
# Validation outcome classification
# ---------------------------------------------------------------------------


class Outcome(str, Enum):
    """Validation outcome classification."""

    PASS = "pass"
    WARNING = "warning"
    BLOCKING = "blocking"


@dataclass
class ValidationResult:
    """Result of validating a processed image."""

    outcome: Outcome = Outcome.PASS
    issues: list[str] = field(default_factory=list)
    file_bytes: int = 0
    blob_count: int = 0
    edge_count: int = 0
    has_content: bool = True

    @property
    def ok(self) -> bool:
        return self.outcome != Outcome.BLOCKING

    def to_dict(self) -> dict[str, Any]:
        return {
            "outcome": self.outcome.value,
            "issues": list(self.issues),
            "file_bytes": self.file_bytes,
            "blob_count": self.blob_count,
            "edge_count": self.edge_count,
            "has_content": self.has_content,
        }


# ---------------------------------------------------------------------------
# Base64 reference loading
# ---------------------------------------------------------------------------


def load_image_base64(path: str | Path) -> str:
    """Load an image file and return base64-encoded data."""
    with open(path, "rb") as f:
        return base64.b64encode(f.read()).decode("ascii")


def decode_image_bytes(b64_data: str) -> bytes:
    """Decode base64 image data to raw bytes."""
    return base64.b64decode(b64_data)


# ---------------------------------------------------------------------------
# Chroma-background removal
# ---------------------------------------------------------------------------


def _sample_corner_color(img, w: int, h: int) -> tuple[int, int, int]:
    """Sample background color from the four corners of an image."""
    from PIL import Image

    pixels = img.load()
    corners = [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)]
    return tuple(
        sum(pixels[c][i] for c in corners) // 4 for i in range(3)
    )


def remove_chroma_background(
    img,
    threshold: float = DEFAULT_CHROMA_THRESHOLD,
) -> tuple[Any, tuple[int, int, int]]:
    """Remove chroma background from an RGBA image.

    Samples the corner color, then makes pixels within ``threshold`` distance
    fully transparent.  Also removes pink/magenta artifacts.

    Returns (processed_image, background_color).
    """
    pixels = img.load()
    w, h = img.width, img.height
    bg = _sample_corner_color(img, w, h)

    for y in range(h):
        for x in range(w):
            r, g, b, a = pixels[x, y]
            dist = math.sqrt((r - bg[0]) ** 2 + (g - bg[1]) ** 2 + (b - bg[2]) ** 2)
            if dist < threshold:
                pixels[x, y] = (0, 0, 0, 0)
            elif r > 180 and b > 150 and g < 120:
                # Remove pink/magenta artifacts
                pixels[x, y] = (0, 0, 0, 0)

    return img, bg


# ---------------------------------------------------------------------------
# Transparent-content bounds and centering
# ---------------------------------------------------------------------------


def find_content_bounds(img) -> tuple[int, int, int, int] | None:
    """Find the bounding box of non-transparent content.

    Returns (left, top, right, bottom) inclusive, or None if no content.
    """
    pixels = img.load()
    w, h = img.width, img.height
    left, right = w, 0
    top, bottom = h, 0

    for y in range(h):
        for x in range(w):
            if pixels[x, y][3] > 0:
                left = min(left, x)
                right = max(right, x)
                top = min(top, y)
                bottom = max(bottom, y)

    if right < left:
        return None
    return (left, top, right, bottom)


def center_on_canvas(
    img,
    canvas_size: int = FRAME_SIZE,
    bg_color: tuple[int, int, int, int] = (0, 0, 0, 0),
) -> Any:
    """Center an image on a square canvas of the given size."""
    from PIL import Image

    frame = Image.new("RGBA", (canvas_size, canvas_size), bg_color)
    x_off = (canvas_size - img.width) // 2
    y_off = (canvas_size - img.height) // 2
    frame.paste(img, (x_off, y_off))
    return frame


# ---------------------------------------------------------------------------
# Edge check
# ---------------------------------------------------------------------------


def check_edges(img, border: int = DEFAULT_EDGE_BORDER) -> tuple[bool, int]:
    """Check that no opaque pixels exist in the outermost border.

    Returns (ok, edge_pixel_count).
    """
    pixels = img.load()
    w, h = img.width, img.height
    edge_count = 0

    for y in range(h):
        for x in range(w):
            if x < border or x >= w - border or y < border or y >= h - border:
                if pixels[x, y][3] > 0:
                    edge_count += 1

    return edge_count == 0, edge_count


# ---------------------------------------------------------------------------
# File-size check
# ---------------------------------------------------------------------------


def check_file_size(
    path: str | Path,
    hard_limit: int = DEFAULT_SIZE_HARD_LIMIT,
    warn_limit: int = DEFAULT_SIZE_WARN_LIMIT,
) -> tuple[bool, int, bool]:
    """Check file size against limits.

    Returns (ok, file_bytes, is_warning).
    """
    file_bytes = os.path.getsize(path)
    if file_bytes > hard_limit:
        return False, file_bytes, False
    if file_bytes > warn_limit:
        return True, file_bytes, True
    return True, file_bytes, False


# ---------------------------------------------------------------------------
# Empty-mask check
# ---------------------------------------------------------------------------


def check_empty_mask(img) -> tuple[bool, int]:
    """Check if the image has any non-transparent content.

    Returns (has_content, opaque_pixel_count).
    """
    pixels = img.load()
    w, h = img.width, img.height
    count = 0
    for y in range(h):
        for x in range(w):
            if pixels[x, y][3] > 0:
                count += 1
    return count > 0, count


# ---------------------------------------------------------------------------
# Connected-component check (multi-blob detection)
# ---------------------------------------------------------------------------


def count_connected_components(img) -> tuple[int, int]:
    """Count connected components of opaque pixels via flood fill.

    Returns (significant_blob_count, total_opaque_pixels).

    A blob is "significant" if it is at least 5% of total opaque pixels.
    Small detached glows/details are not counted as significant.
    """
    pixels = img.load()
    w, h = img.width, img.height

    visited = [[False] * w for _ in range(h)]
    total_opaque = 0
    for y in range(h):
        for x in range(w):
            if pixels[x, y][3] > 0:
                total_opaque += 1

    if total_opaque == 0:
        return 0, 0

    threshold = total_opaque * 0.05
    blob_count = 0

    for sy in range(h):
        for sx in range(w):
            if visited[sy][sx] or pixels[sx, sy][3] == 0:
                continue
            # BFS flood-fill
            queue = [(sx, sy)]
            visited[sy][sx] = True
            size = 0
            while queue:
                cx, cy = queue.pop()
                size += 1
                for dx, dy in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                    nx, ny = cx + dx, cy + dy
                    if (
                        0 <= nx < w
                        and 0 <= ny < h
                        and not visited[ny][nx]
                        and pixels[nx, ny][3] > 0
                    ):
                        visited[ny][nx] = True
                        queue.append((nx, ny))
            if size >= threshold:
                blob_count += 1

    return blob_count, total_opaque


# ---------------------------------------------------------------------------
# Top-down sprite processing
# ---------------------------------------------------------------------------


def process_top_down(
    img_data: bytes,
    out_path: str | Path,
    threshold: float = DEFAULT_CHROMA_THRESHOLD,
    frame_size: int = FRAME_SIZE,
) -> tuple[bool, int, int]:
    """Process a generated top-down image: resize, remove bg, center.

    Returns (ok, pad_top, pad_bottom).
    """
    from PIL import Image

    img = Image.open(io.BytesIO(img_data)).convert("RGBA")

    # Scale to square, fitting into frame_size x frame_size
    max_dim = max(img.width, img.height)
    scale = frame_size / max_dim
    new_w = int(img.width * scale)
    new_h = int(img.height * scale)
    img = img.resize((new_w, new_h), Image.NEAREST)

    # Sample background color from corners
    bg = _sample_corner_color(img, new_w, new_h)

    # Find content bounding box (before chroma removal, using bg distance)
    pixels = img.load()
    left_c, right_c = new_w, 0
    top_c, bot_c = new_h, 0
    for y in range(new_h):
        for x in range(new_w):
            r, g, b, a = pixels[x, y]
            dist = math.sqrt((r - bg[0]) ** 2 + (g - bg[1]) ** 2 + (b - bg[2]) ** 2)
            if dist >= threshold:
                left_c = min(left_c, x)
                right_c = max(right_c, x)
                top_c = min(top_c, y)
                bot_c = max(bot_c, y)

    # No content found — save blank frame
    if right_c < left_c:
        frame = Image.new("RGBA", (frame_size, frame_size), (0, 0, 0, 0))
        frame.save(out_path)
        return False, 0, 0

    content_w = right_c - left_c + 1
    content_h = bot_c - top_c + 1

    # Crop to content
    crop = img.crop((left_c, top_c, right_c + 1, bot_c + 1))

    # Fit into frame_size with padding
    fit_scale = min((frame_size - 10) / content_w, (frame_size - 10) / content_h)
    if fit_scale < 1.0:
        crop = crop.resize(
            (int(content_w * fit_scale), int(content_h * fit_scale)), Image.NEAREST
        )

    # Center on frame_size x frame_size canvas
    frame = Image.new("RGBA", (frame_size, frame_size), (bg[0], bg[1], bg[2], 255))
    x_off = (frame_size - crop.width) // 2
    y_off = (frame_size - crop.height) // 2
    frame.paste(crop, (x_off, y_off))

    # Remove background + pink artifacts
    frame, _ = remove_chroma_background(frame, threshold)

    # Verify padding
    bounds = find_content_bounds(frame)
    if bounds is None:
        frame.save(out_path)
        return False, 0, 0

    _, top_p, _, bot_p = bounds
    pad_top = top_p
    pad_bot = frame_size - 1 - bot_p
    ok = pad_bot >= 3 and pad_top >= 3

    frame.save(out_path)
    return ok, pad_top, pad_bot


# ---------------------------------------------------------------------------
# Portrait processing
# ---------------------------------------------------------------------------


def process_portrait(
    img_data: bytes,
    out_path: str | Path,
    threshold: float = DEFAULT_PORTRAIT_THRESHOLD,
    portrait_size: int = PORTRAIT_SIZE,
) -> tuple[bool, int]:
    """Process a portrait: scale, center on black, clean near-black edges.

    Returns (ok, file_bytes).
    """
    from PIL import Image

    img = Image.open(io.BytesIO(img_data)).convert("RGB")

    # Scale to fit portrait_size x portrait_size
    max_dim = max(img.width, img.height)
    scale = portrait_size / max_dim
    new_w = int(img.width * scale)
    new_h = int(img.height * scale)
    img = img.resize((new_w, new_h), Image.LANCZOS)

    # Center on black canvas
    frame = Image.new("RGB", (portrait_size, portrait_size), (0, 0, 0))
    x_off = (portrait_size - new_w) // 2
    y_off = (portrait_size - new_h) // 2
    frame.paste(img, (x_off, y_off))

    # Clean near-black edges to pure black
    pixels = frame.load()
    for y in range(portrait_size):
        for x in range(portrait_size):
            r, g, b = pixels[x, y]
            if r < threshold and g < threshold and b < threshold:
                pixels[x, y] = (0, 0, 0)

    frame.save(out_path)
    file_bytes = os.path.getsize(out_path)
    return True, file_bytes


# ---------------------------------------------------------------------------
# Resize, flop, trim, and reversible backup operations
# ---------------------------------------------------------------------------


def resize_image(img, new_w: int, new_h: int) -> Any:
    """Resize an image to the given dimensions."""
    from PIL import Image

    return img.resize((new_w, new_h), Image.NEAREST)


def flop_image(img) -> Any:
    """Mirror an image horizontally (left-right flip)."""
    from PIL import Image

    return img.transpose(Image.FLIP_LEFT_RIGHT)


def trim_to_content(img) -> Any:
    """Crop an image to its non-transparent content bounds."""
    bounds = find_content_bounds(img)
    if bounds is None:
        return img
    left, top, right, bottom = bounds
    return img.crop((left, top, right + 1, bottom + 1))


def reversible_backup(src: str | Path, backup_dir: str | Path) -> Path | None:
    """Create a reversible backup of a file.

    Returns the backup path, or None if the source does not exist.
    """
    src = Path(src)
    if not src.is_file():
        return None
    backup_dir = Path(backup_dir)
    backup_dir.mkdir(parents=True, exist_ok=True)
    backup_path = backup_dir / src.name
    shutil.copy2(src, backup_path)
    return backup_path


# ---------------------------------------------------------------------------
# Full validation
# ---------------------------------------------------------------------------


def validate_image(
    img_path: str | Path,
    size_hard_limit: int = DEFAULT_SIZE_HARD_LIMIT,
    size_warn_limit: int = DEFAULT_SIZE_WARN_LIMIT,
    edge_border: int = DEFAULT_EDGE_BORDER,
    is_portrait: bool = False,
) -> ValidationResult:
    """Run all validation checks on a processed image.

    Classifies outcomes as blocking error, warning, or pass.

    For top-down sprites:
      - empty mask: blocking
      - duplicate full silhouettes (multi-blob): blocking
      - small detached glow/details: warning
      - edge-clipped: blocking
      - oversize: blocking (hard limit) or warning (warn limit)

    For portraits:
      - oversize: blocking (portrait size limit)
      - empty: blocking
    """
    from PIL import Image

    result = ValidationResult()

    img = Image.open(img_path).convert("RGBA")

    # Empty mask check
    has_content, opaque_count = check_empty_mask(img)
    result.has_content = has_content
    if not has_content:
        result.outcome = Outcome.BLOCKING
        result.issues.append("empty mask (no opaque pixels)")
        result.file_bytes = os.path.getsize(img_path)
        return result

    if is_portrait:
        # Portrait: only check size and emptiness.
        file_bytes = os.path.getsize(img_path)
        result.file_bytes = file_bytes
        if file_bytes > DEFAULT_PORTRAIT_SIZE_LIMIT:
            result.outcome = Outcome.BLOCKING
            result.issues.append(f"oversized ({file_bytes} bytes > {DEFAULT_PORTRAIT_SIZE_LIMIT})")
        return result

    # Top-down sprite checks

    # Multi-blob check
    blob_count, total_opaque = count_connected_components(img)
    result.blob_count = blob_count
    if blob_count > 1:
        result.outcome = Outcome.BLOCKING
        result.issues.append(f"multi-blob ({blob_count} significant blobs)")
    elif blob_count == 0 and total_opaque > 0:
        # All blobs are small (< 5% of total) — warning, not blocking.
        if result.outcome == Outcome.PASS:
            result.outcome = Outcome.WARNING
        result.issues.append(f"only small detached components ({total_opaque} pixels)")

    # Size check
    size_ok, file_bytes, is_warn = check_file_size(
        img_path, size_hard_limit, size_warn_limit
    )
    result.file_bytes = file_bytes
    if not size_ok:
        result.outcome = Outcome.BLOCKING
        result.issues.append(f"oversized ({file_bytes} bytes > {size_hard_limit})")
    elif is_warn:
        if result.outcome == Outcome.PASS:
            result.outcome = Outcome.WARNING
        result.issues.append(f"size warning ({file_bytes} bytes > {size_warn_limit})")

    # Edge check
    edge_ok, edge_count = check_edges(img, edge_border)
    result.edge_count = edge_count
    if not edge_ok:
        result.outcome = Outcome.BLOCKING
        result.issues.append(f"edge-clipped ({edge_count} border pixels)")

    return result


# ---------------------------------------------------------------------------
# Atomic write
# ---------------------------------------------------------------------------


def atomic_write(data: bytes, dest: str | Path) -> None:
    """Write data to dest atomically: write to temp, then rename."""
    dest = Path(dest)
    dest.parent.mkdir(parents=True, exist_ok=True)
    tmp = dest.with_suffix(dest.suffix + ".tmp")
    with open(tmp, "wb") as f:
        f.write(data)
    os.replace(tmp, dest)
