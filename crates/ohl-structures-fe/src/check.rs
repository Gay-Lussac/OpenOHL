// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Member usage: the member forces of a [`Solution`] against the section's
//! capacity times the load case's strength factor.
//!
//! The capacities come from the model's capacity method
//! (`[analysis] capacity`, [`CapacityMethod`], 2026-10-27):
//!
//! - **ASCE 10-97** (the default, [`crate::asce10`]): compression by the
//!   effective slenderness, local buckling and the column curve, the
//!   least with the bolts' shear and bearing; tension the least of the net
//!   section, bolt shear and bearing — from the member's design data
//!   ([`crate::design`]), missing data assumed on the safe side and said
//!   per member (`[assumed: …]`). A member whose section has no yield
//!   strength falls back to the simple check.
//! - **Simple** ([`SIMPLE`]): tension against the section's allowable
//!   tension, else area × yield; compression against the allowable
//!   compression (else area × yield), but not above the Euler load
//!   π²EI/(kL)² (I: `i_min`, kL: the member's `lk`, else its effective
//!   length factor × its length) — no column curve, no imperfections, no
//!   connections: **not a design code, unsafe compared with design
//!   codes**.
//!
//! Either way the member's `capacity_tension` and `capacity_compression`
//! replace the computed ones (the compression one buckling included).
//! - **Frames**: linear interaction N/N_c + M_y/M_y,c + M_z/M_z,c with the
//!   largest end moments (the allowable moments, else W × yield) — except
//!   **single angles** (a profile `equal_angle` or `unequal_angle`),
//!   checked for their axial force only, as lattice tower practice checks
//!   them (2026-10-27; [`is_single_angle`]); `[analysis]
//!   single_angle_moments = true` adds their end moments as before.
//! - **Cables**: tension only.
//! - **Tension-only members**: tension as trusses; their compression —
//!   never above their own buckling load, which the solver caps
//!   ([`crate::solve::buckling_loads`]) — against the member's
//!   `capacity_compression`, else the allowable compression, else area ×
//!   yield, without the Euler load again (the solve has taken buckling
//!   into account: a bowed member carries its buckling load and no more,
//!   it does not fail by it). One bowed says so ([`BOWED`]: `bowed:
//!   carries its buckling load 0.83 kN`); one left out as slack (no
//!   buckling load) uses 0 ("slack"). One kept active because the active
//!   set did not settle ([`Solution::kept_active`], the last resort) is
//!   checked as a truss (compression too) and marked [`KEPT_ACTIVE`]:
//!   approximate. One that bends (`bending = true`) is checked as a frame
//!   while it is in the model (its moments too).
//! - **Modelling members** (`check = false`, a tower program's fictitious
//!   members): solved, not checked ([`NOT_CHECKED`], no usage).

use crate::model::{CapacityMethod, MemberKind, Model};
use crate::solve::Solution;

/// The simple check in words, as results and notes say it.
pub const SIMPLE: &str = "simple check (Euler buckling, gross yield): not a design code; \
                          unsafe compared with design codes";

/// The capacity method of `model` in words, for the results' notes: ASCE
/// 10-97 with how many members' capacities rest on assumptions (from
/// `usage`), or [`SIMPLE`].
pub fn capacity_note(model: &Model, usage: &[MemberUsage]) -> String {
    let mut text = capacity_words(model, usage);
    let shifted = usage.iter().filter(|u| u.governs.contains(SHIFTED)).count();
    if shifted > 0 {
        text.push_str(&format!(
            "; {shifted} member(s) with eccentric ends (\"{SHIFTED} …]\": the moment of a shift \
             in the frames at the joint, not in an axial check)"
        ));
    }
    text
}

