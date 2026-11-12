// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: the field windows' Setup and Results (owner, 2026-10-31,
//! validated): the span table's ticks flipped by a drag across rows, a
//! cut asked at a station computed by the run, Open cut and Show in 3D.

use super::*;
use crate::span_pick::CutAt;

/// A harness on the synthetic line with the Electric field window open
/// (its ground the plane: quick), one circuit at 400 kV.
fn electric() -> Harness<'static, App> {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let circuits = ohl_electrical::magnetic::circuits(&r.model.line);
    r.electric_field.set_voltage(&circuits[0].label(), 400.0);
    r.electric_field.set_tiled(false);
    r.electric_field.show(r.case);
    harness.run();
    harness
}

/// Waits for the electric field's run started.
fn wait(harness: &mut Harness<'_, App>) {
    let started = std::time::Instant::now();
    loop {
        harness.step();
        if let Some(res) = &harness
            .state_mut()
            .ready()
            .unwrap()
            .electric_field
            .job
            .result
        {
            res.as_ref().expect("a field");
            break;
        }
        assert!(started.elapsed().as_secs() < 120, "no field in 120 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    harness.run();
}

/// The centre of the node labelled `label`, scrolled into the window's
/// body first (the wheel over the window).
fn centre_of(harness: &mut Harness<'_, App>, label: &str) -> egui::Pos2 {
    for _ in 0..30 {
        let r = harness.get_by_label(label).rect();
        if r.center().y < 420.0 {
            return r.center();
        }
        harness.event(egui::Event::PointerMoved(egui::pos2(400.0, 400.0)));
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -120.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        harness.run_steps(3);
    }
    panic!("{label} not scrolled into view");
}

/// A press on the first row's tick dragged across two more rows unticks
/// all three (owner: "you drag the mouse across while holding the click
/// to flip the choices"); the others stay ticked.
#[test]
fn a_drag_across_the_ticks_flips_the_rows() {
    let mut harness = electric();
    let (spans, before) = {
        let r = harness.state_mut().ready().unwrap();
        let spans: Vec<(String, String)> = ohl_electrical::cross_section::line_spans(&r.model.line)
            .into_iter()
            .map(|s| (s.from, s.to))
            .collect();
        let before = r.electric_field.ticked_spans(&r.model);
        (spans, before)
    };
    // The table's first rows: the main alignment's first three spans.
    let line = harness.state_mut().ready().unwrap().model.line.clone();
    let groups = crate::span_pick::groups(&line, &spans);
    let rows: Vec<usize> = groups[0].spans.iter().copied().take(3).collect();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|k| before.contains(k)), "ticked at first");
    let label = |k: usize| format!("span {}–{}", spans[k].0, spans[k].1);
    let p0 = centre_of(&mut harness, &label(rows[0]));
    let p1 = harness.get_by_label(&label(rows[1])).rect().center();
    let p2 = harness.get_by_label(&label(rows[2])).rect().center();
    harness.event(egui::Event::PointerMoved(p0));
    harness.step();
    harness.event(egui::Event::PointerButton {
        pos: p0,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.step();
    for p in [p1, p2] {
        harness.event(egui::Event::PointerMoved(p));
        harness.step();
    }
    harness.event(egui::Event::PointerButton {
        pos: p2,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let after = r.electric_field.ticked_spans(&r.model);
    for k in &rows {
        assert!(
            !after.contains(k),
            "{} still ticked; after {after:?} p {p0:?} {p1:?} {p2:?}",
            label(*k)
        );
    }
    assert_eq!(after.len(), before.len() - 3, "the other rows unchanged");
}

/// A cut asked at mid-span in the table is computed by the run on the
/// alignment (at the alignment station of the span's middle), and Results
/// › Open cut opens a cross-section panel on the same plane (owner,
/// 2026-10-31: "cuts are made on the alignment").
#[test]
fn a_cut_station_is_computed_and_opened() {
    let mut harness = electric();
    let ctx = harness.ctx.clone();
    {
        let r = harness.state_mut().ready().unwrap();
        r.electric_field.tick_only(&r.model, 0);
        r.electric_field.add_cut(&r.model, 0, CutAt::Mid);
        r.electric_field.start(&r.model, &ctx);
    }
    wait(&mut harness);
    let (len, at, cut) = {
        let r = harness.state_mut().ready().unwrap();
        let Some(Ok(a)) = &r.electric_field.job.result else {
            panic!("a field")
        };
        assert_eq!(a.field.cuts.len(), 1, "the cut asked");
        let c = &a.field.cuts[0];
        let len = crate::span_pick::span_length(&r.model, &(c.from.clone(), c.to.clone()));
        // At the alignment station of the span's middle.
        let views = r.model.line.alignment_views();
        let (alignment, station) = r
            .model
            .line
            .span_station(&views, &c.from, &c.to, 0.5)
            .unwrap();
        assert_eq!(
            (c.alignment.as_str(), c.station),
            (alignment.as_str(), station)
        );
        (len, c.at, c.values.cut)
    };
    assert!((at - 0.5 * len).abs() < 1.0, "at mid-span: {at} of {len}");
    harness.get_by_label("Open cut").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.xsections.views.len(), 1, "a panel opened");
    assert!(r.visible.contains(&crate::scene::Layer::ElectricField));
    // The panel's plane is the run's cut's: the same numbers.
    let view = r.xsections.views.values().next().unwrap();
    let placed = view.placed.as_ref().expect("placed");
    assert_eq!(placed.plane.origin, cut.origin);
    assert_eq!(placed.plane.right(), cut.right);
}

/// A cut asked in a span on no alignment cannot be placed (cuts are made
/// on the alignment): why is said; the lowest point of a span on one is
/// left to the run, which places it at its alignment station.
#[test]
fn a_cut_on_no_alignment_is_said() {
    let mut harness = electric();
    let r = harness.state_mut().ready().unwrap();
    let line = r.model.line.clone();
    let spans: Vec<(String, String)> = ohl_electrical::cross_section::line_spans(&line)
        .into_iter()
        .map(|s| (s.from, s.to))
        .collect();
    let views = line.alignment_views();
    let placed = crate::span_pick::CutAt::Lowest
        .place(&line, &views, &spans[0], 100.0)
        .unwrap();
    assert!(matches!(
        placed,
        ohl_electrical::magnetic::CutAt::InSpan { fraction: None, .. }
    ));
    let at = crate::span_pick::CutAt::From(25.0)
        .place(&line, &views, &spans[0], 100.0)
        .unwrap();
    assert!(matches!(
        at,
        ohl_electrical::magnetic::CutAt::Station { .. }
    ));
    // The same line without its alignments.
    let mut bare = line.clone();
    bare.alignments.clear();
    let views = bare.alignment_views();
    for chip in [
        crate::span_pick::CutAt::Mid,
        crate::span_pick::CutAt::Lowest,
    ] {
        let e = chip.place(&bare, &views, &spans[0], 100.0).unwrap_err();
        assert!(e.contains("on no alignment"), "{e}");
    }
}

/// A volume ticked in the table: Results › Show in 3D turns the 3D view
/// to its surfaces.
#[test]
fn show_in_3d_turns_the_view_to_the_volume() {
    let mut harness = electric();
    let ctx = harness.ctx.clone();
    {
        let r = harness.state_mut().ready().unwrap();
        r.electric_field.set_views(None, Some(4.0));
        // Span 0 alone, its volume ticked.
        r.electric_field.tick_only(&r.model, 0);
        r.electric_field.start(&r.model, &ctx);
    }
    wait(&mut harness);
    let centre = {
        let r = harness.state_mut().ready().unwrap();
        let Some(Ok(a)) = &r.electric_field.job.result else {
            panic!("a field")
        };
        assert_eq!(a.field.volumes.len(), 1);
        crate::field_results::centre(a.meshes.iter().map(|m| m.1.as_ref())).expect("surfaces")
    };
    harness.get_by_label("Show in 3D").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let camera = r.view3d.camera.expect("a camera");
    let d = (0..3)
        .map(|k| (camera.target[k] - centre[k]).powi(2))
        .sum::<f64>()
        .sqrt();
    assert!(d < 1e-6, "the camera on the volume: {d} m off");
    assert!(r.visible.contains(&crate::scene::Layer::ElectricField));
}

/// A volume unticked in Results leaves the 3D view (owner, 2026-11-01:
/// "a toggle for the views in 3D, not just Show, a tick box"); the run
/// saved in Performed checks and opened again gives the same field and
/// surfaces (owner, 2026-11-01: "the results need to be saveable in the
/// results library for the checks").
#[test]
fn a_field_run_is_hidden_saved_and_opened_again() {
    let model = crate::tests::synthetic_project("electric-field-saved");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    let ctx = harness.ctx.clone();
    {
        let r = harness.state_mut().ready().unwrap();
        let circuits = ohl_electrical::magnetic::circuits(&r.model.line);
        r.electric_field.set_voltage(&circuits[0].label(), 400.0);
        r.electric_field.set_tiled(false);
        r.electric_field.show(r.case);
        r.electric_field.set_views(None, Some(4.0));
        r.electric_field.tick_only(&r.model, 0);
        r.electric_field.start(&r.model, &ctx);
    }
    wait(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        r.electric_field.hidden.insert(("Volume", 0));
        let o = r.electric_field.overlay_change(&r.model).expect("redrawn");
        assert!(o.surfaces.is_empty(), "the volume unticked is not drawn");
        r.electric_field.hidden.clear();
        let o = r.electric_field.overlay_change(&r.model).expect("redrawn");
        assert!(!o.surfaces.is_empty(), "ticked again: drawn");
        r.electric_field.set_save_name("field 1");
    }
    harness.run();
    harness.get_by_label("Save in Performed checks").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let saved = r.model.session().unwrap().report("field 1").expect("saved");
    assert_eq!(saved.check, crate::field_saved::ELECTRIC_CHECK_ID);
    let Some(Ok(before)) = &r.electric_field.job.result else {
        panic!("a field")
    };
    let (profiles, cuts, meshes) = (
        before.field.profiles.clone(),
        before.field.cuts.len(),
        before.meshes.clone(),
    );
    r.electric_field = Default::default();
    r.check_library_out(crate::check_library::Out {
        open: Some(saved),
        ..Default::default()
    });
    let Some(Ok(back)) = &r.electric_field.job.result else {
        panic!("read back")
    };
    assert_eq!(back.field.profiles, profiles);
    assert_eq!(back.field.cuts.len(), cuts);
    assert_eq!(back.meshes.len(), meshes.len());
    for (a, b) in back.meshes.iter().zip(&meshes) {
        assert_eq!(a.0, b.0);
        assert_eq!(a.1.triangles, b.1.triangles);
    }
    assert!(r.electric_field.open);
    assert_eq!(r.electric_field.out_of_date(&r.model), None);
}

/// The magnetic field's table too (owner, 2026-11-01: 2D the cuts, 3D the
/// volumes, no isolines from above): a cut asked at its lowest point and
/// 100 m from the first structure in the span ticked alone — the 2D run's,
/// listed in Results; then its volume — the 3D run's, alone.
#[test]
fn the_magnetic_table_asks_cuts_volumes_and_grids() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    let ctx = harness.ctx.clone();
    let run = |h: &mut Harness<'_, App>| {
        let started = std::time::Instant::now();
        loop {
            h.step();
            let r = h.state_mut().ready().unwrap();
            if r.magnetic_field.job.result.is_some() && !r.magnetic_field.job.running() {
                break;
            }
            assert!(started.elapsed().as_secs() < 120, "no field in 120 s");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        h.run();
    };
    {
        let r = harness.state_mut().ready().unwrap();
        let circuits = ohl_electrical::magnetic::circuits(&r.model.line);
        r.magnetic_field.set_current(&circuits[0].label(), 800.0);
        r.magnetic_field.set_views(None, None, None);
        r.magnetic_field.tick_only(&r.model, 0);
        assert_eq!(r.magnetic_field.ticked_spans(&r.model), vec![0]);
        r.magnetic_field.add_cut(&r.model, 0, CutAt::Lowest);
        r.magnetic_field.add_cut(&r.model, 0, CutAt::From(100.0));
        r.magnetic_field.show(r.case);
        r.magnetic_field.start(&r.model, &ctx);
    }
    run(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Ok(a)) = &r.magnetic_field.job.result else {
            panic!("a field")
        };
        assert_eq!(a.field.cuts.len(), 2, "the two cuts asked");
        // 100 m from the first structure: at that place's alignment
        // station (the alignment runs through the structures).
        assert!(
            (a.field.cuts[1].at - 100.0).abs() < 1e-3,
            "{}",
            a.field.cuts[1].at
        );
        assert!(!a.field.cuts[0].alignment.is_empty());
        assert!(a.field.volumes.is_empty() && a.field.grids.is_empty());
        assert!(!a.field.profiles.is_empty(), "the profiles too");
    }
    assert_eq!(harness.query_all_by_label("Open cut").count(), 2);
    {
        let r = harness.state_mut().ready().unwrap();
        r.magnetic_field.set_views(None, None, Some(4.0));
        r.magnetic_field.start(&r.model, &ctx);
    }
    run(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Ok(a)) = &r.magnetic_field.job.result else {
            panic!("a field")
        };
        assert_eq!(a.field.volumes.len(), 1);
        assert!(a.field.cuts.is_empty() && a.field.profiles.is_empty());
    }
    assert_eq!(harness.query_all_by_label("Show in 3D").count(), 1);
}

