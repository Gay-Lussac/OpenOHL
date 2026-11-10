// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What the two sides of a conflict hold, in readable terms
//! ([`crate::merge_conflicts`]): a position as a move in metres, a
//! tension, sag, temperature or orientation with its unit, a list as its
//! count and how many of its entries differ — instead of the stored JSON,
//! which is shown (cut short) only for fields not known here.
//!
//! Pure functions of the stored values: nothing of the project is read.

use std::collections::BTreeMap;

use serde_json::Value;

/// The longest stored value shown as it is (characters).
const RAW_LENGTH: usize = 60;

/// The longest list of names or numbers shown entry by entry.
const LISTED: usize = 6;

/// A field's values in words: in the common revision, on this line, on
/// the other side — the two sides as changes against the common one where
/// that says more (a move, a difference). `kind`: the object's kind
/// (`structure`…); `path`: the field (`stringing.horizontal_tension`).
/// `None`: the field is not there.
pub fn describe_field(
    kind: &str,
    path: &str,
    base: Option<&Value>,
    ours: Option<&Value>,
    theirs: Option<&Value>,
) -> [String; 3] {
    [
        match base {
            Some(v) => value_text(kind, path, v),
            None => "(not there)".to_owned(),
        },
        change_text(kind, path, base, ours),
        change_text(kind, path, base, theirs),
    ]
}

/// A field's name as read: `stringing.horizontal_tension` is "stringing ›
/// horizontal tension", `orientation_deg` "orientation".
pub fn field_label(path: &str) -> String {
    path.split('.')
        .map(|p| p.strip_suffix("_deg").unwrap_or(p).replace('_', " "))
        .collect::<Vec<_>>()
        .join(" › ")
}

/// The last name of a path (`horizontal_tension` of
/// `stringing.horizontal_tension`).
fn leaf(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

/// The unit a number of field `leaf` of an object of kind `kind` is shown
/// in: its name, what the stored value is multiplied by, the decimals.
fn unit(kind: &str, leaf: &str) -> Option<(&'static str, f64, usize)> {
    match (kind, leaf) {
        (_, "sag" | "length" | "height_adjust" | "bundle_spacing") => Some(("m", 1.0, 2)),
        // Stored in newtons.
        (_, "horizontal_tension" | "tension") => Some(("kN", 1e-3, 2)),
        (_, "temperature") => Some(("°C", 1.0, 1)),
        (_, l) if l.ends_with("_deg") => Some(("°", 1.0, 1)),
        _ => None,
    }
}

/// A position: three numbers under `position`.
fn position(leaf: &str, v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    if leaf != "position" || a.len() != 3 {
        return None;
    }
    Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?])
}

/// A stored value cut to [`RAW_LENGTH`] characters.
fn raw(v: &Value) -> String {
    let t = v.to_string();
    if t.chars().count() > RAW_LENGTH {
        format!("{}…", t.chars().take(RAW_LENGTH).collect::<String>())
    } else {
        t
    }
}

/// A name or a number of a list, as it is (`None`: something larger).
fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(if *b { "yes" } else { "no" }.to_owned()),
        _ => None,
    }
}

/// "4 points", "1 string": the entries of list `leaf`.
fn count_text(leaf: &str, n: usize) -> String {
    let (one, many) = match leaf {
        "points" => ("point", "points"),
        "strings" => ("string", "strings"),
        "wires" => ("wire point", "wire points"),
        "members" => ("member", "members"),
        "lengths" => ("length", "lengths"),
        "span_points" => ("span point", "span points"),
        "spreads" => ("spread object", "spread objects"),
        _ => ("entry", "entries"),
    };
    format!("{n} {}", if n == 1 { one } else { many })
}

/// A list: its few names or numbers, else how many entries it has.
fn list_text(leaf: &str, a: &[Value]) -> String {
    if a.is_empty() {
        return "none".to_owned();
    }
    if a.len() <= LISTED
        && let Some(names) = a.iter().map(scalar).collect::<Option<Vec<_>>>()
    {
        return names.join(", ");
    }
    count_text(leaf, a.len())
}

/// A value in words, with its unit where the field is known.
pub fn value_text(kind: &str, path: &str, v: &Value) -> String {
    let leaf = leaf(path);
    match v {
        Value::Null => "none".to_owned(),
        Value::Bool(b) => if *b { "yes" } else { "no" }.to_owned(),
        Value::String(s) if s.is_empty() => "(empty)".to_owned(),
        Value::String(s) => s.clone(),
        Value::Number(n) => match (unit(kind, leaf), n.as_f64()) {
            (Some((name, k, decimals)), Some(x)) => format!("{:.decimals$} {name}", x * k),
            _ => n.to_string(),
        },
        Value::Array(a) => match position(leaf, v) {
            Some([x, y, z]) => format!("{x:.2}, {y:.2}, {z:.2} m"),
            None => list_text(leaf, a),
        },
        Value::Object(_) => raw(v),
    }
}

