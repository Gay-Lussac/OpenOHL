#!/usr/bin/env python3
# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.
"""The cross-check of the 2D fields against hvlbuzz (benchmarks/electrical/hvlbuzz.toml).

hvlbuzz (FKH, Switzerland; https://gitlab.com/fkh-ch/emc, GPL-3.0-or-later)
computes the electric and magnetic fields of a line's cross-section with
its own charge simulation (12 line charges round every subconductor) and
Biot–Savart's law. Its project files are in benchmarks/electrical/hvlbuzz/
(induced currents in the earth wires neglected); run it on each,

    hvlbuzz-cli benchmarks/electrical/hvlbuzz/<case>.json

which writes <case>_profile.csv (s [m]; E_eff [V/m]; B_eff [µT]) beside
it, then this script (python3 -I benchmarks/electrical/reference_hvlbuzz.py
TOL_E TOL_B) prints the cases' TOML: the inputs as the program's
`electrical.fields_2d` takes them and hvlbuzz's values as the expected.
Not run by the checks (it needs hvlbuzz built); the values it wrote are
kept in hvlbuzz.toml (run 2026-10-29, hvlbuzz 2.0 from its main branch).
"""

import csv
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CASES = [
    ("electrical-050", "A-flat-380kV", "One 380 kV circuit, flat, two-conductor bundles, 1000 A"),
    ("electrical-051", "B-double-110kV", "A 110 kV double circuit, phases one above another, 1000 A"),
    ("electrical-052", "C-double-110kV-earthwire", "The 110 kV double circuit with an earth wire (at earth potential)"),
]


def main(argv):
    tol_e = float(argv[0]) if argv else 0.002
    tol_b = float(argv[1]) if len(argv) > 1 else 1e-7
    for cid, name, title in CASES:
        p = json.load(open(os.path.join(HERE, "hvlbuzz", name + ".json")))
        rows = list(csv.reader(open(os.path.join(HERE, "hvlbuzz", name + "_profile.csv")), delimiter=";"))[1:]
        offsets = [float(r[0]) for r in rows]
        e = [float(r[1]) for r in rows]
        b = [float(r[2]) for r in rows]
        height = p["transverse_profile"]["y"]
        print("[[case]]")
        print(f'id = "{cid}"')
        print(f'title = "{title}: E and B 1 m above the ground against hvlbuzz"')
        print('calculation = "electrical.fields_2d"')
        print('reference = "hvlbuzz 2.0 (FKH, gitlab.com/fkh-ch/emc), its own charge simulation and Biot–Savart; benchmarks/electrical/hvlbuzz/' + name + '.json"')
        print("[case.inputs]")
        print(f"height = {height}")
        print("offsets = [" + ", ".join(f"{x:g}" for x in offsets) + "]")
        for s in p["systems"]:
            phases = ", ".join(f"[{q['x']:g}, {q['y']:g}]" for q in s["phases"])
            print("[[case.inputs.systems]]")
            print(f"subconductors = {s['n_t']}")
            print(f"spacing = {s['a']}")
            print(f"diameter = {s['d']}")
            print(f"kv = {s['U'] / 1e3:g}")
            print(f"amps = {s['I']:g}")
            print(f"bundle_angle = {s['beta_s']:g}")
            print(f"phases = [{phases}]")
        for g in p["guardwires"]:
            print("[[case.inputs.earth_wires]]")
            print(f"x = {g['x']:g}")
            print(f"y = {g['y']:g}")
            print(f"diameter = {g['d']}")
        print("[case.expected]")
        for k, (ek, bk) in enumerate(zip(e, b)):
            print(f"e_v_m_{k:02} = {{ value = {ek:.10g}, rel_tol = {tol_e} }}")
            print(f"b_ut_{k:02} = {{ value = {bk:.10g}, rel_tol = {tol_b} }}")
        print(f"e_max_v_m = {{ value = {max(e):.10g}, rel_tol = {tol_e} }}")
        print(f"b_max_ut = {{ value = {max(b):.10g}, rel_tol = {tol_b} }}")
        print()


if __name__ == "__main__":
    main(sys.argv[1:])
