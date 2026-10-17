// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The interchange formats on the synthetic example line: round trips
//! and files read back.

use std::collections::BTreeMap;
use std::io::Read as _;

use ols_line::{Line, Stringing};

use super::tests::{model, root};
use super::*;

fn export(id: &str, m: &ExportModel, options: &Options) -> Vec<u8> {
    Registry::with_builtin()
        .exporter(id)
        .unwrap()
        .export(m, options)
        .unwrap()
}

fn opts(pairs: &[(&str, &str)]) -> Options {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

fn unzip(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|k| {
            let mut f = zip.by_index(k).unwrap();
            let mut v = Vec::new();
            f.read_to_end(&mut v).unwrap();
            (f.name().to_owned(), v)
        })
        .collect()
}

/// The example line with every optional part of the model filled in.
fn rich_line() -> Line {
    let mut line =
        Line::load(&root().join("examples/synthetic-line/synthetic-line.ols.json")).unwrap();
    let s = &mut line.structures[1];
    s.structure_type = Some("tangent & <co>".into());
    s.extension = Some("+3".into());
    s.height_adjust = 0.375;
    s.description = "a \"tower\" & <more>".into();
    s.points[0].description = "left arm tip".into();
    s.wires[0].description = "the clamp".into();
    s.strings[0].description = "glass, 2019".into();
    s.omit = vec!["gone \"x\"".into()];
    s.omit_sets = vec![2];
    s.members = vec![vec![[0.5, 1.0, 500.0], [0.25, 1.0, 530.125]]];
    let a = &mut s.strings[0];
    a.label = "top \"A\" & <b>".into();
    a.insulator = Some("v-string".into());
    a.weight = 812.5;
    a.wind_area = 0.3;
    a.stiffness = Some(4e7);
    a.compression = Some(1500.0);
    a.strain = true;
    // A V with a string below: strings hanging from strings.
    let w = s.wires[0].clone();
    s.wires.push(ols_line::WirePoint {
        description: Default::default(),
        label: "tip\nnext".into(),
        position: [w.position[0], w.position[1], w.position[2] + 0.5],
        set: None,
        weight: 0.0,
    });
    s.wires[0].weight = 3500.0;
    s.points.push(ols_line::StructurePoint {
        description: Default::default(),
        label: "VL".into(),
        position: [w.position[0] - 2.0, w.position[1], w.position[2] + 3.0],
    });
    s.strings.push(ols_line::InsulatorString {
        label: "leg".into(),
        top: "VL".into(),
        wire: "tip\nnext".into(),
        weight: 100.0,
        ..Default::default()
    });
    let ball = line.sections[0].objects_mut().next().unwrap();
    ball.shape = ols_line::Shape::Areas {
        wind_area: 0.25,
        ice_area: 1.1,
    };
    ball.drag = Some(0.8);
    ball.type_id = Some("marker-ball-600".into());
    let sec = &mut line.sections[0];
    sec.stringing = Stringing::UnstressedLengths {
        lengths: vec![300.1, 299.987654321, 0.1 + 0.2, 1e-7],
        temperature: -5.0,
        sagged: Some(Box::new(Stringing::Sag {
            sag: 9.5,
            temperature: 15.0,
            span: Some(2),
        })),
    };
    sec.bundle = 2;
    sec.bundle_spacing = Some(0.4);
    sec.bundle_rotation_deg = Some(45.0);
    sec.color = Some("#123abc".into());
    sec.calculation = ols_line::condition::Calculation::CreepRs;
    line.jumpers[0].support = Some(ols_line::JumperSupport {
        point: [1.0, 2.0, 3.0],
        length: 2.5,
        weight: 100.0,
        fraction: 0.25,
    });
    line.jumpers[0].bundle = Some(1);
    line.jumpers[0].description = "loop at the dead end".into();
    line.sections[0].description = "phase L1 of circuit 1".into();
    line.systems[0].description = "the 220 kV system".into();
    let first = line.alignments[0].points[0].clone();
    line.alignment_points
        .iter_mut()
        .find(|p| p.id == first)
        .unwrap()
        .description = "start at the substation".into();
    // A structure giving its route, a route's description.
    line.structures[0].routes = vec![line.routes[0].id.clone()];
    line.routes[0].description = "the main route".into();
    line.jumpers[0].angle_from_deg = Some(-10.0);
    line.systems[0].kind = ols_line::electrical::SystemKind::Dc;
    line.systems[0].phases[0].polarity = Some(-1);
    line.display
        .phase_colors
        .insert("L1".into(), "#ff0000".into());
    line.creep_stretch.creep_case = Some("EDS".into());
    line.creep_stretch.load_cases = vec!["ice".into(), "wind ä".into()];
    line.bimetallic = ols_cables::BimetallicModel {
        behavior: ols_cables::BimetallicBehavior::Compression,
        max_virtual_compression: Some(ols_core::units::Stress::from_megapascals(50.0)),
    };
    line.solver.jumper_loads = ols_line::JumperLoads::Pull;
    line.defaults.spacer_compression = 8000.0;
    line.defaults.thermal_albedo = 0.3;
    line.defaults.clearance_search_margin = 7.0;
    line.sections[0].span_points[0].at = ols_line::SpanPlace::at_distance(30.0);
    for sec in &mut line.sections[..2] {
        sec.span_points.push(ols_line::SpanPoint {
            label: "P1".into(),
            span: 0,
            at: ols_line::SpanPlace::at_distance(40.0),
            description: "a third".into(),
            object: Default::default(),
        });
    }
    line.sections[0].span_points.push(ols_line::SpanPoint {
        label: "P2".into(),
        span: 0,
        at: ols_line::SpanPlace::at_station(123.5),
        description: String::new(),
        object: Default::default(),
    });
    line.sections[1].span_points.push(ols_line::SpanPoint {
        label: "C1".into(),
        span: 0,
        at: ols_line::SpanPlace::at_station(110.0),
        description: String::new(),
        object: Some(ols_line::LineObject {
            clamp_length: Some(0.25),
            ..ols_line::LineObject::clamp(30.0)
        }),
    });
    line.sections[1].span_points.push(ols_line::SpanPoint {
        label: "C2".into(),
        span: 0,
        at: ols_line::SpanPlace::at_fraction(0.7),
        description: String::new(),
        object: Some(ols_line::LineObject::body(
            150.0,
            ols_line::Shape::Cylinder {
                diameter: 0.25,
                length: 0.6,
                axis: ols_line::Axis::Vertical,
            },
        )),
    });
    line.jumpers[0].points.push(ols_line::JumperPoint {
        label: "J1".into(),
        at: ols_line::JumperPlace {
            fraction: 0.3,
            length: Some(2.0),
            distance: None,
        },
        description: "a ball".into(),
        object: Some(ols_line::LineObject::body(
            40.0,
            ols_line::Shape::Box {
                along: 0.3,
                across: 0.2,
                height: 0.4,
            },
        )),
    });
    // A jumper on a tap on S1's clamp C1, the loop turned off.
    let mut tap = line.jumpers[0].clone();
    tap.id = "JT".into();
    tap.supports.clear();
    tap.support = None;
    tap.sag = None;
    tap.length = Some(12.5);
    tap.points.clear();
    tap.from_tap = Some(ols_line::Tap::Span(ols_line::SpanPointRef {
        section: line.sections[1].id.clone(),
        point: "C1".into(),
    }));
    line.jumpers.push(tap);
    line.solver.taps = ols_line::TapLoads::Once;
    line.sections[0].add_spread_load(&ols_line::SpreadLoad {
        span: 0,
        start: ols_line::SpanPlace::at_fraction(0.1),
        end: ols_line::SpanPlace::at_distance(50.0),
        load: 12.5,
        label: "tray".into(),
    });
    line.spacers.push(ols_line::Spacer {
        id: "SP1".into(),
        description: "interphase".into(),
        a: ols_line::SpanPointRef {
            section: line.sections[0].id.clone(),
            point: "P1".into(),
        },
        b: ols_line::SpanPointRef {
            section: line.sections[1].id.clone(),
            point: "P1".into(),
        },
        insulator: None,
        length: Some(3.5),
        weight: 120.0,
        wind_area: 0.05,
        stiffness: Some(5e6),
        compression: Some(8000.0),
        fields: Default::default(),
    });
    let second = line.alignments[1].id.clone();
    let s2 = (
        line.structures[2].position,
        Some(line.structures[2].id.clone()),
    );
    line.set_alignment_points(&second, [s2, ([9.0, 9.0, 9.0], None)]);
    // An alignment of one point (shared with the first), one without.
    let mut alone = ols_line::Alignment::new("alone", &line.routes[1].id, vec![first]);
    alone.name = "one point".into();
    alone.description = "for later".into();
    line.alignments.push(alone);
    line.alignments.push(ols_line::Alignment::new(
        "empty",
        &line.routes[0].id,
        Vec::new(),
    ));
    // A point on no alignment yet.
    line.alignment_points.push(ols_line::AlignmentPoint {
        description: "waiting".into(),
        ..ols_line::AlignmentPoint::free("loose", [5.0, 6.0, 7.0])
    });
    // A circuit's chosen start, a rigid jumper and a jumper's EI factor.
    line.circuit_starts = vec![line.sections[0].points[0].clone()];
    if let Some(j) = line.jumpers.first_mut() {
        j.stiffness_factor = Some(0.3);
    }
    if let Some(j) = line.jumpers.get_mut(1) {
        j.rigid = true;
        j.length = None;
        j.sag = None;
    }
    // A jumper held by support clamps: one on the string to "tip\nnext" of
    // T2, one on a string of the `from` end's structure (no structure
    // given).
    let from = line.jumpers[0].from.structure.clone();
    let t = line.structures.iter_mut().find(|s| s.id == from).unwrap();
    let top = t.points[0].clone();
    t.wires.push(ols_line::WirePoint {
        description: Default::default(),
        label: "JW".into(),
        position: [
            top.position[0],
            top.position[1] + 1.0,
            top.position[2] - 1.5,
        ],
        set: None,
        weight: 250.0,
    });
    t.strings.push(ols_line::InsulatorString {
        label: "jumper string".into(),
        top: top.label,
        wire: "JW".into(),
        ..Default::default()
    });
    let mut held = line.jumpers[0].clone();
    held.id = "held".into();
    held.support = None;
    held.sag = None;
    held.length = Some(12.5);
    held.supports = vec![
        ols_line::JumperClamp {
            structure: Some(line.structures[1].id.clone()),
            wire: "tip\nnext".into(),
            at: 0.25,
            clamp_length: 0.08,
        },
        ols_line::JumperClamp::new("JW", 0.75),
    ];
    line.jumpers.push(held);
    // The strings added here: their lengths and directions from where
    // their wire points are.
    line.place_strings().unwrap();
    line
}

