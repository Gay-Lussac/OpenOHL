// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Joints of one kind (J4): matched geometrically under the tower's
//! symmetry, a plan mapped onto a joint like it, the run laying out once
//! per kind.

use std::collections::{BTreeMap, BTreeSet};

use crate::joint_kinds::{kinds, like, like_joint, map_plan};
use crate::joint_layout::{BoltArea, LayoutSettings, Onto, plan_joint};
use crate::joints::joint_details;
use crate::joints_tests::library;
use crate::layout_run::{RunAsk, lay_out};

/// An invented body (four legs, face diagonals turning round it), its
/// angles oriented from its shape.
fn oriented_body() -> String {
    let text = crate::orient_tests::body(&[(0.0, 2.0, 2.0), (3.0, 1.8, 1.8), (6.0, 1.6, 1.6)], &[]);
    crate::orient_angles(
        &text,
        &library(),
        &crate::OrientAsk {
            members: None,
            replace_hand: true,
            ..Default::default()
        },
    )
    .unwrap()
    .text
}

fn settings() -> LayoutSettings {
    LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    }
}

#[test]
fn a_bodys_corners_at_one_level_are_of_one_kind() {
    let text = oriented_body();
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let all: BTreeSet<String> = d.joints.iter().map(|j| j.id.clone()).collect();
    for strict in [false, true] {
        let k = kinds(&d, &all, strict);
        for c in 1..4 {
            let l = k
                .get(&format!("J1{c}"))
                .unwrap_or_else(|| panic!("J1{c} ({strict}): {:?}", k.keys()));
            assert_eq!(l.of, "J10", "{l:?}");
            // Each member matched to one of the same role: J10's diagonal
            // up to J11's diagonal up.
            assert_eq!(l.member("D10"), Some(format!("D1{c}").as_str()), "{l:?}");
        }
        assert!(!k.contains_key("J10"));
    }
    // A joint at another level (other angles: the body tapers) is not
    // like it.
    let j10 = d.joint("J10").unwrap();
    let j20 = d.joint("J20").unwrap();
    assert!(like(j10, j20, false).is_none());
    assert_eq!(like_joint(&d, "J10").len(), 3);
}

#[test]
fn another_section_or_roll_is_not_alike() {
    let text = oriented_body();
    // D11 of another section: J11 and J12 (its ends) no longer like the
    // others.
    let other = text.replacen(
        "id = \"D11\"\nnodes = [\"J11\", \"J22\"]\nsection = \"EA-61x7\"",
        "id = \"D11\"\nnodes = [\"J11\", \"J22\"]\nsection = \"UA-83x52x6\"",
        1,
    );
    assert_ne!(other, text);
    let d = joint_details(&other, &library(), &Default::default()).unwrap();
    assert!(like(d.joint("J10").unwrap(), d.joint("J11").unwrap(), false).is_none());
    assert!(like(d.joint("J10").unwrap(), d.joint("J13").unwrap(), false).is_some());
}

#[test]
fn a_plan_mapped_onto_a_joint_like_it_is_its_own_plan() {
    let text = oriented_body();
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let a = d.joint("J10").unwrap();
    let b = d.joint("J12").unwrap();
    let l = like(a, b, true).expect("alike");
    let s = settings();
    let pa = plan_joint(a, &s, &|_, _| None);
    let pb = plan_joint(b, &s, &|_, _| None);
    assert!(!pa.ends.is_empty(), "{pa:?}");
    let mapped = map_plan(&pa, &l);
    assert_eq!(mapped.like.as_deref(), Some("J10"));
    assert_eq!(mapped.joint, "J12");
    let ends = |p: &crate::joint_layout::JointPlan| -> BTreeMap<String, (i64, String)> {
        p.ends
            .iter()
            .map(|e| {
                let onto = match &e.onto {
                    Onto::Member(m) => m.clone(),
                    Onto::Plate(k) => format!("plate on {}", p.plates[*k].on),
                };
                (
                    e.member.clone(),
                    ((e.cut.unwrap_or(0.0) * 1000.0).round() as i64, onto),
                )
            })
            .collect()
    };
    assert_eq!(ends(&mapped), ends(&pb));
    assert_eq!(mapped.plates.len(), pb.plates.len());
    for (x, y) in mapped.plates.iter().zip(&pb.plates) {
        if let (Some(o), Some(q)) = (&x.outline, &y.outline) {
            for p in o {
                assert!(
                    q.iter().any(|r| (0..3).all(|i| (p[i] - r[i]).abs() < 2e-3)),
                    "{o:?} {q:?}"
                );
            }
        }
    }
    let hands = |p: &crate::joint_layout::JointPlan| {
        let mut h = p.hand.clone();
        h.sort();
        h
    };
    assert_eq!(hands(&mapped), hands(&pb));
}

