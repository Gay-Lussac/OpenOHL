// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The edits of an extension's body (`[[extension.node]]`,
//! `[[extension.member]]`) and its leg extensions (`[[extension.leg]]`),
//! the rule of `ols_structures::extensions`: joints and members added by
//! hand, or the piece built in one go ([`build_piece`]: one panel of
//! legs and bracing under the body's feet). Each is one function on the
//! file's text (one undo step through [`crate::StructureFile::edit`]).

use std::collections::BTreeSet;

use ols_structures::StructureType;
use ols_structures::structures::TypeNode;
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};

use crate::body::ensure_version;
use crate::edits::{entry, next_id, point_value};

/// Distance under which two heights are one, m.
const SAME: f64 = 1e-3;

/// The `[[extension]]` named `name`.
fn extension<'a>(doc: &'a mut DocumentMut, name: &str) -> Result<&'a mut Table, String> {
    entry(doc, "extension", "name", name).ok_or_else(|| format!("no extension {name}"))
}

/// Its array `key` (`node`, `member`, `leg`), made when missing.
fn own<'a>(e: &'a mut Table, key: &str) -> &'a mut ArrayOfTables {
    if e.get(key).and_then(Item::as_array_of_tables).is_none() {
        e.insert(key, Item::ArrayOfTables(ArrayOfTables::new()));
    }
    e.get_mut(key)
        .and_then(Item::as_array_of_tables_mut)
        .expect("made above")
}

/// Removes array `key` of the extension when it is empty.
fn tidy(e: &mut Table, key: &str) {
    if e.get(key)
        .and_then(Item::as_array_of_tables)
        .is_some_and(ArrayOfTables::is_empty)
    {
        e.remove(key);
    }
}

/// The id stem of extension `name`'s own joints and members: `X` and its
/// name's letters and digits (`+3` → `X3`).
pub fn stem(name: &str) -> String {
    let clean: String = name.chars().filter(char::is_ascii_alphanumeric).collect();
    format!("X{clean}")
}

/// Every joint id of `t` (the body's and every extension's).
fn joint_ids(t: &StructureType) -> BTreeSet<String> {
    t.nodes
        .iter()
        .map(|n| n.id.clone())
        .chain(
            t.extensions
                .iter()
                .flat_map(|e| e.nodes.iter().map(|n| n.id.clone())),
        )
        .collect()
}

/// Every member id of `t`.
fn member_ids(t: &StructureType) -> BTreeSet<String> {
    t.members
        .iter()
        .map(|m| m.id.clone())
        .chain(
            t.extensions
                .iter()
                .flat_map(|e| e.members.iter().map(|m| m.id.clone())),
        )
        .collect()
}

/// A joint of extension `name` at `at` (local axes: below the feet, z
/// from the feet less its `dz` to the feet); its id (or why not).
pub fn add_joint(
    doc: &mut DocumentMut,
    t: &StructureType,
    name: &str,
    at: [f64; 3],
) -> Result<String, String> {
    let e = t
        .extension(name)
        .ok_or_else(|| format!("no extension {name}"))?;
    let foot = t.foot_level();
    if at[2] < foot - e.dz.si() - SAME || at[2] > foot + SAME {
        return Err(format!(
            "extension {name}: its joints are below the feet, z from {:.3} to {:.3} m",
            foot - e.dz.si(),
            foot
        ));
    }
    let ids = joint_ids(t);
    let like = e
        .nodes
        .last()
        .map_or(format!("{}-J1", stem(name)), |n| n.id.clone());
    let id = if ids.contains(&like) {
        next_id(ids.iter().map(String::as_str), &like)
    } else {
        like
    };
    ensure_version(doc);
    let mut j = Table::new();
    j.insert("id", toml_edit::value(id.clone()));
    j.insert("point", point_value(at));
    own(extension(doc, name)?, "node").push(j);
    Ok(id)
}