fn capacity_words(model: &Model, usage: &[MemberUsage]) -> String {
    match model.analysis.capacity {
        CapacityMethod::Simple => format!("member capacities: {SIMPLE}"),
        CapacityMethod::Asce10 | CapacityMethod::Sia263 => {
            let assumed = usage.iter().filter(|u| u.governs.contains(ASSUMED)).count();
            let simple = usage
                .iter()
                .filter(|u| u.governs.contains(NO_YIELD))
                .count();
            let mut text = if model.analysis.capacity == CapacityMethod::Sia263 {
                format!(
                    "member capacities: {} (Construction en acier; design values with γ_M1 {}, \
                     γ_M2 {})",
                    crate::sia263::TITLE,
                    crate::sia263::GAMMA_M1,
                    crate::sia263::GAMMA_M2
                )
            } else {
                "member capacities: ASCE 10-97 (Design of Latticed Steel Transmission Structures)"
                    .to_owned()
            };
            if assumed > 0 {
                text.push_str(&format!(
                    "; {assumed} member(s) with data assumed on the safe side (\"{ASSUMED} …]\" in \
                     what governs)"
                ));
            }
            let not_safe = usage.iter().filter(|u| not_safe(&u.governs)).count();
            if not_safe > 0 {
                text.push_str(&format!(
                    "; {not_safe} member(s) NOT ON THE SAFE SIDE: part of their check not done \
                     (\"{NOT_SAFE} …]\" in what governs says which; their usage may be too low)"
                ));
            }
            let joints = usage.iter().filter(|u| u.governs.contains(JOINTS)).count();
            if joints > 0 {
                text.push_str(&format!(
                    "; {joints} member(s) with end conditions from their connections (\"{JOINTS} \
                     …]\")"
                ));
            }
            if simple > 0 {
                text.push_str(&format!(
                    "; {simple} member(s) without a yield strength: {SIMPLE}"
                ));
            }
            text
        }
    }
}

/// The mark before what a capacity method assumed for a member's missing
/// design data, in what governs: `[assumed: length ratios 1, …]`.
pub const ASSUMED: &str = "[assumed:";

/// The mark before what a member's check could not do, so that its usage
/// may be too low (owner 2026-11-08: "stay on the safe side and … explain
/// why and where"): `[not on the safe side: connection not checked at N4
/// (no bolts given), …]`. A member with it never passes plainly: the
/// views show it apart ([`not_safe`]).
pub const NOT_SAFE: &str = "[not on the safe side:";

/// Whether what governs a member says its check is not on the safe side
/// ([`NOT_SAFE`]).
pub fn not_safe(governs: &str) -> bool {
    governs.contains(NOT_SAFE)
}

/// Whether a capacity method's note on a member is a part it could not
/// check (its usage then possibly too low), not an assumption on the safe
/// side.
fn unchecked_note(note: &str) -> bool {
    note.contains("not checked")
}

/// A single angle's (or lattice member's) end moments are left out of
/// its check unless they would add at least this share of its capacity
/// (1 %): above it the member says what they would add.
pub const ANGLE_MOMENTS_SAID: f64 = 0.01;

/// The mark before the conditions a member's ends take from their
/// connections, per end, in what governs: `[joints: N2 eccentric (worked
/// out: one leg bolted), restrained (given); N4 …]`
/// ([`crate::asce10::Capacity::joints`]).
pub const JOINTS: &str = "[joints:";

/// The mark before a member's eccentric ends (connection shifts, rigid
/// end offsets in the solve: [`crate::offsets`]), in what governs:
/// `[shifted: N2 e 15 mm, 1.20 kN m to the joint's frame members]`.
pub const SHIFTED: &str = "[shifted:";

/// The mark of a member checked by the simple check under a design code
/// because its section has no yield strength.
pub const NO_YIELD: &str = "no yield strength: simple check";

/// Usage of one member.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberUsage {
    /// Member (index).
    pub member: usize,
    /// Share of the capacity used (1 = 100 %); none: the section gives no
    /// capacity.
    pub usage: Option<f64>,
    /// What governs, in words.
    pub governs: String,
}

/// The mark on what governs a tension-only member kept active because the
/// active set did not settle ([`Solution::kept_active`]): its usage is
/// that of a truss, approximate.
pub const KEPT_ACTIVE: &str = "kept active (did not settle)";

/// The mark on what governs a bowed tension-only member
/// ([`crate::MemberForces::bowed`]), followed by its buckling load:
/// `bowed: carries its buckling load 0.83 kN`.
pub const BOWED: &str = "bowed: carries its buckling load";

/// What governs a member not design-checked (`check = false`: a
/// modelling member): no usage.
pub const NOT_CHECKED: &str = "not checked (modelling member)";

/// The largest bending moments along a member, about its local y and z
/// (N·m), between its joints (2026-11-08, owner: "stay on the safe
/// side"): from its end forces and the load along it they include (a
/// frame's self-weight), the moment at every point x — M_y(x) = M_y,i +
/// x F_z,i + q_z x² / 2, M_z(x) = M_z,i − x F_y,i − q_y x² / 2 — its
/// largest at an end or where the shear is zero; plus, for the load put
/// on its joints as halves (member wind, ice, a truss's self-weight),
/// q L² / 8 (a simply supported member's: on the safe side, whatever its
/// ends do).
pub fn moments_along(f: &crate::solve::MemberForces) -> [f64; 2] {
    let e = &f.end_forces;
    let l = f.length;
    let largest = |m0: f64, v: f64, q: f64| -> f64 {
        let at = |x: f64| (m0 + v * x + q * x * x / 2.0).abs();
        let mut best = at(0.0).max(at(l));
        if q.abs() > 0.0 {
            let x = -v / q;
            if x > 0.0 && x < l {
                best = best.max(at(x));
            }
        }
        best
    };
    let my = largest(e[4], e[2], f.span_load[2]) + f.lumped_load[2].abs() * l * l / 8.0;
    let mz = largest(e[5], -e[1], -f.span_load[1]) + f.lumped_load[1].abs() * l * l / 8.0;
    [my, mz]
}

