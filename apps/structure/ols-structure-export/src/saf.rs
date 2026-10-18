// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! SAF, the Structural Analysis Format (an open xlsx standard,
//! <https://www.saf.guide>, version [`SAF_VERSION`]): one sheet per object
//! kind, the columns of the specification in its order, metric units as
//! the column headers give them (m, mm for profile dimensions, MPa, kN).
//!
//! Sheets written: `Project`, `Model`, `StructuralMaterial`,
//! `StructuralCrossSection`, `CompositeShapeDef` (profiles without a
//! parametric SAF shape), `StructuralPointConnection`,
//! `StructuralCurveMember`, `StructuralPointSupport`, and with load cases
//! `StructuralLoadGroup`, `StructuralLoadCase`, `StructuralPointAction`,
//! `StructuralCurveAction`.

use ols_sections::Shape;
use ols_structure_link::{ExportKind, ExportSection, ExportStructure, LoadPlace};

use crate::step::uuid;
use crate::xlsx::{Cell, Sheet};

/// The SAF version the sheets follow (saf.guide, "stable").
pub const SAF_VERSION: &str = "2.2.0";

/// The load group every load case is in.
pub const LOAD_GROUP: &str = "LG1";

/// Columns of each sheet, in the specification's order.
pub mod columns {
    /// `StructuralMaterial`.
    pub const MATERIAL: &[&str] = &[
        "Name",
        "Type",
        "Subtype",
        "Quality",
        "Unit mass [kg/m3]",
        "E modulus [MPa]",
        "G modulus [MPa]",
        "Poisson Coefficient",
        "Thermal expansion [1/K]",
        "Design properties",
        "Id",
    ];
    /// `StructuralCrossSection`.
    pub const CROSS_SECTION: &[&str] = &[
        "Name",
        "Material",
        "Cross-section type",
        "Shape",
        "Parameters [mm]",
        "Profile",
        "Form code",
        "Description ID of the profile",
        "A [m2]",
        "Iy [m4]",
        "Iz [m4]",
        "It [m4]",
        "Iw[m6]",
        "Wply [m3]",
        "Wplz [m3]",
        "Id",
    ];
    /// `CompositeShapeDef` (two polygons: the outline and one opening).
    pub const COMPOSITE_SHAPE: &[&str] = &[
        "Name",
        "Material name 1",
        "Polygon contour 1",
        "Material name 2",
        "Polygon contour 2",
        "Id",
    ];
    /// `StructuralPointConnection`.
    pub const POINT_CONNECTION: &[&str] = &[
        "Name",
        "Coordinate X [m]",
        "Coordinate Y [m]",
        "Coordinate Z [m]",
        "Id",
    ];
    /// `StructuralCurveMember`.
    pub const CURVE_MEMBER: &[&str] = &[
        "Name",
        "Type",
        "Cross section",
        "Arbitrary definition",
        "Nodes",
        "Segments",
        "Internal nodes",
        "Length [m]",
        "Geometrical shape",
        "LCS",
        "LCS Rotation [deg]",
        "Coordinate X [m]",
        "Coordinate Y [m]",
        "Coordinate Z [m]",
        "System line",
        "Structural Y Eccentricity of Beg Node [mm]",
        "Structural Z Eccentricity of Beg Node [mm]",
        "Structural Y Eccentricity of End Node [mm]",
        "Structural Z Eccentricity of End Node [mm]",
        "Analysis Y Eccentricity of Beg Node [mm]",
        "Analysis Z Eccentricity of Beg Node [mm]",
        "Analysis Y Eccentricity of End Node [mm]",
        "Analysis Z Eccentricity of End Node [mm]",
        "Layer",
        "Behaviour in analysis",
        "Color",
        "Parent ID",
        "Id",
    ];
    /// `StructuralPointSupport`.
    pub const POINT_SUPPORT: &[&str] = &[
        "Name",
        "Type",
        "Boundary condition",
        "Node",
        "Member",
        "Coordinate system",
        "Origin",
        "Coordinate definition",
        "Position x [m]",
        "ux",
        "uy",
        "uz",
        "fix",
        "fiy",
        "fiz",
        "Stiffness X [MN/m]",
        "Stiffness Y [MN/m]",
        "Stiffness Z [MN/m]",
        "Stiffness Fix [MNm/rad]",
        "Stiffness Fiy [MNm/rad]",
        "Stiffness Fiz [MNm/rad]",
        "Id",
    ];
    /// `StructuralLoadGroup`.
    pub const LOAD_GROUP: &[&str] = &["Name", "Load group type", "Relation", "Load type", "Id"];
    /// `StructuralLoadCase`.
    pub const LOAD_CASE: &[&str] = &[
        "Name",
        "Description",
        "Action type",
        "Load group",
        "Load type",
        "Duration",
        "Id",
    ];
    /// `StructuralPointAction`.
    pub const POINT_ACTION: &[&str] = &[
        "Name",
        "Type",
        "Direction",
        "Force action",
        "Reference node",
        "Reference member",
        "Value [kN]",
        "Vector (X;Y;Z) [kN]",
        "Load case",
        "Coordinate system",
        "Origin",
        "Coordinate definition",
        "Position x [m]",
        "Repeat (n)",
        "Delta x [m]",
        "Id",
    ];
    /// `StructuralCurveAction`.
    pub const CURVE_ACTION: &[&str] = &[
        "Name",
        "Type",
        "Force action",
        "Distribution",
        "Direction",
        "Value 1 [kN/m]",
        "Value 2 [kN/m]",
        "Vector 1(X;Y;Z) [kN/m]",
        "Vector 2(X;Y;Z) [kN/m]",
        "Member",
        "Member Rib",
        "2D Member",
        "2D Member Region",
        "2D Member Opening",
        "Edge",
        "Internal edge",
        "Load case",
        "Coordinate system",
        "Location",
        "Coordinate definition",
        "Origin",
        "Extent",
        "Start point [m]",
        "End point [m]",
        "Eccentricity ey [mm]",
        "Eccentricity ez [mm]",
        "Parent ID",
        "Id",
    ];
}

