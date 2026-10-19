// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structure bodies as drawn in the line's views and drawings (display
//! only: nothing here is used by a calculation).
//!
//! A structure type at level 2 or 3 (members, or `[[body]]` polylines
//! making a 3D object, or a separate `.ohlfe` model) has a
//! [`DrawnBody`] per extension: its members as lines in the structure's
//! axes as placed — exactly [`StructureType::stick_parts`], the lines the
//! line itself is resolved with (feet on the ground, raised by the
//! extension) — and, where the member's section comes from the section
//! library with a profile, the member as a solid: the profile's outline
//! (fillets left out, round shapes as 16-sided polygons, tubes drawn
//! closed) about the member's axis through its centroid, turned as the
//! finite-element model turns it ([`Member::reference`], [`Member::roll`],
//! the angle's `orientation`). A type at level 1 has none: it is drawn
//! with its symbol.
//!
//! The geometry is made once per type and extension ([`Bodies::load`]) and
//! only moved to each placed structure ([`DrawnBody::placed_lines`],
//! [`DrawnBody::placed_triangles`]): structures of one type share it.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use ohl_sections::{SectionLibrary, Shape};
use ohl_structures::structures::to_global;
use ohl_structures::{StructureLibrary, StructureType};

use crate::model::{Member, Model, cross, norm, sub};

/// Sides of a round profile as drawn.
const ROUND_SIDES: usize = 16;

/// A line of a drawn body.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnMember {
    /// The member's id (none: a `[[body]]` polyline).
    pub id: Option<String>,
    /// Its points, structure axes as placed (feet at z = 0), m.
    pub points: Vec<[f64; 3]>,
    /// Its solid (triangles, structure axes as placed), when its section
    /// has a profile in the section library; empty otherwise.
    pub solid: Vec<[[f64; 3]; 3]>,
}

/// A structure type's body as drawn, on one extension.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DrawnBody {
    /// Its members and polylines.
    pub members: Vec<DrawnMember>,
}

impl DrawnBody {
    /// The body of `t` on `extension` (none: as written), its members'
    /// solids from `model` (the type's finite-element model on that
    /// extension, when it has one); none when `t` is at level 1 (no
    /// member, no 3D body, no model with members).
    pub fn of(t: &StructureType, extension: Option<&str>, model: Option<&Model>) -> Option<Self> {
        let mut members: Vec<DrawnMember> = Vec::new();
        if !t.members.is_empty() || !t.body.is_empty() {
            for (id, points) in t.stick_parts(extension).ok()? {
                let solid = match (&id, model) {
                    (Some(id), Some(m)) => solid(m, id, &points),
                    _ => Vec::new(),
                };
                members.push(DrawnMember { id, points, solid });
            }
        }
        // Members of a separate model (the type has none of its own),
        // raised as the type's points are.
        if t.members.is_empty()
            && t.model.is_some()
            && let Some(m) = model
        {
            let lift = t.lift(extension).ok()?;
            for x in &m.members {
                let up = |k: usize| {
                    let p = m.nodes[k].point;
                    [p[0], p[1], p[2] + lift]
                };
                let points = vec![up(x.i), up(x.j)];
                let solid = solid(m, &x.id, &points);
                members.push(DrawnMember {
                    id: Some(x.id.clone()),
                    points,
                    solid,
                });
            }
        }
        let has_members = members.iter().any(|m| m.id.is_some());
        (has_members || t.body_is_3d()).then_some(Self { members })
    }

    /// The lines placed at `at` (the structure's position raised by its
    /// height adjustment) turned by `rotation_deg`, as the line places
    /// them.
    pub fn placed_lines(&self, at: [f64; 3], rotation_deg: f64) -> Vec<Vec<[f64; 3]>> {
        self.members
            .iter()
            .map(|m| {
                m.points
                    .iter()
                    .map(|p| to_global(at, rotation_deg, *p))
                    .collect()
            })
            .collect()
    }

    /// The solids' triangles placed the same way, three points each.
    pub fn placed_triangles(&self, at: [f64; 3], rotation_deg: f64) -> Vec<[f64; 3]> {
        self.members
            .iter()
            .flat_map(|m| m.solid.iter().flatten())
            .map(|p| to_global(at, rotation_deg, *p))
            .collect()
    }

    /// Triangles of all the solids.
    pub fn triangles(&self) -> usize {
        self.members.iter().map(|m| m.solid.len()).sum()
    }
}

