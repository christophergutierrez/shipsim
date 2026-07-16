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

import argparse
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "data" / "ships"
COST_OUT = ROOT / "data" / "ship_costs.toml"
GENERATED_MARKER = "# Regenerate: python3 tools/generate_size_variants.py"

# --- Combat capacity anchors (JSONL-scaled, destroyer line power≈14) ---
KP = 14 / 34
KS = 8 / 14

TIERS = [
    # size, key, name, stcs power p25/med/p75, ss p25/med/p75, thrust triples, vmax, shields
    # Lever #3 — max_shield_per_facing only on capital heavy face max.
    # DD+ broad buffs overshot (A→97%, B swarm~13% + high IP). Final #3:
    # restore pre-#3 DD/mid shields; titan_heavy 13→12 only (fill weapons stay).
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
        shields=(3, 4, 5),  # same as post-#2 (do not buff swarm faces)
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
        power=(90, 108, 140),
        ss=(52, 72, 100),
        thrust=((1, 2), (1, 3), (1, 3)),
        vmax=5,
        shields=(6, 7, 9),
    ),
    dict(
        size=6,
        key="dreadnought",
        name="Dreadnought",
        power=(150, 170, 195),
        ss=(120, 130, 170),
        thrust=((1, 3), (1, 4), (1, 4)),
        vmax=4,
        shields=(7, 8, 10),
    ),
    dict(
        size=7,
        key="titan",
        name="Titan",
        power=(200, 230, 260),
        ss=(170, 210, 280),
        thrust=((1, 4), (1, 4), (1, 5)),
        vmax=3,
        # post-#2 was (8,10,13); only heavy face-max −1
        shields=(8, 10, 12),
    ),
]

# Flat module / marginal prices (shipsim construction points).
# Lever #5 — cost ratios: slightly higher weapon prices so capital *fill* is
# expensive; slightly softer frame exponent so min titan is not ~9× DD (A used
# 9 DD and stomped). Normalize still forces destroyer_line == 100.
C_POWER = 1.25
C_SHIELD_FACE = 3.0
WEAPON_COST = {"beam": 14, "torp": 20, "plasma": 16}


def c_frame(size: int) -> float:
    """Large positive fixed intercept; α≈1.85 (was 2.0).

    Softer capital sink → after DD=100 normalize, titan_light closer to ~8× DD
    (claim A equal-budget swarm size). Heavy still pays via modules.
    """
    return 52.0 * ((size / 2.0) ** 1.85)


def spower(stcs: float) -> int:
    return max(4, round(stcs * KP))


def sstruct(stcs: float) -> int:
    return max(2, round(stcs * KS))


def weapons(size: int, vi: int) -> list[dict]:
    """Lever #2 — capital fill: size≥5 line/heavy get deeper batteries + charge.

    Light capital fits stay thinner so claim A (min titan) stays swarm-favored.
    """

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

    # Beam charge scales with capital fill (not destroyers).
    if size >= 7 and vi >= 2:
        bch = 6
    elif size >= 6 and vi >= 1:
        bch = 5
    elif size >= 5 and vi >= 2:
        bch = 5
    else:
        bch = 4

    if size == 1:
        w = [beam(1, ch=3 if vi == 0 else 4)]
        if vi == 2:
            w.append(torp(1))
    elif size == 2:
        # Lever #4 tested at n=1k and rejected for this operating point:
        #   beam charge 3 → A~20% swarm, B/C 100% titan (swarm dead)
        #   max_range 9, charge 4 → B~44/49 nicer but A~89% (A worse)
        # Keep charge 4 / range 10 (post-#3). Next soft-swarm work is #5 cost counts.
        w = [beam(1, ch=4)]
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
            beam(1, ch=bch),
            beam(2, mount="forward_starboard", ch=bch),
            torp(1),
        ]
        if vi >= 1:
            w.append(plasma(1))
        if vi == 2:
            w.append(beam(3, mount="forward_port", ch=bch))
            w.append(torp(2))
    elif size == 6:
        w = [
            beam(1, ch=bch),
            beam(2, mount="forward_starboard", ch=bch),
            beam(3, mount="forward_port", ch=bch),
            torp(1),
            plasma(1),
        ]
        if vi >= 1:
            w.append(beam(4, mount="aft", arc="rear", ch=bch))
        if vi == 2:
            w.append(torp(2))
            w.append(plasma(2))
    else:
        # Titan: light stays moderate; line/heavy denser + higher charge.
        w = [
            beam(1, ch=bch if vi > 0 else 4),
            beam(2, mount="forward_starboard", ch=bch if vi > 0 else 4),
            beam(3, mount="forward_port", ch=bch if vi > 0 else 4),
            torp(1),
            plasma(1),
        ]
        if vi >= 1:
            w.append(beam(4, mount="aft", arc="rear", ch=bch))
            w.append(torp(2))
        if vi == 2:
            w.append(plasma(2))
            w.append(beam(5, mount="aft_starboard", arc="rear", ch=bch))
            w.append(beam(6, mount="aft_port", arc="rear", ch=bch))
            w.append(torp(3))
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
    # n≈128: light=4/heavy=5 → A swarm ~88%, B ~62/25. Nudge: floor 5 on
    # size≥7 so min titan is less glass (A softer) while heavy stays 5 (not 6).
    depth = 2 + max(0, size - 1) // 3 + vi // 2
    floor = 3 if size >= 2 else 2
    if size >= 7:
        floor = 5
    return max(floor, depth)


