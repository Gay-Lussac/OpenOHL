// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The line's values, the evaluation of the recipes and the load table.

use std::collections::{BTreeMap, BTreeSet};

use ohl_core::units::Force;
use ohl_structures::StructureType;
use ohl_structures::structures::{set_from_v1, v1_point_label};
use serde::{Deserialize, Serialize};

use crate::parts::{LineForces, OwnLoads, Part};
use crate::recipe::{Direction, LineQuantity, LoadCaseRecipe, LoadRecipe, MODEL_CASE, Pick, Side};

/// A point of the placed structure as the line sees it: the sets and
/// phases hanging there.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LinePoint {
    /// The point's label (a point or wire point of the structure).
    pub label: String,
    /// The sets hanging there.
    #[serde(default)]
    pub sets: Vec<u32>,
    /// Their phase labels.
    #[serde(default)]
    pub phases: Vec<String>,
}

/// One value the line gives: at a point, or for a set (summed over the
/// sets at a point when the point has none of its own).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineValue {
    /// The line's load case.
    pub case: String,
    /// At this point…
    #[serde(default)]
    pub point: Option<String>,
    /// …or for this set.
    #[serde(default)]
    pub set: Option<u32>,
    /// What it is.
    pub quantity: LineQuantity,
    /// Its value, N (structure axes, vertical down).
    pub value: f64,
}

/// The wind on the structure's own body in one of the line's cases, as
/// the line's structure checks take it (`ohl_loads::LoadCase::
/// structure_wind`: the case's `[case.structure]` values, else its weather
/// case's pressure and its transverse factor).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseWind {
    /// The line's load case.
    pub case: String,
    /// Pressure, Pa, unfactored.
    pub pressure: f64,
    /// The factor on it.
    pub factor: f64,
    /// Where it blows: `-x` (from the right of the line) or `x`.
    pub towards: Direction,
    /// Where the values come from when the case gives its own for the
    /// structures (empty: the weather case's pressure, the transverse
    /// factor).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origin: String,
}

impl CaseWind {
    /// The pressure of `quantity` (unfactored, or times the factor).
    pub fn pressure_of(&self, quantity: LineQuantity) -> f64 {
        if quantity == LineQuantity::StructureWindFactored {
            self.pressure * self.factor
        } else {
            self.pressure
        }
    }
}

/// The values the line gives at one placed structure (plain input; empty
/// for a structure opened on its own).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LineValues {
    /// The points, with their sets and phases.
    #[serde(default)]
    pub points: Vec<LinePoint>,
    /// The values.
    #[serde(default)]
    pub values: Vec<LineValue>,
    /// The wind on the structure's own body per line case (line-values
    /// file version 4; none in older files).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub structure_wind: Vec<CaseWind>,
}

impl LineValues {
    /// The wind on the structure body in line case `case`, if the line
    /// gives it.
    pub fn structure_wind(&self, case: &str) -> Option<&CaseWind> {
        self.structure_wind.iter().find(|w| w.case == case)
    }

    /// The value of `quantity` in line case `case` at point `label`
    /// carrying `sets`: the point's own, else the sum over its sets (when
    /// every set has one). `tension_max` is also the larger of the back
    /// and ahead tensions.
    pub fn get(
        &self,
        case: &str,
        label: &str,
        sets: &[u32],
        quantity: LineQuantity,
    ) -> Option<f64> {
        let own = |q: LineQuantity| {
            self.values
                .iter()
                .find(|v| v.case == case && v.quantity == q && v.point.as_deref() == Some(label))
                .map(|v| v.value)
                .or_else(|| {
                    if sets.is_empty() {
                        return None;
                    }
                    sets.iter()
                        .map(|s| {
                            self.values
                                .iter()
                                .find(|v| {
                                    v.case == case
                                        && v.quantity == q
                                        && v.point.is_none()
                                        && v.set == Some(*s)
                                })
                                .map(|v| v.value)
                        })
                        .sum::<Option<f64>>()
                })
        };
        own(quantity).or_else(|| {
            (quantity == LineQuantity::TensionMax).then(|| {
                [LineQuantity::TensionBack, LineQuantity::TensionAhead]
                    .into_iter()
                    .filter_map(own)
                    .map(f64::abs)
                    .reduce(f64::max)
            })?
        })
    }
}

