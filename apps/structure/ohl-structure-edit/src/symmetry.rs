// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The body's edits with its mirror symmetry (`[[symmetry]]`): a joint
//! or member added, moved or deleted takes its mirror images with it.
//! Images are found by position (within 1 mm): the joint at the mirrored
//! position is a joint's partner. Without a `[[symmetry]]` each edit is
//! the plain one of [`crate::edits`].
//!
//! Which joints and members a symmetry keeps in step: all, or (with
//! `groups`) the members of those groups and their joints. A joint on a
//! mirror plane stays on it when moved.

use ohl_structures::StructureType;
use toml_edit::{Array, DocumentMut, Item, Table, Value};

use crate::body::{Symmetry, ensure_version, symmetries};
use crate::edits::{self, Removal, array, entry, next_id, point_value};
use crate::generators::SAME;
use crate::geometry::Handle;

/// The joints and members as the edit goes (the structure as read and
/// what the edit added).
struct Work {
    joints: Vec<(String, [f64; 3])>,
    members: Vec<(String, [String; 2])>,
}

impl Work {
    fn new(t: &StructureType) -> Self {
        Self {
            joints: t
                .nodes
                .iter()
                .map(|n| (n.id.clone(), n.point.map(|v| v.si())))
                .collect(),
            members: t
                .members
                .iter()
                .map(|m| (m.id.clone(), m.nodes.clone()))
                .collect(),
        }
    }

    fn at(&self, id: &str) -> Option<[f64; 3]> {
        self.joints.iter().find(|j| j.0 == id).map(|j| j.1)
    }

    fn find(&self, p: [f64; 3]) -> Option<String> {
        self.joints
            .iter()
            .find(|j| close(j.1, p))
            .map(|j| j.0.clone())
    }

    fn joined(&self, a: &str, b: &str) -> bool {
        self.members
            .iter()
            .any(|m| (m.1[0] == a && m.1[1] == b) || (m.1[0] == b && m.1[1] == a))
    }

    /// A new joint at `p` (written); its id.
    fn add_joint(&mut self, doc: &mut DocumentMut, p: [f64; 3]) -> String {
        let like = self.joints.last().map_or("N1", |j| j.0.as_str()).to_owned();
        let id = next_id(self.joints.iter().map(|j| j.0.as_str()), &like);
        let mut table = Table::new();
        table.insert("id", toml_edit::value(id.clone()));
        table.insert("point", point_value(p));
        array(doc, "node").push(table);
        self.joints.push((id.clone(), p));
        id
    }

    /// The joint at `p`, made when there is none.
    fn joint_at(&mut self, doc: &mut DocumentMut, p: [f64; 3]) -> String {
        match self.find(p) {
            Some(j) => j,
            None => self.add_joint(doc, p),
        }
    }

    /// A new member `a`–`b` with the fields of `like` (but its id, joints
    /// and `made_by`); its id.
    fn add_member(
        &mut self,
        doc: &mut DocumentMut,
        a: &str,
        b: &str,
        like: Option<&Table>,
    ) -> String {
        let last = self
            .members
            .last()
            .map_or("M1", |m| m.0.as_str())
            .to_owned();
        let id = next_id(self.members.iter().map(|m| m.0.as_str()), &last);
        let mut table = Table::new();
        table.insert("id", toml_edit::value(id.clone()));
        let mut nodes = Array::new();
        nodes.push(a);
        nodes.push(b);
        table.insert("nodes", Item::Value(Value::Array(nodes)));
        if let Some(like) = like {
            for (k, v) in like.iter() {
                if !matches!(k, "id" | "nodes" | "made_by") {
                    table.insert(k, v.clone());
                }
            }
        }
        array(doc, "member").push(table);
        self.members
            .push((id.clone(), [a.to_owned(), b.to_owned()]));
        id
    }
}

fn close(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= SAME)
}

fn image(p: [f64; 3], s: [f64; 3]) -> [f64; 3] {
    [p[0] * s[0], p[1] * s[1], p[2] * s[2]]
}

