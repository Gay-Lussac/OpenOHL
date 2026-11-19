// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The checks of the joints themselves (stage 4.5 of the joint details,
//! 2026-11-19; built, in active development, not approved yet): the bolt
//! groups under the members' forces together and the bearing of the
//! member they bolt onto, gathered here per joint and per load case and
//! checked by the structure's design code's plugin (`joint-check-groups`;
//! theory page `docs/theory/joint-groups.md`).
//!
//! - **A plate's bolts onto the leg holding it** (`holding`): where the
//!   joint's leg bolts onto a gusset plate (or welded plate) that other
//!   members bolt onto too, the leg's bolts on it carry the resultant of
//!   the other members' forces on the plate — their end forces in the same
//!   case, each along its line through its bolts' centroid — with its
//!   moment about those bolts' centroid, shared out elastically
//!   ([`ohl_structures_fe::bolt_groups::elastic`]). The ply checked is the
//!   plate (bearing, its edges along each bolt's force).
//! - **The bolts onto a member at a joint** (`bearing`): every bolt
//!   through one member's flange at the joint — the members lapped onto it
//!   (each member's force shared by its own bolts, ASCE 10-97 4.1's equal
//!   share, a frame member's end moment elastically) and the holding bolts
//!   above — summed bolt by bolt where bolts of several parts are one
//!   physical bolt (within a quarter of a diameter). The ply checked is
//!   that member: its bearing under each bolt's resultant, its distances
//!   along and square to it.
//! - **What is not defined** is said, never silently skipped: a plate the
//!   joint's leg does not bolt onto (how it is held not given), a holding
//!   leg whose bolts are not placed, a member whose thickness is not known
//!   ([`GroupPlans::not_checked`]).
//!
//! A member's force is counted at every joint it bolts at, and only the
//! same case's forces are summed. The forces on a part are the members'
//! end forces from the analysis (a frame's end shear and moment with its
//! axial force); without them, the axial force along the member.

use std::borrow::Cow;
use std::collections::BTreeMap;

use ohl_sections::SectionLibrary;
use ohl_structures_fe::bolt_groups::{self as bg, Plane, PlaneLoad, V2, V3};
use ohl_structures_fe::model::Model;

use crate::joints::{Arrival, JointDetail, JointDetails};
use crate::plate_checks::{EndForces, PlateCase, PlatePart, PlateUsage};

/// Bolts of different parts nearer than this share one hole: a quarter of
/// the smaller diameter, at least 2 mm.
fn same_hole(d: f64) -> f64 {
    (0.25 * d).max(0.002)
}

/// Members in line through a joint within this angle are taken as one
/// piece running on when nothing says otherwise (no splice there), degrees.
const IN_LINE_DEG: f64 = 5.0;

/// One physical bolt of a group.
#[derive(Debug, Clone, PartialEq)]
struct Bolt {
    /// Where it is (structure axes), m.
    at: V3,
    diameter: f64,
    hole: Option<f64>,
    ultimate: Option<f64>,
    shear: Option<f64>,
    shank: Option<f64>,
    threads_in: Option<bool>,
    grade: String,
    /// The planes it is checked on.
    planes: u32,
    /// The member ends and plates whose forces it carries.
    carries: Vec<String>,
    /// In a member's flange: its gauge from the heel, the flange's width
    /// and the direction across it from heel to toe.
    flange: Option<(f64, f64, V3)>,
}

/// A ply's edges.
#[derive(Debug, Clone, PartialEq)]
enum Edges {
    /// A plate's outline (empty: not known).
    Outline(Vec<V3>),
    /// A member's flange: its toe from each bolt's `flange`; its steel
    /// stopping at `end` from `origin` along `dir` (none: it runs on).
    Flange {
        dir: V3,
        origin: V3,
        end: Option<f64>,
    },
}

/// A ply checked.
#[derive(Debug, Clone, PartialEq)]
struct Ply {
    part: String,
    member: bool,
    thickness: f64,
    fy: Option<f64>,
    fu: Option<f64>,
    redundant: bool,
    known: bool,
    edges: Edges,
    /// Every hole in it at the joint (structure axes).
    holes: Vec<V3>,
    /// The bolts push it along their force (+1: the member they bolt
    /// onto) or against it (−1: the plate a holding bolt holds).
    sign: f64,
}

/// A member end whose force reaches the group's part.
#[derive(Debug, Clone, PartialEq)]
struct Source {
    member: String,
    end: usize,
    direction: V3,
    /// Its bolts' centroid on the part (the joint where not placed).
    centroid: V3,
    /// Bearing groups: its bolts here, (index in the group, where).
    bolts: Vec<(usize, V3)>,
}

/// One group, planned once per structure.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GroupPlan {
    joint: String,
    holding: bool,
    part: String,
    /// Holding: the member(s) holding the plate, in words.
    holders: String,
    position: V3,
    plane: Plane,
    bolts: Vec<Bolt>,
    plies: Vec<Ply>,
    sources: Vec<Source>,
    /// Holding groups whose bolts are among these: (their index, (their
    /// bolt, ours)).
    from: Vec<(usize, Vec<(usize, usize)>)>,
    notes: Vec<String>,
}

/// The groups of a structure and what is not checked.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct GroupPlans {
    pub(crate) groups: Vec<GroupPlan>,
    /// What at a joint is not checked: (joint, part, words).
    pub(crate) not_checked: Vec<(String, String, String)>,
}

impl GroupPlans {
    /// Nothing planned.
    pub(crate) fn is_empty(&self) -> bool {
        self.groups.is_empty() && self.not_checked.is_empty()
    }

    /// Every row's part in words, as the rows of a case name them.
    fn part_of(&self, k: usize) -> PlatePart {
        let g = &self.groups[k];
        if g.holding {
            PlatePart::Holding(g.holders.clone())
        } else {
            PlatePart::Bearing
        }
    }
}

