"""Regression tests for the ship-art producer/reviewer integration."""

import io
import json
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from unittest import mock

from PIL import Image

_TOOLS_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(_TOOLS_DIR))

import generate_ship_art as generator  # noqa: E402
import review_ship_art as reviewer  # noqa: E402
import ship_art_catalog as sac  # noqa: E402
from ship_art_provider import FakeProvider, FakeProviderConfig  # noqa: E402


def generated_png(color: tuple[int, int, int, int] = (40, 100, 180, 255)) -> bytes:
    image = Image.new("RGBA", (64, 64), (255, 0, 255, 255))
    for y in range(16, 48):
        for x in range(16, 48):
            image.putpixel((x, y), color)
    out = io.BytesIO()
    image.save(out, "PNG")
    return out.getvalue()


def entry() -> sac.CatalogEntry:
    return sac.CatalogEntry("escort", "Escort", "primary", visual_description="escort")


def state_metadata(status: str, state: str = "top_down") -> dict:
    return {
        "image_path": f"escort/{state}.png",
        "width": 64,
        "height": 64,
        "anchor_x": 0.5,
        "anchor_y": 0.5,
        "source_angle": 0.0,
        "scale": 1.0,
        "provider": "fake",
        "model": "fake-model",
        "prompt_hash": "fixture",
        "reference_state": "",
        "processing_version": "1",
        "review_status": status,
    }


