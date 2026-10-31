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

/// The magnetic field's table too: a cut asked at its lowest point and
/// 100 m from the first structure, a volume and isolines from above in
/// the span ticked alone — each computed by the run, listed in Results.
#[test]
fn the_magnetic_table_asks_cuts_volumes_and_grids() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    let ctx = harness.ctx.clone();
    {
        let r = harness.state_mut().ready().unwrap();
        let circuits = ohl_electrical::magnetic::circuits(&r.model.line);
        r.magnetic_field.set_current(&circuits[0].label(), 800.0);
        r.magnetic_field.set_views(Some(5.0), None, Some(4.0));
        // Span 0 alone: its volume and its isolines from above ticked.
        r.magnetic_field.tick_only(&r.model, 0);
        assert_eq!(r.magnetic_field.ticked_spans(&r.model), vec![0]);
        r.magnetic_field.add_cut(&r.model, 0, CutAt::Lowest);
        r.magnetic_field.add_cut(&r.model, 0, CutAt::From(100.0));
        r.magnetic_field.show(r.case);
        r.magnetic_field.start(&r.model, &ctx);
    }
    let started = std::time::Instant::now();
    loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if let Some(res) = &r.magnetic_field.job.result {
            let a = res.as_ref().expect("a field");
            assert_eq!(a.field.cuts.len(), 2, "the two cuts asked");
            // 100 m from the first structure: at that place's alignment
            // station (the alignment runs through the structures).
            assert!(
                (a.field.cuts[1].at - 100.0).abs() < 1e-3,
                "{}",
                a.field.cuts[1].at
            );
            assert!(!a.field.cuts[0].alignment.is_empty());
            assert_eq!(a.field.volumes.len(), 1);
            assert_eq!(a.field.grids.len(), 1);
            break;
        }
        assert!(started.elapsed().as_secs() < 120, "no field in 120 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    harness.run();
    assert_eq!(harness.query_all_by_label("Open cut").count(), 2);
    assert_eq!(harness.query_all_by_label("Show in 3D").count(), 2);
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
