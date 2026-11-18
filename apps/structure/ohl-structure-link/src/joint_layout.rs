// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The automatic joint layout (owner 2026-11-18: "click on a button and it
//! makes the necessary eccentricities and creates the plates on the joint
//! so that it's physically doable"), stage 2: the plane layout engine. A
//! pure, deterministic function of a joint in detail and the settings: the
//! same joint and settings give the same plan, so a run again over what a
//! run wrote changes nothing.
//!
//! The order is the owner's (2026-11-18): everything pulled back (the run
//! takes its values out first), the biggest profiles in line joined first
//! ([`joins_of`]: one part, or the smaller nested in the bigger), then the
//! other members by class and size, each bolted directly onto a flange of
//! the bigger where its bolts land, else onto a plate on that flange; an
//! end no roll lays flat (a welded plate in its own plane) is left for a
//! hand detail. The run names what an end bolts onto first
//! ([`hosts_to_name`]).
//!
//! For each member end bolting by one angle leg onto the flange of a
//! member at the joint (the leg, a chord, a diagonal a redundant meets),
//! not in line with it:
//!
//! 1. **Its plane.** The ends lying on one flange's outer face are one
//!    plane: a 2D problem in the flange's axes (along the member it bolts
//!    onto, across the flange from its heel).
//! 2. **Concentric first.** Its gauge line through the work point (the
//!    owner's choice: the gauge lines meet, the end's eccentricity is
//!    gauge to centroid), the end only pulled back (its cut) until its leg
//!    clears the flange's heel and every end placed before it by the gap
//!    (the ends in order: bracing before redundants, the wider leg first,
//!    the larger angle to what it bolts onto first, then by id).
//! 3. **A sideways shift** only where that needs more than the longest
//!    pull-back (or half the member): the smallest that fits, a millimetre
//!    at a time either way, up to the shift limit (the gauge line moved
//!    off the work point: an eccentricity the analysis takes).
//! 4. **Onto the flange or a plate.** Onto the flange directly when every
//!    end's bolts of the plane land on it (clear of its heel's leg and root
//!    by the nut, the edge distance from its toe); else every end of the
//!    plane onto one plate on that flange, each end's bolts past the
//!    flange's toe by the nut and the gap (the end pulled back further
//!    where needed).
//! 5. **No fit**: the joint is listed as needing a hand detail, with why;
//!    nothing of it is changed. Never forced.
//!
//! Bolts are not chosen here: an end's bolts are those given (or its
//! member's design data); else the connection preset for its class; else
//! the bolt picked once, two in one gauge line — those are *provisional*.
//! Members on both sides of one plate are never laid out automatically.

use std::collections::BTreeMap;

use crate::joints::{Arrival, BOLT_DRAWN, FLAT_OFF, JointDetail, Source, host_flange};
use ohl_structures::connections::Face;

/// The gap between the steel of members that do not bolt to each other,
/// to start with, m (an invented starting value, not a standard's: the
/// owner gave none, 2026-11-18).
pub const GAP_START: f64 = 0.010;
/// The longest pull-back (cut) before a shift is tried, to start with, m:
/// measured on the private real towers (2026-11-18, the larger of the
/// concentric cuts their bracing needs; see the theory page), not a
/// standard's value.
pub const PULL_BACK_START: f64 = 0.300;
/// The largest sideways shift of a gauge line off the work point, to
/// start with, m: measured on the private real towers (2026-11-18), not a
/// standard's value.
pub const SHIFT_LIMIT_START: f64 = 0.030;

/// What the layout is told.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutSettings {
    /// The gap between the steel of members that do not bolt to each
    /// other, m.
    pub gap: f64,
    /// The longest pull-back of a member's end before a shift is tried, m.
    pub pull_back: f64,
    /// The largest sideways shift of a gauge line off the work point, m.
    pub shift_limit: f64,
    /// The bolts of an end that has none, by its class (`bracing`,
    /// `redundant`…; the connection preset for the class), else
    /// [`LayoutSettings::bolt`].
    pub presets: BTreeMap<String, BoltArea>,
    /// The bolt picked once: two in one gauge line for an end without
    /// bolts and without a preset (none: such an end is left for a hand
    /// detail).
    pub bolt: Option<BoltArea>,
    /// The plates of a joint welded together (else each bolted to its
    /// flange of the leg, the default).
    pub welded: bool,
    /// The structure's default minimum bolt distances (third round,
    /// 2026-11-18; [`crate::lay_out`] takes the file's): an end's pitch and
    /// end distance not given, and the edge distance a bolt keeps in the
    /// flange where no code gives one — "default minimum, assumed, not a
    /// code's".
    pub defaults: ohl_structures::bolts::DefaultBoltMinimums,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            gap: GAP_START,
            pull_back: PULL_BACK_START,
            shift_limit: SHIFT_LIMIT_START,
            presets: BTreeMap::new(),
            bolt: None,
            welded: false,
            defaults: Default::default(),
        }
    }
}

/// The bolts the layout gives an end that has none (provisional).
#[derive(Debug, Clone, PartialEq)]
pub struct BoltArea {
    /// Where they come from, in words ("preset 2 × M16", "the bolt
    /// picked").
    pub from: String,
    /// The preset's name (none: the bolt picked).
    pub preset: Option<String>,
    /// How many.
    pub count: u32,
    /// Gauge lines.
    pub lines: u32,
    /// A bolt of the bolt libraries.
    pub bolt: Option<String>,
    /// Diameter, m.
    pub diameter: f64,
    /// Across the nut's flats, m (none: [`BOLT_DRAWN`]).
    pub flats: Option<f64>,
    /// Gauges from the heel, m (empty: the leg divided evenly, as drawn).
    pub gauges: Vec<f64>,
    /// Pitch, m (none: the default minimum spacing, three diameters to
    /// start with, as drawn).
    pub pitch: Option<f64>,
    /// End distance, m (none: the default minimum end distance, one and a
    /// half diameters to start with, as drawn).
    pub end: Option<f64>,
}

impl BoltArea {
    /// Two in one gauge line of a bolt of diameter `d` (`id`: a library
    /// bolt), the layout's provisional bolts.
    pub fn picked(id: Option<String>, d: f64, flats: Option<f64>) -> Self {
        Self {
            from: format!(
                "the bolt picked ({}), two in one line",
                id.clone()
                    .unwrap_or_else(|| format!("{:.0} mm", d * 1000.0))
            ),
            preset: None,
            count: 2,
            lines: 1,
            bolt: id,
            diameter: d,
            flats,
            gauges: Vec::new(),
            pitch: None,
            end: None,
        }
    }
}

