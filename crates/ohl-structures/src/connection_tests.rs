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
    assert!((b.diameter.si() - 0.016).abs() < 1e-12);
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

#[test]
fn wrong_connections_are_refused() {
    for (extra, says) in [
        (format!("{CONNECTION}{CONNECTION}"), "given twice"),
        (
            CONNECTION.replace("gauges = [\"30 mm\"]", "gauges = [\"30 mm\", \"60 mm\"]"),
            "one gauge per gauge line",
        ),
        (
            CONNECTION.replace("gauge_lines = 1", "gauge_lines = 3"),
            "1 or 2",
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
