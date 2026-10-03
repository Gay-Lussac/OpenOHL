// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Exports of a solved line: wire points as CSV and a standalone 3D viewer.

use std::fmt::Write as _;

use crate::jumper::JumperShape;
use crate::solve::SectionShape;
use crate::{Line, Point};

/// Wire points as CSV (`section,span,index,x,y,z`), in the line's CRS.
pub fn wires_csv(shapes: &[SectionShape]) -> String {
    let mut out = String::from("section,span,index,x,y,z\n");
    for shape in shapes {
        for (s, span) in shape.spans.iter().enumerate() {
            for (k, p) in span.points.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "{},{},{},{:.4},{:.4},{:.4}",
                    shape.section, s, k, p[0], p[1], p[2]
                );
            }
        }
    }
    out
}

/// One row per span: tensions, sag, mid-point (half the horizontal clamp
/// distance) and lowest point, in the line's CRS.
pub fn spans_csv(shapes: &[SectionShape]) -> String {
    let mut out = String::from(
        "section,span,horizontal_tension_n,max_tension_n,sag_m,mid_x,mid_y,mid_z,low_x,low_y,low_z\n",
    );
    for shape in shapes {
        for (s, span) in shape.spans.iter().enumerate() {
            let (m, l) = (span.mid_point, span.low_point);
            let _ = writeln!(
                out,
                "{},{},{:.2},{:.2},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4}",
                shape.section,
                s,
                span.horizontal_tension,
                span.max_tension,
                span.sag,
                m[0],
                m[1],
                m[2],
                l[0],
                l[1],
                l[2]
            );
        }
    }
    out
}

/// A self-contained HTML page showing structures, insulators and wires in
/// 3D (three.js from a CDN). Coordinates are shifted to a local origin (the
/// first structure) so that the graphics card's 32-bit floats keep
/// millimetre precision even with national-grid coordinates (~2.6e6 m).
///
/// Wires are coloured by phase ([`Line::section_color`]) with a legend;
/// insulators of solved sections are drawn to their solved clamps (V-string
/// legs included, slack legs highlighted), others to their wire point.
/// `shapes` are matched to the line's sections by id.
pub fn viewer_html(line: &Line, shapes: &[SectionShape], title: &str) -> String {
    viewer_html_with(line, shapes, title, &ViewerExtras::default())
}

/// Largest ground surface drawn by the viewer, triangles: beyond this the
/// page gets too heavy for a standalone file (streaming comes with the
/// application renderer, ADR-0006).
pub const MAX_VIEWER_TRIANGLES: usize = 400_000;

/// Optional layers of the viewer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewerExtras {
    /// Ground surface: points and triangles (indices into the points).
    pub ground: Option<(Vec<Point>, Vec<[usize; 3]>)>,
    /// Marked points with labels (e.g. clearance violations), drawn as red
    /// spheres and listed.
    pub markers: Vec<(Point, String)>,
    /// Solved jumpers, drawn as wires (with their support strings).
    pub jumpers: Vec<JumperShape>,
}

