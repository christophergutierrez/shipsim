#!/usr/bin/env python3

"""review_ship_art.py — tkinter reviewer for ship art.

Features:
  - Searchable completeness overview.
  - State previews with zoom.
  - Structured prompt editing (survives restart, does not modify source).
  - Base-to-target regeneration.
  - Validation display.
  - Repair actions (resize, trim, flop).
  - Undo.
  - Worker-thread generation (UI never freezes).
  - All tkinter mutations marshaled back to the UI thread.

API keys are read only from the environment and never written to logs or
provenance.

Phase 3 of ``docs/SHIP-ART-IMPLEMENTATION-PLAN.md``.
"""

from __future__ import annotations

import json
import io
import queue
import shutil
import sys
import threading
from contextlib import nullcontext
from pathlib import Path
from typing import Any

# Make sibling modules importable.
_TOOLS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_TOOLS_DIR))

import ship_art_catalog as sac
import ship_art_image as sai
from ship_art_provider import (
    ProviderAdapter,
    create_provider,
)

_LOVE_DIR = _TOOLS_DIR.parent
_ASSETS_DIR = _LOVE_DIR / "assets" / "ship_art"
_PROMPT_OVERRIDES = _ASSETS_DIR / "prompt_overrides.json"


class RepairHistory:
    """Apply local image repairs with a per-target, recoverable undo stack."""

    def __init__(
        self,
        backup_dir: str | Path,
        catalog: list[sac.CatalogEntry] | None = None,
        assets_dir: str | Path | None = None,
    ):
        self.backup_dir = Path(backup_dir)
        self.catalog = catalog
        self.assets_dir = Path(assets_dir) if assets_dir is not None else None
        self._history: dict[Path, list[tuple[Path, bytes | None]]] = {}
        self._sequence = 0

    def _publication(self, paths: list[Path]):
        if self.catalog is None or self.assets_dir is None:
            return nullcontext()
        return sac.asset_publication(self.catalog, self.assets_dir, paths)

    def _unpublish_changed_state(self) -> None:
        if self.catalog is None or self.assets_dir is None:
            return
        sac.write_manifest(
            sac.generate_manifest(self.catalog, assets_dir=self.assets_dir),
            path=self.assets_dir / "manifest.json",
        )

    def _backup(self, image_path: Path) -> tuple[Path, bytes | None]:
        self.backup_dir.mkdir(parents=True, exist_ok=True)
        self._sequence += 1
        backup = self.backup_dir / f"{image_path.parent.name}__{image_path.stem}__{self._sequence}.png"
        shutil.copy2(image_path, backup)
        sidecar_path = image_path.parent / "sprite.toml"
        sidecar_bytes = sidecar_path.read_bytes() if sidecar_path.is_file() else None
        return backup, sidecar_bytes

    @staticmethod
    def _restore_sidecar(image_path: Path, sidecar_bytes: bytes | None) -> None:
        sidecar_path = image_path.parent / "sprite.toml"
        if sidecar_bytes is None:
            sidecar_path.unlink(missing_ok=True)
        else:
            sai.atomic_write(sidecar_bytes, sidecar_path)

    @staticmethod
    def _sync_sidecar_dimensions(image_path: Path, width: int, height: int) -> None:
        sidecar_path = image_path.parent / "sprite.toml"
        if not sidecar_path.is_file():
            return
        sidecar = sac.Sidecar.from_toml(sidecar_path)
        state = image_path.stem
        asset = sidecar.states.get(state)
        if asset is None:
            return
        sac.write_sidecar_state(
            sidecar_path,
            class_id=sidecar.class_id,
            display_name=sidecar.display_name,
            state=state,
            metadata=asset.after_pixel_change(width, height),
        )

    def apply(
        self,
        image_path: str | Path,
        operation: str,
        resize_to: tuple[int, int] = (256, 256),
    ) -> None:
        """Apply ``flop``, ``trim``, or ``resize`` via atomic replacement."""
        path = Path(image_path)
        if not path.is_file():
            raise FileNotFoundError(path)
        with ImageContext(path) as image:
            if operation == "flop":
                repaired = sai.flop_image(image)
            elif operation == "trim":
                repaired = sai.trim_to_content(image)
            elif operation == "resize":
                repaired = sai.resize_image(image, *resize_to)
            else:
                raise ValueError(f"unknown repair operation: {operation}")
            output = io.BytesIO()
            repaired.save(output, "PNG")
            size = repaired.size
        backup = self._backup(path)
        try:
            with self._publication([path, path.parent / "sprite.toml"]):
                self._sync_sidecar_dimensions(path, *size)
                # Invalidate runtime visibility before replacing bytes at a
                # path that may have been accepted in the prior manifest.
                self._unpublish_changed_state()
                sai.atomic_write(output.getvalue(), path)
        except Exception:
            sai.atomic_write(backup[0].read_bytes(), path)
            self._restore_sidecar(path, backup[1])
            raise
        self._history.setdefault(path.resolve(), []).append(backup)

    def undo(self, image_path: str | Path) -> bool:
        """Restore the most recent backup for one image."""
        path = Path(image_path)
        stack = self._history.get(path.resolve(), [])
        if not stack:
            return False
        backup_path, backup_sidecar = stack[-1]
        current_image = path.read_bytes()
        current_sidecar_path = path.parent / "sprite.toml"
        current_sidecar = (
            current_sidecar_path.read_bytes() if current_sidecar_path.is_file() else None
        )
        try:
            with self._publication([path, current_sidecar_path]):
                sai.atomic_write(backup_path.read_bytes(), path)
                self._restore_sidecar(path, backup_sidecar)
        except Exception:
            sai.atomic_write(current_image, path)
            self._restore_sidecar(path, current_sidecar)
            raise
        stack.pop()
        return True


