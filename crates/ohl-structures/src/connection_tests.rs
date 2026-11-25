// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Connections and plates (version 9, 2026-11-02): read, refused when
//! wrong, left-over references said and kept.

use std::path::Path;

use crate::LibraryItem;
use crate::connections::{BoltedLeg, ConnectionKind};
use crate::structures::StructureType;

fn parse(text: &str) -> Result<StructureType, String> {
    StructureType::parse(Path::new("t.ohls"), text).map_err(|e| e.message)
}

/// A leg of two members and a diagonal meeting it at N2.
const BODY: &str = "id = \"T\"\nname = \"t\"\nkind = \"lattice\"\n\
    [[node]]\nid = \"N1\"\npoint = [\"0 m\", \"0 m\", \"0 m\"]\nsupport = \"fixed\"\n\
    [[node]]\nid = \"N2\"\npoint = [\"0 m\", \"0 m\", \"5 m\"]\n\
    [[node]]\nid = \"N3\"\npoint = [\"0 m\", \"0 m\", \"10 m\"]\n\
    [[node]]\nid = \"N4\"\npoint = [\"4 m\", \"0 m\", \"0 m\"]\nsupport = \"fixed\"\n\
    [[member]]\nid = \"L1\"\nnodes = [\"N1\", \"N2\"]\n\
    [[member]]\nid = \"L2\"\nnodes = [\"N2\", \"N3\"]\n\
    [[member]]\nid = \"D1\"\nnodes = [\"N4\", \"N2\"]\n";

const CONNECTION: &str = "[[connection]]\nmember = \"D1\"\njoint = \"N2\"\n\
    onto = \"L1\"\nleg = \"long\"\n\
    bolts = { count = 2, diameter = \"16 mm\", hole = \"17.5 mm\", grade = \"8.8\" }\n\
    gauge_lines = 1\ngauges = [\"30 mm\"]\npitch = \"50 mm\"\n\
    end_distance = \"30 mm\"\nedge_distance = \"25 mm\"\n";

#[test]
fn a_connection_and_a_plate_are_read() {
    let text = format!(
        "{BODY}{CONNECTION}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         [[connection]]\nmember = \"L2\"\njoint = \"N2\"\nkind = \"fixed\"\nonto = \"G1\"\n"
    );
    let t = parse(&text).unwrap();
    let c = t.connection("D1", "N2").unwrap();
    assert_eq!(c.kind(), ConnectionKind::Bolted);
    assert_eq!(c.leg, Some(BoltedLeg::Long));
    let b = c.bolts.as_ref().unwrap();
    assert_eq!(b.count, 2);
    assert!((b.diameter.unwrap().si() - 0.016).abs() < 1e-12);
    assert!((c.gauges[0].si() - 0.03).abs() < 1e-12);
    assert_eq!(
        t.connection("L2", "N2").unwrap().kind(),
        ConnectionKind::Fixed
    );
    assert!(t.connection("D1", "N4").is_none());
    assert!((t.plates[0].thickness.si() - 0.01).abs() < 1e-12);
    assert!(
        t.connection_notes().is_empty(),
        "{:?}",
        t.connection_notes()
    );
}

/// Several plates at one joint, one of them of the 2026-11-03 kind
/// `welded` (then: welded onto the leg; since 2026-11-04 read as a plate
/// of the joint's welded plate, said); a plate without a kind is bolted;
/// an unknown kind refused.
#[test]
fn several_plates_and_a_welded_one_are_read() {
    use crate::connections::PlateKind;
    let text = format!(
        "{BODY}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         [[plate]]\nid = \"G2\"\njoint = \"N2\"\nthickness = \"12 mm\"\nkind = \"welded\"\n\
         [[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"G2\"\n"
    );
    let t = parse(&text).unwrap();
    assert_eq!(t.plates.len(), 2);
    assert_eq!(t.plates[0].kind(), PlateKind::Bolted);
    assert_eq!(t.plates[0].kind, None);
    assert_eq!(t.plates[1].kind(), PlateKind::Welded);
    // Alone of its kind: a plate, said.
    assert_eq!(t.welded_assemblies("N2").len(), 2);
    let notes = t.connection_notes();
    assert!(
        notes.len() == 1 && notes[0].contains("G2") && notes[0].contains("read as a plate"),
        "{notes:?}"
    );
    assert!(parse(&text.replace("\"welded\"", "\"glued\"")).is_err());
}

