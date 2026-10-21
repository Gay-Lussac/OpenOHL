// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Joints on members (owner decision 2026-10-21): an attachment point
//! needs a joint; it cannot be put on a member (a bar). A joint is
//! inserted on the member instead and the member split there into pieces
//! that stay **continuous** through the joint: the same properties on
//! both sides, a rigid connection (moment continuity for frames, not a
//! hinge), and each piece keeps the buckling length of the whole member
//! (its `lk`, else k × its whole length).
//!
//! Files written before name a point's place along a member (`member`,
//! `at`, 0–1 from its first joint). They are read as if the joint had
//! been inserted: [`plan`] gives the joints and pieces, applied to the
//! structure as read ([`crate::StructureType`]), to the tables the
//! structure analysis reads ([`apply_to_table`]) and, in the structure
//! application, to the file as edited (`ohl-structure-edit`). Joint ids:
//! the first point's label there (`-a`, `-a2`… when taken), as before;
//! the pieces: the member's id for the first, `<id>-2`, `<id>-3`… for
//! the others (the next free). A point at an end of its member (`at` 0
//! or 1) is on that end's joint: nothing is inserted.

use ohl_core::units::Length;
use serde::Deserialize;

/// A joint inserted on a member.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberJoint {
    /// Its id.
    pub id: String,
    /// The member it is on (its id before the split).
    pub member: String,
    /// Fraction of the member's length from its first joint.
    pub at: f64,
    /// Where it is, local axes, m.
    pub position: [f64; 3],
}

/// A piece of a split member.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberPiece {
    /// Its id (the member's for the first piece).
    pub id: String,
    /// Its two joints.
    pub nodes: [String; 2],
}

/// A member split at joints inserted on it.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberSplit {
    /// The member's id.
    pub member: String,
    /// Its pieces from its first joint to its second.
    pub pieces: Vec<MemberPiece>,
    /// The buckling length every piece takes (m): the whole member's,
    /// k × its length; none: the member has its own `lk`, kept.
    pub lk: Option<f64>,
}

/// What [`plan`] inserts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OnMembers {
    /// The joints inserted, in the order of the points.
    pub joints: Vec<MemberJoint>,
    /// The members split, in the order of the members.
    pub splits: Vec<MemberSplit>,
    /// Each point placed along a member: its label and its joint.
    pub points: Vec<(String, String)>,
}