/// The member `id` of `model` drawn between `points` (its two ends) as a
/// solid of its profile; empty without a profile.
fn solid(model: &Model, id: &str, points: &[[f64; 3]]) -> Vec<[[f64; 3]; 3]> {
    let Some(m) = model.members.iter().find(|m| m.id == id) else {
        return Vec::new();
    };
    let Some(profile) = model
        .sections
        .get(m.section)
        .and_then(|s| s.profile.as_ref())
    else {
        return Vec::new();
    };
    let [a, b] = match points {
        [a, b] => [*a, *b],
        _ => return Vec::new(),
    };
    if norm(sub(b, a)) < 1e-9 {
        return Vec::new();
    }
    let outline = outline(&profile.shape);
    if outline.len() < 3 {
        return Vec::new();
    }
    extrude(&outline, a, b, axes(a, b, m))
}

/// A member's local axes (rows x, y, z) between `a` and `b`: x along it,
/// y in the plane of x and its reference (global z, or global x for a
/// vertical member), z = x × y, then turned by its roll about x — the
/// finite-element model's element axes.
fn axes(a: [f64; 3], b: [f64; 3], m: &Member) -> [[f64; 3]; 3] {
    let d = sub(b, a);
    let l = norm(d);
    let x = [d[0] / l, d[1] / l, d[2] / l];
    let r = m.reference.unwrap_or(if x[2].abs() > 0.999 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    });
    let z = cross(x, r);
    let nz = norm(z);
    let z = [z[0] / nz, z[1] / nz, z[2] / nz];
    let y = cross(z, x);
    let (s, c) = m.roll.sin_cos();
    let y2 = [0, 1, 2].map(|k| c * y[k] + s * z[k]);
    let z2 = [0, 1, 2].map(|k| -s * y[k] + c * z[k]);
    [x, y2, z2]
}

/// The profile's outline in its plane (y along its width, z along its
/// height), about its centroid, counter-clockwise: the outer edge only
/// (fillets left out, round shapes as polygons, tubes closed).
pub fn outline(shape: &Shape) -> Vec<[f64; 2]> {
    let circle = |d: f64| -> Vec<[f64; 2]> {
        (0..ROUND_SIDES)
            .map(|k| {
                let a = 2.0 * PI * k as f64 / ROUND_SIDES as f64;
                [d / 2.0 * a.cos(), d / 2.0 * a.sin()]
            })
            .collect()
    };
    let rect = |b: f64, h: f64| vec![[0.0, 0.0], [b, 0.0], [b, h], [0.0, h]];
    let poly = match *shape {
        Shape::EqualAngle { h, t, .. } => angle(h, h, t),
        Shape::UnequalAngle { h, b, t, .. } => angle(h, b, t),
        Shape::Channel { h, b, tw, tf, .. } => vec![
            [0.0, 0.0],
            [b, 0.0],
            [b, tf],
            [tw, tf],
            [tw, h - tf],
            [b, h - tf],
            [b, h],
            [0.0, h],
        ],
        Shape::ISection { h, b, tw, tf, .. } => {
            let (w0, w1) = ((b - tw) / 2.0, (b + tw) / 2.0);
            vec![
                [0.0, 0.0],
                [b, 0.0],
                [b, tf],
                [w1, tf],
                [w1, h - tf],
                [b, h - tf],
                [b, h],
                [0.0, h],
                [0.0, h - tf],
                [w0, h - tf],
                [w0, tf],
                [0.0, tf],
            ]
        }
        Shape::RectangularHollow { h, b, .. } | Shape::General { h, b } => rect(b, h),
        Shape::CircularHollow { d, .. } | Shape::RoundBar { d } => circle(d),
        Shape::PolygonalHollow { sides, d, .. } => {
            let n = sides.max(3) as usize;
            // Across flats d, a side at the bottom.
            let r = d / 2.0 / (PI / n as f64).cos();
            (0..n)
                .map(|k| {
                    let a = -PI / 2.0 + PI / n as f64 + 2.0 * PI * k as f64 / n as f64;
                    [r * a.cos(), r * a.sin()]
                })
                .collect()
        }
    };
    let c = centroid(&poly);
    poly.iter().map(|p| [p[0] - c[0], p[1] - c[1]]).collect()
}

/// An angle: the heel at the origin, leg `b` along y, leg `h` along z.
fn angle(h: f64, b: f64, t: f64) -> Vec<[f64; 2]> {
    vec![[0.0, 0.0], [b, 0.0], [b, t], [t, t], [t, h], [0.0, h]]
}

/// The area centroid of a simple polygon.
fn centroid(p: &[[f64; 2]]) -> [f64; 2] {
    let (mut a, mut cy, mut cz) = (0.0, 0.0, 0.0);
    for k in 0..p.len() {
        let (u, v) = (p[k], p[(k + 1) % p.len()]);
        let w = u[0] * v[1] - v[0] * u[1];
        a += w;
        cy += (u[0] + v[0]) * w;
        cz += (u[1] + v[1]) * w;
    }
    if a.abs() < 1e-18 {
        return [0.0, 0.0];
    }
    [cy / (3.0 * a), cz / (3.0 * a)]
}