/// A weather case per system (owner, 2026-11-01: "earth wire at 10 °C
/// and conductors at 40 °C … best would be per system"): the line's
/// system in another case than the window's moves its wires, so the
/// profiles change; the run says so in its notes.
#[test]
fn a_system_takes_its_own_weather_case() {
    let mut harness = electric();
    let ctx = harness.ctx.clone();
    let largest = |h: &mut Harness<'_, App>| -> (f64, Vec<String>) {
        wait(h);
        let r = h.state_mut().ready().unwrap();
        let Some(Ok(a)) = &r.electric_field.job.result else {
            panic!("a field")
        };
        (
            a.field
                .profiles
                .iter()
                .map(|p| p.max_kv_m)
                .fold(0.0, f64::max),
            a.field.notes.clone(),
        )
    };
    let other = {
        let r = harness.state_mut().ready().unwrap();
        r.electric_field.set_views(None, None);
        r.electric_field.start(&r.model, &ctx);
        let shown = r.model.cases[r.electric_field.case].case.name.clone();
        r.model
            .cases
            .iter()
            .map(|c| c.case.name.clone())
            .find(|n| *n != shown && n.contains("max"))
            .expect("another case")
    };
    let (before, _) = largest(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        let system = r.model.line.systems[0].id.clone();
        r.electric_field.job.result = None;
        r.electric_field.set_system_case(&system, &other);
        r.electric_field.start(&r.model, &ctx);
    }
    let (after, notes) = largest(&mut harness);
    assert!(
        notes
            .iter()
            .any(|n| n.starts_with("per system:") && n.contains(&other)),
        "{notes:?}"
    );
    assert!(
        (after - before).abs() > 1e-3 * before,
        "{before} and {after}"
    );
}

