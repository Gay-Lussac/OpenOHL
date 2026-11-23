// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **The planes of a joint** (the automatic joint layout's L18, step 1;
//! the owner, 2026-11-22: "the axes of the incoming members into a joint
//! define planes together and we should have the parts rotated
//! accordingly … and if there are multiple planes (e.g. multiple sizes or
//! different forces) then we should go from high to low"). Built, in active
//! development, not approved yet.
//!
//! At a joint the **host** is what the other ends bolt onto
//! ([`crate::joint_classes::joint_features`]: legs first, then the largest
//! steel area, with the piece in line with it where one runs on). Every
//! other end's axis and the host's axis span a plane through the host's
//! axis — the plane one of the host's flanges must lie in for that end's
//! leg to lie flat on it (an angle's leg bolted contains the end's axis).
//! Seen along the host such a plane is a line through its axis, at an
//! angle ψ from the host's y axis towards its z axis (0° to 180°: a plane
//! turned half a turn is the same plane). The ends whose planes lie within
//! [`PLANE_TOL_DEG`] of each other are **one plane** ([`JointPlane`]);
//! the planes are listed **high to low**: the one with the biggest member
//! first (its steel area), then the larger force where the sizes tie and
//! forces are given (the structure file stores no analysis forces: the
//! layout passes none, so size alone orders them there — said), then more
//! ends.
//!
//! For each plane: the host's flange nearer to it (an angle's flange along
//! y lies in the plane ψ = 0°, along z in ψ = 90°), the turn of the host
//! about its own axis that lays that flange in it, and per end how far its
//! axis runs out of that flange's plane as the host lies now (the angle
//! between the axis and the plane, the owner's 2° rule measures this:
//! [`crate::FLAT_OFF`]).
//!
//! An end within [`ALONG_DEG`] of the host's axis (in line with it: its
//! run, a crossing diagonal running on) spans no plane and is listed
//! apart. Pure functions of the joint's details: deterministic. The
//! tolerances are the tool's, not a standard's.

use crate::joints::JointDetail;

/// The planes of two ends through the host's axis are one plane within
/// this, degrees (seen along the host). The tool's value — the owner's 2°
/// flatness ([`crate::FLAT_OFF`]) — not a standard's.
pub const PLANE_TOL_DEG: f64 = crate::FLAT_OFF;

/// An end within this of the host's axis spans no plane with it, degrees
/// (in line: [`crate::joint_classes::IN_LINE_DEG`]). The tool's value.
pub const ALONG_DEG: f64 = crate::joint_classes::IN_LINE_DEG;

/// One incoming end, as the planes read it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaneEnd {
    /// The member.
    pub member: String,
    /// From the joint along it (unit).
    pub direction: [f64; 3],
    /// Its steel area, m² (the size the planes are ordered by).
    pub area: f64,
    /// Its force where known, N (absolute; none: not known).
    pub force: Option<f64>,
}

/// One end in a plane: how far its axis runs out of the host's flange's
/// plane nearer to the plane.
#[derive(Debug, Clone, PartialEq)]
pub struct InPlane {
    /// The member.
    pub member: String,
    /// The plane's angle for this end alone (ψ, degrees, 0 to 180).
    pub angle: f64,
    /// Its angle to the host's axis, degrees (0 to 90).
    pub to_host: f64,
    /// How far its axis runs out of the plane of the host's flange
    /// [`JointPlane::flange`] as the host lies now, degrees.
    pub off: f64,
}

/// One plane through the host's axis and the axes of some ends.
#[derive(Debug, Clone, PartialEq)]
pub struct JointPlane {
    /// Its angle seen along the host, from the host's y towards its z,
    /// degrees (0 to 180): the middle of its ends' angles.
    pub angle: f64,
    /// Its normal (unit, structure's axes): normal to the host's axis and
    /// the plane.
    pub normal: [f64; 3],
    /// Its ends, high to low (area, force, the file's order).
    pub ends: Vec<InPlane>,
    /// Its ends' angles' spread, degrees (at most [`PLANE_TOL_DEG`]).
    pub spread: f64,
    /// The largest steel area of its ends, m².
    pub area: f64,
    /// The largest force of its ends where any is known, N.
    pub force: Option<f64>,
    /// The host's flange nearer to it: 0 along the host's y, 1 along z.
    pub flange: usize,
    /// The turn of the host about its own axis (added to its roll) that
    /// lays that flange in the plane, degrees (−45 to 45).
    pub turn: f64,
}

impl JointPlane {
    /// The largest of its ends' angles out of the host's flange's plane
    /// as the host lies now, degrees.
    pub fn off(&self) -> f64 {
        self.ends.iter().map(|e| e.off).fold(0.0, f64::max)
    }

