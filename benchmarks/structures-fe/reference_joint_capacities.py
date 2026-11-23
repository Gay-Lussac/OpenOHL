# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.
"""Expected values of joint-capacities.toml, by hand: the connection
capacities of invented joints worked out line by line from the methods the
theory pages give (docs/theory/joint-groups.md, joint-plates.md, asce10.md,
sia263.md) and the values the bundled plugins take (plugins/codes/asce10,
plugins/codes/sia263: their constants are named where they come in). Each
usage is demand / resistance for one load case of strength factor 1; a
distance check's usage is the minimum over the distance kept. Run with
`python3 -I`; prints one line per case, the value written in the case.
"""

from math import cos, pi, radians, sqrt, tan

MM = 1e-3
KN = 1e3
MPA = 1e6

# ASCE 10-97 as the asce10 plugin takes it.
BOLT_SHEAR = 0.62  # 4.3.2: design shear stress / f_u of the bolt
ROOT_AREA = 0.6  # root area / gross area without a given capacity
PUNCHED = 1.6 * MM  # 3.10.1, 3.10.2: a punched hole 1/16 in. larger
E_ASCE = 200e3 * MPA  # Appendix A
# SIA 263:2013 as the sia263 plugin takes it (its parameter file's values).
GAMMA_M2 = 1.25
F_UE = 510 * MPA  # 6.3.2.2, the filler metal's tensile strength


def gross(d):
    return pi * d * d / 4


cases = {}

# --- joint groups (each bolt with its force), ASCE 10-97 ---------------
# A: bolt shear, one plane (4.3.2): F_v = 0.62 f_ub on the root area (0.6 of
# the gross); M16 of f_ub 800 MPa, 30 kN. The plate is thick (bearing far
# off), no edge sent (no distance checked).
d = 16 * MM
rv1 = BOLT_SHEAR * 800 * MPA * ROOT_AREA * gross(d)
cases["joints-001"] = 30 * KN / rv1
# B: the same bolt in two shear planes, 50 kN.
cases["joints-002"] = 50 * KN / (2 * rv1)
# C: bearing (4.4): 1.5 F_u d t on the thinner ply, F_u the lower of the
# ply's and the bolt's (the bolt's not given: the ply's), 4 mm of 360 MPa,
# 20 kN; the bolt's shear capacity given as 100 kN a plane.
rb = 1.5 * 360 * MPA * d * 4 * MM
cases["joints-003"] = 20 * KN / rb
# D: the end distance (4.5.1) along the bolt's force: at least the largest
# of 1.2 P / (F_u t), 1.3 d and t + d/2 — 40 kN in a 6 mm ply of 360 MPa:
# 22.22 mm, 20 mm kept: 1.111 (a distance governs when not kept).
p, fu, t = 40 * KN, 360 * MPA, 6 * MM
end_min = max(1.2 * p / (fu * t), 1.3 * d, t + d / 2)
cases["joints-004"] = end_min / (20 * MM)
# E: bolts in tension with prying (4.3.3; the T-stub of Struik and de Back,
# Kulak, Fisher & Struik Eqs. 17.8-17.12 with alpha = delta = 1): M16 in an
# 18 mm hole, pulled by 20 kN through an angle's flange 8 mm thick (f_y 355
# MPa), a = 30 mm to the toe, b = 25 mm to the other leg's face, w = 50 mm
# taken: a <= 1.25 b; a' = a + d/2 = 38 mm, b' = b - d/2 = 17 mm; Q = T b' /
# (2 a') = 4.474 kN; the bolt's tension capacity (F_u A_s) given as 100 kN:
# F_t A_s = 0.6 x 100 kN (4.3.3 (b)); the flange's bending T b' <= (1 +
# delta) w t^2 f_y / 4, delta = 1 - d0/w = 0.64 -> 340 N m of 465.8 N m:
# the flange governs.
T, d0, a, b, w, tf, fy = 20 * KN, 18 * MM, 30 * MM, 25 * MM, 50 * MM, 8 * MM, 355 * MPA
a = min(a, 1.25 * b)
a1, b1 = a + d / 2, b - d / 2
q = T * b1 / (2 * a1)
bolt_e = (T + q) / (0.6 * 100 * KN)
delta = 1 - d0 / w
flange_e = T * b1 / ((1 + delta) * w * tf**2 * fy / 4)
cases["joints-005"] = max(bolt_e, flange_e)
# F: the same with a 16 mm flange (four times as strong in bending): the
# bolt's tension with its prying governs, (T + Q) / (0.6 x 100 kN).
flange_f = T * b1 / ((1 + delta) * w * (16 * MM) ** 2 * fy / 4)
cases["joints-006"] = max(bolt_e, flange_f)