/// A member of extension `name` between joints `a` and `b` (its own or
/// the body's), like its last member (section, kind); its id.
pub fn add_member(
    doc: &mut DocumentMut,
    t: &StructureType,
    name: &str,
    a: &str,
    b: &str,
) -> Result<String, String> {
    let e = t
        .extension(name)
        .ok_or_else(|| format!("no extension {name}"))?;
    if a == b {
        return Err("pick two different joints".into());
    }
    let known = |j: &str| t.nodes.iter().chain(&e.nodes).any(|n| n.id == j);
    for j in [a, b] {
        if !known(j) {
            return Err(format!(
                "joint {j} is not of the body or of extension {name}"
            ));
        }
    }
    if !e.nodes.iter().any(|n| n.id == a || n.id == b) {
        return Err(format!(
            "a member of extension {name} has one end on its own joints at least"
        ));
    }
    if t.members
        .iter()
        .chain(&e.members)
        .any(|m| (m.nodes[0] == a && m.nodes[1] == b) || (m.nodes[0] == b && m.nodes[1] == a))
    {
        return Err(format!("joints {a} and {b} are joined already"));
    }
    let ids = member_ids(t);
    let like = e
        .members
        .last()
        .map_or(format!("{}-M1", stem(name)), |m| m.id.clone());
    let id = if ids.contains(&like) {
        next_id(ids.iter().map(String::as_str), &like)
    } else {
        like
    };
    let mut m = Table::new();
    m.insert("id", toml_edit::value(id.clone()));
    let mut nodes = Array::new();
    nodes.push(a);
    nodes.push(b);
    m.insert("nodes", Item::Value(Value::Array(nodes)));
    if let Some(last) = e.members.last().or(t.members.last()) {
        if let Some(s) = &last.section {
            m.insert("section", toml_edit::value(s.clone()));
        }
        if let Some(k) = &last.kind {
            m.insert("kind", toml_edit::value(k.clone()));
        }
    }
    ensure_version(doc);
    own(extension(doc, name)?, "member").push(m);
    Ok(id)
}

/// Removes joint `id` of extension `name` and its members; the members
/// removed.
pub fn remove_joint(doc: &mut DocumentMut, t: &StructureType, name: &str, id: &str) -> Vec<String> {
    let gone: Vec<String> = t
        .extension(name)
        .map(|e| {
            e.members
                .iter()
                .filter(|m| m.nodes.iter().any(|n| n == id))
                .map(|m| m.id.clone())
                .collect()
        })
        .unwrap_or_default();
    if let Ok(e) = extension(doc, name) {
        own(e, "node").retain(|j| j.get("id").and_then(Item::as_str) != Some(id));
        own(e, "member").retain(|m| {
            !m.get("id")
                .and_then(Item::as_str)
                .is_some_and(|x| gone.iter().any(|g| g == x))
        });
        tidy(e, "node");
        tidy(e, "member");
    }
    gone
}

/// Removes member `id` of extension `name`.
pub fn remove_member(doc: &mut DocumentMut, name: &str, id: &str) {
    if let Ok(e) = extension(doc, name) {
        own(e, "member").retain(|m| m.get("id").and_then(Item::as_str) != Some(id));
        tidy(e, "member");
    }
}

/// Field `field` (`support` of a joint, `section` or `kind` of a member)
/// of table `id` of the extension's `key` (`node`, `member`); none
/// removes it.
pub fn set_field(
    doc: &mut DocumentMut,
    name: &str,
    key: &str,
    id: &str,
    field: &str,
    value: Option<&str>,
) -> Result<(), String> {
    let e = extension(doc, name)?;
    let t = own(e, key)
        .iter_mut()
        .find(|x| x.get("id").and_then(Item::as_str) == Some(id))
        .ok_or_else(|| format!("extension {name}: no {key} {id}"))?;
    match value {
        Some(v) => {
            t.insert(field, toml_edit::value(v));
        }
        None => {
            t.remove(field);
        }
    }
    Ok(())
}

