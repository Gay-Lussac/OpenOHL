// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Apply this joint's detail to similar joints (J4): a laid-out joint and
//! a hand detail copied, turned, onto the joints of its kind.

use std::collections::BTreeMap;

use ohl_structures::{LibraryItem, StructureType};

use crate::apply_detail::apply_detail;
use crate::joint_layout::{BoltArea, LayoutSettings};
use crate::joints::joint_details;
use crate::joints_tests::library;
use crate::layout_run::{LayoutRecord, RunAsk, lay_out};

/// The invented body of the J4 tests, oriented.
fn body() -> String {
    let text = crate::orient_tests::body(&[(0.0, 2.0, 2.0), (3.0, 1.8, 1.8), (6.0, 1.6, 1.6)], &[]);
    crate::orient_angles(
        &text,
        &library(),
        &crate::OrientAsk {
            members: None,
            replace_hand: true,
        },
    )
    .unwrap()
    .text
}

fn parse(text: &str) -> StructureType {
    StructureType::parse(std::path::Path::new("s.ohls"), text).unwrap()
}

#[test]
fn a_laid_out_joint_is_applied_turned_to_the_joints_like_it() {
    let lib = library();
    let bolts = Default::default();
    let s = LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    };
    // Only J10 laid out.
    let r = lay_out(
        &body(),
        &RunAsk {
            library: &lib,
            bolts: &bolts,
            settings: &s,
            preset_fields: BTreeMap::new(),
            joints: Some(["J10".to_owned()].into()),
            replace_hand: false,
        },
    )
    .unwrap();
    let laid = r.joints.iter().find(|j| j.joint == "J10").unwrap();
    assert!(laid.written, "{:?}", laid.hand);
    let a = apply_detail(&r.text, &lib, &bolts, "J10", None).unwrap();
    let to: Vec<&str> = a.applied.iter().map(|x| x.joint.as_str()).collect();
    assert_eq!(to, ["J11", "J12", "J13"], "{:?}", a.left);
    assert!(a.applied.iter().all(|x| x.how.starts_with("turned")));
    let t = parse(&a.text);
    // Each end as at J10, said with its own members: D10's cut is D12's.
    let cut = |m: &str, j: &str| t.connection(m, j).and_then(|c| c.cut).map(|c| c.si());
    assert!(cut("D10", "J10").is_some());
    assert_eq!(cut("D10", "J10"), cut("D12", "J12"));
    // Its plates copied and placed there (none at J12 before).
    let plates = |j: &str| t.plates.iter().filter(|p| p.joint == j).count();
    assert_eq!(plates("J12"), plates("J10"));
    for p in t.plates.iter().filter(|p| p.joint == "J12") {
        let c = t
            .connections
            .iter()
            .filter(|c| c.onto.as_deref() == Some(p.id.as_str()))
            .count();
        assert!(c > 0, "plate {} with nothing on it", p.id);
    }
    // The record follows: J12 laid out, its ends automatic as J10's.
    let rec = LayoutRecord::of(&t);
    assert!(rec.joints.contains("J12"));
    assert_eq!(
        rec.ends.get(&("D12".to_owned(), "J12".to_owned())),
        rec.ends.get(&("D10".to_owned(), "J10".to_owned()))
    );
    // Nothing runs into anything there that did not at J10.
    let d = joint_details(&a.text, &lib, &bolts).unwrap();
    let n = |j: &str| d.joint(j).unwrap().clashes.len();
    for j in ["J11", "J12", "J13"] {
        assert!(n(j) <= n("J10"), "{j}: {:?}", d.joint(j).unwrap().clashes);
    }
    // Applied again: the same file.
    let again = apply_detail(&a.text, &lib, &bolts, "J10", None).unwrap();
    assert_eq!(parse(&again.text).connections, t.connections);
}

#[test]
fn a_hand_detail_is_applied_too_and_only_where_asked() {
    let lib = library();
    let bolts = Default::default();
    let text = format!(
        "{}\n[[connection]]\nmember = \"D10\"\njoint = \"J10\"\ncut = \"150 mm\"\nface = \"outside\"\n",
        body()
    );
    let only = ["J13".to_owned()].into();
    let a = apply_detail(&text, &lib, &bolts, "J10", Some(&only)).unwrap();
    assert_eq!(a.applied.len(), 1, "{:?}", a.left);
    let t = parse(&a.text);
    let c = t.connection("D13", "J13").expect("copied");
    assert_eq!(c.cut.map(|x| (x.si() * 1000.0).round()), Some(150.0));
    assert!(t.connection("D11", "J11").is_none());
    // Nothing like it: nothing changes.
    let none = apply_detail(&text, &lib, &bolts, "J20", Some(&only)).unwrap();
    assert!(none.applied.is_empty());
    assert_eq!(none.text, text);
}
