// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The plates' and welds' checks per case, on a welded plate (invented
//! data).

use std::collections::BTreeMap;

use crate::joints::joint_details;
use crate::joints_tests::{OTHER_FACE, TOWER, library};
use crate::{PlatePart, plate_plans, plate_usage};

/// A welded plate of G1 (D1 onto it) and G2 (D2), a weld of a = 6 mm
/// given on G2 (`[[plate.weld]]`), its length the seam's.
fn welded() -> String {
    format!(
        "{TOWER}{OTHER_FACE}\n[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         \n[[plate]]\nid = \"G2\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         [[plate.weld]]\nto = \"G1\"\nsize = \"6 mm\"\n\
         \n[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"G1\"\n\
         \n[[connection]]\nmember = \"D2\"\njoint = \"N2\"\nonto = \"G2\"\n"
    )
}

fn axial() -> BTreeMap<String, f64> {
    [("D1", 40e3), ("D2", -25e3), ("L1", -100e3), ("L2", -90e3)]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}

/// Under SIA 263 each member's bolts are checked in its plate (bearing,
/// block shear, sections, distances) and the weld by (87) and (88) under
/// the larger resultant of its two sides (D1's 40 kN against D2's 25
/// kN); the strength factor scales the resistances.
#[test]
fn plates_and_welds_are_checked_by_sia263() {
    let sia = format!("{}\n[analysis]\ncapacity = \"sia263-2013\"\n", welded());
    let d = joint_details(&sia, &library(), &Default::default()).unwrap();
    let w = &d.joint("N2").unwrap().plates[0].welds[0];
    assert_eq!(w.size, Some(0.006));
    assert_eq!(w.label().as_deref(), Some("a6"));
    let plans = plate_plans(&sia, &library(), &Default::default()).unwrap();
    let u = plate_usage(&plans, &axial(), 1.0);
    let d1 = u
        .iter()
        .find(|p| p.part == PlatePart::Member("D1".into()))
        .unwrap();
    assert_eq!(d1.plate, "G1");
    assert!(d1.usage.is_some_and(|x| x > 0.0 && x < 1.0), "{d1:?}");
    for c in ["(76)", "(83)", "(39)", "e_1"] {
        assert!(d1.checks.iter().any(|x| x.0.contains(c)), "{c}: {d1:?}");
    }
    let d2 = u
        .iter()
        .find(|p| p.part == PlatePart::Member("D2".into()))
        .unwrap();
    assert!(
        d2.notes
            .iter()
            .any(|n| n.contains("plate buckling not checked"))
    );
    let weld = u
        .iter()
        .find(|p| matches!(&p.part, PlatePart::Weld(_)))
        .unwrap();
    assert!((weld.force - 40e3).abs() < 1e-6, "{weld:?}");
    assert!(weld.governs.contains("(87)"), "{weld:?}");
    assert!(weld.notes.iter().any(|n| n.contains("the seam's")));
    let half = plate_usage(&plans, &axial(), 0.5);
    let w2 = half
        .iter()
        .find(|p| matches!(&p.part, PlatePart::Weld(_)))
        .unwrap();
    assert!((w2.usage.unwrap() - 2.0 * weld.usage.unwrap()).abs() < 1e-12);
    // The worst per joint.
    let j = crate::joint_plate_usage(&u);
    assert!(j["N2"] >= weld.usage.unwrap());
}

/// Under ASCE 10-97 (the default) the weld is not checked (the standard
/// has no weld rule), said; the bolts are (4.4 …); the simple check
/// checks nothing, said.
#[test]
fn asce10_has_no_weld_rule_and_simple_checks_nothing() {
    let plans = plate_plans(&welded(), &library(), &Default::default()).unwrap();
    let u = plate_usage(&plans, &axial(), 1.0);
    let weld = u
        .iter()
        .find(|p| matches!(&p.part, PlatePart::Weld(_)))
        .unwrap();
    assert!(
        weld.usage.is_none() && weld.governs.contains("AWS D1.1"),
        "{weld:?}"
    );
    assert!(
        u.iter()
            .any(|p| p.checks.iter().any(|c| c.0.contains("4.4")))
    );
    let simple = format!("{}\n[analysis]\ncapacity = \"simple\"\n", welded());
    let plans = plate_plans(&simple, &library(), &Default::default()).unwrap();
    assert!(
        plate_usage(&plans, &axial(), 1.0)
            .iter()
            .all(|p| p.usage.is_none() && p.governs.contains("not checked"))
    );
}
