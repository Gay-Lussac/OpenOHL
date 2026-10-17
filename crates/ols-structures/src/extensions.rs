// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Height extensions (`[[extension]]`) and their bodies (decisions 17 and
//! 20 of *Building structures*).
//!
//! An extension lengthens the structure below its reference level (the
//! feet, `foot`): placed, everything above the feet rises by its `dz`. At
//! levels 1 and 2 a name and a `dz` are enough. At level 3 an extension
//! may carry its own **body** — the piece between the ground and the
//! feet, its joints and members — and **leg extensions**, one leg longer
//! than the others (format version 3):
//!
//! ```toml
//! [[extension]]
//! name = "+3"
//! dz = "3 m"
//!
//! [[extension.node]]          # the piece, below the feet: z from foot − dz to foot
//! id = "E3-1"
//! point = ["-3.5 m", "-3.5 m", "-3 m"]
//! support = "pinned"
//!
//! [[extension.member]]        # joints of the piece or of the body, by id
//! id = "E3-M1"
//! nodes = ["E3-1", "body-L0-3"]
//! section = "L-61x7"
//!
//! [[extension.leg]]           # one leg longer: leg A, B, C, D or a joint id
//! leg = "A"
//! dz = "1 m"
//! ```
//!
//! **The rule** ([`StructureType::extended`]), in the structure's own axes
//! (the body is not moved; placed, the whole rises by `dz`):
//!
//! 1. A **body extension** adds its joints and members to the body's. A
//!    body joint that a member of the extension reaches stands on the
//!    extension: its support (and spring) is dropped; the extension's own
//!    joints carry the supports written on them.
//! 2. A **leg extension** lowers one support joint by its `dz`: leg A, B,
//!    C, D is the joint in the quarter x > 0, y > 0 (A), x < 0, y > 0
//!    (B), x < 0, y < 0 (C), x > 0, y < 0 (D) — among the supported joints
//!    when there are any, else among all joints — the lowest one there
//!    (two equally low: name the joint by its id). The joint slides along
//!    its **leg member** (of the members ending at it, the one rising most
//!    steeply to a joint above) until it is `dz` lower, so the leg member
//!    is stretched along its own line; with no member rising from it, it
//!    moves straight down. Every member ending at it follows it.

use std::collections::BTreeSet;

use ols_core::units::Length;
use serde::Deserialize;

use crate::structures::{StructureType, TypeMember, TypeNode};

/// Distance under which two positions or heights are one, m.
const SAME: f64 = 1e-3;

/// The legs named by a letter, with the signs of their quarter (x, y).
pub const LEGS: [(&str, [f64; 2]); 4] = [
    ("A", [1.0, 1.0]),
    ("B", [-1.0, 1.0]),
    ("C", [-1.0, -1.0]),
    ("D", [1.0, -1.0]),
];

/// A named height extension.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extension {
    /// Name, e.g. `"+3"`.
    pub name: String,
    /// Height added to every attachment point.
    pub dz: Length,
    /// Its body's joints, below the feet (level 3).
    #[serde(rename = "node", default)]
    pub nodes: Vec<TypeNode>,
    /// Its body's members, between its joints and the body's (level 3).
    #[serde(rename = "member", default)]
    pub members: Vec<TypeMember>,
    /// Legs made longer (level 3).
    #[serde(rename = "leg", default)]
    pub legs: Vec<LegExtension>,
}

/// One leg made longer than the others.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegExtension {
    /// `"A"`, `"B"`, `"C"`, `"D"` (by quarter) or a joint id.
    pub leg: String,
    /// How much lower its support goes.
    pub dz: Length,
}

impl Extension {
    /// A name and a `dz` only (as at levels 1 and 2).
    pub fn plain(name: &str, dz: f64) -> Self {
        Self {
            name: name.to_owned(),
            dz: Length::from_meters(dz),
            nodes: Vec::new(),
            members: Vec::new(),
            legs: Vec::new(),
        }
    }

    /// It has joints or members of its own.
    pub fn has_body(&self) -> bool {
        !self.nodes.is_empty() || !self.members.is_empty()
    }

    /// It changes the structure model: a body or leg extensions.
    pub fn changes_model(&self) -> bool {
        self.has_body() || !self.legs.is_empty()
    }
}

/// The joints and members of a structure with an extension, in its own
/// axes ([`StructureType::extended`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtendedBody {
    /// The body's joints then the extension's; supports and positions as
    /// the rule makes them.
    pub nodes: Vec<TypeNode>,
    /// The body's members then the extension's.
    pub members: Vec<TypeMember>,
    /// Ids of the extension's own joints and members.
    pub added: BTreeSet<String>,
    /// Body joints standing on the extension (their supports dropped).
    pub unsupported: Vec<String>,
    /// Joints lowered by a leg extension: joint, leg as written.
    pub lowered: Vec<(String, String)>,
}

impl StructureType {
    /// The extension `name`.
    pub fn extension(&self, name: &str) -> Option<&Extension> {
        self.extensions.iter().find(|e| e.name == name)
    }