impl OnMembers {
    /// Nothing to insert.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

/// A member as [`plan`] sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberRow<'a> {
    /// Id.
    pub id: &'a str,
    /// Its joints.
    pub nodes: [&'a str; 2],
    /// Its effective length factor (none: 1).
    pub k: Option<f64>,
    /// Whether it has its own `lk`.
    pub has_lk: bool,
}

/// The next free id after `id`: `<id>-2`, `<id>-3`…
fn next_piece(id: &str, taken: &mut std::collections::BTreeSet<String>) -> String {
    let piece = (2..)
        .map(|n| format!("{id}-{n}"))
        .find(|c| !taken.contains(c))
        .expect("a free id");
    taken.insert(piece.clone());
    piece
}

/// Splits `members` at the joints `at` (member id, fraction, joint id):
/// each member's pieces in order, the first keeping its id.
pub fn split_members(
    joints: &[(&str, [f64; 3])],
    members: &[MemberRow],
    at: &[(String, f64, String)],
) -> Vec<MemberSplit> {
    let mut taken: std::collections::BTreeSet<String> =
        members.iter().map(|m| m.id.to_owned()).collect();
    let pos = |id: &str| joints.iter().find(|j| j.0 == id).map(|j| j.1);
    let mut out = Vec::new();
    for m in members {
        let mut here: Vec<(f64, &str)> = at
            .iter()
            .filter(|(id, _, _)| id == m.id)
            .map(|(_, f, j)| (*f, j.as_str()))
            .collect();
        if here.is_empty() {
            continue;
        }
        here.sort_by(|a, b| a.0.total_cmp(&b.0));
        here.dedup_by(|a, b| a.0.to_bits() == b.0.to_bits());
        let mut ends: Vec<&str> = vec![m.nodes[0]];
        ends.extend(here.iter().map(|(_, j)| *j));
        ends.push(m.nodes[1]);
        let pieces = ends
            .windows(2)
            .enumerate()
            .map(|(k, w)| MemberPiece {
                id: if k == 0 {
                    m.id.to_owned()
                } else {
                    next_piece(m.id, &mut taken)
                },
                nodes: [w[0].to_owned(), w[1].to_owned()],
            })
            .collect();
        let lk = (!m.has_lk).then(|| {
            let (a, b) = (
                pos(m.nodes[0]).unwrap_or_default(),
                pos(m.nodes[1]).unwrap_or_default(),
            );
            let l = (0..3).map(|k| (b[k] - a[k]).powi(2)).sum::<f64>().sqrt();
            m.k.unwrap_or(1.0) * l
        });
        out.push(MemberSplit {
            member: m.id.to_owned(),
            pieces,
            lk,
        });
    }
    out
}

/// The joints to insert for the points placed along members (label,
/// member, fraction), the members split there, and each point's joint.
/// `joints`: the joints' ids and positions (local axes, m). Refused: a
/// member not there, a fraction outside 0–1.
pub fn plan(
    joints: &[(&str, [f64; 3])],
    members: &[MemberRow],
    points: &[(&str, &str, f64)],
) -> Result<OnMembers, String> {
    let mut taken: std::collections::BTreeSet<String> =
        joints.iter().map(|(id, _)| (*id).to_owned()).collect();
    let pos = |id: &str| {
        joints
            .iter()
            .find(|j| j.0 == id)
            .map(|j| j.1)
            .ok_or_else(|| format!("no joint \"{id}\""))
    };
    let mut out = OnMembers::default();
    for &(label, member, at) in points {
        let bad = |m: String| format!("point {label}: {m}");
        let m = members
            .iter()
            .find(|m| m.id == member)
            .ok_or_else(|| bad(format!("no member \"{member}\"")))?;
        if !(0.0..=1.0).contains(&at) {
            return Err(bad("at must be between 0 and 1".into()));
        }
        // At an end: that end's joint.
        if at == 0.0 || at == 1.0 {
            let end = m.nodes[usize::from(at == 1.0)];
            out.points.push((label.to_owned(), end.to_owned()));
            continue;
        }
        if let Some(j) = out
            .joints
            .iter()
            .find(|j| j.member == member && j.at.to_bits() == at.to_bits())
        {
            out.points.push((label.to_owned(), j.id.clone()));
            continue;
        }
        let (a, b) = (pos(m.nodes[0]).map_err(bad)?, pos(m.nodes[1]).map_err(bad)?);
        let id = std::iter::once(label.to_owned())
            .chain(std::iter::once(format!("{label}-a")))
            .chain((2..).map(|n| format!("{label}-a{n}")))
            .find(|id| !taken.contains(id))
            .expect("a free id");
        taken.insert(id.clone());
        out.joints.push(MemberJoint {
            id: id.clone(),
            member: member.to_owned(),
            at,
            position: [0, 1, 2].map(|k| a[k] + at * (b[k] - a[k])),
        });
        out.points.push((label.to_owned(), id));
    }
    let all: Vec<(&str, [f64; 3])> = joints
        .iter()
        .copied()
        .chain(out.joints.iter().map(|j| (j.id.as_str(), j.position)))
        .collect();
    let at: Vec<(String, f64, String)> = out
        .joints
        .iter()
        .map(|j| (j.member.clone(), j.at, j.id.clone()))
        .collect();
    out.splits = split_members(&all, members, &at);
    Ok(out)
}

/// A position as written in files, `"… m"` each, exactly (so the joint
/// reads back bit for bit).
pub fn point_value(p: [f64; 3]) -> toml::Value {
    toml::Value::Array(
        p.iter()
            .map(|v| toml::Value::String(format!("{v:?} m")))
            .collect(),
    )
}

/// A buckling length as written in files, exactly.
pub fn lk_value(lk: f64) -> toml::Value {
    toml::Value::String(format!("{lk:?} m"))
}

/// [`plan`] for the tables of a structure file (`[[node]]`,
/// `[[member]]`, `[[point]]`). None: no point along a member.
pub fn plan_for_table(file: &toml::Table) -> Result<Option<OnMembers>, String> {
    let points: Vec<(String, String, f64)> = file
        .get("point")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let label = p.get("label")?.as_str()?;
            let member = p.get("member")?.as_str()?;
            let at = p.get("at")?;
            let at = at
                .as_float()
                .or_else(|| at.as_integer().map(|i| i as f64))?;
            Some((label.to_owned(), member.to_owned(), at))
        })
        .collect();
    if points.is_empty() {
        return Ok(None);
    }
    let length3 = |v: &toml::Value| -> Option<[f64; 3]> {
        <[Length; 3]>::deserialize(v.clone())
            .ok()
            .map(|l| l.map(Length::si))
    };
    let nodes: Vec<(String, [f64; 3])> = file
        .get("node")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|n| Some((n.get("id")?.as_str()?.to_owned(), length3(n.get("point")?)?)))
        .collect();
    let rows: Vec<(String, [String; 2], Option<f64>, bool)> = file
        .get("member")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let ends = m.get("nodes")?.as_array()?;
            Some((
                m.get("id")?.as_str()?.to_owned(),
                [
                    ends.first()?.as_str()?.to_owned(),
                    ends.get(1)?.as_str()?.to_owned(),
                ],
                m.get("k")
                    .and_then(|k| k.as_float().or_else(|| k.as_integer().map(|i| i as f64))),
                m.get("lk").is_some(),
            ))
        })
        .collect();
    plan(
        &nodes
            .iter()
            .map(|(id, p)| (id.as_str(), *p))
            .collect::<Vec<_>>(),
        &rows
            .iter()
            .map(|(id, n, k, lk)| MemberRow {
                id,
                nodes: [&n[0], &n[1]],
                k: *k,
                has_lk: *lk,
            })
            .collect::<Vec<_>>(),
        &points
            .iter()
            .map(|(l, m, at)| (l.as_str(), m.as_str(), *at))
            .collect::<Vec<_>>(),
    )
    .map(Some)
}

