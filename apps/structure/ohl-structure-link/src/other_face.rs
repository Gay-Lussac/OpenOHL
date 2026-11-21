//! Members on either side of a leg's flange (O44 L10, the owner's idea of
//! 2026-11-21, from photographs of real towers: the horizontals bolted on
//! the outer face of the leg's flange, the diagonal on its inner face,
//! meeting at one point without a plate).
//!
//! Where two or more ends lie on one face of a leg's flange and go onto a
//! plate there, or one of them is left for want of room beside the
//! other, the run ([`crate::layout_run`]) tries one of them on the
//! flange's other face: half a turn of its member (its outstanding leg to
//! the other side, the same leg bolted), so that its leg lies on that
//! face. This module finds the faces and the end to try on each, and how
//! far the end's leg then lies from the work point across the flange.
//!
//! The move across the flange is a seat, as every end's on a face (the
//! analysis takes the members' axes through the work point and leaves
//! every seat across a flange out, the outer face's as the inner's,
//! O45 D). It is applied by the run only where the end's leg then lies no
//! farther from the work point across the flange than it lies now (on its
//! plate, or on the flange) — so that nothing more is left out of the
//! analysis than before; otherwise only proposed, with the bending stress
//! that offset would add over the axial stress.

use crate::Face;
use crate::joint_layout::{ClassRank, JointPlan};
use crate::joints::{Arrival, JointDetails, Source};

/// Where an end's leg bolted lies across the flange it lies on (its
/// middle plane), from the work point along the flange's normal into the
/// flange's thickness, m.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FacePlaces {
    /// On the flange's outer face (the heel's side), bolted directly.
    pub outer: f64,
    /// On its inner face, bolted directly.
    pub inner: f64,
    /// The end's leg's thickness, m.
    pub t_leg: f64,
    /// The flange's thickness, m.
    pub t_flange: f64,
}

/// The places of end `a`'s leg on the outer and inner face of the flange
/// of `host` it lies on (the host's leg most nearly parallel to `a`'s leg
/// bolted, the host where it is drawn). None: either is no single angle.
pub(crate) fn face_places(a: &Arrival, host: &Arrival) -> Option<FacePlaces> {
    let p = a.profile.as_ref()?;
    let g = p.angle?;
    let q = host.profile.as_ref()?;
    let h = q.angle?;
    let nb = if a.leg_y { p.axes[2] } else { p.axes[1] };
    // The host's legs run from its heel along its y and z; the flange
    // most nearly parallel to `a`'s leg, its normal into its thickness.
    let (ey, ez) = (q.axes[1], q.axes[2]);
    let nl = if dot(ez, nb).abs() >= dot(ey, nb).abs() {
        ez
    } else {
        ey
    };
    let heel = [
        q.axes[1][0] * h.heel[0] + q.axes[2][0] * h.heel[1] + host.offset[0],
        q.axes[1][1] * h.heel[0] + q.axes[2][1] * h.heel[1] + host.offset[1],
        q.axes[1][2] * h.heel[0] + q.axes[2][2] * h.heel[1] + host.offset[2],
    ];
    let hn = dot(heel, nl);
    Some(FacePlaces {
        outer: hn - g.t / 2.0,
        inner: hn + h.t + g.t / 2.0,
        t_leg: g.t,
        t_flange: h.t,
    })
}

/// One end tried on its flange's other face.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FaceTry {
    /// The member (turned half a turn: all its ends move).
    pub member: String,
    /// The joint whose face asked for it.
    pub joint: String,
    /// The member whose flange it lies on there.
    pub host: String,
    /// From the outer face onto the inner one (else the other way).
    pub to_inner: bool,
    /// How far its leg lies from the work point across the flange now
    /// (on its plate where it is on one) and on the other face, m.
    pub now: f64,
    /// On the other face, m.
    pub then: f64,
    /// It follows the photographs' pattern (a diagonal onto the inner
    /// face, a horizontal onto the outer one).
    pub pattern: bool,
}

/// What the run tells [`face_tries`] about a member: may it be turned
/// (not a leg or chord, not one part, not held by a joint laid out
/// elsewhere, its roll not given by hand, not turned already in this
/// run), and its class in the owner's order.
pub(crate) struct MemberAsk<'a> {
    /// The member may be turned half a turn by this step.
    pub may_turn: &'a dyn Fn(&str) -> bool,
    /// Its class rank ([`ClassRank`] as a number).
    pub class: &'a dyn Fn(&str) -> u8,
}

