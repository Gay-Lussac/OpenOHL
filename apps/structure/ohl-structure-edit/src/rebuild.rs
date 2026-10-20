// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Applying a body generator and building it again: what it builds
//! ([`crate::generators::generate`]) written as ordinary joints and
//! members with its `made_by`, replacing only what it made before.
//!
//! - A joint or member it made before that comes again (same id) keeps
//!   its place in the file and what was set on it (section, kind, group,
//!   support…); only its position or joints change.
//! - One that does not come again goes, unless something else refers to
//!   it: a joint used by a member made by hand (or by another
//!   generator) or carrying a point, a member a point lies along. Those
//!   stay, become hand-made (no `made_by`) and are reported.
//! - Joints and members made by hand are never touched.
//! - A member it now makes in pieces (`body-M3` cut into `body-M3-1`,
//!   `body-M3-2` by a redundant's joint) gives the pieces what was set on
//!   it (section, kind, group…); pieces made whole again give it the
//!   first piece's.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ohl_structures::{LibraryItem, StructureType};
use toml_edit::{Array, DocumentMut, Item, Table, Value};

use crate::body::{Generator, drop_generator, ensure_version, generators, set_generator};
use crate::edits::{array, entry, point_value};
use crate::generators::{Body, generate};

/// What a build did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    /// Joints and members written (new or rebuilt).
    pub joints: usize,
    /// Members written.
    pub members: usize,
    /// Joints and members it made before that went.
    pub removed: usize,
    /// What it made before that stays because something refers to it
    /// (now made by hand), in words.
    pub kept: Vec<String>,
}

impl Report {
    /// In words, for the message line.
    pub fn summary(&self) -> String {
        let mut s = format!("{} joint(s), {} member(s)", self.joints, self.members);
        if self.removed > 0 {
            s.push_str(&format!(", {} removed", self.removed));
        }
        if !self.kept.is_empty() {
            s.push_str(&format!("; kept: {}", self.kept.join("; ")));
        }
        s
    }

    fn add(&mut self, other: Report) {
        self.joints += other.joints;
        self.members += other.members;
        self.removed += other.removed;
        self.kept.extend(other.kept);
    }
}

/// Records generator `g` and builds it (one edit).
pub fn apply(doc: &mut DocumentMut, t: &StructureType, g: &Generator) -> Result<Report, String> {
    let body = generate(t, g)?;
    set_generator(doc, g);
    write(doc, t, &g.id, &body)
}

/// Builds generator `id` again from its record.
pub fn rebuild(doc: &mut DocumentMut, t: &StructureType, id: &str) -> Result<Report, String> {
    let g = generators(t)?
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| format!("no generator {id}"))?;
    let body = generate(t, &g)?;
    write(doc, t, id, &body)
}

/// Builds every generator again, in file order (each on the structure as
/// the ones before left it).
pub fn rebuild_all(doc: &mut DocumentMut, t: &StructureType) -> Result<Report, String> {
    let mut report = Report::default();
    let mut now = t.clone();
    for g in generators(t)? {
        let body = generate(&now, &g)?;
        report.add(write(doc, &now, &g.id, &body)?);
        now = StructureType::parse(Path::new("structure.ohls"), &doc.to_string())
            .map_err(|e| e.message)?;
    }
    Ok(report)
}

/// Removes generator `id` and what it made (but what something else
/// refers to: kept as made by hand, reported).
pub fn remove_generator(doc: &mut DocumentMut, t: &StructureType, id: &str) -> Report {
    let r = write(doc, t, id, &Body::default()).unwrap_or_default();
    drop_generator(doc, id);
    r
}

fn pair(a: &str, b: &str) -> Item {
    let mut nodes = Array::new();
    nodes.push(a);
    nodes.push(b);
    Item::Value(Value::Array(nodes))
}

