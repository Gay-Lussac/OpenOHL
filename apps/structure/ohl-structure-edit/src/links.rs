// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Attachment points linked to joints in a structure file (2026-10-21):
//! link (or relink) a point to an attach joint at an offset, unlink it,
//! give a joint its kind, insert a joint on a member. Each one edit of
//! the file (one undo step of the application). The file keys stay
//! `[[point]]` `node` and `offset`, `[[node]]` `kind`; a file is raised
//! to version 6 when an offset or a kind is written.
//!
//! Only **attach joints** take a link (owner decision 2026-10-21): a
//! joint without a kind is made one in the Joints table (its kind) first.
//! A point cannot be put on a member: [`insert_joint`] puts a joint on
//! it (the member split there, continuous), then the point is linked.

use ohl_structures::{JointKind, JointSource, StructureType};
use toml_edit::{DocumentMut, Item};

use crate::edits::{array, entry, next_id, point_value, raise_version};

/// The version that reads a joint's `kind` and a point's `offset`.
pub const LINKS_VERSION: i64 = 6;

/// The version that reads a point standing alone (`unlinked = true`,
/// [`ohl_structures::UNLINKED_POINT_VERSION`]).
pub const UNLINKED_VERSION: i64 = ohl_structures::UNLINKED_POINT_VERSION as i64;

/// How a point linked to a joint it is not on goes there (owner
/// 2026-11-15: asked whenever it is farther than
/// [`ohl_structures::ON_JOINT`] from the joint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkHow {
    /// The point moved onto the joint (no offset): what hangs from it
    /// moves with it.
    Onto,
    /// The point kept where it is, at an offset from the joint (a rigid
    /// link whose moment the analysis applies at the joint).
    KeepPosition,
}

/// From joint `joint` to attachment point `label` of `t`, where the point
/// is now, local axes, m (rounding below [`ohl_structures::SAME_JOINT`]
/// taken off). None: no such point, or no joint `joint` among the
/// structure's `[[node]]`s.
pub fn offset_from(t: &StructureType, label: &str, joint: &str) -> Option<[f64; 3]> {
    let p = t.points.iter().find(|p| p.label == label)?.position();
    let j = t.joint(joint).filter(|j| j.source == JointSource::Node)?;
    Some([0, 1, 2].map(|k| {
        let v = p[k].si() - j.position[k];
        if v.abs() < ohl_structures::SAME_JOINT {
            0.0
        } else {
            v
        }
    }))
}

/// Whether linking point `label` to joint `joint` asks how (the point
/// farther than [`ohl_structures::ON_JOINT`] from it): the offset it
/// would keep. None: on the joint (linked onto it, no question), or no
/// such point or joint.
pub fn needs_asking(t: &StructureType, label: &str, joint: &str) -> Option<[f64; 3]> {
    let o = offset_from(t, label, joint)?;
    (o.iter().map(|v| v * v).sum::<f64>().sqrt() > ohl_structures::ON_JOINT).then_some(o)
}

/// Links (or relinks, or moves) attachment point `label` to attach joint
/// `joint`, `how` chosen: onto it, or kept where it is at an offset.
/// Within [`ohl_structures::ON_JOINT`] it goes onto the joint whatever
/// `how` says. The strings hanging from it follow (their top is the
/// point). Refused as [`link_point`] refuses.
pub fn relink_point(
    doc: &mut DocumentMut,
    t: &StructureType,
    label: &str,
    joint: &str,
    how: LinkHow,
) -> Result<(), String> {
    if !t.points.iter().any(|p| p.label == label) {
        return Err(format!("no attachment point {label}"));
    }
    if offset_from(t, label, joint).is_none() {
        return Err(
            not_attach(t, joint).unwrap_or_else(|| format!("{joint} is no joint of the structure"))
        );
    }
    let o = match (how, needs_asking(t, label, joint)) {
        (LinkHow::KeepPosition, Some(o)) => o,
        _ => [0.0; 3],
    };
    link_point(doc, t, label, joint, o)
}