/// What a member end's leg needs of the flange it bolts onto: the code's
/// minimum edge distance of a bolt in a part of thickness `t` (m), where no
/// code gives one the structure's default minimum; none (no bolts of known
/// diameter): the end's own edge distance, else the default minimum.
pub type EdgeRule<'a> = &'a dyn Fn(&Arrival, f64) -> Option<f64>;

/// Where a member end is to bolt.
#[derive(Debug, Clone, PartialEq)]
pub enum Onto {
    /// The member it bolts onto (its flange), directly.
    Member(String),
    /// The plate of [`JointPlan::plates`] at this place.
    Plate(usize),
}

/// What the layout gives a member end.
#[derive(Debug, Clone, PartialEq)]
pub struct EndPlan {
    /// The member.
    pub member: String,
    /// Where its steel stops from the work point, m (none: kept as given
    /// by hand).
    pub cut: Option<f64>,
    /// Its sideways shift (the connection's `shift`), m (none: kept as
    /// given by hand, or no direction to shift in).
    pub shift: Option<f64>,
    /// The gauge line's distance off the work point, m (0: concentric).
    pub off: f64,
    /// Drawn with its gauge line through the work point (the record's
    /// `gauge` mark): its normal framing eccentricity, gauge to centroid,
    /// for the drawings and the clash check only — the analysis keeps the
    /// member's axis through the work point and the code's slenderness
    /// curve takes that eccentricity (ASCE 10-97 C3.7.4); `shift` is only
    /// a move beyond it. False: drawn as before (a shift given by hand,
    /// or no direction to move in).
    pub on_gauge: bool,
    /// What it bolts onto.
    pub onto: Onto,
    /// The provisional bolts to give it (none: it has its own).
    pub bolts: Option<BoltArea>,
}

/// A plate the layout makes.
#[derive(Debug, Clone, PartialEq)]
pub struct PlatePlan {
    /// The member ends bolted onto it.
    pub members: Vec<String>,
    /// Its thickness, m: the thickest leg bolted on it.
    pub thickness: f64,
    /// The member whose flange it lies on.
    pub on: String,
    /// The flange's heel line it keeps to (a point on it and across the
    /// flange towards its toe, structure axes, m): its outline cut there
    /// by half the gap, clear of the other face's plates and members. None:
    /// a member on it leaves the joint over the heel (a crossing), not cut.
    pub heel: Option<([f64; 3], [f64; 3])>,
}

/// Two members in line through the joint joined (the owner's step 1,
/// 2026-11-18: "line up the biggest profiles and join them either
/// continuously if they are the same and aligned or with bolts together if
/// they can fit within each other").
#[derive(Debug, Clone, PartialEq)]
pub struct Join {
    /// The member whose end is written.
    pub member: String,
    /// The member it runs on into, or is nested in.
    pub onto: String,
    /// How.
    pub how: JoinKind,
}

/// How two members in line are joined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinKind {
    /// One part through the joint (`kind = "continuous"`): the same
    /// section and roll, and no bolts at either end (so no connection's
    /// check is left out).
    Continuous,
    /// The smaller angle nested in the bigger one, bolted to it
    /// (`face = "nested"`).
    Nested,
}

/// The layout of one joint.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JointPlan {
    /// The joint.
    pub joint: String,
    /// The members in line joined first (step 1; the run's, see
    /// [`joins_of`]).
    pub joins: Vec<Join>,
    /// What each member end laid out gets (in the joint's member order).
    pub ends: Vec<EndPlan>,
    /// The plates to make.
    pub plates: Vec<PlatePlan>,
    /// Why it needs a hand detail, each "member: why" (empty: laid out
    /// whole). The members named are left exactly as they are; the rest
    /// of the joint is laid out.
    pub hand: Vec<String>,
    /// The members left for a hand detail.
    pub hand_members: Vec<String>,
    /// The ends left as they are and why ("L2: in line with L1").
    pub left: Vec<String>,
    /// Notes ("D3: 2 provisional bolts, the bolt picked").
    pub notes: Vec<String>,
}

impl JointPlan {
    /// Something laid out (some members may be left for a hand detail).
    pub fn laid_out(&self) -> bool {
        !self.ends.is_empty() || !self.joins.is_empty()
    }

    /// Member `m` left for a hand detail, why.
    fn by_hand(&mut self, m: &str, why: String) {
        if !self.hand_members.iter().any(|x| x == m) {
            self.hand_members.push(m.to_owned());
        }
        self.hand.push(why);
    }
}

/// A member end in its plane, 2D: the plane's axes x along what it bolts
/// onto, y across the flange from its heel.
#[derive(Debug, Clone)]
struct Strip {
    member: String,
    /// The plane: its normal and offset (mm, rounded), and the member it
    /// lies on.
    plane: (i64, i64, i64, i64),
    host: String,
    /// The work point, 2D.
    w: [f64; 2],
    /// Along the member (unit), 2D.
    u: [f64; 2],
    /// Across it (unit), 2D: +90° from `u`.
    v: [f64; 2],
    /// Its leg's edges across it now, from the work point along `v`
    /// (lo ≤ hi), m.
    lat: [f64; 2],
    /// Its gauge lines across it now, from the work point along `v`, m.
    gauges: Vec<f64>,
    /// The data's shift per metre moved along `v` (none: no direction).
    shift_per: Option<f64>,
    /// Its shift now (given by hand), m.
    shift_now: f64,
    /// Its cut given by hand.
    hand_cut: Option<f64>,
    /// Its shift given by hand.
    hand_shift: bool,
    /// Half its length, m.
    half: f64,
    /// Bolts along it from where its steel stops: (along, line).
    bolts: Vec<(f64, usize)>,
    /// The bolts' diameter and nut's corner radius, m.
    d: f64,
    nut: f64,
    /// It lies on the flange's inner face (as the file turns it: its
    /// outstanding leg into the angle): clear of the angle's other leg and
    /// root, no plate laid out there.
    inside: bool,
    /// Bolts of the joint not laid out on this face (the leg's own at a
    /// splice…): their centres (2D) and how far each keeps (its head's or
    /// nut's corner radius), m.
    obstacles: Vec<([f64; 2], f64)>,
    /// The flange's heel (structure axes, m) and across it.
    heel: [f64; 3],
    across: [f64; 3],
    /// The flange: width, thickness, root, the edge a bolt keeps in it.
    width: f64,
    t: f64,
    r: f64,
    edge: f64,
    /// Its leg's thickness, m.
    own_t: f64,
    /// Order: class rank, wider first, larger angle first, id.
    rank: (u8, i64, i64),
    /// Provisional bolts to give it.
    give: Option<BoltArea>,
}