/// A welded plate (owner 2026-11-04): plates welded to each other
/// (`welded_to`, read both ways, through others too) are one assembly; a
/// plate welded to none of its joint is said, to itself refused.
#[test]
fn plates_welded_together_are_one_assembly() {
    let text = format!(
        "{BODY}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         [[plate]]\nid = \"G2\"\njoint = \"N2\"\nthickness = \"10 mm\"\nwelded_to = [\"G1\"]\n\
         [[plate]]\nid = \"G3\"\njoint = \"N2\"\nthickness = \"10 mm\"\nwelded_to = [\"G2\"]\n\
         [[plate]]\nid = \"G4\"\njoint = \"N2\"\nthickness = \"10 mm\"\n"
    );
    let t = parse(&text).unwrap();
    assert_eq!(
        t.welded_assemblies("N2"),
        vec![
            vec!["G1".to_owned(), "G2".to_owned(), "G3".to_owned()],
            vec!["G4".to_owned()]
        ]
    );
    assert!(
        t.connection_notes().is_empty(),
        "{:?}",
        t.connection_notes()
    );
    let t = parse(&text.replace("welded_to = [\"G2\"]", "welded_to = [\"G7\"]")).unwrap();
    assert!(t.connection_notes()[0].contains("welded to G7"));
    assert!(parse(&text.replace("welded_to = [\"G2\"]", "welded_to = [\"G3\"]")).is_err());
}

/// A weld with its size and length (`[[plate.weld]]`, 2026-11-04, still
/// version 9): it welds the two plates as `welded_to` does, its data
/// read; the kind fillet by default; a size of 0, a weld given twice or
/// on both plates refused; a file with `welded_to` alone reads as before
/// (no weld data).
#[test]
fn welds_carry_their_size_and_length() {
    use crate::connections::WeldKind;
    let text = format!(
        "{BODY}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         [[plate]]\nid = \"G2\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         [[plate.weld]]\nto = \"G1\"\nsize = \"6 mm\"\nlength = \"220 mm\"\n\
         [[plate]]\nid = \"G3\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         [[plate.weld]]\nto = \"G2\"\nkind = \"full_penetration\"\n"
    );
    let t = parse(&text).unwrap();
    assert_eq!(
        t.welded_assemblies("N2"),
        vec![vec!["G1".to_owned(), "G2".to_owned(), "G3".to_owned()]]
    );
    let w = t.weld_between("G1", "G2").unwrap();
    assert!((w.size.unwrap().si() - 0.006).abs() < 1e-12);
    assert!((w.length.unwrap().si() - 0.220).abs() < 1e-12);
    assert_eq!(w.kind(), WeldKind::Fillet);
    let w = t.weld_between("G3", "G2").unwrap();
    assert_eq!(w.kind(), WeldKind::FullPenetration);
    assert!(w.size.is_none() && w.length.is_none());
    assert!(t.weld_between("G1", "G3").is_none());
    assert!(
        t.connection_notes().is_empty(),
        "{:?}",
        t.connection_notes()
    );
    assert!(parse(&text.replace("size = \"6 mm\"", "size = \"0 mm\"")).is_err());
    let twice = text.replace(
        "kind = \"full_penetration\"\n",
        "kind = \"full_penetration\"\n[[plate.weld]]\nto = \"G2\"\n",
    );
    assert!(parse(&twice).unwrap_err().contains("given twice"));
    let both = text.replacen(
        "thickness = \"10 mm\"\n[[plate]]\nid = \"G2\"",
        "thickness = \"10 mm\"\n[[plate.weld]]\nto = \"G2\"\n[[plate]]\nid = \"G2\"",
        1,
    );
    assert!(parse(&both).unwrap_err().contains("on both"), "{both}");
    // welded_to alone: welded, no weld data.
    let old = format!(
        "{BODY}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         [[plate]]\nid = \"G2\"\njoint = \"N2\"\nthickness = \"10 mm\"\nwelded_to = [\"G1\"]\n"
    );
    let t = parse(&old).unwrap();
    assert_eq!(t.welded_assemblies("N2").len(), 1);
    assert!(t.weld_between("G1", "G2").is_none());
    assert!(t.plates.iter().all(|p| p.welds.is_empty()));
}

