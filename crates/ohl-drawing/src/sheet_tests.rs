// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Regression tests of the sheets drawn with a template (2026-10-20):
//! the furniture inside the frame's padding, empty fields, the template's
//! words, the structures band row's format, the conductor-clearance label
//! clear of the structures, LiDAR dots, the corridor's sides and the
//! centreline.

use std::path::Path;
use std::sync::Arc;

use crate::model::{Anchor, Color, Item, Layer};
use crate::template::SheetTemplate;
use crate::tests::{main_route, solved};
use crate::{
    ConductorClearance, Options, Revision, SheetFields, SheetInfo, curves, page_format, sheets,
};

fn examples() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// The example template's text.
fn example_text() -> String {
    std::fs::read_to_string(examples().join("sheet-templates/example-a1.ohlsheet")).unwrap()
}

/// A template from `text`, its images next to the example.
fn template(text: &str) -> Arc<SheetTemplate> {
    Arc::new(SheetTemplate::parse(text, Some(&examples().join("sheet-templates")), &[]).unwrap())
}

/// The options of the example sheets with template `t`: 1:1000 / 1:200,
/// the clearance line, the lines below the lowest conductor, the fields.
fn options(t: Arc<SheetTemplate>) -> Options {
    let (line, ..) = solved();
    let rules =
        ohl_clearance::RuleSet::load(&examples().join("synthetic-line/clearance-rules.ohlr"))
            .unwrap()
            .rules;
    let specs: Vec<ConductorClearance> = curves::conductor_clearances(&line, &rules)
        .0
        .into_iter()
        .map(|c| ConductorClearance {
            cases: Vec::new(),
            ..c
        })
        .collect();
    let mut fields = SheetFields::default();
    fields.values.insert("project_no".into(), "EX-1".into());
    fields
        .revisions
        .push(Revision::parse("A;2026-10-20;first issue;AB;CD"));
    Options {
        horizontal_scale: 1000.0,
        clearance: Some(7.0),
        fields,
        info: SheetInfo {
            line: "Synthetic".into(),
            date: "2026-10-20".into(),
            ..SheetInfo::default()
        },
        conductor_clearance: specs,
        ..Options::default()
    }
    .with_template(t)
}

/// The left and right ends of text `t` on paper (Helvetica's widths).
fn text_span(t: &crate::model::Text) -> (f64, f64) {
    let w = crate::pdf::text_width(&t.text, t.size, t.bold);
    match t.anchor {
        Anchor::Start => (t.at[0], t.at[0] + w),
        Anchor::Middle => (t.at[0] - w / 2.0, t.at[0] + w / 2.0),
        Anchor::End => (t.at[0] - w, t.at[0]),
    }
}

/// Regression (sheet defect 1): the title block, the revision table and
/// the legend lie inside the frame's padding — with the example template
/// and with a title block of 35/55/45/45 mm and 7 rows (180 mm, bottom
/// right, revisions and legend on it) — every text whole inside it (the
/// right-aligned values not cut by the frame), every line of theirs too.
#[test]
fn the_furniture_stays_inside_the_frame_padding() {
    let wide = example_text()
        .replace(
            "columns = [\"30 mm\", \"60 mm\", \"45 mm\", \"45 mm\"]",
            "columns = [\"35 mm\", \"55 mm\", \"45 mm\", \"45 mm\"]",
        )
        .replace(
            "rows = [\"20 mm\", \"8 mm\", \"8 mm\", \"8 mm\", \"8 mm\", \"8 mm\"]",
            "rows = [\"20 mm\", \"8 mm\", \"8 mm\", \"8 mm\", \"8 mm\", \"8 mm\", \"8 mm\"]",
        );
    assert_ne!(wide, example_text());
    for text in [example_text(), wide] {
        let t = template(&text);
        let o = options(t.clone());
        let page = t.page().unwrap();
        let d = main_route(&o);
        let s = sheets(&d, &o, &page);
        // A1 landscape, margins 20/10/10/10 mm, padding 6 mm.
        let inner = [26.0, 16.0, 841.0 - 30.0 - 12.0, 594.0 - 20.0 - 12.0];
        let (right, bottom) = (inner[0] + inner[2], inner[1] + inner[3]);
        for sheet in &s {
            let mut texts = 0;
            for (item, _) in sheet.drawing.flat() {
                match item {
                    Item::Text(x)
                        if matches!(
                            x.layer,
                            Layer::Title
                                | Layer::TitleFields
                                | Layer::Revisions
                                | Layer::Legend
                                | Layer::KeyMap
                        ) =>
                    {
                        texts += 1;
                        let (a, b) = text_span(x);
                        assert!(
                            a >= inner[0] - 1e-6 && b <= right + 1e-6,
                            "{:?} from {a:.1} to {b:.1} mm, the padding at {right:.1}",
                            x.text
                        );
                        assert!(x.at[1] <= bottom + 1e-6, "{:?} below", x.text);
                    }
                    Item::Path(p) if matches!(p.layer, Layer::Revisions | Layer::Legend) => {
                        for q in &p.points {
                            assert!(
                                q[0] >= inner[0] - 1e-6
                                    && q[0] <= right + 1e-6
                                    && q[1] <= bottom + 1e-6,
                                "{:?} at {q:?}",
                                p.layer
                            );
                        }
                    }
                    _ => {}
                }
            }
            assert!(texts > 20);
            let svg = sheet.svg();
            assert!(svg.contains("1:1000 / 1:200"));
        }
    }
}

