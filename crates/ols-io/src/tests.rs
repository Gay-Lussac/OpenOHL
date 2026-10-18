// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::io::Read as _;
use std::path::Path;

use ols_core::units::Temperature;
use ols_line::jumper::solve_jumpers;
use ols_line::solve::solve_line;
use ols_weather::WeatherCase;

use super::*;
use crate::formats::{StructuresCsv, WiresCsv};

pub(crate) fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// The synthetic example line, moved to LV95 around (2 600 000,
/// 1 200 000), solved at 15 °C.
pub(crate) fn model() -> ExportModel {
    let mut line =
        Line::load(&root().join("examples/synthetic-line/synthetic-line.ols.json")).unwrap();
    line.transform_coordinates(|pts| {
        for p in pts {
            p[0] += 2_600_000.0;
            p[1] += 1_200_000.0;
        }
        Ok(())
    })
    .unwrap();
    line.crs = "EPSG:2056".into();
    let lib = ols_cables::Library::load(&root().join("libraries/cables")).unwrap();
    let case = WeatherCase::still("15C", Temperature::from_celsius(15.0));
    let shapes: Vec<_> = solve_line(&line, &lib, &case)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let jumpers: Vec<_> = solve_jumpers(&line, &lib, &case, &shapes)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    ExportModel::new(
        &line,
        &[(case.name.clone(), case.summary(), shapes, jumpers)],
    )
}

#[test]
fn registry_lists_and_finds_formats() {
    let mut r = Registry::with_builtin();
    let ex: Vec<&str> = r.exporters().map(|e| e.id()).collect();
    assert_eq!(
        ex,
        [
            "structures-csv",
            "wires-csv",
            "kml",
            "kmz",
            "spans-csv",
            "ols-xml",
            "landxml",
            "geojson",
            "shapefile",
            "gpkg",
            "ifc",
            "xlsx"
        ]
    );
    let im: Vec<&str> = r.importers().map(|i| i.id()).collect();
    assert_eq!(
        im,
        [
            "structures-csv",
            "ols-xml",
            "structures-geojson",
            "structures-shp"
        ]
    );
    assert_eq!(r.exporter("xml").unwrap().id(), "ols-xml");
    assert_eq!(r.importer("shp").unwrap().id(), "structures-shp");
    assert_eq!(r.exporter("KMZ").unwrap().id(), "kmz");
    // By extension: the first exporter for it.
    assert_eq!(r.exporter("csv").unwrap().id(), "structures-csv");
    assert!(r.exporter("dxf").is_none());
}

#[test]
fn model_json_has_every_wire_and_the_line() {
    let m = model();
    let v: serde_json::Value = serde_json::from_str(&m.to_json()).unwrap();
    assert_eq!(v["format"], "ols-export-model");
    assert_eq!(v["version"], 1);
    assert_eq!(v["crs"], "EPSG:2056");
    let spans: usize = m.line.sections.iter().map(|s| s.points.len() - 1).sum();
    let wires = v["cases"][0]["wires"].as_array().unwrap();
    assert_eq!(wires.len(), spans + m.line.jumpers.len());
    assert!(
        wires
            .iter()
            .any(|w| w["section"].as_str().unwrap().starts_with("jumper:"))
    );
    assert!(
        wires
            .iter()
            .all(|w| w["color"].as_str().unwrap().starts_with('#'))
    );
    // The line inside is a valid line file.
    let back = Line::from_json(&v["line"].to_string()).unwrap();
    assert_eq!(back, m.line);
}

