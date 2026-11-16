// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Where the structure application meets the core: the one crate that
//! depends on both. The application's crates (`ohl-structure-loads`, …)
//! take and give plain data; this crate turns the core's results into
//! that data and back.
//!
//! - [`line_values`]: the loads the line puts on a placed structure
//!   (`ohl_loads::structure_loads`) as [`LineValues`].
//! - [`fe_loads`]: one case of a [`LoadTable`] as the loads of the
//!   structure's finite-element model (`ohl_structures_fe`).
//! - [`structure_cases`]: the cases the line sends to the placed
//!   structures — its load case library, of which each placed structure
//!   takes the cases of its structure groups (owner, 2026-11-15: the
//!   library cases only; the weather cases once sent unfactored are gone);
//!   [`structure_case_loads`] computes their loads.
//! - [`placed_values`], [`placed_values_with`]: the same with the placed structure's line, its
//!   type and the other places of the type, as the line-values file the
//!   line application hands to the structure application, each case
//!   marked with its source ([`CaseSource`]) and, per case, the wires at
//!   the structure that did not solve ([`missing_wires`]).
//! - [`analyse_cases`]: cases of a [`LoadTable`] solved on the
//!   structure's model, the results as plain data ([`CaseResult`]);
//!   [`analyse_structure`]: the same on an extension, with the pole
//!   check ([`StructureRun`]: what the results panel and the structure
//!   report show).
//! - [`SectionAreas`]: the members' areas from the section library, for
//!   the surface loads of the recipes
//!   ([`ohl_structure_loads::evaluate_with_areas`]); [`section_library`]
//!   reads it.
//! - [`string_volumes`]: the volumes on a structure's strings (their
//!   insulator assemblies' shapes) placed on its strings in still air, as
//!   triangles for OHL-Structure's view (the line's placement,
//!   `ohl_clearance::volumes`).
//! - [`export_structure`]: a level 3 structure with its section library
//!   and load table as plain data for the exports
//!   (`ohl-structure-export`: IFC, SAF, CSV).

mod export;
mod joint_sheet;
mod joints;
#[cfg(test)]
mod joints_tests;
mod member_solids;
#[cfg(test)]
mod member_solids_tests;
mod member_view;
mod plate_checks;
#[cfg(test)]
mod plate_checks_tests;
mod plate_frame;
/// The member check by a plugin: the core crate `ohl-member-check`
/// (shared with `ohl structures fe` and the line's structure check).
pub use ohl_member_check as plugin_check;
mod run;
mod volumes;
mod welds;

pub use export::{
    ExportBolt, ExportKind, ExportLoad, ExportLoadCase, ExportMaterial, ExportMember, ExportNode,
    ExportPlate, ExportSection, ExportStructure, LoadPlace, export_joints, export_structure,
    member_axes,
};
pub use joint_sheet::{
    FaceView, LineKind, Mark, face_axes, face_pdf, face_sheet, face_svg, face_view, faces_of, reach,
};
pub use joints::{
    Angle, Arrival, BOLT_DRAWN, BoltPart, BoltSet, Completeness, CoverDetail, FLAT_OFF,
    JointDetail, JointDetails, NESTED_ANGLE_DEG, NO_OUTLINE, PartChain, PlateDetail, Problem,
    Profile, Similar, Source, SpliceDetail, Valued, bolt_faces, bolted_cover_leg, bolted_onto,
    joint_details, part_chains, similar_joints,
};
pub use member_solids::{
    BoltSolid, MemberSolid, PlateSolid, SolidFace, bolt_solids, ear_clip, member_solids,
    outline_at, plate_solids, prism_faces, prism_faces_capped, section_at, steel_start,
};
pub use member_view::{
    Elevation, FlatLeg, Hole, MemberCapacity, MemberEnds, RESISTANCE_FROM_PLUGIN, bolt_rules,
    elevation, flat_leg, member_capacity, member_ends, no_bolt_rules,
};
pub use ohl_structures::connections::{
    BoltedLeg, ConnectionKind, CoverFace, CoverLeg, Face, WeldKind,
};
pub use ohl_structures_fe::bolt_rules::{BoltRules, Breach, Snap, breaches, snap};
pub use plate_checks::{
    PlateCase, PlatePart, PlatePlans, PlateUsage, joint_plate_usage, plate_plans, plate_usage,
    plate_usage_cases,
};
pub use plate_frame::{PlateFrame, insert_corner, offset_outline, plane_normal};
pub use run::{
    CheckSummary, PoleInfo, PoleResult, RunInput, SectionUse, StructureCase, StructureRun,
    analyse_structure, cases_to_run, has_model,
};
pub use volumes::{VolumeMesh, string_volumes};
pub use welds::{Flat, PlannedPlate, Weld, flat_roll, joint_legs, seam, welded_plan};

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use ohl_core::budget::Budget;
use ohl_line::Line;
use ohl_loads::{CaseLoads, LoadCase};
pub use ohl_sections::SectionLibrary;
use ohl_structure_loads::{
    At, CaseSource, CaseWind, Direction, LinePoint, LineQuantity, LineValue, LineValues, LoadTable,
    MemberAreas, MissingWire, OtherPlace, PlacedValues, PointMoments,
};
use ohl_structures::StructureType;
/// The one usage classification (over / near / pass, a usage that is not
/// a number over) of the line's checks, for the structure application's
/// colours and reports.
pub use ohl_structures_fe::check::{
    NEAR_FULL, NOT_SAFE, UsageClass, classify, classify_usage, not_safe, usage_order,
};
use ohl_structures_fe::{Loads, Model, NodalLoad};