/// [`viewer_html`] with extra layers.
pub fn viewer_html_with(
    line: &Line,
    shapes: &[SectionShape],
    title: &str,
    extras: &ViewerExtras,
) -> String {
    let origin = line.structures.first().map_or([0.0; 3], |s| s.position);
    let local = |p: &Point| [p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]];
    let fmt = |p: [f64; 3]| format!("[{:.3},{:.3},{:.3}]", p[0], p[1], p[2]);

    let mut structures = Vec::new();
    let mut insulators = Vec::new();
    let mut solved_points = std::collections::BTreeSet::new();
    for shape in shapes {
        let Some(section) = line.sections.iter().find(|s| s.id == shape.section) else {
            continue;
        };
        for (k, point) in section.points.iter().enumerate() {
            let (Some(a), Some(clamp)) = (line.attachment(point), shape.clamps.get(k)) else {
                continue;
            };
            solved_points.insert((point.structure.clone(), point.set, point.phase));
            let forces = shape.insulators.get(k).map_or(&[][..], Vec::as_slice);
            let tops: Vec<Point> = match &a.legs {
                Some(legs) if forces.len() == 2 => legs.to_vec(),
                _ => vec![a.insulator_point],
            };
            for (j, top) in tops.iter().enumerate() {
                let slack = forces.len() == 2 && forces[j] <= 0.0;
                insulators.push(format!(
                    "[{},{},{}]",
                    fmt(local(top)),
                    fmt(local(clamp)),
                    u8::from(slack)
                ));
            }
        }
    }
    for s in &line.structures {
        let top = s
            .attachments
            .iter()
            .map(|a| a.insulator_point[2])
            .fold(s.position[2], f64::max);
        // Stick drawing when the structure has one, else a mast line.
        let sticks: Vec<String> = s
            .members
            .iter()
            .map(|m| {
                format!(
                    "[{}]",
                    m.iter()
                        .map(|p| fmt(local(p)))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            })
            .collect();
        structures.push(format!(
            "{{\"id\":{},\"base\":{},\"top\":{},\"sticks\":[{}]}}",
            json_string(&s.id),
            fmt(local(&s.position)),
            fmt(local(&[s.position[0], s.position[1], top])),
            sticks.join(",")
        ));
        for a in &s.attachments {
            if solved_points.contains(&(s.id.clone(), a.set, a.phase)) {
                continue;
            }
            for top in a.legs.map_or(vec![a.insulator_point], |l| l.to_vec()) {
                insulators.push(format!(
                    "[{},{},0]",
                    fmt(local(&top)),
                    fmt(local(&a.wire_point))
                ));
            }
        }
    }
    let mut wires = Vec::new();
    let mut legend: Vec<(String, String)> = Vec::new();
    for shape in shapes {
        let section = line.sections.iter().find(|s| s.id == shape.section);
        let color = section.map_or_else(
            || crate::NEUTRAL_COLOR.to_owned(),
            |s| line.section_color(s),
        );
        let label = section.map_or_else(String::new, |s| {
            let text = format!("{} {}", s.circuit, s.phase_label);
            match text.trim() {
                "" => format!("phase {}", s.points.first().map_or(0, |p| p.phase)),
                t => t.to_owned(),
            }
        });
        if !legend.iter().any(|(l, c)| *l == label && *c == color) {
            legend.push((label, color.clone()));
        }
        for span in &shape.spans {
            let points: Vec<String> = span.points.iter().map(|p| fmt(local(p))).collect();
            wires.push(format!(
                "{{\"c\":{},\"p\":[{}]}}",
                json_string(&color),
                points.join(",")
            ));
        }
    }
    for shape in &extras.jumpers {
        let jumper = line.jumpers.iter().find(|j| j.id == shape.jumper);
        let color = jumper
            .and_then(|j| j.color.clone())
            .or_else(|| {
                let from = &jumper?.from;
                let section = line.sections.iter().find(|s| s.points.contains(from))?;
                Some(line.section_color(section))
            })
            .unwrap_or_else(|| crate::NEUTRAL_COLOR.to_owned());
        let points: Vec<String> = shape.points.iter().map(|p| fmt(local(p))).collect();
        wires.push(format!(
            "{{\"c\":{},\"p\":[{}]}}",
            json_string(&color),
            points.join(",")
        ));
        if let Some((top, clamp, force)) = shape.support {
            insulators.push(format!(
                "[{},{},{}]",
                fmt(local(&top)),
                fmt(local(&clamp)),
                u8::from(force <= 0.0)
            ));
        }
    }
    let legend: Vec<String> = legend
        .iter()
        .map(|(l, c)| format!("[{},{}]", json_string(l), json_string(c)))
        .collect();
    // Ground: flat arrays (x, y, z per point; three indices per triangle).
    let (mut ground_points, mut ground_triangles) = (String::new(), String::new());
    if let Some((points, triangles)) = extras
        .ground
        .as_ref()
        .filter(|(_, t)| t.len() <= MAX_VIEWER_TRIANGLES)
    {
        for p in points {
            let q = local(p);
            let _ = write!(ground_points, "{:.2},{:.2},{:.2},", q[0], q[1], q[2]);
        }
        for t in triangles {
            let _ = write!(ground_triangles, "{},{},{},", t[0], t[1], t[2]);
        }
        ground_points.pop();
        ground_triangles.pop();
    }
    let markers: Vec<String> = extras
        .markers
        .iter()
        .map(|(p, label)| format!("[{},{}]", fmt(local(p)), json_string(label)))
        .collect();
    let data = format!(
        "{{\"structures\":[{}],\"insulators\":[{}],\"wires\":[{}],\"legend\":[{}],\"ground\":{{\"p\":[{}],\"t\":[{}]}},\"markers\":[{}]}}",
        structures.join(","),
        insulators.join(","),
        wires.join(","),
        legend.join(","),
        ground_points,
        ground_triangles,
        markers.join(",")
    );
    VIEWER_TEMPLATE
        .replace("__TITLE__", &html_escape(title))
        .replace("__DATA__", &data)
}

