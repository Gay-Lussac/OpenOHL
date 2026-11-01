// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A wire's tension against its strain (owner, 2026-11-01: "a graph of
//! tension vs deformation of the wire based on its parameters … and for
//! the properties … a point put on the graph at its current condition"):
//! at a temperature, the initial curve, the creep curve where the wire
//! has one and the final lines after creep and after load where they are
//! known (a span's section); a span's state marked from its horizontal
//! to its largest tension. Shown in the wire library and in a span's
//! properties. Tension per subconductor; strain the total strain of the
//! wire (its mechanical strain plus its thermal expansion from the
//! curves' reference temperature), as the solver takes it
//! ([`ohl_cables::mechanics`]).

use eframe::egui;
use ohl_cables::Cable;
use ohl_cables::mechanics::{MaterialState, Mechanics};
use ohl_core::units::{Force, Temperature};

/// A final state: its name, the material state and (linear models) the
/// permanent stretch added to the strain.
pub type Final = (String, MaterialState, f64);

/// What the graph draws.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Curves {
    /// Each curve: its name and its points (strain %, tension kN).
    pub lines: Vec<(String, Vec<[f64; 2]>)>,
    /// The rated tensile strength, kN.
    pub rts: f64,
    /// The temperature, °C.
    pub temperature: f64,
    /// A span's state: its name and its points (strain %, tension kN),
    /// the horizontal tension's and the largest.
    pub marked: Option<(String, [[f64; 2]; 2])>,
    /// What could not be drawn, and why.
    pub notes: Vec<String>,
}

/// Points per curve.
const POINTS: usize = 80;

/// The curves of `cable` at `temperature`, with the `finals` given; up to
/// the rated tensile strength.
pub fn curves(cable: &Cable, temperature: Temperature, finals: &[Final]) -> Result<Curves, String> {
    let m = Mechanics::from_cable(cable).map_err(|e| e.to_string())?;
    let rts = cable.rated_tensile_strength.si();
    if rts.is_nan() || rts <= 0.0 {
        return Err("no rated tensile strength".into());
    }
    let strain = |t: f64, s: &MaterialState| {
        m.strain(Force::from_si(t), temperature, s)
            .map_err(|e| e.to_string())
    };
    let (lo, hi) = (
        strain(0.0, &MaterialState::Initial)?,
        strain(rts, &MaterialState::Initial)?,
    );
    let line = |s: &MaterialState, from: f64, shift: f64| -> Vec<[f64; 2]> {
        (0..=POINTS)
            .filter_map(|k| {
                let e = from + (hi - from) * k as f64 / POINTS as f64;
                let t = m.tension(e, temperature, s).si();
                (t >= 0.0 && t <= 1.05 * rts).then_some([(e + shift) * 100.0, t / 1e3])
            })
            .collect()
    };
    let mut lines = vec![("initial".to_owned(), line(&MaterialState::Initial, lo, 0.0))];
    if m.is_nonlinear() {
        let creep = line(&MaterialState::Creep, lo, 0.0);
        if creep != lines[0].1 {
            lines.push(("creep".to_owned(), creep));
        }
    }
    for (name, s, shift) in finals {
        let from = strain(0.0, s)?.min(hi);
        lines.push((name.clone(), line(s, from, *shift)));
    }
    Ok(Curves {
        lines,
        rts: rts / 1e3,
        temperature: temperature.celsius(),
        marked: None,
        notes: Vec::new(),
    })
}

/// The point at `tension` (N) on state `s` (+ `shift`), strain %, kN.
pub fn point(
    cable: &Cable,
    temperature: Temperature,
    s: &MaterialState,
    shift: f64,
    tension: f64,
) -> Result<[f64; 2], String> {
    let m = Mechanics::from_cable(cable).map_err(|e| e.to_string())?;
    let e = m
        .strain(Force::from_si(tension), temperature, s)
        .map_err(|e| e.to_string())?;
    Ok([(e + shift) * 100.0, tension / 1e3])
}