# --- plate parts (a member's bolts in a plate; welds) ------------------
# G: block shear of a plate (ASCE 10-97 Eq. 3.10-1 applied to the plate's
# block round the bolts): 3 M16 in one line, holes 17.5 mm (+1.6 mm
# punched: d_h 19.1 mm), pitch 40 mm, end 22 mm, plate 10 mm of f_y 235,
# f_u 360 MPa; A_v = 2 t (e + (n-1) p - (n - 1/2) d_h), A_t = t (span -
# (lines-1) d_h) = 0 (one line); 120 kN. Bearing 3 x 1.5 x 360 x 16 x 10 =
# 259.2 kN, the net section across 200 mm far off; the end distance 22 mm
# kept (4.5.1: at least 1.3 d = 20.8 mm with 40 kN a bolt).
t, dh = 10 * MM, 17.5 * MM + PUNCHED
av = 2 * t * (22 * MM + 2 * 40 * MM - 2.5 * dh)
cases["joints-007"] = 120 * KN / (0.60 * av * 360 * MPA)
# H: a gusset plate in compression as a strut of Whitmore's width (an
# assumption on the safe side, no rule in either code): 2 lines 40 mm apart,
# 3 bolts a line at 50 mm: w = 40 + 2 x 100 tan 30 deg = 155.47 mm (below
# the plate's 300 mm); t 8 mm, r = t / sqrt 12, free length 150 mm, K = 2:
# KL/r 129.9 > C_c = pi sqrt(2 E / F_y) = 105.5 (F_y 355 MPa, E 200 GPa):
# F_a = pi^2 E / (KL/r)^2 (ASCE 10-97 3.6) = 116.97 MPa; 100 kN in
# compression.
wb = 40 * MM + 2 * 100 * MM * tan(radians(30))
t = 8 * MM
klr = 2 * 150 * MM / (t / sqrt(12))
cc = pi * sqrt(2 * E_ASCE / (355 * MPA))
fa = pi**2 * E_ASCE / klr**2 if klr > cc else (1 - 0.5 * (klr / cc) ** 2) * 355 * MPA
cases["joints-008"] = 100 * KN / (fa * wb * t)
# I: a fillet weld with its moment about its middle (SIA 263 6.3.2.2, (87)
# and (88); the force per unit length at the weld's most loaded end, F/L +
# 6 M / L^2, elastic): a = 4 mm, L = 100 mm, plates at 90 deg of f_y 355
# MPa, F = 50 kN, M = 0.5 kN m: 800 kN/m, 80 kN on the length; (87) a L 0.6
# f_uE / gamma_M2 = 97.92 kN, (88) s L 0.8 f_y / gamma_M2 with s = a / cos
# 45 deg = 128.5 kN: (87) governs.
a, L = 4 * MM, 100 * MM
dem = 50 * KN + 6 * 500 / L
r87 = a * L * 0.6 * F_UE / GAMMA_M2
s = a / cos(radians(45))
r88 = s * L * 0.8 * 355 * MPA / GAMMA_M2
cases["joints-009"] = max(dem / r87, dem / r88)
# J: the same weld bent about its seam (a moment T = 50 N m about the
# seam's line, taken by the section of the throat, resp. the contact width,
# bent across: 6 T / (L w)), plates of f_y 235 MPa, F = 40 kN: (87) 400 +
# 750 = 1150 kN/m -> 115 kN of 97.92 kN; (88) 400 + 530.3 kN/m -> 93.03 kN
# of 85.08 kN: (87) governs, 1.174 (over 100 %: the case shows a failing
# weld is found failing). NOT ON THE SAFE SIDE is said for a weld bent so.
dem87 = (40 * KN / L + 6 * 50 / (L * a)) * L
dem88 = (40 * KN / L + 6 * 50 / (L * s)) * L
r88b = s * L * 0.8 * 235 * MPA / GAMMA_M2
cases["joints-010"] = max(dem87 / r87, dem88 / r88b)

# --- SIA 263 bolt shear --------------------------------------------------
# K: bolt shear by SIA 263 Table 16 (73), grade 8.8: 0.6 f_ub A_s / gamma_M2
# with A_s taken as 0.6 of the gross area (the plugin's, on the safe side
# against the tensile stress area): M16, f_ub 800 MPa, 30 kN.
rv_sia = 0.6 * 800 * MPA * ROOT_AREA * gross(16 * MM) / GAMMA_M2
cases["joints-011"] = 30 * KN / rv_sia
# L: the same bolt with its grade not given: (74)'s lower factor 0.5.
cases["joints-012"] = 30 * KN / (0.5 * 800 * MPA * ROOT_AREA * gross(16 * MM) / GAMMA_M2)

# M: the edge distance square to the bolt's force in a member's leg (ASCE
# 10-97 4.5.3, Eq. 4.5-6, a rolled edge): at least 0.85 x the end distance
# of 4.5.1 — D's 22.22 mm: 18.89 mm; 15 mm kept: 1.259.
cases["joints-013"] = 0.85 * end_min / (15 * MM)
# N: G's plate torn out towards its side edge 25 mm from the bolt line
# (Eq. 3.10-1, one shear plane along the line, the tension plane from it
# to the side edge): A_v = t (e + (n-1) p - (n - 1/2) d_h), A_t = t (edge +
# span - (lines - 1/2) d_h); 0.6 A_v f_u + A_t f_y = 153.49 kN; 120 kN.
t = 10 * MM
av1 = t * (22 * MM + 2 * 40 * MM - 2.5 * dh)
at1 = t * (25 * MM + 0 - 0.5 * dh)
cases["joints-014"] = 120 * KN / (0.60 * av1 * 360 * MPA + at1 * 235 * MPA)

for k, v in cases.items():
    print(f"{k} {v!r}")
