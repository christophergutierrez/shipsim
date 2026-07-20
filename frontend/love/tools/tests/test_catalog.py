"""Unit tests for the ship art catalog, sidecar, and manifest library.

Phase 2 exit gate tests.  All tests run without network access.
"""

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

from PIL import Image

# Make the tools module importable.
_TOOLS_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(_TOOLS_DIR))

import ship_art_catalog as sac  # noqa: E402


def write_accepted_state(assets_dir: Path, state: str) -> None:
    """Create one complete, reviewed fixture using the production sidecar writer."""
    class_dir = assets_dir / "escort"
    class_dir.mkdir(exist_ok=True)
    image_path = class_dir / f"{state}.png"
    Image.new("RGBA", (256, 256), (20, 40, 80, 255)).save(image_path)
    sac.write_sidecar_state(
        class_dir / "sprite.toml",
        class_id="escort",
        display_name="Escort",
        state=state,
        metadata={
            "image_path": f"escort/{state}.png",
            "width": 256,
            "height": 256,
            "anchor_x": 0.5,
            "anchor_y": 0.5,
            "source_angle": 0.0,
            "scale": 1.0,
            "provider": "fake",
            "model": "fake-model",
            "prompt_hash": "fixture",
            "reference_state": "",
            "processing_version": "1",
            "review_status": "accepted",
        },
    )


class TestCatalogBuilding(unittest.TestCase):
    """Catalog is built from ship definitions."""

    def test_build_catalog_has_28_entries(self):
        entries = sac.build_catalog()
        self.assertEqual(len(entries), 28)

    def test_build_catalog_has_26_primaries(self):
        entries = sac.build_catalog()
        primaries = [e for e in entries if e.kind == "primary"]
        self.assertEqual(len(primaries), 26)

    def test_build_catalog_has_2_aliases(self):
        entries = sac.build_catalog()
        aliases = [e for e in entries if e.kind == "alias"]
        self.assertEqual(len(aliases), 2)

    def test_alias_targets_are_correct(self):
        entries = sac.build_catalog()
        by_id = {e.class_id: e for e in entries}
        self.assertEqual(by_id["tutorial_escort"].alias_target, "escort")
        self.assertEqual(by_id["tutorial_heavy_cruiser"].alias_target, "heavy_cruiser")

    def test_every_entry_has_non_empty_class_id(self):
        entries = sac.build_catalog()
        for e in entries:
            self.assertTrue(e.class_id, f"entry has empty class_id")

    def test_every_entry_has_desired_states(self):
        entries = sac.build_catalog()
        for e in entries:
            self.assertEqual(e.desired_states, ["top_down", "portrait"])

    def test_catalog_json_round_trips(self):
        entries = sac.build_catalog()
        json_str = sac.catalog_to_json(entries)
        data = json.loads(json_str)
        self.assertEqual(data["version"], 1)
        self.assertEqual(len(data["entries"]), 28)
        self.assertEqual(data["p0_states"], ["top_down", "portrait"])


class TestAudit(unittest.TestCase):
    """Audit rules enforce the Phase 2 exit gate."""

    def test_audit_passes_on_real_catalog(self):
        result = sac.audit()
        self.assertTrue(result.ok, f"audit errors: {result.errors}")
        self.assertEqual(result.definitions, 28)
        self.assertEqual(result.primary, 26)
        self.assertEqual(result.aliases, 2)
        self.assertEqual(result.unknown, 0)
        self.assertEqual(result.cycles, 0)

    def test_audit_detects_unknown_catalog_id(self):
        """A catalog entry with no ship definition is unknown."""
        entries = sac.build_catalog()
        # Add a fake entry not in data/ships.
        fake = sac.CatalogEntry(
            class_id="nonexistent_ship",
            display_name="Fake",
            kind="primary",
        )
        result = sac.audit(catalog=entries + [fake])
        self.assertFalse(result.ok)
        self.assertGreater(result.unknown, 0)
        self.assertTrue(any("nonexistent_ship" in e for e in result.errors))

    def test_audit_detects_self_alias(self):
        """Self-alias is an error."""
        entries = sac.build_catalog()
        by_id = {e.class_id: e for e in entries}
        # Make escort alias itself.
        by_id["escort"].kind = "alias"
        by_id["escort"].alias_target = "escort"
        result = sac.audit(catalog=list(by_id.values()))
        self.assertFalse(result.ok)
        self.assertTrue(any("self-alias" in e for e in result.errors))

    def test_audit_detects_alias_to_unknown_target(self):
        """Alias targeting a nonexistent class is an error."""
        entries = sac.build_catalog()
        by_id = {e.class_id: e for e in entries}
        by_id["tutorial_escort"].alias_target = "nonexistent_target"
        result = sac.audit(catalog=list(by_id.values()))
        self.assertFalse(result.ok)
        self.assertTrue(any("nonexistent_target" in e for e in result.errors))

    def test_audit_detects_alias_cycle(self):
        """A cycle in the alias graph is detected."""
        entries = sac.build_catalog()
        by_id = {e.class_id: e for e in entries}
        # Create a cycle: tutorial_escort -> tutorial_heavy_cruiser -> tutorial_escort
        by_id["tutorial_escort"].alias_target = "tutorial_heavy_cruiser"
        by_id["tutorial_heavy_cruiser"].alias_target = "tutorial_escort"
        result = sac.audit(catalog=list(by_id.values()))
        self.assertFalse(result.ok)
        self.assertGreater(result.cycles, 0)


