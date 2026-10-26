// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A whole run of the structure application's results: the cases of a
//! load table solved on the structure standing on an extension (or as
//! written), with the finite-element model and, for a structure with a
//! pole, the pole check — the same model and the same pole check the line
//! uses for a placed structure (`ohl_loads::analysis`).
//!
//! - **The model on the extension**: a structure with its own members is
//!   solved on [`Model::from_structure_type_extended`] (the extension's
//!   piece added, the joints it reaches unsupported, legs lowered); a
//!   `model = "….ohlfe"` has no extensions and is solved as written (a
//!   note says so), as in the line.
//! - **Surface loads on the extension's piece**: a pressure on `members =
//!   "all"` of a case acts on the extension's own members too (the load
//!   table lists the body as written): pressure × the member's projected
//!   area × the load's factor, half to each joint — the rule of the
//!   table's surface rows. The piece's members are what the wind blows on
//!   below the feet; the body above stands `dz` higher, which changes no
//!   force (the pressure does not depend on the height).
//! - **The case's loads on the structure itself on the piece**
//!   (`[load_case.structure]`, 2026-10-25): its wind on the body acts on
//!   the piece's members as on the body (the member wind with the section
//!   library's wind factors on the model on the extension, iced members
//!   wider), its ice and weight per metre on the piece's members it picks
//!   (every member, or those named); a note per load. The point loads and
//!   line forces are rows at points, as the body's.
//! - **The pole check** ([`ohl_structures_fe::check_pole`], as the line's
//!   `pole` check): the case's point rows at the structure's points as
//!   written above the feet (the pole and its loads rise together on an
//!   extension: the moments about the pole's own ground line are the
//!   same), the horizontal pressures of the case's surface loads on
//!   `members = "all"` and of its wind on the body (pressure × factor,
//!   from its direction) as the wind pressure on the pole (times the
//!   pole's drag), the case's strength factor, P-Delta when the pole gives
//!   its modulus. The ice on members is not on the pole (said in the
//!   case's notes: the pole check takes no ice of its own).

use std::path::Path;

use ohl_core::budget::Budget;
use ohl_sections::SectionLibrary;
use ohl_structure_loads::{At, LoadCaseRecipe, LoadTable, Pick};
use ohl_structures::StructureType;
use ohl_structures_fe::{Model, NodalLoad, Pole, PoleLoad};

use crate::{CaseResult, CaseToRun, analyse_one};

/// What a run solves.
pub struct RunInput<'a> {
    /// The structure.
    pub structure: &'a StructureType,
    /// Its file's text.
    pub text: &'a str,
    /// Its folder (a `model = "….ohlfe"` or a pole file it names).
    pub dir: Option<&'a Path>,
    /// The section library (members' sections looked up here first).
    pub sections: &'a SectionLibrary,
    /// The load table.
    pub table: &'a LoadTable,
    /// The recipes of the cases (their surface loads; may be empty).
    pub recipes: &'a [LoadCaseRecipe],
    /// The cases to solve.
    pub cases: &'a [CaseToRun],
    /// The extension the structure stands on (none: as written).
    pub extension: Option<&'a str>,
}

/// A section the model uses.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionUse {
    /// Id.
    pub id: String,
    /// Its material in the section library (none: the structure's own
    /// `[[section]]` table).
    pub material: Option<String>,
    /// Area, m².
    pub area: f64,
    /// Young's modulus, Pa.
    pub elasticity: f64,
    /// Yield strength, Pa.
    pub yield_strength: Option<f64>,
    /// Weight per metre, N/m.
    pub weight: f64,
    /// Members with it.
    pub members: usize,
    /// Their length, m.
    pub length: f64,
}

/// The pole the check reads.
#[derive(Debug, Clone, PartialEq)]
pub struct PoleInfo {
    /// Id.
    pub id: String,
    /// Material (wood, steel, …).
    pub material: String,
    /// Height above the ground line, m.
    pub height: f64,
    /// Diameter at the ground line, m.
    pub ground_diameter: f64,
    /// At the top, m.
    pub top_diameter: f64,
    /// Drag factor on the wind pressure.
    pub drag: f64,
    /// The moment capacity at the ground line, N·m.
    pub ground_capacity: f64,
}