/// The compression a member resists by its design (N): its
/// `capacity_compression` given, else its section's allowable
/// compression, else the model's capacity method's (ASCE 10-97 or SIA
/// 263, its crossing partner taken as not bracing it: the lower); none
/// with the simple check or without a yield strength.
pub fn design_compression(model: &Model, member: usize) -> Option<f64> {
    let m = &model.members[member];
    let s = &model.sections[m.section];
    if let Some(c) = m.capacity_compression.or(s.compression) {
        return Some(c);
    }
    s.yield_strength?;
    match model.analysis.capacity {
        CapacityMethod::Asce10 => crate::asce10::capacity(model, member, Some(false))
            .compression
            .map(|c| c.0),
        CapacityMethod::Sia263 => crate::sia263::capacity(model, member, Some(false))
            .0
            .compression
            .map(|c| c.0),
        CapacityMethod::Simple => None,
    }
}

/// Euler buckling load of a member, N (none without a second moment).
pub fn euler_load(model: &Model, member: usize) -> Option<f64> {
    let m = &model.members[member];
    let s = &model.sections[m.section];
    let kl = match m.lk {
        Some(lk) => lk,
        None => m.k * model.length(m),
    };
    (s.i_min > 0.0).then(|| std::f64::consts::PI.powi(2) * s.e * s.i_min / (kl * kl))
}

/// Usage of every member; `strength`: the load case's strength factor.
pub fn member_usage(model: &Model, solution: &Solution, strength: f64) -> Vec<MemberUsage> {
    member_usage_with(model, solution, strength, None)
}

/// Usage of every member with the capacities `given` by a method outside
/// the core (a plugin's `member-capacity` export, 2026-10-27; one per
/// member, in order; [`crate::capacity::capacity_inputs`] what it
/// received): they replace the model's own method for every member they
/// give a capacity (a member's `capacity_tension` and
/// `capacity_compression` still win; one they give none falls back to
/// the simple check); their notes are said per member as assumptions.
/// None: the model's own method ([`member_usage`]).
pub fn member_usage_with(
    model: &Model,
    solution: &Solution,
    strength: f64,
    given: Option<&[crate::asce10::Capacity]>,
) -> Vec<MemberUsage> {
    let crossings = unbraced_crossings(model);
    (0..model.members.len())
        .map(|idx| {
            usage_of(
                model,
                solution,
                idx,
                &solution.members[idx],
                strength,
                given.map(|g| g.get(idx).cloned().unwrap_or_default()),
            )
        })
        .map(|mut u| {
            if solution.kept_active.contains(&u.member) {
                u.governs = if u.governs.is_empty() {
                    KEPT_ACTIVE.to_owned()
                } else {
                    format!("{} — {KEPT_ACTIVE}", u.governs)
                };
            }
            // Its eccentric ends (connection shifts): where their moment
            // went, or that it was left out.
            let n = solution.members.get(u.member).map_or(0.0, |f| f.axial);
            // A half of a diagonal through a crossing nothing else holds
            // out of plane, in compression: its buckling length may be
            // the whole diagonal's (2026-11-08).
            if n < 0.0
                && let Some(j) = &crossings[u.member]
                && u.usage.is_some()
            {
                u.governs.push_str(&format!(
                    " {NOT_SAFE} buckling length its own: the crossing at {j} is held out of its \
                     plane only by the other diagonal (unless that one is in tension, the length \
                     is the whole diagonal's; give lk, or the crossing partner)]"
                ));
            }
            if let Some(w) = crate::offsets::eccentricity_words(model, u.member, n) {
                u.governs = if u.governs.is_empty() {
                    format!("{SHIFTED} {w}]")
                } else {
                    format!("{} {SHIFTED} {w}]", u.governs)
                };
            }
            u
        })
        .collect()
}

