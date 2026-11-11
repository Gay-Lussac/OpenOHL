// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Section library (`.ohlsec`, TOML): standard profiles that structures
//! refer to by id, shared by a whole project, with their materials; the
//! cross-section properties computed from each profile's dimensions, and
//! its projected width for wind.
//!
//! ```toml
//! format = "ohl-sections"
//! version = 1
//! name = "Example sections (invented)"
//! default_material = "steel-a"     # for sections naming none
//!
//! [[material]]
//! id = "steel-a"
//! elasticity = "210 GPa"           # E
//! shear_modulus = "81 GPa"         # G (default E / 2.6)
//! density = "7850 kg/m3"           # default 0
//! yield = "355 MPa"                # fy, optional
//! ultimate = "490 MPa"             # fu, optional
//! partial_factors = { gamma_m0 = 1.0, gamma_m1 = 1.1 }   # optional, by name
//!
//! [[section]]
//! id = "L-61x7"
//! name = "Angle 61 × 7 (invented)"
//! shape = "equal_angle"
//! h = "61 mm"
//! t = "7 mm"
//! r = "8 mm"                       # root radius (default 0)
//! # optional, replacing the computed values: area, iy, iz, iu, iv, it,
//! # wel_y, wel_z, wpl_y, wpl_z, gyration_y, gyration_z, gyration_u,
//! # gyration_v, shear_area_y, shear_area_z
//! ```
//!
//! Section axes: y along the width `b`, z along the height `h`; `iy` is
//! about y (the strong axis of an I section). Shapes and their dimensions:
//!
//! | `shape` | dimensions |
//! |---|---|
//! | `equal_angle` | `h`, `t`, `r` |
//! | `unequal_angle` | `h`, `b`, `t`, `r` |
//! | `channel` | `h`, `b`, `tw`, `tf`, `r` |
//! | `i_section` (I and H) | `h`, `b`, `tw`, `tf`, `r` |
//! | `rectangular_hollow` | `h`, `b`, `t` |
//! | `circular_hollow` (tube) | `d`, `t` |
//! | `round_bar` | `d` |
//! | `polygonal_hollow` (pole) | `sides`, `d` (across flats), `t` |
//! | `general` | `h`, `b` (outline box); `area`, `iy`, `iz` given |
//! | `double_angle` (two back to back) | `h`, `b`, `t`, `r`, `gap`, `back` |
//! | `star_angle` (two in a star) | `h`, `b`, `t`, `r`, `gap` |
//! | `cross_angle` (four in a cross) | `h`, `t`, `r`, `gap` |
//! | `double_channel` (two back to back) | `h`, `b`, `tw`, `tf`, `r`, `gap` |
//!
//! `r` (root radius) is optional, default 0. The built-up shapes are made
//! of angles of legs `h` and `b` (`b` left out: equal legs) and thickness
//! `t`; `gap` (required, 0 or above) is the space between the angles'
//! backs — a filler or gusset plate's thickness, 0 when they touch; `back`
//! (`"long"` or `"short"`, required for unequal legs) says which legs of a
//! `double_angle` lie back to back. A `double_channel` (2026-11-04) is two
//! channels of the `channel` dimensions with their webs back to back,
//! `gap` apart (a gusset plate's thickness), their flanges pointing away
//! from each other.
//!
//! Torsion and warping (for torsional and torsional-flexural buckling):
//! the shear centre and the warping constant are computed from the plates'
//! midlines (thin-walled theory, the fillets left out); they cannot be
//! given in the file. The shape's symmetry ([`Shape::symmetry`]) says which
//! buckling modes a code has to check.
//!
//! An optional `[wind]` table holds wind factors for structures given as
//! data — a drag factor per shape, a lattice face's drag and shielding
//! factors by solidity ratio ([`wind`]); none are shipped.

mod geometry;
pub mod wind;

use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::path::Path;

use ohl_core::units::{Angle, Area, Density, Length, SecondMoment, SectionModulus, Stress};
use serde::Deserialize;

use geometry::{Outline, Part};
pub use wind::{Curve, Shielding, StructureWind};

/// Value of the `format` header of section library files.
pub const SECTION_FILE_FORMAT: &str = "ohl-sections";
/// Latest section library format version.
pub const SECTION_FILE_VERSION: u32 = 1;
/// File extension of section libraries.
pub const SECTION_FILE_EXTENSION: &str = "ohlsec";

/// Shear modulus over Young's modulus when a material gives none
/// (ν = 0.3, as structure models).
const DEFAULT_G_OVER_E: f64 = 1.0 / 2.6;

/// Shear area of a round bar over its area (Timoshenko's 0.9).
const ROUND_BAR_SHEAR: f64 = 0.9;

/// The profile and its dimensions, m.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Angle with equal legs `h`, thickness `t`, root radius `r`.
    EqualAngle {
        /// Leg length.
        h: f64,
        /// Thickness.
        t: f64,
        /// Root radius.
        r: f64,
    },
    /// Angle with legs `h` (along z) and `b` (along y).
    UnequalAngle {
        /// Leg along z.
        h: f64,
        /// Leg along y.
        b: f64,
        /// Thickness.
        t: f64,
        /// Root radius.
        r: f64,
    },
    /// Channel: web along z at y = 0, flanges towards +y.
    Channel {
        /// Height.
        h: f64,
        /// Flange width.
        b: f64,
        /// Web thickness.
        tw: f64,
        /// Flange thickness.
        tf: f64,
        /// Root radius.
        r: f64,
    },
    /// I or H section, parallel flanges.
    ISection {
        /// Height.
        h: f64,
        /// Flange width.
        b: f64,
        /// Web thickness.
        tw: f64,
        /// Flange thickness.
        tf: f64,
        /// Root radius.
        r: f64,
    },
    /// Rectangular hollow section, sharp corners.
    RectangularHollow {
        /// Height.
        h: f64,
        /// Width.
        b: f64,
        /// Wall.
        t: f64,
    },
    /// Circular hollow section (tube).
    CircularHollow {
        /// Outside diameter.
        d: f64,
        /// Wall.
        t: f64,
    },
    /// Solid round bar.
    RoundBar {
        /// Diameter.
        d: f64,
    },
    /// Regular polygon tube (a pole's shaft), a side at the bottom.
    PolygonalHollow {
        /// Number of sides.
        sides: u32,
        /// Outside width across flats.
        d: f64,
        /// Wall.
        t: f64,
    },
    /// Any section: only its outline box; the properties are given.
    General {
        /// Height.
        h: f64,
        /// Width.
        b: f64,
    },
    /// Two angles back to back: the legs `h` side by side along z, `gap`
    /// apart along y, the legs `b` along y pointing away from each other
    /// at the bottom (an upside-down T).
    DoubleAngle {
        /// The legs back to back, along z.
        h: f64,
        /// The outstanding legs, along y.
        b: f64,
        /// Thickness.
        t: f64,
        /// Root radius.
        r: f64,
        /// Between the two backs.
        gap: f64,
    },
    /// Two angles heel to heel in a star, the second turned 180° about the
    /// first: legs `h` along +z and −z, legs `b` along +y and −y, the backs
    /// `gap` apart in both directions (a gusset plate between them).
    StarAngle {
        /// Legs along z.
        h: f64,
        /// Legs along y.
        b: f64,
        /// Thickness.
        t: f64,
        /// Root radius.
        r: f64,
        /// Between the backs.
        gap: f64,
    },
    /// Four equal angles heel to heel round a point: a cross of four
    /// double legs, the backs `gap` apart in both directions.
    CrossAngle {
        /// Leg length.
        h: f64,
        /// Thickness.
        t: f64,
        /// Root radius.
        r: f64,
        /// Between the backs.
        gap: f64,
    },
    /// Two channels back to back (2026-11-04): their webs along z, `gap`
    /// apart along y, their flanges `b` pointing away from each other
    /// along −y and +y (a ][ seen end on).
    DoubleChannel {
        /// Height.
        h: f64,
        /// Each channel's flange width.
        b: f64,
        /// Web thickness.
        tw: f64,
        /// Flange thickness.
        tf: f64,
        /// Root radius.
        r: f64,
        /// Between the two webs.
        gap: f64,
    },
}

