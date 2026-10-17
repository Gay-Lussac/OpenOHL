// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The edits of a structure's load-case recipes (`[[load_case]]` and
//! `[[load_case.load]]`): cases and loads by their place in the file
//! (from 0), fields as typed. The recipes are read and checked by
//! `ols-structure-loads`; here only the text changes, comments kept.

use ols_core::units::{Force, Stress};
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};

use crate::edits::next_id;

/// The fields of a case, in the order shown.
pub const CASE_FIELDS: [&str; 5] = [
    "name",
    "description",
    "line_case",
    "strength",
    "self_weight",
];

/// The fields of a load, in the order shown.
pub const LOAD_FIELDS: [&str; 13] = [
    "what",
    "value",
    "pressure",
    "line_case",
    "factor",
    "one_sided",
    "side",
    "direction",
    "points",
    "sets",
    "phases",
    "members",
    "note",
];

/// A recipe as written in the file: each field as text (lists joined by
/// `; `), whether it reads or not.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CaseText {
    /// The case's fields (name, description, …), as written.
    pub fields: Vec<(String, String)>,
    /// Its loads, each its fields as written.
    pub loads: Vec<Vec<(String, String)>>,
}

impl CaseText {
    /// Field `name` of the case (empty: not given).
    pub fn get(&self, name: &str) -> &str {
        field(&self.fields, name)
    }

    /// Field `name` of load `load` (empty: not given).
    pub fn load(&self, load: usize, name: &str) -> &str {
        self.loads.get(load).map_or("", |l| field(l, name))
    }
}

fn field<'a>(fields: &'a [(String, String)], name: &str) -> &'a str {
    fields
        .iter()
        .find(|(k, _)| k == name)
        .map_or("", |(_, v)| v.as_str())
}

/// A value as text: a string as it is, a list joined by `; `.
fn text(item: &Item) -> String {
    match item {
        Item::Value(Value::String(s)) => s.value().clone(),
        Item::Value(Value::Array(a)) => a
            .iter()
            .map(|v| match v {
                Value::String(s) => s.value().clone(),
                v => v.to_string().trim().to_owned(),
            })
            .collect::<Vec<_>>()
            .join("; "),
        i => i.to_string().trim().to_owned(),
    }
}

fn fields_of(t: &Table) -> Vec<(String, String)> {
    t.iter()
        .filter(|(_, v)| !v.is_array_of_tables() && !v.is_table())
        .map(|(k, v)| (k.to_owned(), text(v)))
        .collect()
}

/// The recipes of the file, as written.
pub fn load_cases(doc: &DocumentMut) -> Vec<CaseText> {
    let Some(cases) = doc.get("load_case").and_then(Item::as_array_of_tables) else {
        return Vec::new();
    };
    cases
        .iter()
        .map(|c| CaseText {
            fields: fields_of(c),
            loads: c
                .get("load")
                .and_then(Item::as_array_of_tables)
                .map(|a| a.iter().map(fields_of).collect())
                .unwrap_or_default(),
        })
        .collect()
}

fn cases_mut(doc: &mut DocumentMut) -> Option<&mut ArrayOfTables> {
    doc.get_mut("load_case")?.as_array_of_tables_mut()
}

fn case_mut(doc: &mut DocumentMut, case: usize) -> Result<&mut Table, String> {
    cases_mut(doc)
        .and_then(|a| a.get_mut(case))
        .ok_or_else(|| format!("no load case {}", case + 1))
}

fn load_mut(doc: &mut DocumentMut, case: usize, load: usize) -> Result<&mut Table, String> {
    case_mut(doc, case)?
        .get_mut("load")
        .and_then(Item::as_array_of_tables_mut)
        .and_then(|a| a.get_mut(load))
        .ok_or_else(|| format!("load case {}: no load {}", case + 1, load + 1))
}

fn names(doc: &DocumentMut) -> Vec<String> {
    load_cases(doc)
        .iter()
        .map(|c| c.get("name").to_owned())
        .collect()
}

