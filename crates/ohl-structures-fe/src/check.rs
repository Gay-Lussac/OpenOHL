// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL ontributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Member usage: the member forces of a [`Solution`] against the section's
//! capacity times the load case's strength factor.
//!
//! The capacities come from the model's capacity method
//! (`[analysis] capacity`, [`CapacityMethod`], 2026-10-27):
//!
//! - **A design code** ([`CapacityMethod::Code`]): ASCE 10-97 by default,
//!   SIA 263:2013 or another plugin's method — since 2026-11-18 every
//!   design code is a plugin's (the bundled `asce10` and `sia263`,
//!   [`Model::capacity_plugin`]): the core runs no plugins, so a model
//!   whose code no runner checked reports every member not checked
//!   ([`PLUGIN_NOT_RUN`]) — never another method in its place. The runners
//!   (`ohl-member-check`: OHL-Structure, `ohl structures fe`, the line's
//!   structure check) replace the usage with the plugin's.
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
//!   ([`crate::solve::buckling_loads`]; none at all when the model says
//!   `[analysis] tension_compression = "none"`, owner 2026-11-15: then
//!   every shortened one is slack, "slack (no compression)") — against
//!   the member's
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
//! - **Optional, off by default** (owner 2026-11-15): redundant members
//!   also for a share of the compression in the member they brace
//!   (`[analysis] redundant_force`, [`redundant_supports`],
//!   [`REDUNDANT_FORCE`]); flat members for a climbing load alone
//!   (`[analysis] climbing_load`, [`climbing_check`]: a list of its own,
//!   not in the usage).

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
    text.push_str(&shifted_note(usage));
    text
}

/// The members with eccentric ends, counted, for a capacity note ("" none).
pub fn shifted_note(usage: &[MemberUsage]) -> String {
    let shifted = usage.iter().filter(|u| u.governs.contains(SHIFTED)).count();
    if shifted > 0 {
        format!(
            "; {shifted} member(s) with eccentric ends (\"{SHIFTED} …]\": the moment of a shift \
             in the frames at the joint, not in an axial check)"
        )
    } else {
        String::new()
    }
}

fn capacity_words(model: &Model, usage: &[MemberUsage]) -> String {
    match model.analysis.capacity {
        CapacityMethod::Simple => format!("member capacities: {SIMPLE}"),
        CapacityMethod::Code if usage.iter().any(|u| u.governs.starts_with(PLUGIN_NOT_RUN)) => {
            format!(
                "member capacities: {PLUGIN_NOT_RUN} ({}); no other method was used in its place",
                code_name(model)
            )
        }
        // Capacities given from outside (a `member-capacity` plugin).
        CapacityMethod::Code => {
            let mut text = format!("member capacities: {}", code_name(model));
            text.push_str(&marks_note(usage));
            text
        }
    }
}

/// The design code a model's members are checked by, as its file names it
/// (the default: [`crate::model::DEFAULT_METHOD`]).
fn code_name(model: &Model) -> String {
    model
        .capacity_plugin
        .as_ref()
        .map_or_else(|| crate::model::DEFAULT_METHOD.to_owned(), |p| p.name())
}

