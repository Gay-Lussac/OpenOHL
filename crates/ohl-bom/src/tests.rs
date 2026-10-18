// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::{Path, PathBuf};

use ohl_line::Line;

use super::*;

fn ex() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line")
}

fn library() -> ohl_cables::Library {
    ohl_cables::Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"))
        .unwrap()
}

fn typed_line() -> Line {
    let line = Line::load(&ex().join("synthetic-line-typed.ohl.json")).unwrap();
    let types = ohl_structures::StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    line.resolve(&types, &ohl_structures::InsulatorLibrary::default())
        .unwrap()
}

fn cost() -> CostData {
    CostData::load(&ex().join("unit-costs.ohlcost")).unwrap()
}

/// Counts of the synthetic typed line, by hand from the line file: 7
/// structures of 6 types; 13 attachments strung (S1 and S2 through T1–T5,
/// S3 through T3 set 2, T6, T7), of which T1, T5 (2 each), T3 set 2 and T7
/// are strain strings; 6 spans; 10 section spans of a single wire.
#[test]
fn synthetic_line_counts() {
    let bom = bill_of_materials(&typed_line(), &library(), &cost().hardware);
    let count = |t: &str| {
        bom.structures
            .iter()
            .find(|s| s.structure_type.as_deref() == Some(t))
            .map_or(0, |s| s.count)
    };
    assert_eq!(count("type-01"), 1);
    assert_eq!(count("type-02"), 2);
    assert_eq!(
        bom.structures.iter().map(|s| s.count).sum::<usize>(),
        7,
        "{:?}",
        bom.structures
    );
    let insulators: BTreeMap<&str, usize> = bom
        .insulators
        .iter()
        .map(|i| (i.id.as_str(), i.count))
        .collect();
    assert_eq!(
        insulators,
        BTreeMap::from([
            ("string strain 5.025 m", 6),
            ("string suspension 3.500 m", 7)
        ])
    );
    assert_eq!(bom.spans, 6);
    // 2 dampers per wire and span; no bundles, no spacers.
    assert_eq!(
        bom.hardware,
        vec![HardwareCount {
            item: "damper".into(),
            count: 20
        }]
    );
    assert!(bom.notes.is_empty(), "{:?}", bom.notes);
}

/// Wire lengths: S2 is clipped in with given lengths (their sum exactly);
/// S1 is sagged to 25 kN at 15 °C: its cut length is close to the chords
/// plus the parabolic slack 8d²/3c with d = wc²/8H per span, minus the
/// elastic stretch H·L/EA.
#[test]
fn synthetic_line_lengths() {
    let line = typed_line();
    let lib = library();
    let bom = bill_of_materials(&line, &lib, &cost().hardware);
    let s2 = bom.sections.iter().find(|s| s.section == "S2").unwrap();
    assert_eq!(s2.method, "given");
    let given: f64 = 306.074 + 330.73 + 300.741 + 316.129;
    assert!((s2.spans_m.iter().sum::<f64>() - given).abs() < 1e-9);

    let s1 = bom.sections.iter().find(|s| s.section == "S1").unwrap();
    assert_eq!(s1.method, "stringing");
    let cable = lib.get("acsr-drake-26-7").unwrap();
    let (w, ea) = (cable.weight().si(), cable.axial_stiffness().si());
    let h = 25_000.0;
    let section = &line.sections[0];
    let mut hand = 0.0;
    for p in section.points.windows(2) {
        let a = line.wire_point(&p[0]).unwrap().position;
        let b = line.wire_point(&p[1]).unwrap().position;
        let c = distance(a, b);
        let d = w * c * c / (8.0 * h);
        hand += (c + 8.0 * d * d / (3.0 * c)) * (1.0 - h / ea);
    }
    let got: f64 = s1.spans_m.iter().sum();
    // The strain assemblies swing into line with the wire, so the clamps
    // move by centimetres: agree within 0.1 %.
    assert!((got - hand).abs() < 1e-3 * hand, "{got} vs {hand}");

    // Conductors: S1, S2 and S3 (on L1 through the jumper); the jumper by
    // its chord and 1.5 m sag.
    let conductor = bom.cables.iter().find(|c| c.role == "conductor").unwrap();
    assert_eq!(conductor.items, 3);
    let sum: f64 = bom
        .sections
        .iter()
        .map(|s| s.spans_m.iter().sum::<f64>())
        .sum();
    assert!((conductor.length_m - sum).abs() < 1e-9);
    let jumper = bom.cables.iter().find(|c| c.role == "jumper").unwrap();
    let j = &line.jumpers[0];
    let chord = distance(
        line.wire_point(j.from.point().unwrap()).unwrap().position,
        line.wire_point(j.to.point().unwrap()).unwrap().position,
    );
    assert!((jumper.length_m - (chord + 8.0 * 1.5 * 1.5 / (3.0 * chord))).abs() < 1e-9);
}

