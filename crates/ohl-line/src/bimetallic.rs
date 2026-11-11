// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What the results say of the bimetallic model (roadmap O22 item 1).
//!
//! Above its knee point a bimetallic wire (aluminium on steel) is
//! computed in one of three ways ([`ohl_cables::BimetallicUse`]): the
//! outer strands in compression without limit (the default: the
//! composite's expansion, the **largest** hot sag and the lowest
//! tension), in compression up to a limit, or slack (a bird cage: the
//! core alone, the smallest hot sag). Two things are said:
//!
//! - per line, which model each bimetallic wire type has and in which
//!   sections ([`bimetallic_notes`]) — the models other than the default
//!   as "NOT ON THE SAFE SIDE" (their hot sag is not the largest);
//! - per solved weather case, the spans whose state is past the knee
//!   point ([`past_knee_flag`], in [`crate::solve::SectionShape::flags`]):
//!   there the default's tension is the lowest of the models (not on the
//!   safe side for the tension), and the other models' sag is not the
//!   largest.

use std::collections::BTreeMap;

use ohl_cables::mechanics::OuterState;
use ohl_cables::{BimetallicUse, megapascals};

use crate::Line;

/// The most sections a note names (the others: "…").
const NAMED_SECTIONS: usize = 10;

/// What the results of `line` say of its bimetallic wires: per wire type
/// with a core that a section is strung with, the model it is computed
/// with (its own, else the line's: [`ohl_cables::BimetallicModel::apply`])
/// and the sections concerned, up to ten named
/// ([`BimetallicUse::note`]). The default (compression without limit)
/// says its bound; the other models start with "NOT ON THE SAFE SIDE: ".
/// Empty: no section with a bimetallic wire type of `library`.
pub fn bimetallic_notes(line: &Line, library: &ohl_cables::Library) -> Vec<String> {
    let mut by_wire: BTreeMap<&str, (BimetallicUse, Vec<&str>)> = BTreeMap::new();
    for section in &line.sections {
        let Some(cable) = library.get(&section.cable) else {
            continue;
        };
        let Some(model) = line.bimetallic.apply(cable).bimetallic_model() else {
            continue;
        };
        by_wire
            .entry(section.cable.as_str())
            .or_insert((model, Vec::new()))
            .1
            .push(section.id.as_str());
    }
    by_wire
        .into_iter()
        .map(|(wire, (model, sections))| model.note(wire, &named(&sections)))
        .collect()
}

/// "section A" / "sections A, B, …": the first ten.
fn named(sections: &[&str]) -> String {
    let mut first: Vec<&str> = sections.iter().copied().take(NAMED_SECTIONS).collect();
    if sections.len() > NAMED_SECTIONS {
        first.push("…");
    }
    format!(
        "section{} {}",
        if sections.len() == 1 { "" } else { "s" },
        first.join(", ")
    )
}

/// What the notes of bimetallic spans past the knee point start with
/// after "span k: " ([`past_knee_flag`]).
pub const PAST_KNEE: &str = "outer strands ";

/// The flag of span `span` (0 = first) whose outer strands are in
/// `state` in a solved weather case; none below the knee point. Past it:
///
/// - in compression (the default, or a limit not reached): the sag is the
///   largest of the models, the tension the lowest — not on the safe side
///   for the tension;
/// - at a compression limit: neither the largest sag nor the highest
///   tension;
/// - slack (bird cage): the smallest sag — not on the safe side for the
///   sag.
pub fn past_knee_flag(span: usize, state: OuterState) -> Option<String> {
    let k = span + 1;
    match state {
        OuterState::Tension => None,
        OuterState::Compression(stress) => Some(format!(
            "span {k}: {PAST_KNEE}in compression ({} MPa virtual): NOT ON THE SAFE SIDE for the \
             tension — a bird cage gives the higher one",
            megapascals(stress)
        )),
        OuterState::AtLimit(stress) => Some(format!(
            "span {k}: {PAST_KNEE}at their compression limit ({} MPa virtual): NOT ON THE SAFE \
             SIDE for the sag — compression without limit gives the larger one — nor for the \
             tension — a bird cage gives the higher one",
            megapascals(stress)
        )),
        OuterState::Slack => Some(format!(
            "span {k}: {PAST_KNEE}slack (bird cage): NOT ON THE SAFE SIDE for the sag"
        )),
    }
}