/// Leg `leg` (A–D or a joint id) of extension `name` made `dz` m longer
/// (none: as the others).
pub fn set_leg(
    doc: &mut DocumentMut,
    name: &str,
    leg: &str,
    dz: Option<f64>,
) -> Result<(), String> {
    if dz.is_some_and(|d| d.is_nan() || d <= 0.0) {
        return Err("a leg extension above 0".into());
    }
    ensure_version(doc);
    let e = extension(doc, name)?;
    let legs = own(e, "leg");
    let at = legs
        .iter()
        .position(|l| l.get("leg").and_then(Item::as_str) == Some(leg));
    match (at, dz) {
        (Some(k), Some(d)) => {
            if let Some(l) = legs.get_mut(k) {
                l.insert("dz", toml_edit::value(format!("{d:.3} m")));
            }
        }
        (None, Some(d)) => {
            let mut l = Table::new();
            l.insert("leg", toml_edit::value(leg));
            l.insert("dz", toml_edit::value(format!("{d:.3} m")));
            legs.push(l);
        }
        (Some(k), None) => {
            legs.remove(k);
        }
        (None, None) => {}
    }
    tidy(e, "leg");
    Ok(())
}

/// The bracing of a piece's faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PieceBracing {
    /// Legs only.
    None,
    /// One diagonal per face.
    Single,
    /// Both diagonals.
    #[default]
    X,
}

impl PieceBracing {
    /// Every choice, as offered.
    pub const ALL: [PieceBracing; 3] = [PieceBracing::None, PieceBracing::Single, PieceBracing::X];

    /// In words.
    pub fn name(self) -> &'static str {
        match self {
            PieceBracing::None => "legs only",
            PieceBracing::Single => "single diagonal",
            PieceBracing::X => "X bracing",
        }
    }
}

/// What [`build_piece`] wrote.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Piece {
    /// Joints written.
    pub joints: usize,
    /// Members written.
    pub members: usize,
    /// Joints and members of the extension it replaced.
    pub replaced: usize,
}