impl Strip {
    fn at(&self, s: f64, l: f64) -> [f64; 2] {
        [
            self.w[0] + self.u[0] * s + self.v[0] * l,
            self.w[1] + self.u[1] * s + self.v[1] * l,
        ]
    }
}

/// A strip placed: its cut and how far it was moved across.
#[derive(Debug, Clone)]
struct Placed {
    k: usize,
    cut: f64,
    moved: f64,
}

/// The layout of joint `j` by `s` (`edge`: the code's edge distances).
/// The joint as given: the values a run wrote before taken out first by
/// the caller (so that a run again gives the same), those given by hand
/// in (kept).
pub fn plan_joint(j: &JointDetail, s: &LayoutSettings, edge: EdgeRule<'_>) -> JointPlan {
    plan_joint_except(j, s, edge, &BTreeMap::new())
}

/// [`plan_joint`] with the members of `except` left for a hand detail,
/// each with its reason (a run's clash check: a member whose laid-out
/// steel, bolts or plate would run into something is left as it was and
/// the rest of the joint planned again without it; tuning on real towers,
/// 2026-11-18). Their bolts are kept clear of, as those of any member not
/// laid out.
pub fn plan_joint_except(
    j: &JointDetail,
    s: &LayoutSettings,
    edge: EdgeRule<'_>,
    except: &BTreeMap<String, String>,
) -> JointPlan {
    let mut plan = JointPlan {
        joint: j.id.clone(),
        ..Default::default()
    };
    let find = |m: &str| j.arrivals.iter().find(|a| a.member == m);
    let mut strips: Vec<Strip> = Vec::new();
    // The members not laid out here: those in line with what they bolt
    // onto, running on, no angle (their bolts are kept clear of).
    let others: Vec<&str> =
        j.arrivals
            .iter()
            .filter(|a| {
                a.continuous.is_some()
                    || a.profile.as_ref().is_none_or(|p| p.angle.is_none())
                    || a.onto.value.as_deref().and_then(find).is_some_and(|h| {
                        !(20.0..=160.0).contains(&angle_deg(a.direction, h.direction))
                    })
                    || except.contains_key(&a.member)
            })
            .map(|a| a.member.as_str())
            .collect();
    for a in &j.arrivals {
        if let Some(why) = except.get(&a.member) {
            plan.by_hand(&a.member, format!("{}: {why}", a.member));
            continue;
        }
        let host: Result<&Arrival, String> = (|| {
            let p = a.profile.as_ref();
            if p.is_none_or(|p| p.angle.is_none()) {
                return Err("no single angle".to_owned());
            }
            if let Some(o) = &a.continuous {
                return Err(format!("runs on into {o} as one part"));
            }
            let host = match a.onto.value.as_deref() {
                Some(o) => {
                    find(o).ok_or_else(|| format!("onto plate or splice {o}, given by hand"))?
                }
                None => through_member(j, a).ok_or("bolts onto nothing here")?,
            };
            let o = host.member.as_str();
            let angle = angle_deg(a.direction, host.direction);
            if !(20.0..=160.0).contains(&angle) {
                return Err(format!("in line with {o} (a splice or one part)"));
            }
            if a.sits.value == Some(Face::Nested)
                || (a.sits.source == Source::Given
                    && a.sits.value != Some(Face::Outside)
                    && a.sits.value != Some(Face::Inside))
            {
                return Err(format!(
                    "on the {} face of {o}, given by hand",
                    a.sits.value.map_or("?", Face::as_str)
                ));
            }
            if host_flange(a, host).is_none() {
                return Err(format!("{o} has no angle leg it lies on"));
            }
            Ok(host)
        })();
        let host = match host {
            Ok(h) => h,
            Err(w) => {
                plan.left.push(format!("{}: {w}", a.member));
                continue;
            }
        };
        // Laid flat first (Turn it flat): one lying off its face is the
        // caller's to turn; still off, a hand detail.
        if a.flat.is_some_and(|f| f.off > FLAT_OFF) {
            let why = format!(
                "{}: its bolted leg lies {:.0}° off the flange of {} it bolts onto and no turn \
                 lays it flat at both its ends (a bent plate or a hand detail)",
                a.member,
                a.flat.map_or(0.0, |f| f.off),
                host.member
            );
            plan.by_hand(&a.member, why);
            continue;
        }
        match strip_of(j, a, host, s, edge, &others) {
            Ok(x) => strips.push(x),
            Err(e) => plan.by_hand(&a.member, format!("{}: {e}", a.member)),
        }
    }
    // By plane, each in order.
    let mut planes: BTreeMap<(i64, i64, i64, i64), Vec<usize>> = BTreeMap::new();
    for (k, x) in strips.iter().enumerate() {
        planes.entry(x.plane).or_default().push(k);
    }
    let mut placed_all: Vec<Placed> = Vec::new();
    let mut plate_of: BTreeMap<usize, usize> = BTreeMap::new();
    for ks in planes.values() {
        let mut order = ks.clone();
        order.sort_by(|&a, &b| {
            strips[a]
                .rank
                .cmp(&strips[b].rank)
                .then(strips[a].member.cmp(&strips[b].member))
        });
        let mut placed: Vec<Placed> = Vec::new();
        for &k in &order {
            match place(&strips, k, &placed, s) {
                Ok(p) => placed.push(p),
                Err(e) => plan.by_hand(&strips[k].member, e),
            }
        }
        if placed.is_empty() {
            continue;
        }
        // Onto the flange directly where an end's bolts land on it (the
        // owner's step 2a: "on a surface of the bigger ones with bolts at
        // first"); else onto a plate on that flange (2b: "if too many are
        // needed add a plate"). The provisional bolts' plausibility rule:
        // every bolt of the group clear of the flange's other leg and root
        // by the nut, and the edge distance from its toe.
        let lands = |p: &Placed| {
            let x = &strips[p.k];
            bolt_ys(x, p)
                .iter()
                .all(|&y| y >= x.t + x.r + x.nut - 1e-9 && y <= x.width - x.edge + 1e-9)
        };
        let direct = placed.iter().all(lands);
        if !direct
            && find(&strips[placed[0].k].host)
                .and_then(|h| h.profile.as_ref())
                .is_some_and(|q| !q.parts.is_empty())
        {
            // On a built-up member (no plate laid out there, below) those
            // landing on its flange are bolted to it all the same, the
            // others left. (A plate for some ends of a flange and the
            // others directly on it, the plate on the other side of the
            // work point, was tried on two real towers, 2026-11-18: more
            // joints then clashed; every end of the plane goes onto the
            // plate.)
            let (on, off): (Vec<Placed>, Vec<Placed>) = placed.iter().cloned().partition(lands);
            placed_all.extend(on);
            placed = off;
        }
        if !direct {
            // A plate on a built-up member is drawn through the work point
            // (where it lies enters its checks, 2026-11-11), not on the
            // angle: not laid out automatically.
            let host = &strips[placed[0].k].host;
            if find(host)
                .and_then(|h| h.profile.as_ref())
                .is_some_and(|q| !q.parts.is_empty())
            {
                for p in &placed {
                    let m = strips[p.k].member.clone();
                    plan.by_hand(
                        &m,
                        format!(
                            "{m}: its bolts do not all land on {host}'s flange, and {host} is \
                             built up: a plate on it is drawn through the work point, not on its \
                             angle (lay it out by hand)"
                        ),
                    );
                }
                continue;
            }
            let mut kept: Vec<Placed> = Vec::new();
            for mut p in placed {
                let x = &strips[p.k];
                let need = past_toe(x, &p, s.gap);
                if need > p.cut + 1e-9 {
                    let limit = s.pull_back.min(x.half - bolt_run(x));
                    if need > limit + 1e-9 {
                        let why = format!(
                            "{}: on a plate its bolts need it pulled back {:.0} mm to pass the \
                             flange of {} (the longest pull-back {:.0} mm)",
                            x.member,
                            need.min(9.999) * 1000.0,
                            x.host,
                            limit * 1000.0
                        );
                        plan.by_hand(&x.member, why);
                        continue;
                    }
                    p.cut = need;
                }
                kept.push(p);
            }
            let placed = kept;
            if placed.is_empty() {
                continue;
            }
            let index = plan.plates.len();
            let members: Vec<String> = placed.iter().map(|p| strips[p.k].member.clone()).collect();
            let first = &strips[placed[0].k];
            // On an inner face the plate keeps clear of the angle's other
            // leg and its root (its heel line that far across).
            let heel = if first.inside {
                add(first.heel, scale(first.across, first.t + first.r))
            } else {
                first.heel
            };
            plan.plates.push(PlatePlan {
                members,
                thickness: placed.iter().map(|p| strips[p.k].own_t).fold(0.0, f64::max),
                on: first.host.clone(),
                heel: placed
                    .iter()
                    .all(|p| strips[p.k].u[1] > 1e-6)
                    .then_some((heel, first.across)),
            });
            for p in &placed {
                plate_of.insert(p.k, index);
            }
            placed_all.extend(placed);
            continue;
        }
        placed_all.extend(placed);
    }
    // In the joint's member order.
    placed_all.sort_by_key(|p| p.k);
    for p in placed_all {
        let x = &strips[p.k];
        // Concentric: the gauge line through the work point, drawn so (the
        // `gauge` mark), no shift; a move beyond it is the shift.
        let on_gauge = !x.hand_shift && x.shift_per.is_some();
        let concentric = if on_gauge {
            -x.gauges.first().copied().unwrap_or(0.0)
        } else {
            0.0
        };
        let shift = if x.hand_shift {
            None
        } else {
            x.shift_per
                .map(|per| round_to(x.shift_now + (p.moved - concentric) * per, 1e-4))
        };
        let off = x.gauges.first().map_or(0.0, |g| g + p.moved);
        if let Some(b) = &x.give {
            plan.notes.push(format!(
                "{}: {} provisional bolt(s), {}",
                x.member, b.count, b.from
            ));
        }
        plan.ends.push(EndPlan {
            member: x.member.clone(),
            cut: x.hand_cut.is_none().then_some(p.cut),
            shift,
            off,
            on_gauge,
            onto: match plate_of.get(&p.k) {
                Some(i) => Onto::Plate(*i),
                None => Onto::Member(x.host.clone()),
            },
            bolts: x.give.clone(),
        });
    }
    plan
}

