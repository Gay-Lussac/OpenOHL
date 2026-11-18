// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A few joints of a large tower in detail without the whole tower (the
//! faster follow of the joint layout, 2026-11-18): the structure file cut
//! down to what the details of those joints read — every joint (their
//! positions and supports), the members meeting at them and those meeting
//! those members' other joints (an end's conditions read the other end's:
//! one end eccentric, the other not; what a member bolts onto holds it by
//! its stiffness against the members at that joint), the members these
//! name (what they bolt onto or run on into, a crossing partner), their
//! connections and the plates and splices at their joints; every other
//! table as it is. The details of those joints are the same as from the
//! whole file (tested, and compared on the private real towers); only the
//! notes on the whole structure (connections leading nowhere) differ.

use std::collections::BTreeSet;

use toml_edit::{ArrayOfTables, DocumentMut, Item};

/// Below this share of the members kept, the file is cut down; above it
/// the whole file is as quick.
const WORTH: f64 = 0.5;

/// The members of `doc` the details of `joints` read (see the module).
pub fn members_for(doc: &DocumentMut, joints: &BTreeSet<String>) -> BTreeSet<String> {
    let members: Vec<(&str, [&str; 2], Vec<String>)> = doc
        .get("member")
        .and_then(Item::as_array_of_tables)
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?;
                    let n = m.get("nodes")?.as_array()?;
                    let a = n.get(0)?.as_str()?;
                    let b = n.get(1)?.as_str()?;
                    // The members it names: a crossing partner.
                    let named = m
                        .get("design")
                        .and_then(Item::as_table_like)
                        .and_then(|t| t.get("crossing"))
                        .and_then(Item::as_str)
                        .map(str::to_owned)
                        .into_iter()
                        .collect();
                    Some((id, [a, b], named))
                })
                .collect()
        })
        .unwrap_or_default();
    let ring = |nodes: &BTreeSet<&str>| -> BTreeSet<usize> {
        members
            .iter()
            .enumerate()
            .filter(|(_, m)| m.1.iter().any(|n| nodes.contains(n)))
            .map(|(k, _)| k)
            .collect()
    };
    let first: BTreeSet<&str> = joints.iter().map(String::as_str).collect();
    let r1 = ring(&first);
    let second: BTreeSet<&str> = r1.iter().flat_map(|&k| members[k].1).collect();
    let r2 = ring(&second);
    let mut keep: BTreeSet<String> = r2.iter().map(|&k| members[k].0.to_owned()).collect();
    for &k in &r2 {
        keep.extend(members[k].2.iter().cloned());
    }
    // What their connections name (a member they bolt onto or run on
    // into).
    if let Some(a) = doc.get("connection").and_then(Item::as_array_of_tables) {
        let mut more = Vec::new();
        for c in a.iter() {
            if c.get("member")
                .and_then(Item::as_str)
                .is_some_and(|m| keep.contains(m))
                && let Some(o) = c.get("onto").and_then(Item::as_str)
                && members.iter().any(|m| m.0 == o)
            {
                more.push(o.to_owned());
            }
        }
        keep.extend(more);
    }
    keep
}

/// `doc` cut down to the members `keep`, their connections and the
/// plates and splices at their joints (every other table as it is); none
/// when that keeps most of the file (the whole file is as quick).
pub fn cut_down(doc: &DocumentMut, keep: &BTreeSet<String>) -> Option<DocumentMut> {
    let all = doc
        .get("member")
        .and_then(Item::as_array_of_tables)
        .map_or(0, ArrayOfTables::len);
    if all == 0 || keep.len() as f64 > WORTH * all as f64 {
        return None;
    }
    let mut nodes: BTreeSet<String> = BTreeSet::new();
    let mut out = DocumentMut::new();
    for (k, v) in doc.iter() {
        let kept = match (k, v.as_array_of_tables()) {
            ("member", Some(a)) => {
                let mut x = ArrayOfTables::new();
                for m in a.iter() {
                    if m.get("id")
                        .and_then(Item::as_str)
                        .is_some_and(|id| keep.contains(id))
                    {
                        if let Some(n) = m.get("nodes").and_then(Item::as_array) {
                            nodes.extend(n.iter().filter_map(|v| v.as_str().map(str::to_owned)));
                        }
                        x.push(m.clone());
                    }
                }
                Item::ArrayOfTables(x)
            }
            ("connection", Some(a)) => {
                let mut x = ArrayOfTables::new();
                for c in a.iter() {
                    if c.get("member")
                        .and_then(Item::as_str)
                        .is_some_and(|m| keep.contains(m))
                    {
                        x.push(c.clone());
                    }
                }
                Item::ArrayOfTables(x)
            }
            _ => v.clone(),
        };
        out.insert(k, kept);
    }
    // The joint layout's record: its marks of the kept members' ends (the
    // drawing reads them) and their provisional bolts; the rest (why joints
    // need a hand detail, the plates made…) read by no joint's details.
    if let Some(a) = out
        .get_mut("generator")
        .and_then(Item::as_array_of_tables_mut)
    {
        for g in a.iter_mut() {
            if g.get("kind").and_then(Item::as_str) != Some(crate::RECORD_KIND) {
                continue;
            }
            for key in ["hand", "joints", "made", "rolled", "oriented"] {
                g.remove(key);
            }
            for key in ["ends", "provisional"] {
                if let Some(rows) = g.get_mut(key).and_then(Item::as_array_mut) {
                    rows.retain(|r| {
                        r.as_array()
                            .and_then(|r| r.get(0))
                            .and_then(|m| m.as_str())
                            .is_some_and(|m| keep.contains(m))
                    });
                }
            }
        }
    }
    // The plates and splices at the kept members' joints.
    for key in ["plate", "splice"] {
        if let Some(a) = out.get_mut(key).and_then(Item::as_array_of_tables_mut) {
            a.retain(|p| {
                p.get("joint")
                    .and_then(Item::as_str)
                    .is_some_and(|j| nodes.contains(j))
            });
        }
    }
    Some(out)
}