/// The export model gives plugins each string as solved with its limits
/// (OpenOHL checks none): given ones in the case's strings and in the
/// line, absent ones null in the strings and absent in the line.
#[test]
fn model_json_has_the_strings_and_their_limits() {
    let mut line =
        Line::load(&root().join("examples/synthetic-line/synthetic-line.ols.json")).unwrap();
    let t2 = line.structures.iter_mut().find(|s| s.id == "T2").unwrap();
    let label = t2.strings[0].label.clone();
    t2.strings[0].max_load = Some(120_000.0);
    t2.strings[0].max_swing_across_deg = Some(55.0);
    line.validate().unwrap();
    let lib = ols_cables::Library::load(&root().join("libraries/cables")).unwrap();
    let case = WeatherCase::still("15C", Temperature::from_celsius(15.0));
    let shapes: Vec<_> = solve_line(&line, &lib, &case)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let jumpers: Vec<_> = solve_jumpers(&line, &lib, &case, &shapes)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let m = ExportModel::new(
        &line,
        &[(case.name.clone(), case.summary(), shapes, jumpers)],
    );
    let v: serde_json::Value = serde_json::from_str(&m.to_json()).unwrap();
    let strings = v["cases"][0]["strings"].as_array().unwrap();
    assert!(!strings.is_empty());
    // Each string once.
    let mut keys: Vec<(String, String)> = strings
        .iter()
        .map(|x| (x["structure"].to_string(), x["label"].to_string()))
        .collect();
    let n = keys.len();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), n);
    let x = strings
        .iter()
        .find(|x| x["structure"] == "T2" && x["label"] == label.as_str())
        .expect("T2's string");
    assert_eq!(x["max_load_n"], 120_000.0);
    assert_eq!(x["max_swing_across_deg"], 55.0);
    assert!(x["max_swing_deg"].is_null() && x["max_swing_along_deg"].is_null());
    assert!(x["force_n"].as_f64().unwrap() > 0.0);
    assert!(x["swing_deg"].as_f64().unwrap() >= 0.0);
    assert!(x["swing_across_deg"].is_f64() && x["swing_along_deg"].is_f64());
    assert!(
        strings
            .iter()
            .filter(|y| *y != x)
            .all(|y| y["max_load_n"].is_null())
    );
    // The line: the given limits, nothing for the others.
    let t2 = v["line"]["structures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "T2")
        .unwrap();
    assert_eq!(t2["strings"][0]["max_load"], 120_000.0);
    assert_eq!(t2["strings"][0]["max_swing_across_deg"], 55.0);
    assert!(t2["strings"][0].get("max_swing_deg").is_none());
    assert_eq!(v["line"].to_string().matches("max_load").count(), 1);
}

#[test]
fn kml_is_in_wgs84_and_kmz_holds_it() {
    let m = model();
    let mut r = Registry::with_builtin();
    let kml = String::from_utf8(
        r.exporter("kml")
            .unwrap()
            .export(&m, &Options::new())
            .unwrap(),
    )
    .unwrap();
    // The first structure is at LV95 (2 600 000, 1 200 000, 500): swisstopo
    // REFRAME gives 7.438632420871814 E, 46.9510827728495 N.
    assert!(
        kml.contains("<coordinates>7.43863242,46.95108277,500.000</coordinates>"),
        "{}",
        &kml[..kml.len().min(1500)]
    );
    let lines = kml.matches("<LineString>").count();
    assert_eq!(lines, m.cases[0].wires.len());
    // Colours as aabbggrr.
    let c = &m.cases[0].wires[0].color;
    assert!(kml.contains(
        &format!("<color>ff{}{}{}</color>", &c[5..7], &c[3..5], &c[1..3]).to_ascii_lowercase()
    ));

    let kmz = r
        .exporter("kmz")
        .unwrap()
        .export(&m, &Options::new())
        .unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(kmz)).unwrap();
    let mut doc = String::new();
    zip.by_name("doc.kml")
        .unwrap()
        .read_to_string(&mut doc)
        .unwrap();
    assert_eq!(doc, kml);
}

#[test]
fn kml_needs_a_coordinate_system() {
    let mut m = model();
    m.line.crs = "local (invented)".into();
    let mut r = Registry::with_builtin();
    let k = r.exporter("kml").unwrap();
    assert!(k.export(&m, &Options::new()).is_err());
    m.line.crs = String::new();
    assert!(k.export(&m, &Options::new()).unwrap_err().contains("crs"));
    let opts = Options::from([("crs".to_owned(), "EPSG:2056".to_owned())]);
    assert!(k.export(&m, &opts).is_ok());
}

#[test]
fn wires_csv_has_a_row_per_point() {
    let m = model();
    let csv = String::from_utf8(WiresCsv.export(&m, &Options::new()).unwrap()).unwrap();
    let points: usize = m.cases[0].wires.iter().map(|w| w.points.len()).sum();
    assert_eq!(csv.lines().count(), points + 1);
    assert!(csv.starts_with("case,section,span,circuit,phase,index,x,y,z\n15C,"));
}