    /// The joints and members with extension `extension` (none: the body
    /// as written), by the rule of [`mod@crate::extensions`]; an extension
    /// without a body or legs gives the body as written.
    pub fn extended(&self, extension: Option<&str>) -> Result<ExtendedBody, String> {
        let mut out = ExtendedBody {
            nodes: self.nodes.clone(),
            members: self.members.clone(),
            ..ExtendedBody::default()
        };
        let Some(name) = extension else {
            return Ok(out);
        };
        let e = self
            .extension(name)
            .ok_or_else(|| format!("structure type {}: no extension \"{name}\"", self.id))?;
        if e.has_body() {
            let reached: BTreeSet<&str> = e
                .members
                .iter()
                .flat_map(|m| m.nodes.iter().map(String::as_str))
                .collect();
            for n in out.nodes.iter_mut() {
                if reached.contains(n.id.as_str()) && n.support.is_some() {
                    n.support = None;
                    n.spring = None;
                    out.unsupported.push(n.id.clone());
                }
            }
            out.added.extend(e.nodes.iter().map(|n| n.id.clone()));
            out.added.extend(e.members.iter().map(|m| m.id.clone()));
            out.nodes.extend(e.nodes.iter().cloned());
            out.members.extend(e.members.iter().cloned());
        }
        for leg in &e.legs {
            let bad = |m: String| format!("extension {name}: leg {}: {m}", leg.leg);
            let k = leg_joint(&out.nodes, &leg.leg).map_err(bad)?;
            let at = out.nodes[k].point.map(|v| v.si());
            let id = out.nodes[k].id.clone();
            if out.lowered.iter().any(|(j, _)| *j == id) {
                return Err(bad(format!("joint {id} is lowered by another leg")));
            }
            let pos = |j: &str| {
                out.nodes
                    .iter()
                    .find(|n| n.id == j)
                    .map(|n| n.point.map(|v| v.si()))
            };
            // The leg member: the steepest member rising from the joint.
            let along = out
                .members
                .iter()
                .filter_map(|m| {
                    let other = match (&m.nodes[0], &m.nodes[1]) {
                        (a, b) if *a == id => b,
                        (a, b) if *b == id => a,
                        _ => return None,
                    };
                    let o = pos(other)?;
                    let d = [at[0] - o[0], at[1] - o[1], at[2] - o[2]];
                    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                    (d[2] < -SAME).then_some((-d[2] / len, d))
                })
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .map(|x| x.1);
            let drop = leg.dz.si();
            let to = match along {
                Some(d) => [0, 1, 2].map(|i| at[i] + d[i] * drop / -d[2]),
                None => [at[0], at[1], at[2] - drop],
            };
            out.nodes[k].point = to.map(Length::from_meters);
            out.lowered.push((id, leg.leg.clone()));
        }
        Ok(out)
    }

    /// The extensions as read: ids unique and known, the piece below the
    /// feet, legs known and lowered.
    pub(crate) fn check_extensions(&self) -> Result<(), String> {
        let foot = self.foot_level();
        for e in &self.extensions {
            let bad = |m: String| Err(format!("extension {}: {m}", e.name));
            let mut joints: BTreeSet<&str> = self.nodes.iter().map(|n| n.id.as_str()).collect();
            for n in &e.nodes {
                if n.id.is_empty() || !joints.insert(n.id.as_str()) {
                    return bad(format!("joint \"{}\": empty or given twice", n.id));
                }
                let z = n.point[2].si();
                if z < foot - e.dz.si() - SAME || z > foot + SAME {
                    return bad(format!(
                        "joint {}: its piece is below the feet, z from {:.3} to {:.3} m",
                        n.id,
                        foot - e.dz.si(),
                        foot
                    ));
                }
            }
            let mut members: BTreeSet<&str> = self.members.iter().map(|m| m.id.as_str()).collect();
            for m in &e.members {
                if m.id.is_empty() || !members.insert(m.id.as_str()) {
                    return bad(format!("member \"{}\": empty or given twice", m.id));
                }
                for n in &m.nodes {
                    if !joints.contains(n.as_str()) {
                        return bad(format!("member {}: no joint \"{n}\"", m.id));
                    }
                }
                if m.nodes[0] == m.nodes[1] {
                    return bad(format!("member {}: both ends at the same joint", m.id));
                }
            }
            if e.has_body() && !above_zero(e.dz.si()) {
                return bad("a body below the feet needs dz above 0".into());
            }
            let mut legs = BTreeSet::new();
            for l in &e.legs {
                if !LEGS.iter().any(|x| x.0 == l.leg) && !joints.contains(l.leg.as_str()) {
                    return bad(format!(
                        "leg \"{}\": A, B, C, D or the id of a joint",
                        l.leg
                    ));
                }
                if !above_zero(l.dz.si()) {
                    return bad(format!("leg {}: dz above 0", l.leg));
                }
                if !legs.insert(l.leg.as_str()) {
                    return bad(format!("leg {}: given twice", l.leg));
                }
            }
        }
        Ok(())
    }
}

/// The joint leg `leg` names among `nodes`: a joint id, or a letter's
/// quarter (supported joints first), the lowest there.
fn leg_joint(nodes: &[TypeNode], leg: &str) -> Result<usize, String> {
    if let Some(k) = nodes.iter().position(|n| n.id == leg) {
        return Ok(k);
    }
    let sign = LEGS
        .iter()
        .find(|x| x.0 == leg)
        .map(|x| x.1)
        .ok_or("A, B, C, D or the id of a joint")?;
    let supported = nodes.iter().any(|n| n.support.is_some());
    let mut inside: Vec<(usize, f64)> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| !supported || n.support.is_some())
        .filter(|(_, n)| n.point[0].si() * sign[0] > SAME && n.point[1].si() * sign[1] > SAME)
        .map(|(k, n)| (k, n.point[2].si()))
        .collect();
    inside.sort_by(|a, b| a.1.total_cmp(&b.1));
    match inside.as_slice() {
        [] => Err(format!(
            "no {}joint in its quarter",
            if supported { "supported " } else { "" }
        )),
        [a, b, ..] if b.1 - a.1 <= SAME => Err(format!(
            "joints {} and {} are equally low: name the joint",
            nodes[a.0].id, nodes[b.0].id
        )),
        [a, ..] => Ok(a.0),
    }
}

/// Above 0 (not NaN).
fn above_zero(v: f64) -> bool {
    v > 0.0
}