/// The attachment points linked to joint `joint` (a `[[node]]`), in
/// order.
pub fn points_on(t: &StructureType, joint: &str) -> Vec<String> {
    if !t.nodes.iter().any(|n| n.id == joint) {
        return Vec::new();
    }
    t.points
        .iter()
        .filter(|p| !p.unlinked && p.joint == joint)
        .map(|p| p.label.clone())
        .collect()
}

/// Why joint `joint` of `t` takes no link (none: an attach joint).
pub fn not_attach(t: &StructureType, joint: &str) -> Option<String> {
    match t.joint(joint) {
        None => Some(format!("no joint {joint}")),
        Some(j) if j.kind == Some(JointKind::Attach) => None,
        Some(j) if j.kind == Some(JointKind::Support) => Some(format!(
            "joint {joint} is a support joint (strings hang from attach joints)"
        )),
        Some(_) => Some(format!("joint {joint}: {NOT_ATTACH}")),
    }
}

pub use ohl_structures::NOT_ATTACH;

/// Links attachment point `label` to attach joint `joint` at `offset`
/// (local axes, m; zero: on it): its `node`, its `offset`, its own
/// `point` gone. Refused: no such point, or a joint that is not an
/// attach joint ([`not_attach`]).
pub fn link_point(
    doc: &mut DocumentMut,
    t: &StructureType,
    label: &str,
    joint: &str,
    offset: [f64; 3],
) -> Result<(), String> {
    if !t.points.iter().any(|p| p.label == label) {
        return Err(format!("no attachment point {label}"));
    }
    if let Some(why) = not_attach(t, joint) {
        return Err(why);
    }
    let j = t.joint(joint).ok_or_else(|| format!("no joint {joint}"))?;
    // A point and a joint are separate objects (owner 2026-11-15): another
    // point's own place is no joint to link to.
    if j.source == JointSource::Point {
        return Err(format!(
            "{joint} is attachment point {joint}'s own place, not a joint of the structure: link \
             to a joint (a joint of the body, or one added)"
        ));
    }
    let p = entry(doc, "point", "label", label).ok_or("no such point in the file")?;
    let left = p
        .get("node")
        .and_then(Item::as_str)
        .filter(|n| *n != joint)
        .map(str::to_owned);
    for k in ["point", "node", "member", "at", "offset", "unlinked"] {
        p.remove(k);
    }
    p.insert("node", toml_edit::value(joint));
    let shifted = offset != [0.0; 3];
    if shifted {
        p.insert("offset", point_value(offset));
    }
    // The joint the point left stays an attach joint (its kind was worked
    // out from the point on it): a point can be moved back.
    let mut kind = false;
    if let Some(n) = left.and_then(|id| entry(doc, "node", "id", &id))
        && !n.contains_key("kind")
        && n.get("support").is_none()
    {
        n.insert("kind", toml_edit::value(JointKind::Attach.as_str()));
        kind = true;
    }
    if shifted || kind {
        raise_version(doc, LINKS_VERSION);
    }
    Ok(())
}

/// Unlinks attachment point `label`: it keeps its position, standing
/// alone, linked to no joint (`unlinked = true`, version 17; owner
/// 2026-11-15) — its strings' loads reach no member until it is linked
/// again.
pub fn unlink_point(doc: &mut DocumentMut, t: &StructureType, label: &str) -> Result<(), String> {
    let at = t
        .points
        .iter()
        .find(|p| p.label == label)
        .ok_or_else(|| format!("no attachment point {label}"))?
        .position()
        .map(|v| v.si());
    let p = entry(doc, "point", "label", label).ok_or("no such point in the file")?;
    for k in ["node", "member", "at", "offset"] {
        p.remove(k);
    }
    p.insert("point", point_value(at));
    p.insert("unlinked", toml_edit::value(true));
    raise_version(doc, UNLINKED_VERSION);
    Ok(())
}