    /// Every end lies in the host's flange's plane within the owner's 2°.
    pub fn in_flange(&self) -> bool {
        self.off() <= crate::FLAT_OFF
    }
}

/// The planes of a joint about its host.
#[derive(Debug, Clone, PartialEq)]
pub struct HostPlanes {
    /// The joint.
    pub joint: String,
    /// The host (none: no member with a profile at the joint).
    pub host: String,
    /// The host's piece running on in line through the joint (its run).
    pub run: Option<String>,
    /// The host's axes (x along it from the joint, y, z: its section's).
    pub axes: [[f64; 3]; 3],
    /// The host is a single angle (its flanges along y and z); false: a
    /// built-up section (its angles' flanges along the same axes) or
    /// another shape (the planes said all the same).
    pub angle: bool,
    /// The planes, high to low.
    pub planes: Vec<JointPlane>,
    /// The ends in line with the host (no plane), in the joint's order.
    pub along: Vec<String>,
}

impl HostPlanes {
    /// The ends in planes, and of those within the owner's 2° of the
    /// host's flange's plane as it lies now.
    pub fn ends_within(&self) -> (usize, usize) {
        let all = self.planes.iter().map(|p| p.ends.len()).sum();
        let within = self
            .planes
            .iter()
            .flat_map(|p| &p.ends)
            .filter(|e| e.off <= crate::FLAT_OFF)
            .count();
        (all, within)
    }
}

/// ψ of direction `d` about axes `axes` (x the axis), degrees in [0, 180),
/// and its angle to the axis, degrees (0 to 90); none: along the axis
/// within [`ALONG_DEG`].
pub fn plane_angle(axes: [[f64; 3]; 3], d: [f64; 3]) -> Option<(f64, f64)> {
    let [x, y, z] = axes;
    let c = dot(d, x).abs().min(1.0);
    let to = c.acos().to_degrees();
    if to < ALONG_DEG {
        return None;
    }
    let mut psi = dot(d, z).atan2(dot(d, y)).to_degrees();
    while psi < 0.0 {
        psi += 180.0;
    }
    while psi >= 180.0 {
        psi -= 180.0;
    }
    Some((psi, to))
}

/// The signed distance from ψ (degrees) to the nearer of the host's
/// flange planes (0° or 90°, half turns alike): (flange, turn laying it
/// there, −45 to 45 degrees).
pub fn nearer_flange(psi: f64) -> (usize, f64) {
    // Folded into (−45, 45] from the flange at 0° or 90°.
    let mut d = psi % 90.0;
    if d > 45.0 {
        d -= 90.0;
    } else if d <= -45.0 {
        d += 90.0;
    }
    // Which flange: ψ − d is a multiple of 90°; odd multiples are z's.
    let k = ((psi - d) / 90.0).round() as i64;
    (k.rem_euclid(2) as usize, d)
}

/// How far an axis at `to_host` degrees to the host and ψ `psi` lies out of
/// the plane of the host's flange `flange` turned by `turn` degrees, as the
/// angle between the axis and that plane, degrees.
pub fn off_flange(psi: f64, to_host: f64, flange: usize, turn: f64) -> f64 {
    let rel = (psi - turn - 90.0 * flange as f64).to_radians();
    (to_host.to_radians().sin() * rel.sin().abs())
        .min(1.0)
        .asin()
        .to_degrees()
}