/// A move in words: "moved 4.0 m east, 1.2 m north, 0.3 m up". Parts
/// under half a millimetre are left out; millimetres are shown when a
/// part is under 5 cm.
pub fn moved_text(d: [f64; 3]) -> String {
    const NOTHING: f64 = 0.0005;
    let parts: Vec<(f64, &str)> = [
        (d[0], if d[0] >= 0.0 { "east" } else { "west" }),
        (d[1], if d[1] >= 0.0 { "north" } else { "south" }),
        (d[2], if d[2] >= 0.0 { "up" } else { "down" }),
    ]
    .into_iter()
    .filter(|(x, _)| x.abs() >= NOTHING)
    .collect();
    if parts.is_empty() {
        return "moved less than 1 mm".to_owned();
    }
    let decimals = if parts.iter().any(|(x, _)| x.abs() < 0.05) {
        3
    } else {
        1
    };
    let words: Vec<String> = parts
        .iter()
        .map(|(x, to)| format!("{:.decimals$} m {to}", x.abs()))
        .collect();
    format!("moved {}", words.join(", "))
}

/// A side's value as a change against the common one (`base`): a move, a
/// difference, how many entries of a list differ; else the value.
fn change_text(kind: &str, path: &str, base: Option<&Value>, side: Option<&Value>) -> String {
    let Some(side) = side else {
        return if base.is_some() {
            "removed".to_owned()
        } else {
            "(not there)".to_owned()
        };
    };
    let Some(base) = base else {
        return value_text(kind, path, side);
    };
    if base == side {
        return "unchanged".to_owned();
    }
    let leaf = leaf(path);
    if let (Some(a), Some(b)) = (position(leaf, base), position(leaf, side)) {
        return moved_text([b[0] - a[0], b[1] - a[1], b[2] - a[2]]);
    }
    match (base, side) {
        (Value::Number(a), Value::Number(b)) => {
            let text = value_text(kind, path, side);
            match (unit(kind, leaf), a.as_f64(), b.as_f64()) {
                (Some((_, k, decimals)), Some(a), Some(b)) => {
                    format!("{text} ({:+.decimals$})", (b - a) * k)
                }
                _ => text,
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            let text = list_text(leaf, b);
            // A few names: said as they are.
            if b.is_empty() || (b.len() <= LISTED && b.iter().all(|x| scalar(x).is_some())) {
                text
            } else if a.len() == b.len() {
                let differ = a.iter().zip(b).filter(|(x, y)| x != y).count();
                format!("{text}, {differ} differ")
            } else {
                format!("{text} (was {})", a.len())
            }
        }
        _ => value_text(kind, path, side),
    }
}

/// The fields of a stored object by path (`stringing.sag`): objects are
/// gone into, lists and empty objects are fields — as the merge compares
/// them (`ohl_session::versions`).
pub fn fields(v: &Value) -> BTreeMap<String, &Value> {
    fn walk<'a>(v: &'a Value, path: &mut Vec<&'a str>, out: &mut BTreeMap<String, &'a Value>) {
        match v {
            Value::Object(m) if !m.is_empty() => {
                for (k, x) in m {
                    path.push(k);
                    walk(x, path, out);
                    path.pop();
                }
            }
            other => {
                out.insert(path.join("."), other);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(v, &mut Vec::new(), &mut out);
    out
}

/// The paths of the fields two stored objects differ in (one of them
/// lacking a field counts), in order.
pub fn differing(a: &Value, b: &Value) -> Vec<String> {
    let (fa, fb) = (fields(a), fields(b));
    let mut out: Vec<String> = fa
        .iter()
        .filter(|(p, v)| fb.get(*p) != Some(v))
        .map(|(p, _)| p.clone())
        .collect();
    out.extend(fb.keys().filter(|p| !fa.contains_key(*p)).cloned());
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_position_reads_as_a_move() {
        let base = json!([2600000.0, 1200000.0, 430.0]);
        let ours = json!([2600004.0, 1200001.2, 430.3]);
        let theirs = json!([2599998.0, 1200000.0, 429.5]);
        let [common, this, other] = describe_field(
            "structure",
            "position",
            Some(&base),
            Some(&ours),
            Some(&theirs),
        );
        assert_eq!(common, "2600000.00, 1200000.00, 430.00 m");
        assert_eq!(this, "moved 4.0 m east, 1.2 m north, 0.3 m up");
        assert_eq!(other, "moved 2.0 m west, 0.5 m down");
    }

    #[test]
    fn small_moves_are_said_in_millimetres() {
        assert_eq!(moved_text([0.012, 0.0, 0.0]), "moved 0.012 m east");
        assert_eq!(moved_text([0.0, 0.0001, 0.0]), "moved less than 1 mm");
        assert_eq!(moved_text([0.0, -3.0, 0.0]), "moved 3.0 m south");
    }

    #[test]
    fn known_fields_carry_their_unit() {
        // A tension is stored in newtons, shown in kN, with its change.
        let d = describe_field(
            "section",
            "stringing.horizontal_tension",
            Some(&json!(12000.0)),
            Some(&json!(12500.0)),
            Some(&json!(11000.0)),
        );
        assert_eq!(d, ["12.00 kN", "12.50 kN (+0.50)", "11.00 kN (-1.00)"]);
        let d = describe_field(
            "section",
            "stringing.sag",
            Some(&json!(8.0)),
            Some(&json!(8.25)),
            Some(&json!(7.5)),
        );
        assert_eq!(d, ["8.00 m", "8.25 m (+0.25)", "7.50 m (-0.50)"]);
        let d = describe_field(
            "jumper",
            "temperature",
            Some(&json!(15.0)),
            Some(&json!(10.0)),
            Some(&json!(20.0)),
        );
        assert_eq!(d, ["15.0 °C", "10.0 °C (-5.0)", "20.0 °C (+5.0)"]);
        let d = describe_field(
            "structure",
            "orientation_deg",
            Some(&json!(90.0)),
            Some(&json!(92.5)),
            Some(&json!(90.0)),
        );
        assert_eq!(d, ["90.0 °", "92.5 ° (+2.5)", "unchanged"]);
    }

    #[test]
    fn a_type_or_cable_reads_by_its_name() {
        let d = describe_field(
            "structure",
            "type",
            Some(&json!("S1")),
            Some(&json!("S2")),
            None,
        );
        assert_eq!(d, ["S1", "S2", "removed"]);
        let d = describe_field(
            "section",
            "cable",
            None,
            Some(&json!("AAAC 550")),
            Some(&json!("ACSR 490/65")),
        );
        assert_eq!(d, ["(not there)", "AAAC 550", "ACSR 490/65"]);
    }

    #[test]
    fn lists_are_counted_not_dumped() {
        let p = |x: f64| json!({"structure": "T1", "set": 1, "offset": x});
        let base = json!([p(0.0), p(1.0), p(2.0), p(3.0)]);
        let ours = json!([p(0.0), p(1.5), p(2.0), p(3.5)]);
        let theirs = json!([p(0.0), p(1.0), p(2.0)]);
        let d = describe_field("section", "points", Some(&base), Some(&ours), Some(&theirs));
        assert_eq!(d, ["4 points", "4 points, 2 differ", "3 points (was 4)"]);
        // A few names are said as they are.
        let d = describe_field(
            "structure",
            "routes",
            Some(&json!(["R1"])),
            Some(&json!(["R1", "R2"])),
            Some(&json!([])),
        );
        assert_eq!(d, ["R1", "R1, R2", "none"]);
    }

    #[test]
    fn unknown_fields_fall_back_to_the_stored_value_cut_short() {
        let long = json!({"a": "x".repeat(100)});
        let d = describe_field("structure", "fields.plugin", None, Some(&long), None);
        assert!(d[1].ends_with('…') && d[1].chars().count() == RAW_LENGTH + 1);
        // A number without a known unit: as it is.
        let d = describe_field(
            "section",
            "bundle",
            Some(&json!(1)),
            Some(&json!(2)),
            Some(&json!(4)),
        );
        assert_eq!(d, ["1", "2", "4"]);
    }

    #[test]
    fn field_names_read_as_words() {
        assert_eq!(
            field_label("stringing.horizontal_tension"),
            "stringing › horizontal tension"
        );
        assert_eq!(field_label("orientation_deg"), "orientation");
    }

    #[test]
    fn the_fields_two_objects_differ_in() {
        let a = json!({"id": "T1", "position": [0.0, 0.0, 0.0], "stringing": {"sag": 8.0}});
        let b = json!({"id": "T1", "position": [1.0, 0.0, 0.0], "stringing": {"sag": 8.0}, "type": "S"});
        assert_eq!(differing(&a, &b), ["position", "type"]);
    }
}