/// What the marks in what governs the members say, counted, for a design
/// code's note (its own or a plugin's; 2026-11-18: one wording for
/// both): the members with data assumed on the safe side, not on the safe
/// side, with end conditions from their connections, and without a yield
/// strength; "" none.
pub fn marks_note(usage: &[MemberUsage]) -> String {
    let mut text = String::new();
    let assumed = usage.iter().filter(|u| u.governs.contains(ASSUMED)).count();
    let simple = usage
        .iter()
        .filter(|u| u.governs.contains(NO_YIELD))
        .count();
    if assumed > 0 {
        text.push_str(&format!(
            "; {assumed} member(s) with data assumed on the safe side (\"{ASSUMED} …]\" in what \
             governs)"
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
            "; {joints} member(s) with end conditions from their connections (\"{JOINTS} …]\")"
        ));
    }
    if simple > 0 {
        text.push_str(&format!(
            "; {simple} member(s) without a yield strength: {SIMPLE}"
        ));
    }
    text
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
/// ([`crate::capacity::Capacity::joints`]).
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
/// What governs a slack tension-only member of a model whose tension-only
/// members carry no compression (`[analysis] tension_compression =
/// "none"`; 2026-11-15).
pub const SLACK_NO_COMPRESSION: &str = "slack (no compression)";

/// What governs a member not design-checked (`check = false`: a
/// modelling member): no usage.
pub const NOT_CHECKED: &str = "not checked (modelling member)";

/// Said for every checked member of a model whose file asks for a
/// plugin's member check when no plugin was run (the core runs none;
/// 2026-11-11).
pub const PLUGIN_NOT_RUN: &str =
    "NOT CHECKED, NOT ON THE SAFE SIDE: the member check is a plugin's, not run here";

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
/// compression, else the model's design code's: the compression capacity
/// its runner asked of the code's plugin ([`Model::method_compression`],
/// 2026-11-18; ASCE 10-97, the default, its crossing partner taken as not
/// bracing it); none with the simple check, without a yield strength, or
/// when no runner asked (a model solved without its code's plugin: the
/// Euler load alone caps, as for every other check the core cannot make).
pub fn design_compression(model: &Model, member: usize) -> Option<f64> {
    let m = &model.members[member];
    let s = &model.sections[m.section];
    if let Some(c) = m.capacity_compression.or(s.compression) {
        return Some(c);
    }
    s.yield_strength?;
    match model.analysis.capacity {
        // The code's own compression, as its runner asked its plugin —
        // never another code's in its place.
        CapacityMethod::Code => model.method_compression.get(member).copied().flatten(),
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

#[cfg(test)]
thread_local! {
    /// How often this thread made the member check ([`member_usage_with`]):
    /// once per solve, after it, never inside its iterations (tested).
    pub(crate) static CHECKS_MADE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The cross-section properties of the profiles worked out on this thread,
/// by profile id, each with its profile (2026-11-15: a profile's
/// properties — its outline's moments, shear centre and warping — were
/// worked out again for every member, every solve and every load case:
/// most of a tower check's time). Found by the whole profile, compared
/// field by field: a profile changed is worked out again.
type Kept =
    std::collections::HashMap<String, Vec<(ohl_sections::Section, ohl_sections::Properties)>>;

/// Profiles kept per thread at most (then the store starts again).
const KEPT_PROFILES: usize = 4096;

thread_local! {
    static PROFILES: std::cell::RefCell<(usize, Kept)> =
        std::cell::RefCell::new((0, Kept::new()));
}

/// The properties of profile `p` ([`ohl_sections::Section::properties`]),
/// worked out once per thread for each profile (the same answer: the same
/// function of the same profile).
pub fn properties(p: &ohl_sections::Section) -> ohl_sections::Properties {
    let kept = PROFILES.with(|c| {
        c.borrow()
            .1
            .get(&p.id)
            .and_then(|v| v.iter().find(|(s, _)| s == p).map(|(_, q)| q.clone()))
    });
    if let Some(q) = kept {
        return q;
    }
    let q = p.properties();
    PROFILES.with(|c| {
        let mut c = c.borrow_mut();
        if c.0 >= KEPT_PROFILES {
            *c = (0, Kept::new());
        }
        c.0 += 1;
        c.1.entry(p.id.clone())
            .or_default()
            .push((p.clone(), q.clone()));
    });
    q
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
    given: Option<&[crate::capacity::Capacity]>,
) -> Vec<MemberUsage> {
    #[cfg(test)]
    CHECKS_MADE.with(|n| n.set(n.get() + 1));
    // The model's design code is a plugin's (every code since 2026-11-18,
    // ASCE 10-97 the default) and none was run here: no member is checked
    // by another method in its place.
    if given.is_none() && model.analysis.capacity == CapacityMethod::Code {
        let name = code_name(model);
        return model
            .members
            .iter()
            .enumerate()
            .map(|(k, m)| MemberUsage {
                member: k,
                usage: None,
                governs: if m.check {
                    format!("{PLUGIN_NOT_RUN} ({name})")
                } else {
                    NOT_CHECKED.to_owned()
                },
            })
            .collect();
    }
    let crossings = unbraced_crossings(model);
    let supports = model
        .analysis
        .redundant_force
        .map(|share| redundant_supports(model, solution, share));
    (0..model.members.len())
        .map(|idx| {
            let given = given.map(|g| g.get(idx).cloned().unwrap_or_default());
            let own = usage_of(
                model,
                solution,
                idx,
                &solution.members[idx],
                strength,
                given.clone(),
            );
            match (&supports, model.analysis.redundant_force) {
                (Some(s), Some(share)) => {
                    with_redundant_force(model, solution, strength, given, share, &s[idx], own)
                }
                _ => own,
            }
        })
        .map(|u| with_remarks(model, solution, &crossings, u))
        .collect()
}

/// The mark before a redundant member's nominal force check in what
/// governs (`[analysis] redundant_force`, owner 2026-11-15): `[redundant
/// force: 2.5 % of 37.0 kN, the compression of L12 at N4 (the member it
/// braces): 0.93 kN, 6 % — own force governs]`.
pub const REDUNDANT_FORCE: &str = "[redundant force:";

/// Two members at a joint continue one line through it — the member a
/// redundant member braces there — when their directions from the joint
/// are opposite within this angle, degrees (a leg's slope changing at a
/// section joint by a few degrees is still the leg).
pub const STRAIGHT_THROUGH_DEG: f64 = 10.0;

/// Where a redundant member's nominal force comes from
/// ([`redundant_supports`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Supported {
    /// The braced member's piece whose compression gives the force
    /// (index).
    pub member: usize,
    /// The joint where the redundant member braces it (index).
    pub joint: usize,
    /// The compression in it, N (≥ 0; 0: in tension, no nominal force):
    /// its own, or — itself a redundant member — its own design force
    /// (the larger of its own compression and its nominal force).
    pub force: f64,
    /// The braced member is itself a redundant member.
    pub via_redundant: bool,
}

/// Per member, for each redundant member ([`MemberClass::Redundant`],
/// checked): what it braces (ASCE 10-97, 1.0: "members that reduce the
/// unbraced length of primary members by providing intermediate support")
/// and the compression there, else why that could not be determined;
/// none for every other member. `share`: `[analysis] redundant_force`
/// (0.025: 2.5 %).
///
/// The braced member is found from the model: at each end joint of the
/// redundant member, the members (other than itself, design-checked)
/// that continue one straight line through the joint (two pieces whose
/// directions from it are opposite within [`STRAIGHT_THROUGH_DEG`]) —
/// the member it supports there. Its force: the larger compression of the
/// two pieces at that joint, in this case. Of every such line at either
/// end the largest force is taken (on the safe side). A line of redundant
/// members (a redundant bracing a redundant) gives that member's own
/// design force — the larger of its own compression and `share` × what it
/// braces in turn (so a sub-redundant takes `share` of a redundant's
/// nominal force, as practice designs the hierarchy); a ring of
/// redundants bracing each other ends at their own forces. No member
/// passing straight through either end joint: not determined, said.
///
/// [`MemberClass::Redundant`]: crate::design::MemberClass::Redundant
pub fn redundant_supports(
    model: &Model,
    solution: &Solution,
    share: f64,
) -> Vec<Option<Result<Supported, String>>> {
    use crate::design::MemberClass;
    let n = model.members.len();
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
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-12);
        d.map(|x| x / l)
    };
    let opposite = -(STRAIGHT_THROUGH_DEG.to_radians().cos());
    // The straight lines through each joint: pairs of design-checked
    // members.
    let lines: Vec<Vec<(usize, usize)>> = (0..model.nodes.len())
        .map(|j| {
            let here: Vec<usize> = at[j]
                .iter()
                .copied()
                .filter(|&k| model.members[k].check)
                .collect();
            let mut out = Vec::new();
            for (x, &a) in here.iter().enumerate() {
                for &b in &here[x + 1..] {
                    let (da, db) = (dir(a, j), dir(b, j));
                    if da[0] * db[0] + da[1] * db[1] + da[2] * db[2] < opposite {
                        out.push((a, b));
                    }
                }
            }
            out
        })
        .collect();
    let redundant = |k: usize| {
        let m = &model.members[k];
        m.check && m.design.class == Some(MemberClass::Redundant)
    };
    let compression = |k: usize| (-solution.members[k].axial).max(0.0);
    // Memoised, with a guard against rings of redundants.
    #[derive(Clone)]
    enum State {
        Todo,
        Busy,
        Done(Result<Supported, String>),
    }
    fn supported(
        k: usize,
        state: &mut Vec<State>,
        lines: &[Vec<(usize, usize)>],
        model: &Model,
        share: f64,
        redundant: &dyn Fn(usize) -> bool,
        compression: &dyn Fn(usize) -> f64,
    ) -> Result<Supported, String> {
        if let State::Done(r) = &state[k] {
            return r.clone();
        }
        state[k] = State::Busy;
        let m = &model.members[k];
        let mut best: Option<Supported> = None;
        for j in [m.i, m.j] {
            for &(a, b) in &lines[j] {
                if a == k || b == k {
                    continue;
                }
                for x in [a, b] {
                    let (force, via) = if redundant(x) {
                        // Its own design force; a ring: its own force.
                        let nominal = match &state[x] {
                            State::Busy => 0.0,
                            _ => supported(x, state, lines, model, share, redundant, compression)
                                .map_or(0.0, |s| share * s.force),
                        };
                        (compression(x).max(nominal), true)
                    } else {
                        (compression(x), false)
                    };
                    if best.as_ref().is_none_or(|s| force > s.force) {
                        best = Some(Supported {
                            member: x,
                            joint: j,
                            force,
                            via_redundant: via,
                        });
                    }
                }
            }
        }
        let r = best.ok_or_else(|| {
            "the member it braces not found: no member passes straight through its end joints"
                .to_owned()
        });
        state[k] = State::Done(r.clone());
        r
    }
    let mut state = vec![State::Todo; n];
    (0..n)
        .map(|k| {
            redundant(k).then(|| {
                supported(
                    k,
                    &mut state,
                    &lines,
                    model,
                    share,
                    &redundant,
                    &compression,
                )
            })
        })
        .collect()
}

/// A redundant member's usage `own` with its nominal force check
/// (`[analysis] redundant_force`, owner 2026-11-15): `share` of the
/// compression in the member it braces (`supported`,
/// [`redundant_supports`]), checked in compression and in tension (the
/// direction of the bracing force is not known; a tension-only member or
/// cable in tension only) by the same capacities as its own force; its
/// usage the larger of its own and the nominal one, what governs saying
/// which ([`REDUNDANT_FORCE`]). Where the braced member could not be
/// determined: said not on the safe side (the nominal check not done).
/// Every other member: `own`.
fn with_redundant_force(
    model: &Model,
    solution: &Solution,
    strength: f64,
    given: Option<crate::capacity::Capacity>,
    share: f64,
    supported: &Option<Result<Supported, String>>,
    mut own: MemberUsage,
) -> MemberUsage {
    let Some(supported) = supported else {
        return own;
    };
    let idx = own.member;
    let pct = share * 100.0;
    let s = match supported {
        Err(why) => {
            own.governs.push_str(&format!(
                " {NOT_SAFE} redundant force ({pct} %) not checked: {why}]"
            ));
            return own;
        }
        Ok(s) => s,
    };
    let braced = format!(
        "the compression of {} at {} (the member it braces{})",
        model.members[s.member].id,
        model.nodes[s.joint].id,
        if s.via_redundant {
            ", itself redundant: its design force"
        } else {
            ""
        }
    );
    let nominal = share * s.force;
    if nominal <= 0.0 {
        own.governs.push_str(&format!(
            " {REDUNDANT_FORCE} {braced}: none in this case — none]"
        ));
        return own;
    }
    let m = &model.members[idx];
    let f = &solution.members[idx];
    let pieces = |axial: f64| crate::solve::MemberForces {
        axial,
        end_forces: [0.0; 12],
        slack: false,
        bowed: false,
        length: f.length,
        span_load: [0.0; 3],
        lumped_load: [0.0; 3],
    };
    let mut checks = vec![usage_of(
        model,
        solution,
        idx,
        &pieces(nominal),
        strength,
        given.clone(),
    )];
    if !matches!(m.kind, MemberKind::Tension | MemberKind::Cable) {
        checks.push(usage_of(
            model,
            solution,
            idx,
            &pieces(-nominal),
            strength,
            given,
        ));
    }
    let Some(worst) = checks
        .into_iter()
        .filter(|u| u.usage.is_some())
        .max_by(|a, b| usage_order(a.usage.unwrap_or(0.0), b.usage.unwrap_or(0.0)))
    else {
        own.governs.push_str(&format!(
            " {REDUNDANT_FORCE} {pct} % of {:.2} kN, {braced}: {:.2} kN — no capacity]",
            s.force / 1000.0,
            nominal / 1000.0
        ));
        return own;
    };
    let u_nominal = worst.usage.unwrap_or(0.0);
    let governs_nominal = match own.usage {
        Some(u) => usage_order(u_nominal, u) == std::cmp::Ordering::Greater,
        None => true,
    };
    if governs_nominal {
        MemberUsage {
            member: idx,
            usage: worst.usage,
            governs: format!(
                "{} {REDUNDANT_FORCE} {pct} % of {:.2} kN, {braced}: {:.2} kN — governs; own \
                 force {}]",
                worst.governs,
                s.force / 1000.0,
                nominal / 1000.0,
                own.usage
                    .map_or("—".to_owned(), |u| format!("{:.0} %", u * 100.0))
            ),
        }
    } else {
        own.governs.push_str(&format!(
            " {REDUNDANT_FORCE} {pct} % of {:.2} kN, {braced}: {:.2} kN, {:.0} % — own force \
             governs]",
            s.force / 1000.0,
            nominal / 1000.0,
            u_nominal * 100.0
        ));
        own
    }
}

/// Members within this angle of the horizontal are checked for the
/// climbing load ([`climbing_check`]), degrees: those a climber stands on
/// (horizontals, flat bracing and arms); steeper ones are hand-holds,
/// not checked — said in the note.
pub const CLIMBING_INCLINATION_DEG: f64 = 30.0;

/// One member's climbing check ([`climbing_check`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Climbing {
    /// Member (index).
    pub member: usize,
    /// Share of its first-yield moment used (none: no elastic modulus or
    /// no yield strength).
    pub usage: Option<f64>,
    /// In words.
    pub words: String,
}