/// The members in line with `id` through joint `j`, `id` among them,
/// sorted, and whether that was assumed (in line, not said continuous):
/// a member running on as one part (`continuous`), or in line within
/// [`IN_LINE_DEG`] of the same section with no splice of the two there.
fn run_at(j: &JointDetail, id: &str) -> (Vec<String>, bool) {
    let mut out = vec![id.to_owned()];
    let mut assumed = false;
    let Some(a) = j.arrivals.iter().find(|x| x.member == id) else {
        return (out, false);
    };
    for b in j.arrivals.iter().filter(|b| b.member != id) {
        let said = a.continuous.as_deref() == Some(b.member.as_str())
            || b.continuous.as_deref() == Some(id);
        let spliced = j
            .splices
            .iter()
            .any(|s| s.members.iter().any(|m| m == id) && s.members.contains(&b.member));
        let line = dot(a.direction, b.direction) <= -IN_LINE_DEG.to_radians().cos();
        let same = match (&a.profile, &b.profile) {
            (Some(p), Some(q)) => p.section_id == q.section_id,
            _ => false,
        };
        if said {
            out.push(b.member.clone());
        } else if line && same && !spliced && !b.modelling {
            out.push(b.member.clone());
            assumed = true;
        }
    }
    out.sort();
    (out, assumed)
}

/// The bolt data of member end `a` (its bolt set over the member's).
fn bolt_of(a: &Arrival, model: &Model, at: V3) -> Option<Bolt> {
    let set = a.bolts.value.as_ref()?;
    let idx = model.members.iter().position(|m| m.id == a.member);
    let (merged, shank, threads) = idx
        .and_then(|k| {
            let m = &model.members[k];
            let e = m.design.ends.get(a.end).and_then(Option::as_ref);
            ohl_structures_fe::member_ends::end_bolts(&m.design, e, &mut Vec::new())
        })
        .map_or((None, None, None), |(b, s, t)| (Some(b), s, t));
    Some(Bolt {
        at,
        diameter: set.diameter,
        hole: set.hole.or(merged.and_then(|b| b.hole)),
        ultimate: merged.and_then(|b| b.ultimate),
        shear: merged.and_then(|b| b.shear_capacity),
        shank,
        threads_in: threads,
        grade: set
            .grade
            .clone()
            .or_else(|| merged.and_then(|b| b.grade).map(|g| g.name()))
            .unwrap_or_default(),
        planes: a.planes.0.value.unwrap_or(1).max(1),
        carries: vec![format!(
            "{} at {}",
            a.member,
            ["its first end", "its second end"][a.end.min(1)]
        )],
        flange: None,
    })
}

/// Adds bolt `b` to `bolts` (one hole with a bolt already there: what it
/// carries added, the weaker values kept), giving its index.
fn add_bolt(bolts: &mut Vec<Bolt>, b: Bolt, notes: &mut Vec<String>) -> usize {
    let near = same_hole(b.diameter);
    if let Some(k) = bolts
        .iter()
        .position(|x| norm(sub(x.at, b.at)) <= near.max(same_hole(x.diameter)))
    {
        let x = &mut bolts[k];
        if (x.diameter - b.diameter).abs() > 1e-6 {
            push_once(
                notes,
                "bolts of different sizes in one hole: the smaller taken (on the safe side)".into(),
            );
        }
        x.diameter = x.diameter.min(b.diameter);
        x.ultimate = min_opt(x.ultimate, b.ultimate);
        x.shear = min_opt(x.shear, b.shear);
        x.shank = min_opt(x.shank, b.shank);
        x.planes = 1;
        for c in b.carries {
            if !x.carries.contains(&c) {
                x.carries.push(c);
            }
        }
        if x.flange.is_none() {
            x.flange = b.flange;
        }
        return k;
    }
    bolts.push(b);
    bolts.len() - 1
}

fn min_opt(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, None) | (None, x) => x,
    }
}

fn push_once(v: &mut Vec<String>, w: String) {
    if !v.contains(&w) {
        v.push(w);
    }
}

/// The centroid of points (the fallback where there are none).
fn centroid3(points: &[V3], fallback: V3) -> V3 {
    if points.is_empty() {
        return fallback;
    }
    let n = points.len() as f64;
    points
        .iter()
        .fold([0.0; 3], |s, q| add(s, scale(*q, 1.0 / n)))
}

/// A member's thickness and steel as a ply.
fn member_ply(a: &Arrival, model: &Model) -> Option<(f64, Option<f64>, Option<f64>, bool)> {
    let m = model.members.iter().find(|m| m.id == a.member)?;
    let s = &model.sections[m.section];
    let t = a
        .profile
        .as_ref()
        .and_then(|p| p.angle.map(|g| g.t))
        .or(m.design.thickness)?;
    let redundant = m.design.class == Some(ohl_structures_fe::design::MemberClass::Redundant);
    Some((t, s.yield_strength, s.ultimate, redundant))
}

/// Every group of the structure: per joint the plates' holding bolts and
/// the bolts onto each member, with what is not checked.
pub(crate) fn group_plans(
    details: &JointDetails,
    model: &Model,
    library: &SectionLibrary,
) -> GroupPlans {
    let mut out = GroupPlans::default();
    for j in &details.joints {
        let first = out.groups.len();
        holding_groups(j, model, library, &mut out);
        bearing_groups(j, model, first, &mut out);
    }
    out
}

