# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.
"""Writes lightning.toml: the lightning shielding check's cases against
the classical constructions, computed here on their own (no OpenOHL code).

Shielding angle: the angle from the vertical through the shielding wire
to the far side of the phase, atan((|dx| + r) / (dz - r)).

Rolling sphere over level ground at 0 (the electrogeometric model; Young,
Clayton and Hileman 1963; Armstrong and Whitehead 1968): the sphere
resting on the ground keeps its centre at beta*r; touching the shielding
wire S its centre is x_S + sqrt(r^2 - (beta*r - z_S)^2) out from S, and it
reaches the phase P (its surface: radius r + r_P) while that centre lies
within r + r_P of P. r_max is the largest r at which it still does: where
the centre resting on the ground and touching S is exactly r + r_P from P,
found by halving. The phase must be outside the shielding wire (the
sphere comes in from that side) and lower than it.

Run: python3 -I benchmarks/electrical/reference_lightning.py
"""

import math
import os

HERE = os.path.dirname(os.path.abspath(__file__))


def centre_x(s, beta, r):
    """The centre of the sphere resting on the ground and touching S, on
    the outer (positive) side."""
    h = beta * r - s[1]
    if r * r < h * h:
        return None
    return s[0] + math.sqrt(r * r - h * h)


def reaches(s, p, rp, beta, r):
    c = centre_x(s, beta, r)
    if c is None:
        return False
    return math.hypot(c - p[0], beta * r - p[1]) < r + rp


def r_max(s, p, rp, beta):
    # Exposed below r_max, covered above: the scan from the top, then
    # halving.
    hi = 150.0
    lo = None
    r = hi
    while r > 2.0:
        nxt = r / 1.1
        if reaches(s, p, rp, beta, nxt):
            lo = nxt
            break
        hi = nxt
        r = nxt
    if lo is None:
        return 0.0
    while hi - lo > 1e-9:
        m = 0.5 * (lo + hi)
        if reaches(s, p, rp, beta, m):
            lo = m
        else:
            hi = m
    return lo


def angle(s, p, rp):
    return math.degrees(math.atan2(abs(p[0] - s[0]) + rp, s[1] - p[1] - rp))


CASES = []

# 1-3: one shielding wire, the phase outside it; beta 1 and 0.8; a bundle.
for k, (s, p, rp, beta, r) in enumerate(
    [
        ((0.0, 32.0), (5.0, 24.0), 0.0, 1.0, 30.0),
        ((0.0, 32.0), (5.0, 24.0), 0.0, 0.8, 30.0),
        ((0.0, 40.0), (6.0, 30.0), 0.25, 1.0, 45.0),
    ]
):
    rm = r_max(s, p, rp, beta)
    CASES.append(
        {
            "id": f"electrical-06{k}",
            "title": f"Rolling sphere: one shielding wire at {s[1]:.0f} m, a phase {p[0] - s[0]:.0f} m out and "
            f"{s[1] - p[1]:.0f} m below, ground factor {beta}: r_max and the verdict at r = {r:.0f} m",
            "inputs": {
                "method": '"sphere"',
                "shields": f"[[{s[0]}, {s[1]}]]",
                "phase": f"[{p[0]}, {p[1]}, {rp}]",
                "ground": "0.0",
                "striking_distance": f"{r}",
                "ground_factor": f"{beta}",
            },
            "expected": {
                "r_max_m": (rm, "abs_tol = 0.02"),
                "exposed": (1.0 if r < rm else 0.0, "abs_tol = 0.0"),
            },
        }
    )

# 4: two shielding wires, the outer phase beside the right one: the left
# one cannot help (the sphere comes from the right): the same r_max.
s, p = (6.0, 35.0), (9.0, 27.0)
CASES.append(
    {
        "id": "electrical-063",
        "title": "Rolling sphere: two shielding wires 12 m apart at 35 m, the outer phase 3 m outside the right one: r_max as with that wire alone, exposed at r = 40 m",
        "inputs": {
            "method": '"sphere"',
            "shields": "[[-6.0, 35.0], [6.0, 35.0]]",
            "phase": "[9.0, 27.0, 0.0]",
            "ground": "0.0",
            "striking_distance": "40.0",
        },
        "expected": {
            "r_max_m": (r_max(s, p, 0.0, 1.0), "abs_tol = 0.02"),
            "exposed": (1.0 if 40.0 < r_max(s, p, 0.0, 1.0) else 0.0, "abs_tol = 0.0"),
        },
    }
)

# 5-6: shielding angles, outer and inner.
CASES.append(
    {
        "id": "electrical-064",
        "title": "Shielding angle: the outer phase 3 m outside the right wire, 8 m below, radius 0.2 m: the angle it needs against 25° outer",
        "inputs": {
            "method": '"angle"',
            "shields": "[[-6.0, 35.0], [6.0, 35.0]]",
            "phase": "[9.0, 27.0, 0.2]",
            "ground": "0.0",
            "outer_angle": "25.0",
            "inner_angle": "45.0",
        },
        "expected": {
            "needed_deg": (angle((6.0, 35.0), (9.0, 27.0), 0.2), "abs_tol = 1e-9"),
            "exposed": (0.0, "abs_tol = 0.0"),
        },
    }
)
CASES.append(
    {
        "id": "electrical-065",
        "title": "Shielding angle: the middle phase between two wires 12 m apart, 6 m below: covered by the inner angle 45°, not by 30°",
        "inputs": {
            "method": '"angle"',
            "shields": "[[-6.0, 35.0], [6.0, 35.0]]",
            "phase": "[0.0, 29.0, 0.0]",
            "ground": "0.0",
            "outer_angle": "30.0",
            "inner_angle": "45.0",
        },
        "expected": {
            "needed_deg": (angle((6.0, 35.0), (0.0, 29.0), 0.0), "abs_tol = 1e-9"),
            "exposed": (0.0, "abs_tol = 0.0"),
        },
    }
)

with open(os.path.join(HERE, "lightning.toml"), "w") as f:
    f.write(
        "# The lightning shielding check against the classical constructions,\n"
        "# computed on their own by reference_lightning.py (no OpenOHL code).\n"
        "# Rolling sphere over level ground: r_max, the largest striking\n"
        "# distance still reaching the phase, from the sphere resting on the\n"
        "# ground and touching the shielding wire (electrogeometric model;\n"
        "# Young, Clayton and Hileman 1963; Armstrong and Whitehead 1968);\n"
        "# to 2 cm (the program halves to 1 cm). Shielding angle: exact.\n"
    )
    for c in CASES:
        f.write(f'\n[[case]]\nid = "{c["id"]}"\ntitle = "{c["title"]}"\n')
        f.write('calculation = "electrical.lightning_cut"\n')
        f.write('reference = "Classical construction, computed by benchmarks/electrical/reference_lightning.py"\n')
        f.write("[case.inputs]\n")
        for k, v in c["inputs"].items():
            f.write(f"{k} = {v}\n")
        f.write("[case.expected]\n")
        for k, (v, tol) in c["expected"].items():
            f.write(f"{k} = {{ value = {v!r}, {tol} }}\n")
print("written", len(CASES), "cases")
