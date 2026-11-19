// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The plugin interface: the records of `member-check-full`, `joint-check`
//! and `bolt-distances` read into the check's types and its results back
//! (WebAssembly only; the check itself is plain Rust, tested natively).

use std::collections::HashMap;

use crate::bracing::{Bracing, Piece, Point, Restraint};
use crate::lengths::{self, Lengths};
use crate::{bends, check, groups, joints, memo, params, shared, sia263};

wit_bindgen::generate!({
    path: "../../../crates/ohl-plugins/wit",
    world: "ohl:plugin/structure-code-plugin",
    generate_all,
});

use crate::model::{
    Analysis, Bolts, Condition, ConnectedLeg, EndBolts, EndConditions, EndDesign, Grade, Member,
    MemberClass, MemberDesign, MemberForces, MemberKind, Onto, OntoLeg, Section, Source, Supported,
};
use exports::ohl::plugin::member_check_bracing as mb;
use exports::ohl::plugin::member_check_full as full;

/// The method's description.
const DESCRIPTION: &str = "SIA 263:2013 (Construction en acier): design values with γ_M1, γ_M2 \
                           (4.1.3); buckling curves, section classes, angles bolted by one leg \
                           with their eccentricity, tension by the net section and the \
                           connection, bolts by Table 16 — bundled with OpenOHL";

struct Sia263;

impl Guest for Sia263 {
    fn describe() -> PluginInfo {
        PluginInfo {
            name: "sia263".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "SIA 263:2013 for structures: members, joints, bolt distances".into(),
        }
    }
}

/// Err unless `method` is this plugin's.
fn ours(method: &str) -> Result<(), String> {
    if method == sia263::NAME {
        Ok(())
    } else {
        Err(format!(
            "no method \"{method}\" (this plugin gives {})",
            sia263::NAME
        ))
    }
}

impl exports::ohl::plugin::member_check::Guest for Sia263 {
    fn methods() -> Vec<exports::ohl::plugin::member_check::MethodInfo> {
        vec![exports::ohl::plugin::member_check::MethodInfo {
            name: sia263::NAME.into(),
            description: DESCRIPTION.into(),
        }]
    }

    fn check_members(
        method: String,
        _input: exports::ohl::plugin::member_check::CheckInput,
    ) -> Result<Vec<exports::ohl::plugin::member_check::MemberResult>, String> {
        ours(&method)?;
        Err(
            "the SIA 263 check needs the structure as `member-check-full` sends it (the \
             sections' and forces' details); this OpenOHL does not send it — no result, never \
             another check in its place"
                .into(),
        )
    }
}

/// A profile's shape from its name and dimensions (none: not a library
/// profile, or a shape not known).
fn shape(name: &str, dims: &[(String, f64)]) -> Option<ohl_sections::Shape> {
    use ohl_sections::Shape as S;
    let d = |k: &str| dims.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
    Some(match name {
        "equal_angle" => S::EqualAngle {
            h: d("h")?,
            t: d("t")?,
            r: d("r")?,
        },
        "unequal_angle" => S::UnequalAngle {
            h: d("h")?,
            b: d("b")?,
            t: d("t")?,
            r: d("r")?,
        },
        "channel" => S::Channel {
            h: d("h")?,
            b: d("b")?,
            tw: d("tw")?,
            tf: d("tf")?,
            r: d("r")?,
        },
        "i_section" => S::ISection {
            h: d("h")?,
            b: d("b")?,
            tw: d("tw")?,
            tf: d("tf")?,
            r: d("r")?,
        },
        "rectangular_hollow" => S::RectangularHollow {
            h: d("h")?,
            b: d("b")?,
            t: d("t")?,
        },
        "circular_hollow" => S::CircularHollow {
            d: d("d")?,
            t: d("t")?,
        },
        "round_bar" => S::RoundBar { d: d("d")? },
        "polygonal_hollow" => S::PolygonalHollow {
            sides: d("sides")? as u32,
            d: d("d")?,
            t: d("t")?,
        },
        "general" => S::General {
            h: d("h")?,
            b: d("b")?,
        },
        "double_angle" => S::DoubleAngle {
            h: d("h")?,
            b: d("b")?,
            t: d("t")?,
            r: d("r")?,
            gap: d("gap")?,
        },
        "star_angle" => S::StarAngle {
            h: d("h")?,
            b: d("b")?,
            t: d("t")?,
            r: d("r")?,
            gap: d("gap")?,
        },
        "cross_angle" => S::CrossAngle {
            h: d("h")?,
            t: d("t")?,
            r: d("r")?,
            gap: d("gap")?,
        },
        "double_channel" => S::DoubleChannel {
            h: d("h")?,
            b: d("b")?,
            tw: d("tw")?,
            tf: d("tf")?,
            r: d("r")?,
            gap: d("gap")?,
        },
        _ => return None,
    })
}

