// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use ohl_sections::{SectionLibrary, Shape};
use ohl_structure_loads::{At, LoadRow, Source};

use super::*;
use crate::ifc::{IfcOptions, Schema};
use crate::xlsx::Cell;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn type_02() -> String {
    std::fs::read_to_string(root().join("examples/synthetic-line/Structures/type-02.ohls")).unwrap()
}

fn example_library() -> SectionLibrary {
    SectionLibrary::load(&root().join("libraries/sections/example-sections.ohlsec")).unwrap()
}

/// A small invented frame using every shape of the example library.
const SHAPES: &str = r#"
format = "ohl-structure"
version = 2
id = "shapes"
name = "Every profile (invented)"
kind = "other"

[[point]]
label = "P1"
member = "m-EA"
at = 0.25

[[node]]
id = "N0"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N1"
point = ["0 m", "0 m", "5 m"]

[[node]]
id = "N2"
point = ["4 m", "0 m", "5 m"]
support = ["x", "y", "z"]
spring = [0, 0, 0, 0, 0, 2000]

[[node]]
id = "N3"
point = ["4 m", "3 m", "5 m"]

[[member]]
id = "m-EA"
nodes = ["N0", "N1"]
section = "EA-61x7"
kind = "frame"

[[member]]
id = "m-UA"
nodes = ["N1", "N2"]
section = "UA-83x52x6"
kind = "frame"
roll = "30 deg"

[[member]]
id = "m-CH"
nodes = ["N2", "N3"]
section = "CH-157"
kind = "frame"

[[member]]
id = "m-IS"
nodes = ["N0", "N2"]
section = "IS-213"
kind = "frame"
reference = [0, 1, 0]

[[member]]
id = "m-RHS"
nodes = ["N1", "N3"]
section = "RHS-121x79x5"

[[member]]
id = "m-CHS"
nodes = ["N0", "N3"]
section = "CHS-167x7"

[[member]]
id = "m-RB"
nodes = ["N3", "N1"]
section = "RB-23"
kind = "cable"
pretension = "1 kN"

[[member]]
id = "m-POLE"
nodes = ["N2", "N1"]
section = "POLE-12-430"
kind = "frame"

[[member]]
id = "m-GEN"
nodes = ["N0", "N3"]
section = "GEN-1"

[[load]]
node = "N3"
force = ["1 kN", "2 kN", "-3 kN"]

[[load_case]]
name = "along"
self_weight = 1.35

[[load_case.load]]
value = "4 kN"
direction = "x"
points = ["P1"]
"#;

fn shapes() -> ExportStructure {
    let library = example_library();
    let (mut table, recipes) = own_load_table(SHAPES, &library).unwrap();
    // A surface load's total on a member (as an evaluated row with areas).
    table.rows.push(LoadRow {
        case: "along".into(),
        load: None,
        at: At::Member,
        label: "m-IS".into(),
        force: [0.0, 0.0, 800.0],
        source: Source::Typed,
        evaluated: None,
        missing: None,
        note: String::new(),
        area: Some(1.0),
    });
    ohl_structure_link::export_structure(SHAPES, &library, Some(&table), &recipes).unwrap()
}

/// A STEP file read back: entities by number, (type, attributes).
struct Parsed {
    header: String,
    entities: BTreeMap<usize, (String, Vec<String>)>,
}

