// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The **staking table** (owner, 2026-11-14: "make a staking table"): what
//! a survey crew needs to set the structures out on site, from the line as
//! it is.
//!
//! - **One row per structure and alignment** ([`StakingRow`]), in station
//!   order along each alignment (a structure on two alignments is in
//!   both): its type and extension, its **station** and **offset** from the
//!   alignment, the **line angle** there, the **back and ahead spans**
//!   (horizontal, along the alignment's run), its **easting, northing and
//!   elevation** (the structure's centre at the ground, as placed), the
//!   **ground** there (the terrain surface given) and the difference, its
//!   height adjustment (the feet above that point: foundations) and the
//!   **bearing** of its ahead axis. Structures on no alignment follow,
//!   without a station.
//! - **One row per foot** of a typed structure whose type has feet
//!   ([`StakingFoot`]): its supported joints, with the structure's
//!   extension (its own joints, a leg extension's lowered leg;
//!   [`ohl_structures::StructureType::extended`]), placed as the structure
//!   places its type; the leg's letter (A to D by quarter, as the leg
//!   extensions name them), its offset in the structure's axes, its
//!   easting, northing and elevation, the ground there and how far the
//!   foot stands above it (negative: below the ground).
//!
//! The coordinates are the line's (its CRS and height datum, said in the
//! notes). Nothing is computed that the line does not hold: no foundation
//! design, no excavation.

use ohl_structures::StructureLibrary;
use ohl_structures::extensions::LEGS;
use ohl_structures::structures::to_global;

use crate::{Line, Point, Structure};

/// One structure on one alignment (or on none).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct StakingRow {
    /// The alignment's name (empty: on no alignment).
    pub alignment: String,
    /// The structure.
    pub structure: String,
    /// Its type and extension (none: an untyped structure, no extension).
    pub structure_type: Option<String>,
    /// Its extension.
    pub extension: Option<String>,
    /// Horizontal distance along the alignment, m (none: on no alignment).
    pub station: Option<f64>,
    /// Horizontal distance from the alignment, m, positive to the right
    /// looking ahead.
    pub offset: Option<f64>,
    /// The line angle there, degrees, positive turning right (0 at the
    /// alignment's ends and off its run).
    pub line_angle_deg: f64,
    /// Horizontal span to the structure before it on the alignment's run,
    /// m (none: the first, or off the run).
    pub back_span: Option<f64>,
    /// The same to the structure after it.
    pub ahead_span: Option<f64>,
    /// Its centre at the ground as placed: easting, northing, elevation, m.
    pub position: Point,
    /// The ground there (the terrain given), m; none: no ground there.
    pub ground: Option<f64>,
    /// Its height adjustment, m: the type's feet above its position
    /// (foundations).
    pub height_adjust: f64,
    /// The bearing of its ahead axis, degrees clockwise from grid north
    /// (0–360); its cross-arms are square to it.
    pub bearing_deg: f64,
}

impl StakingRow {
    /// The structure's elevation above the ground there, m (none: no
    /// ground): 0 when it stands on the terrain.
    pub fn above_ground(&self) -> Option<f64> {
        self.ground.map(|g| self.position[2] - g)
    }
}

/// One foot of a typed structure.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct StakingFoot {
    /// The structure.
    pub structure: String,
    /// The joint (its id in the type).
    pub joint: String,
    /// Its leg: A (right, ahead), B (left, ahead), C (left, back), D
    /// (right, back) — the quarters the leg extensions name; empty on the
    /// structure's axes (a pole's foot, a joint on an axis).
    pub leg: String,
    /// Its offset from the structure's centre in the structure's axes:
    /// right, ahead, m.
    pub local: [f64; 2],
    /// Where it is: easting, northing, elevation, m.
    pub position: Point,
    /// The ground there, m; none: no ground there.
    pub ground: Option<f64>,
}

impl StakingFoot {
    /// How far the foot stands above the ground there, m (negative:
    /// below it; none: no ground).
    pub fn above_ground(&self) -> Option<f64> {
        self.ground.map(|g| self.position[2] - g)
    }
}