/// Writes `body` as what generator `id` made: replaces what it made
/// before (see the module).
pub fn write(
    doc: &mut DocumentMut,
    t: &StructureType,
    id: &str,
    body: &Body,
) -> Result<Report, String> {
    let mine = |m: &Option<String>| m.as_deref() == Some(id);
    // Ids taken by joints and members not made by it.
    for j in &body.joints {
        if t.nodes.iter().any(|n| n.id == j.id && !mine(&n.made_by)) {
            return Err(format!(
                "joint {} is there already, not made by {id}: rename it first",
                j.id
            ));
        }
    }
    for m in &body.members {
        if t.members.iter().any(|x| x.id == m.id && !mine(&x.made_by)) {
            return Err(format!(
                "member {} is there already, not made by {id}: rename it first",
                m.id
            ));
        }
    }
    let new_joints: BTreeMap<&str, [f64; 3]> =
        body.joints.iter().map(|j| (j.id.as_str(), j.at)).collect();
    let new_members: BTreeMap<&str, &crate::generators::BodyMember> =
        body.members.iter().map(|m| (m.id.as_str(), m)).collect();
    let mut report = Report {
        joints: body.joints.len(),
        members: body.members.len(),
        ..Default::default()
    };
    // What refers to what it made before: points along its members, and
    // members of others or points on its joints.
    let mut keep_members: BTreeMap<String, String> = BTreeMap::new();
    for p in &t.points {
        if let Some(m) = &p.member
            && t.members.iter().any(|x| &x.id == m && mine(&x.made_by))
            && !new_members.contains_key(m.as_str())
        {
            keep_members.insert(m.clone(), format!("point {} lies along it", p.label));
        }
    }
    let mut users: BTreeMap<String, String> = BTreeMap::new();
    for m in t
        .members
        .iter()
        .filter(|m| !mine(&m.made_by) || keep_members.contains_key(&m.id))
    {
        for n in &m.nodes {
            users
                .entry(n.clone())
                .or_insert_with(|| format!("member {} uses it", m.id));
        }
    }
    for p in &t.points {
        if let Some(n) = &p.node
            && !body.points.iter().any(|(l, _)| l == &p.label)
        {
            users
                .entry(n.clone())
                .or_insert_with(|| format!("point {} is on it", p.label));
        }
    }
    let keep_joints: BTreeMap<String, String> = t
        .nodes
        .iter()
        .filter(|n| mine(&n.made_by) && !new_joints.contains_key(n.id.as_str()))
        .filter_map(|n| Some((n.id.clone(), users.get(&n.id)?.clone())))
        .collect();
    for (j, why) in &keep_joints {
        report.kept.push(format!("joint {j} ({why})"));
    }
    for (m, why) in &keep_members {
        report.kept.push(format!("member {m} ({why})"));
    }
    // Joints: rebuilt in place, kept as hand-made, or gone.
    let mut have = BTreeSet::new();
    if let Some(a) = doc.get_mut("node").and_then(Item::as_array_of_tables_mut) {
        let before = a.len();
        a.retain(|n| {
            let ours = n.get("made_by").and_then(Item::as_str) == Some(id);
            let jid = n.get("id").and_then(Item::as_str).unwrap_or_default();
            !ours || new_joints.contains_key(jid) || keep_joints.contains_key(jid)
        });
        report.removed += before - a.len();
        for n in a.iter_mut() {
            let jid = n
                .get("id")
                .and_then(Item::as_str)
                .unwrap_or_default()
                .to_owned();
            if n.get("made_by").and_then(Item::as_str) != Some(id) {
                continue;
            }
            if let Some(at) = new_joints.get(jid.as_str()) {
                n.insert("point", point_value(*at));
                have.insert(jid);
            } else {
                n.remove("made_by");
            }
        }
    }
    for j in body.joints.iter().filter(|j| !have.contains(&j.id)) {
        let mut n = Table::new();
        n.insert("id", toml_edit::value(j.id.clone()));
        n.insert("point", point_value(j.at));
        n.insert("made_by", toml_edit::value(id));
        array(doc, "node").push(n);
    }
    // Members: the same. What was set on a member that goes, for the
    // pieces it is now made in (or the whole made of its pieces).
    let mut gone: BTreeMap<String, Table> = BTreeMap::new();
    if let Some(a) = doc.get("member").and_then(Item::as_array_of_tables) {
        for m in a.iter() {
            let mid = m.get("id").and_then(Item::as_str).unwrap_or_default();
            if m.get("made_by").and_then(Item::as_str) == Some(id)
                && !new_members.contains_key(mid)
                && !keep_members.contains_key(mid)
            {
                gone.insert(mid.to_owned(), m.clone());
            }
        }
    }
    let mut have = BTreeSet::new();
    if let Some(a) = doc.get_mut("member").and_then(Item::as_array_of_tables_mut) {
        let before = a.len();
        a.retain(|m| {
            let ours = m.get("made_by").and_then(Item::as_str) == Some(id);
            let mid = m.get("id").and_then(Item::as_str).unwrap_or_default();
            !ours || new_members.contains_key(mid) || keep_members.contains_key(mid)
        });
        report.removed += before - a.len();
        for m in a.iter_mut() {
            let mid = m
                .get("id")
                .and_then(Item::as_str)
                .unwrap_or_default()
                .to_owned();
            if m.get("made_by").and_then(Item::as_str) != Some(id) {
                continue;
            }
            if let Some(new) = new_members.get(mid.as_str()) {
                m.insert("nodes", pair(&new.nodes[0], &new.nodes[1]));
                if let Some(g) = &new.group
                    && !m.contains_key("group")
                {
                    m.insert("group", toml_edit::value(g.clone()));
                }
                have.insert(mid);
            } else {
                m.remove("made_by");
            }
        }
    }
    for m in body.members.iter().filter(|m| !have.contains(&m.id)) {
        let mut x = Table::new();
        x.insert("id", toml_edit::value(m.id.clone()));
        x.insert("nodes", pair(&m.nodes[0], &m.nodes[1]));
        if let Some(g) = &m.group {
            x.insert("group", toml_edit::value(g.clone()));
        }
        let whole =
            m.id.rsplit_once('-')
                .filter(|(_, n)| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))
                .map(|(w, _)| w.to_owned());
        let from = whole
            .and_then(|w| gone.get(&w))
            .or_else(|| gone.get(&format!("{}-1", m.id)));
        if let Some(from) = from {
            for (k, v) in from.iter() {
                if !matches!(k, "id" | "nodes" | "made_by") {
                    x.insert(k, v.clone());
                }
            }
        }
        x.insert("made_by", toml_edit::value(id));

        array(doc, "member").push(x);
    }
    // Points put on the tips.
    for (label, joint) in &body.points {
        if let Some(p) = entry(doc, "point", "label", label) {
            p.remove("point");
            p.insert("node", toml_edit::value(joint.clone()));
        }
    }
    ensure_version(doc);
    Ok(report)
}