/// A point of the structure that loads act at.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    /// Label.
    pub label: String,
    /// The sets hanging there (through its strings).
    pub sets: Vec<u32>,
    /// The side its strain strings point to (none: suspension, or both).
    pub side: Option<Side>,
    /// The phases hanging there (from the line).
    pub phases: Vec<String>,
}

/// The points of `structure` loads act at: its points (version 2), wire
/// points no string ends at (a wire fixed to the structure) and version 1
/// attachment points; with the sets and phases `line` gives there.
pub fn targets(structure: &StructureType, line: &LineValues) -> Vec<Target> {
    let side_of = |a: f64, d: f64| {
        let c = d.to_radians().cos();
        (a > 45.0 && c.abs() > 1e-9).then_some(if c > 0.0 { Side::Ahead } else { Side::Back })
    };
    let mut out = Vec::new();
    for p in &structure.points {
        // The sets below it, through its strings.
        let mut sets = BTreeSet::new();
        let mut tops = vec![p.label.as_str()];
        let mut seen = BTreeSet::new();
        while let Some(top) = tops.pop() {
            if !seen.insert(top) {
                continue;
            }
            for s in structure.strings.iter().filter(|s| s.top == top) {
                if let Some(w) = structure.wires.iter().find(|w| w.label == s.wire) {
                    sets.extend(w.set);
                    tops.push(&w.label);
                }
            }
        }
        let sides: BTreeSet<_> = structure
            .strings
            .iter()
            .filter(|s| s.top == p.label && s.strain)
            .filter_map(|s| {
                let (a, d) = s.angles();
                side_of(a, d)
            })
            .map(Side::as_str)
            .collect();
        let side = match sides.iter().next() {
            Some(&"ahead") if sides.len() == 1 => Some(Side::Ahead),
            Some(&"back") if sides.len() == 1 => Some(Side::Back),
            _ => None,
        };
        out.push(Target {
            label: p.label.clone(),
            sets: sets.into_iter().collect(),
            side,
            phases: Vec::new(),
        });
    }
    for w in &structure.wires {
        if !structure.strings.iter().any(|s| s.wire == w.label) {
            out.push(Target {
                label: w.label.clone(),
                sets: w.set.into_iter().collect(),
                side: None,
                phases: Vec::new(),
            });
        }
    }
    for a in &structure.attachments {
        let side = a.side.map(|s| match s {
            ohl_structures::structures::Side::Ahead => Side::Ahead,
            ohl_structures::structures::Side::Back => Side::Back,
        });
        out.push(Target {
            label: v1_point_label(a),
            sets: vec![set_from_v1(a.set, a.phase)],
            side,
            phases: Vec::new(),
        });
    }
    for t in &mut out {
        for p in line.points.iter().filter(|p| p.label == t.label) {
            for s in &p.sets {
                if !t.sets.contains(s) {
                    t.sets.push(*s);
                }
            }
            for ph in &p.phases {
                if !t.phases.contains(ph) {
                    t.phases.push(ph.clone());
                }
            }
        }
        t.sets.sort_unstable();
    }
    out
}

/// What a row of the load table is at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum At {
    /// A point or wire point of the structure.
    Point,
    /// A member (a surface load, its total).
    Member,
    /// A joint (`[[load]]` of the model case).
    Joint,
}

impl At {
    /// The name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Point => "point",
            Self::Member => "member",
            Self::Joint => "joint",
        }
    }

    /// From its name in files.
    pub fn parse(s: &str) -> Option<Self> {
        [Self::Point, Self::Member, Self::Joint]
            .into_iter()
            .find(|a| a.as_str() == s)
    }
}

/// Where a row's value comes from: the part of the case that made it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// A value or pressure given in an older load of the recipe.
    Recipe,
    /// A value of the line: an older load taking one, or the case's line
    /// forces.
    Line,
    /// The wind on the body (`[[load_case.structure.wind]]`).
    Wind,
    /// Ice or another weight on members (`[[load_case.structure.ice]]`).
    Ice,
    /// A point load (`[[load_case.structure.point]]`).
    Point,
    /// Typed (over an evaluated value, or a row of its own; `[[load]]`).
    Typed,
}

impl Source {
    /// Every source.
    pub const ALL: [Self; 6] = [
        Self::Recipe,
        Self::Line,
        Self::Wind,
        Self::Ice,
        Self::Point,
        Self::Typed,
    ];

