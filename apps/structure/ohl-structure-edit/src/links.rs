// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Attachment points linked to joints in a structure file (2026-10-21):
//! link (or relink) a point to a joint at an offset, unlink it, give a
//! joint its kind. Each one edit of the file (one undo step of the
//! application). The file keys stay `[[point]]` `node` and `offset`,
//! `[[node]]` `kind`; a file is raised to version 6 when an offset or a
//! kind is written.

use ohl_structures::{JointKind, JointSource, StructureType};
use toml_edit::DocumentMut;

use crate::edits::{entry, point_value, raise_version};

/// The version that reads a joint's `kind` and a point's `offset`.
pub const LINKS_VERSION: i64 = 6;

/// Links attachment point `label` to joint `joint` at `offset` (local
/// axes, m; zero: on it): its `node` (a joint the points make along a
/// member: its `member` and `at`), its `offset`, its own `point` gone.
/// Refused: no such point or joint, or a support joint.
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
    let j = t.joint(joint).ok_or_else(|| format!("no joint {joint}"))?;
    if j.kind == Some(JointKind::Support) {
        return Err(format!(
            "joint {joint} is a support joint (strings hang from attach joints)"
        ));
    }
    let p = entry(doc, "point", "label", label).ok_or("no such point in the file")?;
    for k in ["point", "node", "member", "at", "offset"] {
        p.remove(k);
    }
    match &j.source {
        JointSource::Node => {
            p.insert("node", toml_edit::value(joint));
        }
        JointSource::Member { member, at } => {
            p.insert("member", toml_edit::value(member.clone()));
            p.insert("at", toml_edit::value(*at));
        }
        // Another point's own joint: at its position.
        JointSource::Point => {
            let o = offset;
            let q = j.position;
            p.insert(
                "point",
                point_value([q[0] + o[0], q[1] + o[1], q[2] + o[2]]),
            );
            return Ok(());
        }
    }
    if offset != [0.0; 3] {
        p.insert("offset", point_value(offset));
        raise_version(doc, LINKS_VERSION);
    }
    Ok(())
}

/// Unlinks attachment point `label`: it keeps its position as a point of
/// its own (its own joint, on no member).
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
    Ok(())
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

    /// Link at an offset (the file raised to 6), relink along a member,
    /// unlink back to a point of its own where it was; a support joint
    /// refused; a joint's kind set and taken off.
    #[test]
    fn link_relink_unlink_and_kinds() {
        let mut doc: DocumentMut = TEXT.parse().unwrap();
        let t = read(&doc);
        assert_eq!(t.point_link("A1"), Some(("A1", [0.0; 3])));
        link_point(&mut doc, &t, "A1", "N2", [0.0, 0.0, 1.0]).unwrap();
        let t = read(&doc);
        assert_eq!(t.point_link("A1"), Some(("N2", [0.0, 0.0, 1.0])));
        assert_eq!(t.point_local("A1", None), Some([4.0, 0.0, 21.0]));
        assert_eq!(doc["version"].as_integer(), Some(6));
        assert!(link_point(&mut doc.clone(), &t, "A1", "N1", [0.0; 3]).is_err());
        assert!(link_point(&mut doc.clone(), &t, "A1", "N9", [0.0; 3]).is_err());
        // Unlinked: where it was, its own joint.
        unlink_point(&mut doc, &t, "A1").unwrap();
        let t = read(&doc);
        assert_eq!(t.point_local("A1", None), Some([4.0, 0.0, 21.0]));
        assert_eq!(t.point_link("A1"), Some(("A1", [0.0; 3])));
        assert!(!has(&doc, "point", "label", "A1", "node"));
        // Kinds.
        set_joint_kind(&mut doc, &t, "N2", Some(JointKind::Attach)).unwrap();
        let t = read(&doc);
        assert_eq!(t.joint("N2").unwrap().kind, Some(JointKind::Attach));
        link_point(&mut doc, &t, "A1", "N2", [0.0; 3]).unwrap();
        let t = read(&doc);
        assert!(set_joint_kind(&mut doc.clone(), &t, "N2", Some(JointKind::Support)).is_err());
        set_joint_kind(&mut doc, &t, "N2", None).unwrap();
        assert!(!has(&doc, "node", "id", "N2", "kind"));
        assert!(!has(&doc, "point", "label", "A1", "offset"));
    }
}