/// A weld's sides (2026-11-25, version 28, roadmap O44 J7b): 1 or 2
/// fillets read; none in a file of before (not known, the check takes the
/// worse of the two); 0 or 3 refused; a file of version 28 read, of the
/// next refused.
#[test]
fn a_welds_sides_are_read() {
    let text = format!(
        "format = \"ohl-structure\"\nversion = 28\n{BODY}\
         [[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         [[plate]]\nid = \"G2\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         [[plate.weld]]\nto = \"G1\"\nsize = \"5 mm\"\nsides = 2\n\
         [[plate]]\nid = \"G3\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         [[plate.weld]]\nto = \"G2\"\nsize = \"5 mm\"\n"
    );
    let t = parse(&text).unwrap();
    assert_eq!(t.weld_between("G1", "G2").unwrap().sides, Some(2));
    assert_eq!(t.weld_between("G2", "G3").unwrap().sides, None);
    assert!(
        parse(&text.replace("sides = 2", "sides = 1"))
            .unwrap()
            .weld_between("G1", "G2")
            .is_some_and(|w| w.sides == Some(1))
    );
    for bad in ["sides = 0", "sides = 3"] {
        let e = parse(&text.replace("sides = 2", bad)).unwrap_err();
        assert!(e.contains("sides 1 (one fillet) or 2"), "{e}");
    }
    assert_eq!(crate::connections::WELD_SIDES_VERSION, 28);
    const { assert!(crate::structures::STRUCTURE_FILE_VERSION >= 28) };
    let newer = format!(
        "version = {}",
        crate::structures::STRUCTURE_FILE_VERSION + 1
    );
    assert!(parse(&text.replace("version = 28", &newer)).is_err());
}

/// Three gauge lines (a wide leg, owner 2026-11-03), one gauge each.
#[test]
fn three_gauge_lines_are_read() {
    let three = CONNECTION
        .replace("gauge_lines = 1", "gauge_lines = 3")
        .replace(
            "gauges = [\"30 mm\"]",
            "gauges = [\"35 mm\", \"80 mm\", \"125 mm\"]",
        );
    let t = parse(&format!("{BODY}{three}")).unwrap();
    let c = t.connection("D1", "N2").unwrap();
    assert_eq!(c.gauge_lines, Some(3));
    assert_eq!(c.gauges.len(), 3);
}

#[test]
fn wrong_connections_are_refused() {
    for (extra, says) in [
        (format!("{CONNECTION}{CONNECTION}"), "given twice"),
        (
            CONNECTION.replace("gauges = [\"30 mm\"]", "gauges = [\"30 mm\", \"60 mm\"]"),
            "one gauge per gauge line",
        ),
        (
            CONNECTION.replace("gauge_lines = 1", "gauge_lines = 4"),
            "1 to 3",
        ),
        (
            CONNECTION.replace("hole = \"17.5 mm\"", "hole = \"15 mm\""),
            "hole no smaller",
        ),
        (
            CONNECTION.replace("leg = \"long\"", "leg = \"longest\""),
            "long",
        ),
        (format!("{CONNECTION}bolt = 2\n"), "unknown field"),
        (
            "[[plate]]\nid = \"D1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n".into(),
            "a member has that id",
        ),
        (
            "[[plate]]\nid = \"G\"\njoint = \"N2\"\nthickness = \"0 mm\"\n".into(),
            "thickness above 0",
        ),
    ] {
        let e = parse(&format!("{BODY}{extra}")).unwrap_err();
        assert!(e.contains(says), "{says}: {e}");
    }
}

/// A member or joint gone (deleted, a generator rebuilt): the file still
/// opens, the connection is kept and said.
#[test]
fn left_over_references_are_kept_and_said() {
    let text = format!(
        "{BODY}{}[[plate]]\nid = \"G9\"\njoint = \"N9\"\nthickness = \"8 mm\"\n",
        CONNECTION
            .replace("member = \"D1\"", "member = \"D9\"")
            .replace("onto = \"L1\"", "onto = \"L3\"")
    );
    let t = parse(&text).unwrap();
    let notes = t.connection_notes().join("\n");
    assert!(notes.contains("no member D9"), "{notes}");
    assert!(notes.contains("onto L3"), "{notes}");
    assert!(notes.contains("plate G9: no joint N9"), "{notes}");
    // The member does not end at the joint given.
    let t = parse(&format!(
        "{BODY}{}",
        CONNECTION.replace("joint = \"N2\"", "joint = \"N3\"")
    ))
    .unwrap();
    assert!(t.connection_notes()[0].contains("does not end at joint N3"));
}