/// A weather case the app has not solved yet (it solves a case when it is
/// shown) is solved for the run, in both field windows (2026-10-31: the
/// run took no wire and gave 0 kV/m, listing every section as not
/// solved); sections a case does not hold are said in one warning.
#[test]
fn a_case_not_solved_yet_is_solved_for_the_run() {
    let mut harness = electric();
    let ctx = harness.ctx.clone();
    {
        let r = harness.state_mut().ready().unwrap();
        let k = r.electric_field.case;
        let case = r.model.cases[k].case.clone();
        r.model.cases[k] = crate::model::Solved::pending(case.clone());
        r.electric_field.start(&r.model, &ctx);
        let circuits = ohl_electrical::magnetic::circuits(&r.model.line);
        r.magnetic_field.set_current(&circuits[0].label(), 800.0);
        r.magnetic_field.show(k);
        r.magnetic_field.start(&r.model, &ctx);
    }
    let started = std::time::Instant::now();
    loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if r.magnetic_field.job.result.is_some() && r.electric_field.job.result.is_some() {
            break;
        }
        assert!(started.elapsed().as_secs() < 120, "no fields");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let r = harness.state_mut().ready().unwrap();
    let Some(Ok(e)) = &r.electric_field.job.result else {
        panic!("an electric field")
    };
    assert!(e.field.profiles.iter().all(|p| p.max_kv_m > 0.0));
    assert!(
        !e.field.notes.iter().any(|n| n.contains("not solved")),
        "{:?}",
        e.field.notes
    );
    let Some(Ok(b)) = &r.magnetic_field.job.result else {
        panic!("a magnetic field")
    };
    assert!(b.field.profiles.iter().any(|p| p.max_ut > 0.0));
    assert!(
        !b.field.notes.iter().any(|n| n.contains("not solved")),
        "{:?}",
        b.field.notes
    );
}