    /// The name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recipe => "recipe",
            Self::Line => "line",
            Self::Wind => "wind",
            Self::Ice => "ice",
            Self::Point => "point",
            Self::Typed => "typed",
        }
    }

    /// From its name in files.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.as_str() == s)
    }
}

/// One row of the load table: one load of a case at one point (or member,
/// or joint).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadRow {
    /// The load case.
    pub case: String,
    /// The recipe's load, from 1 (none: a typed row of its own): an older
    /// load 1 to 999, a part's item from its block (line forces 1001…,
    /// wind 2001…, ice 3001…, point loads 4001…: [`Part`]).
    pub load: Option<usize>,
    /// What it is at.
    pub at: At,
    /// The point's label, member's or joint's id (empty: the load selects
    /// nothing).
    pub label: String,
    /// Transverse (right), longitudinal (ahead), vertical (down), N.
    pub force: [f64; 3],
    /// Where the value comes from.
    pub source: Source,
    /// Typed over an evaluated value: the evaluated one.
    pub evaluated: Option<[f64; 3]>,
    /// Why there is no value ("no line value", "no area", …); the force
    /// is then 0.
    pub missing: Option<String>,
    /// A note.
    pub note: String,
    /// A surface load: the member's area it acted on, m².
    #[serde(default)]
    pub area: Option<f64>,
}

impl LoadRow {
    fn new(case: &str, load: Option<usize>, at: At, label: &str) -> Self {
        Self {
            case: case.to_owned(),
            load,
            at,
            label: label.to_owned(),
            force: [0.0; 3],
            source: Source::Recipe,
            evaluated: None,
            missing: None,
            note: String::new(),
            area: None,
        }
    }

    fn missing(mut self, why: String) -> Self {
        self.missing = Some(why);
        self
    }
}

/// A value typed over the table: replaces the row of the same case, load
/// and place, or is a row of its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Override {
    /// The load case.
    pub case: String,
    /// The recipe's load, from 1 (none: a row of its own).
    pub load: Option<usize>,
    /// What it is at.
    pub at: At,
    /// Label or id.
    pub label: String,
    /// Transverse, longitudinal, vertical (down), N.
    pub force: [f64; 3],
}

impl Override {
    /// The override of a row: its place and force.
    pub fn of(r: &LoadRow) -> Self {
        Self {
            case: r.case.clone(),
            load: r.load,
            at: r.at,
            label: r.label.clone(),
            force: r.force,
        }
    }

    /// Whether it is typed over `r` (same case, load and place).
    pub fn is_over(&self, r: &LoadRow) -> bool {
        self.case == r.case && self.load == r.load && self.at == r.at && self.label == r.label
    }

    /// The overrides as a table of typed rows (to keep them as CSV).
    pub fn table(overrides: &[Override]) -> LoadTable {
        let mut t = LoadTable::default();
        t.apply(overrides);
        t
    }
}

/// The loads of a structure: per case, per point (or member, or joint).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LoadTable {
    /// Rows, by case and load.
    pub rows: Vec<LoadRow>,
}