/// Per member, the joint where it is a half of a diagonal through a
/// crossing that only the other diagonal holds out of the plane: a joint
/// without supports or springs where exactly four members meet, as two
/// straight lines (within 1°) — the member one of them — when the member
/// gives no buckling length of its own (`lk`, `k` other than 1) and no
/// crossing partner. Its check takes its own length between joints; out
/// of the plane it may buckle over the whole diagonal: said, not on the
/// safe side.
pub fn unbraced_crossings(model: &Model) -> Vec<Option<String>> {
    let mut at: Vec<Vec<usize>> = vec![Vec::new(); model.nodes.len()];
    for (k, m) in model.members.iter().enumerate() {
        at[m.i].push(k);
        at[m.j].push(k);
    }
    let dir = |k: usize, from: usize| -> [f64; 3] {
        let m = &model.members[k];
        let other = if m.i == from { m.j } else { m.i };
        let (a, b) = (model.nodes[from].point, model.nodes[other].point);
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        d.map(|x| x / n.max(1e-12))
    };
    let opposite = |a: [f64; 3], b: [f64; 3]| {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2] < -(1.0f64.to_radians().cos())
    };
    (0..model.members.len())
        .map(|k| {
            let m = &model.members[k];
            if m.lk.is_some() || (m.k - 1.0).abs() > 1e-12 || m.design.crossing.is_some() {
                return None;
            }
            [m.i, m.j].into_iter().find_map(|n| {
                let node = &model.nodes[n];
                let held = node.fixed.iter().any(|f| *f) || node.spring.iter().any(|s| *s > 0.0);
                let here = &at[n];
                if held || here.len() != 4 {
                    return None;
                }
                let mine = dir(k, n);
                // Its continuation through the joint, and the other two
                // in line with each other.
                let cont = here
                    .iter()
                    .find(|&&o| o != k && opposite(mine, dir(o, n)))?;
                let rest: Vec<usize> = here
                    .iter()
                    .copied()
                    .filter(|&o| o != k && o != *cont)
                    .collect();
                (rest.len() == 2 && opposite(dir(rest[0], n), dir(rest[1], n)))
                    .then(|| node.id.clone())
            })
        })
        .collect()
}

