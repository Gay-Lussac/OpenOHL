// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Rolls from the joints' planes ([`crate::plane_rolls`], Orient angles v2)
//! on invented ends and towers.

use std::collections::BTreeSet;

use crate::joints_tests::library;
use crate::orient::{OrientAsk, OrientMethod, orient_angles};
use crate::plane_rolls::{
    ROLL_WINDOW_DEG, RollEnd, RunKind, Weighting, best_turn, plane_rolls, runs_of, turn_runs,
};

fn hosted(m: &str, turn: f64, to_host: f64, weight: f64) -> RollEnd {
    RollEnd {
        joint: "J".into(),
        member: m.into(),
        piece: "P".into(),
        turn,
        to_host,
        floor: None,
        period: 90.0,
        weight,
    }
}

/// Faces 5° off square: three ends of one face want +2.5°, one of the other
/// face −2.5° (at 90° to the host: none within 2° as it lies). The most
/// within 2° first (the three), centred in their band (+2.5°: in the
/// plane); weighed by area, the one big end.
#[test]
fn the_turn_lays_the_most_ends_within_2_degrees_then_the_least_left() {
    let ends = vec![
        hosted("A1", 2.5, 90.0, 1.0),
        hosted("A2", 2.5, 90.0, 1.0),
        hosted("A3", 2.5, 90.0, 1.0),
        hosted("B1", -2.5, 90.0, 1.0),
    ];
    let t = best_turn(&ends, ROLL_WINDOW_DEG);
    assert!((t - 2.5).abs() < 0.011, "{t}");
    assert!(ends[..3].iter().all(|e| e.off(t) <= crate::FLAT_OFF));
    // By area: the one big end of the other face.
    let mut big = ends.clone();
    big[3].weight = 5.0;
    let t = best_turn(&big, ROLL_WINDOW_DEG);
    assert!(big[3].off(t) <= crate::FLAT_OFF, "{t}");
    // Two faces of one end each: one of them in its plane (the bisector
    // lays none within), the same every time.
    let sym = vec![hosted("A", 2.5, 90.0, 1.0), hosted("B", -2.5, 90.0, 1.0)];
    let t = best_turn(&sym, ROLL_WINDOW_DEG);
    assert!((t.abs() - 2.5).abs() < 0.011, "{t}");
    assert_eq!(best_turn(&sym, ROLL_WINDOW_DEG), t);
    // Both within as they lie (a shallow pair): no turn.
    let sym = vec![hosted("A", 1.0, 90.0, 1.0), hosted("B", -1.0, 90.0, 1.0)];
    assert_eq!(best_turn(&sym, ROLL_WINDOW_DEG), 0.0);
    // Within the window only: an end 40° off is not reached.
    assert_eq!(
        best_turn(&[hosted("F", 40.0, 90.0, 1.0)], ROLL_WINDOW_DEG),
        0.0
    );
    // Nothing to gain: none.
    assert_eq!(best_turn(&[], ROLL_WINDOW_DEG), 0.0);
}

/// A shallow end (30° to the host) is within 2° of the flange's plane over
/// a wider band of turns than one at 90°.
#[test]
fn a_shallow_end_lies_within_over_a_wider_band() {
    let e = hosted("S", 3.5, 30.0, 1.0);
    assert!(e.off(0.0) <= crate::FLAT_OFF, "{}", e.off(0.0));
    let e = hosted("N", 3.5, 90.0, 1.0);
    assert!(e.off(0.0) > crate::FLAT_OFF);
    // The piece's own end: its floor stays however turned.
    let own = RollEnd {
        floor: Some(3.0),
        ..hosted("O", 1.0, 90.0, 1.0)
    };
    assert!((own.off(1.0) - 3.0).abs() < 1e-9);
}

/// The K-brace's leg (two pieces, one leg) turned 5° about its axis: one
/// run, turned back −5° from its joints' planes, both diagonals within 2°
/// after (none before); the file turned so: the run again turns nothing.
#[test]
fn the_k_braces_leg_run_is_turned_back_from_its_joints_planes() {
    let lib = library();
    let bolts = Default::default();
    let k = crate::joint_layout_tests::K.replace(
        "orientation = \"x+y+\"\ndesign = { class = \"leg\" }",
        "orientation = \"x+y+\"\nroll = \"5 deg\"\ndesign = { class = \"leg\" }",
    );
    let d = crate::joint_details(&k, &lib, &bolts).unwrap();
    let legs: BTreeSet<String> = ["L1".to_owned(), "L2".to_owned()].into();
    let runs = runs_of(&d, &legs);
    assert_eq!(runs, vec![vec!["L1".to_owned(), "L2".to_owned()]]);
    let r = &plane_rolls(&d, &runs, RunKind::Leg, Weighting::Count)[0];
    assert!((r.turn + 5.0).abs() < 0.1, "{r:?}");
    assert_eq!(r.within, (0, 2), "{r:?}");
    assert!(r.worst() < 0.1);
    assert!(r.words().starts_with("leg run L1, L2 turned by"));
    let sets = |l: &BTreeSet<String>| vec![(RunKind::Leg, l.clone())];
    let (after, rolls) = turn_runs(&k, &lib, &sets(&legs), Weighting::Count).unwrap();
    assert!(rolls[0].turns());
    let (again, rolls) = turn_runs(&after, &lib, &sets(&legs), Weighting::Count).unwrap();
    assert!(!rolls[0].turns(), "{rolls:?}");
    assert_eq!(again, after);
}

