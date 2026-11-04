#!/usr/bin/env python3
# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.
"""The cross-checks against hvlbuzz (benchmarks/electrical/hvlbuzz.toml,
benchmarks/electrical/hvlbuzz-gradient.toml).

hvlbuzz — "HVLBuzz 2.0" (Michael Kefeder, Gerhard Bräunlich, Joel Winiger,
the authors its Cargo.toml names; © FKH, https://fkh.ch;
https://gitlab.com/fkh-ch/emc; documentation
https://fkh-ch.gitlab.io/emc/hvlbuzz/index.html; GNU General Public
License version 3, its LICENSE file) — computes the electric and magnetic
fields and the corona noise of a line's cross-section: its own charge
simulation (by default 12 line charges in every subconductor), Biot–Savart's
law. Its project files are in benchmarks/electrical/hvlbuzz/ (induced
currents in the earth wires neglected); run it on each,

    hvlbuzz-cli benchmarks/electrical/hvlbuzz/<case>.json

which writes <case>_profile.csv (s [m]; E_eff [V/m]; B_eff [µT]) and
<case>_bundle.csv (per bundle "E_max (bundle, AC) [kV/cm]") beside it.

1. The fields (python3 -I benchmarks/electrical/reference_hvlbuzz.py
   TOL_E TOL_B): the cases' TOML of hvlbuzz.toml, the inputs as the
   program's `electrical.fields_2d` takes them, hvlbuzz's profiles as the
   expected (run 2026-10-29).
2. The surface gradients (python3 -I
   benchmarks/electrical/reference_hvlbuzz.py gradient TOL_CSM TOL_MM):
   the cases' TOML of hvlbuzz-gradient.toml, `electrical.surface_gradient_2d`.
   hvlbuzz's bundle table with the project's "n_contour" set (2026-11-04):
   360 — its charge simulation with 360 charges, the largest gradient
   over the contour points averaged over the bundle (the average
   maximum), kept as <case>_bundle_n360.csv; 2 (below 3) — the mean
   gradient |q|/(2 pi eps0 r) of the bundle's charge shared by its
   subconductors times 1 + (n - 1) r/R (Markt–Mengele), kept as
   <case>_bundle_n2.csv; and its default (12) for the record,
   <case>_bundle_n12.csv. The program's average maximum is checked
   against the first, its Markt–Mengele value against the second.

Not run by the checks (it needs hvlbuzz built); the values it wrote are
kept in the TOML files (hvlbuzz 2.0, built 2026-10-29 from
gitlab.com/fkh-ch/emc).
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
GRADIENT_CASES = [
    # id, project, title, line charges per subconductor (None: the
    # program's default), the average maximum's tolerance (None: TOL_CSM)
    ("electrical-066", "A-flat-380kV", "One 380 kV circuit, flat, two-conductor bundles", None, None),
    ("electrical-067", "B-double-110kV", "A 110 kV double circuit, phases one above another", None, None),
    ("electrical-068", "C-double-110kV-earthwire", "The 110 kV double circuit with an earth wire (at earth potential)", None, None),
    ("electrical-072", "A-flat-380kV", "One 380 kV circuit, flat, two-conductor bundles, 64 charges per subconductor", 64, 2e-5),
    ("electrical-073", "C-double-110kV-earthwire", "The 110 kV double circuit with an earth wire, 64 charges per subconductor", 64, 2e-5),
]


def inputs(p, with_current):
    for s in p["systems"]:
        phases = ", ".join(f"[{q['x']:g}, {q['y']:g}]" for q in s["phases"])
        print("[[case.inputs.systems]]")
        print(f"subconductors = {s['n_t']}")
        print(f"spacing = {s['a']}")
        print(f"diameter = {s['d']}")
        print(f"kv = {s['U'] / 1e3:g}")
        if with_current:
            print(f"amps = {s['I']:g}")
        print(f"bundle_angle = {s['beta_s']:g}")
        print(f"phases = [{phases}]")
    for g in p["guardwires"]:
        print("[[case.inputs.earth_wires]]")
        print(f"x = {g['x']:g}")
        print(f"y = {g['y']:g}")
        print(f"diameter = {g['d']}")


def bundle_column(name, n):
    rows = list(csv.reader(open(os.path.join(HERE, "hvlbuzz", f"{name}_bundle_n{n}.csv")), delimiter=";"))
    k = rows[0].index("E_max (bundle, AC) [kV/cm]")
    return [float(r[k]) for r in rows[1:]]


def fields(argv):
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
        inputs(p, True)
        print("[case.expected]")
        for k, (ek, bk) in enumerate(zip(e, b)):
            print(f"e_v_m_{k:02} = {{ value = {ek:.10g}, rel_tol = {tol_e} }}")
            print(f"b_ut_{k:02} = {{ value = {bk:.10g}, rel_tol = {tol_b} }}")
        print(f"e_max_v_m = {{ value = {max(e):.10g}, rel_tol = {tol_e} }}")
        print(f"b_max_ut = {{ value = {max(b):.10g}, rel_tol = {tol_b} }}")
        print()


def gradient(argv):
    tol_csm = float(argv[0]) if argv else 2e-4
    tol_mm = float(argv[1]) if len(argv) > 1 else 1e-3
    for cid, name, title, charges, tol in GRADIENT_CASES:
        tol = tol or tol_csm
        p = json.load(open(os.path.join(HERE, "hvlbuzz", name + ".json")))
        csm = bundle_column(name, 360)
        mm = bundle_column(name, 2)
        print("[[case]]")
        print(f'id = "{cid}"')
        print(f'title = "{title}: the conductor surface gradients against hvlbuzz"')
        print('calculation = "electrical.surface_gradient_2d"')
        print('reference = "hvlbuzz 2.0 (FKH, gitlab.com/fkh-ch/emc), its bundle table: n_contour 360 (its charge simulation, the average maximum) and 2 (Markt–Mengele); benchmarks/electrical/hvlbuzz/' + name + '_bundle_n360.csv, _n2.csv"')
        print("[case.inputs]")
        if charges:
            print(f"charges = {charges}")
        inputs(p, False)
        print("[case.expected]")
        for k, (a, b) in enumerate(zip(csm, mm)):
            print(f"avg_max_kv_cm_{k:02} = {{ value = {a:.10g}, rel_tol = {tol} }}")
            print(f"markt_mengele_kv_cm_{k:02} = {{ value = {b:.10g}, rel_tol = {tol_mm} }}")
        print("potential_error = { value = 0, abs_tol = 1e-4 }")
        print()


if __name__ == "__main__":
    if sys.argv[1:2] == ["gradient"]:
        gradient(sys.argv[2:])
    else:
        fields(sys.argv[1:])
