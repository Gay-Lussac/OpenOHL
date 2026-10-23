// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The plan's backgrounds (2026-10-23), with invented data along the
//! example line's main route (0 0 → T3 at 640 0, bending 10° left, → T5
//! at 1250.581 107.662): a drawing's lines straightened along the route
//! (along the axis on the axis, at an offset at that offset, cut at the
//! width, following the bend), clipped to each sheet's plan on their own
//! layer; a gradient orthophoto resampled along the route; sheets without
//! the option as before.

use std::sync::Arc;

use ohl_background::dxf::{DrawingLayer, Label, Mark, Polyline};
use ohl_line::route::point_at;

use crate::background::PhotoSource;
use crate::model::{Item, Layer};
use crate::template::{Choice, SheetTemplate};
use crate::tests::main_route;
use crate::{Options, RouteDrawing, page_format, sheets};

/// A line of the invented drawing, on layer `layer`.
fn line(layer: u32, points: &[[f64; 2]]) -> Polyline {
    Polyline {
        layer,
        color: [200, 0, 0],
        points: points.iter().map(|p| [p[0], p[1], 0.0]).collect(),
        closed: false,
    }
}

/// An invented survey drawing: the route's axis (two long segments), a
/// line 20 m right of the first segment from 100 to 500, one across the
/// route at 300, one carrying the first segment's direction past the
/// bend, one far away, one on a layer switched off; a point 10 m right
/// at 200; a text.
fn survey() -> ohl_background::Drawing {
    ohl_background::Drawing {
        layers: vec![
            DrawingLayer {
                name: "survey".into(),
                color: [200, 0, 0],
                off: false,
            },
            DrawingLayer {
                name: "hidden".into(),
                color: [0, 0, 200],
                off: true,
            },
        ],
        lines: vec![
            line(
                0,
                &[
                    [0.0, 0.0],
                    [640.0, 0.0],
                    [1250.580806867569, 107.6618701534968],
                ],
            ),
            line(0, &[[100.0, -20.0], [500.0, -20.0]]),
            line(0, &[[300.0, -500.0], [300.0, 500.0]]),
            line(0, &[[600.0, 0.0], [1000.0, 0.0]]),
            line(0, &[[5000.0, 5000.0], [5100.0, 5000.0]]),
            line(1, &[[100.0, -5.0], [500.0, -5.0]]),
        ],
        points: vec![Mark {
            layer: 0,
            color: [0, 0, 0],
            at: [200.0, -10.0, 0.0],
        }],
        texts: vec![Label {
            layer: 0,
            color: [255, 255, 255],
            at: [250.0, -5.0, 0.0],
            height: 4.0,
            angle_deg: 0.0,
            text: "Parcel 12".into(),
        }],
        skipped: Default::default(),
    }
}

/// The main route with the survey drawing, taken within 60 m.
fn route_with_survey(o: &Options) -> RouteDrawing {
    let mut d = main_route(o);
    d.add_background_drawing("survey", &survey(), 60.0);
    d
}

/// The built-in template with `extra` appended.
fn template_with(extra: &str) -> Arc<SheetTemplate> {
    Arc::new(
        SheetTemplate::parse(
            &format!("{}\n{extra}\n", crate::template::BUILTIN),
            None,
            &[],
        )
        .unwrap(),
    )
}

