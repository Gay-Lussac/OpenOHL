// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A neutral set of CSV tables (comma separated, a header row, SI units
//! in the headers, the structure's axes: x right, y ahead, z up), for any
//! program that reads tables: `materials.csv`, `sections.csv`,
//! `nodes.csv`, `supports.csv`, `members.csv`, `load_cases.csv`,
//! `loads.csv`.

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
    vec![
        ("materials.csv", materials),
        ("sections.csv", sections),
        ("nodes.csv", nodes),
        ("supports.csv", supports),
        ("members.csv", members),
        ("load_cases.csv", cases),
        ("loads.csv", loads),
    ]
}
