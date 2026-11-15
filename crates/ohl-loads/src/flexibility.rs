// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A placed structure's stiffness at its attachment points, for the
//! line's coupled solve of structures and wires (calculation levels 3 and
//! 4, plan of 2026-11-15, phase 1; in active development — no result uses
//! it yet).
//!
//! The structure is the one the structure check analyses
//! ([`Analyses::model_of`]: its type's model, the extension's own when it
//! stands on one that changes it; else its pole, [`Analyses::pole_of`]),
//! its points linked as the check links their loads ([`placed_link`] and
//! [`ohl_structures_fe::Model::attachment_load`], one rule for both), the
//! matrices condensed in the structure's axes
//! ([`ohl_structures_fe::condense`]) and turned to the line's by its
//! orientation (its height adjustment, a translation, changes nothing).
//! A structure with neither a model nor a pole, or one refused, stays
//! rigid with the reason ([`Rigid`]).

use ohl_core::budget::Budget;
use ohl_structures::StructureType;
use ohl_structures_fe::condense::{
    Attachment, CondenseKey, Condensed, Linearisation, Link, Rigid, condense, condense_pole,
};
use ohl_structures_fe::{Loads, PoleLoad};

use crate::analysis::Analyses;

/// How placed structure `placed` links its attachment point `label`: its
/// own link (a relink, an offset) over the type's, unlinked when it says
/// so (2026-10-21) — the rule of the structure check's loads
/// ([`Analyses::fe`]) and of the condensation.
pub(crate) fn placed_link(placed: Option<&ohl_line::Structure>, label: &str) -> Link {
    match placed.and_then(|s| s.point(label)) {
        Some(p) if p.unlinked => Link::Unlinked,
        Some(p) => match &p.joint {
            Some(joint) => Link::Joint {
                joint: joint.clone(),
                offset: p.offset,
            },
            None => Link::Type,
        },
        None => Link::Type,
    }
}

/// The rotation from a structure's axes (right of the line, ahead, up)
/// to the line's for its orientation (rows of R, v_line = R v_local),
/// from the one convention ([`ohl_line::links::from_structure_axes`]).
pub(crate) fn to_line_axes(orientation_deg: f64) -> [[f64; 3]; 3] {
    let columns = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        .map(|e| ohl_line::links::from_structure_axes(orientation_deg, e));
    [0, 1, 2].map(|r| [0, 1, 2].map(|c| columns[c][r]))
}

/// A placed structure condensed to its attachment points.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Flexible {
    /// The memo's key (per type and extension: the orientation is not
    /// in it).
    pub key: CondenseKey,
    /// The matrices in the line's axes.
    pub condensed: Condensed,
}

