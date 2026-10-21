// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Attachment points linked to joints (2026-10-21): the lines written
//! before open where they were, bit for bit; the links written and read
//! back move nothing; an own point follows its joint and offset, turns
//! with its structure, and is unlinked when its joint is gone.

use std::path::{Path, PathBuf};

use ohl_core::units::Temperature;
use ohl_structures::{InsulatorLibrary, LibraryItem, StructureLibrary, StructureType};
use ohl_weather::WeatherCase;

use crate::{Line, StructurePoint};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line")
}

fn typed() -> (Line, StructureLibrary) {
    let line = Line::load(&root().join("synthetic-line-typed.ohl.json")).unwrap();
    let structures = StructureLibrary::load_dir(&root().join("Structures")).unwrap();
    (line, structures)
}

/// Every wire's solved shape, exactly (`{:?}` prints a float's every bit).
fn solved(line: &Line) -> String {
    let cables = ohl_cables::Library::load(&root().join("../../libraries/cables")).unwrap();
    let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
    format!("{:?}", crate::solve::solve_line(line, &cables, &case))
}

/// The points, wire points and strings of every structure, exactly.
fn geometry(line: &Line) -> Vec<String> {
    line.structures
        .iter()
        .map(|s| {
            let points: Vec<_> = s.points.iter().map(|p| (&p.label, p.position)).collect();
            format!("{} {points:?} {:?} {:?}", s.id, s.wires, s.strings)
        })
        .collect()
}

/// The typed example: every type point linked, no offset; written with
/// its links as the structures' own points (as a later save writes
/// overrides) or without them (as files before), it resolves and solves
/// bit for bit as it is.
#[test]
fn links_move_nothing() {
    let (line, structures) = typed();
    let insulators = InsulatorLibrary::default();
    let resolved = line.resolve(&structures, &insulators).unwrap();
    for s in &resolved.structures {
        for p in &s.points {
            assert!(p.is_linked(), "{} {}", s.id, p.label);
            assert_eq!(p.offset, [0.0; 3]);
        }
    }
    // type-02's A11 is on joint A2.
    let t2 = resolved.structure("T2").unwrap();
    assert_eq!(t2.point("A11").unwrap().joint.as_deref(), Some("A2"));
    for (what, keep_links) in [("with links", true), ("written before", false)] {
        let mut own = line.clone();
        for (s, r) in own.structures.iter_mut().zip(&resolved.structures) {
            s.points = r
                .points
                .iter()
                .map(|p| StructurePoint {
                    joint: if keep_links { p.joint.clone() } else { None },
                    ..p.clone()
                })
                .collect();
        }
        // Through the file and back.
        let own = Line::from_json(&own.to_json()).unwrap();
        let again = own.resolve(&structures, &insulators).unwrap();
        assert_eq!(geometry(&again), geometry(&resolved), "{what}");
        assert_eq!(solved(&again), solved(&resolved), "{what}");
        for s in &again.structures {
            assert!(s.points.iter().all(StructurePoint::is_linked), "{what}");
        }
    }
}

/// A type of one arm, joint `N2` at its tip carrying point `A1`, for the
/// own points below.
fn arm_text(tip_x: &str) -> String {
    format!(
        "id = \"arm\"\nname = \"arm\"\nkind = \"pole\"\n\
         [[node]]\nid = \"N1\"\npoint = [\"0 m\", \"0 m\", \"20 m\"]\n\
         [[node]]\nid = \"N2\"\npoint = [\"{tip_x}\", \"0 m\", \"20 m\"]\n\
         [[node]]\nid = \"N3\"\npoint = [\"-3 m\", \"0 m\", \"20 m\"]\n\
         [[member]]\nid = \"arm\"\nnodes = [\"N1\", \"N2\"]\n\
         [[member]]\nid = \"arm2\"\nnodes = [\"N1\", \"N3\"]\n\
         [[point]]\nlabel = \"A1\"\nnode = \"N2\"\n\
         [[wire]]\nlabel = \"W1\"\nset = 1\n\
         [[string]]\nlabel = \"S1\"\ntop = \"A1\"\nwire = \"W1\"\nlength = \"2 m\"\n"
    )
}

fn arm_type(tip_x: &str) -> StructureType {
    StructureType::parse(Path::new("arm.ohls"), &arm_text(tip_x)).unwrap()
}

fn library(t: StructureType) -> StructureLibrary {
    let mut lib = StructureLibrary::default();
    lib.insert(Path::new("arm.ohls"), t).unwrap();
    lib
}