/// One case's pole check.
#[derive(Debug, Clone, PartialEq)]
pub struct PoleResult {
    /// Share of the capacity used (1 = 100 %; infinite: the pole buckles).
    pub usage: f64,
    /// Height of the governing section above the ground line, m.
    pub height: f64,
    /// Bending moment there (amplified), N·m.
    pub moment: f64,
    /// Capacity there (before the strength factor), N·m.
    pub capacity: f64,
    /// Ground-line moment, first order, N·m.
    pub ground_moment: f64,
    /// Of which the wind on the pole, N·m.
    pub wind_moment: f64,
    /// P-Delta amplification (1: not computed).
    pub amplification: f64,
    /// The wind pressure on the pole, Pa, structure axes (x, y).
    pub wind: [f64; 2],
    /// What governs, in words.
    pub governs: String,
    /// What was left out.
    pub notes: Vec<String>,
}

/// One case of a run: the finite-element results (none: the structure has
/// no members) and the pole check (none: no pole).
#[derive(Debug, Clone, PartialEq)]
pub struct StructureCase {
    /// The case.
    pub name: String,
    /// Finite-element results, or why the case did not solve.
    pub fe: Option<Result<CaseResult, String>>,
    /// The pole check, or why it could not run.
    pub pole: Option<Result<PoleResult, String>>,
}

impl StructureCase {
    /// The largest usage of the case over its checks (an error: infinite).
    pub fn usage(&self) -> Option<f64> {
        let fe = match &self.fe {
            Some(Ok(r)) => r.governing().and_then(|g| g.usage),
            Some(Err(_)) => Some(f64::INFINITY),
            None => None,
        };
        let pole = match &self.pole {
            Some(Ok(p)) => Some(p.usage),
            Some(Err(_)) => Some(f64::INFINITY),
            None => None,
        };
        match (fe, pole) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    }
}

/// A check over every case: its largest usage and the case governing.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckSummary {
    /// The check: `members` (finite elements) or `pole`.
    pub check: &'static str,
    /// The largest usage (none: no case gives one).
    pub usage: Option<f64>,
    /// The case governing.
    pub case: String,
    /// What governs there, in words.
    pub governs: String,
    /// Cases where the check did not run, with why.
    pub failed: Vec<(String, String)>,
}

/// A run's answer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StructureRun {
    /// The extension stood on (none: as written).
    pub extension: Option<String>,
    /// The extension's `dz`, m (0: none).
    pub dz: f64,
    /// What applies to every case (the model as written on an extension,
    /// …).
    pub notes: Vec<String>,
    /// The sections the model uses, in the model's order.
    pub sections: Vec<SectionUse>,
    /// The pole (none: no pole).
    pub pole: Option<Result<PoleInfo, String>>,
    /// The cases.
    pub cases: Vec<StructureCase>,
}

impl StructureRun {
    /// Per check (members, pole) the largest usage over the cases and the
    /// case governing.
    pub fn checks(&self) -> Vec<CheckSummary> {
        let mut out = Vec::new();
        let any_fe = self.cases.iter().any(|c| c.fe.is_some());
        let any_pole = self.cases.iter().any(|c| c.pole.is_some());
        if any_fe {
            let mut s = CheckSummary {
                check: "members",
                usage: None,
                case: String::new(),
                governs: String::new(),
                failed: Vec::new(),
            };
            for c in &self.cases {
                match &c.fe {
                    Some(Ok(r)) => {
                        if let Some(g) = r.governing()
                            && let Some(u) = g.usage
                            && s.usage.is_none_or(|w| u > w)
                        {
                            s.usage = Some(u);
                            s.case = c.name.clone();
                            s.governs = format!("member {} ({})", g.id, g.governs);
                        }
                    }
                    Some(Err(e)) => s.failed.push((c.name.clone(), e.clone())),
                    None => {}
                }
            }
            out.push(s);
        }
        if any_pole {
            let mut s = CheckSummary {
                check: "pole",
                usage: None,
                case: String::new(),
                governs: String::new(),
                failed: Vec::new(),
            };
            for c in &self.cases {
                match &c.pole {
                    Some(Ok(p)) if s.usage.is_none_or(|w| p.usage > w) => {
                        s.usage = Some(p.usage);
                        s.case = c.name.clone();
                        s.governs = p.governs.clone();
                    }
                    Some(Err(e)) => s.failed.push((c.name.clone(), e.clone())),
                    _ => {}
                }
            }
            out.push(s);
        }
        out
    }
}