/// The holding groups of joint `j` (module docs).
fn holding_groups(j: &JointDetail, model: &Model, library: &SectionLibrary, out: &mut GroupPlans) {
    let legs: Vec<String> = crate::welds::joint_legs(j)
        .iter()
        .map(|a| a.member.clone())
        .collect();
    let mut done: Vec<Vec<String>> = Vec::new();
    for p in &j.plates {
        let mut asm: Vec<String> = if p.assembly.is_empty() {
            vec![p.id.clone()]
        } else {
            p.assembly.clone()
        };
        asm.sort();
        if done.contains(&asm) {
            continue;
        }
        done.push(asm.clone());
        let words = asm.join("+");
        let on: Vec<(String, Cow<Arrival>)> = asm
            .iter()
            .flat_map(|q| {
                crate::joints::bolted_onto(&j.arrivals, q)
                    .into_iter()
                    .map(move |a| (q.clone(), a))
            })
            .filter(|(_, a)| !a.modelling)
            .collect();
        let held = |a: &Arrival| legs.contains(&a.member);
        if on.iter().all(|(_, a)| held(a)) {
            continue;
        }
        if !on.iter().any(|(_, a)| held(a)) {
            if !legs.is_empty() {
                out.not_checked.push((
                    j.id.clone(),
                    words.clone(),
                    format!(
                        "NOT CHECKED, NOT ON THE SAFE SIDE: how plate {words} is held to the \
                         joint's leg ({}) is not given — no bolts of the leg onto it: the bolts \
                         holding it and the leg's bearing under them",
                        legs.join(", ")
                    ),
                ));
            }
            continue;
        }
        // The holding legs, by the run each belongs to, by the plate
        // their bolts are on.
        let mut runs: Vec<(Vec<String>, bool)> = Vec::new();
        for (_, a) in on.iter().filter(|(_, a)| held(a)) {
            let r = run_at(j, &a.member);
            if !runs.iter().any(|x| x.0 == r.0) {
                runs.push(r);
            }
        }
        for (run, assumed) in runs {
            let mut plates: Vec<String> = Vec::new();
            for (q, a) in &on {
                if run.contains(&a.member) && !plates.contains(q) {
                    plates.push(q.clone());
                }
            }
            for q in &plates {
                let holders: Vec<&Cow<Arrival>> = on
                    .iter()
                    .filter(|(x, a)| x == q && run.contains(&a.member))
                    .map(|(_, a)| a)
                    .collect();
                let names: Vec<String> = holders.iter().map(|a| a.member.clone()).collect();
                let hw = names.join("/");
                let points: Vec<(usize, V3)> = holders
                    .iter()
                    .enumerate()
                    .flat_map(|(k, a)| a.bolt_points.iter().map(move |x| (k, *x)))
                    .collect();
                if points.is_empty() {
                    out.not_checked.push((
                        j.id.clone(),
                        q.clone(),
                        format!(
                            "NOT CHECKED, NOT ON THE SAFE SIDE: plate {q}'s bolts onto {hw} — \
                             their bolts are not placed"
                        ),
                    ));
                    continue;
                }
                let mut notes = Vec::new();
                let mut bolts: Vec<Bolt> = Vec::new();
                for (k, at) in &points {
                    let a = holders[*k];
                    let Some(mut b) = bolt_of(a, model, *at) else {
                        continue;
                    };
                    b.carries = vec![format!("plate {words}")];
                    add_bolt(&mut bolts, b, &mut notes);
                }
                if bolts.is_empty() {
                    out.not_checked.push((
                        j.id.clone(),
                        q.clone(),
                        format!(
                            "NOT CHECKED, NOT ON THE SAFE SIDE: plate {q}'s bolts onto {hw} — \
                             their bolts not known"
                        ),
                    ));
                    continue;
                }
                let c = centroid3(&bolts.iter().map(|b| b.at).collect::<Vec<_>>(), j.position);
                let Some(plane) = Plane::new(c, holders[0].bolt_axis, Some(holders[0].direction))
                else {
                    continue;
                };
                let sources: Vec<Source> = on
                    .iter()
                    .filter(|(_, a)| !run.contains(&a.member))
                    .map(|(_, a)| {
                        if a.bolt_points.is_empty() {
                            push_once(
                                &mut notes,
                                format!(
                                    "{}: its bolts on the plate not placed — its force taken \
                                     through the joint",
                                    a.member
                                ),
                            );
                        }
                        Source {
                            member: a.member.clone(),
                            end: a.end,
                            direction: a.direction,
                            centroid: centroid3(&a.bolt_points, j.position),
                            bolts: Vec::new(),
                        }
                    })
                    .collect();
                let pd = j.plates.iter().find(|x| &x.id == q);
                let (steel, steel_note) = match pd {
                    Some(pd) => crate::plate_checks::plate_steel_of(pd, j, model, library),
                    None => continue,
                };
                notes.extend(steel_note);
                let outline = pd.map(|x| x.outline.clone()).unwrap_or_default();
                let holes: Vec<V3> = crate::joints::bolted_onto(&j.arrivals, q)
                    .iter()
                    .flat_map(|a| a.bolt_points.clone())
                    .collect();
                notes.push(format!(
                    "plate {words}'s bolts onto {hw}: the resultant of the other members' forces \
                     on the plate (each along its line through its bolts' centroid, the same \
                     case) with its moment about these bolts' centroid, shared elastically \
                     (Kulak, Fisher & Struik; on the safe side)"
                ));
                if plates.len() > 1 {
                    notes.push(format!(
                        "{hw} bolts onto several plates of {words}: each plate's bolts taken \
                         carrying the whole (on the safe side)"
                    ));
                }
                if run.len() == 1 && holders.len() == 1 {
                    // A leg ending at the plate: its own force is the
                    // other members' resultant (equilibrium).
                } else if assumed {
                    notes.push(format!(
                        "{} in line at {}, not said continuous and no splice there: taken as one \
                         piece holding the plate (an assumption)",
                        run.join(" and "),
                        j.id
                    ));
                }
                if outline.len() < 3 {
                    notes.push(format!(
                        "plate {q} has no outline: its edges' distances not known"
                    ));
                }
                out.groups.push(GroupPlan {
                    joint: j.id.clone(),
                    holding: true,
                    part: q.clone(),
                    holders: hw,
                    position: j.position,
                    plane,
                    bolts,
                    plies: vec![Ply {
                        part: q.clone(),
                        member: false,
                        thickness: steel.thickness,
                        fy: steel.fy,
                        fu: steel.fu,
                        redundant: false,
                        known: outline.len() >= 3,
                        edges: Edges::Outline(outline),
                        holes,
                        sign: -1.0,
                    }],
                    sources,
                    from: Vec::new(),
                    notes,
                });
            }
        }
    }
}