/// Version 9 is read; a file of the next version is refused.
#[test]
fn version_9_is_read() {
    let t = parse(&format!(
        "format = \"ohl-structure\"\nversion = 9\n{BODY}{CONNECTION}"
    ))
    .unwrap();
    assert_eq!(t.connections.len(), 1);
    let newer = crate::structures::STRUCTURE_FILE_VERSION + 1;
    assert!(
        parse(&format!(
            "format = \"ohl-structure\"\nversion = {newer}\n{BODY}"
        ))
        .is_err()
    );
}

/// The leg L1–L2 in line through N2 with a section, the diagonal D1.
fn in_line(extra: &str) -> String {
    let body = BODY
        .replace(
            "nodes = [\"N1\", \"N2\"]\n",
            "nodes = [\"N1\", \"N2\"]\nsection = \"L100\"\n",
        )
        .replace(
            "nodes = [\"N2\", \"N3\"]\n",
            "nodes = [\"N2\", \"N3\"]\nsection = \"L100\"\n",
        );
    format!("{body}{extra}")
}

#[test]
fn a_continuous_leg_is_one_part_when_in_line_and_alike() {
    use crate::connections::Face;
    let t = parse(&in_line(
        "[[connection]]\nmember = \"L1\"\njoint = \"N2\"\nkind = \"continuous\"\nonto = \"L2\"\n\
         [[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"L1\"\nface = \"inside\"\n",
    ))
    .unwrap();
    // Given on one end: both read it.
    assert_eq!(t.continuous_with("L1", "N2"), Some("L2"));
    assert_eq!(t.continuous_with("L2", "N2"), Some("L1"));
    assert_eq!(t.continuous_with("D1", "N2"), None);
    assert_eq!(t.connection("D1", "N2").unwrap().face(), Face::Inside);
    assert_eq!(t.connection("L1", "N2").unwrap().face(), Face::Outside);
    assert!(
        t.connection_notes().is_empty(),
        "{:?}",
        t.connection_notes()
    );
    // The diagonal is not in line, nor is a member of another section.
    assert!(
        t.continuous_problem("D1", "L1", "N2")
            .unwrap()
            .contains("turn")
    );
    let other = parse(&in_line("").replacen("section = \"L100\"", "section = \"L90\"", 1)).unwrap();
    assert!(
        other
            .continuous_problem("L1", "L2", "N2")
            .unwrap()
            .contains("same section")
    );
}

#[test]
fn a_continuous_end_contradicted_or_not_in_line_is_said_and_not_used() {
    // The other end bolted onto the diagonal: not one part.
    let t = parse(&in_line(
        "[[connection]]\nmember = \"L1\"\njoint = \"N2\"\nkind = \"continuous\"\nonto = \"L2\"\n\
         [[connection]]\nmember = \"L2\"\njoint = \"N2\"\nkind = \"bolted\"\nonto = \"D1\"\n",
    ))
    .unwrap();
    assert_eq!(t.continuous_with("L1", "N2"), None);
    assert!(
        t.connection_notes()
            .iter()
            .any(|n| n.contains("says otherwise")),
        "{:?}",
        t.connection_notes()
    );
    // A leg turned by more than 1°.
    let bent = in_line(
        "[[connection]]\nmember = \"L1\"\njoint = \"N2\"\nkind = \"continuous\"\nonto = \"L2\"\n",
    )
    .replace(
        "point = [\"0 m\", \"0 m\", \"10 m\"]",
        "point = [\"0.2 m\", \"0 m\", \"10 m\"]",
    );
    let t = parse(&bent).unwrap();
    assert_eq!(t.continuous_with("L1", "N2"), None);
    assert!(
        t.connection_notes()
            .iter()
            .any(|n| n.contains("not one part") && n.contains("2.3°")),
        "{:?}",
        t.connection_notes()
    );
}

