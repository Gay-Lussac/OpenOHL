// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Sections of the section library (`.ohlsec`) typed in the structure
//! application: a section as typed ([`SectionDraft`]: its fields as text
//! with their units), checked and its properties computed by the section
//! library itself ([`SectionDraft::section`]), written into a library
//! file's text ([`put_section`], [`remove_section`]: `toml_edit`, so the
//! other sections, comments and layout stay). Only the shapes the library
//! computes ([`SHAPES`]); nothing here holds a profile's values.

use std::collections::BTreeMap;

use ohl_sections::{SECTION_FILE_FORMAT, SECTION_FILE_VERSION, Section, SectionLibrary};
use toml_edit::{DocumentMut, Item, Table};

/// A shape of the section library: its name in files, a title, its
/// dimensions (key, what it is) and, for `general`, the properties to
/// give.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeKind {
    /// Name in files (`equal_angle`, …).
    pub name: &'static str,
    /// In words.
    pub title: &'static str,
    /// Its dimensions: key and what it is.
    pub dims: &'static [(&'static str, &'static str)],
    /// Properties to give (a `general` section's area and second moments).
    pub given: &'static [(&'static str, &'static str)],
}

/// Every key a dimension may have (removed when the shape changes).
pub const DIMENSION_KEYS: [&str; 8] = ["h", "b", "t", "tw", "tf", "r", "d", "sides"];

/// The shapes the section library computes, in the order of the editor.
pub const SHAPES: [ShapeKind; 9] = [
    ShapeKind {
        name: "equal_angle",
        title: "Equal angle",
        dims: &[
            ("h", "leg length"),
            ("t", "thickness"),
            ("r", "root radius (empty: 0)"),
        ],
        given: &[],
    },
    ShapeKind {
        name: "unequal_angle",
        title: "Unequal angle",
        dims: &[
            ("h", "leg along z"),
            ("b", "leg along y"),
            ("t", "thickness"),
            ("r", "root radius (empty: 0)"),
        ],
        given: &[],
    },
    ShapeKind {
        name: "channel",
        title: "Channel",
        dims: &[
            ("h", "height"),
            ("b", "flange width"),
            ("tw", "web thickness"),
            ("tf", "flange thickness"),
            ("r", "root radius (empty: 0)"),
        ],
        given: &[],
    },
    ShapeKind {
        name: "i_section",
        title: "I or H section",
        dims: &[
            ("h", "height"),
            ("b", "flange width"),
            ("tw", "web thickness"),
            ("tf", "flange thickness"),
            ("r", "root radius (empty: 0)"),
        ],
        given: &[],
    },
    ShapeKind {
        name: "rectangular_hollow",
        title: "Rectangular tube",
        dims: &[("h", "height"), ("b", "width"), ("t", "wall")],
        given: &[],
    },
    ShapeKind {
        name: "circular_hollow",
        title: "Round tube",
        dims: &[("d", "outside diameter"), ("t", "wall")],
        given: &[],
    },
    ShapeKind {
        name: "round_bar",
        title: "Round bar",
        dims: &[("d", "diameter")],
        given: &[],
    },
    ShapeKind {
        name: "polygonal_hollow",
        title: "Polygonal tube (pole shaft)",
        dims: &[
            ("sides", "number of sides"),
            ("d", "width across flats"),
            ("t", "wall"),
        ],
        given: &[],
    },
    ShapeKind {
        name: "general",
        title: "General (properties given)",
        dims: &[("h", "outline height"), ("b", "outline width")],
        given: &[
            ("area", "area, e.g. 12 cm2"),
            ("iy", "second moment about y, e.g. 150 cm4"),
            ("iz", "second moment about z"),
        ],
    },
];

/// The shape named `name`.
pub fn shape_kind(name: &str) -> Option<&'static ShapeKind> {
    SHAPES.iter().find(|s| s.name == name)
}