/// The library's given values by name.
fn given(values: &[(String, f64)]) -> ohl_sections::Given {
    let mut g = ohl_sections::Given::default();
    for (k, v) in values {
        let v = Some(*v);
        match k.as_str() {
            "area" => g.area = v,
            "iy" => g.iy = v,
            "iz" => g.iz = v,
            "iu" => g.iu = v,
            "iv" => g.iv = v,
            "it" => g.it = v,
            "wel_y" => g.wel_y = v,
            "wel_z" => g.wel_z = v,
            "wpl_y" => g.wpl_y = v,
            "wpl_z" => g.wpl_z = v,
            "gyration_y" => g.gyration_y = v,
            "gyration_z" => g.gyration_z = v,
            "gyration_u" => g.gyration_u = v,
            "gyration_v" => g.gyration_v = v,
            "shear_area_y" => g.shear_area_y = v,
            "shear_area_z" => g.shear_area_z = v,
            _ => {}
        }
    }
    g
}

/// The properties as OpenOHL sent them (the values the check does not
/// read left at zero).
fn properties(p: &full::ProfileProperties) -> ohl_sections::Properties {
    ohl_sections::Properties {
        area: p.area,
        centroid: [p.centroid.0, p.centroid.1],
        width: 0.0,
        depth: 0.0,
        iy: p.iy,
        iz: p.iz,
        iyz: p.iyz,
        iu: p.iu,
        iv: p.iv,
        principal_angle: p.principal_angle,
        it: p.it,
        shear_centre: p.shear_centre.map(|c| [c.0, c.1]),
        warping: p.warping,
        wel_y: p.wel_y,
        wel_z: p.wel_z,
        wel_u: p.wel_u,
        wel_v: p.wel_v,
        wpl_y: p.wpl_y,
        wpl_z: p.wpl_z,
        gyration_y: 0.0,
        gyration_z: 0.0,
        gyration_u: 0.0,
        gyration_v: 0.0,
        shear_area_y: None,
        shear_area_z: None,
    }
}

fn section(s: &full::Section) -> Section {
    let profile = shape(&s.shape, &s.dimensions).map(|shape| ohl_sections::Section {
        id: s.id.clone(),
        name: String::new(),
        description: String::new(),
        shape,
        material: None,
        given: given(&s.given),
    });
    Section {
        area: s.area,
        iy: s.iy,
        iz: s.iz,
        j: s.j,
        i_min: s.iv,
        wy: s.wy,
        wz: s.wz,
        e: s.e,
        g: s.g,
        yield_strength: s.fy,
        ultimate: s.fu,
        tension: s.tension,
        compression: s.compression,
        moment_y: s.moment_y,
        moment_z: s.moment_z,
        properties: profile.as_ref().and(s.properties.as_ref().map(properties)),
        profile,
    }
}

fn condition(c: &full::Condition) -> Condition {
    Condition {
        value: c.value,
        source: Source::parse(&c.source),
        why: (!c.why.is_empty()).then(|| c.why.clone()),
    }
}

fn end_design(c: &full::EndConnection) -> EndDesign {
    EndDesign {
        kind: (!c.kind.is_empty()).then(|| c.kind.clone()),
        onto: match c.onto.as_str() {
            "member" => Some(Onto::Member),
            "plate" => Some(Onto::Plate),
            "splice" => Some(Onto::Splice),
            "other" => Some(Onto::Other),
            _ => None,
        },
        leg: ConnectedLeg::parse(&c.leg),
        bolts: c.bolts.as_ref().map(|b| EndBolts {
            count: b.count,
            diameter: b.diameter,
            hole: b.hole,
            shear_planes: b.shear_planes,
            shear_capacity: b.shear_capacity,
            shear_capacity_shank: b.shear_capacity_shank,
            ultimate: b.ultimate,
            threads_in_shear: b.threads_in_shear,
            grade: Grade::parse(&b.grade),
        }),
        holes: c.holes,
        gauges: c.gauges.clone(),
        edge_distance: c.edge_distance,
        end_distance: c.end_distance,
        pitch: c.pitch,
        planes: c.planes.map(|p| (p, c.planes_why.clone())),
        continuous: c.continuous,
    }
}