/// The values the line gives at placed structure `structure`: per point,
/// its sets and phases (the phase labels of the sections hanging there)
/// and, per load case, the wires' force unfactored and factored
/// (transverse, longitudinal, vertical down), and the wind on the
/// structure's own body (`ohl_loads::CaseLoads::structure_wind`: pressure,
/// factor, side — the case's `[case.structure]` values or its weather
/// case's pressure and transverse factor), and (2026-11-10) the
/// horizontal pull of the point's back spans and of its ahead spans,
/// along each span, in the structure's axes (transverse, longitudinal),
/// unfactored (`ohl_loads::AttachmentLoad::pull_back`, `pull_ahead`:
/// "back" is the span before the point in its section's order of points,
/// "ahead" the span after it; a side without a span 0) — what a structure
/// load case's `back` and `ahead` shares scale; the line only transmits
/// them. At a point holding more than one wire set (2026-11-11), also
/// each set's part of those pulls (`ohl_loads::AttachmentLoad::
/// pulls_by_set`): values with the point and the set, which make the
/// line-values file version 7. The wind bands' loads (no
/// point) are left out: the structure application puts the body wind on
/// the members; tensions per span, wind on the wires and ice on their own
/// are not given by the line's structure loads yet (the recipes using
/// them get "no line value").
pub fn line_values(loads: &[CaseLoads], line: &Line, structure: &str) -> LineValues {
    let mut out = LineValues::default();
    for case in loads {
        // This structure's: with use_local, times its wind_static (the one
        // rule, ohl_loads::CaseLoads::structure_wind_at).
        let placed = line.structure(structure);
        let mut w = case.structure_wind_at(placed);
        // A wind relative to the line, in a turned structure's axes.
        if let Some(s) = placed {
            w = w.on_structure(line, s);
        }
        out.structure_wind.push(CaseWind {
            case: case.case.name.clone(),
            pressure: w.pressure,
            factor: w.factor,
            towards: if w.towards < 0.0 {
                Direction::Left
            } else {
                Direction::Right
            },
            origin: w.origin().to_owned(),
            // OHL-Structure's convention: 0° from the right, 90° from
            // ahead (an old angle as read, bit for bit).
            from_deg: w.has_longitudinal().then(|| w.direction.old_degrees()),
        });
        // Per point, its loads summed (2026-11-14): a post's base has a
        // load of its own for what acts at the post's top and one for its
        // own weight and wind at its middle — before, each was sent and
        // only the first was read (forces missing, not on the safe side).
        let mut at_points: Vec<ohl_loads::AttachmentLoad> = Vec::new();
        let mut moments: Vec<PointMoments> = Vec::new();
        for l in case
            .loads
            .iter()
            .filter(|l| l.structure == structure && !l.point.is_empty())
        {
            let by = l.moments_by_component();
            if by.iter().flatten().chain(&l.couple).any(|m| *m != 0.0) {
                let m = match moments.iter_mut().find(|m| m.point == l.point) {
                    Some(m) => m,
                    None => {
                        moments.push(PointMoments {
                            case: case.case.name.clone(),
                            point: l.point.clone(),
                            ..PointMoments::default()
                        });
                        moments.last_mut().expect("pushed")
                    }
                };
                let parts = [
                    &mut m.from_transverse,
                    &mut m.from_longitudinal,
                    &mut m.from_vertical,
                ];
                for (sum, part) in parts.into_iter().zip(by) {
                    for (s, x) in sum.iter_mut().zip(part) {
                        *s += x;
                    }
                }
                for (s, x) in m.couple.iter_mut().zip(l.couple) {
                    *s += x;
                }
            }
            match at_points.iter_mut().find(|a| a.point == l.point) {
                Some(a) => {
                    for k in 0..3 {
                        a.local[k] += l.local[k];
                        a.factored[k] += l.factored[k];
                    }
                    for k in 0..2 {
                        a.pull_back[k] += l.pull_back[k];
                        a.pull_ahead[k] += l.pull_ahead[k];
                    }
                    for p in &l.pulls_by_set {
                        match a.pulls_by_set.iter_mut().find(|q| q.set == p.set) {
                            Some(q) => {
                                for k in 0..2 {
                                    q.back[k] += p.back[k];
                                    q.ahead[k] += p.ahead[k];
                                }
                            }
                            None => a.pulls_by_set.push(*p),
                        }
                    }
                    for set in &l.sets {
                        if !a.sets.contains(set) {
                            a.sets.push(*set);
                        }
                    }
                    for x in &l.sections {
                        if !a.sections.contains(x) {
                            a.sections.push(x.clone());
                        }
                    }
                }
                None => at_points.push(l.clone()),
            }
        }
        out.moments.extend(moments);
        for l in &at_points {
            if !out.points.iter().any(|p| p.label == l.point) {
                let mut phases: Vec<String> = Vec::new();
                for s in &l.sections {
                    if let Some(sec) = line.sections.iter().find(|x| &x.id == s)
                        && !sec.phase_label.is_empty()
                        && !phases.contains(&sec.phase_label)
                    {
                        phases.push(sec.phase_label.clone());
                    }
                }
                out.points.push(LinePoint {
                    label: l.point.clone(),
                    sets: l.sets.clone(),
                    phases,
                });
            }
            let quantities = [
                (LineQuantity::Transverse, l.local[0]),
                (LineQuantity::Longitudinal, l.local[1]),
                (LineQuantity::Vertical, l.local[2]),
                (LineQuantity::TransverseFactored, l.factored[0]),
                (LineQuantity::LongitudinalFactored, l.factored[1]),
                (LineQuantity::VerticalFactored, l.factored[2]),
                // The spans' horizontal pulls (line-values file 6): a
                // side without a span sends 0, so that "not sent" is an
                // older file only.
                (LineQuantity::PullBackTransverse, l.pull_back[0]),
                (LineQuantity::PullBackLongitudinal, l.pull_back[1]),
                (LineQuantity::PullAheadTransverse, l.pull_ahead[0]),
                (LineQuantity::PullAheadLongitudinal, l.pull_ahead[1]),
            ];
            for (quantity, value) in quantities {
                out.values.push(LineValue {
                    case: case.case.name.clone(),
                    point: Some(l.point.clone()),
                    set: None,
                    quantity,
                    value,
                });
            }
            // A point holding several sets (line-values file 7): each
            // set's part of the pulls, 0 for a set whose spans pull
            // nothing — so that "not sent" is an older file only.
            if l.sets.len() > 1 {
                for &set in &l.sets {
                    let (back, ahead) = l
                        .pulls_by_set
                        .iter()
                        .find(|p| p.set == set)
                        .map_or(([0.0; 2], [0.0; 2]), |p| (p.back, p.ahead));
                    for (quantity, value) in [
                        (LineQuantity::PullBackTransverse, back[0]),
                        (LineQuantity::PullBackLongitudinal, back[1]),
                        (LineQuantity::PullAheadTransverse, ahead[0]),
                        (LineQuantity::PullAheadLongitudinal, ahead[1]),
                    ] {
                        out.values.push(LineValue {
                            case: case.case.name.clone(),
                            point: Some(l.point.clone()),
                            set: Some(set),
                            quantity,
                            value,
                        });
                    }
                }
            }
        }
    }
    out
}

/// The cases the line sends to the placed structures (owner, 2026-11-15:
/// "the library's cases" only): its load case library, of which each
/// placed structure takes the cases of its structure groups
/// ([`placed_values_with`]), their unfactored values per quantity and
/// their factors. An extra load case in OHL-Structure takes the
/// unfactored line forces of any of them, broken-wire ones included. The
/// weather cases once sent unfactored on their own (Line › Criteria ›
/// Loads for structures, 2026-10-25 to 2026-11-15) are no longer sent.
#[derive(Debug, Clone, Default)]
pub struct StructureCases {
    /// The line's load case library (its cases as they are, factors
    /// kept): each placed structure takes those of its groups.
    pub library: Vec<LoadCase>,
    /// The line's weather cases (those the library's cases name).
    pub library_weather: Vec<ohl_weather::WeatherCase>,
    /// Every case takes the structures' local conditions (the line's
    /// check's **Use local conditions**, [`StructureCases::with_local`]).
    pub local: bool,
    /// Solved at calculation level 3 or 4 (the line's setting, or the
    /// line's Structure loads window's; plan of 2026-11-15, phases 5 and
    /// 6): the level, the structure types and the structure models'
    /// sections the structures are set up from ([`StructureCases::at_level`]).
    /// None: rigid (level 2).
    pub level3: Option<
        Box<(
            ohl_line::solve::flexible::CalculationLevel,
            ohl_structures::StructureLibrary,
            ohl_sections::SectionLibrary,
        )>,
    >,
}

impl StructureCases {
    /// These cases solved at calculation level `level`: 3 as
    /// [`StructureCases::at_level_three`]; 4 with each structure's full
    /// model besides (OHL-Structure gets the rigid and the level-4 solve
    /// of each case); 2: rigid, as without.
    pub fn at_level(
        mut self,
        level: ohl_line::solve::flexible::CalculationLevel,
        types: ohl_structures::StructureLibrary,
        sections: ohl_sections::SectionLibrary,
    ) -> Self {
        self.level3 = level.flexible().then(|| Box::new((level, types, sections)));
        self
    }

    /// These cases solved at calculation level 3: each structure with a
    /// model a linear stiffness at its attachment points, set up from
    /// `types` and `sections` ([`structure_case_loads`]); OHL-Structure
    /// gets the rigid and the level-3 solve of each case, each a case
    /// with its level (the line-values file's `levels`).
    pub fn at_level_three(
        self,
        types: ohl_structures::StructureLibrary,
        sections: ohl_sections::SectionLibrary,
    ) -> Self {
        self.at_level(
            ohl_line::solve::flexible::CalculationLevel::Linear,
            types,
            sections,
        )
    }

    /// No case to send.
    pub fn is_empty(&self) -> bool {
        self.library.is_empty()
    }

    /// These cases as the line's check takes them with **Use local
    /// conditions** ticked (`on`; owner's answer 3 of 2026-11-14): every
    /// library case as if it said `use_local = true`
    /// (`ohl_loads::with_local`: the wires on the localized line, the body
    /// wind × each structure's `wind_static` once, the structures' local
    /// ice) — so OHL-Structure checks with the loads of the line's check.
    /// Unticked: as they are (a case's own `use_local` kept).
    pub fn with_local(mut self, on: bool) -> Self {
        if on {
            self.library = ohl_loads::with_local(self.library, true);
            self.local = true;
        }
        self
    }
}

/// How OHL-Structure is told that the line's cases take the local
/// conditions (the note of the line-values file).
pub const WITH_LOCAL: &str = "With the local conditions";

/// How OHL-Structure is told which structure groups the placed structure
/// is in (the note of the line-values file).
pub const GROUPS: &str = "Groups";

