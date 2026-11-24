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

/// The order a joint's planes are laid out in: how deep what they bolt
/// onto is laid out on others, its rank, (L18, where asked) the plane's
/// biggest member's class and leg width, its id, the plane.
type PlaneOrder = (usize, u8, (u8, i64), String, PlaneKey);

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
    /// L1 (a), bottom up, then out (owner 2026-11-19; [`mod@crate::joint_order`]):
    /// the joints taken in that order where the order matters — the joint a
    /// kind is planned at (J4), and a member that no one roll lays flat at
    /// all its ends, none of them flat now, turned to lie flat at its end
    /// first in the order. On by default (measured on the private real
    /// towers: a little more laid out); off: as before (for the comparison).
    pub bottom_up: bool,
    /// L1 (b), slightly more plates (owner 2026-11-19): a face bolted
    /// directly also tried on a plate where that lays out more of the
    /// joint's ends, and an end bolted directly that would be left after
    /// writing (its clash or misfit not cleared by pulling it back) tried on
    /// a plate first; direct bolting wins a tie. On by default; off: as
    /// before.
    pub more_plates: bool,
    /// L1 (d), what an end bolts onto chosen in the search (owner
    /// 2026-11-19): for an end left for a hand detail, the other members it
    /// could bolt onto tried too (today's rule first and the tie-break),
    /// kept at a joint only where it lays out more there after the clash
    /// rounds than today's plan does. On by default; off: as before.
    pub onto_search: bool,
    /// L2 (a), each plate the layout makes names the member whose flange
    /// it lies on (`on`, structure file version 23; 2026-11-20): drawn,
    /// seated and checked for clashes on that member,
    /// as planned — not on the joint's leg where that is another member.
    /// On by default; off: as before (for the comparison).
    pub plate_on: bool,
    /// L2 (b), clash-aware planning (2026-11-20): the planner sees what the
    /// clash check sees — an end's outstanding leg to its full height, the
    /// nuts behind the flange or plate, each plate's outline as it will be
    /// drawn (round the work point, the members' legs over their bolts and
    /// the flange under it) against the fixed steel and what the planes
    /// before laid out, the laid-out ends' bolts whole. On by default; off:
    /// as before.
    pub exact_plan: bool,
    /// L2 (c), tries before an end is left for a clash (2026-11-20): an end
    /// on a plate whose bolts land on the flange bolted directly instead,
    /// once, where the plate or what is on it runs into something. On by
    /// default; off: as before.
    pub clash_tries: bool,
    /// L1 (c) looked at again with L2 (2026-11-20): on a face that goes onto
    /// a plate, the redundants and the body's diagonals whose bolts land on
    /// the flange bolted directly first. Off by default (measured).
    pub direct_first: bool,
    /// L2's follow-up (a), 2026-11-20: a plate whose outline round the work
    /// point would run into something is given an outline that stops short
    /// of the work point — round its members' legs from the flange's toe
    /// over their bolts (the code's end and edge distances round each bolt)
    /// and the flange under them —, where that clears it. On by default;
    /// off: as before (for the comparison).
    pub short_plates: bool,
    /// L2's follow-up (b), 2026-11-20: a bent plate's own bolts onto the
    /// member it lies on slid along it clear of the holes there (the
    /// default minimum spacing from each). On by default; off: as before.
    pub slide_holds: bool,
    /// L3, 2026-11-20: an end whose leg bolted lies at most
    /// [`BEND_SMALL_MAX`] off its flange (its member needing rolls a few
    /// degrees apart at its two ends) on a plate bent by as much from the
    /// flat plate on the flange — away from the flange, or towards it past
    /// the flange's toe — before a welded plate (step 4). On by default;
    /// off: as before.
    pub small_bends: bool,
    /// L3b, 2026-11-20: an equal-leg angle the half turn (the layout's step
    /// 2b) did not help, left at an end at most [`BEND_SMALL_MAX`] off its
    /// flange with its outstanding leg towards it, tried with its other leg
    /// bolted — the quarter turn that faces that end's leg away from the
    /// flange (its end lying flat stays flat) —, kept where more of its
    /// ends are planned laid out and its joints lay out no fewer. With
    /// [`Self::small_bends`]. On by default; off: as before.
    pub bend_facing: bool,
    /// L3b, 2026-11-20: an end at most [`BEND_SMALL_MAX`] off its flange
    /// that runs towards the bend line going away from the joint also on a
    /// plate bent away from the flange — its steel kept clear of the bend
    /// (half the gap beyond its curved part) wherever it lies over the
    /// flange (to half the gap past the flange's toe), its bolts their edge
    /// distance from it, its axis within a plate's thickness as every bent
    /// plate's. With [`Self::small_bends`]. On by default; off: as before.
    pub bend_back: bool,
    /// L5, 2026-11-20 (the owner's idea: the diagonals "a bit forward" so
    /// that their bolt lies on the leg without them touching each other): on
    /// a face that goes onto a plate, an end whose bolts do not land on the
    /// flange tried pulled back further, else moved sideways by at most the
    /// plate's thickness (the eccentricity that plate would have put it out
    /// of the flange by), until every bolt lands; where every end of the
    /// face then lands, the face is bolted directly — no plate. The move is
    /// the end's `shift`, which the analysis takes. On by default; off: as
    /// before.
    pub direct_moved: bool,
    /// L5o, 2026-11-20 (the owner's decision of that day): a flat plate on
    /// a member's flange that nothing of the joint's legs bolts onto (that
    /// member running on through the joint) given two provisional bolts of
    /// its own onto that member (more where one member on it has more), as
    /// J2b's bent plates' flats have them — so that the joint checks check
    /// how it is held. Where they do not fit, the plate is laid out without
    /// them (its holding "not checked", said). On by default; off: as
    /// before.
    pub hold_plates: bool,
    /// J6, 2026-11-20: a face of ends bolting onto a built-up member whose
    /// angles are known (two back to back, two in a star, four in a cross)
    /// whose bolts do not all land on that angle's flange goes onto a plate
    /// on the angle facing them, as on a single angle — kept at a joint
    /// only where it lays out more of its ends, or as many on fewer plates,
    /// than bolting those that land and leaving the others (as before).
    /// The plate keeps clear of the member's other angles (in the gap
    /// between them: by hand, the packing there not known). On by default;
    /// off: as before.
    pub built_up_plates: bool,
    /// L8, 2026-11-21 (L7's proposal 3): a member the run names what it
    /// bolts onto in its planning copy (so that its leg and turn are those
    /// on that member) but does not lay out is drawn without that name
    /// (the run writes only what it lays out) — elsewhere. Its joint is
    /// then planned once more with that member where the file will draw
    /// it (the name taken out), so that the plates, ends and holding bolts
    /// on it are planned where it lies; kept at a joint only where it lays
    /// out more ends, or as many on no more plates, else planned as before.
    /// On by default; off: as before.
    pub as_drawn: bool,
    /// L8, 2026-11-21 (L7's proposal 4): a member left as it is that bolts
    /// onto an end the plan lays out (the partner of a pair bolting onto
    /// each other, its own end left as it is) keeps its bolts on that end:
    /// the end's cut no further than the partner's first bolt less its end
    /// distance, its move such that every partner bolt's hole stays within
    /// its flange, its own bolts the default minimum spacing from the
    /// partner's; where no cut and move within the longest pull-back and
    /// the shift limit does, the end is left for a hand detail in the plan
    /// (before: laid out, then taken back after writing). On by default;
    /// off: as before.
    pub partner_bolts: bool,
    /// L9, 2026-11-21 (L8's question 5): a flat plate with plates in their
    /// members' own planes welded to it (step 4) planned with its outline
    /// as the run will write it — round the seams and the flange under them
    /// and the work point, and the bolts of the ends flat on it — clear of
    /// the fixed steel and of what the planes before laid out; else stopping
    /// short of the work point and cut straight short of what it would
    /// still run into; else the ends in their own planes left for a hand
    /// detail in the plan (before: not checked, found after writing). On by
    /// default; off: as before.
    pub reach_clear: bool,
    /// L9, 2026-11-21 (the owner's answer to L6's question 2): a flat plate
    /// stopping short of the work point given its own provisional bolts onto
    /// the member it lies on (L5o) where its outline, grown round them as the
    /// joint details draw it, still runs into nothing — else none, said (as
    /// before: none on a short plate). On by default; off: as before.
    pub short_holds: bool,
    /// L10, 2026-11-21 (the owner's idea from photographs of real towers:
    /// members on either side of a leg's flange): where two or more ends on
    /// one face of a leg's flange go onto a plate there, or one is left for
    /// want of room beside another, one of them tried on the flange's other
    /// face — half a turn of its member (the module `other_face`) —, kept
    /// where its joints then lay out more ends, or as many on fewer plates
    /// (none fewer at any of them), and only where its leg then lies no
    /// farther from the work point across the flange than it lies now
    /// (otherwise proposed in the run's notes, not applied). On by default;
    /// off: as before.
    pub other_face: bool,
    /// L10: of a face's ends, those following the photographs' pattern
    /// tried first — a diagonal onto the inner face, a horizontal onto the
    /// outer one —, else the later class first. On by default; off: the
    /// later class first (for the comparison).
    pub face_pattern: bool,
    /// L10 (the owner, 2026-11-21: bolts drawn at a laid-out joint that
    /// join nothing): a joint where an end left as it is bolts onto nothing
    /// but has bolts drawn there (given, or its member's design data) is
    /// not laid out whole — that member by hand, said with why. On by
    /// default; off: as before (such a joint could count as whole).
    pub nothing_joined: bool,
    /// L11, 2026-11-21 (the owner, at a leg splice: "the diagonals need a
    /// horizontal shift and a pull back as well as a plate"): an end going
    /// onto a plate whose bolts would need more than the longest pull-back
    /// to pass the flange's edge moved sideways — the least whole
    /// millimetre within the shift limit (its `shift`, an eccentricity the
    /// analysis takes) — so that they pass it within the pull-back, clear of
    /// the plane's other ends as they lie; kept at a joint only where its
    /// plan lays out more ends, or as many on fewer plates. **Off by default
    /// in a run** (measured on a sample of the private real towers: alone it
    /// laid out fewer ends at more joints than it gained); Joint details'
    /// Solve the clashes tries it per joint ([`crate::solve`]), where the
    /// joint's outcome is weighed.
    pub plate_shift: bool,
    /// L11, 2026-11-21 (the owner's picture: "both diagonals could be on
    /// the same plate just bolted on different sides"): an end the plan puts
    /// on a plate in its own plane whose leg bolted lies flat in a flange's
    /// plane on its inner side, leaving over the heel, on the back of the
    /// flat plate on that flange's outer face instead, past the heel
    /// (the module `both_faces`). On by default; off: as before.
    pub both_faces: bool,
    /// L12, 2026-11-21 (the joints grouped by what comes in): a chord
    /// running on through joints whose ends say it is turned (as L11's leg,
    /// [`crate::piece_rolls`] of [`crate::joint_classes::chord_pieces`])
    /// turned by what they say, with its pieces in line alike, kept where
    /// its joints lay out more ends, or as many on fewer plates, none fewer
    /// at any of them. **Off by default** (a chord's roll is Orient
    /// angles'; the owner decides); on: measured.
    pub chord_turns: bool,
    /// L13, 2026-11-22 (L12's finding: an end's axis out of its host's
    /// flanges' planes leaves most ends for a hand detail): an end whose
    /// axis lies in neither flange's plane of what today's rule names for
    /// it, nor of a member running through the joint, named onto a member
    /// **ending** at the joint in a flange's plane of which its axis lies
    /// (a plan or hip member onto a face horizontal's level leg; a face
    /// member onto another of its face) — of a class at least its own, as
    /// wide, before today's choice ([`hosts_to_name_by`]); weighed joint by
    /// joint after writing (the run's as-drawn pass: the joint planned
    /// again without these names, today's kept where the new is not
    /// better). On by default; off: as before.
    pub plane_hosts: bool,
    /// L14 (the owner's answer of 2026-11-22 to L12's question 3, L10's
    /// question 5): where an end left as it is would have its bolts drawn
    /// joining nothing ([`LayoutSettings::nothing_joined`]) and a plate of
    /// the plan lies on that member's flange where its bolts are (nothing
    /// runs through the joint: an arm tip, a K-brace's apex), its end is
    /// bolted onto that plate and its own end bolts are the plate's holding
    /// bolts (the plate's `held_by`, structure file version 24; the plate's
    /// own provisional bolts onto it, if planned, become its end bolts),
    /// applied as a plan is written (`joint_layout::hold_by_base`). On by default; off:
    /// as before (that member by hand).
    pub base_holds: bool,
    /// L15, 2026-11-22 (L14's item 3, J6's rest): an end onto a built-up
    /// member whose leg bolted lies across a gap between two of its angles
    /// (its outstanding leg pointing out of one angle's back, across the
    /// gap — L13's seat by the plane its axis lies in) goes onto a plate
    /// **between** those angles: on the back of the one, as thick as the gap
    /// (the section's), the end on its far face clear of the other angle's
    /// steel, the plate's own provisional bolts through both angles
    /// (`BETWEEN_LEAST`; the plate's `between`, structure file version 25).
    /// Kept at a joint only where it lays out more ends, or as many on fewer
    /// plates, than without. On by default; off: as before (such an end
    /// planned on the angle facing it).
    pub between_plates: bool,
    /// L16, 2026-11-22 (L15's question 4): an end is planned on the face
    /// of the leg of what the layout lays it onto that the joint details
    /// then seat it on (`joints::seat_face`: the face given at
    /// the end, else the one turned towards its outer face, on a built-up
    /// member the angle facing it) — before, the face it sat on what it
    /// bolted onto while planning (for an end naming nothing, the joint's
    /// leg worked out: on a built-up member often the other face, the end
    /// planned on the outer face and drawn on the inner). On by default;
    /// off: as before.
    pub seat_as_drawn: bool,
    /// O44 L18 (the owner, 2026-11-22: "if there are multiple planes …
    /// then we should go from high to low"): a joint's planes on members of
    /// one rank laid out from the plane of the biggest member down (its
    /// class, then its wider leg; the structure file stores no analysis
    /// forces, so size alone), before the member's id decided between them.
    /// Off by default: measured on six private real towers, fewer joints
    /// whole and more plates than the planes in the members' order (−4
    /// whole after the outline's rolls, −15 after the joints' planes').
    pub planes_high_to_low: bool,
    /// O44 L19 (the owner, 2026-11-22: "when turning a member both sides
    /// should be reverified"): the turns Orient angles set from the joints'
    /// planes and the layout record lists as not weighed yet
    /// ([`crate::layout_run::LayoutRecord::plane_turns`]) weighed by the
    /// run: each run's turn kept only where every joint its members reach
    /// is laid out no worse and one better, else its roll taken back and
    /// the turn offered ([`crate::weighed_turns`]). On by default; off: the
    /// rolls as written (L18).
    pub weigh_turns: bool,
    /// O44 L20 (L19's question 6, the owner's "go ahead to improve" of
    /// 2026-11-23): a **level plate across two horizontals** — a plan or
    /// hip end lying on no flange of what today's rule names for it, at a
    /// joint where two other level single angles ending there (not in line)
    /// have their level legs in one plane with its axis
    /// (`joint_layout::level_pair`), is named onto the first ([`hosts_to_name_by`];
    /// weighed joint by joint as L13's names); a flat plate the plan lays on
    /// that member's level leg (held by its own bolts onto it, L5o) is also
    /// given its own provisional bolts onto the second where they fit (the
    /// plate's `[plate.also_bolts]`, structure file version 26): one plate
    /// held by both. On by default; off: as before.
    pub level_plates: bool,
    /// O44 L20 item 2 (L19's question 7a): an end on a plate in its own
    /// plane welded to the flat plate (step 4) whose axis that plate puts
    /// more than a plate's thickness farther out of the flange is laid out
    /// up to `joint_layout::OWN_OUT_MAX` — the offset recorded with the plate (the
    /// record's `layout_run::OWN_OUT` mark) and carried into the
    /// analysis by the estimated eccentricities (O45; an option, off by
    /// default: off, or at level 4, the analysis says NOT ON THE SAFE SIDE
    /// for those ends — their moments missing, so those members' usage can
    /// be too low; a hand-written eccentricity takes their place). On by
    /// default; off: as before (a hand detail beyond
    /// a plate's thickness).
    pub own_offset_analysed: bool,
    /// O44 L22 (L21's question 4): a plate's own holding bolts (L5o,
    /// `[plate.leg_bolts]`) placed clear of what the plan lays out at the
    /// joint after that plate — the other plates and ends, their bolts'
    /// heads and nuts (those on the other flange of the member it lies on,
    /// in its corner, among them) and the other plates' own holding bolts:
    /// where they would meet such a piece, the joint is planned again with
    /// them kept clear of it, kept only where fewer meet something and
    /// nothing else is laid out worse. On by default; off: as before.
    pub holds_clear: bool,
}

/// The least gap between a built-up member's angles a plate between them
/// is laid out in, in thicknesses of the thickest leg on the plate (L15):
/// **the program's value, not a code's** — the plate is as thick as the
/// gap (the section's; fitted to its properties on imported sections), and
/// the joint checks check it so (its bearing under every bolt, its edges,
/// its own sections).
pub const BETWEEN_LEAST: f64 = 0.5;

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
            bottom_up: true,
            more_plates: true,
            onto_search: true,
            plate_on: true,
            exact_plan: true,
            clash_tries: true,
            direct_first: false,
            short_plates: true,
            slide_holds: true,
            small_bends: true,
            bend_facing: true,
            bend_back: true,
            direct_moved: true,
            hold_plates: true,
            built_up_plates: true,
            as_drawn: true,
            partner_bolts: true,
            reach_clear: true,
            short_holds: true,
            other_face: true,
            face_pattern: true,
            nothing_joined: true,
            plate_shift: false,
            both_faces: true,
            chord_turns: false,
            plane_hosts: true,
            base_holds: true,
            between_plates: true,
            seat_as_drawn: true,
            planes_high_to_low: false,
            weigh_turns: true,
            level_plates: true,
            own_offset_analysed: true,
            holds_clear: true,
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
    /// flange, J2b; since L5o, 2026-11-20, a flat plate that nothing of the
    /// joint's legs bolts onto: `[plate.leg_bolts]`); none: held by its
    /// members' bolts (how, the joint checks say: not checked where none of
    /// them is a leg's).
    pub hold: Option<HoldPlan>,
    /// Its outline stops short of the work point (L2's follow-up,
    /// 2026-11-20; the record's `short` mark): round its members' legs from
    /// the flange's toe over their bolts and the flange under them — its
    /// outline round the work point would run into something.
    pub short: bool,
    /// Straight cuts of that short outline, each short of what it would
    /// still run into (L2's follow-up a): a point of the cut (structure
    /// axes, m) and, in the plate's plane, the normal towards the part cut
    /// away (unit). Applied where the run writes the outline.
    pub cuts: Vec<([f64; 3], [f64; 3])>,
    /// Between the angles of the built-up member it is on (L15, the
    /// record's `between`): on the back of one of them, as thick as the gap.
    pub between: bool,
    /// L20: its own bolts onto a second member it lies on (a level plate
    /// across two horizontals: `hold` onto the one it is `on`, these onto
    /// the other; written as `[plate.also_bolts]`); none: as before.
    pub also: Option<HoldPlan>,
}

/// A plate's own bolts onto the member whose flange it lies on (J2b,
/// 2026-11-19: a bent plate's flat on that flange, so that the flat is
/// bolted to it as the owner asked; L5o, 2026-11-20: a flat plate on a
/// member running on through the joint; written as `[plate.leg_bolts]`).
#[derive(Debug, Clone, PartialEq)]
pub struct HoldPlan {
    /// The member.
    pub member: String,
    /// Its leg the plate lies on: `long` or `short` (L5o's always named —
    /// an equal-leg angle's `long` its leg along the section's y, as the
    /// joint details read it; none: worked out from the plate's face).
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
    /// the owner's step 1c, 2026-11-19). **No longer made by
    /// [`joins_of`]** since O44 J7 (2026-11-19: never written silently —
    /// proposed by [`crate::leg_splices::leg_splices`], written when
    /// confirmed); a run still takes out such a splice an older run made.
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
    /// L8 (L7's proposal 4): the bolts of the members left as they are that
    /// bolt onto this end (2D, as `obstacles`), each with its hole's
    /// diameter and the end distance it keeps in this end's steel, m —
    /// kept on its steel ([`partner_misfit`]), not kept clear of.
    partners: Vec<([f64; 2], f64, f64)>,
    /// Nothing runs on into it as one part at the joint: its steel starts
    /// at its cut (the partners' bolts past it by their end distance).
    own_ends: bool,
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
    /// L2 (b, 2026-11-20): the fixed steel as high as its outstanding leg
    /// reaches (over the band of that leg, `out_band`), and behind the face
    /// where its bolts' nuts and shanks reach (over each bolt's disc).
    fixed_out: Vec<(String, Vec<[f64; 2]>)>,
    fixed_far: Vec<(String, Vec<[f64; 2]>)>,
    /// Its outstanding leg across it now (lo ≤ hi), m.
    out_band: [f64; 2],
    /// The edge and end distances its bolts keep in a plate of its leg's
    /// thickness (the code's, else the default minimums), m.
    keep_plate: (f64, f64),
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
    /// What it bolts onto ends at the joint (not one part through it,
    /// `continuous`): its steel there starts at its cut (J9 and D16,
    /// 2026-11-19; pieces in line not confirmed one part end at the joint,
    /// on the safe side, as in the joint checks).
    host_ends: bool,
    /// Where that steel starts as the joint is given (the cut given or
    /// worked out; not known: the work point), m along it.
    host_cut: f64,
    /// Along what it bolts onto from the work point, per metre of the
    /// plane's x.
    host_along: f64,
    /// The end distance a bolt of it keeps in that steel, m.
    host_end: f64,
    /// On the outer face of a spliced member's flange where the splice's
    /// outer cover lies (O44 J7, 2026-11-19): the splice. The strip lies
    /// on the cover (its face moved out by the cover's thickness), its
    /// bolts through it; directly only — a plate over the cover is not
    /// laid out (by hand).
    through: Option<String>,
    /// L15: across a gap between the angles of the built-up member it bolts
    /// onto — its face the back of one of them, onto a plate between them
    /// as thick as the gap (the gap, m; whether the other angle's leg lies
    /// straight across it, so that a bolt through the plate passes through
    /// both).
    between: Option<(f64, bool)>,
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
///
/// (J2b fix, 2026-11-20.) Its own bolts lie on the steel of the member they
/// bolt onto: where that member ends at the joint, past where its steel
/// starts (its cut as laid out in this plan, `laid_cut`, else as given or
/// worked out, else the work point) by the end distance — the pair moved
/// out along it where needed, the flat with them. Where that member's own
/// end is laid out here, the plate is laid out where that member will be
/// drawn (moved across and onto its plate as laid out), so that the flat's
/// outline and its bolts (drawn from that member) agree.
#[allow(clippy::too_many_arguments)]
fn bent_over(
    strips: &[Strip],
    group: &[Placed],
    j: &JointDetail,
    s: &LayoutSettings,
    laid: &[Placed],
    laid_cut: &BTreeMap<String, f64>,
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
    // (J2b fix) The member it bolts onto as it will be drawn: where its own
    // end is laid out in this plan, moved across by its move and lifted onto
    // its plate (structure axes, m; with L2's clash-aware planning the
    // plane is taken there already); on a plate in its own plane (step 4):
    // not followed, left.
    let mut moved3 = [0.0; 3];
    if let Some(hk) = strips.iter().position(|y| y.member == first.host)
        && let Some(hp) = laid.iter().find(|q| q.k == hk)
    {
        let h = &strips[hk];
        if h.tilted.is_some() {
            let e = format!(
                "{} is laid out on a welded plate in its own plane here (its place for the bent \
                 plate not followed)",
                first.host
            );
            return Err(group.iter().map(|p| (p.k, why(p.k, &e))).collect());
        }
        if !s.exact_plan {
            let v3 = add(scale(h.x3, h.v[0]), scale(h.y3, h.v[1]));
            let lift = match plate_of.get(&hk) {
                Some(&i) if plates[i].own.is_none() => plates[i].thickness,
                _ => 0.0,
            };
            moved3 = add(scale(v3, hp.moved), scale(h.n3, lift));
        }
    }
    // Its other leg's outer face moved across this plane's y by the member's
    // move: the ends pulled back by as much more.
    let dy = dot(moved3, first.y3);
    let mut placed: Vec<Placed> = Vec::new();
    let mut left: Vec<(usize, String)> = Vec::new();
    for p in group {
        let x = &strips[p.k];
        let keep = x.over.as_ref().map_or(o.keep, |v| v.keep);
        let (need, _) = over_cut(x, p.moved, over_steel(tp, s.gap) - dy, tp + r + keep.0 - dy);
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
    let mut bx = [xc - spacing_min / 2.0, xc + spacing_min / 2.0];
    // (J2b fix) On that member's steel where it ends at the joint: past
    // where it starts by the end distance (its cut as laid out here, else
    // as given or worked out, else the work point), the pair moved out
    // along it where needed. *Conservative*: a cut not known is taken at the
    // work point, the end distance the code's (else the default minimum).
    let wp_x = first.w[0];
    if first.host_ends {
        let from = laid_cut.get(&first.host).copied().unwrap_or(first.host_cut);
        let need = from + first.host_end;
        let at = |x: f64| (x - wp_x) * o.along_host;
        let least = at(bx[0]).min(at(bx[1]));
        if least < need - 1e-9 {
            let d = (need - least) * o.along_host.signum();
            bx = [bx[0] + d, bx[1] + d];
        }
    }
    // 3D from the work point: the plane's heel, along, across, down the
    // other leg (into the angle) — the heel where the member it bolts onto
    // will be drawn.
    let (heel, x3, y3, down) = (add(first.heel3, moved3), first.x3, first.y3, o.down);
    // (L2's follow-up, 2026-11-20) Clear of the holes already in that leg:
    // the member's own end there and the ends bolted onto it — as laid out
    // in this plan, else as they are —, the pair slid along it (the least
    // slide first, either way, away from the work point on a tie, at most
    // the longest pull-back) until each of its bolts keeps the default
    // minimum spacing from every such hole (*on the safe side*: the joint
    // checks' rule is only that holes do not overlap). Before, the first
    // holding bolt often fell on the member's own first bolt, and the run
    // left the plate after writing.
    if s.slide_holds {
        let holes = leg_holes(strips, laid, group, j, first, heel, (y3, down), t_a + tp);
        let at = |x: f64| (x - wp_x) * o.along_host;
        let need = first
            .host_ends
            .then(|| laid_cut.get(&first.host).copied().unwrap_or(first.host_cut) + first.host_end);
        let clear = |b: [f64; 2]| {
            need.is_none_or(|n| at(b[0]).min(at(b[1])) >= n - 1e-9)
                && holds_clear(b, gauge, &holes, spacing_min)
        };
        match slide_holds(bx, o.along_host.signum(), s.pull_back, clear) {
            Some(b) => bx = b,
            None => {
                return Err(all(
                    &format!(
                        "no place for its own bolts on {}'s other leg clear of the holes there \
                         within {:.0} mm",
                        first.host,
                        s.pull_back * 1000.0
                    ),
                    left,
                    &placed,
                ));
            }
        }
    }
    let flo = lo.min(bx[0] - end_min);
    let fhi = hi.max(bx[1] + end_min);
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
            let p3 = add(
                add(add(b.heel3, moved3), scale(b.x3, p2[0])),
                scale(b.y3, p2[1]),
            );
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
        short: false,
        cuts: Vec::new(),
        between: false,
        also: None,
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
        short: false,
        cuts: Vec::new(),
        between: false,
        also: None,
    };
    Ok(BentOver {
        flat,
        into,
        bent,
        placed,
        left,
    })
}

/// Whether a bent plate's holding bolts at `b` (along the leg, m) on the
/// gauge line `gauge` (down the leg from its heel, m) keep `spacing` (the
/// default minimum, m) from each of `holes` (along, down; [`leg_holes`])
/// — L2's follow-up (b), 2026-11-20 (*on the safe side*: the joint checks'
/// rule is only that holes do not overlap).
fn holds_clear(b: [f64; 2], gauge: f64, holes: &[[f64; 2]], spacing: f64) -> bool {
    b.iter().all(|x| {
        holes
            .iter()
            .all(|h| (h[0] - x).hypot(h[1] - gauge) >= spacing - 1e-9)
    })
}

/// A bent plate's two holding bolts at `bx` (along the leg, m) slid along
/// it until `clear` holds — L2's follow-up (b), 2026-11-20: as they are
/// where it holds already; else the least slide, in millimetre steps up to
/// `reach` (the longest pull-back), either way — `out` (the sign of away
/// from the work point along the leg) first on a tie. None: no place within
/// `reach`. Deterministic.
fn slide_holds(
    bx: [f64; 2],
    out: f64,
    reach: f64,
    clear: impl Fn([f64; 2]) -> bool,
) -> Option<[f64; 2]> {
    if clear(bx) {
        return Some(bx);
    }
    let steps = (reach * 1000.0).round() as i64;
    (1..=steps).find_map(|k| {
        [out, -out].into_iter().find_map(|w| {
            let d = w * k as f64 * 0.001;
            let b = [bx[0] + d, bx[1] + d];
            clear(b).then_some(b)
        })
    })
}

/// The holes through the other leg of the member `first` bolts onto (J2b's
/// bent plate's flat lies on that leg; L2's follow-up, 2026-11-20), in that
/// leg's axes from `heel` (along the plane's x, down the leg): the member's
/// own end and the ends bolted onto it — those laid out in this plan
/// (`laid`, not `group`'s) where they will be, the others where they are.
/// `axes`: the leg's normal (the plane's across) and down it; a hole counts
/// within `depth` of the leg's outer face (its thickness and the plate's).
#[allow(clippy::too_many_arguments)]
fn leg_holes(
    strips: &[Strip],
    laid: &[Placed],
    group: &[Placed],
    j: &JointDetail,
    first: &Strip,
    heel: [f64; 3],
    axes: ([f64; 3], [f64; 3]),
    depth: f64,
) -> Vec<[f64; 2]> {
    let (normal, down) = axes;
    let host = first.host.as_str();
    let mut out = Vec::new();
    let mut take = |q: [f64; 3]| {
        let d = sub(q, heel);
        if dot(d, normal).abs() <= depth + 0.005 {
            out.push([dot(d, first.x3), dot(d, down)]);
        }
    };
    let mut planned: BTreeSet<&str> = BTreeSet::new();
    for q in laid.iter().filter(|q| !group.iter().any(|g| g.k == q.k)) {
        let b = &strips[q.k];
        planned.insert(b.member.as_str());
        if b.tilted.is_some() || (b.host != host && b.member != host) {
            continue;
        }
        let off = dot(sub(b.face3, b.heel3), b.n3);
        for (along, line) in &b.bolts {
            let l = b.gauges.get(*line).copied().unwrap_or(0.0) + q.moved;
            let p2 = b.at(q.cut + along, l);
            take(add(
                add(add(b.heel3, scale(b.x3, p2[0])), scale(b.y3, p2[1])),
                scale(b.n3, off),
            ));
        }
    }
    for g in group {
        planned.insert(strips[g.k].member.as_str());
    }
    for a in &j.arrivals {
        if planned.contains(a.member.as_str())
            || (a.member != host && a.onto.value.as_deref() != Some(host))
        {
            continue;
        }
        for q in &a.bolt_points {
            take(sub(*q, j.position));
        }
    }
    out
}

/// What [`plate_holds`] keeps clear of on the flange a flat plate lies on
/// (L5o, 2026-11-20), in that plane's axes (m).
struct HoldRoom<'a> {
    /// The holes through the flange and the bolts on the plate (centres).
    holes: Vec<[f64; 2]>,
    /// The ends lying on this face (on the plate or on the flange) as
    /// placed: their legs from their cut on.
    ends: Vec<&'a Placed>,
    /// The seams of the plates bent or welded from it: a point of each and
    /// square to it towards the flat plate (unit).
    seams: Vec<([f64; 2], [f64; 2])>,
    /// The plate's outline as it will be drawn (convex).
    poly: &'a [[f64; 2]],
    /// (L15) The members its bolts pass through (a plate between a built-up
    /// member's angles: that member and those in line with it), not in
    /// their way.
    through: BTreeSet<String>,
    /// (L22) The footprints (in the flange's plane, within the slab its
    /// bolts fill: [`hold_slab`]) of what [`HoldAt::avoid`] asks them to
    /// keep clear of.
    avoid: Vec<Vec<[f64; 2]>>,
}