/// The staking table of a line.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Staking {
    /// The structures, alignment by alignment in station order, then
    /// those on no alignment.
    pub rows: Vec<StakingRow>,
    /// The feet of the typed structures, in the order of [`Self::rows`]'s
    /// first appearance of each.
    pub feet: Vec<StakingFoot>,
    /// What was assumed or left out, in words.
    pub notes: Vec<String>,
}

/// The bearing of a structure's ahead axis for its orientation (its
/// local x, degrees counter-clockwise from east): degrees clockwise from
/// north, 0–360.
pub fn bearing_ahead(orientation_deg: f64) -> f64 {
    // (+ 0.0: no "-0".)
    let b = (-orientation_deg).rem_euclid(360.0) + 0.0;
    if b >= 360.0 - 1e-9 { 0.0 } else { b }
}

/// The leg a foot at `local` (right, ahead) is: A to D by quarter, empty
/// within 1 mm of an axis.
fn leg_of(local: [f64; 2]) -> String {
    LEGS.iter()
        .find(|(_, s)| local[0] * s[0] > 1e-3 && local[1] * s[1] > 1e-3)
        .map_or(String::new(), |(l, _)| (*l).to_owned())
}

/// The feet of typed structure `s` of type library `types` ([`StakingFoot`]),
/// or why there are none.
fn feet_of(
    s: &Structure,
    types: &StructureLibrary,
    ground: &dyn Fn(f64, f64) -> Option<f64>,
) -> Result<Vec<StakingFoot>, String> {
    let Some(id) = &s.structure_type else {
        return Ok(Vec::new());
    };
    let t = types
        .get(id)
        .ok_or_else(|| format!("{}: no structure type \"{id}\" in the library", s.id))?;
    let body = t
        .extended(s.extension.as_deref())
        .map_err(|e| format!("{}: {e}", s.id))?;
    let lift = t
        .lift(s.extension.as_deref())
        .map_err(|e| format!("{}: {e}", s.id))?;
    let base = crate::links::feet(s);
    Ok(body
        .nodes
        .iter()
        .filter(|n| n.support.is_some())
        .map(|n| {
            let p = n.point.map(|v| v.si());
            let position = to_global(base, s.orientation_deg, [p[0], p[1], p[2] + lift]);
            StakingFoot {
                structure: s.id.clone(),
                joint: n.id.clone(),
                leg: leg_of([p[0], p[1]]),
                local: [p[0], p[1]],
                ground: ground(position[0], position[1]),
                position,
            }
        })
        .collect())
}