/// Attachment point `label` moved to `at` (local axes, m): on a joint, it
/// stays linked to it, its offset the new distance from it (the joint
/// does not move); a point that is its own joint moves.
pub fn move_point(
    doc: &mut DocumentMut,
    t: &StructureType,
    label: &str,
    at: [f64; 3],
) -> Result<(), String> {
    let p = t
        .points
        .iter()
        .find(|p| p.label == label)
        .ok_or_else(|| format!("no attachment point {label}"))?;
    let entry = entry(doc, "point", "label", label).ok_or("no such point in the file")?;
    let Some(node) = &p.node else {
        entry.insert("point", point_value(at));
        return Ok(());
    };
    let j = t
        .nodes
        .iter()
        .find(|n| &n.id == node)
        .ok_or_else(|| format!("no joint {node}"))?
        .point
        .map(|v| v.si());
    let o = [0, 1, 2].map(|k| {
        let v = at[k] - j[k];
        if v.abs() < ohl_structures::SAME_JOINT {
            0.0
        } else {
            v
        }
    });
    entry.remove("offset");
    if o != [0.0; 3] {
        entry.insert("offset", point_value(o));
        raise_version(doc, LINKS_VERSION);
    }
    Ok(())
}

/// Inserts a joint on member `member` at fraction `at` of its length from
/// its first joint (strictly between 0 and 1): a joint of kind attach
/// there, the member split into two pieces that stay continuous — the
/// same fields both sides (section, kind, orientation…: a rigid
/// connection, not a hinge), each buckling over the whole member's
/// length (its `lk`, else k × its length, written as `lk`;
/// [`ohl_structures::on_members`]). The first piece keeps the member's
/// id, the second takes `<id>-2` (the next free). The joint's id.
pub fn insert_joint(
    doc: &mut DocumentMut,
    t: &StructureType,
    member: &str,
    at: f64,
) -> Result<String, String> {
    if !(at > 0.0 && at < 1.0) {
        return Err("a joint goes strictly between the member's ends (0 < at < 1)".into());
    }
    let m = t
        .members
        .iter()
        .find(|m| m.id == member)
        .ok_or_else(|| format!("no member {member}"))?;
    let pos = |id: &str| {
        t.nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.point.map(|v| v.si()))
            .ok_or_else(|| format!("no joint {id}"))
    };
    let (a, b) = (pos(&m.nodes[0])?, pos(&m.nodes[1])?);
    let position = [0, 1, 2].map(|k| a[k] + at * (b[k] - a[k]));
    let like = t.nodes.last().map_or("N1", |n| n.id.as_str());
    let id = next_id(t.nodes.iter().map(|n| n.id.as_str()), like);
    let joints: Vec<(&str, [f64; 3])> = t
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.point.map(|v| v.si())))
        .chain(std::iter::once((id.as_str(), position)))
        .collect();
    let rows: Vec<ohl_structures::on_members::MemberRow> = t
        .members
        .iter()
        .map(|x| ohl_structures::on_members::MemberRow {
            id: &x.id,
            nodes: [&x.nodes[0], &x.nodes[1]],
            k: x.k,
            has_lk: x.lk.is_some(),
        })
        .collect();
    let split = ohl_structures::on_members::split_members(
        &joints,
        &rows,
        &[(member.to_owned(), at, id.clone())],
    )
    .pop()
    .ok_or("nothing to split")?;
    let mut node = toml_edit::Table::new();
    node.insert("id", toml_edit::value(id.clone()));
    node.insert("point", point_value(position));
    node.insert("kind", toml_edit::value(JointKind::Attach.as_str()));
    array(doc, "node").push(node);
    let first = entry(doc, "member", "id", member).ok_or("no such member in the file")?;
    if let Some(lk) = split.lk {
        first.insert("lk", toml_edit::value(format!("{lk:?} m")));
    }
    let mut pieces = Vec::new();
    for (k, piece) in split.pieces.iter().enumerate() {
        let mut nodes = toml_edit::Array::new();
        nodes.push(piece.nodes[0].as_str());
        nodes.push(piece.nodes[1].as_str());
        if k == 0 {
            first.insert("nodes", Item::Value(toml_edit::Value::Array(nodes)));
            continue;
        }
        let mut p = first.clone();
        p.insert("id", toml_edit::value(piece.id.clone()));
        p.insert("nodes", Item::Value(toml_edit::Value::Array(nodes)));
        p.decor_mut().clear();
        pieces.push(p);
    }
    for p in pieces {
        array(doc, "member").push(p);
    }
    // The member's end at its second joint is now the last piece's: its
    // connection and those onto it there follow (version 9).
    if let Some(last) = split.pieces.last().filter(|p| p.id != member) {
        let far = m.nodes[1].as_str();
        if let Some(a) = doc
            .get_mut("connection")
            .and_then(Item::as_array_of_tables_mut)
        {
            for c in a.iter_mut() {
                if c.get("joint").and_then(Item::as_str) != Some(far) {
                    continue;
                }
                for f in ["member", "onto"] {
                    if c.get(f).and_then(Item::as_str) == Some(member) {
                        c.insert(f, toml_edit::value(last.id.clone()));
                    }
                }
            }
        }
        // And the splices there that join it (2026-11-05).
        if let Some(a) = doc.get_mut("splice").and_then(Item::as_array_of_tables_mut) {
            for sp in a.iter_mut() {
                if sp.get("joint").and_then(Item::as_str) != Some(far) {
                    continue;
                }
                if let Some(list) = sp.get_mut("members").and_then(Item::as_array_mut) {
                    for v in list.iter_mut() {
                        if v.as_str() == Some(member) {
                            *v = toml_edit::Value::from(last.id.clone());
                        }
                    }
                }
            }
        }
    }
    raise_version(doc, LINKS_VERSION);
    Ok(id)
}

