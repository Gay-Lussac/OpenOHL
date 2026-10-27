// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What a member-capacity method outside the core (a plugin's
//! `member-capacity` export, 2026-10-27) needs per member, and the check
//! with its capacities ([`crate::check::member_usage_with`]). The core
//! gives the data; the method gives compression and tension capacities;
//! the core computes the usage. Neutral types: the plugin host maps them
//! to its interface.

use std::fmt::Write as _;

use crate::design::MemberDesign;
use crate::model::Model;
use crate::solve::Solution;

/// One member as a capacity method receives it, SI units.
#[derive(Debug, Clone, PartialEq)]
pub struct CapacityInput {
    /// Its id.
    pub id: String,
    /// `truss`, `frame`, `cable` or `tension`.
    pub kind: String,
    /// The profile's shape name ("" without a profile).
    pub shape: String,
    /// The profile's dimensions by name, m.
    pub dimensions: Vec<(String, f64)>,
    /// Area, m².
    pub area: f64,
    /// Second moment about y, m⁴.
    pub iy: f64,
    /// Second moment about z, m⁴.
    pub iz: f64,
    /// Minor principal second moment, m⁴.
    pub iv: f64,
    /// Elastic modulus about y, m³.
    pub wy: Option<f64>,
    /// Elastic modulus about z, m³.
    pub wz: Option<f64>,
    /// Young's modulus, Pa.
    pub e: f64,
    /// Yield strength, Pa.
    pub fy: Option<f64>,
    /// Ultimate strength, Pa.
    pub fu: Option<f64>,
    /// Length, m.
    pub length: f64,
    /// Effective length factor.
    pub k: f64,
    /// Effective length, m.
    pub lk: Option<f64>,
    /// Its design data as a TOML table ([`design_toml`]).
    pub design: String,
    /// Axial force, N, tension positive.
    pub axial: f64,
    /// End moments my-i, mz-i, my-j, mz-j, N·m.
    pub moments: [f64; 4],
    /// The crossing partner's axial force, N.
    pub partner_axial: Option<f64>,
}

/// A profile's dimensions by name, m.
fn dimensions(shape: &ohl_sections::Shape) -> Vec<(String, f64)> {
    use ohl_sections::Shape as S;
    let named = |v: &[(&str, f64)]| v.iter().map(|(k, x)| ((*k).to_owned(), *x)).collect();
    match *shape {
        S::EqualAngle { h, t, r } => named(&[("h", h), ("t", t), ("r", r)]),
        S::UnequalAngle { h, b, t, r } => named(&[("h", h), ("b", b), ("t", t), ("r", r)]),
        S::Channel { h, b, tw, tf, r } | S::ISection { h, b, tw, tf, r } => {
            named(&[("h", h), ("b", b), ("tw", tw), ("tf", tf), ("r", r)])
        }
        S::RectangularHollow { h, b, t } => named(&[("h", h), ("b", b), ("t", t)]),
        S::CircularHollow { d, t } => named(&[("d", d), ("t", t)]),
        S::RoundBar { d } => named(&[("d", d)]),
        S::PolygonalHollow { sides, d, t } => {
            named(&[("sides", f64::from(sides)), ("d", d), ("t", t)])
        }
        S::General { h, b } => named(&[("h", h), ("b", b)]),
    }
}

/// A member's design data as a TOML table: the keys of `[member.design]`,
/// lengths in m, forces in N, stresses in Pa as plain numbers, the
/// crossing partner by id ("" for none).
pub fn design_toml(model: &Model, d: &MemberDesign) -> String {
    let mut t = String::new();
    if let Some(c) = d.class {
        let _ = writeln!(t, "class = \"{}\"", c.as_str());
    }
    if let Some(p) = d.crossing.and_then(|p| model.members.get(p)) {
        let _ = writeln!(t, "crossing = {:?}", p.id);
    }
    if let Some([a, b, c]) = d.length_ratios {
        let _ = writeln!(t, "length_ratios = [{a:?}, {b:?}, {c:?}]");
    }
    if let Some(e) = d.end_eccentricity {
        let _ = writeln!(t, "end_eccentricity = \"{}\"", e.as_str());
    }
    if let Some(e) = d.end_restraint {
        let _ = writeln!(t, "end_restraint = \"{}\"", e.as_str());
    }
    for (k, v) in [
        ("width_thickness", d.width_thickness),
        ("thickness", d.thickness),
        ("edge_distance", d.edge_distance),
        ("end_distance", d.end_distance),
        ("spacing", d.spacing),
    ] {
        if let Some(v) = v {
            let _ = writeln!(t, "{k} = {v:?}");
        }
    }
    if let Some(p) = d.plies {
        let _ = writeln!(t, "plies = {p}");
    }
    if let Some(c) = d.connected_leg {
        let _ = writeln!(t, "connected_leg = \"{}\"", c.as_str());
    }
    if let Some(b) = d.bolts {
        let mut parts = vec![
            format!("diameter = {:?}", b.diameter),
            format!("count = {}", b.count),
            format!("shear_planes = {}", b.shear_planes),
        ];
        if let Some(h) = b.hole {
            parts.push(format!("hole = {h:?}"));
        }
        if let Some(v) = b.shear_capacity {
            parts.push(format!("shear_capacity = {v:?}"));
        }
        if let Some(v) = b.ultimate {
            parts.push(format!("ultimate = {v:?}"));
        }
        if let Some(n) = b.holes {
            parts.push(format!("holes = {n}"));
        }
        let _ = writeln!(t, "bolts = {{ {} }}", parts.join(", "));
    }
    t
}

/// Every member of `model` with its forces in `solution`, as a capacity
/// method receives them.
pub fn capacity_inputs(model: &Model, solution: &Solution) -> Vec<CapacityInput> {
    model
        .members
        .iter()
        .enumerate()
        .map(|(k, m)| {
            let s = &model.sections[m.section];
            let f = &solution.members[k];
            let e = &f.end_forces;
            CapacityInput {
                id: m.id.clone(),
                kind: m.kind.as_str().to_owned(),
                shape: s
                    .profile
                    .as_ref()
                    .map_or(String::new(), |p| p.shape.as_str().to_owned()),
                dimensions: s
                    .profile
                    .as_ref()
                    .map_or(Vec::new(), |p| dimensions(&p.shape)),
                area: s.area,
                iy: s.iy,
                iz: s.iz,
                iv: s.i_min,
                wy: s.wy,
                wz: s.wz,
                e: s.e,
                fy: s.yield_strength,
                fu: s.ultimate,
                length: model.length(m),
                k: m.k,
                lk: m.lk,
                design: design_toml(model, &m.design),
                axial: f.axial,
                moments: [e[4], e[5], e[10], e[11]],
                partner_axial: m
                    .design
                    .crossing
                    .and_then(|p| solution.members.get(p))
                    .map(|p| p.axial),
            }
        })
        .collect()
}