/// Usage of member `idx` under forces `f` (its own, or a piece's).
fn usage_of(
    model: &Model,
    solution: &Solution,
    idx: usize,
    f: &crate::solve::MemberForces,
    strength: f64,
    given: Option<crate::asce10::Capacity>,
) -> MemberUsage {
    {
        {
            let m = &model.members[idx];
            let s = &model.sections[m.section];
            if !m.check {
                // A modelling member: solved, not checked.
                return MemberUsage {
                    member: idx,
                    usage: None,
                    governs: NOT_CHECKED.into(),
                };
            }
            if m.kind == MemberKind::Tension && f.slack {
                return MemberUsage {
                    member: idx,
                    usage: Some(0.0),
                    governs: "slack".into(),
                };
            }
            let n = f.axial;
            let kn = |x: f64| x / 1000.0;
            // The design method's capacities (none: the simple check).
            let external = given.is_some();
            let partner = m
                .design
                .crossing
                .map(|p| solution.members.get(p).is_some_and(|f| f.axial > 0.0));
            let mut stability = None;
            let asce = match given {
                Some(g) => Some(g),
                None if s.yield_strength.is_none() => None,
                None => match model.analysis.capacity {
                    CapacityMethod::Asce10 => Some(crate::asce10::capacity(model, idx, partner)),
                    CapacityMethod::Sia263 => {
                        let (c, st) = crate::sia263::capacity(model, idx, partner);
                        stability = st;
                        Some(c)
                    }
                    CapacityMethod::Simple => None,
                },
            };
            let mut assumed: &[String] = &[];
            let mut joints: &[String] = &[];
            let mut extra = String::new();
            if !external
                && model.analysis.capacity != CapacityMethod::Simple
                && s.yield_strength.is_none()
            {
                extra = format!(" ({NO_YIELD})");
            }
            // Axial: (ratio, words).
            let axial: Option<(f64, String)> = if n >= 0.0 {
                match (
                    m.capacity_tension,
                    asce.as_ref().and_then(|c| c.tension.as_ref()),
                ) {
                    (Some(cap), _) => Some((
                        n / (strength * cap),
                        format!("tension {:.1} kN of {:.1} kN (given)", kn(n), kn(cap)),
                    )),
                    (None, Some((cap, what))) if m.kind != MemberKind::Cable => {
                        assumed = asce.as_ref().map_or(&[][..], |c| &c.tension_assumed[..]);
                        joints = asce.as_ref().map_or(&[][..], |c| &c.joints[..]);
                        Some((
                            n / (strength * cap),
                            format!("tension {:.1} kN of {:.1} kN ({what})", kn(n), kn(*cap)),
                        ))
                    }
                    _ => s
                        .tension
                        .or(s.yield_strength.map(|y| y * s.area))
                        .map(|cap| {
                            (
                                n / (strength * cap),
                                format!("tension {:.1} kN of {:.1} kN", kn(n), kn(cap)),
                            )
                        }),
                }
            } else if m.kind == MemberKind::Cable {
                Some((0.0, "slack".into()))
            } else if m.kind == MemberKind::Tension && !solution.kept_active.contains(&idx) {
                // Buckling is in the solve (the cap): no Euler term.
                let bowed = if f.bowed {
                    format!("{BOWED} {:.2} kN — ", kn(-n))
                } else {
                    String::new()
                };
                let cap = m.capacity_compression.map(|c| (c, " (given)")).or(s
                    .compression
                    .or(s.yield_strength.map(|y| y * s.area))
                    .map(|c| (c, "")));
                let Some((cap, given)) = cap else {
                    // No capacity: no usage, but a bowed member says so.
                    return MemberUsage {
                        member: idx,
                        usage: None,
                        governs: bowed.trim_end_matches(" — ").to_owned(),
                    };
                };
                Some((
                    -n / (strength * cap),
                    format!(
                        "{bowed}compression {:.1} kN of {:.1} kN{given}",
                        kn(-n),
                        kn(cap)
                    ),
                ))
            } else if let Some(cap) = m.capacity_compression {
                Some((
                    -n / (strength * cap),
                    format!("compression {:.1} kN of {:.1} kN (given)", kn(-n), kn(cap)),
                ))
            } else if let Some(c) = &asce
                && let Some((cap, what)) = &c.compression
            {
                assumed = &c.compression_assumed;
                joints = &c.joints;
                if let Some(l) = &c.limit {
                    extra = format!("; {l}");
                }
                Some((
                    -n / (strength * cap),
                    format!(
                        "compression {:.1} kN of {:.1} kN ({what})",
                        kn(-n),
                        kn(*cap)
                    ),
                ))
            } else {
                let squash = s.compression.or(s.yield_strength.map(|y| y * s.area));
                let euler = euler_load(model, idx);
                let cap = match (squash, euler) {
                    (Some(a), Some(b)) => Some(if b < a { (b, true) } else { (a, false) }),
                    (Some(a), None) => Some((a, false)),
                    (None, Some(b)) => Some((b, true)),
                    (None, None) => None,
                };
                cap.map(|(cap, buckling)| {
                    (
                        -n / (strength * cap),
                        format!(
                            "compression {:.1} kN of {:.1} kN{}",
                            kn(-n),
                            kn(cap),
                            if buckling { " (buckling)" } else { "" }
                        ),
                    )
                })
            };
            let mut suffix = extra.clone();
            if !joints.is_empty() {
                suffix.push_str(&format!(" {JOINTS} {}]", joints.join("; ")));
            }
            // What the method assumed on the safe side, and what it could
            // not check (its usage then possibly too low).
            let (unchecked, safe): (Vec<&String>, Vec<&String>) =
                assumed.iter().partition(|n| unchecked_note(n));
            if !safe.is_empty() {
                let safe: Vec<&str> = safe.iter().map(|n| n.as_str()).collect();
                suffix.push_str(&format!(" {ASSUMED} {}]", safe.join(", ")));
            }
            let mut not_safe_notes: Vec<String> = unchecked.into_iter().cloned().collect();
            // A single angle's end moments, left out of its check (ASCE
            // 10-97's curves take the connection's eccentricity), said when
            // they would add to its usage.
            // Likewise any member checked for its axial force alone: its
            // bending between its joints from the loads along it (its own
            // weight, member wind, ice), said when it would add.
            let axial_only =
                !m.is_frame() || (axial_member(m, s) && !model.analysis.single_angle_moments);
            if axial_only && m.kind != MemberKind::Cable {
                let [my, mz] = moments_along(f);
                let fy = s.yield_strength;
                let cap_y = s.moment_y.or(s.wy.zip(fy).map(|(w, y)| w * y));
                let cap_z = s.moment_z.or(s.wz.zip(fy).map(|(w, y)| w * y));
                let add = cap_y.map_or(0.0, |c| my / (strength * c))
                    + cap_z.map_or(0.0, |c| mz / (strength * c));
                if add >= ANGLE_MOMENTS_SAID {
                    not_safe_notes.push(format!(
                        "bending not checked (axial check{}): it would add {:.0} % (My {:.2}, Mz \
                         {:.2} kN m along it)",
                        if m.is_frame() {
                            ", single_angle_moments off"
                        } else {
                            ": a truss member"
                        },
                        add * 100.0,
                        kn(my),
                        kn(mz)
                    ));
                }
            }
            if !external {
                not_safe_notes.extend(uncovered_clauses(model, idx, f, strength));
            }
            if !not_safe_notes.is_empty() {
                suffix.push_str(&format!(" {NOT_SAFE} {}]", not_safe_notes.join(", ")));
            }
            // SIA 263: compression with the bending of an eccentric
            // connection, class 4 and frames by the stability check
            // (formulas 50, 62, 63), the larger of it and the axial one
            // (the connection's).
            if let Some(st) = &stability {
                let frame =
                    m.is_frame() && !(axial_member(m, s) && !model.analysis.single_angle_moments);
                let given_cap = if n < 0.0 {
                    m.capacity_compression.is_some()
                } else {
                    m.capacity_tension.is_some()
                };
                if !given_cap && (frame || (n < 0.0 && (st.eccentric || st.class4))) {
                    let moments = if frame {
                        // A moment larger between the joints: as a uniform
                        // moment that large (on the safe side).
                        let mut mm = crate::sia263::frame_moments(st, &f.end_forces);
                        let [my, mz] = moments_along(f);
                        for (k, a) in st.axes.iter().enumerate() {
                            let along = match a.name {
                                "y" => my,
                                "z" => mz,
                                _ => 0.0,
                            };
                            if along > mm[k][0].abs().max(mm[k][1].abs()) + 1e-9 {
                                mm[k] = [along, along];
                            }
                        }
                        mm
                    } else {
                        [[0.0; 2]; 2]
                    };
                    let (u, words) = st.usage(n, moments, strength);
                    let (u, words) = match &axial {
                        Some((ua, wa)) if *ua > u || u.is_nan() && !ua.is_nan() => {
                            (*ua, wa.clone())
                        }
                        _ => (u, words),
                    };
                    let mut words = words;
                    words.push_str(&suffix);
                    return MemberUsage {
                        member: idx,
                        usage: Some(u),
                        governs: words,
                    };
                }
            }
            let axial = axial.map(|(u, mut words)| {
                words.push_str(&suffix);
                (u, words)
            });
            if !m.is_frame() || (axial_member(m, s) && !model.analysis.single_angle_moments) {
                return MemberUsage {
                    member: idx,
                    usage: axial.as_ref().map(|a| a.0),
                    governs: axial.map(|a| a.1).unwrap_or_default(),
                };
            }
            // The largest moments along it, not only at its ends.
            let [my, mz] = moments_along(f);
            let fy = s.yield_strength;
            let cap_y = s.moment_y.or(s.wy.zip(fy).map(|(w, y)| w * y));
            let cap_z = s.moment_z.or(s.wz.zip(fy).map(|(w, y)| w * y));
            let mut parts: Vec<(f64, String)> = Vec::new();
            if let Some(a) = axial {
                parts.push(a);
            }
            if let Some(c) = cap_y {
                parts.push((
                    my / (strength * c),
                    format!("My {:.1} kN m of {:.1}", kn(my), kn(c)),
                ));
            }
            if let Some(c) = cap_z {
                parts.push((
                    mz / (strength * c),
                    format!("Mz {:.1} kN m of {:.1}", kn(mz), kn(c)),
                ));
            }
            if parts.is_empty() {
                return MemberUsage {
                    member: idx,
                    usage: None,
                    governs: String::new(),
                };
            }
            let usage = parts.iter().map(|p| p.0).sum();
            let governs = if parts.len() == 1 {
                parts[0].1.clone()
            } else {
                parts
                    .iter()
                    .map(|p| format!("{} ({:.0} %)", p.1, p.0 * 100.0))
                    .collect::<Vec<_>>()
                    .join(" + ")
            };
            MemberUsage {
                member: idx,
                usage: Some(usage),
                governs,
            }
        }
    }
}