/// The bearing groups of joint `j`: per member others bolt onto (with the
/// members running on in line with it) and per flange, every bolt
/// through it — the lapped members' and the holding groups' from `first`
/// on (this joint's).
fn bearing_groups(j: &JointDetail, model: &Model, first: usize, out: &mut GroupPlans) {
    // (run, the arrival whose bolt it is, the host arrival, holding group).
    struct Entry<'a> {
        run: Vec<String>,
        assumed: bool,
        a: &'a Arrival,
        host: &'a Arrival,
        holding: Option<usize>,
    }
    let mut entries: Vec<Entry> = Vec::new();
    for a in j
        .arrivals
        .iter()
        .filter(|a| !a.modelling && !a.bolt_points.is_empty())
    {
        let Some(h) = a.onto.value.as_deref() else {
            continue;
        };
        let Some(host) = j.arrivals.iter().find(|x| x.member == h) else {
            continue;
        };
        if a.kind
            .value
            .is_some_and(|k| k != ohl_structures::connections::ConnectionKind::Bolted)
        {
            continue;
        }
        let (run, assumed) = run_at(j, h);
        entries.push(Entry {
            run,
            assumed,
            a,
            host,
            holding: None,
        });
    }
    // The holding bolts: in the holding leg's flange.
    for (k, g) in out.groups.iter().enumerate().skip(first) {
        if !g.holding {
            continue;
        }
        for a in j.arrivals.iter().filter(|a| {
            g.holders.split('/').any(|h| h == a.member)
                && (a.onto.value.as_deref() == Some(g.part.as_str())
                    || a.other_onto.as_deref() == Some(g.part.as_str()))
        }) {
            let (run, assumed) = run_at(j, &a.member);
            entries.push(Entry {
                run,
                assumed,
                a,
                host: a,
                holding: Some(k),
            });
        }
    }
    // By run, then by flange (the bolts' axes within 10°).
    let mut keys: Vec<Vec<String>> = Vec::new();
    for e in &entries {
        if !keys.contains(&e.run) {
            keys.push(e.run.clone());
        }
    }
    for run in keys {
        let here: Vec<&Entry> = entries.iter().filter(|e| e.run == run).collect();
        let mut flanges: Vec<(V3, Vec<&Entry>)> = Vec::new();
        for e in here {
            let axis = if e.holding.is_some() && e.a.other_onto.is_some() {
                e.a.other_axis
            } else {
                e.a.bolt_axis
            };
            match flanges
                .iter_mut()
                .find(|f| dot(f.0, axis).abs() >= 10f64.to_radians().cos())
            {
                Some(f) => f.1.push(e),
                None => flanges.push((axis, vec![e])),
            }
        }
        for (axis, list) in flanges {
            let host = list[0].host;
            let part = run.join("/");
            let mut notes = Vec::new();
            let Some((t, fy, fu, redundant)) = member_ply(host, model) else {
                out.not_checked.push((
                    j.id.clone(),
                    part.clone(),
                    format!(
                        "NOT CHECKED, NOT ON THE SAFE SIDE: the bolts onto {part} — its thickness \
                         is not known"
                    ),
                ));
                continue;
            };
            let mut bolts: Vec<Bolt> = Vec::new();
            let mut sources: Vec<Source> = Vec::new();
            let mut from: Vec<(usize, Vec<(usize, usize)>)> = Vec::new();
            for e in &list {
                match e.holding {
                    Some(k) => {
                        let g = out.groups[k].clone();
                        let mut map = Vec::new();
                        let width =
                            e.a.profile
                                .as_ref()
                                .and_then(|p| p.angle)
                                .map(|g| if e.a.leg_y { g.b } else { g.h });
                        let across =
                            e.a.profile
                                .as_ref()
                                .map(|p| if e.a.leg_y { p.axes[1] } else { p.axes[2] });
                        for (n, b) in g.bolts.iter().enumerate() {
                            let mut b = b.clone();
                            b.carries = vec![format!("plate {}", g.part)];
                            b.planes = 1;
                            // Its gauge: the holding leg's own layout.
                            b.flange =
                                e.a.bolt_points
                                    .iter()
                                    .position(|q| norm(sub(*q, b.at)) < 1e-6)
                                    .and_then(|i| e.a.bolt_flat.get(i))
                                    .zip(width)
                                    .zip(across)
                                    .map(|((f, w), c)| (f[1], w, toward_toe(c, e.a)));
                            let ours = add_bolt(&mut bolts, b, &mut notes);
                            map.push((n, ours));
                        }
                        from.push((k, map));
                    }
                    None => {
                        let flange = crate::joints::host_flange(e.a, e.host);
                        let mut mine = Vec::new();
                        for q in &e.a.bolt_points {
                            let Some(mut b) = bolt_of(e.a, model, *q) else {
                                continue;
                            };
                            b.flange = flange.map(|f| {
                                let rel = sub(sub(*q, j.position), f.heel);
                                (dot(rel, f.across), f.width, f.across)
                            });
                            if let Some((g, w, _)) = b.flange
                                && outside(g, w)
                            {
                                push_once(
                                    &mut notes,
                                    format!(
                                        "{}'s bolt {:.0} mm from {part}'s heel lies outside its \
                                         flange (0 to {:.0} mm): the joint does not fit as given \
                                         — its distances taken as 0",
                                        e.a.member,
                                        g * 1000.0,
                                        w * 1000.0
                                    ),
                                );
                            }
                            let k = add_bolt(&mut bolts, b, &mut notes);
                            mine.push((k, *q));
                        }
                        if flange.is_none() {
                            push_once(
                                &mut notes,
                                format!(
                                    "{}'s flange under {} not found (not an angle): its edges' \
                                     distances not known",
                                    part, e.a.member
                                ),
                            );
                        }
                        sources.push(Source {
                            member: e.a.member.clone(),
                            end: e.a.end,
                            direction: e.a.direction,
                            centroid: centroid3(&e.a.bolt_points, j.position),
                            bolts: mine,
                        });
                    }
                }
            }
            if bolts.is_empty() {
                continue;
            }
            let shared = bolts.iter().filter(|b| b.carries.len() > 1).count();
            if shared > 0 {
                notes.push(format!(
                    "{shared} bolt(s) carry the forces of several parts: summed as vectors, \
                     the same case, on one shear plane (on the safe side)"
                ));
            }
            notes.push(format!(
                "the bolts onto {part}: each member's force shared equally by its own bolts \
                 (ASCE 10-97 4.1; a frame member's end moment elastically), summed bolt by bolt"
            ));
            // Where the host's steel stops at the joint.
            let assumed = list.iter().any(|e| e.assumed);
            let end = if run.len() > 1 {
                if assumed {
                    notes.push(format!(
                        "{} in line at {}, not said continuous and no splice there: taken as one \
                         angle running on (no end at the joint; an assumption — a splice there \
                         would be in the file)",
                        run.join(" and "),
                        j.id
                    ));
                }
                None
            } else {
                Some(host.cut.value.unwrap_or(0.0))
            };
            let known =
                (end.is_none() || host.cut.known()) && bolts.iter().all(|b| b.flange.is_some());
            if end.is_some() && !host.cut.known() {
                notes.push(format!(
                    "{part} ends at {}: where its steel stops is not given (taken at the work \
                     point; the code's conservative case)",
                    j.id
                ));
            }
            let c = centroid3(&bolts.iter().map(|b| b.at).collect::<Vec<_>>(), j.position);
            let Some(plane) = Plane::new(c, axis, Some(host.direction)) else {
                continue;
            };
            let holes = bolts.iter().map(|b| b.at).collect();
            out.groups.push(GroupPlan {
                joint: j.id.clone(),
                holding: false,
                part: part.clone(),
                holders: String::new(),
                position: j.position,
                plane,
                bolts,
                plies: vec![Ply {
                    part,
                    member: true,
                    thickness: t,
                    fy,
                    fu,
                    redundant,
                    known,
                    edges: Edges::Flange {
                        dir: host.direction,
                        origin: j.position,
                        end,
                    },
                    holes,
                    sign: 1.0,
                }],
                sources,
                from,
                notes,
            });
        }
    }
}