/// The anchors of the headings of the user guide page `page` (mdBook's:
/// lower case, spaces as `-`, punctuation dropped).
fn page_anchors(page: &str) -> Vec<String> {
    std::fs::read_to_string(crate::tests::root().join(format!("docs/{page}.md")))
        .unwrap()
        .lines()
        .filter_map(|l| l.strip_prefix('#'))
        .map(|h| {
            h.trim_start_matches('#')
                .trim()
                .to_lowercase()
                .chars()
                .filter_map(|c| match c {
                    ' ' => Some('-'),
                    c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
                    _ => None,
                })
                .collect()
        })
        .collect()
}

/// Clicks every **user guide ↗** of the field window opened in `h`, in
/// turn (scrolled to each): each opens the window's own page `page`
/// (owner, 2026-11-12), at a section that is a heading of it.
fn guide_buttons_open(h: &mut Harness<'_, App>, page: &str, count: usize) {
    let anchors = page_anchors(page);
    let name = page.rsplit('/').next().unwrap();
    for k in 0..count {
        h.get_all_by_label("user guide ↗")
            .nth(k)
            .expect("a heading's link")
            .scroll_to_me();
        h.run();
        h.get_all_by_label("user guide ↗").nth(k).unwrap().click();
        h.run_steps(2);
        let (text, error) = h.state_mut().ready().unwrap().status.clone().expect("said");
        assert!(!error, "{text}");
        assert!(text.contains(name), "{text}");
        let anchor = text
            .rsplit(['#', ' '])
            .next()
            .unwrap()
            .trim_end_matches(')');
        assert!(
            anchors.iter().any(|a| a == anchor),
            "{anchor} not in {anchors:?}"
        );
    }
    assert!(h.get_all_by_label("user guide ↗").nth(count).is_none());
    let opened = crate::help::OPENED.lock().unwrap().clone();
    assert!(
        opened
            .iter()
            .any(|p| p.display().to_string().contains(name)),
        "{opened:?}"
    );
}

/// The magnetic field window's five headings open its own page.
#[test]
fn the_magnetic_window_opens_its_own_guide_page() {
    let model = synthetic();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.run();
    let r = h.state_mut().ready().unwrap();
    r.magnetic_field.show(r.case);
    h.run();
    guide_buttons_open(&mut h, crate::magnetic_field::GUIDE, 5);
    // The modelling settings folded under 4 Model (owner, 2026-11-12: the
    // setup for a human), shown when it is opened; the scope keeps the
    // limit and its unit.
    h.get_by_label("Limit [µT]");
    for l in ["Profiles by", "Earth wires", "Jumpers", "Wire pieces"] {
        assert!(h.query_by_label(l).is_none(), "{l} folded");
    }
    h.get_by_label_contains("Profiles by, earth wires").click();
    h.run();
    for l in ["Profiles by", "Earth wires", "Jumpers", "Wire pieces"] {
        h.get_by_label(l);
    }
}