/// A row from (column, value) pairs, in the sheet's column order.
fn row(columns: &[&str], values: Vec<(&str, Cell)>) -> Vec<Cell> {
    let mut out = vec![Cell::Empty; columns.len()];
    for (c, v) in values {
        let k = columns
            .iter()
            .position(|x| *x == c)
            .unwrap_or_else(|| panic!("no SAF column {c}"));
        out[k] = v;
    }
    out
}

/// A number in millimetres, as SAF's parameter lists give them.
fn mm(x: f64) -> String {
    let v = (x * 1000.0 * 1e6).round() / 1e6;
    format!("{v}")
}

/// The SAF shape and its parameters (mm) of a section library profile;
/// none: written as a `General` section with its outline polygons.
pub fn parametric(shape: &Shape) -> Option<(&'static str, String)> {
    Some(match *shape {
        Shape::EqualAngle { h, t, r } => ("Angle", [h, h, t, r, 0.0].map(mm).join("; ")),
        Shape::UnequalAngle { h, b, t, r } => ("Angle", [h, b, t, r, 0.0].map(mm).join("; ")),
        Shape::Channel { h, b, tw, tf, r } => ("Channel", [h, b, tf, tw, r].map(mm).join("; ")),
        Shape::ISection { h, b, tw, tf, r } => ("I rolled", [h, b, tf, tw, r].map(mm).join("; ")),
        Shape::RectangularHollow { h, b, t } => ("Tube", [h, b, t, 0.0, 0.0].map(mm).join("; ")),
        Shape::CircularHollow { d, t } => ("Pipe", [d, t].map(mm).join("; ")),
        Shape::RoundBar { d } => ("Circle", mm(d)),
        Shape::PolygonalHollow { .. } | Shape::General { .. } => return None,
    })
}

/// The polygons (mm, about the centroid; outline counter-clockwise, the
/// opening clockwise) of a section without a parametric SAF shape.
fn contours(s: &ExportSection) -> (Vec<[f64; 2]>, Option<Vec<[f64; 2]>>) {
    let poly = |sides: u32, a: f64| -> Vec<[f64; 2]> {
        let n = f64::from(sides);
        let r = a / (std::f64::consts::PI / n).cos();
        (0..sides)
            .map(|k| {
                let t = -std::f64::consts::FRAC_PI_2
                    + std::f64::consts::PI / n
                    + 2.0 * std::f64::consts::PI * f64::from(k) / n;
                [r * t.cos(), r * t.sin()]
            })
            .collect()
    };
    let rect = |w: f64, h: f64| {
        vec![
            [-w / 2.0, -h / 2.0],
            [w / 2.0, -h / 2.0],
            [w / 2.0, h / 2.0],
            [-w / 2.0, h / 2.0],
        ]
    };
    match s.shape {
        Some(Shape::PolygonalHollow { sides, d, t }) => {
            let mut inner = poly(sides, d / 2.0 - t);
            inner.reverse();
            (poly(sides, d / 2.0), Some(inner))
        }
        Some(Shape::General { h, b }) => (rect(b, h), None),
        // No profile: a square of the same area.
        _ => {
            let a = s.area.max(0.0).sqrt();
            (rect(a, a), None)
        }
    }
}

