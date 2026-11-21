// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The joints grouped ([`crate::joint_classes`]) on invented joints.

use crate::joint_classes::{
    EndPlane, Geometry, HostKind, Incoming, MemberClass, Through, chord_pieces, group_of,
    joint_features,
};
use crate::joint_layout_tests::K;
use crate::joint_order::Zone;
use crate::joints_tests::library;
use crate::{joint_details, piece_rolls};

/// The K-brace's joint N2 of `text` described and grouped.
fn group(
    text: &str,
) -> (
    crate::joint_classes::JointFeatures,
    crate::joint_classes::JointGroup,
) {
    let d = joint_details(text, &library(), &Default::default()).unwrap();
    let j = d.joint("N2").unwrap();
    let f = joint_features(j, Zone::Body);
    let g = group_of(&f);
    (f, g)
}

/// A node and a member added to the K-brace.
fn with_member(text: &str, node: &str, point: &str, member: &str) -> String {
    format!("{text}\n[[node]]\nid = \"{node}\"\npoint = {point}\n\n{member}")
}

/// The K-brace: its leg in two pieces of one section running on through
/// N2, two diagonals in the plane of one of its flanges.
#[test]
fn a_leg_running_on_with_two_diagonals_in_one_flange_plane() {
    let (f, g) = group(K);
    assert_eq!(f.host.as_deref(), Some("L1"));
    assert_eq!(f.host_class, MemberClass::Leg);
    assert!(f.host_steep && f.host_angle);
    assert_eq!(f.through, Through::SameSection);
    let hosts: Vec<&str> = f
        .ends
        .iter()
        .filter(|e| e.host)
        .map(|e| e.member.as_str())
        .collect();
    assert_eq!(hosts, ["L1", "L2"]);
    for e in f.incoming() {
        assert_eq!(e.class, MemberClass::Bracing);
        assert!(matches!(e.plane, EndPlane::In(_, d) if d < 0.01), "{e:?}");
        assert!((e.to_host - 39.8).abs() < 0.2, "{e:?}");
        assert!((e.size - 1.0).abs() < 1e-9);
    }
    assert_eq!(f.flanges_used().count_ones(), 1);
    assert_eq!(g.host, HostKind::LegRunning);
    assert_eq!(g.incoming, Incoming::Bracing);
    assert_eq!(g.geometry, Geometry::Flat);
    assert_eq!(
        g.name(),
        "leg running on · bracing · all in a flange's plane"
    );
}