/// The cases of `table` to solve (`only`: that one; none: every case),
/// each with the strength and self-weight factors of its recipe in
/// `recipes` (1 when it has none: `model` and typed cases).
pub fn cases_to_run(
    table: &LoadTable,
    recipes: &[LoadCaseRecipe],
    only: Option<&str>,
) -> Vec<CaseToRun> {
    table
        .cases()
        .into_iter()
        .filter(|c| only.is_none_or(|o| o == *c))
        .map(|c| {
            let recipe = recipes.iter().find(|r| r.name == c);
            CaseToRun {
                name: c.to_owned(),
                strength: recipe.map_or(1.0, |r| r.strength),
                self_weight: recipe.map_or(1.0, |r| r.self_weight_factor()),
            }
        })
        .collect()
}

/// The structure has a finite-element model: its own members or a
/// `model = "….ohlfe"`.
pub fn has_model(structure: &StructureType) -> bool {
    structure.model.is_some() || !structure.members.is_empty()
}

/// The model of `input`'s structure on its extension, with what applies
/// to every case.
fn model_on(input: &RunInput) -> (Result<Model, String>, Vec<String>) {
    let s = input.structure;
    let mut notes = Vec::new();
    let model = match &s.model {
        Some(file) => {
            if let Some(e) = input.extension {
                notes.push(format!(
                    "extension {e}: the model file {file} has no extensions — solved as written \
                     (as in the line)"
                ));
            }
            Model::load_with(
                &input.dir.unwrap_or(Path::new(".")).join(file),
                input.sections,
            )
        }
        None => {
            if let Some(e) = input.extension.and_then(|e| s.extension(e))
                && !e.changes_model()
            {
                notes.push(format!(
                    "extension {}: no piece and no legs — the model as written, standing {} m \
                     higher (no force changes)",
                    e.name,
                    e.dz.si()
                ));
            }
            Model::from_structure_type_extended(input.text, input.sections, input.extension)
        }
    };
    (model, notes)
}

/// The sections `model` uses, with their members' count and length.
fn sections_of(model: &Model) -> Vec<SectionUse> {
    model
        .sections
        .iter()
        .enumerate()
        .map(|(k, s)| {
            let members: Vec<_> = model.members.iter().filter(|m| m.section == k).collect();
            SectionUse {
                id: s.id.clone(),
                material: s.profile.as_ref().and_then(|p| p.material.clone()),
                area: s.area,
                elasticity: s.e,
                yield_strength: s.yield_strength,
                weight: s.weight(),
                members: members.len(),
                length: members.iter().map(|m| model.length(m)).sum(),
            }
        })
        .collect()
}

/// The surface loads of case `name` on every member: (pressure × factor,
/// unit vector in structure axes, the load's number).
fn pressures_on_all(recipes: &[LoadCaseRecipe], name: &str) -> Vec<(f64, [f64; 3], usize)> {
    recipes
        .iter()
        .filter(|r| r.name == name)
        .flat_map(|r| r.loads.iter().enumerate())
        .filter(|(_, l)| matches!(l.members, Some(Pick::All)))
        .filter_map(|(k, l)| {
            let p = l.pressure? * l.scale();
            let d = l.direction?.table_vector();
            Some((p, [d[0], d[1], -d[2]], k + 1))
        })
        .collect()
}