#[test]
fn ols_xml_round_trips_every_line() {
    let ex = root().join("examples/synthetic-line");
    let lines = [
        Line::load(&ex.join("synthetic-line.ols.json")).unwrap(),
        Line::load(&ex.join("synthetic-line-typed.ols.json")).unwrap(),
        Line::load(&ex.join("crossing-line.ols.json")).unwrap(),
        rich_line(),
    ];
    for line in lines {
        let xml = olsxml::to_xml(&line);
        let back = olsxml::from_xml(&xml).unwrap();
        assert_eq!(back, line);
        assert_eq!(back.to_json(), line.to_json());
        // Through the registry (by extension).
        let imported = Registry::with_builtin()
            .importer("xml")
            .unwrap()
            .import(xml.as_bytes(), None, &Options::new())
            .unwrap();
        assert_eq!(imported, line);
    }
    // The export writes the model's line.
    let m = model();
    let bytes = export("ols-xml", &m, &Options::new());
    assert_eq!(
        olsxml::from_xml(std::str::from_utf8(&bytes).unwrap()).unwrap(),
        m.line
    );
    assert!(
        String::from_utf8(bytes)
            .unwrap()
            .contains(olsxml::NAMESPACE)
    );
}

#[test]
fn ols_xml_refuses_what_it_does_not_know() {
    let ok = "<line name=\"x\"/>";
    assert_eq!(olsxml::from_xml(ok).unwrap().name, "x");
    for (bad, says) in [
        ("<line name=\"x\" colour=\"red\"/>", "unknown attribute"),
        ("<line name=\"x\"><tower/></line>", "unknown element"),
        ("<line/>", "\"name\" missing"),
        ("<lines name=\"x\"/>", "not an OLS XML line"),
        (
            "<line name=\"x\"><structure id=\"a\" x=\"1\" y=\"one\" z=\"0\"/></line>",
            "not a number",
        ),
    ] {
        let e = olsxml::from_xml(bad).unwrap_err();
        assert!(e.contains(says), "{bad}: {e}");
    }
}