/// How a library case that does not compute is said in the note of the
/// line-values file of the structures taking it (2026-11-14).
pub const NOT_SENT: &str = "NOT SENT";

/// The cases the line sends to its placed structures ([`StructureCases`]):
/// its load case library `load_cases`, with the line's weather cases
/// `weather` (in their order).
pub fn structure_cases(
    weather: &[ohl_weather::WeatherCase],
    load_cases: &[LoadCase],
) -> StructureCases {
    StructureCases {
        library: load_cases.to_vec(),
        library_weather: weather.to_vec(),
        local: false,
        level3: None,
    }
}

/// The loads of the cases sent to the placed structures.
#[derive(Debug, Clone, Default)]
pub struct StructureCaseLoads {
    /// Every case takes the structures' local conditions
    /// ([`StructureCases::with_local`]).
    pub local: bool,
    /// The loads of the line's load case library's cases, factored as
    /// the library says ([`StructureCases::library`]); a structure not
    /// taking a case has no loads in it.
    pub library: Vec<CaseLoads>,
    /// The line's weather cases (for the ice on the structures).
    pub library_weather: Vec<ohl_weather::WeatherCase>,
    /// The library's cases that do not compute (a case naming a weather
    /// case that is not there…), each with why: each fails only itself
    /// (2026-11-14), the others are sent.
    pub library_failed: Vec<(String, String)>,
    /// The names of every case of the library (the groups' cases are
    /// checked against them).
    pub library_names: Vec<String>,
}

/// The structure loads of the line's load case library
/// ([`StructureCases::library`]) on `line` with its cables `library`, each
/// case on its own (`ohl_loads::structure_loads_each`: a faulty one fails
/// only itself, 2026-11-14). Expected cost: that of the structure loads,
/// about one solve of every section per case (seconds for a long line),
/// bounded by `budget` per section.
pub fn structure_case_loads(
    line: &Line,
    library: &ohl_cables::Library,
    cases: StructureCases,
    budget: &Budget,
) -> Result<StructureCaseLoads, String> {
    let library_names: Vec<String> = cases.library.iter().map(|c| c.name.clone()).collect();
    let (mut lib, mut library_failed) = (Vec::new(), Vec::new());
    if !cases.library.is_empty() {
        // A set of wind directions: one case per direction (OHL-Structure
        // gets one case each).
        let expanded = ohl_loads::expand_directions(&cases.library);
        // At calculation level 3 or 4 the structures set up once for
        // every case (their stiffness, their own wind and ice per case;
        // level 4 their full models).
        let level = cases.level3.as_ref().map(|l| {
            ohl_loads::prepare_structure_loads_at(
                line,
                library,
                &l.1,
                &l.2,
                &cases.library_weather,
                &expanded,
                l.0,
                budget,
                None,
                None,
            )
        });
        let each = ohl_loads::structure_loads_at(
            line,
            library,
            &cases.library_weather,
            &expanded,
            &library_names,
            budget,
            level.as_ref(),
        );
        for (c, r) in expanded.iter().zip(each) {
            match r {
                Ok(l) => lib.push(l),
                Err(e) => library_failed.push((c.set_name().to_owned(), e)),
            }
        }
    }
    Ok(StructureCaseLoads {
        local: cases.local,
        library: lib,
        library_weather: cases.library_weather,
        library_failed,
        library_names,
    })
}

/// [`placed_values`] of the loads the line sends (`sent`): placed
/// structure `structure`'s cases of the line's load case library — those
/// of its structure groups (`ohl_line::Line::case_sets`, 2026-11-15: the
/// union of its groups'; in no group, every case) — their values as load
/// cases, and as its load cases in OHL-Structure
/// ([`PlacedValues::library_cases`], [`library_recipes`]). The file's note
/// says its groups; a structure that takes no load case (its groups tick
/// none) gets none, and the note says so.
pub fn placed_values_with(
    sent: &StructureCaseLoads,
    line: &Line,
    structure: &str,
    line_name: &str,
) -> PlacedValues {
    let sets = line.case_sets();
    let own: Vec<CaseLoads> = sent
        .library
        .iter()
        .filter(|c| sets.takes(structure, c.case.set_name()))
        .cloned()
        .collect();
    let mut out = placed_values(&own, line, structure, line_name);
    out.library_cases = library_recipes(&own, &sent.library_weather, line, structure);
    // Never silently absent (2026-11-14): a case of its groups that did
    // not compute, and the cases of its groups not in the library, said
    // first in the note (OHL-Structure's header).
    let mut said: Vec<String> = sent
        .library_failed
        .iter()
        .filter(|(name, _)| sets.takes(structure, name))
        .map(|(name, why)| format!("{NOT_SENT}: library case \"{name}\" does not compute ({why})"))
        .collect();
    if sent.local {
        said.push(format!(
            "{WITH_LOCAL} (Checks › Structure loads › Use local conditions in the line): every \
             case takes the structures' local wind and ice, as the line's check"
        ));
    }
    if let Some(u) = ohl_loads::unknown_case_sets(line, &sent.library_names)
        .into_iter()
        .find(|u| u.structure == structure)
    {
        said.push(format!(
            "NOT CHECKED: load case(s) {} of {structure}'s groups not in the line's load case \
             library{}",
            u.names.join(", "),
            if u.none_known {
                " — no library case is sent for it"
            } else {
                ""
            }
        ));
    }
    // Its groups, live (2026-11-15): which it is in and what that gives;
    // the load cases' own lists picking or excluding it.
    let groups = sets.groups_of(structure);
    let mut own = Vec::new();
    if !sets.picked_for(structure).is_empty() {
        own.push(format!(
            "picked for {}",
            sets.picked_for(structure).join(", ")
        ));
    }
    if !sets.excluded_from(structure).is_empty() {
        own.push(format!(
            "excluded from {}",
            sets.excluded_from(structure).join(", ")
        ));
    }
    if !sets.ruled_out_of(structure).is_empty() {
        own.push(format!(
            "not {}: its own list's rules do not hold here",
            sets.ruled_out_of(structure).join(", ")
        ));
    }
    if !sets.ruled_into(structure).is_empty() {
        own.push(format!(
            "{} by its own list's rules",
            sets.ruled_into(structure).join(", ")
        ));
    }
    let own = if own.is_empty() {
        String::new()
    } else {
        format!(" ({} in the load cases' own lists)", own.join("; "))
    };
    said.push(if groups.is_empty() {
        format!(
            "{GROUPS}: {structure} is in no structure group of the line{own} — it takes {} of \
             the load case library",
            match sets.of(structure) {
                ohl_line::CaseSet::EveryBut(c) => format!("every case but {}", c.join(", ")),
                _ => "every case".to_owned(),
            }
        )
    } else {
        format!(
            "{GROUPS}: {structure} is in {}{own} — it takes {} (Structures › Load cases per group \
             in OHL-Design)",
            groups.join(", "),
            match sets.of(structure) {
                ohl_line::CaseSet::These(c) => format!("their load cases: {}", c.join(", ")),
                _ => "no load case (its groups tick none)".to_owned(),
            }
        )
    });
    let said = said.join(" · ");
    out.note = Some(match out.note.take() {
        Some(n) => format!("{said} · {n}"),
        None => said,
    });
    // A structure taking no load case (2026-11-10): none of the library's,
    // said first.
    if sets.takes_none(structure) {
        let said = format!(
            "{structure} {} in the line (its structure groups tick no case): no load case of \
             the line's library is sent for it, and the line makes no structure check of it",
            ohl_loads::TAKES_NO_LOAD_CASE
        );
        out.note = Some(match out.note.take() {
            Some(n) => format!("{said} · {n}"),
            None => said,
        });
    }
    out
}