/// The member `a` bolts onto when nothing says (no `onto`, none worked
/// out): of the members running through the joint (in line with another
/// of its members, or one part with it) and not in line with `a`, the one
/// part first, then by class (leg, bracing, redundant), the wider, the id.
fn through_member<'a>(j: &'a JointDetail, a: &Arrival) -> Option<&'a Arrival> {
    through_of(&j.arrivals, a)
}

/// The member ends of `j` the run names a member to bolt onto (in its
/// planning copy only, as the layout writes `onto`), each with that member,
/// in the joint's member order:
///
/// - an end bolting onto nothing named (no `onto` given or worked out):
///   the member running through the joint (`through_member`), so that
///   the end's leg bolted is the one lying on it and a turn that lays it
///   flat is found (tuning on real towers, 2026-11-18: such ends were left
///   with "its axis runs out of the face", measured against the leg that
///   is not bolted);
/// - an end whose `onto` is only worked out (the joint's leg) and whose
///   axis lies in neither flange's plane of that member (no turn lays it
///   flat there: a plan diagonal at a vertical post): a member running
///   through the joint in a flange's plane of which its axis lies (the
///   horizontal running through), if there is one (`host_in_plane`).
pub fn hosts_to_name(j: &JointDetail) -> Vec<(String, String)> {
    let find = |m: &str| j.arrivals.iter().find(|a| a.member == m);
    j.arrivals
        .iter()
        .filter(|a| {
            a.continuous.is_none()
                && a.profile.as_ref().is_some_and(|p| p.angle.is_some())
                && (a.onto.value.is_none() || a.onto.source == Source::WorkedOut)
        })
        .filter_map(|a| {
            let h0 = match a.onto.value.as_deref() {
                None => through_member(j, a),
                Some(o) => find(o),
            };
            let fits = |h: &Arrival| {
                angled(h) && (20.0..=160.0).contains(&angle_deg(a.direction, h.direction))
            };
            let h0 = h0.filter(|h| fits(h));
            if let Some(h) = h0.filter(|h| in_flange_plane(a, h)) {
                return a
                    .onto
                    .value
                    .is_none()
                    .then(|| (a.member.clone(), h.member.clone()));
            }
            match host_in_plane(&j.arrivals, a) {
                Some(h) if h0.is_none_or(|x| x.member != h.member) => {
                    Some((a.member.clone(), h.member.clone()))
                }
                _ => match (a.onto.value.is_none(), h0) {
                    (true, Some(h)) => Some((a.member.clone(), h.member.clone())),
                    _ => None,
                },
            }
        })
        .collect()
}