/// The XSD names every element the writer uses.
#[test]
fn xsd_lists_every_element_written() {
    let xsd = std::fs::read_to_string(root().join("docs/schema/ols-line.xsd")).unwrap();
    let schema = xml::parse(&xsd).unwrap();
    let mut declared = Vec::new();
    schema.descendants("element", &mut declared);
    let names: Vec<&str> = declared.iter().filter_map(|e| e.attr("name")).collect();
    let doc = xml::parse(&olsxml::to_xml(&rich_line())).unwrap();
    fn walk<'a>(e: &'a xml::Element, out: &mut Vec<&'a str>) {
        out.push(&e.name);
        e.children.iter().for_each(|c| walk(c, out));
    }
    let mut used = Vec::new();
    walk(&doc, &mut used);
    for n in used {
        assert!(names.contains(&n), "<{n}> not in the schema");
    }
}

#[test]
fn landxml_has_alignments_and_points_and_reads_back() {
    let m = model();
    let bytes = export("landxml", &m, &Options::new());
    let doc = xml::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
    assert_eq!(doc.name, "LandXML");
    assert_eq!(doc.attr("version"), Some("1.2"));
    let mut points = Vec::new();
    doc.descendants("CgPoint", &mut points);
    assert_eq!(points.len(), m.line.structures.len());
    // Northing first.
    let p = m.line.structures[0].position;
    assert_eq!(
        points[0].text,
        format!("{:.4} {:.4} {:.4}", p[1], p[0], p[2])
    );
    let mut alignments = Vec::new();
    doc.descendants("Alignment", &mut alignments);
    assert_eq!(alignments.len(), m.line.alignments.len());
    let mut lines = Vec::new();
    alignments[0].descendants("Line", &mut lines);
    assert_eq!(lines.len(), m.line.alignments[0].points.len() - 1);
    let total: f64 = lines
        .iter()
        .map(|l| l.attr("length").unwrap().parse::<f64>().unwrap())
        .sum();
    let length: f64 = alignments[0].attr("length").unwrap().parse().unwrap();
    assert!((total - length).abs() < 1e-3);

    // Read back: the structures along the main alignment, in LV95.
    let line = Registry::with_builtin()
        .importer("xml")
        .unwrap()
        .import(&bytes, None, &Options::new())
        .unwrap();
    line.validate().unwrap();
    assert_eq!(line.crs, "EPSG:2056");
    assert_eq!(line.structures.len(), m.line.structures.len());
    for s in &line.structures {
        let o = m.line.structure(&s.id).unwrap();
        assert!((0..3).all(|k| (s.position[k] - o.position[k]).abs() < 1e-4));
    }
    let views = line.alignment_views();
    let main: Vec<&str> = views[0].structures.iter().map(String::as_str).collect();
    assert_eq!(&main[..2], ["T1", "T2"]);
    assert_eq!(line.alignments[0].points.len(), 3);

    // An alignment alone updates the route of a line.
    let mut moved = m.clone();
    let p1 = moved.line.alignments[0].points[1].clone();
    let p1 = moved
        .line
        .alignment_points
        .iter_mut()
        .find(|p| p.id == p1)
        .unwrap();
    p1.position[1] += 10.0;
    let full = landxml::document(&moved.line);
    let (head, rest) = full.split_once("<CgPoints").unwrap();
    let (_, tail) = rest.split_once("</CgPoints>").unwrap();
    let only = format!("{head}{tail}");
    let updated = Registry::with_builtin()
        .importer("ols-xml")
        .unwrap()
        .import(only.as_bytes(), Some(&m.line), &Options::new())
        .unwrap();
    assert_eq!(updated.structures, m.line.structures);
    let a = updated.points_of(&updated.alignments[0])[1].position;
    let b = m.line.points_of(&m.line.alignments[0])[1].position;
    assert!((a[1] - (b[1] + 10.0)).abs() < 1e-4);
}