/// The surface loads of case `name` on members `"all"` put on the
/// extension's own members of `model` (`ids`): half to each joint, with a
/// note per load.
fn piece_loads(
    model: &Model,
    ids: &[String],
    recipes: &[LoadCaseRecipe],
    name: &str,
    extension: &str,
    notes: &mut Vec<String>,
) -> Vec<NodalLoad> {
    let mut out = Vec::new();
    for (p, d, number) in pressures_on_all(recipes, name) {
        let (mut total, mut on, mut no_area) = ([0.0; 3], 0, 0);
        for id in ids {
            let Some(k) = model.members.iter().position(|m| &m.id == id) else {
                continue;
            };
            let Some(a) = model.projected_area(k, d) else {
                no_area += 1;
                continue;
            };
            let f = d.map(|x| x * p * a);
            for node in [model.members[k].i, model.members[k].j] {
                out.push(NodalLoad {
                    node,
                    force: f.map(|x| 0.5 * x),
                    moment: [0.0; 3],
                });
            }
            for (t, x) in total.iter_mut().zip(f) {
                *t += x;
            }
            on += 1;
        }
        let size = total.iter().map(|x| x * x).sum::<f64>().sqrt();
        notes.push(format!(
            "extension {extension}: the pressure of load {number} also on its {on} member(s), \
             {:.2} kN{}",
            size / 1000.0,
            if no_area > 0 {
                format!(" ({no_area} without an area: not in the cross-section library, left out)")
            } else {
                String::new()
            }
        ));
    }
    out
}

/// The case's own loads on the structure (`[load_case.structure]`) on the
/// extension's own members of `model` (`ids`), as on the body: its wind
/// (the member wind with the section library's wind factors, pressure ×
/// factor, iced members wider) and its ice and weight per metre (on the
/// piece's members it picks: every member, or those named), half to each
/// joint, with a note per load. A wind whose pressure the line does not
/// give (a `pressure_of` not resolved) is said and left out.
fn piece_part_loads(
    model: &Model,
    ids: &[String],
    recipe: Option<&LoadCaseRecipe>,
    sections: &SectionLibrary,
    extension: &str,
    notes: &mut Vec<String>,
) -> Vec<NodalLoad> {
    let mut out = Vec::new();
    let Some(own) = recipe.and_then(|r| r.structure.as_ref()) else {
        return out;
    };
    let piece: Vec<usize> = ids
        .iter()
        .filter_map(|id| model.members.iter().position(|m| &m.id == id))
        .collect();
    let mut half = |k: usize, f: [f64; 3]| {
        for node in [model.members[k].i, model.members[k].j] {
            out.push(NodalLoad {
                node,
                force: f.map(|x| 0.5 * x),
                moment: [0.0; 3],
            });
        }
    };
    if !own.wind.is_empty() {
        let setup = ohl_structures_fe::wind::MemberWindSetup::new(model, sections.wind.as_ref());
        let extra: Vec<f64> = model.members.iter().map(|m| own.widening(&m.id)).collect();
        for (k, w) in own.wind.iter().enumerate() {
            let Some(p) = w.pressure else {
                notes.push(format!(
                    "extension {extension}: wind {} takes the pressure of {} — the line gives \
                     none: not on its members",
                    k + 1,
                    w.pressure_of.as_deref().unwrap_or("—")
                ));
                continue;
            };
            let t = w.from.towards();
            let wind = setup.wind_iced(p * w.factor, [t[0], t[1], -t[2]], &extra);
            let mut total = [0.0; 3];
            for (row, r) in wind.members.iter().enumerate() {
                if piece.contains(&r.member) {
                    let f = wind.member_force(row);
                    for (s, x) in total.iter_mut().zip(f) {
                        *s += x;
                    }
                    half(r.member, f);
                }
            }
            notes.push(format!(
                "extension {extension}: wind {} ({}) also on its {} member(s), {:.2} kN",
                k + 1,
                ohl_structure_loads::wind_text(w),
                piece.len(),
                total.iter().map(|x| x * x).sum::<f64>().sqrt() / 1000.0
            ));
        }
    }
    for (k, ice) in own.ice.iter().enumerate() {
        let (mut total, mut on, mut without) = (0.0, 0, 0);
        for &m in &piece {
            if !ice.on(&model.members[m].id) {
                continue;
            }
            let perimeter = model.sections[model.members[m].section]
                .profile
                .as_ref()
                .map(crate::outline_perimeter);
            match ice.weight_per_metre(perimeter) {
                Ok(w) => {
                    let f = w * model.length(&model.members[m]) * ice.factor;
                    // Down: −z in structure axes.
                    half(m, [0.0, 0.0, -f]);
                    total += f;
                    on += 1;
                }
                Err(_) => without += 1,
            }
        }
        if on + without > 0 {
            notes.push(format!(
                "extension {extension}: ice {} ({}) also on its {on} member(s), {:.2} kN{}",
                k + 1,
                ohl_structure_loads::ice_text(ice),
                total / 1000.0,
                if without > 0 {
                    format!(" ({without} without an outline: not in the cross-section library, left out)")
                } else {
                    String::new()
                }
            ));
        }
    }
    out
}