#[test]
fn a_run_lays_out_once_per_kind_as_every_joint_on_its_own() {
    let text = oriented_body();
    let lib = library();
    let bolts = Default::default();
    let run = |once: bool| {
        let s = LayoutSettings {
            once_per_kind: once,
            ..settings()
        };
        lay_out(
            &text,
            &RunAsk {
                library: &lib,
                bolts: &bolts,
                settings: &s,
                preset_fields: BTreeMap::new(),
                joints: None,
                replace_hand: false,
            },
        )
        .unwrap()
    };
    let (once, own) = (run(true), run(false));
    assert!(
        once.joints.iter().any(|j| j.plan.like.is_some()),
        "no joint laid out as one like it"
    );
    assert!(own.joints.iter().all(|j| j.plan.like.is_none()));
    let laid = |r: &crate::LayoutRun| -> Vec<(String, bool, usize)> {
        r.joints
            .iter()
            .map(|j| (j.joint.clone(), j.written, j.hand.len()))
            .collect()
    };
    assert_eq!(laid(&once), laid(&own));
    // The same details (the plates listed, so numbered, in the order of
    // the joint each was planned on).
    assert_eq!(details_of(&once.text), details_of(&own.text));
    assert_ne!(once.text, text);
    // A run again changes nothing.
    let s = settings();
    let again = lay_out(
        &once.text,
        &RunAsk {
            library: &lib,
            bolts: &bolts,
            settings: &s,
            preset_fields: BTreeMap::new(),
            joints: None,
            replace_hand: false,
        },
    )
    .unwrap();
    assert_eq!(again.text, once.text);
}

/// Each member end's connection (what it bolts onto by kind, its cut and
/// shift) and each joint's plates (thickness, outline to a millimetre),
/// whatever the plates' ids.
fn details_of(text: &str) -> BTreeSet<String> {
    use ohl_structures::LibraryItem;
    let t = ohl_structures::StructureType::parse(std::path::Path::new("s.ohls"), text).unwrap();
    let mut out = BTreeSet::new();
    for c in &t.connections {
        let onto = match &c.onto {
            Some(o) if t.plates.iter().any(|p| &p.id == o) => "a plate".to_owned(),
            other => format!("{other:?}"),
        };
        out.insert(format!(
            "{} at {}: {onto} cut {:?} shift {:?}",
            c.member, c.joint, c.cut, c.shift
        ));
    }
    for p in &t.plates {
        let mut pts: Vec<String> = p
            .outline
            .iter()
            .map(|q| {
                format!(
                    "{:.0} {:.0} {:.0}",
                    q[0].si() * 1e3,
                    q[1].si() * 1e3,
                    q[2].si() * 1e3
                )
            })
            .collect();
        pts.sort();
        out.insert(format!(
            "plate at {} {:.1} mm: {}",
            p.joint,
            p.thickness.si() * 1e3,
            pts.join("; ")
        ));
    }
    out
}

#[test]
fn words_are_said_with_the_joints_own_members() {
    let text = oriented_body();
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let l = like(d.joint("J10").unwrap(), d.joint("J11").unwrap(), false).unwrap();
    assert_eq!(
        l.words("D10: runs into L10 at J10. H10, D100 kept."),
        "D11: runs into L11 at J11. H11, D100 kept."
    );
}

/// L7 (2026-11-20): a plate's own bolts onto a leg, its leg named (`long`
/// or `short`), mapped onto a joint like it name the leg the map carries
/// the representative's onto — under a mirror an equal angle's leg along
/// its y is the matched member's leg along its z (before, the name was
/// copied and the bolts drawn on the other leg, off the plate).
#[test]
fn a_plates_own_bolts_name_the_leg_the_map_carries_theirs_onto() {
    use crate::joint_kinds::{Like, Map, map_hold_legs};
    use crate::joint_layout::{HoldPlan, JointPlan, PlatePlan};
    let text = oriented_body();
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let a = d.joint("J10").unwrap();
    let leg = a
        .arrivals
        .iter()
        .find(|x| x.class.as_deref() == Some("leg"))
        .expect("a leg")
        .clone();
    // Its mirror across the x = 0 plane: the same joint mirrored (an
    // angle's legs swap their roles: its leg along y now along z).
    let map = Map {
        m: [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        mirrors: true,
        name: "mirrored across x",
    };
    let mut b = a.clone();
    b.id = "M".into();
    for x in &mut b.arrivals {
        x.direction = map.apply(x.direction);
        if let Some(p) = x.profile.as_mut() {
            let (y, z) = (map.apply(p.axes[1]), map.apply(p.axes[2]));
            p.axes = [map.apply(p.axes[0]), z, y];
            if let Some(g) = p.angle.as_mut() {
                g.heel = [g.heel[1], g.heel[0]];
                (g.b, g.h) = (g.h, g.b);
            }
        }
    }
    let l = Like {
        joint: "M".into(),
        of: "J10".into(),
        map,
        from: a.position,
        to: map.apply(a.position),
        pairs: a
            .arrivals
            .iter()
            .map(|x| (x.member.clone(), x.member.clone()))
            .collect(),
    };
    for (given, mirrored) in [("long", "short"), ("short", "long")] {
        let plan = JointPlan {
            joint: "J10".into(),
            plates: vec![PlatePlan {
                members: Vec::new(),
                thickness: 0.008,
                on: leg.member.clone(),
                heel: None,
                own: None,
                reach: None,
                outline: None,
                hold: Some(HoldPlan {
                    member: leg.member.clone(),
                    leg: Some(given),
                    bolt: BoltArea::picked(None, 0.016, None),
                    provisional: true,
                    points: vec![[0.05, 0.03], [0.1, 0.03]],
                }),
                short: false,
                cuts: Vec::new(),
                between: false,
                also: None,
            }],
            ..Default::default()
        };
        let mut p = map_plan(&plan, &l);
        map_hold_legs(&mut p, &l, a, &b);
        let h = p.plates[0].hold.as_ref().unwrap();
        assert_eq!(h.leg, Some(mirrored), "{given}");
    }
}