#[test]
fn geojson_exports_layers_and_imports_structures() {
    let m = model();
    let doc: serde_json::Value =
        serde_json::from_slice(&export("geojson", &m, &Options::new())).unwrap();
    let features = doc["features"].as_array().unwrap();
    let wires = m.cases[0].wires.len();
    assert_eq!(features.len(), m.line.structures.len() + 2 + wires);
    // WGS84 by default (RFC 7946): no crs member.
    assert!(doc.get("crs").is_none());
    let c = &features[0]["geometry"]["coordinates"];
    assert!((c[0].as_f64().unwrap() - 7.438_632_42).abs() < 1e-8);
    assert!((c[1].as_f64().unwrap() - 46.951_082_77).abs() < 1e-8);
    let w = features
        .iter()
        .find(|f| f["properties"]["layer"] == "wires")
        .unwrap();
    assert_eq!(w["properties"]["case"], "15C");
    assert!(w["properties"]["sag_m"].as_f64().unwrap() > 0.0);
    assert!(w["properties"]["h_tens_n"].as_f64().unwrap() > 1000.0);
    assert_eq!(w["geometry"]["type"], "LineString");

    // In the line's coordinates, read back as a new line.
    let line_coords = export("geojson", &m, &opts(&[("coordinates", "line")]));
    let doc: serde_json::Value = serde_json::from_slice(&line_coords).unwrap();
    assert_eq!(doc["crs"]["properties"]["name"], "EPSG:2056");
    let line = Registry::with_builtin()
        .importer("geojson")
        .unwrap()
        .import(&line_coords, None, &opts(&[("type", "tangent")]))
        .unwrap();
    line.validate().unwrap();
    assert_eq!(line.crs, "EPSG:2056");
    assert_eq!(line.structures.len(), m.line.structures.len());
    let t5 = line.structure("T5").unwrap();
    assert!((t5.position[0] - m.line.structure("T5").unwrap().position[0]).abs() < 1e-3);
    // Types from the file are kept; the default fills the others (none here).
    assert!(line.structures.iter().all(|s| s.structure_type.is_some()));
    // The WGS84 file into the line: positions back to the millimetre.
    let wgs = export("geojson", &m, &Options::new());
    let same = Registry::with_builtin()
        .importer("structures-geojson")
        .unwrap()
        .import(&wgs, Some(&m.line), &Options::new())
        .unwrap();
    for (a, b) in same.structures.iter().zip(&m.line.structures) {
        assert!((0..3).all(|k| (a.position[k] - b.position[k]).abs() < 2e-3));
    }
    // WGS84 needs a real coordinate system.
    let mut local = m.clone();
    local.line.crs = "local (invented)".into();
    assert!(
        Registry::with_builtin()
            .exporter("geojson")
            .unwrap()
            .export(&local, &Options::new())
            .is_err()
    );
}

