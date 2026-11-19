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
//! ([`joins_of`]: one part, the smaller nested in the bigger, or — other
//! sections that do not nest, step 1c, 2026-11-19 — a splice of covers),
//! then the other members by class and size, each bolted directly onto a
//! flange of the bigger where its bolts land, else onto a plate on that
//! flange; an end no roll lays flat onto a plate in its own plane welded
//! to that flat plate (step 4, 2026-11-19: [`OwnPlane`]), where that puts
//! it at most a plate's thickness farther out of the flange than a flat
//! plate would, else a hand detail. The run names what an end bolts onto
//! first ([`hosts_to_name`]); a pair of one section in line with bolts is
//! a splice whose place is not known, said ([`splice_unknown`]).
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
//! **Searched** (J1, 2026-11-19, [`plan_joint_with`]): the planes on the
//! biggest members first, each plane's ends placed in every order that can
//! matter (the owner's order first, backtracking where one does not fit),
//! clear of the joint's fixed steel (what is not laid out) and of what the
//! planes before laid out; the most ends, then the least eccentricity, then
//! the owner's order win; at most [`NODE_LIMIT`] placings per joint. A
//! member left for its own sake keeps its place as it is (J2,
//! [`JointPlan::kept_free`]): the ends laid out keep clear of it.
//!
//! Bolts are not chosen here: an end's bolts are those given (or its
//! member's design data); else the connection preset for its class; else
//! the bolt picked once, two in one gauge line — those are *provisional*.
//! Members on both sides of one plate are never laid out automatically.
//!
//! **Step 4, a welded plate** (owner 2026-11-18: "if the angle at which the
//! profile is coming really doesn't allow for it then add a weleded plate
//! that is also bolted to the other profiles with the other part of the
//! plate"): an end whose leg bolted lies more than [`FLAT_OFF`] off the
//! flange is laid out on the flange in whose plane its axis lies (else the
//! other; its inner face when it comes from the flange's other side) as
//! its footprint on that plane; its plate lies in its own plane from its
//! bolts down to the line where that plane meets the flat plate's face
//! (the seam), the member lifted along the flange's normal to lie on it;
//! the flat plate covers the seam and the flange and keeps to its side of
//! the seam; the weld both sides, its throat [`weld_throat`].

use std::collections::{BTreeMap, BTreeSet};

use crate::joints::{
    Arrival, BOLT_DRAWN, FLAT_OFF, JointDetail, Source, host_flange, host_flange_along,
};
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
    /// Bent plates laid out (J2b, 2026-11-19; [`BendPlan`]): on by
    /// default. Off: as before them — the run plans its half turns so, the
    /// owner's order being the flat plate first.
    pub bent_plates: bool,
    /// One joint of each kind planned and its plan applied, mapped, to the
    /// joints like it (J4, 2026-11-19; [`crate::joint_kinds`]): on by
    /// default. Off: every joint planned on its own (as before; for the
    /// comparison).
    pub once_per_kind: bool,
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
            bent_plates: true,
            once_per_kind: true,
        }
    }
}

/// The inside bend radius of a bent plate the layout makes, in the plate's
/// thicknesses (J2b, 2026-11-19): **the program's starting value, not a
/// standard's** — the least radius a plate may be bent to cold depends on
/// its steel, thickness and the bend's direction to its rolling, which the
/// steel's own rules give (to check by hand; a value given on a bend is
/// kept).
pub const BEND_RADIUS_RATIO: f64 = 2.0;

/// The inside bend radius of a bent plate `t` thick the layout makes, m:
/// [`BEND_RADIUS_RATIO`] thicknesses, up to the millimetre.
pub fn bend_radius(t: f64) -> f64 {
    ceil_to(BEND_RADIUS_RATIO * t, 0.001)
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

/// What a member end's bolts need of a part of thickness `t` (m) they pass
/// through: the code's minimum edge and end distances (edge, end), where no
/// code gives them the structure's default minimums; none (no bolts of
/// known diameter): the end's own, else the default minimums.
pub type EdgeRule<'a> = &'a dyn Fn(&Arrival, f64) -> Option<(f64, f64)>;

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
    /// A plate in its member's own plane, welded to a flat plate of this
    /// plan (the owner's step 4, 2026-11-19); none: a flat plate on the
    /// flange (step 2b).
    pub own: Option<OwnPlane>,
    /// A flat plate with plates in their members' own planes welded to
    /// it: what its outline must also cover and keep to.
    pub reach: Option<Reach>,
    /// Its outline as laid out (structure axes, m; a bent plate's flat on
    /// the flange, J2b); none: worked out round its bolts after writing.
    pub outline: Option<Vec<[f64; 3]>>,
    /// Its own bolts onto the member it lies on (a bent plate's flat on the
    /// flange, J2b: `[plate.leg_bolts]`); none: held by its members' bolts.
    pub hold: Option<HoldPlan>,
}

/// A plate's own bolts onto the member whose flange it lies on (J2b,
/// 2026-11-19: a bent plate's flat on that flange, so that the flat is
/// bolted to it as the owner asked; written as `[plate.leg_bolts]`).
#[derive(Debug, Clone, PartialEq)]
pub struct HoldPlan {
    /// The member.
    pub member: String,
    /// Its leg the plate lies on: `long` or `short` (none: an equal-leg
    /// angle, worked out from the plate's face).
    pub leg: Option<&'static str>,
    /// The bolts (provisional when `provisional`).
    pub bolt: BoltArea,
    /// Provisional: the layout's (the bolt picked or a preset's), until the
    /// codes choose them.
    pub provisional: bool,
    /// Each bolt along the member from the work point and across its flange
    /// from the heel, m.
    pub points: Vec<[f64; 2]>,
}

/// A bent plate's bend (J2b, 2026-11-19): the plate in its members' own
/// plane ([`OwnPlane`] with a bend) is one plate with the flat on the
/// flange, folded along `line`.
#[derive(Debug, Clone, PartialEq)]
pub struct BendPlan {
    /// The angle the plate turns through, degrees.
    pub angle: f64,
    /// Its inside radius, m ([`bend_radius`]).
    pub inner_radius: f64,
    /// Where the two flats' middle planes meet (structure axes, m): two
    /// points, over the flat in the members' plane.
    pub line: [[f64; 3]; 2],
}

/// What a flat plate's outline must cover and keep to where plates in
/// their members' own planes are welded to it (step 4).
#[derive(Debug, Clone, PartialEq)]
pub struct Reach {
    /// Points it must cover (structure axes, m, on its face away from the
    /// flange): the seams and the flange under them.
    pub points: Vec<[f64; 3]>,
    /// Its normal from that face into the flange (unit).
    pub normal: [f64; 3],
    /// The side of each seam it keeps to: a point on the seam and, in its
    /// plane, square to the seam towards the plate (unit).
    pub keep_to: Vec<([f64; 3], [f64; 3])>,
}

/// A plate in its member's own plane (the owner's step 4, 2026-11-19: "if
/// the angle at which the profile is coming really doesn't allow for it
/// then add a welded plate that is also bolted to the other profiles with
/// the other part of the plate"): its member's leg bolted onto it, its
/// edge welded to the flat plate on the flange along the line where the
/// two planes meet — a plate bent there, made of two plates welded. Or
/// (J2b, 2026-11-19, `bend`) the flat of a bent plate, folded from the flat
/// on the flange: no weld.
#[derive(Debug, Clone, PartialEq)]
pub struct OwnPlane {
    /// The flat plate it is welded to, or bent from ([`JointPlan::plates`]).
    pub welded_to: usize,
    /// Its outline (structure axes, m), in the face its member lies on.
    pub outline: Vec<[f64; 3]>,
    /// How far its member's leg bolted lies off the flange, degrees.
    pub off: f64,
    /// The member lifted off the flange along the flange's normal to lie
    /// on it (drawing only), m.
    pub lift: f64,
    /// How much farther out of the flange its axis lies at its bolts than
    /// on a flat plate of the same thickness, m.
    pub out_more: f64,
    /// The weld's throat, each side, m: welded from both sides along the
    /// seam (partly penetrated, checked as fillet welds: SIA 263 6.3.3.5).
    pub weld_size: f64,
    /// The weld's length, both sides together, m.
    pub weld_length: f64,
    /// Bent from the flat plate (J2b), not welded to it (its weld's size and
    /// length then 0).
    pub bend: Option<BendPlan>,
}

/// Two members in line through the joint joined (the owner's step 1,
/// 2026-11-18: "line up the biggest profiles and join them either
/// continuously if they are the same and aligned or with bolts together if
/// they can fit within each other").
#[derive(Debug, Clone, PartialEq)]
pub struct Join {
    /// The member whose end is written.
    pub member: String,
    /// The member it runs on into, or is nested in (spliced: the other).
    pub onto: String,
    /// How.
    pub how: JoinKind,
    /// Spliced: the angles' legs its covers lie on (outer and inner on
    /// each) and their thickness, m (the thicker member's leg); else none.
    pub covers: Vec<ohl_structures::connections::CoverLeg>,
    /// Spliced: the covers' thickness, m.
    pub thickness: f64,
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
    /// Two angles of different sections that do not nest, each bolted
    /// onto a splice of covers on both faces of its legs (`[[splice]]`;
    /// the owner's step 1c, 2026-11-19).
    Spliced,
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
    /// Of those, the ones whose place is kept free (J2, 2026-11-19): left
    /// for their own sake (no roll lays them flat, too short, built up…),
    /// as they are, and every end laid out keeps clear of them. The others
    /// (no room beside an end laid out, a clash) are said with what they
    /// run into.
    pub kept_free: Vec<String>,
    /// The ends left as they are and why ("L2: in line with L1").
    pub left: Vec<String>,
    /// Notes ("D3: 2 provisional bolts, the bolt picked").
    pub notes: Vec<String>,
    /// Of the members left for a hand detail, those whose bolted leg lies
    /// off the flange they bolt onto (no roll lays it flat): a plate in
    /// their own plane would be needed (the owner's step 4), with the most
    /// that plate may move the member's axis farther out of the flange
    /// (step 4's limit: its leg's thickness, m) — the plate case of the
    /// estimated eccentricities' upper bound (O45 A1,
    /// [`crate::eccentricities`]).
    pub off_flange: Vec<(String, f64)>,
    /// Of the members left for a hand detail, what each would bolt onto
    /// (the member named, else the one running through the joint): the
    /// direction of the estimated eccentricities' upper bound (O45 A1).
    pub hand_hosts: Vec<(String, String)>,
    /// Laid out once for its kind (J4, 2026-11-19): the joint whose plan
    /// this is, mapped here ([`crate::joint_kinds`]); none: planned on its
    /// own.
    pub like: Option<String>,
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
    /// No roll lays it flat here: onto a plate in its own plane, welded
    /// to the flat plate on the flange (step 4). Its `lat` and `gauges`
    /// are then where it is drawn with its gauge line through the work
    /// point (no move needed for that).
    tilted: Option<Tilted>,
    /// Its plane in 3D, from the work point: the face it lies on (a point
    /// on the heel line), out of the steel, along what it bolts onto,
    /// across the flange; the heel (the plane's 2D origin).
    face3: [f64; 3],
    n3: [f64; 3],
    x3: [f64; 3],
    y3: [f64; 3],
    heel3: [f64; 3],
    /// Its heel's edge across it (before a move) and the outstanding leg's
    /// width, m.
    heel_lat: f64,
    out_w: f64,
    /// How high over the face what it carries reaches (its leg on a plate
    /// and its bolts' heads), m: the slab in which fixed steel is in its
    /// way.
    slab: f64,
    /// The fixed steel in its way (2026-11-19, J1/J2): the parts of the
    /// joint not laid out in this plan that reach into its slab, each by
    /// name as its footprint on the plane (2D, convex).
    fixed: Vec<(String, Vec<[f64; 2]>)>,
    /// The least cut a clash after asks (the run's rounds: pulled back
    /// past what it ran into) and why.
    floor: Option<(f64, String)>,
    /// What it bolts onto: its class rank (the planes on the biggest
    /// members first).
    host_rank: u8,
    /// On the flange's inner face leaving the joint over the heel (towards
    /// the angle's other leg): onto a bent plate folded round that other
    /// leg (J2b, 2026-11-19).
    over: Option<Over>,
}