/// The pole check of case `case` with the rows of `table`.
fn pole_case(
    input: &RunInput,
    pole: &Pole,
    case: &CaseToRun,
    budget: &Budget,
) -> Result<PoleResult, String> {
    let t = input.structure;
    let mut notes = Vec::new();
    let mut loads = Vec::new();
    let mut elsewhere = 0;
    for r in input.table.case(&case.name).filter(|r| r.missing.is_none()) {
        match r.at {
            At::Point => match t.point_local(&r.label, None) {
                Some(point) => loads.push(PoleLoad {
                    point,
                    force: [r.force[0], r.force[1], -r.force[2]],
                }),
                None => notes.push(format!(
                    "point {}: not a point of the structure (its load left out)",
                    r.label
                )),
            },
            At::Joint | At::Member => elsewhere += 1,
        }
    }
    if elsewhere > 0 {
        notes.push(format!(
            "{elsewhere} row(s) on joints or members: not on the pole{}",
            if has_model(t) {
                " (in the finite-element model)"
            } else {
                ", left out"
            }
        ));
    }
    let mut wind = [0.0; 2];
    for (p, d, _) in pressures_on_all(input.recipes, &case.name) {
        wind[0] += p * d[0];
        wind[1] += p * d[1];
    }
    if wind != [0.0; 2] {
        notes.push(format!(
            "wind on the pole: {:.0} Pa from the case's pressure on all members, × drag {}",
            wind[0].hypot(wind[1]),
            pole.drag
        ));
    }
    // The case's wind on the body: on the pole too, pressure × factor
    // from its direction (× the pole's drag).
    let own = input
        .recipes
        .iter()
        .find(|r| r.name == case.name)
        .and_then(|r| r.structure.as_ref());
    if let Some(own) = own {
        for (k, w) in own.wind.iter().enumerate() {
            match w.pressure {
                Some(p) => {
                    let t = w.from.towards();
                    wind[0] += p * w.factor * t[0];
                    wind[1] += p * w.factor * t[1];
                    notes.push(format!(
                        "wind on the pole: wind {} ({}), × drag {}",
                        k + 1,
                        ohl_structure_loads::wind_text(w),
                        pole.drag
                    ));
                }
                None => notes.push(format!(
                    "wind {}: the pressure of {} — the line gives none: not on the pole",
                    k + 1,
                    w.pressure_of.as_deref().unwrap_or("—")
                )),
            }
        }
        if !own.ice.is_empty() {
            notes.push(
                "ice on members: not on the pole (the pole check takes no ice of its own; \
                 the ice on the members is in the finite-element model)"
                    .into(),
            );
        }
    }
    // The pole's own weight in the P-Delta model times the case's
    // self-weight factor, as the members' (physics review 2026-10-26).
    let c = ohl_structures_fe::check_pole_factored(
        pole,
        &loads,
        wind,
        case.strength,
        case.self_weight,
        true,
        budget,
    )?;
    Ok(PoleResult {
        usage: c.usage,
        height: c.height,
        moment: c.moment,
        capacity: c.capacity,
        ground_moment: c.ground_moment,
        wind_moment: c.wind_moment,
        amplification: c.amplification,
        wind,
        governs: format!("pole {}: {}", pole.id, c.governs),
        notes,
    })
}