/// The capacity method's clauses that apply to member `idx` under its
/// forces `f` but are not covered by the program (2026-11-08): each named,
/// for the member's `[not on the safe side: …]`. Under ASCE 10-97 and
/// SIA 263 only (the simple check says it is no design code; a design
/// code's capacities given are the code's); a resistance given on the
/// member (`capacity_compression`) covers the compression clauses.
///
/// - a compressed tube, channel or I section under ASCE 10-97: its
///   elements' effective widths (3.9.3) are not applied;
/// - a compressed built-up member (two angles, a star or cross of angles,
///   two channels): its stitch bolts or battens (ASCE 10-97 3.11, SIA 263
///   5.5), each part buckling between them;
/// - a frame checked with its moments under ASCE 10-97: compression with
///   bending by the linear sum, without the amplification and C_m (3.12,
///   3.13); an I section, channel or two channels bent about its major
///   axis: lateral-torsional buckling (3.14);
/// - a frame under either: its shear and torsion (ASCE 10-97 3.15, SIA
///   263 5.1.4, 5.1.7), when the shear reaches 1 % of A·f_y/√3 (the
///   torsion given with it).
///
/// Each only above 1 % of the member's capacity where a share can be
/// worked out (the moments, the shear), as the moments' notes.
fn uncovered_clauses(
    model: &Model,
    idx: usize,
    f: &crate::solve::MemberForces,
    strength: f64,
) -> Vec<String> {
    use ohl_sections::Shape;
    let m = &model.members[idx];
    let s = &model.sections[m.section];
    let method = model.analysis.capacity;
    if method == CapacityMethod::Simple || m.kind == MemberKind::Cable {
        return Vec::new();
    }
    let asce = method == CapacityMethod::Asce10;
    let shape = s.profile.as_ref().map(|p| &p.shape);
    let compressed = f.axial < 0.0 && m.capacity_compression.is_none();
    let mut out = Vec::new();
    if compressed
        && asce
        && matches!(
            shape,
            Some(
                Shape::Channel { .. }
                    | Shape::ISection { .. }
                    | Shape::RectangularHollow { .. }
                    | Shape::CircularHollow { .. }
                    | Shape::PolygonalHollow { .. }
                    | Shape::DoubleChannel { .. }
            )
        )
    {
        out.push("effective widths of its elements not applied (ASCE 10-97 3.9.3)".to_owned());
    }
    if compressed
        && matches!(
            shape,
            Some(
                Shape::DoubleAngle { .. }
                    | Shape::StarAngle { .. }
                    | Shape::CrossAngle { .. }
                    | Shape::DoubleChannel { .. }
            )
        )
    {
        out.push(if asce {
            "stitch bolts of the built-up member not checked (ASCE 10-97 3.11)".to_owned()
        } else {
            "battens and connectors of the built-up member not checked (SIA 263 5.5)".to_owned()
        });
    }
    let frame = m.is_frame() && !(axial_member(m, s) && !model.analysis.single_angle_moments);
    if !frame {
        return out;
    }
    let fy = s.yield_strength;
    let [my, mz] = moments_along(f);
    let cap_y = s.moment_y.or(s.wy.zip(fy).map(|(w, y)| w * y));
    let cap_z = s.moment_z.or(s.wz.zip(fy).map(|(w, y)| w * y));
    let share = |mo: f64, cap: Option<f64>| cap.map_or(0.0, |c| mo / (strength * c));
    if asce {
        if f.axial < 0.0 && share(my, cap_y) + share(mz, cap_z) >= ANGLE_MOMENTS_SAID {
            out.push(
                "compression with bending by the linear sum, without the amplification and C_m \
                 (ASCE 10-97 3.12, 3.13)"
                    .to_owned(),
            );
        }
        if matches!(
            shape,
            Some(Shape::ISection { .. } | Shape::Channel { .. } | Shape::DoubleChannel { .. })
        ) && share(my, cap_y) >= ANGLE_MOMENTS_SAID
        {
            out.push("lateral-torsional buckling not checked (ASCE 10-97 3.14)".to_owned());
        }
    }
    let e = &f.end_forces;
    let shear = e[1].hypot(e[2]).max(e[7].hypot(e[8]));
    let torsion = e[3].abs().max(e[9].abs());
    if let Some(fy) = fy {
        let cap = s.area * fy / 3f64.sqrt();
        if cap > 0.0 && shear / (strength * cap) >= ANGLE_MOMENTS_SAID {
            out.push(format!(
                "shear {:.1} kN and torsion {:.2} kN m not checked ({})",
                shear / 1000.0,
                torsion / 1000.0,
                if asce {
                    "ASCE 10-97 3.15"
                } else {
                    "SIA 263 5.1.4, 5.1.7"
                }
            ));
        }
    }
    out
}