/// A text that does not fit its cell is made smaller (to 40 %), a
/// revision cell's shortened with "…".
#[test]
fn texts_fit_their_cells() {
    // One line's height (3 mm): smaller, on one line.
    let (s, lines) = crate::furniture::fit_text("1:1000 / 1:200", 2.5, false, 10.0, 3.0);
    assert!((1.0..2.5).contains(&s), "{s}");
    assert_eq!(lines.len(), 1);
    let widest = lines
        .iter()
        .map(|l| crate::pdf::text_width(l, s, false))
        .fold(0.0, f64::max);
    assert!(widest <= 10.0 + 1e-9);
    let (s, t) =
        crate::furniture::fit_line("a very long description of a revision", 2.0, false, 12.0);
    assert!((s - 1.2).abs() < 1e-9 && t.ends_with('…'), "{s} {t}");
    assert!(crate::pdf::text_width(&t, s, false) <= 12.0);
    // Made smaller only, when that is enough.
    let (s, t) =
        crate::furniture::fit_line("a very long description of a revision", 2.0, false, 30.0);
    assert!(s < 2.0 && s > 1.2 && !t.ends_with('…'), "{s} {t}");
    assert_eq!(
        crate::furniture::fit_line("A", 2.0, false, 20.0),
        (2.0, "A".into())
    );
}

/// Regression (sheet defect 2): `{height_datum}` comes from the
/// coordinate system (a compound EPSG code's vertical part, a proj4
/// geoid); unknown, "unknown datum" or the template's `[if_empty]`; any
/// other field that comes out empty "–" (or the template's `"*"`), but a
/// station range's ends and the title lines may say nothing.
#[test]
fn empty_fields_leave_no_gap() {
    assert_eq!(crate::height_datum_of("EPSG:2056+5728"), "EPSG:5728");
    assert_eq!(
        crate::height_datum_of("+proj=somerc +geoidgrids=ch.gtx +units=m"),
        "geoid ch.gtx"
    );
    assert_eq!(crate::height_datum_of("EPSG:2056"), "");
    assert_eq!(crate::height_datum_of("local (invented)"), "");
    let head = "format = \"ohl-sheet-template\"\nversion = 1\n[title_block]\ncolumns = \
                [\"*\"]\nrows = [\"30 mm\"]\n[[title_block.cell]]\nat = [0, 0]\ntext = \
                \"H {height_datum}|C {crs}|D {drawn}|E {ends}|T {title3}\"\n[[field]]\nkey = \
                \"drawn\"\n";
    let svg = |t: &str, crs: &str| {
        let o = Options {
            info: SheetInfo {
                crs: crs.into(),
                ..SheetInfo::default()
            },
            ..Options::default()
        }
        .with_template(template(t));
        let d = main_route(&o);
        sheets(&d, &o, &page_format("A3").unwrap())[0].svg()
    };
    let s = svg(head, "");
    assert!(
        s.contains("H unknown datum|C unknown coordinate system|D –|E    (T1 – T3)|T <"),
        "{}",
        &s[s.find("H ").unwrap()..][..120]
    );
    let s = svg(head, "EPSG:2056+5728");
    assert!(s.contains("H EPSG:5728|C EPSG:2056+5728|"));
    // The project's height datum first (2026-10-21), over the coordinate
    // system's vertical part.
    let o = Options {
        info: SheetInfo {
            crs: "EPSG:2056+5728".into(),
            height_datum: "LN02 (EPSG:5728)".into(),
            ..SheetInfo::default()
        },
        ..Options::default()
    }
    .with_template(template(head));
    let s = sheets(&main_route(&o), &o, &page_format("A3").unwrap())[0].svg();
    assert!(s.contains("H LN02 (EPSG:5728)|C EPSG:2056+5728|"));
    let own = format!(
        "{head}[if_empty]\nheight_datum = \"sea level (not given)\"\n\"*\" = \"n/a\"\ntitle3 = \
         \"no title\"\n"
    );
    let s = svg(&own, "");
    assert!(s.contains("H sea level (not given)|C n/a|D n/a|E    (T1 – T3)|T no title<"));
    // A name neither computed nor declared is refused.
    let bad = SheetTemplate::parse(&format!("{head}[if_empty]\nnothing = \"x\""), None, &[]);
    assert!(bad.unwrap_err().contains("if_empty: no field \"nothing\""));
}