/// The staking table of `line` (resolved or not: its structures as
/// placed), the feet from the types of `types` (none: no feet), the ground
/// from `ground` (easting, northing → elevation; none there: left empty).
pub fn staking(
    line: &Line,
    types: Option<&StructureLibrary>,
    ground: &dyn Fn(f64, f64) -> Option<f64>,
) -> Staking {
    let mut out = Staking::default();
    let mut listed: Vec<&str> = Vec::new();
    let row = |s: &Structure, alignment: &str| StakingRow {
        alignment: alignment.to_owned(),
        structure: s.id.clone(),
        structure_type: s.structure_type.clone(),
        extension: s.extension.clone(),
        station: None,
        offset: None,
        line_angle_deg: 0.0,
        back_span: None,
        ahead_span: None,
        position: s.position,
        ground: ground(s.position[0], s.position[1]),
        height_adjust: s.height_adjust,
        bearing_deg: bearing_ahead(s.orientation_deg),
    };
    for view in line.alignment_views() {
        let stations = match view.stations(line) {
            Ok(st) => st,
            Err(e) => {
                out.notes.push(format!("alignment {}: {e}", view.name));
                continue;
            }
        };
        let mut rows: Vec<StakingRow> = Vec::with_capacity(stations.len());
        // The run along the alignment: the ahead span of one is the back
        // span of the next.
        let mut back: Option<f64> = None;
        for st in &stations {
            let Some(s) = line.structure(&st.structure) else {
                continue;
            };
            let name = if view.name.is_empty() {
                &view.id
            } else {
                &view.name
            };
            let mut r = row(s, name);
            // (Rounding noise below a micrometre or a microdegree: 0.)
            let clean = |v: f64| if v.abs() < 1e-6 { 0.0 } else { v };
            r.station = Some(clean(st.station));
            r.offset = Some(clean(st.offset));
            r.line_angle_deg = clean(st.line_angle_deg);
            if view.away.contains(&st.structure) {
                // Off the run: a station and an offset only.
            } else {
                r.back_span = back;
                r.ahead_span = (st.ahead_span > 0.0).then_some(st.ahead_span);
                back = r.ahead_span;
            }
            rows.push(r);
            if !listed.contains(&s.id.as_str()) {
                listed.push(&s.id);
            }
        }
        rows.sort_by(|a, b| {
            a.station
                .unwrap_or(0.0)
                .total_cmp(&b.station.unwrap_or(0.0))
        });
        out.rows.extend(rows);
    }
    let alone: Vec<&Structure> = line
        .structures
        .iter()
        .filter(|s| !listed.contains(&s.id.as_str()))
        .collect();
    if !alone.is_empty() {
        out.notes.push(format!(
            "{} structure(s) on no alignment: listed last, without a station",
            alone.len()
        ));
    }
    for s in &alone {
        out.rows.push(row(s, ""));
        listed.push(&s.id);
    }
    // The feet, each structure once.
    let typed = line
        .structures
        .iter()
        .filter(|s| s.structure_type.is_some())
        .count();
    match types {
        None if typed > 0 => out.notes.push(format!(
            "{typed} typed structure(s): their feet not listed (no structure library)"
        )),
        None => {}
        Some(types) => {
            let mut without = 0;
            for id in &listed {
                let Some(s) = line.structure(id) else {
                    continue;
                };
                match feet_of(s, types, ground) {
                    Ok(f) if f.is_empty() && s.structure_type.is_some() => without += 1,
                    Ok(f) => out.feet.extend(f),
                    Err(e) => out.notes.push(format!("feet of {e}")),
                }
            }
            if without > 0 {
                out.notes.push(format!(
                    "{without} typed structure(s) whose type has no supported joints (a type \
                     below level 3, or without supports): their centre only"
                ));
            }
        }
    }
    if out.rows.iter().any(|r| r.ground.is_none()) {
        let n = out.rows.iter().filter(|r| r.ground.is_none()).count();
        out.notes.push(format!(
            "{n} row(s) without ground under the structure: no terrain surface there"
        ));
    }
    let crs = if line.crs.is_empty() {
        "not given".to_owned()
    } else {
        line.crs.clone()
    };
    let datum = line
        .height_datum
        .as_ref()
        .map_or("not given".to_owned(), |d| d.name.clone());
    out.notes.push(format!(
        "coordinates in the line's CRS ({crs}), elevations in its height datum ({datum}); \
         bearings from grid north"
    ));
    out
}

/// `v` to `digits` decimals, without a minus before a zero ("-0.00").
fn num(v: f64, digits: usize) -> String {
    let t = format!("{v:.digits$}");
    match t.strip_prefix('-') {
        Some(rest) if rest.chars().all(|c| c == '0' || c == '.') => rest.to_owned(),
        _ => t,
    }
}

fn f(v: Option<f64>, digits: usize) -> String {
    v.map_or(String::new(), |v| num(v, digits))
}

/// A line angle in words: "10.00° L", "3.50° R", "0" (below the
/// hundredth of a degree shown).
pub fn angle_text(deg: f64) -> String {
    if deg.abs() < 0.005 {
        "0".to_owned()
    } else {
        format!("{:.2}° {}", deg.abs(), if deg > 0.0 { "R" } else { "L" })
    }
}

impl Staking {
    /// The structures as CSV: `alignment,structure,type,extension,station,
    /// offset,line_angle_deg,back_span,ahead_span,easting,northing,
    /// elevation,ground,above_ground,height_adjust,bearing_deg` (m and
    /// degrees; empty: not there).
    pub fn csv(&self) -> String {
        use std::fmt::Write;
        let q = ohl_core::text::csv_field;
        let mut out = String::from(
            "alignment,structure,type,extension,station,offset,line_angle_deg,back_span,\
             ahead_span,easting,northing,elevation,ground,above_ground,height_adjust,bearing_deg\n",
        );
        for r in &self.rows {
            let _ = writeln!(
                out,
                "{},{},{},{},{},{},{},{},{},{:.3},{:.3},{:.3},{},{},{},{}",
                q(&r.alignment),
                q(&r.structure),
                q(r.structure_type.as_deref().unwrap_or("")),
                q(r.extension.as_deref().unwrap_or("")),
                f(r.station, 3),
                f(r.offset, 3),
                num(r.line_angle_deg, 4),
                f(r.back_span, 3),
                f(r.ahead_span, 3),
                r.position[0],
                r.position[1],
                r.position[2],
                f(r.ground, 3),
                f(r.above_ground(), 3),
                num(r.height_adjust, 3),
                num(r.bearing_deg, 4),
            );
        }
        out
    }