/// A section as typed: every value text, with its unit (`61 mm`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SectionDraft {
    /// Id members refer to.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Free text.
    pub description: String,
    /// Shape name ([`SHAPES`]).
    pub shape: String,
    /// Material id (empty: the file's default material).
    pub material: String,
    /// Dimensions by key (`h`, `t`, …, `sides`), as typed.
    pub dims: BTreeMap<String, String>,
    /// Properties given by key (`area`, `iy`, `iz` for a general
    /// section), as typed.
    pub given: BTreeMap<String, String>,
}

/// A table's value as text: a string as it is, a number as written.
fn text_of(item: &Item) -> String {
    match item.as_str() {
        Some(s) => s.to_owned(),
        None => item.to_string().trim().to_owned(),
    }
}

/// The `[[section]]` table with id `id`.
fn section_table<'a>(doc: &'a DocumentMut, id: &str) -> Option<&'a Table> {
    doc.get("section")?
        .as_array_of_tables()?
        .iter()
        .find(|t| t.get("id").and_then(Item::as_str) == Some(id))
}

impl SectionDraft {
    /// A new section of shape `shape`, nothing typed.
    pub fn new(id: &str, shape: &str) -> Self {
        Self {
            id: id.to_owned(),
            shape: shape.to_owned(),
            ..Self::default()
        }
    }

    /// Section `id` of a library file, as text.
    pub fn read(doc: &DocumentMut, id: &str) -> Option<Self> {
        let t = section_table(doc, id)?;
        let get = |k: &str| t.get(k).map(text_of).unwrap_or_default();
        let shape = get("shape");
        let mut d = Self {
            id: id.to_owned(),
            name: get("name"),
            description: get("description"),
            shape: shape.clone(),
            material: get("material"),
            ..Self::default()
        };
        for k in DIMENSION_KEYS {
            if let Some(v) = t.get(k) {
                d.dims.insert(k.to_owned(), text_of(v));
            }
        }
        for (k, _) in shape_kind(&shape).map_or(&[][..], |s| s.given) {
            if let Some(v) = t.get(k) {
                d.given.insert((*k).to_owned(), text_of(v));
            }
        }
        Some(d)
    }

    /// The `[[section]]` table: `base` (the section as it is in the file:
    /// its other keys, e.g. properties given, and comments kept) with the
    /// fields typed set — empty ones removed, the dimensions the shape
    /// does not take removed.
    pub fn table(&self, base: Option<&Table>) -> Table {
        let mut t = base.cloned().unwrap_or_default();
        set(&mut t, "id", &self.id);
        set(&mut t, "name", &self.name);
        set(&mut t, "description", &self.description);
        set(&mut t, "shape", &self.shape);
        set(&mut t, "material", &self.material);
        let kind = shape_kind(&self.shape);
        for k in DIMENSION_KEYS {
            let takes = kind.is_some_and(|s| s.dims.iter().any(|d| d.0 == k));
            let v = if takes {
                self.dims.get(k).map_or("", String::as_str)
            } else {
                ""
            };
            set(&mut t, k, v);
        }
        if let Some(kind) = kind {
            for (k, _) in kind.given {
                set(&mut t, k, self.given.get(*k).map_or("", String::as_str));
            }
        }
        t
    }

    /// The section as the library reads it (checked by the library: a
    /// missing or wrong dimension, a shape it does not take, plates that
    /// do not fit), with the properties given in `base` (the file's
    /// table, if the section is there).
    pub fn section(&self, base: Option<&Table>) -> Result<Section, String> {
        if self.id.trim().is_empty() {
            return Err(NO_ID.into());
        }
        let mut doc = DocumentMut::new();
        let mut a = toml_edit::ArrayOfTables::new();
        a.push(self.table(base));
        doc.insert("section", Item::ArrayOfTables(a));
        let lib = SectionLibrary::parse(&doc.to_string()).map_err(|e| short_error(&e))?;
        lib.sections
            .into_iter()
            .next()
            .ok_or_else(|| "no section".to_owned())
    }
}

/// What a section without an id is told.
pub const NO_ID: &str = "type its id (letters, digits, - _ .)";