/// An angle, or a built-up section of angles.
fn angled(h: &Arrival) -> bool {
    h.profile
        .as_ref()
        .is_some_and(|p| p.angle.is_some() || !p.parts.is_empty())
}

/// `a`'s axis lies in the plane of a flange of `h` (a single angle; within
/// [`FLAT_OFF`]): a turn of `a` can lay it flat on that flange. A built-up
/// `h`: taken as yes (its angles' flanges, as the layout finds them).
fn in_flange_plane(a: &Arrival, h: &Arrival) -> bool {
    let Some(p) = h.profile.as_ref() else {
        return false;
    };
    if p.angle.is_none() {
        return !p.parts.is_empty();
    }
    let s = FLAT_OFF.to_radians().sin();
    dot(a.direction, p.axes[1]).abs() < s || dot(a.direction, p.axes[2]).abs() < s
}

/// Of the single angles running through the joint (in line with another
/// of its members, or one part with it) and not in line with `a`, one in a
/// flange's plane of which `a`'s axis lies; the one part first, then by
/// class (leg, bracing, redundant), the wider, the id (as
/// [`through_of`]).
fn host_in_plane<'a>(all: &'a [Arrival], a: &Arrival) -> Option<&'a Arrival> {
    let rank = |b: &Arrival| match b.class.as_deref() {
        Some("leg") => 0,
        Some("bracing") | Some("crossing") => 1,
        Some("redundant") => 2,
        _ => 3,
    };
    let width = |b: &Arrival| {
        b.profile
            .as_ref()
            .and_then(|p| p.angle)
            .map_or(0, |g| -((g.b.max(g.h) * 1e6) as i64))
    };
    all.iter()
        .filter(|b| b.member != a.member)
        .filter(|b| b.profile.as_ref().is_some_and(|p| p.angle.is_some()))
        .filter(|b| (20.0..=160.0).contains(&angle_deg(a.direction, b.direction)))
        .filter(|b| {
            b.continuous.is_some()
                || all
                    .iter()
                    .any(|c| c.member != b.member && angle_deg(b.direction, c.direction) > 160.0)
        })
        .filter(|b| in_flange_plane(a, b))
        .min_by(|x, y| {
            (x.continuous.is_none(), rank(x), width(x), &x.member).cmp(&(
                y.continuous.is_none(),
                rank(y),
                width(y),
                &y.member,
            ))
        })
}

/// The owner's step 1 at joint `j` (2026-11-18): of the single angles
/// running in line through it (within 20°), the biggest pair (the joint's
/// base: the larger legs, then the nearest in line, then by id; none where
/// a member runs on through the joint already) joined —
///
/// - **continuous** (one part) when they have the same section and lie
///   turned alike (their axes within about 1°), and neither end has bolts
///   (given or its member's: a splice whose bolts would then go unchecked
///   is left a bolted splice — on the safe side);
/// - **nested** (the smaller in the bigger, bolted to it) when the smaller
///   angle's legs fit inside the bigger's (each at most the bigger's leg
///   less its thickness and root radius) — their legs along each other
///   within the nesting tolerance, else not;
/// - else not joined (a splice of other sections: covers or a plate, by
///   hand).
///
/// Only ends with nothing given of how they are joined (`kind`, `face`,
/// `onto`, bolts given at the end), as the run takes them out first.
pub fn joins_of(j: &JointDetail) -> Vec<Join> {
    let free = |a: &Arrival| {
        a.continuous.is_none()
            && a.onto.source != Source::Given
            && a.sits.source != Source::Given
            && a.kind.source != Source::Given
            && a.bolts.source != Source::Given
            && !a.modelling
    };
    let angle = |a: &Arrival| a.profile.as_ref().and_then(|p| p.angle);
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for x in 0..j.arrivals.len() {
        for y in x + 1..j.arrivals.len() {
            let (a, b) = (&j.arrivals[x], &j.arrivals[y]);
            if angle(a).is_none() || angle(b).is_none() || !free(a) || !free(b) {
                continue;
            }
            let dev = 180.0 - angle_deg(a.direction, b.direction);
            if dev <= 20.0 {
                pairs.push((dev, x, y));
            }
        }
    }
    pairs.sort_by(|p, q| {
        p.0.total_cmp(&q.0).then_with(|| {
            (&j.arrivals[p.1].member, &j.arrivals[p.2].member)
                .cmp(&(&j.arrivals[q.1].member, &j.arrivals[q.2].member))
        })
    });
    // Only the biggest pair (the joint's base: its legs or chords; the
    // larger legs, then the nearest in line, then by id), and none where a
    // member already runs on through the joint: a pair crossing the base
    // bolts onto it (step 2), it is not one part across it.
    if j.arrivals.iter().any(|a| a.continuous.is_some()) {
        return Vec::new();
    }
    let size = |k: usize| {
        let g = angle(&j.arrivals[k]).expect("an angle");
        -((g.b + g.h) * 1e6).round() as i64
    };
    pairs.sort_by_key(|p| size(p.1).min(size(p.2)));
    pairs.truncate(1);
    let mut used: Vec<usize> = Vec::new();
    let mut out = Vec::new();
    for (_, x, y) in pairs {
        if used.contains(&x) || used.contains(&y) {
            continue;
        }
        let (a, b) = (&j.arrivals[x], &j.arrivals[y]);
        let (pa, pb) = (
            a.profile.as_ref().expect("an angle"),
            b.profile.as_ref().expect("an angle"),
        );
        let (ga, gb) = (pa.angle.expect("an angle"), pb.angle.expect("an angle"));
        let bolted = |z: &Arrival| z.bolts.value.as_ref().is_some_and(|v| v.count > 0);
        let alike = |u: [f64; 3], v: [f64; 3]| dot(u, v) > 0.9998;
        let how = if pa.section_id == pb.section_id
            && alike(pa.axes[1], pb.axes[1])
            && alike(pa.axes[2], pb.axes[2])
        {
            (!bolted(a) && !bolted(b)).then_some((x, y, JoinKind::Continuous))
        } else {
            let fits = |s: crate::joints::Angle, g: crate::joints::Angle| {
                s.b <= g.b - g.t - g.r + 1e-9 && s.h <= g.h - g.t - g.r + 1e-9
            };
            // Legs along each other (within the nesting tolerance).
            let along = |p: &crate::joints::Profile, q: &crate::joints::Profile| {
                let c = crate::joints::NESTED_ANGLE_DEG.to_radians().cos();
                dot(p.axes[1], q.axes[1]) >= c && dot(p.axes[2], q.axes[2]) >= c
            };
            if fits(ga, gb) && along(pa, pb) {
                Some((x, y, JoinKind::Nested))
            } else if fits(gb, ga) && along(pa, pb) {
                Some((y, x, JoinKind::Nested))
            } else {
                None
            }
        };
        if let Some((m, o, how)) = how {
            used.extend([x, y]);
            // Continuous: the end of the smaller id says it.
            let (m, o) =
                if how == JoinKind::Continuous && j.arrivals[o].member < j.arrivals[m].member {
                    (o, m)
                } else {
                    (m, o)
                };
            out.push(Join {
                member: j.arrivals[m].member.clone(),
                onto: j.arrivals[o].member.clone(),
                how,
            });
        }
    }
    out
}

