// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Weight and quantities, and the self-weight from the counted mass
//! (joint details stage 4.6, 2026-11-19), checked by hand on an invented
//! joint: a leg A (N1–N2) spliced at N2 to the leg B (N2–N4) by an outer
//! cover on the long leg, and a member C (N2–N3) bolted onto a gusset
//! plate G1, to which a plate G2 is welded. Every member end has its
//! connection; the far ends are held (`fixed`). The example section and
//! bolt libraries: EA-61x7 in steel of 7 850 kg/m³, the bolt
//! X17-example (0.14 kg with its nut and washer as sold, grip 24 mm; hole
//! 18.5 mm).
//!
//! By hand (heavy count; the light count deducts the holes):
//!
//! - G1, 300 × 200 × 10 mm, the steel of C on it: 0.06 × 0.01 × 7 850 =
//!   4.71 kg; light (0.06 − 2 × π 0.0185²/4) × 0.01 × 7 850 = 4.6678 kg;
//! - G2, 200 × 200 × 8 mm, nothing bolted on it: SIA 261's steel
//!   (78.5 kN/m³ ÷ 9.80665 = 8 004.77 kg/m³): 0.04 × 0.008 × 8 004.77 =
//!   2.5615 kg, light the same;
//! - the weld G2–G1, throat 6 mm, 220 mm given, plates at 90°: 36 mm² ×
//!   0.22 m × 8 004.77 = 0.063 40 kg (a length given: both ways);
//! - the cover, 60 × 400 × 8 mm: 0.024 × 0.008 × 7 850 = 1.5072 kg; light
//!   with the 4 holes of A and B: (0.024 − 4 × 268.80 mm²) × 0.008 × 7 850
//!   = 1.4397 kg;
//! - the bolts: 2 of C, 2 of A, 2 of B, each clamping less than 24 mm:
//!   6 × 0.14 = 0.84 kg.

use std::path::Path;

use ohl_bom::structure::PartKind;
use ohl_sections::SectionLibrary;
use ohl_structure_link::{CaseToRun, analyse_cases, structure_quantities};
use ohl_structure_loads::{At, LoadRow, LoadTable, Source};
use ohl_structures::bolts::BoltLibrary;
use ohl_structures::{LibraryItem, StructureType};

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."))
}

fn sections() -> SectionLibrary {
    SectionLibrary::load(&root().join("libraries/sections/example-sections.ohlsec")).unwrap()
}

fn bolts() -> BoltLibrary {
    BoltLibrary::load(&root().join("libraries/bolts/example-bolts.ohlb")).unwrap()
}

/// The joint; `far`: the far end of C at N3 (its connection, or nothing:
/// a joint not defined); `option`: a top-level line.
fn joint(far: &str, option: &str) -> String {
    format!(
        r#"id = "W"
name = "weighed joint (invented)"
kind = "lattice"
{option}
[analysis]
nonlinear = false
dead_load_factor = 1.3
capacity = "simple"
[[node]]
id = "N1"
point = [0, 0, 0]
support = "fixed"
[[node]]
id = "N2"
point = [0, 0, 3]
[[node]]
id = "N3"
point = [2, 0, 3]
support = "fixed"
[[node]]
id = "N4"
point = [0, 0, 6]
support = "fixed"
[[member]]
id = "A"
nodes = ["N1", "N2"]
section = "EA-61x7"
[[member]]
id = "B"
nodes = ["N2", "N4"]
section = "EA-61x7"
[[member]]
id = "C"
nodes = ["N2", "N3"]
section = "EA-61x7"
[[connection]]
member = "A"
joint = "N1"
kind = "fixed"
[[connection]]
member = "B"
joint = "N4"
kind = "fixed"
{far}
[[connection]]
member = "A"
joint = "N2"
onto = "S1"
leg = "long"
bolts = {{ count = 2, bolt = "X17-example" }}
bolt_points = [["40 mm", "30 mm"], ["100 mm", "30 mm"]]
[[connection]]
member = "B"
joint = "N2"
onto = "S1"
leg = "long"
bolts = {{ count = 2, bolt = "X17-example" }}
bolt_points = [["40 mm", "30 mm"], ["100 mm", "30 mm"]]
[[connection]]
member = "C"
joint = "N2"
onto = "G1"
leg = "long"
bolts = {{ count = 2, bolt = "X17-example" }}
bolt_points = [["60 mm", "30 mm"], ["120 mm", "30 mm"]]
[[splice]]
id = "S1"
joint = "N2"
members = ["A", "B"]
[[splice.cover]]
face = "outer"
leg = "long"
thickness = "8 mm"
outline = [["0 m", "-0.01 m", "2.8 m"], ["0.06 m", "-0.01 m", "2.8 m"], ["0.06 m", "-0.01 m", "3.2 m"], ["0 m", "-0.01 m", "3.2 m"]]
[[plate]]
id = "G1"
joint = "N2"
thickness = "10 mm"
outline = [["0 m", "0 m", "2.9 m"], ["0.3 m", "0 m", "2.9 m"], ["0.3 m", "0 m", "3.1 m"], ["0 m", "0 m", "3.1 m"]]
[[plate]]
id = "G2"
joint = "N2"
thickness = "8 mm"
outline = [["0 m", "0 m", "2.9 m"], ["0 m", "0.2 m", "2.9 m"], ["0 m", "0.2 m", "3.1 m"], ["0 m", "0 m", "3.1 m"]]
[[plate.weld]]
to = "G1"
size = "6 mm"
length = "220 mm"
"#
    )
}