#[test]
fn structures_csv_round_trips_and_moves_attachments() {
    let m = model();
    let csv = StructuresCsv.export(&m, &Options::new()).unwrap();
    // Re-importing the export into the line changes nothing (to the 0.1 mm
    // of the file).
    let same = StructuresCsv
        .import(&csv, Some(&m.line), &Options::new())
        .unwrap();
    for (a, b) in same.structures.iter().zip(&m.line.structures) {
        assert!((0..3).all(|k| (a.position[k] - b.position[k]).abs() < 1e-4));
        assert!((0..3).all(|k| { (a.wires[0].position[k] - b.wires[0].position[k]).abs() < 1e-4 }));
    }
    // Move the first structure 2 m east and turn it by 90°.
    let first = &m.line.structures[0];
    let text = format!(
        "structure,x,y,z,orientation_deg\n\"{}\",{},{},{},{}\nNEW,1,2,3,\n",
        first.id,
        first.position[0] + 2.0,
        first.position[1],
        first.position[2],
        first.orientation_deg + 90.0
    );
    let moved = StructuresCsv
        .import(text.as_bytes(), Some(&m.line), &Options::new())
        .unwrap();
    let s = &moved.structures[0];
    let (a0, a1) = (first.wires[0].position, s.wires[0].position);
    let (dx, dy) = (a0[0] - first.position[0], a0[1] - first.position[1]);
    assert!(
        (a1[0] - (s.position[0] - dy)).abs() < 1e-9 && (a1[1] - (s.position[1] + dx)).abs() < 1e-9
    );
    assert_eq!(a1[2], a0[2]);
    assert_eq!(moved.structures.len(), m.line.structures.len() + 1);
    assert_eq!(moved.structures.last().unwrap().position, [1.0, 2.0, 3.0]);
    // Errors name the line.
    let bad = StructuresCsv.import(b"structure,x,y,z\nT1,1,two,3\n", None, &Options::new());
    assert!(bad.unwrap_err().contains("line 2"));
    assert!(
        StructuresCsv
            .import(b"id,x\n", None, &Options::new())
            .is_err()
    );
}

/// The example GeoJSON plugin, when built (CI builds it; see
/// CONTRIBUTING.md): its formats join the registry and run like built-in
/// ones.
#[test]
fn plugin_formats_export_and_import() {
    let dir = root().join("plugins/examples/geojson");
    if !dir
        .join("target/wasm32-wasip2/release/geojson.wasm")
        .exists()
    {
        assert!(
            std::env::var_os("OLS_REQUIRE_PLUGIN_TESTS").is_none(),
            "plugin not built: {}",
            dir.display()
        );
        eprintln!("skipping: geojson plugin not built");
        return;
    }
    let host = ols_plugins::WasmHost::new().unwrap();
    let mut r = Registry::with_builtin();
    assert_eq!(r.add_plugin(&host, &dir).unwrap(), "geojson");
    let m = model();
    let exporter = r.exporter("geojson").unwrap();
    assert_eq!(exporter.origin(), "plugin geojson");
    let bytes = exporter.export(&m, &Options::new()).unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let features = doc["features"].as_array().unwrap();
    assert_eq!(
        features.len(),
        m.line.structures.len() + m.cases[0].wires.len()
    );
    assert_eq!(doc["crs"]["properties"]["name"], "EPSG:2056");

    // Import the structures back into the line, one moved 5 m east.
    let mut points = doc.clone();
    points["features"] = features
        .iter()
        .filter(|f| f["geometry"]["type"] == "Point")
        .cloned()
        .collect();
    let x = points["features"][1]["geometry"]["coordinates"][0]
        .as_f64()
        .unwrap();
    points["features"][1]["geometry"]["coordinates"][0] = (x + 5.0).into();
    let data = serde_json::to_vec(&points).unwrap();
    let line = r
        .importer("geojson-structures")
        .unwrap()
        .import(&data, Some(&m.line), &Options::new())
        .unwrap();
    assert_eq!(line.structures.len(), m.line.structures.len());
    assert_eq!(
        line.structures[1].position[0],
        m.line.structures[1].position[0] + 5.0
    );
    assert_eq!(line.sections, m.line.sections);
    // The plugin's own errors come back as the format's error.
    let err = r
        .importer("geojson-structures")
        .unwrap()
        .import(b"{\"features\": []}", None, &Options::new())
        .unwrap_err();
    assert!(err.contains("plugin geojson: no Point features"), "{err}");
}