/// What a shape's symmetry makes of its buckling in compression (the
/// clauses of a code differ by it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Symmetry {
    /// A single angle (equal or unequal legs).
    Angle,
    /// Symmetric about both axes, open (I and H): torsional buckling.
    Double,
    /// Symmetric about one axis parallel to y (a channel): its shear
    /// centre on that axis, torsional-flexural buckling about it.
    AboutY,
    /// Symmetric about one axis parallel to z (two angles back to back).
    AboutZ,
    /// Symmetric about a point, open (a star or cross of angles):
    /// torsional buckling.
    Point,
    /// Closed or solid (tubes, hollow sections, round bars): flexural
    /// buckling only.
    Closed,
    /// Not known (a `general` section).
    Unknown,
}

impl Shape {
    /// Name in files.
    pub fn as_str(&self) -> &'static str {
        match self {
            Shape::EqualAngle { .. } => "equal_angle",
            Shape::UnequalAngle { .. } => "unequal_angle",
            Shape::Channel { .. } => "channel",
            Shape::ISection { .. } => "i_section",
            Shape::RectangularHollow { .. } => "rectangular_hollow",
            Shape::CircularHollow { .. } => "circular_hollow",
            Shape::RoundBar { .. } => "round_bar",
            Shape::PolygonalHollow { .. } => "polygonal_hollow",
            Shape::General { .. } => "general",
            Shape::DoubleAngle { .. } => "double_angle",
            Shape::StarAngle { .. } => "star_angle",
            Shape::CrossAngle { .. } => "cross_angle",
            Shape::DoubleChannel { .. } => "double_channel",
        }
    }

    /// The angle (or channel) a built-up shape is made of, turned as its
    /// parts lie in the section (a `double_angle`'s leg `h` along z), and
    /// how many;
    /// none for the other shapes. Its properties (each angle's own minor
    /// radius of gyration, for its buckling between the stitch bolts):
    /// [`Section::part_properties`].
    pub fn part(&self) -> Option<(Shape, u32)> {
        let angle = |h: f64, b: f64, t: f64, r: f64| {
            if h == b {
                Shape::EqualAngle { h, t, r }
            } else {
                Shape::UnequalAngle { h, b, t, r }
            }
        };
        match *self {
            Shape::DoubleAngle { h, b, t, r, .. } | Shape::StarAngle { h, b, t, r, .. } => {
                Some((angle(h, b, t, r), 2))
            }
            Shape::CrossAngle { h, t, r, .. } => Some((Shape::EqualAngle { h, t, r }, 4)),
            Shape::DoubleChannel {
                h, b, tw, tf, r, ..
            } => Some((Shape::Channel { h, b, tw, tf, r }, 2)),
            _ => None,
        }
    }

    /// The shape's symmetry for buckling.
    pub fn symmetry(&self) -> Symmetry {
        match self {
            Shape::EqualAngle { .. } | Shape::UnequalAngle { .. } => Symmetry::Angle,
            // Two channels back to back: symmetric about both axes, open.
            Shape::ISection { .. } | Shape::DoubleChannel { .. } => Symmetry::Double,
            Shape::Channel { .. } => Symmetry::AboutY,
            Shape::DoubleAngle { .. } => Symmetry::AboutZ,
            Shape::StarAngle { .. } | Shape::CrossAngle { .. } => Symmetry::Point,
            Shape::RectangularHollow { .. }
            | Shape::CircularHollow { .. }
            | Shape::RoundBar { .. }
            | Shape::PolygonalHollow { .. } => Symmetry::Closed,
            Shape::General { .. } => Symmetry::Unknown,
        }
    }

    /// The shear centre (from the lower left of the outline box, m; `None`
    /// at the centroid `c`) and the warping constant C_w, m⁶, from the
    /// plates' midlines, the fillets left out. Closed and solid shapes:
    /// at the centroid, C_w 0 (it plays no part in their buckling).
    /// Built-up shapes: C_w 0 — their angles' own warping left out, on the
    /// safe side. None for a `general` section.
    fn shear_centre_warping(&self, c: [f64; 2]) -> Option<([f64; 2], f64)> {
        Some(match *self {
            Shape::EqualAngle { h, t, .. } => {
                let l = h - t / 2.0;
                ([t / 2.0, t / 2.0], t.powi(3) / 36.0 * 2.0 * l.powi(3))
            }
            Shape::UnequalAngle { h, b, t, .. } => {
                let (lh, lb) = (h - t / 2.0, b - t / 2.0);
                (
                    [t / 2.0, t / 2.0],
                    t.powi(3) / 36.0 * (lh.powi(3) + lb.powi(3)),
                )
            }
            Shape::ISection { h, b, tf, .. } => {
                let h0 = h - tf;
                ([b / 2.0, h / 2.0], tf * b.powi(3) * h0 * h0 / 24.0)
            }
            Shape::Channel { h, b, tw, tf, .. } => {
                // b0 from the web's midline to the flange tip, h0 between
                // the flanges' midlines; e0 the shear centre's distance
                // from the web's midline, away from the flanges.
                let (b0, h0) = (b - tw / 2.0, h - tf);
                let d = 6.0 * b0 * tf + h0 * tw;
                let e0 = 3.0 * b0 * b0 * tf / d;
                let cw = tf * b0.powi(3) * h0 * h0 / 12.0 * (3.0 * b0 * tf + 2.0 * h0 * tw) / d;
                ([tw / 2.0 - e0, h / 2.0], cw)
            }
            Shape::DoubleAngle { b, t, gap, .. } => ([b + gap / 2.0, t / 2.0], 0.0),
            Shape::StarAngle { .. }
            | Shape::CrossAngle { .. }
            | Shape::DoubleChannel { .. }
            | Shape::RectangularHollow { .. }
            | Shape::CircularHollow { .. }
            | Shape::RoundBar { .. }
            | Shape::PolygonalHollow { .. } => (c, 0.0),
            Shape::General { .. } => return None,
        })
    }

    /// Whether the shape is made of several angles.
    pub fn is_built_up(&self) -> bool {
        self.part().is_some()
    }

    /// The angles of a built-up shape of angles where its properties
    /// place them, in the axes of [`Shape::built_up_outline`] (the lower
    /// left of the shape's outline box at the origin): two back to back,
    /// two heel to heel in a star, four in a cross. None for the other
    /// shapes (two channels have no heel). For drawings and for seating
    /// what bolts onto one of them: no property is worked out from these.
    pub fn built_up_angles(&self) -> Option<Vec<PlacedAngle>> {
        let at = |heel: [f64; 2], towards: [f64; 2], h: f64, b: f64, t: f64| PlacedAngle {
            heel,
            towards,
            h,
            b,
            t,
        };
        Some(match *self {
            Shape::DoubleAngle { h, b, t, gap, .. } => vec![
                at([b, 0.0], [-1.0, 1.0], h, b, t),
                at([b + gap, 0.0], [1.0, 1.0], h, b, t),
            ],
            Shape::StarAngle { h, b, t, gap, .. } => vec![
                at([b + gap, h + gap], [1.0, 1.0], h, b, t),
                at([b, h], [-1.0, -1.0], h, b, t),
            ],
            Shape::CrossAngle { h, t, gap, .. } => vec![
                at([h + gap, h + gap], [1.0, 1.0], h, h, t),
                at([h, h + gap], [-1.0, 1.0], h, h, t),
                at([h, h], [-1.0, -1.0], h, h, t),
                at([h + gap, h], [1.0, -1.0], h, h, t),
            ],
            _ => return None,
        })
    }

    /// The parts of a built-up shape as drawn, each its own outline
    /// (counter-clockwise, the fillets left out), in the axes of
    /// [`Shape::built_up_outline`] (the lower left of the shape's outline
    /// box at the origin), where the properties place them: two angles
    /// back to back or heel to heel with the gap between them, four in a
    /// cross ([`Shape::built_up_angles`]), two channels back to back.
    /// None for the other shapes. Drawing only: no property is worked out
    /// from these.
    pub fn built_up_parts(&self) -> Option<Vec<Vec<[f64; 2]>>> {
        if let Some(angles) = self.built_up_angles() {
            return Some(angles.iter().map(PlacedAngle::outline).collect());
        }
        Some(match *self {
            Shape::DoubleChannel {
                h, b, tw, tf, gap, ..
            } => {
                // The first channel's flanges towards −y, the second's
                // towards +y.
                let w = 2.0 * b + gap;
                let (y0, y1) = (b - tw, b + gap + tw);
                vec![
                    vec![
                        [0.0, 0.0],
                        [b, 0.0],
                        [b, h],
                        [0.0, h],
                        [0.0, h - tf],
                        [y0, h - tf],
                        [y0, tf],
                        [0.0, tf],
                    ],
                    vec![
                        [b + gap, 0.0],
                        [w, 0.0],
                        [w, tf],
                        [y1, tf],
                        [y1, h - tf],
                        [w, h - tf],
                        [w, h],
                        [b + gap, h],
                    ],
                ]
            }
            _ => return None,
        })
    }

    /// The outer edge of a built-up shape, counter-clockwise, the lower
    /// left of its outline box at the origin, for drawings: the space
    /// between the angles filled (as by a filler plate), the fillets left
    /// out. None for the other shapes.
    pub fn built_up_outline(&self) -> Option<Vec<[f64; 2]>> {
        Some(match *self {
            Shape::DoubleAngle { h, b, t, gap, .. } => {
                let w = 2.0 * b + gap;
                vec![
                    [0.0, 0.0],
                    [w, 0.0],
                    [w, t],
                    [b + gap + t, t],
                    [b + gap + t, h],
                    [b - t, h],
                    [b - t, t],
                    [0.0, t],
                ]
            }
            Shape::StarAngle { h, b, t, gap, .. } => {
                // The second angle's heel at (b, h), the first's at
                // (b + gap, h + gap); the square round the heels filled.
                let (y1, z1) = (b + gap, h + gap);
                vec![
                    [b - t, 0.0],
                    [b, 0.0],
                    [b, h - t],
                    [y1 + t, h - t],
                    [y1 + t, z1],
                    [y1 + b, z1],
                    [y1 + b, z1 + t],
                    [y1 + t, z1 + t],
                    [y1 + t, z1 + h],
                    [y1, z1 + h],
                    [y1, z1 + t],
                    [b - t, z1 + t],
                    [b - t, h],
                    [0.0, h],
                    [0.0, h - t],
                    [b - t, h - t],
                ]
            }
            Shape::CrossAngle { h, t, gap, .. } => {
                // A plus of arms 2 t + gap wide about the centre (c, c).
                let (a, c) = (t + gap / 2.0, h + gap / 2.0);
                [
                    [-a, -c],
                    [a, -c],
                    [a, -a],
                    [c, -a],
                    [c, a],
                    [a, a],
                    [a, c],
                    [-a, c],
                    [-a, a],
                    [-c, a],
                    [-c, -a],
                    [-a, -a],
                ]
                .iter()
                .map(|p| [p[0] + c, p[1] + c])
                .collect()
            }
            Shape::DoubleChannel {
                h, b, tw, tf, gap, ..
            } => {
                // An I: the webs and the gap between them as one.
                let w = 2.0 * b + gap;
                let (y0, y1) = (b - tw, b + gap + tw);
                vec![
                    [0.0, 0.0],
                    [w, 0.0],
                    [w, tf],
                    [y1, tf],
                    [y1, h - tf],
                    [w, h - tf],
                    [w, h],
                    [0.0, h],
                    [0.0, h - tf],
                    [y0, h - tf],
                    [y0, tf],
                    [0.0, tf],
                ]
            }
            _ => return None,
        })
    }

    /// The section as parts, the lower left of its outline box at the origin.
    fn outline(&self) -> Outline {
        let mut o = Outline::default();
        match *self {
            Shape::EqualAngle { h, t, r } => angle(&mut o, h, h, t, r),
            Shape::UnequalAngle { h, b, t, r } => angle(&mut o, h, b, t, r),
            Shape::Channel { h, b, tw, tf, r } => {
                o.rect(0.0, 0.0, tw, h);
                o.rect(tw, 0.0, b, tf);
                o.rect(tw, h - tf, b, h);
                o.fillet([tw, tf], r, [1.0, 1.0]);
                o.fillet([tw, h - tf], r, [1.0, -1.0]);
            }
            Shape::ISection { h, b, tw, tf, r } => {
                let (w0, w1) = ((b - tw) / 2.0, (b + tw) / 2.0);
                o.rect(0.0, 0.0, b, tf);
                o.rect(0.0, h - tf, b, h);
                o.rect(w0, tf, w1, h - tf);
                o.fillet([w0, tf], r, [-1.0, 1.0]);
                o.fillet([w1, tf], r, [1.0, 1.0]);
                o.fillet([w0, h - tf], r, [-1.0, -1.0]);
                o.fillet([w1, h - tf], r, [1.0, -1.0]);
            }
            Shape::RectangularHollow { h, b, t } => {
                o.rect(0.0, 0.0, b, h);
                o.cut(Part::Polygon(vec![
                    [t, t],
                    [b - t, t],
                    [b - t, h - t],
                    [t, h - t],
                ]));
            }
            Shape::CircularHollow { d, t } => {
                let c = [d / 2.0, d / 2.0];
                o.add(Part::Circle { c, r: d / 2.0 });
                o.cut(Part::Circle { c, r: d / 2.0 - t });
            }
            Shape::RoundBar { d } => o.add(Part::Circle {
                c: [d / 2.0, d / 2.0],
                r: d / 2.0,
            }),
            Shape::PolygonalHollow { sides, d, t } => {
                let outer = geometry::regular_polygon(sides, d / 2.0, [0.0, 0.0]);
                let lo_y = outer.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
                let c = [-lo_y, d / 2.0];
                o.add(Part::Polygon(geometry::regular_polygon(sides, d / 2.0, c)));
                o.cut(Part::Polygon(geometry::regular_polygon(
                    sides,
                    d / 2.0 - t,
                    c,
                )));
            }
            Shape::General { h, b } => o.rect(0.0, 0.0, b, h),
            Shape::DoubleAngle { h, b, t, r, gap } => {
                turned_angle(&mut o, [b, 0.0], [-1.0, 1.0], h, b, t, r);
                turned_angle(&mut o, [b + gap, 0.0], [1.0, 1.0], h, b, t, r);
            }
            Shape::StarAngle { h, b, t, r, gap } => {
                turned_angle(&mut o, [b + gap, h + gap], [1.0, 1.0], h, b, t, r);
                turned_angle(&mut o, [b, h], [-1.0, -1.0], h, b, t, r);
            }
            Shape::CrossAngle { h, t, r, gap } => {
                for (heel, s) in [
                    ([h + gap, h + gap], [1.0, 1.0]),
                    ([h, h + gap], [-1.0, 1.0]),
                    ([h, h], [-1.0, -1.0]),
                    ([h + gap, h], [1.0, -1.0]),
                ] {
                    turned_angle(&mut o, heel, s, h, h, t, r);
                }
            }
            Shape::DoubleChannel {
                h,
                b,
                tw,
                tf,
                r,
                gap,
            } => {
                // The first channel's flanges towards −y, the second's
                // towards +y.
                let w = 2.0 * b + gap;
                let (y0, y1) = (b - tw, b + gap + tw);
                o.rect(y0, 0.0, b, h);
                o.rect(0.0, 0.0, y0, tf);
                o.rect(0.0, h - tf, y0, h);
                o.fillet([y0, tf], r, [-1.0, 1.0]);
                o.fillet([y0, h - tf], r, [-1.0, -1.0]);
                o.rect(b + gap, 0.0, y1, h);
                o.rect(y1, 0.0, w, tf);
                o.rect(y1, h - tf, w, h);
                o.fillet([y1, tf], r, [1.0, 1.0]);
                o.fillet([y1, h - tf], r, [1.0, -1.0]);
            }
        }
        o
    }

    /// Torsion constant, m⁴: exact for round shapes, Bredt's thin wall for
    /// closed ones, Σ b t³ / 3 over the plates of open ones (fillets left
    /// out).
    fn torsion(&self) -> Option<f64> {
        let open = |plates: &[(f64, f64)]| plates.iter().map(|(b, t)| b * t.powi(3) / 3.0).sum();
        Some(match *self {
            Shape::EqualAngle { h, t, .. } => open(&[(h, t), (h - t, t)]),
            Shape::UnequalAngle { h, b, t, .. } => open(&[(h, t), (b - t, t)]),
            Shape::Channel { h, b, tw, tf, .. } | Shape::ISection { h, b, tw, tf, .. } => {
                open(&[(b, tf), (b, tf), (h - 2.0 * tf, tw)])
            }
            Shape::RectangularHollow { h, b, t } => {
                let am = (h - t) * (b - t);
                let p = 2.0 * (h - t + b - t);
                4.0 * am * am * t / p
            }
            Shape::CircularHollow { d, t } => PI * (d.powi(4) - (d - 2.0 * t).powi(4)) / 32.0,
            Shape::RoundBar { d } => PI * d.powi(4) / 32.0,
            Shape::PolygonalHollow { sides, d, t } => {
                let am_r = (d - t) / 2.0;
                let tan = (PI / sides as f64).tan();
                let am = sides as f64 * am_r * am_r * tan;
                let p = 2.0 * sides as f64 * am_r * tan;
                4.0 * am * am * t / p
            }
            Shape::General { .. } => return None,
            Shape::DoubleAngle { .. }
            | Shape::StarAngle { .. }
            | Shape::CrossAngle { .. }
            | Shape::DoubleChannel { .. } => {
                // The parts joined only at points (the stitch bolts): the
                // sum of theirs.
                let (part, n) = self.part()?;
                f64::from(n) * part.torsion()?
            }
        })
    }

    /// Shear areas along y and z, m²: the plates parallel to the shear for
    /// open and rectangular shapes, half the area for tubes, 0.9 A for a
    /// round bar.
    fn shear_areas(&self, area: f64) -> Option<[f64; 2]> {
        Some(match *self {
            Shape::EqualAngle { h, t, .. } => [h * t, h * t],
            Shape::UnequalAngle { h, b, t, .. } => [b * t, h * t],
            Shape::Channel { h, b, tw, tf, .. } | Shape::ISection { h, b, tw, tf, .. } => {
                [2.0 * b * tf, (h - 2.0 * tf) * tw]
            }
            Shape::RectangularHollow { h, b, .. } => [area * b / (b + h), area * h / (b + h)],
            Shape::CircularHollow { .. } | Shape::PolygonalHollow { .. } => {
                [area / 2.0, area / 2.0]
            }
            Shape::RoundBar { .. } => [ROUND_BAR_SHEAR * area; 2],
            Shape::General { .. } => return None,
            Shape::DoubleAngle { .. }
            | Shape::StarAngle { .. }
            | Shape::CrossAngle { .. }
            | Shape::DoubleChannel { .. } => {
                // The parts' plates along y and z stay along y and z.
                let (part, n) = self.part()?;
                let [y, z] = part.shear_areas(area / f64::from(n))?;
                [f64::from(n) * y, f64::from(n) * z]
            }
        })
    }
}