/// The images (sign vectors) of the symmetries that apply: their union,
/// closed (mirrored about x and about y: about both too).
fn signs<'a>(applying: impl Iterator<Item = &'a Symmetry>) -> Vec<[f64; 3]> {
    let (mut x, mut y, mut xy) = (false, false, false);
    for s in applying {
        for v in s.mirror.signs() {
            match (v[0] < 0.0, v[1] < 0.0) {
                (true, false) => x = true,
                (false, true) => y = true,
                _ => xy = true,
            }
        }
    }
    if x && y {
        xy = true;
    }
    let mut out = Vec::new();
    if x {
        out.push([-1.0, 1.0, 1.0]);
    }
    if y {
        out.push([1.0, -1.0, 1.0]);
    }
    if xy {
        out.push([-1.0, -1.0, 1.0]);
    }
    out
}

/// The images that apply to a member of `group`.
fn member_signs(syms: &[Symmetry], group: Option<&str>) -> Vec<[f64; 3]> {
    signs(
        syms.iter().filter(|s| {
            s.groups.is_empty() || group.is_some_and(|g| s.groups.iter().any(|x| x == g))
        }),
    )
}

/// The images that apply to joint `id` (a joint on no member of a group
/// a symmetry names: only the symmetries of everything).
fn joint_signs(t: &StructureType, syms: &[Symmetry], id: &str) -> Vec<[f64; 3]> {
    signs(syms.iter().filter(|s| {
        s.groups.is_empty()
            || t.members.iter().any(|m| {
                m.nodes.iter().any(|n| n == id)
                    && m.group.as_ref().is_some_and(|g| s.groups.contains(g))
            })
    }))
}

fn syms(t: &StructureType) -> Vec<Symmetry> {
    symmetries(t).unwrap_or_default()
}

/// Adds a joint at `at` and its mirror images (a joint there already is
/// not doubled); the ids, the joint first.
pub fn add_joint(doc: &mut DocumentMut, t: &StructureType, at: [f64; 3]) -> Vec<String> {
    let syms = syms(t);
    let mut w = Work::new(t);
    let first = w.add_joint(doc, at);
    let mut out = vec![first];
    for s in signs(syms.iter().filter(|s| s.groups.is_empty())) {
        let q = image(at, s);
        if w.find(q).is_none() {
            out.push(w.add_joint(doc, q));
        }
    }
    out
}

/// Adds a member between joints `a` and `b` (like the last member: its
/// section and kind) and its mirror images (joints made where an image
/// has none); the ids, the member first.
pub fn add_member(
    doc: &mut DocumentMut,
    t: &StructureType,
    a: &str,
    b: &str,
) -> Result<Vec<String>, String> {
    let first = edits::add_member(doc, t, a, b)?;
    let syms = syms(t);
    let mut w = Work::new(t);
    w.members
        .push((first.clone(), [a.to_owned(), b.to_owned()]));
    let like = like_last(t);
    let (pa, pb) = (
        w.at(a).ok_or("no such joint")?,
        w.at(b).ok_or("no such joint")?,
    );
    let mut out = vec![first];
    for s in member_signs(&syms, None) {
        let ja = w.joint_at(doc, image(pa, s));
        let jb = w.joint_at(doc, image(pb, s));
        if ja != jb && !w.joined(&ja, &jb) {
            out.push(w.add_member(doc, &ja, &jb, like.as_ref()));
        }
    }
    Ok(out)
}

/// The last member's section and kind (what a new member takes).
fn like_last(t: &StructureType) -> Option<Table> {
    let last = t.members.last()?;
    let mut table = Table::new();
    if let Some(s) = &last.section {
        table.insert("section", toml_edit::value(s.clone()));
    }
    if let Some(k) = &last.kind {
        table.insert("kind", toml_edit::value(k.clone()));
    }
    Some(table)
}

/// The joints mirroring joint `id` (by position), but itself.
pub fn partners(t: &StructureType, id: &str) -> Vec<String> {
    let syms = syms(t);
    let w = Work::new(t);
    let Some(p) = w.at(id) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for s in joint_signs(t, &syms, id) {
        if let Some(j) = w.find(image(p, s))
            && j != id
            && !out.contains(&j)
        {
            out.push(j);
        }
    }
    out
}