/// The outline swept from `a` to `b` in the member's axes: its sides and
/// both ends.
fn extrude(outline: &[[f64; 2]], a: [f64; 3], b: [f64; 3], e: [[f64; 3]; 3]) -> Vec<[[f64; 3]; 3]> {
    let at = |o: [f64; 3], q: [f64; 2]| [0, 1, 2].map(|k| o[k] + q[0] * e[1][k] + q[1] * e[2][k]);
    let ra: Vec<[f64; 3]> = outline.iter().map(|q| at(a, *q)).collect();
    let rb: Vec<[f64; 3]> = outline.iter().map(|q| at(b, *q)).collect();
    let n = outline.len();
    let mut out = Vec::with_capacity(2 * n + 2 * (n - 2));
    for k in 0..n {
        let j = (k + 1) % n;
        out.push([ra[k], ra[j], rb[j]]);
        out.push([ra[k], rb[j], rb[k]]);
    }
    for [i, j, k] in ears(outline) {
        out.push([ra[k], ra[j], ra[i]]);
        out.push([rb[i], rb[j], rb[k]]);
    }
    out
}

/// A simple counter-clockwise polygon in triangles (ear clipping; a few
/// vertices).
fn ears(p: &[[f64; 2]]) -> Vec<[usize; 3]> {
    let turn = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let mut left: Vec<usize> = (0..p.len()).collect();
    let mut out = Vec::new();
    // Exit clause: one ear per pass at least, else a fan of what is left.
    while left.len() > 3 {
        let n = left.len();
        let ear = (0..n).find(|&k| {
            let (i, j, l) = (left[(k + n - 1) % n], left[k], left[(k + 1) % n]);
            turn(p[i], p[j], p[l]) > 1e-15
                && left.iter().all(|&m| {
                    m == i
                        || m == j
                        || m == l
                        || !(turn(p[i], p[j], p[m]) >= 0.0
                            && turn(p[j], p[l], p[m]) >= 0.0
                            && turn(p[l], p[i], p[m]) >= 0.0)
                })
        });
        let Some(k) = ear else {
            break;
        };
        out.push([left[(k + n - 1) % n], left[k], left[(k + 1) % n]]);
        left.remove(k);
    }
    for k in 1..left.len().saturating_sub(1) {
        out.push([left[0], left[k], left[k + 1]]);
    }
    out
}

/// The drawn bodies of a structure library, by type and extension.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bodies {
    bodies: BTreeMap<(String, Option<String>), DrawnBody>,
}

impl Bodies {
    /// The bodies of every type of `types` at level 2 or 3, as written and
    /// on each extension, the members' profiles from `sections` first (as
    /// the finite-element check reads them; a model that does not read
    /// leaves its members as lines).
    ///
    /// Expected cost: reading each designed type's model once per
    /// extension that changes it (a few milliseconds for a lattice tower
    /// of some hundred members).
    pub fn load(types: &StructureLibrary, sections: &SectionLibrary) -> Self {
        let mut out = Self::default();
        for t in types.iter() {
            let path = types.path(&t.id);
            let read = |extension: Option<&str>| -> Option<Model> {
                let path = path?;
                if let Some(file) = &t.model {
                    let dir = path.parent()?;
                    return Model::load_with(&dir.join(file), sections).ok();
                }
                if !t.members_designed() {
                    return None;
                }
                Model::load_extended(path, sections, extension).ok()
            };
            let base = read(None);
            out.add(t, None, base.as_ref());
            for e in &t.extensions {
                if e.changes_model() && t.model.is_none() {
                    let own = read(Some(&e.name));
                    out.add(t, Some(&e.name), own.as_ref());
                } else {
                    out.add(t, Some(&e.name), base.as_ref());
                }
            }
        }
        out
    }

    fn add(&mut self, t: &StructureType, extension: Option<&str>, model: Option<&Model>) {
        if let Some(b) = DrawnBody::of(t, extension, model) {
            self.bodies
                .insert((t.id.clone(), extension.map(str::to_owned)), b);
        }
    }

    /// The body of type `id` on `extension` (none: as written); none at
    /// level 1.
    pub fn get(&self, id: &str, extension: Option<&str>) -> Option<&DrawnBody> {
        self.bodies
            .get(&(id.to_owned(), extension.map(str::to_owned)))
    }

    /// Whether no type has a body drawn.
    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_structures::LibraryItem;
    use std::path::Path;

