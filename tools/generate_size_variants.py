#!/usr/bin/env python3
"""Regenerate data/ships/*_{light,line,heavy}.toml and data/ship_costs.toml.

Cost model (docs/BALANCE-COST.md): frame-sunk + flat modules — NOT Combat D totals.

  Cost(s, L) = C_frame(s) + c_power * power + c_shield * max_shield_per_facing
               + sum(weapon list prices)

Combat stats (power/structure/weapons) still use JSONL-scaled capacity anchors so
hulls keep a sensible size ladder; only *pricing* is frame/module.

Regenerate: python3 tools/generate_size_variants.py
"""
from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "data" / "ships"
COST_OUT = ROOT / "data" / "ship_costs.toml"

# --- Combat capacity anchors (JSONL-scaled, destroyer line power≈14) ---
KP = 14 / 34
KS = 8 / 14

TIERS = [
    # size, key, name, stcs power p25/med/p75, ss p25/med/p75, thrust triples, vmax, shields
    dict(
        size=1,
        key="fighter",
        name="Fighter",
        power=(11, 14, 20),
        ss=(3, 4, 8),
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
        thrust=((1, 3), (1, 4), (1, 4)),
        vmax=4,
        shields=(7, 8, 9),
    ),
    dict(
        size=7,
        key="titan",
        name="Titan",
        power=(213, 219, 225),
        ss=(180, 210, 240),  # fill buys hull now (was flat 210)
        thrust=((1, 4), (1, 4), (1, 5)),
        vmax=3,
        shields=(8, 9, 11),
    ),
]

# Flat module / marginal prices (shipsim construction points).
# Tuned so destroyer_line ≈ 100.
C_POWER = 1.2  # design power is capability → marginal, not pure sunk engine
C_SHIELD_FACE = 3.0  # per max_shield_per_facing point (one face cap)
WEAPON_COST = {"beam": 12, "torp": 18, "plasma": 15}


def c_frame(size: int) -> float:
    """Large positive fixed intercept; α=2 so capitals carry heavy sunk hull cost.

    Relative scale: size 7 frame ≈ 12× size 2 frame. Absolute scale is applied
    after assembly so destroyer_line == 100 (see normalize in main).
    """
    return 50.0 * ((size / 2.0) ** 2.0)


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


def compute_cost(size: int, power: int, shields: int, weps: list[dict]) -> int:
    fixed = c_frame(size)
    weapons_c = sum(WEAPON_COST.get(w["kind"], 12) for w in weps)
    total = fixed + C_POWER * power + C_SHIELD_FACE * shields + weapons_c
    return max(1, int(round(total)))


def power_sys_boxes(size: int, vi: int) -> int:
    """SSD power boxes — frame depth (same lever as engine_boxes).

    Floor 3 from destroyer up (swarms must re-allocate); capitals add slowly
    so equal-budget focus can still strip a titan reactor over a fight, without
    the size+vi=9 stomp or the global-2 hulk bug.

      destroyer_line → 3; titan_light → 4; titan_heavy → 5
    """
    # +1 per 3 sizes above 1; +0/1 from variant (vi//2).
    # Empirically (abc_claims): titan_heavy=5 keeps B competitive; 6+ capital-stomps.
    depth = 2 + max(0, size - 1) // 3 + vi // 2
    floor = 3 if size >= 2 else 2
    return max(floor, depth)


def engine_boxes(size: int, vmax: int, vi: int) -> int:
    """SSD engine boxes — same depth family as power_sys."""
    depth = 2 + max(0, size - 1) // 3 + vi // 2
    floor = 3 if size >= 2 else 2
    return max(floor, depth)


def emit_weapon(w: dict) -> str:
    lines = ["[[weapons]]"]
    for k in ("id", "kind", "arc", "mount", "max_range", "max_charge"):
        val = w[k]
        lines.append(f'{k} = "{val}"' if isinstance(val, str) else f"{k} = {val}")
    return "\n".join(lines)