/// The curves of span `span` of section `section` in weather case
/// `case` of `model`: its wire as the solver takes it (the bimetallic
/// option, its stringing temperature), at the case's temperature, the
/// final lines after creep and after load where the line's criteria give
/// them, and the span marked on the line of its section's calculation
/// (max sag: the final line of the larger strain) from its horizontal to
/// its largest tension.
pub fn of_span(
    model: &crate::model::Model,
    section: &str,
    span: usize,
    case: usize,
) -> Result<Curves, String> {
    use ohl_line::condition::Condition;
    let line = &model.line;
    let sec = line
        .sections
        .iter()
        .find(|s| s.id == section)
        .ok_or("not in the line")?;
    let raw = model
        .library
        .get(&sec.cable)
        .ok_or_else(|| format!("wire type {} not in the library", sec.cable))?;
    let own = line.bimetallic.apply(raw);
    let cable = ohl_line::solve::strung_cable(sec, &own).map_err(|e| e.to_string())?;
    let solved = model.cases.get(case).ok_or("no weather case")?;
    let temperature = solved.case.temperature;
    let budget = ohl_core::budget::Budget::new();
    let mut finals: Vec<(Condition, Final)> = Vec::new();
    let mut notes = Vec::new();
    for (condition, name) in [
        (Condition::Creep, "after creep"),
        (Condition::Load, "after load"),
    ] {
        match ohl_line::solve::final_state(line, sec, raw, condition, &budget) {
            Ok(f) => finals.push((
                condition,
                (
                    if f.case.is_empty() {
                        name.to_owned()
                    } else {
                        format!("{name} ({})", f.case)
                    },
                    f.material,
                    if cable.model == ohl_cables::CableModel::Nonlinear {
                        0.0
                    } else {
                        f.stretch
                    },
                ),
            )),
            Err(e) => notes.push(format!("{name}: {e}")),
        }
    }
    let list: Vec<Final> = finals.iter().map(|f| f.1.clone()).collect();
    let mut c = curves(&cable, temperature, &list)?;
    if let Some(sp) = solved
        .shapes
        .iter()
        .find(|x| x.section == section)
        .and_then(|x| x.spans.get(span))
    {
        let initial: Final = ("initial".into(), MaterialState::Initial, 0.0);
        let of = |cond: Condition| finals.iter().find(|f| f.0 == cond).map(|f| &f.1);
        let calc = sec.calculation;
        let state = match calc.condition() {
            Condition::Initial => Some(&initial),
            Condition::Creep => of(Condition::Creep),
            Condition::Load => of(Condition::Load),
            Condition::MaxSag => {
                let strain = |f: &Final| {
                    point(&cable, temperature, &f.1, f.2, sp.max_tension).map_or(f64::MIN, |p| p[0])
                };
                [of(Condition::Creep), of(Condition::Load)]
                    .into_iter()
                    .flatten()
                    .max_by(|a, b| strain(a).total_cmp(&strain(b)))
            }
        };
        if let Some(f) = state {
            let p = |t: f64| point(&cable, temperature, &f.1, f.2, t);
            c.marked = Some((
                format!(
                    "span {} in {} ({})",
                    span + 1,
                    solved.case.name,
                    calc.label()
                ),
                [p(sp.horizontal_tension)?, p(sp.max_tension)?],
            ));
        }
    }
    c.notes = notes;
    Ok(c)
}