/// A three-legged body: `levels` (z, the corners' distance from the
/// axis), each level's corners joined by face horizontals, each panel's
/// faces by one diagonal except the faces from corner 2 to corner 0 (their
/// horizontals only). Legs class `leg`, the rest `bracing`; no rolls.
pub(crate) fn three_legs(levels: &[(f64, f64)]) -> String {
    use std::fmt::Write as _;
    let mut s = String::from("id = \"T3\"\nname = \"t3\"\nkind = \"lattice\"\n");
    for (i, (z, r)) in levels.iter().enumerate() {
        for c in 0..3 {
            let a = (90.0 + 120.0 * c as f64).to_radians();
            let _ = write!(
                s,
                "\n[[node]]\nid = \"J{i}{c}\"\npoint = [\"{:.6} m\", \"{:.6} m\", \"{z} m\"]\n{}",
                r * a.cos(),
                r * a.sin(),
                if i == 0 { "support = \"fixed\"\n" } else { "" }
            );
        }
    }
    let mut member = |id: String, a: String, b: String, class: &str| {
        let _ = write!(
            s,
            "\n[[member]]\nid = \"{id}\"\nnodes = [\"{a}\", \"{b}\"]\nsection = \"EA-61x7\"\n\
             kind = \"frame\"\ndesign = {{ class = \"{class}\" }}\n"
        );
    };
    for i in 0..levels.len() - 1 {
        for c in 0..3 {
            member(
                format!("L{i}{c}"),
                format!("J{i}{c}"),
                format!("J{}{c}", i + 1),
                "leg",
            );
            if c != 2 {
                member(
                    format!("D{i}{c}"),
                    format!("J{i}{c}"),
                    format!("J{}{}", i + 1, (c + 1) % 3),
                    "bracing",
                );
            }
        }
    }
    for i in 1..levels.len() {
        for c in 0..3 {
            member(
                format!("H{i}{c}"),
                format!("J{i}{c}"),
                format!("J{i}{}", (c + 1) % 3),
                "bracing",
            );
        }
    }
    s
}

/// A three-legged body (its faces meeting at 60° seen along a leg): as the
/// outline turns them, each leg's flanges lie 15° off both faces (no end
/// within 2°); from the joints' planes each leg's run is turned so that one
/// face's members lie in its flange's plane — for legs 0 and 2 the face
/// with the diagonals (more ends) — marked automatic; a run again changes
/// nothing. A square body whose taper changes keeps its legs as the faces
/// want them.
#[test]
fn orient_v2_turns_the_legs_into_the_planes_of_what_bolts_onto_them() {
    let text = three_legs(&[(0.0, 2.0), (4.0, 1.6), (8.0, 1.2)]);
    let lib = library();
    let ask = |method| OrientAsk {
        members: None,
        replace_hand: true,
        method,
        ..Default::default()
    };
    let outline = orient_angles(&text, &lib, &ask(OrientMethod::Outline)).unwrap();
    assert!(outline.planes.is_empty());
    let x = outline
        .turned
        .iter()
        .find(|o| o.member == "L00")
        .unwrap_or_else(|| panic!("{:?}", outline.left));
    assert!((x.off - 15.0).abs() < 1.0, "{x:?}");
    let planes = orient_angles(&text, &lib, &ask(OrientMethod::Planes)).unwrap();
    let legs: Vec<_> = planes
        .planes
        .iter()
        .filter(|r| r.kind == RunKind::Leg)
        .collect();
    assert_eq!(legs.len(), 3, "{:?}", planes.planes);
    for r in &legs {
        assert!(r.turns(), "{r:?}");
        assert!(r.turn.abs() > 10.0, "{r:?}");
        assert!(r.within.1 > r.within.0, "{r:?}");
    }
    // Leg 0: the face to corner 1 (diagonals) lies in its flange's plane.
    let r0 = legs
        .iter()
        .find(|r| r.members.contains(&"L00".to_owned()))
        .unwrap();
    for e in r0.ends.iter().filter(|e| e.member.starts_with('D')) {
        assert!(e.after <= crate::FLAT_OFF, "{r0:?}");
    }
    let o = planes.turned.iter().find(|o| o.member == "L00").unwrap();
    assert!((o.roll - x.roll).abs() > 10.0, "{o:?} {x:?}");
    let rec = crate::LayoutRecord::of(
        &<ohl_structures::StructureType as ohl_structures::LibraryItem>::parse(
            std::path::Path::new("s.ohls"),
            &planes.text,
        )
        .unwrap(),
    );
    assert!(rec.oriented.contains_key("L00"));
    let again = orient_angles(&planes.text, &lib, &ask(OrientMethod::Planes)).unwrap();
    assert!(again.turned.is_empty(), "{:?}", again.turned);
    assert_eq!(again.text, planes.text);
    // A square body whose taper changes (below its faces 7° off square, the
    // outline leaving every end 3.5° out; above nearly square): below, one
    // face in each leg's flange, never fewer ends within; above, every end
    // within as the outline turns it, no turn; the ends of the upper panel
    // measured against the upper piece at the taper change.
    let text =
        crate::orient_tests::body(&[(0.0, 4.0, 4.0), (4.0, 2.5, 2.5), (10.0, 2.0, 2.0)], &[]);
    let r = orient_angles(&text, &lib, &ask(OrientMethod::Planes)).unwrap();
    let legs: Vec<_> = r.planes.iter().filter(|p| p.kind == RunKind::Leg).collect();
    assert_eq!(legs.len(), 8, "{legs:?}");
    for p in &legs {
        assert!(p.within.1 >= p.within.0, "{p:?}");
        if p.members[0].starts_with("L0") {
            assert!(p.turns() && p.within.1 > p.within.0, "{p:?}");
            assert!(p.ends.iter().all(|e| e.member != "D10"), "{p:?}");
        } else {
            assert!(!p.turns(), "{p:?}");
            assert_eq!(p.within.1, p.ends.len(), "{p:?}");
        }
    }
}