#[test]
fn a_drawing_is_straightened_along_the_route() {
    let d = route_with_survey(&Options::default());
    let b = &d.background[0];
    assert_eq!(b.name, "survey");
    // The axis: on the axis from 0 to the end, through the bend.
    let axis: Vec<&Vec<[f64; 2]>> = b
        .lines
        .iter()
        .map(|(l, _)| l)
        .filter(|l| l.iter().all(|p| p[1].abs() < 1e-6))
        .collect();
    let from = axis
        .iter()
        .flat_map(|l| l.iter())
        .map(|p| p[0])
        .fold(f64::INFINITY, f64::min);
    let to = axis
        .iter()
        .flat_map(|l| l.iter())
        .map(|p| p[0])
        .fold(0.0, f64::max);
    assert!(
        from.abs() < 1e-6 && (to - d.length).abs() < 1e-6,
        "{from} {to}"
    );
    // 20 m right of the first segment: at offset 20 from 100 to 500.
    let right = b
        .lines
        .iter()
        .find(|(l, _)| l.iter().all(|p| (p[1] - 20.0).abs() < 1e-6))
        .expect("the line 20 m right");
    assert!(
        (right.0[0][0] - 100.0).abs() < 1e-6 && (right.0.last().unwrap()[0] - 500.0).abs() < 1e-6
    );
    // Across the route at 300: cut at ±60 m.
    let across = b
        .lines
        .iter()
        .find(|(l, _)| l.iter().all(|p| (p[0] - 300.0).abs() < 1e-6))
        .expect("the line across");
    let ends = [across.0[0][1], across.0.last().unwrap()[1]];
    assert!(
        (ends[0] - 60.0).abs() < 1e-6 && (ends[1] + 60.0).abs() < 1e-6,
        "{ends:?}"
    );
    // Straight on past the bend: right of the second segment, by
    // (x − 640)·sin 10° at station 640 + (x − 640)·cos 10°; cut at 60 m.
    let (s10, c10) = (
        107.662f64.atan2(610.581).sin(),
        107.662f64.atan2(610.581).cos(),
    );
    let past = b
        .lines
        .iter()
        .map(|(l, _)| l)
        .find(|l| l.iter().any(|p| p[0] > 700.0 && p[1] > 10.0))
        .expect("the line past the bend");
    let at = past
        .iter()
        .min_by(|a, b| {
            (a[0] - (640.0 + 160.0 * c10))
                .abs()
                .total_cmp(&(b[0] - (640.0 + 160.0 * c10)).abs())
        })
        .unwrap();
    assert!((at[0] - (640.0 + 160.0 * c10)).abs() < 1.0, "{at:?}");
    assert!((at[1] - 160.0 * s10).abs() < 0.2, "{at:?} {}", 160.0 * s10);
    assert!(past.iter().all(|p| p[1] <= 60.0 + 1e-6));
    assert!((past.last().unwrap()[1] - 60.0).abs() < 1e-6);
    // Far away and switched off: left out. The point and the text kept.
    assert_eq!(b.lines.len(), axis.len() + 3, "{:?}", b.lines.len());
    assert!(
        b.lines
            .iter()
            .all(|(l, _)| l.iter().all(|p| (p[1] - 5.0).abs() > 1e-3))
    );
    assert_eq!(b.points.len(), 1);
    assert!((b.points[0].0[0] - 200.0).abs() < 1e-6 && (b.points[0].0[1] - 10.0).abs() < 1e-6);
    assert_eq!(b.texts[0].text, "Parcel 12");
}

/// The plan panel's rectangle and its axis's height on sheet `items`.
fn plan_frame(items: &[Item]) -> ([f64; 4], f64) {
    let mut out = None;
    for i in items {
        if let Item::Clip(c) = i {
            for x in &c.items {
                if let Item::Path(p) = x
                    && p.layer == Layer::Centreline
                {
                    out = Some((c.rect, p.points[0][1]));
                }
            }
        }
    }
    out.expect("a plan with its axis")
}

/// The background paths of a sheet, with their clip rectangle.
fn background_paths(items: &[Item]) -> Vec<(Vec<[f64; 2]>, [f64; 4])> {
    let mut out = Vec::new();
    for i in items {
        if let Item::Clip(c) = i {
            for x in &c.items {
                if let Item::Path(p) = x
                    && p.layer == Layer::BackgroundDrawings
                {
                    out.push((p.points.clone(), c.rect));
                }
            }
        }
    }
    out
}

