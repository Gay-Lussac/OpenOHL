// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The windows the tools open, rendered as a user sees them (the tool
//! pages of the user guide and their review, owner 2026-11-14): with
//! `OHL_RENDER_PNG` naming a folder (and a GPU), each tool used on the
//! synthetic line as a click would, its window saved as a PNG
//! (`tool-<name>.png`; `OHL_RENDER_ONLY` a comma list of names). Made as
//! `USER=tester`. Without it, the test does nothing.

use super::*;

use crate::tools::Tool;

/// A tool window: its name and how it is opened on the synthetic line.
struct Shot {
    name: &'static str,
    /// The window's title (or its start).
    title: &'static str,
    open: fn(&mut Harness<'_, App>),
}

/// The position of structure `id`.
fn at(h: &mut Harness<'_, App>, id: &str) -> [f64; 3] {
    let r = h.state_mut().ready().unwrap();
    r.model.line.structure(id).unwrap().position
}

/// A point on the ground beside the line, between `a` and `b` and
/// `side` metres off it.
fn ground_by(h: &mut Harness<'_, App>, a: &str, b: &str, side: f64) -> [f64; 3] {
    let (pa, pb) = (at(h, a), at(h, b));
    let (dx, dy) = (pb[0] - pa[0], pb[1] - pa[1]);
    let n = dx.hypot(dy).max(1e-9);
    let (x, y) = (
        (pa[0] + pb[0]) / 2.0 - dy / n * side,
        (pa[1] + pb[1]) / 2.0 + dx / n * side,
    );
    let r = h.state_mut().ready().unwrap();
    let z = r.model.ground_at(x, y).unwrap_or(pa[2]);
    [x, y, z]
}

const SHOTS: &[Shot] = &[
    Shot {
        name: "goto",
        title: "Go to structure",
        open: |h| use_tool(h, Tool::GoTo, Selection::Structure("T3".into()), None),
    },
    Shot {
        name: "move",
        title: "Move structure T3",
        open: |h| use_tool(h, Tool::Move, Selection::Structure("T3".into()), None),
    },
    Shot {
        name: "delete",
        title: "Delete ",
        open: |h| use_tool(h, Tool::Delete, Selection::Structure("T3".into()), None),
    },
    Shot {
        name: "edit-structure",
        title: "Structure T2",
        open: |h| use_tool(h, Tool::Edit, Selection::Structure("T2".into()), None),
    },
    Shot {
        name: "edit-string",
        title: "Strings of set 1 of T2",
        open: |h| use_tool(h, Tool::Edit, string_at("T2", 1), None),
    },
    Shot {
        name: "edit-section",
        title: "Section S1",
        open: |h| {
            let span = Selection::Span {
                section: "S1".into(),
                span: 0,
            };
            use_tool(h, Tool::Edit, span, None);
        },
    },
    Shot {
        name: "edit-jumper",
        title: "Jumper J-T3-branch",
        open: |h| use_tool(h, Tool::Edit, Selection::Jumper("J-T3-branch".into()), None),
    },
    Shot {
        name: "edit-alignment-point",
        title: "Alignment point P2",
        open: |h| {
            let p = Selection::AlignmentPoint { id: "P2".into() };
            use_tool(h, Tool::Edit, p, None);
        },
    },
    Shot {
        name: "add-structure",
        title: "Add structure",
        open: |h| {
            let p = ground_by(h, "T4", "T5", 40.0);
            use_tool(h, Tool::AddStructure, Selection::None, Some(p));
        },
    },
    Shot {
        name: "add-string",
        title: "Add strings to T7",
        open: |h| use_tool(h, Tool::AddString, Selection::Structure("T7".into()), None),
    },
    Shot {
        name: "add-alignment-point",
        title: "New alignment point",
        open: |h| {
            let p = ground_by(h, "T4", "T5", 40.0);
            use_tool(h, Tool::AddAlignment, Selection::None, Some(p));
        },
    },
    Shot {
        name: "new-alignment",
        title: "New alignment: its route",
        open: |h| {
            for side in [40.0, 80.0] {
                let p = ground_by(h, "T4", "T5", side);
                use_tool(h, Tool::NewAlignment, Selection::None, Some(p));
                h.get_by_label("Add the point").click();
                h.run();
            }
            press(h, egui::Key::Enter, egui::Modifiers::NONE);
        },
    },
    Shot {
        name: "new-alignment-point",
        title: "Point coordinates",
        open: |h| {
            let p = ground_by(h, "T4", "T5", 40.0);
            use_tool(h, Tool::NewAlignment, Selection::None, Some(p));
        },
    },
    Shot {
        name: "on-span",
        title: "On span 2 of S1",
        open: |h| {
            let (a, b) = (at(h, "T2"), at(h, "T3"));
            let mid = [
                (a[0] + b[0]) / 2.0,
                (a[1] + b[1]) / 2.0,
                (a[2] + b[2]) / 2.0,
            ];
            let span = Selection::Span {
                section: "S1".into(),
                span: 1,
            };
            use_tool(h, Tool::OnSpan, span, Some(mid));
        },
    },
    Shot {
        name: "replace",
        title: "Replace structure T3 by type-02",
        // The type chosen in the picker first (as its test does): the
        // window that maps the wire points.
        open: |h| {
            crate::structure_picker::Picker::set_filter(&h.ctx, "type-02");
            use_tool(h, Tool::Replace, Selection::Structure("T3".into()), None);
            h.get_by_label("type-02").click();
            h.run();
            h.get_by_label("Choose").click();
            h.run();
        },
    },
    Shot {
        name: "measure-distance",
        title: "Measurements",
        open: |h| {
            for id in ["T2", "T3"] {
                let p = at(h, id);
                use_tool(
                    h,
                    Tool::MeasureDistance,
                    Selection::Structure(id.into()),
                    Some(p),
                );
            }
        },
    },
    Shot {
        name: "note",
        title: "Note",
        open: |h| {
            let p = at(h, "T2");
            use_tool(h, Tool::Note, Selection::Structure("T2".into()), Some(p));
        },
    },
    Shot {
        name: "follow",
        title: "Follow and snap",
        open: |h| {
            let p = at(h, "T2");
            use_tool(h, Tool::Follow, Selection::Structure("T2".into()), Some(p));
        },
    },
    Shot {
        name: "cross-section",
        title: "Cross-section",
        open: |h| {
            h.state_mut().ready().unwrap().open_cut_dialog();
            h.run();
        },
    },
    Shot {
        name: "inventory",
        title: "Inventory — tools",
        open: |h| {
            h.state_mut().ready().unwrap().inventory = true;
            h.run();
        },
    },
];

/// The window whose title holds `title`: of the nodes so named,
/// the largest (the window, not its title bar).
pub(super) fn window_rect(h: &Harness<'_, App>, title: &str) -> Option<egui::Rect> {
    h.query_all_by_label_contains(title)
        .map(|n| n.rect())
        .max_by(|a, b| a.area().total_cmp(&b.area()))
}

/// `image` (RGBA, `ppp` pixels per point) cut to `rect` and a margin.
pub(super) fn crop(image: &image::RgbaImage, rect: egui::Rect, ppp: f32) -> image::RgbaImage {
    let m = 6.0;
    let x0 = ((rect.left() - m) * ppp).max(0.0) as u32;
    let y0 = ((rect.top() - m) * ppp).max(0.0) as u32;
    let x1 = (((rect.right() + m) * ppp) as u32).min(image.width());
    let y1 = (((rect.bottom() + m) * ppp) as u32).min(image.height());
    image::imageops::crop_imm(image, x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
        .to_image()
}

/// Every tool window as a PNG in `OHL_RENDER_PNG` (made as
/// `USER=tester`): the window alone (`tool-<name>.png`, the user guide's
/// pictures) and the whole screen (`screen-tool-<name>.png`, where it
/// opens).
#[test]
fn tool_windows_rendered_for_review() {
    let Some(dir) =
        ohl_core::env::var_os("OHL_RENDER_PNG").filter(|_| ohl_render::headless_device().is_ok())
    else {
        return;
    };
    assert_eq!(crate::files::user(), "tester", "made with USER=tester");
    let dir = PathBuf::from(dir);
    let only = ohl_core::env::var_os("OHL_RENDER_ONLY").map(|s| s.to_string_lossy().into_owned());
    for shot in SHOTS {
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|x| x == shot.name))
        {
            continue;
        }
        // Edit on a typed structure opens its type in the Structure
        // library: the Structure editor of an untyped one is shown.
        let name = format!("tool-shots-{}", shot.name);
        let model = if shot.name == "edit-structure" {
            synthetic_project(&name)
        } else {
            typed_project(&name)
        };
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1300.0, 900.0))
            .wgpu()
            .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
        harness.run();
        (shot.open)(&mut harness);
        harness.run_steps(6);
        let image = harness.render().unwrap();
        let save = |name: String, image: &image::RgbaImage| {
            let path = dir.join(name);
            crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
        };
        save(format!("screen-tool-{}.png", shot.name), &image);
        let ppp = image.width() as f32 / 1300.0;
        // The window alone; a page docked in the layout (the Structure
        // editor): the whole screen.
        match window_rect(&harness, shot.title).filter(|r| r.area() > 20_000.0) {
            Some(rect) => save(format!("tool-{}.png", shot.name), &crop(&image, rect, ppp)),
            None => save(format!("tool-{}.png", shot.name), &image),
        }
    }
}