class ImageContext:
    """Open a Pillow image and expose a detached RGBA copy."""

    def __init__(self, path: Path):
        self.path = path
        self.image = None

    def __enter__(self):
        from PIL import Image

        with Image.open(self.path) as source:
            self.image = source.convert("RGBA")
        return self.image

    def __exit__(self, exc_type, exc, traceback):
        if self.image is not None:
            self.image.close()


# ---------------------------------------------------------------------------
# Prompt overrides (survive restart, do not modify source)
# ---------------------------------------------------------------------------


def load_prompt_overrides() -> dict[str, str]:
    """Load prompt overrides from JSON. Does not modify Python source."""
    if not _PROMPT_OVERRIDES.is_file():
        return {}
    try:
        return json.loads(_PROMPT_OVERRIDES.read_text())
    except (json.JSONDecodeError, OSError):
        return {}


def save_prompt_overrides(overrides: dict[str, str]) -> None:
    """Save prompt overrides to JSON."""
    data = (json.dumps(overrides, indent=2, sort_keys=True) + "\n").encode()
    sai.atomic_write(data, _PROMPT_OVERRIDES)


# ---------------------------------------------------------------------------
# Completeness check
# ---------------------------------------------------------------------------


def check_completeness(catalog: list[sac.CatalogEntry], assets_dir: Path = _ASSETS_DIR) -> list[dict[str, Any]]:
    """Check completeness of each catalog entry.

    Returns a list of dicts with class_id, kind, display_name, states, and
    complete flag.
    """
    sidecars = sac.load_sidecars(assets_dir)

    def present_states(sidecar: sac.Sidecar | None) -> list[str]:
        if sidecar is None:
            return []
        present = []
        for state in sac.P0_STATES:
            asset = sidecar.states.get(state)
            if asset is not None and asset.is_accepted(assets_dir):
                present.append(state)
        return present

    results = []
    for entry in sorted(catalog, key=lambda e: e.class_id):
        sc = sidecars.get(entry.class_id)
        if entry.kind == "alias":
            # Aliases are complete if their target is.
            target = sac._resolve_alias_chain(entry.class_id, catalog)
            target_sc = sidecars.get(target)
            states_present = present_states(target_sc)
            complete = len(states_present) == len(sac.P0_STATES)
        else:
            states_present = present_states(sc)
            complete = len(states_present) == len(sac.P0_STATES)

        results.append({
            "class_id": entry.class_id,
            "display_name": entry.display_name,
            "kind": entry.kind,
            "states_present": states_present,
            "complete": complete,
        })
    return results