const SPLICE: &str = "[[splice]]\nid = \"S1\"\njoint = \"N2\"\nmembers = [\"L1\", \"L2\"]\n\
    [[splice.cover]]\nface = \"outer\"\nleg = \"long\"\nthickness = \"8 mm\"\n\
    [[splice.cover]]\nface = \"inner\"\nleg = \"long\"\nthickness = \"6 mm\"\n\
    [[splice.cover]]\nface = \"outer\"\nleg = \"short\"\nthickness = \"8 mm\"\n\
    [[connection]]\nmember = \"L1\"\njoint = \"N2\"\nonto = \"S1\"\n\
    [[connection]]\nmember = \"L2\"\njoint = \"N2\"\nonto = \"S1\"\n";

#[test]
fn a_splice_gives_a_shear_plane_per_cover() {
    use crate::connections::CoverLeg;
    let t = parse(&in_line(SPLICE)).unwrap();
    let s = t.splice("S1", "N2").unwrap();
    assert_eq!(s.shear_planes(Some(CoverLeg::Long)), 2);
    assert_eq!(s.shear_planes(Some(CoverLeg::Short)), 1);
    assert!(
        t.connection_notes().is_empty(),
        "{:?}",
        t.connection_notes()
    );
    // Onto a splice that does not join it.
    let t = parse(&in_line(&format!(
        "{SPLICE}[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"S1\"\n"
    )))
    .unwrap();
    assert!(
        t.connection_notes()
            .iter()
            .any(|n| n.contains("does not join it")),
        "{:?}",
        t.connection_notes()
    );
}

#[test]
fn a_wrong_splice_is_refused() {
    let twice = SPLICE
        .replace("thickness = \"6 mm\"", "thickness = \"6 mm\"\n")
        .replace(
            "face = \"inner\"\nleg = \"long\"",
            "face = \"outer\"\nleg = \"long\"",
        );
    assert!(
        parse(&in_line(&twice))
            .unwrap_err()
            .contains("two outer covers")
    );
    let clash = SPLICE.replace("id = \"S1\"", "id = \"D1\"");
    assert!(parse(&in_line(&clash)).unwrap_err().contains("has that id"));
    let thin = SPLICE.replace("\"8 mm\"", "\"0 mm\"");
    assert!(parse(&in_line(&thin)).unwrap_err().contains("above 0"));
}