#[test]
fn the_drawings_lie_under_the_plan_on_their_layer() {
    let o = Options {
        template: template_with("[plan_background]\ndrawings = true\ntexts = true\npoints = true"),
        ..Options::default()
    };
    let d = route_with_survey(&o);
    let s = sheets(&d, &o, &page_format("A3").unwrap());
    assert!(s.len() >= 2);
    let first = &s[0].drawing;
    let (rect, axis) = plan_frame(&first.items);
    let paths = background_paths(&first.items);
    assert!(!paths.is_empty());
    // Clipped to the plan panel.
    for (p, r) in &paths {
        assert_eq!(*r, rect);
        assert!(p.iter().all(|q| crate::model::inside(
            *q,
            [r[0] - 1e-9, r[1] - 1e-9, r[2] + 2e-9, r[3] + 2e-9]
        )));
    }
    // On the axis, along the whole panel; 20 m right: 10 mm below it
    // (1:2000).
    assert!(
        paths
            .iter()
            .any(|(p, _)| p.iter().all(|q| (q[1] - axis).abs() < 1e-9)
                && (p[0][0] - rect[0]).abs() < 1e-6
                && (p.last().unwrap()[0] - rect[0] - rect[2]).abs() < 1e-6)
    );
    assert!(
        paths
            .iter()
            .any(|(p, _)| p.iter().all(|q| (q[1] - axis - 10.0).abs() < 1e-9))
    );
    // Under everything else: before the grid.
    let at = |pred: &dyn Fn(&Item) -> bool| first.items.iter().position(pred).unwrap();
    let background = at(
        &|i| matches!(i, Item::Clip(c) if c.items.iter().any(|x| matches!(x, Item::Path(p) if p.layer == Layer::BackgroundDrawings))),
    );
    let grid = at(&|i| matches!(i, Item::Path(p) if p.layer == Layer::Grid));
    assert!(background < grid);
    // The point (a dot), the text (white drawn in the pen's grey), the
    // legend; SVG and DXF on the layer.
    let svg = first.svg();
    assert!(svg.contains(r#"class="OHL_BACKGROUND_DRAWINGS""#));
    assert!(svg.contains(">Parcel 12</text>"));
    let dxf = crate::dxf::write(first);
    assert!(dxf.contains("\nOHL_BACKGROUND_DRAWINGS\n"));
    // A drawing named: only that one.
    let other = Options {
        template: template_with("[plan_background]\ndrawings = [\"cadastre\"]"),
        ..Options::default()
    };
    assert_eq!(
        other.plan_drawings(),
        Choice::Named(vec!["cadastre".into()])
    );
    let s2 = sheets(&d, &other, &page_format("A3").unwrap());
    assert!(background_paths(&s2[0].drawing.items).is_empty());
}

#[test]
fn sheets_without_the_option_are_as_before() {
    let o = Options::default();
    let plain = main_route(&o);
    let with = route_with_survey(&o);
    let a3 = page_format("A3").unwrap();
    let before: Vec<String> = sheets(&plain, &o, &a3)
        .iter()
        .map(|s| s.drawing.svg())
        .collect();
    let after: Vec<String> = sheets(&with, &o, &a3)
        .iter()
        .map(|s| s.drawing.svg())
        .collect();
    assert_eq!(before, after);
    assert!(!after[0].contains("OHL_BACKGROUND"));
    // The template asks, the export switches them off: as before too.
    let off = Options {
        template: template_with("[plan_background]\ndrawings = true\northophoto = true"),
        background_drawings: Some(false),
        background_photo: Some(false),
        ..Options::default()
    };
    let mut with_photo = route_with_survey(&off);
    with_photo.set_photo("Ortho", Some(Arc::new(Gradient)));
    let switched: Vec<String> = sheets(&with_photo, &off, &a3)
        .iter()
        .map(|s| s.drawing.svg())
        .collect();
    assert_eq!(before, switched);
    let pdf_before = crate::pdf::write(
        &sheets(&plain, &o, &a3)
            .iter()
            .map(|s| s.drawing.clone())
            .collect::<Vec<_>>()
            .iter()
            .collect::<Vec<_>>(),
        "t",
    );
    let pdf_after = crate::pdf::write(
        &sheets(&with_photo, &off, &a3)
            .iter()
            .map(|s| s.drawing.clone())
            .collect::<Vec<_>>()
            .iter()
            .collect::<Vec<_>>(),
        "t",
    );
    assert_eq!(pdf_before, pdf_after);
}

/// An invented orthophoto: red rises along x (0 at x = 0, 255 at
/// x = 1275), green across (0 at y = −150, 255 at y = 105), blue 100;
/// none beyond x = 1300.
struct Gradient;

impl Gradient {
    fn at(p: [f64; 2]) -> [u8; 3] {
        [
            (p[0] / 5.0).clamp(0.0, 255.0).round() as u8,
            (p[1] + 150.0).clamp(0.0, 255.0).round() as u8,
            100,
        ]
    }
}

impl PhotoSource for Gradient {
    fn sample(&self, points: &[[f64; 2]], _: f64, out: &mut [Option<[u8; 3]>]) {
        for (p, o) in points.iter().zip(out.iter_mut()) {
            if p[0] <= 1300.0 {
                *o = Some(Self::at(*p));
            }
        }
    }
}

#[test]
fn an_orthophoto_is_resampled_along_the_route() {
    let o = Options {
        template: template_with(
            "[plan_background]\northophoto = true\nveil = 0\ndpi = 100\nquality = 95",
        ),
        ..Options::default()
    };
    let mut d = main_route(&o);
    d.set_photo("Ortho", Some(Arc::new(Gradient)));
    let s = sheets(&d, &o, &page_format("A3").unwrap());
    let sheet = &s[1].drawing;
    let (rect, axis) = plan_frame(&sheet.items);
    let image = sheet
        .flat()
        .into_iter()
        .find_map(|(i, clip)| match i {
            Item::Image(x) if x.layer == Layer::BackgroundPhoto => Some((x.clone(), clip)),
            _ => None,
        })
        .expect("the orthophoto");
    let (img, clip) = image;
    assert_eq!(clip, Some(rect));
    assert_eq!([img.at[0], img.at[1], img.width, img.height], rect);
    let [w, h] = img.pixels.unwrap();
    // 100 dpi.
    assert!(
        (f64::from(w) - (rect[2] * 100.0 / 25.4).ceil()).abs() < 0.5,
        "{w}"
    );
    let jpeg = crate::unbase64(img.href.strip_prefix("data:image/jpeg;base64,").unwrap()).unwrap();
    let pixels = image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg)
        .unwrap()
        .to_rgb8();
    assert_eq!(pixels.dimensions(), (w, h));
    // The pixel at station s and offset o shows the photo at the point
    // of the ground there (1:2000: 0.5 mm per m).
    let (s0, s1) = (s[1].from, s[1].to);
    for (st, off) in [
        (s0 + 30.0, 0.0),
        (0.5 * (s0 + s1), 20.0),
        (s1 - 30.0, -25.0),
    ] {
        let x = rect[0] + (st - s0) * 0.5;
        let y = axis + off * 0.5;
        let (i, j) = (
            ((x - rect[0]) / rect[2] * f64::from(w)) as u32,
            ((y - rect[1]) / rect[3] * f64::from(h)) as u32,
        );
        let got = pixels.get_pixel(i, j).0;
        let want = Gradient::at(point_at(&d.centreline, st, off));
        assert!(
            (0..3).all(|k| got[k].abs_diff(want[k]) <= 8),
            "at {st} {off}: {got:?} ≠ {want:?}"
        );
    }
    // SVG, PDF (the JPEG as it is), PNG; DXF says it has none.
    let svg = sheet.svg();
    assert!(svg.contains(r#"preserveAspectRatio="none" class="OHL_BACKGROUND_PHOTO""#));
    let pdf = crate::pdf::write(&[sheet], "t");
    crate::pdf::read_back(&pdf).unwrap();
    assert!(pdf.windows(10).any(|w| w == b"/DCTDecode"));
    let dxf = crate::dxf::write(sheet);
    assert!(dxf.contains("orthophoto(s) of the plan not written"));
    let (png, _) = crate::png::render(sheet, 30.0).unwrap();
    assert!(png.starts_with(b"\x89PNG"));
    // A coarser resolution asked by the export (the preview): fewer
    // pixels.
    let coarse = Options {
        background_dpi: Some(25.0),
        ..o.clone()
    };
    let s = sheets(&d, &coarse, &page_format("A3").unwrap());
    let small = s[1]
        .drawing
        .flat()
        .into_iter()
        .find_map(|(i, _)| match i {
            Item::Image(x) if x.layer == Layer::BackgroundPhoto => x.pixels,
            _ => None,
        })
        .unwrap();
    assert!(small[0] * 3 < w, "{small:?}");
    // The veil: 60 % white by default.
    let veiled = Options {
        template: template_with("[plan_background]\northophoto = true\ndpi = 50"),
        ..Options::default()
    };
    assert_eq!(veiled.template.plan_background.veil, 0.6);
}

#[test]
fn the_template_keys_are_checked() {
    let t = template_with(
        "[plan_background]\ndrawings = [\"a\", \"b\"]\northophoto = [\"Ortho 2024\"]\nown_colours = true\npen = { colour = \"#444444\", width = \"0.2 mm\" }",
    );
    let b = &t.plan_background;
    assert_eq!(b.drawings, Choice::Named(vec!["a".into(), "b".into()]));
    assert!(b.orthophoto.takes("Ortho 2024") && !b.orthophoto.takes("other"));
    assert!(b.own_colours);
    assert_eq!(b.dpi, 200.0);
    for (bad, why) in [
        ("veil = 2", "veil"),
        ("dpi = 10", "dpi"),
        ("quality = 0", "quality"),
        ("drawings = [\"\"]", "may not be empty"),
        ("photo = true", "unknown field"),
        ("pen = { colour = \"grey\" }", "#rrggbb"),
    ] {
        let e = SheetTemplate::parse(
            &format!("{}\n[plan_background]\n{bad}\n", crate::template::BUILTIN),
            None,
            &[],
        )
        .unwrap_err();
        assert!(e.contains(why), "{bad}: {e}");
    }
    // The export's switch over the template's choice.
    let o = Options {
        template: t,
        background_photo: Some(false),
        background_drawings: Some(true),
        ..Options::default()
    };
    assert_eq!(o.plan_photos(), Choice::Off);
    assert_eq!(
        o.plan_drawings(),
        Choice::Named(vec!["a".into(), "b".into()])
    );
    let plain = Options {
        background_drawings: Some(true),
        ..Options::default()
    };
    assert_eq!(plain.plan_drawings(), Choice::All);
    assert_eq!(Options::default().plan_photos(), Choice::Off);
}

/// With a corridor the plan's backgrounds stop at its edges (the area
/// shown), not at the panel's.
#[test]
fn the_backgrounds_stop_at_the_corridor() {
    let o = Options {
        template: template_with("[plan_background]\northophoto = true\nveil = 0\ndpi = 50"),
        corridor: Some(20.0),
        ..Options::default()
    };
    let mut d = main_route(&o);
    d.set_photo("Ortho", Some(Arc::new(Gradient)));
    let s = sheets(&d, &o, &page_format("A3").unwrap());
    let sheet = &s[1].drawing;
    let (rect, _) = plan_frame(&sheet.items);
    let img = sheet
        .flat()
        .into_iter()
        .find_map(|(i, _)| match i {
            Item::Image(x) if x.layer == Layer::BackgroundPhoto => Some(x.clone()),
            _ => None,
        })
        .expect("the orthophoto");
    // ±20 m at 1:2000 across: 20 mm high, inside the panel.
    let across = 1000.0 / o.plan_offset_scale.unwrap_or(o.horizontal_scale);
    assert!((img.height - 40.0 * across).abs() < 1e-6, "{}", img.height);
    assert!(img.height < rect[3]);
    assert!(img.at[1] > rect[1] && img.at[1] + img.height < rect[1] + rect[3]);
}

/// The key map: the project's drawings and orthophotos under it north up
/// when asked (none by default), the sheet's box in its own pen, the
/// structures' names apart from each other and inside the map.
#[test]
fn the_key_map_has_backgrounds_a_box_and_names_apart() {
    let key = "[key_map]\nplace = \"bottom\"\nwidth = \"110 mm\"\nheight = \"70 mm\"\n\
               structures = true\nbox = { colour = \"#e00000\", width = \"0.35 mm\" }\n";
    let on = Options {
        template: template_with(&format!(
            "{key}drawings = true\northophoto = true\nveil = 0"
        )),
        ..Options::default()
    };
    let off = Options {
        template: template_with(key),
        ..Options::default()
    };
    let mut d = main_route(&on);
    d.add_key_map_drawing("survey", &survey());
    d.set_photo("Ortho", Some(Arc::new(Gradient)));
    // The far line (5 km away) and the hidden layer left out.
    assert_eq!(d.key_background[0].lines.len(), 4);
    let key_items = |o: &Options| {
        let s = sheets(&d, o, &page_format("A1").unwrap());
        s[0].drawing
            .flat()
            .into_iter()
            .filter(|(i, _)| match i {
                Item::Path(p) => p.layer == Layer::KeyMap,
                Item::Image(x) => x.layer == Layer::KeyMap,
                Item::Text(t) => t.layer == Layer::KeyMap,
                _ => false,
            })
            .map(|(i, _)| i.clone())
            .collect::<Vec<_>>()
    };
    let with = key_items(&on);
    let without = key_items(&off);
    let images = |v: &[Item]| v.iter().filter(|i| matches!(i, Item::Image(_))).count();
    assert_eq!(images(&with), 1);
    assert_eq!(images(&without), 0);
    let paths = |v: &[Item]| v.iter().filter(|i| matches!(i, Item::Path(_))).count();
    assert!(paths(&with) > paths(&without));
    // The box in its pen.
    assert!(without.iter().any(|i| matches!(i, Item::Path(p)
        if p.pen.color == crate::model::Color(0xe0, 0, 0) && (p.pen.width - 0.35).abs() < 1e-9)));
    // Names: some, none overlapping another.
    let boxes: Vec<[f64; 4]> = without
        .iter()
        .filter_map(|i| match i {
            Item::Text(t) if t.text.starts_with('T') => {
                let w = 0.6 * t.size * t.text.chars().count() as f64;
                let x0 = if t.anchor == crate::model::Anchor::End {
                    t.at[0] - w
                } else {
                    t.at[0]
                };
                Some([x0, t.at[1] - t.size, x0 + w, t.at[1]])
            }
            _ => None,
        })
        .collect();
    let ids: Vec<&str> = d.structures.iter().map(|m| m.id.as_str()).collect();
    let texts: Vec<&str> = without
        .iter()
        .filter_map(|i| match i {
            Item::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect();
    assert!(!boxes.is_empty(), "{ids:?} {texts:?}");
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            assert!(
                a[2] < b[0] || a[0] > b[2] || a[3] < b[1] || a[1] > b[3],
                "{a:?} {b:?}"
            );
        }
    }
}