/// A file reader's error in one line: for a TOML error (a value that does
/// not read, e.g. a length without its unit), the key and the reason
/// without the file's lines around it.
pub fn short_error(e: &str) -> String {
    if !e.starts_with("TOML parse error") {
        return e.to_owned();
    }
    let key = e.lines().find_map(|l| {
        let (n, code) = l.split_once('|')?;
        n.trim().parse::<u32>().ok()?;
        Some(code.split('=').next()?.trim().to_owned())
    });
    let reason = e
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or(e);
    match key {
        Some(k) if !k.is_empty() => format!("{k}: {reason}"),
        _ => reason.to_owned(),
    }
}

/// Field `k` of `t` as typed; empty removes it. A value already there
/// keeps its decoration (a comment after it).
fn set(t: &mut Table, k: &str, v: &str) {
    let v = v.trim();
    if v.is_empty() {
        t.remove(k);
    } else if let Some(existing) = t.get_mut(k).and_then(Item::as_value_mut) {
        let decor = existing.decor().clone();
        *existing = typed(k, v);
        *existing.decor_mut() = decor;
    } else {
        t.insert(k, Item::Value(typed(k, v)));
    }
}

/// A value as typed for key `k`: `sides` a whole number, the rest text
/// (`61 mm`, a number without a unit is refused by the library).
fn typed(k: &str, v: &str) -> toml_edit::Value {
    if k == "sides"
        && let Ok(n) = v.parse::<i64>()
    {
        return n.into();
    }
    v.trim_matches('"').into()
}

/// The ids of the sections of a library file.
pub fn section_ids(doc: &DocumentMut) -> Vec<String> {
    crate::edits::item_ids(doc, "section")
}

/// Writes `draft` into a library file: over section `old` (in its place,
/// its other keys and comments kept) or, with none, as a new section at
/// the end. Refused: an id already in the file (another section's).
pub fn put_section(
    doc: &mut DocumentMut,
    old: Option<&str>,
    draft: &SectionDraft,
) -> Result<(), String> {
    let id = draft.id.trim();
    if section_ids(doc)
        .iter()
        .any(|s| s == id && Some(s.as_str()) != old)
    {
        return Err(format!("cross-section \"{id}\" is in the file already"));
    }
    match old {
        Some(old) => {
            let a = doc
                .get_mut("section")
                .and_then(Item::as_array_of_tables_mut)
                .ok_or_else(|| format!("no cross-section \"{old}\" in the file"))?;
            let t = a
                .iter_mut()
                .find(|t| t.get("id").and_then(Item::as_str) == Some(old))
                .ok_or_else(|| format!("no cross-section \"{old}\" in the file"))?;
            *t = draft.table(Some(t));
        }
        None => {
            if doc.get("section").is_none() {
                doc.insert("section", Item::ArrayOfTables(Default::default()));
            }
            doc.get_mut("section")
                .and_then(Item::as_array_of_tables_mut)
                .ok_or("[section] is not a list of tables")?
                .push(draft.table(None));
        }
    }
    Ok(())
}

/// Removes section `id` from a library file.
pub fn remove_section(doc: &mut DocumentMut, id: &str) -> Result<(), String> {
    if !section_ids(doc).iter().any(|s| s == id) {
        return Err(format!("no cross-section \"{id}\" in the file"));
    }
    crate::edits::remove_item(doc, "section", "id", id);
    Ok(())
}

/// The table of section `id` in a library file (for
/// [`SectionDraft::table`] and [`SectionDraft::section`]).
pub fn section_table_of<'a>(doc: &'a DocumentMut, id: &str) -> Option<&'a Table> {
    section_table(doc, id)
}

/// An id for a copy of `id` none of `taken` has: `id-copy`, `id-copy2`, …
pub fn copy_id<'a>(taken: impl Iterator<Item = &'a str> + Clone, id: &str) -> String {
    let base = format!("{id}-copy");
    let mut n = 1;
    loop {
        let c = if n == 1 {
            base.clone()
        } else {
            format!("{base}{n}")
        };
        if !taken.clone().any(|t| t == c) {
            return c;
        }
        n += 1;
    }
}

