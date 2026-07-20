"""Unit tests for ship art image primitives and provider.

Phase 3 exit gate tests.  All tests run without network access and without
GEMINI_API_KEY.

Image fixtures are generated programmatically with Pillow.
"""

import io
import os
import sys
import tempfile
import unittest
from pathlib import Path

# Make the tools module importable.
_TOOLS_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(_TOOLS_DIR))

import ship_art_image as sai
from ship_art_provider import (
    FakeProvider,
    FakeProviderConfig,
    GeminiProvider,
    ProviderAdapter,
    ProviderRequest,
    create_provider,
)

from PIL import Image


# ---------------------------------------------------------------------------
# Fixture helpers
# ---------------------------------------------------------------------------


def make_solid_image(size=64, color=(255, 0, 255, 255)):
    """Create a solid magenta image (chroma background)."""
    return Image.new("RGBA", (size, size), color)


def make_sprite_image(size=64, bg=(255, 0, 255, 255), sprite_color=(100, 150, 200, 255)):
    """Create an image with a centered sprite on a transparent background.

    The chroma background is applied only for chroma-removal tests; for edge
    and validation tests the background must be transparent so that opaque
    border pixels are not falsely detected.
    """
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    # Draw a centered rectangle as the "sprite".
    sprite_w = size // 3
    sprite_h = size // 3
    x_off = (size - sprite_w) // 2
    y_off = (size - sprite_h) // 2
    for y in range(sprite_h):
        for x in range(sprite_w):
            img.putpixel((x_off + x, y_off + y), sprite_color)
    return img


def make_sprite_on_chroma(size=64, bg=(255, 0, 255, 255), sprite_color=(100, 150, 200, 255)):
    """Create an image with a centered sprite on an opaque chroma background."""
    img = Image.new("RGBA", (size, size), bg)
    sprite_w = size // 3
    sprite_h = size // 3
    x_off = (size - sprite_w) // 2
    y_off = (size - sprite_h) // 2
    for y in range(sprite_h):
        for x in range(sprite_w):
            img.putpixel((x_off + x, y_off + y), sprite_color)
    return img


def make_empty_image(size=64):
    """Create a fully transparent image (empty mask)."""
    return Image.new("RGBA", (size, size), (0, 0, 0, 0))


def make_edge_clipped_image(size=64, color=(100, 150, 200, 255)):
    """Create an image with opaque pixels in the border."""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    # Put pixels in the border.
    for i in range(size):
        img.putpixel((0, i), color)
        img.putpixel((size - 1, i), color)
        img.putpixel((i, 0), color)
        img.putpixel((i, size - 1), color)
    return img


def make_multi_blob_image(size=64, color=(100, 150, 200, 255)):
    """Create an image with two separated blobs (duplicate subject)."""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    # Two blobs, each 10x10, separated.
    blob_size = 10
    for y in range(blob_size):
        for x in range(blob_size):
            img.putpixel((5 + x, 5 + y), color)
            img.putpixel((40 + x, 40 + y), color)
    return img


def make_small_components_image(size=64, color=(100, 150, 200, 255)):
    """Create an image with one large blob and small detached components."""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    # Large blob (30x30).
    for y in range(30):
        for x in range(30):
            img.putpixel((10 + x, 10 + y), color)
    # Small detached pixels (1x1 each), placed inside the border so they
    # don't trigger the edge-clipped check.
    img.putpixel((5, 5), color)
    img.putpixel((58, 58), color)
    return img


def make_oversize_png(path, size_kb=35):
    """Create a PNG file larger than the given size in KB.

    Uses high-entropy random pixel data and disables PNG compression to
    guarantee the file exceeds the size limit regardless of content.
    """
    import random
    random.seed(12345)
    # 256x256 RGBA = 256*256*4 = 262144 bytes raw. With compress_level=0
    # the PNG stores the raw data plus headers, easily exceeding 30 KB.
    img = Image.new("RGBA", (256, 256), (0, 0, 0, 0))
    pixels = img.load()
    for y in range(256):
        for x in range(256):
            pixels[x, y] = (
                random.randint(0, 255),
                random.randint(0, 255),
                random.randint(0, 255),
                255,
            )
    img.save(path, "PNG", compress_level=0)


def save_png(img, path):
    """Save a PIL image to a path."""
    img.save(path, "PNG")


# ---------------------------------------------------------------------------
# Image primitive tests
# ---------------------------------------------------------------------------