/// A member of the interface as the check reads it.
pub fn member(m: &full::Member) -> Member {
    let end = |k: usize| m.ends.get(k);
    let cond = |k: usize, f: fn(&full::MemberEnd) -> &full::Condition| {
        end(k).map_or(
            Condition {
                value: None,
                source: Source::Assumed,
                why: None,
            },
            |e| condition(f(e)),
        )
    };
    let ratios = <[f64; 3]>::try_from(m.length_ratios.as_slice()).ok();
    Member {
        id: m.id.clone(),
        kind: match m.kind.as_str() {
            "frame" => MemberKind::Frame,
            "cable" => MemberKind::Cable,
            "tension" => MemberKind::Tension,
            _ => MemberKind::Truss,
        },
        frame: m.frame,
        design: MemberDesign {
            class: MemberClass::parse(&m.class),
            length_ratios: ratios,
            thickness: m.thickness,
            plies: m.plies,
            connected_leg: ConnectedLeg::parse(&m.connected_leg),
            bolts: m.bolts.as_ref().map(|b| Bolts {
                diameter: b.diameter,
                hole: b.hole,
                count: b.count,
                shear_planes: b.shear_planes,
                shear_capacity: b.shear_capacity,
                ultimate: b.ultimate,
                holes: b.holes,
                grade: Grade::parse(&b.grade),
            }),
            edge_distance: m.edge_distance,
            end_distance: m.end_distance,
            spacing: m.spacing,
            ends: [0, 1].map(|k| end(k).and_then(|e| e.connection.as_ref()).map(end_design)),
        },
        section: section(&m.section),
        length: m.length,
        k: m.k,
        lk: m.lk,
        capacity_tension: m.capacity_tension,
        capacity_compression: m.capacity_compression,
        joints: [0, 1].map(|k| end(k).map_or(String::new(), |e| e.joint.clone())),
        conditions: EndConditions {
            eccentric: [cond(0, |e| &e.eccentric), cond(1, |e| &e.eccentric)],
            connected: [0, 1].map(|k| end(k).is_some_and(|e| e.connection.is_some())),
        },
        onto: [0, 1].map(|k| {
            end(k)
                .and_then(|e| e.connection.as_ref())
                .and_then(|c| c.onto_leg.as_ref())
                .map(|o| OntoLeg {
                    id: o.member.clone(),
                    thickness: o.thickness,
                    fu: o.fu,
                })
        }),
    }
}

/// A member's forces in one case.
fn forces(f: &full::CaseForces) -> MemberForces {
    let three = |v: &[f64]| <[f64; 3]>::try_from(v).unwrap_or([0.0; 3]);
    MemberForces {
        axial: f.axial,
        end_forces: <[f64; 12]>::try_from(f.end_forces.as_slice()).unwrap_or([0.0; 12]),
        slack: f.slack,
        bowed: f.bowed,
        length: f.length,
        span_load: three(&f.span_load),
        lumped_load: three(&f.lumped_load),
    }
}

/// The member's capacities by name (those that are positive numbers).
fn capacities(c: &shared::Capacity) -> Vec<full::Capacity> {
    let slenderness = c
        .kl_r
        .map(|l| (l, "λ_K, the largest of the member's axes".to_owned()));
    [
        ("tension", &c.tension),
        ("compression", &c.compression),
        ("slenderness", &slenderness),
    ]
    .into_iter()
    .filter_map(|(name, v)| {
        let (value, governs) = v.as_ref()?;
        (value.is_finite() && *value >= 0.0).then(|| full::Capacity {
            name: name.into(),
            value: *value,
            governs: governs.clone(),
        })
    })
    .collect()
}