/// Solves the cases of `input`: the finite-element model on the
/// extension (when the structure has members or a model file) and the
/// pole check (when it has a pole). Err: the model does not read, or the
/// structure has neither a model nor a pole.
///
/// Expected cost: that of [`crate::analyse_cases`] per case (a few
/// milliseconds for a few hundred members) plus a pole check (a 12
/// element frame, P-Delta: well under a millisecond); `budget` bounds
/// each case.
pub fn analyse_structure(input: &RunInput, budget: &Budget) -> Result<StructureRun, String> {
    let t = input.structure;
    let dz = match input.extension {
        Some(e) => t
            .extension(e)
            .ok_or_else(|| format!("no extension \"{e}\""))?
            .dz
            .si(),
        None => 0.0,
    };
    let mut run = StructureRun {
        extension: input.extension.map(str::to_owned),
        dz,
        ..StructureRun::default()
    };
    let dir = input.dir.unwrap_or(Path::new("."));
    let pole = ohl_structures_fe::pole::structure_pole(t, dir, input.sections);
    let model = if has_model(t) {
        let (m, notes) = model_on(input);
        run.notes.extend(notes);
        match m {
            Ok(m) => Some(m),
            // A pole with members not designed: the pole check alone.
            Err(e) if pole.is_some() => {
                run.notes.push(format!(
                    "members: the model does not build ({e}) — the pole check alone"
                ));
                None
            }
            Err(e) => return Err(e),
        }
    } else {
        None
    };
    if let Some(m) = &model {
        run.sections = sections_of(m);
    }
    if model.is_none() && pole.is_none() {
        return Err("the structure has no members, no model file and no pole".into());
    }
    run.pole = pole.as_ref().map(|p| {
        p.as_ref().map_err(Clone::clone).map(|p| PoleInfo {
            id: p.id.clone(),
            material: format!("{:?}", p.material).to_lowercase(),
            height: p.height(),
            ground_diameter: p.ground_diameter,
            top_diameter: p.top_diameter,
            drag: p.drag,
            ground_capacity: p.capacity_at(0.0),
        })
    });
    // The extension's own members (their surface loads).
    let piece: Vec<String> = match (input.extension.and_then(|e| t.extension(e)), &t.model) {
        (Some(e), None) => e.members.iter().map(|m| m.id.clone()).collect(),
        _ => Vec::new(),
    };
    // The cases on the cores, in their order (2026-10-26: each case on
    // its own, the same results whichever thread solves it).
    let cases = input.cases;
    let solved = crate::in_case_order(cases.len(), crate::threads(cases.len()), |k| {
        let c = &cases[k];
        let fe = model.as_ref().map(|m| {
            let mut notes = Vec::new();
            let extra = match input.extension {
                Some(e) if !piece.is_empty() => {
                    let mut l = piece_loads(m, &piece, input.recipes, &c.name, e, &mut notes);
                    l.extend(piece_part_loads(
                        m,
                        &piece,
                        input.recipes.iter().find(|r| r.name == c.name),
                        input.sections,
                        e,
                        &mut notes,
                    ));
                    l
                }
                _ => Vec::new(),
            };
            analyse_one(m, input.table, c, &extra, budget).map(|mut r| {
                r.notes.extend(notes);
                r
            })
        });
        let pole = pole.as_ref().map(|p| match p {
            Ok(p) => pole_case(input, p, c, budget),
            Err(e) => Err(e.clone()),
        });
        StructureCase {
            name: c.name.clone(),
            fe,
            pole,
        }
    });
    run.cases.extend(solved);
    Ok(run)
}