/// [`plan`] applied to the tables of a structure file: the joints
/// inserted (kind `attach`) after the others, each split member's first
/// piece in its place (its second joint the first inserted), the further
/// pieces after the members (every field of the member, its `lk` set),
/// each point on its joint (`node`) instead of `member` and `at`.
/// Whether anything changed.
pub fn apply_to_table(file: &mut toml::Table) -> Result<bool, String> {
    let Some(plan) = plan_for_table(file)? else {
        return Ok(false);
    };
    let table = |pairs: Vec<(&str, toml::Value)>| {
        toml::Value::Table(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
    };
    let array = |file: &mut toml::Table, key: &str| -> Vec<toml::Value> {
        file.get(key)
            .and_then(toml::Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let mut node_tables = array(file, "node");
    for j in &plan.joints {
        node_tables.push(table(vec![
            ("id", toml::Value::String(j.id.clone())),
            ("point", point_value(j.position)),
            ("kind", toml::Value::String("attach".into())),
        ]));
    }
    file.insert("node".into(), toml::Value::Array(node_tables));
    let mut members = array(file, "member");
    let mut more = Vec::new();
    for s in &plan.splits {
        let Some(m) = members
            .iter_mut()
            .find(|m| m.get("id").and_then(toml::Value::as_str) == Some(s.member.as_str()))
        else {
            continue;
        };
        let Some(t) = m.as_table_mut() else { continue };
        if let Some(lk) = s.lk {
            t.insert("lk".into(), lk_value(lk));
        }
        for (k, piece) in s.pieces.iter().enumerate() {
            let nodes = toml::Value::Array(
                piece
                    .nodes
                    .iter()
                    .map(|n| toml::Value::String(n.clone()))
                    .collect(),
            );
            if k == 0 {
                t.insert("nodes".into(), nodes);
                continue;
            }
            let mut p = t.clone();
            p.insert("id".into(), toml::Value::String(piece.id.clone()));
            p.insert("nodes".into(), nodes);
            more.push(toml::Value::Table(p));
        }
    }
    members.extend(more);
    file.insert("member".into(), toml::Value::Array(members));
    let mut point_tables = array(file, "point");
    for p in point_tables
        .iter_mut()
        .filter_map(toml::Value::as_table_mut)
    {
        let Some(label) = p.get("label").and_then(toml::Value::as_str) else {
            continue;
        };
        let Some((_, joint)) = plan.points.iter().find(|(l, _)| l == label) else {
            continue;
        };
        let joint = joint.clone();
        p.remove("member");
        p.remove("at");
        p.insert("node".into(), toml::Value::String(joint));
    }
    file.insert("point".into(), toml::Value::Array(point_tables));
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row<'a>(id: &'a str, a: &'a str, b: &'a str) -> MemberRow<'a> {
        MemberRow {
            id,
            nodes: [a, b],
            k: None,
            has_lk: false,
        }
    }

    /// Two points at one place share a joint; another place, another
    /// joint; the member split in three pieces keeping its whole buckling
    /// length; a point at an end is on that end's joint; ids taken get a
    /// suffix.
    #[test]
    fn joints_inserted_and_members_split() {
        let joints = [("N1", [0.0, 0.0, 30.0]), ("N2", [-8.0, 0.0, 30.0])];
        let p = plan(
            &joints,
            &[
                MemberRow {
                    k: Some(0.5),
                    ..row("arm", "N1", "N2")
                },
                row("arm-2", "N2", "N1"),
            ],
            &[
                ("B", "arm", 0.75),
                ("B2", "arm", 0.75),
                ("N1", "arm", 0.25),
                ("E", "arm", 1.0),
            ],
        )
        .unwrap();
        assert_eq!(
            p.points,
            [
                ("B".to_owned(), "B".to_owned()),
                ("B2".to_owned(), "B".to_owned()),
                ("N1".to_owned(), "N1-a".to_owned()),
                ("E".to_owned(), "N2".to_owned()),
            ]
        );
        assert_eq!(p.joints.len(), 2);
        assert_eq!(p.joints[0].position, [-6.0, 0.0, 30.0]);
        assert_eq!(p.joints[1].position, [-2.0, 0.0, 30.0]);
        assert_eq!(p.splits.len(), 1);
        let s = &p.splits[0];
        let ids: Vec<&str> = s.pieces.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, ["arm", "arm-3", "arm-4"]);
        let ends: Vec<[&str; 2]> = s
            .pieces
            .iter()
            .map(|x| [x.nodes[0].as_str(), x.nodes[1].as_str()])
            .collect();
        assert_eq!(ends, [["N1", "N1-a"], ["N1-a", "B"], ["B", "N2"]]);
        // k × the whole length: 0.5 × 8 m.
        assert_eq!(s.lk, Some(4.0));
        assert!(plan(&joints, &[row("arm", "N1", "N2")], &[("x", "nope", 0.5)]).is_err());
        assert!(plan(&joints, &[row("arm", "N1", "N2")], &[("x", "arm", 1.5)]).is_err());
    }

    /// The tables: the joint added (attach), the member split, the point
    /// on its joint; a file without points along members unchanged.
    #[test]
    fn tables_rewritten() {
        let text = "[[node]]\nid = \"A\"\npoint = [\"0 m\", \"0 m\", \"0 m\"]\n\
            [[node]]\nid = \"B\"\npoint = [\"4 m\", \"0 m\", \"0 m\"]\n\
            [[member]]\nid = \"beam\"\nnodes = [\"A\", \"B\"]\nsection = \"s\"\nkind = \"frame\"\n\
            [[point]]\nlabel = \"P1\"\nmember = \"beam\"\nat = 0.25\n";
        let mut file: toml::Table = toml::from_str(text).unwrap();
        assert!(apply_to_table(&mut file).unwrap());
        let nodes = file["node"].as_array().unwrap();
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[2]["id"].as_str(), Some("P1"));
        assert_eq!(nodes[2]["kind"].as_str(), Some("attach"));
        let members = file["member"].as_array().unwrap();
        assert_eq!(members.len(), 2);
        assert_eq!(members[1]["id"].as_str(), Some("beam-2"));
        assert_eq!(members[1]["section"].as_str(), Some("s"));
        assert_eq!(members[0]["lk"].as_str(), Some("4.0 m"));
        let point = &file["point"].as_array().unwrap()[0];
        assert_eq!(point["node"].as_str(), Some("P1"));
        assert!(point.get("member").is_none() && point.get("at").is_none());
        let mut plain: toml::Table =
            toml::from_str("[[point]]\nlabel = \"x\"\nnode = \"A\"\n").unwrap();
        assert!(!apply_to_table(&mut plain).unwrap());
    }
}
