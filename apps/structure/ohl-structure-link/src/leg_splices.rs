// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Leg splices proposed** (O44 J7, part a; the owner's decisions of
//! 2026-11-19): "a change of section is a splice; the same section is one
//! part unless marked; a command proposes the splice places and the user
//! confirms". Lay out joints' preview lists them; **never written
//! silently**: *Confirm* per joint or *Confirm all*, each one undo step
//! ([`confirm`]). Until confirmed each piece ends at the joint in the
//! checks and in the layout (on the safe side), as for pieces in line
//! ([`crate::in_line`], J9).
//!
//! Two angles in line through a joint (within [`NESTED_ANGLE_DEG`], their
//! legs along each other within it — the nesting tolerance, as step 1c
//! had it) of **other sections**, both pieces of a **leg** (class `leg`),
//! are proposed spliced: a `[[splice]]` at the joint, its covers
//! outer and inner on each leg bolted (both legs where an end bolts by
//! both), as thick as the thicker piece's leg (**the program's starting
//! value**, not a standard's: the covers are checked as plates for their
//! share, which says when it is too thin), both ends bolted onto it. The
//! bolts are the import's where it gives them (the members' design data:
//! a leg's splice bolts), else provisional (the layout's preset for the
//! class or the bolt picked; the tower then stays level 3). Two pieces of
//! other sections of no leg are proposed only where both have bolts and
//! nothing else bolts onto them there (step 1c's rule of
//! 2026-11-19, which wrote such splices itself before J7).
//!
//! Pieces of **one section** of a leg with bolts are J9's pieces in line
//! ([`crate::InLine::splice_bolts`]): proposed one part by the owner's
//! rule, listed with the splices.
//!
//! Not proposed, said why (pieces of a leg; pieces of no leg are listed
//! only where proposed): a splice given at the joint naming either piece,
//! a `kind`, `onto`, `face` or bolts given by hand at either end, not
//! single angles (a built-up leg's splice: by hand), in line within more
//! than the nesting tolerance (a leg's knee), legs not along each other,
//! the smaller angle nesting in the bigger (the layout bolts it nested,
//! step 1b).

use std::collections::BTreeSet;

use ohl_structures::StructureType;
use ohl_structures::connections::{BoltedLeg, CoverLeg};
use toml_edit::DocumentMut;

use crate::joint_layout::BoltArea;
use crate::joints::{Arrival, JointDetail, JointDetails, NESTED_ANGLE_DEG, Source};

/// Two pieces in line through a joint, of other sections: a splice
/// proposed, or not and why.
#[derive(Debug, Clone, PartialEq)]
pub struct LegSplice {
    /// The joint.
    pub joint: String,
    /// The piece of the smaller id.
    pub member: String,
    /// The other piece.
    pub onto: String,
    /// None: proposed; else why not.
    pub not: Option<String>,
    /// The legs its covers lie on (outer and inner on each).
    pub covers: Vec<CoverLeg>,
    /// The covers' thickness, m: the thicker piece's leg.
    pub thickness: f64,
    /// The pieces without bolts (neither given nor the import's): given
    /// provisional bolts when confirmed.
    pub provisional: Vec<String>,
}

impl LegSplice {
    /// Proposed (to confirm).
    pub fn proposed(&self) -> bool {
        self.not.is_none()
    }

    /// In words: "L1 and L2 at 12".
    pub fn words(&self) -> String {
        format!("{} and {} at {}", self.member, self.onto, self.joint)
    }

    /// What is assumed, in words: the covers, their thickness, the bolts.
    pub fn assumed(&self) -> String {
        let legs: Vec<&str> = self.covers.iter().map(|l| l.as_str()).collect();
        let bolts = if self.provisional.is_empty() {
            "the bolts the import gives (their members' design data)".to_owned()
        } else {
            format!(
                "provisional bolts for {} (none given), the import's for the other",
                self.provisional.join(" and ")
            )
        };
        format!(
            "covers outer and inner on the {} leg{}, {} thick (the thicker piece's leg: the \
             program's starting value, checked as plates); {bolts}",
            legs.join(" and "),
            if legs.len() > 1 { "s" } else { "" },
            crate::mm_text(self.thickness)
        )
    }
}

/// The splices proposed (or not, with why) at the joints `joints` of `d`
/// (none: every joint), the structure `t`: pieces in line of other
/// sections, both of a leg (else both of no leg, with bolts and nothing
/// else on them: step 1c's rule); in the order of `d`'s joints, then by the
/// pieces' ids. `d` is the joints as the layout plans them (its own
/// values taken out).
pub fn leg_splices(
    d: &JointDetails,
    t: &StructureType,
    joints: Option<&BTreeSet<String>>,
) -> Vec<LegSplice> {
    let mut out = Vec::new();
    for j in &d.joints {
        if joints.is_some_and(|x| !x.contains(&j.id)) {
            continue;
        }
        out.extend(at_joint(j, t));
    }
    out
}