/// `across` (an axis of the angle's leg bolted) pointing from its heel to
/// its toe: the angle's heel lies at the legs' far side from the toe.
fn toward_toe(across: V3, a: &Arrival) -> V3 {
    let Some(p) = a.profile.as_ref() else {
        return across;
    };
    let Some(g) = p.angle else {
        return across;
    };
    // The heel's coordinate along this axis; the toe lies the other way
    // from the centroid.
    let heel = if a.leg_y { g.heel[0] } else { g.heel[1] };
    if heel > 0.0 {
        scale(across, -1.0)
    } else {
        across
    }
}

/// A member end's force and moment on the part in one case: the end
/// force from the analysis (on the member, reversed), its moment taken to
/// its bolts' centroid without the axial force's arm (each member's bolts
/// share its axial force equally) and with the levers laid into the
/// group's `plane` (the plies' offsets through their thickness left out,
/// as every lapped connection's checks take them); without end forces, the
/// axial force along it.
fn source_load(
    s: &Source,
    plane: &Plane,
    position: V3,
    axial: &BTreeMap<String, f64>,
    ends: Option<&EndForces>,
) -> (V3, V3) {
    let n = axial.get(&s.member).copied().unwrap_or(0.0);
    match ends.and_then(|e| e.get(&s.member)) {
        Some(ef) => {
            let e = ef[s.end.min(1)];
            let f = [-e[0], -e[1], -e[2]];
            let m = [-e[3], -e[4], -e[5]];
            let v = sub(f, scale(s.direction, dot(f, s.direction)));
            (f, add(m, cross(in_plane(plane, position, s.centroid), v)))
        }
        None => (scale(s.direction, n), [0.0; 3]),
    }
}

/// From `b` to `a` laid into `plane` (structure axes).
fn in_plane(plane: &Plane, a: V3, b: V3) -> V3 {
    let (pa, pb) = (plane.point(a), plane.point(b));
    plane.to_3d([pa[0] - pb[0], pa[1] - pb[1]])
}

/// A bolt `g` from a flange's heel lies outside its width `w` (by more
/// than a millimetre either way).
fn outside(g: f64, w: f64) -> bool {
    !(-0.001..=w + 0.001).contains(&g)
}

/// The lever a moment on bolts is compared with: the group's largest
/// radius, or three bolt diameters for one bolt (a pin's), m.
fn lever(points: &[V2], d: f64) -> f64 {
    let c = bg::centroid(points);
    points
        .iter()
        .map(|p| (p[0] - c[0]).hypot(p[1] - c[1]))
        .fold(0.0, f64::max)
        .max(3.0 * d)
}

/// Each group's bolt forces on the part it bolts onto in one case (the
/// plane's vectors, N) and what is not on the safe side.
fn case_forces(
    plans: &GroupPlans,
    axial: &BTreeMap<String, f64>,
    ends: Option<&EndForces>,
) -> Vec<(Vec<V2>, Vec<String>)> {
    let mut out: Vec<(Vec<V2>, Vec<String>)> = Vec::with_capacity(plans.groups.len());
    for g in &plans.groups {
        let mut forces = vec![[0.0, 0.0]; g.bolts.len()];
        let mut said = Vec::new();
        let d = g
            .bolts
            .iter()
            .map(|b| b.diameter)
            .fold(f64::INFINITY, f64::min);
        if g.holding {
            let pts: Vec<V2> = g.bolts.iter().map(|b| g.plane.point(b.at)).collect();
            let c3 = g.plane.origin;
            let mut f3 = [0.0; 3];
            let mut m3 = [0.0; 3];
            for s in &g.sources {
                let (f, m) = source_load(s, &g.plane, g.position, axial, ends);
                f3 = add(f3, f);
                m3 = add(m3, add(m, cross(in_plane(&g.plane, s.centroid, c3), f)));
            }
            let f2 = g.plane.vector(f3);
            let mn = dot(m3, g.plane.n);
            let e = bg::elastic(
                &pts,
                &PlaneLoad {
                    force: f2,
                    at: bg::centroid(&pts),
                    moment: mn,
                },
            );
            forces = e.forces.clone();
            let l = lever(&pts, d);
            let size = f2[0].hypot(f2[1]);
            if !e.shared && mn.abs() / l >= 0.01 * size.max(1e-9) && mn.abs() > 1e-6 {
                said.push(format!(
                    "one bolt cannot carry the moment of the members' forces about it ({:.2} kN \
                     m): a pin, the moment not carried",
                    mn.abs() / 1000.0
                ));
            }
            let normal = dot(f3, g.plane.n).abs();
            let out_m = norm(sub(m3, scale(g.plane.n, mn)));
            if normal + out_m / l >= 0.01 * size.max(1e-9) && normal + out_m > 1e-6 {
                said.push(format!(
                    "the members' forces square to the plate ({:.1} kN) and their moment bending \
                     it out of its plane ({:.2} kN m) pull on its bolts: bolts in tension not \
                     checked",
                    normal / 1000.0,
                    out_m / 1000.0
                ));
            }
        } else {
            for s in &g.sources {
                let (f, m) = source_load(s, &g.plane, g.position, axial, ends);
                let pts: Vec<V2> = s.bolts.iter().map(|x| g.plane.point(x.1)).collect();
                if pts.is_empty() {
                    continue;
                }
                let f2 = g.plane.vector(f);
                let mn = dot(m, g.plane.n);
                let e = bg::elastic(
                    &pts,
                    &PlaneLoad {
                        force: f2,
                        at: bg::centroid(&pts),
                        moment: mn,
                    },
                );
                for ((k, _), v) in s.bolts.iter().zip(&e.forces) {
                    forces[*k][0] += v[0];
                    forces[*k][1] += v[1];
                }
                let l = lever(&pts, d);
                let size = f2[0].hypot(f2[1]);
                if !e.shared && mn.abs() / l >= 0.01 * size.max(1e-9) && mn.abs() > 1e-6 {
                    push_once(
                        &mut said,
                        format!(
                            "one bolt of {} cannot carry its end moment ({:.2} kN m): a pin, the \
                             moment not carried",
                            s.member,
                            mn.abs() / 1000.0
                        ),
                    );
                }
                let normal = dot(f, g.plane.n).abs();
                let out_m = norm(sub(m, scale(g.plane.n, mn)));
                if normal + out_m / l >= 0.01 * size.max(1e-9) && normal + out_m > 1e-6 {
                    push_once(
                        &mut said,
                        format!(
                            "{}'s force square to {} ({:.1} kN) and its moment out of the plane \
                             ({:.2} kN m) pull on its bolts: bolts in tension not checked",
                            s.member,
                            g.part,
                            normal / 1000.0,
                            out_m / 1000.0
                        ),
                    );
                }
            }
            for (k, map) in &g.from {
                let Some((theirs, _)) = out.get(*k) else {
                    continue;
                };
                let hp = plans.groups[*k].plane;
                for (n, ours) in map {
                    let v3 = hp.to_3d(theirs[*n]);
                    let v = g.plane.vector(v3);
                    forces[*ours][0] += v[0];
                    forces[*ours][1] += v[1];
                }
            }
        }
        out.push((forces, said));
    }
    out
}