/// The attributes of `args` at the top level (quotes and lists kept).
fn split_args(args: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth, mut quoted) = (Vec::new(), String::new(), 0, false);
    for c in args.chars() {
        match c {
            '\'' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth -= 1,
            ',' if !quoted && depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out.push(cur);
    out
}

/// Structural check of a STEP file: header, numbering from 1 without
/// gaps, every reference to an entity of the file, balanced quotes.
fn parse_step(text: &str) -> Parsed {
    assert!(text.starts_with("ISO-10303-21;\nHEADER;\n"));
    assert!(text.ends_with("ENDSEC;\nEND-ISO-10303-21;\n"));
    let (header, data) = text.split_once("DATA;\n").unwrap();
    assert!(header.contains("FILE_DESCRIPTION(("));
    assert!(header.contains("FILE_NAME("));
    assert!(header.contains("FILE_SCHEMA(('IFC"));
    let data = data.strip_suffix("ENDSEC;\nEND-ISO-10303-21;\n").unwrap();
    let mut entities = BTreeMap::new();
    for (k, line) in data.lines().enumerate() {
        let (number, rest) = line.split_once('=').unwrap();
        let n: usize = number.strip_prefix('#').unwrap().parse().unwrap();
        assert_eq!(n, k + 1, "entities numbered in order: {line}");
        let rest = rest.strip_suffix(';').unwrap();
        let open = rest.find('(').unwrap();
        let kind = &rest[..open];
        assert!(
            kind.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        );
        let args = rest[open + 1..].strip_suffix(')').unwrap();
        assert_eq!(args.matches('\'').count() % 2, 0, "{line}");
        entities.insert(n, (kind.to_owned(), split_args(args)));
    }
    let count = entities.len();
    for (n, (_, args)) in &entities {
        for a in args {
            let mut quoted = false;
            for (i, c) in a.char_indices() {
                if c == '\'' {
                    quoted = !quoted;
                }
                if c == '#' && !quoted {
                    let digits: String = a[i + 1..]
                        .chars()
                        .take_while(char::is_ascii_digit)
                        .collect();
                    let r: usize = digits.parse().unwrap();
                    assert!(r >= 1 && r <= count, "#{n} refers to #{r}");
                    assert_ne!(r, *n);
                }
            }
        }
    }
    Parsed {
        header: header.to_owned(),
        entities,
    }
}

impl Parsed {
    fn count(&self, kind: &str) -> usize {
        self.entities.values().filter(|(k, _)| k == kind).count()
    }

    fn of(&self, kind: &str) -> Vec<&Vec<String>> {
        self.entities
            .values()
            .filter(|(k, _)| k == kind)
            .map(|(_, a)| a)
            .collect()
    }

    /// Every entity has the number of attributes of its schema
    /// (`ADD2 TC1` / `TC1` EXPRESS, inherited ones included); GlobalIds
    /// unique.
    fn check_attributes(&self, schema: Schema) {
        let ifc4 = schema == Schema::Ifc4;
        let expected: BTreeMap<&str, usize> = [
            ("IFCCARTESIANPOINT", 1),
            ("IFCDIRECTION", 1),
            ("IFCAXIS2PLACEMENT3D", 3),
            ("IFCAXIS2PLACEMENT2D", 2),
            ("IFCGEOMETRICREPRESENTATIONCONTEXT", 6),
            ("IFCGEOMETRICREPRESENTATIONSUBCONTEXT", 10),
            ("IFCSIUNIT", 4),
            ("IFCDERIVEDUNITELEMENT", 2),
            ("IFCDERIVEDUNIT", 3),
            ("IFCUNITASSIGNMENT", 1),
            ("IFCPROJECT", 9),
            ("IFCLOCALPLACEMENT", 2),
            ("IFCSITE", 14),
            ("IFCBUILDING", 12),
            ("IFCBUILDINGSTOREY", 10),
            ("IFCRELAGGREGATES", 6),
            ("IFCMATERIAL", if ifc4 { 3 } else { 1 }),
            ("IFCPROPERTYSINGLEVALUE", 4),
            ("IFCMATERIALPROPERTIES", 4),
            ("IFCPROFILEPROPERTIES", 4),
            ("IFCLSHAPEPROFILEDEF", if ifc4 { 9 } else { 11 }),
            ("IFCUSHAPEPROFILEDEF", if ifc4 { 10 } else { 11 }),
            ("IFCISHAPEPROFILEDEF", if ifc4 { 10 } else { 8 }),
            ("IFCRECTANGLEHOLLOWPROFILEDEF", 8),
            ("IFCCIRCLEHOLLOWPROFILEDEF", 5),
            ("IFCCIRCLEPROFILEDEF", 4),
            ("IFCARBITRARYPROFILEDEFWITHVOIDS", 4),
            ("IFCPROFILEDEF", 2),
            ("IFCPOLYLINE", 1),
            ("IFCMATERIALPROFILE", 6),
            ("IFCMATERIALPROFILESET", 4),
            ("IFCMATERIALPROFILESETUSAGE", 3),
            ("IFCEXTRUDEDAREASOLID", 4),
            ("IFCSHAPEREPRESENTATION", 4),
            ("IFCTOPOLOGYREPRESENTATION", 4),
            ("IFCPRODUCTDEFINITIONSHAPE", 3),
            ("IFCMEMBER", if ifc4 { 9 } else { 8 }),
            ("IFCPROPERTYSET", 5),
            ("IFCRELDEFINESBYPROPERTIES", 6),
            ("IFCRELCONTAINEDINSPATIALSTRUCTURE", 6),
            ("IFCVERTEXPOINT", 1),
            ("IFCEDGE", 2),
            ("IFCBOUNDARYNODECONDITION", 7),
            ("IFCSTRUCTURALPOINTCONNECTION", 9),
            ("IFCSTRUCTURALCURVEMEMBER", 9),
            ("IFCRELCONNECTSSTRUCTURALMEMBER", 10),
            ("IFCRELASSIGNSTOPRODUCT", 7),
            ("IFCSTRUCTURALLOADCASE", 11),
            ("IFCSTRUCTURALLOADSINGLEFORCE", 7),
            ("IFCSTRUCTURALLOADLINEARFORCE", 7),
            ("IFCSTRUCTURALPOINTACTION", 10),
            ("IFCSTRUCTURALCURVEACTION", 12),
            ("IFCRELCONNECTSSTRUCTURALACTIVITY", 6),
            ("IFCRELASSIGNSTOGROUP", 7),
            ("IFCSTRUCTURALANALYSISMODEL", 10),
            ("IFCRELSERVICESBUILDINGS", 6),
            ("IFCRELASSOCIATESMATERIAL", 6),
            ("IFCPERSON", 8),
            ("IFCORGANIZATION", 5),
            ("IFCPERSONANDORGANIZATION", 3),
            ("IFCAPPLICATION", 4),
            ("IFCOWNERHISTORY", 8),
        ]
        .into_iter()
        .collect();
        let rooted = [
            "IFCPROJECT",
            "IFCSITE",
            "IFCBUILDING",
            "IFCBUILDINGSTOREY",
            "IFCRELAGGREGATES",
            "IFCMEMBER",
            "IFCPROPERTYSET",
            "IFCRELDEFINESBYPROPERTIES",
            "IFCRELCONTAINEDINSPATIALSTRUCTURE",
            "IFCSTRUCTURALPOINTCONNECTION",
            "IFCSTRUCTURALCURVEMEMBER",
            "IFCRELCONNECTSSTRUCTURALMEMBER",
            "IFCRELASSIGNSTOPRODUCT",
            "IFCSTRUCTURALLOADCASE",
            "IFCSTRUCTURALPOINTACTION",
            "IFCSTRUCTURALCURVEACTION",
            "IFCRELCONNECTSSTRUCTURALACTIVITY",
            "IFCRELASSIGNSTOGROUP",
            "IFCSTRUCTURALANALYSISMODEL",
            "IFCRELSERVICESBUILDINGS",
            "IFCRELASSOCIATESMATERIAL",
        ];
        let mut ids = BTreeSet::new();
        for (n, (kind, args)) in &self.entities {
            let want = expected
                .get(kind.as_str())
                .unwrap_or_else(|| panic!("#{n}: {kind} not expected"));
            assert_eq!(args.len(), *want, "#{n} {kind}({})", args.join(","));
            if rooted.contains(&kind.as_str()) {
                let g = &args[0];
                assert_eq!(g.len(), 24, "#{n} GlobalId {g}");
                assert!(ids.insert(g.clone()), "#{n}: GlobalId {g} twice");
                if !ifc4 {
                    assert!(
                        args[1].starts_with('#'),
                        "#{n}: IFC2x3 needs an owner history"
                    );
                }
            }
        }
    }
}

#[test]
fn type_02_as_export_data() {
    let s = structure(&type_02(), &SectionLibrary::default(), None).unwrap();
    assert_eq!(s.id, "type-02");
    assert_eq!(s.nodes.len(), 46);
    assert_eq!(s.members.len(), 178);
    assert_eq!(s.sections.len(), 3);
    assert_eq!(s.materials.len(), 1);
    assert_eq!(s.materials[0].id, "steel");
    assert_eq!(material_kind(&s.materials[0]), "Steel");
    assert_eq!(s.nodes.iter().filter(|n| n.is_support()).count(), 4);
    assert!(s.sections.iter().all(|x| x.shape.is_none()));
    let names: Vec<&str> = s.load_cases.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["wind", "ice, unbalanced", "body wind", "model"]);
    let model = &s.load_cases[3];
    assert_eq!(model.loads.len(), 2);
    assert_eq!(model.loads[0].force, [4000.0, 0.0, -9000.0]);
    assert!(matches!(model.loads[0].place, LoadPlace::Node(_)));
    // 2 kN ahead at set 1 (point A1, on joint A1).
    let ice = &s.load_cases[1];
    assert_eq!(ice.loads.len(), 1);
    assert_eq!(ice.loads[0].force, [0.0, 2000.0, 0.0]);
    assert_eq!(ice.self_weight, 1.0);
    // The line's values and the members' areas are missing on their own.
    assert!(s.notes.iter().any(|n| n.contains("without a value")));
    // Local axes are unit and orthogonal.
    for m in &s.members {
        let [x, y, z] = m.axes;
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        for (a, b) in [(x, y), (y, z), (x, z)] {
            assert!(dot(a, b).abs() < 1e-12);
        }
        assert!((dot(z, z) - 1.0).abs() < 1e-12);
    }
}

