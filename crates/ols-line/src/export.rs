// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Exports of a solved line: wire points as CSV and a standalone 3D viewer.

use std::fmt::Write as _;

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
pub fn viewer_html(line: &Line, shapes: &[SectionShape], title: &str) -> String {
    let origin = line.structures.first().map_or([0.0; 3], |s| s.position);
    let local = |p: &Point| [p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]];
    let fmt = |p: [f64; 3]| format!("[{:.3},{:.3},{:.3}]", p[0], p[1], p[2]);

    let mut structures = Vec::new();
    let mut insulators = Vec::new();
    for s in &line.structures {
        let top = s
            .attachments
            .iter()
            .map(|a| a.insulator_point[2])
            .fold(s.position[2], f64::max);
        structures.push(format!(
            "{{\"id\":{:?},\"base\":{},\"top\":{}}}",
            s.id,
            fmt(local(&s.position)),
            fmt(local(&[s.position[0], s.position[1], top]))
        ));
        for a in &s.attachments {
            insulators.push(format!(
                "[{},{}]",
                fmt(local(&a.insulator_point)),
                fmt(local(&a.wire_point))
            ));
        }
    }
    let mut wires = Vec::new();
    for shape in shapes {
        for span in &shape.spans {
            let points: Vec<String> = span.points.iter().map(|p| fmt(local(p))).collect();
            wires.push(format!("[{}]", points.join(",")));
        }
    }
    let data = format!(
        "{{\"structures\":[{}],\"insulators\":[{}],\"wires\":[{}]}}",
        structures.join(","),
        insulators.join(","),
        wires.join(",")
    );
    VIEWER_TEMPLATE
        .replace("__TITLE__", &html_escape(title))
        .replace("__DATA__", &data)
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
</style>
</head>
<body>
<div id="info"><b>__TITLE__</b><br>drag: rotate · right-drag: pan · wheel: zoom · <span id="counts"></span><br>
<label>vertical exaggeration <input id="exag" type="range" min="1" max="10" step="1" value="1"> <span id="exagv">×1</span></label></div>
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
data.structures.forEach(s => line([s.base, s.top], 0x9aa7b4));
data.insulators.forEach(i => line(i, 0x6fd3ff));
data.wires.forEach(w => line(w, 0xffc857));
const box = new THREE.Box3().setFromObject(model);
const center = box.getCenter(new THREE.Vector3()), size = box.getSize(new THREE.Vector3()).length();
controls.target.copy(center);
// Start looking across the line from the side, far enough to see all of it.
camera.position.copy(center).add(new THREE.Vector3(size * 0.35, size * 0.45, size * 0.75));
controls.update();
document.getElementById("counts").textContent = `${data.structures.length} structures, ${data.wires.length} spans`;
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
    }
}