/// A new case without loads, its name the next free after `like`
/// (`case 1` → `case 2`); its place and name.
pub fn add_load_case(doc: &mut DocumentMut, like: &str) -> (usize, String) {
    let taken = names(doc);
    let name = if taken.iter().any(|n| n == like) {
        next_id(taken.iter().map(String::as_str), like)
    } else {
        like.to_owned()
    };
    let mut t = Table::new();
    t.insert("name", toml_edit::value(name.clone()));
    if doc
        .get("load_case")
        .and_then(Item::as_array_of_tables)
        .is_none()
    {
        doc.insert("load_case", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    let a = cases_mut(doc).expect("made above");
    a.push(t);
    (a.len() - 1, name)
}

/// Removes case `case` with its loads.
pub fn remove_load_case(doc: &mut DocumentMut, case: usize) {
    if let Some(a) = cases_mut(doc)
        && case < a.len()
    {
        a.remove(case);
        if a.is_empty() {
            doc.remove("load_case");
        }
    }
}

/// A new load at the end of case `case`: the line's vertical load at
/// every point (a valid load to start from); its place.
pub fn add_load(doc: &mut DocumentMut, case: usize) -> Result<usize, String> {
    let c = case_mut(doc, case)?;
    if c.get("load").and_then(Item::as_array_of_tables).is_none() {
        c.insert("load", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    let a = c
        .get_mut("load")
        .and_then(Item::as_array_of_tables_mut)
        .expect("made above");
    let mut t = Table::new();
    t.insert("what", toml_edit::value("vertical"));
    t.insert("points", toml_edit::value("all"));
    a.push(t);
    Ok(a.len() - 1)
}

/// Removes load `load` of case `case`.
pub fn remove_load(doc: &mut DocumentMut, case: usize, load: usize) {
    if let Ok(c) = case_mut(doc, case)
        && let Some(a) = c.get_mut("load").and_then(Item::as_array_of_tables_mut)
        && load < a.len()
    {
        a.remove(load);
        if a.is_empty() {
            c.remove("load");
        }
    }
}

fn number(typed: &str, what: &str) -> Result<f64, String> {
    typed
        .parse::<f64>()
        .ok()
        .filter(|x| x.is_finite())
        .ok_or_else(|| format!("{what}: \"{typed}\" is not a number"))
}

/// A number as written: an integer when it is one.
fn number_value(x: f64) -> Item {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        toml_edit::value(x as i64)
    } else {
        toml_edit::value(x)
    }
}

/// `"all"` or a list (separated by `;` or `,`).
fn pick(typed: &str) -> Item {
    if typed == "all" {
        return toml_edit::value("all");
    }
    let mut a = Array::new();
    for s in list(typed) {
        a.push(s);
    }
    Item::Value(Value::Array(a))
}

fn list(typed: &str) -> Vec<&str> {
    typed
        .split([';', ','])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// Field `field` of case `case` as typed (empty removes it; the name
/// stays and is unique). `strength` above 0, `self_weight` 0 or more.
pub fn set_load_case_field(
    doc: &mut DocumentMut,
    case: usize,
    field: &str,
    typed: &str,
) -> Result<(), String> {
    let typed = typed.trim();
    let value = match field {
        "name" => {
            if typed.is_empty() {
                return Err("a load case needs a name".into());
            }
            let taken = names(doc);
            if taken
                .iter()
                .enumerate()
                .any(|(k, n)| k != case && n == typed)
            {
                return Err(format!("a load case \"{typed}\" exists already"));
            }
            Some(toml_edit::value(typed))
        }
        "description" | "line_case" => (!typed.is_empty()).then(|| toml_edit::value(typed)),
        "strength" | "self_weight" if typed.is_empty() => None,
        "strength" => {
            let x = number(typed, field)?;
            if x <= 0.0 {
                return Err(format!("strength {x}: above 0"));
            }
            Some(number_value(x))
        }
        "self_weight" => {
            let x = number(typed, field)?;
            if x < 0.0 {
                return Err(format!("self_weight {x}: 0 or more"));
            }
            Some(number_value(x))
        }
        other => return Err(format!("a load case has no field \"{other}\"")),
    };
    let c = case_mut(doc, case)?;
    match value {
        Some(v) => {
            c.insert(field, v);
        }
        None => {
            c.remove(field);
        }
    }
    Ok(())
}

/// Field `field` of load `load` of case `case` as typed; empty removes
/// it. A value takes a force (`2 kN`; a plain number is N), a pressure a
/// pressure (`0.6 kPa`; plain: Pa), `one_sided` a share (`0.4` or
/// `40 %`), `points` and `members` `all` or labels, `sets` numbers and
/// `phases` labels (lists separated by `;` or `,`). Whether the load as
/// a whole makes sense is checked when the recipes are read.
pub fn set_load_field(
    doc: &mut DocumentMut,
    case: usize,
    load: usize,
    field: &str,
    typed: &str,
) -> Result<(), String> {
    let typed = typed.trim();
    let value = if typed.is_empty() {
        None
    } else {
        Some(match field {
            "what" | "line_case" | "side" | "direction" | "note" => toml_edit::value(typed),
            "value" => match typed.parse::<f64>() {
                Ok(x) if x.is_finite() => number_value(x),
                _ => {
                    typed
                        .parse::<Force>()
                        .map_err(|e| format!("value \"{typed}\": {e}"))?;
                    toml_edit::value(typed)
                }
            },
            "pressure" => match typed.parse::<f64>() {
                Ok(x) if x.is_finite() => number_value(x),
                _ => {
                    typed
                        .parse::<Stress>()
                        .map_err(|e| format!("pressure \"{typed}\": {e}"))?;
                    toml_edit::value(typed)
                }
            },
            "factor" => number_value(number(typed, field)?),
            "one_sided" => match typed.strip_suffix('%') {
                Some(p) => {
                    number(p.trim(), field)?;
                    toml_edit::value(typed)
                }
                None => number_value(number(typed, field)?),
            },
            "points" | "members" => pick(typed),
            "sets" => {
                let mut a = Array::new();
                for s in list(typed) {
                    let n: i64 = s
                        .parse()
                        .ok()
                        .filter(|n| *n >= 0)
                        .ok_or_else(|| format!("sets: \"{s}\" is not a set number"))?;
                    a.push(n);
                }
                Item::Value(Value::Array(a))
            }
            "phases" => {
                let mut a = Array::new();
                for s in list(typed) {
                    a.push(s);
                }
                Item::Value(Value::Array(a))
            }
            other => return Err(format!("a load has no field \"{other}\"")),
        })
    };
    if !LOAD_FIELDS.contains(&field) {
        return Err(format!("a load has no field \"{field}\""));
    }
    let l = load_mut(doc, case, load)?;
    match value {
        Some(v) => {
            l.insert(field, v);
        }
        None => {
            l.remove(field);
        }
    }
    Ok(())
}

/// Takes the cases of a structure load-case file's text (`.olsslc`, or
/// any text with `[[load_case]]` tables): a case with the name of one in
/// the file replaces it there, the others are added at the end. How many
/// were added and replaced.
pub fn merge_load_cases(doc: &mut DocumentMut, text: &str) -> Result<(usize, usize), String> {
    let other: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    let Some(new) = other.get("load_case").and_then(Item::as_array_of_tables) else {
        return Ok((0, 0));
    };
    let (mut added, mut replaced) = (0, 0);
    for t in new.iter() {
        let mut t = t.clone();
        // Placed by the file it goes into, not the one it comes from.
        t.set_position(None);
        if let Some(loads) = t.get_mut("load").and_then(Item::as_array_of_tables_mut) {
            for l in loads.iter_mut() {
                l.set_position(None);
            }
        }
        let name = t
            .get("name")
            .and_then(Item::as_str)
            .unwrap_or("")
            .to_owned();
        let at = names(doc).iter().position(|n| *n == name);
        if doc
            .get("load_case")
            .and_then(Item::as_array_of_tables)
            .is_none()
        {
            doc.insert("load_case", Item::ArrayOfTables(ArrayOfTables::new()));
        }
        let a = cases_mut(doc).expect("made above");
        match at.and_then(|k| a.get_mut(k)) {
            Some(old) => {
                // Keeps its place (and the comments before it).
                let decor = old.decor().clone();
                let position = old.position();
                *old = t;
                *old.decor_mut() = decor;
                old.set_position(position);
                replaced += 1;
            }
            None => {
                a.push(t);
                added += 1;
            }
        }
    }
    Ok((added, replaced))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use ols_structures::{LibraryItem, StructureType};

    use super::*;

    const FILE: &str = r#"id = "P"
name = "p"
kind = "lattice"

[[point]]
label = "A1"
point = ["0 m", "0 m", "10 m"]

# The cases (a comment kept).
[[load_case]]
name = "wind"
line_case = "wind"

[[load_case.load]]
what = "transverse_factored"
points = "all"

[[load_case]]
name = "ice"

[[load_case.load]]
value = "2 kN"
direction = "y"
points = ["A1"]

# After the cases.
[[wire]]
label = "W1"
set = 1
"#;

    fn doc() -> DocumentMut {
        FILE.parse().unwrap()
    }

    fn reads(doc: &DocumentMut) -> StructureType {
        StructureType::parse(Path::new("p.olss"), &doc.to_string()).unwrap()
    }

    #[test]
    fn the_recipes_read_as_written() {
        let cases = load_cases(&doc());
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].get("name"), "wind");
        assert_eq!(cases[0].load(0, "what"), "transverse_factored");
        assert_eq!(cases[1].load(0, "value"), "2 kN");
        assert_eq!(cases[1].load(0, "points"), "A1");
        assert_eq!(cases[1].load(0, "factor"), "");
    }

    #[test]
    fn a_load_added_to_the_first_case_stays_in_it() {
        let mut d = doc();
        assert_eq!(add_load(&mut d, 0), Ok(1));
        let t = reads(&d);
        let first = &t.load_cases[0];
        let loads = first["load"].as_array().unwrap();
        assert_eq!(loads.len(), 2);
        assert_eq!(loads[1]["what"].as_str(), Some("vertical"));
        assert_eq!(t.load_cases[1]["load"].as_array().unwrap().len(), 1);
        // Comments stay; the wire is still a wire.
        let text = d.to_string();
        assert!(text.contains("# The cases (a comment kept)."), "{text}");
        assert!(text.contains("# After the cases."), "{text}");
        assert_eq!(t.wires.len(), 1);
    }

    #[test]
    fn cases_are_added_named_and_removed() {
        let mut d = doc();
        let (k, name) = add_load_case(&mut d, "case 1");
        assert_eq!((k, name.as_str()), (2, "case 1"));
        let (_, again) = add_load_case(&mut d, "case 1");
        assert_eq!(again, "case 2");
        assert_eq!(add_load(&mut d, 2), Ok(0));
        set_load_field(&mut d, 2, 0, "factor", "1.3").unwrap();
        let t = reads(&d);
        assert_eq!(t.load_cases.len(), 4);
        assert_eq!(t.load_cases[2]["load"][0]["factor"].as_float(), Some(1.3));
        // A name taken is refused; an empty one too.
        assert!(set_load_case_field(&mut d, 2, "name", "wind").is_err());
        assert!(set_load_case_field(&mut d, 2, "name", " ").is_err());
        set_load_case_field(&mut d, 2, "name", "broken wire").unwrap();
        set_load_case_field(&mut d, 2, "strength", "0.9").unwrap();
        assert!(set_load_case_field(&mut d, 2, "strength", "0").is_err());
        assert!(set_load_case_field(&mut d, 2, "self_weight", "x").is_err());
        assert_eq!(load_cases(&d)[2].get("name"), "broken wire");
        assert_eq!(load_cases(&d)[2].get("strength"), "0.9");
        remove_load(&mut d, 2, 0);
        assert!(load_cases(&d)[2].loads.is_empty());
        remove_load_case(&mut d, 0);
        assert_eq!(load_cases(&d)[0].get("name"), "ice");
        for k in (0..3).rev() {
            remove_load_case(&mut d, k);
        }
        assert!(d.get("load_case").is_none());
        assert_eq!(reads(&d).wires.len(), 1);
    }

    #[test]
    fn load_fields_take_units_lists_and_shares() {
        let mut d = doc();
        set_load_field(&mut d, 1, 0, "value", "1.5 kN").unwrap();
        set_load_field(&mut d, 1, 0, "one_sided", "40 %").unwrap();
        set_load_field(&mut d, 1, 0, "side", "ahead").unwrap();
        set_load_field(&mut d, 1, 0, "points", "").unwrap();
        set_load_field(&mut d, 1, 0, "sets", "1; 11").unwrap();
        set_load_field(&mut d, 1, 0, "phases", "L1, L2").unwrap();
        set_load_field(&mut d, 1, 0, "note", "a note").unwrap();
        let c = &load_cases(&d)[1];
        assert_eq!(c.load(0, "value"), "1.5 kN");
        assert_eq!(c.load(0, "one_sided"), "40 %");
        assert_eq!(c.load(0, "points"), "");
        assert_eq!(c.load(0, "sets"), "1; 11");
        assert_eq!(c.load(0, "phases"), "L1; L2");
        set_load_field(&mut d, 1, 0, "points", "all").unwrap();
        set_load_field(&mut d, 1, 0, "value", "2000").unwrap();
        assert_eq!(load_cases(&d)[1].load(0, "value"), "2000");
        assert!(set_load_field(&mut d, 1, 0, "value", "2 m").is_err());
        assert!(set_load_field(&mut d, 1, 0, "pressure", "fast").is_err());
        assert!(set_load_field(&mut d, 1, 0, "sets", "one").is_err());
        assert!(set_load_field(&mut d, 1, 0, "factor", "x").is_err());
        assert!(set_load_field(&mut d, 1, 0, "colour", "red").is_err());
        assert!(set_load_field(&mut d, 1, 5, "factor", "1").is_err());
        let t = reads(&d);
        assert_eq!(t.load_cases[1]["load"][0]["sets"][1].as_integer(), Some(11));
    }

    #[test]
    fn cases_imported_replace_or_add() {
        let mut d = doc();
        let text = r#"format = "ols-structure-load-cases"
version = 1

[[load_case]]
name = "ice"
self_weight = 1.1

[[load_case.load]]
value = "3 kN"
direction = "-z"
points = "all"

[[load_case]]
name = "construction"

[[load_case.load]]
value = "1 kN"
direction = "-z"
points = "all"
"#;
        assert_eq!(merge_load_cases(&mut d, text), Ok((1, 1)));
        let cases = load_cases(&d);
        assert_eq!(
            cases.iter().map(|c| c.get("name")).collect::<Vec<_>>(),
            ["wind", "ice", "construction"]
        );
        assert_eq!(cases[1].load(0, "value"), "3 kN");
        assert_eq!(cases[1].get("self_weight"), "1.1");
        let out = d.to_string();
        assert!(out.contains("# The cases (a comment kept)."), "{out}");
        // The file still reads, the cases in their order.
        let t = reads(&d);
        assert_eq!(t.load_cases[2]["name"].as_str(), Some("construction"));
        assert_eq!(t.wires.len(), 1);
        assert!(merge_load_cases(&mut d, "not [toml").is_err());
    }
}
