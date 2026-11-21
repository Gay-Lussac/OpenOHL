// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Both faces of one plate** (O44 L11, the owner's picture, 2026-11-21:
//! two diagonals meeting a horizontal, one on a flat plate on its flange,
//! the other on a bent plate from its other flange — "both diagonals could
//! be on the same plate just bolted on different sides").
//!
//! A plan's flat plate on a flange's outer face, cut at the flange's heel,
//! and an end of the joint the plan puts on a plate in its own plane (bent
//! or welded, step 4) whose leg bolted lies flat (within the owner's 2°) in
//! the flange's plane, on the flange's inner side, leaving the joint over
//! the heel: that end on the flat plate's **back**, past the heel — where
//! the flange is not, the plate's back is free —, pulled back until its
//! steel clears the heel's line by the gap (its leg's whole width, on the
//! safe side) and its bolts by their nuts and the gap, within the longest
//! pull-back; the plate no longer cut at the heel; its own plate (and the
//! flat it was bent from or welded to, where nothing else is on it) gone.
//! And first, **one plate per plane**: two flat plates of the plan on
//! flanges in one plane of members one part through the joint (a chord
//! running on, its pieces named apart) are one plate, its ends shared (the
//! owner: "one plate for each plane of the main legs and everything
//! bolted to the plates").
//!
//! Applied to the plan, then kept as every plan is (the run's clash check
//! after writing takes an end back that still runs into something). The
//! joint details draw it there ([`crate::joints`]: a plate with members
//! on its leg's outer face and others sitting on the inner face side of
//! it — both faces used — has those on its back).

use crate::joint_layout::{JointPlan, LayoutSettings, Onto, mm_text};
use crate::joints::{Arrival, BOLT_DRAWN, FLAT_OFF, JointDetail, host_flange};

/// An end leaves over the heel when its direction runs at least this much
/// (the cosine's share) towards the heel across the flange: a member nearly
/// along the flange's heel line is not taken (its cut would grow without
/// bound). The program's value.
const OVER_HEEL: f64 = 0.2;

/// Each end of `plan` (at joint `j`) on a plate in its own plane that can
/// lie on the back of a flat plate of the plan instead, there (see the
/// module); the plates left over taken away.
pub(crate) fn both_faces(j: &JointDetail, plan: &mut JointPlan, s: &LayoutSettings) {
    let before = plan.clone();
    passes(j, plan, s);
    // (Every end onto a plate of the plan, every plate welded to one: else
    // the plan as it was.)
    let n = plan.plates.len();
    let ok = plan
        .ends
        .iter()
        .all(|e| !matches!(e.onto, Onto::Plate(k) if k >= n))
        && plan
            .plates
            .iter()
            .all(|p| p.own.as_ref().is_none_or(|o| o.welded_to < n));
    if !ok {
        *plan = before;
    }
}