/// The line's load cases `loads` (library cases, factored) as load cases
/// of placed structure `structure` in OHL-Structure (owner 2026-11-03:
/// "The library's cases"), one per case (a broken wire in every span: one
/// per break, [`CaseLoads::each_break`]), named as the line's case:
///
/// - **line forces**: the case's own values at the attachment points,
///   unfactored, × its vertical, transverse and longitudinal factors;
/// - **self weight** × its vertical factor;
/// - **wind on the body** from the case's direction (any angle,
///   [`ohl_structure_loads::WindFrom::from_degrees`]), its pressure on the structure
///   (with `use_local` × the structure's `wind_static`,
///   [`CaseLoads::structure_wind_at`]) × its factor;
/// - **ice** on every member (`ohl_loads::LoadCase::structure_ice_at`:
///   the thickness, or the structure's local ice — also when the case
///   takes the local conditions, gives no ice of its own and its weather
///   case has ice —, and its density) and
///   **snow** on every member's plan (q = μ·C_e·C_T·s_k,
///   `ohl_loads::LoadCase::structure_snow_at`), × the vertical factor;
/// - its strength factor.
///
/// The rule of the line's own structure checks (`ohl_loads::capacity`),
/// so both applications give the same loads.
pub fn library_recipes(
    loads: &[CaseLoads],
    weather: &[ohl_weather::WeatherCase],
    line: &Line,
    structure: &str,
) -> Vec<ohl_structure_loads::LoadCaseRecipe> {
    use ohl_structure_loads::{
        BodyWindLoad, IceLoad, LineForces, LoadCaseRecipe, OwnLoads, Pick, SnowLoad, WindFrom,
    };
    let placed = line.structure(structure);
    loads
        .iter()
        .flat_map(CaseLoads::each_break)
        .map(|c| {
            let lc = &c.case;
            let mut r = LoadCaseRecipe::new(&lc.name);
            let mut about = vec![format!(
                "from the line's load case library: weather {}, wires {}",
                lc.weather,
                lc.condition.words()
            )];
            if !lc.description.is_empty() {
                about.insert(0, lc.description.clone());
            }
            about.extend(c.notes.iter().cloned());
            r.description = about.join("\n");
            r.strength = lc.strength;
            r.line.push(LineForces {
                vertical: lc.vertical,
                transverse: lc.transverse,
                longitudinal: lc.longitudinal,
                ..LineForces::new(&lc.name)
            });
            let mut own = OwnLoads {
                self_weight: Some(lc.vertical),
                ..OwnLoads::default()
            };
            let mut body = c.structure_wind_at(placed);
            if let Some(s) = placed {
                body = body.on_structure(line, s);
            }
            if body.factored() > 0.0 {
                own.wind.push(BodyWindLoad {
                    // OHL-Structure's convention (0° from the right, 90°
                    // from ahead; an old angle as read, bit for bit).
                    from: WindFrom::from_degrees(body.direction.old_degrees()),
                    pressure: Some(body.pressure),
                    pressure_of: None,
                    factor: body.factor,
                });
            }
            if let Some(w) = weather.iter().find(|w| w.name == lc.weather)
                && let Ok(Some(ice)) = lc.structure_ice_at(w, placed)
            {
                if lc.takes_local_ice(w) {
                    about.push(format!(
                        "the structure's local ice, {:.0} mm (the case takes the local \
                         conditions and weather case \"{}\" has ice)",
                        ice.thickness * 1000.0,
                        w.name
                    ));
                    r.description = about.join("\n");
                }
                own.ice.push(IceLoad {
                    thickness: Some(ice.thickness),
                    density: Some(ice.density),
                    load: None,
                    members: Pick::All,
                    factor: lc.vertical,
                });
            }
            if let Some(snow) = lc.structure_snow_at(placed)
                && snow.load() > 0.0
            {
                own.snow.push(SnowLoad {
                    load: snow.load(),
                    members: Pick::All,
                    factor: lc.vertical,
                });
            }
            r.self_weight = 1.0;
            r.structure = Some(own);
            r
        })
        .collect()
}

/// The line-values file of placed structure `structure` in line
/// `line_name`: its values ([`line_values`]) in the load cases `loads`
/// (sources marked), its structure type and the other placed structures
/// of that type with their values; the note names the sections a case did
/// not solve, and per case the sections and jumpers at the structure that
/// did not solve are listed ([`PlacedValues::missing`],
/// [`missing_wires`]): those cases' values are incomplete.
pub fn placed_values(
    loads: &[CaseLoads],
    line: &Line,
    structure: &str,
    line_name: &str,
) -> PlacedValues {
    // A broken wire in every span: a case per break, as the structure's
    // own checks combine the components (the line's envelope is for its
    // allowable-loads check only; physics review 2026-10-26); at
    // calculation level 3 the rigid and the flexible solve, each a case
    // with its level (2026-11-16).
    let levels: Vec<u8> = loads.iter().flat_map(CaseLoads::each_level).collect();
    let loads: Vec<CaseLoads> = loads.iter().flat_map(CaseLoads::each_break).collect();
    let sources: Vec<CaseSource> = std::iter::repeat_n(CaseSource::LoadCase, loads.len()).collect();
    let loads = loads.as_slice();
    let structure_type = line
        .structures
        .iter()
        .find(|s| s.id == structure)
        .and_then(|s| s.structure_type.clone())
        .unwrap_or_default();
    let others = line
        .structures
        .iter()
        .filter(|s| {
            s.id != structure
                && !structure_type.is_empty()
                && s.structure_type.as_deref() == Some(structure_type.as_str())
        })
        .map(|s| OtherPlace {
            structure: s.id.clone(),
            values: line_values(loads, line, &s.id),
            missing: missing_wires(loads, &s.id),
        })
        .collect();
    // Sections a case did not solve: their loads are missing.
    let failed: Vec<String> = loads
        .iter()
        .filter(|c| !c.failed.is_empty())
        .map(|c| {
            format!(
                "{}: {} section(s) not solved ({})",
                c.case.name,
                c.failed.len(),
                c.failed.join("; ")
            )
        })
        .collect();
    PlacedValues {
        cases: loads.iter().map(|c| c.case.name.clone()).collect(),
        sources,
        values: line_values(loads, line, structure),
        others,
        note: (!failed.is_empty()).then(|| format!("values missing — {}", failed.join(" · "))),
        missing: missing_wires(loads, structure),
        levels: if levels.iter().all(|l| *l == 2) {
            Vec::new()
        } else {
            levels
        },
        ..PlacedValues::pending(structure, line_name, &structure_type, "")
    }
    // Version 7 only with a set's part of a point's pulls among them.
    .at_needed_version()
}

/// Per case of `loads`, the sections and jumpers at placed structure
/// `structure` that did not solve (`ohl_loads::CaseLoads::missing`): that
/// case's values at it lack their forces.
pub fn missing_wires(loads: &[CaseLoads], structure: &str) -> Vec<MissingWire> {
    loads
        .iter()
        .flat_map(|c| {
            c.missing_at(structure).map(|m| MissingWire {
                case: c.case.name.clone(),
                kind: m.kind.as_str().to_owned(),
                id: m.id.clone(),
                reason: m.reason.clone(),
            })
        })
        .collect()
}

/// The section library at `path` (a `.ohlsec` file, or every one of a
/// folder); a folder that is not there is an empty library.
pub fn section_library(path: &Path) -> Result<SectionLibrary, String> {
    if !path.exists() {
        return Ok(SectionLibrary::default());
    }
    SectionLibrary::load(path)
}

/// A pole file's (`.ohlp`) id, read and checked as the pole check reads
/// it; or why it does not read (the application's Libraries panel).
pub fn pole_file_id(text: &str) -> Result<String, String> {
    ohl_structures_fe::Pole::parse(text).map(|p| p.id)
}

/// A structure model file's (`.ohlfe`) name, read with the section library
/// `sections` as the finite-element check reads it; or why it does not
/// read (the application's Libraries panel).
pub fn model_file_name(text: &str, sections: &SectionLibrary) -> Result<String, String> {
    Model::parse_with(text, sections).map(|m| m.name)
}

/// The members' areas from the section library: the structure's
/// finite-element model built with it, a member's area for a pressure in
/// a direction its silhouette seen along it
/// ([`Model::projected_area`]: the profile's width for that direction ×
/// the member's length × sin θ). A member whose section is not in the
/// library, or a structure whose model does not build, has no area (the
/// row says why).
///
/// The wind on the body of a case's loads on the structure itself is the
/// member wind of the line's own structure checks
/// ([`ohl_structures_fe::wind::MemberWindSetup`] with the library's
/// `[wind]` factors: faces, solidity, shielding; the faces found once);
/// a member's ice takes its profile's outline
/// ([`ohl_structures_fe::drawn::outline`]: its full perimeter) and its
/// length.
pub struct SectionAreas {
    model: Result<Model, String>,
    wind: Option<ohl_sections::StructureWind>,
    faces: std::cell::OnceCell<Vec<ohl_structures_fe::wind::Face>>,
    /// The structure file's text, its folder and its sections, for the
    /// joints' gusset plates (worked out once, when the wind needs them).
    text: String,
    dir: Option<std::path::PathBuf>,
    sections: SectionLibrary,
    plates: std::cell::OnceCell<(Vec<ohl_structures_fe::wind::WindPlate>, Vec<String>)>,
}