/// Builds extension `name`'s piece in one go, replacing its own joints
/// and members: under every foot of the body (its supported joints, else
/// its lowest joints) a joint the extension's `dz` lower along the foot's
/// leg (the steepest member rising from it; none: straight down), with
/// the foot's support (else pinned); a leg member from each new joint to
/// its foot (group `{stem}-leg`), the faces between neighbouring feet
/// (around the centre) braced as asked (group `{stem}-bracing`), a
/// horizontal at the bottom of each face when `horizontals`. The body's
/// feet then stand on it (the rule drops their supports).
pub fn build_piece(
    doc: &mut DocumentMut,
    t: &StructureType,
    name: &str,
    bracing: PieceBracing,
    horizontals: bool,
) -> Result<Piece, String> {
    let e = t
        .extension(name)
        .ok_or_else(|| format!("no extension {name}"))?;
    let dz = e.dz.si();
    if dz.is_nan() || dz <= 0.0 {
        return Err(format!(
            "extension {name}: give it a dz above 0 first (the piece's height)"
        ));
    }
    let at = |n: &TypeNode| n.point.map(|v| v.si());
    let supported: Vec<&TypeNode> = t.nodes.iter().filter(|n| n.support.is_some()).collect();
    let feet: Vec<&TypeNode> = if supported.is_empty() {
        let low = t
            .nodes
            .iter()
            .map(|n| at(n)[2])
            .fold(f64::INFINITY, f64::min);
        t.nodes.iter().filter(|n| at(n)[2] <= low + SAME).collect()
    } else {
        supported
    };
    if feet.is_empty() {
        return Err("the body has no joints: build the body first (Body)".into());
    }
    // Around the centre, so neighbours make the faces.
    let mut feet = feet;
    feet.sort_by(|a, b| {
        let (p, q) = (at(a), at(b));
        p[1].atan2(p[0]).total_cmp(&q[1].atan2(q[0]))
    });
    let s = stem(name);
    let others: BTreeSet<String> = joint_ids(t)
        .into_iter()
        .filter(|j| !e.nodes.iter().any(|n| &n.id == j))
        .collect();
    let other_members: BTreeSet<String> = member_ids(t)
        .into_iter()
        .filter(|m| !e.members.iter().any(|x| &x.id == m))
        .collect();
    let fresh = |taken: &BTreeSet<String>, like: String| {
        if taken.contains(&like) {
            next_id(taken.iter().map(String::as_str), &like)
        } else {
            like
        }
    };
    let mut joints = Vec::new();
    for (k, f) in feet.iter().enumerate() {
        let p = at(f);
        // The leg: the steepest member rising from the foot.
        let leg = t
            .members
            .iter()
            .filter_map(|m| {
                let other = if m.nodes[0] == f.id {
                    &m.nodes[1]
                } else if m.nodes[1] == f.id {
                    &m.nodes[0]
                } else {
                    return None;
                };
                let o = t.nodes.iter().find(|n| &n.id == other).map(at)?;
                let d = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
                let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                (d[2] < -SAME).then_some((-d[2] / len, d))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|x| x.1);
        let foot_z = t.foot_level();
        let drop = p[2] - (foot_z - dz);
        let below = match leg {
            Some(d) => [0, 1, 2].map(|i| p[i] + d[i] * drop / -d[2]),
            None => [p[0], p[1], foot_z - dz],
        };
        let id = fresh(&others, format!("{s}-J{}", k + 1));
        let support = f
            .support
            .as_ref()
            .and_then(|v| v.as_str())
            .unwrap_or("pinned")
            .to_owned();
        joints.push((id, below, support, f.id.clone()));
    }
    let mut members: Vec<(String, String, String)> = Vec::new();
    let group_leg = format!("{s}-leg");
    let group_brace = format!("{s}-bracing");
    for (id, _, _, foot) in &joints {
        members.push((id.clone(), foot.clone(), group_leg.clone()));
    }
    let n = joints.len();
    if n >= 2 {
        let faces = if n == 2 { 1 } else { n };
        for i in 0..faces {
            let j = (i + 1) % n;
            let (bi, ti) = (&joints[i].0, &joints[i].3);
            let (bj, tj) = (&joints[j].0, &joints[j].3);
            match bracing {
                PieceBracing::None => {}
                PieceBracing::Single => members.push((bi.clone(), tj.clone(), group_brace.clone())),
                PieceBracing::X => {
                    members.push((bi.clone(), tj.clone(), group_brace.clone()));
                    members.push((bj.clone(), ti.clone(), group_brace.clone()));
                }
            }
            if horizontals {
                members.push((bi.clone(), bj.clone(), group_brace.clone()));
            }
        }
    }
    let section = t.members.iter().rev().find_map(|m| m.section.clone());
    let replaced = e.nodes.len() + e.members.len();
    ensure_version(doc);
    let ext = extension(doc, name)?;
    ext.remove("node");
    ext.remove("member");
    let nodes = own(ext, "node");
    for (id, p, support, _) in &joints {
        let mut j = Table::new();
        j.insert("id", toml_edit::value(id.clone()));
        j.insert("point", point_value(*p));
        j.insert("support", toml_edit::value(support.clone()));
        nodes.push(j);
    }
    let list = own(ext, "member");
    let mut taken = other_members;
    for (k, (a, b, group)) in members.iter().enumerate() {
        let id = fresh(&taken, format!("{s}-M{}", k + 1));
        taken.insert(id.clone());
        let mut m = Table::new();
        m.insert("id", toml_edit::value(id));
        let mut ends = Array::new();
        ends.push(a.as_str());
        ends.push(b.as_str());
        m.insert("nodes", Item::Value(Value::Array(ends)));
        if let Some(sec) = &section {
            m.insert("section", toml_edit::value(sec.clone()));
        }
        m.insert("group", toml_edit::value(group.clone()));
        list.push(m);
    }
    Ok(Piece {
        joints: joints.len(),
        members: members.len(),
        replaced,
    })
}
