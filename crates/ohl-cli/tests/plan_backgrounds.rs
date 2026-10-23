// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl drawing` with the project's drawings and orthophotos under the
//! plan (2026-10-23): the example line imported into a project, an
//! invented survey drawing and an invented orthophoto (a textured
//! gradient, 0.5 m pixels) kept in it; `--plan-drawings` and
//! `--plan-orthophoto` draw them on their layers in SVG, PDF and PNG
//! (DXF: the drawings, a warning for the photo); without the flags the
//! sheets have neither. Set `OHL_KEEP_PLAN_BACKGROUNDS` to keep the
//! project and the sheets (to measure them).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use ohl_background::dxf::{DrawingLayer, Polyline};

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/synthetic-line")
        .join(name)
}

fn ohl(args: &[OsString]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(args)
        .output()
        .unwrap()
}

fn ok(out: &Output) -> String {
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    stdout
}

fn os(s: impl AsRef<std::ffi::OsStr>) -> OsString {
    s.as_ref().to_owned()
}

/// The invented orthophoto: 3000 × 1200 pixels of 0.5 m from (−100, 300)
/// (the main route's corridor), a gradient with a fine texture.
fn photo(dir: &Path) -> PathBuf {
    let png = dir.join("ortho.png");
    image::RgbImage::from_fn(3000, 1200, |x, y| {
        let noise = ((x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(40_503)) >> 7) % 48;
        image::Rgb([
            (x * 200 / 3000 + noise) as u8,
            (y * 200 / 1200 + noise) as u8,
            (90 + noise) as u8,
        ])
    })
    .save(&png)
    .unwrap();
    std::fs::write(dir.join("ortho.pgw"), "0.5\n0\n0\n-0.5\n-99.75\n299.75\n").unwrap();
    png
}

/// The invented survey: the axis of the main route's first segment, a
/// line 20 m right of it, a road across.
fn survey() -> ohl_background::Drawing {
    let line = |p: &[[f64; 2]]| Polyline {
        layer: 0,
        color: [180, 60, 0],
        points: p.iter().map(|q| [q[0], q[1], 0.0]).collect(),
        closed: false,
    };
    ohl_background::Drawing {
        layers: vec![DrawingLayer {
            name: "survey".into(),
            color: [180, 60, 0],
            off: false,
        }],
        lines: vec![
            line(&[[0.0, 0.0], [640.0, 0.0]]),
            line(&[[50.0, -20.0], [600.0, -20.0]]),
            line(&[[300.0, -400.0], [320.0, 400.0]]),
        ],
        ..Default::default()
    }
}

#[test]
fn the_projects_backgrounds_go_under_the_plan() {
    let dir = std::env::temp_dir().join(format!("ohl-plan-backgrounds-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let project = dir.join("backgrounds.ohl");
    ok(&ohl(&[
        os("project"),
        os("import"),
        os(example("synthetic-line.ohl.json")),
        os("--into"),
        os(&project),
        os("--user"),
        os("ann"),
    ]));
    {
        let mut store = ohl_background::Store::open(&project).unwrap();
        store
            .put_drawing("survey", "invented", "", &survey())
            .unwrap();
        store
            .import_image(
                "Ortho",
                "invented",
                &photo(&dir),
                "invented",
                "",
                None,
                &ohl_core::budget::Budget::new(),
                |_| {},
            )
            .unwrap();
    }
    let sheets = |out: &Path, more: &[&str]| {
        let mut args = vec![
            os("drawing"),
            os(&project),
            os("--route"),
            os("main"),
            os("--format"),
            os("svg,dxf,pdf,png"),
            os("--page"),
            os("A3"),
            os("--dpi"),
            os("60"),
            os("--out"),
            os(out),
        ];
        args.extend(more.iter().map(os));
        let t = std::time::Instant::now();
        let stdout = ok(&ohl(&args));
        (stdout, t.elapsed())
    };
    let with = dir.join("with");
    let (stdout, took) = sheets(
        &with,
        &[
            "--plan-drawings",
            "--plan-orthophoto",
            "--plan-background-dpi",
            "150",
        ],
    );
    assert!(stdout.contains("1 drawing(s) under the plan"), "{stdout}");
    assert!(stdout.contains("orthophoto Ortho (1 image(s))"), "{stdout}");
    assert!(
        stdout.contains("the orthophoto under the plan left out"),
        "{stdout}"
    );
    let svg = std::fs::read_to_string(with.join("main-sheet-01.svg")).unwrap();
    assert!(svg.contains(r#"class="OHL_BACKGROUND_DRAWINGS""#));
    assert!(svg.contains(r#"class="OHL_BACKGROUND_PHOTO""#));
    assert!(svg.contains("orthophoto Ortho") || !svg.contains("Legend"));
    let dxf = std::fs::read_to_string(with.join("main-sheet-01.dxf")).unwrap();
    assert!(dxf.contains("\nOHL_BACKGROUND_DRAWINGS\n"));
    assert!(dxf.contains("orthophoto(s) of the plan not written"));
    let pdf = std::fs::read(with.join("main.pdf")).unwrap();
    assert!(pdf.windows(10).any(|w| w == b"/DCTDecode"));
    // Names: a drawing the project does not hold is said.
    let named = dir.join("named");
    let (stdout, _) = sheets(&named, &["--plan-drawings", "cadastre"]);
    assert!(stdout.contains("no drawing \"cadastre\""), "{stdout}");
    // Without the flags (the built-in template asks for none): neither.
    let without = dir.join("without");
    let (_, plain) = sheets(&without, &[]);
    let svg = std::fs::read_to_string(without.join("main-sheet-01.svg")).unwrap();
    assert!(!svg.contains("OHL_BACKGROUND"));
    if std::env::var_os("OHL_KEEP_PLAN_BACKGROUNDS").is_some() {
        println!(
            "kept in {}: {took:.1?} with, {plain:.1?} without",
            dir.display()
        );
        for d in [&with, &without] {
            for e in std::fs::read_dir(d).unwrap() {
                let e = e.unwrap();
                println!(
                    "  {}: {} bytes",
                    e.path().display(),
                    e.metadata().unwrap().len()
                );
            }
        }
    } else {
        std::fs::remove_dir_all(&dir).ok();
    }
}