/// Bundles multiply wire lengths and dampers and get spacers.
#[test]
fn bundles_multiply() {
    let mut line = typed_line();
    line.sections[1].bundle = 2;
    line.sections[1].bundle_spacing = Some(0.4);
    let hw = cost().hardware;
    let bom = bill_of_materials(&line, &library(), &hw);
    let s2 = bom.sections.iter().find(|s| s.section == "S2").unwrap();
    assert_eq!(s2.bundle, 2);
    // Spans of 306, 331, 301, 316 m with a spacer every 70 m: 4 + 4 + 4 + 4.
    let spacers = bom.hardware.iter().find(|h| h.item == "spacer").unwrap();
    assert_eq!(spacers.count, 16);
    let dampers = bom.hardware.iter().find(|h| h.item == "damper").unwrap();
    assert_eq!(dampers.count, 20 + 8);
}

#[test]
fn estimate_prices_and_lists_missing() {
    let line = typed_line();
    let mut c = cost();
    let bom = bill_of_materials(&line, &library(), &c.hardware);
    let e = CostEstimate::new(bom.clone(), Some(&c));
    assert!(e.missing.is_empty(), "{:?}", e.missing);
    // Structures: supply + foundations + erection.
    let structures: f64 = e
        .lines
        .iter()
        .filter(|l| ["structures", "foundations", "erection"].contains(&l.category.as_str()))
        .filter_map(|l| l.amount)
        .sum();
    let by_hand = 30000.0
        + 9000.0
        + 3000.0
        + 2.0 * (14000.0 + 5000.0 + 3000.0)
        + (26000.0 + 8000.0 + 5000.0)
        + (30000.0 + 9000.0 + 3000.0)
        + (12000.0 + 4500.0 + 3000.0)
        + (28000.0 + 8500.0 + 3000.0);
    assert!((structures - by_hand).abs() < 1e-6, "{structures}");
    let fixed = e.lines.iter().find(|l| l.category == "fixed").unwrap();
    assert_eq!(fixed.amount, Some(20000.0));
    let total: f64 = e.lines.iter().filter_map(|l| l.amount).sum();
    assert_eq!(e.total, total);

    c.structures.retain(|s| s.structure_type != "type-05");
    c.cables.clear();
    let e = CostEstimate::new(bom.clone(), Some(&c));
    assert_eq!(
        e.missing,
        vec![
            "structures: type-05".to_owned(),
            "wire: acsr-drake-26-7 (conductor)".to_owned(),
            "wire: acsr-drake-26-7 (jumper)".to_owned()
        ]
    );

    // Outputs: text, CSV (one row per line, a total), JSON.
    let e = CostEstimate::new(bom.clone(), Some(&cost()));
    let text = e.text();
    assert!(
        text.contains("Total:") && text.contains("type-03"),
        "{text}"
    );
    let csv = e.csv();
    assert_eq!(csv.lines().count(), 1 + e.lines.len() + 1);
    assert!(csv.lines().last().unwrap().starts_with("total,"));
    let json = serde_json::to_string(&e).unwrap();
    assert!(json.contains("\"currency\":\"EUR\""));

    // Without a cost file: quantities only.
    let q = CostEstimate::new(bom, None);
    assert!(!q.priced && q.missing.is_empty() && q.total == 0.0);
    assert!(!q.text().contains("Total"));
}

#[test]
fn shipped_example_cost_file_reads() {
    let c = cost();
    assert_eq!(c.currency, "EUR");
    assert_eq!(c.structure_total("type-03", None), Some(39000.0));
}