/// The file `doc` as read now: points written along a member (`member`,
/// `at`, files written before 2026-10-21) put on a joint inserted there,
/// the member split ([`ohl_structures::on_members`]) — as the structure
/// is read, so the file and the structure agree. Whether anything
/// changed (the file is then written so when next saved).
pub fn insert_joints_on_members(doc: &mut DocumentMut) -> Result<bool, String> {
    let table: toml::Table = toml::from_str(&doc.to_string()).map_err(|e| e.to_string())?;
    let Some(plan) = ohl_structures::on_members::plan_for_table(&table)? else {
        return Ok(false);
    };
    for j in &plan.joints {
        let mut node = toml_edit::Table::new();
        node.insert("id", toml_edit::value(j.id.clone()));
        node.insert("point", exact_point(j.position));
        node.insert("kind", toml_edit::value(JointKind::Attach.as_str()));
        array(doc, "node").push(node);
    }
    let mut more = Vec::new();
    for s in &plan.splits {
        let Some(first) = entry(doc, "member", "id", &s.member) else {
            continue;
        };
        if let Some(lk) = s.lk {
            first.insert("lk", toml_edit::value(format!("{lk:?} m")));
        }
        for (k, piece) in s.pieces.iter().enumerate() {
            let mut nodes = toml_edit::Array::new();
            nodes.push(piece.nodes[0].as_str());
            nodes.push(piece.nodes[1].as_str());
            if k == 0 {
                first.insert("nodes", Item::Value(toml_edit::Value::Array(nodes)));
                continue;
            }
            let mut p = first.clone();
            p.insert("id", toml_edit::value(piece.id.clone()));
            p.insert("nodes", Item::Value(toml_edit::Value::Array(nodes)));
            p.decor_mut().clear();
            more.push(p);
        }
    }
    for p in more {
        array(doc, "member").push(p);
    }
    for (label, joint) in &plan.points {
        if let Some(p) = entry(doc, "point", "label", label) {
            p.remove("member");
            p.remove("at");
            p.insert("node", toml_edit::value(joint.clone()));
        }
    }
    raise_version(doc, LINKS_VERSION);
    Ok(true)
}