/// What braces a member, as the check reads it.
fn bracing_of(b: &mb::MemberBracing) -> Bracing {
    Bracing {
        run: b
            .run
            .iter()
            .map(|p| Piece {
                member: p.member.clone(),
                start: p.start,
                end: p.end,
                forward: p.forward,
            })
            .collect(),
        length: b.length,
        points: b
            .points
            .iter()
            .map(|p| Point {
                at: p.at,
                joint: p.joint.clone(),
                support: p.support,
                restraints: p
                    .restraints
                    .iter()
                    .map(|r| Restraint {
                        member: r.member.clone(),
                        direction: [r.direction.0, r.direction.1],
                        through: r.through,
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// An angle (single, or crossed angles): its u and v (5.4.1.6).
fn angle(m: &Member) -> bool {
    matches!(
        m.section.profile.as_ref().map(|p| &p.shape),
        Some(
            ohl_sections::Shape::EqualAngle { .. }
                | ohl_sections::Shape::UnequalAngle { .. }
                | ohl_sections::Shape::CrossAngle { .. }
        )
    )
}

/// The check of every member of `input` (the parameters already set),
/// with what braces each (`bracing`, `member-check-bracing`; none: the
/// members' own lengths or assumed, as before).
pub fn check_all(
    input: &full::FullInput,
    bracing: Option<&[mb::MemberBracing]>,
) -> Vec<full::MemberResult> {
    let analysis = Analysis {
        single_angle_moments: input.single_angle_moments,
        no_compression: input.tension_compression == "none",
        redundant_force: input.redundant_share,
    };
    let members: Vec<Member> = input.members.iter().map(member).collect();
    let braced: Option<HashMap<String, Bracing>> = bracing.map(|b| {
        b.iter()
            .map(|x| (x.member.clone(), bracing_of(x)))
            .collect()
    });
    let lengths = braced.as_ref().map(|b| Lengths::new(&members, b));
    // Each case's axial forces by member (the compression a support holds).
    let axial: Vec<HashMap<String, f64>> = match &lengths {
        Some(_) => (0..input.cases.len())
            .map(|n| {
                input
                    .members
                    .iter()
                    .filter_map(|w| {
                        w.forces
                            .iter()
                            .find(|f| f.load_case as usize == n)
                            .map(|f| (w.id.clone(), f.axial))
                    })
                    .collect()
            })
            .collect(),
        None => Vec::new(),
    };
    input
        .members
        .iter()
        .zip(&members)
        .map(|(w, m)| {
            let m = m.clone();
            // Its capacities as the solve's cap took them (its crossing
            // partner not taken as bracing it).
            let partner = (!w.crossing.is_empty()).then_some(false);
            let plain = lengths.as_ref().and_then(|l| l.worked(&m, None, 1.0));
            let (c, _) = sia263::capacity_worked(&m, partner, plain.as_ref());
            let mut notes = Vec::new();
            let all = lengths.as_ref().and_then(|l| l.all(&m, None, 1.0));
            match (&lengths, &plain, &all) {
                (_, Some(x), _) => notes.push(format!(
                    "buckling lengths worked out from the bracing: {}",
                    x.words(angle(&m))
                )),
                (_, None, Some(x)) => notes.push(format!(
                    "buckling lengths worked out from the bracing: its own length between its \
                     joints about every axis, as before ({})",
                    x.words(angle(&m))
                )),
                (Some(_), None, None) if lengths::given(&m) => {
                    notes.push("buckling lengths given (not worked out from the bracing)".into())
                }
                (Some(_), None, None) => notes
                    .push("buckling lengths assumed: no bracing found (buckling lengths L)".into()),
                (None, _, _) => {}
            }
            notes.extend(c.compression_assumed.iter().cloned());
            for a in &c.tension_assumed {
                if !notes.contains(a) {
                    notes.push(a.clone());
                }
            }
            notes.extend(c.joints.iter().map(|j| format!("joints: {j}")));
            notes.extend(c.limit.iter().map(|l| format!("limit: {l}")));
            let cases: Vec<full::CaseResult> = w
                .forces
                .iter()
                .filter_map(|f| {
                    let strength = input.cases.get(f.load_case as usize)?.strength;
                    let supported = match (&f.braced, f.braced_why.is_empty()) {
                        (Some(b), _) => Some(Ok(Supported {
                            member: b.member.clone(),
                            joint: b.joint.clone(),
                            force: b.force,
                            via_redundant: b.via_redundant,
                        })),
                        (None, false) => Some(Err(f.braced_why.clone())),
                        (None, true) => None,
                    };
                    let worked = lengths
                        .as_ref()
                        .and_then(|l| l.worked(&m, axial.get(f.load_case as usize), strength));
                    let u = check::member_usage(
                        &analysis,
                        &m,
                        &forces(f),
                        f.partner_axial,
                        f.kept_active,
                        strength,
                        supported,
                        worked.as_ref(),
                    );
                    Some(full::CaseResult {
                        load_case: f.load_case,
                        usage: u.usage,
                        check: u.governs,
                    })
                })
                .collect();
            let worst = cases
                .iter()
                .filter(|c| c.usage.is_some())
                .max_by(|a, b| check::usage_order(a.usage.unwrap_or(0.0), b.usage.unwrap_or(0.0)))
                .map(|c| c.check.clone());
            let governs = worst.unwrap_or_else(|| {
                c.compression
                    .as_ref()
                    .map(|x| format!("compression {}", x.1))
                    .unwrap_or_default()
            });
            full::MemberResult {
                id: w.id.clone(),
                capacities: capacities(&c),
                cases,
                governs,
                notes,
                not_safe: None,
            }
        })
        .collect()
}

impl full::Guest for Sia263 {
    fn parameters_template(method: String) -> Option<String> {
        (method == sia263::NAME).then(|| params::TEMPLATE.to_owned())
    }

    fn check_members_full(
        method: String,
        input: full::FullInput,
    ) -> Result<full::FullOutput, String> {
        ours(&method)?;
        let p = params::parse(&input.parameters)?;
        params::set(p);
        memo::start();
        let results = check_all(&input, None);
        memo::stop();
        Ok(full::FullOutput {
            method_note: format!(
                "{} (Construction en acier; {})",
                sia263::TITLE,
                params::words(&p)
            ),
            results,
        })
    }
}

impl mb::Guest for Sia263 {
    fn check_members_bracing(
        method: String,
        input: full::FullInput,
        bracing: Vec<mb::MemberBracing>,
    ) -> Result<full::FullOutput, String> {
        ours(&method)?;
        let p = params::parse(&input.parameters)?;
        params::set(p);
        memo::start();
        let results = check_all(&input, Some(&bracing));
        memo::stop();
        Ok(full::FullOutput {
            method_note: format!(
                "{} (Construction en acier; {})",
                sia263::TITLE,
                params::words(&p)
            ),
            results,
        })
    }
}

use exports::ohl::plugin::bolt_distances::{DistanceInput, Distances};
use exports::ohl::plugin::joint_check::{CaseUsage as PartUsage, JointInput, Part, PartResult};

/// One joint part's checks under force `n` with what else acts on it
/// (`x`) and its plate's free length in compression (`free`).
fn part_checked(p: &Part, n: f64, x: &joints::Extra, free: Option<f64>) -> Option<joints::Checked> {
    match p.kind.as_str() {
        "bolts" => {
            let (b, s) = (p.bolts.as_ref()?, p.steel.as_ref()?);
            let g = joints::BoltGroup {
                count: b.count,
                diameter: b.diameter,
                hole: b.hole,
                lines: b.lines,
                per_line: b.per_line,
                pitch: b.pitch,
                span: b.span,
                end: [b.end_tension, b.end_compression],
                edge: b.edge,
                width: b.width,
                width_words: b.width_note.clone(),
            };
            let steel = joints::PlateSteel {
                thickness: s.thickness,
                fy: s.fy,
                fu: s.fu,
            };
            Some(joints::bolted_with(&g, &steel, n, x, free))
        }
        "weld" => {
            let w = p.weld.as_ref()?;
            let input = joints::WeldInput {
                kind: if w.kind == "full_penetration" {
                    joints::WeldKind::FullPenetration
                } else {
                    joints::WeldKind::Fillet
                },
                size: w.throat,
                length: w.length,
                length_given: w.length_given,
                angle: w.angle,
                fy: w.fy,
                thickness: [
                    w.thicknesses.first().copied().unwrap_or(0.0),
                    w.thicknesses.get(1).copied().unwrap_or(0.0),
                ],
            };
            Some(joints::weld_with(&input, n, x))
        }
        _ => None,
    }
}

/// Every part of `input` checked (the parameters already set): one
/// result per part it checks.
fn joint_results(input: &JointInput) -> Vec<PartResult> {
    joint_results_with(input, &[])
}

/// What acts on one part beside its force, per load case, and its
/// plate's free length in compression (`joint-check-moments`).
type PartExtra = (Vec<joints::Extra>, Option<f64>);

/// [`joint_results`] with what acts on each part beside its force
/// (`extra`, by the part's place in `input`; none: nothing).
fn joint_results_with(input: &JointInput, extra: &[PartExtra]) -> Vec<PartResult> {
    input
        .parts
        .iter()
        .enumerate()
        .filter_map(|(k, p)| {
            let mut notes: Vec<String> = Vec::new();
            let mut not_safe: Vec<String> = Vec::new();
            let mut usages = Vec::new();
            let mut governs = String::new();
            let mut worst: Option<f64> = None;
            let none = (Vec::new(), None);
            let (xs, free) = extra.get(k).unwrap_or(&none);
            for (n, case) in input.cases.iter().enumerate() {
                let force = p.forces.get(n).copied().unwrap_or(0.0);
                let x = xs.get(n).copied().unwrap_or_default();
                let checked = part_checked(p, force, &x, *free)?;
                for note in &checked.notes {
                    if !notes.contains(note) {
                        notes.push(note.clone());
                    }
                }
                for w in &checked.not_safe {
                    if !not_safe.contains(w) {
                        not_safe.push(w.clone());
                    }
                }
                if let Some((u, c)) = checked.governing(case.strength) {
                    if worst.is_none_or(|w| check::usage_order(u, w).is_gt()) {
                        worst = Some(u);
                        governs = c.what.clone();
                    }
                    usages.push(PartUsage {
                        load_case: n as u32,
                        usage: u,
                        check: c.what.clone(),
                    });
                }
            }
            Some(PartResult {
                id: p.id,
                usages,
                governs,
                notes,
                not_safe: (!not_safe.is_empty()).then(|| not_safe.join("; ")),
            })
        })
        .collect()
}

impl exports::ohl::plugin::joint_check::Guest for Sia263 {
    fn check_joints(method: String, input: JointInput) -> Result<Vec<PartResult>, String> {
        ours(&method)?;
        // The structure's parameter file comes with `joint-check-full`
        // (OpenOHL calls that one): here, an OpenOHL without it, the
        // standard's values.
        params::set(params::Params::default());
        Ok(joint_results(&input))
    }
}

use exports::ohl::plugin::joint_check_full as jfull;

/// A `joint-check-full` input as `joint-check`'s (the same records).
fn plain_input(i: &jfull::JointInput) -> JointInput {
    use exports::ohl::plugin::joint_check as j;
    JointInput {
        structure: i.structure.clone(),
        cases: i
            .cases
            .iter()
            .map(|c| j::LoadCase {
                name: c.name.clone(),
                strength: c.strength,
            })
            .collect(),
        parts: i
            .parts
            .iter()
            .map(|p| Part {
                id: p.id,
                joint: p.joint.clone(),
                plate: p.plate.clone(),
                kind: p.kind.clone(),
                other: p.other.clone(),
                bolts: p.bolts.as_ref().map(|b| j::BoltGroup {
                    count: b.count,
                    diameter: b.diameter,
                    hole: b.hole,
                    bolt_ultimate: b.bolt_ultimate,
                    lines: b.lines,
                    per_line: b.per_line,
                    pitch: b.pitch,
                    span: b.span,
                    end_tension: b.end_tension,
                    end_compression: b.end_compression,
                    edge: b.edge,
                    width: b.width,
                    width_note: b.width_note.clone(),
                }),
                steel: p.steel.map(|s| j::PlateSteel {
                    thickness: s.thickness,
                    fy: s.fy,
                    fu: s.fu,
                }),
                weld: p.weld.as_ref().map(|w| j::Weld {
                    kind: w.kind.clone(),
                    throat: w.throat,
                    length: w.length,
                    length_given: w.length_given,
                    angle: w.angle,
                    fy: w.fy,
                    thicknesses: w.thicknesses.clone(),
                }),
                forces: p.forces.clone(),
                notes: p.notes.clone(),
            })
            .collect(),
    }
}

impl jfull::Guest for Sia263 {
    fn check_joints_full(
        method: String,
        input: jfull::JointInput,
    ) -> Result<Vec<jfull::PartResult>, String> {
        ours(&method)?;
        params::set(params::parse(&input.parameters)?);
        Ok(joint_results(&plain_input(&input))
            .into_iter()
            .map(|r| jfull::PartResult {
                id: r.id,
                usages: r
                    .usages
                    .into_iter()
                    .map(|u| jfull::CaseUsage {
                        load_case: u.load_case,
                        usage: u.usage,
                        check: u.check,
                    })
                    .collect(),
                governs: r.governs,
                notes: r.notes,
                not_safe: r.not_safe,
            })
            .collect())
    }
}

use exports::ohl::plugin::joint_check_moments as jmom;

/// A `joint-check-moments` input as `joint-check`'s (the same records)
/// and, per part, what else acts on it and its plate's free length.
fn moments_input(i: &jmom::JointInput) -> (JointInput, Vec<PartExtra>) {
    use exports::ohl::plugin::joint_check as j;
    let input = JointInput {
        structure: i.structure.clone(),
        cases: i
            .cases
            .iter()
            .map(|c| j::LoadCase {
                name: c.name.clone(),
                strength: c.strength,
            })
            .collect(),
        parts: i
            .parts
            .iter()
            .map(|p| Part {
                id: p.id,
                joint: p.joint.clone(),
                plate: p.plate.clone(),
                kind: p.kind.clone(),
                other: p.other.clone(),
                bolts: p.bolts.as_ref().map(|b| j::BoltGroup {
                    count: b.count,
                    diameter: b.diameter,
                    hole: b.hole,
                    bolt_ultimate: b.bolt_ultimate,
                    lines: b.lines,
                    per_line: b.per_line,
                    pitch: b.pitch,
                    span: b.span,
                    end_tension: b.end_tension,
                    end_compression: b.end_compression,
                    edge: b.edge,
                    width: b.width,
                    width_note: b.width_note.clone(),
                }),
                steel: p.steel.map(|s| j::PlateSteel {
                    thickness: s.thickness,
                    fy: s.fy,
                    fu: s.fu,
                }),
                weld: p.weld.as_ref().map(|w| j::Weld {
                    kind: w.kind.clone(),
                    throat: w.throat,
                    length: w.length,
                    length_given: w.length_given,
                    angle: w.angle,
                    fy: w.fy,
                    thicknesses: w.thicknesses.clone(),
                }),
                forces: p.forces.clone(),
                notes: p.notes.clone(),
            })
            .collect(),
    };
    let extra = i
        .parts
        .iter()
        .map(|p| {
            (
                p.moments
                    .iter()
                    .map(|m| joints::Extra {
                        shear: m.shear.abs(),
                        in_plane: m.in_plane.abs(),
                        out_of_plane: m.out_of_plane.abs(),
                        normal: m.normal.abs(),
                    })
                    .collect(),
                p.free_length,
            )
        })
        .collect();
    (input, extra)
}

impl jmom::Guest for Sia263 {
    fn check_joints_moments(
        method: String,
        input: jmom::JointInput,
    ) -> Result<Vec<jmom::PartResult>, String> {
        ours(&method)?;
        params::set(params::parse(&input.parameters)?);
        let (plain, extra) = moments_input(&input);
        Ok(joint_results_with(&plain, &extra)
            .into_iter()
            .map(|r| jmom::PartResult {
                id: r.id,
                usages: r
                    .usages
                    .into_iter()
                    .map(|u| jmom::CaseUsage {
                        load_case: u.load_case,
                        usage: u.usage,
                        check: u.check,
                    })
                    .collect(),
                governs: r.governs,
                notes: r.notes,
                not_safe: r.not_safe,
            })
            .collect())
    }
}

impl exports::ohl::plugin::bolt_distances::Guest for Sia263 {
    fn minimum_distances(
        method: String,
        input: DistanceInput,
    ) -> Result<Option<Distances>, String> {
        ours(&method)?;
        // SIA 263 Table 17: minimums by bolt size, the same for any force
        // and edge (none for a size the table does not list).
        Ok(
            sia263::bolt_minimums(input.diameter).map(|[end, spacing, edge]| Distances {
                end,
                spacing,
                edge,
                hole: sia263::standard_hole(input.diameter),
                clauses: ["Table 17, e_1", "Table 17, p_1", "Table 17, e_2"]
                    .map(str::to_owned)
                    .to_vec(),
                notes: Vec::new(),
            }),
        )
    }
}

impl exports::ohl::plugin::bolt_distances_full::Guest for Sia263 {
    fn minimum_distances_full(
        method: String,
        input: exports::ohl::plugin::bolt_distances_full::DistanceInput,
    ) -> Result<Option<exports::ohl::plugin::bolt_distances_full::Distances>, String> {
        ours(&method)?;
        // γ_M2 from the structure's parameter file, as the joints take it.
        params::set(params::parse(&input.parameters)?);
        // The analysis' force per bolt, else the bolt's own design shear
        // resistance by Table 16 (third round, 2026-11-18; before, ASCE
        // 10-97's from the core). Table 17's minimums take no force.
        let own = input.bolts.as_ref().and_then(|b| {
            sia263::bolt_shear(
                input.diameter,
                b.shear_planes,
                b.shear_capacity,
                b.shear_capacity_shank,
                b.threads_in_shear,
                b.ultimate,
                Grade::parse(&b.grade),
            )
        });
        let bolt_force = input.analysis_force.or(own.as_ref().map(|o| o.0));
        Ok(
            sia263::bolt_minimums(input.diameter).map(|[end, spacing, edge]| {
                exports::ohl::plugin::bolt_distances_full::Distances {
                    end,
                    spacing,
                    edge,
                    hole: sia263::standard_hole(input.diameter),
                    clauses: ["Table 17, e_1", "Table 17, p_1", "Table 17, e_2"]
                        .map(str::to_owned)
                        .to_vec(),
                    notes: vec!["Table 17's minimums do not depend on the force".to_owned()],
                    bolt_force,
                    force_basis: match (&input.analysis_force, own) {
                        (None, Some((_, words))) => words,
                        _ => String::new(),
                    },
                }
            }),
        )
    }
}

use exports::ohl::plugin::joint_check_groups as jgr;

/// A `joint-check-groups` group as [`groups::Group`] takes it.
fn group_of(g: &jgr::Group) -> groups::Group {
    groups::Group {
        plies: g
            .plies
            .iter()
            .map(|p| groups::Ply {
                part: p.part.clone(),
                member: p.kind == "member",
                thickness: p.thickness,
                fy: p.fy,
                fu: p.fu,
                redundant: p.redundant,
                known: p.known,
            })
            .collect(),
        bolts: g
            .bolts
            .iter()
            .map(|b| groups::Bolt {
                diameter: b.diameter,
                hole: b.hole,
                ultimate: b.bolt_ultimate,
                shear_capacity: b.shear_capacity,
                shank: b.shear_capacity_shank,
                threads_in: b.threads_in_shear,
                grade: b.grade.clone(),
                planes: b.planes,
                carries: b.carries.clone(),
                at: [b.at.0, b.at.1],
            })
            .collect(),
        loads: g
            .loads
            .iter()
            .map(|c| {
                c.iter()
                    .map(|l| groups::Load {
                        force: l.force,
                        ends: l.ends.clone(),
                        sides: l.sides.clone(),
                        ahead: l.ahead.clone(),
                    })
                    .collect()
            })
            .collect(),
    }
}

impl jgr::Guest for Sia263 {
    fn check_joint_groups(
        method: String,
        input: jgr::GroupInput,
    ) -> Result<Vec<jgr::GroupResult>, String> {
        ours(&method)?;
        params::set(params::parse(&input.parameters)?);
        let strengths: Vec<f64> = input.cases.iter().map(|c| c.strength).collect();
        Ok(input
            .groups
            .iter()
            .map(|g| {
                let r = groups::check_group(&group_of(g), &strengths, None);
                jgr::GroupResult {
                    id: g.id,
                    usages: r
                        .usages
                        .into_iter()
                        .enumerate()
                        .filter_map(|(n, u)| {
                            u.map(|(usage, check)| jgr::CaseUsage {
                                load_case: n as u32,
                                usage,
                                check,
                            })
                        })
                        .collect(),
                    governs: r.governs,
                    notes: r.notes,
                    not_safe: (!r.not_safe.is_empty()).then(|| r.not_safe.join("; ")),
                }
            })
            .collect())
    }
}

impl jgt::Guest for Sia263 {
    fn check_joint_groups_tension(
        method: String,
        input: jgr::GroupInput,
        tension: Vec<jgt::GroupTension>,
    ) -> Result<Vec<jgr::GroupResult>, String> {
        ours(&method)?;
        params::parse(&input.parameters)?;
        let strengths: Vec<f64> = input.cases.iter().map(|c| c.strength).collect();
        Ok(input
            .groups
            .iter()
            .map(|g| {
                let t = tension.iter().find(|t| t.id == g.id).map(tension_of);
                let r = groups::check_group(&group_of(g), &strengths, t.as_ref());
                result_of(g.id, r)
            })
            .collect())
    }
}

use exports::ohl::plugin::joint_check_groups_prying as jgp;
use exports::ohl::plugin::joint_check_groups_tension as jgt;

impl jgp::Guest for Sia263 {
    fn check_joint_groups_prying(
        method: String,
        input: jgr::GroupInput,
        tension: Vec<jgp::GroupPrying>,
    ) -> Result<Vec<jgr::GroupResult>, String> {
        ours(&method)?;
        params::parse(&input.parameters)?;
        let strengths: Vec<f64> = input.cases.iter().map(|c| c.strength).collect();
        Ok(input
            .groups
            .iter()
            .map(|g| {
                let t = tension.iter().find(|t| t.id == g.id).map(prying_of);
                let r = groups::check_group(&group_of(g), &strengths, t.as_ref());
                result_of(g.id, r)
            })
            .collect())
    }
}

/// A flange or plate of the interface as [`groups::Tee`].
fn tee_of(x: &jgt::Tee) -> groups::Tee {
    groups::Tee {
        part: x.part.clone(),
        thickness: x.thickness,
        fy: x.fy,
        edge: x.edge,
        web: x.web,
        width: x.width,
    }
}

/// A `joint-check-groups-prying` group's tensions as
/// [`groups::GroupTension`] takes them: the plates too (2026-11-19).
fn prying_of(t: &jgp::GroupPrying) -> groups::GroupTension {
    groups::GroupTension {
        bolts: t
            .bolts
            .iter()
            .map(|b| groups::BoltTension {
                capacity: b.tension_capacity,
                tees: b.tees.iter().map(tee_of).collect(),
                plates: b.plates.iter().map(tee_of).collect(),
                unknown: b.unknown.clone(),
            })
            .collect(),
        loads: t.loads.clone(),
    }
}

/// A `joint-check-groups-tension` group's tensions as
/// [`groups::GroupTension`] takes them.
fn tension_of(t: &jgt::GroupTension) -> groups::GroupTension {
    groups::GroupTension {
        bolts: t
            .bolts
            .iter()
            .map(|b| groups::BoltTension {
                capacity: b.tension_capacity,
                tees: b.tees.iter().map(tee_of).collect(),
                plates: Vec::new(),
                unknown: b.unknown.clone(),
            })
            .collect(),
        loads: t.loads.clone(),
    }
}

/// A group's result as the interface gives it.
fn result_of(id: u32, r: groups::GroupResult) -> jgr::GroupResult {
    jgr::GroupResult {
        id,
        usages: r
            .usages
            .into_iter()
            .enumerate()
            .filter_map(|(n, u)| {
                u.map(|(usage, check)| jgr::CaseUsage {
                    load_case: n as u32,
                    usage,
                    check,
                })
            })
            .collect(),
        governs: r.governs,
        notes: r.notes,
        not_safe: (!r.not_safe.is_empty()).then(|| r.not_safe.join("; ")),
    }
}

use exports::ohl::plugin::joint_check_bends as jbe;

impl jbe::Guest for Sia263 {
    fn check_bends(method: String, input: jbe::BendInput) -> Result<Vec<jgr::GroupResult>, String> {
        ours(&method)?;
        params::set(params::parse(&input.parameters)?);
        let strengths: Vec<f64> = input.cases.iter().map(|c| c.strength).collect();
        Ok(input
            .bends
            .iter()
            .map(|b| {
                let r = bends::check_bend(
                    &bends::Bend {
                        part: b.part.clone(),
                        thickness: b.thickness,
                        fy: b.fy,
                        width: b.width,
                        inner_radius: b.inner_radius,
                        welded: b.welded,
                    },
                    &b.loads
                        .iter()
                        .map(|l| bends::BendLoad {
                            axial: l.axial,
                            shear: l.shear,
                            moment: l.moment,
                            moment_in_plane: l.moment_in_plane,
                        })
                        .collect::<Vec<_>>(),
                    &strengths,
                );
                jgr::GroupResult {
                    id: b.id,
                    usages: r
                        .usages
                        .into_iter()
                        .enumerate()
                        .filter_map(|(n, u)| {
                            u.map(|(usage, check)| jgr::CaseUsage {
                                load_case: n as u32,
                                usage,
                                check,
                            })
                        })
                        .collect(),
                    governs: r.governs,
                    notes: r.notes,
                    not_safe: (!r.not_safe.is_empty()).then(|| r.not_safe.join("; ")),
                }
            })
            .collect())
    }
}

export!(Sia263);
