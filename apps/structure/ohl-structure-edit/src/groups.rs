// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Member groups: a member's `group` (`"body-leg"`, `"body-bracing-3"`…).
//! There is no table of groups: a group is the members naming it, and a
//! section or kind set for a group is written on each of its members
//! (so the finite-element model and the level read members only).

use std::collections::BTreeMap;

use ohl_structures::StructureType;
use toml_edit::DocumentMut;

use crate::body::ensure_version;
use crate::edits::{entry, set_member_kind, set_member_section};

/// A group as the Design tab shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// Its name.
    pub name: String,
    /// Its members.
    pub members: Vec<String>,
    /// The section all its members have (none: none or several).
    pub section: Option<String>,
    /// Several sections among its members.
    pub mixed_sections: bool,
    /// The kind all its members have (none: several; `truss` when none
    /// is written).
    pub kind: Option<String>,
}

/// The groups of `t`, by name.
pub fn groups(t: &StructureType) -> Vec<Group> {
    let mut by: BTreeMap<&str, Vec<&ohl_structures::structures::TypeMember>> = BTreeMap::new();
    for m in &t.members {
        if let Some(g) = &m.group {
            by.entry(g.as_str()).or_default().push(m);
        }
    }
    by.into_iter()
        .map(|(name, ms)| {
            let first = ms[0];
            let same_section = ms.iter().all(|m| m.section == first.section);
            let kind = |m: &ohl_structures::structures::TypeMember| {
                m.kind.clone().unwrap_or_else(|| "truss".into())
            };
            let same_kind = ms.iter().all(|m| kind(m) == kind(first));
            Group {
                name: name.to_owned(),
                members: ms.iter().map(|m| m.id.clone()).collect(),
                section: if same_section {
                    first.section.clone()
                } else {
                    None
                },
                mixed_sections: !same_section,
                kind: same_kind.then(|| kind(first)),
            }
        })
        .collect()
}

/// Group `group` (none: no group) on `members`. A member joining a group
/// whose members all have one section (or kind) takes it.
pub fn set_group(
    doc: &mut DocumentMut,
    t: &StructureType,
    members: &[String],
    group: Option<&str>,
) {
    let group = group.map(str::trim).filter(|g| !g.is_empty());
    let joined = group.and_then(|g| groups(t).into_iter().find(|x| x.name == g));
    for id in members {
        let Some(e) = entry(doc, "member", "id", id) else {
            continue;
        };
        match group {
            Some(g) => {
                e.insert("group", toml_edit::value(g));
            }
            None => {
                e.remove("group");
            }
        }
        if let Some(j) = &joined
            && !j.members.contains(id)
        {
            if let Some(s) = &j.section {
                set_member_section(doc, id, Some(s));
            }
            if let Some(k) = j.kind.as_deref().filter(|k| *k != "truss") {
                set_member_kind(doc, id, k);
            }
        }
    }
    if group.is_some() {
        ensure_version(doc);
    }
}

/// The section of every member of `group` (none: sticks).
pub fn set_group_section(
    doc: &mut DocumentMut,
    t: &StructureType,
    group: &str,
    section: Option<&str>,
) {
    for m in t
        .members
        .iter()
        .filter(|m| m.group.as_deref() == Some(group))
    {
        set_member_section(doc, &m.id, section);
    }
}

/// Field `field` of every member of `group` as typed (a number,
/// `true`/`false`, else a text such as `"180 kN"`; empty removes it),
/// [`crate::edits::set_item_field`].
pub fn set_group_field(
    doc: &mut DocumentMut,
    t: &StructureType,
    group: &str,
    field: &str,
    typed: &str,
) -> Result<(), String> {
    for m in t
        .members
        .iter()
        .filter(|m| m.group.as_deref() == Some(group))
    {
        crate::edits::set_item_field(doc, "member", &m.id, field, typed)?;
    }
    Ok(())
}

/// The kind of every member of `group`.
pub fn set_group_kind(doc: &mut DocumentMut, t: &StructureType, group: &str, kind: &str) {
    for m in t
        .members
        .iter()
        .filter(|m| m.group.as_deref() == Some(group))
    {
        set_member_kind(doc, &m.id, kind);
    }
}
