// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The edits of a structure's pole (`[pole]`, `ohl_structures::pole`):
//! its outline rows (height, diameter), its shape, its level 3 fields,
//! and a named `.ohlp` pole brought into the file. Each is one function
//! on the file's text (one undo step through
//! [`crate::StructureFile::edit`]); comments and the rest stay.

use ohl_structures::pole::SOLID;
use ohl_structures::{PoleRef, PoleShape, PoleWall, StructureType};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

use crate::body::ensure_version;

/// Distance under which two heights are one, m.
const SAME: f64 = 1e-3;

/// A length in metres as written: `"12.500 m"`.
fn meters(v: f64) -> Item {
    toml_edit::value(format!("{v:.3} m"))
}

/// A diameter as written: `"290 mm"` (to a tenth of a millimetre).
fn millimetres(d: f64) -> Item {
    let mm = (d * 10_000.0).round() / 10.0;
    toml_edit::value(format!("{mm} mm"))
}

/// The `[pole]` table (made when there is none; a `pole = "id"` stays
/// until it is brought in: none then).
fn table(doc: &mut DocumentMut) -> Option<&mut Table> {
    if doc.get("pole").is_none() {
        ensure_version(doc);
        let mut t = Table::new();
        t.set_implicit(false);
        doc.insert("pole", Item::Table(t));
    }
    doc.get_mut("pole")?.as_table_mut()
}