/// Level 4 (owner 2026-11-11): a full design whose every checked member
/// end has its joint detail — a `[[connection]]`, or the end running on
/// through the joint as one part with a member that says so. One end
/// without: level 3, and `joint_details = true` is not applied, said.
#[test]
fn level_4_is_a_full_design_with_every_end_detailed() {
    use crate::Detail;
    let mut body = BODY.to_owned();
    for nodes in ["[\"N1\", \"N2\"]", "[\"N2\", \"N3\"]", "[\"N4\", \"N2\"]"] {
        body = body.replace(nodes, &format!("{nodes}\nsection = \"S\""));
    }
    let end = |m: &str, j: &str| format!("[[connection]]\nmember = \"{m}\"\njoint = \"{j}\"\n");
    // L2 runs on into L1 through N2: both ends there are covered.
    let through = "[[connection]]\nmember = \"L2\"\njoint = \"N2\"\nkind = \"continuous\"\n\
                   onto = \"L1\"\n";
    let t = parse(&body).unwrap();
    assert_eq!(t.detail(), Detail::Design);
    assert_eq!(t.ends_without_joint_details().len(), 6);
    let most = format!(
        "{body}{CONNECTION}{through}{}{}",
        end("L1", "N1"),
        end("L2", "N3")
    );
    let t = parse(&most).unwrap();
    assert_eq!(t.detail(), Detail::Design);
    assert_eq!(t.ends_without_joint_details(), vec![("D1", "N4")]);
    assert!(!t.joint_details_taken());
    assert_eq!(t.joint_details_not_applied(), None);
    // Asked for at level 3: not applied, in words.
    let asked = parse(&format!("joint_details = true\n{most}")).unwrap();
    assert!(!asked.joint_details_taken());
    let why = asked.joint_details_not_applied().unwrap();
    assert!(
        why.contains("not taken") && why.contains("1 member end(s)") && why.contains("simple form"),
        "{why}"
    );
    // Every end: level 4; taken only when asked for.
    let all = format!("{most}{}", end("D1", "N4"));
    let t = parse(&all).unwrap();
    assert_eq!(t.detail(), Detail::Joints);
    assert!(t.ends_without_joint_details().is_empty());
    assert!(!t.joint_details_taken());
    let on = parse(&format!("joint_details = true\n{all}")).unwrap();
    assert!(on.joint_details_taken());
    assert_eq!(on.joint_details_not_applied(), None);
    // Provisional bolts (the joint layout's record lists D1 at N4; owner
    // 2026-11-18, third round): level 3 in the core, said, the joint
    // details not taken; the bolt confirmed (the list empty): level 4.
    let record = |list: &str| {
        format!(
            "joint_details = true\n{all}\n[[generator]]\nkind = \"joint_layout\"\nid = \
             \"joint-layout\"\nprovisional = [{list}]\n"
        )
    };
    let held = parse(&record("[\"D1\", \"N4\"]")).unwrap();
    assert_eq!(held.provisional_bolts(), vec![("D1".into(), "N4".into())]);
    assert_eq!(held.detail(), Detail::Design);
    assert!(!held.joint_details_taken());
    let why = held.level_note().unwrap();
    assert!(
        why.contains("1 member end(s) with provisional bolts"),
        "{why}"
    );
    let not = held.joint_details_not_applied().unwrap();
    assert!(
        not.contains("provisional bolts") && not.contains("simple form"),
        "{not}"
    );
    let confirmed = parse(&record("")).unwrap();
    assert!(confirmed.provisional_bolts().is_empty());
    assert_eq!(confirmed.detail(), Detail::Joints);
    assert!(confirmed.joint_details_taken());
    assert_eq!(confirmed.level_note(), None);
    // Below level 4 for another reason: no note about bolts.
    let partial = parse(&format!(
        "{most}\n[[generator]]\nkind = \"joint_layout\"\nprovisional = [[\"D1\", \"N4\"]]\n"
    ))
    .unwrap();
    assert_eq!(partial.detail(), Detail::Design);
    assert_eq!(partial.level_note(), None);
    // A modelling member (check = false) needs none.
    let modelling = most.replace(
        "[\"N4\", \"N2\"]\nsection = \"S\"",
        "[\"N4\", \"N2\"]\nsection = \"S\"\ncheck = false",
    );
    assert_eq!(parse(&modelling).unwrap().detail(), Detail::Joints);
    // The level's number and name; everywhere else it counts as level 3.
    assert_eq!(Detail::try_from(4), Ok(Detail::Joints));
    assert_eq!(Detail::Joints.level(), 4);
    assert_eq!(Detail::Joints.name(), "full design with joint details");
    assert!(Detail::Joints.is_design() && Detail::Design.is_design());
    assert!(!Detail::Outline.is_design());
    assert!(Detail::try_from(5).is_err());
    // A file is read at version 11; the key is unknown to nothing older
    // than it (an older program refuses the key).
    assert!(
        parse(&format!(
            "format = \"ohl-structure\"\nversion = 11\njoint_details = true\n{all}"
        ))
        .is_ok()
    );
}

/// A plate's own bolts onto the leg (`[plate.leg_bolts]`, version 20,
/// 2026-11-19): read; a member not at the joint, a count differing from
/// the places and `leg = "both"` said in the notes, kept.
#[test]
fn a_plates_own_bolts_onto_the_leg_are_read() {
    let text = format!(
        "{BODY}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         [plate.leg_bolts]\nmember = \"L1\"\nleg = \"long\"\n\
         bolts = {{ count = 2, diameter = \"16 mm\", grade = \"8.8\" }}\n\
         bolt_points = [[\"-60 mm\", \"30 mm\"], [\"60 mm\", \"30 mm\"]]\n"
    );
    let t = parse(&text).unwrap();
    let lb = t.plates[0].leg_bolts.as_ref().unwrap();
    assert_eq!(lb.member, "L1");
    assert_eq!(lb.leg, Some(BoltedLeg::Long));
    assert_eq!(lb.bolt_points.len(), 2);
    assert!(
        t.connection_notes().is_empty(),
        "{:?}",
        t.connection_notes()
    );
    let odd = text
        .replace(
            "member = \"L1\"\nleg = \"long\"",
            "member = \"D9\"\nleg = \"both\"",
        )
        .replace("count = 2", "count = 3");
    let notes = parse(&odd).unwrap().connection_notes();
    assert!(
        notes
            .iter()
            .any(|n| n.contains("does not meet at joint N2")),
        "{notes:?}"
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("count 3 but 2 bolt_points")),
        "{notes:?}"
    );
    assert!(
        notes.iter().any(|n| n.contains("long or short only")),
        "{notes:?}"
    );
    // A plate without them reads as before.
    let plain = format!("{BODY}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n");
    assert!(parse(&plain).unwrap().plates[0].leg_bolts.is_none());
}