/// The outer strands' state of a bimetallic cable **without curves** in
/// the finite-element calculation, at the mean tension `tension` (N) of a
/// span `dt` kelvin warmer than at stringing — the closed form of the
/// composite: \\( \varepsilon = T / EA + \bar\alpha \Delta t \\), the outer
/// load \\( E_oA_o (\varepsilon - \alpha_o \Delta t) \\) against the limit.
/// None without a core.
pub fn linear_outer_state(cable: &ohl_cables::Cable, tension: f64, dt: f64) -> Option<OuterState> {
    cable.core.as_ref()?;
    let limit = cable.outer_compression_limit_with_curves();
    let area = cable.area().si();
    let strain = tension / cable.axial_stiffness().si() + cable.thermal_expansion() * dt;
    let outer = cable.outer.axial_stiffness().si() * (strain - cable.outer.thermal_expansion * dt);
    Some(if outer >= 0.0 {
        OuterState::Tension
    } else if outer >= -limit {
        OuterState::Compression(-outer / area)
    } else if limit > 0.0 {
        OuterState::AtLimit(limit / area)
    } else {
        OuterState::Slack
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_cables::BimetallicBehavior;

    fn library() -> ohl_cables::Library {
        ohl_cables::Library::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap()
    }

    /// The notes name the wire type, its model and its sections; the
    /// default unmarked, the others not on the safe side; a wire type
    /// without a core says nothing.
    #[test]
    fn notes_name_the_model_and_the_sections() {
        let library = library();
        let mut line = crate::tests::example();
        let wire = line.sections[0].cable.clone();
        assert!(library.get(&wire).unwrap().core.is_some(), "{wire}");
        let with_wire: Vec<&str> = line
            .sections
            .iter()
            .filter(|s| s.cable == wire)
            .map(|s| s.id.as_str())
            .collect();
        let notes = bimetallic_notes(&line, &library);
        let note = notes
            .iter()
            .find(|n| n.starts_with(&format!("wire type {wire} (section")))
            .unwrap_or_else(|| panic!("{notes:?}"));
        assert!(note.contains("the largest hot sag of the models"), "{note}");
        for id in with_wire.iter().take(NAMED_SECTIONS) {
            assert!(note.contains(id), "{note}");
        }
        assert!(notes.iter().all(|n| !n.contains("NOT ON THE SAFE SIDE")));
        // The line's choice: said as not on the safe side.
        line.bimetallic.behavior = BimetallicBehavior::BirdCage;
        let notes = bimetallic_notes(&line, &library);
        assert!(
            notes.iter().any(|n| n
                .starts_with(&format!("NOT ON THE SAFE SIDE: wire type {wire} (section"))
                && n.contains("modelled as a bird cage")),
            "{notes:?}"
        );
        line.bimetallic = ohl_cables::BimetallicModel {
            behavior: BimetallicBehavior::Compression,
            max_virtual_compression: Some(ohl_core::units::Stress::from_megapascals(20.0)),
        };
        assert!(
            bimetallic_notes(&line, &library)
                .iter()
                .any(|n| n.contains("with compression limited to 20 MPa"))
        );
        // More than ten sections: ten named.
        let many: Vec<String> = (0..12).map(|k| format!("S{k}")).collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        assert!(named(&refs).ends_with("S9, …"));
        assert_eq!(named(&refs[..1]), "section S0");
        // A wire type not in the library (its sections do not solve):
        // nothing to say; a homogeneous one has no model
        // (`Cable::bimetallic_model`, tested with the cables).
        for s in &mut line.sections {
            s.cable = "not-in-the-library".into();
        }
        assert!(bimetallic_notes(&line, &library).is_empty());
    }

    /// The closed form of the linear finite-element path: the textbook
    /// cable (benchmark `tbbim`: knee where the mean tension is 143.77 N/K
    /// × Δt) in tension below it, in compression, at a limit or slack
    /// above.
    #[test]
    fn linear_outer_state_finds_the_knee() {
        let library = library();
        let mut cable = library
            .iter()
            .find(|c| c.core.is_some())
            .expect("a bimetallic wire type")
            .clone();
        cable.outer.stress_strain = None;
        if let Some(core) = cable.core.as_mut() {
            core.stress_strain = None;
        }
        let core = cable.core.as_ref().unwrap();
        // The knee: the core carries E_cA_c (α_o − α_c) Δt.
        let per_kelvin =
            core.axial_stiffness().si() * (cable.outer.thermal_expansion - core.thermal_expansion);
        let dt = 100.0;
        let knee = per_kelvin * dt;
        assert_eq!(
            linear_outer_state(&cable, 1.01 * knee, dt),
            Some(OuterState::Tension)
        );
        let Some(OuterState::Compression(stress)) = linear_outer_state(&cable, 0.9 * knee, dt)
        else {
            panic!("in compression below the knee tension");
        };
        // The outer load: −(knee − T) · E_oA_o / EA.
        let expected = 0.1 * knee * cable.outer.axial_stiffness().si()
            / cable.axial_stiffness().si()
            / cable.area().si();
        assert!((stress - expected).abs() < 1e-6 * expected, "{stress}");
        cable.bimetallic_behavior = Some(BimetallicBehavior::BirdCage);
        assert_eq!(
            linear_outer_state(&cable, 0.9 * knee, dt),
            Some(OuterState::Slack)
        );
        cable.bimetallic_behavior = Some(BimetallicBehavior::Compression);
        cable.max_virtual_compression = Some(ohl_core::units::Stress::from_si(stress / 2.0));
        assert!(matches!(
            linear_outer_state(&cable, 0.9 * knee, dt),
            Some(OuterState::AtLimit(_))
        ));
        // The flags: none below the knee, each state's own above.
        assert_eq!(past_knee_flag(0, OuterState::Tension), None);
        assert_eq!(
            past_knee_flag(1, OuterState::Compression(3.2e6)).unwrap(),
            "span 2: outer strands in compression (3.2 MPa virtual): NOT ON THE SAFE SIDE for \
             the tension — a bird cage gives the higher one"
        );
        assert!(
            past_knee_flag(0, OuterState::Slack)
                .unwrap()
                .ends_with("NOT ON THE SAFE SIDE for the sag")
        );
    }
}