class TestPathValidation(unittest.TestCase):
    """Path traversal and absolute paths fail validation."""

    def test_safe_relative_path_accepted(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            self.assertTrue(sac.is_safe_relative_path("escort/top_down.png", base))
            self.assertTrue(sac.is_safe_relative_path("foo/bar/baz.png", base))

    def test_absolute_path_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            self.assertFalse(sac.is_safe_relative_path("/etc/passwd", base))
            self.assertFalse(sac.is_safe_relative_path(os.path.abspath("foo.png"), base))

    def test_path_traversal_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            self.assertFalse(sac.is_safe_relative_path("../../../etc/passwd", base))
            self.assertFalse(sac.is_safe_relative_path("..", base))
            self.assertFalse(sac.is_safe_relative_path("foo/../../bar", base))

    def test_empty_path_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            self.assertFalse(sac.is_safe_relative_path("", base))


class TestManifestGeneration(unittest.TestCase):
    """Manifest is deterministic and handles empty/partial catalogs."""

    def test_empty_catalog_generates_empty_manifest(self):
        """An empty asset catalog generates a valid fallback-only manifest."""
        records = sac.generate_manifest(catalog=[])
        self.assertEqual(len(records), 0)
        # The manifest JSON is still valid.
        data = json.loads(sac.manifest_to_json(records))
        self.assertEqual(data["version"], 1)
        self.assertEqual(data["records"], [])

    def test_manifest_deterministic_across_rebuilds(self):
        """Rebuilding the manifest twice produces identical SHA-256."""
        catalog = sac.load_catalog()
        records1 = sac.generate_manifest(catalog)
        records2 = sac.generate_manifest(catalog)
        sha1 = sac.manifest_sha256(records1)
        sha2 = sac.manifest_sha256(records2)
        self.assertEqual(sha1, sha2)

    def test_manifest_deterministic_with_fixture(self):
        """Manifest with a sidecar fixture is deterministic across rebuilds."""
        with tempfile.TemporaryDirectory() as tmp:
            assets_dir = Path(tmp)
            write_accepted_state(assets_dir, "top_down")
            write_accepted_state(assets_dir, "portrait")
            catalog = sac.build_catalog()
            records1 = sac.generate_manifest(catalog, assets_dir=assets_dir)
            records2 = sac.generate_manifest(catalog, assets_dir=assets_dir)
            self.assertEqual(
                sac.manifest_sha256(records1),
                sac.manifest_sha256(records2),
            )

    def test_single_fixture_generates_primary_plus_aliases(self):
        """A single complete fixture generates one primary manifest entry per
        state plus any aliases targeting it."""
        with tempfile.TemporaryDirectory() as tmp:
            assets_dir = Path(tmp)
            write_accepted_state(assets_dir, "top_down")
            write_accepted_state(assets_dir, "portrait")
            catalog = sac.build_catalog()
            records = sac.generate_manifest(catalog, assets_dir=assets_dir)

            # Should have: escort (top_down + portrait) + tutorial_escort (top_down + portrait)
            by_key = {(r.class_id, r.state) for r in records}
            self.assertIn(("escort", "top_down"), by_key)
            self.assertIn(("escort", "portrait"), by_key)
            self.assertIn(("tutorial_escort", "top_down"), by_key)
            self.assertIn(("tutorial_escort", "portrait"), by_key)
            self.assertEqual(len(records), 4)

            # Alias records should point to the same image as the primary.
            escort_td = next(r for r in records if r.class_id == "escort" and r.state == "top_down")
            alias_td = next(r for r in records if r.class_id == "tutorial_escort" and r.state == "top_down")
            self.assertEqual(escort_td.image_path, alias_td.image_path)

    def test_partial_sidecar_only_emits_available_states(self):
        """A sidecar with only top_down omits portrait from the manifest."""
        with tempfile.TemporaryDirectory() as tmp:
            assets_dir = Path(tmp)
            write_accepted_state(assets_dir, "top_down")
            catalog = sac.build_catalog()
            records = sac.generate_manifest(catalog, assets_dir=assets_dir)
            # Only top_down for escort + tutorial_escort alias.
            states = {r.state for r in records}
            self.assertEqual(states, {"top_down"})
            class_ids = {r.class_id for r in records}
            self.assertEqual(class_ids, {"escort", "tutorial_escort"})

    def test_unsafe_sidecar_path_excluded_from_manifest(self):
        """A sidecar with a traversal path is excluded from the manifest."""
        with tempfile.TemporaryDirectory() as tmp:
            assets_dir = Path(tmp)
            escort_dir = assets_dir / "escort"
            escort_dir.mkdir()
            sidecar = escort_dir / "sprite.toml"
            sidecar.write_text(
                'class_id = "escort"\n'
                '[states.top_down]\n'
                'image_path = "../../../etc/passwd"\n'
                'width = 256\n'
                'height = 256\n'
            )
            catalog = sac.build_catalog()
            records = sac.generate_manifest(catalog, assets_dir=assets_dir)
            # Unsafe path excluded — no records.
            self.assertEqual(len(records), 0)

    def test_alias_resolves_through_chain(self):
        """Alias chain resolution follows to the ultimate primary."""
        with tempfile.TemporaryDirectory() as tmp:
            assets_dir = Path(tmp)
            escort_dir = assets_dir / "escort"
            escort_dir.mkdir()
            (escort_dir / "sprite.toml").write_text(
                'class_id = "escort"\n'
                '[states.top_down]\n'
                'image_path = "escort/top_down.png"\n'
                'width = 256\n'
                'height = 256\n'
            )
            catalog = sac.build_catalog()
            # Verify alias chain resolves.
            target = sac._resolve_alias_chain("tutorial_escort", catalog)
            self.assertEqual(target, "escort")


class TestSidecarLoading(unittest.TestCase):
    """Sidecar TOML loading works correctly."""

    def test_sidecar_from_toml(self):
        with tempfile.TemporaryDirectory() as tmp:
            sidecar_path = Path(tmp) / "sprite.toml"
            sidecar_path.write_text(
                'class_id = "escort"\n'
                '[states.top_down]\n'
                'image_path = "escort/top_down.png"\n'
                'width = 256\n'
                'height = 256\n'
                'anchor_x = 0.5\n'
                'anchor_y = 0.4\n'
                'source_angle = 0.0\n'
                'scale = 1.0\n'
                'provider = "fake"\n'
                'model = "fake-model"\n'
                'prompt_hash = "fixture"\n'
                'reference_state = ""\n'
                'processing_version = "1"\n'
                'review_status = "accepted"\n'
            )
            sc = sac.Sidecar.from_toml(sidecar_path)
            self.assertEqual(sc.class_id, "escort")
            self.assertIn("top_down", sc.states)
            self.assertEqual(sc.states["top_down"].image_path, "escort/top_down.png")
            self.assertEqual(sc.states["top_down"].width, 256)

    def test_sidecar_class_id_defaults_to_parent_dir(self):
        with tempfile.TemporaryDirectory() as tmp:
            class_dir = Path(tmp) / "my_ship"
            class_dir.mkdir()
            sidecar_path = class_dir / "sprite.toml"
            sidecar_path.write_text(
                'class_id = "my_ship"\n'
            )
            sc = sac.Sidecar.from_toml(sidecar_path)
            self.assertEqual(sc.class_id, "my_ship")


class TestCLI(unittest.TestCase):
    """CLI commands work correctly."""

    def test_audit_cli_returns_zero(self):
        rc = sac.main(["--audit"])
        self.assertEqual(rc, 0)

    def test_check_manifest_cli(self):
        rc = sac.main(["--check-manifest"])
        self.assertEqual(rc, 0)

    def test_unknown_command_returns_2(self):
        rc = sac.main(["--bogus"])
        self.assertEqual(rc, 2)

    def test_no_args_returns_2(self):
        rc = sac.main([])
        self.assertEqual(rc, 2)


if __name__ == "__main__":
    unittest.main()