/// The leg's pieces of other sections: a splice; one part: running on.
#[test]
fn a_change_of_section_is_a_splice_and_one_part_runs_on() {
    let text = K.replacen(
        "nodes = [\"N2\", \"N3\"]\nsection = \"EA-61x7\"",
        "nodes = [\"N2\", \"N3\"]\nsection = \"UA-83x52x6\"",
        1,
    );
    let (f, g) = group(&text);
    assert_eq!(f.through, Through::SectionChange);
    assert_eq!(g.host, HostKind::LegSplice);
    // (The piece of the larger steel area is the host: 61 × 7 over
    // 83 × 52 × 6.)
    assert_eq!(f.host.as_deref(), Some("L1"));
    assert!(f.ends.iter().filter(|e| e.host).count() == 2);
    let text = format!(
        "{K}\n[[connection]]\nmember = \"L1\"\njoint = \"N2\"\nkind = \"continuous\"\nonto = \"L2\"\n"
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let j = d.joint("N2").unwrap();
    assert!(j.arrivals.iter().any(|a| a.continuous.is_some()));
    let f = joint_features(j, Zone::Body);
    assert_eq!(f.through, Through::OnePart);
    assert_eq!(group_of(&f).host, HostKind::LegRunning);
    assert!(f.ends.iter().filter(|e| e.host).all(|e| e.one_part));
}

/// The leg ending at N2 (its upper piece taken away): the leg ends here;
/// of class bracing: nothing runs through.
#[test]
fn a_leg_ending_here_and_nothing_running_through() {
    let start = K.find("[[member]]\nid = \"L2\"").unwrap();
    let end = K[start..].find("[[member]]\nid = \"D1\"").unwrap() + start;
    let text = format!("{}{}", &K[..start], &K[end..]);
    let (f, g) = group(&text);
    assert_eq!(f.through, Through::Nothing);
    assert_eq!(g.host, HostKind::LegEnds);
    let text = text.replace(
        "design = { class = \"leg\" }",
        "design = { class = \"bracing\" }",
    );
    let (_, g) = group(&text);
    assert_eq!(g.host, HostKind::NothingThrough);
}

/// A diagonal 4° out of the flange's plane: some a few degrees out; a plan
/// member arriving between the leg's flanges: with plan members, out of
/// plane.
#[test]
fn ends_out_of_the_flange_plane_and_plan_members() {
    let near = K.replace(
        "point = [\"2.5 m\", \"0 m\", \"0 m\"]",
        "point = [\"2.5 m\", \"0.25 m\", \"0 m\"]",
    );
    let (f, g) = group(&near);
    let d1 = f.ends.iter().find(|e| e.member == "D1").unwrap();
    assert!(
        matches!(d1.plane, EndPlane::Near(_, d) if (d - 3.7).abs() < 0.3),
        "{d1:?}"
    );
    assert_eq!(g.geometry, Geometry::Near);
    assert_eq!(g.incoming, Incoming::Bracing);
    let plan = with_member(
        K,
        "N6",
        "[\"2 m\", \"2 m\", \"3 m\"]",
        "[[member]]\nid = \"P1\"\nnodes = [\"N2\", \"N6\"]\nsection = \"EA-61x7\"\ndesign = { class = \"bracing\" }\n",
    );
    let (f, g) = group(&plan);
    let p1 = f.ends.iter().find(|e| e.member == "P1").unwrap();
    assert!(p1.plan, "{p1:?}");
    assert!(
        matches!(p1.plane, EndPlane::Out(_, d) if (d - 45.0).abs() < 1.0),
        "{p1:?}"
    );
    assert_eq!(g.incoming, Incoming::WithPlan);
    assert_eq!(g.geometry, Geometry::Out);
}

/// Redundants only; a built-up leg.
#[test]
fn redundants_only_and_a_built_up_leg() {
    let red = K.replace(
        "design = { class = \"bracing\" }",
        "design = { class = \"redundant\" }",
    );
    let (_, g) = group(&red);
    assert_eq!(g.incoming, Incoming::Redundants);
    let bu = K.replace(
        "section = \"EA-61x7\"\norientation = \"x+y+\"\ndesign = { class = \"leg\" }",
        "section = \"2L-83x52x6\"\norientation = \"x+y+\"\ndesign = { class = \"leg\" }",
    );
    let (f, g) = group(&bu);
    assert!(f.ends.iter().find(|e| e.member == "L1").unwrap().built_up);
    assert_eq!(g.host, HostKind::BuiltUpLeg);
}

/// Two bracing members crossing, each running on through the joint: a
/// crossing, nothing else coming in.
#[test]
fn an_x_brace_centre_is_a_crossing() {
    let text = K.replace(
        "design = { class = \"leg\" }",
        "design = { class = \"bracing\" }",
    );
    let text = with_member(
        &text,
        "N6",
        "[\"-2.5 m\", \"0 m\", \"6 m\"]",
        "[[member]]\nid = \"D3\"\nnodes = [\"N2\", \"N6\"]\nsection = \"EA-61x7\"\norientation = \"y-z+\"\ndesign = { class = \"bracing\" }\n",
    );
    let text = with_member(
        &text,
        "N7",
        "[\"-2.5 m\", \"0 m\", \"0 m\"]",
        "[[member]]\nid = \"D4\"\nnodes = [\"N2\", \"N7\"]\nsection = \"EA-61x7\"\norientation = \"y-z+\"\ndesign = { class = \"bracing\" }\n",
    );
    let (f, g) = group(&text);
    assert!(f.incoming().filter(|e| e.runs_on).count() >= 2, "{f:?}");
    assert_eq!(g.host, HostKind::Crossing);
}

/// A chord (bracing running on, not a leg) turned 5° against its two
/// diagonals: its pieces named, and their turn read from the joint as a
/// leg's is.
#[test]
fn a_chord_turned_against_its_joint_is_found() {
    let text = K.replace(
        "orientation = \"x+y+\"\ndesign = { class = \"leg\" }",
        "orientation = \"x+y+\"\nroll = \"5 deg\"\ndesign = { class = \"bracing\" }",
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let j = d.joint("N2").unwrap();
    let pieces: Vec<&str> = chord_pieces(j)
        .into_iter()
        .map(|i| j.arrivals[i].member.as_str())
        .collect();
    assert_eq!(pieces, ["L1", "L2"]);
    let rolls = piece_rolls(&d, &chord_pieces);
    let r = rolls.iter().find(|r| r.member == "L2").unwrap();
    let t = r.turn.unwrap_or_else(|| panic!("{r:?}"));
    assert!((t + 5.0).abs() < 0.1, "{r:?}");
    // A leg is not a chord.
    let d = joint_details(K, &library(), &Default::default()).unwrap();
    assert!(chord_pieces(d.joint("N2").unwrap()).is_empty());
}

/// The run's chord turns (off by default): on, the chord turned 5° against
/// its joint is turned back where its joints then lay out more (said in the
/// notes, its roll written); off, kept as it is. A run again changes
/// nothing.
#[test]
fn the_run_turns_a_chord_only_when_asked_and_only_where_it_helps() {
    use crate::joint_layout::{BoltArea, LayoutSettings};
    let text = K.replace(
        "orientation = \"x+y+\"\ndesign = { class = \"leg\" }",
        "orientation = \"x+y+\"\nroll = \"5 deg\"\ndesign = { class = \"bracing\" }",
    );
    let lib = library();
    let bolts = Default::default();
    let run = |on: bool, text: &str| {
        let s = LayoutSettings {
            bolt: Some(BoltArea::picked(None, 0.016, None)),
            chord_turns: on,
            ..Default::default()
        };
        crate::lay_out(
            text,
            &crate::RunAsk {
                library: &lib,
                bolts: &bolts,
                settings: &s,
                preset_fields: Default::default(),
                joints: None,
                replace_hand: true,
            },
        )
        .unwrap()
    };
    assert!(!LayoutSettings::default().chord_turns);
    let off = run(false, &text);
    assert!(off.notes.iter().all(|n| !n.contains("L12")));
    let on = run(true, &text);
    let ends = |r: &crate::LayoutRun| -> usize {
        r.joints
            .iter()
            .filter(|o| o.written)
            .map(|o| o.plan.ends.len())
            .sum()
    };
    let plates = |r: &crate::LayoutRun| -> usize { r.joints.iter().map(|o| o.plates.len()).sum() };
    assert!(
        on.notes.iter().any(|n| n.contains("L12, a chord turned")),
        "{:?}",
        on.notes
    );
    // Kept only where its joints lay out more, or as many on fewer plates.
    assert!(
        ends(&on) > ends(&off) || (ends(&on) == ends(&off) && plates(&on) < plates(&off)),
        "{:?}",
        on.notes
    );
    assert!(off.text.contains("roll = \"5 deg\""));
    assert!(!on.text.contains("roll = \"5 deg\""));
    let again = run(true, &on.text);
    assert_eq!(again.text, on.text);
}