impl SectionAreas {
    /// The areas of `structure` (`text`: its file's text, `dir`: its
    /// folder, for a `model = "….ohlfe"` it names) with `sections` (and
    /// its wind factors).
    pub fn new(
        structure: &StructureType,
        text: &str,
        dir: Option<&Path>,
        sections: &SectionLibrary,
    ) -> Self {
        Self {
            model: model_of(structure, text, dir, sections),
            wind: sections.wind.clone(),
            faces: std::cell::OnceCell::new(),
            text: text.to_owned(),
            dir: dir.map(Path::to_path_buf),
            sections: sections.clone(),
            plates: std::cell::OnceCell::new(),
        }
    }

    /// The gusset plates and splice covers of the structure's joints for
    /// the wind ([`wind_plates`]), with what could not be used.
    fn plates(&self, model: &Model) -> &(Vec<ohl_structures_fe::wind::WindPlate>, Vec<String>) {
        self.plates.get_or_init(|| {
            let bolts = match &self.dir {
                Some(d) => ohl_structures::bolts::BoltLibrary::beside(d).0,
                None => ohl_structures::bolts::BoltLibrary::default(),
            };
            match joint_details(&self.text, &self.sections, &bolts) {
                Ok(d) => wind_plates(model, &d),
                Err(e) => (
                    Vec::new(),
                    vec![format!(
                        "NOT ON THE SAFE SIDE: gusset plates' own wind area not included (the \
                         joints do not read: {e})"
                    )],
                ),
            }
        })
    }

    fn model(&self, what: &str) -> Result<&Model, String> {
        self.model
            .as_ref()
            .map_err(|e| format!("{what}: the model does not build ({e})"))
    }

    fn member(&self, id: &str, what: &str) -> Result<(&Model, usize), String> {
        let model = self.model(what)?;
        let k = model
            .members
            .iter()
            .position(|m| m.id == id)
            .ok_or_else(|| format!("{what}: no member \"{id}\" in the model"))?;
        Ok((model, k))
    }
}

/// The joints' gusset plates and splice covers of `details` as plates in
/// the wind of `model`: at the model's joint of the same id (and place,
/// within a centimetre), the area of the outline's face, the members
/// bolted onto them. With what could not be used: plates without an
/// outline (no bolts onto them to draw it round) and joints not in the
/// model, said not on the safe side.
pub fn wind_plates(
    model: &Model,
    details: &JointDetails,
) -> (Vec<ohl_structures_fe::wind::WindPlate>, Vec<String>) {
    let mut out = Vec::new();
    let (mut no_outline, mut no_joint) = (Vec::new(), Vec::new());
    let member_of = |id: &str| model.members.iter().position(|m| m.id == id);
    for j in &details.joints {
        let pieces = j
            .plates
            .iter()
            .map(|p| (&p.id, &p.outline, &p.members))
            .chain(j.splices.iter().flat_map(|s| {
                s.covers
                    .iter()
                    .map(move |c| (&s.id, &c.outline, &s.members))
            }));
        let node = model.nodes.iter().position(|n| {
            n.id == j.id && (0..3).all(|k| (n.point[k] - j.position[k]).abs() <= 0.01)
        });
        for (id, outline, members) in pieces {
            let Some(node) = node else {
                no_joint.push(format!("{}/{id}", j.id));
                continue;
            };
            let (Some(normal), area) = (plane_normal(outline), polygon_area(outline)) else {
                no_outline.push(format!("{}/{id}", j.id));
                continue;
            };
            out.push(ohl_structures_fe::wind::WindPlate {
                id: format!("{}/{id}", j.id),
                node,
                area,
                normal,
                members: members.iter().filter_map(|m| member_of(m)).collect(),
            });
        }
    }
    let mut notes = Vec::new();
    for (what, list) in [
        ("without an outline (no bolts onto them)", no_outline),
        ("at joints not in the model", no_joint),
    ] {
        if !list.is_empty() {
            notes.push(format!(
                "NOT ON THE SAFE SIDE: {} gusset plate(s) {what} not in the wind: {}",
                list.len(),
                first_ten(&list)
            ));
        }
    }
    (out, notes)
}

/// The first ten of a list, joined, "…" after them.
fn first_ten(list: &[String]) -> String {
    let mut s = list.iter().take(10).cloned().collect::<Vec<_>>().join(", ");
    if list.len() > 10 {
        s.push_str(", …");
    }
    s
}