/// The keys of a material's values, as files write them, with what each
/// is (in the order of the editor).
pub const MATERIAL_KEYS: [(&str, &str); 5] = [
    ("elasticity", "Young's modulus E, e.g. 210 GPa (required)"),
    (
        "shear_modulus",
        "shear modulus G, e.g. 81 GPa (empty: from E and Poisson's 0.3, as the library does)",
    ),
    ("density", "density, e.g. 7850 kg/m3 (the members' weight)"),
    ("yield", "yield strength fy, e.g. 355 MPa"),
    ("ultimate", "ultimate strength fu, e.g. 510 MPa"),
];

/// A material as typed (roadmap U5, 2026-10-25): every value text with its
/// unit; the partial factors as `name = value, …`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MaterialDraft {
    /// Id sections refer to.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Free text.
    pub description: String,
    /// Values by key ([`MATERIAL_KEYS`]), as typed.
    pub values: BTreeMap<String, String>,
    /// Partial factors, `gamma_m0 = 1.0, gamma_m1 = 1.1`.
    pub partial_factors: String,
}

/// The `[[material]]` table with id `id`.
fn material_table<'a>(doc: &'a DocumentMut, id: &str) -> Option<&'a Table> {
    doc.get("material")?
        .as_array_of_tables()?
        .iter()
        .find(|t| t.get("id").and_then(Item::as_str) == Some(id))
}

/// `name = value, …` as an inline table of numbers.
fn factors_table(text: &str) -> Result<toml_edit::InlineTable, String> {
    let mut t = toml_edit::InlineTable::new();
    for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (k, v) = part
            .split_once('=')
            .ok_or_else(|| format!("partial factors: \"{part}\": write name = value"))?;
        let v: f64 = v
            .trim()
            .parse()
            .map_err(|_| format!("partial factors: \"{}\" is not a number", v.trim()))?;
        t.insert(k.trim(), v.into());
    }
    Ok(t)
}

impl MaterialDraft {
    /// Material `id` of a library file, as text.
    pub fn read(doc: &DocumentMut, id: &str) -> Option<Self> {
        let t = material_table(doc, id)?;
        let get = |k: &str| t.get(k).map(text_of).unwrap_or_default();
        let mut d = Self {
            id: id.to_owned(),
            name: get("name"),
            description: get("description"),
            ..Self::default()
        };
        for (k, _) in MATERIAL_KEYS {
            if let Some(v) = t.get(k) {
                d.values.insert(k.to_owned(), text_of(v));
            }
        }
        if let Some(f) = t.get("partial_factors").and_then(Item::as_inline_table) {
            d.partial_factors = f
                .iter()
                .map(|(k, v)| format!("{k} = {}", v.to_string().trim()))
                .collect::<Vec<_>>()
                .join(", ");
        }
        Some(d)
    }

    /// The `[[material]]` table: `base` (as it is in the file, its
    /// comments kept) with the fields typed set, empty ones removed.
    pub fn table(&self, base: Option<&Table>) -> Result<Table, String> {
        let mut t = base.cloned().unwrap_or_default();
        set(&mut t, "id", &self.id);
        set(&mut t, "name", &self.name);
        set(&mut t, "description", &self.description);
        for (k, _) in MATERIAL_KEYS {
            set(&mut t, k, self.values.get(k).map_or("", String::as_str));
        }
        let f = factors_table(&self.partial_factors)?;
        if f.is_empty() {
            t.remove("partial_factors");
        } else {
            t.insert("partial_factors", Item::Value(f.into()));
        }
        Ok(t)
    }

    /// The material as the library reads it (checked by the library).
    pub fn material(&self, base: Option<&Table>) -> Result<ohl_sections::Material, String> {
        if self.id.trim().is_empty() {
            return Err(NO_ID.into());
        }
        let mut doc = DocumentMut::new();
        let mut a = toml_edit::ArrayOfTables::new();
        a.push(self.table(base)?);
        doc.insert("material", Item::ArrayOfTables(a));
        let lib = SectionLibrary::parse(&doc.to_string()).map_err(|e| short_error(&e))?;
        lib.materials
            .into_iter()
            .next()
            .ok_or_else(|| "no material".to_owned())
    }
}

/// The ids of the materials of a library file.
pub fn material_ids(doc: &DocumentMut) -> Vec<String> {
    crate::edits::item_ids(doc, "material")
}