impl LoadTable {
    /// The rows of one case.
    pub fn case<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a LoadRow> + 'a {
        self.rows.iter().filter(move |r| r.case == name)
    }

    /// The cases, in table order.
    pub fn cases(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for r in &self.rows {
            if !out.contains(&r.case.as_str()) {
                out.push(&r.case);
            }
        }
        out
    }

    /// Rows without a value.
    pub fn missing(&self) -> impl Iterator<Item = &LoadRow> {
        self.rows.iter().filter(|r| r.missing.is_some())
    }

    /// The sum per case and place: (case, at, label) → force.
    pub fn totals(&self) -> BTreeMap<(String, At, String), [f64; 3]> {
        let mut out: BTreeMap<(String, At, String), [f64; 3]> = BTreeMap::new();
        for r in self.rows.iter().filter(|r| r.missing.is_none()) {
            let e = out
                .entry((r.case.clone(), r.at, r.label.clone()))
                .or_default();
            for (sum, f) in e.iter_mut().zip(r.force) {
                *sum += f;
            }
        }
        out
    }

    /// Types values over the table: each replaces its row (the evaluated
    /// value kept, the row marked typed), or is added as a row of its own.
    pub fn apply(&mut self, overrides: &[Override]) {
        for o in overrides {
            let found = self.rows.iter_mut().find(|r| {
                r.case == o.case && r.load == o.load && r.at == o.at && r.label == o.label
            });
            match found {
                Some(r) => {
                    if r.source != Source::Typed && r.missing.is_none() {
                        r.evaluated = Some(r.force);
                    }
                    r.force = o.force;
                    r.source = Source::Typed;
                    r.missing = None;
                }
                None => {
                    let mut r = LoadRow::new(&o.case, o.load, o.at, &o.label);
                    r.force = o.force;
                    r.source = Source::Typed;
                    self.rows.push(r);
                }
            }
        }
    }

    /// What this table (read back, e.g. from CSV) types over `evaluated`:
    /// its rows with a value that `evaluated` has not (another value, a
    /// row it lacks, a value where it has none, or a row typed there and
    /// not here). `evaluated.apply` of them gives this table again.
    pub fn overrides_over(&self, evaluated: &LoadTable) -> Vec<Override> {
        let same = |a: [f64; 3], b: [f64; 3]| {
            a.iter()
                .zip(b)
                .all(|(x, y)| (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0))
        };
        self.rows
            .iter()
            .filter(|r| r.missing.is_none())
            .filter(|r| {
                let e = evaluated.rows.iter().find(|e| {
                    e.case == r.case && e.load == r.load && e.at == r.at && e.label == r.label
                });
                !e.is_some_and(|e| {
                    e.missing.is_none()
                        && same(e.force, r.force)
                        && (r.source != Source::Typed || e.source == Source::Typed)
                })
            })
            .map(Override::of)
            .collect()
    }

    /// The typed rows, as overrides (to keep with the placed structure).
    pub fn overrides(&self) -> Vec<Override> {
        self.rows
            .iter()
            .filter(|r| r.source == Source::Typed && r.case != MODEL_CASE)
            .map(Override::of)
            .collect()
    }
}

/// The members of the structure as the loads on them need them (from the
/// section library and the structure's finite-element model:
/// `ohl-structure-link`): the area a pressure meets, the member wind of
/// the wind on the body, a member's outline and length for its ice. What
/// is not known gives the row's "missing" (why).
pub trait MemberAreas {
    /// The area of member `id`, if known.
    fn area(&self, id: &str) -> Option<f64>;

    /// The area of member `id` a pressure acting in `direction` meets, or
    /// why there is none (the row's "missing", starting "no area"). By
    /// default [`MemberAreas::area`], whatever the direction.
    fn area_along(&self, id: &str, direction: Direction) -> Result<f64, String> {
        let _ = direction;
        self.area(id).ok_or_else(|| "no area".to_owned())
    }

    /// The members' ids (none: not known here, the structure's own
    /// members are taken).
    fn member_ids(&self) -> Option<Vec<String>> {
        None
    }

    /// The wind of `pressure` (Pa, factored) blowing towards `towards`
    /// (a unit vector, table axes: vertical down) on every member: the
    /// member wind of the line's own structure checks (faces, solidity,
    /// shielding with the section library's wind factors), each member
    /// `widened(id)` m wider (iced). Per member its force (table axes) or
    /// why it has none, and what was applied in words.
    fn member_wind(
        &self,
        pressure: f64,
        towards: [f64; 3],
        widened: &dyn Fn(&str) -> f64,
    ) -> Result<BodyWind, String> {
        let _ = (pressure, towards, widened);
        Err(
            "no member wind: the structure's model and cross-section library are not known here"
                .into(),
        )
    }

    /// The perimeter of member `id`'s outline (the profile's outer edge,
    /// an angle's inside faces too), m, or why it is not known.
    fn perimeter(&self, id: &str) -> Result<f64, String> {
        let _ = id;
        Err("no outline: the cross-section library is not known here".into())
    }

    /// The length of member `id`, m, or why it is not known.
    fn length(&self, id: &str) -> Result<f64, String> {
        let _ = id;
        Err("no length: the structure's model is not known here".into())
    }
}

/// The wind on the body's members ([`MemberAreas::member_wind`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BodyWind {
    /// Per member (id): its force (transverse, longitudinal, vertical
    /// down), N, or why it has none.
    pub members: Vec<(String, Result<[f64; 3], String>)>,
    /// What was applied (drag factors, shielding), in words.
    pub notes: Vec<String>,
}

/// No areas known: surface loads give rows marked "no area".
pub struct NoAreas;

impl MemberAreas for NoAreas {
    fn area(&self, _: &str) -> Option<f64> {
        None
    }
}