/// The optional climbing-load check (`[analysis] climbing_load`, owner
/// 2026-11-15; ASCE 10-97 commentary C2.2: members "that may be used for
/// support by maintenance personnel when climbing" should carry a
/// vertical load of 1100 N "applied independently of all other loads
/// without permanent distortion"): every design-checked member other than
/// a cable within [`CLIMBING_INCLINATION_DEG`] of the horizontal, the
/// load at mid-length, **alone** (no load case), the member simply
/// supported (its ends' restraint — the bolts' friction the commentary
/// allows — left out: on the safe side), the whole load across it (not
/// its component normal to the member: on the safe side): M = P L / 4 +
/// q L² / 8 with q its own weight × its dead-load factor, against first
/// yield W_el,min · f_y, W_el,min the smallest elastic modulus the
/// section gives (an angle's about its minor principal axis v: it bends
/// about its principal axes; a table section's the smaller of wy, wz).
/// No load or strength factor: the commentary's criterion is no
/// permanent distortion, i.e. no yield. Empty when the check is off.
pub fn climbing_check(model: &Model) -> Vec<Climbing> {
    let Some(p) = model.analysis.climbing_load else {
        return Vec::new();
    };
    let flat = CLIMBING_INCLINATION_DEG.to_radians().sin();
    let mut out = Vec::new();
    for (k, m) in model.members.iter().enumerate() {
        if !m.check || m.kind == MemberKind::Cable {
            continue;
        }
        let (a, b) = (model.nodes[m.i].point, model.nodes[m.j].point);
        let l = model.length(m);
        if l <= 0.0 || (b[2] - a[2]).abs() / l > flat + 1e-12 {
            continue;
        }
        let s = &model.sections[m.section];
        let mut moduli: Vec<f64> = [s.wy, s.wz].into_iter().flatten().collect();
        if let Some(p) = &s.profile {
            let q = properties(p);
            moduli.extend([q.wel_y, q.wel_z, q.wel_u, q.wel_v].into_iter().flatten());
        }
        let w = moduli
            .into_iter()
            .filter(|w| *w > 0.0)
            .min_by(|x, y| x.total_cmp(y));
        let q = s.weight() * model.dead_load_factor(m);
        let moment = p * l / 4.0 + q * l * l / 8.0;
        let (usage, words) = match (w, s.yield_strength) {
            (Some(w), Some(fy)) => (
                Some(moment / (w * fy)),
                format!(
                    "{:.2} kN at mid-length of {:.2} m: M {:.3} kN m (own weight {:.3}) of \
                     W_el,min {:.2} cm³ × f_y {:.0} MPa = {:.3} kN m",
                    p / 1000.0,
                    l,
                    moment / 1000.0,
                    q * l * l / 8.0 / 1000.0,
                    w * 1e6,
                    fy / 1e6,
                    w * fy / 1000.0
                ),
            ),
            (None, _) => (
                None,
                "not checked: its section gives no elastic modulus".to_owned(),
            ),
            (_, None) => (
                None,
                "not checked: its section gives no yield strength".to_owned(),
            ),
        };
        out.push(Climbing {
            member: k,
            usage,
            words,
        });
    }
    out
}