/// The area of a plane polygon in 3D (Newell's method), m².
fn polygon_area(outline: &[[f64; 3]]) -> f64 {
    let mut n = [0.0; 3];
    for k in 0..outline.len() {
        let (a, b) = (outline[k], outline[(k + 1) % outline.len()]);
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
}

/// The perimeter of a profile's outline (its outer edge, an angle's
/// inside faces too: [`ohl_structures_fe::drawn::outline`]), m.
pub fn outline_perimeter(profile: &ohl_sections::Section) -> f64 {
    let o = ohl_structures_fe::drawn::outline(&profile.shape);
    (0..o.len())
        .map(|k| {
            let (a, b) = (o[k], o[(k + 1) % o.len()]);
            (b[0] - a[0]).hypot(b[1] - a[1])
        })
        .sum()
}

impl MemberAreas for SectionAreas {
    fn area(&self, id: &str) -> Option<f64> {
        self.area_along(id, Direction::Right).ok()
    }

    fn member_ids(&self) -> Option<Vec<String>> {
        let model = self.model.as_ref().ok()?;
        Some(model.members.iter().map(|m| m.id.clone()).collect())
    }

    fn member_wind(
        &self,
        pressure: f64,
        towards: [f64; 3],
        widened: &dyn Fn(&str) -> f64,
    ) -> Result<ohl_structure_loads::BodyWind, String> {
        let model = self.model("no member wind")?;
        let faces = self.faces.get_or_init(|| {
            if ohl_structures_fe::wind::needs_faces(self.wind.as_ref()) {
                ohl_structures_fe::wind::faces(model)
            } else {
                Vec::new()
            }
        });
        let (plates, plate_notes) = self.plates(model);
        let setup = ohl_structures_fe::wind::MemberWindSetup::with_faces(
            model,
            self.wind.as_ref(),
            faces.clone(),
        )
        .with_plates(plates.clone());
        let extra: Vec<f64> = model.members.iter().map(|m| widened(&m.id)).collect();
        // Table axes (vertical down) → structure axes (z up), and back.
        let w = setup.wind_iced(pressure, [towards[0], towards[1], -towards[2]], &extra);
        let mut row_of = vec![None; model.members.len()];
        for (row, r) in w.members.iter().enumerate() {
            row_of[r.member] = Some(row);
        }
        let members = model
            .members
            .iter()
            .enumerate()
            .map(|(k, m)| {
                let section = &model.sections[m.section];
                let force = if section.profile.is_none() {
                    Err(format!(
                        "no area: cross-section \"{}\" is not in the cross-section library",
                        section.id
                    ))
                } else {
                    // A member along the wind has no row there: no force.
                    Ok(row_of[k].map_or([0.0; 3], |row| {
                        let f = w.member_force(row);
                        [f[0], f[1], -f[2]]
                    }))
                };
                (m.id.clone(), force)
            })
            .collect();
        // The plates' loads come after the members' (two per row).
        let joints = w
            .plates
            .iter()
            .zip(w.loads.iter().skip(2 * w.members.len()))
            .map(|(r, l)| {
                let node = plates[r.plate].node;
                let f = l.force;
                (model.nodes[node].id.clone(), [f[0], f[1], -f[2]])
            })
            .collect();
        let mut notes = w.notes;
        notes.extend(plate_notes.iter().cloned());
        Ok(ohl_structure_loads::BodyWind {
            members,
            joints,
            notes,
        })
    }

    fn perimeter(&self, id: &str) -> Result<f64, String> {
        let (model, k) = self.member(id, "no outline")?;
        let section = &model.sections[model.members[k].section];
        section
            .profile
            .as_ref()
            .map(outline_perimeter)
            .ok_or_else(|| {
                format!(
                    "no outline: cross-section \"{}\" is not in the cross-section library",
                    section.id
                )
            })
    }

    fn length(&self, id: &str) -> Result<f64, String> {
        let (model, k) = self.member(id, "no length")?;
        Ok(model.length(&model.members[k]))
    }

    fn plan_area(&self, id: &str) -> Result<f64, String> {
        let (model, k) = self.member(id, "no plan")?;
        let m = &model.members[k];
        let section = &model.sections[m.section];
        if section.profile.is_none() {
            return Err(format!(
                "no plan: cross-section \"{}\" is not in the cross-section library",
                section.id
            ));
        }
        // Its width seen from above × its length on the ground (the rule
        // of ohl_loads' snow on the members); vertical: none.
        Ok(model
            .across(m, [0.0, 0.0, -1.0])
            .map_or(0.0, |(_, sin, width)| width * model.length(m) * sin))
    }

    fn area_along(&self, id: &str, direction: Direction) -> Result<f64, String> {
        let model = self
            .model
            .as_ref()
            .map_err(|e| format!("no area: the model does not build ({e})"))?;
        let k = model
            .members
            .iter()
            .position(|m| m.id == id)
            .ok_or_else(|| format!("no area: no member \"{id}\" in the model"))?;
        // Table axes (vertical down) → structure axes (z up).
        let d = direction.table_vector();
        model.projected_area(k, [d[0], d[1], -d[2]]).ok_or_else(|| {
            let s = &model.sections[model.members[k].section].id;
            format!("no area: cross-section \"{s}\" is not in the cross-section library")
        })
    }
}

/// Case `case` of `table` as the loads of `model` (structure axes, z up):
/// a point's load at the joint carrying it (or shared to the ends of the
/// member it sits on), a joint's at that joint, a
/// member's shared half to each of its joints; rows without a value left
/// out. `self_weight`: the case's factor on the members' weight.
pub fn fe_loads(
    model: &Model,
    table: &LoadTable,
    case: &str,
    self_weight: f64,
) -> Result<Loads, String> {
    let mut nodal = Vec::new();
    // A member's load, put half on each joint, kept whole for the bending
    // it puts on the member between them (2026-11-08).
    let mut spread: Vec<(usize, [f64; 3])> = Vec::new();
    let at = |node: usize, f: [f64; 3], share: f64| NodalLoad {
        node,
        force: [f[0] * share, f[1] * share, -f[2] * share],
        moment: [0.0; 3],
    };
    // Joints and members by id (the first of an id, as a search finds
    // it): a search per row was slow on a large lattice (2026-10-26).
    let mut node_of: HashMap<&str, usize> = HashMap::with_capacity(model.nodes.len());
    for (k, n) in model.nodes.iter().enumerate() {
        node_of.entry(n.id.as_str()).or_insert(k);
    }
    let mut member_of: HashMap<&str, usize> = HashMap::with_capacity(model.members.len());
    for (k, m) in model.members.iter().enumerate() {
        member_of.entry(m.id.as_str()).or_insert(k);
    }
    for r in table.case(case).filter(|r| r.missing.is_none()) {
        match r.at {
            // An attachment point linked to no joint (owner 2026-11-15):
            // its load reaches no member, left out and said with the case
            // (not on the safe side).
            At::Point if model.unlinked_points.contains(&r.label) => {}
            At::Point => {
                // At its joint, or shared to the ends of its member.
                // With a fixed post's moment (2026-11-14): at a frame
                // joint, else left out (said with the case).
                let loads = model
                    .point_load_with(&r.label, [r.force[0], r.force[1], -r.force[2]], r.moment)
                    .ok_or_else(|| {
                        format!("point {}: no joint of the model carries it", r.label)
                    })?;
                nodal.extend(loads.loads);
            }
            At::Joint => {
                let n = node_of
                    .get(r.label.as_str())
                    .copied()
                    .ok_or_else(|| format!("no joint \"{}\" in the model", r.label))?;
                nodal.push(at(n, r.force, 1.0));
            }
            At::Member => {
                let k = member_of
                    .get(r.label.as_str())
                    .copied()
                    .ok_or_else(|| format!("no member \"{}\" in the model", r.label))?;
                let m = &model.members[k];
                nodal.push(at(m.i, r.force, 0.5));
                nodal.push(at(m.j, r.force, 0.5));
                spread.push((k, [r.force[0], r.force[1], -r.force[2]]));
            }
        }
    }
    Ok(Loads {
        nodal,
        self_weight,
        member: spread,
    })
}

/// A case of the load table to solve: its name, the strength factor of
/// the checks and the factor on the structure's own weight.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseToRun {
    /// The case's name in the table.
    pub name: String,
    /// Strength factor (1: none).
    pub strength: f64,
    /// Factor on the members' weight (0: none).
    pub self_weight: f64,
}

/// A joint of the solved model.
#[derive(Debug, Clone, PartialEq)]
pub struct JointResult {
    /// Id.
    pub id: String,
    /// Where it is, structure axes (z up), m.
    pub point: [f64; 3],
    /// How far it moved, m.
    pub displacement: [f64; 3],
    /// It has a support or a spring.
    pub support: bool,
}

/// A member of the solved model.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberResult {
    /// Id.
    pub id: String,
    /// Its joints (places in [`CaseResult::joints`]).
    pub ends: [usize; 2],
    /// Axial force, N (tension positive).
    pub axial: f64,
    /// Share of its capacity used (1 = 100 %); none: no capacity known.
    pub usage: Option<f64>,
    /// What governs, in words.
    pub governs: String,
    /// A cable or a tension-only member gone slack.
    pub slack: bool,
    /// A tension-only member bowed: shortened beyond its own buckling
    /// load, it carries that load (`axial`) and no more.
    pub bowed: bool,
    /// Its eccentric ends (connection shifts) in words: e, and where the
    /// moment went or that it was left out
    /// ([`ohl_structures_fe::offsets::eccentricity_words`]); none: its
    /// ends at its joints.
    pub eccentric: Option<String>,
}

/// A support's reaction: the force and moment on the structure.
#[derive(Debug, Clone, PartialEq)]
pub struct ReactionResult {
    /// The joint.
    pub joint: String,
    /// Force, N, structure axes (z up).
    pub force: [f64; 3],
    /// Moment, N·m.
    pub moment: [f64; 3],
}

/// One case solved: plain data for the application.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseResult {
    /// The case.
    pub case: String,
    /// The joints, with their displacements.
    pub joints: Vec<JointResult>,
    /// The members, with their forces and usage.
    pub members: Vec<MemberResult>,
    /// The reactions.
    pub reactions: Vec<ReactionResult>,
    /// The sum of the loads (the table's and the self-weight), N,
    /// structure axes (z up).
    pub load_sum: [f64; 3],
    /// The sum of the reactions, N (in equilibrium: minus the loads).
    pub reaction_sum: [f64; 3],
    /// Iterations of the solver.
    pub iterations: u64,
    /// Out-of-balance forces at the end, share of the loads.
    pub residual: f64,
    /// P-Delta included.
    pub nonlinear: bool,
    /// How long it took.
    pub took: Duration,
    /// What was left out (rows without a value, moments).
    pub notes: Vec<String>,
    /// The moment of each attachment point's offset (a rigid link from its
    /// joint, 2026-10-21): the points at an offset only, in the order of
    /// the load table.
    pub points: Vec<PointMoment>,
    /// The plates' and welds' checks (2026-11-04, [`plate_usage`]): per
    /// plate, each member's bolts in it and each weld; empty without
    /// plates (or a model of its own file).
    pub plates: Vec<PlateUsage>,
    /// The optional climbing check (`[analysis] climbing_load`, owner
    /// 2026-11-15): flat members under the climbing load alone — not this
    /// case's, the same in every case, apart from the members' usage;
    /// empty when the check is off.
    pub climbing: Vec<ClimbingResult>,
}

