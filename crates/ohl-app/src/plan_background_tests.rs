// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! File › Export › Drawings…: the project's drawings and orthophotos
//! under the sheets' plan (2026-10-23), with invented backgrounds along
//! the example line: the window's ticks over the template's choice, kept
//! in the drawing settings; the export draws them on their layers, and
//! nothing of them without the ticks.

use crate::drawings::{DrawingSettings, DrawingsWindow, Input};
use crate::tests::synthetic;
use ohl_background::dxf::{DrawingLayer, Polyline};

#[test]
fn the_export_draws_the_projects_backgrounds_under_the_plan() {
    let dir = std::env::temp_dir().join(format!("ohl-app-plan-bg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // A project file holding an invented survey line and orthophoto
    // (10 m pixels over the main route).
    let file = dir.join("backgrounds.ohl");
    {
        let mut store = ohl_background::Store::open(&file).unwrap();
        let survey = ohl_background::Drawing {
            layers: vec![DrawingLayer {
                name: "survey".into(),
                color: [0, 0, 0],
                off: false,
            }],
            lines: vec![Polyline {
                layer: 0,
                color: [0, 0, 0],
                points: vec![[0.0, -20.0, 0.0], [640.0, -20.0, 0.0]],
                closed: false,
            }],
            ..Default::default()
        };
        store
            .put_drawing("survey", "invented", "", &survey)
            .unwrap();
        let png = dir.join("ortho.png");
        image::RgbImage::from_fn(200, 100, |x, y| image::Rgb([x as u8, y as u8, 90]))
            .save(&png)
            .unwrap();
        std::fs::write(dir.join("ortho.pgw"), "10\n0\n0\n-10\n-95\n495\n").unwrap();
        store
            .import_image(
                "Ortho",
                "invented",
                &png,
                "",
                "",
                None,
                &ohl_core::budget::Budget::new(),
                |_| {},
            )
            .unwrap();
    }
    let mut m = synthetic();
    let mut log = Vec::new();
    m.backgrounds = crate::backgrounds::load(&file, &mut log);
    m.sources.file = file.clone();
    assert_eq!(m.backgrounds.drawings.len(), 1, "{log:?}");
    let mut w = DrawingsWindow {
        route: Some(0),
        formats: vec![ohl_drawing::export::Format::Svg],
        page: "A3".into(),
        out: Some(dir.join("with")),
        ..Default::default()
    };
    w.sheet.background_drawings = Some(true);
    w.sheet.background_photo = Some(true);
    // Kept in the drawing settings.
    let kept: DrawingSettings =
        serde_json::from_str(&serde_json::to_string(&DrawingSettings::of(&w, false)).unwrap())
            .unwrap();
    let mut back = DrawingsWindow::default();
    kept.apply(&mut back);
    assert_eq!(back.sheet.background_drawings, Some(true));
    assert_eq!(back.sheet.background_photo, Some(true));
    let input = Input::new(&m, 0, &w).unwrap();
    assert_eq!(input.plan_drawings.len(), 1);
    assert!(input.plan_photo.is_some());
    crate::drawings::run(&input, &|_, _| {}).unwrap();
    let svg = std::fs::read_to_string(dir.join("with/main-sheet-01.svg")).unwrap();
    assert!(svg.contains(r#"class="OHL_BACKGROUND_DRAWINGS""#));
    assert!(svg.contains(r#"class="OHL_BACKGROUND_PHOTO""#));
    // Without the ticks (the built-in template asks for none): neither.
    let plain = DrawingsWindow {
        out: Some(dir.join("without")),
        sheet: crate::sheet_ui::SheetChoice::default(),
        ..w
    };
    let input = Input::new(&m, 0, &plain).unwrap();
    assert!(input.plan_drawings.is_empty() && input.plan_photo.is_none());
    crate::drawings::run(&input, &|_, _| {}).unwrap();
    let svg = std::fs::read_to_string(dir.join("without/main-sheet-01.svg")).unwrap();
    assert!(!svg.contains("OHL_BACKGROUND"));
    let _ = std::fs::remove_dir_all(&dir);
}