#[test]
fn ifc4_of_type_02() {
    let s = structure(&type_02(), &SectionLibrary::default(), None).unwrap();
    let text = ifc::ifc(&s, &IfcOptions::default()).unwrap();
    let p = parse_step(&text);
    assert!(p.header.contains("FILE_SCHEMA(('IFC4'))"));
    assert!(p.header.contains("StructuralAnalysisView"));
    p.check_attributes(Schema::Ifc4);
    assert_eq!(p.count("IFCPROJECT"), 1);
    assert_eq!(p.count("IFCMEMBER"), 178);
    assert_eq!(p.count("IFCSTRUCTURALCURVEMEMBER"), 178);
    assert_eq!(p.count("IFCSTRUCTURALPOINTCONNECTION"), 46);
    assert_eq!(p.count("IFCBOUNDARYNODECONDITION"), 4);
    assert_eq!(p.count("IFCRELCONNECTSSTRUCTURALMEMBER"), 2 * 178);
    assert_eq!(p.count("IFCSTRUCTURALANALYSISMODEL"), 1);
    assert_eq!(p.count("IFCSTRUCTURALLOADCASE"), 4);
    assert_eq!(p.count("IFCSTRUCTURALPOINTACTION"), 3);
    // [[section]] tables: profiles by name only, no solids.
    assert_eq!(p.count("IFCPROFILEDEF"), 3);
    assert_eq!(p.count("IFCEXTRUDEDAREASOLID"), 0);
    assert_eq!(p.count("IFCMATERIALPROFILESETUSAGE"), 3);
    // Pinned feet: translations held, rotations free.
    for c in p.of("IFCBOUNDARYNODECONDITION") {
        assert_eq!(c[1..4], ["IFCBOOLEAN(.T.)"; 3]);
        assert_eq!(c[4..7], ["IFCBOOLEAN(.F.)"; 3]);
    }
    // Truss members.
    assert!(
        p.of("IFCSTRUCTURALCURVEMEMBER")
            .iter()
            .all(|a| a[7] == ".PIN_JOINED_MEMBER.")
    );
    // Self-weight with every case (factor 1), down.
    for c in p.of("IFCSTRUCTURALLOADCASE") {
        assert_eq!(c[10], "(0.,0.,-1.0)");
    }
}