/// From `q` along `u` to ply `p`'s edge (none: no edge that way).
fn ray(p: &Ply, b: &Bolt, q: V3, u: V3) -> Option<f64> {
    match &p.edges {
        Edges::Outline(o) if o.len() >= 3 => {
            // A bolt outside the outline: nothing there (0, on the safe side).
            Some(crate::plate_checks::ray_to_outline(o, q, u).unwrap_or(0.0))
        }
        Edges::Outline(_) => None,
        Edges::Flange { dir, origin, end } => {
            let mut best: Option<f64> = None;
            // A bolt outside the flange: no steel round it (0, on the
            // safe side).
            if b.flange.is_some_and(|(g, w, _)| outside(g, w)) {
                return Some(0.0);
            }
            if let Some((g, w, across)) = b.flange {
                let c = dot(u, across);
                if c > 1e-6 {
                    best = Some((w - g).max(0.0) / c);
                }
            }
            if let Some(cut) = end {
                let a = dot(u, *dir);
                if a < -1e-6 {
                    let s = dot(sub(q, *origin), *dir);
                    let e = (s - cut).max(0.0) / -a;
                    best = Some(best.map_or(e, |x| x.min(e)));
                }
            }
            best
        }
    }
}

/// The groups' rows of every case (`ends`: each case's end forces, or
/// empty), checked by the structure's plugin `p` — one call with every
/// group and case — or, without a plugin, `none` said on each.
pub(crate) fn group_rows(
    plans: &GroupPlans,
    structure: &str,
    plugin: Option<&ohl_structures_fe::model::PluginMethod>,
    none: Option<&str>,
    cases: &[PlateCase],
    ends: &[&EndForces],
) -> Vec<Vec<PlateUsage>> {
    use ohl_member_check::joints as by_plugin;
    use ohl_member_check::joints::groups as w;
    let per_case: Vec<Vec<(Vec<V2>, Vec<String>)>> = cases
        .iter()
        .enumerate()
        .map(|(n, c)| {
            case_forces(
                plans,
                c.2,
                ends.get(n).copied().filter(|_| ends.len() == cases.len()),
            )
        })
        .collect();
    // The rows without a usage first.
    let said_rows = |_: usize| -> Vec<PlateUsage> {
        plans
            .not_checked
            .iter()
            .map(|(joint, part, words)| PlateUsage {
                joint: joint.clone(),
                plate: part.clone(),
                part: PlatePart::NotChecked(words.clone()),
                force: 0.0,
                usage: None,
                governs: words.clone(),
                checks: Vec::new(),
                notes: Vec::new(),
            })
            .collect()
    };
    let largest = |n: usize, k: usize| -> f64 {
        per_case[n][k]
            .0
            .iter()
            .map(|f| f[0].hypot(f[1]))
            .fold(0.0, f64::max)
    };
    let row = |n: usize, k: usize| -> PlateUsage {
        let g = &plans.groups[k];
        PlateUsage {
            joint: g.joint.clone(),
            plate: g.part.clone(),
            part: plans.part_of(k),
            force: largest(n, k),
            usage: None,
            governs: String::new(),
            checks: Vec::new(),
            notes: g.notes.clone(),
        }
    };
    let Some(p) = plugin else {
        let words = none.unwrap_or(ohl_structures_fe::plates::NOT_RUN);
        return (0..cases.len())
            .map(|n| {
                let mut rows = said_rows(n);
                rows.extend((0..plans.groups.len()).map(|k| {
                    let mut r = row(n, k);
                    r.governs = words.to_owned();
                    r
                }));
                rows
            })
            .collect();
    };
    let input = w::GroupInput {
        structure: structure.to_owned(),
        parameters: String::new(),
        cases: cases
            .iter()
            .map(|c| w::LoadCase {
                name: c.0.to_owned(),
                strength: c.1,
            })
            .collect(),
        groups: plans
            .groups
            .iter()
            .enumerate()
            .map(|(k, g)| {
                let c = bg::centroid(
                    &g.bolts
                        .iter()
                        .map(|b| g.plane.point(b.at))
                        .collect::<Vec<_>>(),
                );
                w::Group {
                    id: k as u32,
                    joint: g.joint.clone(),
                    kind: if g.holding { "holding" } else { "bearing" }.to_owned(),
                    part: g.part.clone(),
                    plies: g
                        .plies
                        .iter()
                        .map(|p| w::Ply {
                            part: p.part.clone(),
                            kind: if p.member { "member" } else { "plate" }.to_owned(),
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
                        .map(|b| {
                            let q = g.plane.point(b.at);
                            w::GroupBolt {
                                diameter: b.diameter,
                                hole: b.hole,
                                bolt_ultimate: b.ultimate,
                                shear_capacity: b.shear,
                                shear_capacity_shank: b.shank,
                                threads_in_shear: b.threads_in,
                                grade: b.grade.clone(),
                                planes: b.planes,
                                carries: b.carries.clone(),
                                at: (q[0] - c[0], q[1] - c[1]),
                            }
                        })
                        .collect(),
                    loads: (0..cases.len())
                        .map(|n| {
                            let (forces, _) = &per_case[n][k];
                            g.bolts
                                .iter()
                                .zip(forces)
                                .map(|(b, f)| bolt_load(g, b, *f))
                                .collect()
                        })
                        .collect(),
                    notes: g.notes.clone(),
                }
            })
            .collect(),
    };
    let checked = if plans.groups.is_empty() {
        by_plugin::GroupsChecked::Results(Vec::new())
    } else {
        by_plugin::check_joint_groups(p, &input)
    };
    let not_checked = by_plugin::not_checked_by(p);
    (0..cases.len())
        .map(|n| {
            let mut rows = said_rows(n);
            for (k, forces) in per_case[n].iter().enumerate() {
                let mut r = row(n, k);
                let results = match &checked {
                    by_plugin::GroupsChecked::NotChecked(why) => {
                        r.governs = format!("{not_checked}: {why}");
                        rows.push(r);
                        continue;
                    }
                    by_plugin::GroupsChecked::Results(x) => x,
                };
                let Some(x) = results.iter().find(|x| x.id as usize == k) else {
                    r.governs =
                        format!("{not_checked}: the plugin returned nothing for this group");
                    rows.push(r);
                    continue;
                };
                r.notes.extend(x.notes.iter().cloned());
                let Some(u) = x.usages.iter().find(|u| u.load_case as usize == n) else {
                    r.governs =
                        format!("{not_checked}: the plugin returned no usage for this load case");
                    rows.push(r);
                    continue;
                };
                r.usage = Some(u.usage);
                r.governs = if u.check.is_empty() {
                    x.governs.clone()
                } else {
                    u.check.clone()
                };
                r.checks = vec![(r.governs.clone(), u.usage)];
                let mut unsafe_words: Vec<String> = Vec::new();
                if let Some(ws) = &x.not_safe {
                    unsafe_words.push(ws.clone());
                }
                unsafe_words.extend(forces.1.iter().cloned());
                if !unsafe_words.is_empty() {
                    r.governs.push_str(&format!(
                        " {} {}]",
                        ohl_structures_fe::check::NOT_SAFE,
                        unsafe_words.join("; ")
                    ));
                }
                rows.push(r);
            }
            rows
        })
        .collect()
}

/// One bolt's load as the plugin takes it: the size of its force `f` and,
/// per ply, the distances along the direction it pushes the ply and
/// square to it.
fn bolt_load(g: &GroupPlan, b: &Bolt, f: V2) -> ohl_member_check::joints::groups::BoltLoad {
    let size = f[0].hypot(f[1]);
    let mut ends = Vec::new();
    let mut sides = Vec::new();
    let mut ahead = Vec::new();
    for p in &g.plies {
        if size < 1e-9 {
            ends.push(None);
            sides.push(None);
            ahead.push(None);
            continue;
        }
        let u = scale(g.plane.to_3d([f[0] / size, f[1] / size]), p.sign);
        let wq = cross(g.plane.n, u);
        ends.push(ray(p, b, b.at, u));
        let s = match (ray(p, b, b.at, wq), ray(p, b, b.at, scale(wq, -1.0))) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (x, None) | (None, x) => x,
        };
        sides.push(s);
        // The next hole ahead: in front, within a bolt diameter of the
        // force's line; its distance centre to centre.
        let mut next: Option<f64> = None;
        for h in &p.holes {
            let d = sub(*h, b.at);
            let along = dot(d, u);
            let lat = dot(d, wq).abs();
            if along > 1e-4 && lat < b.diameter {
                let c = norm(d);
                next = Some(next.map_or(c, |x: f64| x.min(c)));
            }
        }
        ahead.push(next);
    }
    ohl_member_check::joints::groups::BoltLoad {
        force: size,
        ends,
        sides,
        ahead,
    }
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m16(at: V3, carries: &[&str]) -> Bolt {
        Bolt {
            at,
            diameter: 0.016,
            hole: None,
            ultimate: Some(800e6),
            shear: None,
            shank: None,
            threads_in: None,
            grade: "8.8".into(),
            planes: 1,
            carries: carries.iter().map(|s| (*s).to_owned()).collect(),
            flange: None,
        }
    }

    fn source(member: &str, direction: V3, centroid: V3, bolts: Vec<(usize, V3)>) -> Source {
        Source {
            member: member.into(),
            end: 1,
            direction,
            centroid,
            bolts,
        }
    }

    fn group(holding: bool, bolts: Vec<Bolt>, sources: Vec<Source>, edges: Edges) -> GroupPlan {
        let c = centroid3(&bolts.iter().map(|b| b.at).collect::<Vec<_>>(), [0.0; 3]);
        GroupPlan {
            joint: "N2".into(),
            holding,
            part: if holding { "G1" } else { "L1" }.into(),
            holders: if holding { "L1" } else { "" }.into(),
            position: [0.0; 3],
            plane: Plane::new(c, [0.0, 0.0, 1.0], Some([1.0, 0.0, 0.0])).unwrap(),
            plies: vec![Ply {
                part: "X".into(),
                member: !holding,
                thickness: 0.008,
                fy: Some(355e6),
                fu: Some(510e6),
                redundant: false,
                known: true,
                edges,
                holes: bolts.iter().map(|b| b.at).collect(),
                sign: if holding { -1.0 } else { 1.0 },
            }],
            bolts,
            sources,
            from: Vec::new(),
            notes: Vec::new(),
        }
    }

    fn forces(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect()
    }

    /// Two diagonals on one plate onto a leg (invented, by hand): D1
    /// pulling 60 kN along (1, 1)/√2 through its bolts at (120, 120) mm,
    /// D2 pushing 60 kN along (1, −1)/√2 through (120, −120) mm. On the
    /// plate: (42.43, 42.43) + (−42.43, 42.43) = (0, 84.85) kN along the
    /// leg. Holding bolts at (0, ±50) mm: both lines pass through their
    /// centroid — no moment, 42.43 kN each. Moved to (−40, ±50) mm: M =
    /// 84.85 × 0.04 = 3.394 kN m, Σr² = 2 × 0.05² = 0.005 m², the moment's
    /// share 3.394 × 0.05 / 0.005 = 33.94 kN across: each bolt
    /// √(33.94² + 42.43²) = 54.33 kN.
    #[test]
    fn a_plates_holding_bolts_carry_the_resultant_and_its_moment() {
        let s = 0.5f64.sqrt();
        let sources = || {
            vec![
                source("D1", [s, s, 0.0], [0.12, 0.12, 0.0], Vec::new()),
                source("D2", [s, -s, 0.0], [0.12, -0.12, 0.0], Vec::new()),
            ]
        };
        let axial = forces(&[("D1", 60e3), ("D2", -60e3)]);
        for (x, want) in [(0.0, 60e3 * s), (-0.04, 54.33e3)] {
            let g = group(
                true,
                vec![
                    m16([x, -0.05, 0.0], &["plate G1"]),
                    m16([x, 0.05, 0.0], &["plate G1"]),
                ],
                sources(),
                Edges::Outline(Vec::new()),
            );
            let plans = GroupPlans {
                groups: vec![g],
                not_checked: Vec::new(),
            };
            let f = case_forces(&plans, &axial, None);
            for v in &f[0].0 {
                assert!((v[0].hypot(v[1]) - want).abs() < 10.0, "{x}: {v:?}");
            }
            // The sum is the resultant, along the leg.
            let sum = [f[0].0[0][0] + f[0].0[1][0], f[0].0[0][1] + f[0].0[1][1]];
            assert!(
                sum[0].abs() < 1e-6 && (sum[1] - 120e3 * s).abs() < 1e-6,
                "{sum:?}"
            );
            assert!(f[0].1.is_empty(), "{:?}", f[0].1);
        }
    }

    /// A redundant and a diagonal on the same leg bolt line (invented, by
    /// hand): D1's two bolts at (0, 0) and (0, 50) mm push the leg 40 kN
    /// along (0.6, 0.8), 20 kN each; R1's one bolt is the same hole as
    /// D1's first and pushes 10 kN along −x. That bolt: (12, 16) + (−10,
    /// 0) = (2, 16) kN, 16.12 kN; the other 20 kN. In the leg's flange
    /// (gauge 30 mm of 61 mm, the toe along +x, the leg running on): the
    /// second bolt's force reaches the toe 31/0.6 = 51.7 mm away, the edge
    /// square to it 31/0.8 = 38.75 mm; the first has D1's second bolt
    /// ahead, 50 mm centre to centre (6 mm off its force's line).
    #[test]
    fn a_shared_bolt_on_the_leg_carries_both_members() {
        let flange = Some((0.03, 0.061, [1.0, 0.0, 0.0]));
        let mut b0 = m16([0.0, 0.0, 0.0], &["D1", "R1"]);
        b0.flange = flange;
        let mut b1 = m16([0.0, 0.05, 0.0], &["D1"]);
        b1.flange = flange;
        let g = group(
            false,
            vec![b0, b1],
            vec![
                source(
                    "D1",
                    [0.6, 0.8, 0.0],
                    [0.0, 0.025, 0.0],
                    vec![(0, [0.0, 0.0, 0.0]), (1, [0.0, 0.05, 0.0])],
                ),
                source("R1", [-1.0, 0.0, 0.0], [0.0; 3], vec![(0, [0.0; 3])]),
            ],
            Edges::Flange {
                dir: [0.0, 1.0, 0.0],
                origin: [0.0; 3],
                end: None,
            },
        );
        let plans = GroupPlans {
            groups: vec![g],
            not_checked: Vec::new(),
        };
        let axial = forces(&[("D1", 40e3), ("R1", 10e3)]);
        let f = case_forces(&plans, &axial, None);
        let v = &f[0].0;
        assert!(
            (v[0][0] - 2e3).abs() < 1e-6 && (v[0][1] - 16e3).abs() < 1e-6,
            "{v:?}"
        );
        assert!((v[1][0].hypot(v[1][1]) - 20e3).abs() < 1e-6);
        let g = &plans.groups[0];
        let l1 = bolt_load(g, &g.bolts[1], v[1]);
        assert!((l1.ends[0].unwrap() - 0.031 / 0.6).abs() < 1e-9, "{l1:?}");
        assert!((l1.sides[0].unwrap() - 0.031 / 0.8).abs() < 1e-9, "{l1:?}");
        assert!(l1.ahead[0].is_none());
        let l0 = bolt_load(g, &g.bolts[0], v[0]);
        assert!((l0.ahead[0].unwrap() - 0.05).abs() < 1e-9, "{l0:?}");
    }

    /// A holding bolt pushes the plate against its force: the end distance
    /// in the plate is taken the other way (a 200 × 100 mm plate, the bolt
    /// 50 mm from its left edge, the force on the leg along +x: 50 mm).
    #[test]
    fn the_plate_is_pushed_the_other_way() {
        let outline = vec![
            [-0.05, -0.05, 0.0],
            [0.15, -0.05, 0.0],
            [0.15, 0.05, 0.0],
            [-0.05, 0.05, 0.0],
        ];
        let g = group(
            true,
            vec![m16([0.0; 3], &["plate G1"])],
            Vec::new(),
            Edges::Outline(outline),
        );
        let l = bolt_load(&g, &g.bolts[0], [10e3, 0.0]);
        assert!((l.ends[0].unwrap() - 0.05).abs() < 1e-12, "{l:?}");
        assert!((l.sides[0].unwrap() - 0.05).abs() < 1e-12, "{l:?}");
    }
}