fn contour_text(points: &[[f64; 2]]) -> String {
    points
        .iter()
        .map(|p| format!("{}; {}", mm(p[0]), mm(p[1])))
        .collect::<Vec<_>>()
        .join("|")
}

/// SAF's support type (informative; each direction is given on its own).
fn support_type(fixed: [bool; 6], spring: [f64; 6]) -> &'static str {
    let springs = spring.iter().any(|k| *k != 0.0);
    if springs {
        "Custom"
    } else if fixed == [true; 6] {
        "Fixed"
    } else if fixed == [true, true, true, false, false, false] {
        "Hinged"
    } else {
        "Custom"
    }
}

/// The sheets of `s`, in the order written.
pub fn sheets(s: &ExportStructure) -> Vec<Sheet> {
    let scope = |kind: &str, id: &str| uuid(&format!("{}/{kind}:{id}", s.id));
    let mut out = Vec::new();

    // Project and model: property names in column A, values in B.
    let mut project = Sheet::new("Project", &["Name", &s.name]);
    project
        .rows
        .push(vec!["Description".into(), s.description.clone().into()]);
    project
        .rows
        .push(vec!["Id".into(), scope("project", "").into()]);
    out.push(project);
    let mut model = Sheet::new("Model", &["Name", &s.id]);
    for (k, v) in [
        ("Description", s.name.clone()),
        ("Discipline", "Load-bearing structure".into()),
        ("Source application", "OpenOHL".into()),
        ("SAF Version", SAF_VERSION.into()),
        ("Global coordinate system", "Z vertical".into()),
        ("LCS of cross-section", "ZYX".into()),
        ("System of units", "Metric".into()),
        ("National code", "EC-Standard-EN".into()),
        ("Id", scope("model", "")),
    ] {
        model.rows.push(vec![k.into(), v.into()]);
    }
    out.push(model);

    let mut materials = Sheet::new("StructuralMaterial", columns::MATERIAL);
    for m in &s.materials {
        materials.rows.push(row(
            columns::MATERIAL,
            vec![
                ("Name", m.id.clone().into()),
                ("Type", crate::material_kind(m).into()),
                ("Quality", m.name.clone().into()),
                ("Unit mass [kg/m3]", m.density.into()),
                ("E modulus [MPa]", (m.elasticity / 1e6).into()),
                ("G modulus [MPa]", (m.shear_modulus / 1e6).into()),
                ("Poisson Coefficient", m.poisson().into()),
                ("Id", scope("material", &m.id).into()),
            ],
        ));
    }
    out.push(materials);

    let mut sections = Sheet::new("StructuralCrossSection", columns::CROSS_SECTION);
    let mut shapes = Sheet::new("CompositeShapeDef", columns::COMPOSITE_SHAPE);
    for sec in &s.sections {
        let material = s.materials[sec.material].id.clone();
        let mut values = vec![
            ("Name", sec.id.clone().into()),
            ("Material", material.clone().into()),
            ("A [m2]", sec.area.into()),
            ("Iy [m4]", sec.iy.into()),
            ("Iz [m4]", sec.iz.into()),
            ("It [m4]", sec.it.into()),
            ("Wply [m3]", sec.wpl[0].into()),
            ("Wplz [m3]", sec.wpl[1].into()),
            ("Id", scope("section", &sec.id).into()),
        ];
        match sec.shape.as_ref().and_then(parametric) {
            Some((shape, parameters)) => {
                values.push(("Cross-section type", "Parametric".into()));
                values.push(("Shape", shape.into()));
                values.push(("Parameters [mm]", parameters.into()));
            }
            None => {
                let name = format!("{}-shape", sec.id);
                values.push(("Cross-section type", "General".into()));
                values.push(("Profile", name.clone().into()));
                let (outer, inner) = contours(sec);
                let mut v = vec![
                    ("Name", name.clone().into()),
                    ("Material name 1", material.into()),
                    ("Polygon contour 1", contour_text(&outer).into()),
                    ("Id", scope("shape", &name).into()),
                ];
                if let Some(inner) = inner {
                    v.push(("Polygon contour 2", contour_text(&inner).into()));
                }
                shapes.rows.push(row(columns::COMPOSITE_SHAPE, v));
            }
        }
        sections.rows.push(row(columns::CROSS_SECTION, values));
    }
    out.push(sections);
    if shapes.rows.len() > 1 {
        out.push(shapes);
    }

    let mut nodes = Sheet::new("StructuralPointConnection", columns::POINT_CONNECTION);
    for n in &s.nodes {
        nodes.rows.push(row(
            columns::POINT_CONNECTION,
            vec![
                ("Name", n.id.clone().into()),
                ("Coordinate X [m]", n.point[0].into()),
                ("Coordinate Y [m]", n.point[1].into()),
                ("Coordinate Z [m]", n.point[2].into()),
                ("Id", scope("node", &n.id).into()),
            ],
        ));
    }
    out.push(nodes);

    let mut members = Sheet::new("StructuralCurveMember", columns::CURVE_MEMBER);
    for m in &s.members {
        let z = m.axes[2];
        members.rows.push(row(
            columns::CURVE_MEMBER,
            vec![
                ("Name", m.id.clone().into()),
                ("Type", "General".into()),
                ("Cross section", s.sections[m.section].id.clone().into()),
                (
                    "Nodes",
                    format!("{}; {}", s.nodes[m.nodes[0]].id, s.nodes[m.nodes[1]].id).into(),
                ),
                ("Segments", "Line".into()),
                ("Length [m]", m.length.into()),
                ("Geometrical shape", "Line".into()),
                ("LCS", "z by vector".into()),
                ("LCS Rotation [deg]", 0.0.into()),
                ("Coordinate X [m]", z[0].into()),
                ("Coordinate Y [m]", z[1].into()),
                ("Coordinate Z [m]", z[2].into()),
                ("System line", "Centre".into()),
                ("Analysis Y Eccentricity of Beg Node [mm]", 0.0.into()),
                ("Analysis Z Eccentricity of Beg Node [mm]", 0.0.into()),
                ("Analysis Y Eccentricity of End Node [mm]", 0.0.into()),
                ("Analysis Z Eccentricity of End Node [mm]", 0.0.into()),
                (
                    "Behaviour in analysis",
                    match m.kind {
                        ExportKind::Truss => "Axial force only",
                        ExportKind::Frame => "Standard",
                        ExportKind::Cable | ExportKind::Tension => "Tension only",
                    }
                    .into(),
                ),
                ("Id", scope("member", &m.id).into()),
            ],
        ));
    }
    out.push(members);

    let mut supports = Sheet::new("StructuralPointSupport", columns::POINT_SUPPORT);
    for n in s.nodes.iter().filter(|n| n.is_support()) {
        let dof = |k: usize| -> Cell {
            if n.fixed[k] {
                "Rigid".into()
            } else if n.spring[k] != 0.0 {
                "Flexible".into()
            } else {
                "Free".into()
            }
        };
        let stiffness = |k: usize| -> Cell {
            (!n.fixed[k] && n.spring[k] != 0.0)
                .then(|| n.spring[k] / 1e6)
                .into()
        };
        supports.rows.push(row(
            columns::POINT_SUPPORT,
            vec![
                ("Name", format!("S-{}", n.id).into()),
                ("Type", support_type(n.fixed, n.spring).into()),
                ("Boundary condition", "In node".into()),
                ("Node", n.id.clone().into()),
                ("Coordinate system", "Global".into()),
                ("ux", dof(0)),
                ("uy", dof(1)),
                ("uz", dof(2)),
                ("fix", dof(3)),
                ("fiy", dof(4)),
                ("fiz", dof(5)),
                ("Stiffness X [MN/m]", stiffness(0)),
                ("Stiffness Y [MN/m]", stiffness(1)),
                ("Stiffness Z [MN/m]", stiffness(2)),
                ("Stiffness Fix [MNm/rad]", stiffness(3)),
                ("Stiffness Fiy [MNm/rad]", stiffness(4)),
                ("Stiffness Fiz [MNm/rad]", stiffness(5)),
                ("Id", scope("support", &n.id).into()),
            ],
        ));
    }
    out.push(supports);

    if s.load_cases.is_empty() {
        return out;
    }
    let mut groups = Sheet::new("StructuralLoadGroup", columns::LOAD_GROUP);
    groups.rows.push(row(
        columns::LOAD_GROUP,
        vec![
            ("Name", LOAD_GROUP.into()),
            ("Load group type", "Permanent".into()),
            ("Relation", "Standard".into()),
            ("Id", scope("load-group", LOAD_GROUP).into()),
        ],
    ));
    out.push(groups);
    let mut cases = Sheet::new("StructuralLoadCase", columns::LOAD_CASE);
    let mut points = Sheet::new("StructuralPointAction", columns::POINT_ACTION);
    let mut lines = Sheet::new("StructuralCurveAction", columns::CURVE_ACTION);
    let (mut np, mut nl) = (0, 0);
    for case in &s.load_cases {
        let mut description = case.description.clone();
        if case.self_weight != 0.0 {
            if !description.is_empty() {
                description.push_str("; ");
            }
            description.push_str(&format!("self-weight × {}", case.self_weight));
        }
        cases.rows.push(row(
            columns::LOAD_CASE,
            vec![
                ("Name", case.name.clone().into()),
                ("Description", description.into()),
                ("Action type", "Permanent".into()),
                ("Load group", LOAD_GROUP.into()),
                ("Load type", "Standard".into()),
                ("Id", scope("case", &case.name).into()),
            ],
        ));
        for l in &case.loads {
            for (k, axis) in ["X", "Y", "Z"].into_iter().enumerate() {
                let v = l.force[k] / 1000.0;
                if v == 0.0 {
                    continue;
                }
                match l.place {
                    LoadPlace::Node(n) => {
                        np += 1;
                        points.rows.push(row(
                            columns::POINT_ACTION,
                            vec![
                                ("Name", format!("F{np}").into()),
                                ("Type", "Standard".into()),
                                ("Direction", axis.into()),
                                ("Force action", "In node".into()),
                                ("Reference node", s.nodes[n].id.clone().into()),
                                ("Value [kN]", v.into()),
                                ("Load case", case.name.clone().into()),
                                ("Coordinate system", "Global".into()),
                                ("Id", scope("point-action", &format!("F{np}")).into()),
                            ],
                        ));
                    }
                    LoadPlace::OnMember { member, at } => {
                        np += 1;
                        points.rows.push(row(
                            columns::POINT_ACTION,
                            vec![
                                ("Name", format!("F{np}").into()),
                                ("Type", "Standard".into()),
                                ("Direction", axis.into()),
                                ("Force action", "On beam".into()),
                                ("Reference member", s.members[member].id.clone().into()),
                                ("Value [kN]", v.into()),
                                ("Load case", case.name.clone().into()),
                                ("Coordinate system", "Global".into()),
                                ("Origin", "From start".into()),
                                ("Coordinate definition", "Relative".into()),
                                ("Position x [m]", at.into()),
                                ("Repeat (n)", 1.0.into()),
                                ("Id", scope("point-action", &format!("F{np}")).into()),
                            ],
                        ));
                    }
                    LoadPlace::AlongMember(member) => {
                        nl += 1;
                        lines.rows.push(row(
                            columns::CURVE_ACTION,
                            vec![
                                ("Name", format!("LF{nl}").into()),
                                ("Type", "Standard".into()),
                                ("Force action", "On beam".into()),
                                ("Distribution", "Uniform".into()),
                                ("Direction", axis.into()),
                                ("Value 1 [kN/m]", v.into()),
                                ("Member", s.members[member].id.clone().into()),
                                ("Load case", case.name.clone().into()),
                                ("Coordinate system", "Global".into()),
                                ("Location", "Length".into()),
                                ("Coordinate definition", "Relative".into()),
                                ("Origin", "From start".into()),
                                ("Extent", "Full".into()),
                                ("Start point [m]", 0.0.into()),
                                ("End point [m]", 1.0.into()),
                                ("Eccentricity ey [mm]", 0.0.into()),
                                ("Eccentricity ez [mm]", 0.0.into()),
                                ("Id", scope("curve-action", &format!("LF{nl}")).into()),
                            ],
                        ));
                    }
                }
            }
        }
    }
    out.push(cases);
    if points.rows.len() > 1 {
        out.push(points);
    }
    if lines.rows.len() > 1 {
        out.push(lines);
    }
    out
}

/// The SAF workbook of `s`, as xlsx bytes.
pub fn workbook(s: &ExportStructure) -> Result<Vec<u8>, String> {
    crate::xlsx::write(&sheets(s))
}