class TestManifestContract(unittest.TestCase):
    def test_love_fixture_is_exact_python_producer_output(self):
        record = sac.ManifestRecord(
            class_id="escort",
            state="top_down",
            image_path="escort/top_down.png",
            width=256,
            height=256,
            anchor_x=0.5,
            anchor_y=0.5,
            source_angle=0.0,
            scale=1.0,
        )
        fixture = _TOOLS_DIR.parent / "tests" / "fixtures" / "ship_art_manifest.json"
        self.assertEqual(fixture.read_text(), sac.manifest_to_json([record]))

    def test_manifest_paths_are_asset_root_relative(self):
        with tempfile.TemporaryDirectory() as tmp:
            assets = Path(tmp)
            class_dir = assets / "escort"
            class_dir.mkdir()
            (class_dir / "top_down.png").write_bytes(generated_png())
            sac.write_sidecar_state(
                class_dir / "sprite.toml",
                class_id="escort",
                display_name="Escort",
                state="top_down",
                metadata=state_metadata("accepted"),
            )
            records = sac.generate_manifest([entry()], assets_dir=assets)
            payload = json.loads(sac.manifest_to_json(records))
            self.assertEqual(
                payload["records"][0]["image_path"],
                "escort/top_down.png",
            )

    def test_sidecar_update_preserves_existing_states(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "escort" / "sprite.toml"
            sac.write_sidecar_state(
                path, class_id="escort", display_name="Escort", state="top_down",
                metadata=state_metadata("accepted", "top_down"),
            )
            sac.write_sidecar_state(
                path, class_id="escort", display_name="Escort", state="portrait",
                metadata=state_metadata("accepted", "portrait"),
            )
            sidecar = sac.Sidecar.from_toml(path)
            self.assertEqual(set(sidecar.states), {"top_down", "portrait"})


class TestStateAssetSchema(unittest.TestCase):
    def test_incomplete_state_cannot_be_persisted(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "escort" / "sprite.toml"
            with self.assertRaisesRegex(ValueError, "missing metadata 'anchor_x'"):
                sac.write_sidecar_state(
                    path,
                    class_id="escort",
                    display_name="Escort",
                    state="top_down",
                    metadata={
                        "image_path": "escort/top_down.png",
                        "width": 64,
                        "height": 64,
                    },
                )
            self.assertFalse(path.exists())

    def test_unsafe_state_path_cannot_be_persisted(self):
        for unsafe in ("../top_down.png", "/tmp/top_down.png", "escort\\top_down.png"):
            with self.subTest(unsafe=unsafe), tempfile.TemporaryDirectory() as tmp:
                path = Path(tmp) / "escort" / "sprite.toml"
                metadata = state_metadata("unreviewed")
                metadata["image_path"] = unsafe
                with self.assertRaisesRegex(ValueError, "invalid image_path"):
                    sac.write_sidecar_state(
                        path,
                        class_id="escort",
                        display_name="Escort",
                        state="top_down",
                        metadata=metadata,
                    )
                self.assertFalse(path.exists())

    def test_invalid_review_transition_preserves_sidecar(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "escort" / "sprite.toml"
            sac.write_sidecar_state(
                path,
                class_id="escort",
                display_name="Escort",
                state="top_down",
                metadata=state_metadata("unreviewed"),
            )
            original = path.read_bytes()
            with self.assertRaisesRegex(ValueError, "unknown review status"):
                sac.set_review_status(
                    path,
                    class_id="escort",
                    state="top_down",
                    review_status="approved",
                )
            self.assertEqual(path.read_bytes(), original)


class TestAuditRegressions(unittest.TestCase):
    def test_definition_internal_id_must_match_file_stem(self):
        with tempfile.TemporaryDirectory() as ships_tmp, tempfile.TemporaryDirectory() as assets_tmp:
            ships = Path(ships_tmp)
            (ships / "escort.toml").write_text('id = "wrong"\nname = "Escort"\nsize = 1\n')
            result = sac.audit(catalog=[entry()], ships_dir=ships, assets_dir=Path(assets_tmp))
            self.assertFalse(result.ok)
            self.assertTrue(any("does not match file stem" in error for error in result.errors))

    def test_empty_definition_internal_id_matches_rust_file_stem_fallback(self):
        with tempfile.TemporaryDirectory() as ships_tmp, tempfile.TemporaryDirectory() as assets_tmp:
            ships = Path(ships_tmp)
            (ships / "escort.toml").write_text('id = ""\nname = "Escort"\nsize = 1\n')
            result = sac.audit(catalog=[entry()], ships_dir=ships, assets_dir=Path(assets_tmp))
            self.assertTrue(result.ok, result.errors)

    def test_sidecar_state_requires_complete_valid_metadata_and_asset(self):
        with tempfile.TemporaryDirectory() as ships_tmp, tempfile.TemporaryDirectory() as assets_tmp:
            ships = Path(ships_tmp)
            assets = Path(assets_tmp)
            (ships / "escort.toml").write_text('id = "escort"\nname = "Escort"\nsize = 1\n')
            class_dir = assets / "escort"
            class_dir.mkdir()
            (class_dir / "sprite.toml").write_text(
                'class_id = "escort"\n[states.top_down]\nimage_path = "escort/missing.png"\n'
            )
            result = sac.audit(catalog=[entry()], ships_dir=ships, assets_dir=assets)
            self.assertFalse(result.ok)
            self.assertTrue(any("missing metadata 'width'" in error for error in result.errors))
            (class_dir / "sprite.toml").unlink()
            sac.write_sidecar_state(
                class_dir / "sprite.toml",
                class_id="escort",
                display_name="Escort",
                state="top_down",
                metadata=state_metadata("accepted"),
            )
            result = sac.audit(catalog=[entry()], ships_dir=ships, assets_dir=assets)
            self.assertFalse(result.ok)
            self.assertTrue(any("asset does not exist" in error for error in result.errors))

    def test_sidecar_class_id_must_match_directory(self):
        with tempfile.TemporaryDirectory() as ships_tmp, tempfile.TemporaryDirectory() as assets_tmp:
            ships = Path(ships_tmp)
            assets = Path(assets_tmp)
            (ships / "escort.toml").write_text('id = "escort"\nname = "Escort"\nsize = 1\n')
            class_dir = assets / "escort"
            class_dir.mkdir()
            (class_dir / "sprite.toml").write_text('class_id = "wrong"\n')
            result = sac.audit(catalog=[entry()], ships_dir=ships, assets_dir=assets)
            self.assertFalse(result.ok)
            self.assertTrue(any("declares class_id 'wrong'" in error for error in result.errors))

    def test_malformed_sidecar_is_an_audit_error_not_an_exception(self):
        with tempfile.TemporaryDirectory() as ships_tmp, tempfile.TemporaryDirectory() as assets_tmp:
            ships = Path(ships_tmp)
            assets = Path(assets_tmp)
            (ships / "escort.toml").write_text('id = "escort"\nname = "Escort"\nsize = 1\n')
            class_dir = assets / "escort"
            class_dir.mkdir()
            (class_dir / "sprite.toml").write_text("not = [valid")
            result = sac.audit(catalog=[entry()], ships_dir=ships, assets_dir=assets)
            self.assertFalse(result.ok)
            self.assertTrue(any("invalid sidecar 'escort'" in error for error in result.errors))


class TestGenerationRegressions(unittest.TestCase):
    def test_redo_selects_one_state_instead_of_treating_it_as_a_path(self):
        self.assertEqual(generator.requested_states(None, "portrait"), ["portrait"])
        with self.assertRaises(ValueError):
            generator.requested_states("top_down", "portrait")

    def test_success_writes_sidecar_visible_to_manifest_and_completeness(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            assets = root / "assets" / "ship_art"
            result = generator.generate_one(
                FakeProvider(FakeProviderConfig(image_data=generated_png())),
                entry(),
                "top_down",
                assets_dir=assets,
                scratch_dir=root / "scratch",
                backup_dir=root / "backups",
                model="fake-model",
            )
            self.assertTrue(result.success, result.error)
            sidecar = sac.load_sidecars(assets)["escort"]
            self.assertIn("top_down", sidecar.states)
            self.assertEqual(sidecar.states["top_down"].provider, "fake")
            self.assertEqual(sidecar.states["top_down"].model, "fake-model")
            self.assertEqual(sidecar.states["top_down"].review_status, "unreviewed")
            self.assertEqual(len(sac.generate_manifest([entry()], assets_dir=assets)), 0)
            self.assertEqual(
                reviewer.check_completeness([entry()], assets)[0]["states_present"],
                [],
            )
            sac.set_review_status(
                assets / "escort" / "sprite.toml",
                class_id="escort",
                state="top_down",
                review_status="accepted",
            )
            self.assertEqual(len(sac.generate_manifest([entry()], assets_dir=assets)), 1)
            self.assertEqual(
                reviewer.check_completeness([entry()], assets)[0]["states_present"],
                ["top_down"],
            )

    def test_regeneration_unpublishes_replacement_until_review(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            assets = root / "assets"
            class_dir = assets / "escort"
            class_dir.mkdir(parents=True)
            class_dir.joinpath("top_down.png").write_bytes(generated_png())
            sac.write_sidecar_state(
                class_dir / "sprite.toml",
                class_id="escort",
                display_name="Escort",
                state="top_down",
                metadata=state_metadata("accepted"),
            )
            sac.write_manifest(
                sac.generate_manifest([entry()], assets_dir=assets),
                assets / "manifest.json",
            )
            self.assertEqual(len(sac.load_manifest(assets / "manifest.json")), 1)

            result = generator.generate_one(
                FakeProvider(FakeProviderConfig(image_data=generated_png((180, 80, 40, 255)))),
                entry(),
                "top_down",
                assets_dir=assets,
                scratch_dir=root / "scratch",
                backup_dir=root / "backups",
                model="fake-model",
                catalog=[entry()],
            )

            self.assertTrue(result.success, result.error)
            self.assertEqual(
                sac.Sidecar.from_toml(class_dir / "sprite.toml")
                .states["top_down"].review_status,
                "unreviewed",
            )
            self.assertEqual(sac.load_manifest(assets / "manifest.json"), [])

    def test_manifest_failure_rolls_back_image_sidecar_and_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            assets = root / "assets"
            class_dir = assets / "escort"
            class_dir.mkdir(parents=True)
            image_path = class_dir / "top_down.png"
            sidecar_path = class_dir / "sprite.toml"
            image_path.write_bytes(generated_png())
            sac.write_sidecar_state(
                sidecar_path,
                class_id="escort",
                display_name="Escort",
                state="top_down",
                metadata=state_metadata("accepted"),
            )
            sac.write_manifest(
                sac.generate_manifest([entry()], assets_dir=assets),
                assets / "manifest.json",
            )
            original_image = image_path.read_bytes()
            original_sidecar = sidecar_path.read_bytes()
            original_manifest = (assets / "manifest.json").read_bytes()

            with mock.patch.object(sac, "write_manifest", side_effect=OSError("disk")):
                result = generator.generate_one(
                    FakeProvider(FakeProviderConfig(image_data=generated_png((180, 80, 40, 255)))),
                    entry(),
                    "top_down",
                    assets_dir=assets,
                    scratch_dir=root / "scratch",
                    backup_dir=root / "backups",
                    model="fake-model",
                    catalog=[entry()],
                )

            self.assertFalse(result.success)
            self.assertIn("metadata update failed", result.error)
            self.assertEqual(image_path.read_bytes(), original_image)
            self.assertEqual(sidecar_path.read_bytes(), original_sidecar)
            self.assertEqual((assets / "manifest.json").read_bytes(), original_manifest)

    def test_manifest_is_unpublished_before_stable_image_replacement(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            assets = root / "assets"
            class_dir = assets / "escort"
            class_dir.mkdir(parents=True)
            image_path = class_dir / "top_down.png"
            image_path.write_bytes(generated_png())
            sac.write_sidecar_state(
                class_dir / "sprite.toml",
                class_id="escort",
                display_name="Escort",
                state="top_down",
                metadata=state_metadata("accepted"),
            )
            sac.write_manifest(
                sac.generate_manifest([entry()], assets_dir=assets),
                assets / "manifest.json",
            )
            original_image = image_path.read_bytes()
            real_replace = generator.os.replace

            def replace_then_stop(source, destination):
                real_replace(source, destination)
                if Path(destination) == image_path:
                    raise SystemExit("simulated process termination")

            with mock.patch.object(generator.os, "replace", side_effect=replace_then_stop):
                with self.assertRaises(SystemExit):
                    generator.generate_one(
                        FakeProvider(
                            FakeProviderConfig(
                                image_data=generated_png((180, 80, 40, 255))
                            )
                        ),
                        entry(),
                        "top_down",
                        assets_dir=assets,
                        scratch_dir=root / "scratch",
                        backup_dir=root / "backups",
                        model="fake-model",
                        catalog=[entry()],
                    )

            self.assertNotEqual(image_path.read_bytes(), original_image)
            self.assertEqual(
                sac.Sidecar.from_toml(class_dir / "sprite.toml")
                .states["top_down"].review_status,
                "unreviewed",
            )
            self.assertEqual(sac.load_manifest(assets / "manifest.json"), [])

    def test_missing_plan_contains_only_missing_pairs(self):
        with tempfile.TemporaryDirectory() as tmp:
            assets = Path(tmp)
            sac.write_sidecar_state(
                assets / "escort" / "sprite.toml",
                class_id="escort",
                display_name="Escort",
                state="top_down",
                metadata=state_metadata("accepted"),
            )
            (assets / "escort" / "top_down.png").write_bytes(generated_png())
            plan = generator.plan_batch([entry()], missing_only=True, assets_dir=assets)
            self.assertEqual(plan.jobs, [("escort", "portrait")])
            self.assertEqual(plan.min_calls, 1)
            self.assertFalse(plan.would_overwrite)

    def test_missing_plan_regenerates_sidecar_state_whose_asset_is_absent(self):
        with tempfile.TemporaryDirectory() as tmp:
            assets = Path(tmp)
            sac.write_sidecar_state(
                assets / "escort" / "sprite.toml",
                class_id="escort",
                display_name="Escort",
                state="top_down",
                metadata=state_metadata("accepted"),
            )
            plan = generator.plan_batch(
                [entry()], states=["top_down"], missing_only=True, assets_dir=assets
            )
            self.assertEqual(plan.jobs, [("escort", "top_down")])

    def test_missing_plan_skips_pending_review_but_retries_rejected_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            assets = Path(tmp)
            sidecar_path = assets / "escort" / "sprite.toml"
            sac.write_sidecar_state(
                sidecar_path,
                class_id="escort",
                display_name="Escort",
                state="top_down",
                metadata=state_metadata("unreviewed"),
            )
            (assets / "escort" / "top_down.png").write_bytes(generated_png())
            pending = generator.plan_batch(
                [entry()], states=["top_down"], missing_only=True, assets_dir=assets
            )
            self.assertEqual(pending.jobs, [])
            sac.set_review_status(
                sidecar_path, class_id="escort", state="top_down", review_status="rejected"
            )
            rejected = generator.plan_batch(
                [entry()], states=["top_down"], missing_only=True, assets_dir=assets
            )
            self.assertEqual(rejected.jobs, [("escort", "top_down")])

    def test_non_top_down_uses_accepted_top_down_reference(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            assets = root / "assets"
            class_dir = assets / "escort"
            class_dir.mkdir(parents=True)
            (class_dir / "top_down.png").write_bytes(generated_png())
            sac.write_sidecar_state(
                class_dir / "sprite.toml",
                class_id="escort",
                display_name="Escort",
                state="top_down",
                metadata=state_metadata("accepted"),
            )
            provider = FakeProvider(FakeProviderConfig(image_data=generated_png()))
            result = generator.generate_one(
                provider, entry(), "portrait", assets_dir=assets,
                scratch_dir=root / "scratch", backup_dir=root / "backups",
            )
            self.assertTrue(result.success, result.error)
            self.assertIsNotNone(provider.requests[0].reference_image_b64)

    def test_non_top_down_does_not_use_unreviewed_top_down_reference(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            assets = root / "assets"
            class_dir = assets / "escort"
            class_dir.mkdir(parents=True)
            (class_dir / "top_down.png").write_bytes(generated_png())
            sac.write_sidecar_state(
                class_dir / "sprite.toml",
                class_id="escort",
                display_name="Escort",
                state="top_down",
                metadata=state_metadata("unreviewed"),
            )
            provider = FakeProvider(FakeProviderConfig(image_data=generated_png()))
            result = generator.generate_one(
                provider, entry(), "portrait", assets_dir=assets,
                scratch_dir=root / "scratch", backup_dir=root / "backups",
            )
            self.assertTrue(result.success, result.error)
            self.assertIsNone(provider.requests[0].reference_image_b64)

    def test_cancellation_discards_provider_result(self):
        with tempfile.TemporaryDirectory() as tmp:
            cancelled = threading.Event()
            cancelled.set()
            result = generator.generate_one(
                FakeProvider(FakeProviderConfig(image_data=generated_png())),
                entry(), "top_down", assets_dir=Path(tmp) / "assets",
                scratch_dir=Path(tmp) / "scratch", backup_dir=Path(tmp) / "backup",
                cancel_event=cancelled,
            )
            self.assertFalse(result.success)
            self.assertEqual(result.outcome, "cancelled")


class TestReviewerRepairs(unittest.TestCase):
    def test_flop_and_undo_are_reversible(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            image_path = root / "sprite.png"
            image = Image.new("RGBA", (4, 1), (0, 0, 0, 0))
            image.putpixel((0, 0), (255, 0, 0, 255))
            image.putpixel((3, 0), (0, 0, 255, 255))
            image.save(image_path)
            original = image_path.read_bytes()
            history = reviewer.RepairHistory(root / "backups")
            history.apply(image_path, "flop")
            with Image.open(image_path) as repaired:
                self.assertEqual(repaired.getpixel((0, 0)), (0, 0, 255, 255))
            self.assertTrue(history.undo(image_path))
            self.assertEqual(image_path.read_bytes(), original)

    def test_repair_rolls_back_image_when_sidecar_update_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            image_path = root / "top_down.png"
            Image.new("RGBA", (4, 1), (255, 0, 0, 255)).save(image_path)
            sac.write_sidecar_state(
                root / "sprite.toml",
                class_id="escort",
                display_name="Escort",
                state="top_down",
                metadata=state_metadata("accepted"),
            )
            original_image = image_path.read_bytes()
            original_sidecar = (root / "sprite.toml").read_bytes()
            history = reviewer.RepairHistory(root / "backups")
            with mock.patch.object(sac, "write_sidecar_state", side_effect=OSError("disk")):
                with self.assertRaises(OSError):
                    history.apply(image_path, "flop")
            self.assertEqual(image_path.read_bytes(), original_image)
            self.assertEqual((root / "sprite.toml").read_bytes(), original_sidecar)
            self.assertFalse(history.undo(image_path))


if __name__ == "__main__":
    unittest.main()