/// L5o (2026-11-20, the owner's decision of that day): the places of a
/// flat plate's own provisional bolts onto the member whose flange it lies
/// on, where nothing of the joint's legs bolts onto the plate — that member
/// running on through the joint, or an end of it not on the plate (before:
/// how the plate is held "not checked"). Up to `n` bolts (at least two),
/// one gauge line along the member, the default minimum spacing apart;
/// each within the flange (clear of its heel's leg and root by the nut,
/// its edge distance from the toe), on that member's steel past where it
/// starts (`from`) by the end distance where it ends at the joint, inside
/// the plate's outline by the plate's edge and end distances, the larger
/// all round (the plate drawn round them stays as planned), the default
/// minimum spacing from every hole through the flange and every bolt on
/// the plate (*on the safe side*: the joint checks' rule is only that
/// holes do not overlap), its head clear of the members' legs on the face,
/// of the seams of the plates bent or welded from it and of what is fixed
/// over the face, its nut of what is fixed behind the flange, each by half
/// the gap. Fewer than `n` (not fewer than two) where more do not fit.
/// The places nearest the work point first, the gauge line in the band's
/// middle first; deterministic. An error in words where two do not fit.
fn plate_holds(
    strips: &[Strip],
    x0: &Strip,
    room: &HoldRoom<'_>,
    s: &LayoutSettings,
    from: Option<f64>,
    n: usize,
    ring_min: f64,
) -> Result<Vec<[f64; 2]>, String> {
    let nut = x0.nut;
    let [_, spacing, _] = s.defaults.of(x0.d);
    let (lo_y, hi_y) = (x0.t + x0.r + nut, x0.width - x0.edge);
    if lo_y > hi_y + 1e-9 {
        return Err(format!(
            "no room on {}'s flange ({:.0} mm wide; a bolt needs {:.0} mm from its heel and {:.0} \
             mm from its toe)",
            x0.host,
            x0.width * 1000.0,
            lo_y * 1000.0,
            x0.edge * 1000.0
        ));
    }
    let ring = room
        .ends
        .iter()
        .filter(|p| strips[p.k].plane == x0.plane)
        .map(|p| strips[p.k].keep_plate.0.max(strips[p.k].keep_plate.1))
        .fold(x0.keep_plate.0.max(x0.keep_plate.1), f64::max)
        .max(ring_min);
    let head = nut + s.gap / 2.0;
    let poly = room.poly;
    let sgn = (0..poly.len())
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            a[0] * b[1] - a[1] * b[0]
        })
        .sum::<f64>()
        .signum();
    let inside = |q: [f64; 2]| -> bool {
        poly.len() >= 3
            && (0..poly.len()).all(|i| {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                let e = sub2(b, a);
                let l = dot2(e, e).sqrt();
                l < 1e-12 || sgn * (e[0] * (q[1] - a[1]) - e[1] * (q[0] - a[0])) / l >= ring - 1e-9
            })
    };
    // Clear of a convex footprint by `r`: the disc's square apart from it
    // (*on the safe side*).
    let off_poly = |q: [f64; 2], p: &[[f64; 2]], r: f64| -> bool {
        let sq = [
            [q[0] - r, q[1] - r],
            [q[0] + r, q[1] - r],
            [q[0] + r, q[1] + r],
            [q[0] - r, q[1] + r],
        ];
        sat2(&sq, p) <= 0.0
    };
    // (A seam: on the flat plate's side of it — the run cuts the plate
    // there —, its plate's thickness and bend beside it.)
    let seam_keep = (head + bend_radius(x0.own_t) + x0.own_t).max(ring);
    let ok = |q: [f64; 2]| -> bool {
        if !(lo_y - 1e-9..=hi_y + 1e-9).contains(&q[1]) || !inside(q) {
            return false;
        }
        if from.is_some_and(|f| (q[0] - x0.w[0]) * x0.host_along < f + x0.host_end - 1e-9) {
            return false;
        }
        if room
            .holes
            .iter()
            .any(|h| (h[0] - q[0]).hypot(h[1] - q[1]) < spacing - 1e-9)
            || x0
                .obstacles
                .iter()
                .any(|(c, r)| (c[0] - q[0]).hypot(c[1] - q[1]) < spacing.max(r + head) - 1e-9)
        {
            return false;
        }
        for p in &room.ends {
            let x = &strips[p.k];
            let rel = sub2(q, x.w);
            let (al, ac) = (dot2(rel, x.u), dot2(rel, x.v));
            if al > p.cut - head && ac > x.lat[0] + p.moved - head && ac < x.lat[1] + p.moved + head
            {
                return false;
            }
        }
        room.seams
            .iter()
            .all(|(at, side)| dot2(sub2(q, *at), *side) >= seam_keep)
            && x0
                .fixed
                .iter()
                .filter(|(m, _)| !room.through.contains(m))
                .all(|(_, p)| off_poly(q, p, head))
            && x0.fixed_far.iter().all(|(_, p)| off_poly(q, p, head))
            && room.avoid.iter().all(|p| off_poly(q, p, head))
    };
    let (x_lo, x_hi) = poly
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), q| {
            (lo.min(q[0]), hi.max(q[0]))
        });
    if !x_lo.is_finite() {
        return Err("the plate has no outline to place them in".to_owned());
    }
    // Millimetre pairs: the program's search step.
    const STEP: f64 = 0.002;
    let mid = (lo_y + hi_y) / 2.0;
    let band = ((hi_y - lo_y) / STEP).floor().max(0.0) as i64;
    let gauges: Vec<f64> = (0..=band)
        .map(|k| {
            let dd = ((k + 1) / 2) as f64 * STEP;
            if k % 2 == 1 { mid - dd } else { mid + dd }
        })
        .filter(|g| (lo_y - 1e-9..=hi_y + 1e-9).contains(g))
        .collect();
    let mut last = format!("no place for two on {}'s flange", x0.host);
    for m in (2..=n.max(2)).rev() {
        let span = (m - 1) as f64 * spacing;
        if span > x_hi - x_lo {
            continue;
        }
        // (The group's middle a whole number of steps from the work point:
        // the same places at a joint turned or mirrored.)
        let (k_lo, k_hi) = (
            ((x_lo + span / 2.0 - x0.w[0]) / STEP - 1e-9).ceil() as i64,
            ((x_hi - span / 2.0 - x0.w[0]) / STEP + 1e-9).floor() as i64,
        );
        let mut starts: Vec<f64> = (k_lo..=k_hi)
            .map(|k| x0.w[0] + k as f64 * STEP - span / 2.0)
            .collect();
        // Nearest the work point first; of two as near, the one along the
        // member (its direction from the joint: the same choice at a joint
        // turned or mirrored).
        let key = |a: &f64| {
            let c = (a + span / 2.0 - x0.w[0]) * x0.host_along;
            ((c.abs() * 1e6).round() as i64, -((c * 1e6).round() as i64))
        };
        starts.sort_by_key(key);
        for &g in &gauges {
            for &x in &starts {
                let pts: Vec<[f64; 2]> = (0..m).map(|i| [x + i as f64 * spacing, g]).collect();
                if pts.iter().all(|q| ok(*q)) {
                    return Ok(pts);
                }
            }
        }
        last = format!(
            "no place for {m} on {}'s flange inside the plate, clear of the holes there, the \
             members' legs and the steel in the way",
            x0.host
        );
    }
    Err(last)
}

/// Where a flat plate's holding bolts go (L5o): its outline stopping short
/// of the work point (none: round it), the seams of the plates bent or
/// welded from it (a point of each, 3D from the work point, and square to
/// it towards the flat plate), the cuts laid out so far.
struct HoldAt<'a> {
    poly: Option<&'a [[f64; 2]]>,
    seams3: &'a [([f64; 3], [f64; 3])],
    laid_cut: &'a BTreeMap<String, f64>,
    /// (L9) What a plate's outline (in its plane's axes) would run into
    /// (none: clear) — a plate stopping short of the work point checked
    /// with its outline grown round its holding bolts.
    clear: &'a dyn Fn(&[[f64; 2]]) -> Option<String>,
    /// (L9) The edge and end distances the holding bolts keep in the plate
    /// (the code's for the member it lies on, in the plate's thickness;
    /// none: not known).
    host_keep: Option<(f64, f64)>,
    /// (L22) What the plan laid out at this joint after this plate (or
    /// beside it) that its bolts' heads, shanks and nuts are kept clear of
    /// (3D corners from the work point; [`hold_conflicts`] found them in
    /// the way of the places chosen without them); empty: none asked.
    avoid: &'a [Vec<[f64; 3]>],
}

/// L5o (2026-11-20): flat plate `flat` of the ends `placed` given its own
/// provisional bolts onto the member it lies on ([`plate_holds`]) where
/// none of the ends on it is one of the joint's legs and that member's own
/// end is not on it — said in `notes`, with why where they do not fit
/// (the plate then laid out without them: how it is held not checked).
#[allow(clippy::too_many_arguments)]
fn hold_flat(
    flat: &mut PlatePlan,
    notes: &mut Vec<String>,
    strips: &[Strip],
    placed: &[Placed],
    placed_all: &[Placed],
    j: &JointDetail,
    s: &LayoutSettings,
    at: HoldAt<'_>,
) -> Option<(usize, Vec<[f64; 2]>)> {
    let flats: Vec<&Placed> = placed
        .iter()
        .filter(|p| strips[p.k].tilted.is_none())
        .collect();
    let k0 = flats.first().map(|p| p.k)?;
    let x0 = &strips[k0];
    let legs: Vec<&str> = crate::welds::joint_legs(j)
        .iter()
        .map(|a| a.member.as_str())
        .collect();
    if flat.hold.is_some()
        || flat
            .members
            .iter()
            .any(|m| legs.contains(&m.as_str()) || *m == x0.host)
    {
        return None;
    }
    // A plate stopping short of the work point (L2's follow-up a): drawn
    // round its members' legs from the flange's toe, cut straight short of
    // what it runs into — its own bolts would change that outline (measured
    // on the private real towers: their members then left after writing).
    // L9 (2026-11-21, the owner's answer to L6's question 2): since then
    // given them where its outline, grown round them as `joints` draws it,
    // still runs into nothing (below); off: as before.
    if flat.short && !s.short_holds {
        notes.push(format!(
            "a plate on {} stopping short of the work point: no provisional bolts of its own \
             onto {} (its outline would change); how it is held not checked (lay them out by \
             hand)",
            x0.host, x0.host
        ));
        return None;
    }
    // (J6) A built-up member: its plate's own bolts onto one of its angles
    // are not read yet (`[plate.leg_bolts]` names a single angle's leg) —
    // but (L15) a plate between its angles: through the back it lies on
    // (and the angle across the gap).
    if !flat.between
        && j.arrivals
            .iter()
            .find(|a| a.member == x0.host)
            .and_then(|h| h.profile.as_ref())
            .is_some_and(|q| q.angle.is_none())
    {
        notes.push(format!(
            "a plate on built-up {}: no provisional bolts of its own onto it (bolts onto one of \
             its angles not given yet); how it is held not checked (lay them out by hand)",
            x0.host
        ));
        return None;
    }
    let round;
    let poly = match at.poly {
        Some(p) => p,
        None => {
            round = plate_outline2(strips, placed, s.gap, Outline::Round);
            &round
        }
    };
    // (L15) Between a built-up member's angles: the plate may grow along the
    // member within the gap — its holding bolts placed on the flange beyond
    // its outline too (as many as asked, the default spacing apart, their
    // distances kept), the outline grown round them checked clear below.
    let keep_ring = placed
        .iter()
        .filter(|p| strips[p.k].tilted.is_none())
        .map(|p| strips[p.k].keep_plate.0.max(strips[p.k].keep_plate.1))
        .fold(x0.keep_plate.0.max(x0.keep_plate.1), f64::max);
    let planned = poly;
    let grown_room;
    // (L20) A level plate (an end on it whose level plate's first holder is
    // this member): grown along it as a plate between is, so that its
    // holding bolts find room on the flange beyond the ends' legs.
    let level = s.level_plates
        && j.arrivals
            .iter()
            .find(|h| h.member == x0.host)
            .is_some_and(|h| {
                flat.members.iter().any(|m| {
                    j.arrivals
                        .iter()
                        .find(|a| &a.member == m)
                        .and_then(|a| level_partner(&j.arrivals, a, h))
                        .is_some()
                })
            });
    let grows = flat.between || level;
    let poly = if grows && poly.len() >= 3 {
        let [_, spacing, _] = s.defaults.of(x0.d);
        let reach = 4.0 * spacing + 2.0 * keep_ring;
        let (lo, hi) = poly
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), q| {
                (lo.min(q[0]), hi.max(q[0]))
            });
        let ylo = poly.iter().map(|q| q[1]).fold(f64::INFINITY, f64::min);
        let mut pts = poly.to_vec();
        for xq in [lo - reach, hi + reach] {
            pts.push([xq, ylo]);
            pts.push([xq, x0.width]);
        }
        grown_room = crate::joint_sheet::hull(pts);
        &grown_room
    } else {
        poly
    };
    let to2 = |q: [f64; 3]| {
        let d = sub(q, x0.heel3);
        [dot(d, x0.x3), dot(d, x0.y3)]
    };
    let mut holes = leg_holes(
        strips,
        placed_all,
        placed,
        j,
        x0,
        x0.heel3,
        (x0.n3, x0.y3),
        x0.t + flat.thickness,
    );
    for p in &flats {
        let x = &strips[p.k];
        for (along, line) in &x.bolts {
            let l = x.gauges.get(*line).copied().unwrap_or(0.0) + p.moved;
            holes.push(x.at(p.cut + along, l));
        }
    }
    let ends: Vec<&Placed> = placed_all
        .iter()
        .filter(|p| strips[p.k].plane == x0.plane && strips[p.k].tilted.is_none())
        .chain(flats.iter().copied())
        .collect();
    let room = HoldRoom {
        holes,
        ends,
        seams: at
            .seams3
            .iter()
            .map(|(p, side)| {
                let n = [dot(*side, x0.x3), dot(*side, x0.y3)];
                let l = dot2(n, n).sqrt().max(1e-12);
                (to2(*p), [n[0] / l, n[1] / l])
            })
            .collect(),
        poly,
        through: if flat.between {
            let host = j.arrivals.iter().find(|a| a.member == x0.host);
            j.arrivals
                .iter()
                .filter(|b| {
                    b.member == x0.host
                        || host.is_some_and(|h| angle_deg(h.direction, b.direction) > 160.0)
                })
                .map(|b| b.member.clone())
                .collect()
        } else {
            BTreeSet::new()
        },
        avoid: {
            let slab = hold_slab(x0, flat.thickness);
            at.avoid
                .iter()
                .filter_map(|p| footprint_in(x0, p, slab))
                .collect()
        },
    };
    let mut room = room;
    let from = x0
        .host_ends
        .then(|| at.laid_cut.get(&x0.host).copied().unwrap_or(x0.host_cut));
    let n = flats
        .iter()
        .map(|p| strips[p.k].bolts.len())
        .max()
        .unwrap_or(2)
        .max(2);
    let d = x0.d;
    // (L9) A plate stopping short of the work point, cut straight: its
    // holding bolts kept from its edge by the larger of their own end and
    // edge distances in it too (the code's for the member it lies on, as
    // the joint checks take them): its outline does not grow round them
    // past its cuts.
    let ring_min = if flat.short {
        at.host_keep.map_or(0.0, |(e, d)| e.max(d))
    } else {
        0.0
    };
    // (L22) Clear of what the plan laid out after it at this joint where
    // asked; where no place is, as before (said; the check after writing
    // tests them).
    let mut clear_words = String::new();
    let first = if room.avoid.is_empty() {
        None
    } else {
        match plate_holds(strips, x0, &room, s, from, n, ring_min) {
            Ok(p) => {
                clear_words = "; placed clear of the plates, ends and bolts laid out after it \
                               at this joint"
                    .to_owned();
                Some(Ok(p))
            }
            Err(e) => {
                clear_words = format!(
                    "; not clear of the plates, ends and bolts laid out after it at this joint \
                     ({e}), placed as before — the check after writing tests them"
                );
                room.avoid.clear();
                None
            }
        }
    };
    let pts = match first.unwrap_or_else(|| plate_holds(strips, x0, &room, s, from, n, ring_min)) {
        Ok(p) => p,
        Err(e) => {
            notes.push(format!(
                "a plate on {}: no provisional bolts of its own onto {} — {e}; how it is held \
                 not checked (lay them out by hand)",
                x0.host, x0.host
            ));
            return None;
        }
    };
    // (L15) Between the angles, grown along the member round them: its
    // outline (each bolt with its distances in it) clear of what it could
    // run into; else none (as before), said. (L20: a level plate alike.)
    if grows {
        const N: usize = 16;
        let grow = 1.0 / (std::f64::consts::PI / N as f64).cos();
        let mut all = planned.to_vec();
        for q in &pts {
            for i in 0..N {
                let a = i as f64 * std::f64::consts::TAU / N as f64;
                all.push([
                    q[0] + keep_ring * grow * a.cos(),
                    q[1] + keep_ring * grow * a.sin(),
                ]);
            }
        }
        let grown = crate::joint_sheet::hull(all);
        if let Some(by) = (at.clear)(&grown) {
            let what = if flat.between {
                format!("a plate between {}'s angles", x0.host)
            } else {
                format!("a level plate on {}", x0.host)
            };
            notes.push(format!(
                "{what}: no provisional bolts of its own onto {} (round them its outline would \
                 run into {by}); how it is held not checked (lay them out by hand)",
                x0.host
            ));
            return None;
        }
    }
    // (L9) A plate stopping short of the work point: its outline as
    // `joints` will draw it with them — round its members' legs from the
    // flange's toe and their bolts, each holding bolt with its distances,
    // the flange under them as far along as those reach, cut at the heel
    // and by its straight cuts — clear of what it could run into; else none
    // (as before), said.
    if flat.short {
        let grown = short_round_holds(
            strips,
            placed,
            (s.gap, ring_min),
            x0,
            &pts,
            flat,
            j.position,
        );
        if let Some(by) = (at.clear)(&grown) {
            notes.push(format!(
                "a plate on {} stopping short of the work point: no provisional bolts of its own \
                 onto {} (round them its outline would run into {by}); how it is held not \
                 checked (lay them out by hand)",
                x0.host, x0.host
            ));
            return None;
        }
        // Each of them inside that outline, cut, by its distances too.
        let keep = ring_min.max(
            placed
                .iter()
                .filter(|p| strips[p.k].tilted.is_none())
                .map(|p| strips[p.k].keep_plate.0.max(strips[p.k].keep_plate.1))
                .fold(x0.keep_plate.0.max(x0.keep_plate.1), f64::max),
        );
        let sgn = (0..grown.len())
            .map(|i| {
                let (a, b) = (grown[i], grown[(i + 1) % grown.len()]);
                a[0] * b[1] - a[1] * b[0]
            })
            .sum::<f64>()
            .signum();
        let deep = |q: [f64; 2]| {
            (0..grown.len())
                .map(|i| {
                    let (a, b) = (grown[i], grown[(i + 1) % grown.len()]);
                    let e = sub2(b, a);
                    let l = dot2(e, e).sqrt();
                    if l < 1e-12 {
                        f64::INFINITY
                    } else {
                        sgn * (e[0] * (q[1] - a[1]) - e[1] * (q[0] - a[0])) / l
                    }
                })
                .fold(f64::INFINITY, f64::min)
        };
        if grown.len() < 3 || pts.iter().any(|q| deep(*q) < keep - 1e-9) {
            notes.push(format!(
                "a plate on {} stopping short of the work point: no provisional bolts of its own \
                 onto {} (cut straight, its outline would not keep their distances); how it is \
                 held not checked (lay them out by hand)",
                x0.host, x0.host
            ));
            return None;
        }
    }
    let host = j.arrivals.iter().find(|a| a.member == x0.host);
    // The flange it lies on named (`long` or `short`; an equal-leg angle:
    // `long` its leg along the section's y, as the joint details read it):
    // never left to a member end's bolts on the plate to tell (one of
    // another face's would put them on the other leg).
    let leg = host
        .and_then(|h| h.profile.as_ref())
        .and_then(|p| p.angle.map(|g| (p.axes, g)))
        .map(|(axes, g)| {
            let along_y = dot(x0.y3, axes[1]).abs() > 0.5;
            if along_y == (g.b >= g.h) {
                "long"
            } else {
                "short"
            }
        });
    let m = pts.len();
    let bolt = match &x0.give {
        Some(b) => BoltArea {
            count: m as u32,
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
    let from_words = format!("{m} bolt(s) of {:.0} mm", d * 1000.0);
    notes.push(format!(
        "{}: {m} provisional bolt(s) holding a plate on it ({from_words}{}{clear_words})",
        x0.host,
        if m < n {
            format!("; {n} asked by its members' bolts, fewer fit — the joint checks check them")
        } else {
            String::new()
        }
    ));
    flat.hold = Some(HoldPlan {
        member: x0.host.clone(),
        leg,
        bolt: BoltArea {
            from: from_words,
            ..bolt
        },
        provisional: true,
        points: pts
            .iter()
            .map(|q| {
                [
                    round_to((q[0] - x0.w[0]) * x0.host_along, 1e-4),
                    round_to(q[1], 1e-4),
                ]
            })
            .collect(),
    });
    Some((k0, pts))
}

/// (L9) The outline (in `x0`'s plane) a flat plate stopping short of the
/// work point will be drawn with when it has its own bolts `holds` onto the
/// member it lies on (as `joints` draws a plate with `[plate.leg_bolts]`):
/// the short outline of its ends ([`plate_outline2`]), each holding bolt
/// with the largest edge or end distance its plane's ends keep in a plate
/// (*an assumption, on the safe side for the fit*: the plate as large as the
/// largest of them asks), the flange under all of it as far along as those
/// reach, cut at the heel as the short outline is and by its straight cuts.
fn short_round_holds(
    strips: &[Strip],
    placed: &[Placed],
    (gap, ring_min): (f64, f64),
    x0: &Strip,
    holds: &[[f64; 2]],
    flat: &PlatePlan,
    wp: [f64; 3],
) -> Vec<[f64; 2]> {
    let flat_now: Vec<Placed> = placed
        .iter()
        .filter(|p| strips[p.k].tilted.is_none())
        .cloned()
        .collect();
    let mut pts = plate_outline2(strips, &flat_now, gap, Outline::Short);
    let ring = flat_now
        .iter()
        .map(|p| strips[p.k].keep_plate.0.max(strips[p.k].keep_plate.1))
        .fold(x0.keep_plate.0.max(x0.keep_plate.1), f64::max)
        .max(ring_min);
    const N: usize = 16;
    let grow = 1.0 / (std::f64::consts::PI / N as f64).cos();
    for q in holds {
        for i in 0..N {
            let a = i as f64 * std::f64::consts::TAU / N as f64;
            pts.push([q[0] + ring * grow * a.cos(), q[1] + ring * grow * a.sin()]);
        }
    }
    // (A flat plate with plates in their own planes welded to it: the seams
    // and the flange under them too, kept to their sides.)
    let to2 = |q: [f64; 3]| {
        let d = sub(sub(q, wp), x0.heel3);
        [dot(d, x0.x3), dot(d, x0.y3)]
    };
    if let Some(rc) = &flat.reach {
        pts.extend(rc.points.iter().map(|q| to2(*q)));
    }
    let lo = pts.iter().map(|q| q[0]).fold(f64::INFINITY, f64::min);
    let hi = pts.iter().map(|q| q[0]).fold(f64::NEG_INFINITY, f64::max);
    for xq in [lo, hi] {
        pts.push([xq, 0.0]);
        pts.push([xq, x0.width]);
    }
    let mut poly = crate::joint_sheet::hull(pts);
    if let Some(rc) = &flat.reach {
        for (at, side) in &rc.keep_to {
            let a2 = to2(*at);
            let s2 = [dot(*side, x0.x3), dot(*side, x0.y3)];
            poly = clip(&poly, |q| dot2(sub2(q, a2), s2) + 1e-9);
        }
    }
    if flat_now.iter().all(|p| strips[p.k].u[1] > 1e-6) {
        let yc = if x0.inside { x0.t + x0.r } else { 0.0 } + gap / 2.0;
        poly = clip(&poly, |q| q[1] - yc);
    }
    for (at, n) in &flat.cuts {
        poly = clip(&poly, |q| {
            let q3 = add(
                add(add(wp, x0.heel3), scale(x0.x3, q[0])),
                scale(x0.y3, q[1]),
            );
            -dot(sub(q3, *at), *n)
        });
    }
    poly
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
    /// The flange across it from that heel to its toe (unit) and its width,
    /// m (L3b).
    band: ([f64; 3], f64),
    /// L3b: bent away from the flange even running towards the bend away
    /// from the joint ([`LayoutSettings::bend_back`]).
    back: bool,
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
    plan_joint_ctx(j, s, edge, except, floors, limit, &PlanCtx::default())
}

/// What a run tells the plan of one joint beyond the settings (L1, owner
/// 2026-11-19).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct PlanCtx {
    /// Members whose face goes onto a plate even where their bolts land on
    /// the flange (L1 b: a clash after writing a plate may clear).
    pub plated: BTreeSet<String>,
    /// Members bolted directly where their bolts land even where their face
    /// goes onto a plate (L2 c: the plate, or what is on it, ran into
    /// something after writing).
    pub direct: BTreeSet<String>,
    /// The joint is of the tower's body ([`mod@crate::joint_order`]): its
    /// diagonals bolted directly first (L1 c, with
    /// [`LayoutSettings::direct_first`]).
    pub body: bool,
    /// Planned as before L1 (b's faces tried on a plate off; since L6,
    /// 2026-11-20, also without L5o's holding bolts and J6's plates on a
    /// built-up member): the run's guard found the joint laid out worse
    /// with them.
    pub plain: bool,
    /// Planned without L11's steps (the end moved to pass the flange on a
    /// plate, one plate per plane, both faces of one plate): the run's
    /// guard found the joint laid out with fewer ends after writing with
    /// them than without.
    pub no_l11: bool,
    /// Planned without the plates between a built-up member's angles (L15):
    /// the run's guard found the joint laid out with fewer ends after
    /// writing with them than without.
    pub no_between: bool,
    /// Planned without the level plates' bolts onto their second holders
    /// and (item 2) the plates in their members' own planes farther out
    /// than a plate's thickness (L20): the run's guard found the joint laid
    /// out with fewer ends after writing with them than without, or as
    /// many on more plates.
    pub no_level: bool,
}

