// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Cross-sections as sums of signed parts (convex polygons, circles, quarter
//! discs) and their integrals: area, first and second moments, the width
//! of a strip (for plastic moduli) and the outline's extent in a direction
//! (for elastic moduli and wind).

use std::f64::consts::PI;

/// One part of a section, in the section's axes (y, z), m.
#[derive(Debug, Clone)]
pub(crate) enum Part {
    /// A convex polygon, vertices in either order.
    Polygon(Vec<[f64; 2]>),
    /// A full circle.
    Circle { c: [f64; 2], r: f64 },
    /// The quarter of a disc centred at `c` on the side `s` (±1, ±1).
    QuarterDisc { c: [f64; 2], r: f64, s: [f64; 2] },
}

/// A part added (`sign` 1) or cut away (`sign` -1).
#[derive(Debug, Clone)]
pub(crate) struct Signed {
    pub part: Part,
    pub sign: f64,
}

/// ∫dA, ∫y, ∫z, ∫y², ∫z², ∫yz over a part, about the origin.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Moments {
    pub a: f64,
    pub sy: f64,
    pub sz: f64,
    pub yy: f64,
    pub zz: f64,
    pub yz: f64,
}

impl Part {
    fn moments(&self) -> Moments {
        match self {
            Part::Polygon(p) => {
                let mut m = Moments::default();
                for i in 0..p.len() {
                    let [y0, z0] = p[i];
                    let [y1, z1] = p[(i + 1) % p.len()];
                    let c = y0 * z1 - y1 * z0;
                    m.a += c / 2.0;
                    m.sy += (y0 + y1) * c / 6.0;
                    m.sz += (z0 + z1) * c / 6.0;
                    m.yy += (y0 * y0 + y0 * y1 + y1 * y1) * c / 12.0;
                    m.zz += (z0 * z0 + z0 * z1 + z1 * z1) * c / 12.0;
                    m.yz += (y0 * z1 + 2.0 * y0 * z0 + 2.0 * y1 * z1 + y1 * z0) * c / 24.0;
                }
                // Clockwise vertices give every integral negative.
                let s = m.a.signum();
                Moments {
                    a: m.a * s,
                    sy: m.sy * s,
                    sz: m.sz * s,
                    yy: m.yy * s,
                    zz: m.zz * s,
                    yz: m.yz * s,
                }
            }
            Part::Circle { c, r } => {
                let a = PI * r * r;
                let own = PI * r.powi(4) / 4.0;
                Moments {
                    a,
                    sy: a * c[0],
                    sz: a * c[1],
                    yy: own + a * c[0] * c[0],
                    zz: own + a * c[1] * c[1],
                    yz: a * c[0] * c[1],
                }
            }
            Part::QuarterDisc { c, r, s } => {
                // In u = s_y (y - c_y), v = s_z (z - c_z): ∫u = ∫v = r³/3,
                // ∫u² = ∫v² = π r⁴/16, ∫uv = r⁴/8.
                let a = PI * r * r / 4.0;
                let first = r.powi(3) / 3.0;
                let second = PI * r.powi(4) / 16.0;
                let product = r.powi(4) / 8.0;
                Moments {
                    a,
                    sy: a * c[0] + s[0] * first,
                    sz: a * c[1] + s[1] * first,
                    yy: a * c[0] * c[0] + 2.0 * c[0] * s[0] * first + second,
                    zz: a * c[1] * c[1] + 2.0 * c[1] * s[1] * first + second,
                    yz: a * c[0] * c[1]
                        + (c[0] * s[1] + c[1] * s[0]) * first
                        + s[0] * s[1] * product,
                }
            }
        }
    }