/// One member's climbing check ([`ohl_structures_fe::check::climbing_check`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ClimbingResult {
    /// The member's id.
    pub member: String,
    /// Share of its first-yield moment used (none: not checked).
    pub usage: Option<f64>,
    /// In words.
    pub words: String,
}

/// The moment of an attachment point's offset in one case: r × F at its
/// joint ([`ohl_structures_fe::PointLoad`]).
#[derive(Debug, Clone, PartialEq)]
pub struct PointMoment {
    /// The attachment point's label.
    pub label: String,
    /// The moment, N·m, structure axes (z up): the sum over the point's
    /// rows of the load table.
    pub moment: [f64; 3],
    /// Applied at its joint (a frame member carries it); false: left out,
    /// only trusses meet the joint ([`PointMoment::note`]).
    pub applied: bool,
    /// What was left out, in words (none: applied).
    pub note: Option<String>,
}

impl PointMoment {
    /// Its size, N·m.
    pub fn size(&self) -> f64 {
        let m = self.moment;
        (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt()
    }
}

impl CaseResult {
    /// The joint that moved most, and how far, m.
    pub fn largest_displacement(&self) -> Option<(&str, f64)> {
        self.joints
            .iter()
            .map(|j| {
                let d = j.displacement;
                (
                    j.id.as_str(),
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt(),
                )
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// The member using most of its capacity (a usage that is not a
    /// number first, never hidden).
    pub fn governing(&self) -> Option<&MemberResult> {
        self.members
            .iter()
            .filter(|m| m.usage.is_some())
            .max_by(|a, b| {
                ohl_structures_fe::check::usage_order(
                    a.usage.unwrap_or(0.0),
                    b.usage.unwrap_or(0.0),
                )
            })
    }

    /// The plate check using most (a usage that is not a number first).
    pub fn governing_plate(&self) -> Option<&PlateUsage> {
        self.plates
            .iter()
            .filter(|p| p.usage.is_some())
            .max_by(|a, b| {
                ohl_structures_fe::check::usage_order(
                    a.usage.unwrap_or(0.0),
                    b.usage.unwrap_or(0.0),
                )
            })
    }

    /// The largest out-of-balance of Σ loads + Σ reactions over the three
    /// axes, N.
    pub fn imbalance(&self) -> f64 {
        (0..3)
            .map(|k| (self.load_sum[k] + self.reaction_sum[k]).abs())
            .fold(0.0, f64::max)
    }
}

/// A case's name and its result, or why it did not solve.
pub type CaseRun = (String, Result<CaseResult, String>);

/// The finite-element model of `structure`: its own members, or the
/// `model = "….ohlfe"` it names (beside the file, in `dir`); `text` is the
/// structure file's text. A member's section is looked up in `sections`
/// first, the bolts its connections name in the bolt libraries beside it
/// (`Bolts/` in `dir`).
fn model_of(
    structure: &StructureType,
    text: &str,
    dir: Option<&Path>,
    sections: &SectionLibrary,
) -> Result<Model, String> {
    match &structure.model {
        Some(file) => Model::load_with(&dir.unwrap_or(Path::new(".")).join(file), sections),
        None => Model::from_structure_type_with_bolts(
            text,
            sections,
            &dir.map(|d| ohl_structures::bolts::BoltLibrary::beside(d).0)
                .unwrap_or_default(),
            None,
        ),
    }
    // A plugin's member check: found now, or the run refused.
    .and_then(|m| plugin_check::find(m, dir))
}

/// Solves `cases` of `table` on the finite-element model of `structure`
/// (`text`: its file's text, `dir`: its folder; members' sections looked
/// up in `sections` first): per case its result, or why it did not solve.
/// Err: the model does not read.
///
/// Expected cost: a few milliseconds per case for a lattice tower of a
/// few hundred members (one or two iterations first order, a few tens
/// with P-Delta). `budget` bounds each case (iterations, time, cancel).
pub fn analyse_cases(
    structure: &StructureType,
    text: &str,
    dir: Option<&Path>,
    sections: &SectionLibrary,
    table: &LoadTable,
    cases: &[CaseToRun],
    budget: &Budget,
) -> Result<Vec<CaseRun>, String> {
    let model = model_of(structure, text, dir, sections)?;
    let solved = in_case_order(cases.len(), threads(cases.len()), |k| {
        analyse_one(&model, table, &cases[k], &Default::default(), budget)
    });
    Ok(cases
        .iter()
        .map(|c| c.name.clone())
        .zip(plugin_checked(&model, cases, solved))
        .collect())
}

/// One case solved, with its solution kept when a plugin checks the
/// members (for the one call with every case, [`plugin_checked`]).
pub(crate) type SolvedCase = Result<(CaseResult, Option<ohl_structures_fe::Solution>), String>;

/// The cases' results with the members checked by the plugin the model's
/// file names: **one call** for all the solved cases
/// ([`ohl_member_check::check`]); each case's members take the plugin's
/// usage and what governs, its capacity note the plugin's. A plugin that
/// fails fails every case it was to check, with its message — no other
/// method in its place. A model without a plugin: the results as solved.
pub(crate) fn plugin_checked(
    model: &Model,
    cases: &[CaseToRun],
    solved: Vec<SolvedCase>,
) -> Vec<Result<CaseResult, String>> {
    let kept: Vec<(usize, ohl_member_check::Solved)> = solved
        .iter()
        .enumerate()
        .filter_map(|(k, r)| {
            let (_, solution) = r.as_ref().ok()?;
            let c = cases.get(k)?;
            Some((
                k,
                ohl_member_check::Solved {
                    name: &c.name,
                    strength: c.strength,
                    self_weight: if model.analysis.self_weight {
                        c.self_weight
                    } else {
                        0.0
                    },
                    solution: solution.as_ref()?,
                },
            ))
        })
        .collect();
    let checked = if kept.is_empty() {
        None
    } else {
        let list: Vec<ohl_member_check::Solved> = kept.iter().map(|(_, s)| *s).collect();
        ohl_member_check::check(model, &list)
    };
    let places: Vec<usize> = kept.iter().map(|(k, _)| *k).collect();
    drop(kept);
    let mut out: Vec<Result<CaseResult, String>> = Vec::with_capacity(solved.len());
    let mut checked = checked;
    for (k, r) in solved.into_iter().enumerate() {
        let n = places.iter().position(|p| *p == k);
        out.push(match (r, n, &mut checked) {
            (Ok((mut result, _)), Some(n), Some(Ok(c))) => {
                for u in c.usage.get(n).map_or(&[][..], |u| &u[..]) {
                    if let Some(m) = result.members.get_mut(u.member) {
                        m.usage = u.usage;
                        m.governs.clone_from(&u.governs);
                    }
                }
                result
                    .notes
                    .retain(|n| !n.starts_with("member capacities:"));
                result.notes.push(c.note.clone());
                Ok(result)
            }
            (Ok(_), Some(_), Some(Err(e))) => Err(e.clone()),
            (r, _, _) => r.map(|(result, _)| result),
        });
    }
    out
}

/// Threads for `n` cases: the cores, at most one per case.
pub(crate) fn threads(n: usize) -> usize {
    std::thread::available_parallelism()
        .map_or(1, |c| c.get())
        .min(n)
        .max(1)
}

/// `f` of 0..n on `threads` threads (each takes the next case when done),
/// the results in case order (2026-10-26: the cases are independent, each
/// solve the same whichever thread does it).
pub(crate) fn in_case_order<T: Send>(
    n: usize,
    threads: usize,
    f: impl Fn(usize) -> T + Sync,
) -> Vec<T> {
    if threads <= 1 || n <= 1 {
        return (0..n).map(f).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let done: std::sync::Mutex<Vec<(usize, T)>> = std::sync::Mutex::new(Vec::with_capacity(n));
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if k >= n {
                        return;
                    }
                    let r = f(k);
                    done.lock().expect("not poisoned").push((k, r));
                }
            });
        }
    });
    let mut done = done.into_inner().expect("not poisoned");
    done.sort_by_key(|(k, _)| *k);
    done.into_iter().map(|(_, r)| r).collect()
}