    /// The feet as CSV: `structure,joint,leg,right,ahead,easting,northing,
    /// elevation,ground,above_ground` (m).
    pub fn feet_csv(&self) -> String {
        use std::fmt::Write;
        let q = ohl_core::text::csv_field;
        let mut out = String::from(
            "structure,joint,leg,right,ahead,easting,northing,elevation,ground,above_ground\n",
        );
        for p in &self.feet {
            let _ = writeln!(
                out,
                "{},{},{},{},{},{:.3},{:.3},{:.3},{},{}",
                q(&p.structure),
                q(&p.joint),
                q(&p.leg),
                num(p.local[0], 3),
                num(p.local[1], 3),
                p.position[0],
                p.position[1],
                p.position[2],
                f(p.ground, 3),
                f(p.above_ground(), 3),
            );
        }
        out
    }

    /// The table as text, for the command line.
    pub fn text(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{:<14} {:<10} {:<14} {:>10} {:>8} {:>9} {:>9} {:>13} {:>13} {:>9} {:>9} {:>7}",
            "alignment",
            "structure",
            "type",
            "station",
            "offset",
            "angle",
            "ahead",
            "easting",
            "northing",
            "elev.",
            "ground",
            "bearing"
        );
        for r in &self.rows {
            let angle = angle_text(r.line_angle_deg);
            let ty = match (&r.structure_type, &r.extension) {
                (Some(t), Some(e)) => format!("{t} {e}"),
                (Some(t), None) => t.clone(),
                _ => String::new(),
            };
            let _ = writeln!(
                out,
                "{:<14} {:<10} {:<14} {:>10} {:>8} {:>9} {:>9} {:>13.3} {:>13.3} {:>9.3} {:>9} {:>7}",
                r.alignment,
                r.structure,
                ty,
                f(r.station, 2),
                f(r.offset, 2),
                angle,
                f(r.ahead_span, 2),
                r.position[0],
                r.position[1],
                r.position[2],
                f(r.ground, 3),
                num(r.bearing_deg, 2)
            );
        }
        if !self.feet.is_empty() {
            let _ = writeln!(
                out,
                "\n{:<10} {:<14} {:<4} {:>8} {:>8} {:>13} {:>13} {:>9} {:>9} {:>8}",
                "structure",
                "joint",
                "leg",
                "right",
                "ahead",
                "easting",
                "northing",
                "elev.",
                "ground",
                "above"
            );
            for p in &self.feet {
                let _ = writeln!(
                    out,
                    "{:<10} {:<14} {:<4} {:>8} {:>8} {:>13.3} {:>13.3} {:>9.3} {:>9} {:>8}",
                    p.structure,
                    p.joint,
                    p.leg,
                    num(p.local[0], 3),
                    num(p.local[1], 3),
                    p.position[0],
                    p.position[1],
                    p.position[2],
                    f(p.ground, 3),
                    f(p.above_ground(), 3)
                );
            }
        }
        for n in &self.notes {
            let _ = writeln!(out, "note: {n}");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bearings: a structure whose cross-arms point east (orientation 0)
    /// looks north; one turned 90° counter-clockwise looks west (270).
    #[test]
    fn bearings_and_legs() {
        assert_eq!(bearing_ahead(0.0), 0.0);
        assert_eq!(bearing_ahead(90.0), 270.0);
        assert_eq!(bearing_ahead(-90.0), 90.0);
        assert_eq!(leg_of([2.0, 3.0]), "A");
        assert_eq!(leg_of([-2.0, 3.0]), "B");
        assert_eq!(leg_of([-2.0, -3.0]), "C");
        assert_eq!(leg_of([2.0, -3.0]), "D");
        assert_eq!(leg_of([0.0, 3.0]), "");
        assert_eq!(bearing_ahead(360.0), 0.0);
        assert_eq!(num(-0.0001, 2), "0.00");
        assert_eq!(num(-0.5, 2), "-0.50");
        assert_eq!(angle_text(-0.001), "0");
        assert_eq!(angle_text(-10.0), "10.00° L");
    }

    /// The example line: every structure listed in station order along its
    /// alignment, the stations rising, the ahead span of one the back span
    /// of the next and the distance between their centres; the ground as
    /// given (a flat 5 m below every structure here); the CSV a header and
    /// a row per row.
    #[test]
    fn the_example_lines_structures_in_station_order() {
        let line = crate::tests::example();
        let t = staking(&line, None, &|_, _| Some(-5.0));
        assert!(!t.rows.is_empty());
        let on: Vec<&StakingRow> = t.rows.iter().filter(|r| r.station.is_some()).collect();
        for w in on.windows(2).filter(|w| w[0].alignment == w[1].alignment) {
            assert!(w[1].station >= w[0].station, "{w:?}");
            if let (Some(a), Some(b)) = (w[0].ahead_span, w[1].back_span) {
                assert!((a - b).abs() < 1e-9);
                let d = (w[1].position[0] - w[0].position[0])
                    .hypot(w[1].position[1] - w[0].position[1]);
                assert!((a - d).abs() < 1e-6, "{a} {d}");
            }
        }
        for s in &line.structures {
            assert!(t.rows.iter().any(|r| r.structure == s.id), "{}", s.id);
        }
        let r = &t.rows[0];
        assert_eq!(r.above_ground(), Some(r.position[2] + 5.0));
        assert_eq!(t.csv().lines().count(), 1 + t.rows.len());
        assert!(t.notes.iter().any(|n| n.contains("CRS")));
        assert!(t.text().contains(&r.structure));
    }

    /// The typed example: type-02 has four pinned feet 2.5 m either way
    /// of its centre; each structure of that type gets the four, each in
    /// its own quarter (A to D once), 2.5·√2 m from its centre in plan,
    /// at its elevation plus its height adjustment (the feet on the
    /// type's foot level); the ground 1 m lower everywhere: 1 m above it.
    #[test]
    fn the_feet_of_a_lattice_type() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line");
        let line = Line::load(&root.join("synthetic-line-typed.ohl.json")).unwrap();
        let types = StructureLibrary::load_dir(&root.join("Structures")).unwrap();
        let towers: Vec<&Structure> = line
            .structures
            .iter()
            .filter(|s| s.structure_type.as_deref() == Some("type-02"))
            .collect();
        assert_eq!(towers.len(), 2);
        let t = staking(&line, Some(&types), &|x, y| {
            line.structures
                .iter()
                .find(|s| (s.position[0] - x).hypot(s.position[1] - y) < 10.0)
                .map(|s| s.position[2] + s.height_adjust - 1.0)
        });
        for s in towers {
            let feet: Vec<&StakingFoot> = t.feet.iter().filter(|f| f.structure == s.id).collect();
            assert_eq!(feet.len(), 4, "{feet:?}");
            let mut legs: Vec<&str> = feet.iter().map(|f| f.leg.as_str()).collect();
            legs.sort_unstable();
            assert_eq!(legs, ["A", "B", "C", "D"]);
            for f in feet {
                let d = (f.position[0] - s.position[0]).hypot(f.position[1] - s.position[1]);
                assert!((d - 2.5 * 2f64.sqrt()).abs() < 1e-6, "{d}");
                assert!((f.position[2] - (s.position[2] + s.height_adjust)).abs() < 1e-6);
                assert!((f.above_ground().unwrap() - 1.0).abs() < 1e-6);
            }
        }
        // The other types have no supported joints: said.
        assert!(
            t.notes.iter().any(|n| n.contains("no supported joints")),
            "{:?}",
            t.notes
        );
        assert_eq!(t.feet_csv().lines().count(), 1 + t.feet.len());
        // Without the library: no feet, said.
        let t = staking(&line, None, &|_, _| None);
        assert!(t.feet.is_empty());
        assert!(t.notes.iter().any(|n| n.contains("no structure library")));
        assert!(t.notes.iter().any(|n| n.contains("without ground")));
    }
}