    /// Width along the other axis of the strip at coordinate `x` of axis
    /// `k` (0: y, 1: z).
    fn chord(&self, k: usize, x: f64) -> f64 {
        let o = 1 - k;
        match self {
            Part::Polygon(p) => {
                let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                for i in 0..p.len() {
                    let (a, b) = (p[i], p[(i + 1) % p.len()]);
                    if a[k] == b[k] || (a[k] - x) * (b[k] - x) > 0.0 {
                        continue;
                    }
                    let w = a[o] + (x - a[k]) / (b[k] - a[k]) * (b[o] - a[o]);
                    lo = lo.min(w);
                    hi = hi.max(w);
                }
                (hi - lo).max(0.0)
            }
            Part::Circle { c, r } => {
                let d = x - c[k];
                if d.abs() < *r {
                    2.0 * (r * r - d * d).sqrt()
                } else {
                    0.0
                }
            }
            Part::QuarterDisc { c, r, s } => {
                let u = (x - c[k]) * s[k];
                if (0.0..*r).contains(&u) {
                    (r * r - u * u).sqrt()
                } else {
                    0.0
                }
            }
        }
    }

    /// Coordinates of axis `k` where the strip width has a kink or an end.
    fn breaks(&self, k: usize, out: &mut Vec<f64>) {
        match self {
            Part::Polygon(p) => out.extend(p.iter().map(|v| v[k])),
            Part::Circle { c, r } => out.extend([c[k] - r, c[k], c[k] + r]),
            Part::QuarterDisc { c, r, s } => out.extend([c[k], c[k] + s[k] * r]),
        }
    }

    /// The largest `p · n` over the part (n a unit vector).
    fn support(&self, n: [f64; 2]) -> f64 {
        let dot = |p: [f64; 2]| p[0] * n[0] + p[1] * n[1];
        match self {
            Part::Polygon(p) => p.iter().map(|&v| dot(v)).fold(f64::NEG_INFINITY, f64::max),
            Part::Circle { c, r } => dot(*c) + r,
            Part::QuarterDisc { c, r, s } => {
                let mut best = dot(*c)
                    .max(dot([c[0] + s[0] * r, c[1]]))
                    .max(dot([c[0], c[1] + s[1] * r]));
                if n[0] * s[0] >= 0.0 && n[1] * s[1] >= 0.0 {
                    best = best.max(dot(*c) + r);
                }
                best
            }
        }
    }
}

/// Five-point Gauss–Legendre nodes and weights on [-1, 1].
const GAUSS: [(f64, f64); 5] = [
    (0.0, 0.568_888_888_888_888_9),
    (-0.538_469_310_105_683, 0.478_628_670_499_366_5),
    (0.538_469_310_105_683, 0.478_628_670_499_366_5),
    (-0.906_179_845_938_664, 0.236_926_885_056_189_1),
    (0.906_179_845_938_664, 0.236_926_885_056_189_1),
];

/// Sub-intervals per piece between two breaks.
const PIECES: usize = 16;

/// A section as signed parts.
#[derive(Debug, Clone, Default)]
pub(crate) struct Outline {
    pub parts: Vec<Signed>,
}

impl Outline {
    pub fn add(&mut self, part: Part) {
        self.parts.push(Signed { part, sign: 1.0 });
    }

    pub fn cut(&mut self, part: Part) {
        self.parts.push(Signed { part, sign: -1.0 });
    }

    /// An axis-aligned rectangle from (y0, z0) to (y1, z1), added.
    pub fn rect(&mut self, y0: f64, z0: f64, y1: f64, z1: f64) {
        self.add(Part::Polygon(vec![[y0, z0], [y1, z0], [y1, z1], [y0, z1]]));
    }

    /// The fillet of radius `r` in the inner corner `k` of two plates, the
    /// fillet lying towards `s` (±1, ±1) from the corner.
    pub fn fillet(&mut self, k: [f64; 2], r: f64, s: [f64; 2]) {
        if r <= 0.0 {
            return;
        }
        let far = [k[0] + s[0] * r, k[1] + s[1] * r];
        self.rect(
            k[0].min(far[0]),
            k[1].min(far[1]),
            k[0].max(far[0]),
            k[1].max(far[1]),
        );
        self.cut(Part::QuarterDisc {
            c: far,
            r,
            s: [-s[0], -s[1]],
        });
    }

