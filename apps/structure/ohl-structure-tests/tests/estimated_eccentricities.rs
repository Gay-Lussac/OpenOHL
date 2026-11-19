// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The option "estimated eccentricities" (O45 A2, owner 2026-11-19) run
//! through the link as OHL-Structure runs it, on an invented K-brace (two
//! diagonals and a shallow redundant meeting a leg at N2, the example
//! sections): off, the members as before; on, the joint layout's planner
//! estimates the ends' shifts and the results say "estimated" and "upper
//! bound at N ends"; every member's usage at least that without them (the
//! larger of the two solves), the same every run; an upper bound's offset
//! (its sign not known) only in its member's own check, at a frame joint
//! too (2026-11-19, the owner's decision on A1); at level 4 with the joint
//! details taken, not used, said.

use std::path::Path;

use ohl_structure_link::eccentricities::{Source, set_option};
use ohl_structure_link::{CaseToRun, RunInput, StructureRun, analyse_structure};
use ohl_structure_loads::{At, LoadRow, LoadTable, Source as RowSource};
use ohl_structures::{LibraryItem, StructureType};

/// The K-brace; `legs`: the legs' member kind.
fn tower(legs: &str) -> String {
    format!(
        r#"id = "K"
name = "k"
kind = "lattice"

[analysis]
self_weight = false
capacity = "simple"

[[node]]
id = "N1"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N2"
point = ["0 m", "0 m", "3 m"]

[[node]]
id = "N3"
point = ["0 m", "0 m", "6 m"]
support = "fixed"

[[node]]
id = "N4"
point = ["2.5 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N5"
point = ["2.5 m", "0 m", "6 m"]
support = "fixed"

[[node]]
id = "N6"
point = ["2.5 m", "0 m", "0.13 m"]
support = "fixed"

[[member]]
id = "L1"
nodes = ["N1", "N2"]
section = "EA-61x7"
kind = "{legs}"
orientation = "x+y+"
design = {{ class = "leg" }}

[[member]]
id = "L2"
nodes = ["N2", "N3"]
section = "EA-61x7"
kind = "{legs}"
orientation = "x+y+"
design = {{ class = "leg" }}

[[member]]
id = "D1"
nodes = ["N4", "N2"]
section = "EA-61x7"
orientation = "y-z+"
design = {{ class = "bracing" }}

[[member]]
id = "D2"
nodes = ["N5", "N2"]
section = "EA-61x7"
orientation = "z-y-"
design = {{ class = "bracing" }}

[[member]]
id = "R1"
nodes = ["N6", "N2"]
section = "EA-61x7"
orientation = "y-z+"
design = {{ class = "redundant" }}
"#
    )
}

fn with_option(text: &str) -> String {
    let mut doc: toml_edit::DocumentMut = text.parse().unwrap();
    set_option(&mut doc, true);
    doc.to_string()
}

fn library() -> ohl_sections::SectionLibrary {
    ohl_sections::SectionLibrary::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
    )
    .unwrap()
}

fn run(text: &str) -> StructureRun {
    let t = StructureType::parse(Path::new("k.ohls"), text).unwrap();
    let row = |force: [f64; 3]| LoadRow {
        case: "push".into(),
        load: None,
        at: At::Joint,
        label: "N2".into(),
        force,
        source: RowSource::Typed,
        evaluated: None,
        missing: None,
        note: String::new(),
        area: None,
        moment: [0.0; 3],
    };
    let table = LoadTable {
        rows: vec![row([4e3, 1e3, 6e3])],
    };
    let cases = [CaseToRun {
        name: "push".into(),
        strength: 1.0,
        self_weight: 0.0,
    }];
    analyse_structure(
        &RunInput {
            structure: &t,
            text,
            dir: None,
            sections: &library(),
            table: &table,
            recipes: &[],
            cases: &cases,
            extension: None,
            bolts: None,
            feet: None,
        },
        &ohl_structures_fe::default_budget(),
    )
    .unwrap()
}

fn usages(r: &StructureRun) -> Vec<(String, Option<f64>)> {
    let fe = r.cases[0].fe.as_ref().unwrap().as_ref().unwrap();
    fe.members.iter().map(|m| (m.id.clone(), m.usage)).collect()
}