/// An end on a flange's inner face that leaves the joint over the heel —
/// towards the angle's other leg, where it cannot lie on the flange — laid
/// out onto a **bent plate** (J2b, owner-approved 2026-11-19): its flat in
/// the end's own plane (the flange's inner face, beyond the other leg's
/// outer face), folded along the angle's line onto the other leg's outer
/// face and bolted there. The end stays where it is drawn (its gauge line
/// through the work point), only pulled back: the bend's inside radius and
/// the plate past the other leg's outer face, by half the gap for its
/// steel and by its edge distance for its bolts.
#[derive(Debug, Clone)]
struct Over {
    /// The plate's thickness taken while placing (its own leg's; the plate
    /// laid out takes the thickest leg on it, checked again then), m.
    tp: f64,
    /// The edge and end distances its bolts keep in a plate that thick, m.
    keep: (f64, f64),
    /// The other leg's width from the heel, m.
    other_w: f64,
    /// The other leg's outer face as a plane (as a strip there keys it).
    plane_b: PlaneKey,
    /// Across the other leg from the heel to its toe (structure axes).
    down: [f64; 3],
    /// The angle's leg the plate lies on (`long`, `short`; none: equal).
    leg: Option<&'static str>,
    /// Along the member it bolts onto from the work point, per metre of
    /// the plane's x.
    along_host: f64,
}

/// How far beyond the other leg's outer face (m) the steel of an end
/// leaving over the heel keeps for a bent plate `tp` thick (J2b): the
/// plate, its inside radius and half the gap.
fn over_steel(tp: f64, gap: f64) -> f64 {
    tp + bend_radius(tp) + gap / 2.0
}

/// The least cut (from the work point, on the plane) that takes end `x`,
/// moved `moved` across, past the bent plate's flat on the other leg
/// (J2b): its leg's edges `steel` beyond the other leg's outer face, its
/// bolts `bolts`; and what asks it.
fn over_cut(x: &Strip, moved: f64, steel: f64, bolts: f64) -> (f64, &'static str) {
    let u1 = -x.u[1];
    if u1 <= 1e-6 {
        return (f64::INFINITY, "the other leg");
    }
    let mut need = f64::NEG_INFINITY;
    let mut by = "the other leg";
    for l in [x.lat[0] + moved, x.lat[1] + moved] {
        let c = (x.at(0.0, l)[1] + steel) / u1;
        if c > need {
            need = c;
            by = "the bend of its plate";
        }
    }
    for (along, line) in &x.bolts {
        let l = x.gauges.get(*line).copied().unwrap_or(0.0) + moved;
        let c = (x.at(*along, l)[1] + bolts) / u1;
        if c > need {
            need = c;
            by = "the bend of its plate (its bolts' edge distance)";
        }
    }
    (need, by)
}

/// A bent plate laid out for the ends of one plane leaving over the heel
/// (J2b): its flat on the other leg (bolted there), its flat in their
/// plane (bent from the first; [`OwnPlane::welded_to`] set by the caller),
/// the ends placed on it and those left with why.
struct BentOver {
    flat: PlatePlan,
    /// The flat plate of the other leg's outer face (2b) the bent plate's
    /// flat there is (its thickness, reach and own bolts then added to it);
    /// none: `flat` is a plate of its own.
    into: Option<usize>,
    bent: PlatePlan,
    placed: Vec<Placed>,
    left: Vec<(usize, String)>,
}

/// The bent plate for the ends `group` of one plane leaving over the heel
/// (J2b, 2026-11-19), the ends `laid` out on the joint's other planes kept
/// clear of; Err: every end left, each with why.
///
/// In the plane's axes (x along what they bolt onto, y across its flange
/// from the heel; the other leg's outer face is y = 0): the plate as thick
/// as the thickest leg on it, its inside bend radius [`bend_radius`]; each
/// end pulled back further where that plate asks (its steel beyond the
/// bend's inside radius by half the gap, its bolts by their edge
/// distance). The flat in their plane: round their bolts (the end and
/// edge distances, along and across each end), to the bend line (the two
/// flats' middle planes). The flat on the other leg: from the bend line to
/// that leg's toe, as far along as the bend and two bolts onto the leg
/// (provisional: the end's bolt, one gauge line where their nut's corner
/// clears the flange and its root, and their head the bend — the plate
/// runs on there, no free edge —, the edge distance from the leg's toe, the
/// default spacing apart). Refused where those bolts find no room, or the flat
/// would lie under an end laid out on that leg's outer face.
fn bent_over(
    strips: &[Strip],
    group: &[Placed],
    j: &JointDetail,
    s: &LayoutSettings,
    laid: &[Placed],
    plate_of: &BTreeMap<usize, usize>,
    plates: &[PlatePlan],
) -> Result<BentOver, Vec<(usize, String)>> {
    let first = &strips[group[0].k];
    let o = first.over.as_ref().expect("an end over the heel");
    let why = |k: usize, e: &str| {
        format!(
            "{}: on the inner face of {}'s flange it runs towards the angle's other leg; a bent \
             plate round that leg does not fit: {e}",
            strips[k].member, strips[k].host
        )
    };
    let all = |e: &str, left: Vec<(usize, String)>, placed: &[Placed]| {
        let mut out = left;
        out.extend(placed.iter().map(|p| (p.k, why(p.k, e))));
        out
    };
    // The ends laid out on the other leg's outer face: all on one flat
    // plate there (2b) — the bent plate's flat on that leg is that plate,
    // as thick as the thicker of the two —, else kept clear of.
    let on_face: Vec<&Placed> = laid
        .iter()
        .filter(|q| strips[q.k].plane == o.plane_b)
        .collect();
    let into = match on_face.first().and_then(|q| plate_of.get(&q.k)) {
        Some(&i)
            if on_face.iter().all(|q| plate_of.get(&q.k) == Some(&i))
                && plates[i].own.is_none()
                && plates[i].hold.is_none() =>
        {
            Some(i)
        }
        _ => None,
    };
    if into.is_none()
        && let Some(q) = on_face.iter().find(|q| plate_of.contains_key(&q.k))
    {
        let e = format!(
            "its flat on {}'s other leg would lie on the plate {} is laid out on there",
            first.host, strips[q.k].member
        );
        return Err(group.iter().map(|p| (p.k, why(p.k, &e))).collect());
    }
    let tp = group
        .iter()
        .map(|p| strips[p.k].own_t)
        .fold(into.map_or(0.0, |i| plates[i].thickness), f64::max);
    let r = bend_radius(tp);
    let mut placed: Vec<Placed> = Vec::new();
    let mut left: Vec<(usize, String)> = Vec::new();
    for p in group {
        let x = &strips[p.k];
        let keep = x.over.as_ref().map_or(o.keep, |v| v.keep);
        let (need, _) = over_cut(x, p.moved, over_steel(tp, s.gap), tp + r + keep.0);
        let need = ceil_to(need, 0.001);
        if need <= p.cut + 1e-9 {
            placed.push(p.clone());
            continue;
        }
        let limit = s.pull_back.min(x.half - bolt_run(x));
        if x.hand_cut.is_some() {
            left.push((p.k, why(p.k, "its cut given by hand leaves it on the bend")));
        } else if need > limit + 1e-9 {
            left.push((
                p.k,
                why(
                    p.k,
                    &format!(
                        "it would be pulled back {:.0} mm to pass the bend of a {:.0} mm plate (the \
                         longest {:.0} mm)",
                        need.min(9.999) * 1000.0,
                        tp * 1000.0,
                        limit * 1000.0
                    ),
                ),
            ));
        } else {
            placed.push(Placed {
                k: p.k,
                cut: need,
                moved: p.moved,
            });
        }
    }
    if placed.is_empty() {
        return Err(left);
    }
    // The flat in their plane: round their bolts, to the bend line.
    let yb = -tp / 2.0;
    let mut pts: Vec<[f64; 2]> = Vec::new();
    for p in &placed {
        let x = &strips[p.k];
        let (edge, end) = x.over.as_ref().map_or(o.keep, |v| v.keep);
        for (along, line) in &x.bolts {
            let l = x.gauges.get(*line).copied().unwrap_or(0.0) + p.moved;
            let c = x.at(p.cut + along, l);
            for k in 0..8 {
                let a = f64::from(k) * std::f64::consts::FRAC_PI_4;
                let (da, dl) = (end * RING_GROW * a.cos(), edge * RING_GROW * a.sin());
                pts.push([
                    c[0] + x.u[0] * da + x.v[0] * dl,
                    c[1] + x.u[1] * da + x.v[1] * dl,
                ]);
            }
        }
    }
    if pts.is_empty() {
        return Err(all("no bolts to lay its plate round", left, &placed));
    }
    let lo = pts.iter().map(|q| q[0]).fold(f64::INFINITY, f64::min);
    let hi = pts.iter().map(|q| q[0]).fold(f64::NEG_INFINITY, f64::max);
    pts.push([lo, yb]);
    pts.push([hi, yb]);
    let poly2 = clip(&crate::joint_sheet::hull(pts), |q| yb - q[1]);
    // The flat on the other leg, and its own bolts onto that leg.
    let (t_a, root) = (first.t, first.r);
    let s_top = t_a - tp / 2.0;
    let x0 = &strips[placed[0].k];
    let (d, nut) = (x0.d, x0.nut);
    let edge_b = x0.edge.max(o.keep.0);
    let [end_min, spacing_min, _] = s.defaults.of(d);
    // Towards the bend the plate runs on (no free edge there): its head
    // flat on the flat past the bend, and its nut past the flange's other
    // leg and root; towards the toe the edge distance (the leg's free
    // edge).
    let gauge = t_a + r.max(root) + nut;
    if gauge > o.other_w - edge_b + 1e-9 {
        return Err(all(
            &format!(
                "no room for its own bolts on {}'s other leg ({:.0} mm wide; they need {:.0} mm \
                 from its heel and {:.0} mm from its toe)",
                first.host,
                o.other_w * 1000.0,
                gauge * 1000.0,
                edge_b * 1000.0
            ),
            left,
            &placed,
        ));
    }
    let xc = (lo + hi) / 2.0;
    let bx = [xc - spacing_min / 2.0, xc + spacing_min / 2.0];
    let flo = lo.min(bx[0] - end_min);
    let fhi = hi.max(bx[1] + end_min);
    // 3D from the work point: the plane's heel, along, across, down the
    // other leg (into the angle).
    let (heel, x3, y3, down) = (first.heel3, first.x3, first.y3, o.down);
    let on_b = |x: f64, sd: f64| add(add(heel, scale(x3, x)), scale(down, sd));
    // Clear of the ends laid out on that leg's outer face (by half the
    // gap): their leg from their cut to half their length.
    let h = s.gap / 2.0;
    for q in on_face.iter().filter(|_| into.is_none()) {
        let b = &strips[q.k];
        let lat = [b.lat[0] + q.moved, b.lat[1] + q.moved];
        let leg: Vec<[f64; 2]> = [
            b.at(q.cut, lat[0]),
            b.at(b.half, lat[0]),
            b.at(b.half, lat[1]),
            b.at(q.cut, lat[1]),
        ]
        .iter()
        .map(|p2| {
            let p3 = add(add(b.heel3, scale(b.x3, p2[0])), scale(b.y3, p2[1]));
            let d3 = sub(p3, heel);
            [dot(d3, x3), dot(d3, down)]
        })
        .collect();
        let c = clip(&leg, |p| p[0] - (flo - h));
        let c = clip(&c, |p| (fhi + h) - p[0]);
        let c = clip(&c, |p| p[1] - (s_top - h));
        let c = clip(&c, |p| (o.other_w + h) - p[1]);
        if c.len() >= 3 {
            return Err(all(
                &format!(
                    "its flat on {}'s other leg would lie under {}, laid out on that leg's face",
                    first.host, b.member
                ),
                left,
                &placed,
            ));
        }
    }
    let abs = |q: [f64; 3]| add(q, j.position);
    let mut flat_outline: Vec<[f64; 3]> = [
        (flo, s_top),
        (fhi, s_top),
        (fhi, o.other_w),
        (flo, o.other_w),
    ]
    .iter()
    .map(|(x, sd)| abs(on_b(*x, *sd)))
    .collect();
    // Its face on the leg, through its thickness away from it: the
    // outline's normal out of the leg's outer face.
    if crate::welds::polygon_normal(&flat_outline).is_some_and(|n| dot(n, y3) > 0.0) {
        flat_outline.reverse();
    }
    let face2 = scale(down, t_a);
    let bent_outline: Vec<[f64; 3]> = poly2
        .iter()
        .map(|q| abs(add(add(add(heel, scale(x3, q[0])), scale(y3, q[1])), face2)))
        .collect();
    let line_at = |x: f64| {
        abs(add(
            add(add(heel, scale(x3, x)), scale(y3, yb)),
            scale(down, s_top),
        ))
    };
    let angle = 180.0 - angle_deg(down, scale(y3, -1.0));
    let bolt = match &x0.give {
        Some(b) => BoltArea {
            count: 2,
            lines: 1,
            gauges: Vec::new(),
            pitch: None,
            end: None,
            ..b.clone()
        },
        None => s
            .bolt
            .clone()
            .filter(|b| (b.diameter - d).abs() < 1e-9)
            .unwrap_or_else(|| BoltArea::picked(None, d, None)),
    };
    let wp_x = first.w[0];
    let hold = HoldPlan {
        member: first.host.clone(),
        leg: o.leg,
        bolt: BoltArea {
            from: format!("{} bolt(s) of {:.0} mm", 2, d * 1000.0),
            ..bolt
        },
        provisional: true,
        points: bx
            .iter()
            .map(|x| {
                [
                    round_to((x - wp_x) * o.along_host, 1e-4),
                    round_to(gauge, 1e-4),
                ]
            })
            .collect(),
    };
    let flat = PlatePlan {
        members: Vec::new(),
        thickness: tp,
        on: first.host.clone(),
        heel: None,
        own: None,
        reach: None,
        outline: Some(flat_outline),
        hold: Some(hold),
    };
    let bent = PlatePlan {
        members: placed.iter().map(|p| strips[p.k].member.clone()).collect(),
        thickness: tp,
        on: first.host.clone(),
        heel: None,
        own: Some(OwnPlane {
            welded_to: 0,
            outline: bent_outline,
            off: angle,
            lift: 0.0,
            // Where it lies flat on the flange's inner face, not farther
            // out (a flat plate there would put it a plate farther).
            out_more: 0.0,
            weld_size: 0.0,
            weld_length: 0.0,
            bend: Some(BendPlan {
                angle: round_to(angle, 0.01),
                inner_radius: r,
                line: [line_at(lo), line_at(hi)],
            }),
        }),
        reach: None,
        outline: None,
        hold: None,
    };
    Ok(BentOver {
        flat,
        into,
        bent,
        placed,
        left,
    })
}