/// Built-in surface formats: LandXML (several surfaces), points
/// triangulated with a longest side; by id or extension.
#[test]
fn surface_formats_read_meshes() {
    let mut r = Registry::with_builtin();
    let t = ols_terrain::Terrain::from_mesh(
        vec![[0.0, 0.0, 1.0], [10.0, 0.0, 2.0], [0.0, 10.0, 3.0]],
        vec![[0, 1, 2]],
    )
    .unwrap();
    let xml = ols_terrain::formats::write_landxml(&[("a", &t), ("b", &t)]);
    let s = r
        .surface_importer("xml")
        .unwrap()
        .import(xml.as_bytes(), "f", &Options::new())
        .unwrap();
    assert_eq!(s.len(), 2);
    assert_eq!(s[1].name, "b");
    let csv = "x,y,z\n0,0,1\n10,0,2\n0,10,3\n10,10,4\n100,100,9\n";
    let mut options = Options::new();
    let all = r
        .surface_importer("points-csv")
        .unwrap()
        .import(csv.as_bytes(), "pts", &options)
        .unwrap();
    options.insert("max_edge".into(), "20".into());
    let near = r
        .surface_importer("csv")
        .unwrap()
        .import(csv.as_bytes(), "pts", &options)
        .unwrap();
    assert_eq!(near[0].name, "pts");
    assert!(near[0].triangles.len() < all[0].triangles.len());
    assert_eq!(r.surface_importers().count(), 4);
}

#[test]
fn options_are_parsed_or_refused() {
    let o = parse_options(" crs = EPSG:2056,\nmax_edge=50\n\n").unwrap();
    assert_eq!(o["crs"], "EPSG:2056");
    assert_eq!(o["max_edge"], "50");
    assert!(parse_options("").unwrap().is_empty());
    assert_eq!(
        parse_options("a=1,oops").unwrap_err(),
        "option \"oops\": use KEY=VALUE"
    );
    assert!(parse_option("=1").is_err());
    assert_eq!(parse_option("crs=a,b").unwrap().1, "a,b");
    assert_eq!(format_of(Path::new("g.XML"), None), "xml");
    assert_eq!(format_of(Path::new("g.XML"), Some(" obj ")), "obj");
}