/// [`through_member`] among the member ends `all` of a joint (also where a
/// plate lies when its members name no leg, [`crate::joints`]).
pub(crate) fn through_of<'a>(all: &'a [Arrival], a: &Arrival) -> Option<&'a Arrival> {
    let rank = |b: &Arrival| match b.class.as_deref() {
        Some("leg") => 0,
        Some("bracing") | Some("crossing") => 1,
        Some("redundant") => 2,
        _ => 3,
    };
    let width = |b: &Arrival| {
        b.profile
            .as_ref()
            .and_then(|p| p.angle)
            .map_or(0, |g| -((g.b.max(g.h) * 1e6) as i64))
    };
    all.iter()
        .filter(|b| b.member != a.member && b.profile.is_some())
        .filter(|b| (20.0..=160.0).contains(&angle_deg(a.direction, b.direction)))
        .filter(|b| {
            b.continuous.is_some()
                || all
                    .iter()
                    .any(|c| c.member != b.member && angle_deg(b.direction, c.direction) > 160.0)
        })
        .min_by(|x, y| {
            (x.continuous.is_none(), rank(x), width(x), &x.member).cmp(&(
                y.continuous.is_none(),
                rank(y),
                width(y),
                &y.member,
            ))
        })
}

/// The angle between two directions, degrees.
fn angle_deg(a: [f64; 3], b: [f64; 3]) -> f64 {
    dot(a, b).clamp(-1.0, 1.0).acos().to_degrees()
}

/// The turns that lay the member ends of `j` flat on what they bolt onto
/// (Turn it flat, [`crate::Problem::NotFlat`]): member, turn (radians, to
/// add to its roll); in the joint's member order, each member once.
pub fn flat_turns(j: &JointDetail) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = Vec::new();
    for a in &j.arrivals {
        for (p, _) in &a.problems {
            if let crate::Problem::NotFlat { turn, .. } = p
                && !out.iter().any(|(m, _)| m == &a.member)
            {
                out.push((a.member.clone(), *turn));
            }
        }
    }
    out
}

/// A length as the files write it: millimetres to a tenth (`"30 mm"`,
/// `"12.5 mm"`; as `ohl_structure_edit::connection_edits::mm`).
pub fn mm_text(v: f64) -> String {
    let v = (v * 1e4).round() / 10.0;
    if v.fract() == 0.0 {
        format!("{v:.0} mm")
    } else {
        format!("{v} mm")
    }
}

/// `v` to the nearest multiple of `step`.
fn round_to(v: f64, step: f64) -> f64 {
    let r = (v / step).round() * step;
    if r == 0.0 { 0.0 } else { r }
}

/// `v` up to the next multiple of `step`.
fn ceil_to(v: f64, step: f64) -> f64 {
    let r = ((v / step) - 1e-9).ceil() * step;
    if r == 0.0 { 0.0 } else { r }
}