/// The ends to try on their flange's other face, per face of a leg's (or
/// chord's) flange: a flat plate on it with two or more ends
/// (they would need no plate if one of them lay on the other face), or an
/// end left for want of room beside another end of that face. With
/// `pattern`, every end of the face following the photographs' pattern (a
/// diagonal from the outer face onto the inner one, a horizontal from the
/// inner face onto the outer one) where one end at least stays; else one
/// end: those following the pattern first, then the later class (the
/// owner's order: of two members in one clash the later class gives way),
/// the narrower leg, the id.
pub(crate) fn face_tries(
    plans: &[(String, JointPlan)],
    details: &JointDetails,
    ask: &MemberAsk<'_>,
    pattern: bool,
) -> Vec<FaceTry> {
    let mut out: Vec<FaceTry> = Vec::new();
    for (jid, p) in plans {
        let Some(j) = details.joint(jid) else {
            continue;
        };
        let find = |m: &str| j.arrivals.iter().find(|a| a.member == m);
        // Each face: (host, the ends on it).
        let mut faces: Vec<(String, Vec<String>)> = Vec::new();
        for plate in &p.plates {
            if plate.own.is_some() || plate.reach.is_some() || plate.members.len() < 2 {
                continue;
            }
            faces.push((plate.on.clone(), plate.members.clone()));
        }
        for h in &p.hand {
            // "X and Y within d°: no room on one plate …"
            let Some((pair, _)) = h.split_once(": no room on one plate") else {
                continue;
            };
            let Some((x, rest)) = pair.split_once(" and ") else {
                continue;
            };
            let Some((y, _)) = rest.split_once(" within ") else {
                continue;
            };
            let (Some(ax), Some(ay)) = (find(x), find(y)) else {
                continue;
            };
            if ax.onto.value.is_some() && ax.onto.value == ay.onto.value {
                let host = ax.onto.value.clone().unwrap_or_default();
                faces.push((host, vec![x.to_owned(), y.to_owned()]));
            }
        }
        for (host, ends) in faces {
            let Some(h) = find(&host) else { continue };
            if (ask.class)(&host) != ClassRank::Leg as u8 {
                continue;
            }
            if out
                .iter()
                .any(|t| t.joint == *jid && ends.contains(&t.member))
            {
                continue;
            }
            let mut tries: Vec<(bool, std::cmp::Reverse<u8>, i64, String, FaceTry)> = Vec::new();
            for m in &ends {
                let Some(a) = find(m) else { continue };
                if !(ask.may_turn)(m)
                    || a.sits.source == Source::Given
                    || a.onto.value.as_deref() != Some(host.as_str())
                    || a.flat.is_none_or(|f| f.off > crate::FLAT_OFF)
                    || out.iter().any(|t| &t.member == m)
                {
                    continue;
                }
                let Some(face) = a.sits.value.filter(|f| *f != Face::Nested) else {
                    continue;
                };
                let Some(places) = face_places(a, h) else {
                    continue;
                };
                let to_inner = face == Face::Outside;
                // On a plate now: its leg the plate's thickness farther
                // off the flange.
                let t_plate = p
                    .plates
                    .iter()
                    .find(|q| q.own.is_none() && q.on == host && q.members.contains(m))
                    .map_or(0.0, |q| q.thickness);
                let (now, then) = if to_inner {
                    (places.outer - t_plate, places.inner)
                } else {
                    (places.inner + t_plate, places.outer)
                };
                let class = (ask.class)(m);
                let fits = (to_inner && class == ClassRank::Diagonal as u8)
                    || (!to_inner && class == ClassRank::Secondary as u8);
                let width = a.profile.as_ref().and_then(|q| q.angle).map_or(0, |g| {
                    ((if a.leg_y { g.b } else { g.h }) * 1e4).round() as i64
                });
                tries.push((
                    !(pattern && fits),
                    std::cmp::Reverse(class),
                    width,
                    m.clone(),
                    FaceTry {
                        member: m.clone(),
                        joint: jid.clone(),
                        host: host.clone(),
                        to_inner,
                        now,
                        then,
                        pattern: fits,
                    },
                ));
            }
            tries.sort_by(|a, b| (a.0, a.1, a.2, &a.3).cmp(&(b.0, b.1, b.2, &b.3)));
            // With the pattern: every end of the face following it (the
            // diagonals onto the inner face together, as in the
            // photographs), while one end at least stays; else the first.
            let fitting = tries.iter().filter(|t| !t.0).count();
            let take = if pattern && fitting > 1 && fitting < ends.len() {
                fitting
            } else {
                1
            };
            out.extend(tries.into_iter().take(take).map(|t| t.4));
        }
    }
    out
}