/// A member end whose leg bolted lies off the flange (step 4), in 3D: from
/// the work point, m, unit vectors.
#[derive(Debug, Clone)]
struct Tilted {
    /// How far off, degrees.
    off: f64,
    /// Along it from the joint.
    dir: [f64; 3],
    /// Its bolts along it from where its steel stops (m, in 3D) and their
    /// gauge lines.
    bolts: Vec<(f64, usize)>,
    /// Lengths along it on the flange's plane per metre along it.
    lu: f64,
    /// Half its length, m.
    half: f64,
    /// Its leg bolted's normal, towards its own side.
    nb: [f64; 3],
    /// Along its leg bolted from the heel to the toe.
    across: [f64; 3],
    /// Its heel drawn with its gauge line through the work point.
    heel: [f64; 3],
    /// Its axis there (at the joint).
    axis: [f64; 3],
    /// Its leg bolted's width, m.
    width: f64,
    /// Its gauges from the heel along `across`, m.
    gauges: Vec<f64>,
    /// Its sideways shift's direction (the data's, per metre moved along
    /// the plane's `v`: `Strip::shift_per`).
    shift_dir: [f64; 3],
    /// Its shift written per metre moved along the plane's `v`.
    shift_per: f64,
    /// The edge and end distances its bolts keep in a plate of its leg's
    /// thickness, m.
    keep: (f64, f64),
    /// Along the member it bolts onto (the flange's plane's x).
    x3: [f64; 3],
    /// Out of the flange's face it lies on (its inner one: into the
    /// angle).
    out: [f64; 3],
    /// That face's heel line at the joint, from the work point.
    face: [f64; 3],
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
    plan_joint_with(j, s, edge, except, &BTreeMap::new())
}

/// The most placings the search of one joint tries (J1, 2026-11-19): a
/// joint whose search stops there says so with the members it leaves. A
/// starting value of the program's (measured on the private real towers:
/// see the theory page), not a standard's.
pub const NODE_LIMIT: usize = 400;

/// [`plan_joint_except`] with the least cut of some members (`floors`:
/// member → the cut, m, and why; the run's clash rounds: an end laid out
/// whose steel, bolts or plate would run into something is pulled back
/// past it before it is left for a hand detail).
///
/// Each plane is **searched** (J1, owner-approved 2026-11-19), not filled
/// one member after the other: the ends are placed in every order that
/// can matter (the owner's order first: class, the wider leg, the larger
/// angle, the id), each concentric and pulled back, else with the least
/// sideways shift, backtracking where one does not fit; the layout placing
/// the most ends wins, then the least eccentricity (the shifts), then the
/// first found — so the owner's order wins every tie. At most
/// [`NODE_LIMIT`] placings per joint. What is left for a hand detail keeps
/// its place (J2): every end laid out keeps clear of it as it is.
pub fn plan_joint_with(
    j: &JointDetail,
    s: &LayoutSettings,
    edge: EdgeRule<'_>,
    except: &BTreeMap<String, String>,
    floors: &BTreeMap<String, (f64, String)>,
) -> JointPlan {
    plan_joint_budget(j, s, edge, except, floors, NODE_LIMIT)
}