/// `text` as an SVG holds it: escaped, but a leading `>` and a trailing
/// `<` (the element's ends) kept.
fn in_svg(text: &str) -> String {
    let core = text.trim_start_matches('>').trim_end_matches('<');
    format!(
        "{}{}{}",
        if text.starts_with('>') { ">" } else { "" },
        crate::model::escape(core),
        if text.ends_with('<') { "<" } else { "" }
    )
}

/// Regression (sheet defect 3): every legend entry the export makes and
/// the panels' titles take the template's words (`[legend.labels]`, with
/// their placeholders; `[layout] plan_title`, `profile_title`); English
/// stays the default. Unknown entries and placeholders are refused.
#[test]
fn the_legend_and_titles_take_the_templates_words() {
    let t = template(&example_text());
    let o = options(t);
    let mut d = main_route(&o);
    d.set_case("t");
    let s = sheets(&d, &o, &page_format("A1").unwrap());
    let svg = s[0].svg();
    for text in [
        "Plan view",
        "Longitudinal profile",
        "centreline (axis of the line)",
        "load case t",
        "support",
        "ground on the centreline",
        "ground + 7 m (minimum clearance)",
        "match line<",
        "corridor ±25 m, near side (right): edge and ground",
        "corridor ±25 m, far side (left): edge and ground",
        "7.5 m under the lowest conductor of",
    ] {
        assert!(svg.contains(&in_svg(text)), "{text}");
    }
    for english in [
        ">Plan<",
        ">Profile<",
        "axis of the line<",
        "weather case",
        "ground on the axis",
        "match line (sheet",
        ">lowest conductor of",
    ] {
        assert!(!svg.contains(&in_svg(english)), "{english}");
    }
    // The built-in template: English.
    let o = Options {
        clearance: Some(7.0),
        ..Options::default()
    };
    let s = sheets(&main_route(&o), &o, &page_format("A1").unwrap());
    let svg = s[0].svg();
    assert!(svg.contains(">Plan<") && svg.contains(">Profile<"));
    let head = "format = \"ohl-sheet-template\"\nversion = 1\n";
    let err = |body: &str| SheetTemplate::parse(&format!("{head}{body}"), None, &[]).unwrap_err();
    assert!(err("[legend.labels]\nnothing = \"x\"").contains("no such entry"));
    assert!(err("[legend.labels]\naxis = \"{case}\"").contains("unknown field {case}"));
    assert!(err("[legend.labels]\nweather_case = \"{x}\"").contains("give {case}"));
}