/// Each end of member `m` at `joints` (of `plans`, in detail by `joint`)
/// lying on a face of an angle's flange, half a turn moving it onto that
/// flange's other face: (joint, host, onto the inner face, how far its leg
/// lies from the work point across the flange now — on its plate where on
/// one — and then, m).
pub(crate) fn ends_moved(
    m: &str,
    joints: &[&str],
    plans: &[(String, JointPlan)],
    details: &JointDetails,
) -> Vec<(String, String, bool, f64, f64)> {
    let mut out = Vec::new();
    for jid in joints {
        let Some(j) = details.joint(jid) else {
            continue;
        };
        let Some(a) = j.arrivals.iter().find(|a| a.member == m) else {
            continue;
        };
        let Some(host) = a.onto.value.as_deref() else {
            continue;
        };
        let Some(h) = j.arrivals.iter().find(|b| b.member == host) else {
            continue;
        };
        let Some(face) = a.sits.value.filter(|f| *f != Face::Nested) else {
            continue;
        };
        let Some(places) = face_places(a, h) else {
            continue;
        };
        let t_plate = plans
            .iter()
            .find(|(x, _)| x == jid)
            .and_then(|(_, p)| {
                p.plates
                    .iter()
                    .find(|q| q.own.is_none() && q.on == host && q.members.iter().any(|x| x == m))
            })
            .map_or(0.0, |q| q.thickness);
        let to_inner = face == Face::Outside;
        let (now, then) = if to_inner {
            (places.outer - t_plate, places.inner)
        } else {
            (places.inner + t_plate, places.outer)
        };
        out.push(((*jid).to_owned(), host.to_owned(), to_inner, now, then));
    }
    out
}

/// The joints' ends and plates where member `m` meets (of `plans`), for
/// the guard: (ends laid out, plates) per joint.
pub(crate) fn counts_at(plans: &[(String, JointPlan)], joints: &[&str]) -> Vec<(usize, usize)> {
    joints
        .iter()
        .map(|j| {
            plans
                .iter()
                .find(|(x, _)| x == j)
                .map_or((0, 0), |(_, p)| (p.ends.len(), p.plates.len()))
        })
        .collect()
}

/// The guard (the owner's weighing rule, joint by joint): at no joint
/// fewer ends, nor as many on more plates; and in all more ends, or as many
/// on fewer plates.
pub(crate) fn helps(before: &[(usize, usize)], after: &[(usize, usize)]) -> bool {
    if before
        .iter()
        .zip(after)
        .any(|(b, a)| a.0 < b.0 || (a.0 == b.0 && a.1 > b.1))
    {
        return false;
    }
    let sum = |v: &[(usize, usize)]| v.iter().fold((0, 0), |(e, p), (x, y)| (e + x, p + y));
    let (e0, p0) = sum(before);
    let (e1, p1) = sum(after);
    e1 > e0 || (e1 == e0 && p1 < p0)
}

/// The elastic bending stress an offset `e` (m) of end `a`'s force across
/// its leg bolted adds at its farthest fibre, over the axial stress:
/// e·A/W about the axis along its leg bolted (from its outline). None:
/// no outline.
pub(crate) fn stress_ratio(a: &Arrival, e: f64) -> Option<f64> {
    let p = a.profile.as_ref()?;
    // Across the leg bolted: the section's z for a leg along y.
    let k = if a.leg_y { 1 } else { 0 };
    let pts = &p.outline;
    if pts.len() < 3 {
        return None;
    }
    let (mut area, mut first, mut second) = (0.0, 0.0, 0.0);
    for i in 0..pts.len() {
        let (p0, p1) = (pts[i], pts[(i + 1) % pts.len()]);
        let c = p0[0] * p1[1] - p1[0] * p0[1];
        area += c / 2.0;
        first += (p0[k] + p1[k]) * c / 6.0;
        second += (p0[k] * p0[k] + p0[k] * p1[k] + p1[k] * p1[k]) * c / 12.0;
    }
    if area.abs() < 1e-12 {
        return None;
    }
    let mid = first / area;
    let i = (second - area * mid * mid).abs();
    let far = pts.iter().map(|q| (q[k] - mid).abs()).fold(0.0, f64::max);
    (i > 0.0 && far > 0.0).then(|| e.abs() * area.abs() * far / i)
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