/// How many faces bolted directly are tried on a plate per joint (L1 b,
/// [`LayoutSettings::more_plates`]): the program's bound on the extra
/// work, not a standard's.
pub const PLATE_TRIES: usize = 4;

/// [`plan_joint_budget`] with what the run tells it (`ctx`). L1 (b): where
/// the plan leaves members for a hand detail, each face bolted directly
/// (up to [`PLATE_TRIES`], in the order planned) also tried on a plate;
/// one laying out more of the joint's ends is taken — direct bolting wins
/// a tie (the plate's penalty).
pub(crate) fn plan_joint_ctx(
    j: &JointDetail,
    s: &LayoutSettings,
    edge: EdgeRule<'_>,
    except: &BTreeMap<String, String>,
    floors: &BTreeMap<String, (f64, String)>,
    limit: usize,
    ctx: &PlanCtx,
) -> JointPlan {
    // L11: planned as before (the run's guard, [`PlanCtx::plain`]): without
    // the end moved to pass the flange and the plates' both faces — so that
    // a joint laid out with fewer ends after writing than the plan without
    // them is planned again without them (the owner: kept only where
    // better).
    let plain;
    let l11_off = (ctx.plain || ctx.no_l11) && (s.plate_shift || s.both_faces);
    // (L15) Without the plates between a built-up member's angles.
    let between_off = ctx.no_between && s.between_plates;
    // (L20) Without the level plates' bolts onto their second holders and
    // the plates in their members' own planes farther out (item 2).
    let level_off = ctx.no_level && (s.level_plates || s.own_offset_analysed);
    let s = if l11_off || between_off || level_off {
        plain = LayoutSettings {
            plate_shift: s.plate_shift && !l11_off,
            both_faces: s.both_faces && !l11_off,
            between_plates: s.between_plates && !between_off,
            level_plates: s.level_plates && !level_off,
            own_offset_analysed: s.own_offset_analysed && !level_off,
            ..s.clone()
        };
        &plain
    } else {
        s
    };
    let mut plan = plan_joint_searched(j, s, edge, except, floors, limit, ctx);
    // L11: both faces of one plate (as many ends, fewer plates).
    if s.both_faces {
        crate::both_faces::both_faces(j, &mut plan, s);
    }
    plan
}

/// [`plan_joint_ctx`] before both faces of one plate.
fn plan_joint_searched(
    j: &JointDetail,
    s: &LayoutSettings,
    edge: EdgeRule<'_>,
    except: &BTreeMap<String, String>,
    floors: &BTreeMap<String, (f64, String)>,
    limit: usize,
    ctx: &PlanCtx,
) -> JointPlan {
    // More of the joint's ends, or as many on fewer plates.
    let better = |a: &JointPlan, b: &JointPlan| {
        a.ends.len() > b.ends.len()
            || (a.ends.len() == b.ends.len() && a.plates.len() < b.plates.len())
    };
    let once = |s: &LayoutSettings| {
        let (mut plan, mut direct) =
            plan_once(j, s, edge, except, floors, limit, ctx, &BTreeSet::new());
        // L5: the faces bolted directly by moving their ends kept only where
        // that lays out more of the joint's ends, or as many on fewer plates;
        // else the plan without (no move).
        if s.direct_moved {
            let s0 = LayoutSettings {
                direct_moved: false,
                ..s.clone()
            };
            let (p0, d0) = plan_once(j, &s0, edge, except, floors, limit, ctx, &BTreeSet::new());
            if !better(&plan, &p0) {
                plan = p0;
                direct = d0;
            }
        }
        // L11: the ends moved to pass the flange on a plate kept the same
        // way (only where some end was so moved).
        if s.plate_shift
            && plan
                .notes
                .iter()
                .any(|n| n.contains("within the longest pull-back"))
        {
            let s1 = LayoutSettings {
                plate_shift: false,
                ..s.clone()
            };
            let (p1, d1) = plan_once(j, &s1, edge, except, floors, limit, ctx, &BTreeSet::new());
            if !better(&plan, &p1) {
                plan = p1;
                direct = d1;
            }
        }
        (plan, direct)
    };
    let (mut plan, mut direct) = once(s);
    // L15: the plates between a built-up member's angles kept unless the
    // plan without them lays out more ends, or as many on fewer plates (only
    // where some end was planned across such a gap): on a tie the plate
    // between — in the member's own plane, its bolts through both angles —
    // rather than one on an angle's outer face.
    if s.between_plates && plan.notes.iter().any(|n| n.contains(BETWEEN_NOTE)) {
        let s1 = LayoutSettings {
            between_plates: false,
            ..s.clone()
        };
        let (p1, d1) = once(&s1);
        if better(&p1, &plan) {
            plan = p1;
            direct = d1;
        }
    }
    // J6: the plates on a built-up member's angle kept the same way; else
    // the plan without (those landing bolted, the others left).
    if s.built_up_plates
        && j.arrivals.iter().any(|a| {
            a.profile
                .as_ref()
                .is_some_and(|q| !q.parts.is_empty() && !q.part_angles.is_empty())
        })
    {
        let s1 = LayoutSettings {
            built_up_plates: false,
            ..s.clone()
        };
        let (p1, d1) = once(&s1);
        if !better(&plan, &p1) {
            plan = p1;
            direct = d1;
        }
    }
    // L20 item 2: the plates in their members' own planes farther out than
    // a plate's thickness kept only where the plan lays out more ends, or
    // as many on fewer plates, than without them (only where an end lies
    // off its flange); on a tie the plan without.
    if s.own_offset_analysed
        && j.arrivals
            .iter()
            .any(|a| a.flat.is_some_and(|f| f.off > FLAT_OFF))
    {
        let s1 = LayoutSettings {
            own_offset_analysed: false,
            ..s.clone()
        };
        let (p1, d1) = once(&s1);
        if !better(&plan, &p1) {
            plan = p1;
            direct = d1;
        }
        // (An end still left off its flange: such a plate may put it as
        // far out — the estimates' upper bound that far either way.)
        for x in plan.off_flange.iter_mut() {
            x.1 = x.1.max(OWN_OUT_MAX);
        }
    }
    if !s.more_plates || ctx.plain || plan.hand_members.is_empty() {
        return plan;
    }
    let mut best = plan;
    for key in direct.into_iter().take(PLATE_TRIES) {
        let force: BTreeSet<PlaneKey> = [key].into();
        let (alt, _) = plan_once(j, s, edge, except, floors, limit, ctx, &force);
        if alt.ends.len() > best.ends.len() {
            best = alt;
        }
    }
    best
}

/// One plan of joint `j` (see [`plan_joint_with`]), the faces of `force`
/// onto a plate even where their bolts land on the flange; with the faces
/// bolted directly, in the order planned.
///
/// L22 ([`LayoutSettings::holds_clear`]): where a plate's own holding bolts
/// (L5o) as placed would meet what the plan lays out after that plate at
/// the joint ([`hold_conflicts`]), the joint is planned again with those
/// bolts kept clear of it (at most [`HOLD_TRIES`] times, what was found in
/// the way gathered); the plan again kept only where fewer of those bolts
/// meet something, with as many ends, no more members left and as many
/// plates held by their own bolts — else the first, as before.
#[allow(clippy::too_many_arguments)]
fn plan_once(
    j: &JointDetail,
    s: &LayoutSettings,
    edge: EdgeRule<'_>,
    except: &BTreeMap<String, String>,
    floors: &BTreeMap<String, (f64, String)>,
    limit: usize,
    ctx: &PlanCtx,
    force: &BTreeSet<PlaneKey>,
) -> (JointPlan, Vec<PlaneKey>) {
    let none = HoldAvoid::new();
    let (mut plan, mut direct, found, mut count) =
        plan_avoiding(j, s, edge, except, floors, limit, ctx, force, &none);
    if !s.holds_clear || ctx.plain || count == 0 {
        return (plan, direct);
    }
    let held = |p: &JointPlan| p.plates.iter().filter(|q| q.hold.is_some()).count();
    let mut asked = found;
    for _ in 0..HOLD_TRIES {
        let (p2, d2, found2, n2) =
            plan_avoiding(j, s, edge, except, floors, limit, ctx, force, &asked);
        if n2 < count
            && p2.ends.len() >= plan.ends.len()
            && p2.hand_members.len() <= plan.hand_members.len()
            && held(&p2) >= held(&plan)
        {
            plan = p2;
            direct = d2;
            count = n2;
        }
        if n2 == 0 {
            break;
        }
        let mut more = false;
        for (k, v) in found2 {
            let e = asked.entry(k).or_default();
            for q in v {
                if !e.contains(&q) {
                    e.push(q);
                    more = true;
                }
            }
        }
        if !more {
            break;
        }
    }
    (plan, direct)
}

/// The most times [`plan_once`] plans a joint again with its plates' own
/// holding bolts kept clear of what was found in their way (L22): **the
/// program's value**, a bound on the time only.
pub const HOLD_TRIES: usize = 2;

/// (L22) Per plate holding itself ([`hold_key`]): what its own bolts keep
/// clear of (3D corners from the work point).
type HoldAvoid = BTreeMap<String, Vec<Vec<[f64; 3]>>>;

/// (L22) A plate's own holding bolts as [`hold_flat`] placed them: the
/// plate ([`hold_key`]), the strip of the flange it lies on, the bolts'
/// places in that flange's plane, the plate's thickness, and the steel the
/// plan laid out with that plate (`laid3`'s entries: its own, not in the
/// way).
struct HoldRec {
    key: String,
    k: usize,
    pts: Vec<[f64; 2]>,
    tp: f64,
    own: std::ops::Range<usize>,
}

/// (L22) A plate holding itself by name, the same in each plan of a joint:
/// the member it lies on and its members.
fn hold_key(flat: &PlatePlan, x0: Option<&Strip>) -> Option<String> {
    let x0 = x0?;
    let mut m: Vec<&str> = flat.members.iter().map(String::as_str).collect();
    m.sort_unstable();
    Some(format!("{}|{}", x0.host, m.join(",")))
}

/// (L22) The slab a plate's own holding bolt fills across the flange it
/// lies on (heights over that flange's face), as the joint details draw
/// it ([`BOLT_DRAWN`]; L21): behind the flange its head, over the plate
/// its nut and the shank beyond it.
fn hold_slab(x: &Strip, tp: f64) -> (f64, f64) {
    (
        -(x.t + BOLT_DRAWN[1] * x.d),
        tp + (BOLT_DRAWN[2] + BOLT_DRAWN[3]) * x.d,
    )
}

/// (L22) The boxes a plate's own holding bolts fill (3D corners from the
/// work point): each bolt's nut's corner circle's square through the slab
/// of [`hold_slab`].
fn hold_solids(x: &Strip, pts: &[[f64; 2]], tp: f64) -> Vec<Vec<[f64; 3]>> {
    let off = dot(sub(x.face3, x.heel3), x.n3);
    let (lo, hi) = hold_slab(x, tp);
    let r = x.nut;
    pts.iter()
        .map(|q| {
            let mut c = Vec::with_capacity(8);
            for a in [q[0] - r, q[0] + r] {
                for b in [q[1] - r, q[1] + r] {
                    for z in [lo, hi] {
                        c.push(add(
                            add(add(x.heel3, scale(x.x3, a)), scale(x.y3, b)),
                            scale(x.n3, off + z),
                        ));
                    }
                }
            }
            c
        })
        .collect()
}

/// (L22) The plates' own holding bolts (L5o) of a plan that meet what the
/// plan laid out at the joint — the plates and ends of the other planes
/// and of the same face, their bolts' heads and nuts (among them those on
/// the other flange of the member the plate lies on, in its corner), and
/// the other plates' own holding bolts: each bolt's nut's corner circle's
/// square through the slab it fills ([`hold_slab`]) overlapping such a
/// piece's footprint there by more than the clash check's tolerance. Not
/// in the way: the plate's own steel and its ends (placed clear by
/// [`plate_holds`]), the member it lies on (its bolts are), and the bolts
/// of ends bolted onto that member (as the clash check excuses them). Per plate
/// with one such bolt: everything of the joint in its slab (to keep clear
/// of when planned again); and how many bolts meet something. Only the
/// plates still holding themselves at the end of the plan.
fn hold_conflicts(
    strips: &[Strip],
    recs: &[HoldRec],
    laid3: &[Laid3],
    pieces: &[(crate::ClashPart, Vec<[f64; 3]>)],
    fixed: &BTreeSet<String>,
    j: &JointDetail,
    plan: &JointPlan,
) -> (HoldAvoid, usize) {
    use crate::ClashPart;
    let mut out = HoldAvoid::new();
    let mut count = 0;
    let find = |m: &str| j.arrivals.iter().find(|a| a.member == m);
    let kept: Vec<bool> = recs
        .iter()
        .map(|r| {
            plan.plates.iter().any(|p| {
                p.hold
                    .as_ref()
                    .is_some_and(|h| h.points.len() == r.pts.len())
                    && hold_key(p, Some(&strips[r.k])).as_deref() == Some(r.key.as_str())
            })
        })
        .collect();
    for (i, r) in recs.iter().enumerate() {
        if !kept[i] {
            continue;
        }
        let x = &strips[r.k];
        let host = find(&x.host);
        let host_run = |m: &str| {
            m == x.host
                || host
                    .zip(find(m))
                    .is_some_and(|(h, b)| angle_deg(h.direction, b.direction) > 160.0)
        };
        let slab = hold_slab(x, r.tp);
        let mut pieces_in: Vec<Vec<[f64; 3]>> = laid3
            .iter()
            .enumerate()
            .filter(|(n, (m, bolt, _))| {
                !r.own.contains(n)
                    && if *bolt {
                        // (As the clash check: an end's bolt onto the
                        // member the plate is held onto may be the same.)
                        find(m)
                            .and_then(|a| a.onto.value.as_deref())
                            .is_none_or(|o| !host_run(o))
                    } else {
                        !host_run(m)
                    }
            })
            .map(|(_, (_, _, p))| p.clone())
            .collect();
        // What the plan does not lay out (as drawn): the members not laid
        // out and their bolts (the shank of one through the member the
        // plate is held onto excused, as the clash check does), plates and
        // covers given by hand.
        let onto_host = |m: &str| {
            find(m)
                .and_then(|a| a.onto.value.as_deref())
                .is_some_and(host_run)
        };
        for (part, pts) in pieces {
            let keep = match part {
                ClashPart::Member(m) => fixed.contains(m) && !host_run(m),
                ClashPart::Bolt {
                    member: m, piece, ..
                } => {
                    fixed.contains(m)
                        && !onto_host(m)
                        && !(*piece == crate::clash::BoltPiece::Shank && host_run(m))
                }
                ClashPart::Plate(_) | ClashPart::Cover(..) => true,
                ClashPart::PlateBolt { .. } => false,
            };
            if keep {
                pieces_in.push(pts.clone());
            }
        }
        for (i2, r2) in recs.iter().enumerate() {
            if i2 != i && kept[i2] {
                pieces_in.extend(hold_solids(&strips[r2.k], &r2.pts, r2.tp));
            }
        }
        let feet: Vec<(usize, Vec<[f64; 2]>)> = pieces_in
            .iter()
            .enumerate()
            .filter_map(|(n, p)| footprint_in(x, p, slab).map(|f| (n, f)))
            .collect();
        let rr = x.nut;
        let hits = r
            .pts
            .iter()
            .filter(|q| {
                let sq = [
                    [q[0] - rr, q[1] - rr],
                    [q[0] + rr, q[1] - rr],
                    [q[0] + rr, q[1] + rr],
                    [q[0] - rr, q[1] + rr],
                ];
                feet.iter()
                    .any(|(_, f)| sat2(&sq, f) > crate::clash::CLASH_TOLERANCE)
            })
            .count();
        if hits > 0 {
            count += hits;
            out.insert(
                r.key.clone(),
                feet.iter().map(|(n, _)| pieces_in[*n].clone()).collect(),
            );
        }
    }
    (out, count)
}