impl<F: Fn(&str) -> Option<f64>> MemberAreas for F {
    fn area(&self, id: &str) -> Option<f64> {
        self(id)
    }
}

/// The load table of `structure` from its recipes and the line's values
/// (no member areas: surface loads are marked "no area"). The structure's
/// `[[load]]` tables come last, as case `model`.
pub fn evaluate(
    recipes: &[LoadCaseRecipe],
    structure: &StructureType,
    line: &LineValues,
) -> LoadTable {
    evaluate_with_areas(recipes, structure, line, &NoAreas)
}

/// [`evaluate`] with the members' areas.
pub fn evaluate_with_areas(
    recipes: &[LoadCaseRecipe],
    structure: &StructureType,
    line: &LineValues,
    areas: &dyn MemberAreas,
) -> LoadTable {
    let targets = targets(structure, line);
    let mut rows = Vec::new();
    for case in recipes {
        for (k, load) in case.loads.iter().enumerate() {
            let number = Some(k + 1);
            if let Some(members) = &load.members {
                rows.extend(surface_rows(
                    case, number, load, members, structure, line, areas,
                ));
            } else {
                rows.extend(point_rows(case, number, load, &targets, line));
            }
        }
        rows.extend(part_rows(case, structure, &targets, line, areas));
    }
    if !structure.loads.is_empty() && !recipes.iter().any(|c| c.name == MODEL_CASE) {
        match model_rows(structure) {
            Ok(r) => rows.extend(r),
            Err(e) => rows.push(LoadRow::new(MODEL_CASE, None, At::Joint, "").missing(e)),
        }
    }
    LoadTable { rows }
}

/// The pressure a load of `case` takes from the line (`structure_wind`,
/// `structure_wind_factored`): the line case's ([`CaseWind`]) and its
/// direction (the load's own, else the wind's). None: the load takes no
/// pressure from the line; Err: the line gives none (why, as a row's
/// "missing").
pub fn line_pressure(
    case: &LoadCaseRecipe,
    load: &LoadRecipe,
    line: &LineValues,
) -> Option<Result<(f64, Direction, String), String>> {
    let q = load.what.filter(|q| q.is_pressure())?;
    let Some(lc) = load.line_case.as_ref().or(case.line_case.as_ref()) else {
        return Some(Err(format!(
            "no line value: {} (no line case named)",
            q.as_str()
        )));
    };
    Some(match line.structure_wind(lc) {
        Some(w) => Ok((
            w.pressure_of(q),
            load.direction.unwrap_or(w.towards),
            w.origin.clone(),
        )),
        None => Err(format!("no line value: {} in \"{lc}\"", q.as_str())),
    })
}

/// `recipes` with every pressure the line gives (`structure_wind`,
/// `structure_wind_factored`, [`line_pressure`]) written as the load's
/// own `pressure` and `direction`, and every body wind's `pressure_of` as
/// its `pressure` (the line result's weather case's, unfactored), for
/// what reads pressures from the recipes (the run on an extension, the
/// pole's wind); a load the line gives no value for is left as it is (no
/// pressure).
pub fn resolve_line_pressures(
    recipes: &[LoadCaseRecipe],
    line: &LineValues,
) -> Vec<LoadCaseRecipe> {
    recipes
        .iter()
        .map(|c| {
            let mut c = c.clone();
            let loads = c
                .loads
                .iter()
                .map(|l| match line_pressure(&c, l, line) {
                    Some(Ok((p, d, _))) => LoadRecipe {
                        what: None,
                        pressure: Some(p),
                        direction: Some(d),
                        ..l.clone()
                    },
                    _ => l.clone(),
                })
                .collect();
            c.loads = loads;
            if let Some(s) = &mut c.structure {
                for w in &mut s.wind {
                    if let Some(r) = &w.pressure_of
                        && let Some(cw) = line.structure_wind(r)
                    {
                        w.pressure = Some(cw.pressure);
                        w.pressure_of = None;
                    }
                }
            }
            c
        })
        .collect()
}