#[test]
fn ifc_members_and_ifc2x3() {
    let s = shapes();
    let members = ifc::ifc(
        &s,
        &IfcOptions {
            analysis: false,
            ..Default::default()
        },
    )
    .unwrap();
    let p = parse_step(&members);
    assert!(p.header.contains("ReferenceView_V1.2"));
    p.check_attributes(Schema::Ifc4);
    assert_eq!(p.count("IFCSTRUCTURALCURVEMEMBER"), 0);
    assert_eq!(p.count("IFCMEMBER"), 9);
    let old = ifc::ifc(
        &s,
        &IfcOptions {
            schema: Schema::Ifc2x3,
            analysis: false,
            ..Default::default()
        },
    )
    .unwrap();
    let p = parse_step(&old);
    assert!(p.header.contains("FILE_SCHEMA(('IFC2X3'))"));
    assert!(p.header.contains("CoordinationView_V2.0"));
    p.check_attributes(Schema::Ifc2x3);
    assert_eq!(p.count("IFCMEMBER"), 9);
    assert_eq!(p.count("IFCBUILDINGSTOREY"), 1);
    // IfcPerson WR1: a family or a given name (IfcOpenShell's rule check).
    let person = &p.of("IFCPERSON")[0];
    assert!(person[1] != "$" || person[2] != "$", "{person:?}");
    // Every profile with geometry is a solid; "general" has none.
    assert_eq!(p.count("IFCEXTRUDEDAREASOLID"), 8);
    assert_eq!(p.count("IFCPROFILEDEF"), 0);
    assert!(
        ifc::ifc(
            &s,
            &IfcOptions {
                schema: Schema::Ifc2x3,
                ..Default::default()
            }
        )
        .is_err()
    );
}