/// Regression (sheet defect 4): the structures band row writes what its
/// `format` says — the type and extension where there are some, nothing
/// (no gap) where not, two lines at a line break.
#[test]
fn the_structures_band_row_has_a_format() {
    let m = crate::Mark {
        id: "T4".into(),
        station: 640.0,
        base: 500.0,
        top: 532.5,
        type_name: Some("D2 +3".into()),
        type_id: Some("D2".into()),
        extension: Some("+3".into()),
        ..crate::Mark::default()
    };
    let e = |f: &str, m: &crate::Mark| crate::furniture::structure_entry(f, m, 1);
    assert_eq!(e("{name} {type}", &m), ["T4 D2 +3"]);
    assert_eq!(
        e("{name}\n{type_id} ext. {extension}", &m),
        ["T4", "D2 ext. +3"]
    );
    assert_eq!(
        e("{name} ({station}, {height} m)", &m),
        ["T4 (0+640.0, 32.5 m)"]
    );
    let plain = crate::Mark {
        id: "T1".into(),
        ..crate::Mark::default()
    };
    assert_eq!(e("{name} {type}", &plain), ["T1"]);
    assert_eq!(e("{name}\n{type}", &plain), ["T1"]);
    // Drawn: the typed line's types in the row.
    let t = template(&example_text());
    let o = options(t);
    let mut d = main_route(&o);
    d.structures[1].type_name = Some("D2 +3".into());
    let svg = sheets(&d, &o, &page_format("A1").unwrap())[0].svg();
    assert!(svg.contains(">D2 +3<"));
    let err = SheetTemplate::parse(
        "format = \"ohl-sheet-template\"\nversion = 1\n[[band]]\nkind = \"stations\"\nformat = \
         \"{name}\"",
        None,
        &[],
    )
    .unwrap_err();
    assert!(err.contains("for the kind structures"));
    let err = SheetTemplate::parse(
        "format = \"ohl-sheet-template\"\nversion = 1\n[[band]]\nkind = \"structures\"\nformat = \
         \"{nothing}\"",
        None,
        &[],
    )
    .unwrap_err();
    assert!(err.contains("unknown field {nothing}"));
}

/// The boxes of the structures on a sheet: from the panel's top to the
/// bottom, the structure's width (its mast at level 1: 1 mm each side).
fn structure_columns(s: &crate::Sheet) -> Vec<f64> {
    s.drawing
        .flat()
        .iter()
        .filter_map(|(i, _)| match i {
            Item::Path(p) if p.layer == Layer::Structures && p.points.len() == 2 => {
                (p.points[0][0] == p.points[1][0]).then_some(p.points[0][0])
            }
            _ => None,
        })
        .collect()
}

/// Regression (sheet defect 5): the label of the line below the lowest
/// conductor lay over the tower in the middle of the sheet (T2); placed
/// `"auto"` it lies clear of every structure, `"middle"` keeps the old
/// place.
#[test]
fn the_conductor_clearance_label_keeps_clear_of_the_structures() {
    let label = |t: &str| {
        let o = options(template(t));
        let d = main_route(&o);
        let s = sheets(&d, &o, &page_format("A1").unwrap());
        let masts = structure_columns(&s[0]);
        assert!(!masts.is_empty());
        let texts: Vec<(f64, f64)> = s[0]
            .drawing
            .flat()
            .iter()
            .filter_map(|(i, _)| match i {
                Item::Text(x) if x.layer == Layer::ConductorClearance => Some(text_span(x)),
                _ => None,
            })
            .collect();
        assert_eq!(texts.len(), 1);
        let (a, b) = texts[0];
        masts.iter().any(|x| *x > a - 1.0 && *x < b + 1.0)
    };
    let example = example_text();
    assert!(!label(&example), "auto: over a structure");
    let middle = example.replace("label_at = \"auto\"", "label_at = \"middle\"");
    assert!(label(&middle), "middle: the old place, over T2");
    assert!(
        SheetTemplate::parse(
            "format = \"ohl-sheet-template\"\nversion = 1\n[conductor_clearance]\nlabel_at = \
             \"left\"",
            None,
            &[]
        )
        .unwrap_err()
        .contains("give auto, middle, start or end")
    );
}

/// The LiDAR dots seen on a sheet: inside their clip region (the next
/// sheet's, faded beyond the match line, are clipped there).
fn seen_dots(s: &crate::Sheet) -> Vec<crate::model::Circle> {
    s.drawing
        .flat()
        .iter()
        .filter_map(|(i, clip)| match (i, clip) {
            (Item::Circle(c), Some(r))
                if c.layer == Layer::Lidar && crate::model::inside(c.centre, *r) =>
            {
                Some(c.clone())
            }
            _ => None,
        })
        .collect()
}