/// One angle of a built-up shape where it is placed
/// ([`Shape::built_up_angles`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedAngle {
    /// Its heel (y, z), m.
    pub heel: [f64; 2],
    /// Where its legs point from the heel: the leg `b` along y towards
    /// `towards[0]` (±1), the leg `h` along z towards `towards[1]`.
    pub towards: [f64; 2],
    /// The leg along z, m.
    pub h: f64,
    /// The leg along y, m.
    pub b: f64,
    /// Thickness, m.
    pub t: f64,
}

impl PlacedAngle {
    /// Its outline, counter-clockwise, the fillet left out.
    pub fn outline(&self) -> Vec<[f64; 2]> {
        let s = self.towards;
        let at = |y: f64, z: f64| [self.heel[0] + s[0] * y, self.heel[1] + s[1] * z];
        let (h, b, t) = (self.h, self.b, self.t);
        let mut o = vec![
            at(0.0, 0.0),
            at(b, 0.0),
            at(b, t),
            at(t, t),
            at(t, h),
            at(0.0, h),
        ];
        // Mirrored once: turned the other way round.
        if s[0] * s[1] < 0.0 {
            o.reverse();
        }
        o
    }
}

/// An angle with its heel at `heel`, leg `b` along y towards `s[0]` (±1),
/// leg `h` along z towards `s[1]`.
fn turned_angle(o: &mut Outline, heel: [f64; 2], s: [f64; 2], h: f64, b: f64, t: f64, r: f64) {
    let at = |y: f64, z: f64| [heel[0] + s[0] * y, heel[1] + s[1] * z];
    let rect = |o: &mut Outline, p: [f64; 2], q: [f64; 2]| {
        o.rect(
            p[0].min(q[0]),
            p[1].min(q[1]),
            p[0].max(q[0]),
            p[1].max(q[1]),
        );
    };
    rect(o, at(0.0, 0.0), at(b, t));
    rect(o, at(0.0, t), at(t, h));
    o.fillet(at(t, t), r, s);
}