#[test]
fn on_every_member_takes_at_least_its_usage_without_and_the_results_say_so() {
    for legs in ["truss", "frame"] {
        let off = run(&tower(legs));
        assert!(off.estimates.is_none());
        assert!(
            !off.notes
                .iter()
                .any(|n| n.contains("estimated eccentricities")),
            "{:?}",
            off.notes
        );
        let text = with_option(&tower(legs));
        let on = run(&text);
        let used = on.estimates.as_ref().expect("used");
        let e = &used.estimates;
        // D1 and D2 planned, R1 (in line with D1 on one plate) bounded.
        assert!(e.count(Source::Estimated) >= 2, "{e:?}");
        let r1 = e.ends.iter().find(|x| x.member == "R1").unwrap();
        assert_eq!(r1.source, Source::UpperBound);
        // Two solves (2026-11-19): R1's upper bound of unknown sign in its
        // own check, never in the solve.
        assert_eq!(used.solves, 2);
        assert!(used.sign_free >= 1, "{used:?}");
        assert!(
            !on.notes.iter().any(|n| n.contains("their signs are taken")),
            "no mixed-sign gap: {:?}",
            on.notes
        );
        let note = on
            .notes
            .iter()
            .find(|n| n.starts_with("estimated eccentricities (an option at level 3)"))
            .unwrap_or_else(|| panic!("{:?}", on.notes));
        assert!(note.contains("estimated from its plan"), "{note}");
        assert!(
            note.contains(&format!(
                "upper bound at {} end(s)",
                e.count(Source::UpperBound)
            )),
            "{note}"
        );
        // Each member's usage at least that without the estimates.
        let (a, b) = (usages(&off), usages(&on));
        assert_eq!(a.len(), b.len());
        for ((id, u0), (_, u1)) in a.iter().zip(&b) {
            let (u0, u1) = (u0.unwrap(), u1.unwrap());
            assert!(u1 >= u0 - 1e-12, "{legs} {id}: {u1} < {u0}");
        }
        // R1's own end at N2 bounded: said in its words, its usage up — in
        // its own check whether the legs are frames or not (sign-free).
        let fe = on.cases[0].fe.as_ref().unwrap().as_ref().unwrap();
        let r1 = fe.members.iter().find(|m| m.id == "R1").unwrap();
        assert!(
            r1.eccentric
                .as_deref()
                .is_some_and(|w| w.contains("upper bound at N2")),
            "{:?}",
            r1.eccentric
        );
        let u = |list: &[(String, Option<f64>)], id: &str| {
            list.iter().find(|x| x.0 == id).unwrap().1.unwrap()
        };
        assert!(u(&b, "R1") > u(&a, "R1") + 1e-6, "{legs}: {b:?} vs {a:?}");
        assert!(r1.governs.contains("sign-free"), "{legs}: {}", r1.governs);
        // Nothing of R1's bound in the solve: the other members as without
        // the option (the planned shifts here are all 0).
        for id in ["L1", "L2", "D1", "D2"] {
            assert!((u(&b, id) - u(&a, id)).abs() < 1e-12, "{legs} {id}");
        }
        // The same every run.
        assert_eq!(usages(&run(&text)), b);
    }
}

#[test]
fn with_the_joint_details_taken_the_estimates_are_not_used() {
    // Every end with a connection, joint details asked: level 4.
    let mut text = with_option(&tower("frame"));
    text = text.replacen(
        "kind = \"lattice\"\n",
        "kind = \"lattice\"\njoint_details = true\n",
        1,
    );
    for (m, a, b) in [
        ("L1", "N1", "N2"),
        ("L2", "N2", "N3"),
        ("D1", "N4", "N2"),
        ("D2", "N5", "N2"),
        ("R1", "N6", "N2"),
    ] {
        for j in [a, b] {
            text.push_str(&format!(
                "\n[[connection]]\nmember = \"{m}\"\njoint = \"{j}\"\nbolts = {{ count = 2, \
                 diameter = \"16 mm\" }}\n"
            ));
        }
    }
    let t = StructureType::parse(Path::new("k.ohls"), &text).unwrap();
    assert_eq!(
        t.detail(),
        ohl_structures::Detail::Joints,
        "{:?}",
        t.ends_without_joint_details()
    );
    let on = run(&text);
    assert!(on.estimates.is_none());
    assert!(
        on.notes
            .iter()
            .any(|n| n.starts_with("estimated eccentricities: not used — the joint details")),
        "{:?}",
        on.notes
    );
}