/// [`leg_splices`] at one joint.
fn at_joint(j: &JointDetail, t: &StructureType) -> Vec<LegSplice> {
    let leg = |a: &Arrival| a.class.as_deref() == Some("leg");
    let bolted = |a: &Arrival| a.bolts.value.as_ref().is_some_and(|b| b.count > 0);
    // Pairs in line (within 20°: what the layout takes as in line), the
    // nearest first; each piece once.
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for x in 0..j.arrivals.len() {
        for y in x + 1..j.arrivals.len() {
            let (a, b) = (&j.arrivals[x], &j.arrivals[y]);
            if a.modelling || b.modelling || a.continuous.is_some() || b.continuous.is_some() {
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
    let mut used: BTreeSet<usize> = BTreeSet::new();
    let mut out = Vec::new();
    for (dev, x, y) in pairs {
        if used.contains(&x) || used.contains(&y) {
            continue;
        }
        let (a, b) = (&j.arrivals[x], &j.arrivals[y]);
        let (sa, sb) = (section(a), section(b));
        if sa.is_none() || sa == sb {
            // (One section: J9's pieces in line; no section: nothing.)
            continue;
        }
        let alone = j.arrivals.iter().all(|z| {
            z.member == a.member
                || z.member == b.member
                || z.modelling
                || z.onto
                    .value
                    .as_deref()
                    .is_some_and(|o| o != a.member && o != b.member)
        });
        // Two pieces of a leg; else (step 1c's rule) two pieces of no leg,
        // both with bolts, nothing else on them.
        let legs = leg(a) && leg(b);
        let others = !leg(a) && !leg(b) && bolted(a) && bolted(b) && alone;
        if !(legs || others) {
            continue;
        }
        used.extend([x, y]);
        // Spliced together already (given, or confirmed): nothing to propose.
        if t.splices.iter().any(|s| {
            s.joint == j.id && s.members.contains(&a.member) && s.members.contains(&b.member)
        }) {
            continue;
        }
        let (a, b) = if a.member <= b.member { (a, b) } else { (b, a) };
        let mut s = LegSplice {
            joint: j.id.clone(),
            member: a.member.clone(),
            onto: b.member.clone(),
            not: None,
            covers: Vec::new(),
            thickness: 0.0,
            provisional: Vec::new(),
        };
        s.not = why_not(j, t, a, b, dev);
        // (Pieces of no leg: listed only where proposed, as step 1c made
        // them; the others are no splice place of a leg.)
        if !legs && s.not.is_some() {
            continue;
        }
        if s.not.is_none() {
            let (ga, gb) = (
                a.profile.as_ref().and_then(|p| p.angle).expect("checked"),
                b.profile.as_ref().and_then(|p| p.angle).expect("checked"),
            );
            let mut legs: Vec<CoverLeg> = Vec::new();
            for z in [a, b] {
                let of = if z.leg.value == Some(BoltedLeg::Both) {
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
            s.covers = legs;
            s.thickness = ga.t.max(gb.t);
            s.provisional = [a, b]
                .iter()
                .filter(|z| !bolted(z))
                .map(|z| z.member.clone())
                .collect();
        }
        out.push(s);
    }
    out
}

/// Why pieces `a` and `b` in line at joint `j` (`dev` degrees off) are not
/// proposed spliced (none: proposed).
fn why_not(
    j: &JointDetail,
    t: &StructureType,
    a: &Arrival,
    b: &Arrival,
    dev: f64,
) -> Option<String> {
    if let Some(s) = t
        .splices
        .iter()
        .find(|s| s.joint == j.id && s.members.iter().any(|m| *m == a.member || *m == b.member))
    {
        return Some(format!(
            "a splice is given here ({}): detailed by hand",
            s.id
        ));
    }
    for z in [a, b] {
        let given = [
            ("kind", z.kind.source),
            ("onto", z.onto.source),
            ("face", z.sits.source),
            ("bolts", z.bolts.source),
        ]
        .into_iter()
        .find(|(_, s)| *s == Source::Given);
        if let Some((f, _)) = given {
            return Some(format!(
                "{}'s {f} here is given by hand: a joint detailed by hand",
                z.member
            ));
        }
    }
    let (Some(pa), Some(pb)) = (a.profile.as_ref(), b.profile.as_ref()) else {
        return Some("no profile".into());
    };
    let (Some(ga), Some(gb)) = (pa.angle, pb.angle) else {
        return Some(
            "not two single angles (a built-up or other section): a splice by hand".into(),
        );
    };
    if dev > NESTED_ANGLE_DEG {
        return Some(format!(
            "in line within {dev:.1}° only (a splice of covers: within {NESTED_ANGLE_DEG}°)"
        ));
    }
    let c = NESTED_ANGLE_DEG.to_radians().cos();
    if dot(pa.axes[1], pb.axes[1]) < c || dot(pa.axes[2], pb.axes[2]) < c {
        return Some(format!(
            "their legs are not along each other (more than {NESTED_ANGLE_DEG}° apart): a \
             splice by hand"
        ));
    }
    let fits = |s: crate::joints::Angle, g: crate::joints::Angle| {
        s.b <= g.b - g.t - g.r + 1e-9 && s.h <= g.h - g.t - g.r + 1e-9
    };
    if fits(ga, gb) || fits(gb, ga) {
        return Some(
            "the smaller angle nests in the bigger: bolted nested by the layout (step 1b)".into(),
        );
    }
    None
}

/// The section of `a` (none: no profile).
fn section(a: &Arrival) -> Option<&str> {
    a.profile.as_ref().map(|p| p.section_id.as_str())
}

/// The splices `list` proposed (those with no reason against) written into
/// `doc`: each a `[[splice]]` at its joint (the first free id `S1`, `S2`…),
/// its covers outer and inner on each leg, both ends `onto` it; the pieces
/// without bolts given `bolt` (provisional: listed so in the layout's
/// record, the tower staying level 3; none: left without, said). The
/// splices written, and those that could not be (in words, with why).
pub fn confirm(
    doc: &mut DocumentMut,
    t: &StructureType,
    list: &[LegSplice],
    bolt: Option<&BoltArea>,
) -> Result<(usize, Vec<String>), String> {
    let mut record = crate::layout_run::LayoutRecord::of(t);
    let mut conns = ohl_structure_edit::connection_edits::Connections::of(doc);
    let mut taken = crate::layout_run::ids_of(doc, t);
    let mut n = 0;
    let mut not = Vec::new();
    let mut touched = false;
    for s in list.iter().filter(|s| s.proposed()) {
        if t.splices.iter().any(|x| {
            x.joint == s.joint && (x.members.contains(&s.member) || x.members.contains(&s.onto))
        }) {
            not.push(format!("{}: a splice is there already", s.words()));
            continue;
        }
        // The layout's own values at the two ends go (a run before laid
        // them out as pieces ending there).
        for m in [&s.member, &s.onto] {
            let key = (m.clone(), s.joint.clone());
            if let Some(fields) = record.ends.remove(&key) {
                for f in fields {
                    if f == "gauge" {
                        continue;
                    }
                    if f == "preset" {
                        for k in ohl_structure_edit::connection_edits::CARRIED {
                            let _ = conns.set(doc, m, &s.joint, k, None);
                        }
                        continue;
                    }
                    let _ = conns.set(doc, m, &s.joint, &f, None);
                }
                touched = true;
            }
            touched |= record.provisional.remove(&key);
        }
        crate::layout_run::write_splice(
            doc,
            &mut conns,
            &s.joint,
            [&s.member, &s.onto],
            &s.covers,
            s.thickness,
            &mut taken,
        );
        for m in &s.provisional {
            match bolt {
                Some(b) => {
                    crate::layout_run::write_bolts(doc, &mut conns, m, &s.joint, b);
                    record.provisional.insert((m.clone(), s.joint.clone()));
                    touched = true;
                }
                None => not.push(format!(
                    "{}: {m} has no bolts and no bolt is picked (spliced without)",
                    s.words()
                )),
            }
        }
        n += 1;
    }
    conns.tidy(doc);
    if touched {
        record.write(doc);
    }
    Ok((n, not))
}

/// The leg splice places `one_part` (pieces of one section of a leg with
/// bolts, [`crate::InLine::splice_bolts`]: one part) and `splices` (other
/// sections: spliced) confirmed together into `doc` — one edit, one undo
/// step for the caller. The pairs one part, the splices written, and those
/// that could not be (in words, with why).
pub fn confirm_places(
    doc: &mut DocumentMut,
    t: &StructureType,
    one_part: &[crate::InLine],
    splices: &[LegSplice],
    bolt: Option<&BoltArea>,
) -> Result<(usize, usize, Vec<String>), String> {
    let (n, mut not) = crate::in_line::confirm(doc, t, one_part)?;
    if splices.iter().all(|s| !s.proposed()) {
        return Ok((n, 0, not));
    }
    // (The record as the one part left it.)
    let now = <StructureType as ohl_structures::LibraryItem>::parse(
        std::path::Path::new("structure.ohls"),
        &doc.to_string(),
    )
    .map_err(|e| e.message)?;
    let (m, more) = confirm(doc, &now, splices, bolt)?;
    not.extend(more);
    Ok((n, m, not))
}

/// [`confirm`] on a structure file's text: the text after.
pub fn confirm_text(
    text: &str,
    list: &[LegSplice],
    bolt: Option<&BoltArea>,
) -> Result<String, String> {
    let t = <StructureType as ohl_structures::LibraryItem>::parse(
        std::path::Path::new("structure.ohls"),
        text,
    )
    .map_err(|e| e.message)?;
    let mut doc: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    confirm(&mut doc, &t, list, bolt)?;
    Ok(doc.to_string())
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The angle between two directions, degrees.
fn angle_deg(a: [f64; 3], b: [f64; 3]) -> f64 {
    let la = dot(a, a).sqrt();
    let lb = dot(b, b).sqrt();
    if la < 1e-12 || lb < 1e-12 {
        return 0.0;
    }
    (dot(a, b) / (la * lb)).clamp(-1.0, 1.0).acos().to_degrees()
}
