// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

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
                ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
                "no GPU: {e}"
            );
            eprintln!("skipping: {e}");
            return;
        }
    };
    assert_eq!(image.len(), (4 * w * h) as usize);
    if let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        let path = std::path::Path::new(&dir).join("ohl-render-test.png");
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

/// A slab looking east about x = 0: ±5 m along x, ±20 m across (y).
fn slab_east() -> Clip {
    Clip {
        origin: at(0.0, 0.0, 0.0),
        ahead: [1.0, 0.0],
        depth: 5.0,
        width: 20.0,
    }
}

/// Looking east the right is south: a point 3 m ahead and 10 m south is
/// inside; 6 m ahead or 21 m north is not. A box reaching into the slab
/// from far away meets it; one beside it does not, even when its plan
/// extent overlaps the slab's on one axis only.
#[test]
fn the_clip_box_holds_the_slab() {
    let c = slab_east();
    assert_eq!(c.right(), [0.0, -1.0]);
    assert!(c.contains(at(3.0, -10.0, 99.0)));
    assert!(!c.contains(at(6.0, 0.0, 0.0)));
    assert!(!c.contains(at(0.0, 21.0, 0.0)));
    let b = |x0: f64, y0: f64, x1: f64, y1: f64| Bounds {
        min: at(x0, y0, -10.0),
        max: at(x1, y1, 10.0),
    };
    assert!(c.meets(&b(4.0, 19.0, 400.0, 400.0)));
    assert!(!c.meets(&b(5.5, -100.0, 400.0, 100.0)));
    assert!(!c.meets(&b(-4.0, 20.5, 4.0, 30.0)));
    // A slab at 45°: the bounds' corner near its side, outside it (an
    // overlap of the plan ranges is not enough).
    let d = Clip {
        ahead: [std::f64::consts::FRAC_1_SQRT_2; 2],
        ..c
    };
    assert!(!d.meets(&b(10.0, 10.0, 14.0, 14.0)));
    assert!(d.meets(&b(-1.0, -1.0, 1.0, 1.0)));
    // Culling: a frustum clipped to it sees only what meets it.
    let mut cam = Camera::fit(&b(-50.0, -50.0, 50.0, 50.0));
    cam.yaw = std::f64::consts::PI; // eye west, looking east
    cam.pitch = 0.0;
    let f = crate::lod::Frustum::new(&cam, 1.5);
    assert!(f.sees(&b(30.0, -1.0, 32.0, 1.0)));
    let f = f.clipped(Some(c));
    assert!(!f.sees(&b(30.0, -1.0, 32.0, 1.0)));
    assert!(f.sees(&b(-1.0, -1.0, 1.0, 1.0)));
}

/// Orthographic, looking east level: one scale everywhere — two points
/// 10 m apart across are as far apart on the screen at the target and
/// 40 m nearer the eye; heights at the same scale; the depth grows away
/// from the eye; a ray goes straight ahead through where it was cast.
#[test]
fn an_orthographic_camera_draws_one_scale() {
    let mut cam = Camera::fit(&Bounds {
        min: at(-10.0, -10.0, 0.0),
        max: at(10.0, 10.0, 20.0),
    });
    cam.target = at(0.0, 0.0, 10.0);
    cam.yaw = std::f64::consts::PI;
    cam.pitch = 0.0;
    cam.ortho = true;
    let size = [800.0, 600.0];
    let m = cam.metres_per_pixel(size[1]);
    let px = |p| cam.project(p, size).unwrap();
    for x in [0.0, -40.0] {
        let (a, _) = px(at(x, 0.0, 10.0));
        let (b, _) = px(at(x, -10.0, 10.0));
        // South is to the right looking east.
        assert!(((b[0] - a[0]) * m - 10.0).abs() < 1e-6, "{a:?} {b:?}");
        let (c, _) = px(at(x, 0.0, 20.0));
        assert!(((a[1] - c[1]) * m - 10.0).abs() < 1e-6);
    }
    let (_, near) = px(at(-40.0, 0.0, 10.0));
    let (_, far) = px(at(0.0, 0.0, 10.0));
    assert!(near < far && (far - near - 40.0).abs() < 1e-6);
    // Nothing behind the eye is dropped (the eye stands far back).
    assert!(cam.project(cam.eye(), size).is_some());
    let (a, d) = cam.ray([500.0, 200.0], size).unwrap();
    assert!((d[0] - 1.0).abs() < 1e-9 && d[1].abs() < 1e-9 && d[2].abs() < 1e-9);
    let (q, _) = px([a[0] + 1000.0, a[1], a[2]]);
    assert!((q[0] - 500.0).abs() < 1e-6 && (q[1] - 200.0).abs() < 1e-6);
}

/// Clipped, a wire across the slab is drawn only within it, a point
/// beyond it not at all.
#[test]
fn the_renderer_draws_only_what_is_in_the_clip_box() {
    let mut s = Scene::new(O);
    // A red wire along y (across the slab, beyond its width too), at
    // x = 0; a green dot 30 m ahead (outside).
    s.polyline(
        &[at(0.0, -60.0, 10.0), at(0.0, 60.0, 10.0)],
        [230, 40, 40, 255],
        4.0,
    );
    s.point(at(30.0, 0.0, 15.0), [40, 230, 40, 255], 12.0);
    let mut cam = Camera::fit(&s.bounds().unwrap());
    cam.target = at(0.0, 0.0, 10.0);
    cam.yaw = std::f64::consts::PI;
    cam.pitch = 0.0;
    cam.ortho = true;
    cam.distance = 60.0 / (0.5 * cam.fov_y).tan() * 0.5;
    let (w, h) = (320, 240);
    let image = match render_rgba_clipped(&[&s], &cam, w, h, Some(slab_east())) {
        Ok(i) => i,
        Err(e) => {
            assert!(
                ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
                "no GPU: {e}"
            );
            eprintln!("skipping: {e}");
            return;
        }
    };
    let px = |x: u32, y: u32| {
        let k = (4 * (y * w + x)) as usize;
        [image[k], image[k + 1], image[k + 2]]
    };
    let size = [f64::from(w), f64::from(h)];
    let screen = |p| {
        let (q, _) = cam.project(p, size).unwrap();
        (q[0].round() as u32, q[1].round() as u32)
    };
    let red = |c: [u8; 3]| c[0] > 150 && c[1] < 100;
    // Inside: the wire 10 m right; outside: 40 m left, 40 m right.
    let (x, y) = screen(at(0.0, -10.0, 10.0));
    assert!(red(px(x, y)), "{:?}", px(x, y));
    for side in [40.0, -40.0] {
        let (x, y) = screen(at(0.0, side, 10.0));
        assert_eq!(px(x, y), BACKGROUND, "at {side}");
    }
    let (x, y) = screen(at(30.0, 0.0, 15.0));
    assert_eq!(px(x, y), BACKGROUND, "the dot beyond the depth");
}
