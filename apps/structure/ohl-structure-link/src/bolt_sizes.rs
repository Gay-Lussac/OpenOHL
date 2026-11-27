// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **One bolt size per joint in Lay out joints** (O49 stage 1c; the
//! owner's words of 2026-11-27: "for the bolts please try to use the same
//! ones for a joint. this would ease the construction once outside";
//! built, in active development, not approved yet).
//!
//! - **Provisional bolts** ([`joint_size`]): a member end without bolts of
//!   its own gets the layout's provisional bolts (its class's preset, the
//!   detailing practice's picked bolt); where the joint's ends with bolts of
//!   their own have one size, that size is taken instead — the preset's
//!   count and lines kept, its pitch and end distance left to the code's
//!   rules for the joint's bolt — and said in the bolts' words.
//! - **What stays mixed** ([`mixed_note`]): after a run, the joints laid
//!   out whose bolts are of several sizes, listed in the run's notes (the
//!   members' own bolts, a laid-out end's, a plate's own; one size per
//!   joint is a preference, never silent where it is not met).
//!
//! A plate's own holding bolts take the bolts of the member they bolt it
//! onto (the joint's leg), and a second plate's own bolts the end's or the
//! leg's they share ([`crate::two_plates`]): both already one of the
//! joint's sizes.

use crate::joint_layout::BoltArea;
use crate::joints::JointDetail;

/// The provisional bolts `pick` of a member end at joint `j` with the
/// joint's size where the joint's ends with bolts of their own have one
/// diameter (to 0.1 mm) other than `pick`'s ([module docs](self)).
pub fn joint_size(j: &JointDetail, pick: BoltArea) -> BoltArea {
    let mut sizes: Vec<(i64, &crate::joints::BoltSet)> = Vec::new();
    for a in &j.arrivals {
        if !a.bolts.known() {
            continue;
        }
        if let Some(b) = a.bolts.value.as_ref() {
            let d = (b.diameter * 1e4).round() as i64;
            if !sizes.iter().any(|s| s.0 == d) {
                sizes.push((d, b));
            }
        }
    }
    let [(d, b)] = sizes.as_slice() else {
        return pick;
    };
    if *d == (pick.diameter * 1e4).round() as i64 {
        return pick;
    }
    BoltArea {
        from: format!(
            "{} — at the joint's bolt size, {:.0} mm (one bolt size per joint; {:.0} mm asked)",
            pick.from,
            b.diameter * 1e3,
            pick.diameter * 1e3
        ),
        bolt: b.bolt.clone(),
        diameter: b.diameter,
        flats: b.drawn[0],
        pitch: None,
        end: None,
        ..pick
    }
}

/// The joints of `text` (those asked: `asked`; none: every joint) whose
/// bolts are of several sizes, in words for a layout run's notes (none:
/// every such joint has one size).
pub fn mixed_note(
    text: &str,
    library: &ohl_sections::SectionLibrary,
    bolts: &ohl_structures::bolts::BoltLibrary,
    asked: Option<&std::collections::BTreeSet<String>>,
) -> Option<String> {
    use ohl_structures::LibraryItem as _;
    let t =
        ohl_structures::StructureType::parse(std::path::Path::new("structure.ohls"), text).ok()?;
    let model = crate::model_of(&t, text, None, library).ok()?;
    let js = crate::sizing_bolts::JointSizes::of(&model, &t, bolts);
    let none = vec![None; model.members.len()];
    let (_, mixed) = js.count(&model, &none);
    let mixed: Vec<String> = mixed
        .into_iter()
        .filter(|(j, _)| asked.is_none_or(|a| a.contains(j)))
        .map(|(j, w)| format!("{j} ({w})"))
        .collect();
    if mixed.is_empty() {
        return None;
    }
    let n = mixed.len();
    Some(format!(
        "bolt sizes mixed at {n} joint(s) (one bolt size per joint wherever it can, the owner \
         2026-11-27; these keep the sizes their members' bolts give — change a member's bolts, \
         or choose the members again, to have one): {}{}",
        mixed
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join(", "),
        if n > 20 {
            format!(" and {} more", n - 20)
        } else {
            String::new()
        }
    ))
}