/// One structure of type `arm` at (100, 200, 500), turned 90° (its x
/// axis, the right of the line, to the north), with `points` of its own.
fn one(points: Vec<StructurePoint>) -> Line {
    let mut line = Line::from_json(
        r#"{"format": "ohl-line", "version": 7, "name": "t", "structures": [
            {"id": "T1", "position": [100.0, 200.0, 500.0], "orientation_deg": 90.0,
             "type": "arm"}], "sections": []}"#,
    )
    .unwrap();
    line.structures[0].points = points;
    line
}

fn close(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() < 1e-9)
}

/// An own point naming a joint sits at the joint as placed plus its
/// offset turned with the structure; it follows the joint when the type
/// moves it, and is unlinked (kept where it was) when the joint is gone.
#[test]
fn an_own_point_follows_its_joint() {
    let insulators = InsulatorLibrary::default();
    let point = StructurePoint {
        joint: Some("N2".into()),
        offset: [0.0, 0.4, -0.2],
        ..StructurePoint::at("A1", [0.0; 3])
    };
    let line = one(vec![point]);
    let at = |lib: &StructureLibrary| {
        let r = line.resolve(lib, &insulators).unwrap();
        r.structures[0].point("A1").unwrap().clone()
    };
    // Tip 5 m to the right = 5 m north; the offset 0.4 m ahead = west.
    let p = at(&library(arm_type("5 m")));
    assert!(close(p.position, [99.6, 205.0, 519.8]), "{p:?}");
    assert!(p.is_linked());
    // The type's arm made longer: the point follows its joint.
    let p = at(&library(arm_type("6 m")));
    assert!(close(p.position, [99.6, 206.0, 519.8]), "{p:?}");
    // The joint renamed in the type: unlinked, where it was written.
    let mut gone = line.clone();
    gone.structures[0].points[0].position = [1.0, 2.0, 3.0];
    let text = arm_text("5 m").replace("N2", "N9");
    let renamed = library(StructureType::parse(Path::new("arm.ohls"), &text).unwrap());
    let r = gone.resolve(&renamed, &insulators).unwrap();
    let p = r.structures[0].point("A1").unwrap();
    assert!(p.unlinked && !p.is_linked(), "{p:?}");
    assert_eq!(p.position, [1.0, 2.0, 3.0]);
    // Its string still hangs from it: the line stays whole.
    assert_eq!(r.structures[0].strings_to("W1").count(), 1);
}

/// Points written before the links: an override of the type's point
/// takes its joint, the offset the difference (the point stays where it
/// was, bit for bit); a point of its own at a joint is linked to it, one
/// elsewhere is unlinked; an unlinked one stays so.
#[test]
fn points_written_before_the_links() {
    let insulators = InsulatorLibrary::default();
    let lib = library(arm_type("5 m"));
    // The type's A1 as placed: (100, 205, 520).
    let moved = [100.1 + 1e-12, 205.0, 519.5];
    let line = one(vec![
        StructurePoint::at("A1", moved),
        StructurePoint::at("B", [100.0, 197.0, 520.0]),
        StructurePoint::at("C", [90.0, 190.0, 510.0]),
        StructurePoint {
            unlinked: true,
            joint: Some("N2".into()),
            ..StructurePoint::at("D", [1.0, 1.0, 1.0])
        },
    ]);
    let r = line.resolve(&lib, &insulators).unwrap();
    let s = &r.structures[0];
    let a1 = s.point("A1").unwrap();
    assert_eq!(a1.position, moved);
    assert_eq!(a1.joint.as_deref(), Some("N2"));
    // 0.1 m north = the structure's −y (behind), 0.5 m down.
    assert!(close(a1.offset, [0.0, -0.1, -0.5]), "{a1:?}");
    let b = s.point("B").unwrap();
    assert_eq!((b.joint.as_deref(), b.offset), (Some("N3"), [0.0; 3]));
    let c = s.point("C").unwrap();
    assert!(c.unlinked && c.joint.is_none());
    let d = s.point("D").unwrap();
    assert!(d.unlinked);
    assert_eq!(d.position, [1.0, 1.0, 1.0]);
    // Written and read again (links and all), it resolves the same.
    let mut stored = line.clone();
    stored.structures[0].points = s.points.clone();
    let back = Line::from_json(&stored.to_json()).unwrap();
    assert_eq!(back, stored);
    let again = back.resolve(&lib, &insulators).unwrap();
    assert_eq!(geometry(&again), geometry(&r));
}