/// Whether a section is a single angle: its profile an `equal_angle` or
/// `unequal_angle` of the section library. A frame of such a section is
/// checked for its axial force only unless `[analysis]
/// single_angle_moments` (the end moments of a lattice tower's angles
/// modelled as frames are secondary: tower practice designs them axially,
/// the connections' eccentricity in the effective slenderness).
pub fn is_single_angle(s: &crate::model::Section) -> bool {
    matches!(
        s.profile.as_ref().map(|p| &p.shape),
        Some(ohl_sections::Shape::EqualAngle { .. } | ohl_sections::Shape::UnequalAngle { .. })
    )
}

/// Whether a frame member is checked for its axial force only (unless
/// `[analysis] single_angle_moments`): a single angle
/// ([`is_single_angle`]), or a lattice member — one whose design data
/// give it a class (leg, bracing, redundant, crossing diagonal: built-up
/// angles too), which lattice tower practice designs axially.
pub fn axial_member(m: &crate::model::Member, s: &crate::model::Section) -> bool {
    is_single_angle(s) || m.design.class.is_some()
}

/// The member using most of its capacity (a usage that is not a number
/// ranks above every other, [`usage_order`]).
pub fn worst(usage: &[MemberUsage]) -> Option<&MemberUsage> {
    usage
        .iter()
        .filter(|u| u.usage.is_some())
        .max_by(|a, b| usage_order(a.usage.unwrap_or(0.0), b.usage.unwrap_or(0.0)))
}