/// Surfaces stored under free names: within one file and beside those
/// the project holds (renamed, replaced or refused).
#[test]
fn surfaces_are_stored_under_free_names() {
    use crate::surfaces::{WhenTaken, free_name, import_into};
    assert_eq!(free_name("g", &["g".into(), "g (2)".into()]), "g (3)");
    let dir = std::env::temp_dir().join(format!("ols-io-surfaces-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let t = ols_terrain::Terrain::from_mesh(
        vec![[0.0, 0.0, 1.0], [10.0, 0.0, 2.0], [0.0, 10.0, 3.0]],
        vec![[0, 1, 2]],
    )
    .unwrap();
    let file = dir.join("ground.xml");
    std::fs::write(
        &file,
        ols_terrain::formats::write_landxml(&[("a", &t), ("b", &t)]),
    )
    .unwrap();
    let s = ols_session::Session::open(&dir.join("p.ols")).unwrap();
    s.set_user("ann", "ann", ols_session::Role::Admin).unwrap();
    let mut r = Registry::with_builtin();
    let o = Options::new();
    let mut store = |name: Option<&str>, when: WhenTaken| {
        import_into(&mut r, &file, None, &o, &s, "ann", name, when)
            .map(|v| v.into_iter().map(|s| s.name).collect::<Vec<_>>().join("|"))
    };
    assert_eq!(store(None, WhenTaken::Refuse).unwrap(), "a|b");
    assert_eq!(store(Some("g"), WhenTaken::Rename).unwrap(), "g|g (2)");
    assert_eq!(store(Some("g"), WhenTaken::Rename).unwrap(), "g (3)|g (4)");
    assert!(
        store(None, WhenTaken::Refuse)
            .unwrap_err()
            .contains("\"a\"")
    );
    assert_eq!(store(None, WhenTaken::Replace).unwrap(), "a|b");
    assert_eq!(s.surfaces().unwrap().len(), 6);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The example terrain plugin (ESRI ASCII grid) once built: its format
/// next to the built-in ones; a point per cell, no data left out.
#[test]
fn terrain_plugin_reads_a_grid() {
    let dir = root().join("plugins/examples/esri-grid");
    if !dir
        .join("target/wasm32-wasip2/release/esri_grid.wasm")
        .exists()
    {
        assert!(
            std::env::var_os("OLS_REQUIRE_PLUGIN_TESTS").is_none(),
            "esri-grid plugin not built"
        );
        eprintln!("skipping: esri-grid plugin not built");
        return;
    }
    let host = ols_plugins::WasmHost::new().unwrap();
    let mut r = Registry::with_builtin();
    assert_eq!(r.add_plugin(&host, &dir).unwrap(), "esri-grid");
    let asc = "ncols 3\nnrows 2\nxllcorner 1000\nyllcorner 2000\ncellsize 10\nNODATA_value -9999\n\
               5 6 7\n1 2 -9999\n";
    let i = r.surface_importer("asc").unwrap();
    assert_eq!(i.origin(), "plugin esri-grid");
    let s = i.import(asc.as_bytes(), "dem", &Options::new()).unwrap();
    assert_eq!(s[0].points.len(), 5);
    // Lower-left cell centre, from the southern row.
    assert_eq!(s[0].points[3], [1005.0, 2005.0, 1.0]);
    assert_eq!(s[0].triangles.len(), 2);
    assert!(
        r.surface_importer("asc")
            .unwrap()
            .import(b"ncols 3\n", "x", &Options::new())
            .unwrap_err()
            .contains("plugin esri-grid")
    );
}

/// The example project plugin once built: a bundle becomes a line and its
/// library files; clipping in fixes each span's length.
#[test]
fn project_plugin_reads_a_bundle() {
    let dir = root().join("plugins/examples/project-bundle");
    if !dir
        .join("target/wasm32-wasip2/release/project_bundle.wasm")
        .exists()
    {
        assert!(
            std::env::var_os("OLS_REQUIRE_PLUGIN_TESTS").is_none(),
            "project-bundle plugin not built"
        );
        eprintln!("skipping: project-bundle plugin not built");
        return;
    }
    let ex = root().join("examples/synthetic-line");
    let line: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(ex.join("synthetic-line.ols.json")).unwrap())
            .unwrap();
    let cables =
        std::fs::read_to_string(root().join("libraries/cables/acsr-examples.olsw")).unwrap();
    let bundle = serde_json::json!({
        "line": line,
        "libraries": [{"kind": "cables", "name": "acsr-examples.olsw", "text": cables}],
    });
    let host = ols_plugins::WasmHost::new().unwrap();
    let mut r = Registry::with_builtin();
    assert_eq!(r.add_plugin(&host, &dir).unwrap(), "project-bundle");
    let i = r.project_importer("olsb").unwrap();
    assert_eq!(i.origin(), "plugin project-bundle");
    let mut options = Options::new();
    options.insert("name".into(), "Bundled".into());
    let mut p = i.import(bundle.to_string().as_bytes(), &options).unwrap();
    assert_eq!(p.line.name, "Bundled");
    assert_eq!(p.libraries.len(), 1);
    assert_eq!(p.notes, ["1 library files"]);
    let before = p
        .line
        .sections
        .iter()
        .filter(|s| s.stringing.clipped())
        .count();
    let (n, failed) = p.clip_in();
    assert!(failed.is_empty(), "{failed:?}");
    assert_eq!(n, p.line.sections.len() - before);
    assert!(p.line.sections.iter().all(|s| s.stringing.clipped()));
    assert!(
        r.project_importer("olsb")
            .unwrap()
            .import(b"{}", &Options::new())
            .unwrap_err()
            .contains("no line")
    );
}