const FAR: &str = "[[connection]]\nmember = \"C\"\njoint = \"N3\"\nkind = \"fixed\"";

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn every_part_weighed_by_hand() {
    let text = joint(FAR, "");
    let b = structure_quantities(&text, &sections(), &bolts()).unwrap();
    assert!(b.every_joint_defined(), "{:?}", b.undefined);
    let one = |id: &str| {
        b.parts
            .iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("{id} not counted: {:#?}", b.parts))
    };
    let g1 = one("G1");
    assert!(near(g1.quantity, 0.06, 1e-12));
    assert!(near(g1.mass, 4.71, 1e-9), "{}", g1.mass);
    let holes = 2.0 * std::f64::consts::PI * 0.0185f64.powi(2) / 4.0;
    assert!(near(g1.light, (0.06 - holes) * 0.01 * 7850.0, 1e-9));
    assert!(near(g1.light, 4.6678, 1e-4), "{}", g1.light);
    assert_eq!(g1.item, "plate 10 mm");
    let g2 = one("G2");
    assert!(near(g2.mass, 2.5615, 1e-4), "{}", g2.mass);
    assert_eq!(g2.mass, g2.light);
    let weld = b.parts.iter().find(|p| p.kind == PartKind::Weld).unwrap();
    assert_eq!(weld.item, "fillet a 6 mm");
    assert!(near(weld.quantity, 0.22, 1e-12));
    assert!(
        near(weld.mass, 36e-6 * 0.22 * 8004.77, 1e-6),
        "{}",
        weld.mass
    );
    assert_eq!(weld.mass, weld.light);
    let cover = b.parts.iter().find(|p| p.kind == PartKind::Cover).unwrap();
    assert!(near(cover.mass, 1.5072, 1e-9), "{}", cover.mass);
    assert!(near(cover.light, 1.4397, 1e-4), "{}", cover.light);
    let bolts: Vec<_> = b
        .parts
        .iter()
        .filter(|p| p.kind == PartKind::Bolt)
        .collect();
    assert_eq!(
        bolts.iter().map(|p| p.count).sum::<usize>(),
        6,
        "{bolts:#?}"
    );
    assert!(near(bolts.iter().map(|p| p.mass).sum::<f64>(), 0.84, 1e-12));
    assert!(
        bolts
            .iter()
            .all(|p| !p.estimated && p.item.starts_with("X17-example grip"))
    );
    // The members: the section's area × 7 850 kg/m³ × 8 m.
    let area = sections().section("EA-61x7").unwrap().properties().area;
    assert!(near(b.members_mass, area * 7850.0 * 8.0, 1e-9));
    // C's light count: short of the work point at N2 (its cut) and less
    // its two holes through its 7 mm leg (2 × 268.80 mm² × 7 mm × 7 850 =
    // 0.0295 kg).
    let c = one("C");
    let per_m = area * 7850.0;
    assert!(near(c.mass, per_m * 2.0, 1e-9));
    assert!(c.light < c.mass - 0.0295 + 1e-6, "{} {}", c.light, c.mass);
    let parts = 4.71 + 2.5615 + 36e-6 * 0.22 * 8004.77 + 1.5072 + 0.84;
    assert!(near(b.mass, b.members_mass + parts, 1e-3), "{}", b.text());
    // The held far ends: said, not on the safe side.
    assert!(
        b.notes
            .iter()
            .any(|n| n.starts_with("NOT ON THE SAFE SIDE: 3 pinned or fixed"))
    );
    // The bill: one row per kind and item, its CSV and text.
    assert!(b.csv().contains("total,,,,,"));
    assert!(b.text().contains("plates: 2 pcs"));
}