/// The graph.
pub fn plot(ui: &mut egui::Ui, c: &Curves) {
    let width = ui.available_width().clamp(260.0, 640.0);
    let (response, painter) = ui.allocate_painter(egui::vec2(width, 220.0), egui::Sense::hover());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "tension against strain")
    });
    let r = response.rect;
    let v = ui.visuals();
    let (text, faint) = (v.text_color(), v.weak_text_color());
    let font = egui::FontId::proportional(11.0);
    let frame = egui::Rect::from_min_max(
        r.min + egui::vec2(44.0, 8.0),
        r.max - egui::vec2(10.0, 24.0),
    );
    painter.rect_stroke(
        frame,
        0.0,
        egui::Stroke::new(1.0, faint),
        egui::StrokeKind::Inside,
    );
    let all = c
        .lines
        .iter()
        .flat_map(|l| &l.1)
        .chain(c.marked.iter().flat_map(|m| &m.1));
    let (mut x0, mut x1) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in all {
        x0 = x0.min(p[0]);
        x1 = x1.max(p[0]);
    }
    if !(x0.is_finite() && x1 > x0) {
        return;
    }
    let top = 1.1 * c.rts;
    let at = |x: f64, y: f64| {
        egui::pos2(
            frame.left() + ((x - x0) / (x1 - x0)) as f32 * frame.width(),
            frame.bottom() - (y / top) as f32 * frame.height(),
        )
    };
    // Ticks at round values (1, 2 or 5 times a power of ten).
    let grid = egui::Stroke::new(0.5, faint.gamma_multiply(0.4));
    let dx = round_step((x1 - x0) / 4.0);
    let mut x = (x0 / dx).ceil() * dx;
    while x <= x1 + 1e-12 {
        let p = at(x, 0.0);
        painter.line_segment([p, egui::pos2(p.x, frame.top())], grid);
        painter.text(
            egui::pos2(p.x, frame.bottom() + 3.0),
            egui::Align2::CENTER_TOP,
            format!("{x:.*}", decimals(dx)),
            font.clone(),
            faint,
        );
        x += dx;
    }
    let dy = round_step(top / 4.0);
    let mut y = 0.0;
    while y <= top + 1e-12 {
        let p = at(x0, y);
        painter.line_segment([p, egui::pos2(frame.right(), p.y)], grid);
        painter.text(
            egui::pos2(frame.left() - 4.0, p.y),
            egui::Align2::RIGHT_CENTER,
            format!("{y:.*}", decimals(dy)),
            font.clone(),
            faint,
        );
        y += dy;
    }
    painter.text(
        egui::pos2(frame.left() + 4.0, frame.top() + 2.0),
        egui::Align2::LEFT_TOP,
        format!("T kN (per subconductor), at {:.0} °C", c.temperature),
        font.clone(),
        text,
    );
    painter.text(
        egui::pos2(frame.right(), r.bottom()),
        egui::Align2::RIGHT_BOTTOM,
        "strain %",
        font.clone(),
        faint,
    );
    // The rated tensile strength.
    let y = at(x0, c.rts).y;
    painter.extend(egui::Shape::dashed_line(
        &[egui::pos2(frame.left(), y), egui::pos2(frame.right(), y)],
        egui::Stroke::new(1.0, v.warn_fg_color),
        6.0,
        4.0,
    ));
    painter.text(
        egui::pos2(frame.right() - 4.0, y - 2.0),
        egui::Align2::RIGHT_BOTTOM,
        "RTS",
        font.clone(),
        v.warn_fg_color,
    );
    let colors = [
        v.hyperlink_color,
        egui::Color32::from_rgb(200, 120, 40),
        egui::Color32::from_rgb(60, 160, 90),
        egui::Color32::from_rgb(150, 100, 200),
    ];
    let mut legend = frame.left() + 4.0;
    for (k, (name, pts)) in c.lines.iter().enumerate() {
        let col = colors[k % colors.len()];
        let screen: Vec<egui::Pos2> = pts.iter().map(|p| at(p[0], p[1])).collect();
        painter.add(egui::Shape::line(screen, egui::Stroke::new(1.6, col)));
        let g = painter.layout_no_wrap(name.clone(), font.clone(), col);
        let w = g.size().x;
        painter.galley(egui::pos2(legend, frame.top() + 16.0), g, col);
        legend += w + 12.0;
    }
    if let Some((name, [a, b])) = &c.marked {
        let red = v.error_fg_color;
        let (pa, pb) = (at(a[0], a[1]), at(b[0], b[1]));
        painter.line_segment([pa, pb], egui::Stroke::new(3.0, red));
        painter.circle_filled(pa, 3.5, red);
        painter.circle_filled(pb, 3.5, red);
        painter.text(
            pb + egui::vec2(6.0, -4.0),
            egui::Align2::LEFT_BOTTOM,
            name,
            font.clone(),
            red,
        );
    }
    if let Some(pos) = response.hover_pos()
        && frame.contains(pos)
    {
        let x = x0 + f64::from((pos.x - frame.left()) / frame.width()) * (x1 - x0);
        let y = f64::from((frame.bottom() - pos.y) / frame.height()) * top;
        painter.text(
            pos + egui::vec2(8.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{x:.4} %, {y:.1} kN ({:.0} % RTS)", 100.0 * y / c.rts),
            font,
            text,
        );
    }
}