/// The rows of case `case`'s parts, after its older loads: its line
/// forces, the wind on its body, its ice, its point loads, each numbered
/// in its part's block ([`Part`]). The self weight has no rows: the solve
/// puts it on the members (each member's weight × its dead-load factor ×
/// [`LoadCaseRecipe::self_weight_factor`]), as before.
fn part_rows(
    case: &LoadCaseRecipe,
    structure: &StructureType,
    targets: &[Target],
    line: &LineValues,
    areas: &dyn MemberAreas,
) -> Vec<LoadRow> {
    let mut rows = Vec::new();
    for (k, l) in case.line.iter().enumerate() {
        rows.extend(line_force_rows(
            case,
            Part::Line.number(k),
            l,
            targets,
            line,
        ));
    }
    let Some(own) = &case.structure else {
        return rows;
    };
    let ids: Vec<String> = areas
        .member_ids()
        .unwrap_or_else(|| structure.members.iter().map(|m| m.id.clone()).collect());
    for (k, w) in own.wind.iter().enumerate() {
        let number = Some(Part::Wind.number(k));
        let text = crate::parts::wind_text(w);
        let missing = |why: String| {
            let mut r = LoadRow::new(&case.name, number, At::Member, "").missing(why);
            r.source = Source::Wind;
            r.note.clone_from(&text);
            r
        };
        let pressure = match (w.pressure, &w.pressure_of) {
            (Some(p), _) => p,
            (None, Some(r)) => match line.structure_wind(r) {
                Some(cw) => cw.pressure,
                None => {
                    rows.push(missing(format!(
                        "no line value: the wind pressure of \"{r}\""
                    )));
                    continue;
                }
            },
            (None, None) => continue,
        };
        let widened = |id: &str| own.widening(id);
        match areas.member_wind(pressure * w.factor, w.from.towards(), &widened) {
            Err(why) => rows.push(missing(why)),
            Ok(body) => {
                for (n, (id, f)) in body.members.into_iter().enumerate() {
                    let mut note = text.clone();
                    let wider = own.widening(&id);
                    if wider > 0.0 {
                        note.push_str(&format!(
                            "; iced: {:.0} mm wider for the wind",
                            wider * 1000.0
                        ));
                    }
                    if n == 0 && !body.notes.is_empty() {
                        note.push_str("; ");
                        note.push_str(&body.notes.join("; "));
                    }
                    let row = LoadRow {
                        source: Source::Wind,
                        note,
                        ..LoadRow::new(&case.name, number, At::Member, &id)
                    };
                    rows.push(match f {
                        Ok(f) => LoadRow { force: f, ..row },
                        Err(why) => row.missing(why),
                    });
                }
            }
        }
    }
    for (k, ice) in own.ice.iter().enumerate() {
        let number = Some(Part::Ice.number(k));
        let text = crate::parts::ice_text(ice);
        let chosen: Vec<&str> = match &ice.members {
            Pick::All => ids.iter().map(String::as_str).collect(),
            Pick::These(v) => v.iter().map(String::as_str).collect(),
        };
        if chosen.is_empty() {
            let mut r = LoadRow::new(&case.name, number, At::Member, "")
                .missing("selects no member".into());
            r.source = Source::Ice;
            rows.push(r);
        }
        for id in chosen {
            let row = LoadRow {
                source: Source::Ice,
                note: text.clone(),
                ..LoadRow::new(&case.name, number, At::Member, id)
            };
            if !ids.iter().any(|m| m == id) {
                rows.push(row.missing(format!("no member \"{id}\"")));
                continue;
            }
            let perimeter = if ice.thickness.is_some() {
                match areas.perimeter(id) {
                    Ok(p) => Some(p),
                    Err(why) => {
                        rows.push(row.missing(why));
                        continue;
                    }
                }
            } else {
                None
            };
            let weight = match (ice.weight_per_metre(perimeter), areas.length(id)) {
                (Ok(w), Ok(len)) => w * len * ice.factor,
                (Err(why), _) | (_, Err(why)) => {
                    rows.push(row.missing(why));
                    continue;
                }
            };
            rows.push(LoadRow {
                force: [0.0, 0.0, weight],
                ..row
            });
        }
    }
    for (k, p) in own.point.iter().enumerate() {
        let number = Some(Part::Point.number(k));
        for mut r in point_rows(case, number, &p.as_load(), targets, line) {
            r.source = Source::Point;
            if r.note.is_empty() {
                r.note = crate::parts::point_text(p);
            }
            rows.push(r);
        }
    }
    rows
}