impl Analyses {
    /// The stiffness of placed structure `placed` of type `t` at its
    /// attachment points `labels` (those carrying wires), in the line's
    /// axes. `reference`: the wires' forces at points at the reference
    /// state (the stringing state: label, N, the structure's axes,
    /// unfactored) — the tangent there, with the structure's self-weight
    /// when its model takes it; none: the elastic structure (first
    /// order).
    ///
    /// Expected cost: one reference solve and one factorisation of the
    /// structure's tangent plus a substitution per attachment direction
    /// (measured, theory: Structure flexibility); to be kept per
    /// [`Flexible::key`] by its caller.
    pub(crate) fn flexibility(
        &self,
        t: &StructureType,
        placed: Option<&ohl_line::Structure>,
        labels: &[String],
        reference: Option<&[(String, [f64; 3])]>,
        budget: &Budget,
    ) -> Result<Flexible, Rigid> {
        let extension = placed.and_then(|s| s.extension.as_deref());
        let orientation = placed.map_or(0.0, |s| s.orientation_deg);
        let (key, condensed) = if let Some((model, extended)) = self.model_of(t, extension) {
            let model = model.as_ref().map_err(|e| Rigid::ModelNotRead(e.clone()))?;
            let points: Vec<Attachment> = labels
                .iter()
                .map(|l| Attachment {
                    label: l.clone(),
                    link: placed_link(placed, l),
                })
                .collect();
            let linearisation = match reference {
                None => Linearisation::Unloaded,
                Some(forces) => {
                    let mut nodal = Vec::new();
                    for (label, force) in forces {
                        let link = placed_link(placed, label);
                        if link == Link::Unlinked {
                            return Err(Rigid::Unlinked(label.clone()));
                        }
                        let at = model
                            .attachment_load(label, &link, *force)
                            .ok_or_else(|| Rigid::NotInModel(label.clone()))?;
                        nodal.extend(at.loads);
                    }
                    Linearisation::Tangent(Loads {
                        nodal,
                        self_weight: if model.analysis.self_weight { 1.0 } else { 0.0 },
                        ..Loads::default()
                    })
                }
            };
            let key = CondenseKey::of_model(
                &t.id,
                extension.filter(|_| extended),
                model,
                &points,
                &linearisation,
            );
            (key, condense(model, &points, &linearisation, budget)?)
        } else if let Some(pole) = self.pole_of(&t.id) {
            let pole = pole.as_ref().map_err(|e| Rigid::ModelNotRead(e.clone()))?;
            // The points as the pole check places them: the type's.
            let points: Vec<(String, [f64; 3])> = labels
                .iter()
                .map(|l| {
                    t.point_local(l, None)
                        .map(|p| (l.clone(), p))
                        .ok_or_else(|| Rigid::NotInModel(l.clone()))
                })
                .collect::<Result<_, _>>()?;
            let wires: Option<Vec<PoleLoad>> = reference.map(|forces| {
                forces
                    .iter()
                    .filter_map(|(l, f)| {
                        Some(PoleLoad {
                            point: t.point_local(l, None)?,
                            force: *f,
                        })
                    })
                    .collect()
            });
            let key = CondenseKey::of_pole(&t.id, pole, &points, wires.as_deref());
            (key, condense_pole(pole, &points, wires.as_deref(), budget)?)
        } else {
            return Err(Rigid::NoModel);
        };
        Ok(Flexible {
            key,
            condensed: condensed.turned(to_line_axes(orientation)),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    fn ex() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line")
    }

    fn setup() -> (ohl_structures::StructureLibrary, ohl_line::Line, Analyses) {
        let types = ohl_structures::StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
        let line = ohl_line::Line::load(&ex().join("synthetic-line-typed.ohl.json"))
            .unwrap()
            .resolve(&types, &ohl_structures::InsulatorLibrary::default())
            .unwrap();
        let analyses = Analyses::load(&types, &ohl_sections::SectionLibrary::default());
        (types, line, analyses)
    }

    fn labels(s: &ohl_line::Structure) -> Vec<String> {
        s.strings.iter().map(|st| st.top.clone()).collect()
    }

    /// Two structures of one type (the example's type-02, a full design)
    /// turned differently: one key (the condensation is the type's), the
    /// matrices the structure-axes condensation turned by each one's
    /// orientation; at a reference with the wires' weight, softer or as
    /// stiff — never refused.
    #[test]
    fn placed_structures_are_their_types_condensation_turned() {
        let (types, line, analyses) = setup();
        let budget = ohl_structures_fe::default_budget();
        let t = types.get("type-02").unwrap();
        let t2 = line.structure("T2").unwrap();
        let t4 = line.structure("T4").unwrap();
        let points = labels(t2);
        assert!(!points.is_empty());
        let a = analyses
            .flexibility(t, Some(t2), &points, None, &budget)
            .unwrap();
        let b = analyses
            .flexibility(t, Some(t4), &labels(t4), None, &budget)
            .unwrap();
        assert_eq!(a.key, b.key);
        assert_ne!(a.condensed.flexibility, b.condensed.flexibility);
        let model = analyses.model("type-02", None).unwrap();
        let local = condense(
            model,
            &points
                .iter()
                .map(|l| Attachment::of_type(l))
                .collect::<Vec<_>>(),
            &Linearisation::Unloaded,
            &budget,
        )
        .unwrap();
        let turned = local.turned(to_line_axes(t4.orientation_deg));
        assert!(
            (&turned.flexibility - &b.condensed.flexibility).abs().max()
                < 1e-12 * turned.flexibility.abs().max()
        );
        // A force across the line in T4's axes moves its points across
        // T4 (turned with it): the same as the local matrix gives.
        let f_local = [1e3, 0.0, 0.0];
        let f_line = ohl_line::links::from_structure_axes(t4.orientation_deg, f_local);
        let u_line = b.condensed.deflection(&[(points[0].as_str(), f_line)])[0];
        let u_local = local.deflection(&[(points[0].as_str(), f_local)])[0];
        let back = ohl_line::links::to_structure_axes(t4.orientation_deg, u_line);
        for k in 0..3 {
            assert!((back[k] - u_local[k]).abs() < 1e-12, "{back:?} {u_local:?}");
        }
        // At a reference: the wires' weight on the points, the
        // self-weight.
        let wires: Vec<(String, [f64; 3])> = points
            .iter()
            .map(|l| (l.clone(), [0.0, 0.0, -20e3]))
            .collect();
        let at_ref = analyses
            .flexibility(t, Some(t2), &points, Some(&wires), &budget)
            .unwrap();
        assert_ne!(at_ref.key, a.key);
        assert!(at_ref.condensed.stiffness.is_some());
    }

    /// A relink of the placed structure moves its point's place (the
    /// structure check's rule); an unlinked point, a type without a
    /// model, a pole: as the structure check takes them.
    #[test]
    fn links_poles_and_types_without_a_model() {
        let (types, line, analyses) = setup();
        let budget = ohl_structures_fe::default_budget();
        let t = types.get("type-02").unwrap();
        let mut t2 = line.structure("T2").unwrap().clone();
        let points = labels(&t2);
        let model = analyses.model("type-02", None).unwrap();
        let other = model
            .nodes
            .iter()
            .position(|n| n.id == "A2")
            .expect("joint A2");
        for p in t2.points.iter_mut().filter(|p| p.label == points[0]) {
            p.joint = Some("A2".into());
            p.offset = [0.0, 0.3, 0.0];
        }
        let relinked = analyses
            .flexibility(t, Some(&t2), &points, None, &budget)
            .unwrap();
        let place = &relinked.condensed.places[relinked.condensed.place_of[0]];
        assert_eq!(place.node, other);
        // A2 is a lattice arm's tip (trusses only): the offset's rotation
        // left out, said (as the check leaves its moment out).
        assert_eq!(place.offset.map(f64::abs), [0.0; 3]);
        assert!(
            relinked
                .condensed
                .notes
                .iter()
                .any(|n| n.contains("rotation left out")),
            "{:?}",
            relinked.condensed.notes
        );
        for p in t2.points.iter_mut().filter(|p| p.label == points[0]) {
            p.unlinked = true;
        }
        assert_eq!(
            analyses.flexibility(t, Some(&t2), &points, None, &budget),
            Err(Rigid::Unlinked(points[0].clone()))
        );
        // type-01: no model, no pole.
        let t1 = line.structure("T1").unwrap();
        assert_eq!(
            analyses.flexibility(
                types.get("type-01").unwrap(),
                Some(t1),
                &labels(t1),
                None,
                &budget
            ),
            Err(Rigid::NoModel)
        );
        // type-05: a steel pole with an elasticity.
        let t6 = line.structure("T6").unwrap();
        let pole = analyses
            .flexibility(
                types.get("type-05").unwrap(),
                Some(t6),
                &labels(t6),
                None,
                &budget,
            )
            .unwrap();
        assert!(pole.condensed.stiffness.is_some());
        assert!(
            pole.condensed
                .notes
                .iter()
                .any(|n| n.contains("NOT ON THE SAFE SIDE")),
            "{:?}",
            pole.condensed.notes
        );
    }
}