fn passes(j: &JointDetail, plan: &mut JointPlan, s: &LayoutSettings) {
    one_plate_per_plane(j, plan);
    let find = |m: &str| j.arrivals.iter().find(|a| a.member == m);
    // The members one part with `h` (it and those it runs on into).
    let one_part = |a: &str, b: &str| {
        a == b
            || find(a).is_some_and(|x| x.continuous.as_deref() == Some(b))
            || find(b).is_some_and(|x| x.continuous.as_deref() == Some(a))
    };
    let mut gone: Vec<usize> = Vec::new();
    let flats: Vec<usize> = (0..plan.plates.len())
        .filter(|&k| {
            let p = &plan.plates[k];
            p.own.is_none()
                && p.reach.is_none()
                && p.outline.is_none()
                && !p.short
                && p.cuts.is_empty()
                && !p.members.is_empty()
        })
        .collect();
    for kp in flats {
        let (host, front) = {
            let p = &plan.plates[kp];
            (p.on.clone(), p.members.clone())
        };
        let (Some(h), Some(a)) = (find(&host), front.first().and_then(|m| find(m))) else {
            continue;
        };
        let Some(f) = host_flange(a, h) else { continue };
        // Not where an end bolts directly onto the host or a member one part
        // with it (the plate reaching over the heel would run over it).
        if plan
            .ends
            .iter()
            .any(|e| matches!(&e.onto, Onto::Member(m) if one_part(m, &host)))
        {
            continue;
        }
        for ke in 0..plan.ends.len() {
            let Onto::Plate(kq) = plan.ends[ke].onto else {
                continue;
            };
            let Some(own) = plan.plates.get(kq).and_then(|q| q.own.as_ref()) else {
                continue;
            };
            let w = own.welded_to;
            if gone.contains(&kq) || !one_part(&plan.plates[w].on, &host) {
                continue;
            }
            // Alone on its own plate.
            if plan
                .ends
                .iter()
                .filter(|x| x.onto == Onto::Plate(kq))
                .count()
                != 1
            {
                continue;
            }
            let Some(b) = find(&plan.ends[ke].member) else {
                continue;
            };
            let Some(c) = back_cut(j, b, &f, plan.ends[ke].bolts.as_ref(), s) else {
                continue;
            };
            let e = &mut plan.ends[ke];
            e.onto = Onto::Plate(kp);
            e.cut = Some(c);
            e.shift = Some(0.0);
            e.off = 0.0;
            e.on_gauge = true;
            plan.notes.push(format!(
                "{}: on the back of the plate on {host} with {} (both faces of one plate): its \
                 steel stops at {} from the work point, past the flange's heel",
                b.member,
                front.join(", "),
                mm_text(c)
            ));
            let p = &mut plan.plates[kp];
            p.members.push(b.member.clone());
            p.heel = None;
            gone.push(kq);
            // Its flat, where nothing else is on it.
            let used = plan.ends.iter().any(|x| x.onto == Onto::Plate(w))
                || plan.plates.iter().enumerate().any(|(k, q)| {
                    k != kq
                        && !gone.contains(&k)
                        && q.own.as_ref().is_some_and(|o| o.welded_to == w)
                });
            if !used && plan.plates[w].members.iter().all(|m| *m == b.member) {
                gone.push(w);
            }
        }
    }
    take_away(plan, gone);
}

/// Two flat plates of `plan` on flanges in one plane of members one part
/// through joint `j` made one (the first), the other's ends onto it.
fn one_plate_per_plane(j: &JointDetail, plan: &mut JointPlan) {
    let find = |m: &str| j.arrivals.iter().find(|a| a.member == m);
    let one_part = |a: &str, b: &str| {
        find(a).is_some_and(|x| x.continuous.as_deref() == Some(b))
            || find(b).is_some_and(|x| x.continuous.as_deref() == Some(a))
    };
    let flats: Vec<bool> = plan
        .plates
        .iter()
        .map(|p| {
            p.own.is_none()
                && p.reach.is_none()
                && p.outline.is_none()
                && !p.short
                && p.cuts.is_empty()
                && !p.members.is_empty()
        })
        .collect();
    let flat = |k: usize| flats[k];
    // Its flange's outer face: a point and the normal.
    let faces: Vec<Option<([f64; 3], [f64; 3])>> = plan
        .plates
        .iter()
        .map(|p| {
            let h = find(&p.on)?;
            let a = find(p.members.first()?)?;
            let f = host_flange(a, h)?;
            Some((f.heel, f.out))
        })
        .collect();
    let face = |k: usize| faces[k];
    let mut gone: Vec<usize> = Vec::new();
    for k1 in 0..plan.plates.len() {
        if gone.contains(&k1) || !flat(k1) {
            continue;
        }
        let Some((h1, n1)) = face(k1) else { continue };
        for k2 in k1 + 1..plan.plates.len() {
            if gone.contains(&k2) || !flat(k2) {
                continue;
            }
            if !one_part(&plan.plates[k1].on, &plan.plates[k2].on) {
                continue;
            }
            // Not where an end bolts directly onto either member (the one
            // plate round both would run over it: measured, ends lost).
            let (o1, o2) = (&plan.plates[k1].on, &plan.plates[k2].on);
            if plan
                .ends
                .iter()
                .any(|e| matches!(&e.onto, Onto::Member(m) if m == o1 || m == o2))
            {
                continue;
            }
            let Some((h2, n2)) = face(k2) else { continue };
            let off = [h2[0] - h1[0], h2[1] - h1[1], h2[2] - h1[2]];
            if dot(n1, n2) < FLAT_OFF.to_radians().cos() || dot(off, n1).abs() > 1e-3 {
                continue;
            }
            let (m2, t2, heel2, hold2) = {
                let q = &plan.plates[k2];
                (q.members.clone(), q.thickness, q.heel, q.hold.clone())
            };
            let words = format!(
                "{} and {}: one plate on {} and {} (one part, their flanges in one plane)",
                plan.plates[k1].members.join(", "),
                m2.join(", "),
                plan.plates[k1].on,
                plan.plates[k2].on
            );
            plan.notes.push(words);
            let p = &mut plan.plates[k1];
            p.members.extend(m2);
            p.thickness = p.thickness.max(t2);
            if heel2.is_none() {
                p.heel = None;
            }
            if p.hold.is_none() {
                p.hold = hold2;
            }
            for e in &mut plan.ends {
                if e.onto == Onto::Plate(k2) {
                    e.onto = Onto::Plate(k1);
                }
            }
            gone.push(k2);
        }
    }
    take_away(plan, gone);
}

