// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The plugin interface: the records of `member-check-full`, `joint-check`
//! and `bolt-distances` read into the check's types and its results back
//! (WebAssembly only; the check itself is plain Rust, tested natively).

use crate::{asce10, check, joints, memo, params};

wit_bindgen::generate!({
    path: "../../../crates/ohl-plugins/wit",
    world: "ohl:plugin/structure-code-plugin",
    generate_all,
});

use crate::model::{
    Analysis, Bolts, Condition, ConnectedLeg, EndBolts, EndConditions, EndDesign, Grade, Member,
    MemberClass, MemberDesign, MemberForces, MemberKind, Onto, OntoLeg, Section, Source, Supported,
};
use exports::ohl::plugin::member_check_full as full;

/// The method's description.
const DESCRIPTION: &str = "ASCE 10-97 (Design of Latticed Steel Transmission Structures): \
                           compression by the effective slenderness, local buckling and the \
                           column curve, tension by the net section, block shear and the \
                           connection, bolts by 4.3 and 4.4 — OpenOHL's default, bundled with it";

struct Asce10;

impl Guest for Asce10 {
    fn describe() -> PluginInfo {
        PluginInfo {
            name: "asce10".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "ASCE 10-97 for structures: members, joints, bolt distances".into(),
        }
    }
}

/// Err unless `method` is this plugin's.
fn ours(method: &str) -> Result<(), String> {
    if method == asce10::NAME {
        Ok(())
    } else {
        Err(format!(
            "no method \"{method}\" (this plugin gives {})",
            asce10::NAME
        ))
    }
}

impl exports::ohl::plugin::member_check::Guest for Asce10 {
    fn methods() -> Vec<exports::ohl::plugin::member_check::MethodInfo> {
        vec![exports::ohl::plugin::member_check::MethodInfo {
            name: asce10::NAME.into(),
            description: DESCRIPTION.into(),
        }]
    }