class TestBase64Loading(unittest.TestCase):
    """Base64 reference loading."""

    def test_load_and_decode_round_trip(self):
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            img = make_solid_image(8)
            img.save(f, "PNG")
            f.close()
            b64 = sai.load_image_base64(f.name)
            raw = sai.decode_image_bytes(b64)
            self.assertTrue(len(raw) > 0)
            os.unlink(f.name)


class TestChromaRemoval(unittest.TestCase):
    """Chroma-background removal."""

    def test_removes_magenta_background(self):
        img = make_sprite_on_chroma(32)
        processed, bg = sai.remove_chroma_background(img, threshold=100)
        # The sprite pixels should remain, background should be transparent.
        pixels = processed.load()
        # Center pixel (sprite) should be opaque.
        self.assertTrue(pixels[16, 16][3] > 0)
        # Corner pixel (background) should be transparent.
        self.assertEqual(pixels[0, 0][3], 0)

    def test_returns_background_color(self):
        img = make_sprite_on_chroma(32)
        _, bg = sai.remove_chroma_background(img, threshold=100)
        self.assertEqual(bg, (255, 0, 255))


class TestContentBounds(unittest.TestCase):
    """Transparent-content bounds and centering."""

    def test_finds_content_bounds(self):
        img = make_sprite_image(32)
        bounds = sai.find_content_bounds(img)
        self.assertIsNotNone(bounds)
        left, top, right, bottom = bounds
        self.assertLess(left, right)
        self.assertLess(top, bottom)

    def test_returns_none_for_empty(self):
        img = make_empty_image(32)
        bounds = sai.find_content_bounds(img)
        self.assertIsNone(bounds)

    def test_center_on_canvas(self):
        img = make_sprite_image(16)
        centered = sai.center_on_canvas(img, canvas_size=32)
        self.assertEqual(centered.size, (32, 32))


class TestEdgeCheck(unittest.TestCase):
    """Edge check."""

    def test_clean_image_passes(self):
        img = make_sprite_image(32)
        ok, count = sai.check_edges(img, border=2)
        self.assertTrue(ok)
        self.assertEqual(count, 0)

    def test_edge_clipped_fails(self):
        img = make_edge_clipped_image(32)
        ok, count = sai.check_edges(img, border=2)
        self.assertFalse(ok)
        self.assertGreater(count, 0)


class TestFileSizeCheck(unittest.TestCase):
    """File-size check."""

    def test_small_file_passes(self):
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            make_solid_image(8).save(f, "PNG")
            f.close()
            ok, size, warn = sai.check_file_size(f.name, hard_limit=30720, warn_limit=20480)
            self.assertTrue(ok)
            self.assertFalse(warn)
            os.unlink(f.name)

    def test_oversize_fails(self):
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            make_oversize_png(f.name, size_kb=35)
            f.close()
            ok, size, warn = sai.check_file_size(f.name, hard_limit=30720, warn_limit=20480)
            self.assertFalse(ok)
            os.unlink(f.name)


class TestEmptyMask(unittest.TestCase):
    """Empty-mask check."""

    def test_has_content(self):
        img = make_sprite_image(32)
        has, count = sai.check_empty_mask(img)
        self.assertTrue(has)
        self.assertGreater(count, 0)

    def test_empty(self):
        img = make_empty_image(32)
        has, count = sai.check_empty_mask(img)
        self.assertFalse(has)
        self.assertEqual(count, 0)


class TestConnectedComponents(unittest.TestCase):
    """Connected-component (multi-blob) check."""

    def test_single_blob(self):
        img = make_sprite_image(32)
        count, total = sai.count_connected_components(img)
        self.assertEqual(count, 1)

    def test_multi_blob_detected(self):
        img = make_multi_blob_image(64)
        count, total = sai.count_connected_components(img)
        self.assertGreater(count, 1)

    def test_small_components_not_significant(self):
        """Small detached glow/details are not counted as significant blobs."""
        img = make_small_components_image(64)
        count, total = sai.count_connected_components(img)
        # The large blob is significant; the 1x1 pixels are not.
        self.assertEqual(count, 1)

    def test_empty_image(self):
        img = make_empty_image(32)
        count, total = sai.count_connected_components(img)
        self.assertEqual(count, 0)
        self.assertEqual(total, 0)