/// [`plan_joint_with`] with at most `limit` placings (1: the owner's order
/// filled one after the other, as before the search).
pub(crate) fn plan_joint_budget(
    j: &JointDetail,
    s: &LayoutSettings,
    edge: EdgeRule<'_>,
    except: &BTreeMap<String, String>,
    floors: &BTreeMap<String, (f64, String)>,
    limit: usize,
) -> JointPlan {
    let mut plan = JointPlan {
        joint: j.id.clone(),
        ..Default::default()
    };
    if let Some(w) = splice_unknown(j) {
        plan.left.push(w);
    }
    let find = |m: &str| j.arrivals.iter().find(|a| a.member == m);
    let mut strips: Vec<Strip> = Vec::new();
    let mutual = mutual_hosts(j);
    for (m, why) in &mutual {
        if !except.contains_key(m) {
            plan.left.push(format!("{m}: {why}"));
        }
    }
    // The members not laid out here: those in line with what they bolt
    // onto, running on, no angle, onto a splice, the host of a pair
    // bolting onto each other (their bolts are kept clear of).
    let others: Vec<&str> =
        j.arrivals
            .iter()
            .filter(|a| {
                a.continuous.is_some()
                    || mutual.contains_key(&a.member)
                    || a.profile.as_ref().is_none_or(|p| p.angle.is_none())
                    || a.onto.value.as_deref().and_then(find).is_some_and(|h| {
                        !(20.0..=160.0).contains(&angle_deg(a.direction, h.direction))
                    })
                    || a.onto
                        .value
                        .as_deref()
                        .is_some_and(|o| j.splices.iter().any(|s| s.id == o))
                    || except.contains_key(&a.member)
            })
            .map(|a| a.member.as_str())
            .collect();
    for a in &j.arrivals {
        if let Some(why) = except.get(&a.member) {
            plan.by_hand(&a.member, format!("{}: {why}", a.member));
            continue;
        }
        if mutual.contains_key(&a.member) {
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
        // caller's to turn; still off, onto a plate in its own plane
        // welded to a flat plate on the flange (the owner's step 4), else
        // a hand detail.
        let off = a.flat.filter(|f| f.off > FLAT_OFF).map(|f| f.off);
        // Off: on the flange in whose plane its axis lies most nearly, else
        // on the other.
        let strip = strip_of(j, a, host, s, edge, &others, off, false).or_else(|e| {
            if off.is_some() {
                strip_of(j, a, host, s, edge, &others, off, true).map_err(|_| e)
            } else {
                Err(e)
            }
        });
        match strip {
            Ok(x) => strips.push(x),
            Err(e) => {
                let why = match off {
                    Some(deg) => format!(
                        "{}: its bolted leg lies {deg:.0}° off the flange of {} it bolts onto and \
                         no turn lays it flat at both its ends; a welded plate in its own plane \
                         does not fit: {e}",
                        a.member, host.member
                    ),
                    None => format!("{}: {e}", a.member),
                };
                plan.by_hand(&a.member, why);
            }
        }
    }
    for x in strips.iter_mut() {
        // (Off the flange: its cut measured on the flange's plane.)
        let lu = x.tilted.as_ref().map_or(1.0, |t| t.lu);
        x.floor = floors.get(&x.member).map(|(f, w)| (f * lu, w.clone()));
    }
    // The fixed steel (J1/J2, 2026-11-19): every part of the joint that is
    // not laid out in this plan — the members in line, running on, not
    // single angles, given by hand, and those left for a hand detail (their
    // place kept free) — each piece as the clash check cuts it.
    let pieces =
        crate::clash::piece_corners(j.position, &j.arrivals, &j.plates, &j.splices, |_, _| true);
    // (Left for their own sake: their place kept free, J2.)
    plan.kept_free = plan
        .hand_members
        .iter()
        .filter(|m| !except.contains_key(*m))
        .cloned()
        .collect();
    let mut fixed: BTreeSet<String> = j
        .arrivals
        .iter()
        .map(|a| a.member.clone())
        .filter(|m| !strips.iter().any(|x| &x.member == m) && !except.contains_key(m))
        .collect();
    // The steel laid out in the planes before (3D corners, by member).
    let mut laid3: Vec<Laid3> = Vec::new();
    // By plane, the planes on the biggest members first (what bolts onto
    // a member laid out on a leg keeps clear of it as laid out), each in
    // the owner's order.
    // (A plane is a flange's face, whatever member of a run through the
    // joint its ends name: in line, they share it.)
    let mut by_plane: BTreeMap<PlaneKey, Vec<usize>> = BTreeMap::new();
    for (k, x) in strips.iter().enumerate() {
        by_plane.entry(x.plane).or_default().push(k);
    }
    let mut planes: BTreeMap<(u8, String, PlaneKey), Vec<usize>> = BTreeMap::new();
    for (key, ks) in by_plane {
        let first = ks
            .iter()
            .map(|&k| (strips[k].host_rank, strips[k].host.clone()))
            .min()
            .expect("a plane has ends");
        planes.insert((first.0, first.1, key), ks);
    }
    let mut placed_all: Vec<Placed> = Vec::new();
    let mut plate_of: BTreeMap<usize, usize> = BTreeMap::new();
    let mut bent_wait: BTreeMap<PlaneKey, Vec<Placed>> = BTreeMap::new();
    let mut budget = limit;
    for ks in planes.values() {
        let mut order = ks.clone();
        order.sort_by(|&a, &b| {
            strips[a]
                .rank
                .cmp(&strips[b].rank)
                .then(strips[a].member.cmp(&strips[b].member))
        });
        for &k in &order {
            let polys = fixed_in(&strips[k], j, &pieces, &fixed, &laid3);
            strips[k].fixed = polys;
        }
        let found = search_plane(&strips, &order, s, &mut budget);
        let mut placed = found.placed;
        let mut left_here: Vec<usize> = Vec::new();
        for (k, e) in found.failed {
            let e = if found.stopped {
                format!("{e} (the search stopped at its limit of {limit} tries)")
            } else {
                e
            };
            plan.by_hand(&strips[k].member, e);
            // Left for its own sake (it fits nowhere even alone): its place
            // kept free; left for want of room beside the others, or for a
            // clash: said with them.
            if strips[k].floor.is_none() && place(&strips, k, &[], s).is_err() {
                plan.kept_free.push(strips[k].member.clone());
                left_here.push(k);
            }
        }
        // J2: the place of a member left for a hand detail kept free —
        // those laid out pulled back clear of it as it is, else left too.
        loop {
            let mut dropped: Option<(usize, String)> = None;
            for p in placed.iter_mut() {
                let x = &strips[p.k];
                let mut need = f64::NEG_INFINITY;
                let mut by = String::new();
                for &h in &left_here {
                    let hm = &strips[h].member;
                    let one: BTreeSet<String> = [hm.clone()].into();
                    for (_, poly) in fixed_in(x, j, &pieces, &one, &[]) {
                        let c =
                            poly_need(x, [x.lat[0] + p.moved, x.lat[1] + p.moved], &poly, s.gap);
                        if c > need {
                            need = c;
                            by = hm.clone();
                        }
                    }
                }
                if need <= p.cut + 1e-9 {
                    continue;
                }
                let limit = (s.pull_back * x.tilted.as_ref().map_or(1.0, |t| t.lu))
                    .min(x.half - bolt_run(x));
                if x.hand_cut.is_none() && need <= limit {
                    p.cut = ceil_to(need, 0.001);
                } else {
                    dropped = Some((
                        p.k,
                        format!(
                            "{}: it would run into {by}, left for a hand detail (its place kept \
                             free: {:.0} mm pulled back, the longest {:.0} mm)",
                            x.member,
                            need.min(9.999) * 1000.0,
                            limit * 1000.0
                        ),
                    ));
                    break;
                }
            }
            match dropped {
                Some((k, why)) => {
                    placed.retain(|p| p.k != k);
                    plan.by_hand(&strips[k].member, why);
                }
                None => break,
            }
        }
        for &k in &left_here {
            fixed.insert(strips[k].member.clone());
        }
        // J2b: the ends leaving over the heel go onto a bent plate, made
        // once every plane is laid out (its flat on the other leg keeps
        // clear of what that leg's face holds).
        let (overs, rest): (Vec<Placed>, Vec<Placed>) =
            placed.into_iter().partition(|p| strips[p.k].over.is_some());
        if let Some(p) = overs.first() {
            bent_wait
                .entry(strips[p.k].plane)
                .or_default()
                .extend(overs);
        }
        let mut placed = rest;
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
            x.tilted.is_none()
                && bolt_ys(x, p)
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
            laid3.extend(solids_of(&strips, &on, 0.0));
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
                let mut need = past_toe(x, &p, s.gap);
                // Step 4: its plate in its own plane past the flange's toe
                // by half the gap too (under it nothing but air: its nuts
                // and its edge low near the seam).
                if let Some(t) = &x.tilted {
                    match toe_clear(x, t, &p, need.max(p.cut), j.position, s.gap) {
                        Ok(c) => need = need.max(c),
                        Err(e) => {
                            plan.by_hand(&x.member, tilted_why(x, t, &e));
                            continue;
                        }
                    }
                }
                if need > p.cut + 1e-9 {
                    let limit = (s.pull_back * x.tilted.as_ref().map_or(1.0, |t| t.lu))
                        .min(x.half - bolt_run(x));
                    if need > limit + 1e-9 {
                        let why = format!(
                            "{}: on a plate its bolts need it pulled back {:.0} mm to pass the \
                             flange of {} (the longest pull-back {:.0} mm)",
                            x.member,
                            need.min(9.999) * 1000.0,
                            x.host,
                            limit * 1000.0
                        );
                        let why = match &x.tilted {
                            Some(t) => tilted_why(x, t, &why[x.member.len() + 2..]),
                            None => why,
                        };
                        plan.by_hand(&x.member, why);
                        continue;
                    }
                    p.cut = need;
                }
                kept.push(p);
            }
            // Step 4: each end off the flange onto a plate in its own
            // plane, welded to the flat plate along the line their planes
            // share; kept where that plate reaches the flange and every
            // other end's bolts on it, in order.
            let tf = |kept: &[Placed]| kept.iter().map(|p| strips[p.k].own_t).fold(0.0, f64::max);
            let mut owns: Vec<(usize, OwnGeo)> = Vec::new();
            let flat_ends: Vec<Placed> = kept
                .iter()
                .filter(|p| strips[p.k].tilted.is_none())
                .cloned()
                .collect();
            let mut dropped: Vec<usize> = Vec::new();
            for p in kept.iter().filter(|p| strips[p.k].tilted.is_some()) {
                let x = &strips[p.k];
                let t = x.tilted.as_ref().expect("tilted");
                let geo =
                    match own_geometry(t, cut3(t, p.cut), p.moved, t.face, t.out, tf(&kept), s.gap)
                    {
                        Ok(g) => g,
                        Err(e) => {
                            plan.by_hand(&x.member, tilted_why(x, t, &e));
                            dropped.push(p.k);
                            continue;
                        }
                    };
                // Its axis at most a plate's thickness farther out of the
                // flange than on a flat plate (its line of action off the
                // work point about as a filler would put it; more is not
                // in the analysis: a hand detail).
                if geo.out_more > x.own_t + 1e-9 {
                    let e = farther_out(geo.out_more, x.own_t);
                    plan.by_hand(&x.member, tilted_why(x, t, &e));
                    dropped.push(p.k);
                    continue;
                }
                match own_fits(&strips, x, &geo, &flat_ends, &owns, j.position, s.gap) {
                    Ok(()) => owns.push((p.k, geo)),
                    Err(e) => {
                        plan.by_hand(&x.member, tilted_why(x, t, &e));
                        dropped.push(p.k);
                    }
                }
            }
            kept.retain(|p| !dropped.contains(&p.k));
            let placed = kept;
            if placed.is_empty() {
                continue;
            }
            let thickness = tf(&placed);
            let index = plan.plates.len();
            let members: Vec<String> = placed
                .iter()
                .filter(|p| strips[p.k].tilted.is_none())
                .map(|p| strips[p.k].member.clone())
                .collect();
            let first = &strips[placed[0].k];
            // On an inner face the plate keeps clear of the angle's other
            // leg and its root (its heel line that far across).
            let heel = if first.inside {
                add(first.heel, scale(first.across, first.t + first.r))
            } else {
                first.heel
            };
            let mut flat = PlatePlan {
                members,
                thickness,
                on: first.host.clone(),
                heel: placed
                    .iter()
                    .all(|p| strips[p.k].u[1] > 1e-6)
                    .then_some((heel, first.across)),
                own: None,
                reach: None,
                outline: None,
                hold: None,
            };
            let mut own_plates: Vec<PlatePlan> = Vec::new();
            for (k, _) in &owns {
                let x = &strips[*k];
                let t = x.tilted.as_ref().expect("tilted");
                let p = placed.iter().find(|p| p.k == *k).expect("kept");
                // Its final place with the flat plate's thickness.
                let geo = own_geometry(t, cut3(t, p.cut), p.moved, t.face, t.out, thickness, s.gap)
                    .expect("laid out before");
                let abs = |q: [f64; 3]| add(q, j.position);
                let r = flat.reach.get_or_insert_with(|| Reach {
                    points: Vec::new(),
                    normal: scale(t.out, -1.0),
                    keep_to: Vec::new(),
                });
                r.points.extend(own_reach(x, &geo, thickness, j.position));
                r.keep_to.push((abs(geo.seam_at), geo.side));
                let seam = norm(sub(geo.seam[1], geo.seam[0]));
                own_plates.push(PlatePlan {
                    members: vec![x.member.clone()],
                    thickness,
                    on: x.host.clone(),
                    heel: None,
                    own: Some(OwnPlane {
                        welded_to: index,
                        outline: geo.outline.iter().map(|q| abs(*q)).collect(),
                        off: t.off,
                        lift: geo.lift,
                        out_more: geo.out_more,
                        weld_size: weld_throat(thickness),
                        weld_length: round_to(2.0 * seam, 1e-4),
                        bend: None,
                    }),
                    reach: None,
                    outline: None,
                    hold: None,
                });
            }
            plan.plates.push(flat);
            for p in &placed {
                if strips[p.k].tilted.is_none() {
                    plate_of.insert(p.k, index);
                }
            }
            for ((k, _), w) in owns.iter().zip(own_plates) {
                plate_of.insert(*k, plan.plates.len());
                plan.plates.push(w);
            }
            laid3.extend(solids_of(&strips, &placed, thickness));
            placed_all.extend(placed);
            continue;
        }
        laid3.extend(solids_of(&strips, &placed, 0.0));
        placed_all.extend(placed);
    }
    // J2b: the bent plates of the ends leaving over the heel, the ends of
    // one plane sharing its flat.
    for (_, group) in bent_wait {
        match bent_over(&strips, &group, j, s, &placed_all, &plate_of, &plan.plates) {
            Ok(b) => {
                if let Some(h) = &b.flat.hold {
                    plan.notes.push(format!(
                        "{}: {} provisional bolt(s) holding a bent plate's flat, {}",
                        h.member, h.bolt.count, h.bolt.from
                    ));
                }
                let flat = match b.into {
                    // The other leg's face plate is the flat: it reaches
                    // over the bent plate's flat there too (on its face
                    // away from the leg), as thick, bolted to the leg.
                    Some(i) => {
                        let f = &mut plan.plates[i];
                        f.thickness = f.thickness.max(b.flat.thickness);
                        f.hold = b.flat.hold;
                        let pts = b.flat.outline.unwrap_or_default();
                        let n = crate::welds::polygon_normal(&pts).unwrap_or([0.0; 3]);
                        let away: Vec<[f64; 3]> = pts
                            .iter()
                            .map(|q| add(*q, scale(n, b.flat.thickness)))
                            .collect();
                        let rc = f.reach.get_or_insert_with(|| Reach {
                            points: Vec::new(),
                            normal: scale(n, -1.0),
                            keep_to: Vec::new(),
                        });
                        rc.points.extend(away);
                        i
                    }
                    None => {
                        plan.plates.push(b.flat);
                        plan.plates.len() - 1
                    }
                };
                let mut bent = b.bent;
                if let Some(o) = bent.own.as_mut() {
                    o.welded_to = flat;
                }
                let index = plan.plates.len();
                plan.plates.push(bent);
                for p in b.placed {
                    plate_of.insert(p.k, index);
                    placed_all.push(p);
                }
                for (k, why) in b.left {
                    plan.by_hand(&strips[k].member, why);
                }
            }
            Err(left) => {
                for (k, why) in left {
                    plan.by_hand(&strips[k].member, why);
                }
            }
        }
    }
    // In the joint's member order.
    placed_all.sort_by_key(|p| p.k);
    for p in placed_all {
        let x = &strips[p.k];
        // Concentric: the gauge line through the work point, drawn so (the
        // `gauge` mark), no shift; a move beyond it is the shift.
        let on_gauge = !x.hand_shift && x.shift_per.is_some();
        let concentric = concentric_of(x);
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
            cut: x.hand_cut.is_none().then_some(match &x.tilted {
                Some(t) => cut3(t, p.cut),
                None => p.cut,
            }),
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
    // The members left whose bolted leg lies off its flange (the plate
    // case of the estimated eccentricities, O45 A1).
    plan.off_flange = plan
        .hand_members
        .iter()
        .filter_map(|m| {
            let a = find(m.as_str())?;
            a.flat.filter(|f| f.off > FLAT_OFF)?;
            let t = a.profile.as_ref()?.angle.as_ref()?.t;
            Some((m.clone(), t))
        })
        .collect();
    plan.hand_hosts = plan
        .hand_members
        .iter()
        .filter_map(|m| {
            let a = find(m.as_str())?;
            let host = match a.onto.value.as_deref() {
                Some(o) => find(o)?,
                None => through_member(j, a)?,
            };
            Some((m.clone(), host.member.clone()))
        })
        .collect();
    plan
}