/// Usage at or above this share (and up to 100 %) is shown as near full
/// (a warning, orange) by both applications and the reports. A display
/// threshold, not a design limit.
pub const NEAR_FULL: f64 = 0.9;

/// What a usage comes to, one rule for every check, view and report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageClass {
    /// Below [`NEAR_FULL`].
    Pass,
    /// From [`NEAR_FULL`] to 100 %.
    Near,
    /// Over 100 %, not allowed (infinite), or not a number (code review
    /// 2026-10-26: a usage that is not a number never passes).
    Over,
    /// Within its capacity, but part of the check not done
    /// ([`NOT_SAFE`]): its usage may be too low — never a plain pass
    /// (owner 2026-11-08).
    NotSafe,
}

/// The class of usage `u` (1 = 100 %): over above 1 and when not a
/// number, near from [`NEAR_FULL`], else pass.
pub fn classify_usage(u: f64) -> UsageClass {
    if u.is_nan() || u > 1.0 {
        UsageClass::Over
    } else if u >= NEAR_FULL {
        UsageClass::Near
    } else {
        UsageClass::Pass
    }
}

/// The class of usage `u` of what `governs` says: as [`classify_usage`],
/// but a usage within its capacity whose check is not on the safe side
/// ([`not_safe`]) is [`UsageClass::NotSafe`], never a pass.
pub fn classify(u: f64, governs: &str) -> UsageClass {
    match classify_usage(u) {
        UsageClass::Over => UsageClass::Over,
        _ if not_safe(governs) => UsageClass::NotSafe,
        c => c,
    }
}

/// Orders two usages, a usage that is not a number above every other
/// (whatever its sign bit), so that the worst of a list is never a pass
/// hiding a failed computation.
pub fn usage_order(a: f64, b: f64) -> std::cmp::Ordering {
    let key = |u: f64| if u.is_nan() { f64::INFINITY } else { u };
    key(a)
        .total_cmp(&key(b))
        .then_with(|| a.is_nan().cmp(&b.is_nan()))
}

#[cfg(test)]
mod usage_class_tests {
    use super::*;

    #[test]
    fn usages_are_classed_and_ordered_nan_safe() {
        assert_eq!(classify_usage(0.5), UsageClass::Pass);
        assert_eq!(classify_usage(NEAR_FULL), UsageClass::Near);
        assert_eq!(classify_usage(1.0), UsageClass::Near);
        assert_eq!(classify_usage(1.0001), UsageClass::Over);
        assert_eq!(classify_usage(f64::INFINITY), UsageClass::Over);
        assert_eq!(classify_usage(f64::NAN), UsageClass::Over);
        assert_eq!(classify_usage(-f64::NAN), UsageClass::Over);
        // NaN (either sign) above everything, infinity included.
        let mut v = [0.3, -f64::NAN, 2.0, f64::INFINITY, 0.9];
        v.sort_by(|a, b| usage_order(*a, *b));
        assert!(v[4].is_nan() && v[3] == f64::INFINITY && v[0] == 0.3);
        let members = [
            MemberUsage {
                member: 0,
                usage: Some(0.4),
                governs: String::new(),
            },
            MemberUsage {
                member: 1,
                usage: Some(-f64::NAN),
                governs: String::new(),
            },
            MemberUsage {
                member: 2,
                usage: Some(3.0),
                governs: String::new(),
            },
        ];
        assert_eq!(worst(&members).map(|m| m.member), Some(1));
    }
}