#[test]
fn profiles_per_shape() {
    let s = shapes();
    let entity = |id: &str| {
        s.sections
            .iter()
            .find(|x| x.id == id)
            .and_then(|x| x.shape.as_ref())
            .and_then(ifc::profile_entity)
    };
    assert_eq!(entity("EA-61x7"), Some("IFCLSHAPEPROFILEDEF"));
    assert_eq!(entity("UA-83x52x6"), Some("IFCLSHAPEPROFILEDEF"));
    assert_eq!(entity("CH-157"), Some("IFCUSHAPEPROFILEDEF"));
    assert_eq!(entity("IS-213"), Some("IFCISHAPEPROFILEDEF"));
    assert_eq!(entity("RHS-121x79x5"), Some("IFCRECTANGLEHOLLOWPROFILEDEF"));
    assert_eq!(entity("CHS-167x7"), Some("IFCCIRCLEHOLLOWPROFILEDEF"));
    assert_eq!(entity("RB-23"), Some("IFCCIRCLEPROFILEDEF"));
    assert_eq!(
        entity("POLE-12-430"),
        Some("IFCARBITRARYPROFILEDEFWITHVOIDS")
    );
    assert_eq!(entity("GEN-1"), None);

    let text = ifc::ifc(&s, &IfcOptions::default()).unwrap();
    let p = parse_step(&text);
    p.check_attributes(Schema::Ifc4);
    for kind in [
        "IFCUSHAPEPROFILEDEF",
        "IFCISHAPEPROFILEDEF",
        "IFCRECTANGLEHOLLOWPROFILEDEF",
        "IFCCIRCLEHOLLOWPROFILEDEF",
        "IFCCIRCLEPROFILEDEF",
        "IFCARBITRARYPROFILEDEFWITHVOIDS",
        "IFCPROFILEDEF",
    ] {
        assert_eq!(p.count(kind), 1, "{kind}");
    }
    assert_eq!(p.count("IFCLSHAPEPROFILEDEF"), 2);
    assert_eq!(p.count("IFCEXTRUDEDAREASOLID"), 8);
    // The equal angle: 61 × 61 × 7, root radius 8 mm; its centroid on the
    // axis (the position moves the box centre off it).
    let l = p
        .of("IFCLSHAPEPROFILEDEF")
        .into_iter()
        .find(|a| a[1] == "'EA-61x7'")
        .unwrap();
    assert_eq!(l[3..6], ["0.061", "0.061", "0.007"]);
    assert_eq!(l[6], "0.008");
    let ea = s.sections.iter().find(|x| x.id == "EA-61x7").unwrap();
    assert!(ea.centroid[0] < 0.0 && (ea.centroid[0] - ea.centroid[1]).abs() < 1e-12);
    // I section: width first.
    let i = p.of("IFCISHAPEPROFILEDEF")[0].clone();
    let Shape::ISection { h, b, .. } = s
        .sections
        .iter()
        .find(|x| x.id == "IS-213")
        .unwrap()
        .shape
        .clone()
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(i[3], step::real(b));
    assert_eq!(i[4], step::real(h));
    // Supports: fixed foot; the hinge with a rotational spring about z.
    let conds = p.of("IFCBOUNDARYNODECONDITION");
    assert_eq!(conds.len(), 2);
    assert!(conds.iter().any(|c| c[1..7] == ["IFCBOOLEAN(.T.)"; 6]));
    assert!(
        conds
            .iter()
            .any(|c| c[6] == "IFCROTATIONALSTIFFNESSMEASURE(2000.0)" && c[4] == "IFCBOOLEAN(.F.)")
    );
    // Loads: N3 (model), P1 along m-EA, the uniform load on m-IS.
    assert_eq!(p.count("IFCSTRUCTURALPOINTACTION"), 2);
    assert_eq!(p.count("IFCSTRUCTURALCURVEACTION"), 1);
    let line = p.of("IFCSTRUCTURALLOADLINEARFORCE")[0].clone();
    let len = s.members.iter().find(|m| m.id == "m-IS").unwrap().length;
    assert_eq!(line[3], step::real(-800.0 / len));
    // The case's own self-weight factor.
    assert!(
        p.of("IFCSTRUCTURALLOADCASE")
            .iter()
            .any(|c| c[2] == "'along'" && c[10] == "(0.,0.,-1.35)")
    );
    // Frame, truss and cable members.
    let kinds: BTreeSet<&str> = p
        .of("IFCSTRUCTURALCURVEMEMBER")
        .iter()
        .map(|a| a[7].as_str())
        .collect();
    assert_eq!(
        kinds,
        BTreeSet::from([".CABLE.", ".PIN_JOINED_MEMBER.", ".RIGID_JOINED_MEMBER."])
    );
}