/// An angle: the heel at the origin, leg `b` along y, leg `h` along z.
fn angle(o: &mut Outline, h: f64, b: f64, t: f64, r: f64) {
    o.rect(0.0, 0.0, b, t);
    o.rect(0.0, t, t, h);
    o.fillet([t, t], r, [1.0, 1.0]);
}

/// Properties given in the file, replacing the computed ones (SI).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Given {
    /// Area, m².
    pub area: Option<f64>,
    /// Second moment about y, m⁴.
    pub iy: Option<f64>,
    /// Second moment about z, m⁴.
    pub iz: Option<f64>,
    /// Second moment about the major principal axis u, m⁴.
    pub iu: Option<f64>,
    /// Second moment about the minor principal axis v, m⁴.
    pub iv: Option<f64>,
    /// Torsion constant, m⁴.
    pub it: Option<f64>,
    /// Elastic modulus about y, m³.
    pub wel_y: Option<f64>,
    /// Elastic modulus about z, m³.
    pub wel_z: Option<f64>,
    /// Plastic modulus about y, m³.
    pub wpl_y: Option<f64>,
    /// Plastic modulus about z, m³.
    pub wpl_z: Option<f64>,
    /// Radius of gyration about y, m.
    pub gyration_y: Option<f64>,
    /// Radius of gyration about z, m.
    pub gyration_z: Option<f64>,
    /// Radius of gyration about u, m.
    pub gyration_u: Option<f64>,
    /// Radius of gyration about v, m.
    pub gyration_v: Option<f64>,
    /// Shear area along y, m².
    pub shear_area_y: Option<f64>,
    /// Shear area along z, m².
    pub shear_area_z: Option<f64>,
}

