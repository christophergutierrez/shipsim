#!/usr/bin/env python3
"""Regenerate data/ships/*_{light,line,heavy}.toml and data/ship_costs.toml.

Methodology: docs/SIZE-VARIANTS.md (JSONL class buckets + Combat D cost).
"""
from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "data" / "ships"
COST_OUT = ROOT / "data" / "ship_costs.toml"

TIERS = [
    dict(
        size=1,
        key="fighter",
        name="Fighter",
        power=(11, 14, 20),
        ss=(3, 4, 8),
        d=41.3,
        thrust=((3, 1), (2, 1), (2, 1)),
        vmax=8,
        shields=(2, 3, 4),
    ),
    dict(
        size=2,
        key="destroyer",
        name="Destroyer",
        power=(26, 34, 40),
        ss=(10, 14, 18),
        d=67.7,
        thrust=((3, 1), (2, 1), (1, 1)),
        vmax=8,
        shields=(3, 4, 5),
    ),
    dict(
        size=3,
        key="light_cruiser",
        name="Light Cruiser",
        power=(38, 43, 52),
        ss=(18, 24, 29),
        d=109.7,
        thrust=((2, 1), (1, 1), (1, 2)),
        vmax=7,
        shields=(4, 5, 6),
    ),
    dict(
        size=4,
        key="heavy_cruiser",
        name="Heavy Cruiser",
        power=(60, 70, 80),
        ss=(30, 34, 44),
        d=166.9,
        thrust=((1, 1), (1, 1), (1, 2)),
        vmax=6,
        shields=(5, 6, 7),
    ),
    dict(
        size=5,
        key="battleship",
        name="Battleship",
        power=(90, 108, 130),
        ss=(52, 72, 90),
        d=266.85,
        thrust=((1, 2), (1, 3), (1, 3)),
        vmax=5,
        shields=(6, 7, 8),
    ),
    dict(
        size=6,
        key="dreadnought",
        name="Dreadnought",
        power=(150, 170, 180),
        ss=(120, 130, 154),
        d=384.8,
        thrust=((1, 3), (1, 4), (1, 4)),
        vmax=4,
        shields=(7, 8, 9),
    ),
    dict(
        size=7,
        key="titan",
        name="Titan",
        power=(213, 219, 225),
        ss=(210, 210, 210),
        d=540.3,
        thrust=((1, 4), (1, 4), (1, 5)),
        vmax=3,
        shields=(8, 8, 10),
    ),
]

KP = 14 / 34
KS = 8 / 14
VARIANTS = [("light", 0, 0.85), ("line", 1, 1.00), ("heavy", 2, 1.20)]


def spower(stcs: float) -> int:
    return max(4, round(stcs * KP))


def sstruct(stcs: float) -> int:
    return max(2, round(stcs * KS))


def weapons(size: int, vi: int) -> list[dict]:
    def beam(i, mount="forward", arc="forward", ch=4):
        return {
            "id": f"beam_{i}",
            "kind": "beam",
            "arc": arc,
            "mount": mount,
            "max_range": 10,
            "max_charge": ch,
        }

    def torp(i):
        return {
            "id": f"torp_{i}",
            "kind": "torp",
            "arc": "forward",
            "mount": "forward",
            "max_range": 12,
            "max_charge": 1,
        }

    def plasma(i):
        return {
            "id": f"plasma_{i}",
            "kind": "plasma",
            "arc": "forward",
            "mount": "forward",
            "max_range": 6,
            "max_charge": 1,
        }

    if size == 1:
        w = [beam(1, ch=3 if vi == 0 else 4)]
        if vi == 2:
            w.append(torp(1))
    elif size == 2:
        w = [beam(1)]
        if vi == 2:
            w.append(torp(1))
    elif size == 3:
        w = [beam(1)]
        if vi >= 1:
            w.append(torp(1))
        if vi == 2:
            w.append(plasma(1))
    elif size == 4:
        if vi == 0:
            w = [beam(1), torp(1)]
        else:
            w = [beam(1), torp(1), plasma(1)]
        if vi == 2:
            w.append(beam(2, mount="forward_starboard"))
    elif size == 5:
        w = [
            beam(1),
            beam(2, mount="forward_starboard"),
            torp(1),
        ]
        if vi >= 1:
            w.append(plasma(1))
        if vi == 2:
            w.append(beam(3, mount="forward_port"))
    elif size == 6:
        w = [
            beam(1),
            beam(2, mount="forward_starboard"),
            beam(3, mount="forward_port"),
            torp(1),
            plasma(1),
        ]
        if vi == 2:
            w.append(torp(2))
    else:
        w = [
            beam(1),
            beam(2, mount="forward_starboard"),
            beam(3, mount="forward_port"),
            torp(1),
            plasma(1),
        ]
        if vi >= 1:
            w.append(beam(4, mount="aft", arc="rear"))
        if vi == 2:
            w.append(torp(2))
            w.append(plasma(2))
    return w


def emit_weapon(w: dict) -> str:
    lines = ["[[weapons]]"]
    for k in ("id", "kind", "arc", "mount", "max_range", "max_charge"):
        val = w[k]
        lines.append(f'{k} = "{val}"' if isinstance(val, str) else f"{k} = {val}")
    return "\n".join(lines)


def main() -> None:
    catalog: list[str] = [
        "# Fleet cost catalog for size variants (mirrors cost= in each ship TOML).",
        "# Unit: destroyer_line = 100, ratioed from JSONL Combat D medians.",
        "# Regenerate: python3 tools/generate_size_variants.py",
        "",
    ]
    for t in TIERS:
        line_cost = round(100 * t["d"] / 67.7)
        for vi, (vkey, idx, cost_mult) in enumerate(VARIANTS):
            pid = f"{t['key']}_{vkey}"
            power = spower(t["power"][idx])
            structure = sstruct(t["ss"][idx])
            cost = max(1, round(line_cost * cost_mult))
            tpp, ppt = t["thrust"][idx]
            shields = t["shields"][idx]
            vmax = t["vmax"]
            speed = max(1, vmax)
            name = f"{t['name']} ({vkey.title()})"
            weps = weapons(t["size"], vi)
            text = f'''# Auto-derived draft from tmp/sfb/ships.jsonl size bucket + Combat-D cost ratio.
# See docs/SIZE-VARIANTS.md. Regenerate: python3 tools/generate_size_variants.py
id = "{pid}"
name = "{name}"
size = {t["size"]}
speed = {speed}
power = {power}
max_shield_per_facing = {shields}
structure = {structure}
max_velocity = {vmax}
thrust_per_power = {tpp}
power_per_thrust = {ppt}
cost = {cost}

'''
            text += "\n\n".join(emit_weapon(w) for w in weps) + "\n"
            (OUT / f"{pid}.toml").write_text(text)
            catalog += [
                "[[ships]]",
                f'class = "{pid}"',
                f"size = {t['size']}",
                f'variant = "{vkey}"',
                f"cost = {cost}",
                f"power = {power}",
                f"structure = {structure}",
                "",
            ]
    COST_OUT.write_text("\n".join(catalog))
    print(f"wrote 21 ships under {OUT} and {COST_OUT}")


if __name__ == "__main__":
    main()