#[test]
fn member_axes_follow_the_model() {
    let s = shapes();
    let m = |id: &str| s.members.iter().find(|m| m.id == id).unwrap();
    // Vertical: reference global x; y ⟂ x in the x–z plane.
    let ea = m("m-EA");
    assert_eq!(ea.axes[0], [0.0, 0.0, 1.0]);
    assert!((ea.axes[1][0] - 1.0).abs() < 1e-12);
    // Horizontal along x, rolled 30°: y from global z towards local z.
    let ua = m("m-UA");
    let r = 30f64.to_radians();
    assert!((ua.axes[1][2] - r.cos()).abs() < 1e-12);
    // Reference [0, 1, 0]: y in the x–y plane.
    let is = m("m-IS");
    assert!(is.axes[1][2].abs() < 1e-12);
    assert_eq!(m("m-RB").kind, ExportKind::Cable);
    assert_eq!(m("m-RB").pretension, 1000.0);
}

#[test]
fn saf_sheets_and_round_trip() {
    let s = structure(&type_02(), &SectionLibrary::default(), None).unwrap();
    let bytes = saf::workbook(&s).unwrap();
    let back = xlsx::read(&bytes).unwrap();
    let names: Vec<&str> = back.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Project",
            "Model",
            "StructuralMaterial",
            "StructuralCrossSection",
            "CompositeShapeDef",
            "StructuralPointConnection",
            "StructuralCurveMember",
            "StructuralPointSupport",
            "StructuralLoadGroup",
            "StructuralLoadCase",
            "StructuralPointAction",
        ]
    );
    let sheet = |n: &str| back.iter().find(|x| x.name == n).unwrap();
    for (n, cols) in [
        ("StructuralMaterial", saf::columns::MATERIAL),
        ("StructuralCrossSection", saf::columns::CROSS_SECTION),
        ("CompositeShapeDef", saf::columns::COMPOSITE_SHAPE),
        ("StructuralPointConnection", saf::columns::POINT_CONNECTION),
        ("StructuralCurveMember", saf::columns::CURVE_MEMBER),
        ("StructuralPointSupport", saf::columns::POINT_SUPPORT),
        ("StructuralLoadGroup", saf::columns::LOAD_GROUP),
        ("StructuralLoadCase", saf::columns::LOAD_CASE),
        ("StructuralPointAction", saf::columns::POINT_ACTION),
    ] {
        assert_eq!(sheet(n).header(), cols, "{n}");
    }
    let rows = |n: &str| sheet(n).rows.len() - 1;
    assert_eq!(rows("StructuralMaterial"), 1);
    assert_eq!(rows("StructuralCrossSection"), 3);
    assert_eq!(rows("CompositeShapeDef"), 3);
    assert_eq!(rows("StructuralPointConnection"), 46);
    assert_eq!(rows("StructuralCurveMember"), 178);
    assert_eq!(rows("StructuralPointSupport"), 4);
    assert_eq!(rows("StructuralLoadCase"), 4);
    // model: 2 × (x, z); ice: 1 × y.
    assert_eq!(rows("StructuralPointAction"), 5);
    let model = sheet("Model");
    let value = |k: &str| {
        model
            .rows
            .iter()
            .find(|r| r[0] == Cell::from(k))
            .map(|r| r[1].text())
    };
    assert_eq!(value("SAF Version").as_deref(), Some(saf::SAF_VERSION));
    assert_eq!(value("System of units").as_deref(), Some("Metric"));
    assert_eq!(
        value("Global coordinate system").as_deref(),
        Some("Z vertical")
    );
    let m = sheet("StructuralCurveMember");
    assert_eq!(m.get(1, "Nodes"), Some(&Cell::from("L0A; L1A")));
    assert_eq!(
        m.get(1, "Behaviour in analysis"),
        Some(&Cell::from("Axial force only"))
    );
    let sup = sheet("StructuralPointSupport");
    assert_eq!(sup.get(1, "Type"), Some(&Cell::from("Hinged")));
    assert_eq!(sup.get(1, "fix"), Some(&Cell::from("Free")));
    let mat = sheet("StructuralMaterial");
    assert_eq!(mat.get(1, "E modulus [MPa]"), Some(&Cell::Number(210000.0)));
    let f = sheet("StructuralPointAction");
    assert_eq!(f.get(1, "Value [kN]"), Some(&Cell::Number(2.0)));
    assert_eq!(f.get(1, "Direction"), Some(&Cell::from("Y")));
}

