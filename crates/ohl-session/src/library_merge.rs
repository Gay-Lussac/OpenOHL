// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A library file changed on both sides of a merge, merged **entry by
//! entry** (owner, 2026-11-10; before, the file was one conflict and one
//! side's work was redone by hand): two lines each adding a weather case,
//! a rule or a cable to the same file, or changing different entries or
//! different values of one entry, combine by themselves.
//!
//! Library files are TOML. The three texts (the common ancestor's, ours,
//! theirs) are compared by value, key by key:
//!
//! - a key changed on one side only is taken from that side;
//! - tables (`[wind]`) are merged key by key, the same way;
//! - lists of tables (`[[case]]`, `[[rule]]`) whose entries all carry a
//!   text `name`, `id` or `label`, unique in each text, are merged entry by
//!   entry (an entry is a table: key by key again); theirs' new entries
//!   come after ours;
//! - anything else changed differently on both sides — a value, a list
//!   without names, an entry deleted on one side and changed on the other —
//!   is not merged here: the file stays one conflict, as before.
//!
//! The result is **ours' text with theirs' changes laid into it**
//! (`toml_edit`): ours' comments, order and layout stay; an entry taken
//! from theirs comes with its own comments. A text that is not TOML is not
//! merged (one conflict).

use std::collections::{BTreeMap, BTreeSet};

use toml::Value;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

/// `ours` and `theirs` of a library file merged from `base` (none: the
/// file is new on both sides), or `None` where they do not combine by
/// themselves (see the module).
pub fn merge(base: Option<&str>, ours: &str, theirs: &str) -> Option<String> {
    let read = |text: &str| toml::from_str::<toml::Table>(text).ok();
    let b = match base {
        Some(text) => Some(read(text)?),
        None => None,
    };
    let (o, t) = (read(ours)?, read(theirs)?);
    let mut out: DocumentMut = ours.parse().ok()?;
    let from: DocumentMut = theirs.parse().ok()?;
    merge_table(b.as_ref(), &o, &t, out.as_table_mut(), from.as_table())?;
    Some(out.to_string())
}

/// Lays theirs' changes of one table into `out` (ours'); `from`: theirs'.
fn merge_table(
    b: Option<&toml::Table>,
    o: &toml::Table,
    t: &toml::Table,
    out: &mut Table,
    from: &Table,
) -> Option<()> {
    let keys: BTreeSet<&String> = b
        .into_iter()
        .flat_map(|b| b.keys())
        .chain(o.keys())
        .chain(t.keys())
        .collect();
    for k in keys {
        let (vb, vo, vt) = (b.and_then(|b| b.get(k)), o.get(k), t.get(k));
        if vo == vt || vb == vt {
            continue;
        }
        if vb == vo {
            match from.get(k) {
                Some(item) => {
                    out.insert(k, item.clone());
                }
                None => {
                    out.remove(k);
                }
            }
            continue;
        }
        // Changed on both sides.
        match (vo, vt) {
            (Some(Value::Table(to)), Some(Value::Table(tt))) => {
                let (Some(Item::Table(out)), Some(Item::Table(from))) =
                    (out.get_mut(k), from.get(k))
                else {
                    return None;
                };
                merge_table(vb.and_then(Value::as_table), to, tt, out, from)?;
            }
            (Some(Value::Array(ao)), Some(Value::Array(at))) => {
                let ab: &[Value] = match vb {
                    Some(Value::Array(a)) => a,
                    None => &[],
                    Some(_) => return None,
                };
                let (Some(Item::ArrayOfTables(out)), Some(Item::ArrayOfTables(from))) =
                    (out.get_mut(k), from.get(k))
                else {
                    return None;
                };
                merge_entries(ab, ao, at, out, from)?;
            }
            _ => return None,
        }
    }
    Some(())
}

/// The key the entries of the three lists are told by: `name`, `id` or
/// `label` — a text every entry has, unique in each list.
fn entry_key(lists: [&[Value]; 3]) -> Option<&'static str> {
    ["name", "id", "label"].into_iter().find(|f| {
        lists.iter().all(|list| {
            let mut seen = BTreeSet::new();
            list.iter().all(|e| {
                e.get(f)
                    .and_then(Value::as_str)
                    .is_some_and(|n| seen.insert(n))
            })
        })
    })
}