/// Cross-section properties, SI. Axes: y along the width, z along the
/// height; u, v the principal axes (u the major one).
#[derive(Debug, Clone, PartialEq)]
pub struct Properties {
    /// Area, m².
    pub area: f64,
    /// Centroid from the lower left of the outline box (the heel of an
    /// angle, the back of a channel's web at its bottom), m.
    pub centroid: [f64; 2],
    /// Outline width along y, m.
    pub width: f64,
    /// Outline height along z, m.
    pub depth: f64,
    /// Second moment about the centroidal y axis, ∫z² dA, m⁴.
    pub iy: f64,
    /// Second moment about the centroidal z axis, ∫y² dA, m⁴.
    pub iz: f64,
    /// Product of inertia ∫yz dA about the centroid, m⁴ (0 for symmetric
    /// shapes; negative for an angle with its legs along +y and +z).
    pub iyz: f64,
    /// Second moment about the major principal axis u, m⁴.
    pub iu: f64,
    /// Second moment about the minor principal axis v, m⁴.
    pub iv: f64,
    /// Angle of u from y, radians (towards z).
    pub principal_angle: f64,
    /// Torsion constant, m⁴.
    pub it: Option<f64>,
    /// Shear centre from the lower left of the outline box, m (thin-walled,
    /// [`Shape::symmetry`] for what it means; none for `general`).
    pub shear_centre: Option<[f64; 2]>,
    /// Warping constant C_w, m⁶ (thin-walled; none for `general`).
    pub warping: Option<f64>,
    /// Elastic modulus about y (iy over the farthest fibre), m³.
    pub wel_y: Option<f64>,
    /// Elastic modulus about z, m³.
    pub wel_z: Option<f64>,
    /// Elastic modulus about u, m³.
    pub wel_u: Option<f64>,
    /// Elastic modulus about v, m³.
    pub wel_v: Option<f64>,
    /// Plastic modulus about y, m³.
    pub wpl_y: Option<f64>,
    /// Plastic modulus about z, m³.
    pub wpl_z: Option<f64>,
    /// Radius of gyration about y, m.
    pub gyration_y: f64,
    /// Radius of gyration about z, m.
    pub gyration_z: f64,
    /// Radius of gyration about u, m.
    pub gyration_u: f64,
    /// Radius of gyration about v (the smallest: buckling of angles), m.
    pub gyration_v: f64,
    /// Shear area along y, m².
    pub shear_area_y: Option<f64>,
    /// Shear area along z, m².
    pub shear_area_z: Option<f64>,
}

/// A section of the library, SI.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// Id structures refer to.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Free text.
    pub description: String,
    /// Profile and dimensions.
    pub shape: Shape,
    /// Material id (the file's `default_material` when it names none).
    pub material: Option<String>,
    /// Properties given in the file.
    pub given: Given,
}