/// A position written exactly (`"… m"` each, so it reads back bit for
/// bit).
fn exact_point(p: [f64; 3]) -> Item {
    let mut a = toml_edit::Array::new();
    for v in p {
        a.push(format!("{v:?} m"));
    }
    Item::Value(toml_edit::Value::Array(a))
}

/// Gives joint `id` its kind (none: worked out again). Refused: making a
/// joint that carries attachment points a support joint.
pub fn set_joint_kind(
    doc: &mut DocumentMut,
    t: &StructureType,
    id: &str,
    kind: Option<JointKind>,
) -> Result<(), String> {
    if kind == Some(JointKind::Support)
        && let Some(p) = t.points.iter().find(|p| p.joint == id)
    {
        return Err(format!(
            "joint {id} carries attachment point {}: link it elsewhere first",
            p.label
        ));
    }
    let n = entry(doc, "node", "id", id).ok_or_else(|| format!("no joint {id}"))?;
    match kind {
        Some(k) => {
            n.insert("kind", toml_edit::value(k.as_str()));
            raise_version(doc, LINKS_VERSION);
        }
        None => {
            n.remove("kind");
        }
    }
    Ok(())
}

/// Whether item `key` of a table is there (for the tests).
#[cfg(test)]
fn has(doc: &DocumentMut, table: &str, field: &str, value: &str, key: &str) -> bool {
    doc.get(table)
        .and_then(toml_edit::Item::as_array_of_tables)
        .and_then(|a| {
            a.iter()
                .find(|t| t.get(field).and_then(toml_edit::Item::as_str) == Some(value))
        })
        .is_some_and(|t| t.contains_key(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Handle;
    use ohl_structures::LibraryItem;
    use std::path::Path;

    const TEXT: &str = "format = \"ohl-structure\"\nversion = 2\nid = \"T\"\nname = \"t\"\n\
        kind = \"lattice\"\n\
        [[node]]\nid = \"N1\"\npoint = [\"0 m\", \"0 m\", \"20 m\"]\nsupport = \"fixed\"\n\
        [[node]]\nid = \"N2\"\npoint = [\"4 m\", \"0 m\", \"20 m\"]\n\
        [[member]]\nid = \"arm\"\nnodes = [\"N1\", \"N2\"]\n\
        [[point]]\nlabel = \"A1\"\npoint = [\"4 m\", \"0 m\", \"21 m\"]\n";

    fn read(doc: &DocumentMut) -> StructureType {
        StructureType::parse(Path::new("t.ohls"), &doc.to_string()).unwrap()
    }

    /// Link at an offset (the file raised to 6) to an attach joint only,
    /// unlink back to a point of its own where it was; a support joint
    /// and a joint without a kind refused; a joint's kind set and taken
    /// off.
    #[test]
    fn link_relink_unlink_and_kinds() {
        let mut doc: DocumentMut = TEXT.parse().unwrap();
        let t = read(&doc);
        assert_eq!(t.point_link("A1"), Some(("A1", [0.0; 3])));
        // N2 has no kind: not an attach joint.
        let e = link_point(&mut doc.clone(), &t, "A1", "N2", [0.0, 0.0, 1.0]).unwrap_err();
        assert!(e.contains(NOT_ATTACH), "{e}");
        set_joint_kind(&mut doc, &t, "N2", Some(JointKind::Attach)).unwrap();
        let t = read(&doc);
        assert_eq!(t.joint("N2").unwrap().kind, Some(JointKind::Attach));
        link_point(&mut doc, &t, "A1", "N2", [0.0, 0.0, 1.0]).unwrap();
        let t = read(&doc);
        assert_eq!(t.point_link("A1"), Some(("N2", [0.0, 0.0, 1.0])));
        assert_eq!(t.point_local("A1", None), Some([4.0, 0.0, 21.0]));
        assert_eq!(doc["version"].as_integer(), Some(6));
        assert!(link_point(&mut doc.clone(), &t, "A1", "N1", [0.0; 3]).is_err());
        assert!(link_point(&mut doc.clone(), &t, "A1", "N9", [0.0; 3]).is_err());
        // Unlinked: where it was, standing alone (no joint at all; owner
        // 2026-11-15), the file raised to 17.
        unlink_point(&mut doc, &t, "A1").unwrap();
        let t = read(&doc);
        assert_eq!(t.point_local("A1", None), Some([4.0, 0.0, 21.0]));
        assert_eq!(t.point_link("A1"), None);
        assert!(t.points[0].unlinked);
        assert!(!has(&doc, "point", "label", "A1", "node"));
        assert!(has(&doc, "point", "label", "A1", "unlinked"));
        assert_eq!(doc["version"].as_integer(), Some(17));
        link_point(&mut doc, &t, "A1", "N2", [0.0; 3]).unwrap();
        assert!(!has(&doc, "point", "label", "A1", "unlinked"));
        let t = read(&doc);
        assert!(set_joint_kind(&mut doc.clone(), &t, "N2", Some(JointKind::Support)).is_err());
        set_joint_kind(&mut doc, &t, "N2", None).unwrap();
        assert!(!has(&doc, "node", "id", "N2", "kind"));
        assert!(!has(&doc, "point", "label", "A1", "offset"));
    }

    /// The link follows edits (owner decision 2026-10-21): a point on a
    /// joint moved (the table, a drag) stays on it, its offset the new
    /// distance, the joint where it was; moved back onto it: no offset;
    /// the joint moved takes the point along (its offset kept); with a
    /// mirror symmetry too; a point of its own moves.
    #[test]
    fn a_moved_point_keeps_its_joint() {
        let text = TEXT.replace(
            "[[point]]\nlabel = \"A1\"\npoint = [\"4 m\", \"0 m\", \"21 m\"]\n",
            "[[point]]\nlabel = \"A1\"\nnode = \"N2\"\n",
        );
        let mut doc: DocumentMut = text.parse().unwrap();
        let t = read(&doc);
        let a1 = Handle::Point("A1".into());
        crate::edits::move_to(&mut doc, &t, &a1, [4.0, 0.3, 19.5]).unwrap();
        let t = read(&doc);
        assert_eq!(t.point_link("A1"), Some(("N2", [0.0, 0.3, -0.5])));
        assert_eq!(t.joint("N2").unwrap().position, [4.0, 0.0, 20.0]);
        assert_eq!(t.point_local("A1", None), Some([4.0, 0.3, 19.5]));
        // The joint moved: the point follows at its offset.
        crate::edits::move_to(&mut doc, &t, &Handle::Joint("N2".into()), [5.0, 0.0, 20.0]).unwrap();
        let t = read(&doc);
        assert_eq!(t.point_local("A1", None), Some([5.0, 0.3, 19.5]));
        // With a mirror symmetry (the drag in the 3D view): the same.
        let mut sym: DocumentMut = format!("{doc}[[symmetry]]\nmirror = \"x\"\n")
            .parse()
            .unwrap();
        let ts = read(&sym);
        crate::symmetry::move_to(&mut sym, &ts, &a1, [5.0, 0.0, 20.0]).unwrap();
        let ts = read(&sym);
        assert_eq!(ts.point_link("A1"), Some(("N2", [0.0; 3])));
        assert!(!has(&sym, "point", "label", "A1", "offset"));
        assert_eq!(ts.joint("N2").unwrap().position, [5.0, 0.0, 20.0]);
        // A point of its own moves.
        let mut free: DocumentMut = TEXT.parse().unwrap();
        let tf = read(&free);
        move_point(&mut free, &tf, "A1", [3.0, 0.0, 21.0]).unwrap();
        assert_eq!(read(&free).point_local("A1", None), Some([3.0, 0.0, 21.0]));
    }

    /// Linking a point that is not on the joint (owner 2026-11-15): within
    /// 1 mm onto it without a question; farther, asked — onto it (no
    /// offset, the point moves) or its position kept (the offset). A point
    /// and a joint of the same name are separate: point N2 linked to joint
    /// N1… is found by its label, joint N2 by its id; another point's own
    /// place is no joint to link to. The points on a joint, in order.
    #[test]
    fn linking_asks_how_and_keeps_points_and_joints_apart() {
        let text = TEXT.replace(
            "[[point]]\nlabel = \"A1\"\npoint = [\"4 m\", \"0 m\", \"21 m\"]\n",
            "[[point]]\nlabel = \"A1\"\npoint = [\"4 m\", \"0 m\", \"21 m\"]\n\
             [[point]]\nlabel = \"N2\"\npoint = [\"4.0005 m\", \"0 m\", \"20 m\"]\nunlinked = true\n\
             [[point]]\nlabel = \"F\"\npoint = [\"9 m\", \"0 m\", \"9 m\"]\n",
        ).replace("id = \"N2\"\n", "id = \"N2\"\nkind = \"attach\"\n");
        let mut doc: DocumentMut = text.parse().unwrap();
        let t = read(&doc);
        // Point N2 stands alone, though joint N2 is 0.5 mm away.
        assert_eq!(t.point_link("N2"), None);
        assert_eq!(t.joint("N2").unwrap().source, JointSource::Node);
        // Within 1 mm: no question, onto it.
        assert_eq!(needs_asking(&t, "N2", "N2"), None);
        relink_point(&mut doc, &t, "N2", "N2", LinkHow::KeepPosition).unwrap();
        let t = read(&doc);
        assert_eq!(t.point_link("N2"), Some(("N2", [0.0; 3])));
        // A1 is 1 m above N2: asked.
        let o = needs_asking(&t, "A1", "N2").unwrap();
        assert!((o[2] - 1.0).abs() < 1e-9, "{o:?}");
        let mut onto = doc.clone();
        relink_point(&mut onto, &t, "A1", "N2", LinkHow::Onto).unwrap();
        let to = read(&onto);
        assert_eq!(to.point_link("A1"), Some(("N2", [0.0; 3])));
        assert_eq!(to.point_local("A1", None), Some([4.0, 0.0, 20.0]));
        relink_point(&mut doc, &t, "A1", "N2", LinkHow::KeepPosition).unwrap();
        let t = read(&doc);
        assert_eq!(t.point_link("A1"), Some(("N2", [0.0, 0.0, 1.0])));
        assert_eq!(points_on(&t, "N2"), ["A1", "N2"]);
        // Moving point N2 does not move joint N2.
        move_point(&mut doc, &t, "N2", [4.0, 0.5, 20.0]).unwrap();
        let t = read(&doc);
        assert_eq!(t.joint("N2").unwrap().position, [4.0, 0.0, 20.0]);
        assert_eq!(t.point_link("N2"), Some(("N2", [0.0, 0.5, 0.0])));
        // F (a stick figure's point, its own place) is no joint.
        let e = link_point(&mut doc.clone(), &t, "A1", "F", [0.0; 3]).unwrap_err();
        assert!(e.contains("own place"), "{e}");
        assert!(relink_point(&mut doc.clone(), &t, "A1", "F", LinkHow::Onto).is_err());
        assert!(points_on(&t, "F").is_empty());
    }

    /// A string hangs from an attachment point, never from a joint (owner
    /// 2026-11-15): a string whose top names a joint is refused, said.
    #[test]
    fn a_string_never_hangs_from_a_joint() {
        let text = format!(
            "{TEXT}[[wire]]\nlabel = \"W1\"\n[[string]]\nlabel = \"S1\"\ntop = \"N2\"\nwire = \"W1\"\nlength = \"2 m\"\n"
        );
        let e = StructureType::parse(Path::new("t.ohls"), &text).unwrap_err();
        assert!(
            e.message.contains("is a joint") && e.message.contains("never from a joint"),
            "{}",
            e.message
        );
    }

    /// A joint inserted on a member: an attach joint at the fraction, the
    /// member split in two continuous pieces (the same fields, each
    /// buckling over the whole member: lk = k × 4 m), the first keeping
    /// the id; a point linked to it; the ends refused.
    #[test]
    fn a_joint_inserted_on_a_member() {
        let text = TEXT.replace(
            "[[member]]\nid = \"arm\"\nnodes = [\"N1\", \"N2\"]\n",
            "[[member]]\nid = \"arm\"\nnodes = [\"N1\", \"N2\"]\nsection = \"L\"\nkind = \"frame\"\nk = 0.5\n",
        );
        let mut doc: DocumentMut = text.parse().unwrap();
        let t = read(&doc);
        assert!(insert_joint(&mut doc.clone(), &t, "arm", 0.0).is_err());
        assert!(insert_joint(&mut doc.clone(), &t, "nope", 0.5).is_err());
        let id = insert_joint(&mut doc, &t, "arm", 0.25).unwrap();
        assert_eq!(id, "N3");
        let t = read(&doc);
        let j = t.joint("N3").unwrap();
        assert_eq!(
            (j.position, j.kind),
            ([1.0, 0.0, 20.0], Some(JointKind::Attach))
        );
        let pieces: Vec<(&str, [&str; 2])> = t
            .members
            .iter()
            .map(|m| (m.id.as_str(), [m.nodes[0].as_str(), m.nodes[1].as_str()]))
            .collect();
        assert_eq!(pieces, [("arm", ["N1", "N3"]), ("arm-2", ["N3", "N2"])]);
        for m in &t.members {
            assert_eq!(m.section.as_deref(), Some("L"));
            assert_eq!(m.kind.as_deref(), Some("frame"));
            assert_eq!(m.lk.as_ref().and_then(|v| v.as_str()), Some("2.0 m"));
        }
        assert_eq!(doc["version"].as_integer(), Some(6));
        link_point(&mut doc, &t, "A1", "N3", [0.0, 0.0, 1.0]).unwrap();
        assert_eq!(read(&doc).point_link("A1"), Some(("N3", [0.0, 0.0, 1.0])));
    }

    /// A file written before, a point along a member: the file as edited
    /// gets the joint and the split the structure reads (so it reads the
    /// same), the point on it; a file without such a point unchanged.
    #[test]
    fn points_along_members_written_on_joints() {
        let text = TEXT.replace(
            "[[point]]\nlabel = \"A1\"\npoint = [\"4 m\", \"0 m\", \"21 m\"]\n",
            "[[point]]\nlabel = \"P1\"\nmember = \"arm\"\nat = 0.25\n",
        );
        let read_before = read(&text.parse().unwrap());
        let mut doc: DocumentMut = text.parse().unwrap();
        assert!(insert_joints_on_members(&mut doc).unwrap());
        assert!(!doc.to_string().contains("member = \"arm\""));
        let t = read(&doc);
        assert_eq!(t.nodes, read_before.nodes);
        assert_eq!(t.members, read_before.members);
        assert_eq!(t.points, read_before.points);
        assert_eq!(t.point_link("P1"), Some(("P1", [0.0; 3])));
        let mut plain: DocumentMut = TEXT.parse().unwrap();
        assert!(!insert_joints_on_members(&mut plain).unwrap());
        assert_eq!(plain.to_string(), TEXT);
    }
}