/// A tick step near `rough`: 1, 2 or 5 times a power of ten.
fn round_step(rough: f64) -> f64 {
    let p = 10f64.powf(rough.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|m| m * p)
        .find(|s| *s >= rough)
        .unwrap_or(10.0 * p)
}

/// The decimals a tick step needs.
fn decimals(step: f64) -> usize {
    (-step.log10().floor()).max(0.0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wire with curves: the creep curve to the right of the initial one
    /// (more strain at a tension), both rising.
    #[test]
    fn a_wire_with_curves_has_its_creep_curve() {
        let lib = ohl_cables::Library::load(
            &crate::tests::root().join("examples/sagtension/synthetic-library/synthetic.ohlw"),
        )
        .unwrap();
        let id = "synthetic";
        let c = curves(lib.get(id).unwrap(), Temperature::from_celsius(20.0), &[]).unwrap();
        let names: Vec<&str> = c.lines.iter().map(|l| l.0.as_str()).collect();
        assert_eq!(names, ["initial", "creep"]);
        let at = |line: &[[f64; 2]], t: f64| {
            line.windows(2)
                .find(|w| w[0][1] <= t && t <= w[1][1])
                .map(|w| w[0][0] + (w[1][0] - w[0][0]) * (t - w[0][1]) / (w[1][1] - w[0][1]))
                .unwrap()
        };
        let t = 0.2 * c.rts;
        assert!(at(&c.lines[1].1, t) > at(&c.lines[0].1, t));
    }

    /// A span of the example line: marked from its horizontal to its
    /// largest tension, on the initial line (its calculation Initial FE).
    #[test]
    fn a_span_is_marked_on_its_line() {
        let model = crate::tests::synthetic();
        let section = model.line.sections[0].id.clone();
        let c = of_span(&model, &section, 0, 0).unwrap();
        let (_, [h, max]) = c.marked.clone().expect("marked");
        let sp = &model.cases[0]
            .shapes
            .iter()
            .find(|x| x.section == section)
            .unwrap()
            .spans[0];
        assert!((max[1] - sp.max_tension / 1e3).abs() < 1e-9);
        assert!((h[1] - sp.horizontal_tension / 1e3).abs() < 1e-9);
        assert!(max[0] > h[0], "more strain at the larger tension");
        // On the initial line: interpolated there, the same strain.
        let line = &c.lines[0].1;
        let w = line
            .windows(2)
            .find(|w| w[0][1] <= max[1] && max[1] <= w[1][1])
            .unwrap();
        let e = w[0][0] + (w[1][0] - w[0][0]) * (max[1] - w[0][1]) / (w[1][1] - w[0][1]);
        assert!((e - max[0]).abs() < 1e-6, "{e} against {}", max[0]);
    }

    /// A linear wire: one straight line from zero to the rated tensile
    /// strength, its slope the axial stiffness; a stretch moves the final
    /// line by it.
    #[test]
    fn a_linear_wire_is_a_straight_line() {
        let lib =
            ohl_cables::Library::load(&crate::tests::root().join("libraries/cables")).unwrap();
        let cable = lib.get("acsr-drake-26-7").unwrap();
        let t = Temperature::from_celsius(20.0);
        let stretch = 4e-4;
        let c = curves(
            cable,
            t,
            &[("after creep".into(), MaterialState::Initial, stretch)],
        )
        .unwrap();
        let first = &c.lines[0].1;
        assert!(first.len() > 10);
        assert!(first[0][1].abs() < 1e-6, "from zero tension");
        assert!((first.last().unwrap()[1] - c.rts).abs() < 0.02 * c.rts);
        let last = &c.lines.last().unwrap().1;
        assert!((last[0][0] - first[0][0] - stretch * 100.0).abs() < 1e-9);
        let p = point(cable, t, &MaterialState::Initial, 0.0, 30e3).unwrap();
        assert!((p[1] - 30.0).abs() < 1e-9 && p[0] > first[0][0]);
    }
}