/// The rows of one line forces table (`number`: its row number): per
/// point carrying a wire (a set) or given a value by the line in its
/// result (only those of its sets when it names some), the line's
/// transverse, longitudinal and vertical forces, each × its factor; a
/// point turned off (`off`) keeps its row with no force, noted "off" (a
/// value typed over it stays on the same row whether it is off or on).
fn line_force_rows(
    case: &LoadCaseRecipe,
    number: usize,
    l: &LineForces,
    targets: &[Target],
    line: &LineValues,
) -> Vec<LoadRow> {
    let number = Some(number);
    let text = crate::parts::line_forces_text(l);
    let given_at = |label: &str| {
        line.values
            .iter()
            .any(|v| v.case == l.result && v.point.as_deref() == Some(label))
    };
    let chosen: Vec<&Target> = targets
        .iter()
        .filter(|t| !t.sets.is_empty() || given_at(&t.label))
        .filter(|t| l.sets.is_empty() || t.sets.iter().any(|s| l.sets.contains(s)))
        .collect();
    let mut rows = Vec::new();
    for o in &l.off {
        if !targets.iter().any(|t| &t.label == o) {
            let mut r = LoadRow::new(&case.name, number, At::Point, o)
                .missing(format!("off: no point \"{o}\" in the structure"));
            r.source = Source::Line;
            rows.push(r);
        }
    }
    if chosen.is_empty() {
        let mut r =
            LoadRow::new(&case.name, number, At::Point, "").missing("selects no point".into());
        r.source = Source::Line;
        rows.push(r);
    }
    let quantities = [
        LineQuantity::Transverse,
        LineQuantity::Longitudinal,
        LineQuantity::Vertical,
    ];
    for t in chosen {
        let mut row = LoadRow {
            source: Source::Line,
            note: text.clone(),
            ..LoadRow::new(&case.name, number, At::Point, &t.label)
        };
        if l.off.contains(&t.label) {
            row.note = format!("off: {}'s line forces left out", t.label);
            rows.push(row);
            continue;
        }
        let values = quantities.map(|q| line.get(&l.result, &t.label, &t.sets, q));
        if values.iter().any(Option::is_none) {
            let lacking: Vec<&str> = quantities
                .iter()
                .zip(values)
                .filter(|(_, v)| v.is_none())
                .map(|(q, _)| q.as_str())
                .collect();
            rows.push(row.missing(format!(
                "no line value: {} in \"{}\"",
                lacking.join(", "),
                l.result
            )));
            continue;
        }
        let f = l.factors();
        row.force = [0, 1, 2].map(|k| values[k].unwrap_or_default() * f[k]);
        rows.push(row);
    }
    rows
}

/// Whether case `case`'s loads on the structure itself include any (a
/// body wind, ice), for what builds the members' model only when needed.
pub fn needs_members(case: &LoadCaseRecipe) -> bool {
    case.loads.iter().any(|l| l.members.is_some())
        || case
            .structure
            .as_ref()
            .is_some_and(|s: &OwnLoads| !s.wind.is_empty() || !s.ice.is_empty())
}

fn surface_rows(
    case: &LoadCaseRecipe,
    number: Option<usize>,
    load: &LoadRecipe,
    members: &Pick,
    structure: &StructureType,
    line: &LineValues,
    areas: &dyn MemberAreas,
) -> Vec<LoadRow> {
    let ids: Vec<&str> = match members {
        Pick::All => structure.members.iter().map(|m| m.id.as_str()).collect(),
        Pick::These(v) => v.iter().map(String::as_str).collect(),
    };
    if ids.is_empty() {
        return vec![
            LoadRow::new(&case.name, number, At::Member, "").missing("selects no member".into()),
        ];
    }
    let (p, direction, source, note) = match line_pressure(case, load, line) {
        None => (
            load.pressure.unwrap_or_default(),
            load.direction.unwrap_or(Direction::Right),
            Source::Recipe,
            String::new(),
        ),
        Some(Ok((p, d, origin))) => (p, d, Source::Line, origin),
        Some(Err(why)) => {
            return ids
                .into_iter()
                .map(|id| LoadRow::new(&case.name, number, At::Member, id).missing(why.clone()))
                .collect();
        }
    };
    let dir = direction.table_vector();
    ids.into_iter()
        .map(|id| {
            let row = LoadRow {
                source,
                note: note.clone(),
                ..LoadRow::new(&case.name, number, At::Member, id)
            };
            if !structure.members.iter().any(|m| m.id == id) {
                return row.missing(format!("no member \"{id}\""));
            }
            match areas.area_along(id, direction) {
                Err(why) => row.missing(why),
                Ok(a) => LoadRow {
                    force: dir.map(|d| d * p * a * load.scale()),
                    area: Some(a),
                    ..row
                },
            }
        })
        .collect()
}