/// A JSON string literal safe inside a `<script>` element.
fn json_string(text: &str) -> String {
    serde_json::to_string(text)
        .expect("strings serialise")
        .replace("</", "<\\/")
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

const VIEWER_TEMPLATE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>__TITLE__</title>
<style>
  html, body { margin: 0; height: 100%; background: #10151c; color: #dfe6ee; font: 13px system-ui, sans-serif; }
  #info { position: absolute; top: 8px; left: 10px; background: rgba(16,21,28,.8); padding: 6px 10px; border-radius: 6px; }
  canvas { display: block; }
  #legend { margin-top: 4px; }
  #legend span { display: inline-block; margin-right: 10px; white-space: nowrap; }
  #markers { margin-top: 4px; color: #ff8080; max-height: 30vh; overflow: auto; }
  #legend i { display: inline-block; width: 14px; height: 3px; margin-right: 4px; vertical-align: middle; }
</style>
</head>
<body>
<div id="info"><b>__TITLE__</b><br>drag: rotate · right-drag: pan · wheel: zoom · <span id="counts"></span><br>
<label>vertical exaggeration <input id="exag" type="range" min="1" max="10" step="1" value="1"> <span id="exagv">×1</span></label>
<div id="legend"></div><div id="markers"></div></div>
<script type="importmap">
{ "imports": { "three": "https://cdn.jsdelivr.net/npm/three@0.160.0/build/three.module.js",
               "three/addons/": "https://cdn.jsdelivr.net/npm/three@0.160.0/examples/jsm/" } }
</script>
<script type="module">
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
const data = __DATA__;
// Local coordinates: x east, y north, z up. three.js is y-up: map (x, y, z) -> (x, z, -y).
const v = p => new THREE.Vector3(p[0], p[2], -p[1]);
const scene = new THREE.Scene();
scene.background = new THREE.Color(0x10151c);
const camera = new THREE.PerspectiveCamera(50, innerWidth / innerHeight, 0.5, 200000);
const renderer = new THREE.WebGLRenderer({ antialias: true });
renderer.setSize(innerWidth, innerHeight);
document.body.appendChild(renderer.domElement);
const controls = new OrbitControls(camera, renderer.domElement);
const model = new THREE.Group();
scene.add(model);
const line = (points, color) => model.add(new THREE.Line(new THREE.BufferGeometry().setFromPoints(points.map(v)), new THREE.LineBasicMaterial({ color })));
data.structures.forEach(s => s.sticks.length ? s.sticks.forEach(m => line(m, 0x9aa7b4)) : line([s.base, s.top], 0x9aa7b4));
// Insulators: [top, clamp, slack]; slack V-string legs in magenta.
data.insulators.forEach(i => line([i[0], i[1]], i[2] ? 0xff4fd8 : 0x6fd3ff));
data.wires.forEach(w => line(w.p, new THREE.Color(w.c)));
// Ground surface (lit, flat-shaded) and markers.
if (data.ground.t.length) {
  const g = new THREE.BufferGeometry();
  const p = data.ground.p, pos = new Float32Array(p.length);
  for (let k = 0; k < p.length; k += 3) { pos[k] = p[k]; pos[k + 1] = p[k + 2]; pos[k + 2] = -p[k + 1]; }
  g.setAttribute("position", new THREE.BufferAttribute(pos, 3));
  g.setIndex(data.ground.t);
  g.computeVertexNormals();
  model.add(new THREE.Mesh(g, new THREE.MeshLambertMaterial({ color: 0x4f6b45, side: THREE.DoubleSide, flatShading: true })));
  scene.add(new THREE.HemisphereLight(0xdfe6ee, 0x202a20, 1.6));
  const sun = new THREE.DirectionalLight(0xffffff, 1.2);
  sun.position.set(1, 2, 1);
  scene.add(sun);
}
const markerList = document.getElementById("markers");
data.markers.forEach(([p, label]) => {
  const m = new THREE.Mesh(new THREE.SphereGeometry(2, 16, 12), new THREE.MeshBasicMaterial({ color: 0xff3030 }));
  m.position.copy(v(p));
  model.add(m);
  const item = document.createElement("div");
  item.textContent = "⚠ " + label;
  markerList.append(item);
});
const legend = document.getElementById("legend");
data.legend.forEach(([label, color]) => {
  const item = document.createElement("span");
  item.innerHTML = `<i style="background:${color}"></i>`;
  item.append(label);
  legend.append(item);
});
const box = new THREE.Box3().setFromObject(model);
const center = box.getCenter(new THREE.Vector3()), size = box.getSize(new THREE.Vector3()).length();
controls.target.copy(center);
// Start looking across the line from the side, far enough to see all of it.
camera.position.copy(center).add(new THREE.Vector3(size * 0.35, size * 0.45, size * 0.75));
controls.update();
document.getElementById("counts").textContent = `${data.structures.length} structures, ${data.wires.length} spans and jumpers`;
// Vertical exaggeration scales heights (three.js y) around the line's centre height.
const exag = document.getElementById("exag"), exagv = document.getElementById("exagv");
exag.addEventListener("input", () => {
  const k = Number(exag.value);
  model.scale.set(1, k, 1);
  model.position.y = center.y * (1 - k);
  exagv.textContent = `×${k}`;
});
addEventListener("resize", () => { camera.aspect = innerWidth / innerHeight; camera.updateProjectionMatrix(); renderer.setSize(innerWidth, innerHeight); });
renderer.setAnimationLoop(() => renderer.render(scene, camera));
</script>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solve::solve_line;
    use ols_core::units::Temperature;
    use ols_weather::WeatherCase;
    use std::path::Path;

    #[test]
    fn exports_contain_all_spans() {
        let line = crate::tests::example();
        let library = ols_cables::Library::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap();
        let shapes: Vec<_> = solve_line(
            &line,
            &library,
            &WeatherCase::still("15", Temperature::from_celsius(15.0)),
        )
        .into_iter()
        .map(Result::unwrap)
        .collect();
        let csv = wires_csv(&shapes);
        let spans: usize = shapes.iter().map(|s| s.spans.len()).sum();
        assert_eq!(
            csv.lines().count() - 1,
            shapes
                .iter()
                .flat_map(|s| &s.spans)
                .map(|s| s.points.len())
                .sum::<usize>()
        );
        let html = viewer_html(&line, &shapes, "Test <line>");
        assert!(html.contains("Test &lt;line&gt;"));
        assert_eq!(html.matches("\"wires\":[").count(), 1);
        assert!(spans > 0 && !html.contains("__DATA__"));
        // Default phase colours: phases 1 and 2 of the example.
        assert!(html.contains(crate::DEFAULT_PHASE_COLORS[0]));
        assert!(html.contains(crate::DEFAULT_PHASE_COLORS[1]));
    }
}