/// LiDAR points: kept within the corridor (beside the line, not beyond
/// its ends) and of the classes drawn; drawn as dots (one per class and
/// paper cell: what is drawn only), each class in its colour and in the
/// legend by its name; the export's classes over the template's; the
/// plan too when asked.
#[test]
fn lidar_points_are_drawn_within_the_corridor() {
    let text = example_text();
    let t = template(&text);
    let mut o = options(t);
    o.class_names = [
        (2, ("Ground".to_owned(), Some([1, 2, 3]))),
        (5, ("High vegetation".to_owned(), None)),
        (6, ("Building".to_owned(), None)),
        (7, ("Low noise".to_owned(), None)),
    ]
    .into();
    let mut d = main_route(&o);
    let points = vec![
        ([100.0, 10.0, 510.0], 5),   // in, a group style (3-5)
        ([100.0, 10.0, 510.0], 5),   // the same place: one dot
        ([100.0, 10.001, 510.0], 5), // within the same 0.2 mm cell
        ([200.0, -20.0, 505.0], 2),  // in
        ([200.0, 40.0, 505.0], 2),   // beyond the corridor (25 m)
        ([-30.0, 0.0, 505.0], 2),    // before the route's start
        ([300.0, 5.0, 520.0], 7),    // a class not drawn
        ([310.0, 5.0, 515.0], 6),    // in
    ];
    d.set_lidar(points.clone(), &o.lidar_class_list(), o.lidar_half_width());
    assert_eq!(o.lidar_half_width(), 25.0);
    assert_eq!(d.lidar.len(), 5, "{:?}", d.lidar);
    assert!(d.lidar.windows(2).all(|w| w[0].0[0] <= w[1].0[0]));
    let s = sheets(&d, &o, &page_format("A1").unwrap());
    let dots = seen_dots(&s[0]);
    assert_eq!(dots.len(), 3, "{dots:?}");
    // The template's colours (#2e8b57 the group 3-5, #b08850 ground).
    assert!(dots.iter().any(|c| c.fill == Some(Color(0x2e, 0x8b, 0x57))));
    assert!(dots.iter().any(|c| c.fill == Some(Color(0xb0, 0x88, 0x50))));
    let svg = s[0].svg();
    for t in [
        "LiDAR: vegetation (3-5)",
        "LiDAR: Ground (2)",
        "LiDAR: Building (6)",
    ] {
        assert!(svg.contains(t), "{t}");
    }
    // The export's classes over the template's; the plan too.
    let plan_too = template(&text.replace(
        "plan = false                  # seen from above",
        "plan = true",
    ));
    let o2 = Options {
        lidar_classes: Some(vec![2]),
        template: plan_too,
        ..o.clone()
    };
    let mut d2 = main_route(&o2);
    d2.set_lidar(points, &o2.lidar_class_list(), o2.lidar_half_width());
    assert_eq!(d2.lidar.len(), 1);
    let s = sheets(&d2, &o2, &page_format("A1").unwrap());
    assert_eq!(
        seen_dots(&s[0]).len(),
        2,
        "the profile's dot and the plan's"
    );
    assert!(!o2.template.lidar.styles.is_empty());
    assert_eq!(
        crate::template::class_codes("3-5, 7").unwrap(),
        [3, 4, 5, 7]
    );
    assert!(crate::template::class_codes("5-3").is_err());
}