/// The `[[pole.outline]]` rows (made when missing).
fn rows(doc: &mut DocumentMut) -> Option<&mut ArrayOfTables> {
    let t = table(doc)?;
    if t.get("outline")
        .and_then(Item::as_array_of_tables)
        .is_none()
    {
        t.insert("outline", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    t.get_mut("outline")?.as_array_of_tables_mut()
}

fn ring(z: f64, d: f64) -> Table {
    let mut r = Table::new();
    r.insert("z", meters(z));
    r.insert("diameter", millimetres(d));
    r
}

/// The outline as read: (local z, diameter), m.
fn outline(t: &StructureType) -> Vec<(f64, f64)> {
    t.pole
        .as_ref()
        .and_then(PoleRef::inline)
        .map(|p| p.rings())
        .unwrap_or_default()
}

/// Adds a height to the outline: without one, two heights (the feet and
/// the top, 400 and 200 mm); with one, the top; else a height half-way up
/// the highest step, its diameter between. Its row (or why not).
pub fn add_outline_row(doc: &mut DocumentMut, t: &StructureType) -> Result<usize, String> {
    if matches!(t.pole, Some(PoleRef::File(_))) {
        return Err("the pole is a .ohlp file: bring it into this file first".into());
    }
    let rings = outline(t);
    let foot = t.foot_level();
    let top = t
        .height
        .map(|h| h.si())
        .unwrap_or_else(|| {
            t.points
                .iter()
                .map(|p| p.position()[2].si())
                .fold(foot + 10.0, f64::max)
        })
        .max(foot + 1.0);
    let r = rows(doc).ok_or("no [pole] table")?;
    match rings.as_slice() {
        [] => {
            r.push(ring(foot, 0.4));
            r.push(ring(top, 0.2));
            Ok(1)
        }
        [one] => {
            let z = if top > one.0 + SAME { top } else { one.0 + 1.0 };
            r.push(ring(z, one.1));
            Ok(1)
        }
        [.., a, b] => {
            let at = rings.len() - 1;
            let mid = ring(0.5 * (a.0 + b.0), 0.5 * (a.1 + b.1));
            // Inserted before the top row: rebuilt in order.
            let mut all: Vec<Table> = r.iter().cloned().collect();
            all.insert(at, mid);
            r.clear();
            for t in all {
                r.push(t);
            }
            Ok(at)
        }
    }
}

/// Row `row` of the outline at height `z` with diameter `d`, m (written
/// as a diameter); refused when the heights would no longer rise.
pub fn set_outline_row(
    doc: &mut DocumentMut,
    t: &StructureType,
    row: usize,
    z: f64,
    d: f64,
) -> Result<(), String> {
    let mut rings = outline(t);
    if row >= rings.len() {
        return Err(format!("no outline row {}", row + 1));
    }
    if d.is_nan() || d <= 0.0 {
        return Err("a diameter above 0".into());
    }
    rings[row] = (z, d);
    if rings.windows(2).any(|w| w[1].0 - w[0].0 <= SAME) {
        return Err("the heights must rise from row to row".into());
    }
    let r = rows(doc).ok_or("no [pole] table")?;
    let x = r.get_mut(row).ok_or("no such row")?;
    x.insert("z", meters(z));
    x.remove("circumference");
    x.insert("diameter", millimetres(d));
    Ok(())
}

/// Removes row `row` of the outline.
pub fn remove_outline_row(doc: &mut DocumentMut, row: usize) {
    if let Some(r) = doc
        .get_mut("pole")
        .and_then(Item::as_table_mut)
        .and_then(|t| t.get_mut("outline"))
        .and_then(Item::as_array_of_tables_mut)
        && row < r.len()
    {
        r.remove(row);
    }
}

/// The cross-section: round, or a polygon of `sides`.
pub fn set_shape(doc: &mut DocumentMut, shape: PoleShape, sides: u32) {
    let Some(t) = table(doc) else {
        return;
    };
    match shape {
        PoleShape::Round => {
            t.remove("shape");
            t.remove("sides");
        }
        PoleShape::Polygon => {
            t.insert("shape", toml_edit::value("polygon"));
            t.insert("sides", toml_edit::value(i64::from(sides.max(3))));
        }
    }
}

/// The level 3 fields and `embedment` a pole's table may hold, with how
/// they are typed: a text with its unit, else a number.
pub const POLE_FIELDS: [(&str, &str); 9] = [
    ("material", "wood, steel, concrete or other"),
    ("wall", "e.g. 8 mm, or solid (none: from the section)"),
    ("section", "a hollow section of the section library"),
    ("fibre_stress", "wood: e.g. 55 MPa"),
    ("moment_capacity", "at the ground line, e.g. 900 kN m"),
    ("elasticity", "e.g. 210 GPa (P-Delta)"),
    ("density", "e.g. 7850 kg/m3 (the pole's weight)"),
    ("drag", "a number: shape factor on the wind (default 1)"),
    ("embedment", "depth in the ground, e.g. 3 m"),
];

/// Field `field` of the pole (one of [`POLE_FIELDS`]) as typed: `drag` a
/// number, the others a text; empty removes it.
pub fn set_field(doc: &mut DocumentMut, field: &str, typed: &str) -> Result<(), String> {
    if !POLE_FIELDS.iter().any(|f| f.0 == field) {
        return Err(format!("pole: no field {field}"));
    }
    let typed = typed.trim();
    let t = table(doc).ok_or("the pole is a .ohlp file: bring it into this file first")?;
    if typed.is_empty() {
        t.remove(field);
        return Ok(());
    }
    if field == "drag" {
        let v: f64 = typed
            .parse()
            .map_err(|_| format!("drag: a number, not \"{typed}\""))?;
        t.insert(field, toml_edit::value(v));
    } else if field == "wall" {
        // A length or "solid"; a word else refused here, not in the file.
        let typed = typed.trim_matches('"');
        match PoleWall::from_value(&toml::Value::String(typed.to_owned()))? {
            PoleWall::Solid => t.insert(field, toml_edit::value(SOLID)),
            PoleWall::Thickness(_) => t.insert(field, toml_edit::value(typed)),
        };
    } else {
        t.insert(field, toml_edit::value(typed.trim_matches('"')));
    }
    Ok(())
}

/// Whether the pole's table says it is solid (`wall = "solid"`).
pub fn is_solid(doc: &DocumentMut) -> bool {
    doc.get("pole")
        .and_then(|p| p.get("wall"))
        .and_then(Item::as_str)
        .is_some_and(|s| s.trim().eq_ignore_ascii_case(SOLID))
}

/// The pole solid (`wall = "solid"`: no hollow, level 3's wall) or not
/// (a `"solid"` removed: the wall is not given again; a typed thickness
/// stays).
pub fn set_solid(doc: &mut DocumentMut, solid: bool) -> Result<(), String> {
    if solid {
        let t = table(doc).ok_or("the pole is a .ohlp file: bring it into this file first")?;
        t.insert("wall", toml_edit::value(SOLID));
    } else if is_solid(doc)
        && let Some(t) = doc.get_mut("pole").and_then(Item::as_table_mut)
    {
        t.remove("wall");
    }
    Ok(())
}

/// Removes the pole (its table, or the `.ohlp` named).
pub fn remove_pole(doc: &mut DocumentMut) {
    doc.remove("pole");
}

/// Brings the `.ohlp` pole named by `pole = "id"` into the file (`text`:
/// that file's text): its diameters at the ground line and the top
/// become a two-height outline from the feet, its other fields are
/// copied (the pole check gives the same numbers).
pub fn bring_in(doc: &mut DocumentMut, t: &StructureType, text: &str) -> Result<(), String> {
    let file: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let length = |k: &str| -> Result<Option<f64>, String> {
        file.get(k)
            .map(|v| {
                v.clone()
                    .try_into::<ohl_core::units::Length>()
                    .map(|l| l.si())
                    .map_err(|e| format!("{k}: {}", e.message()))
            })
            .transpose()
    };
    let pi = std::f64::consts::PI;
    let diameter = |what: &str| -> Result<f64, String> {
        match (
            length(&format!("{what}_diameter"))?,
            length(&format!("{what}_circumference"))?,
        ) {
            (Some(d), None) => Ok(d),
            (None, Some(c)) => Ok(c / pi),
            _ => Err(format!("{what}_diameter or {what}_circumference")),
        }
    };
    let (ground, top) = (diameter("ground")?, diameter("top")?);
    let total = length("length")?.ok_or("no length")?;
    let embedment = length("embedment")?.unwrap_or(0.0);
    let foot = t.foot_level();
    let mut pole = Table::new();
    pole.set_implicit(false);
    if embedment > 0.0 {
        pole.insert("embedment", meters(embedment));
    }
    for key in [
        "material",
        "wall",
        "fibre_stress",
        "moment_capacity",
        "elasticity",
        "density",
        "drag",
    ] {
        if let Some(v) = file.get(key).and_then(plain) {
            pole.insert(key, v);
        }
    }
    let mut outline = ArrayOfTables::new();
    outline.push(ring(foot, ground));
    outline.push(ring(foot + total - embedment, top));
    pole.insert("outline", Item::ArrayOfTables(outline));
    if let Some(caps) = file.get("capacity").and_then(toml::Value::as_array) {
        let mut a = ArrayOfTables::new();
        for c in caps {
            let mut row = Table::new();
            for (k, v) in c.as_table().into_iter().flatten() {
                if let Some(v) = plain(v) {
                    row.insert(k, v);
                }
            }
            a.push(row);
        }
        pole.insert("capacity", Item::ArrayOfTables(a));
    }
    ensure_version(doc);
    doc.remove("pole");
    doc.insert("pole", Item::Table(pole));
    Ok(())
}

/// A text, number or truth value of a `.ohlp` file as written here.
fn plain(v: &toml::Value) -> Option<Item> {
    match v {
        toml::Value::String(s) => Some(toml_edit::value(s.as_str())),
        toml::Value::Float(f) => Some(toml_edit::value(*f)),
        toml::Value::Integer(i) => Some(toml_edit::value(*i)),
        toml::Value::Boolean(b) => Some(toml_edit::value(*b)),
        _ => None,
    }
}