def main() -> None:
    # First pass: raw costs
    built = []
    for t in TIERS:
        for vi, vkey in enumerate(("light", "line", "heavy")):
            power = spower(t["power"][vi])
            structure = sstruct(t["ss"][vi])
            tpp, ppt = t["thrust"][vi]
            shields = t["shields"][vi]
            weps = weapons(t["size"], vi)
            raw = compute_cost(t["size"], power, shields, weps)
            p_sys = power_sys_boxes(t["size"], vi)
            e_boxes = engine_boxes(t["size"], t["vmax"], vi)
            built.append(
                dict(
                    t=t,
                    vi=vi,
                    vkey=vkey,
                    power=power,
                    structure=structure,
                    tpp=tpp,
                    ppt=ppt,
                    shields=shields,
                    weps=weps,
                    raw=raw,
                    power_sys=p_sys,
                    engine_boxes=e_boxes,
                )
            )

    # Normalize so destroyer_line == 100 (unit budget).
    dd_raw = next(
        b["raw"]
        for b in built
        if b["t"]["key"] == "destroyer" and b["vkey"] == "line"
    )
    scale = 100.0 / dd_raw

    catalog: list[str] = [
        "# Fleet cost catalog — frame-sunk + flat modules (docs/BALANCE-COST.md).",
        "# Cost = scale * (C_frame(size) + 1.2*power + 3*shield_cap + weapon prices).",
        "# Normalized destroyer_line = 100. NOT Combat-D totals.",
        "# Regenerate: python3 tools/generate_size_variants.py",
        "",
    ]
    rows = []
    for b in built:
        t = b["t"]
        pid = f"{t['key']}_{b['vkey']}"
        cost = max(1, int(round(b["raw"] * scale)))
        vmax = t["vmax"]
        speed = max(1, vmax)
        name = f"{t['name']} ({b['vkey'].title()})"
        text = f'''# Frame/module cost model (docs/BALANCE-COST.md).
# Regenerate: python3 tools/generate_size_variants.py
id = "{pid}"
name = "{name}"
size = {t["size"]}
speed = {speed}
power = {b["power"]}
max_shield_per_facing = {b["shields"]}
structure = {b["structure"]}
power_sys = {b["power_sys"]}
engine_boxes = {b["engine_boxes"]}
max_velocity = {vmax}
thrust_per_power = {b["tpp"]}
power_per_thrust = {b["ppt"]}
cost = {cost}

'''
        text += "\n\n".join(emit_weapon(w) for w in b["weps"]) + "\n"
        (OUT / f"{pid}.toml").write_text(text)
        catalog += [
            "[[ships]]",
            f'class = "{pid}"',
            f"size = {t['size']}",
            f'variant = "{b["vkey"]}"',
            f"cost = {cost}",
            f"power = {b['power']}",
            f"structure = {b['structure']}",
            f"c_frame = {c_frame(t['size']) * scale:.1f}",
            "",
        ]
        rows.append(
            (pid, t["size"], b["vkey"], cost, b["power"], b["structure"], len(b["weps"]))
        )

    COST_OUT.write_text("\n".join(catalog))
    print(f"wrote {len(rows)} ships under {OUT} and {COST_OUT}")
    print(f"{'class':28} sz var   cost  pwr  str  #w  $/pwr")
    for pid, size, vkey, cost, power, structure, nw in rows:
        print(
            f"{pid:28} {size:2} {vkey:6} {cost:5} {power:4} {structure:4} {nw:3}  {cost/power:5.2f}"
        )
    by = {r[0]: r for r in rows}
    dd = by["destroyer_line"][3]
    print(f"\ndestroyer_line cost={dd}; 8× = {8*dd}")
    for k in ("titan_light", "titan_line", "titan_heavy"):
        c = by[k][3]
        print(f"  {k}: cost={c}  vs DD = {c/dd:.2f}×  (8 DD budget buys {dd*8//c if c else 0} of these)")


if __name__ == "__main__":
    main()