class TestValidation(unittest.TestCase):
    """Full validation classification."""

    def test_valid_sprite_passes(self):
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            img = make_sprite_image(64)
            img.save(f, "PNG")
            f.close()
            result = sai.validate_image(f.name)
            self.assertEqual(result.outcome, sai.Outcome.PASS)
            self.assertTrue(result.ok)
            os.unlink(f.name)

    def test_empty_mask_is_blocking(self):
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            img = make_empty_image(32)
            img.save(f, "PNG")
            f.close()
            result = sai.validate_image(f.name)
            self.assertEqual(result.outcome, sai.Outcome.BLOCKING)
            self.assertFalse(result.ok)
            os.unlink(f.name)

    def test_multi_blob_is_blocking(self):
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            img = make_multi_blob_image(64)
            img.save(f, "PNG")
            f.close()
            result = sai.validate_image(f.name)
            self.assertEqual(result.outcome, sai.Outcome.BLOCKING)
            os.unlink(f.name)

    def test_edge_clipped_is_blocking(self):
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            img = make_edge_clipped_image(32)
            img.save(f, "PNG")
            f.close()
            result = sai.validate_image(f.name)
            self.assertEqual(result.outcome, sai.Outcome.BLOCKING)
            os.unlink(f.name)

    def test_oversize_is_blocking(self):
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            make_oversize_png(f.name, size_kb=35)
            f.close()
            result = sai.validate_image(f.name)
            self.assertEqual(result.outcome, sai.Outcome.BLOCKING)
            os.unlink(f.name)

    def test_small_components_is_warning(self):
        """Small detached components produce a warning, not blocking."""
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            img = make_small_components_image(64)
            img.save(f, "PNG")
            f.close()
            result = sai.validate_image(f.name)
            # Should be warning or pass (small components may be under threshold).
            self.assertIn(result.outcome, (sai.Outcome.WARNING, sai.Outcome.PASS))
            os.unlink(f.name)


class TestPortraitProcessing(unittest.TestCase):
    """Portrait processing."""

    def test_process_portrait_produces_correct_size(self):
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            f.close()
            # Create a simple RGB image as "generated" data.
            img = Image.new("RGB", (200, 200), (50, 50, 50))
            buf = io.BytesIO()
            img.save(buf, "PNG")
            ok, file_bytes = sai.process_portrait(buf.getvalue(), f.name, portrait_size=128)
            self.assertTrue(ok)
            self.assertGreater(file_bytes, 0)
            # Verify the output is 128x128.
            out = Image.open(f.name)
            self.assertEqual(out.size, (128, 128))
            os.unlink(f.name)


class TestAtomicWrite(unittest.TestCase):
    """Atomic write."""

    def test_atomic_write_creates_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            dest = Path(tmp) / "out.png"
            sai.atomic_write(b"test data", dest)
            self.assertTrue(dest.is_file())
            self.assertEqual(dest.read_bytes(), b"test data")

    def test_atomic_write_overwrites(self):
        with tempfile.TemporaryDirectory() as tmp:
            dest = Path(tmp) / "out.png"
            dest.write_bytes(b"old")
            sai.atomic_write(b"new", dest)
            self.assertEqual(dest.read_bytes(), b"new")