#[test]
fn a_missing_end_leaves_its_joint_out() {
    let b = structure_quantities(&joint("", ""), &sections(), &bolts()).unwrap();
    assert!(!b.every_joint_defined());
    assert_eq!(b.undefined.len(), 1, "{:?}", b.undefined);
    assert_eq!(b.undefined[0].0, "N3");
    assert!(b.undefined[0].1.contains("C: no connection"));
}

fn table() -> LoadTable {
    let row = |case: &str| LoadRow {
        case: case.into(),
        load: None,
        at: At::Joint,
        label: "N2".into(),
        force: [0.0, 0.0, 0.0],
        source: Source::Typed,
        evaluated: None,
        missing: None,
        note: String::new(),
        area: None,
        moment: [0.0; 3],
    };
    LoadTable {
        rows: vec![row("dead"), row("uplift")],
    }
}

/// Σ of the vertical reactions of the cases "dead" (self-weight × 1.35)
/// and "uplift" (× 0.9), kg (÷ g).
fn reactions(text: &str) -> [f64; 2] {
    let s = StructureType::parse(Path::new("w.ohls"), text).unwrap();
    let cases = [
        CaseToRun {
            name: "dead".into(),
            strength: 1.0,
            self_weight: 1.35,
        },
        CaseToRun {
            name: "uplift".into(),
            strength: 1.0,
            self_weight: 0.9,
        },
    ];
    let lib = sections();
    // The bolts beside the file (`Bolts/` in its folder).
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("structure_weight");
    std::fs::create_dir_all(dir.join("Bolts")).unwrap();
    std::fs::copy(
        root().join("libraries/bolts/example-bolts.ohlb"),
        dir.join("Bolts/example-bolts.ohlb"),
    )
    .unwrap();
    let runs = analyse_cases(
        &s,
        text,
        Some(&dir),
        &lib,
        &table(),
        &cases,
        &ohl_core::budget::Budget::new(),
    )
    .unwrap();
    let z = |k: usize| {
        let r = runs[k].1.as_ref().unwrap();
        r.reaction_sum[2] / ohl_core::units::STANDARD_GRAVITY
    };
    [z(0), z(1)]
}

/// Every joint defined: the dead load is the counted mass (heavy count
/// × 1.35; the light count × 0.9 for the case holding the structure);
/// one end without its connection, or `counted_self_weight = false`: the
/// members' steel × 1.3, as before.
#[test]
fn self_weight_from_the_counted_mass() {
    let text = joint(FAR, "");
    let b = structure_quantities(&text, &sections(), &bolts()).unwrap();
    let [dead, uplift] = reactions(&text);
    assert!(
        near(dead, b.mass * 1.35, 1e-6),
        "{dead} vs {}",
        b.mass * 1.35
    );
    assert!(
        near(uplift, b.light * 0.9, 1e-6),
        "{uplift} vs {}",
        b.light * 0.9
    );
    let factor = b.members_mass * 1.3;
    for kept in [joint("", ""), joint(FAR, "counted_self_weight = false")] {
        let [dead, uplift] = reactions(&kept);
        assert!(near(dead, factor * 1.35, 1e-6), "{dead}");
        assert!(near(uplift, factor * 0.9, 1e-6), "{uplift}");
    }
    // Here the counted mass is lighter than the members × 1.3.
    assert!(b.mass < factor);
}

/// What a run says: the counted mass and the light count, or the factor
/// kept and why (per structure).
#[test]
fn the_run_says_which() {
    let note = |text: &str| {
        let t = StructureType::parse(Path::new("w.ohls"), text).unwrap();
        let m = ohl_structures_fe::Model::from_structure_type_with_bolts(
            text,
            &sections(),
            &bolts(),
            None,
        )
        .unwrap();
        ohl_structure_link::counted_self_weight(&t, text, &sections(), &bolts(), None, &m)
    };
    let counted = note(&joint(FAR, ""));
    assert!(counted.counted && counted.light_ratio < 1.0);
    let words = counted.note.unwrap();
    assert!(
        words.starts_with("self weight: the counted mass,"),
        "{words}"
    );
    assert!(
        words.contains("instead of the dead-load factor 1.3"),
        "{words}"
    );
    assert!(words.contains("take the light count"), "{words}");
    let kept = note(&joint("", "")).note.unwrap();
    assert_eq!(
        kept,
        "self weight: the members' steel × the dead-load factor 1.3 (factor kept: 1 joint(s) \
         not defined)"
    );
    let asked = note(&joint(FAR, "counted_self_weight = false"))
        .note
        .unwrap();
    assert!(
        asked.ends_with("(factor kept: counted_self_weight = false)"),
        "{asked}"
    );
}