def get_missing(catalog: list[sac.CatalogEntry], assets_dir: Path = _ASSETS_DIR) -> list[dict[str, Any]]:
    """Get only incomplete entries."""
    return [e for e in check_completeness(catalog, assets_dir) if not e["complete"]]


# ---------------------------------------------------------------------------
# Worker-thread generation
# ---------------------------------------------------------------------------


class GenerationWorker:
    """Runs generation in a worker thread, marshaling results to the UI thread."""

    def __init__(
        self,
        provider: ProviderAdapter,
        catalog: list[sac.CatalogEntry] | None = None,
    ):
        self.provider = provider
        self.catalog = catalog
        self._thread: threading.Thread | None = None
        self._result_queue: queue.Queue = queue.Queue()
        self._cancel = threading.Event()

    def start_generation(
        self,
        entry: sac.CatalogEntry,
        state: str,
        retry_cap: int = 3,
        reference_image_path: str | Path | None = None,
    ) -> None:
        """Start a generation in a worker thread."""
        if self.is_running():
            raise RuntimeError("generation is already running")
        self._cancel.clear()
        self._thread = threading.Thread(
            target=self._run, args=(entry, state, retry_cap, reference_image_path), daemon=True
        )
        self._thread.start()

    def _run(
        self,
        entry: sac.CatalogEntry,
        state: str,
        retry_cap: int,
        reference_image_path: str | Path | None,
    ) -> None:
        """Worker thread: generate and put result in queue."""
        from generate_ship_art import generate_one
        try:
            result = generate_one(
                self.provider, entry, state,
                retry_cap=retry_cap,
                reference_image_path=reference_image_path,
                cancel_event=self._cancel,
                catalog=self.catalog,
            )
            self._result_queue.put(("result", result))
        except Exception as e:
            self._result_queue.put(("error", str(e)))

    def poll_result(self) -> tuple[str, Any] | None:
        """Poll for a result (non-blocking). Called from the UI thread."""
        try:
            return self._result_queue.get_nowait()
        except queue.Empty:
            return None

    def is_running(self) -> bool:
        return self._thread is not None and self._thread.is_alive()

    def cancel(self) -> None:
        self._cancel.set()


# ---------------------------------------------------------------------------
# tkinter reviewer
# ---------------------------------------------------------------------------


