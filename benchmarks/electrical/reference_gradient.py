#!/usr/bin/env python3
# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.
"""Exact surface gradients of smooth cylinders (benchmarks/electrical/gradient.toml).

A cylinder of radius r whose axis is h above a perfectly conducting plane,
at V to earth: its surface is an equipotential of one line charge q at
d = sqrt(h^2 - r^2) above the plane and its image -q at -d (the circle of
Apollonius; bipolar coordinates), with

    q = 2 pi eps0 V / ln((h + d) / r)      (C = 2 pi eps0 / arccosh(h/r))

and the field at a surface point x, distances a and b to the charge and
its image,

    E = q / (2 pi eps0) * 2 d / (a b),

largest at the bottom (a = d - h + r, b = h - r + d):
E_max = V d / (r (h - r) ln((h + d)/r)); smallest at the top:
E_min = V d / (r (h + r) ln((h + d)/r)).

Two cylinders at +V and -V in free space, axes 2h apart: the plane midway
is at zero, so each is the cylinder over the plane above (the same
values). Exact in closed form — an independent check of the program's
charge simulation (`electrical.cylinder_gradient`). Run:

    python3 -I benchmarks/electrical/reference_gradient.py

which prints the expected values (kV/cm, rms when V is rms).
"""

import math

CASES = [
    # id, title, height m, radius m, kV to earth, opposite
    ("electrical-069", "A 31.8 mm wire 20 m over the ground at 231 kV (a 400 kV phase)", 20.0, 0.0159, 231.0, 0),
    ("electrical-070", "A 0.5 m cylinder 2 m over the ground at 100 kV (strongly non-uniform)", 2.0, 0.5, 100.0, 0),
    ("electrical-071", "Two 0.1 m cylinders 0.6 m apart at +10 and -10 kV in free space", 0.3, 0.1, 10.0, 1),
]


def exact(h, r, v_kv):
    d = math.sqrt(h * h - r * r)
    l = math.log((h + d) / r)
    v = v_kv * 1e3
    return v * d / (r * (h - r) * l) / 1e5, v * d / (r * (h + r) * l) / 1e5


def main():
    for cid, title, h, r, kv, opp in CASES:
        emax, emin = exact(h, r, kv)
        print(cid, title)
        print(f"  e_max_kv_cm = {emax:.10g}")
        print(f"  e_min_kv_cm = {emin:.10g}")


if __name__ == "__main__":
    main()