/// The strip of member end `a` on `host`'s flange.
fn strip_of(
    j: &JointDetail,
    a: &Arrival,
    host: &Arrival,
    s: &LayoutSettings,
    edge: EdgeRule<'_>,
    others: &[&str],
) -> Result<Strip, String> {
    let inside = a.sits.value == Some(Face::Inside);
    let p = a.profile.as_ref().expect("checked");
    let g = p.angle.expect("checked");
    let f = host_flange(a, host).ok_or("no flange to lie on")?;
    let x3 = host.direction;
    let y3 = f.across;
    let to2 = |q: [f64; 3]| -> [f64; 2] {
        let d = sub(q, f.heel);
        [dot(d, x3), dot(d, y3)]
    };
    let dir2 = |q: [f64; 3]| -> [f64; 2] { [dot(q, x3), dot(q, y3)] };
    let u = dir2(a.direction);
    let lu = (u[0] * u[0] + u[1] * u[1]).sqrt();
    // Its axis along the flange's face (within the flatness allowed).
    if lu < FLAT_OFF.to_radians().cos() {
        return Err(format!(
            "its axis runs {:.0}° out of the face of {}'s flange it bolts onto",
            lu.clamp(-1.0, 1.0).acos().to_degrees(),
            host.member
        ));
    }
    let u = [u[0] / lu, u[1] / lu];
    let v = [-u[1], u[0]];
    // On the inner face, leaving the joint towards the heel: it would run
    // across the angle's other leg.
    if inside && u[1] < -1e-6 {
        return Err(format!(
            "on the inner face of {}'s flange it runs towards the angle's other leg (turn it, \
             or lay it out by hand)",
            host.member
        ));
    }
    let w = to2([0.0; 3]);
    // Its leg bolted: from its heel across, where it is drawn.
    let heel3 = add(
        a.offset,
        add(scale(p.axes[1], g.heel[0]), scale(p.axes[2], g.heel[1])),
    );
    let across3 = if a.leg_y { p.axes[1] } else { p.axes[2] };
    let leg_w = if a.leg_y { g.b } else { g.h };
    let h2 = to2(heel3);
    let lat0 = dot2(sub2(h2, w), v);
    let sv = dot2(dir2(across3), v).signum();
    let lat = {
        let (x, y) = (lat0, lat0 + sv * leg_w);
        [x.min(y), x.max(y)]
    };
    // The bolts: its own (given, its member's), else provisional.
    let (area, give) = match &a.bolts.value {
        Some(b) if a.bolts.known() => (
            BoltArea {
                from: "its own".into(),
                preset: None,
                count: b.count,
                lines: a.gauge_lines.value.unwrap_or(1).max(1),
                bolt: b.bolt.clone(),
                diameter: b.diameter,
                flats: b.drawn[0],
                gauges: a.gauges.value.clone().unwrap_or_default(),
                pitch: a.pitch.value,
                end: a.end_distance.value,
            },
            None,
        ),
        _ => {
            let pick = a
                .class
                .as_ref()
                .and_then(|c| s.presets.get(c))
                .or(s.bolt.as_ref())
                .ok_or("no bolts, no preset for its class and no bolt picked")?;
            (pick.clone(), Some(pick.clone()))
        }
    };
    let d = area.diameter;
    let lines = area.lines.max(1) as usize;
    let gauges: Vec<f64> = if area.gauges.len() == lines {
        area.gauges.clone()
    } else {
        (1..=lines)
            .map(|k| leg_w * k as f64 / (lines as f64 + 1.0))
            .collect()
    };
    let [end_min, spacing_min, edge_min] = s.defaults.of(d);
    let pitch = area.pitch.unwrap_or(spacing_min);
    let end = area.end.unwrap_or(end_min);
    // As the drawing places them (`joints::place_bolts`): along from the
    // steel's end, every second line staggered by half a pitch.
    let bolts: Vec<(f64, usize)> = (0..area.count)
        .map(|k| {
            let line = (k as usize) % lines;
            let row = (k as usize / lines) as f64;
            let stagger = if line % 2 == 1 { pitch / 2.0 } else { 0.0 };
            (end + row * pitch + stagger, line)
        })
        .collect();
    let flats = area.flats.unwrap_or(BOLT_DRAWN[0] * d);
    let nut = flats / 3f64.sqrt();
    let edge = edge(a, f.t)
        .or(a.edge_distance.value.filter(|_| a.edge_distance.known()))
        .unwrap_or(edge_min);
    // The data's shift: along `shift.1`, per metre along `v`.
    // (Towards the member it is to bolt onto, as the drawing measures it
    // once `onto` names it: `ohl_structures::connections::shift_direction`.)
    let shift_per = ohl_structures::connections::shift_direction(a.direction, host.direction)
        .and_then(|sd| {
            let k = dot2(dir2(sd), v);
            (k.abs() > 0.2).then(|| 1.0 / k)
        });
    let n = f.out;
    let off = dot(add(j.position, f.heel), n) - if inside { f.t } else { 0.0 };
    let mm = |x: f64| (x * 1000.0).round() as i64;
    let rank = match a.class.as_deref() {
        Some("leg") => 0,
        Some("bracing") | Some("crossing") => 1,
        Some("redundant") => 2,
        _ => 3,
    };
    let angle = a.angle.as_ref().map_or(0.0, |x| x.1.min(180.0 - x.1));
    // The bolts of the joint's members not laid out here lying on this face
    // (within a thickness or two of it, through it): kept clear of.
    let face_at = add(
        add(j.position, f.heel),
        scale(f.out, if inside { -f.t } else { 0.0 }),
    );
    let mut obstacles = Vec::new();
    for b in &j.arrivals {
        if b.member == a.member || !others.contains(&b.member.as_str()) {
            continue;
        }
        let Some(bs) = &b.bolts.value else { continue };
        let r = bs.drawn[0].unwrap_or(BOLT_DRAWN[0] * bs.diameter) / 3f64.sqrt();
        for (q, axis, _) in b.bolts_with_axes() {
            let q = add(j.position, q);
            if dot(axis, f.out).abs() < 0.9 || dot(sub(q, face_at), f.out).abs() > 0.03 {
                continue;
            }
            obstacles.push((to2(sub(q, j.position)), r));
        }
    }
    Ok(Strip {
        inside,
        obstacles,
        heel: add(j.position, f.heel),
        across: f.across,
        member: a.member.clone(),
        plane: (mm(n[0] * 1.0), mm(n[1] * 1.0), mm(n[2] * 1.0), mm(off)),
        host: host.member.clone(),
        w,
        u,
        v,
        lat,
        gauges: gauges.iter().map(|gg| lat0 + sv * gg).collect(),
        shift_per,
        shift_now: a.shift.0,
        hand_cut: (a.cut.source == Source::Given).then(|| a.cut.value.unwrap_or(0.0)),
        hand_shift: a.shift.0 != 0.0,
        half: a.length / 2.0,
        bolts,
        d,
        nut,
        width: f.width,
        t: f.t,
        r: f.r,
        edge,
        own_t: g.t,
        rank: (rank, -mm(leg_w), -mm(angle)),
        give,
    })
}

/// The length its bolts take along it from where its steel stops, m.
fn bolt_run(x: &Strip) -> f64 {
    x.bolts.iter().map(|b| b.0).fold(0.0, f64::max) + x.d
}

/// Each bolt's y (across the flange from its heel) with `p`.
fn bolt_ys(x: &Strip, p: &Placed) -> Vec<f64> {
    x.bolts
        .iter()
        .map(|(along, line)| {
            let l = x.gauges.get(*line).copied().unwrap_or(0.0) + p.moved;
            x.at(p.cut + along, l)[1]
        })
        .collect()
}

/// The cut that takes every bolt of `x` past the flange by the nut and
/// `gap` (on a plate: the nuts clear of the flange behind it): past its
/// toe for a member leaving towards it, past its heel for one leaving
/// towards that (a crossing diagonal).
fn past_toe(x: &Strip, p: &Placed, gap: f64) -> f64 {
    let mut need = p.cut;
    if x.u[1].abs() <= 1e-6 {
        return f64::INFINITY;
    }
    for (along, line) in &x.bolts {
        let l = x.gauges.get(*line).copied().unwrap_or(0.0) + p.moved;
        let y0 = x.at(*along, l)[1];
        let c = if x.u[1] > 0.0 {
            (x.width + x.nut + gap - y0) / x.u[1]
        } else {
            (y0 + x.nut + gap) / -x.u[1]
        };
        need = need.max(c);
    }
    ceil_to(need, 0.001)
}

