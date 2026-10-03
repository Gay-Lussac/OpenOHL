// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;

/// LV95-sized coordinates: precision must hold far from 0.
const O: [f64; 3] = [2_600_000.0, 1_200_000.0, 500.0];

fn at(x: f64, y: f64, z: f64) -> [f64; 3] {
    [O[0] + x, O[1] + y, O[2] + z]
}

#[test]
fn camera_projects_the_target_to_the_centre() {
    let b = Bounds {
        min: at(-100.0, -50.0, 0.0),
        max: at(100.0, 50.0, 30.0),
    };
    let cam = Camera::fit(&b);
    let ([x, y], depth) = cam.project(cam.target, [800.0, 600.0]).unwrap();
    assert!(
        (x - 400.0).abs() < 1e-6 && (y - 300.0).abs() < 1e-6,
        "{x} {y}"
    );
    assert!((depth - cam.distance).abs() < 1e-6 * cam.distance);
    // A point above the target is above the centre on screen.
    let ([_, y_up], _) = cam.project(at(0.0, 0.0, 40.0), [800.0, 600.0]).unwrap();
    assert!(y_up < 300.0);
    // Behind the camera: nothing.
    let behind = {
        let e = cam.eye();
        [0, 1, 2].map(|k| e[k] + (e[k] - cam.target[k]))
    };
    assert!(cam.project(behind, [800.0, 600.0]).is_none());
    // Pan moves the target in the screen plane; zoom changes the distance.
    let mut c = cam;
    c.pan(100.0, 0.0, 600.0);
    assert!(c.target != cam.target && (c.target[2] - cam.target[2]).abs() < 1e-9);
    c.zoom(0.5);
    assert!((c.distance - 0.5 * cam.distance).abs() < 1e-9);
}

#[test]
fn renders_lines_and_surfaces_headless() {
    let mut ground = Scene::new(O);
    let g = [90, 120, 70, 255];
    ground.triangle(
        [
            at(-100.0, -100.0, 0.0),
            at(100.0, -100.0, 0.0),
            at(100.0, 100.0, 0.0),
        ],
        [g; 3],
    );
    ground.triangle(
        [
            at(-100.0, -100.0, 0.0),
            at(100.0, 100.0, 0.0),
            at(-100.0, 100.0, 0.0),
        ],
        [g; 3],
    );
    let mut wires = Scene::new(O);
    wires.polyline(
        &[at(-80.0, 0.0, 20.0), at(80.0, 0.0, 20.0)],
        [230, 60, 40, 255],
        3.0,
    );
    assert_eq!(wires.counts(), (1, 0, 0));
    let mut cam = Camera::fit(&ground.bounds().unwrap());
    cam.yaw = (-90f64).to_radians(); // from the south
    cam.pitch = 30f64.to_radians();
    let (w, h) = (320, 240);
    let image = match render_rgba(&[&ground, &wires], &cam, w, h) {
        Ok(i) => i,
        Err(e) => {
            assert!(
                std::env::var_os("OLS_REQUIRE_GPU_TESTS").is_none(),
                "no GPU: {e}"
            );
            eprintln!("skipping: {e}");
            return;
        }
    };
    assert_eq!(image.len(), (4 * w * h) as usize);
    if let Some(dir) = std::env::var_os("OLS_RENDER_PNG") {
        let path = std::path::Path::new(&dir).join("ols-render-test.png");
        let file = std::fs::File::create(&path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .unwrap()
            .write_image_data(&image)
            .unwrap();
    }

    let px = |x: u32, y: u32| {
        let k = (4 * (y * w + x)) as usize;
        [image[k], image[k + 1], image[k + 2]]
    };
    // Corner: background; centre: ground.
    assert_eq!(px(0, 0), BACKGROUND);
    let c = px(w / 2, h * 3 / 4);
    assert!(c[1] > c[0] && c[1] > c[2], "ground expected, got {c:?}");
    // The wire crosses the view horizontally above the target: some red
    // pixels in the middle column.
    let red = (0..h).any(|y| {
        let p = px(w / 2, y);
        p[0] > 150 && p[1] < 120
    });
    assert!(red, "wire not drawn");
}

#[test]
fn flying_and_looking() {
    let mut c = Camera::fit(&Bounds {
        min: at(-100.0, -100.0, 0.0),
        max: at(100.0, 100.0, 50.0),
    });
    c.yaw = (-90f64).to_radians(); // eye south of the target, looking north
    c.pitch = 0.3;
    let (t0, e0) = (c.target, c.eye());
    // W: north; D: east; Space: up — eye and target move together.
    c.fly(10.0, 0.0, 0.0);
    assert!((c.target[1] - t0[1] - 10.0).abs() < 1e-9 && (c.target[0] - t0[0]).abs() < 1e-9);
    c.fly(0.0, 5.0, 2.0);
    assert!((c.target[0] - t0[0] - 5.0).abs() < 1e-9);
    assert!((c.target[2] - t0[2] - 2.0).abs() < 1e-9);
    let e1 = c.eye();
    assert!((e1[1] - e0[1] - 10.0).abs() < 1e-9 && (e1[2] - e0[2] - 2.0).abs() < 1e-9);
    // Looking around keeps the eye in place and turns the view.
    let before = c.eye();
    c.look(0.5, 0.2);
    let after = c.eye();
    assert!(
        (0..3).all(|k| (before[k] - after[k]).abs() < 1e-6),
        "{before:?} {after:?}"
    );
    assert!((c.yaw - ((-90f64).to_radians() - 0.5)).abs() < 1e-12);
}

#[test]
fn a_ray_goes_through_what_is_projected_there() {
    let mut cam = Camera::fit(&Bounds {
        min: at(0.0, 0.0, 0.0),
        max: at(300.0, 100.0, 50.0),
    });
    cam.exaggeration = 3.0;
    let size = [800.0, 600.0];
    let p = at(100.0, 50.0, 10.0);
    let (q, _) = cam.project(p, size).unwrap();
    let (a, d) = cam.ray(q, size).unwrap();
    // Where the ray passes p's plan position, it is at p's true height.
    let t = ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / (d[0] * d[0] + d[1] * d[1]);
    let on = [a[0] + t * d[0], a[1] + t * d[1], a[2] + t * d[2]];
    for k in 0..3 {
        assert!((on[k] - p[k]).abs() < 1e-3, "{on:?} vs {p:?}");
    }
}