#[test]
fn saf_parametric_shapes() {
    let s = shapes();
    let back = xlsx::read(&saf::workbook(&s).unwrap()).unwrap();
    let cs = back
        .iter()
        .find(|x| x.name == "StructuralCrossSection")
        .unwrap();
    let shape_of = |id: &str| {
        let r = (1..cs.rows.len())
            .find(|r| cs.get(*r, "Name") == Some(&Cell::from(id)))
            .unwrap();
        (
            cs.get(r, "Cross-section type").unwrap().text(),
            cs.get(r, "Shape").map(Cell::text).unwrap_or_default(),
            cs.get(r, "Parameters [mm]")
                .map(Cell::text)
                .unwrap_or_default(),
        )
    };
    assert_eq!(
        shape_of("EA-61x7"),
        (
            "Parametric".into(),
            "Angle".into(),
            "61; 61; 7; 8; 0".into()
        )
    );
    assert_eq!(shape_of("CHS-167x7").1, "Pipe");
    assert_eq!(shape_of("RB-23").1, "Circle");
    assert_eq!(shape_of("IS-213").1, "I rolled");
    assert_eq!(shape_of("CH-157").1, "Channel");
    assert_eq!(shape_of("RHS-121x79x5").1, "Tube");
    assert_eq!(shape_of("POLE-12-430").0, "General");
    let shapes = back.iter().find(|x| x.name == "CompositeShapeDef").unwrap();
    assert_eq!(shapes.rows.len(), 3);
    let pole = shapes.get(1, "Polygon contour 2").unwrap().text();
    assert_eq!(pole.split('|').count(), 12);
    // Two materials: steel and the alloy.
    let mats = back
        .iter()
        .find(|x| x.name == "StructuralMaterial")
        .unwrap();
    assert_eq!(mats.rows.len(), 3);
    assert_eq!(mats.get(2, "Type"), Some(&Cell::from("Aluminium")));
    // On-beam point load and uniform load.
    let pa = back
        .iter()
        .find(|x| x.name == "StructuralPointAction")
        .unwrap();
    assert!((1..pa.rows.len()).any(
        |r| pa.get(r, "Force action") == Some(&Cell::from("On beam"))
            && pa.get(r, "Position x [m]") == Some(&Cell::Number(0.25))
    ));
    let ca = back
        .iter()
        .find(|x| x.name == "StructuralCurveAction")
        .unwrap();
    assert_eq!(ca.header(), saf::columns::CURVE_ACTION);
    assert_eq!(ca.get(1, "Member"), Some(&Cell::from("m-IS")));
}

#[test]
fn csv_tables() {
    let s = shapes();
    let t = csv::tables(&s);
    let names: Vec<&str> = t.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names,
        [
            "materials.csv",
            "sections.csv",
            "nodes.csv",
            "supports.csv",
            "members.csv",
            "load_cases.csv",
            "loads.csv"
        ]
    );
    let lines = |n: &str| t.iter().find(|(x, _)| *x == n).unwrap().1.lines().count() - 1;
    assert_eq!(lines("nodes.csv"), 4);
    assert_eq!(lines("members.csv"), 9);
    assert_eq!(lines("sections.csv"), 9);
    assert_eq!(lines("supports.csv"), 2);
    assert_eq!(lines("loads.csv"), 3);
    let members = &t.iter().find(|(x, _)| *x == "members.csv").unwrap().1;
    assert!(members.starts_with("id,node_i,node_j,section,kind,"));
    assert!(members.contains("m-RB,N3,N1,RB-23,cable,"));
}

#[test]
fn exports_are_deterministic() {
    let text = type_02();
    let a = structure(&text, &SectionLibrary::default(), None).unwrap();
    let b = structure(&text, &SectionLibrary::default(), None).unwrap();
    assert_eq!(a, b);
    let o = IfcOptions::default();
    assert_eq!(ifc::ifc(&a, &o).unwrap(), ifc::ifc(&b, &o).unwrap());
    assert_eq!(saf::workbook(&a).unwrap(), saf::workbook(&b).unwrap());
    assert_eq!(csv::tables(&a), csv::tables(&b));
    // GlobalIds follow the ids: the member m1 keeps its id when a joint
    // is renamed elsewhere.
    let renamed = text.replace("\"A2\"", "\"A2x\"");
    let c = structure(&renamed, &SectionLibrary::default(), None).unwrap();
    let id_of = |t: &str| {
        let p = parse_step(t);
        p.of("IFCMEMBER")
            .into_iter()
            .find(|a| a[2] == "'m1'")
            .unwrap()[0]
            .clone()
    };
    assert_eq!(
        id_of(&ifc::ifc(&a, &o).unwrap()),
        id_of(&ifc::ifc(&c, &o).unwrap())
    );
}