/// Strip `k` placed against those `placed` before it: concentric and
/// pulled back, else shifted, else why not.
fn place(
    strips: &[Strip],
    k: usize,
    placed: &[Placed],
    s: &LayoutSettings,
) -> Result<Placed, String> {
    let x = &strips[k];
    let limit = s.pull_back.min(x.half - bolt_run(x));
    if limit <= 0.0 {
        return Err(format!(
            "{}: too short for its bolts ({:.0} mm to half its length)",
            x.member,
            x.half * 1000.0
        ));
    }
    // Concentric: its first gauge line through the work point.
    let concentric = if x.hand_shift || x.shift_per.is_none() {
        0.0
    } else {
        -x.gauges.first().copied().unwrap_or(0.0)
    };
    let cut_at = |moved: f64| -> (f64, Option<String>) {
        let lat = [x.lat[0] + moved, x.lat[1] + moved];
        // Its leg on the flange's side of the heel (a member leaving the
        // joint towards the toe, as bracing does at a leg; one leaving towards
        // the heel crosses over it, as a crossing diagonal: nothing of the
        // host there — the clash check sees the rest).
        let mut need = f64::NEG_INFINITY;
        let mut by: Option<String> = None;
        for l in lat {
            // Inside: clear of the angle's other leg and its root too.
            let floor = if x.inside {
                x.t + x.r + s.gap / 2.0
            } else {
                0.0
            };
            let y0 = x.at(0.0, l)[1] - floor;
            let c = if x.u[1] > 1e-6 {
                -y0 / x.u[1]
            } else if x.u[1] >= -1e-6 && y0 < 0.0 {
                f64::INFINITY
            } else {
                f64::NEG_INFINITY
            };
            if c > need {
                need = c;
                by = Some(format!("the heel of {}", x.host));
            }
        }
        // The bolts not laid out on this face: each a disc to keep clear of.
        for (c2, r) in &x.obstacles {
            let rel = sub2(*c2, x.w);
            let (along, across) = (dot2(rel, x.u), dot2(rel, x.v));
            let reach = r + s.gap / 2.0;
            if across + reach > lat[0] - s.gap / 2.0 && across - reach < lat[1] + s.gap / 2.0 {
                let c = along + reach + s.gap / 2.0;
                if c > need {
                    need = c;
                    by = Some("a bolt already there".to_owned());
                }
            }
        }
        for q in placed {
            let b = &strips[q.k];
            let c = clear_of(x, lat, b, q, s.gap);
            if c > need {
                need = c;
                by = Some(b.member.clone());
            }
        }
        (need.max(0.0), by)
    };
    if let Some(c) = x.hand_cut {
        let (need, by) = cut_at(concentric);
        if need > c + 1e-4 {
            return Err(format!(
                "{}: its cut given by hand ({:.0} mm) leaves it on {} (it needs {:.0} mm)",
                x.member,
                c * 1000.0,
                by.unwrap_or_default(),
                need * 1000.0
            ));
        }
        return Ok(Placed {
            k,
            cut: c,
            moved: concentric,
        });
    }
    let (need, by) = cut_at(concentric);
    if need <= limit {
        return Ok(Placed {
            k,
            cut: ceil_to(need, 0.001),
            moved: concentric,
        });
    }
    // A shift: the smallest that fits, a millimetre at a time.
    if x.shift_per.is_some() && !x.hand_shift {
        let steps = (s.shift_limit * 1000.0 + 1e-9).floor() as i64;
        for n in 1..=steps {
            for sign in [1.0, -1.0] {
                let moved = concentric + sign * n as f64 * 0.001;
                let (c, _) = cut_at(moved);
                if c <= limit {
                    return Ok(Placed {
                        k,
                        cut: ceil_to(c, 0.001),
                        moved,
                    });
                }
            }
        }
    }
    let by = by.unwrap_or_default();
    let angle = strips
        .iter()
        .find(|b| b.member == by)
        .map(|b| (dot2(b.u, x.u)).clamp(-1.0, 1.0).acos().to_degrees());
    Err(match angle {
        Some(deg) => format!(
            "{} and {by} within {deg:.0}°: no room on one plate ({} needs {:.0} mm pulled back, \
             the longest {:.0} mm; no shift up to {:.0} mm fits)",
            x.member,
            x.member,
            need.min(9.999) * 1000.0,
            limit * 1000.0,
            s.shift_limit * 1000.0
        ),
        None => format!(
            "{}: {:.0} mm pulled back to clear {by} (the longest {:.0} mm; no shift up to \
             {:.0} mm fits)",
            x.member,
            need.min(9.999) * 1000.0,
            limit * 1000.0,
            s.shift_limit * 1000.0
        ),
    })
}

/// The least cut of `x` (its leg across `lat`) that keeps it `gap` clear
/// of strip `b` placed as `q`: the farthest point along `x` of where
/// their strips (each widened by half the gap, `b` from its cut less
/// half the gap) overlap, and half the gap. Minus infinity: they never
/// meet; infinity: they run together (parallel and overlapping).
fn clear_of(x: &Strip, lat: [f64; 2], b: &Strip, q: &Placed, gap: f64) -> f64 {
    const FAR: f64 = 50.0;
    let h = gap / 2.0;
    let poly = vec![
        x.at(-FAR, lat[0] - h),
        x.at(FAR, lat[0] - h),
        x.at(FAR, lat[1] + h),
        x.at(-FAR, lat[1] + h),
    ];
    let blat = [b.lat[0] + q.moved - h, b.lat[1] + q.moved + h];
    // b's half-planes: across within blat, along from its cut less h.
    let rel = |p: [f64; 2]| sub2(p, b.w);
    let poly = clip(&poly, |p| dot2(rel(p), b.v) - blat[0]);
    let poly = clip(&poly, |p| blat[1] - dot2(rel(p), b.v));
    let poly = clip(&poly, |p| dot2(rel(p), b.u) - (q.cut - h));
    if poly.is_empty() {
        return f64::NEG_INFINITY;
    }
    let far = poly
        .iter()
        .map(|p| dot2(sub2(*p, x.w), x.u))
        .fold(f64::NEG_INFINITY, f64::max);
    if far >= FAR - 1.0 {
        f64::INFINITY
    } else {
        far + h
    }
}

/// `poly` (convex, 2D) clipped to where `f` ≥ 0 (`f` linear).
fn clip(poly: &[[f64; 2]], f: impl Fn([f64; 2]) -> f64) -> Vec<[f64; 2]> {
    let n = poly.len();
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (fa, fb) = (f(a), f(b));
        if fa >= 0.0 {
            out.push(a);
        }
        if (fa >= 0.0) != (fb >= 0.0) {
            let t = fa / (fa - fb);
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out
}

fn sub2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