fn be(b: &[u8], at: usize) -> i32 {
    i32::from_be_bytes(b[at..at + 4].try_into().unwrap())
}

fn le(b: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

#[test]
fn shapefiles_read_back() {
    let m = model();
    let bytes = export("shapefile", &m, &Options::new());
    let files = unzip(&bytes);
    for layer in ["structures", "routes", "wires"] {
        for ext in ["shp", "shx", "dbf", "cpg"] {
            assert!(
                files.contains_key(&format!("{layer}.{ext}")),
                "{layer}.{ext}"
            );
        }
        assert!(!files.contains_key(&format!("{layer}.prj")));
        let shp = &files[&format!("{layer}.shp")];
        let shx = &files[&format!("{layer}.shx")];
        assert_eq!(be(shp, 0), 9994);
        assert_eq!(be(shp, 24) as usize * 2, shp.len());
        assert_eq!(be(shx, 24) as usize * 2, shx.len());
        assert_eq!(le(shp, 28), 1000);
        let expected = match layer {
            "structures" => m.line.structures.len(),
            "routes" => m.line.routes.len(),
            _ => m.cases[0].wires.len(),
        };
        assert_eq!((shx.len() - 100) / 8, expected, "{layer}");
        assert_eq!(le(shp, 32), if layer == "structures" { 11 } else { 13 });
        // Each index entry points at its record, numbered from 1.
        for k in 0..expected {
            let offset = be(shx, 100 + 8 * k) as usize * 2;
            assert_eq!(be(shp, offset), k as i32 + 1);
        }
        let (columns, rows) = shapefile::read_dbf(&files[&format!("{layer}.dbf")]).unwrap();
        assert_eq!(rows.len(), expected);
        if layer == "wires" {
            assert!(columns.iter().any(|c| c == "sag_m"));
            let sag: f64 = rows[0][10].parse().unwrap();
            assert!((sag - m.cases[0].wires[0].sag.unwrap()).abs() < 1e-5);
        }
    }
    let geoms = shapefile::read_shp(&files["structures.shp"]).unwrap();
    assert_eq!(
        geoms[0],
        gis::Geometry::Point(m.line.structures[0].position)
    );
    // The zip read back as structures along the main route.
    let line = Registry::with_builtin()
        .importer("shp")
        .unwrap()
        .import(&bytes, None, &Options::new())
        .unwrap();
    line.validate().unwrap();
    assert_eq!(line.structures.len(), m.line.structures.len());
    assert_eq!(line.structures[0].id, "T1");
    assert_eq!(line.alignments[0].points.len(), 3);
    // A bare .shp: points only, ids made up.
    let bare = Registry::with_builtin()
        .importer("structures-shp")
        .unwrap()
        .import(&files["structures.shp"], None, &opts(&[("prefix", "X")]))
        .unwrap();
    assert_eq!(bare.structures[0].id, "X1");
    // WGS84 output has a .prj.
    let wgs = unzip(&export("shapefile", &m, &opts(&[("coordinates", "wgs84")])));
    assert!(wgs.contains_key("wires.prj"));
}

#[test]
fn geopackage_reads_back_with_sqlite() {
    let m = model();
    let bytes = export("gpkg", &m, &Options::new());
    assert_eq!(&bytes[..16], b"SQLite format 3\0");
    let path = std::env::temp_dir().join(format!("ols-io-test-{}.gpkg", std::process::id()));
    std::fs::write(&path, &bytes).unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    let pragma = |p: &str| -> i64 {
        conn.query_row(&format!("PRAGMA {p}"), [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(pragma("application_id"), 0x4750_4B47);
    assert_eq!(pragma("user_version"), 10200);
    let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(
        count("SELECT COUNT(*) FROM gpkg_contents WHERE data_type = 'features' AND srs_id = 2056"),
        3
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM gpkg_spatial_ref_sys WHERE srs_id IN (-1, 0, 4326, 2056)"),
        4
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM gpkg_geometry_columns WHERE z = 1"),
        3
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM structures") as usize,
        m.line.structures.len()
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM wires") as usize,
        m.cases[0].wires.len()
    );
    let (id, blob): (String, Vec<u8>) = conn
        .query_row(
            "SELECT id, geom FROM structures ORDER BY fid LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(id, "T1");
    assert_eq!(&blob[..2], b"GP");
    assert_eq!(le(&blob, 4), 2056);
    // Point Z after the 8-byte header: byte order, type 1001, x, y, z.
    assert_eq!(blob[8], 1);
    assert_eq!(le(&blob, 9), 1001);
    let x = f64::from_le_bytes(blob[13..21].try_into().unwrap());
    assert_eq!(x, m.line.structures[0].position[0]);
    let (min_x, max_y): (f64, f64) = conn
        .query_row(
            "SELECT min_x, max_y FROM gpkg_contents WHERE table_name = 'structures'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(min_x <= x && max_y >= m.line.structures[0].position[1]);
    let sag: f64 = conn
        .query_row("SELECT sag_m FROM wires ORDER BY fid LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(sag, m.cases[0].wires[0].sag.unwrap());
    drop(conn);
    let _ = std::fs::remove_file(&path);
}

/// Top-level attributes of a STEP instance's argument list.
fn step_args(args: &str) -> Vec<String> {
    let (mut depth, mut quoted) = (0, false);
    let mut out = vec![String::new()];
    for c in args.chars() {
        match c {
            '\'' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth -= 1,
            ',' if !quoted && depth == 0 => {
                out.push(String::new());
                continue;
            }
            _ => {}
        }
        out.last_mut().unwrap().push(c);
    }
    out
}

#[test]
fn ifc_is_a_well_formed_step_file() {
    let m = model();
    let text = String::from_utf8(export("ifc", &m, &Options::new())).unwrap();
    assert!(text.starts_with("ISO-10303-21;\nHEADER;"));
    assert!(text.contains("FILE_SCHEMA(('IFC4'));"));
    assert!(text.trim_end().ends_with("END-ISO-10303-21;"));
    let data = text.split("DATA;\n").nth(1).unwrap();
    let data = data.split("ENDSEC;").next().unwrap();
    let mut entities: BTreeMap<usize, (String, Vec<String>)> = BTreeMap::new();
    for (k, l) in data.lines().enumerate() {
        let (id, rest) = l.split_once('=').unwrap();
        let id: usize = id.strip_prefix('#').unwrap().parse().unwrap();
        assert_eq!(id, k + 1);
        let rest = rest.strip_suffix(");").unwrap();
        let (name, args) = rest.split_once('(').unwrap();
        entities.insert(id, (name.to_owned(), step_args(args)));
    }
    // Every reference points at an entity.
    for (_, args) in entities.values() {
        for a in args {
            for r in a.split('#').skip(1) {
                let n: usize = r
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse()
                    .unwrap();
                assert!(entities.contains_key(&n), "#{n} missing");
            }
        }
    }
    let count = |name: &str| entities.values().filter(|(n, _)| n == name).count();
    let attrs = |name: &str| {
        entities
            .values()
            .find(|(n, _)| n == name)
            .map(|(_, a)| a.len())
            .unwrap()
    };
    // Attribute counts of IFC4.
    for (name, n) in [
        ("IFCPROJECT", 9),
        ("IFCSITE", 14),
        ("IFCBUILDINGELEMENTPROXY", 9),
        ("IFCCABLESEGMENT", 9),
        ("IFCRELAGGREGATES", 6),
        ("IFCRELCONTAINEDINSPATIALSTRUCTURE", 6),
        ("IFCRELDEFINESBYPROPERTIES", 6),
        ("IFCPROPERTYSET", 5),
        ("IFCPROPERTYSINGLEVALUE", 4),
        ("IFCSHAPEREPRESENTATION", 4),
        ("IFCGEOMETRICREPRESENTATIONCONTEXT", 6),
        ("IFCGEOMETRICREPRESENTATIONSUBCONTEXT", 10),
        ("IFCLOCALPLACEMENT", 2),
        ("IFCAXIS2PLACEMENT3D", 3),
        ("IFCSIUNIT", 4),
    ] {
        assert_eq!(attrs(name), n, "{name}");
    }
    assert_eq!(count("IFCPROJECT"), 1);
    assert_eq!(count("IFCBUILDINGELEMENTPROXY"), m.line.structures.len());
    assert_eq!(count("IFCCABLESEGMENT"), m.cases[0].wires.len());
    assert_eq!(
        count("IFCRELDEFINESBYPROPERTIES"),
        m.line.structures.len() + m.cases[0].wires.len()
    );
    // GlobalIds are unique, 22 characters.
    let mut ids: Vec<&String> = entities
        .values()
        .filter(|(n, _)| n.starts_with("IFCREL") || n == "IFCPROJECT" || n == "IFCSITE")
        .map(|(_, a)| &a[0])
        .collect();
    assert!(ids.iter().all(|g| g.len() == 24));
    let n = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), n);
    assert!(text.contains("'Sag',$,IFCLENGTHMEASURE("));
}

#[test]
fn xlsx_has_a_sheet_per_table() {
    let mut m = model();
    m.tables.push(tables::Table::from_csv(
        "Structure loads",
        "case,structure,transverse_n\nwind,T1,1200.5\nwind,\"T, 2\",\n",
    ));
    let files = unzip(&export("xlsx", &m, &Options::new()));
    for part in [
        "[Content_Types].xml",
        "_rels/.rels",
        "xl/workbook.xml",
        "xl/_rels/workbook.xml.rels",
        "xl/styles.xml",
    ] {
        xml::parse(std::str::from_utf8(&files[part]).unwrap()).unwrap();
    }
    let book = xml::parse(std::str::from_utf8(&files["xl/workbook.xml"]).unwrap()).unwrap();
    let mut sheets = Vec::new();
    book.descendants("sheet", &mut sheets);
    let names: Vec<&str> = sheets.iter().map(|s| s.attr("name").unwrap()).collect();
    assert_eq!(
        names,
        [
            "Structures",
            "Sections",
            "Spans",
            "Stations",
            "Structure loads"
        ]
    );
    let sheet = |k: usize| {
        xml::parse(std::str::from_utf8(&files[&format!("xl/worksheets/sheet{k}.xml")]).unwrap())
            .unwrap()
    };
    let (s1, s3, s5) = (sheet(1), sheet(3), sheet(5));
    let mut rows = Vec::new();
    s1.descendants("row", &mut rows);
    assert_eq!(rows.len(), m.line.structures.len() + 1);
    let mut cells = Vec::new();
    rows[1].descendants("c", &mut cells);
    assert_eq!(cells[0].attr("r"), Some("A2"));
    assert_eq!(cells[0].child("is").unwrap().child("t").unwrap().text, "T1");
    assert_eq!(
        cells[1].child("v").unwrap().text,
        format!("{}", m.line.structures[0].position[0])
    );
    let mut spans = Vec::new();
    s3.descendants("row", &mut spans);
    assert_eq!(spans.len(), m.cases[0].wires.len() + 1);
    let mut loads = Vec::new();
    s5.descendants("c", &mut loads);
    assert!(
        loads
            .iter()
            .any(|c| c.child("v").is_some_and(|v| v.text == "1200.5"))
    );
    assert!(loads.iter().any(|c| {
        c.child("is")
            .is_some_and(|i| i.child("t").unwrap().text == "T, 2")
    }));
}

#[test]
fn spans_csv_has_every_case() {
    let m = model();
    let csv = String::from_utf8(export("spans-csv", &m, &Options::new())).unwrap();
    assert_eq!(csv.lines().count(), m.cases[0].wires.len() + 1);
    assert!(csv.starts_with("case,section,span,back,ahead,"));
    assert!(csv.contains("\n15C,S1,1,T1,T2,"));
}

/// A neutral file into a project: a new project gets the line, a project
/// holding one gets it updated; one log entry each.
#[test]
fn neutral_files_go_into_a_project_as_one_command() {
    let dir = std::env::temp_dir().join(format!("ols-io-neutral-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let points = dir.join("points.geojson");
    let doc = serde_json::json!({
        "type": "FeatureCollection",
        "crs": {"type": "name", "properties": {"name": "urn:ogc:def:crs:EPSG::2056"}},
        "features": [
            {"type": "Feature", "properties": {"name": "B"},
             "geometry": {"type": "Point", "coordinates": [2600300.0, 1200000.0, 510.0]}},
            {"type": "Feature", "properties": {"name": "A"},
             "geometry": {"type": "Point", "coordinates": [2600000.0, 1200000.0, 500.0]}},
            {"type": "Feature", "properties": {},
             "geometry": {"type": "LineString", "coordinates": [[2599990.0, 1200000.0], [2600400.0, 1200000.0]]}}
        ]
    });
    std::fs::write(&points, doc.to_string()).unwrap();
    let project = dir.join("p.ols");
    let mut r = Registry::with_builtin();
    let o = opts(&[("type", "tangent")]);
    let (applied, line) =
        neutral::import_into_project(&mut r, &points, None, &o, &project, "ann").unwrap();
    assert_eq!(line.alignment_views()[0].structures, ["A", "B"]);
    assert_eq!(line.crs, "EPSG:2056");
    assert!(applied.seq > 0);
    // Moved 5 m east: the project's line is updated.
    let moved = doc.to_string().replace("2600300.0", "2600305.0");
    std::fs::write(&points, moved).unwrap();
    neutral::import_into_project(&mut r, &points, Some("geojson"), &o, &project, "ann").unwrap();
    let s = ols_session::Session::open(&project).unwrap();
    assert_eq!(s.export().structure("B").unwrap().position[0], 2_600_305.0);
    let log = s.log().unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].kind, "import");
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}

/// CSV points with other column names: structures of a default type along
/// a route in file order, turned to the line.
#[test]
fn csv_points_with_mapped_columns() {
    let csv = "Nr;Easting;Northing;H\n;0;0;500\n;100;0;501\n;200;100;502\n";
    let o = opts(&[
        ("delimiter", ";"),
        ("x_column", "Easting"),
        ("y_column", "Northing"),
        ("z_column", "H"),
        ("type", "tangent"),
        ("prefix", "M"),
    ]);
    let line = Registry::with_builtin()
        .importer("csv")
        .unwrap()
        .import(csv.as_bytes(), None, &o)
        .unwrap();
    line.validate().unwrap();
    let ids: Vec<&str> = line.structures.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["M1", "M2", "M3"]);
    assert_eq!(line.alignment_views()[0].structures, ids);
    assert_eq!(line.structures[2].position, [200.0, 100.0, 502.0]);
    assert!(
        line.structures
            .iter()
            .all(|s| s.structure_type.as_deref() == Some("tangent"))
    );
    // At M2 the line turns 45° left: the cross-arm on the bisector.
    assert!((line.structures[1].orientation_deg - (22.5 - 90.0)).abs() < 1e-9);
    let e = Registry::with_builtin()
        .importer("csv")
        .unwrap()
        .import(csv.as_bytes(), None, &opts(&[("x_column", "East")]))
        .unwrap_err();
    assert!(e.contains("x_column"), "{e}");
}

/// Another program's XML report (here a PLS-CADD one, in Windows-1252) is
/// read in its encoding and refused with where to open it instead.
#[test]
fn other_programs_xml_reports_say_where_they_go() {
    let report = b"<?xml version='1.0' encoding='windows-1252' standalone='yes'?>\r\n\
        <!-- Created by the program (L\xe9on) -->\r\n\
        <root><creator application='PLS-CADD' version='20'/><project_notes>Gr\xfcn</project_notes></root>";
    let e = Registry::with_builtin()
        .importer("xml")
        .unwrap()
        .import(report, None, &Options::new())
        .unwrap_err();
    assert!(
        e.contains("PLS-CADD") && e.contains("New project from another program"),
        "{e}"
    );
}