/// The corridor's sides: the ground along both edges in the profile
/// (near: the right by default, the template may say left), each edge in
/// its own pen in the plan, the key map and the profile alike, both in
/// the legend; the ground on the axis always solid, left out only when
/// the template says so; the axis in the plan in the template's pen.
#[test]
fn the_corridor_sides_and_the_centreline() {
    let text = example_text().replace(
        "[layers]                      # pens by layer: width, colour, dash (any of them)",
        "[layers]\nOHL_GROUND = { width = \"0.4 mm\", colour = \"#6b3f12\", dash = \"2 2\" }",
    );
    let text = text.replace(
        "OHL_GROUND = { width = \"0.4 mm\", colour = \"#6b3f12\" }\n",
        "",
    );
    let o = options(template(&text));
    let d = main_route(&o);
    assert_eq!(
        d.corridor_ground.iter().map(|c| c.0).collect::<Vec<_>>(),
        [25.0, -25.0]
    );
    assert!(d.corridor_ground.iter().all(|c| !c.1.is_empty()));
    let near = Color(0x2a, 0x7f, 0x3f);
    let far = Color(0x7a, 0x3f, 0xa0);
    let s = sheets(&d, &o, &page_format("A1").unwrap());
    let flat = s[0].drawing.flat();
    let pens = |layer: Layer| -> Vec<Color> {
        flat.iter()
            .filter_map(|(i, _)| match i {
                Item::Path(p) if p.layer == layer => Some(p.pen.color),
                _ => None,
            })
            .collect()
    };
    for layer in [Layer::Corridor, Layer::GroundSide, Layer::KeyMap] {
        let p = pens(layer);
        assert!(
            p.contains(&near) && p.contains(&far),
            "{}: {p:?}",
            layer.name()
        );
    }
    // The plan: the near (right) edge below the axis on paper.
    let edge_y = |c: Color| {
        flat.iter()
            .find_map(|(i, _)| match i {
                Item::Path(p) if p.layer == Layer::Corridor && p.pen.color == c => {
                    Some(p.points[0][1])
                }
                _ => None,
            })
            .unwrap()
    };
    assert!(edge_y(near) > edge_y(far));
    // The ground on the axis: solid though its layer's pen is dashed.
    let ground: Vec<&crate::model::Path> = flat
        .iter()
        .filter_map(|(i, _)| match i {
            Item::Path(p) if p.layer == Layer::Ground => Some(p),
            _ => None,
        })
        .collect();
    assert!(!ground.is_empty() && ground.iter().all(|p| p.pen.dash.is_none()));
    // The axis in the template's pen (solid black).
    // (The last: the next sheet's, faded, is drawn first.)
    let axis = flat
        .iter()
        .filter_map(|(i, _)| match i {
            Item::Path(p) if p.layer == Layer::Centreline => Some(p.pen.clone()),
            _ => None,
        })
        .next_back()
        .unwrap();
    assert_eq!((axis.color, axis.dash), (Color::BLACK, None));
    // Near side left; no ground on the axis.
    let left = text
        .replace("near_side = \"right\"", "near_side = \"left\"")
        .replace(
            "ground = true                 # the ground",
            "ground = false                # the ground",
        );
    let o = options(template(&left));
    let d = main_route(&o);
    assert_eq!(
        d.corridor_ground.iter().map(|c| c.0).collect::<Vec<_>>(),
        [-25.0, 25.0]
    );
    let s = sheets(&d, &o, &page_format("A1").unwrap());
    let svg = s[0].svg();
    assert!(svg.contains("near side (left)"));
    assert!(
        !s[0]
            .drawing
            .flat()
            .iter()
            .any(|(i, _)| matches!(i, Item::Path(p) if p.layer == Layer::Ground))
    );
    // The built-in template: the axis as before (grey, dash-dot), one
    // corridor entry when no ground is drawn along it.
    let o = Options {
        corridor: Some(20.0),
        ..Options::default()
    };
    let mut d = main_route(&o);
    d.corridor_ground.clear();
    let s = sheets(&d, &o, &page_format("A1").unwrap());
    let axis = s[0]
        .drawing
        .flat()
        .iter()
        .find_map(|(i, _)| match i {
            Item::Path(p) if p.layer == Layer::Centreline => Some(p.pen.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(axis.color, Color(0x99, 0x99, 0x99));
    assert!(axis.dash.is_some());
    assert!(
        SheetTemplate::parse(
            "format = \"ohl-sheet-template\"\nversion = 1\n[corridor]\nnear_side = \"up\"",
            None,
            &[]
        )
        .unwrap_err()
        .contains("give right or left")
    );
}

/// A note over the key map is said.
#[test]
fn a_note_over_the_furniture_is_warned() {
    let text = example_text().replace("at = [\"4 mm\", \"-80 mm\"]", "at = [\"4 mm\", \"-40 mm\"]");
    let o = options(template(&text));
    let d = main_route(&o);
    let s = sheets(&d, &o, &page_format("A1").unwrap());
    assert!(
        s[0].warnings
            .iter()
            .any(|w| w == "note 1 lies over the key map: move it in the template (its `at`)"),
        "{:?}",
        s[0].warnings
    );
    let o = options(template(&example_text()));
    let s = sheets(&main_route(&o), &o, &page_format("A1").unwrap());
    assert!(s[0].warnings.iter().all(|w| !w.starts_with("note ")));
}