def run_reviewer(missing_only: bool = False) -> int:
    """Run the tkinter reviewer.

    This function requires a display.  In headless environments it will
    raise ImportError or RuntimeError.  Tests should not call this directly;
    they should test the worker and completeness logic above.
    """
    import tkinter as tk
    from tkinter import ttk, messagebox

    catalog = sac.load_catalog()
    overrides = load_prompt_overrides()

    if missing_only:
        entries_data = get_missing(catalog)
    else:
        entries_data = check_completeness(catalog)

    root = tk.Tk()
    root.title("Ship Art Reviewer")
    root.geometry("900x600")

    # --- Layout ---
    # Left: search + list.  Right: preview + details.
    paned = ttk.PanedWindow(root, orient=tk.HORIZONTAL)
    paned.pack(fill=tk.BOTH, expand=True)

    # Left panel.
    left = ttk.Frame(paned)
    paned.add(left, weight=1)

    search_var = tk.StringVar()
    search_var.trace_add("write", lambda *_: _filter_list())

    ttk.Label(left, text="Search:").pack(anchor=tk.W)
    search_entry = ttk.Entry(left, textvariable=search_var)
    search_entry.pack(fill=tk.X, padx=4, pady=2)

    listbox = tk.Listbox(left, height=20)
    listbox.pack(fill=tk.BOTH, expand=True, padx=4, pady=4)

    # Right panel.
    right = ttk.Frame(paned)
    paned.add(right, weight=2)

    detail_label = ttk.Label(right, text="Select a class to view details", wraplength=400)
    detail_label.pack(anchor=tk.W, padx=8, pady=4)

    # State/reference controls and zoomable preview.
    preview_controls = ttk.Frame(right)
    preview_controls.pack(fill=tk.X, padx=8, pady=2)
    state_var = tk.StringVar(value=sac.P0_STATES[0])
    reference_var = tk.StringVar(value="top_down")
    zoom_var = tk.DoubleVar(value=1.0)
    ttk.Label(preview_controls, text="State:").pack(side=tk.LEFT)
    state_box = ttk.Combobox(
        preview_controls, textvariable=state_var, values=sac.P0_STATES,
        state="readonly", width=12,
    )
    state_box.pack(side=tk.LEFT, padx=(2, 8))
    ttk.Label(preview_controls, text="Reference:").pack(side=tk.LEFT)
    reference_box = ttk.Combobox(
        preview_controls, textvariable=reference_var,
        values=("none",) + sac.P0_STATES, state="readonly", width=12,
    )
    reference_box.pack(side=tk.LEFT, padx=(2, 8))
    ttk.Label(preview_controls, text="Zoom:").pack(side=tk.LEFT)
    zoom_scale = ttk.Scale(
        preview_controls, variable=zoom_var, from_=0.25, to=3.0,
        orient=tk.HORIZONTAL,
    )
    zoom_scale.pack(side=tk.LEFT, fill=tk.X, expand=True)

    canvas = tk.Canvas(right, width=320, height=300, bg="#333")
    canvas.pack(padx=8, pady=4)

    # Prompt editor.
    prompt_frame = ttk.LabelFrame(right, text="Prompt Override")
    prompt_frame.pack(fill=tk.X, padx=8, pady=4)
    prompt_text = tk.Text(prompt_frame, height=3, width=50)
    prompt_text.pack(fill=tk.X, padx=4, pady=2)

    # Validation display.
    val_label = ttk.Label(right, text="Validation: (none)", wraplength=400, justify=tk.LEFT)
    val_label.pack(anchor=tk.W, padx=8, pady=4)

    # Buttons.
    btn_frame = ttk.Frame(right)
    btn_frame.pack(fill=tk.X, padx=8, pady=4)

    # Worker.
    provider = create_provider("gemini")
    worker = GenerationWorker(provider, catalog)
    repair_history = RepairHistory(
        _LOVE_DIR / "local" / "backups" / "reviewer",
        catalog,
        _ASSETS_DIR,
    )

    selected_class_id: list[str | None] = [None]

    def _filter_list():
        """Filter the list by search text."""
        query = search_var.get().lower()
        listbox.delete(0, tk.END)
        for entry in entries_data:
            text = f"{entry['class_id']} ({entry['display_name']})"
            if entry["complete"]:
                text += " ✓"
            else:
                text += f" [{','.join(entry['states_present']) or 'empty'}]"
            if query and query not in text.lower():
                continue
            listbox.insert(tk.END, text)

    def _reload_entries():
        """Refresh review status without replacing the callback-captured list."""
        refreshed = get_missing(catalog) if missing_only else check_completeness(catalog)
        entries_data[:] = refreshed
        _filter_list()

    def _on_select(event):
        """Handle list selection."""
        sel = listbox.curselection()
        if not sel:
            return
        idx = sel[0]
        text = listbox.get(idx)
        class_id = text.split(" ")[0]
        selected_class_id[0] = class_id

        entry = next((e for e in catalog if e.class_id == class_id), None)
        if entry is None:
            return

        detail_label.config(text=f"{class_id}\n{entry.display_name}\n{entry.kind} — {entry.visual_description}")

        # Load prompt override.
        key = f"{class_id}"
        prompt_text.delete("1.0", tk.END)
        prompt_text.insert("1.0", overrides.get(key, entry.visual_description))

        # Show preview if available.
        _show_preview(class_id, state_var.get())

        # Show validation.
        _show_validation(class_id, state_var.get())

    def _asset_owner(class_id: str) -> str:
        entry = next((item for item in catalog if item.class_id == class_id), None)
        if entry is not None and entry.kind == "alias":
            return sac._resolve_alias_chain(class_id, catalog)
        return class_id

    def _selected_image_path() -> Path | None:
        class_id = selected_class_id[0]
        if class_id is None:
            return None
        return _ASSETS_DIR / _asset_owner(class_id) / f"{state_var.get()}.png"

    def _show_preview(class_id: str, state: str):
        """Show the selected state at the requested zoom."""
        canvas.delete("all")
        img_path = _ASSETS_DIR / _asset_owner(class_id) / f"{state}.png"
        if not img_path.is_file():
            canvas.create_text(160, 150, text=f"(no {state} image)", fill="white")
            return
        try:
            from PIL import Image, ImageTk
            with Image.open(img_path) as source:
                img = source.convert("RGBA")
            factor = zoom_var.get()
            img = img.resize(
                (max(1, int(img.width * factor)), max(1, int(img.height * factor))),
                Image.NEAREST,
            )
            photo = ImageTk.PhotoImage(img)
            canvas.image = photo  # Keep reference.
            canvas.create_image(160, 150, image=photo)
        except Exception as e:
            canvas.create_text(160, 150, text=f"(error: {e})", fill="white")

    def _show_validation(class_id: str, state: str):
        """Show validation results for the selected class."""
        img_path = _ASSETS_DIR / _asset_owner(class_id) / f"{state}.png"
        if not img_path.is_file():
            val_label.config(text="Validation: (no image)")
            return
        result = sai.validate_image(img_path, is_portrait=state == "portrait")
        val_label.config(text=f"Validation: {result.outcome.value}\n{', '.join(result.issues) or 'no issues'}")

    def _refresh_selected(*_args):
        class_id = selected_class_id[0]
        if class_id is not None:
            _show_preview(class_id, state_var.get())
            _show_validation(class_id, state_var.get())

    def _save_prompt():
        """Save the current prompt override."""
        class_id = selected_class_id[0]
        if class_id is None:
            return
        text = prompt_text.get("1.0", tk.END).strip()
        if text:
            overrides[class_id] = text
        elif class_id in overrides:
            del overrides[class_id]
        save_prompt_overrides(overrides)
        messagebox.showinfo("Saved", "Prompt override saved.")

    def _regenerate():
        """Regenerate the selected class/state in a worker thread."""
        class_id = selected_class_id[0]
        if class_id is None:
            return
        owner = _asset_owner(class_id)
        entry = next((e for e in catalog if e.class_id == owner), None)
        if entry is None:
            return
        # Use prompt override if present.
        if class_id in overrides:
            entry.visual_description = overrides[class_id]
        target_state = state_var.get()
        reference_state = reference_var.get()
        reference_path = None
        if reference_state != "none":
            sidecar = sac.load_sidecars(_ASSETS_DIR).get(owner)
            asset = sidecar.states.get(reference_state) if sidecar is not None else None
            if asset is not None and asset.is_accepted(_ASSETS_DIR):
                reference_path = _ASSETS_DIR / asset.image_path
            else:
                messagebox.showerror(
                    "Missing reference", f"No accepted {reference_state} image exists."
                )
                return
        worker.start_generation(entry, target_state, reference_image_path=reference_path)
        messagebox.showinfo("Started", "Generation started in background.")

    def _cancel_generation():
        if worker.is_running():
            worker.cancel()
            val_label.config(text="Generation cancellation requested; output will be discarded.")

    def _repair(operation: str):
        image_path = _selected_image_path()
        if image_path is None or not image_path.is_file():
            messagebox.showerror("Repair", "No selected image to repair.")
            return
        try:
            repair_history.apply(image_path, operation)
            _refresh_selected()
        except Exception as error:
            messagebox.showerror("Repair failed", str(error))

    def _undo():
        image_path = _selected_image_path()
        if image_path is None or not repair_history.undo(image_path):
            messagebox.showinfo("Undo", "No repair to undo for this image.")
            return
        _refresh_selected()

    def _set_review_status(status: str):
        class_id = selected_class_id[0]
        image_path = _selected_image_path()
        if class_id is None or image_path is None or not image_path.is_file():
            messagebox.showerror("Review", "No selected image to review.")
            return
        owner = _asset_owner(class_id)
        try:
            sac.publish_review_status(
                catalog,
                _ASSETS_DIR,
                class_id=owner,
                state=state_var.get(),
                review_status=status,
            )
            _reload_entries()
            _refresh_selected()
        except Exception as error:
            messagebox.showerror("Review failed", str(error))

    def _poll_worker():
        """Poll the worker for results (called periodically from UI thread)."""
        result = worker.poll_result()
        if result:
            kind, data = result
            if kind == "result":
                gen_result = data
                if gen_result.success:
                    # Newly generated art remains unreviewed and unpublished
                    # until the reviewer explicitly accepts it.
                    messagebox.showinfo("Done", f"Generated {gen_result.class_id}/{gen_result.state}")
                    _refresh_selected()
                elif gen_result.outcome == "cancelled":
                    messagebox.showinfo("Cancelled", "Generation output was discarded.")
                else:
                    messagebox.showerror("Failed", f"Generation failed: {gen_result.error}")
            elif kind == "error":
                messagebox.showerror("Error", str(data))
        root.after(200, _poll_worker)

    ttk.Button(btn_frame, text="Save Prompt", command=_save_prompt).pack(side=tk.LEFT, padx=2)
    ttk.Button(btn_frame, text="Regenerate", command=_regenerate).pack(side=tk.LEFT, padx=2)
    ttk.Button(btn_frame, text="Accept", command=lambda: _set_review_status("accepted")).pack(side=tk.LEFT, padx=2)
    ttk.Button(btn_frame, text="Reject", command=lambda: _set_review_status("rejected")).pack(side=tk.LEFT, padx=2)
    ttk.Button(btn_frame, text="Cancel", command=_cancel_generation).pack(side=tk.LEFT, padx=2)
    ttk.Button(btn_frame, text="Flop", command=lambda: _repair("flop")).pack(side=tk.LEFT, padx=2)
    ttk.Button(btn_frame, text="Trim", command=lambda: _repair("trim")).pack(side=tk.LEFT, padx=2)
    ttk.Button(btn_frame, text="Resize 256", command=lambda: _repair("resize")).pack(side=tk.LEFT, padx=2)
    ttk.Button(btn_frame, text="Undo", command=_undo).pack(side=tk.LEFT, padx=2)

    listbox.bind("<<ListboxSelect>>", _on_select)
    state_box.bind("<<ComboboxSelected>>", _refresh_selected)
    zoom_scale.configure(command=_refresh_selected)

    _filter_list()
    root.after(200, _poll_worker)
    root.mainloop()
    return 0


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main(argv: list[str] | None = None) -> int:
    args = argv if argv is not None else sys.argv[1:]
    missing_only = "--missing" in args
    return run_reviewer(missing_only=missing_only)


if __name__ == "__main__":
    sys.exit(main())