impl Section {
    /// The cross-section properties: computed from the shape, those given
    /// in the file replacing them. Radii of gyration not given follow from
    /// the final area and second moments; principal values not given from
    /// the final iy, iz and the computed product.
    pub fn properties(&self) -> Properties {
        let g = &self.given;
        let o = self.shape.outline();
        let m = o.moments();
        let general = matches!(self.shape, Shape::General { .. });
        let lo = [-o.support([-1.0, 0.0]), -o.support([0.0, -1.0])];
        let (width, depth) = (o.support([1.0, 0.0]) - lo[0], o.support([0.0, 1.0]) - lo[1]);
        let (area, c) = if general {
            (g.area.unwrap_or(0.0), [width / 2.0, depth / 2.0])
        } else {
            (m.a, [m.sy / m.a - lo[0], m.sz / m.a - lo[1]])
        };
        let at = [c[0] + lo[0], c[1] + lo[1]];
        let (mut iy, mut iz, mut iyz) = if general {
            (0.0, 0.0, 0.0)
        } else {
            (
                m.zz - m.a * at[1] * at[1],
                m.yy - m.a * at[0] * at[0],
                m.yz - m.a * at[0] * at[1],
            )
        };
        // Rounding of a symmetric section's product.
        if iyz.abs() <= 1e-12 * (iy + iz) {
            iyz = 0.0;
        }
        iy = g.iy.unwrap_or(iy);
        iz = g.iz.unwrap_or(iz);
        let area = g.area.unwrap_or(area);
        let mean = (iy + iz) / 2.0;
        let radius = (((iy - iz) / 2.0).powi(2) + iyz * iyz).sqrt();
        let alpha = if iyz == 0.0 {
            if iy >= iz { 0.0 } else { PI / 2.0 }
        } else {
            0.5 * (-2.0 * iyz).atan2(iy - iz)
        };
        let iu = g.iu.unwrap_or(mean + radius);
        let iv = g.iv.unwrap_or(mean - radius);
        let (u, v) = ([alpha.cos(), alpha.sin()], [-alpha.sin(), alpha.cos()]);
        // Farthest fibre from the centroid along `e`.
        let reach = |e: [f64; 2]| {
            let ce = at[0] * e[0] + at[1] * e[1];
            (o.support(e) - ce).max(o.support([-e[0], -e[1]]) + ce)
        };
        let elastic = |i: f64, e: [f64; 2]| (!general).then(|| i / reach(e));
        let shear = self.shape.shear_areas(area);
        let gyr = |i: f64| (i / area).sqrt();
        let sc = self.shape.shear_centre_warping(c);
        Properties {
            area,
            centroid: c,
            width,
            depth,
            iy,
            iz,
            iyz,
            iu,
            iv,
            principal_angle: alpha,
            it: g.it.or_else(|| self.shape.torsion()),
            shear_centre: sc.map(|s| s.0),
            warping: sc.map(|s| s.1),
            wel_y: g.wel_y.or_else(|| elastic(iy, [0.0, 1.0])),
            wel_z: g.wel_z.or_else(|| elastic(iz, [1.0, 0.0])),
            wel_u: elastic(iu, v),
            wel_v: elastic(iv, u),
            wpl_y: g.wpl_y.or_else(|| (!general).then(|| o.plastic_modulus(1))),
            wpl_z: g.wpl_z.or_else(|| (!general).then(|| o.plastic_modulus(0))),
            gyration_y: g.gyration_y.unwrap_or_else(|| gyr(iy)),
            gyration_z: g.gyration_z.unwrap_or_else(|| gyr(iz)),
            gyration_u: g.gyration_u.unwrap_or_else(|| gyr(iu)),
            gyration_v: g.gyration_v.unwrap_or_else(|| gyr(iv)),
            shear_area_y: g.shear_area_y.or(shear.map(|s| s[0])),
            shear_area_z: g.shear_area_z.or(shear.map(|s| s[1])),
        }
    }

    /// Width of the profile seen by a wind blowing in `direction` (in the
    /// section's plane, from the y axis towards z), m: the outline's extent
    /// normal to the wind. The silhouette of the whole profile — an angle's
    /// legs do not shield each other, and no drag factor is applied.
    pub fn wind_width(&self, direction: Angle) -> f64 {
        let a = direction.radians();
        self.shape.outline().width_across([a.cos(), a.sin()])
    }

    /// Projected area per metre of member for the same wind, m²/m (the
    /// width times one metre).
    pub fn wind_area_per_length(&self, direction: Angle) -> Area {
        Area::from_square_meters(self.wind_width(direction))
    }

    /// The properties of one angle of a built-up section, computed from
    /// its dimensions (the values given in the file are the whole
    /// section's): its own minor radius of gyration `gyration_v` is what
    /// buckles between the stitch bolts. None for the other shapes.
    pub fn part_properties(&self) -> Option<Properties> {
        let (shape, _) = self.shape.part()?;
        let part = Section {
            shape,
            given: Given::default(),
            ..self.clone()
        };
        Some(part.properties())
    }
}

/// A material of the library, SI.
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    /// Id sections refer to.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Free text.
    pub description: String,
    /// Young's modulus E, Pa.
    pub elasticity: f64,
    /// Shear modulus G, Pa.
    pub shear_modulus: f64,
    /// Density, kg/m³.
    pub density: f64,
    /// Yield strength fy, Pa.
    pub yield_strength: Option<f64>,
    /// Ultimate strength fu, Pa.
    pub ultimate: Option<f64>,
    /// Partial factors by name (as a code check names them).
    pub partial_factors: BTreeMap<String, f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialFile {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    elasticity: Stress,
    #[serde(default)]
    shear_modulus: Option<Stress>,
    #[serde(default)]
    density: Density,
    #[serde(default, rename = "yield")]
    yield_strength: Option<Stress>,
    #[serde(default)]
    ultimate: Option<Stress>,
    #[serde(default)]
    partial_factors: BTreeMap<String, f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SectionFile {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    shape: String,
    #[serde(default)]
    material: Option<String>,
    #[serde(default)]
    h: Option<Length>,
    #[serde(default)]
    b: Option<Length>,
    #[serde(default)]
    t: Option<Length>,
    #[serde(default)]
    tw: Option<Length>,
    #[serde(default)]
    tf: Option<Length>,
    #[serde(default)]
    r: Option<Length>,
    #[serde(default)]
    d: Option<Length>,
    #[serde(default)]
    sides: Option<u32>,
    #[serde(default)]
    gap: Option<Length>,
    #[serde(default)]
    back: Option<String>,
    #[serde(default)]
    area: Option<Area>,
    #[serde(default)]
    iy: Option<SecondMoment>,
    #[serde(default)]
    iz: Option<SecondMoment>,
    #[serde(default)]
    iu: Option<SecondMoment>,
    #[serde(default)]
    iv: Option<SecondMoment>,
    #[serde(default)]
    it: Option<SecondMoment>,
    #[serde(default)]
    wel_y: Option<SectionModulus>,
    #[serde(default)]
    wel_z: Option<SectionModulus>,
    #[serde(default)]
    wpl_y: Option<SectionModulus>,
    #[serde(default)]
    wpl_z: Option<SectionModulus>,
    #[serde(default)]
    gyration_y: Option<Length>,
    #[serde(default)]
    gyration_z: Option<Length>,
    #[serde(default)]
    gyration_u: Option<Length>,
    #[serde(default)]
    gyration_v: Option<Length>,
    #[serde(default)]
    shear_area_y: Option<Area>,
    #[serde(default)]
    shear_area_z: Option<Area>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryFile {
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    default_material: Option<String>,
    #[serde(rename = "material", default)]
    materials: Vec<MaterialFile>,
    #[serde(rename = "section", default)]
    sections: Vec<SectionFile>,
    #[serde(default)]
    wind: Option<wind::WindFile>,
}

/// An id of letters, digits, `-`, `_` and `.`.
fn check_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(format!(
            "id \"{id}\" must be letters, digits, - _ . (not empty)"
        ));
    }
    Ok(())
}

fn positive(name: &str, v: Option<f64>) -> Result<(), String> {
    match v {
        Some(x) if !(x > 0.0 && x.is_finite()) => Err(format!("{name} must be above 0")),
        _ => Ok(()),
    }
}

impl MaterialFile {
    fn resolve(self) -> Result<Material, String> {
        check_id(&self.id)?;
        let e = self.elasticity.si();
        positive("elasticity", Some(e))?;
        let g = self.shear_modulus.map_or(e * DEFAULT_G_OVER_E, |g| g.si());
        positive("shear_modulus", Some(g))?;
        let density = self.density.si();
        if !(density >= 0.0 && density.is_finite()) {
            return Err("density must be 0 or above".into());
        }
        positive("yield", self.yield_strength.map(|s| s.si()))?;
        positive("ultimate", self.ultimate.map(|s| s.si()))?;
        for (k, v) in &self.partial_factors {
            positive(&format!("partial factor {k}"), Some(*v))?;
        }
        Ok(Material {
            id: self.id,
            name: self.name,
            description: self.description,
            elasticity: e,
            shear_modulus: g,
            density,
            yield_strength: self.yield_strength.map(|s| s.si()),
            ultimate: self.ultimate.map(|s| s.si()),
            partial_factors: self.partial_factors,
        })
    }
}