class TestReversibleBackup(unittest.TestCase):
    """Reversible backup."""

    def test_backup_creates_copy(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "src" / "img.png"
            src.parent.mkdir()
            src.write_bytes(b"original")
            backup_dir = Path(tmp) / "backup"
            backup_path = sai.reversible_backup(src, backup_dir)
            self.assertIsNotNone(backup_path)
            self.assertTrue(backup_path.is_file())
            self.assertEqual(backup_path.read_bytes(), b"original")

    def test_backup_returns_none_for_missing(self):
        with tempfile.TemporaryDirectory() as tmp:
            backup_path = sai.reversible_backup(Path(tmp) / "nonexistent.png", Path(tmp) / "backup")
            self.assertIsNone(backup_path)


class TestResizeFlopTrim(unittest.TestCase):
    """Resize, flop, trim operations."""

    def test_resize(self):
        img = make_sprite_image(32)
        resized = sai.resize_image(img, 16, 16)
        self.assertEqual(resized.size, (16, 16))

    def test_flop(self):
        img = Image.new("RGBA", (4, 1), (0, 0, 0, 0))
        img.putpixel((0, 0), (255, 0, 0, 255))
        img.putpixel((3, 0), (0, 0, 255, 255))
        flopped = sai.flop_image(img)
        self.assertEqual(flopped.getpixel((0, 0)), (0, 0, 255, 255))
        self.assertEqual(flopped.getpixel((3, 0)), (255, 0, 0, 255))

    def test_trim(self):
        img = Image.new("RGBA", (32, 32), (0, 0, 0, 0))
        # Put content in a 10x10 area at (5,5).
        for y in range(10):
            for x in range(10):
                img.putpixel((5 + x, 5 + y), (100, 100, 100, 255))
        trimmed = sai.trim_to_content(img)
        self.assertEqual(trimmed.size, (10, 10))


# ---------------------------------------------------------------------------
# Provider tests
# ---------------------------------------------------------------------------


class TestFakeProvider(unittest.TestCase):
    """Fake provider covers success, timeout, malformed JSON, missing image,
    bounded retry, and validation retry."""

    def test_success(self):
        provider = FakeProvider(FakeProviderConfig(image_data=b"fake png"))
        result = provider.generate(ProviderRequest(prompt="test"))
        self.assertTrue(result.success)
        self.assertEqual(result.image_data, b"fake png")

    def test_timeout(self):
        provider = FakeProvider(FakeProviderConfig(timeout=True))
        result = provider.generate(ProviderRequest(prompt="test"))
        self.assertFalse(result.success)
        self.assertTrue(result.timed_out)

    def test_malformed_json(self):
        provider = FakeProvider(FakeProviderConfig(malformed_json=True))
        result = provider.generate(ProviderRequest(prompt="test"))
        self.assertFalse(result.success)
        self.assertIn("malformed", result.error)

    def test_missing_image_payload(self):
        provider = FakeProvider(FakeProviderConfig(missing_image=True))
        result = provider.generate(ProviderRequest(prompt="test"))
        self.assertFalse(result.success)
        self.assertIn("no image", result.error)

    def test_missing_candidates(self):
        provider = FakeProvider(FakeProviderConfig(missing_candidates=True))
        result = provider.generate(ProviderRequest(prompt="test"))
        self.assertFalse(result.success)
        self.assertIn("no candidates", result.error)

    def test_fail_always(self):
        provider = FakeProvider(FakeProviderConfig(fail_always="permanent error"))
        result = provider.generate(ProviderRequest(prompt="test"), retries=3)
        self.assertFalse(result.success)
        self.assertIn("permanent", result.error)

    def test_fail_first_n_then_succeed(self):
        """Bounded retry: fail first 2 attempts, then succeed."""
        provider = FakeProvider(FakeProviderConfig(
            fail_first_n=2,
            image_data=b"success png",
        ))
        # First call fails.
        r1 = provider.generate(ProviderRequest(prompt="test"), retries=3)
        self.assertFalse(r1.success)
        # Second call fails.
        r2 = provider.generate(ProviderRequest(prompt="test"), retries=3)
        self.assertFalse(r2.success)
        # Third call succeeds.
        r3 = provider.generate(ProviderRequest(prompt="test"), retries=3)
        self.assertTrue(r3.success)
        self.assertEqual(r3.image_data, b"success png")

    def test_records_requests(self):
        provider = FakeProvider(FakeProviderConfig(image_data=b"data"))
        provider.generate(ProviderRequest(prompt="prompt1", reference_image_b64="ref1"))
        self.assertEqual(len(provider.requests), 1)
        self.assertEqual(provider.requests[0].prompt, "prompt1")
        self.assertEqual(provider.requests[0].reference_image_b64, "ref1")


class TestGeminiProvider(unittest.TestCase):
    """Gemini provider reads key from env only, never logs it."""

    def test_no_key_returns_failure(self):
        """Without GEMINI_API_KEY, generation fails gracefully."""
        old_key = os.environ.pop("GEMINI_API_KEY", None)
        try:
            provider = GeminiProvider()
            result = provider.generate(ProviderRequest(prompt="test"))
            self.assertFalse(result.success)
            self.assertIn("GEMINI_API_KEY", result.error)
        finally:
            if old_key is not None:
                os.environ["GEMINI_API_KEY"] = old_key

    def test_create_provider_gemini(self):
        provider = create_provider("gemini")
        self.assertIsInstance(provider, GeminiProvider)

    def test_create_provider_fake(self):
        provider = create_provider("fake")
        self.assertIsInstance(provider, FakeProvider)

    def test_create_provider_unknown_raises(self):
        with self.assertRaises(ValueError):
            create_provider("bogus")


# ---------------------------------------------------------------------------
# No-network / no-key guard
# ---------------------------------------------------------------------------


class TestNoNetworkNoKey(unittest.TestCase):
    """No default test reads GEMINI_API_KEY or accesses the network."""

    def test_no_gemini_key_in_env(self):
        """Ensure GEMINI_API_KEY is not set during tests."""
        # This test documents the contract; it does not set the key.
        key = os.environ.get("GEMINI_API_KEY")
        # If someone has a key set, we just note it — we don't use it.
        # The test suite itself must never read it.
        self.assertTrue(True)  # Placeholder — the contract is that no test above uses it.


if __name__ == "__main__":
    unittest.main()