/// The plates `gone` of `plan` taken away, the indices of the rest moved
/// down.
fn take_away(plan: &mut JointPlan, mut gone: Vec<usize>) {
    if gone.is_empty() {
        return;
    }
    gone.sort_unstable();
    gone.dedup();
    // Taken away, the indices of the rest moved down.
    let new_of = |k: usize| k - gone.iter().filter(|g| **g < k).count();
    let mut k = 0;
    plan.plates.retain(|_| {
        let keep = !gone.contains(&k);
        k += 1;
        keep
    });
    // (Onto a plate taken away: left pointing past the plates, so that the
    // check after takes the plan back as it was.)
    let new_of = |k: usize| {
        if gone.contains(&k) {
            usize::MAX
        } else {
            new_of(k)
        }
    };
    for e in &mut plan.ends {
        if let Onto::Plate(x) = e.onto {
            e.onto = Onto::Plate(new_of(x));
        }
    }
    for p in &mut plan.plates {
        if let Some(o) = p.own.as_mut() {
            o.welded_to = new_of(o.welded_to);
        }
    }
}

/// Where `b`'s steel stops on the back of a plate on flange `f` (at joint
/// `j`), m: none where it does not lie there — not flat in the flange's
/// plane on its inner side, not leaving over the heel, or needing more than
/// the longest pull-back.
fn back_cut(
    j: &JointDetail,
    b: &Arrival,
    f: &crate::joints::HostFlange,
    bolts: Option<&crate::BoltArea>,
    s: &LayoutSettings,
) -> Option<f64> {
    let p = b.profile.as_ref()?;
    let g = p.angle?;
    let nb = if b.leg_y { p.axes[2] } else { p.axes[1] };
    // Flat in the flange's plane, on its inner side (its own side inwards).
    if dot(nb, f.out) > -FLAT_OFF.to_radians().cos() {
        return None;
    }
    let da = dot(b.direction, f.across);
    if da > -OVER_HEEL {
        return None;
    }
    // The work point across the flange from its heel (the heel is from the
    // work point).
    let y0 = -dot(f.heel, f.across);
    let v = cross(f.out, b.direction);
    let va = dot(v, f.across).abs() / norm(v).max(1e-12);
    let width = if b.leg_y { g.b } else { g.h };
    // Its steel past the heel's line by the gap, over its leg's whole width
    // either side of its gauge line (on the safe side).
    let steel = (y0 + width * va + s.gap) / -da;
    // Its bolts past it by their nuts and the gap.
    let (d, end, pitch, count, lines) = match (bolts, b.bolts.value.as_ref()) {
        (Some(x), _) => (
            x.diameter,
            x.end.unwrap_or(s.defaults.of(x.diameter)[0]),
            x.pitch.unwrap_or(s.defaults.of(x.diameter)[1]),
            x.count.max(1),
            x.lines.max(1),
        ),
        (None, Some(x)) => {
            let d = x.diameter;
            (
                d,
                b.end_distance.value.unwrap_or(s.defaults.of(d)[0]),
                b.pitch.value.unwrap_or(s.defaults.of(d)[1]),
                x.count.max(1),
                b.gauge_lines.value.unwrap_or(1).max(1),
            )
        }
        (None, None) => return None,
    };
    let nut = BOLT_DRAWN[0] * d / 3f64.sqrt();
    let bolt = (y0 + nut + s.gap) / -da - end;
    let c = (steel.max(bolt).max(0.0) * 1000.0).ceil() / 1000.0;
    let rows = count.div_ceil(lines);
    let run = end + (rows.saturating_sub(1)) as f64 * pitch + d;
    let limit = s.pull_back.min(b.length / 2.0 - run);
    (c <= limit + 1e-9)
        .then_some(c)
        .filter(|_| j.arrivals.len() > 1)
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
