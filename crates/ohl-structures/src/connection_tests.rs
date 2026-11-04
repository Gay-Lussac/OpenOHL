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
    assert!(parse(&format!("format = \"ohl-structure\"\nversion = 10\n{BODY}")).is_err());
}