impl SectionFile {
    fn resolve(self, default_material: Option<&str>) -> Result<Section, String> {
        check_id(&self.id)?;
        let dims: [(&str, Option<f64>); 8] = [
            ("h", self.h.map(|l| l.si())),
            ("b", self.b.map(|l| l.si())),
            ("t", self.t.map(|l| l.si())),
            ("tw", self.tw.map(|l| l.si())),
            ("tf", self.tf.map(|l| l.si())),
            ("r", self.r.map(|l| l.si())),
            ("d", self.d.map(|l| l.si())),
            ("gap", self.gap.map(|l| l.si())),
        ];
        let takes: &[&str] = match self.shape.as_str() {
            "equal_angle" => &["h", "t", "r"],
            "unequal_angle" => &["h", "b", "t", "r"],
            "channel" | "i_section" => &["h", "b", "tw", "tf", "r"],
            "rectangular_hollow" => &["h", "b", "t"],
            "circular_hollow" => &["d", "t"],
            "round_bar" => &["d"],
            "polygonal_hollow" => &["sides", "d", "t"],
            "general" => &["h", "b"],
            "double_angle" => &["h", "b", "t", "r", "gap", "back"],
            "star_angle" => &["h", "b", "t", "r", "gap"],
            "cross_angle" => &["h", "t", "r", "gap"],
            "double_channel" => &["h", "b", "tw", "tf", "r", "gap"],
            other => {
                return Err(format!(
                    "shape \"{other}\" unknown (equal_angle, unequal_angle, channel, i_section, \
                     rectangular_hollow, circular_hollow, round_bar, polygonal_hollow, general, \
                     double_angle, star_angle, cross_angle, double_channel)"
                ));
            }
        };
        let shape_name = self.shape.as_str();
        let sides_given = self.sides.map(|_| ("sides", Some(1.0)));
        let back_given = self.back.as_ref().map(|_| ("back", Some(1.0)));
        for (k, v) in dims.iter().copied().chain(sides_given).chain(back_given) {
            if v.is_some() && !takes.contains(&k) {
                return Err(format!(
                    "{k} is not a dimension of {shape_name} (it takes {})",
                    takes.join(", ")
                ));
            }
        }
        let get = |k: &str| -> Result<f64, String> {
            let v = dims.iter().find(|d| d.0 == k).and_then(|d| d.1);
            if k == "r" {
                let r = v.unwrap_or(0.0);
                if !(r >= 0.0 && r.is_finite()) {
                    return Err("r must be 0 or above".into());
                }
                return Ok(r);
            }
            if k == "gap" {
                // No default: how far apart the angles are is the data's.
                let g = v.ok_or_else(|| {
                    format!(
                        "{shape_name} needs gap (the space between the angles' backs, e.g. the \
                         filler or gusset plate's thickness; \"0 mm\" when they touch)"
                    )
                })?;
                if !(g >= 0.0 && g.is_finite()) {
                    return Err("gap must be 0 or above".into());
                }
                return Ok(g);
            }
            let v = v.ok_or_else(|| format!("{shape_name} needs {k}"))?;
            positive(k, Some(v))?;
            Ok(v)
        };
        let fits = |ok: bool, what: &str| if ok { Ok(()) } else { Err(what.to_owned()) };
        let shape = match shape_name {
            "equal_angle" | "unequal_angle" => {
                let (h, t, r) = (get("h")?, get("t")?, get("r")?);
                let b = if shape_name == "equal_angle" {
                    h
                } else {
                    get("b")?
                };
                fits(t + r <= h.min(b), "t + r must not exceed a leg")?;
                if shape_name == "equal_angle" {
                    Shape::EqualAngle { h, t, r }
                } else {
                    Shape::UnequalAngle { h, b, t, r }
                }
            }
            "channel" | "i_section" | "double_channel" => {
                let (h, b, tw, tf, r) = (get("h")?, get("b")?, get("tw")?, get("tf")?, get("r")?);
                fits(
                    h - 2.0 * tf >= 2.0 * r,
                    "h - 2 tf must leave room for both root radii",
                )?;
                if shape_name == "double_channel" {
                    fits(tw + r <= b, "tw + r must not exceed b")?;
                    Shape::DoubleChannel {
                        h,
                        b,
                        tw,
                        tf,
                        r,
                        gap: get("gap")?,
                    }
                } else if shape_name == "channel" {
                    fits(tw + r <= b, "tw + r must not exceed b")?;
                    Shape::Channel { h, b, tw, tf, r }
                } else {
                    fits(tw + 2.0 * r <= b, "tw + 2 r must not exceed b")?;
                    Shape::ISection { h, b, tw, tf, r }
                }
            }
            "rectangular_hollow" => {
                let (h, b, t) = (get("h")?, get("b")?, get("t")?);
                fits(2.0 * t < h.min(b), "2 t must be below h and b")?;
                Shape::RectangularHollow { h, b, t }
            }
            "circular_hollow" => {
                let (d, t) = (get("d")?, get("t")?);
                fits(2.0 * t < d, "2 t must be below d")?;
                Shape::CircularHollow { d, t }
            }
            "round_bar" => Shape::RoundBar { d: get("d")? },
            "polygonal_hollow" => {
                let sides = self.sides.ok_or("polygonal_hollow needs sides")?;
                fits(sides >= 3, "sides must be 3 or more")?;
                let (d, t) = (get("d")?, get("t")?);
                fits(2.0 * t < d, "2 t must be below d")?;
                Shape::PolygonalHollow { sides, d, t }
            }
            "double_angle" | "star_angle" | "cross_angle" => {
                let (h, t, r, gap) = (get("h")?, get("t")?, get("r")?, get("gap")?);
                let b = if self.b.is_some() { get("b")? } else { h };
                fits(t + r <= h.min(b), "t + r must not exceed a leg")?;
                match shape_name {
                    "double_angle" => {
                        // h along z: the legs back to back.
                        let (long, short) = (h.max(b), h.min(b));
                        let (h, b) = match self.back.as_deref() {
                            Some("long") => (long, short),
                            Some("short") => (short, long),
                            Some(other) => {
                                return Err(format!(
                                    "back \"{other}\" must be \"long\" or \"short\" (the legs \
                                     back to back)"
                                ));
                            }
                            None if h == b => (h, b),
                            None => {
                                return Err("double_angle with unequal legs needs back = \
                                            \"long\" or \"short\" (the legs back to back)"
                                    .into());
                            }
                        };
                        Shape::DoubleAngle { h, b, t, r, gap }
                    }
                    "star_angle" => Shape::StarAngle { h, b, t, r, gap },
                    _ => Shape::CrossAngle { h, t, r, gap },
                }
            }
            _ => Shape::General {
                h: get("h")?,
                b: get("b")?,
            },
        };
        let given = Given {
            area: self.area.map(|q| q.si()),
            iy: self.iy.map(|q| q.si()),
            iz: self.iz.map(|q| q.si()),
            iu: self.iu.map(|q| q.si()),
            iv: self.iv.map(|q| q.si()),
            it: self.it.map(|q| q.si()),
            wel_y: self.wel_y.map(|q| q.si()),
            wel_z: self.wel_z.map(|q| q.si()),
            wpl_y: self.wpl_y.map(|q| q.si()),
            wpl_z: self.wpl_z.map(|q| q.si()),
            gyration_y: self.gyration_y.map(|q| q.si()),
            gyration_z: self.gyration_z.map(|q| q.si()),
            gyration_u: self.gyration_u.map(|q| q.si()),
            gyration_v: self.gyration_v.map(|q| q.si()),
            shear_area_y: self.shear_area_y.map(|q| q.si()),
            shear_area_z: self.shear_area_z.map(|q| q.si()),
        };
        for (k, v) in [
            ("area", given.area),
            ("iy", given.iy),
            ("iz", given.iz),
            ("iu", given.iu),
            ("iv", given.iv),
            ("it", given.it),
            ("wel_y", given.wel_y),
            ("wel_z", given.wel_z),
            ("wpl_y", given.wpl_y),
            ("wpl_z", given.wpl_z),
            ("gyration_y", given.gyration_y),
            ("gyration_z", given.gyration_z),
            ("gyration_u", given.gyration_u),
            ("gyration_v", given.gyration_v),
            ("shear_area_y", given.shear_area_y),
            ("shear_area_z", given.shear_area_z),
        ] {
            positive(k, v)?;
        }
        if matches!(shape, Shape::General { .. }) {
            for (k, v) in [("area", given.area), ("iy", given.iy), ("iz", given.iz)] {
                if v.is_none() {
                    return Err(format!("a general section needs {k}"));
                }
            }
        }
        if let Some(m) = &self.material {
            check_id(m).map_err(|e| format!("material: {e}"))?;
        }
        Ok(Section {
            id: self.id,
            name: self.name,
            description: self.description,
            shape,
            material: self.material.or(default_material.map(str::to_owned)),
            given,
        })
    }
}