/// [`plan_once`] with the plates' own holding bolts kept clear of
/// `avoid` (L22); also what [`hold_conflicts`] finds in the plan.
#[allow(clippy::too_many_arguments)]
fn plan_avoiding(
    j: &JointDetail,
    s: &LayoutSettings,
    edge: EdgeRule<'_>,
    except: &BTreeMap<String, String>,
    floors: &BTreeMap<String, (f64, String)>,
    limit: usize,
    ctx: &PlanCtx,
    force: &BTreeSet<PlaneKey>,
    avoid: &HoldAvoid,
) -> (JointPlan, Vec<PlaneKey>, HoldAvoid, usize) {
    let mut direct_planes: Vec<PlaneKey> = Vec::new();
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
        // (L15) Across a gap between a built-up member's angles: onto a
        // plate between them (laid flat only).
        let between = (s.between_plates
            && s.built_up_plates
            && !ctx.plain
            && off.is_none()
            && host
                .profile
                .as_ref()
                .is_some_and(|q| q.angle.is_none() && q.part_angles.len() >= 2))
        .then(|| crate::joints::host_flange_between(a, host))
        .flatten();
        if between.is_some() {
            plan.notes.push(format!(
                "{}: {BETWEEN_NOTE} {}'s angles",
                a.member, host.member
            ));
        }
        let strip =
            strip_of(j, a, host, s, edge, (&others, except), off, false, between).or_else(|e| {
                if off.is_some() {
                    strip_of(j, a, host, s, edge, (&others, except), off, true, None).map_err(|_| e)
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
    // (L22) The plates' own holding bolts placed (L5o), for the check at
    // the end of the plan.
    let mut hold_recs: Vec<HoldRec> = Vec::new();
    // By plane, the planes on the biggest members first (what bolts onto
    // a member laid out on a leg keeps clear of it as laid out), each in
    // the owner's order.
    // (A plane is a flange's face, whatever member of a run through the
    // joint its ends name: in line, they share it.)
    let mut by_plane: BTreeMap<PlaneKey, Vec<usize>> = BTreeMap::new();
    for (k, x) in strips.iter().enumerate() {
        by_plane.entry(x.plane).or_default().push(k);
    }
    // (A plane whose ends bolt onto a member whose own end is laid out
    // here comes after that member's plane: its bolts then keep to that
    // member's steel past its cut, below; J9 and D16, 2026-11-19.)
    let depth = |k: usize| -> usize {
        let mut d = 0;
        let mut at = k;
        while d < strips.len() {
            match strips.iter().position(|y| y.member == strips[at].host) {
                Some(h) => {
                    d += 1;
                    at = h;
                }
                None => break,
            }
        }
        d
    };
    // (L18: of one rank, high to low — the plane of the biggest member
    // first — where asked.)
    let mut planes: BTreeMap<PlaneOrder, Vec<usize>> = BTreeMap::new();
    for (key, ks) in by_plane {
        let first = ks
            .iter()
            .map(|&k| (strips[k].host_rank, strips[k].host.clone()))
            .min()
            .expect("a plane has ends");
        let size = if s.planes_high_to_low {
            ks.iter()
                .map(|&k| (strips[k].rank.0, strips[k].rank.1))
                .min()
                .unwrap_or((0, 0))
        } else {
            (0, 0)
        };
        let deep = ks.iter().map(|&k| depth(k)).max().unwrap_or(0);
        planes.insert((deep, first.0, size, first.1, key), ks);
    }
    // The cut of each member laid out so far (where its steel now starts).
    let mut laid_cut: BTreeMap<String, f64> = BTreeMap::new();
    let mut placed_all: Vec<Placed> = Vec::new();
    let mut plate_of: BTreeMap<usize, usize> = BTreeMap::new();
    let mut bent_wait: BTreeMap<PlaneKey, Vec<Placed>> = BTreeMap::new();
    let mut budget = limit;
    for ((_, _, _, _, plane_key), ks) in &planes {
        // L2 (b), 2026-11-20: a plane on a member whose own end is laid out
        // in this plan (its planes come first) taken where that member will
        // be drawn — moved across by its move and lifted onto its plate —,
        // so that what bolts onto it, and a plate on it, are planned where
        // they will be drawn (before, where the member was: their bolts on
        // its flange and their clashes not as written). The ends themselves
        // stay where they are drawn (their gauge lines through the work
        // point); their bolts' places on that flange, its face and what is
        // in their way follow it. Not followed: a member on a plate in its
        // own plane (step 4) — as before.
        if s.exact_plan {
            for &k in ks {
                if strips[k].tilted.is_some() {
                    continue;
                }
                let Some(hk) = strips.iter().position(|y| y.member == strips[k].host) else {
                    continue;
                };
                let Some(hp) = placed_all.iter().find(|q| q.k == hk) else {
                    continue;
                };
                let h = &strips[hk];
                if h.tilted.is_some() {
                    continue;
                }
                let v3 = add(scale(h.x3, h.v[0]), scale(h.y3, h.v[1]));
                let lift = match plate_of.get(&hk) {
                    Some(&i) if plan.plates[i].own.is_none() => plan.plates[i].thickness,
                    _ => 0.0,
                };
                let d = add(scale(v3, hp.moved), scale(h.n3, lift));
                let x = &mut strips[k];
                let dp = [dot(d, x.x3), dot(d, x.y3)];
                x.heel3 = add(x.heel3, d);
                x.face3 = add(x.face3, d);
                x.heel = add(x.heel, d);
                x.w = sub2(x.w, dp);
                for (c, _) in x.obstacles.iter_mut() {
                    *c = sub2(*c, dp);
                }
                for (c, _, _) in x.partners.iter_mut() {
                    *c = sub2(*c, dp);
                }
            }
        }
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
            // L2 (b): as high as its outstanding leg, and behind the face
            // where its nuts and shanks lie (a flange or plate as thick as
            // the thicker of its leg and the flange).
            if s.exact_plan && strips[k].tilted.is_none() && strips[k].over.is_none() {
                let x = &strips[k];
                let thick = x.own_t.max(x.t);
                let top = x.out_w + thick;
                let far = thick + (BOLT_DRAWN[2] + BOLT_DRAWN[3]) * x.d;
                let up = fixed_slab(x, j, &pieces, &fixed, &laid3, (FACE_TOUCH, top));
                let down = fixed_slab(x, j, &pieces, &fixed, &laid3, (-far, -FACE_TOUCH));
                strips[k].fixed_out = up;
                strips[k].fixed_far = down;
            }
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
            // (Not one left for a partner's bolts, L8: its place is not
            // kept free — as a member taken back after writing.)
            if strips[k].floor.is_none()
                && place(&strips, k, &[], s).is_err_and(|e| !e.contains(PARTNER_LEFT))
            {
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
        //
        // J9 and D16 (2026-11-19): what it bolts onto ending at the joint
        // (not one part through it), every bolt on that member's steel —
        // past where it starts (its cut: as laid out here, else as given
        // or worked out, else the work point) by the end distance; else
        // onto a plate.
        let lands = |p: &Placed| {
            let x = &strips[p.k];
            let from = laid_cut.get(&x.host).copied().unwrap_or(x.host_cut);
            x.tilted.is_none()
                && x.between.is_none()
                && bolt_ys(x, p)
                    .iter()
                    .all(|&y| y >= x.t + x.r + x.nut - 1e-9 && y <= x.width - x.edge + 1e-9)
                && (!x.host_ends
                    || bolt_along(x, p)
                        .iter()
                        .all(|&s| s >= from + x.host_end - 1e-9))
        };
        // (L1 b: a face forced onto a plate, or an end on it the run's
        // clash rounds put onto one.)
        let plated = force.contains(plane_key)
            || placed
                .iter()
                .any(|p| ctx.plated.contains(&strips[p.k].member));
        let direct = !plated && placed.iter().all(lands);
        // (Each end's landing as it is now, for the ends on a splice's
        // cover below.)
        let landed: BTreeSet<usize> = placed.iter().filter(|p| lands(p)).map(|p| p.k).collect();
        // L2 (c): on a face that goes onto a plate, the members the run's
        // clash rounds asked bolted directly (their bolts landing) — and
        // (L1 c, owner 2026-11-19: "Redundant members are usually bolted
        // into another and diagonals of the shaft too") the redundants and
        // the body's diagonals whose bolts land, first — bolted directly; the
        // others onto the plate.
        if !direct {
            let first = |p: &Placed| {
                let x = &strips[p.k];
                landed.contains(&p.k)
                    && (ctx.direct.contains(&x.member)
                        || (s.direct_first
                            && !ctx.plain
                            && !plated
                            && (x.rank.0 == ClassRank::Redundant as u8
                                || (ctx.body && x.rank.0 == ClassRank::Diagonal as u8))))
            };
            let (on, off): (Vec<Placed>, Vec<Placed>) = placed.iter().cloned().partition(first);
            if !on.is_empty() {
                laid3.extend(solids_of(&strips, &on, 0.0, s, None));
                for p in &on {
                    laid_cut.insert(strips[p.k].member.clone(), cut_of(&strips[p.k], p));
                }
                placed_all.extend(on);
                placed = off;
                if placed.is_empty() {
                    continue;
                }
            }
        }
        // L5 (2026-11-20, the owner's idea: "move the 2 diagonal a bit
        // forward maybe to have one bolt on the main leg without them
        // touching each other"): on a face that goes onto a plate, an end
        // whose bolts do not land bolted directly where pulled back further,
        // or moved sideways by at most the plate's thickness — the
        // eccentricity the plate would have put it out of the flange by —,
        // every bolt lands on the flange, clear of the others as they lie.
        // Where every end of the face then lands, the face is bolted
        // directly: no plate.
        if !direct && s.direct_moved && !plated {
            let tp = placed.iter().map(|p| strips[p.k].own_t).fold(0.0, f64::max);
            let most = s.shift_limit.min(tp);
            let mut cur = placed.clone();
            let mut moved_ks: BTreeSet<usize> = BTreeSet::new();
            for i in 0..cur.len() {
                let x = &strips[cur[i].k];
                if landed.contains(&cur[i].k)
                    || x.tilted.is_some()
                    || x.over.is_some()
                    || x.between.is_some()
                {
                    continue;
                }
                let others: Vec<Placed> = cur
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, q)| q.clone())
                    .collect();
                let from = laid_cut.get(&x.host).copied().unwrap_or(x.host_cut);
                if let Some(q) = land_moved(&strips, &cur[i], &others, s, from, most) {
                    moved_ks.insert(q.k);
                    cur[i] = q;
                }
            }
            let all = cur
                .iter()
                .all(|p| landed.contains(&p.k) || moved_ks.contains(&p.k));
            if !moved_ks.is_empty() && all {
                for p in cur.iter().filter(|p| moved_ks.contains(&p.k)) {
                    let x = &strips[p.k];
                    let by = (p.moved - concentric_of(x)).abs();
                    if by > 1e-9 {
                        plan.notes.push(format!(
                            "{}: moved {} sideways to bolt directly onto {} (no plate): an \
                             eccentricity, its shift — at most the plate's thickness",
                            x.member,
                            mm_text(by),
                            x.host
                        ));
                    }
                }
                laid3.extend(solids_of(&strips, &cur, 0.0, s, None));
                for p in &cur {
                    laid_cut.insert(strips[p.k].member.clone(), cut_of(&strips[p.k], p));
                }
                placed_all.extend(cur);
                direct_planes.push(*plane_key);
                continue;
            }
        }
        // (J6, 2026-11-20) A built-up member whose angles are known:
        // a plate on the angle facing the ends, as on a single angle.
        let built = |m: &str| {
            find(m)
                .and_then(|h| h.profile.as_ref())
                .is_some_and(|q| !q.parts.is_empty())
        };
        let built_known = |m: &str| {
            s.built_up_plates
                && !ctx.plain
                && find(m)
                    .and_then(|h| h.profile.as_ref())
                    .is_some_and(|q| !q.parts.is_empty() && !q.part_angles.is_empty())
        };
        if !direct && built(&strips[placed[0].k].host) && !built_known(&strips[placed[0].k].host) {
            // On a built-up member (no plate laid out there, below) those
            // landing on its flange are bolted to it all the same, the
            // others left. (A plate for some ends of a flange and the
            // others directly on it, the plate on the other side of the
            // work point, was tried on two real towers, 2026-11-18: more
            // joints then clashed; every end of the plane goes onto the
            // plate.)
            let (on, off): (Vec<Placed>, Vec<Placed>) =
                placed.iter().cloned().partition(|p| landed.contains(&p.k));
            laid3.extend(solids_of(&strips, &on, 0.0, s, None));
            for p in &on {
                laid_cut.insert(strips[p.k].member.clone(), cut_of(&strips[p.k], p));
            }
            placed_all.extend(on);
            placed = off;
            if placed.is_empty() {
                continue;
            }
        }
        if !direct && placed.iter().any(|p| strips[p.k].through.is_some()) {
            // On a splice's outer cover (O44 J7): those whose bolts land on
            // the flange bolted through the cover; the others left — a
            // plate over the cover is not laid out.
            let (on, off): (Vec<Placed>, Vec<Placed>) =
                placed.iter().cloned().partition(|p| landed.contains(&p.k));
            laid3.extend(solids_of(&strips, &on, 0.0, s, None));
            for p in &on {
                laid_cut.insert(strips[p.k].member.clone(), cut_of(&strips[p.k], p));
            }
            placed_all.extend(on);
            for p in &off {
                let x = &strips[p.k];
                plan.by_hand(
                    &x.member,
                    format!(
                        "{}: its bolts do not all land on {}'s flange through splice {}'s outer \
                         cover, and a plate over the cover is not laid out (lay it out by hand)",
                        x.member,
                        x.host,
                        x.through.as_deref().unwrap_or("?")
                    ),
                );
            }
            continue;
        }
        if !direct {
            // A plate on a built-up member is drawn through the work point
            // (where it lies enters its checks, 2026-11-11), not on the
            // angle: not laid out automatically.
            let host = &strips[placed[0].k].host;
            if built(host) && !built_known(host) {
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
            // (L2 c: as placed before going onto a plate.)
            let orig: Vec<Placed> = placed.clone();
            let mut kept: Vec<Placed> = Vec::new();
            // (L3) Each end off the flange: the plates it can take (bent by
            // its few degrees up or down, L3; welded, step 4), each with the
            // cut it needs.
            let mut rooms: BTreeMap<usize, Vec<(Room, f64)>> = BTreeMap::new();
            let mut room_errs: BTreeMap<usize, Vec<(Room, String)>> = BTreeMap::new();
            for mut p in placed {
                let x = &strips[p.k];
                let need = past_toe(x, &p, s.gap);
                // Step 4: its plate in its own plane past the flange's toe
                // by half the gap too (under it nothing but air: its nuts
                // and its edge low near the seam). L3: per plate it can take.
                if let Some(t) = &x.tilted {
                    let limit = (s.pull_back * t.lu).min(x.half - bolt_run(x));
                    let mut ok: Vec<(Room, f64)> = Vec::new();
                    let mut errs: Vec<(Room, String)> = Vec::new();
                    for room in rooms_for(t, s) {
                        match toe_clear(x, t, &p, need.max(p.cut), j.position, s.gap, room) {
                            Ok(c) if c.max(need) <= p.cut + 1e-9 || c.max(need) <= limit + 1e-9 => {
                                ok.push((room, c.max(need).max(p.cut)));
                            }
                            Ok(c) => errs.push((
                                room,
                                format!(
                                    "on a plate its bolts need it pulled back {:.0} mm to pass the \
                                     flange of {} (the longest pull-back {:.0} mm)",
                                    c.max(need).min(9.999) * 1000.0,
                                    x.host,
                                    limit * 1000.0
                                ),
                            )),
                            Err(e) => errs.push((room, e)),
                        }
                    }
                    if ok.is_empty() {
                        plan.by_hand(&x.member, tilted_why(x, t, &rooms_words(&errs)));
                        continue;
                    }
                    p.cut = ok[0].1;
                    rooms.insert(p.k, ok);
                    room_errs.insert(p.k, errs);
                    kept.push(p);
                    continue;
                }
                if need > p.cut + 1e-9 {
                    let limit = (s.pull_back * x.tilted.as_ref().map_or(1.0, |t| t.lu))
                        .min(x.half - bolt_run(x));
                    // L11: moved sideways so that its bolts pass the flange
                    // within the pull-back, where the shift limit allows.
                    if need > limit + 1e-9
                        && s.plate_shift
                        && let Some(q) = {
                            let others: Vec<Placed> =
                                orig.iter().filter(|q| q.k != p.k).cloned().collect();
                            shift_past_toe(&strips, &p, &others, s, limit)
                        }
                    {
                        plan.notes.push(format!(
                            "{}: moved {} sideways so that on its plate its bolts pass the \
                             flange of {} within the longest pull-back: an eccentricity, its \
                             shift",
                            x.member,
                            mm_text((q.moved - concentric_of(x)).abs()),
                            x.host
                        ));
                        kept.push(q);
                        continue;
                    }
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
            let tf_all = tf(&kept);
            for kp in kept.iter_mut() {
                let p = kp.clone();
                let x = &strips[p.k];
                let Some(t) = x.tilted.as_ref() else {
                    continue;
                };
                // Each plate it can take (L3: bent up, bent down; then
                // welded): its geometry, its axis within the limit, the
                // flat plate it is bent from or welded to fitting. Of the
                // bent ones the one moving its axis least (up on a tie),
                // then welded.
                let mut fits: Vec<(f64, OwnGeo, f64)> = Vec::new();
                let mut errs: Vec<(Room, String)> = Vec::new();
                for (room, cut) in rooms.get(&p.k).cloned().unwrap_or_default() {
                    let geo = match own_geometry(
                        t,
                        cut3(t, cut),
                        p.moved,
                        t.face,
                        t.out,
                        tf_all,
                        s.gap,
                        room,
                    ) {
                        Ok(g) => g,
                        Err(e) => {
                            errs.push((room, e));
                            continue;
                        }
                    };
                    // Its axis at most a plate's thickness farther out of the
                    // flange than on a flat plate (its line of action off the
                    // work point about as a filler would put it; more is not
                    // in the analysis: a hand detail). L3, bent down: at
                    // most a plate's thickness nearer either.
                    if !within(room, geo.out_more, x.own_t, s) {
                        errs.push((room, farther_out(geo.out_more, x.own_t, room, s)));
                        continue;
                    }
                    if let Err(e) = own_fits(&strips, x, &geo, &flat_ends, &owns, j.position, s.gap)
                    {
                        errs.push((room, e));
                        continue;
                    }
                    let rank = match room {
                        Room::Weld => f64::INFINITY,
                        Room::Bend { down } => geo.out_more.abs() + if down { 1e-6 } else { 0.0 },
                    };
                    fits.push((rank, geo, cut));
                }
                let best = fits
                    .into_iter()
                    .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                match best {
                    Some((_, geo, cut)) => {
                        kp.cut = cut;
                        owns.push((p.k, geo));
                    }
                    None => {
                        let mut all = room_errs.remove(&p.k).unwrap_or_default();
                        all.extend(errs);
                        plan.by_hand(&x.member, tilted_why(x, t, &rooms_words(&all)));
                        dropped.push(p.k);
                    }
                }
            }
            kept.retain(|p| !dropped.contains(&p.k));
            let mut placed = kept;
            if placed.is_empty() {
                continue;
            }
            let mut thickness = tf(&placed);
            // (L15) Between a built-up member's angles: as thick as the gap,
            // where that is at least BETWEEN_LEAST of the thickest leg on it.
            let between = strips[placed[0].k].between;
            if let Some((gap, _)) = between {
                if gap + 1e-9 < BETWEEN_LEAST * thickness {
                    let host = strips[placed[0].k].host.clone();
                    for p in &placed {
                        let m = strips[p.k].member.clone();
                        plan.by_hand(
                            &m,
                            format!(
                                "{m}: across the gap between {host}'s angles, a plate between \
                                 them would be {} thick (the gap), less than {:.0} % of the \
                                 thickest leg on it ({}; lay it out by hand)",
                                mm_text(gap),
                                BETWEEN_LEAST * 100.0,
                                mm_text(thickness)
                            ),
                        );
                    }
                    continue;
                }
                thickness = gap;
            }
            // (L2's follow-up a, 2026-11-20) Its outline stopping short of
            // the work point, where the one round it would run into
            // something (below).
            let mut short = false;
            let mut cuts: Vec<([f64; 3], [f64; 3])> = Vec::new();
            let mut short_poly: Option<Vec<[f64; 2]>> = None;
            // L2 (b), 2026-11-20: the plate as it will be drawn (round the
            // work point, the legs over their bolts with their end and edge
            // distances, and the flange under it as far as those reach, cut
            // at the heel) clear of the fixed steel and of what the planes
            // before laid out, in its thickness; else (L2 c) the ends whose
            // bolts land on the flange bolted directly instead, the others
            // left (by hand).
            if s.exact_plan && placed.iter().any(|p| strips[p.k].tilted.is_none()) {
                let poly = plate_outline2(&strips, &placed, s.gap, Outline::Round);
                let on: BTreeSet<&str> =
                    placed.iter().map(|p| strips[p.k].member.as_str()).collect();
                let excused: BTreeSet<String> = plan.hand_members.iter().cloned().collect();
                let x0 = placed
                    .iter()
                    .map(|p| &strips[p.k])
                    .find(|x| x.tilted.is_none())
                    .expect("a flat end");
                let mut hit = plate_hits(
                    x0, &on, j, &pieces, &fixed, &excused, &laid3, &poly, thickness,
                )
                .map(|h| h.0);
                // (L2's follow-up a) Stopping short of the work point — round
                // the members' legs from the flange's toe over their bolts and
                // the flange under them — and cut straight short of what it
                // would still run into (each bolt keeping its distances, the
                // legs from the toe kept), where that clears it and the plate
                // still lies on the flange over half a square of its width:
                // not for a flat plate with plates in their members' own
                // planes welded to it (it must reach the seams and the flange
                // from the work point).
                let mut also = None;
                if hit.is_some() && s.short_plates && owns.is_empty() {
                    let mut poly = plate_outline2(&strips, &placed, s.gap, Outline::Short);
                    let core = plate_outline2(&strips, &placed, s.gap, Outline::Core);
                    let mut lines: Vec<([f64; 2], f64)> = Vec::new();
                    let mut clear = false;
                    for _ in 0..=CUT_TRIES {
                        match plate_hits(
                            x0, &on, j, &pieces, &fixed, &excused, &laid3, &poly, thickness,
                        ) {
                            None => {
                                clear = true;
                                break;
                            }
                            Some((by, foot)) => {
                                also.get_or_insert(by);
                                if lines.len() == CUT_TRIES {
                                    break;
                                }
                                let Some((ax, lim)) = separate(&core, &foot, s.gap) else {
                                    break;
                                };
                                poly = clip(&poly, |q| lim - dot2(q, ax));
                                lines.push((ax, lim));
                            }
                        }
                    }
                    if clear && on_flange(&poly, x0) {
                        short = true;
                        hit = None;
                        cuts = lines
                            .iter()
                            .map(|(ax, lim)| {
                                let n = add(scale(x0.x3, ax[0]), scale(x0.y3, ax[1]));
                                let at = add(
                                    j.position,
                                    add(
                                        x0.heel3,
                                        add(scale(x0.x3, ax[0] * lim), scale(x0.y3, ax[1] * lim)),
                                    ),
                                );
                                (at, n)
                            })
                            .collect();
                        short_poly = Some(poly);
                    } else if clear {
                        also = Some(format!(
                            "nothing, but it would then lie on {}'s flange over less than half \
                             a square of its width",
                            x0.host
                        ));
                    }
                }
                if let Some(by) = hit {
                    let by = match also {
                        Some(b2) if b2 != by => {
                            format!("{by} (stopping short of the work point: {b2})")
                        }
                        Some(_) => format!("{by} (stopping short of the work point too)"),
                        None => by,
                    };
                    let direct_now: Vec<Placed> = if s.clash_tries {
                        orig.iter()
                            .filter(|p| placed.iter().any(|q| q.k == p.k) && landed.contains(&p.k))
                            .cloned()
                            .collect()
                    } else {
                        Vec::new()
                    };
                    for p in &placed {
                        if direct_now.iter().any(|q| q.k == p.k) {
                            continue;
                        }
                        let m = strips[p.k].member.clone();
                        plan.by_hand(
                            &m,
                            format!(
                                "{m}: on a plate on {}, the plate would run into {by} (its \
                                 outline round the work point, the legs over their bolts and \
                                 the flange under it; lay it out by hand)",
                                x0.host
                            ),
                        );
                    }
                    if !direct_now.is_empty() {
                        laid3.extend(solids_of(&strips, &direct_now, 0.0, s, None));
                        for p in &direct_now {
                            laid_cut.insert(strips[p.k].member.clone(), cut_of(&strips[p.k], p));
                        }
                        placed_all.extend(direct_now);
                    }
                    continue;
                }
            }
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
                short,
                cuts,
                between: between.is_some(),
                also: None,
            };
            let mut own_plates: Vec<PlatePlan> = Vec::new();
            let mut seams3: Vec<([f64; 3], [f64; 3])> = Vec::new();
            let mut short_reach: Vec<[f64; 3]> = Vec::new();
            let mut seam_pts: Vec<[f64; 3]> = Vec::new();
            for (k, g0) in &owns {
                let x = &strips[*k];
                let t = x.tilted.as_ref().expect("tilted");
                let p = placed.iter().find(|p| p.k == *k).expect("kept");
                // Its final place with the flat plate's thickness.
                let geo = own_geometry(
                    t,
                    cut3(t, p.cut),
                    p.moved,
                    t.face,
                    t.out,
                    thickness,
                    s.gap,
                    g0.room,
                )
                .expect("laid out before");
                let abs = |q: [f64; 3]| add(q, j.position);
                let r = flat.reach.get_or_insert_with(|| Reach {
                    points: Vec::new(),
                    normal: scale(t.out, -1.0),
                    keep_to: Vec::new(),
                });
                let all = own_reach(x, &geo, thickness, j.position);
                // (L9) Stopping short of the work point: the seams and the
                // flange under them only (not under the work point).
                short_reach.extend(
                    all.iter()
                        .enumerate()
                        .filter(|(i, _)| *i != 2 && *i != 3)
                        .map(|(_, q)| *q),
                );
                seam_pts.extend(all.iter().take(2).copied());
                r.points.extend(all);
                r.keep_to.push((abs(geo.seam_at), geo.side));
                let seam = norm(sub(geo.seam[1], geo.seam[0]));
                seams3.push((geo.seam_at, geo.side));
                // L3: bent from the flat plate by its few degrees (no weld),
                // its bend line where the two flats' middle planes meet.
                let (weld_size, weld_length, bend) = match geo.room {
                    Room::Weld => (weld_throat(thickness), round_to(2.0 * seam, 1e-4), None),
                    Room::Bend { .. } => (
                        0.0,
                        0.0,
                        Some(BendPlan {
                            angle: round_to(geo.angle, 0.01),
                            inner_radius: bend_radius(thickness),
                            line: [abs(geo.middle[0]), abs(geo.middle[1])],
                        }),
                    ),
                };
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
                        weld_size,
                        weld_length,
                        bend,
                    }),
                    reach: None,
                    outline: None,
                    hold: None,
                    short: false,
                    cuts: Vec::new(),
                    between: false,
                    also: None,
                });
            }
            // L9 (L8's question 5, 2026-11-21): a flat plate with plates in
            // their members' own planes welded to it, its outline as the run
            // will write it — round the bolts of the ends flat on it, the
            // seams and the flange under them and under the work point,
            // kept to the seams' sides, cut at the heel —, clear of the fixed
            // steel and of what the planes before laid out (before: not
            // checked; it ran into a run lapped on the same face, found after
            // writing). Else stopping short of the work point — the seams
            // and the flange under them only — and cut straight short of what
            // it would still run into (the seams and the flat ends' legs over
            // their bolts kept), where it then still lies on the flange over
            // half a square of its width. Else the ends in their own planes
            // left for a hand detail, said; the ends flat on it keep their
            // plate (checked above).
            if s.reach_clear
                && s.exact_plan
                && let Some(rc) = flat.reach.clone()
            {
                let x0 = &strips[placed[0].k];
                let to2 = |q: [f64; 3]| {
                    let d = sub(sub(q, j.position), x0.heel3);
                    [dot(d, x0.x3), dot(d, x0.y3)]
                };
                let keep = |poly: Vec<[f64; 2]>| -> Vec<[f64; 2]> {
                    let mut poly = poly;
                    for (at, side) in &rc.keep_to {
                        let a2 = to2(*at);
                        let s2 = [dot(*side, x0.x3), dot(*side, x0.y3)];
                        poly = clip(&poly, |q| dot2(sub2(q, a2), s2) + 1e-9);
                    }
                    if let Some((heel, across)) = flat.heel {
                        let half = s.gap / 2.0;
                        poly = clip(&poly, |q| {
                            let q3 = add(
                                add(add(j.position, x0.heel3), scale(x0.x3, q[0])),
                                scale(x0.y3, q[1]),
                            );
                            dot(sub(q3, heel), across) - half
                        });
                    }
                    poly
                };
                let flat_now: Vec<Placed> = placed
                    .iter()
                    .filter(|p| strips[p.k].tilted.is_none())
                    .cloned()
                    .collect();
                let hull_of = |pts: &[[f64; 3]], form: Outline| {
                    let mut q: Vec<[f64; 2]> = pts.iter().map(|p| to2(*p)).collect();
                    if !flat_now.is_empty() {
                        q.extend(plate_outline2(&strips, &flat_now, s.gap, form));
                    }
                    keep(crate::joint_sheet::hull(q))
                };
                let on: BTreeSet<&str> =
                    placed.iter().map(|p| strips[p.k].member.as_str()).collect();
                let excused: BTreeSet<String> = plan.hand_members.iter().cloned().collect();
                let hits = |poly: &[[f64; 2]]| {
                    plate_hits(
                        x0, &on, j, &pieces, &fixed, &excused, &laid3, poly, thickness,
                    )
                };
                let round = hull_of(&rc.points, Outline::Round);
                if let Some((by, _)) = hits(&round) {
                    // Short of the work point, cut straight.
                    let mut poly = hull_of(&short_reach, Outline::Short);
                    let core = hull_of(&seam_pts, Outline::Core);
                    let mut lines: Vec<([f64; 2], f64)> = Vec::new();
                    let mut clear = false;
                    let mut also: Option<String> = None;
                    for _ in 0..=CUT_TRIES {
                        match hits(&poly) {
                            None => {
                                clear = true;
                                break;
                            }
                            Some((b2, foot)) => {
                                also.get_or_insert(b2);
                                if lines.len() == CUT_TRIES {
                                    break;
                                }
                                let Some((ax, lim)) = separate(&core, &foot, s.gap) else {
                                    break;
                                };
                                poly = clip(&poly, |q| lim - dot2(q, ax));
                                lines.push((ax, lim));
                            }
                        }
                    }
                    if clear && poly.len() >= 3 && on_flange(&poly, x0) {
                        if let Some(r) = flat.reach.as_mut() {
                            r.points.clone_from(&short_reach);
                        }
                        flat.short = true;
                        short_poly = Some(poly.clone());
                        for (ax, lim) in &lines {
                            let n = add(scale(x0.x3, ax[0]), scale(x0.y3, ax[1]));
                            let at = add(
                                j.position,
                                add(
                                    x0.heel3,
                                    add(scale(x0.x3, ax[0] * lim), scale(x0.y3, ax[1] * lim)),
                                ),
                            );
                            flat.cuts.push((at, n));
                        }
                    } else {
                        let by = match also {
                            Some(b2) if b2 != by => {
                                format!("{by} (stopping short of the work point: {b2})")
                            }
                            Some(_) => format!("{by} (stopping short of the work point too)"),
                            None => by,
                        };
                        for (k, _) in &owns {
                            let m = strips[*k].member.clone();
                            plan.by_hand(
                                &m,
                                format!(
                                    "{m}: on a plate in its own plane welded to a plate on {}, \
                                     that plate would run into {by} (its outline round the \
                                     seams, the flange under them and the work point; lay it \
                                     out by hand)",
                                    x0.host
                                ),
                            );
                        }
                        let gone: Vec<usize> = owns.iter().map(|(k, _)| *k).collect();
                        owns.clear();
                        own_plates.clear();
                        seams3.clear();
                        placed.retain(|p| !gone.contains(&p.k));
                        flat.reach = None;
                        if placed.is_empty() {
                            continue;
                        }
                        flat.thickness = tf(&placed);
                    }
                }
            }
            // L5o (2026-11-20): its own provisional bolts onto the member
            // it lies on, where nothing of the joint's legs bolts onto it.
            // (Not where the run's guard plans the joint as before: the
            // after-writing checks found it laid out worse.)
            if s.hold_plates && !ctx.plain {
                let excused: BTreeSet<String> = plan.hand_members.iter().cloned().collect();
                let on: BTreeSet<&str> =
                    placed.iter().map(|p| strips[p.k].member.as_str()).collect();
                let x0 = placed
                    .iter()
                    .map(|p| &strips[p.k])
                    .find(|x| x.tilted.is_none());
                let tp = flat.thickness;
                let clear = |poly: &[[f64; 2]]| -> Option<String> {
                    let x0 = x0?;
                    plate_hits(x0, &on, j, &pieces, &fixed, &excused, &laid3, poly, tp).map(|h| h.0)
                };
                let key = hold_key(&flat, x0);
                let rec = hold_flat(
                    &mut flat,
                    &mut plan.notes,
                    &strips,
                    &placed,
                    &placed_all,
                    j,
                    s,
                    HoldAt {
                        poly: short_poly.as_deref(),
                        seams3: &seams3,
                        laid_cut: &laid_cut,
                        clear: &clear,
                        host_keep: x0
                            .and_then(|x| j.arrivals.iter().find(|a| a.member == x.host))
                            .and_then(|h| edge(h, tp)),
                        avoid: key
                            .as_ref()
                            .and_then(|k| avoid.get(k))
                            .map_or(&[][..], Vec::as_slice),
                    },
                );
                if let (Some(key), Some((k, pts))) = (key, rec) {
                    hold_recs.push(HoldRec {
                        key,
                        k,
                        pts,
                        tp,
                        own: laid3.len()..laid3.len(),
                    });
                }
            }
            let own_from = laid3.len();
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
            laid3.extend(solids_of(
                &strips,
                &placed,
                thickness,
                s,
                short_poly.as_deref(),
            ));
            if let Some(r) = hold_recs.last_mut()
                && r.own.start == own_from
            {
                r.own = own_from..laid3.len();
            }
            for p in &placed {
                laid_cut.insert(strips[p.k].member.clone(), cut_of(&strips[p.k], p));
            }
            placed_all.extend(placed);
            continue;
        }
        direct_planes.push(*plane_key);
        laid3.extend(solids_of(&strips, &placed, 0.0, s, None));
        for p in &placed {
            laid_cut.insert(strips[p.k].member.clone(), cut_of(&strips[p.k], p));
        }
        placed_all.extend(placed);
    }
    // J2b: the bent plates of the ends leaving over the heel, the ends of
    // one plane sharing its flat.
    for (_, group) in bent_wait {
        match bent_over(
            &strips,
            &group,
            j,
            s,
            &placed_all,
            &laid_cut,
            &plate_of,
            &plan.plates,
        ) {
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
    for p in placed_all.iter().cloned() {
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
    // L10 (the owner, 2026-11-21: "here there are 2 bolts not used in the
    // laid out joint"): an end left as it is that bolts onto nothing here
    // but has bolts drawn (given, or its member's design data) — they join
    // nothing. The joint is then not laid out whole: that member by hand,
    // said (its splice or what it bolts onto given, the pieces confirmed
    // one part, or its bolts there taken out by hand).
    if s.nothing_joined && !plan.ends.is_empty() {
        for a in &j.arrivals {
            let n = a.bolt_points.len() + a.other_points.len();
            if n == 0
                || a.onto.value.is_some()
                || a.continuous.is_some()
                || except.contains_key(&a.member)
                || plan.hand_members.contains(&a.member)
                || plan.ends.iter().any(|e| e.member == a.member)
            {
                continue;
            }
            let whose = match a.bolts.source {
                Source::Given => "given at this end",
                _ => "its member's design data",
            };
            let legs = if a.other_points.is_empty() {
                String::new()
            } else {
                format!(", {} on each leg", a.bolt_points.len())
            };
            plan.by_hand(
                &a.member,
                format!(
                    "{}: {BOLTS_JOIN_NOTHING} — its {n} bolt(s) ({whose}{legs}) are drawn \
                     at this joint, but it bolts onto nothing here; by hand: what it bolts \
                     onto or its splice given, or the pieces in line confirmed one part",
                    a.member
                ),
            );
        }
    }
    // L20: a flat plate on a level leg holding level ends given its own
    // bolts onto the second member whose level leg it lies on too.
    if s.level_plates && !ctx.plain {
        level_holds(
            &mut plan,
            Laid {
                strips: &strips,
                placed: &placed_all,
                plate_of: &plate_of,
                pieces: &pieces,
                fixed: &fixed,
                laid3: &laid3,
                laid_cut: &laid_cut,
            },
            j,
            s,
            edge,
            (&others, except),
        );
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
            // (L20 item 2: such a plate may put it up to the most the
            // analysis takes farther out — the upper bound that far.)
            Some((
                m.clone(),
                if s.own_offset_analysed {
                    t.max(OWN_OUT_MAX)
                } else {
                    t
                },
            ))
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
    let (found, count) = if s.holds_clear && !hold_recs.is_empty() {
        hold_conflicts(&strips, &hold_recs, &laid3, &pieces, &fixed, j, &plan)
    } else {
        (HoldAvoid::new(), 0)
    };
    (plan, direct_planes, found, count)
}

/// What a plan laid out, for a pass after its planes ([`level_holds`]).
struct Laid<'a> {
    strips: &'a [Strip],
    placed: &'a [Placed],
    plate_of: &'a BTreeMap<usize, usize>,
    pieces: &'a [(crate::ClashPart, Vec<[f64; 3]>)],
    fixed: &'a BTreeSet<String>,
    laid3: &'a [Laid3],
    laid_cut: &'a BTreeMap<String, f64>,
}

/// The note of a level plate whose bolts onto its second member do not fit
/// (L20): the plate stays held by the first alone, as before.
pub const LEVEL_NOT_HELD: &str = "no level plate across two horizontals";

/// L20 (the level plate across two horizontals): each flat plate of `plan`
/// on a member's flange, held by its own bolts onto that member (L5o), with
/// a level end on it whose [`level_pair`] is that member and a second one —
/// that second member's level leg in the plate's plane **as drawn** (each
/// holder moved as the plan moves it) — given its own provisional bolts
/// onto the second ([`PlatePlan::also`]): placed on its flange as the first
/// holder's are ([`plate_holds`]: inside the plate grown over that flange,
/// clear of its holes, of the bolts and legs on the plate and of the steel
/// in the way, past that member's steel start by the end distance), as
/// many as onto the first, the plate grown round them clear of what it
/// would run into ([`plate_hits`]), of its heel line and of its straight
/// cuts. Where they do not fit, said ([`LEVEL_NOT_HELD`]); the plate stays
/// as before.
fn level_holds(
    plan: &mut JointPlan,
    l: Laid<'_>,
    j: &JointDetail,
    s: &LayoutSettings,
    edge: EdgeRule<'_>,
    who: (&[&str], &BTreeMap<String, String>),
) {
    let find = |m: &str| j.arrivals.iter().find(|a| a.member == m);
    let strips = l.strips;
    for pi in 0..plan.plates.len() {
        let pl = &plan.plates[pi];
        if pl.own.is_some()
            || pl.reach.is_some()
            || pl.between
            || pl.outline.is_some()
            || pl.also.is_some()
        {
            continue;
        }
        let Some(hold) = pl.hold.clone() else {
            continue;
        };
        if hold.member != pl.on {
            continue;
        }
        let flats: Vec<Placed> = l
            .placed
            .iter()
            .filter(|p| l.plate_of.get(&p.k) == Some(&pi) && strips[p.k].tilted.is_none())
            .cloned()
            .collect();
        let Some(x0) = flats.first().map(|p| &strips[p.k]) else {
            continue;
        };
        let Some(host) = find(&pl.on) else {
            continue;
        };
        let Some((lead, b)) = flats.iter().enumerate().find_map(|(i, p)| {
            let a = find(&strips[p.k].member)?;
            level_partner(&j.arrivals, a, host).map(|b| (i, b))
        }) else {
            continue;
        };
        let on = pl.on.clone();
        let tp = pl.thickness;
        let short = pl.short;
        let cuts = pl.cuts.clone();
        let heel = pl.heel;
        let mut members: BTreeSet<&str> = pl.members.iter().map(String::as_str).collect();
        members.insert(b.member.as_str());
        let why = |e: String| format!("{}: {LEVEL_NOT_HELD} — {e}", b.member);
        // (What the planes laid out, the plate itself left out: it grows.)
        let own_name = format!("a plate on {on}");
        let laid3: Vec<Laid3> = l
            .laid3
            .iter()
            .filter(|x| x.0 != own_name)
            .cloned()
            .collect();
        // Each holder as drawn: moved across and lifted as the plan lays
        // its own end out (a member not laid out here: as it is).
        let moved = |m: &str| -> Option<[f64; 3]> {
            let Some(hk) = strips.iter().position(|y| y.member == m) else {
                return Some([0.0; 3]);
            };
            let Some(hp) = l.placed.iter().find(|q| q.k == hk) else {
                return Some([0.0; 3]);
            };
            let h = &strips[hk];
            if h.tilted.is_some() {
                return None;
            }
            let v3 = add(scale(h.x3, h.v[0]), scale(h.y3, h.v[1]));
            let lift = match l.plate_of.get(&hk) {
                Some(&i) if plan.plates[i].own.is_none() => plan.plates[i].thickness,
                _ => 0.0,
            };
            Some(add(scale(v3, hp.moved), scale(h.n3, lift)))
        };
        let (Some(da), Some(db)) = (moved(&on), moved(&b.member)) else {
            plan.notes.push(why(format!(
                "{on} or {} lies on a plate in its own plane",
                b.member
            )));
            continue;
        };
        let Some(lead_a) = find(&strips[flats[lead].k].member) else {
            continue;
        };
        let (Some(ha), Some(fa), Some(fb)) = (
            find(&on),
            find(&on).and_then(|h| crate::joints::host_flange_along(lead_a, h, false)),
            crate::joints::host_flange_along(lead_a, b, false),
        ) else {
            continue;
        };
        let _ = ha;
        let step = dot(sub(add(fb.heel, db), add(fa.heel, da)), fa.out);
        if dot(fa.out, fb.out) < FLAT_OFF.to_radians().cos() - 1e-12
            || step.abs() > crate::clash::CLASH_TOLERANCE + 1e-9
        {
            plan.notes.push(why(format!(
                "as laid out, {}'s level leg lies {:.0} mm out of the plate's plane on {on}",
                b.member,
                step.abs() * 1000.0
            )));
            continue;
        }
        // The plate's ends lying on the second member's flange (one plane
        // with the first's), in its axes; each where the plan places it.
        let mut sb: Vec<Strip> = Vec::new();
        let mut pb: Vec<Placed> = Vec::new();
        let mut failed: Option<String> = None;
        for p in &flats {
            let x = &strips[p.k];
            let Some(e) = find(&x.member) else {
                failed = Some(format!("{} not at the joint", x.member));
                break;
            };
            let mut y = match strip_of(j, e, b, s, edge, who, None, false, None) {
                Ok(y) => y,
                Err(w) => {
                    failed = Some(format!("{} on {}'s flange: {w}", x.member, b.member));
                    break;
                }
            };
            let dp = [dot(db, y.x3), dot(db, y.y3)];
            y.heel3 = add(y.heel3, db);
            y.face3 = add(y.face3, db);
            y.heel = add(y.heel, db);
            y.w = sub2(y.w, dp);
            for (c, _) in y.obstacles.iter_mut() {
                *c = sub2(*c, dp);
            }
            for (c, _, _) in y.partners.iter_mut() {
                *c = sub2(*c, dp);
            }
            let va = add(scale(x.x3, x.v[0]), scale(x.y3, x.v[1]));
            let vb = add(scale(y.x3, y.v[0]), scale(y.y3, y.v[1]));
            pb.push(Placed {
                k: sb.len(),
                cut: p.cut,
                moved: p.moved * dot(va, vb),
            });
            sb.push(y);
        }
        if let Some(w) = failed {
            plan.notes.push(why(w));
            continue;
        }
        {
            let y = &mut sb[lead];
            y.fixed = fixed_in(y, j, l.pieces, l.fixed, &laid3);
            let thick = y.own_t.max(y.t);
            let far = thick + (BOLT_DRAWN[2] + BOLT_DRAWN[3]) * y.d;
            y.fixed_far = fixed_slab(y, j, l.pieces, l.fixed, &laid3, (-far, -FACE_TOUCH));
        }
        let xb = sb[lead].clone();
        // Where the second member's steel starts (its cut as laid out).
        let from = xb
            .host_ends
            .then(|| l.laid_cut.get(&b.member).copied().unwrap_or(xb.host_cut));
        let [_, spacing, _] = s.defaults.of(xb.d);
        let keep = flats
            .iter()
            .map(|p| strips[p.k].keep_plate.0.max(strips[p.k].keep_plate.1))
            .fold(x0.keep_plate.0.max(x0.keep_plate.1), f64::max);
        // The plate as planned, and the second member's flange beside it.
        let poly_a = plate_outline2(
            strips,
            &flats,
            s.gap,
            if short {
                Outline::Short
            } else {
                Outline::Round
            },
        );
        let a3 = |q: [f64; 2]| add(x0.heel3, add(scale(x0.x3, q[0]), scale(x0.y3, q[1])));
        let b3 = |q: [f64; 2]| add(xb.heel3, add(scale(xb.x3, q[0]), scale(xb.y3, q[1])));
        let to_b = |q: [f64; 3]| {
            let d = sub(q, xb.heel3);
            [dot(d, xb.x3), dot(d, xb.y3)]
        };
        let to_a = |q: [f64; 3]| {
            let d = sub(q, x0.heel3);
            [dot(d, x0.x3), dot(d, x0.y3)]
        };
        let start = from.unwrap_or(0.0) + xb.host_end;
        let reach = 4.0 * spacing + 2.0 * keep;
        let mut pts: Vec<[f64; 2]> = poly_a.iter().map(|q| to_b(a3(*q))).collect();
        for along in [start - keep, start + reach] {
            let xq = xb.w[0] + along / xb.host_along;
            pts.push([xq, 0.0]);
            pts.push([xq, xb.width]);
        }
        let poly_b = crate::joint_sheet::hull(pts);
        let mut holes = leg_holes(
            strips,
            l.placed,
            &flats,
            j,
            &xb,
            xb.heel3,
            (xb.n3, xb.y3),
            xb.t + tp,
        );
        for p in &pb {
            let y = &sb[p.k];
            for (along, line) in &y.bolts {
                let g = y.gauges.get(*line).copied().unwrap_or(0.0) + p.moved;
                holes.push(y.at(p.cut + along, g));
            }
        }
        let room = HoldRoom {
            holes,
            ends: pb.iter().collect(),
            seams: Vec::new(),
            poly: &poly_b,
            through: BTreeSet::new(),
            // (L22: the bolts onto the second holder not yet kept clear of
            // what the plan lays out after the plate — the check after
            // writing tests them.)
            avoid: Vec::new(),
        };
        let n = hold.points.len().max(2);
        let got = match plate_holds(&sb, &xb, &room, s, from, n, 0.0) {
            Ok(q) => q,
            Err(e) => {
                plan.notes.push(why(e));
                continue;
            }
        };
        // The plate grown round them (each with its distances, the flange
        // under them), as the joint details will draw it: clear of what it
        // would run into, of its heel line and of its straight cuts.
        const N: usize = 16;
        let grow = 1.0 / (std::f64::consts::PI / N as f64).cos();
        let mut all = poly_a.clone();
        let (lo, hi) = got
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), q| {
                (lo.min(q[0]), hi.max(q[0]))
            });
        for xq in [lo - keep, hi + keep] {
            all.push(to_a(b3([xq, 0.0])));
            all.push(to_a(b3([xq, xb.width])));
        }
        for h in &hold.points {
            let c = [x0.w[0] + h[0] / x0.host_along, h[1]];
            for i in 0..N {
                let t = i as f64 * std::f64::consts::TAU / N as f64;
                all.push([c[0] + keep * grow * t.cos(), c[1] + keep * grow * t.sin()]);
            }
        }
        let mut deep_ok = true;
        for q in &got {
            for i in 0..N {
                let t = i as f64 * std::f64::consts::TAU / N as f64;
                all.push(to_a(b3([
                    q[0] + keep * grow * t.cos(),
                    q[1] + keep * grow * t.sin(),
                ])));
            }
            let q3 = add(j.position, b3(*q));
            if let Some((h, across)) = heel
                && dot(sub(q3, h), across) < s.gap / 2.0 + keep - 1e-9
            {
                deep_ok = false;
            }
            if cuts
                .iter()
                .any(|(at, nn)| -dot(sub(q3, *at), *nn) < keep - 1e-9)
            {
                deep_ok = false;
            }
        }
        if !deep_ok {
            plan.notes.push(why(format!(
                "its bolts onto {} would lie beyond the plate's heel line or a straight cut",
                b.member
            )));
            continue;
        }
        let mut grown = crate::joint_sheet::hull(all);
        for (at, nn) in &cuts {
            grown = clip(&grown, |q| -dot(sub(add(j.position, a3(q)), *at), *nn));
        }
        let excused: BTreeSet<String> = plan.hand_members.iter().cloned().collect();
        if let Some((by, _)) = plate_hits(
            x0, &members, j, l.pieces, l.fixed, &excused, &laid3, &grown, tp,
        ) {
            plan.notes.push(why(format!(
                "grown over {}'s flange the plate would run into {by}",
                b.member
            )));
            continue;
        }
        // The flange named (`long` or `short`, as `hold_flat` names it).
        let leg = b
            .profile
            .as_ref()
            .and_then(|p| p.angle.map(|g| (p.axes, g)))
            .map(|(axes, g)| {
                let along_y = dot(xb.y3, axes[1]).abs() > 0.5;
                if along_y == (g.b >= g.h) {
                    "long"
                } else {
                    "short"
                }
            });
        let m = got.len();
        let d = xb.d;
        let from_words = format!("{m} bolt(s) of {:.0} mm", d * 1000.0);
        plan.notes.push(format!(
            "{}: {m} provisional bolt(s) holding a level plate on {on} too ({from_words}; a level              plate across two horizontals, L20)",
            b.member
        ));
        let also = HoldPlan {
            member: b.member.clone(),
            leg,
            bolt: BoltArea {
                count: m as u32,
                from: from_words,
                ..hold.bolt.clone()
            },
            provisional: true,
            points: got
                .iter()
                .map(|q| {
                    [
                        round_to((q[0] - xb.w[0]) * xb.host_along, 1e-4),
                        round_to(q[1], 1e-4),
                    ]
                })
                .collect(),
        };
        plan.plates[pi].also = Some(also);
    }
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
/// part first, then by class (leg, bracing, redundant), the wider, the
/// piece on `a`'s side (J9, 2026-11-19), the id.
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
    hosts_to_name_by(j, &LayoutSettings::default())
}

/// [`hosts_to_name`] with the settings `s`: with
/// [`LayoutSettings::plane_hosts`] (L13), an end whose axis lies in
/// neither flange's plane of today's choice nor of a member running
/// through is named onto a member ending at the joint in a flange's plane
/// of which its axis lies (`host_ending_in_plane`), before today's
/// choice.
pub fn hosts_to_name_by(j: &JointDetail, s: &LayoutSettings) -> Vec<(String, String)> {
    named_hosts(j, s.plane_hosts, s.level_plates)
        .into_iter()
        .map(|(m, h, _)| (m, h))
        .collect()
}

/// [`hosts_to_name_by`]'s names, each with whether it is L13's (a member
/// ending at the joint, chosen by the plane the end's axis lies in) or
/// L20's (`level`: the first of a level plate's two holders,
/// [`level_pair`]) — both weighed alike.
pub(crate) fn named_hosts(
    j: &JointDetail,
    plane_hosts: bool,
    level: bool,
) -> Vec<(String, String, bool)> {
    let first = named_hosts_except(j, plane_hosts, level, &BTreeSet::new());
    // (No chain of such names: an end another is named onto by its plane
    // keeps today's choice — measured on the private real towers, renaming
    // both lost more joints than it gained.)
    let hosts: BTreeSet<String> = first.iter().filter(|x| x.2).map(|x| x.1.clone()).collect();
    if !first.iter().any(|x| x.2 && hosts.contains(&x.0)) {
        return first;
    }
    named_hosts_except(j, plane_hosts, level, &hosts)
}

/// [`named_hosts`], the ends `keep` never named by their plane.
fn named_hosts_except(
    j: &JointDetail,
    plane_hosts: bool,
    level_plates: bool,
    keep: &BTreeSet<String>,
) -> Vec<(String, String, bool)> {
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
                    .then(|| (a.member.clone(), h.member.clone(), false));
            }
            match host_in_plane(&j.arrivals, a) {
                Some(h) if h0.is_none_or(|x| x.member != h.member) => {
                    Some((a.member.clone(), h.member.clone(), false))
                }
                _ => {
                    // (Not an end in line with a member at the joint: the
                    // plan leaves such an end as it is.)
                    let in_line = j.arrivals.iter().any(|c| {
                        c.member != a.member && angle_deg(a.direction, c.direction) > 160.0
                    });
                    // (Not an end lying within a few degrees of today's
                    // choice's flange: a bent plate lays most of those out.)
                    let near = h0.is_some_and(|h| plane_off(a, h) <= PLANE_HOST_OFF);
                    // (Where nothing runs through the joint, only a plan or
                    // hip member: the base of such a joint waits for the
                    // owner's decision on its own bolts — measured on the
                    // private real towers, the face members named there made
                    // more joints worse than better.)
                    let level =
                        a.direction[2].abs() <= crate::joints::HORIZONTAL_OFF.to_radians().sin();
                    let through_here = j.arrivals.iter().any(|b| {
                        b.continuous.is_some()
                            || j.arrivals.iter().any(|c| {
                                c.member != b.member && angle_deg(b.direction, c.direction) > 160.0
                            })
                    });
                    if plane_hosts
                        && !in_line
                        && !near
                        && (through_here || level)
                        && !keep.contains(&a.member)
                        && let Some(h) = host_ending_in_plane(&j.arrivals, a)
                    {
                        return Some((a.member.clone(), h.member.clone(), true));
                    }
                    // (L20) A level end lying on no flange of today's
                    // choice nor of a member running through, none ending
                    // here named for it by L13, two level angles ending
                    // here with their level legs in its plane: onto the
                    // first, a level plate held by both (the plan gives the
                    // second its bolts). Only here: measured on the private
                    // real towers, named before a member running through it
                    // took ends from joints laid out whole.
                    if level_plates
                        && level
                        && !in_line
                        && !near
                        && !keep.contains(&a.member)
                        && let Some((h, _)) = level_pair(&j.arrivals, a)
                    {
                        return Some((a.member.clone(), h.member.clone(), true));
                    }
                    match (a.onto.value.is_none(), h0) {
                        (true, Some(h)) => Some((a.member.clone(), h.member.clone(), false)),
                        _ => None,
                    }
                }
            }
        })
        .collect()
}