/// Writes `draft` into a library file: over material `old` (in its place,
/// its comments kept) or, with none, as a new one at the end; a rename
/// carries the file's sections and `default_material` naming it along.
/// Refused: an id already in the file (another material's).
pub fn put_material(
    doc: &mut DocumentMut,
    old: Option<&str>,
    draft: &MaterialDraft,
) -> Result<(), String> {
    let id = draft.id.trim();
    if material_ids(doc)
        .iter()
        .any(|m| m == id && Some(m.as_str()) != old)
    {
        return Err(format!("material \"{id}\" is in the file already"));
    }
    match old {
        Some(old) => {
            let a = doc
                .get_mut("material")
                .and_then(Item::as_array_of_tables_mut)
                .ok_or_else(|| format!("no material \"{old}\" in the file"))?;
            let t = a
                .iter_mut()
                .find(|t| t.get("id").and_then(Item::as_str) == Some(old))
                .ok_or_else(|| format!("no material \"{old}\" in the file"))?;
            *t = draft.table(Some(t))?;
            if old != id {
                if let Some(a) = doc
                    .get_mut("section")
                    .and_then(Item::as_array_of_tables_mut)
                {
                    for t in a.iter_mut() {
                        if t.get("material").and_then(Item::as_str) == Some(old) {
                            set(t, "material", id);
                        }
                    }
                }
                if doc.get("default_material").and_then(Item::as_str) == Some(old) {
                    doc["default_material"] = toml_edit::value(id);
                }
            }
        }
        None => {
            let t = draft.table(None)?;
            if doc.get("material").is_none() {
                doc.insert("material", Item::ArrayOfTables(Default::default()));
            }
            doc.get_mut("material")
                .and_then(Item::as_array_of_tables_mut)
                .ok_or("[material] is not a list of tables")?
                .push(t);
        }
    }
    Ok(())
}