/// The climbing check in words, for the notes (none: off): how many flat
/// members were checked, how many exceed first yield (the worst named),
/// those not checked, and that steeper members are not — said apart from
/// the load cases' usage (owner 2026-11-15: a separate result).
pub fn climbing_note(model: &Model, climbing: &[Climbing]) -> Option<String> {
    let p = model.analysis.climbing_load?;
    let over: Vec<&Climbing> = climbing
        .iter()
        .filter(|c| {
            c.usage
                .is_some_and(|u| classify_usage(u) == UsageClass::Over)
        })
        .collect();
    let unchecked = climbing.iter().filter(|c| c.usage.is_none()).count();
    let worst = climbing
        .iter()
        .filter(|c| c.usage.is_some())
        .max_by(|a, b| usage_order(a.usage.unwrap_or(0.0), b.usage.unwrap_or(0.0)));
    let mut text = format!(
        "climbing load (alone, not a load case; ASCE 10-97 commentary C2.2): {:.2} kN at \
         mid-length of each member within {CLIMBING_INCLINATION_DEG:.0}° of the horizontal, \
         simply supported, against first yield: {} member(s) checked",
        p / 1000.0,
        climbing.len()
    );
    if let Some(w) = worst {
        text.push_str(&format!(
            ", worst {} {:.0} %",
            model.members[w.member].id,
            w.usage.unwrap_or(0.0) * 100.0
        ));
    }
    if !over.is_empty() {
        let names: Vec<String> = over
            .iter()
            .take(12)
            .map(|c| {
                format!(
                    "{} {:.0} %",
                    model.members[c.member].id,
                    c.usage.unwrap_or(0.0) * 100.0
                )
            })
            .collect();
        text.push_str(&format!(
            "; {} OVER first yield: {}{}",
            over.len(),
            names.join(", "),
            if over.len() > 12 { ", …" } else { "" }
        ));
    }
    if unchecked > 0 {
        text.push_str(&format!(
            "; NOT ON THE SAFE SIDE: {unchecked} not checked (no elastic modulus or yield strength)"
        ));
    }
    text.push_str(&format!(
        "; members steeper than {CLIMBING_INCLINATION_DEG:.0}° not checked (NOT ON THE SAFE SIDE \
         for any a climber stands on)"
    ));
    Some(text)
}