/// L20: the second holder of a level plate for end `a` on the flange of
/// `first` in whose plane `a`'s axis lies (the member the plate is `on`,
/// ending at the joint or running through it): `a` level, a single angle;
/// another level single angle at the joint, not `a`, not `first` nor in
/// line with either, not bolting onto `a`, 20° to 160° from both, with a
/// flange in whose plane `a`'s axis lies, its outer face in one plane with
/// `first`'s (normals within [`FLAT_OFF`], faces within the clash check's
/// tolerance at the joint, as given) — by class, the wider, the id; none:
/// no such member.
pub(crate) fn level_partner<'a>(
    all: &'a [Arrival],
    a: &Arrival,
    first: &Arrival,
) -> Option<&'a Arrival> {
    let level_sin = crate::joints::HORIZONTAL_OFF.to_radians().sin();
    let single = |b: &Arrival| {
        b.profile
            .as_ref()
            .is_some_and(|p| p.angle.is_some() && p.parts.is_empty())
    };
    if a.direction[2].abs() > level_sin
        || first.direction[2].abs() > level_sin
        || !single(a)
        || !single(first)
        || a.continuous.is_some()
    {
        return None;
    }
    let sin_flat = FLAT_OFF.to_radians().sin();
    let cos_flat = FLAT_OFF.to_radians().cos();
    let fa = crate::joints::host_flange_along(a, first, false)?;
    if dot(a.direction, fa.out).abs() > sin_flat {
        return None;
    }
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
        .filter(|b| b.member != a.member && b.member != first.member && single(b))
        .filter(|b| b.direction[2].abs() <= level_sin)
        .filter(|b| {
            (20.0..=160.0).contains(&angle_deg(a.direction, b.direction))
                && (20.0..=160.0).contains(&angle_deg(first.direction, b.direction))
        })
        .filter(|b| b.onto.value.as_deref() != Some(a.member.as_str()))
        // (Not an end itself bolting onto the first holder or a piece in
        // line with it — an end on the plate's face, not a holder —, nor
        // one the first bolts onto.)
        .filter(|b| {
            let onto_first = b
                .onto
                .value
                .as_deref()
                .and_then(|o| all.iter().find(|x| x.member == o))
                .is_some_and(|x| {
                    x.member == first.member || angle_deg(x.direction, first.direction) > 160.0
                });
            !onto_first && first.onto.value.as_deref() != Some(b.member.as_str())
        })
        .filter(|b| {
            crate::joints::host_flange_along(a, b, false).is_some_and(|f| {
                dot(a.direction, f.out).abs() <= sin_flat
                    && dot(fa.out, f.out) >= cos_flat - 1e-12
                    && dot(sub(f.heel, fa.heel), fa.out).abs()
                        <= crate::clash::CLASH_TOLERANCE + 1e-9
            })
        })
        .min_by(|x, y| (rank(x), width(x), &x.member).cmp(&(rank(y), width(y), &y.member)))
}