    fn check_members(
        method: String,
        _input: exports::ohl::plugin::member_check::CheckInput,
    ) -> Result<Vec<exports::ohl::plugin::member_check::MemberResult>, String> {
        ours(&method)?;
        Err(
            "the ASCE 10-97 check needs the structure as `member-check-full` sends it (the \
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
        bolt: c
            .bolts
            .as_ref()
            .map(|b| b.bolt.clone())
            .filter(|b| !b.is_empty()),
        notes: c.notes.clone(),
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
            width_thickness: m.width_thickness,
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
            restrained: [cond(0, |e| &e.restrained), cond(1, |e| &e.restrained)],
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
fn capacities(c: &asce10::Capacity) -> Vec<full::Capacity> {
    let slenderness = c
        .kl_r
        .map(|l| (l, "KL/r, the effective slenderness".to_owned()));
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

/// The check of every member of `input` (the parameters already set).
pub fn check_all(input: &full::FullInput) -> Vec<full::MemberResult> {
    let analysis = Analysis {
        single_angle_moments: input.single_angle_moments,
        no_compression: input.tension_compression == "none",
        redundant_force: input.redundant_share,
    };
    input
        .members
        .iter()
        .map(|w| {
            let m = member(w);
            // Its capacities as the solve's cap took them (its crossing
            // partner not taken as bracing it).
            let partner = (!w.crossing.is_empty()).then_some(false);
            let c = asce10::capacity(&m, partner);
            let mut notes = c.compression_assumed.clone();
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
                    let u = check::member_usage(
                        &analysis,
                        &m,
                        &forces(f),
                        f.partner_axial,
                        f.kept_active,
                        strength,
                        supported,
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

impl full::Guest for Asce10 {
    fn parameters_template(method: String) -> Option<String> {
        // No parameters (ASCE 10-97 has no factors to set here).
        let _ = method;
        None
    }

    fn check_members_full(
        method: String,
        input: full::FullInput,
    ) -> Result<full::FullOutput, String> {
        ours(&method)?;
        params::parse(&input.parameters)?;
        memo::start();
        let results = check_all(&input);
        memo::stop();
        Ok(full::FullOutput {
            method_note: asce10::TITLE.to_owned(),
            results,
        })
    }
}

use exports::ohl::plugin::bolt_distances::{DistanceInput, Distances};
use exports::ohl::plugin::joint_check::{CaseUsage as PartUsage, JointInput, Part, PartResult};

/// One joint part's checks under force `n`.
fn part_checked(p: &Part, n: f64) -> Option<joints::Checked> {
    match p.kind.as_str() {
        "bolts" => {
            let (b, s) = (p.bolts.as_ref()?, p.steel.as_ref()?);
            let g = joints::BoltGroup {
                count: b.count,
                diameter: b.diameter,
                hole: b.hole,
                bolt_ultimate: b.bolt_ultimate,
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
            Some(joints::bolted(&g, &steel, n))
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
            Some(joints::weld(&input, n))
        }
        _ => None,
    }
}

/// Every part of `input` checked (the parameters already set): one
/// result per part it checks.
fn joint_results(input: &JointInput) -> Vec<PartResult> {
    input
        .parts
        .iter()
        .filter_map(|p| {
            let mut notes: Vec<String> = Vec::new();
            let mut usages = Vec::new();
            let mut governs = String::new();
            let mut worst: Option<f64> = None;
            for (n, case) in input.cases.iter().enumerate() {
                let force = p.forces.get(n).copied().unwrap_or(0.0);
                let checked = part_checked(p, force)?;
                for note in &checked.notes {
                    if !notes.contains(note) {
                        notes.push(note.clone());
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
                not_safe: None,
            })
        })
        .collect()
}

impl exports::ohl::plugin::joint_check::Guest for Asce10 {
    fn check_joints(method: String, input: JointInput) -> Result<Vec<PartResult>, String> {
        ours(&method)?;
        // No parameters (the method takes none).
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

impl jfull::Guest for Asce10 {
    fn check_joints_full(
        method: String,
        input: jfull::JointInput,
    ) -> Result<Vec<jfull::PartResult>, String> {
        ours(&method)?;
        params::parse(&input.parameters)?;
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

impl exports::ohl::plugin::bolt_distances::Guest for Asce10 {
    fn minimum_distances(
        method: String,
        input: DistanceInput,
    ) -> Result<Option<Distances>, String> {
        ours(&method)?;
        // ASCE 10-97 4.5 with the bolt's force (none: the force terms left
        // out), the edge sheared or rolled, a redundant member's own rule.
        let [end, spacing, edge] = asce10::bolt_minimums(
            input.diameter,
            input.thickness,
            input.ultimate.unwrap_or(0.0),
            input.bolt_force,
            input.redundant,
            input.sheared_edge,
        );
        Ok(Some(Distances {
            end,
            spacing,
            edge,
            hole: Some(input.diameter + asce10::STANDARD_HOLE),
            clauses: [
                if input.redundant {
                    "4.5.1, redundant"
                } else {
                    "4.5.1"
                },
                "4.5.2",
                if input.sheared_edge {
                    "4.5.3, Eq. 4.5-7"
                } else {
                    "4.5.3, Eq. 4.5-6"
                },
            ]
            .map(str::to_owned)
            .to_vec(),
            notes: Vec::new(),
        }))
    }
}

impl exports::ohl::plugin::bolt_distances_full::Guest for Asce10 {
    fn minimum_distances_full(
        method: String,
        input: exports::ohl::plugin::bolt_distances_full::DistanceInput,
    ) -> Result<Option<exports::ohl::plugin::bolt_distances_full::Distances>, String> {
        ours(&method)?;
        params::parse(&input.parameters)?;
        // The analysis' force per bolt, else the bolt's own design shear
        // capacity (4.3.2; third round, 2026-11-18: worked out here, as the
        // core did before for every code).
        let own = input.bolts.as_ref().and_then(|b| {
            asce10::bolt_shear(
                input.diameter,
                b.shear_planes,
                b.shear_capacity,
                b.shear_capacity_shank,
                b.threads_in_shear,
                b.ultimate,
            )
        });
        let bolt_force = input.analysis_force.or(own);
        let d = <Asce10 as exports::ohl::plugin::bolt_distances::Guest>::minimum_distances(
            method,
            DistanceInput {
                diameter: input.diameter,
                thickness: input.thickness,
                ultimate: input.ultimate,
                bolt_force,
                sheared_edge: input.sheared_edge,
                redundant: input.redundant,
            },
        )?;
        Ok(
            d.map(|d| exports::ohl::plugin::bolt_distances_full::Distances {
                end: d.end,
                spacing: d.spacing,
                edge: d.edge,
                hole: d.hole,
                clauses: d.clauses,
                notes: d.notes,
                bolt_force,
                force_basis: if input.analysis_force.is_none() && own.is_some() {
                    "the bolt's design shear capacity, ASCE 10-97 4.3.2".to_owned()
                } else {
                    String::new()
                },
            }),
        )
    }
}

export!(Asce10);