def engine_boxes(size: int, vmax: int, vi: int) -> int:
    """SSD engine boxes — same depth family as power_sys."""
    depth = 2 + max(0, size - 1) // 3 + vi // 2
    floor = 3 if size >= 2 else 2
    if size >= 7:
        floor = 5
    return max(floor, depth)


def weapon_boxes(size: int, vi: int) -> int:
    """SSD boxes per weapon; Phase 3 titan survival lever."""
    if size == 7 and vi == 0:
        return 5
    if size == 7 and vi == 2:
        return 3
    return 1


def attack_accuracy_bonus(size: int, vi: int) -> int:
    """Catalog fire control against exact size-2 targets."""
    if size == 7 and vi == 0:
        return 12
    if size == 7 and vi == 2:
        return 10
    return 0


def emit_weapon(w: dict) -> str:
    lines = ["[[weapons]]"]
    for k in ("id", "kind", "arc", "mount", "max_range", "max_charge"):
        val = w[k]
        lines.append(f'{k} = "{val}"' if isinstance(val, str) else f"{k} = {val}")
    return "\n".join(lines)


def check_breakpoints(built: list[dict], *, strict: bool) -> bool:
    """Warn (stderr) when a hull's shield facing sits at or one off an exact
    range-1 beam-damage breakpoint: max_shield_per_facing % (2*max_charge).

    r1 beam damage = 2 * max_charge of the hull's best (highest-charge) beam.
    remainder == 0 means a single beam volley exactly zeroes the facing on the
    Nth hit with nothing left over (exact breakpoint); remainder == 1 means
    the facing is one point off from that same exact-kill threshold.

    This does not change any generated values — it only reports. The catalog
    currently HAS exact breakpoints (titans, dreadnought_heavy) and must keep
    generating regardless; use --strict to fail the build on exact breakpoints
    once that's no longer acceptable.

    Returns True if any exact breakpoint (remainder == 0) was found.
    """
    rows = []
    has_exact = False
    for b in built:
        t = b["t"]
        pid = f"{t['key']}_{b['vkey']}"
        beams = [w for w in b["weps"] if w["kind"] == "beam"]
        if not beams:
            continue
        max_charge = max(w["max_charge"] for w in beams)
        dmg = 2 * max_charge
        shield = b["shields"]
        remainder = shield % dmg if dmg else shield
        flag = ""
        if remainder == 0:
            flag = "BREAKPOINT (exact)"
            has_exact = True
        elif remainder == 1:
            flag = "one point off"
        rows.append((pid, shield, max_charge, dmg, remainder, flag))

    print("\nBreakpoint guard: shield-facing vs r1 beam damage (2*max_charge)",
          file=sys.stderr)
    print(f"{'hull':28} shield charge  dmg  rem  flag", file=sys.stderr)
    for pid, shield, max_charge, dmg, remainder, flag in rows:
        print(
            f"{pid:28} {shield:6} {max_charge:6} {dmg:4} {remainder:4}  {flag}",
            file=sys.stderr,
        )
    flagged = [r for r in rows if r[5]]
    if flagged:
        print(
            f"\n{len(flagged)} hull(s) at or one point off an exact r1 beam breakpoint:",
            file=sys.stderr,
        )
        for pid, shield, max_charge, dmg, remainder, flag in flagged:
            print(f"  {pid}: shield={shield} 2*max_charge={dmg} remainder={remainder} ({flag})",
                  file=sys.stderr)
    else:
        print("\nno breakpoints found", file=sys.stderr)

    if strict and has_exact:
        print(
            "\n--strict: exact r1 beam breakpoint(s) present, failing",
            file=sys.stderr,
        )
    return has_exact