/// The member ends of `j` the plan leaves as they are because the member
/// they would bolt onto bolts back onto them, or onto a piece in line with
/// them (the does-not-fit investigation, 2026-11-19): laid out both, each
/// would be cut back onto the other (or onto a plate on it) and the bolts
/// of one would lie beyond the other's steel — physically impossible. Of
/// such a pair the member running through the joint (in line with another
/// of its members), else the one [`through_member`] ranks first (class,
/// the wider), stays the host: its own end is left as it is, with why.
fn mutual_hosts(j: &JointDetail) -> BTreeMap<String, String> {
    let find = |m: &str| j.arrivals.iter().find(|a| a.member == m);
    // What `a` would be laid onto (the plan's own choice below), if it is
    // an end the plan lays out at all.
    let host_of = |a: &Arrival| -> Option<&Arrival> {
        if a.continuous.is_some() || a.profile.as_ref().is_none_or(|p| p.angle.is_none()) {
            return None;
        }
        let h = match a.onto.value.as_deref() {
            Some(o) => find(o)?,
            None => through_member(j, a)?,
        };
        (20.0..=160.0)
            .contains(&angle_deg(a.direction, h.direction))
            .then_some(h)
    };
    let in_line_with = |a: &Arrival| {
        j.arrivals
            .iter()
            .find(|c| c.member != a.member && angle_deg(a.direction, c.direction) > 160.0)
    };
    let through = |a: &Arrival| a.continuous.is_some() || in_line_with(a).is_some();
    let rank = |b: &Arrival| {
        let class = match b.class.as_deref() {
            Some("leg") => 0,
            Some("bracing") | Some("crossing") => 1,
            Some("redundant") => 2,
            _ => 3,
        };
        let width = b
            .profile
            .as_ref()
            .and_then(|p| p.angle)
            .map_or(0, |g| -((g.b.max(g.h) * 1e6) as i64));
        (!through(b), class, width, b.member.clone())
    };
    let mut out = BTreeMap::new();
    for a in &j.arrivals {
        let Some(h) = host_of(a) else { continue };
        let Some(back) = host_of(h) else { continue };
        let onto_a = back.member == a.member;
        let onto_run = !onto_a && angle_deg(a.direction, back.direction) > 160.0;
        if !(onto_a || onto_run) || rank(a) > rank(h) {
            continue;
        }
        let onto = if onto_a {
            "it".to_owned()
        } else {
            format!("{} (in line with it)", back.member)
        };
        let runs = in_line_with(a)
            .map(|c| format!(" (it runs through the joint in line with {})", c.member))
            .unwrap_or_default();
        out.insert(
            a.member.clone(),
            format!(
                "it would bolt onto {h} and {h} onto {onto}: each cut back onto the other, the \
                 bolts of one beyond the other's steel — it stays what {h} bolts onto, its own end \
                 left as it is{runs}",
                h = h.member
            ),
        );
    }
    out
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

/// The phrase that says where a splice is is not known
/// ([`splice_unknown`]).
pub const SPLICE_UNKNOWN: &str = "where the splice is is not known";

/// The biggest pair of angles in line through joint `j` (as
/// [`joins_of`] takes it) when they have one section, are turned alike and
/// have bolts — a splice whose place is not known (their bolts their
/// members' design data, at both ends: spliced here, or one part through
/// the joint and spliced elsewhere) — in words; none otherwise (2026-11-19:
/// the leg splices of the private real towers, drawn lapped, their bolts
/// in the bracing's way).
pub fn splice_unknown(j: &JointDetail) -> Option<String> {
    let free = |a: &Arrival| {
        a.continuous.is_none()
            && a.onto.source != Source::Given
            && a.sits.source != Source::Given
            && a.kind.source != Source::Given
            && a.bolts.source != Source::Given
            && !a.modelling
    };
    let angle = |a: &Arrival| a.profile.as_ref().and_then(|p| p.angle);
    if j.arrivals.iter().any(|a| a.continuous.is_some()) {
        return None;
    }
    let mut best: Option<(i64, f64, &Arrival, &Arrival)> = None;
    for x in 0..j.arrivals.len() {
        for y in x + 1..j.arrivals.len() {
            let (a, b) = (&j.arrivals[x], &j.arrivals[y]);
            let (Some(ga), Some(gb)) = (angle(a), angle(b)) else {
                continue;
            };
            let dev = 180.0 - angle_deg(a.direction, b.direction);
            if dev > 20.0 || !free(a) || !free(b) {
                continue;
            }
            let size = ((ga.b + ga.h).min(gb.b + gb.h) * 1e6).round() as i64;
            if best.is_none_or(|(s, d, _, _)| size > s || (size == s && dev < d)) {
                best = Some((size, dev, a, b));
            }
        }
    }
    let (_, _, a, b) = best?;
    let (pa, pb) = (a.profile.as_ref()?, b.profile.as_ref()?);
    let alike = |u: [f64; 3], v: [f64; 3]| dot(u, v) > 0.9998;
    let bolted = |z: &Arrival| z.bolts.value.as_ref().is_some_and(|v| v.count > 0);
    (pa.section_id == pb.section_id
        && alike(pa.axes[1], pb.axes[1])
        && alike(pa.axes[2], pb.axes[2])
        && (bolted(a) || bolted(b)))
    .then(|| {
        let (a, b) = if a.member <= b.member { (a, b) } else { (b, a) };
        format!(
            "{} and {}: in line, of one section, with bolts (a splice): {SPLICE_UNKNOWN} (their \
             bolts may be their members' design data, at both ends) — drawn as given, lapped; \
             give the splice here, or the two one part",
            a.member, b.member
        )
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
///
/// Two angles of one section in line, turned alike, with bolts (a splice,
/// 2026-11-19): not joined — the bolts of a member's design data are at
/// both its ends, so where the splice is (here, or the legs one part
/// through this joint and spliced elsewhere) is not known; said
/// ([`splice_unknown`]), drawn as before.
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
    for (dev, x, y) in pairs {
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
        // No other end at the joint bolting onto them (or onto nothing
        // named, which would be named one of them): a splice's covers lie
        // where such an end would (its bolts through a cover: by hand;
        // measured on the private real towers, 2026-11-19: every such
        // splice of a leg had bracing on it).
        let alone = |a: &Arrival, b: &Arrival| {
            j.arrivals.iter().all(|z| {
                z.member == a.member
                    || z.member == b.member
                    || z.modelling
                    || z.onto
                        .value
                        .as_deref()
                        .is_some_and(|o| o != a.member && o != b.member)
            })
        };
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
            } else if pa.section_id != pb.section_id
                && along(pa, pb)
                && dev <= crate::joints::NESTED_ANGLE_DEG
                && bolted(a)
                && bolted(b)
                && alone(a, b)
            {
                // Step 1c: other sections in line, both with their bolts
                // (a splice where the section changes), nothing else
                // bolting onto them here: covers on both faces of the legs
                // bolted (the bolts' shear planes as the covers give them,
                // a given count still winning).
                Some((x, y, JoinKind::Spliced))
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
            let (covers, thickness) = if how == JoinKind::Spliced {
                use ohl_structures::connections::CoverLeg;
                let mut legs: Vec<CoverLeg> = Vec::new();
                for z in [&j.arrivals[m], &j.arrivals[o]] {
                    let both = z.leg.value == Some(ohl_structures::connections::BoltedLeg::Both);
                    let of = if both {
                        vec![CoverLeg::Long, CoverLeg::Short]
                    } else {
                        crate::joints::bolted_cover_leg(z).into_iter().collect()
                    };
                    for l in of {
                        if !legs.contains(&l) {
                            legs.push(l);
                        }
                    }
                }
                legs.sort_by_key(|l| l.as_str());
                (legs, ga.t.max(gb.t))
            } else {
                (Vec::new(), 0.0)
            };
            out.push(Join {
                member: j.arrivals[m].member.clone(),
                onto: j.arrivals[o].member.clone(),
                how,
                covers,
                thickness,
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

/// The classes of members in the owner's order (2026-11-19: "start with
/// aligning the main legs then diagonals and finally secondary braces with
/// redundant members being last"): the order the layout turns them in and
/// lays out a plane's ends and the planes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClassRank {
    /// The main legs (class `leg`; the chords as Orient angles turns them).
    Leg = 0,
    /// The diagonals: bracing (class `bracing` or `crossing`) not lying
    /// horizontal.
    Diagonal = 1,
    /// The secondary bracing: horizontals, plan and hip members (bracing
    /// lying horizontal within [`FLAT_OFF`]), and members of no known
    /// class (said).
    Secondary = 2,
    /// The redundant members (class `redundant`), last.
    Redundant = 3,
}

/// A member's place in the owner's order of the classes from its `class`
/// and its direction (unit): bracing lying horizontal (within
/// [`FLAT_OFF`]) is secondary, the rest of it a diagonal.
pub fn class_rank(class: Option<&str>, direction: [f64; 3]) -> ClassRank {
    match class {
        Some("leg") => ClassRank::Leg,
        Some("redundant") => ClassRank::Redundant,
        Some("bracing") | Some("crossing") if direction[2].abs() > FLAT_OFF.to_radians().sin() => {
            ClassRank::Diagonal
        }
        _ => ClassRank::Secondary,
    }
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
#[allow(clippy::too_many_arguments)]
fn strip_of(
    j: &JointDetail,
    a: &Arrival,
    host: &Arrival,
    s: &LayoutSettings,
    edge: EdgeRule<'_>,
    others: &[&str],
    tilt: Option<f64>,
    other: bool,
) -> Result<Strip, String> {
    let p = a.profile.as_ref().expect("checked");
    let g = p.angle.expect("checked");
    // Off the flange (step 4): the flange in whose plane its axis lies
    // (else the other).
    let f = match tilt {
        Some(_) => host_flange_along(a, host, other),
        None => host_flange(a, host),
    }
    .ok_or("no flange to lie on")?;
    // On the flange's inner face: as it sits; off the flange (step 4),
    // when it comes from the flange's other side (its axis behind the outer
    // face, or along it with its leg bolted facing away from it).
    let nb = if a.leg_y { p.axes[2] } else { p.axes[1] };
    let behind = dot(a.direction, f.out);
    let sin_flat = FLAT_OFF.to_radians().sin();
    let inside = match tilt {
        None => a.sits.value == Some(Face::Inside),
        Some(_) => behind < -sin_flat || (behind <= sin_flat && dot(nb, f.out) < 0.0),
    };
    // The face it lies on: its normal out of the steel.
    let face_out = if inside { scale(f.out, -1.0) } else { f.out };
    // Along the member it bolts onto, one way for every member of a run
    // through the joint (the ends on a leg in two members share a plane
    // and its axes).
    let x3 = {
        let d = host.direction;
        let first = d.iter().copied().find(|c| c.abs() > 1e-9).unwrap_or(1.0);
        if first < 0.0 { scale(d, -1.0) } else { d }
    };
    let y3 = f.across;
    let to2 = |q: [f64; 3]| -> [f64; 2] {
        let d = sub(q, f.heel);
        [dot(d, x3), dot(d, y3)]
    };
    let dir2 = |q: [f64; 3]| -> [f64; 2] { [dot(q, x3), dot(q, y3)] };
    let u = dir2(a.direction);
    let lu = (u[0] * u[0] + u[1] * u[1]).sqrt();
    // Its axis along the flange's face (within the flatness allowed); off
    // the flange (step 4) up to the most a plate in its own plane takes,
    // leaving the flange's face outwards.
    let axis_ok = match tilt {
        None => lu >= FLAT_OFF.to_radians().cos(),
        Some(_) => lu >= TILT_MAX.to_radians().cos(),
    };
    if !axis_ok {
        return Err(format!(
            "its axis runs {:.0}° out of the face of {}'s flange it bolts onto",
            lu.clamp(-1.0, 1.0).acos().to_degrees(),
            host.member
        ));
    }
    // Lengths along it measured on the flange's plane (an end off it,
    // step 4: its axis may leave the plane; 1 for the others, as before).
    let scale2 = if tilt.is_some() { lu } else { 1.0 };
    let u = [u[0] / lu, u[1] / lu];
    let v = [-u[1], u[0]];
    // On the inner face, leaving the joint towards the heel: it would run
    // across the angle's other leg — onto a bent plate round that leg
    // (J2b), a single angle's, the end itself flat on the face.
    let leaves_over = inside && u[1] < -1e-6;
    let host_angle = host
        .profile
        .as_ref()
        .and_then(|q| q.angle.map(|g| (q.axes, g)))
        .filter(|_| host.profile.as_ref().is_some_and(|q| q.parts.is_empty()));
    if leaves_over && (!s.bent_plates || tilt.is_some() || host_angle.is_none()) {
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
    let own_edge = a.edge_distance.value.filter(|_| a.edge_distance.known());
    let edge_end = |t: f64| -> (f64, f64) {
        edge(a, t).unwrap_or((own_edge.unwrap_or(edge_min), end.max(end_min)))
    };
    let edge = edge_end(f.t).0;
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
    // The owner's order of the classes (2026-11-19).
    let rank = class_rank(a.class.as_deref(), a.direction) as u8;
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
    let hand_shift = a.shift.0 != 0.0;
    let mut lat = lat;
    let gauges3 = gauges;
    let mut gauges: Vec<f64> = gauges3.iter().map(|gg| lat0 + sv * gg).collect();
    let tilted = match tilt {
        None => None,
        Some(deg) => {
            if u[1].abs() <= 1e-6 {
                return Err(format!("it runs along {}'s flange", host.member));
            }
            let nbo = dot(nb, face_out);
            if nbo < TILT_MAX.to_radians().cos() {
                return Err(if nbo <= 0.0 {
                    "its outstanding leg points towards the flange (it would stand on the plate)"
                        .to_owned()
                } else {
                    format!(
                        "more than {TILT_MAX:.0}° off (the plate would stand nearly square to \
                         the flange)"
                    )
                });
            }
            if hand_shift {
                return Err("its shift is given by hand".to_owned());
            }
            let shift_dir =
                ohl_structures::connections::shift_direction(a.direction, host.direction)
                    .filter(|_| shift_per.is_some())
                    .ok_or("no direction to shift it in")?;
            // Drawn with its gauge line through the work point (the
            // record's `gauge` mark): its heel there, as `joints` draws it.
            let heel_across = if a.leg_y { g.heel[0] } else { g.heel[1] };
            let g0 = gauges3.first().copied().unwrap_or(leg_w / 2.0);
            let base = scale(across3, -(heel_across + g0));
            let heel = add(
                base,
                add(scale(p.axes[1], g.heel[0]), scale(p.axes[2], g.heel[1])),
            );
            if a.cut.source == Source::Given {
                return Err("its cut is given by hand".to_owned());
            }
            let t = Tilted {
                off: deg,
                dir: a.direction,
                bolts: bolts.clone(),
                lu: scale2,
                half: a.length / 2.0,
                nb,
                across: across3,
                heel,
                axis: base,
                width: leg_w,
                gauges: gauges3.clone(),
                shift_dir,
                shift_per: shift_per.unwrap_or(1.0),
                keep: edge_end(g.t),
                x3,
                out: face_out,
                face: if inside {
                    add(f.heel, scale(f.out, -f.t))
                } else {
                    f.heel
                },
            };
            // Its footprint on the flange's plane: its section where it is
            // drawn and its plate's (to the seam), across the plane's `v`.
            let v3 = add(scale(x3, v[0]), scale(y3, v[1]));
            let l = |q: [f64; 3]| dot(q, v3);
            let mut lo = f64::INFINITY;
            let mut hi = f64::NEG_INFINITY;
            for pt in &p.outline {
                let x = l(add(
                    base,
                    add(scale(p.axes[1], pt[0]), scale(p.axes[2], pt[1])),
                ));
                lo = lo.min(x);
                hi = hi.max(x);
            }
            let geo = own_geometry(&t, 0.0, 0.0, t.face, t.out, g.t, s.gap)?;
            // At most a plate's thickness (its own leg's) farther out of
            // the flange than on a flat plate: else a hand detail at once
            // (its face's other ends then laid out as before).
            if geo.out_more > g.t + 1e-9 {
                return Err(farther_out(geo.out_more, g.t));
            }
            for c in &geo.outline {
                lo = lo.min(l(*c));
                hi = hi.max(l(*c));
            }
            lat = [lo, hi];
            gauges = t
                .gauges
                .iter()
                .map(|gg| l(add(heel, scale(across3, *gg))))
                .collect();
            Some(t)
        }
    };
    // Over the heel onto a bent plate (J2b): the other leg's outer face.
    let over = match host_angle.filter(|_| leaves_over) {
        None => None,
        Some((axes, hg)) => {
            let down = scale(f.out, -1.0);
            let other = if dot(down, axes[1]).abs() > 0.5 {
                hg.b
            } else {
                hg.h
            };
            let mine = if dot(f.across, axes[1]).abs() > 0.5 {
                hg.b
            } else {
                hg.h
            };
            let leg = if (hg.b - hg.h).abs() < 1e-9 {
                None
            } else if other > mine {
                Some("long")
            } else {
                Some("short")
            };
            let nb = scale(f.across, -1.0);
            let ob = dot(add(j.position, f.heel), nb);
            Some(Over {
                tp: g.t,
                keep: edge_end(g.t),
                other_w: other,
                plane_b: (mm(nb[0]), mm(nb[1]), mm(nb[2]), mm(ob)),
                down,
                leg,
                along_host: dot(x3, host.direction),
            })
        }
    };
    let other_w = if a.leg_y { g.h } else { g.b };
    let host_t = f.t;
    Ok(Strip {
        face3: if inside {
            add(f.heel, scale(f.out, -f.t))
        } else {
            f.heel
        },
        n3: face_out,
        x3,
        y3,
        heel3: f.heel,
        heel_lat: lat0,
        out_w: other_w,
        slab: g.t + g.t.max(host_t) + 0.7 * d,
        fixed: Vec::new(),
        floor: None,
        host_rank: class_rank(host.class.as_deref(), host.direction) as u8,
        over,
        tilted,
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
        gauges,
        shift_per,
        shift_now: a.shift.0,
        hand_cut: (a.cut.source == Source::Given).then(|| a.cut.value.unwrap_or(0.0)),
        hand_shift,
        half: a.length / 2.0 * scale2,
        bolts: bolts.iter().map(|(x, l)| (x * scale2, *l)).collect(),
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

/// How far strip `x` is moved across for its gauge line to run through
/// the work point (a shift given by hand, or no direction: not moved; an
/// end off the flange: its strip is drawn so already).
fn concentric_of(x: &Strip) -> f64 {
    if x.hand_shift || x.shift_per.is_none() || x.tilted.is_some() {
        0.0
    } else {
        -x.gauges.first().copied().unwrap_or(0.0)
    }
}

/// The most a member's leg bolted may lie off the flange for a plate in
/// its own plane, degrees (the owner's step 4, 2026-11-19): beyond it the
/// plate would stand nearly square to the flat one and the member high
/// over the flange. A starting value of the program's, not a standard's.
pub const TILT_MAX: f64 = 75.0;

/// A plate in a member's own plane where it is (from the work point, m).
#[derive(Debug, Clone)]
struct OwnGeo {
    /// The member lifted along the flange's normal to lie on it, m.
    lift: f64,
    /// How much farther out of the flange its axis lies at its bolts than
    /// on a flat plate of the same thickness (its line of action off the
    /// work point beyond a usual gusset's), m.
    out_more: f64,
    /// Its outline, on the face its member lies on.
    outline: Vec<[f64; 3]>,
    /// The seam's ends (on the flat plate's face).
    seam: [[f64; 3]; 2],
    /// A point of the seam.
    seam_at: [f64; 3],
    /// In the flat plate's plane, square to the seam, towards the flat
    /// plate (away from the member).
    side: [f64; 3],
}

/// The cut of end `t` along it (m, up to the millimetre) from its cut on
/// the flange's plane `c`.
fn cut3(t: &Tilted, c: f64) -> f64 {
    ceil_to(c / t.lu, 0.001)
}

/// How much a polygon drawn round an ellipse is grown so it never cuts
/// into it (eight corners: 1 / cos 22.5°).
const RING_GROW: f64 = 1.0824;

/// The plate in end `t`'s own plane (step 4) with its steel stopping
/// `cut` from the work point along it, moved `moved` across the plane's `v`
/// (its shift), the flat plate `tf` thick on the flange whose heel is at
/// `heel` (from the work point) and whose outer face looks along `out`.
///
/// The member is lifted along `out` (its place across the flange as laid
/// out) the least that keeps its leg bolted, from where its steel stops
/// to past its bolts, `gap` (the weld's room) on the plate's side of the
/// line where its plane meets the flat plate's face — the seam — and its
/// bolts the edge distance from it. The plate: round its bolts with the
/// end and edge distances (along and across it), down to the seam.
fn own_geometry(
    t: &Tilted,
    cut: f64,
    moved: f64,
    heel: [f64; 3],
    out: [f64; 3],
    tf: f64,
    gap: f64,
) -> Result<OwnGeo, String> {
    if t.bolts.is_empty() {
        return Err("no bolts to lay its plate round".to_owned());
    }
    let h = add(t.heel, scale(t.shift_dir, moved * t.shift_per));
    let pts: Vec<[f64; 3]> = t
        .bolts
        .iter()
        .map(|(along, line)| {
            let g = t.gauges.get(*line).copied().unwrap_or(t.width / 2.0);
            add(h, add(scale(t.dir, cut + along), scale(t.across, g)))
        })
        .collect();
    let (keep_edge, keep_end) = t.keep;
    // The flat plate's face.
    let q0 = add(heel, scale(out, tf));
    // The seam's direction, and up the plate from it.
    let d = unit(cross(t.nb, out)).ok_or("its leg lies parallel to the flange")?;
    let d = if dot(d, t.dir) < 0.0 {
        scale(d, -1.0)
    } else {
        d
    };
    let q = unit(cross(t.nb, d)).ok_or("its leg lies parallel to the flange")?;
    let q = if dot(q, out) < 0.0 { scale(q, -1.0) } else { q };
    let qo = dot(q, out);
    if qo < FLAT_OFF.to_radians().sin() / 2.0 {
        return Err("its leg lies parallel to the flange".to_owned());
    }
    if dot(q, t.dir) < -FLAT_OFF.to_radians().sin() {
        return Err("it runs down towards the flange away from the joint".to_owned());
    }
    // What keeps clear of the seam, each by how much along the plate:
    // its leg's two edges where its steel stops and past its bolts (the
    // gap), its bolts (the edge distance).
    let along = t.bolts.iter().map(|b| b.0).fold(0.0, f64::max) + keep_end;
    let mut keeps: Vec<([f64; 3], f64)> = Vec::new();
    for s in [cut, cut + along, t.half.max(cut + along)] {
        for w in [0.0, t.width] {
            keeps.push((add(h, add(scale(t.dir, s), scale(t.across, w))), gap));
        }
    }
    for b in &pts {
        keeps.push((*b, keep_edge * RING_GROW));
    }
    let lift = keeps
        .iter()
        .map(|(p, r)| dot(out, sub(q0, *p)) + r * qo)
        .fold(f64::NEG_INFINITY, f64::max);
    // Its axis at its bolts' middle out of the flange's face, against the
    // same member on a flat plate: its leg's face on the plate, its
    // centroid that far from it.
    let mid = t.bolts.iter().map(|b| b.0).sum::<f64>() / t.bolts.len() as f64 + cut;
    let shifted = sub(h, t.heel);
    let axis_at = add(
        add(add(t.axis, shifted), scale(out, lift)),
        scale(t.dir, mid),
    );
    let centroid = -dot(sub(t.heel, t.axis), t.nb);
    let out_more = dot(out, sub(axis_at, heel)) - (tf + centroid);
    // A point of the seam: from its heel where its steel stops, lifted,
    // down the plate to the flat plate's face.
    let p0 = add(add(h, scale(t.dir, cut)), scale(out, lift));
    let lp = sub(p0, scale(q, dot(out, sub(p0, q0)) / qo));
    // Its outline round its bolts (as `joints` draws a plate: the end
    // distance along the member, the edge distance across), to the seam.
    let across = unit(cross(t.nb, t.dir)).ok_or("no direction across it")?;
    let mut flat: Vec<[f64; 2]> = Vec::new();
    for b in &pts {
        let b = add(*b, scale(out, lift));
        for k in 0..8 {
            let a = f64::from(k) * std::f64::consts::FRAC_PI_4;
            let x = add(
                b,
                add(
                    scale(t.dir, keep_end * RING_GROW * a.cos()),
                    scale(across, keep_edge * RING_GROW * a.sin()),
                ),
            );
            let r = sub(x, lp);
            flat.push([dot(r, d), dot(r, q)]);
        }
    }
    let lo = flat.iter().map(|x| x[0]).fold(f64::INFINITY, f64::min);
    let hi = flat.iter().map(|x| x[0]).fold(f64::NEG_INFINITY, f64::max);
    flat.push([lo, 0.0]);
    flat.push([hi, 0.0]);
    let poly = clip(&crate::joint_sheet::hull(flat), |x| x[1]);
    let to3 = |x: &[f64; 2]| add(lp, add(scale(d, x[0]), scale(q, x[1])));
    // Towards the flat plate, square to the seam on its face.
    let side = unit(sub(scale(out, qo), q)).ok_or("no side")?;
    Ok(OwnGeo {
        lift,
        out_more,
        outline: poly.iter().map(to3).collect(),
        seam: [to3(&[lo, 0.0]), to3(&[hi, 0.0])],
        seam_at: lp,
        side,
    })
}

/// The cut (from `cut` on, m) that takes the plate in end `x`'s own plane
/// past the flange's toe by half the gap (step 4: under it, near the seam,
/// it is as low as the flat plate).
fn toe_clear(
    x: &Strip,
    t: &Tilted,
    p: &Placed,
    cut: f64,
    wp: [f64; 3],
    gap: f64,
) -> Result<f64, String> {
    let heel = t.face;
    let _ = wp;
    // Across the flange per metre of cut on the flange's plane: past the
    // toe for an end leaving towards it, past the heel for one leaving
    // over it (outside the angle).
    let rate = dot(t.dir, x.across) / t.lu;
    if rate.abs() <= 1e-6 {
        return Err("it runs along the flange".to_owned());
    }
    let mut c = cut;
    for _ in 0..4 {
        let geo = own_geometry(t, c / t.lu, p.moved, heel, t.out, x.own_t, gap)?;
        let ys = geo.outline.iter().map(|q| dot(sub(*q, heel), x.across));
        let need = if rate > 0.0 {
            x.width + gap / 2.0 - ys.fold(f64::INFINITY, f64::min)
        } else {
            ys.fold(f64::NEG_INFINITY, f64::max) + gap / 2.0
        };
        if need <= 1e-9 {
            break;
        }
        c += need / rate.abs() + 1e-6;
    }
    Ok(ceil_to(c, 0.001))
}

/// The points the flat plate under the plate in end `x`'s own plane must
/// cover (structure axes): the seam's ends and the flange from the work
/// point along the member it bolts onto as far as the seam reaches, heel
/// to toe, on the flat plate's face.
fn own_reach(x: &Strip, geo: &OwnGeo, tf: f64, wp: [f64; 3]) -> Vec<[f64; 3]> {
    let t = x.tilted.as_ref().expect("tilted");
    let heel = t.face;
    let mut out = vec![add(geo.seam[0], wp), add(geo.seam[1], wp)];
    for c in [[0.0; 3], geo.seam[0], geo.seam[1]] {
        let along = dot(sub(c, heel), t.x3);
        let h = add(add(heel, scale(t.x3, along)), scale(t.out, tf));
        out.push(add(h, wp));
        out.push(add(add(h, scale(x.across, x.width)), wp));
    }
    out
}

/// Whether the plate in end `x`'s own plane, as `geo`, can be welded to
/// the flat plate of its face: the flat plate keeps to its side of the
/// seam with the bolts of the ends on it (`flat`) and the seams of the
/// plates kept before (`owns`), and reaches the flange over at least half a
/// square of its width (room for its own bolts to it; a starting value of
/// the program's, not a standard's).
fn own_fits(
    strips: &[Strip],
    x: &Strip,
    geo: &OwnGeo,
    flat: &[Placed],
    owns: &[(usize, OwnGeo)],
    wp: [f64; 3],
    gap: f64,
) -> Result<(), String> {
    let t = x.tilted.as_ref().expect("tilted");
    let heel = sub(x.heel, wp);
    let to2 = |q: [f64; 3]| [dot(sub(q, heel), t.x3), dot(sub(q, heel), x.across)];
    let dir2 = |q: [f64; 3]| {
        let v = [dot(q, t.x3), dot(q, x.across)];
        let l = (v[0] * v[0] + v[1] * v[1]).sqrt().max(1e-12);
        [v[0] / l, v[1] / l]
    };
    let side = |g: &OwnGeo, q: [f64; 2]| dot2(sub2(q, to2(g.seam_at)), dir2(g.side));
    for p in flat {
        let b = &strips[p.k];
        for (along, line) in &b.bolts {
            let l = b.gauges.get(*line).copied().unwrap_or(0.0) + p.moved;
            let q = b.at(p.cut + along, l);
            let d = side(geo, q);
            if d < b.edge - 1e-9 {
                return Err(format!(
                    "its seam would pass {:.0} mm from {}'s bolts on the flat plate (they keep \
                     {:.0} mm)",
                    d.max(0.0) * 1000.0,
                    b.member,
                    b.edge * 1000.0
                ));
            }
        }
    }
    for (k, g) in owns {
        let ends = |h: &OwnGeo| [to2(h.seam[0]), to2(h.seam[1])];
        if ends(g).iter().any(|q| side(geo, *q) < -1e-3)
            || ends(geo).iter().any(|q| side(g, *q) < -1e-3)
        {
            return Err(format!(
                "its seam would cut the flat plate under {}'s welded plate",
                strips[*k].member
            ));
        }
    }
    // The flat plate's reach on the flange (its own reach and the others'),
    // each seam's side kept, heel (by half the gap) to toe.
    let mut pts: Vec<[f64; 2]> = own_reach(x, geo, 0.0, wp)
        .into_iter()
        .map(|q| to2(sub(q, wp)))
        .collect();
    let mut cuts: Vec<&OwnGeo> = vec![geo];
    for (k, g) in owns {
        pts.extend(
            own_reach(&strips[*k], g, 0.0, wp)
                .into_iter()
                .map(|q| to2(sub(q, wp))),
        );
        cuts.push(g);
    }
    let mut poly = crate::joint_sheet::hull(pts);
    for g in cuts {
        let (at, n) = (to2(g.seam_at), dir2(g.side));
        poly = clip(&poly, |q| dot2(sub2(q, at), n) + 1e-9);
    }
    // On an inner face clear of the angle's other leg and root.
    let floor = if x.inside { x.t + x.r } else { 0.0 } + gap / 2.0;
    let poly = clip(&poly, |q| q[1] - floor);
    let poly = clip(&poly, |q| x.width - q[1]);
    let area = poly
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let b = poly[(i + 1) % poly.len()];
            a[0] * b[1] - a[1] * b[0]
        })
        .sum::<f64>()
        .abs()
        / 2.0;
    if area < x.width * x.width / 2.0 {
        return Err(format!(
            "the flat plate it is welded to would reach only {:.0} cm² of {}'s flange (half a \
             square of its width, {:.0} cm², at least)",
            area * 1e4,
            x.host,
            x.width * x.width / 2.0 * 1e4
        ));
    }
    Ok(())
}

/// Why a plate in a member's own plane is not laid out: its axis `more`
/// farther out of the flange than on a flat plate, more than a plate of
/// thickness `t` (its leg's), m.
fn farther_out(more: f64, t: f64) -> String {
    format!(
        "its axis would lie {:.0} mm farther out of the flange than on a flat plate (more than a \
         plate's {:.0} mm: its line of action that far off the work point is not in the analysis)",
        more * 1000.0,
        t * 1000.0
    )
}

/// Why end `x` off the flange is left for a hand detail.
fn tilted_why(x: &Strip, t: &Tilted, e: &str) -> String {
    format!(
        "{}: its bolted leg lies {:.0}° off the flange of {} it bolts onto and no turn lays it \
         flat at both its ends; a welded plate in its own plane does not fit: {e}",
        x.member, t.off, x.host
    )
}

/// The throat of the weld of a plate in its member's own plane to the
/// flat plate, each side, m: welded from both sides, partly penetrated,
/// four tenths of the plates' thickness each (whole millimetres, at least
/// 3 mm) — the layout's choice, not a standard's; checked as fillet welds
/// (SIA 263 6.3.3.5).
pub fn weld_throat(t: f64) -> f64 {
    ((0.4 * t * 1000.0 + 1e-6).floor() / 1000.0).max(0.003)
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
    let limit = (s.pull_back * x.tilted.as_ref().map_or(1.0, |t| t.lu)).min(x.half - bolt_run(x));
    if limit <= 0.0 {
        return Err(format!(
            "{}: too short for its bolts ({:.0} mm to half its length)",
            x.member,
            x.half * 1000.0
        ));
    }
    // Concentric: its first gauge line through the work point.
    let concentric = concentric_of(x);
    let cut_at = |moved: f64| -> (f64, Option<String>) {
        let lat = [x.lat[0] + moved, x.lat[1] + moved];
        // Its leg on the flange's side of the heel (a member leaving the
        // joint towards the toe, as bracing does at a leg; one leaving towards
        // the heel crosses over it, as a crossing diagonal: nothing of the
        // host there — the clash check sees the rest).
        let mut need = f64::NEG_INFINITY;
        let mut by: Option<String> = None;
        // Over the heel onto a bent plate (J2b): past the other leg's
        // outer face, the plate and its bend.
        if let Some(o) = &x.over {
            let r = bend_radius(o.tp);
            let (c, w) = over_cut(x, moved, over_steel(o.tp, s.gap), o.tp + r + o.keep.0);
            need = c;
            by = Some(w.to_owned());
        }
        for l in lat {
            if x.over.is_some() {
                break;
            }
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
        // The fixed steel in its plane (J1/J2): clear of it by the gap.
        for (name, poly) in &x.fixed {
            let c = poly_need(x, lat, poly, s.gap);
            if c > need {
                need = c;
                by = Some(name.clone());
            }
        }
        (need.max(0.0), by)
    };
    // The least cut a clash asked for (the run's rounds): beyond the
    // longest pull-back it is left with that clash.
    if let Some((f, why)) = &x.floor
        && x.hand_cut.is_none()
        && *f > limit + 1e-9
    {
        return Err(format!("{}: {why}", x.member));
    }
    let floor = x.floor.as_ref().map_or(0.0, |f| f.0);
    let cut_at = |moved: f64| -> (f64, Option<String>) {
        let (need, by) = cut_at(moved);
        if floor > need {
            (floor, by)
        } else {
            (need, by)
        }
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

/// A plane: its normal and offset (mm, rounded).
type PlaneKey = (i64, i64, i64, i64);

/// The best placing a search found: the ends placed, those left with why,
/// how many placed and the eccentricity (tenths of a millimetre).
type Best = (Vec<Placed>, Vec<(usize, String)>, usize, i64);

/// What [`search_plane`] found for a plane: the ends placed, those that
/// fit in no order with why, and whether it stopped at the limit.
struct Found {
    placed: Vec<Placed>,
    failed: Vec<(usize, String)>,
    stopped: bool,
}

/// The ends `order` of one plane (the owner's order) placed by a search
/// with backtracking (J1, 2026-11-19): at each step every end not placed
/// yet is tried (concentric and pulled back, else the least shift that
/// fits, [`place`]); an end that fits nowhere now fits nowhere later (each
/// end placed only adds what the others keep clear of), so it is left;
/// each that fits is placed in turn, in the owner's order, and the rest
/// searched again. The best: the most ends placed, then the least
/// eccentricity (the moves off concentric, mm), then the first found — the
/// first found is the owner's order filled one after the other, as before.
/// Placings already reached in another order are not searched again.
/// `budget`: the placings left for the joint, spent.
fn search_plane(
    strips: &[Strip],
    order: &[usize],
    s: &LayoutSettings,
    budget: &mut usize,
) -> Found {
    struct Dfs<'a> {
        strips: &'a [Strip],
        order: &'a [usize],
        s: &'a LayoutSettings,
        budget: &'a mut usize,
        stopped: bool,
        seen: BTreeSet<Vec<(usize, i64, i64)>>,
        // Placed, failed with why, (count, eccentricity in tenths of mm).
        best: Option<Best>,
    }
    impl Dfs<'_> {
        fn ecc(&self, placed: &[Placed]) -> i64 {
            placed
                .iter()
                .map(|p| {
                    let c = concentric_of(&self.strips[p.k]);
                    ((p.moved - c).abs() * 1e4).round() as i64
                })
                .sum()
        }
        fn done(&self) -> bool {
            self.best
                .as_ref()
                .is_some_and(|b| b.2 == self.order.len() && b.3 == 0)
        }
        fn go(&mut self, placed: &mut Vec<Placed>) {
            if *self.budget == 0 {
                self.stopped = true;
                return;
            }
            *self.budget -= 1;
            let mut key: Vec<(usize, i64, i64)> = placed
                .iter()
                .map(|p| {
                    (
                        p.k,
                        (p.cut * 1e4).round() as i64,
                        (p.moved * 1e4).round() as i64,
                    )
                })
                .collect();
            key.sort_unstable();
            if !self.seen.insert(key) {
                return;
            }
            let mut fits: Vec<Placed> = Vec::new();
            let mut fails: Vec<(usize, String)> = Vec::new();
            for &k in self.order {
                if placed.iter().any(|p| p.k == k) {
                    continue;
                }
                match place(self.strips, k, placed, self.s) {
                    Ok(p) => fits.push(p),
                    Err(e) => fails.push((k, e)),
                }
            }
            if fits.is_empty() {
                let (n, e) = (placed.len(), self.ecc(placed));
                if self
                    .best
                    .as_ref()
                    .is_none_or(|b| n > b.2 || (n == b.2 && e < b.3))
                {
                    self.best = Some((placed.clone(), fails, n, e));
                }
                return;
            }
            // No placing below can beat the best.
            if let Some(b) = &self.best
                && (placed.len() + fits.len() < b.2
                    || (placed.len() + fits.len() == b.2 && b.3 == 0))
            {
                return;
            }
            for p in fits {
                placed.push(p);
                self.go(placed);
                placed.pop();
                if self.done() || self.stopped {
                    return;
                }
            }
        }
    }
    let mut d = Dfs {
        strips,
        order,
        s,
        budget,
        stopped: false,
        seen: BTreeSet::new(),
        best: None,
    };
    d.go(&mut Vec::new());
    let stopped = d.stopped;
    match d.best {
        Some((mut placed, failed, _, _)) => {
            // In the owner's order (as the plates and the record list
            // them).
            placed.sort_by_key(|p| order.iter().position(|k| *k == p.k));
            Found {
                placed,
                failed,
                stopped,
            }
        }
        // Not even the first placing (no budget left): each end tried
        // alone, in order, as before the search.
        None => {
            let mut placed: Vec<Placed> = Vec::new();
            let mut failed = Vec::new();
            for &k in order {
                match place(strips, k, &placed, s) {
                    Ok(p) => placed.push(p),
                    Err(e) => failed.push((k, e)),
                }
            }
            Found {
                placed,
                failed,
                stopped: true,
            }
        }
    }
}

/// How far a part reaching above the face may lie under it and still
/// only touch it, m (half the clash check's tolerance).
const FACE_TOUCH: f64 = crate::clash::CLASH_TOLERANCE / 2.0;

/// The footprint on `x`'s plane of a convex piece (its corners `pts`, 3D
/// from the work point) where it lies in `x`'s slab (from the face to what
/// `x` carries over it): convex, 2D in the plane's axes; none where it
/// lies outside.
fn footprint(x: &Strip, pts: &[[f64; 3]]) -> Option<Vec<[f64; 2]>> {
    let h = |q: &[f64; 3]| dot(sub(*q, x.face3), x.n3);
    let (lo, hi) = (FACE_TOUCH, x.slab);
    let hs: Vec<f64> = pts.iter().map(h).collect();
    if hs.iter().all(|v| *v < lo) || hs.iter().all(|v| *v > hi) {
        return None;
    }
    // Its corners in the slab, and where the lines between two corners
    // cross the slab's faces (inside the piece: its section's corners
    // among them).
    let mut inside: Vec<[f64; 3]> = Vec::new();
    for (i, a) in pts.iter().enumerate() {
        let ha = hs[i];
        if (lo..=hi).contains(&ha) {
            inside.push(*a);
        }
        for (k, b) in pts.iter().enumerate().skip(i + 1) {
            let hb = hs[k];
            for lim in [lo, hi] {
                if (ha - lim) * (hb - lim) < 0.0 {
                    let t = (lim - ha) / (hb - ha);
                    inside.push(add(*a, scale(sub(*b, *a), t)));
                }
            }
        }
    }
    let flat: Vec<[f64; 2]> = inside
        .iter()
        .map(|q| {
            let d = sub(*q, x.heel3);
            [dot(d, x.x3), dot(d, x.y3)]
        })
        .collect();
    let poly = crate::joint_sheet::hull(flat);
    (poly.len() >= 3).then_some(poly)
}

/// The least cut of `x` (its leg across `lat`) that keeps it `gap` clear
/// of the fixed footprint `poly`: the farthest point along it of the
/// footprint within its leg widened by the gap (and within half its
/// length), and the gap; minus infinity where they never meet.
fn poly_need(x: &Strip, lat: [f64; 2], poly: &[[f64; 2]], gap: f64) -> f64 {
    let rel = |p: [f64; 2]| sub2(p, x.w);
    let q = clip(poly, |p| dot2(rel(p), x.v) - (lat[0] - gap));
    let q = clip(&q, |p| (lat[1] + gap) - dot2(rel(p), x.v));
    let q = clip(&q, |p| x.half - dot2(rel(p), x.u));
    if q.is_empty() {
        return f64::NEG_INFINITY;
    }
    q.iter()
        .map(|p| dot2(rel(*p), x.u))
        .fold(f64::NEG_INFINITY, f64::max)
        + gap
}

/// What laid out steel is (3D corners from the work point): a member's,
/// or a bolt's head (whose member's steel it is not).
type Laid3 = (String, bool, Vec<[f64; 3]>);

/// The fixed steel in strip `x`'s way, each by name as its footprint on
/// its plane: the pieces of the members of `fixed` (and their bolts), the
/// plates and covers given by hand, and what the planes before laid out
/// (`laid3`) — not what `x` bolts onto (and a member running on from it
/// in line), nor what bolts onto `x`; a bolt's shank through those is not
/// in the way (its head and nut are).
fn fixed_in(
    x: &Strip,
    j: &JointDetail,
    pieces: &[(crate::ClashPart, Vec<[f64; 3]>)],
    fixed: &BTreeSet<String>,
    laid3: &[Laid3],
) -> Vec<(String, Vec<[f64; 2]>)> {
    use crate::ClashPart;
    use crate::clash::BoltPiece;
    let find = |m: &str| j.arrivals.iter().find(|a| a.member == m);
    let me = find(&x.member);
    let host = find(&x.host);
    let in_line = |a: &Arrival, b: &Arrival| angle_deg(a.direction, b.direction) > 160.0;
    let host_run = |m: &str| m == x.host || host.zip(find(m)).is_some_and(|(h, b)| in_line(h, b));
    let onto_me = |m: &str| {
        find(m).is_some_and(|b| {
            b.onto.value.as_deref() == Some(x.member.as_str())
                || b.other_onto.as_deref() == Some(x.member.as_str())
                || b.continuous.as_deref() == Some(x.member.as_str())
        }) || me.is_some_and(|a| a.continuous.as_deref() == Some(m))
    };
    let mut out = Vec::new();
    for (part, pts) in pieces {
        let name = match part {
            ClashPart::Member(m) => {
                if m == &x.member || !fixed.contains(m) || host_run(m) || onto_me(m) {
                    continue;
                }
                m.clone()
            }
            ClashPart::Bolt {
                member: m, piece, ..
            } => {
                if m == &x.member || !fixed.contains(m) {
                    continue;
                }
                if *piece == BoltPiece::Shank && (host_run(m) || onto_me(m)) {
                    continue;
                }
                part.words()
            }
            ClashPart::Plate(id) => {
                if me.is_some_and(|a| a.onto.value.as_deref() == Some(id.as_str())) {
                    continue;
                }
                part.words()
            }
            ClashPart::Cover(sid, _) => {
                if j.splices.iter().any(|sp| {
                    &sp.id == sid
                        && (sp.members.contains(&x.member) || sp.bolted.contains(&x.member))
                }) {
                    continue;
                }
                part.words()
            }
        };
        if let Some(p) = footprint(x, pts) {
            out.push((name, p));
        }
    }
    for (m, bolt, pts) in laid3 {
        if m == &x.member || (!bolt && (host_run(m) || onto_me(m))) {
            continue;
        }
        if let Some(p) = footprint(x, pts) {
            out.push((m.clone(), p));
        }
    }
    out
}

/// The steel of the ends `placed` (single angles flat on their flange) as
/// laid out, `lift` over their face (on a plate that thick: the plate
/// too), 3D corners from the work point: each leg's box from its cut to
/// half its length (the outstanding leg as high as it is wide), each bolt's
/// head a box round it — what the planes after keep clear of.
fn solids_of(strips: &[Strip], placed: &[Placed], lift: f64) -> Vec<Laid3> {
    let mut out: Vec<Laid3> = Vec::new();
    let mut plate: Vec<[f64; 2]> = Vec::new();
    let mut first: Option<&Strip> = None;
    for p in placed {
        let x = &strips[p.k];
        if x.tilted.is_some() {
            continue;
        }
        first.get_or_insert(x);
        let off = dot(sub(x.face3, x.heel3), x.n3);
        let to3 = |q: [f64; 2], h: f64| {
            add(
                add(add(x.heel3, scale(x.x3, q[0])), scale(x.y3, q[1])),
                scale(x.n3, off + h),
            )
        };
        let boxed = |s: [f64; 2], l: [f64; 2], h: [f64; 2]| -> Vec<[f64; 3]> {
            let mut c = Vec::new();
            for a in s {
                for b in l {
                    for z in h {
                        c.push(to3(x.at(a, b), z));
                    }
                }
            }
            c
        };
        let lat = [x.lat[0] + p.moved, x.lat[1] + p.moved];
        out.push((
            x.member.clone(),
            false,
            boxed([p.cut, x.half], lat, [lift, lift + x.own_t]),
        ));
        // The outstanding leg, at the heel's edge.
        let lh = x.heel_lat + p.moved;
        let inward = if (lh - lat[0]).abs() <= (lh - lat[1]).abs() {
            x.own_t
        } else {
            -x.own_t
        };
        out.push((
            x.member.clone(),
            false,
            boxed(
                [p.cut, x.half],
                [lh.min(lh + inward), lh.max(lh + inward)],
                [lift, lift + x.out_w],
            ),
        ));
        for (along, line) in &x.bolts {
            let l = x.gauges.get(*line).copied().unwrap_or(0.0) + p.moved;
            let a = p.cut + along;
            let r = x.nut;
            out.push((
                x.member.clone(),
                true,
                boxed(
                    [a - r, a + r],
                    [l - r, l + r],
                    [lift + x.own_t, lift + x.own_t + 0.7 * x.d],
                ),
            ));
            if lift > 0.0 {
                let e = x.edge + 1e-3;
                for (da, dl) in [(-e, -e), (-e, e), (e, -e), (e, e)] {
                    plate.push(x.at(a + da, l + dl));
                }
            }
        }
    }
    if let Some(x) = first
        && lift > 0.0
    {
        let off = dot(sub(x.face3, x.heel3), x.n3);
        let mut c = Vec::new();
        for q in crate::joint_sheet::hull(plate) {
            for z in [0.0, lift] {
                c.push(add(
                    add(add(x.heel3, scale(x.x3, q[0])), scale(x.y3, q[1])),
                    scale(x.n3, off + z),
                ));
            }
        }
        out.push((format!("a plate on {}", x.host), false, c));
    }
    out
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

/// `a` made a unit vector (none: about zero).
fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let l = norm(a);
    (l > 1e-9).then(|| scale(a, 1.0 / l))
}