/// Lays theirs' changes of a list of tables into `out` (ours', its
/// entries in the order of `o`); `from`: theirs', in the order of `t`.
fn merge_entries(
    b: &[Value],
    o: &[Value],
    t: &[Value],
    out: &mut ArrayOfTables,
    from: &ArrayOfTables,
) -> Option<()> {
    let field = entry_key([b, o, t])?;
    if out.len() != o.len() || from.len() != t.len() {
        return None;
    }
    let index = |list: &'_ [Value]| -> BTreeMap<String, usize> {
        list.iter()
            .enumerate()
            .filter_map(|(i, e)| Some((e.get(field)?.as_str()?.to_owned(), i)))
            .collect()
    };
    let (ib, io, it) = (index(b), index(o), index(t));
    let mut remove: Vec<usize> = Vec::new();
    let mut add: Vec<usize> = Vec::new();
    let names: BTreeSet<&String> = ib.keys().chain(io.keys()).chain(it.keys()).collect();
    for name in names {
        let (eb, eo, et) = (
            ib.get(name).map(|&i| &b[i]),
            io.get(name).map(|&i| &o[i]),
            it.get(name).map(|&i| &t[i]),
        );
        if eo == et || eb == et {
            continue;
        }
        if eb == eo {
            match (io.get(name), it.get(name)) {
                (Some(&i), Some(&j)) => *out.get_mut(i)? = from.get(j)?.clone(),
                (None, Some(&j)) => add.push(j),
                (Some(&i), None) => remove.push(i),
                (None, None) => {}
            }
            continue;
        }
        // Changed on both sides: key by key, where both still have it.
        let (Some(Value::Table(to)), Some(Value::Table(tt))) = (eo, et) else {
            return None;
        };
        merge_table(
            eb.and_then(Value::as_table),
            to,
            tt,
            out.get_mut(io[name])?,
            from.get(it[name])?,
        )?;
    }
    // Theirs' new entries after ours, in their order; then the removals,
    // from the end (the places before them stay).
    add.sort_unstable();
    for j in add {
        out.push(from.get(j)?.clone());
    }
    remove.sort_unstable();
    for i in remove.into_iter().rev() {
        out.remove(i);
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "# Weather cases (invented)\nformat = \"ohl-weather-cases\"\nversion = 1\n\n\
        [[case]]\nname = \"everyday\"\ntemperature = \"10 °C\"\n\n\
        # the hot one\n[[case]]\nname = \"hot\"\ntemperature = \"80 °C\"\n";

    fn cases(text: &str) -> Vec<(String, String)> {
        let t: toml::Table = toml::from_str(text).unwrap();
        t["case"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                (
                    c["name"].as_str().unwrap().to_owned(),
                    c["temperature"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    }

    /// Each side adds a case, theirs changes another: all of it, ours'
    /// comments and order kept, theirs' new case with its comment.
    #[test]
    fn entries_added_and_changed_on_each_side_combine() {
        let ours = format!("{BASE}\n[[case]]\nname = \"ours cold\"\ntemperature = \"-20 °C\"\n");
        let theirs = BASE.replace("80 °C", "75 °C")
            + "\n# theirs' wind case\n[[case]]\nname = \"theirs wind\"\ntemperature = \"10 °C\"\n\
               wind_pressure = \"500 Pa\"\n";
        let m = merge(Some(BASE), &ours, &theirs).unwrap();
        assert_eq!(
            cases(&m),
            [
                ("everyday".to_owned(), "10 °C".to_owned()),
                ("hot".to_owned(), "75 °C".to_owned()),
                ("ours cold".to_owned(), "-20 °C".to_owned()),
                ("theirs wind".to_owned(), "10 °C".to_owned()),
            ]
        );
        assert!(m.starts_with("# Weather cases (invented)"), "{m}");
        assert!(m.contains("# theirs' wind case"), "{m}");
        assert!(m.contains("wind_pressure = \"500 Pa\""), "{m}");
        // The same either way round, as values.
        let w = merge(Some(BASE), &theirs, &ours).unwrap();
        let sorted = |t: &str| {
            let mut c = cases(t);
            c.sort();
            c
        };
        assert_eq!(sorted(&m), sorted(&w));
    }

    /// One entry, different values of it changed on each side: both; a
    /// case removed on one side and untouched on the other: removed.
    #[test]
    fn one_entry_merges_key_by_key() {
        let ours = BASE.replace(
            "name = \"hot\"\n",
            "name = \"hot\"\ndescription = \"maximum conductor temperature\"\n",
        );
        let theirs = BASE.replace("80 °C", "90 °C").replace(
            "[[case]]\nname = \"everyday\"\ntemperature = \"10 °C\"\n\n",
            "",
        );
        let m = merge(Some(BASE), &ours, &theirs).unwrap();
        let t: toml::Table = toml::from_str(&m).unwrap();
        let c = t["case"].as_array().unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0]["temperature"].as_str(), Some("90 °C"));
        assert_eq!(
            c[0]["description"].as_str(),
            Some("maximum conductor temperature")
        );
        // A plain key changed on one side, a table key by key.
        let base = "version = 1\n\n[wind]\nsteps = true\nstep = \"50 Pa\"\n";
        let ours = base.replace("version = 1", "version = 2");
        let theirs = base.replace("50 Pa", "25 Pa") + "sides = \"both\"\n";
        let m = merge(Some(base), &ours, &theirs).unwrap();
        let t: toml::Table = toml::from_str(&m).unwrap();
        assert_eq!(t["version"].as_integer(), Some(2));
        assert_eq!(t["wind"]["step"].as_str(), Some("25 Pa"));
        assert_eq!(t["wind"]["sides"].as_str(), Some("both"));
    }

    /// What does not combine by itself stays a conflict: the same value
    /// changed differently, an entry removed here and changed there, a
    /// list without names, the same new entry made differently, a text
    /// that is not TOML.
    #[test]
    fn what_does_not_combine_is_left_alone() {
        let a = BASE.replace("80 °C", "85 °C");
        let b = BASE.replace("80 °C", "90 °C");
        assert_eq!(merge(Some(BASE), &a, &b), None);
        let removed = BASE.replace(
            "# the hot one\n[[case]]\nname = \"hot\"\ntemperature = \"80 °C\"\n",
            "",
        );
        assert_eq!(merge(Some(BASE), &removed, &b), None);
        let base = "[[row]]\nx = 1\n";
        assert_eq!(
            merge(Some(base), "[[row]]\nx = 2\n", "[[row]]\nx = 3\n"),
            None
        );
        let new = |t: &str| format!("[[case]]\nname = \"n\"\ntemperature = \"{t}\"\n");
        assert_eq!(merge(None, &new("1 °C"), &new("2 °C")), None);
        // New on both sides with other entries: both.
        let other = "[[case]]\nname = \"m\"\ntemperature = \"3 °C\"\n";
        assert_eq!(cases(&merge(None, &new("1 °C"), other).unwrap()).len(), 2);
        assert_eq!(merge(Some(BASE), "not = [toml", &b), None);
        // Nothing to merge: ours as it is, to the letter.
        assert_eq!(merge(Some(BASE), &a, BASE).as_deref(), Some(a.as_str()));
    }
}