/// A member's usage with what the core says about it whatever method
/// checked it (its own, or a plugin's): kept active though it would go
/// slack ([`KEPT_ACTIVE`]), a half of a diagonal through a crossing only
/// the other diagonal holds (`crossings`: [`unbraced_crossings`]; said
/// not on the safe side), and its eccentric ends ([`SHIFTED`]).
pub fn with_remarks(
    model: &Model,
    solution: &Solution,
    crossings: &[Option<String>],
    mut u: MemberUsage,
) -> MemberUsage {
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
    given: Option<crate::capacity::Capacity>,
) -> MemberUsage {
    {
        {
            let m = &model.members[idx];
            let s = &model.sections[m.section];
            // The moments of eccentric ends the solve could not carry
            // (only bars meet the joint), in its check (2026-11-18).
            let shifted = crate::offsets::eccentric_bending(model, idx);
            let f = &crate::offsets::with_eccentric_moments(model, idx, f);
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
                    governs: match model.analysis.tension_compression {
                        crate::TensionCompression::Buckling => "slack".into(),
                        crate::TensionCompression::None => SLACK_NO_COMPRESSION.into(),
                    },
                };
            }
            let n = f.axial;
            let kn = |x: f64| x / 1000.0;
            // The capacities given from outside (none: the simple check;
            // a design code's are its plugin's, [`member_usage_with`]).
            let asce = given;
            let mut assumed: &[String] = &[];
            let mut joints: &[String] = &[];
            let mut extra = String::new();
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
            let axial_only = !shifted
                && (!m.is_frame() || (axial_member(m, s) && !model.analysis.single_angle_moments));
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
            if !not_safe_notes.is_empty() {
                suffix.push_str(&format!(" {NOT_SAFE} {}]", not_safe_notes.join(", ")));
            }
            let axial = axial.map(|(u, mut words)| {
                words.push_str(&suffix);
                (u, words)
            });
            if axial_only {
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
