// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Tables page's row cache: the rows shown are the rows built, and
//! are built again when the line, a solution or the settings change; a
//! benchmark of the page's frames (ignored by default).

use eframe::egui;

use super::*;

/// One frame of table `kind` drawn headless; how long the page took.
fn frame(
    ctx: &egui::Context,
    page: &mut TablePage,
    kind: TableKind,
    model: &Model,
    palette: &mut crate::color_tool::Palette,
    preview: &mut crate::preview3d::Preview,
) -> std::time::Duration {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1600.0, 1000.0),
        )),
        ..Default::default()
    };
    let mut took = std::time::Duration::ZERO;
    let mut out = ctx.run_ui(input, |ui| {
        let mut tools = TableTools {
            palette,
            rs: None,
            preview,
        };
        let t = std::time::Instant::now();
        let _ = page.ui(ui, kind, model, 0, &Selection::None, &mut tools);
        took = t.elapsed();
    });
    // No renderer: the textures are not uploaded.
    out.textures_delta.clear();
    took
}

fn cells(rows: &[Row]) -> Vec<(String, Vec<String>)> {
    rows.iter()
        .map(|r| (r.key.clone(), r.cells.clone()))
        .collect()
}

/// The rows kept are those built; an edit of the line, a weather case
/// solved again and another weather case shown build them again.
#[test]
fn cached_rows_follow_the_model() {
    let mut model = crate::tests::synthetic();
    let ctx = egui::Context::default();
    let mut palette = crate::color_tool::Palette::default();
    let mut preview = crate::preview3d::Preview::default();
    for kind in TableKind::ALL {
        let mut page = TablePage::default();
        frame(&ctx, &mut page, kind, &model, &mut palette, &mut preview);
        assert_eq!(
            cells(&page.cached_rows(kind, &model, 0)),
            cells(&kind.rows(&model, 0)),
            "{kind:?}"
        );
    }
    let mut page = TablePage::default();
    let kind = TableKind::Structures;
    let first = page.cached_rows(kind, &model, 0);
    // Nothing changed: the same rows, not built again.
    assert!(Arc::ptr_eq(&first, &page.cached_rows(kind, &model, 0)));
    // The line changed.
    model.line.structures[0].position[2] += 1.0;
    model.line_version += 1;
    let moved = page.cached_rows(kind, &model, 0);
    assert!(!Arc::ptr_eq(&first, &moved));
    assert_eq!(cells(&moved), cells(&kind.rows(&model, 0)));
    // A weather case solved again (the line as it was): sections' results.
    let kind = TableKind::Sections;
    let before = page.cached_rows(kind, &model, 0);
    let case = model.cases[0].case.clone();
    model.cases[0] = crate::model::Solved::new(&model.line, &model.library, case);
    assert!(!Arc::ptr_eq(&before, &page.cached_rows(kind, &model, 0)));
    // Another case.
    let again = page.cached_rows(kind, &model, 0);
    if model.cases.len() > 1 {
        assert!(!Arc::ptr_eq(&again, &page.cached_rows(kind, &model, 1)));
        assert_eq!(
            cells(&page.cached_rows(kind, &model, 1)),
            cells(&kind.rows(&model, 1))
        );
    }
}

/// The page's frames per table on the synthetic example (median of 60;
/// release build: `cargo test --release -p ohl-app --bin ohl-design
/// tables_frame_benchmark -- --ignored --nocapture`).
#[test]
#[ignore]
fn tables_frame_benchmark() {
    let model = crate::tests::synthetic();
    let ctx = egui::Context::default();
    let mut palette = crate::color_tool::Palette::default();
    let mut preview = crate::preview3d::Preview::default();
    println!("table | rows | median frame | slowest");
    for kind in TableKind::ALL {
        let mut page = TablePage::default();
        for _ in 0..5 {
            frame(&ctx, &mut page, kind, &model, &mut palette, &mut preview);
        }
        let mut t: Vec<std::time::Duration> = (0..60)
            .map(|_| frame(&ctx, &mut page, kind, &model, &mut palette, &mut preview))
            .collect();
        t.sort();
        println!(
            "{} | {} | {:.3} ms | {:.3} ms",
            kind.title(),
            kind.rows(&model, 0).len(),
            t[30].as_secs_f64() * 1e3,
            t[59].as_secs_f64() * 1e3
        );
    }
}