/// One case solved; `extra`: loads added to the table's (the surface
/// loads on an extension's piece).
fn analyse_one(
    model: &Model,
    table: &LoadTable,
    case: &CaseToRun,
    extra: &ohl_structures_fe::Loads,
    budget: &Budget,
) -> SolvedCase {
    let self_weight = if model.analysis.self_weight {
        case.self_weight
    } else {
        0.0
    };
    let mut loads = fe_loads(model, table, &case.name, self_weight)?;
    loads.nodal.extend_from_slice(&extra.nodal);
    loads.member.extend_from_slice(&extra.member);
    let mut notes = Vec::new();
    let left_out = table
        .case(&case.name)
        .filter(|r| r.missing.is_some())
        .count();
    if left_out > 0 {
        notes.push(format!(
            "{left_out} row(s) without a value left out (no line value, no area, …)"
        ));
    }
    // Loads at attachment points linked to no joint: left out, said.
    let unlinked: Vec<&str> = model
        .unlinked_points
        .iter()
        .filter(|l| {
            table
                .case(&case.name)
                .any(|r| r.missing.is_none() && r.at == At::Point && &r.label == *l)
        })
        .map(String::as_str)
        .collect();
    if !unlinked.is_empty() {
        notes.push(ohl_structures::unlinked_words(&unlinked));
    }
    // The moment of each attachment point's offset (a rigid link to its
    // joint), applied or left out (2026-10-21).
    let mut points: Vec<PointMoment> = Vec::new();
    for r in table
        .case(&case.name)
        .filter(|r| r.missing.is_none() && r.at == At::Point)
    {
        let Some(at) =
            model.point_load_with(&r.label, [r.force[0], r.force[1], -r.force[2]], r.moment)
        else {
            continue;
        };
        if at.moment != [0.0; 3] {
            match points.iter_mut().find(|p| p.label == r.label) {
                Some(p) => {
                    for (s, m) in p.moment.iter_mut().zip(at.moment) {
                        *s += m;
                    }
                    p.applied &= at.moment_applied;
                    if p.note.is_none() {
                        p.note.clone_from(&at.note);
                    }
                }
                None => points.push(PointMoment {
                    label: r.label.clone(),
                    moment: at.moment,
                    applied: at.moment_applied,
                    note: at.note.clone(),
                }),
            }
        }
        match at.note {
            Some(n) => notes.push(n),
            None if at.moment_applied => {
                let m = at.moment;
                notes.push(format!(
                    "attachment point {}: {} ({:.2}, {:.2}, {:.2}) kN m applied at its joint",
                    r.label,
                    if r.moment == [0.0; 3] {
                        "the moment of its offset"
                    } else {
                        "its moment (a post insulator's, with its offset's)"
                    },
                    m[0] / 1000.0,
                    m[1] / 1000.0,
                    m[2] / 1000.0
                ));
            }
            None => {}
        }
    }
    // Line forces taking a share of the spans' pull: a load assumption
    // (2026-11-10), said with the case.
    if table
        .case(&case.name)
        .any(|r| r.note.contains(ohl_structure_loads::SPAN_SHARE_ASSUMPTION))
    {
        notes.push(format!(
            "NOT ON THE SAFE SIDE: {}",
            ohl_structure_loads::SPAN_SHARE_ASSUMPTION
        ));
    }
    if table.case(&case.name).any(|r| r.note.contains("moment")) {
        notes.push("moments of [[load]] are not in the table: left out".into());
    }
    let mut load_sum = [0.0, 0.0, -model.weight() * self_weight];
    for l in &loads.nodal {
        for (s, f) in load_sum.iter_mut().zip(l.force) {
            *s += f;
        }
    }
    let started = Instant::now();
    let (solution, usage) = ohl_structures_fe::analyse(model, &loads, case.strength, budget)
        .map_err(|e| {
            format!(
                "did not solve: {}",
                ohl_structures_fe::refusal_text(model, &e)
            )
        })?;
    let took = started.elapsed();
    if let Some(note) = ohl_structures_fe::dead_load_note(model) {
        notes.push(note);
    }
    if let Some(note) = ohl_structures_fe::stabilisation_note(model, &solution) {
        notes.push(note);
    }
    if let Some(note) = ohl_structures_fe::tension_only_note(model, &solution) {
        notes.push(note);
    }
    // Tension-only members kept active (their set did not settle): the
    // results are approximate; their members' `governs` say so too.
    if let Some(note) = ohl_structures_fe::kept_active_note(model, &solution) {
        notes.push(note);
    }
    if let Some(note) = ohl_structures_fe::convergence_note(&solution) {
        notes.push(note);
    }
    // Joints moved far relative to their own members (a local
    // near-mechanism; owner 2026-11-15): a warning.
    if let Some(note) = ohl_structures_fe::local_displacement_note(model, &solution) {
        notes.push(note);
    }
    // The climbing check (optional, alone: not a load case; owner
    // 2026-11-15): its own list and a note.
    let climbing_check = ohl_structures_fe::check::climbing_check(model);
    if let Some(note) = ohl_structures_fe::check::climbing_note(model, &climbing_check) {
        notes.push(note);
    }
    let climbing = climbing_check
        .into_iter()
        .map(|c| ClimbingResult {
            member: model.members[c.member].id.clone(),
            usage: c.usage,
            words: c.words,
        })
        .collect();
    // Eccentric member ends (connection shifts, rigid end offsets): how
    // many, and those left out where only bars meet the joint.
    if let Some(note) = ohl_structures_fe::eccentricity_note(model, &solution) {
        notes.push(note);
    }
    // With a plugin's member check the note is the plugin's, put in its
    // place once every case is solved ([`plugin_checked`]).
    notes.push(ohl_structures_fe::check::capacity_note(model, &usage));
    let joints = model
        .nodes
        .iter()
        .zip(&solution.displacements)
        .map(|(n, d)| JointResult {
            id: n.id.clone(),
            point: n.point,
            displacement: [d[0], d[1], d[2]],
            support: n.is_support(),
        })
        .collect();
    // Each member's usage (the first given for it), by member: a search
    // per member was O(members²) (2026-10-26).
    let mut usage_of: Vec<Option<&_>> = vec![None; model.members.len()];
    for u in &usage {
        if let Some(slot) = usage_of.get_mut(u.member)
            && slot.is_none()
        {
            *slot = Some(u);
        }
    }
    let members = model
        .members
        .iter()
        .enumerate()
        .map(|(k, m)| {
            let u = usage_of[k];
            MemberResult {
                id: m.id.clone(),
                ends: [m.i, m.j],
                axial: solution.members[k].axial,
                usage: u.and_then(|u| u.usage),
                governs: u.map(|u| u.governs.clone()).unwrap_or_default(),
                slack: solution.members[k].slack,
                bowed: solution.members[k].bowed,
                eccentric: ohl_structures_fe::offsets::eccentricity_words(
                    model,
                    k,
                    solution.members[k].axial,
                ),
            }
        })
        .collect();
    let mut reaction_sum = [0.0; 3];
    let reactions = solution
        .reactions
        .iter()
        .map(|r| {
            for (s, f) in reaction_sum.iter_mut().zip(r.force) {
                *s += f;
            }
            ReactionResult {
                joint: model.nodes[r.node].id.clone(),
                force: r.force,
                moment: r.moment,
            }
        })
        .collect();
    // Kept for the plugin's one call with every case.
    let keep = model
        .capacity_plugin
        .as_ref()
        .is_some_and(|p| p.dir.is_some());
    let result = CaseResult {
        case: case.name.clone(),
        joints,
        members,
        reactions,
        load_sum,
        reaction_sum,
        iterations: solution.iterations,
        residual: solution.residual,
        nonlinear: solution.nonlinear,
        took,
        notes,
        points,
        plates: Vec::new(),
        climbing,
    };
    Ok((result, keep.then_some(solution)))
}

#[cfg(test)]
mod order_tests {
    /// The cases solved side by side come back in their order, each the
    /// result of its own index, whatever the threads.
    #[test]
    fn cases_come_back_in_their_order() {
        let f = |k: usize| (k, (k as f64).sqrt().to_bits());
        let one: Vec<_> = super::in_case_order(97, 1, f);
        for threads in [2, 4, 16] {
            assert_eq!(super::in_case_order(97, threads, f), one);
        }
        assert!(super::in_case_order(0, 4, f).is_empty());
    }
}
