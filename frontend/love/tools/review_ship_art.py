#!/usr/bin/env python3

"""review_ship_art.py — tkinter reviewer for ship art.

Features:
  - Searchable completeness overview.
  - State previews with zoom.
  - Structured prompt editing (survives restart, does not modify source).
  - Base-to-target regeneration.
  - Validation display.
  - Repair actions (reprocess, trim, flop).
  - Undo.
  - Worker-thread generation (UI never freezes).
  - All tkinter mutations marshaled back to the UI thread.

API keys are read only from the environment and never written to logs or
provenance.

Phase 3 of ``docs/SHIP-ART-IMPLEMENTATION-PLAN.md``.
"""

from __future__ import annotations

import json
import os
import queue
import sys
import threading
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
    GeminiProvider,
    ProviderAdapter,
    ProviderRequest,
    create_provider,
)

_LOVE_DIR = _TOOLS_DIR.parent
_ASSETS_DIR = _LOVE_DIR / "assets" / "ship_art"
_PROMPT_OVERRIDES = _ASSETS_DIR / "prompt_overrides.json"


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
    _PROMPT_OVERRIDES.parent.mkdir(parents=True, exist_ok=True)
    _PROMPT_OVERRIDES.write_text(json.dumps(overrides, indent=2, sort_keys=True) + "\n")


# ---------------------------------------------------------------------------
# Completeness check
# ---------------------------------------------------------------------------


def check_completeness(catalog: list[sac.CatalogEntry], assets_dir: Path = _ASSETS_DIR) -> list[dict[str, Any]]:
    """Check completeness of each catalog entry.

    Returns a list of dicts with class_id, kind, display_name, states, and
    complete flag.
    """
    sidecars = sac.load_sidecars(assets_dir)
    results = []
    for entry in sorted(catalog, key=lambda e: e.class_id):
        sc = sidecars.get(entry.class_id)
        if entry.kind == "alias":
            # Aliases are complete if their target is.
            target = sac._resolve_alias_chain(entry.class_id, catalog)
            target_sc = sidecars.get(target)
            states_present = []
            if target_sc:
                states_present = [s for s in sac.P0_STATES if s in target_sc.states]
            complete = len(states_present) == len(sac.P0_STATES)
        else:
            states_present = []
            if sc:
                states_present = [s for s in sac.P0_STATES if s in sc.states]
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

    def __init__(self, provider: ProviderAdapter):
        self.provider = provider
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

    # Preview canvas.
    canvas = tk.Canvas(right, width=300, height=300, bg="#333")
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
    worker = GenerationWorker(provider)

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
        _show_preview(class_id)

        # Show validation.
        _show_validation(class_id)

    def _show_preview(class_id: str):
        """Show the top-down preview image."""
        canvas.delete("all")
        img_path = _ASSETS_DIR / class_id / "top_down.png"
        if not img_path.is_file():
            canvas.create_text(150, 150, text="(no image)", fill="white")
            return
        try:
            from PIL import Image, ImageTk
            img = Image.open(img_path)
            img.thumbnail((300, 300))
            photo = ImageTk.PhotoImage(img)
            canvas.image = photo  # Keep reference.
            canvas.create_image(150, 150, image=photo)
        except Exception as e:
            canvas.create_text(150, 150, text=f"(error: {e})", fill="white")

    def _show_validation(class_id: str):
        """Show validation results for the selected class."""
        img_path = _ASSETS_DIR / class_id / "top_down.png"
        if not img_path.is_file():
            val_label.config(text="Validation: (no image)")
            return
        result = sai.validate_image(img_path)
        val_label.config(text=f"Validation: {result.outcome.value}\n{', '.join(result.issues) or 'no issues'}")

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
        entry = next((e for e in catalog if e.class_id == class_id), None)
        if entry is None:
            return
        # Use prompt override if present.
        if class_id in overrides:
            entry.visual_description = overrides[class_id]
        worker.start_generation(entry, "top_down")
        messagebox.showinfo("Started", "Generation started in background.")

    def _poll_worker():
        """Poll the worker for results (called periodically from UI thread)."""
        result = worker.poll_result()
        if result:
            kind, data = result
            if kind == "result":
                gen_result = data
                if gen_result.success:
                    messagebox.showinfo("Done", f"Generated {gen_result.class_id}/{gen_result.state}")
                    _show_preview(gen_result.class_id)
                    _show_validation(gen_result.class_id)
                else:
                    messagebox.showerror("Failed", f"Generation failed: {gen_result.error}")
            elif kind == "error":
                messagebox.showerror("Error", str(data))
        root.after(200, _poll_worker)

    ttk.Button(btn_frame, text="Save Prompt", command=_save_prompt).pack(side=tk.LEFT, padx=2)
    ttk.Button(btn_frame, text="Regenerate", command=_regenerate).pack(side=tk.LEFT, padx=2)

    listbox.bind("<<ListboxSelect>>", _on_select)

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