    pub fn moments(&self) -> Moments {
        let mut m = Moments::default();
        for p in &self.parts {
            let q = p.part.moments();
            m.a += p.sign * q.a;
            m.sy += p.sign * q.sy;
            m.sz += p.sign * q.sz;
            m.yy += p.sign * q.yy;
            m.zz += p.sign * q.zz;
            m.yz += p.sign * q.yz;
        }
        m
    }

    /// The largest `p · n` over the outline: the parts added only (a part
    /// cut away lies inside them).
    pub fn support(&self, n: [f64; 2]) -> f64 {
        self.parts
            .iter()
            .filter(|p| p.sign > 0.0)
            .map(|p| p.part.support(n))
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// The outline's width across the direction `d` (a unit vector): its
    /// extent along the normal of `d`.
    pub fn width_across(&self, d: [f64; 2]) -> f64 {
        let n = [-d[1], d[0]];
        self.support(n) + self.support([-n[0], -n[1]])
    }

    fn chord(&self, k: usize, x: f64) -> f64 {
        self.parts.iter().map(|p| p.sign * p.part.chord(k, x)).sum()
    }

    /// ∫ f(x) · chord(x) dx along axis `k` from `from` to `to`, piece by
    /// piece between the breaks (f smooth between them).
    fn integrate(
        &self,
        k: usize,
        from: f64,
        to: f64,
        extra: &[f64],
        f: impl Fn(f64) -> f64,
    ) -> f64 {
        let mut breaks = extra.to_vec();
        for p in &self.parts {
            p.part.breaks(k, &mut breaks);
        }
        breaks.retain(|x| *x > from && *x < to);
        breaks.extend([from, to]);
        breaks.sort_by(f64::total_cmp);
        breaks.dedup();
        let mut sum = 0.0;
        for w in breaks.windows(2) {
            // x = a + (b − a)(1 − cos πτ)/2: smooth where a circle's chord
            // has its square root at the ends.
            let len = w[1] - w[0];
            let h = 1.0 / PIECES as f64;
            for i in 0..PIECES {
                let mid = (i as f64 + 0.5) * h;
                for (x, wt) in GAUSS {
                    let tau = mid + x * h / 2.0;
                    let at = w[0] + len * (1.0 - (PI * tau).cos()) / 2.0;
                    let dx = len * PI / 2.0 * (PI * tau).sin();
                    sum += wt * h / 2.0 * dx * f(at) * self.chord(k, at);
                }
            }
        }
        sum
    }

    /// Plastic modulus for bending with stresses varying along axis `k`:
    /// ∫ |x − p| dA about the line `p` halving the area.
    pub fn plastic_modulus(&self, k: usize) -> f64 {
        let mut e = [0.0; 2];
        e[k] = 1.0;
        let lo = -self.support([-e[0], -e[1]]);
        let hi = self.support(e);
        let half = self.moments().a / 2.0;
        let (mut a, mut b) = (lo, hi);
        for _ in 0..80 {
            let mid = (a + b) / 2.0;
            if self.integrate(k, lo, mid, &[], |_| 1.0) < half {
                a = mid;
            } else {
                b = mid;
            }
        }
        let p = (a + b) / 2.0;
        self.integrate(k, lo, hi, &[p], |x| (x - p).abs())
    }
}

/// The vertices of a regular polygon of `n` sides and inradius `a` about
/// `c`, a side at the bottom (normal to z).
pub(crate) fn regular_polygon(n: u32, a: f64, c: [f64; 2]) -> Vec<[f64; 2]> {
    let n = n as f64;
    let r = a / (PI / n).cos();
    (0..n as u32)
        .map(|k| {
            let t = -PI / 2.0 + PI / n + 2.0 * PI * k as f64 / n;
            [c[0] + r * t.cos(), c[1] + r * t.sin()]
        })
        .collect()
}