/// The sections of a library file that name material `id` (and whether
/// it is the file's `default_material`).
pub fn material_users(doc: &DocumentMut, id: &str) -> (Vec<String>, bool) {
    let sections = doc
        .get("section")
        .and_then(Item::as_array_of_tables)
        .map(|a| {
            a.iter()
                .filter(|t| t.get("material").and_then(Item::as_str) == Some(id))
                .filter_map(|t| t.get("id").and_then(Item::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let default = doc.get("default_material").and_then(Item::as_str) == Some(id);
    (sections, default)
}

/// Removes material `id` from a library file. Refused while a section of
/// the file names it or it is the file's `default_material` (sections of
/// other files are the library's to report: a material missing).
pub fn remove_material(doc: &mut DocumentMut, id: &str) -> Result<(), String> {
    if !material_ids(doc).iter().any(|m| m == id) {
        return Err(format!("no material \"{id}\" in the file"));
    }
    let (sections, default) = material_users(doc, id);
    if !sections.is_empty() {
        return Err(format!(
            "material \"{id}\" is used by {}: give them another first",
            sections.join(", ")
        ));
    }
    if default {
        return Err(format!(
            "material \"{id}\" is the file's default_material: choose another first"
        ));
    }
    crate::edits::remove_item(doc, "material", "id", id);
    Ok(())
}

/// The table of material `id` in a library file (for
/// [`MaterialDraft::table`] and [`MaterialDraft::material`]).
pub fn material_table_of<'a>(doc: &'a DocumentMut, id: &str) -> Option<&'a Table> {
    material_table(doc, id)
}

/// The text of a new, empty section library file.
pub fn new_library_text(name: &str) -> String {
    format!(
        "format = \"{SECTION_FILE_FORMAT}\"\nversion = {SECTION_FILE_VERSION}\nname = \"{}\"\n",
        name.replace('"', "'")
    )
}

#[cfg(test)]
mod tests {
    //! Sections typed into a library file (dimensions invented); the
    //! properties against hand values.

    use super::*;

    const FILE: &str = "# my sections (invented)\n\
        format = \"ohl-sections\"\nversion = 1\ndefault_material = \"s\"\n\n\
        [[material]]\nid = \"s\"\nelasticity = \"210 GPa\"\ndensity = \"7850 kg/m3\"\n\n\
        [[section]]\nid = \"A\"\nshape = \"equal_angle\"\nh = \"50 mm\" # leg\nt = \"5 mm\"\n\
        iy = \"11 cm4\"\n";

    /// A material typed: written over the file's (its comment kept),
    /// renamed (the sections and default_material naming it follow),
    /// read back; partial factors as name = value; refused to delete while
    /// used; a value without its unit refused by the library.
    #[test]
    fn materials_are_typed() {
        let mut doc: DocumentMut = FILE
            .replace(
                "density = \"7850 kg/m3\"",
                "density = \"7850 kg/m3\" # steel",
            )
            .parse()
            .unwrap();
        let mut d = MaterialDraft::read(&doc, "s").unwrap();
        assert_eq!(d.values["elasticity"], "210 GPa");
        d.id = "S355".into();
        d.values.insert("yield".into(), "355 MPa".into());
        d.partial_factors = "gamma_m0 = 1.0, gamma_m1 = 1.1".into();
        let m = d.material(material_table_of(&doc, "s")).unwrap();
        assert_eq!(m.yield_strength, Some(355e6));
        assert_eq!(m.partial_factors["gamma_m1"], 1.1);
        put_material(&mut doc, Some("s"), &d).unwrap();
        let text = doc.to_string();
        assert!(text.contains("# steel"), "{text}");
        assert!(text.contains("default_material = \"S355\""), "{text}");
        let lib = SectionLibrary::parse(&text).unwrap();
        assert_eq!(lib.section("A").unwrap().material.as_deref(), Some("S355"));
        assert_eq!(
            MaterialDraft::read(&doc, "S355").unwrap().partial_factors,
            d.partial_factors
        );
        assert!(remove_material(&mut doc, "S355").is_err());
        let mut n = MaterialDraft {
            id: "alu".into(),
            ..Default::default()
        };
        n.values.insert("elasticity".into(), "70".into());
        assert!(n.material(None).is_err(), "a value without its unit");
        n.values.insert("elasticity".into(), "70 GPa".into());
        put_material(&mut doc, None, &n).unwrap();
        assert!(put_material(&mut doc, None, &n).is_err(), "the id twice");
        remove_material(&mut doc, "alu").unwrap();
        assert_eq!(material_ids(&doc), ["S355"]);
        assert!(
            MaterialDraft {
                partial_factors: "gamma".into(),
                ..n
            }
            .table(None)
            .is_err()
        );
    }

    fn draft(id: &str, shape: &str, dims: &[(&str, &str)]) -> SectionDraft {
        let mut d = SectionDraft::new(id, shape);
        for (k, v) in dims {
            d.dims.insert((*k).into(), (*v).into());
        }
        d
    }

    #[test]
    fn a_new_section_is_added_and_computed() {
        let mut doc: DocumentMut = FILE.parse().unwrap();
        // Equal angle 60 × 6, no root radius: A = 60·6 + 54·6 = 684 mm².
        let d = draft("L60", "equal_angle", &[("h", "60 mm"), ("t", "6 mm")]);
        let s = d.section(None).unwrap();
        assert!((s.properties().area - 684e-6).abs() < 1e-12);
        put_section(&mut doc, None, &d).unwrap();
        let text = doc.to_string();
        assert!(text.starts_with("# my sections (invented)"), "{text}");
        assert!(text.contains("h = \"50 mm\" # leg"), "{text}");
        let lib = SectionLibrary::parse(&text).unwrap();
        assert_eq!(section_ids(&doc), ["A", "L60"]);
        let read = lib.section("L60").unwrap();
        assert_eq!(read.material.as_deref(), Some("s"), "the default material");
        assert!((read.properties().area - 684e-6).abs() < 1e-12);
        // Read back as typed.
        assert_eq!(SectionDraft::read(&doc, "L60").unwrap(), d);
    }

    #[test]
    fn an_edit_keeps_its_place_and_what_was_given() {
        let mut doc: DocumentMut = FILE.parse().unwrap();
        let mut d = SectionDraft::read(&doc, "A").unwrap();
        assert_eq!(d.dims["h"], "50 mm");
        // Thicker, and a round bar after: the angle's dimensions go.
        d.dims.insert("t".into(), "6 mm".into());
        let base = section_table_of(&doc, "A").cloned();
        assert!(d.section(base.as_ref()).is_ok());
        put_section(&mut doc, Some("A"), &d).unwrap();
        assert!(doc.to_string().contains("t = \"6 mm\""));
        let mut bar = d.clone();
        bar.shape = "round_bar".into();
        bar.dims = BTreeMap::from([("d".into(), "20 mm".into())]);
        bar.id = "B".into();
        put_section(&mut doc, Some("A"), &bar).unwrap();
        let text = doc.to_string();
        let lib = SectionLibrary::parse(&text).unwrap();
        let s = lib.section("B").unwrap();
        assert!(matches!(s.shape, ohl_sections::Shape::RoundBar { .. }));
        // The given iy stays: its value replaces the computed one.
        assert!((s.properties().iy - 11e-8).abs() < 1e-15, "{text}");
        assert!(!text.contains("h = "), "{text}");
        assert_eq!(section_ids(&doc), ["B"]);
    }

    #[test]
    fn mistakes_are_said_and_ids_stay_unique() {
        let mut doc: DocumentMut = FILE.parse().unwrap();
        let e = draft("X", "equal_angle", &[("h", "60 mm")])
            .section(None)
            .unwrap_err();
        assert!(e.contains("needs t"), "{e}");
        let e = draft("X", "equal_angle", &[("h", "60 mm"), ("t", "70 mm")])
            .section(None)
            .unwrap_err();
        assert!(e.contains("must not exceed a leg"), "{e}");
        let e = draft("X", "equal_angle", &[("h", "60"), ("t", "6 mm")])
            .section(None)
            .unwrap_err();
        assert!(e.starts_with("h: ") && !e.contains('\n'), "{e}");
        assert_eq!(
            draft("", "round_bar", &[]).section(None).unwrap_err(),
            NO_ID
        );
        let e = draft("X", "general", &[("h", "60 mm"), ("b", "60 mm")])
            .section(None)
            .unwrap_err();
        assert!(e.contains("needs area"), "{e}");
        let mut g = draft("G", "general", &[("h", "60 mm"), ("b", "60 mm")]);
        for (k, v) in [("area", "10 cm2"), ("iy", "100 cm4"), ("iz", "50 cm4")] {
            g.given.insert(k.into(), v.into());
        }
        assert!((g.section(None).unwrap().properties().area - 1e-3).abs() < 1e-15);
        let e = draft("bad id", "round_bar", &[("d", "20 mm")])
            .section(None)
            .unwrap_err();
        assert!(e.contains("letters, digits"), "{e}");
        // Polygonal: sides typed as a number.
        let p = draft(
            "P",
            "polygonal_hollow",
            &[("sides", "8"), ("d", "300 mm"), ("t", "5 mm")],
        );
        assert!(p.section(None).is_ok());
        assert!(p.table(None).to_string().contains("sides = 8"));
        // A second "A" is refused; a copy gets a free id.
        let e =
            put_section(&mut doc, None, &draft("A", "round_bar", &[("d", "1 mm")])).unwrap_err();
        assert!(e.contains("already"), "{e}");
        let ids = section_ids(&doc);
        assert_eq!(copy_id(ids.iter().map(String::as_str), "A"), "A-copy");
        let taken = ["A", "A-copy"];
        assert_eq!(copy_id(taken.into_iter(), "A"), "A-copy2");
        remove_section(&mut doc, "A").unwrap();
        assert!(section_ids(&doc).is_empty());
        assert!(remove_section(&mut doc, "A").is_err());
        // Every shape of the editor is one the library reads.
        for s in SHAPES {
            assert!(
                ohl_sections::wind::SHAPE_NAMES.contains(&s.name),
                "{}",
                s.name
            );
        }
        let text = new_library_text("Typed");
        assert!(SectionLibrary::parse(&text).is_ok());
    }
}