/// A bent plate (`[plate.bend]`, version 22, 2026-11-19): read with its
/// angle, inside radius and line; the pair listed by `bent_pairs`, not one
/// welded assembly; a bend from no plate of the joint, flats of other
/// thicknesses and an angle out of range said, kept.
#[test]
fn a_bent_plate_is_read_as_two_flats() {
    let text = format!(
        "{BODY}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"8 mm\"\n\
         [[plate]]\nid = \"G2\"\njoint = \"N2\"\nthickness = \"8 mm\"\n\
         [plate.bend]\nto = \"G1\"\nangle = \"90 deg\"\ninner_radius = \"16 mm\"\n\
         line = [[\"0 m\", \"0 m\", \"0 m\"], [\"0.1 m\", \"0 m\", \"0 m\"]]\n"
    );
    let t = parse(&text).unwrap();
    let b = t.plates[1].bend.as_ref().unwrap();
    assert_eq!(b.to, "G1");
    assert!((b.angle.si().to_degrees() - 90.0).abs() < 1e-9);
    assert!((b.inner_radius.si() - 0.016).abs() < 1e-12);
    assert_eq!(b.line.len(), 2);
    assert!(t.plates[0].bend.is_none());
    assert_eq!(t.bent_pairs("N2"), vec![("G2".to_owned(), "G1".to_owned())]);
    // Two flats, not welded: each its own assembly.
    assert_eq!(t.welded_assemblies("N2").len(), 2);
    assert!(
        t.connection_notes().is_empty(),
        "{:?}",
        t.connection_notes()
    );
    let odd = text
        .replace("to = \"G1\"", "to = \"G9\"")
        .replace("angle = \"90 deg\"", "angle = \"190 deg\"");
    let t = parse(&odd).unwrap();
    assert!(t.bent_pairs("N2").is_empty());
    let notes = t.connection_notes();
    assert!(
        notes.iter().any(|n| n.contains("no such plate")),
        "{notes:?}"
    );
    assert!(
        notes.iter().any(|n| n.contains("between 0 and 180")),
        "{notes:?}"
    );
    let thick = text.replacen("thickness = \"8 mm\"", "thickness = \"10 mm\"", 1);
    let notes = parse(&thick).unwrap().connection_notes();
    assert!(
        notes.iter().any(|n| n.contains("differ in thickness")),
        "{notes:?}"
    );
}

/// The member whose own end bolts hold a plate (`held_by`, version 24,
/// 2026-11-22, the joint layout's L14): read; a member not at the joint, or
/// one whose end there does not bolt onto the plate, said and kept; a
/// plate without it reads as before; a file of version 24 is read.
#[test]
fn a_plate_held_by_a_members_end_bolts_is_read() {
    let text = format!(
        "format = \"ohl-structure\"\nversion = 24\n{BODY}\
         [[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\non = \"D1\"\n\
         held_by = \"D1\"\n\
         [[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"G1\"\n"
    );
    let t = parse(&text).unwrap();
    assert_eq!(t.plates[0].held_by.as_deref(), Some("D1"));
    assert!(
        t.connection_notes().is_empty(),
        "{:?}",
        t.connection_notes()
    );
    let not_onto = text.replace("onto = \"G1\"", "onto = \"L1\"");
    let notes = parse(&not_onto).unwrap().connection_notes();
    assert!(
        notes
            .iter()
            .any(|n| n.contains("held_by D1") && n.contains("does not bolt onto the plate")),
        "{notes:?}"
    );
    let away = text.replace("held_by = \"D1\"", "held_by = \"D9\"");
    let notes = parse(&away).unwrap().connection_notes();
    assert!(
        notes
            .iter()
            .any(|n| n.contains("held_by D9") && n.contains("does not meet at joint N2")),
        "{notes:?}"
    );
    let plain = format!("{BODY}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n");
    assert!(parse(&plain).unwrap().plates[0].held_by.is_none());
    assert_eq!(crate::connections::PLATE_HELD_BY_VERSION, 24);
}