def build_catalog(output_root: Path = ROOT) -> tuple[list[dict], dict[Path, str]]:
    """Compute the full generated catalog in memory.

    Returns the raw per-hull `built` records (for the breakpoint guard/summary
    printing) and a mapping of every generated file path to its expected text,
    without writing anything to disk.
    """
    out = output_root / "data" / "ships"
    cost_out = output_root / "data" / "ship_costs.toml"

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
            w_boxes = weapon_boxes(t["size"], vi)
            accuracy = attack_accuracy_bonus(t["size"], vi)
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
                    weapon_boxes=w_boxes,
                    attack_accuracy_bonus=accuracy,
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
    outputs: dict[Path, str] = {}
    rows = []
    for b in built:
        t = b["t"]
        pid = f"{t['key']}_{b['vkey']}"
        cost = max(1, int(round(b["raw"] * scale)))
        vmax = t["vmax"]
        speed = max(1, vmax)
        name = f"{t['name']} ({b['vkey'].title()})"
        phase3_fields = ""
        if b["weapon_boxes"] != 1:
            phase3_fields += f'weapon_boxes = {b["weapon_boxes"]}\n'
        if b["attack_accuracy_bonus"] != 0:
            phase3_fields += (
                f'attack_accuracy_bonus = {b["attack_accuracy_bonus"]}\n'
            )
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
{phase3_fields}\
max_velocity = {vmax}
thrust_per_power = {b["tpp"]}
power_per_thrust = {b["ppt"]}
cost = {cost}

'''
        text += "\n\n".join(emit_weapon(w) for w in b["weps"]) + "\n"
        outputs[out / f"{pid}.toml"] = text
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

    outputs[cost_out] = "\n".join(catalog)
    return built, outputs


def catalog_mismatches(
    outputs: dict[Path, str], output_root: Path = ROOT
) -> list[Path]:
    """Return missing, changed, and obsolete generated catalog paths."""
    mismatches = {
        path
        for path, expected in outputs.items()
        if not path.exists() or path.read_text() != expected
    }
    expected_paths = set(outputs)
    ships_dir = output_root / "data" / "ships"
    if ships_dir.exists():
        for path in ships_dir.glob("*.toml"):
            if path not in expected_paths and GENERATED_MARKER in path.read_text():
                mismatches.add(path)
    return sorted(mismatches)


def print_summary(
    built: list[dict], outputs: dict[Path, str], output_root: Path = ROOT
) -> None:
    out = output_root / "data" / "ships"
    cost_out = output_root / "data" / "ship_costs.toml"
    rows = []
    for b in built:
        t = b["t"]
        pid = f"{t['key']}_{b['vkey']}"
        text = outputs[out / f"{pid}.toml"]
        cost = int(next(line for line in text.splitlines() if line.startswith("cost = ")).split()[-1])
        rows.append((pid, t["size"], b["vkey"], cost, b["power"], b["structure"], len(b["weps"])))

    print(f"wrote {len(rows)} ships under {out} and {cost_out}")
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


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--strict",
        action="store_true",
        help="exit nonzero if any hull has an exact r1 beam-damage breakpoint",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="non-destructive: compare generated output against tracked files "
        "and exit nonzero on any mismatch, without writing anything",
    )
    args = parser.parse_args()

    built, outputs = build_catalog()

    if args.check:
        mismatches = catalog_mismatches(outputs)
        if mismatches:
            print("generator --check: mismatched files:", file=sys.stderr)
            for path in mismatches:
                print(f"  {path.relative_to(ROOT)}", file=sys.stderr)
            print(
                f"\n{len(mismatches)} file(s) differ from `python3 tools/generate_size_variants.py` output. "
                "Regenerate and commit, or fix the generator.",
                file=sys.stderr,
            )
            sys.exit(1)
        print(f"generator --check: {len(outputs)} file(s) match tracked output")
    else:
        for path, text in outputs.items():
            path.write_text(text)
        print_summary(built, outputs)

    has_exact = check_breakpoints(built, strict=args.strict)
    if args.strict and has_exact:
        sys.exit(1)


if __name__ == "__main__":
    main()