/// The members mirroring member `id` (between its joints' images), but
/// itself.
pub fn member_partners(t: &StructureType, id: &str) -> Vec<String> {
    let syms = syms(t);
    let w = Work::new(t);
    let Some(m) = t.members.iter().find(|m| m.id == id) else {
        return Vec::new();
    };
    let (Some(pa), Some(pb)) = (w.at(&m.nodes[0]), w.at(&m.nodes[1])) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for s in member_signs(&syms, m.group.as_deref()) {
        let (Some(ja), Some(jb)) = (w.find(image(pa, s)), w.find(image(pb, s))) else {
            continue;
        };
        if let Some(x) = t.members.iter().find(|x| {
            (x.nodes[0] == ja && x.nodes[1] == jb) || (x.nodes[0] == jb && x.nodes[1] == ja)
        }) && x.id != id
            && !out.contains(&x.id)
        {
            out.push(x.id.clone());
        }
    }
    out
}

/// Moves joint `id` to `at`, its mirror images to theirs. A joint on a
/// mirror plane stays on it.
pub fn move_joint(
    doc: &mut DocumentMut,
    t: &StructureType,
    id: &str,
    at: [f64; 3],
) -> Result<(), String> {
    let syms = syms(t);
    let w = Work::new(t);
    let old = w.at(id).ok_or("no such joint")?;
    let signs = joint_signs(t, &syms, id);
    let mut at = at;
    for s in &signs {
        for k in 0..2 {
            if s[k] < 0.0 && old[k].abs() <= SAME {
                at[k] = 0.0;
            }
        }
    }
    let n = entry(doc, "node", "id", id).ok_or("no such joint")?;
    n.insert("point", point_value(at));
    for s in signs {
        if let Some(j) = w.find(image(old, s))
            && j != id
            && let Some(n) = entry(doc, "node", "id", &j)
        {
            n.insert("point", point_value(image(at, s)));
        }
    }
    Ok(())
}

/// [`edits::move_to`] with the symmetry: a joint (or a point on a joint)
/// takes its images along.
pub fn move_to(
    doc: &mut DocumentMut,
    t: &StructureType,
    h: &Handle,
    at: [f64; 3],
) -> Result<(), String> {
    // A point moves on its own (its offset), its joint stays.
    let joint = match h {
        Handle::Joint(id) => Some(id.clone()),
        _ => None,
    };
    match joint {
        Some(j) if !t.symmetry.is_empty() => move_joint(doc, t, &j, at),
        _ => edits::move_to(doc, t, h, at),
    }
}

/// [`edits::removal`] with the symmetry: a joint or member takes its
/// images (and what goes with them).
pub fn removal(t: &StructureType, h: &Handle) -> Removal {
    let mut r = edits::removal(t, h);
    let images: Vec<Handle> = match h {
        Handle::Joint(id) => partners(t, id).into_iter().map(Handle::Joint).collect(),
        Handle::Member(id) => member_partners(t, id)
            .into_iter()
            .map(Handle::Member)
            .collect(),
        _ => Vec::new(),
    };
    for i in images {
        r.merge(edits::removal(t, &i));
    }
    r
}

/// Make symmetric: every joint and member gets the mirror images it
/// lacks (made by hand; a member's copy takes its fields: section, kind,
/// group…). The joints and members added.
pub fn make_symmetric(doc: &mut DocumentMut, t: &StructureType) -> (usize, usize) {
    let syms = syms(t);
    let mut w = Work::new(t);
    let mut joints = 0;
    for n in &t.nodes {
        let p = n.point.map(|v| v.si());
        for s in joint_signs(t, &syms, &n.id) {
            if w.find(image(p, s)).is_none() {
                w.add_joint(doc, image(p, s));
                joints += 1;
            }
        }
    }
    let mut members = 0;
    for m in &t.members {
        let (Some(pa), Some(pb)) = (w.at(&m.nodes[0]), w.at(&m.nodes[1])) else {
            continue;
        };
        let like = doc
            .get("member")
            .and_then(Item::as_array_of_tables)
            .and_then(|a| {
                a.iter()
                    .find(|x| x.get("id").and_then(Item::as_str) == Some(&m.id))
            })
            .cloned();
        for s in member_signs(&syms, m.group.as_deref()) {
            let ja = w.joint_at(doc, image(pa, s));
            let jb = w.joint_at(doc, image(pb, s));
            if ja != jb && !w.joined(&ja, &jb) {
                w.add_member(doc, &ja, &jb, like.as_ref());
                members += 1;
            }
        }
    }
    if joints + members > 0 {
        ensure_version(doc);
    }
    (joints, members)
}