    fn example() -> (StructureLibrary, SectionLibrary) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let types =
            StructureLibrary::load_dir(&root.join("examples/synthetic-line/Structures")).unwrap();
        let sections = SectionLibrary::load(&root.join("libraries/sections")).unwrap();
        (types, sections)
    }

    #[test]
    fn a_lattice_type_is_drawn_with_its_members_and_level_1_with_none() {
        let (types, sections) = example();
        let bodies = Bodies::load(&types, &sections);
        // type-02: 178 members, each a line between its joints, as the line
        // is resolved with (the stick drawing).
        let t = types.get("type-02").unwrap();
        let b = bodies.get("type-02", None).unwrap();
        assert_eq!(b.members.len(), 178);
        let stick = t.stick(None).unwrap();
        let lines: Vec<_> = b.members.iter().map(|m| m.points.clone()).collect();
        assert_eq!(lines, stick);
        assert_eq!(b.members[0].id.as_deref(), Some("m1"));
        // Its sections are `[[section]]` tables (no profile): lines only.
        assert_eq!(b.triangles(), 0);
        // type-01 is a stick figure: its symbol.
        assert!(bodies.get("type-01", None).is_none());
    }

    #[test]
    fn placed_lines_are_the_engine_placement() {
        let (types, sections) = example();
        let bodies = Bodies::load(&types, &sections);
        let t = types.get("type-02").unwrap();
        let at = [1000.0, 2000.0, 400.0];
        assert_eq!(
            bodies.get("type-02", None).unwrap().placed_lines(at, 30.0),
            t.place_stick(at, 30.0, None).unwrap()
        );
    }

    /// A one-member frame of a library angle: the solid is the angle's
    /// outline swept along the member.
    #[test]
    fn a_profile_is_swept_along_its_member() {
        let (_, sections) = example();
        let text = r#"
format = "ohl-structure"
version = 2
id = "post"
name = "post"
kind = "other"
[[node]]
id = "a"
point = ["0 m", "0 m", "0 m"]
support = "fixed"
[[node]]
id = "b"
point = ["0 m", "0 m", "10 m"]
[[member]]
id = "m"
nodes = ["a", "b"]
section = "EA-61x7"
kind = "frame"
"#;
        let t = StructureType::parse(Path::new("post.ohls"), text).unwrap();
        let model = Model::from_structure_type_with(text, &sections).unwrap();
        let b = DrawnBody::of(&t, None, Some(&model)).unwrap();
        let s = &b.members[0].solid;
        // 6 sides × 2 triangles, two ends of 4 triangles.
        assert_eq!(s.len(), 12 + 8);
        // Every corner lies on the member's ends, within the leg length
        // (61 mm) of the axis.
        for p in s.iter().flatten() {
            assert!(p[2].abs() < 1e-9 || (p[2] - 10.0).abs() < 1e-9);
            assert!(p[0].hypot(p[1]) < 0.061);
        }
        // The outline's area is the angle's without the root fillet:
        // 61 × 7 + 54 × 7 = 805 mm².
        let o = outline(&sections.section("EA-61x7").unwrap().shape);
        let area: f64 = (0..o.len())
            .map(|k| {
                let (u, v) = (o[k], o[(k + 1) % o.len()]);
                u[0] * v[1] - v[0] * u[1]
            })
            .sum::<f64>()
            / 2.0;
        assert!((area - 805e-6).abs() < 1e-9, "{area}");
        // About its centroid.
        let c = centroid(&o);
        assert!(c[0].abs() < 1e-12 && c[1].abs() < 1e-12);
    }

    #[test]
    fn members_turn_as_the_finite_element_model_turns_them() {
        let mut model = Model::default();
        for (id, p) in [("a", [0.0, 0.0, 0.0]), ("b", [3.0, 1.0, 4.0])] {
            model.nodes.push(crate::model::Node::free(id, p));
        }
        let mut m = Member::new("m", crate::MemberKind::Frame, 0, 1, 0);
        m.reference = Some([1.0, 0.0, 0.0]);
        m.roll = 0.4;
        let e = crate::element::axes(&model, &m);
        let ours = axes(model.nodes[0].point, model.nodes[1].point, &m);
        for (r, row) in ours.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                assert!((v - e[(r, c)]).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn concave_outlines_split_into_their_triangles() {
        // An I of 12 corners: 10 triangles, their areas the outline's.
        let o = outline(&Shape::ISection {
            h: 0.2,
            b: 0.1,
            tw: 0.01,
            tf: 0.02,
            r: 0.0,
        });
        let t = ears(&o);
        assert_eq!(t.len(), 10);
        let area: f64 = t
            .iter()
            .map(|[i, j, k]| {
                let (a, b, c) = (o[*i], o[*j], o[*k]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])) / 2.0
            })
            .sum();
        // 2 × 0.1 × 0.02 + 0.16 × 0.01 = 0.0056 m².
        assert!((area - 0.0056).abs() < 1e-12, "{area}");
    }
}