/// L20: the two holders of a **level plate** for end `a` — `a` level
/// (within [`crate::joints::HORIZONTAL_OFF`]), a single angle; two other
/// single angles at the joint, each level, **ending** there (not one part
/// with a member, not in line with another of its members), not in line
/// with `a` nor with each other (20° to 160°), neither bolting onto `a`,
/// each with a flange in whose plane `a`'s axis lies ([`FLAT_OFF`]), those
/// two flanges' outer faces in one plane (their normals within
/// [`FLAT_OFF`], their faces within the clash check's tolerance of each
/// other at the joint, as given). The pair by the members' class, then the
/// wider, the id (the first holder first, the plate is `on` it); none: no
/// such pair. Where the plan moves either member, its plate checks that
/// plane again as drawn.
pub(crate) fn level_pair<'a>(
    all: &'a [Arrival],
    a: &Arrival,
) -> Option<(&'a Arrival, &'a Arrival)> {
    let level_sin = crate::joints::HORIZONTAL_OFF.to_radians().sin();
    let single = |b: &Arrival| {
        b.profile
            .as_ref()
            .is_some_and(|p| p.angle.is_some() && p.parts.is_empty())
    };
    if a.direction[2].abs() > level_sin || !single(a) || a.continuous.is_some() {
        return None;
    }
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
    let runs = |b: &Arrival| {
        b.continuous.is_some()
            || all.iter().any(|c| {
                c.member != b.member
                    && (c.continuous.as_deref() == Some(b.member.as_str())
                        || angle_deg(b.direction, c.direction) > 160.0)
            })
    };
    let sin_flat = FLAT_OFF.to_radians().sin();
    let mut cands: Vec<(&Arrival, crate::joints::HostFlange)> = all
        .iter()
        .filter(|b| b.member != a.member && single(b) && !runs(b))
        .filter(|b| b.direction[2].abs() <= level_sin)
        .filter(|b| (20.0..=160.0).contains(&angle_deg(a.direction, b.direction)))
        .filter(|b| b.onto.value.as_deref() != Some(a.member.as_str()))
        .filter_map(|b| {
            let f = crate::joints::host_flange_along(a, b, false)?;
            (dot(a.direction, f.out).abs() <= sin_flat).then_some((b, f))
        })
        .collect();
    cands.sort_by(|x, y| {
        (rank(x.0), width(x.0), &x.0.member).cmp(&(rank(y.0), width(y.0), &y.0.member))
    });
    let cos_flat = FLAT_OFF.to_radians().cos();
    for (i, (x, fx)) in cands.iter().enumerate() {
        for (y, fy) in &cands[i + 1..] {
            if !(20.0..=160.0).contains(&angle_deg(x.direction, y.direction))
                || x.onto.value.as_deref() == Some(y.member.as_str())
                || y.onto.value.as_deref() == Some(x.member.as_str())
            {
                continue;
            }
            if dot(fx.out, fy.out) < cos_flat - 1e-12 {
                continue;
            }
            if dot(sub(fy.heel, fx.heel), fx.out).abs() > crate::clash::CLASH_TOLERANCE + 1e-9 {
                continue;
            }
            return Some((x, y));
        }
    }
    None
}

/// L13: an end lying within this of a flange's plane of today's choice
/// for it (degrees) keeps that choice — a bent plate by a few degrees lays
/// many of those out (measured on the 78 private real towers: named onto
/// a member ending at the joint, the ends 5° to 10° out made 250 joints
/// worse and 137 better; those farther out 1 to 4 times more better than
/// worse, those with no choice today none worse). The program's value, not
/// a standard's.
pub const PLANE_HOST_OFF: f64 = 10.0;

/// How far `a`'s axis runs out of the nearer of `h`'s flanges' planes
/// (an angle's, or a built-up section's angles' along its axes), degrees;
/// 90 for a member of another section.
fn plane_off(a: &Arrival, h: &Arrival) -> f64 {
    match h.profile.as_ref() {
        Some(p) if p.angle.is_some() || !p.parts.is_empty() => dot(a.direction, p.axes[1])
            .abs()
            .min(dot(a.direction, p.axes[2]).abs())
            .min(1.0)
            .asin()
            .to_degrees(),
        _ => 90.0,
    }
}

/// L13 (2026-11-22): of the single angles **ending** at the joint (not
/// one part with a member there, not running through it in line with
/// another of its members) and not in line with `a`, one in a flange's
/// plane of which `a`'s axis lies ([`FLAT_OFF`]): not bolting onto `a`,
/// of a class at least `a`'s (leg, bracing, redundant), as wide as `a`
/// where of its class (the id deciding between equals: never two ends
/// named onto each other) — the class first, then the wider, the id. A
/// plan or hip member at a leg finds a face horizontal ending there whose
/// outstanding leg it lies on; none: today's choice stays.
fn host_ending_in_plane<'a>(all: &'a [Arrival], a: &Arrival) -> Option<&'a Arrival> {
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
    let key = |b: &'a Arrival| (rank(b), width(b), b.member.as_str());
    // Its leg bolted already flat on the flange whose plane holds its axis
    // (no turn needed: its member's other ends keep theirs) first.
    let flat_now = |b: &Arrival| -> bool {
        let (Some(pa), Some(pb)) = (a.profile.as_ref(), b.profile.as_ref()) else {
            return false;
        };
        let Some(g) = pa.angle else {
            return false;
        };
        let s = FLAT_OFF.to_radians().sin();
        let c = FLAT_OFF.to_radians().cos();
        let either = (g.b - g.h).abs() < 1e-9
            && !matches!(a.leg.value, Some(ohl_structures::connections::BoltedLeg::Long | ohl_structures::connections::BoltedLeg::Short) if a.leg.known());
        let legs: Vec<[f64; 3]> = if either {
            vec![pa.axes[1], pa.axes[2]]
        } else if a.leg_y {
            vec![pa.axes[2]]
        } else {
            vec![pa.axes[1]]
        };
        [pb.axes[1], pb.axes[2]]
            .into_iter()
            .filter(|n| dot(a.direction, *n).abs() < s)
            .any(|n| legs.iter().any(|l| dot(*l, n).abs() > c))
    };
    let runs = |b: &Arrival| {
        b.continuous.is_some()
            || all
                .iter()
                .any(|c| c.member != b.member && angle_deg(b.direction, c.direction) > 160.0)
    };
    let own = (rank(a), width(a), a.member.as_str());
    all.iter()
        .filter(|b| b.member != a.member)
        .filter(|b| {
            b.profile
                .as_ref()
                .is_some_and(|p| p.angle.is_some() && p.parts.is_empty())
        })
        .filter(|b| (20.0..=160.0).contains(&angle_deg(a.direction, b.direction)))
        .filter(|b| !runs(b))
        .filter(|b| {
            b.onto.value.as_deref() != Some(a.member.as_str())
                && b.continuous.as_deref() != Some(a.member.as_str())
        })
        .filter(|b| key(b) < own)
        .filter(|b| in_flange_plane(a, b))
        .min_by(|x, y| (!flat_now(x), key(x)).cmp(&(!flat_now(y), key(y))))
}

/// The members end `a` at joint `j` could bolt onto (L1 d, owner
/// 2026-11-19), in today's rule's order ([`through_of`]: those running
/// through the joint first, then the one part, by class, the wider, the
/// piece on `a`'s side, the id): the single angles at the joint not in
/// line with it (20° to 160°), in a flange's plane of which its axis lies
/// ([`FLAT_OFF`]), not bolting onto it or running on into it.
pub(crate) fn onto_candidates(j: &JointDetail, a: &Arrival) -> Vec<String> {
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
    let runs = |b: &Arrival| {
        b.continuous.is_some()
            || j.arrivals
                .iter()
                .any(|c| c.member != b.member && angle_deg(b.direction, c.direction) > 160.0)
    };
    let mut out: Vec<&Arrival> = j
        .arrivals
        .iter()
        .filter(|b| b.member != a.member)
        .filter(|b| {
            b.profile
                .as_ref()
                .is_some_and(|p| p.angle.is_some() && p.parts.is_empty())
        })
        .filter(|b| (20.0..=160.0).contains(&angle_deg(a.direction, b.direction)))
        .filter(|b| {
            b.onto.value.as_deref() != Some(a.member.as_str())
                && b.continuous.as_deref() != Some(a.member.as_str())
        })
        .filter(|b| in_flange_plane(a, b))
        .collect();
    out.sort_by(|x, y| {
        (
            !runs(x),
            x.continuous.is_none(),
            rank(x),
            width(x),
            !same_side(a, x),
            &x.member,
        )
            .cmp(&(
                !runs(y),
                y.continuous.is_none(),
                rank(y),
                width(y),
                !same_side(a, y),
                &y.member,
            ))
    });
    out.into_iter().map(|b| b.member.clone()).collect()
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
            (
                x.continuous.is_none(),
                rank(x),
                width(x),
                !same_side(a, x),
                &x.member,
            )
                .cmp(&(
                    y.continuous.is_none(),
                    rank(y),
                    width(y),
                    !same_side(a, y),
                    &y.member,
                ))
        })
}

/// `b` leaves the joint on the side `a` does (of two pieces in line, the
/// one whose steel lies under `a`'s bolts; J9, 2026-11-19: pieces not one
/// part end at the joint).
fn same_side(a: &Arrival, b: &Arrival) -> bool {
    dot(a.direction, b.direction) > 0.05
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
             bolts may be their members' design data, at both ends) — drawn as given, lapped, \
             each ending at the joint; confirm the layout's leg splice places (one part here, \
             the same section), or give the splice here",
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
/// - else not joined (a splice of other sections: proposed by
///   [`crate::leg_splices::leg_splices`] for the user to confirm, O44 J7;
///   else covers or a plate by hand).
///
/// Only ends with nothing given of how they are joined (`kind`, `face`,
/// `onto`, bolts given at the end), as the run takes them out first.
///
/// Two angles of one section in line, turned alike, with bolts (a splice,
/// 2026-11-19): not joined — the bolts of a member's design data are at
/// both its ends, so where the splice is (here, or the legs one part
/// through this joint and spliced elsewhere) is not known; said
/// ([`splice_unknown`]), drawn as before — the leg splice places proposed
/// (O44 J7) settle it when confirmed.
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
            // (Step 1c, other sections in line spliced with covers, is no
            // longer made here: since O44 J7 (2026-11-19) a splice is
            // proposed and written only when the user confirms it —
            // [`crate::leg_splices::leg_splices`].)
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
            (
                x.continuous.is_none(),
                rank(x),
                width(x),
                !same_side(a, x),
                &x.member,
            )
                .cmp(&(
                    y.continuous.is_none(),
                    rank(y),
                    width(y),
                    !same_side(a, y),
                    &y.member,
                ))
        })
}

/// The angle between two directions, degrees.
/// The angle between two directions, degrees.
pub(crate) fn angle_between_deg(a: [f64; 3], b: [f64; 3]) -> f64 {
    angle_deg(a, b)
}

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

/// How end `a` at `j` would lie with its member turned by `turn` more
/// (radians, about its axis; L3b, 2026-11-20): its leg bolted's angle off
/// the flange it bolts onto (degrees; the leg lying flatter where either
/// leg may be bolted, `either`) and whether a plate bent by as much from
/// the flat plate on that flange can take it — its outstanding leg away
/// from the flange (as [`strip_of`] reads it for an end off its flange, on
/// either of the flange's two faces it may come to). None: it bolts onto
/// no single angle drawn here (onto a plate, nothing named, or not an
/// angle). A prediction from the geometry, for choosing a member's turn;
/// the plan decides.
pub(crate) fn turned_end(
    j: &JointDetail,
    a: &Arrival,
    turn: f64,
    either: bool,
) -> Option<(f64, bool)> {
    let p = a.profile.as_ref()?;
    p.angle?;
    let host = j
        .arrivals
        .iter()
        .find(|b| a.onto.value.as_deref() == Some(b.member.as_str()))?;
    let q = host.profile.as_ref()?;
    if q.angle.is_none() || !q.parts.is_empty() {
        return None;
    }
    let [x, y, z] = p.axes;
    let (c, s) = (turn.cos(), turn.sin());
    let y2 = add(scale(y, c), scale(z, s));
    let z2 = add(scale(y, -s), scale(z, c));
    let axes = [x, y2, z2];
    // The leg bolted: as given, else (either leg) the one lying flatter
    // on the member it bolts onto (as the joint details take it).
    let leg_y = if either {
        let flat = |n: [f64; 3]| dot(n, q.axes[1]).abs().max(dot(n, q.axes[2]).abs());
        flat(z2) >= flat(y2)
    } else {
        a.leg_y
    };
    // How far off: on the flange whose face runs most nearly along it (as
    // the joint details' `flat_of`).
    let across = |n: [f64; 3]| (dot(n, x).abs() * 1e3).round();
    let off = [q.axes[1], q.axes[2]]
        .into_iter()
        .filter_map(|n| crate::welds::flat_roll(axes, leg_y, n).map(|f| (across(n), f.off)))
        .min_by(|u, v| u.0.total_cmp(&v.0).then(u.1.total_cmp(&v.1)))?
        .1;
    if off <= FLAT_OFF {
        return Some((off, true));
    }
    let mut turned = a.clone();
    if let Some(pp) = turned.profile.as_mut() {
        pp.axes = axes;
    }
    turned.leg_y = leg_y;
    let nb = if leg_y { z2 } else { y2 };
    let sin_flat = FLAT_OFF.to_radians().sin();
    let takes = [false, true].into_iter().any(|other| {
        host_flange_along(&turned, host, other).is_some_and(|f| {
            let behind = dot(a.direction, f.out);
            let inside = behind < -sin_flat || (behind <= sin_flat && dot(nb, f.out) < 0.0);
            let face_out = if inside { scale(f.out, -1.0) } else { f.out };
            dot(nb, face_out) >= TILT_MAX.to_radians().cos()
        })
    });
    Some((off, takes))
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
    /// lying horizontal within [`crate::joints::HORIZONTAL_OFF`]), and
    /// members of no known class (said).
    Secondary = 2,
    /// The redundant members (class `redundant`), last.
    Redundant = 3,
}