/// Sections and materials of one or more `.ohlsec` files.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SectionLibrary {
    /// Name (of the first file read).
    pub name: String,
    /// Free text.
    pub description: String,
    /// Sections, in file order.
    pub sections: Vec<Section>,
    /// Materials, in file order.
    pub materials: Vec<Material>,
    /// Wind factors for structures (`[wind]`: drag per shape, a lattice
    /// face's drag and shielding by solidity), when given — data from a
    /// design-code plugin or the user, never shipped values.
    pub wind: Option<StructureWind>,
}

/// What [`SectionLibrary::merge`] reports for a second `[wind]` table.
pub const WIND_TABLE: &str = "[wind]";

impl SectionLibrary {
    /// Parses and checks a file's text. Ids are unique within the file; a
    /// material a section names may be in another library merged later
    /// ([`Self::missing_materials`]).
    pub fn parse(text: &str) -> Result<Self, String> {
        let file: LibraryFile = toml::from_str(text).map_err(|e| e.to_string())?;
        ohl_core::files::check_header(
            file.format.as_deref(),
            file.version,
            SECTION_FILE_FORMAT,
            SECTION_FILE_VERSION,
            "section library",
        )?;
        if let Some(m) = &file.default_material {
            check_id(m).map_err(|e| format!("default_material: {e}"))?;
        }
        let mut lib = Self {
            name: file.name,
            description: file.description,
            ..Self::default()
        };
        for m in file.materials {
            let id = m.id.clone();
            if lib.material(&id).is_some() {
                return Err(format!("material \"{id}\" is defined twice"));
            }
            lib.materials
                .push(m.resolve().map_err(|e| format!("material \"{id}\": {e}"))?);
        }
        for s in file.sections {
            let id = s.id.clone();
            if lib.section(&id).is_some() {
                return Err(format!("section \"{id}\" is defined twice"));
            }
            lib.sections.push(
                s.resolve(file.default_material.as_deref())
                    .map_err(|e| format!("section \"{id}\": {e}"))?,
            );
        }
        if let Some(w) = file.wind {
            lib.wind = Some(w.resolve().map_err(|e| format!("[wind]: {e}"))?);
        }
        Ok(lib)
    }

    /// Reads a file, or every `.ohlsec` file of a folder in name order (an
    /// id in two of them is refused).
    pub fn load(path: &Path) -> Result<Self, String> {
        let read = |p: &Path| {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            Self::parse(&text).map_err(|e| format!("{}: {e}", p.display()))
        };
        if !path.is_dir() {
            return read(path);
        }
        let mut lib = Self::default();
        for p in ohl_core::files::files_with_extension(path, SECTION_FILE_EXTENSION) {
            let next = read(&p)?;
            if let Some(id) = lib.merge(next).first() {
                return Err(if id == WIND_TABLE {
                    format!(
                        "{}: wind factors ({WIND_TABLE}) are also in another file: keep them in one",
                        p.display()
                    )
                } else {
                    format!("{}: \"{id}\" is also in another file", p.display())
                });
            }
        }
        Ok(lib)
    }

    /// Adds the sections and materials of `other` whose ids this library
    /// does not have (the earlier one wins: merge the project's library
    /// first, then the user's); returns the ids left out ([`WIND_TABLE`]
    /// for a `[wind]` table when this library has one).
    pub fn merge(&mut self, other: SectionLibrary) -> Vec<String> {
        if self.name.is_empty() {
            self.name = other.name;
            self.description = other.description;
        }
        let mut left_out = Vec::new();
        match (&self.wind, other.wind) {
            (_, None) => {}
            (None, Some(w)) => self.wind = Some(w),
            (Some(_), Some(_)) => left_out.push(WIND_TABLE.to_owned()),
        }
        for m in other.materials {
            if self.material(&m.id).is_some() {
                left_out.push(m.id);
            } else {
                self.materials.push(m);
            }
        }
        for s in other.sections {
            if self.section(&s.id).is_some() {
                left_out.push(s.id);
            } else {
                self.sections.push(s);
            }
        }
        left_out
    }

    /// The section with this id.
    pub fn section(&self, id: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.id == id)
    }

    /// The material with this id.
    pub fn material(&self, id: &str) -> Option<&Material> {
        self.materials.iter().find(|m| m.id == id)
    }

    /// The material of a section, if it names one the library has.
    pub fn material_of(&self, section: &Section) -> Option<&Material> {
        section.material.as_deref().and_then(|m| self.material(m))
    }

    /// "section S: no material M" for every section naming a material the
    /// library does not have.
    pub fn missing_materials(&self) -> Vec<String> {
        self.sections
            .iter()
            .filter_map(|s| {
                let m = s.material.as_deref()?;
                self.material(m)
                    .is_none()
                    .then(|| format!("section \"{}\": no material \"{m}\"", s.id))
            })
            .collect()
    }
}

#[cfg(test)]
mod built_up_tests;
#[cfg(test)]
mod tests;