#[test]
fn write_files() {
    let dir = std::env::temp_dir().join(format!("ohl-structure-export-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let s = shapes();
    for f in Format::ALL {
        let out = match f.extension() {
            Some(e) => dir.join(format!("{}.{e}", f.as_str())),
            None => dir.join("csv"),
        };
        let written = write(&s, f, &out, 0).unwrap();
        assert!(written.iter().all(|p| p.exists()), "{f:?}");
        assert_eq!(Format::parse(f.as_str()), Some(f));
    }
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(ifc::iso_time(0), "1970-01-01T00:00:00");
    assert_eq!(ifc::iso_time(1_760_659_200), "2025-10-17T00:00:00");
}

/// The report of a structure file on its own (its recipes without line
/// values, its `[[load]]`s as case `model`).
fn report_of(text: &str, dir: Option<&Path>) -> report::Report {
    let library = SectionLibrary::default();
    let (table, recipes) = own_load_table(text, &library).unwrap();
    report::from_text(&report::FromText {
        text,
        file: "type.ohls",
        dir,
        sections: &library,
        sections_from: "none",
        table: &table,
        recipes: &recipes,
        extension: None,
        case: None,
        incomplete: &[("wind".to_owned(), "S12 not solved".to_owned())],
        software: "OHL-Structure 9.9.9",
        date: Some("2026-10-19"),
    })
    .unwrap()
}

/// The report of the example lattice structure: one page without any
/// external resource, the warning, the version, every case, the members
/// by usage with their governing case, the reactions; the CSV one row
/// per case and member.
#[test]
fn the_structure_report_of_a_lattice_structure() {
    let text = type_02();
    let r = report_of(&text, None);
    let h = &r.html;
    assert!(h.starts_with("<!DOCTYPE html>"));
    for gone in [
        "http:", "https:", "<script", "<link", "<img", "src=", "@import", "url(",
    ] {
        assert!(!h.contains(gone), "{gone}");
    }
    for shown in [
        report::WARNING,
        "OHL-Structure 9.9.9",
        "2026-10-19",
        "Level</th><td>3 — full design",
        "none — as written",
        "<h2>Sections and materials</h2>",
        "<h2>Load cases</h2>",
        "ice, unbalanced",
        "<h2>Usage per check</h2>",
        "members (finite elements)",
        "<h2>Members — largest usage over the cases</h2>",
        "<h2>Reactions</h2>",
        "<h2>Notes and flags</h2>",
        "wind: not checked — S12 not solved",
    ] {
        assert!(h.contains(shown), "{shown}");
    }
    // The warning at the top and at the end.
    assert_eq!(h.matches(report::WARNING).count(), 2);
    // Quotes are escaped.
    assert!(h.contains("the line&#39;s wind case"));
    assert!(!h.contains("Pole check"));
    let lines: Vec<&str> = r.csv.lines().collect();
    assert_eq!(lines[0], "case,member,axial_n,usage,governs,slack");
    let members = text.matches("[[member]]").count();
    assert_eq!(lines.len(), 1 + 4 * members);
    assert!(r.summary[0].starts_with("members: "), "{:?}", r.summary);
}

/// The report of the example pole structure with one case of its own:
/// the pole, its check per case and the governing case.
#[test]
fn the_structure_report_of_a_pole() {
    let dir = root().join("examples/synthetic-line/Structures");
    let text = std::fs::read_to_string(dir.join("type-05.ohls"))
        .unwrap()
        .replace("version = 2", "version = 3")
        + "\n[[load_case]]\nname = \"c1\"\n[[load_case.load]]\nvalue = \"5 kN\"\n\
           direction = \"x\"\npoints = [\"A1\"]\n";
    let r = report_of(&text, Some(&dir));
    for shown in ["<h2>Pole</h2>", "steel-pole-30m", "<h2>Pole check</h2>"] {
        assert!(r.html.contains(shown), "{shown}");
    }
    assert!(!r.html.contains("<h2>Reactions</h2>"));
    assert_eq!(r.csv.lines().count(), 1);
    assert!(
        r.summary[0].starts_with("pole: ") && r.summary[0].contains("in case c1"),
        "{:?}",
        r.summary
    );
    // An extension the structure does not have.
    let library = SectionLibrary::default();
    let (table, recipes) = own_load_table(&text, &library).unwrap();
    let e = report::from_text(&report::FromText {
        text: &text,
        file: "",
        dir: Some(&dir),
        sections: &library,
        sections_from: "none",
        table: &table,
        recipes: &recipes,
        extension: Some("+3"),
        case: None,
        incomplete: &[],
        software: "x",
        date: None,
    });
    assert!(e.unwrap_err().contains("+3"));
    assert_eq!(report::today().len(), 10);
}