/// A member's place in the owner's order of the classes from its `class`
/// and its direction (unit): bracing lying horizontal (within
/// [`crate::joints::HORIZONTAL_OFF`]) is secondary, the rest of it a
/// diagonal.
pub fn class_rank(class: Option<&str>, direction: [f64; 3]) -> ClassRank {
    match class {
        Some("leg") => ClassRank::Leg,
        Some("redundant") => ClassRank::Redundant,
        Some("bracing") | Some("crossing")
            if direction[2].abs() > crate::joints::HORIZONTAL_OFF.to_radians().sin() =>
        {
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
    // The members not laid out here, and of those the ones left for a hand
    // detail by the run's rounds (their hand detail places their bolts).
    (others, except): (&[&str], &BTreeMap<String, String>),
    tilt: Option<f64>,
    other: bool,
    // (L15) Across a gap between built-up `host`'s angles: the back it lies
    // on, the gap and whether the other angle's leg lies straight across.
    between: Option<(crate::joints::HostFlange, f64, bool)>,
) -> Result<Strip, String> {
    let p = a.profile.as_ref().expect("checked");
    let g = p.angle.expect("checked");
    // Off the flange (step 4): the flange in whose plane its axis lies
    // (else the other).
    let f = match (tilt, &between) {
        (None, Some((f, _, _))) => Some(*f),
        (Some(_), _) => host_flange_along(a, host, other),
        (None, None) => host_flange(a, host),
    }
    .ok_or("no flange to lie on")?;
    // On the flange's inner face: as it sits; off the flange (step 4),
    // when it comes from the flange's other side (its axis behind the outer
    // face, or along it with its leg bolted facing away from it).
    let nb = if a.leg_y { p.axes[2] } else { p.axes[1] };
    let behind = dot(a.direction, f.out);
    let sin_flat = FLAT_OFF.to_radians().sin();
    // (L16) The face as the joint details seat it on this host
    // ([`crate::joints::seat_face`]; before: as it sat on what it bolted
    // onto while planning — for an end naming nothing, the joint's leg
    // worked out, not always this host).
    let inside = match tilt {
        None if between.is_some() => false,
        None if s.seat_as_drawn => {
            crate::joints::seat_face(a, host).or(a.sits.value) == Some(Face::Inside)
        }
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
    let edge_rule = edge;
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
    // L8 (L7's proposal 4): the members left as they are bolting onto this
    // end — their bolts kept on its steel, not kept clear of (an end off
    // the flange or leaving over the heel: as before, the check after
    // writing judges them; a member left for a hand detail by the run's
    // rounds: not asked, as after writing).
    let mut partners = Vec::new();
    for b in &j.arrivals {
        if b.member == a.member || !others.contains(&b.member.as_str()) {
            continue;
        }
        let Some(bs) = &b.bolts.value else { continue };
        if s.partner_bolts
            && !except.contains_key(&b.member)
            && tilt.is_none()
            && !leaves_over
            && b.continuous.is_none()
            && b.onto.value.as_deref() == Some(a.member.as_str())
            && crate::joints::host_flange(b, a).is_some()
        {
            let hole = bs.hole.unwrap_or(bs.diameter);
            let end = edge_rule(b, g.t).map_or(s.defaults.of(bs.diameter)[0], |e| e.1);
            for q in &b.bolt_points {
                partners.push((to2(sub(*q, j.position)), hole, end));
            }
            continue;
        }
        let r = bs.drawn[0].unwrap_or(BOLT_DRAWN[0] * bs.diameter) / 3f64.sqrt();
        for (q, axis, _) in b.bolts_with_axes() {
            // (The bolts are where they are in the structure's axes; L8,
            // 2026-11-21, found measuring: before, the joint's position was
            // added once more — the bolts already there were missed unless
            // the face passed near the structure's origin.)
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
                band: (f.across, f.width),
                back: s.bend_back && s.small_bends,
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
            // At most a plate's thickness (its own leg's) farther out of
            // the flange than on a flat plate: else a hand detail at once
            // (its face's other ends then laid out as before). L3: each
            // plate it can take (bent away from or towards the flange,
            // welded); its footprint covers them all.
            let mut errs: Vec<(Room, String)> = Vec::new();
            let mut fit = false;
            for room in rooms_for(&t, s) {
                match own_geometry(&t, 0.0, 0.0, t.face, t.out, g.t, s.gap, room) {
                    Ok(geo) if within(room, geo.out_more, g.t, s) => {
                        fit = true;
                        for c in &geo.outline {
                            lo = lo.min(l(*c));
                            hi = hi.max(l(*c));
                        }
                    }
                    Ok(geo) => errs.push((room, farther_out(geo.out_more, g.t, room, s))),
                    Err(e) => errs.push((room, e)),
                }
            }
            if !fit {
                return Err(rooms_words(&errs));
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
    // O44 J7: on a spliced member's outer face where the splice's outer
    // cover lies — on the cover.
    let through = if inside || tilt.is_some() || between.is_some() {
        None
    } else {
        outer_cover(j, a, host, &f)
    };
    let lift = through.as_ref().map_or(0.0, |c| c.1);
    let off = off + lift;
    Ok(Strip {
        face3: if inside {
            add(f.heel, scale(f.out, -f.t))
        } else {
            add(f.heel, scale(f.out, lift))
        },
        n3: face_out,
        x3,
        y3,
        heel3: f.heel,
        heel_lat: lat0,
        out_w: other_w,
        slab: g.t + g.t.max(host_t) + 0.7 * d,
        fixed: Vec::new(),
        fixed_out: Vec::new(),
        fixed_far: Vec::new(),
        out_band: {
            // The outstanding leg at the heel's edge, its thickness inwards
            // (as `solids_of` draws it).
            let inward = if (lat0 - lat[0]).abs() <= (lat0 - lat[1]).abs() {
                g.t
            } else {
                -g.t
            };
            [lat0.min(lat0 + inward), lat0.max(lat0 + inward)]
        },
        keep_plate: edge_end(g.t),
        floor: None,
        host_rank: class_rank(host.class.as_deref(), host.direction) as u8,
        over,
        tilted,
        inside,
        obstacles,
        partners,
        own_ends: !j
            .arrivals
            .iter()
            .any(|b| b.continuous.as_deref() == Some(a.member.as_str())),
        heel: add(j.position, f.heel),
        across: f.across,
        member: a.member.clone(),
        // (L15) A plane of its own across a gap (never shared with ends
        // lying on that back otherwise). (L17) One plate in one gap: the
        // key the gap's own — its normal turned one way, its first back
        // along it — so that the ends across it from either side (and those on a
        // member one part with it through the joint) share the plate, each
        // on its own face, rather than two plates in the gap.
        plane: match &between {
            Some((_, gap, _)) => between_key(n, off, *gap),
            None => (mm(n[0] * 1.0), mm(n[1] * 1.0), mm(n[2] * 1.0), mm(off)),
        },
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
        host_ends: host.continuous.is_none(),
        host_cut: if host.cut.known() {
            host.cut.value.unwrap_or(0.0)
        } else {
            0.0
        },
        host_along: dot(x3, host.direction),
        host_end: edge_end(f.t).1,
        through: through.map(|c| c.0),
        between: between.map(|b| (b.1, b.2)),
    })
}

/// (L15) Added to the plane key's offset of the ends across a gap between a
/// built-up member's angles (mm): their plane apart from that back's other
/// ends.
const BETWEEN_PLANE: i64 = 1_000_000_000;

/// (L17) The plane key of a gap between a built-up member's angles, the
/// gap from the back an end lies on (normal `n` out of it across the gap,
/// `off` from the origin along it, m) `gap` wide: the normal with its first
/// component that is not nought made positive and the gap's back first
/// along it — so that an end across the gap from the other side keys it
/// alike —, apart from every other plane ([`BETWEEN_PLANE`]).
fn between_key(n: [f64; 3], off: f64, gap: f64) -> PlaneKey {
    let mm = |x: f64| (x * 1000.0).round() as i64;
    let first = n.iter().copied().find(|c| c.abs() > 1e-3).unwrap_or(1.0);
    let (n, from) = if first < 0.0 {
        (scale(n, -1.0), -off - gap)
    } else {
        (n, off)
    };
    (mm(n[0]), mm(n[1]), mm(n[2]), mm(from) + BETWEEN_PLANE)
}

/// (L15) The words of a plan's note naming an end planned across a gap
/// between a built-up member's angles.
pub(crate) const BETWEEN_NOTE: &str = "planned onto a plate in the gap between";

/// The splice of joint `j` whose outer cover lies on the flange `f` of
/// `host` that `a` bolts onto (`host` one of the splice's members, `a`
/// not), with the outer covers' thickness there, m (O44 J7; as
/// [`crate::joints::through_cover`] finds it once written).
fn outer_cover(
    j: &JointDetail,
    a: &Arrival,
    host: &Arrival,
    f: &crate::joints::HostFlange,
) -> Option<(String, f64)> {
    use ohl_structures::connections::{CoverFace, CoverLeg};
    let q = host.profile.as_ref()?;
    let g = q.angle?;
    let s = j
        .splices
        .iter()
        .find(|s| s.members.contains(&host.member) && !s.members.contains(&a.member))?;
    let y = dot(f.across, q.axes[1]).abs() > 0.5;
    let leg = if y == (g.b >= g.h) {
        CoverLeg::Long
    } else {
        CoverLeg::Short
    };
    let outer: f64 = s
        .covers
        .iter()
        .filter(|c| c.face == CoverFace::Outer && c.leg.is_none_or(|l| l == leg))
        .map(|c| c.thickness)
        .sum();
    (outer > 0.0).then(|| (s.id.clone(), outer))
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
    /// Welded to the flat plate, or bent from it (L3).
    room: Room,
    /// The angle between its plane and the flat plate's, degrees.
    angle: f64,
    /// Bent (L3): where the two flats' middle planes meet, over the seam's
    /// ends (from the work point); welded: the seam's ends.
    middle: [[f64; 3]; 2],
}

/// How a plate in a member's own plane meets the flat plate on the flange
/// (step 4, L3).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Room {
    /// Welded to it along the seam (step 4): its member's steel the gap
    /// from the seam (the weld's room), its bolts their edge distance.
    Weld,
    /// Bent from it by the member's few degrees (L3, 2026-11-20): the seam
    /// is the bend, the plate turning away from the flange (`down` false,
    /// the member on its inside) or towards it past the flange's toe
    /// (`down`, the member on its outside); the member's steel clear of the
    /// bend's curved part by half the gap, its bolts their edge distance
    /// from it.
    Bend { down: bool },
}

/// The most a member's leg bolted may lie off the flange for a plate bent
/// by as much (L3, 2026-11-20), degrees: the owner's range of the ends
/// whose member needs rolls 5°–20° apart at its two ends; beyond, a welded
/// plate (step 4). A starting value of the program's, not a standard's.
pub const BEND_SMALL_MAX: f64 = 20.0;

/// The plates an end off its flange can take, in the order tried: bent up
/// and down (L3, where it lies at most [`BEND_SMALL_MAX`] off and
/// [`LayoutSettings::small_bends`]), then welded (step 4).
fn rooms_for(t: &Tilted, s: &LayoutSettings) -> Vec<Room> {
    let mut out = Vec::new();
    if s.small_bends && t.off <= BEND_SMALL_MAX {
        out.push(Room::Bend { down: false });
        out.push(Room::Bend { down: true });
    }
    out.push(Room::Weld);
    out
}

/// Why each plate tried for an end off its flange does not fit, in words:
/// the welded plate's (as before L3), then the bent ones' (L3).
fn rooms_words(errs: &[(Room, String)]) -> String {
    let weld = errs
        .iter()
        .find(|(r, _)| *r == Room::Weld)
        .map_or("", |x| x.1.as_str());
    let bent: Vec<String> = errs
        .iter()
        .filter_map(|(r, e)| match r {
            Room::Bend { down } => Some(format!(
                "{}, {e}",
                if *down {
                    "towards the flange"
                } else {
                    "away from it"
                }
            )),
            Room::Weld => None,
        })
        .collect();
    if bent.is_empty() {
        weld.to_owned()
    } else {
        format!("{weld} (a plate bent by as much: {})", bent.join("; "))
    }
}

/// Whether a plate in its member's own plane keeps the member's axis
/// within the limit (`more` farther out of the flange than on a flat
/// plate, m): at most a plate's thickness `t` farther out (step 4); bent
/// (L3), at most that nearer either.
fn within(room: Room, more: f64, t: f64, s: &LayoutSettings) -> bool {
    match room {
        Room::Weld => more <= t + 1e-9 || (s.own_offset_analysed && more <= OWN_OUT_MAX + 1e-9),
        Room::Bend { .. } => more.abs() <= t + 1e-9,
    }
}

/// The most a plate in an end's own plane welded to the flat plate (step
/// 4) may put its axis farther out of the flange than a flat plate would,
/// m, where that offset is carried into the analysis
/// ([`LayoutSettings::own_offset_analysed`]; L20 item 2): **the program's
/// value, not a standard's** — measured on the private real towers (L19:
/// of the ends refused beyond a plate's thickness, 89 % lie within 40 mm).
/// The analysis takes the offset whole and of either sign in the member's
/// own check (O45's estimated eccentricities).
pub const OWN_OUT_MAX: f64 = 0.040;

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
///
/// `room` (L3, 2026-11-20): bent from the flat plate instead, the seam its
/// bend — its member's steel the bend's curved part, (r + t) tan(θ/2) from
/// the seam (r the inside radius, [`bend_radius`], t the plate's thickness,
/// θ the angle), and half the gap clear of it, its bolts their edge
/// distance from that curved part; turning away from the flange (the
/// member lifted the least that keeps it so, on the bend's inside) or
/// towards it (the member lowered the least that keeps it so, on the
/// outside; the caller keeps that past the flange's toe).
#[allow(clippy::too_many_arguments)]
fn own_geometry(
    t: &Tilted,
    cut: f64,
    moved: f64,
    heel: [f64; 3],
    out: [f64; 3],
    tf: f64,
    gap: f64,
    room: Room,
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
    if dot(q, out) < FLAT_OFF.to_radians().sin() / 2.0 {
        return Err("its leg lies parallel to the flange".to_owned());
    }
    // (L3) Bent towards the flange: down its plate from the bend.
    let down = matches!(room, Room::Bend { down: true });
    let q = if down { scale(q, -1.0) } else { q };
    let qo = dot(q, out);
    // (L3b) Bent away from the flange, running towards the bend away from
    // the joint: its steel kept clear of the bend only over the flange.
    let back = !down
        && t.back
        && matches!(room, Room::Bend { .. })
        && dot(q, t.dir) < -FLAT_OFF.to_radians().sin();
    if dot(q, t.dir) < -FLAT_OFF.to_radians().sin() && !back {
        return Err(if down {
            "it runs up towards its bend away from the joint".to_owned()
        } else {
            "it runs down towards the flange away from the joint".to_owned()
        });
    }
    // The angle between its plane and the flat plate's.
    let angle = dot(t.nb, out).abs().clamp(0.0, 1.0).acos().to_degrees();
    // The bend's curved part from the seam (L3): (r + t) tan(θ/2).
    let curved = match room {
        Room::Weld => 0.0,
        Room::Bend { .. } => (bend_radius(tf) + tf) * (angle.to_radians() / 2.0).tan(),
    };
    // What keeps clear of the seam, each by how much along the plate:
    // its leg's two edges where its steel stops and past its bolts (the
    // gap; bent: the curved part and half the gap), its bolts (the edge
    // distance; bent: from the curved part).
    let (steel_room, bolt_room) = match room {
        Room::Weld => (gap, keep_edge * RING_GROW),
        Room::Bend { .. } => (curved + gap / 2.0, keep_edge * RING_GROW + curved),
    };
    let along = t.bolts.iter().map(|b| b.0).fold(0.0, f64::max) + keep_end;
    let mut keeps: Vec<([f64; 3], f64)> = Vec::new();
    let far = t.half.max(cut + along);
    for w in [0.0, t.width] {
        let edge = |s: f64| add(h, add(scale(t.dir, s), scale(t.across, w)));
        // (L3b, `back`) Where this edge leaves the flange's band — past its
        // toe (or heel) by half the gap — going away from the joint: beyond,
        // no flange under it (its steel may pass the bend's plane there).
        let to = if back {
            let (fa, fw) = t.band;
            let at = |s: f64| dot(sub(edge(s), t.face), fa);
            let rate = at(1.0) - at(0.0);
            let lim = if rate > 0.0 {
                fw + gap / 2.0
            } else {
                -gap / 2.0
            };
            if rate.abs() > 1e-6 {
                ((lim - at(0.0)) / rate).clamp(cut, far)
            } else {
                far
            }
        } else {
            far
        };
        for s in [cut, cut + along, to] {
            keeps.push((edge(s), steel_room));
        }
    }
    for b in &pts {
        keeps.push((*b, bolt_room));
    }
    // Lifted the least that keeps each up the plate from the seam (bent
    // towards the flange: lowered the least that keeps each down it).
    let lifts = keeps.iter().map(|(p, r)| dot(out, sub(q0, *p)) + r * qo);
    let lift = if down {
        lifts.fold(f64::INFINITY, f64::min)
    } else {
        lifts.fold(f64::NEG_INFINITY, f64::max)
    };
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
    let seam = [to3(&[lo, 0.0]), to3(&[hi, 0.0])];
    // (L3) Bent: where the two flats' middle planes meet — half the
    // thickness under the seam (the flat plate's face), and (t/2) tan(θ/2)
    // towards the flat in its own plane (bent away from the flange) or
    // towards the flat plate (bent towards it).
    let middle = match room {
        Room::Weld => seam,
        Room::Bend { down } => {
            let k = tf / 2.0 * (angle.to_radians() / 2.0).tan();
            let shift = add(
                scale(out, -tf / 2.0),
                scale(side, if down { k } else { -k }),
            );
            [add(seam[0], shift), add(seam[1], shift)]
        }
    };
    Ok(OwnGeo {
        lift,
        out_more,
        outline: poly.iter().map(to3).collect(),
        seam,
        seam_at: lp,
        side,
        room,
        angle,
        middle,
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
    room: Room,
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
        let geo = own_geometry(t, c / t.lu, p.moved, heel, t.out, x.own_t, gap, room)?;
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
fn farther_out(more: f64, t: f64, room: Room, s: &LayoutSettings) -> String {
    if room == Room::Weld && s.own_offset_analysed && more > 0.0 {
        return format!(
            "its axis would lie {:.0} mm farther out of the flange than on a flat plate (more \
             than the {:.0} mm the analysis takes for a plate in its own plane)",
            more * 1000.0,
            OWN_OUT_MAX * 1000.0
        );
    }
    format!(
        "its axis would lie {:.0} mm {} than on a flat plate (more than a plate's {:.0} mm: its \
         line of action that far off the work point is not in the analysis)",
        more.abs() * 1000.0,
        if more < 0.0 {
            "nearer the flange"
        } else {
            "farther out of the flange"
        },
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
/// Where the bolts of `x` placed `p` lie along what it bolts onto, from
/// the work point (m).
fn bolt_along(x: &Strip, p: &Placed) -> Vec<f64> {
    x.bolts
        .iter()
        .map(|(along, line)| {
            let l = x.gauges.get(*line).copied().unwrap_or(0.0) + p.moved;
            (x.at(p.cut + along, l)[0] - x.w[0]) * x.host_along
        })
        .collect()
}

/// The cut of `x` placed `p`, along it (as its end is written).
fn cut_of(x: &Strip, p: &Placed) -> f64 {
    match &x.tilted {
        Some(t) => cut3(t, p.cut),
        None => p.cut,
    }
}

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

/// L11 ([`LayoutSettings::plate_shift`]): end `p` going onto a plate,
/// moved sideways by the least whole millimetre within the shift limit so
/// that its bolts pass the flange by the nut and the gap within `limit`
/// (the longest pull-back), clear of the plane's ends `others` as they lie
/// and of the fixed steel, keeping the bolts of a member left as it is that
/// bolts onto it; none where nothing within the limit does (or its cut or
/// shift is given by hand, or it is off its flange or over the heel).
fn shift_past_toe(
    strips: &[Strip],
    p: &Placed,
    others: &[Placed],
    s: &LayoutSettings,
    limit: f64,
) -> Option<Placed> {
    let x = &strips[p.k];
    if x.shift_per.is_none()
        || x.hand_shift
        || x.hand_cut.is_some()
        || x.tilted.is_some()
        || x.over.is_some()
    {
        return None;
    }
    let floor = x.floor.as_ref().map_or(0.0, |f| f.0);
    let conc = concentric_of(x);
    let steps = (s.shift_limit * 1000.0 + 1e-9).floor() as i64;
    for n in 1..=steps {
        for sign in [1.0, -1.0] {
            let moved = conc + sign * n as f64 * 0.001;
            let (clear, _) = cut_need(strips, p.k, others, s, moved);
            let cut = clear.max(floor);
            let c = past_toe(x, &Placed { k: p.k, cut, moved }, s.gap);
            let c = ceil_to(c.max(cut), 0.001);
            if c.is_finite() && c <= limit + 1e-9 && partner_misfit(x, c, moved, s).is_none() {
                return Some(Placed {
                    k: p.k,
                    cut: c,
                    moved,
                });
            }
        }
    }
    None
}

/// `lo` and `hi` narrowed to the cuts `c` with `a ≤ v0 + c·dv ≤ b`.
fn narrow(lo: &mut f64, hi: &mut f64, v0: f64, dv: f64, a: f64, b: f64) {
    if dv.abs() < 1e-9 {
        if v0 < a - 1e-9 || v0 > b + 1e-9 {
            *lo = f64::INFINITY;
        }
        return;
    }
    let (c1, c2) = ((a - v0) / dv, (b - v0) / dv);
    let (mn, mx) = if dv > 0.0 { (c1, c2) } else { (c2, c1) };
    *lo = lo.max(mn);
    *hi = hi.min(mx);
}

/// L5 (2026-11-20): end `p` of a face going onto a plate placed so that
/// every bolt lands on the flange — clear of its heel's leg and root by the
/// nut, the edge distance from its toe, on the steel of what it bolts onto
/// past where that starts (`from`) by the end distance — pulled back
/// further, else moved sideways by the least whole millimetre up to `most`
/// (m), clear of the ends `others` as they lie and of the fixed steel; none
/// where nothing within that does. A cut or shift given by hand is kept
/// (none).
fn land_moved(
    strips: &[Strip],
    p: &Placed,
    others: &[Placed],
    s: &LayoutSettings,
    from: f64,
    most: f64,
) -> Option<Placed> {
    let x = &strips[p.k];
    if x.hand_cut.is_some() || x.tilted.is_some() || x.over.is_some() {
        return None;
    }
    let limit = s.pull_back.min(x.half - bolt_run(x));
    let floor = x.floor.as_ref().map_or(0.0, |f| f.0);
    let conc = concentric_of(x);
    let can = x.shift_per.is_some() && !x.hand_shift;
    let steps = if can {
        (most * 1000.0 + 1e-9).floor() as i64
    } else {
        0
    };
    let (lo_y, hi_y) = (x.t + x.r + x.nut, x.width - x.edge);
    for n in 0..=steps {
        for sign in [1.0, -1.0] {
            if n == 0 && sign < 0.0 {
                continue;
            }
            let moved = if can {
                conc + sign * n as f64 * 0.001
            } else {
                p.moved
            };
            let (need, _) = cut_need(strips, p.k, others, s, moved);
            let mut lo = need.max(floor);
            let mut hi = limit;
            for (along, line) in &x.bolts {
                let l = x.gauges.get(*line).copied().unwrap_or(0.0) + moved;
                let q = x.at(*along, l);
                narrow(&mut lo, &mut hi, q[1], x.u[1], lo_y, hi_y);
                if x.host_ends {
                    narrow(
                        &mut lo,
                        &mut hi,
                        (q[0] - x.w[0]) * x.host_along,
                        x.u[0] * x.host_along,
                        from + x.host_end,
                        f64::INFINITY,
                    );
                }
            }
            let c = ceil_to(lo, 0.001);
            if c.is_finite() && c <= hi + 1e-9 && partner_misfit(x, c, moved, s).is_none() {
                return Some(Placed {
                    k: p.k,
                    cut: c,
                    moved,
                });
            }
        }
    }
    None
}

/// The least cut (from the work point) that takes strip `k`, moved `moved`
/// across, clear of the flange's heel, the bolts not laid out, the ends
/// `placed` and the fixed steel (its floor not applied), and what asks it.
fn cut_need(
    strips: &[Strip],
    k: usize,
    placed: &[Placed],
    s: &LayoutSettings,
    moved: f64,
) -> (f64, Option<String>) {
    let x = &strips[k];
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
    // L2 (b): its outstanding leg to its full height, and its bolts'
    // nuts and shanks behind the face (each bolt's disc, the nut's
    // corner radius), clear of the fixed steel by the gap.
    let band = [x.out_band[0] + moved, x.out_band[1] + moved];
    for (name, poly) in &x.fixed_out {
        let c = poly_need(x, band, poly, s.gap);
        if c > need {
            need = c;
            by = Some(name.clone());
        }
    }
    for (name, poly) in &x.fixed_far {
        for (along, line) in &x.bolts {
            let l = x.gauges.get(*line).copied().unwrap_or(0.0) + moved;
            let c = poly_need(x, [l - x.nut, l + x.nut], poly, s.gap) + x.nut - along;
            if c > need {
                need = c;
                by = Some(format!("{name} (behind the face, at its nuts)"));
            }
        }
    }
    (need.max(0.0), by)
}

/// The words of an end left because the bolts of a member left as it is
/// bolting onto it would not stay on it (L8, [`LayoutSettings::partner_bolts`]).
pub const PARTNER_LEFT: &str =
    "a bolt of a member left as it is that bolts onto it would not stay on it";

/// L8 (L7's proposal 4): why strip `x` cut at `cut` and moved `moved`
/// across does not keep the bolts of the members left as they are that bolt
/// onto it (`Strip::partners`) — a hole not within its flange (the joint
/// checks' rule), a bolt nearer its steel's end than its end distance, or
/// one of its own bolts nearer a partner's than the default minimum
/// spacing; none: it keeps them.
fn partner_misfit(x: &Strip, cut: f64, moved: f64, s: &LayoutSettings) -> Option<String> {
    let (lo, hi) = (x.lat[0] + moved, x.lat[1] + moved);
    let spacing = s.defaults.of(x.d)[1];
    for (c, hole, end) in &x.partners {
        let rel = sub2(*c, x.w);
        let (along, across) = (dot2(rel, x.u), dot2(rel, x.v));
        if across - hole / 2.0 < lo - 1e-9 || across + hole / 2.0 > hi + 1e-9 {
            return Some("its hole would not lie within the flange".to_owned());
        }
        if x.own_ends && along < cut + end - 1e-9 {
            return Some(
                "its bolt would lie nearer the steel's end than its end distance".to_owned(),
            );
        }
        let keep = spacing.max(s.defaults.of(*hole)[1]);
        for (b, line) in &x.bolts {
            let l = x.gauges.get(*line).copied().unwrap_or(0.0) + moved;
            let q = x.at(cut + b, l);
            let d = sub2(q, *c);
            if (d[0] * d[0] + d[1] * d[1]).sqrt() < keep - 1e-9 {
                return Some("a bolt of the end would lie nearer it than the spacing".to_owned());
            }
        }
    }
    None
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
    let cut_at = |moved: f64| cut_need(strips, k, placed, s, moved);
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
    // L8: the bolts of the members left as they are bolting onto it kept
    // on its steel (`Strip::partners`).
    let partner = |cut: f64, moved: f64| partner_misfit(x, cut, moved, s);
    let mut partner_why: Option<String> = None;
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
        if let Some(w) = partner(c, concentric) {
            return Err(format!(
                "{}: with its cut given by hand, {PARTNER_LEFT} ({w})",
                x.member
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
        let cut = ceil_to(need, 0.001);
        match partner(cut, concentric) {
            None => {
                return Ok(Placed {
                    k,
                    cut,
                    moved: concentric,
                });
            }
            Some(w) => partner_why = Some(w),
        }
    }
    // A shift: the smallest that fits, a millimetre at a time.
    if x.shift_per.is_some() && !x.hand_shift {
        let steps = (s.shift_limit * 1000.0 + 1e-9).floor() as i64;
        for n in 1..=steps {
            for sign in [1.0, -1.0] {
                let moved = concentric + sign * n as f64 * 0.001;
                let (c, _) = cut_at(moved);
                if c <= limit {
                    let cut = ceil_to(c, 0.001);
                    match partner(cut, moved) {
                        None => return Ok(Placed { k, cut, moved }),
                        Some(w) => {
                            partner_why.get_or_insert(w);
                        }
                    }
                }
            }
        }
    }
    if let Some(w) = partner_why {
        return Err(format!(
            "{}: {PARTNER_LEFT} with any cut and move that fits ({w}; the longest pull-back \
             {:.0} mm, the shift limit {:.0} mm)",
            x.member,
            limit * 1000.0,
            s.shift_limit * 1000.0
        ));
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

/// Said of an end left as it is whose bolts drawn join nothing at the
/// joint (L10, [`LayoutSettings::nothing_joined`]).
pub const BOLTS_JOIN_NOTHING: &str = "its bolts join nothing";

/// Said of a base member whose own end bolts hold the plate on its flange
/// (L14, [`LayoutSettings::base_holds`]).
pub const BASE_HOLDS: &str = "its end bolts hold the plate";

/// L14 ([`LayoutSettings::base_holds`]): of the members `plan` leaves by
/// hand because their bolts join nothing ([`BOLTS_JOIN_NOTHING`]), each
/// with a plate of the plan lying on its own flange ([`base_plate`]) is
/// laid onto that plate, its own end bolts the plate's holding bolts —
/// where the plate was given provisional bolts of its own onto it (L5o:
/// placed clear of what is there) those become its end bolts (one gauge
/// line, an even pitch, [`hold_as_end`]); else its own bolts as drawn; bolts
/// of the plate's own that cannot be so written: left as before. Applied to
/// a plan as it is written (after it has been weighed against others, so
/// that the end gained never decides between plans); how many so laid.
/// `written_onto`: what a member's end at the joint bolts onto as the file
/// written gives it (none: not given there — the joint's leg then worked
/// out for it, [`Arrival::joint_leg`]); a name the run gave it only while
/// planning is not written for an end left as it is (L16).
pub(crate) fn hold_by_base(
    plan: &mut JointPlan,
    j: &JointDetail,
    written_onto: &dyn Fn(&str) -> Option<String>,
) -> usize {
    let mut n = 0;
    let flagged: Vec<String> = plan
        .hand
        .iter()
        .filter(|h| h.contains(BOLTS_JOIN_NOTHING))
        .filter_map(|h| h.split(": ").next().map(str::to_owned))
        .collect();
    for m in flagged {
        let Some(a) = j.arrivals.iter().find(|x| x.member == m) else {
            continue;
        };
        if plan.ends.iter().any(|e| e.member == m) {
            continue;
        }
        let Some(k) = base_plate(a, &plan.plates, &plan.ends, j, written_onto) else {
            continue;
        };
        let cut = a.cut.value.unwrap_or(0.0);
        let bolts = match &plan.plates[k].hold {
            Some(h) => match hold_as_end(h, a, cut) {
                Some(b) => Some(b),
                None => continue,
            },
            None => None,
        };
        let count = bolts
            .as_ref()
            .map_or(a.bolt_points.len(), |b| b.count as usize);
        let whose = if bolts.is_some() {
            "the plate's provisional bolts onto it (L5o) written as its end bolts"
        } else {
            "its own bolts as drawn"
        };
        plan.plates[k].hold = None;
        plan.ends.push(EndPlan {
            member: m.clone(),
            cut: bolts.as_ref().map(|_| cut),
            shift: None,
            off: 0.0,
            on_gauge: false,
            onto: Onto::Plate(k),
            bolts,
        });
        let head = format!("{m}: ");
        plan.hand
            .retain(|h| !(h.starts_with(&head) && h.contains(BOLTS_JOIN_NOTHING)));
        if !plan.hand.iter().any(|h| h.starts_with(&head)) {
            plan.hand_members.retain(|x| x != &m);
            plan.kept_free.retain(|x| x != &m);
            plan.off_flange.retain(|x| x.0 != m);
            plan.hand_hosts.retain(|x| x.0 != m);
        }
        plan.notes.push(format!(
            "{m}: {BASE_HOLDS} — its {count} end bolt(s) onto the plate on its flange ({whose}), \
             the plate's holding bolts (L14)"
        ));
        n += 1;
    }
    n
}

/// The plate's own provisional bolts `h` onto `a` (L5o) as `a`'s end bolts
/// (L14): on one gauge line at an even pitch (within half a millimetre),
/// the first at least one and a half diameters from where its steel stops
/// (`cut`, m); none: not so written.
fn hold_as_end(h: &HoldPlan, a: &Arrival, cut: f64) -> Option<BoltArea> {
    if h.member != a.member || h.points.is_empty() {
        return None;
    }
    let mut pts = h.points.clone();
    pts.sort_by(|x, y| x[0].total_cmp(&y[0]));
    let g = pts[0][1];
    if pts.iter().any(|q| (q[1] - g).abs() > 5e-4) {
        return None;
    }
    let pitch = if pts.len() > 1 {
        let p0 = pts[1][0] - pts[0][0];
        if pts
            .windows(2)
            .any(|w| ((w[1][0] - w[0][0]) - p0).abs() > 5e-4)
            || p0 <= 0.0
        {
            return None;
        }
        Some(p0)
    } else {
        None
    };
    let end = pts[0][0] - cut;
    if end < 1.5 * h.bolt.diameter - 1e-9 {
        return None;
    }
    Some(BoltArea {
        from: "the plate's provisional bolts onto it, as its end bolts".into(),
        preset: None,
        count: pts.len() as u32,
        lines: 1,
        bolt: h.bolt.bolt.clone(),
        diameter: h.bolt.diameter,
        flats: h.bolt.flats,
        gauges: vec![g],
        pitch,
        end: Some(end),
    })
}

/// The plate of `plates` lying on `a`'s own flange whose end bolts may
/// hold it (L14): a flat plate on the flange (not one in its members' own
/// plane, not bent from one, its outline not laid out), with members bolted
/// onto it; `a` an angle bolted by one leg. Its bolts go onto the flange the
/// plate lies on (the joint details' leg lying flat against what it bolts
/// onto) — unless its leg bolted is given as the other one. Not where an
/// end left as it is bolts onto it directly: as planned, or as the file
/// written gives it (`written_onto`; not given there: the joint's leg
/// worked out for it — L16, an end with no connection at the joint whose
/// worked-out `onto` is `a`). None: no such plate.
fn base_plate(
    a: &Arrival,
    plates: &[PlatePlan],
    ends: &[EndPlan],
    j: &JointDetail,
    written_onto: &dyn Fn(&str) -> Option<String>,
) -> Option<usize> {
    if !a.other_points.is_empty() || a.bolt_points.is_empty() {
        return None;
    }
    let p = a.profile.as_ref().filter(|p| p.angle.is_some())?;
    let dot = |x: [f64; 3], y: [f64; 3]| x[0] * y[0] + x[1] * y[1] + x[2] * y[2];
    let near = 10f64.to_radians().cos();
    plates.iter().position(|q| {
        if q.on != a.member
            || q.own.is_some()
            || q.outline.is_some()
            || q.members.is_empty()
            || q.members.contains(&a.member)
        {
            return false;
        }
        // Which of its flanges the plate lies on (along y or z): by the
        // flange's heel line it keeps to (across it towards the toe), else
        // by the bolts' axis of a member on it (the plate's normal).
        let on_y = if let Some((_, c)) = q.heel {
            if dot(c, p.axes[1]).abs() >= near {
                true
            } else if dot(c, p.axes[2]).abs() >= near {
                false
            } else {
                return false;
            }
        } else {
            let Some(n) = q.members.iter().find_map(|m| {
                j.arrivals
                    .iter()
                    .find(|x| &x.member == m && !x.bolt_points.is_empty())
                    .map(|x| x.bolt_axis)
            }) else {
                return false;
            };
            if dot(n, p.axes[2]).abs() >= near {
                true
            } else if dot(n, p.axes[1]).abs() >= near {
                false
            } else {
                return false;
            }
        };
        // Its leg bolted turned to the plate's flange only where it is not
        // given.
        // (An end left as it is that bolts onto it directly: what the plan
        // lays out keeps clear of its bolts, as of any.)
        let direct = j.arrivals.iter().any(|x| {
            x.member != a.member
                && !ends.iter().any(|e| e.member == x.member)
                && (x.onto.value.as_deref() == Some(a.member.as_str())
                    || written_onto(&x.member)
                        .or_else(|| x.joint_leg.clone())
                        .as_deref()
                        == Some(a.member.as_str()))
        });
        // No end left as it is bolts onto it directly: its own bolts would
        // share its flange with theirs (measured: holes overlapping, bolts
        // off its flange as the joint details read it).
        !direct && (on_y == a.leg_y || a.leg.source != Source::Given)
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
/// from the work point) where it lies in a slab over `x`'s face, from
/// `slab.0` to `slab.1` (negative: behind it; `x`'s own slab: from the face
/// to what `x` carries over it), m: convex, 2D in the plane's axes; none
/// where it lies outside.
fn footprint_in(x: &Strip, pts: &[[f64; 3]], slab: (f64, f64)) -> Option<Vec<[f64; 2]>> {
    let h = |q: &[f64; 3]| dot(sub(*q, x.face3), x.n3);
    let (lo, hi) = slab;
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
    let f0 = |p: [f64; 2]| dot2(rel(p), x.v) - (lat[0] - gap);
    let f1 = |p: [f64; 2]| (lat[1] + gap) - dot2(rel(p), x.v);
    let f2 = |p: [f64; 2]| x.half - dot2(rel(p), x.u);
    // (L9, speed only: the same numbers) Each cut made only where it cuts:
    // a footprint wholly on the kept side of it stays as it is, one wholly
    // on the other is gone — as [`clip`] gives them, without its copies.
    let mut q: std::borrow::Cow<'_, [[f64; 2]]> = std::borrow::Cow::Borrowed(poly);
    for f in [&f0 as &dyn Fn([f64; 2]) -> f64, &f1, &f2] {
        let kept = q.iter().filter(|p| f(**p) >= 0.0).count();
        if kept == 0 {
            return f64::NEG_INFINITY;
        }
        if kept < q.len() {
            q = std::borrow::Cow::Owned(clip(&q, f));
        }
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
    fixed_slab(x, j, pieces, fixed, laid3, (FACE_TOUCH, x.slab))
}

/// [`fixed_in`] in the slab `slab` over `x`'s face (L2 b: as high as its
/// outstanding leg, or behind the face where its nuts lie).
fn fixed_slab(
    x: &Strip,
    j: &JointDetail,
    pieces: &[(crate::ClashPart, Vec<[f64; 3]>)],
    fixed: &BTreeSet<String>,
    laid3: &[Laid3],
    slab: (f64, f64),
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
                // (L15) Across a gap between the host's angles: its steel
                // over the face (the angle across the gap) is in the way.
                let across_gap = x.between.is_some() && host_run(m) && slab.1 > 0.0;
                if m == &x.member
                    || !fixed.contains(m)
                    || (host_run(m) && !across_gap)
                    || onto_me(m)
                {
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
            // (L21) A plate's own holding bolts: not fixed steel to plan
            // round (the plan places them itself; the check after writing
            // tests them).
            ClashPart::PlateBolt { .. } => continue,
            ClashPart::Cover(sid, _) => {
                if x.through.as_ref() == Some(sid)
                    || j.splices.iter().any(|sp| {
                        &sp.id == sid
                            && (sp.members.contains(&x.member) || sp.bolted.contains(&x.member))
                    })
                {
                    continue;
                }
                part.words()
            }
        };
        if let Some(p) = footprint_in(x, pts, slab) {
            out.push((name, p));
        }
    }
    for (m, bolt, pts) in laid3 {
        if m == &x.member || (!bolt && (host_run(m) || onto_me(m))) {
            continue;
        }
        if let Some(p) = footprint_in(x, pts, slab) {
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
fn solids_of(
    strips: &[Strip],
    placed: &[Placed],
    lift: f64,
    s: &LayoutSettings,
    outline: Option<&[[f64; 2]]>,
) -> Vec<Laid3> {
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
            // L2 (b): its nut and the shank beyond it, behind the flange (or
            // the plate) it bolts through.
            if s.exact_plan {
                let g1 = if lift > 0.0 { lift } else { x.t };
                out.push((
                    x.member.clone(),
                    true,
                    boxed(
                        [a - r, a + r],
                        [l - r, l + r],
                        [lift - g1 - (BOLT_DRAWN[2] + BOLT_DRAWN[3]) * x.d, lift - g1],
                    ),
                ));
            }
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
        // L2 (b): the plate's outline as it will be drawn.
        let hull = if s.exact_plan {
            outline.map_or_else(
                || plate_outline2(strips, placed, s.gap, Outline::Round),
                <[[f64; 2]]>::to_vec,
            )
        } else {
            crate::joint_sheet::hull(plate)
        };
        for q in hull {
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
    // (At most one corner more: a convex polygon cut by a line.)
    let mut out = Vec::with_capacity(n + 1);
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

/// The outline (in the plane's axes) the plate of the ends `placed` will be
/// drawn with (L2 b, 2026-11-20; as `joints` draws a plate round its
/// bolts): round the work point, each end's leg over its bolts (from its
/// first bolt less the end distance, not before the work point, to its last
/// plus it, across the leg), each bolt with its end and edge distances in
/// the plate, and the flange under it from its heel across its width as far
/// along as those reach; cut at the heel by half the gap where every end
/// leaves towards the toe (as the run cuts it). *An assumption, on the safe
/// side for the fit*: the distances its bolts keep those of a plate as thick
/// as each end's leg (the plate is as thick as the thickest).
///
/// `short` (L2's follow-up a, 2026-11-20): stopping short of the work point
/// — not round it, each end's leg from where it crosses the flange's toe
/// (or from its cut, the nearer the joint, not before the work point) over
/// its bolts, and the flange under them only as far along as those reach;
/// as `joints` draws a plate with the layout's `short` mark (from the
/// cut: at least as large there, on the safe side for the fit).
///
/// `Core`: as `Short` without the flange under it — what a plate cut
/// straight short of something must keep.
fn plate_outline2(strips: &[Strip], placed: &[Placed], gap: f64, form: Outline) -> Vec<[f64; 2]> {
    let short = form != Outline::Round;
    let flat: Vec<&Placed> = placed
        .iter()
        .filter(|p| strips[p.k].tilted.is_none())
        .collect();
    let Some(p0) = flat.first() else {
        return Vec::new();
    };
    let x0 = &strips[p0.k];
    let mut pts: Vec<[f64; 2]> = if short { Vec::new() } else { vec![x0.w] };
    const N: usize = 16;
    let grow = 1.0 / (std::f64::consts::PI / N as f64).cos();
    for p in &flat {
        let x = &strips[p.k];
        let lat = [x.lat[0] + p.moved, x.lat[1] + p.moved];
        let amin = x.bolts.iter().map(|b| b.0).fold(f64::INFINITY, f64::min);
        let amax = x
            .bolts
            .iter()
            .map(|b| b.0)
            .fold(f64::NEG_INFINITY, f64::max);
        // (Short: from where the leg's nearer edge crosses the flange's toe,
        // where that is nearer the joint than its cut.)
        let mut from = p.cut;
        if short && x.u[1] > 1e-6 {
            for l in lat {
                let c = x.at(0.0, l);
                from = from.min((x.width - c[1]) / x.u[1]);
            }
        }
        if amin.is_finite() {
            for s in [from.max(0.0), p.cut + amax + amin] {
                for l in lat {
                    pts.push(x.at(s, l));
                }
            }
        }
        let (edge, end) = x.keep_plate;
        for (along, line) in &x.bolts {
            let l = x.gauges.get(*line).copied().unwrap_or(0.0) + p.moved;
            let c = x.at(p.cut + along, l);
            for i in 0..N {
                let a = i as f64 * std::f64::consts::TAU / N as f64;
                let (da, dl) = (end * grow * a.cos(), edge * grow * a.sin());
                pts.push([
                    c[0] + x.u[0] * da + x.v[0] * dl,
                    c[1] + x.u[1] * da + x.v[1] * dl,
                ]);
            }
        }
    }
    if form != Outline::Core {
        let lo = pts.iter().map(|q| q[0]).fold(f64::INFINITY, f64::min);
        let hi = pts.iter().map(|q| q[0]).fold(f64::NEG_INFINITY, f64::max);
        for xq in [lo, hi] {
            pts.push([xq, 0.0]);
            pts.push([xq, x0.width]);
        }
    }
    let hull = crate::joint_sheet::hull(pts);
    if flat.iter().all(|p| strips[p.k].u[1] > 1e-6) {
        let yc = if x0.inside { x0.t + x0.r } else { 0.0 } + gap / 2.0;
        clip(&hull, |q| q[1] - yc)
    } else {
        hull
    }
}

/// Which outline [`plate_outline2`] works out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outline {
    /// Round the work point (as before L2's follow-up).
    Round,
    /// Stopping short of the work point (the layout's `short` mark).
    Short,
    /// The part of the short outline no cut may take: the legs from the
    /// flange's toe over their bolts and the bolts' distances.
    Core,
}

/// How many straight cuts a short plate's outline may take (L2's
/// follow-up a): the program's bound, not a standard's.
pub const CUT_TRIES: usize = 4;

/// A straight cut keeping `core` (2D, convex) and clearing `foot` (what the
/// plate would run into): the axis (unit, from the core towards it) and
/// where the plate stops along it — clear of it by half the gap, or by half
/// of what parts them where that is less —; none: they are not apart by
/// more than twice the clash check's tolerance.
fn separate(core: &[[f64; 2]], foot: &[[f64; 2]], gap: f64) -> Option<([f64; 2], f64)> {
    let tol = crate::clash::CLASH_TOLERANCE;
    let mut best: Option<([f64; 2], f64, f64)> = None;
    for poly in [core, foot] {
        let n = poly.len();
        for i in 0..n {
            let e = sub2(poly[(i + 1) % n], poly[i]);
            let l = (e[0] * e[0] + e[1] * e[1]).sqrt();
            if l < 1e-12 {
                continue;
            }
            for ax in [[-e[1] / l, e[0] / l], [e[1] / l, -e[0] / l]] {
                let span = |p: &[[f64; 2]]| {
                    p.iter()
                        .map(|q| dot2(*q, ax))
                        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), d| {
                            (lo.min(d), hi.max(d))
                        })
                };
                let (_, c1) = span(core);
                let (f0, _) = span(foot);
                let sep = f0 - c1;
                if sep > 2.0 * tol && best.is_none_or(|b| sep > b.2 + 1e-12) {
                    best = Some((ax, f0, sep));
                }
            }
        }
    }
    best.map(|(ax, f0, sep)| (ax, f0 - (gap / 2.0).min(sep / 2.0)))
}

/// Whether a plate's outline `poly` (in `x0`'s plane) still lies on the
/// flange over at least half a square of its width (room for it to bear
/// on the flange; the program's value, as step 4's flat plate's).
fn on_flange(poly: &[[f64; 2]], x0: &Strip) -> bool {
    let floor = if x0.inside { x0.t + x0.r } else { 0.0 };
    let p = clip(poly, |q| q[1] - floor);
    let p = clip(&p, |q| x0.width - q[1]);
    let area = (0..p.len())
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            a[0] * b[1] - a[1] * b[0]
        })
        .sum::<f64>()
        .abs()
        / 2.0;
    area >= x0.width * x0.width / 2.0 - 1e-12
}

/// What the plate `poly` (in `x0`'s plane, `tp` thick over its face) of the
/// ends `on` would run into (L2 b): the fixed steel of the joint (not a
/// member on it, the member it lies on or one in line with that, nor a
/// member left for a hand detail — the run lets a plate lie under those),
/// the plates and covers given by hand, and what the planes before laid
/// out — each as its footprint in the plate's thickness, overlapping by
/// more than the clash check's tolerance (separating axes in the plane:
/// exact for the convex footprints). None: clear.
#[allow(clippy::too_many_arguments)]
fn plate_hits(
    x0: &Strip,
    on: &BTreeSet<&str>,
    j: &JointDetail,
    pieces: &[(crate::ClashPart, Vec<[f64; 3]>)],
    fixed: &BTreeSet<String>,
    excused: &BTreeSet<String>,
    laid3: &[Laid3],
    poly: &[[f64; 2]],
    tp: f64,
) -> Option<(String, Vec<[f64; 2]>)> {
    use crate::ClashPart;
    if poly.len() < 3 {
        return None;
    }
    let find = |m: &str| j.arrivals.iter().find(|a| a.member == m);
    let host = find(&x0.host);
    let host_run = |m: &str| {
        m == x0.host
            || host
                .zip(find(m))
                .is_some_and(|(h, b)| angle_deg(h.direction, b.direction) > 160.0)
    };
    let slab = (FACE_TOUCH, (tp - FACE_TOUCH).max(2.0 * FACE_TOUCH));
    let hit = |pts: &[[f64; 3]]| {
        footprint_in(x0, pts, slab).filter(|f| sat2(poly, f) > crate::clash::CLASH_TOLERANCE)
    };
    // (J6, 2026-11-20) On a built-up member: its other angles too (the
    // one it lies on is under its face, or beyond its heel's cut).
    let built = host.is_some_and(|h| h.profile.as_ref().is_some_and(|q| !q.parts.is_empty()));
    for (part, pts) in pieces {
        let name = match part {
            ClashPart::Member(m) | ClashPart::Bolt { member: m, .. } => {
                let own_steel = built && matches!(part, ClashPart::Member(_)) && host_run(m);
                if on.contains(m.as_str())
                    || (host_run(m) && !own_steel)
                    || (!fixed.contains(m) && !own_steel)
                    || excused.contains(m)
                {
                    continue;
                }
                if own_steel && x0.between.is_some() {
                    format!("{m}'s steel in the gap between its angles")
                } else if own_steel {
                    format!(
                        "{m}'s other angle (built up: a plate between its angles is laid out \
                         only for ends across the gap)"
                    )
                } else {
                    part.words()
                }
            }
            ClashPart::Plate(_) | ClashPart::Cover(..) => part.words(),
            // (L21) As above: not fixed steel to plan round.
            ClashPart::PlateBolt { .. } => continue,
        };
        if let Some(f) = hit(pts) {
            return Some((name, f));
        }
    }
    for (m, _, pts) in laid3 {
        if on.contains(m.as_str()) || host_run(m) {
            continue;
        }
        if let Some(f) = hit(pts) {
            return Some((m.clone(), f));
        }
    }
    None
}

/// How deep two convex polygons (2D) overlap: the least overlap of their
/// spans over their edges' normals (≤ 0: apart).
fn sat2(a: &[[f64; 2]], b: &[[f64; 2]]) -> f64 {
    let mut best = f64::INFINITY;
    for poly in [a, b] {
        let n = poly.len();
        for i in 0..n {
            let e = sub2(poly[(i + 1) % n], poly[i]);
            let l = (e[0] * e[0] + e[1] * e[1]).sqrt();
            if l < 1e-12 {
                continue;
            }
            let ax = [-e[1] / l, e[0] / l];
            let span = |p: &[[f64; 2]]| {
                p.iter()
                    .map(|q| dot2(*q, ax))
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), d| {
                        (lo.min(d), hi.max(d))
                    })
            };
            let (a0, a1) = span(a);
            let (b0, b1) = span(b);
            let o = a1.min(b1) - a0.max(b0);
            if o <= 0.0 {
                return o;
            }
            best = best.min(o);
        }
    }
    best
}

#[cfg(test)]
mod cut_tests {
    use super::{dot2, separate};

    /// L2's follow-up (a): the straight cut keeps the core whole and stops
    /// half the gap short of what the plate would run into — or half of
    /// what parts them where that is less; none where they touch.
    #[test]
    fn a_cut_keeps_the_core_and_stops_short_of_the_steel() {
        let sq =
            |x0: f64, y0: f64, w: f64| vec![[x0, y0], [x0 + w, y0], [x0 + w, y0 + w], [x0, y0 + w]];
        let core = sq(0.0, 0.0, 0.10);
        let (ax, lim) = separate(&core, &sq(0.15, 0.0, 0.05), 0.010).expect("apart");
        assert!((ax[0] - 1.0).abs() < 1e-9 && ax[1].abs() < 1e-9, "{ax:?}");
        assert!((lim - 0.145).abs() < 1e-9, "{lim}");
        assert!(core.iter().all(|q| dot2(*q, ax) <= lim));
        // 4 mm apart: the cut half way.
        let (_, lim) = separate(&core, &sq(0.104, 0.0, 0.05), 0.010).expect("apart");
        assert!((lim - 0.102).abs() < 1e-9, "{lim}");
        // Touching (or overlapping): no cut.
        assert!(separate(&core, &sq(0.1015, 0.0, 0.05), 0.010).is_none());
        assert!(separate(&core, &sq(0.05, 0.05, 0.10), 0.010).is_none());
    }
}

#[cfg(test)]
mod back_tests {
    use super::{Room, Tilted, add, bend_radius, cross, dot, own_geometry, scale, sub, unit};

    /// L3b (2026-11-20): an end running towards its bend going away from
    /// the joint — its leg's plane turned 8° about the member it bolts onto
    /// (x), sloping down towards the flange's toe (+y), the member leaving
    /// over the toe. Bent away from the flange it was refused; with
    /// `back`, laid out with its steel clear of the bend (the curved part
    /// and half the gap) wherever it lies over the flange — to half the gap
    /// past the toe —, passing the bend's plane beyond (no flange there).
    #[test]
    fn an_end_running_towards_its_bend_is_bent_away_over_the_flange_only() {
        let th = 8f64.to_radians();
        let (out, across_f, width) = ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0], 0.1);
        let nb = [0.0, th.sin(), th.cos()];
        let q = [0.0, -th.cos(), th.sin()];
        let dir = unit(add(scale([1.0, 0.0, 0.0], 0.3), scale(q, -1.0))).unwrap();
        let across = unit(cross(nb, dir)).unwrap();
        let heel = [0.0, 0.01, 0.012];
        let (tf, gap) = (0.008, 0.01);
        let tilted = |back: bool| Tilted {
            off: 8.0,
            dir,
            bolts: vec![(0.03, 0), (0.08, 0)],
            lu: 1.0,
            half: 1.5,
            nb,
            across,
            heel,
            axis: heel,
            width: 0.06,
            gauges: vec![0.03],
            shift_dir: [1.0, 0.0, 0.0],
            shift_per: 1.0,
            keep: (0.025, 0.03),
            x3: [1.0, 0.0, 0.0],
            out,
            face: [0.0; 3],
            band: (across_f, width),
            back,
        };
        let room = Room::Bend { down: false };
        let cut = 0.02;
        let e = own_geometry(&tilted(false), cut, 0.0, [0.0; 3], out, tf, gap, room)
            .expect_err("refused before");
        assert!(e.contains("runs down towards the flange"), "{e}");
        let t = tilted(true);
        let geo = own_geometry(&t, cut, 0.0, [0.0; 3], out, tf, gap, room).expect("laid out");
        assert!((geo.angle - 8.0).abs() < 0.01, "{}", geo.angle);
        // Up the flap from the bend line (in its plane), each point lifted.
        let face = scale(out, tf);
        let up = |p: [f64; 3]| dot(out, sub(add(p, scale(out, geo.lift)), face)) / th.sin();
        let curved = (bend_radius(tf) + tf) * (th / 2.0).tan();
        let edge = |s: f64, w: f64| add(heel, add(scale(dir, s), scale(across, w)));
        for w in [0.0, t.width] {
            // Where the edge leaves the flange's band, past its toe by half
            // the gap.
            let rate = dot(sub(edge(1.0, w), edge(0.0, w)), across_f);
            let exit = (width + gap / 2.0 - dot(edge(0.0, w), across_f)) / rate;
            assert!(exit > cut && exit < t.half, "{exit}");
            for s in [cut, exit] {
                assert!(
                    up(edge(s, w)) >= curved + gap / 2.0 - 1e-9,
                    "{s} {w} {}",
                    up(edge(s, w))
                );
            }
            // Beyond: past the bend's plane at half its length.
            assert!(up(edge(t.half, w)) < 0.0);
        }
        // Its bolts their edge distance (grown) and the curved part up it.
        for (s, _) in &t.bolts {
            let b = edge(cut + s, 0.03);
            assert!(up(b) >= t.keep.0 * super::RING_GROW + curved - 1e-9);
        }
    }
}

#[cfg(test)]
mod slide_tests {
    use super::{holds_clear, slide_holds};

    /// L2's follow-up (b): a bent plate's holding bolts on a hole of the
    /// leg slide along it the least that keeps the default minimum spacing
    /// from every hole — away from the work point first on a tie, back
    /// towards it where that is less —, and stay where already clear.
    #[test]
    fn the_holding_bolts_slide_clear_of_the_holes() {
        let (gauge, spacing) = (0.045, 0.048);
        let bx = [0.10, 0.148];
        let near =
            |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
        // A hole under the first bolt: out by the spacing (back would put
        // the second bolt on it).
        let holes = [[0.10, gauge]];
        let clear = |b: [f64; 2]| holds_clear(b, gauge, &holes, spacing);
        let b = slide_holds(bx, 1.0, 0.3, clear).unwrap();
        assert!(near(b, [0.148, 0.196]), "{b:?}");
        // A hole midway between them: 72 mm either way, a tie — away from
        // the work point first, whichever way the leg runs (`out`).
        let holes = [[0.124, gauge]];
        let clear = |b: [f64; 2]| holds_clear(b, gauge, &holes, spacing);
        let b = slide_holds(bx, 1.0, 0.3, clear).unwrap();
        assert!(near(b, [0.172, 0.220]), "{b:?}");
        let b = slide_holds(bx, -1.0, 0.3, clear).unwrap();
        assert!(near(b, [0.028, 0.076]), "{b:?}");
        // A hole just ahead of the second bolt: back towards the work
        // point by the least that clears it (6 mm), not out past it.
        let holes = [[0.19, gauge]];
        let clear = |b: [f64; 2]| holds_clear(b, gauge, &holes, spacing);
        let b = slide_holds(bx, 1.0, 0.3, clear).unwrap();
        assert!(near(b, [0.094, 0.142]), "{b:?}");
        // A hole off the gauge line by the spacing: they stay.
        let holes = [[0.10, gauge + spacing]];
        let clear = |b: [f64; 2]| holds_clear(b, gauge, &holes, spacing);
        assert_eq!(slide_holds(bx, 1.0, 0.3, clear), Some(bx));
        // Holes every 40 mm along the gauge line: no place within the reach
        // (the layout then leaves the plate, said).
        let holes: Vec<[f64; 2]> = (-20..40).map(|k| [f64::from(k) * 0.04, gauge]).collect();
        let clear = |b: [f64; 2]| holds_clear(b, gauge, &holes, spacing);
        assert_eq!(slide_holds(bx, 1.0, 0.3, clear), None);
    }
}

#[cfg(test)]
mod narrow_tests {
    use super::narrow;

    /// L5: the cuts that keep a bolt within a band, whichever way it moves
    /// with the cut, and none where it stands still outside the band.
    #[test]
    fn the_cuts_keeping_a_bolt_on_the_flange() {
        let (mut lo, mut hi) = (0.0, 1.0);
        // y = 0.02 + 0.5 c within [0.03, 0.08]: c within [0.02, 0.12].
        narrow(&mut lo, &mut hi, 0.02, 0.5, 0.03, 0.08);
        assert!(
            (lo - 0.02).abs() < 1e-12 && (hi - 0.12).abs() < 1e-12,
            "{lo} {hi}"
        );
        // Moving the other way: y = 0.10 - 0.5 c within [0.03, 0.08]: c within
        // [0.04, 0.14], narrowed with the above to [0.04, 0.12].
        narrow(&mut lo, &mut hi, 0.10, -0.5, 0.03, 0.08);
        assert!(
            (lo - 0.04).abs() < 1e-12 && (hi - 0.12).abs() < 1e-12,
            "{lo} {hi}"
        );
        // An end distance only from below.
        narrow(&mut lo, &mut hi, 0.0, 1.0, 0.05, f64::INFINITY);
        assert!(
            (lo - 0.05).abs() < 1e-12 && (hi - 0.12).abs() < 1e-12,
            "{lo} {hi}"
        );
        // Standing still outside the band: no cut.
        let (mut lo2, mut hi2) = (0.0, 1.0);
        narrow(&mut lo2, &mut hi2, 0.2, 0.0, 0.03, 0.08);
        assert!(lo2 > hi2);
    }
}

#[cfg(test)]
mod hold_clear_tests {
    use super::*;
    use crate::joints_tests::library;

    /// The invented K-brace with its leg L1 running on into L2 (one part):
    /// the gusset on the leg is held by its own bolts (L5o).
    fn joint_n2() -> JointDetail {
        let text = format!(
            "{}\n[[connection]]\nmember = \"L1\"\njoint = \"N2\"\nkind = \"continuous\"\nonto = \"L2\"\n",
            crate::joint_layout_tests::K.replace(
                "design = { class = \"bracing\" }",
                "design = { class = \"bracing\", bolts = { diameter = \"16 mm\", count = 4 } }",
            )
        );
        let d = crate::joints::joint_details(&text, &library(), &Default::default()).unwrap();
        d.joint("N2").unwrap().clone()
    }

    fn settings() -> LayoutSettings {
        LayoutSettings {
            bolt: Some(BoltArea::picked(None, 0.016, None)),
            ..Default::default()
        }
    }

    fn plan(j: &JointDetail, avoid: &HoldAvoid) -> (JointPlan, HoldAvoid, usize) {
        let (p, _, found, n) = plan_avoiding(
            j,
            &settings(),
            &|_, _| None,
            &BTreeMap::new(),
            &BTreeMap::new(),
            NODE_LIMIT,
            &PlanCtx::default(),
            &BTreeSet::new(),
            avoid,
        );
        (p, found, n)
    }

    /// A thin box square to `dir` at `c` (from the work point), reaching
    /// 0.5 m every other way.
    fn wall(c: [f64; 3], dir: [f64; 3], half: f64) -> Vec<[f64; 3]> {
        let any = if dir[2].abs() < 0.9 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let e1 = unit(cross(dir, any)).unwrap();
        let e2 = cross(dir, e1);
        let mut out = Vec::new();
        for a in [-half, half] {
            for b in [-0.5, 0.5] {
                for k in [-0.5, 0.5] {
                    out.push(add(add(c, scale(dir, a)), add(scale(e1, b), scale(e2, k))));
                }
            }
        }
        out
    }

    /// L22: the gusset's own bolts as placed meet nothing the plan lays
    /// out (none found); asked to keep clear of a wall across the leg
    /// through the first of them, they move along the leg clear of it by
    /// the nut and half the gap (said), the same each time; asked to keep
    /// clear of everything round the joint, no place is clear — placed as
    /// before, said.
    #[test]
    fn a_plates_own_bolts_keep_clear_of_what_they_are_asked_to() {
        let j = joint_n2();
        let (p0, found, n) = plan(&j, &HoldAvoid::new());
        assert_eq!(n, 0, "{found:?}");
        assert!(found.is_empty());
        let flat = p0.plates.iter().find(|q| q.hold.is_some()).expect("held");
        let h0 = flat.hold.clone().unwrap();
        let mut m: Vec<&str> = flat.members.iter().map(String::as_str).collect();
        m.sort_unstable();
        let key = format!("{}|{}", h0.member, m.join(","));
        let host = j.arrivals.iter().find(|a| a.member == h0.member).unwrap();
        let a0 = h0.points[0][0];
        // Either way along the leg (whichever way the points run).
        let half = 0.002;
        let walls: Vec<Vec<[f64; 3]>> = [a0, -a0]
            .iter()
            .map(|a| wall(scale(host.direction, *a), host.direction, half))
            .collect();
        let avoid: HoldAvoid = [(key.clone(), walls)].into();
        let (p1, _, _) = plan(&j, &avoid);
        let h1 = p1
            .plates
            .iter()
            .find_map(|q| q.hold.clone())
            .expect("still held");
        // (Fewer where no more fit clear, at least two: the joint checks
        // check them.)
        assert!(h1.points.len() >= 2, "{h1:?}");
        assert_ne!(h1.points, h0.points);
        let s = settings();
        let nut = BOLT_DRAWN[0] * 0.016 / 3f64.sqrt();
        for q in &h1.points {
            assert!(
                (q[0].abs() - a0.abs()).abs() >= half + nut + s.gap / 2.0 - 2e-4,
                "{q:?} {a0}"
            );
        }
        assert!(
            p1.notes
                .iter()
                .any(|w| w.contains("placed clear of the plates")),
            "{:?}",
            p1.notes
        );
        assert_eq!(plan(&j, &avoid).0, p1);
        // Everything round the joint: as before, said.
        let all: HoldAvoid = [(key, vec![wall([0.0; 3], host.direction, 1.0)])].into();
        let (p2, _, _) = plan(&j, &all);
        let h2 = p2.plates.iter().find_map(|q| q.hold.clone()).unwrap();
        assert_eq!(h2.points, h0.points);
        assert!(
            p2.notes
                .iter()
                .any(|w| w.contains("not clear of the plates")),
            "{:?}",
            p2.notes
        );
    }

    /// L22: off, the plan is the first as before; on, a joint whose
    /// plates' own bolts meet nothing is planned once, the same.
    #[test]
    fn a_joint_whose_holding_bolts_meet_nothing_is_planned_as_before() {
        let j = joint_n2();
        let on = plan_joint(&j, &settings(), &|_, _| None);
        let off = plan_joint(
            &j,
            &LayoutSettings {
                holds_clear: false,
                ..settings()
            },
            &|_, _| None,
        );
        assert_eq!(on, off);
        assert!(on.plates.iter().any(|q| q.hold.is_some()));
    }
}