/// A plate between the angles of the built-up member it is on (`between`,
/// version 25, 2026-11-22, the joint layout's L15): read; without the
/// member it is on, said and kept; a plate without it reads as before; a
/// file of version 25 is read, a newer one refused.
#[test]
fn a_plate_between_a_built_up_members_angles_is_read() {
    let text = format!(
        "format = \"ohl-structure\"\nversion = 25\n{BODY}\
         [[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"6 mm\"\non = \"D1\"\n\
         between = true\n\
         [[connection]]\nmember = \"L1\"\njoint = \"N2\"\nonto = \"G1\"\n"
    );
    let t = parse(&text).unwrap();
    assert!(t.plates[0].between);
    assert!(
        t.connection_notes().is_empty(),
        "{:?}",
        t.connection_notes()
    );
    let no_on = text.replace("on = \"D1\"\n", "");
    let notes = parse(&no_on).unwrap().connection_notes();
    assert!(
        notes
            .iter()
            .any(|n| n.contains("between") && n.contains("names no member it is on")),
        "{notes:?}"
    );
    let plain = format!("{BODY}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"6 mm\"\n");
    assert!(!parse(&plain).unwrap().plates[0].between);
    assert_eq!(crate::connections::PLATE_BETWEEN_VERSION, 25);
    assert!(parse(&text.replace("version = 25", "version = 26")).is_ok());
}

/// A plate's own bolts onto a second member (`[plate.also_bolts]`, version
/// 26, 2026-11-24, the joint layout's L20: a level plate across two
/// horizontals): read; a member not at the joint, the member its
/// `leg_bolts` name, no places or a count differing said and kept; a plate
/// without them reads as before; a file of version 26 is read, a newer one
/// refused.
#[test]
fn a_plates_own_bolts_onto_a_second_member_are_read() {
    let text = format!(
        "format = \"ohl-structure\"\nversion = 26\n{BODY}\
         [[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"8 mm\"\non = \"L2\"\n\
         [plate.leg_bolts]\nmember = \"L2\"\nleg = \"long\"\n\
         bolts = {{ count = 2, diameter = \"16 mm\" }}\n\
         bolt_points = [[\"60 mm\", \"35 mm\"], [\"110 mm\", \"35 mm\"]]\n\
         [plate.also_bolts]\nmember = \"D1\"\nleg = \"short\"\n\
         bolts = {{ count = 2, diameter = \"16 mm\" }}\n\
         bolt_points = [[\"70 mm\", \"30 mm\"], [\"120 mm\", \"30 mm\"]]\n"
    );
    let t = parse(&text).unwrap();
    let a = t.plates[0].also_bolts.as_ref().expect("read");
    assert_eq!(a.member, "D1");
    assert_eq!(a.bolt_points.len(), 2);
    assert!(
        t.connection_notes().is_empty(),
        "{:?}",
        t.connection_notes()
    );
    let away = text.replace("member = \"D1\"", "member = \"D9\"");
    let notes = parse(&away).unwrap().connection_notes();
    assert!(
        notes
            .iter()
            .any(|n| n.contains("also_bolts onto D9") && n.contains("does not meet at joint N2")),
        "{notes:?}"
    );
    let same = text.replace("member = \"D1\"", "member = \"L2\"");
    let notes = parse(&same).unwrap().connection_notes();
    assert!(
        notes
            .iter()
            .any(|n| n.contains("also_bolts onto L2") && n.contains("its leg_bolts name")),
        "{notes:?}"
    );
    let count = text.replacen("count = 2", "count = 3", 2);
    let notes = parse(&count).unwrap().connection_notes();
    assert!(
        notes.iter().any(|n| n.contains("also_bolts count 3 but 2")),
        "{notes:?}"
    );
    let plain = format!("{BODY}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"8 mm\"\n");
    assert!(parse(&plain).unwrap().plates[0].also_bolts.is_none());
    assert_eq!(crate::connections::PLATE_ALSO_BOLTS_VERSION, 26);
    let newer = format!(
        "version = {}",
        crate::structures::STRUCTURE_FILE_VERSION + 1
    );
    assert!(parse(&text.replace("version = 26", &newer)).is_err());
}
