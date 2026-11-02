# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.
"""Expected values of built-up-sections.toml, by hand: each angle as its
two plates, moved by the parallel-axis theorem (no root radius). The
library computes the outline's integrals instead (polygons and fillets,
the plastic moduli by quadrature). Run with `python3 -I`.

The angle (legs h, thickness t) is invented: 71 x 6.3 mm, the gap 9.5 mm.
"""

from math import sqrt

H, T, GAP = 0.071, 0.0063, 0.0095


def angle(h, t):
    """Area, centroid e from the heel, own I about a leg-parallel axis,
    own product (legs along +y and +z)."""
    plates = [(h * t, h / 2, t / 2, h, t), (t * (h - t), t / 2, (h + t) / 2, t, h - t)]
    a = sum(p[0] for p in plates)
    e = sum(p[0] * p[1] for p in plates) / a
    i = sum(w * d**3 / 12 + ar * (zc - e) ** 2 for ar, _, zc, w, d in plates)
    iyz = sum(ar * (yc - e) * (zc - e) for ar, yc, zc, _, _ in plates)
    return a, e, i, iyz


def plastic_single(h, t):
    """A single equal angle's plastic modulus about a leg-parallel axis
    (the axis in the outstanding leg: h t >= A / 2)."""
    a = t * (2 * h - t)
    zp = a / (2 * h)
    assert zp < t
    return h * zp**2 / 2 + h * (t - zp) ** 2 / 2 + t * (h - t) * ((h + t) / 2 - zp)


def main():
    h, t, g = H, T, GAP
    a1, e, i1, i1yz = angle(h, t)
    it1 = (2 * h - t) * t**3 / 3
    d = g / 2 + e
    part_rv = sqrt((i1 + i1yz) / a1)
    print("one angle: area", a1, "part_gyration_v", part_rv)

    # Back to back: legs h together along z, g apart.
    iy, iz = 2 * i1, 2 * (i1 + a1 * d * d)
    print("double_angle")
    print(" area", 2 * a1, "centroid", h + g / 2, e)
    print(" iy", iy, "iz", iz, "it", 2 * it1)
    print(" wel_y", iy / (h - e), "wel_z", iz / (h + g / 2))
    print(" wpl_y", 2 * plastic_single(h, t), "wpl_z", 2 * a1 * d)
    print(" wind_width (along y)", h)

    # Star: point symmetric, principal axes at 45 degrees.
    i, iyz = 2 * (i1 + a1 * d * d), 2 * (i1yz + a1 * d * d)
    print("star_angle")
    print(" area", 2 * a1, "centroid", h + g / 2, h + g / 2)
    print(" iy = iz", i, "iyz", iyz, "iu", i + iyz, "iv", i - iyz)
    print(" it", 2 * it1, "wel_y", i / (h + g / 2), "wpl_y", 2 * a1 * d)
    print(" wind_width", 2 * h + g)

    # Cross: four angles, the products cancel.
    i = 4 * (i1 + a1 * d * d)
    print("cross_angle")
    print(" area", 4 * a1, "centroid", h + g / 2, h + g / 2)
    print(" iy = iz = iv", i, "it", 4 * it1)
    print(" wel_y", i / (h + g / 2), "wpl_y", 4 * a1 * d)
    print(" wind_width", 2 * h + g)


if __name__ == "__main__":
    main()