fn point_rows(
    case: &LoadCaseRecipe,
    number: Option<usize>,
    load: &LoadRecipe,
    targets: &[Target],
    line: &LineValues,
) -> Vec<LoadRow> {
    let mut rows = Vec::new();
    let mut chosen: Vec<&Target> = match &load.points {
        Some(Pick::These(labels)) => {
            let mut v = Vec::new();
            for l in labels {
                match targets.iter().find(|t| &t.label == l) {
                    Some(t) => v.push(t),
                    None => rows.push(
                        LoadRow::new(&case.name, number, At::Point, l)
                            .missing(format!("no point \"{l}\" in the structure")),
                    ),
                }
            }
            v
        }
        _ => targets.iter().collect(),
    };
    chosen.retain(|t| {
        (load.sets.is_empty() || t.sets.iter().any(|s| load.sets.contains(s)))
            && (load.phases.is_empty() || t.phases.iter().any(|p| load.phases.contains(p)))
            && load
                .side
                .is_none_or(|side| t.side.is_none_or(|s| s == side))
    });
    if chosen.is_empty() && rows.is_empty() {
        let why = if load.phases.is_empty() {
            "selects no point".into()
        } else {
            "selects no point (phases come from the line)".into()
        };
        rows.push(LoadRow::new(&case.name, number, At::Point, "").missing(why));
    }
    for t in chosen {
        let mut row = LoadRow::new(&case.name, number, At::Point, &t.label);
        let value = match (load.what, load.value) {
            (_, Some(v)) => v,
            (Some(q), None) => {
                let Some(lc) = load.line_case.as_ref().or(case.line_case.as_ref()) else {
                    rows.push(row.missing(format!(
                        "no line value: {} (no line case named)",
                        q.as_str()
                    )));
                    continue;
                };
                match line.get(lc, &t.label, &t.sets, q) {
                    Some(v) => {
                        row.source = Source::Line;
                        v
                    }
                    None => {
                        rows.push(
                            row.missing(format!("no line value: {} in \"{lc}\"", q.as_str())),
                        );
                        continue;
                    }
                }
            }
            (None, None) => continue,
        };
        let s = load.scale();
        row.force = match (load.what, load.direction) {
            // A value of the case: signed, along its direction.
            (None, Some(d)) => d.table_vector().map(|x| x * value * s),
            (Some(q), d) if q.is_tension() => {
                let d = d.or(load.side.map(Side::direction)).unwrap_or(
                    if q == LineQuantity::TensionBack {
                        Direction::Back
                    } else {
                        Direction::Ahead
                    },
                );
                d.table_vector().map(|x| x * value.abs() * s)
            }
            // A line value along a direction: its size.
            (Some(_), Some(d)) => d.table_vector().map(|x| x * value.abs() * s),
            (Some(q), None) => {
                let mut f = [0.0; 3];
                f[q.axis()] = value * s;
                f
            }
            (None, None) => [0.0; 3],
        };
        rows.push(row);
    }
    rows
}

/// The structure's `[[load]]` tables (nodal forces, structure axes, z up)
/// as the rows of case `model`, at their joints (moments are not in the
/// table; the finite-element check reads them from `[[load]]`).
pub fn model_rows(structure: &StructureType) -> Result<Vec<LoadRow>, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct NodeLoad {
        node: String,
        #[serde(default)]
        force: [Force; 3],
        #[serde(default)]
        moment: Option<toml::Value>,
    }
    structure
        .loads
        .iter()
        .enumerate()
        .map(|(k, t)| {
            let l: NodeLoad = toml::Value::Table(t.clone())
                .try_into()
                .map_err(|e| format!("load {}: {}", k + 1, e.to_string().trim()))?;
            let f = l.force.map(|x| x.si());
            let mut row = LoadRow::new(MODEL_CASE, Some(k + 1), At::Joint, &l.node);
            row.force = [f[0], f[1], -f[2]];
            row.source = Source::Typed;
            if l.moment.is_some() {
                row.note = "moment in [[load]], not in the table".into();
            }
            Ok(row)
        })
        .collect()
}