/// The planes of `ends` about a host of axes `axes` (x along it), within
/// `tol` degrees of each other, high to low (see the module); the ends
/// along the host apart (their indices).
pub fn planes_about(
    axes: [[f64; 3]; 3],
    ends: &[PlaneEnd],
    tol: f64,
) -> (Vec<JointPlane>, Vec<usize>) {
    let mut along = Vec::new();
    // (ψ, to the host, index)
    let mut psis: Vec<(f64, f64, usize)> = Vec::new();
    for (i, e) in ends.iter().enumerate() {
        match plane_angle(axes, e.direction) {
            Some((psi, to)) => psis.push((psi, to, i)),
            None => along.push(i),
        }
    }
    psis.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
    if psis.is_empty() {
        return (Vec::new(), along);
    }
    // On a circle of 180°: start after the widest gap, so that no plane
    // straddles the start.
    let n = psis.len();
    let mut start = 0;
    let mut widest = -1.0;
    for k in 0..n {
        let next = if k + 1 < n {
            psis[k + 1].0
        } else {
            psis[0].0 + 180.0
        };
        let gap = next - psis[k].0;
        if gap > widest + 1e-12 {
            widest = gap;
            start = (k + 1) % n;
        }
    }
    let ring: Vec<(f64, f64, usize)> = (0..n)
        .map(|k| {
            let (psi, to, i) = psis[(start + k) % n];
            let base = psis[start].0;
            (if psi < base { psi + 180.0 } else { psi }, to, i)
        })
        .collect();
    // Greedy: a plane takes the next end while the spread stays within
    // the tolerance.
    let mut groups: Vec<Vec<(f64, f64, usize)>> = Vec::new();
    for x in ring {
        match groups.last_mut() {
            Some(g) if x.0 - g[0].0 <= tol + 1e-9 => g.push(x),
            _ => groups.push(vec![x]),
        }
    }
    let [hx, _, _] = axes;
    let mut planes: Vec<JointPlane> = groups
        .into_iter()
        .map(|g| {
            let lo = g[0].0;
            let hi = g[g.len() - 1].0;
            let mut angle = 0.5 * (lo + hi);
            if angle >= 180.0 {
                angle -= 180.0;
            }
            let (flange, turn) = nearer_flange(angle);
            let mut members: Vec<InPlane> = g
                .iter()
                .map(|&(psi, to, i)| {
                    let psi = if psi >= 180.0 { psi - 180.0 } else { psi };
                    InPlane {
                        member: ends[i].member.clone(),
                        angle: round3(psi),
                        to_host: round3(to),
                        off: round3(off_flange(psi, to, flange, 0.0)),
                    }
                })
                .collect();
            let key = |m: &InPlane| {
                let e = ends.iter().find(|e| e.member == m.member);
                (
                    e.map_or(0.0, |e| e.area),
                    e.and_then(|e| e.force).unwrap_or(0.0),
                )
            };
            members.sort_by(|a, b| {
                let (ka, kb) = (key(a), key(b));
                kb.0.total_cmp(&ka.0).then(kb.1.total_cmp(&ka.1))
            });
            let area = g.iter().map(|x| ends[x.2].area).fold(0.0, f64::max);
            let force = g
                .iter()
                .filter_map(|x| ends[x.2].force.map(f64::abs))
                .reduce(f64::max);
            // The normal: across the plane, normal to the host's axis.
            let a = angle.to_radians();
            let inplane = add(scale(axes[1], a.cos()), scale(axes[2], a.sin()));
            let normal = unit(cross(hx, inplane));
            JointPlane {
                angle: round3(angle),
                normal,
                ends: members,
                spread: round3(hi - lo),
                area,
                force,
                flange,
                turn: round3(turn),
            }
        })
        .collect();
    planes.sort_by(|a, b| {
        b.area
            .total_cmp(&a.area)
            .then(b.force.unwrap_or(0.0).total_cmp(&a.force.unwrap_or(0.0)))
            .then(b.ends.len().cmp(&a.ends.len()))
            .then(a.angle.total_cmp(&b.angle))
    });
    (planes, along)
}

/// The planes of joint `j` about its host
/// ([`crate::joint_classes::joint_features`]); none where it has no host
/// with a profile. Forces: none (the structure file stores none; ordered by
/// size, then the number of ends).
pub fn joint_planes(j: &JointDetail) -> Option<HostPlanes> {
    joint_planes_with(j, &|_| None)
}

/// [`joint_planes`] with the members' forces where known (`force` of a
/// member, N): the planes of equal size ordered by force.
pub fn joint_planes_with(
    j: &JointDetail,
    force: &dyn Fn(&str) -> Option<f64>,
) -> Option<HostPlanes> {
    let f = crate::joint_classes::joint_features(j, crate::joint_order::Zone::Body);
    let host = f.host.clone()?;
    let ha = j.arrivals.iter().find(|a| a.member == host)?;
    let p = ha.profile.as_ref()?;
    // Seen from the joint along the host: x along it, its section's y, z.
    let axes = [ha.direction, p.axes[1], p.axes[2]];
    let run = f
        .ends
        .iter()
        .find(|e| e.host && e.member != host)
        .map(|e| e.member.clone());
    let ends: Vec<PlaneEnd> = f
        .ends
        .iter()
        .filter(|e| !e.host)
        .map(|e| PlaneEnd {
            member: e.member.clone(),
            direction: e.direction,
            area: e.area,
            force: force(&e.member),
        })
        .collect();
    let (planes, along) = planes_about(axes, &ends, PLANE_TOL_DEG);
    Some(HostPlanes {
        joint: j.id.clone(),
        host,
        run,
        axes,
        angle: p.angle.is_some() && p.parts.is_empty(),
        planes,
        along: along.into_iter().map(|i| ends[i].member.clone()).collect(),
    })
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(a: [f64; 3]) -> [f64; 3] {
    let n = dot(a, a).sqrt().max(1e-300);
    scale(a, 1.0 / n)
}
