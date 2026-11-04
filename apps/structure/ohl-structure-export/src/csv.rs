// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A neutral set of CSV tables (comma separated, a header row, SI units
//! in the headers, the structure's axes: x right, y ahead, z up), for any
//! program that reads tables: `materials.csv`, `sections.csv`,
//! `nodes.csv`, `supports.csv`, `members.csv`, `load_cases.csv`,
//! `loads.csv`; and when the file has them (version 9) `connections.csv`
//! and `plates.csv`, the values as the file gives them.

use ohl_core::text::csv_field;
use ohl_sections::Shape;
use ohl_structure_link::{ExportStructure, LoadPlace};

/// A number in full precision (empty when not finite).
fn num(x: f64) -> String {
    if x.is_finite() {
        format!("{x}")
    } else {
        String::new()
    }
}

fn opt(x: Option<f64>) -> String {
    x.map(num).unwrap_or_default()
}

fn table(header: &[&str], rows: Vec<Vec<String>>) -> String {
    let mut out = header.join(",");
    out.push('\n');
    for r in rows {
        let cells: Vec<String> = r.iter().map(|c| csv_field(c)).collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

/// A section's dimensions as `name=value` (m), `;` separated.
fn dimensions(shape: &Shape) -> String {
    let d = |pairs: &[(&str, f64)]| {
        pairs
            .iter()
            .map(|(n, v)| format!("{n}={}", num(*v)))
            .collect::<Vec<_>>()
            .join(";")
    };
    match *shape {
        Shape::EqualAngle { h, t, r } => d(&[("h", h), ("t", t), ("r", r)]),
        Shape::UnequalAngle { h, b, t, r } => d(&[("h", h), ("b", b), ("t", t), ("r", r)]),
        Shape::Channel { h, b, tw, tf, r } | Shape::ISection { h, b, tw, tf, r } => {
            d(&[("h", h), ("b", b), ("tw", tw), ("tf", tf), ("r", r)])
        }
        Shape::RectangularHollow { h, b, t } => d(&[("h", h), ("b", b), ("t", t)]),
        Shape::CircularHollow { d: dd, t } => d(&[("d", dd), ("t", t)]),
        Shape::RoundBar { d: dd } => d(&[("d", dd)]),
        Shape::PolygonalHollow { sides, d: dd, t } => {
            d(&[("sides", f64::from(sides)), ("d", dd), ("t", t)])
        }
        Shape::General { h, b } => d(&[("h", h), ("b", b)]),
        // h: the legs back to back (along z), b: the outstanding ones.
        Shape::DoubleAngle { h, b, t, r, gap } | Shape::StarAngle { h, b, t, r, gap } => {
            d(&[("h", h), ("b", b), ("t", t), ("r", r), ("gap", gap)])
        }
        Shape::CrossAngle { h, t, r, gap } => d(&[("h", h), ("t", t), ("r", r), ("gap", gap)]),
    }
}

/// The tables of `s`: (file name, CSV text), in order.
pub fn tables(s: &ExportStructure) -> Vec<(&'static str, String)> {
    let materials = table(
        &[
            "id",
            "name",
            "elasticity_pa",
            "shear_modulus_pa",
            "density_kg_m3",
            "yield_pa",
        ],
        s.materials
            .iter()
            .map(|m| {
                vec![
                    m.id.clone(),
                    m.name.clone(),
                    num(m.elasticity),
                    num(m.shear_modulus),
                    num(m.density),
                    opt(m.yield_strength),
                ]
            })
            .collect(),
    );
    let sections = table(
        &[
            "id",
            "name",
            "material",
            "shape",
            "dimensions_m",
            "area_m2",
            "iy_m4",
            "iz_m4",
            "it_m4",
            "wpl_y_m3",
            "wpl_z_m3",
        ],
        s.sections
            .iter()
            .map(|x| {
                vec![
                    x.id.clone(),
                    x.name.clone(),
                    s.materials[x.material].id.clone(),
                    x.shape
                        .as_ref()
                        .map(|sh| sh.as_str().to_owned())
                        .unwrap_or_default(),
                    x.shape.as_ref().map(dimensions).unwrap_or_default(),
                    num(x.area),
                    num(x.iy),
                    num(x.iz),
                    num(x.it),
                    opt(x.wpl[0]),
                    opt(x.wpl[1]),
                ]
            })
            .collect(),
    );
    let nodes = table(
        &["id", "x_m", "y_m", "z_m"],
        s.nodes
            .iter()
            .map(|n| {
                vec![
                    n.id.clone(),
                    num(n.point[0]),
                    num(n.point[1]),
                    num(n.point[2]),
                ]
            })
            .collect(),
    );
    let flag = |b: bool| if b { "1" } else { "0" }.to_owned();
    let supports = table(
        &[
            "node",
            "x",
            "y",
            "z",
            "rx",
            "ry",
            "rz",
            "kx_n_m",
            "ky_n_m",
            "kz_n_m",
            "krx_nm_rad",
            "kry_nm_rad",
            "krz_nm_rad",
        ],
        s.nodes
            .iter()
            .filter(|n| n.is_support())
            .map(|n| {
                let mut r = vec![n.id.clone()];
                r.extend(n.fixed.iter().map(|f| flag(*f)));
                r.extend(n.spring.iter().map(|k| num(*k)));
                r
            })
            .collect(),
    );
    let members = table(
        &[
            "id",
            "node_i",
            "node_j",
            "section",
            "kind",
            "bending",
            "check",
            "length_m",
            "k",
            "pretension_n",
            "y_x",
            "y_y",
            "y_z",
            "z_x",
            "z_y",
            "z_z",
        ],
        s.members
            .iter()
            .map(|m| {
                let mut r = vec![
                    m.id.clone(),
                    s.nodes[m.nodes[0]].id.clone(),
                    s.nodes[m.nodes[1]].id.clone(),
                    s.sections[m.section].id.clone(),
                    m.kind.as_str().to_owned(),
                    flag(m.bending),
                    flag(m.check),
                    num(m.length),
                    num(m.k),
                    num(m.pretension),
                ];
                r.extend(m.axes[1].iter().chain(&m.axes[2]).map(|v| num(*v)));
                r
            })
            .collect(),
    );
    let cases = table(
        &["case", "description", "self_weight_factor", "loads"],
        s.load_cases
            .iter()
            .map(|c| {
                vec![
                    c.name.clone(),
                    c.description.clone(),
                    num(c.self_weight),
                    c.loads.len().to_string(),
                ]
            })
            .collect(),
    );
    let loads = table(
        &[
            "case", "label", "at", "node", "member", "position", "fx", "fy", "fz", "unit",
        ],
        s.load_cases
            .iter()
            .flat_map(|c| {
                c.loads.iter().map(move |l| {
                    let (at, node, member, position, unit) = match l.place {
                        LoadPlace::Node(n) => {
                            ("node", s.nodes[n].id.clone(), "", String::new(), "N")
                        }
                        LoadPlace::OnMember { member, at } => (
                            "member_point",
                            String::new(),
                            s.members[member].id.as_str(),
                            num(at),
                            "N",
                        ),
                        LoadPlace::AlongMember(member) => (
                            "member_uniform",
                            String::new(),
                            s.members[member].id.as_str(),
                            String::new(),
                            "N/m",
                        ),
                    };
                    vec![
                        c.name.clone(),
                        l.label.clone(),
                        at.to_owned(),
                        node,
                        member.to_owned(),
                        position,
                        num(l.force[0]),
                        num(l.force[1]),
                        num(l.force[2]),
                        unit.to_owned(),
                    ]
                })
            })
            .collect(),
    );
    let mut out = vec![
        ("materials.csv", materials),
        ("sections.csv", sections),
        ("nodes.csv", nodes),
        ("supports.csv", supports),
        ("members.csv", members),
        ("load_cases.csv", cases),
        ("loads.csv", loads),
    ];
    if !s.connections.is_empty() {
        out.push(("connections.csv", connections(s)));
    }
    if !s.plates.is_empty() {
        out.push(("plates.csv", plates(s)));
    }
    out
}

/// The connections as the file gives them (empty: not given).
fn connections(s: &ExportStructure) -> String {
    let len = |l: &ohl_core::units::Length| num(l.si());
    let ol = |l: &Option<ohl_core::units::Length>| l.as_ref().map(len).unwrap_or_default();
    let ob = |b: Option<bool>| b.map(|b| b.to_string()).unwrap_or_default();
    table(
        &[
            "member",
            "joint",
            "kind",
            "onto",
            "leg",
            "bolts",
            "bolt",
            "bolt_diameter_m",
            "hole_m",
            "grade",
            "shear_planes",
            "gauge_lines",
            "gauges_m",
            "pitch_m",
            "end_distance_m",
            "edge_distance_m",
            "bolt_points_m",
            "eccentric",
            "restrained",
            "eccentricity_m",
            "cut_m",
            "shift_m",
        ],
        s.connections
            .iter()
            .map(|c| {
                let b = c.bolts.as_ref();
                vec![
                    c.member.clone(),
                    c.joint.clone(),
                    c.kind.map(|k| k.as_str().to_owned()).unwrap_or_default(),
                    c.onto.clone().unwrap_or_default(),
                    c.leg.map(|l| l.as_str().to_owned()).unwrap_or_default(),
                    b.map(|b| b.count.to_string()).unwrap_or_default(),
                    b.and_then(|b| b.bolt.clone()).unwrap_or_default(),
                    b.map(|b| ol(&b.diameter)).unwrap_or_default(),
                    b.map(|b| ol(&b.hole)).unwrap_or_default(),
                    b.and_then(|b| b.grade.clone()).unwrap_or_default(),
                    b.and_then(|b| b.shear_planes)
                        .map(|n| n.to_string())
                        .unwrap_or_default(),
                    c.gauge_lines.map(|n| n.to_string()).unwrap_or_default(),
                    c.gauges.iter().map(len).collect::<Vec<_>>().join(";"),
                    ol(&c.pitch),
                    ol(&c.end_distance),
                    ol(&c.edge_distance),
                    c.bolt_points
                        .iter()
                        .map(|p| format!("{} {}", len(&p[0]), len(&p[1])))
                        .collect::<Vec<_>>()
                        .join(";"),
                    ob(c.eccentric),
                    ob(c.restrained),
                    ol(&c.eccentricity),
                    ol(&c.cut),
                    ol(&c.shift),
                ]
            })
            .collect(),
    )
}

/// The plates as the file gives them.
fn plates(s: &ExportStructure) -> String {
    table(
        &[
            "id",
            "joint",
            "thickness_m",
            "material",
            "outline_m",
            "kind",
            "welded_to",
        ],
        s.plates
            .iter()
            .map(|p| {
                vec![
                    p.id.clone(),
                    p.joint.clone(),
                    num(p.thickness.si()),
                    p.material.clone().unwrap_or_default(),
                    p.outline
                        .iter()
                        .map(|q| q.iter().map(|v| num(v.si())).collect::<Vec<_>>().join(" "))
                        .collect::<Vec<_>>()
                        .join(";"),
                    p.kind.map(|k| k.as_str().to_owned()).unwrap_or_default(),
                    p.welded_to.join(";"),
                ]
            })
            .collect(),
    )
}
