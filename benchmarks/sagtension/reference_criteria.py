# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""Independent reference values for benchmarks/sagtension/criteria.toml.

Governing stringing tension: the largest horizontal tension at the reference
state (15 C, bare) such that every criterion holds. Uses the independent
change-of-state model of reference_change_of_state.py (numerical integrals,
secant root) with an outer bisection of fixed length (exit clause).

Run: python3 benchmarks/sagtension/reference_criteria.py  (standard library only)
"""

import math
import reference_change_of_state as cos

RTS = 138_800.0  # N
S, H_RISE = 300.0, 0.0
REF = (15.97, 15.0)  # load N/m, temperature C
CRITERIA = [
    # label, (load, temperature), kind, ratio of RTS
    ("everyday 20 % RTS horizontal", (15.97, 15.0), "horizontal", 0.20),
    ("ice + wind 30 % RTS support", (32.0, -20.0), "support", 0.30),
]


def state_after(h_ref, load, temperature):
    L0 = cos.unstressed_length(S, H_RISE, h_ref, REF[0])
    # The own weight REF[0] conserved; the rest of the load ice.
    return cos.solve(S, H_RISE, L0, REF[1], load, temperature, own=REF[0])


def measure(result, kind):
    if kind == "horizontal":
        return result["horizontal_tension"]
    return max(result["tension_left"], result["tension_right"])


def max_reference(criterion):
    _, (load, temperature), kind, ratio = criterion
    lo, hi = 1_000.0, 100_000.0
    for _ in range(60):  # fixed iteration count
        mid = 0.5 * (lo + hi)
        if measure(state_after(mid, load, temperature), kind) > ratio * RTS:
            hi = mid
        else:
            lo = mid
    return lo


if __name__ == "__main__":
    cos.length_and_tension_integral.__defaults__ = (2_000,)  # faster, still ~1e-12
    limits = [max_reference(c) for c in CRITERIA]
    h_ref = min(limits)
    print(f"reference tension  {h_ref:.10g}")
    print(f"governing          {CRITERIA[limits.index(h_ref)][0]}")
    for label, (load, temperature), kind, ratio in CRITERIA:
        value = measure(state_after(h_ref, load, temperature), kind)
        print(f"  {label:32s} utilisation {value / (ratio * RTS):.10g}")
