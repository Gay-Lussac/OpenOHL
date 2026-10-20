// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! How wires and structures are drawn: by phase colour (the default), or
//! by **kind** (high voltage, low voltage, telecom, earth wire) and
//! construction (overhead solid, cable dashed) as a template's
//! `[line_styles]` says, combined with the **role** of the version of the
//! line drawn (`[roles]`: existing thin, project thick, removal in its own
//! colour …); and the **legend** made from what a sheet draws.

use crate::Trace;
use crate::model::{Color, Pen};
use crate::template::{LineStyles, SheetTemplate};

/// Pen width of the wires without a style, mm.
pub const WIRE_WIDTH: f64 = 0.35;

/// What a wire is, for its style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// A system above the low voltage limit (or any, without one).
    HighVoltage,
    /// A system up to the template's low voltage limit.
    LowVoltage,
    /// Telecom or weak current (only from a section's field).
    Telecom,
    /// No system: an earth wire (or a field says so).
    Earth,
}

impl Kind {
    /// Its key in `[line_styles.kinds]`.
    pub fn key(self) -> &'static str {
        match self {
            Self::HighVoltage => "hv",
            Self::LowVoltage => "lv",
            Self::Telecom => "telecom",
            Self::Earth => "earth",
        }
    }

    /// What the legend calls it without a label of the template's.
    pub fn words(self) -> &'static str {
        match self {
            Self::HighVoltage => "high voltage",
            Self::LowVoltage => "low voltage",
            Self::Telecom => "telecom / weak current",
            Self::Earth => "earth wire",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "hv" => Some(Self::HighVoltage),
            "lv" => Some(Self::LowVoltage),
            "telecom" => Some(Self::Telecom),
            "earth" => Some(Self::Earth),
            _ => None,
        }
    }
}

/// The kind of wire `t`: its section's field when the template names one
/// and it holds a kind, else by its voltage (none: an earth wire; up to
/// the low voltage limit: low voltage; else high voltage).
pub fn kind_of(t: &Trace, s: &LineStyles) -> Kind {
    if let Some(k) = s
        .kind_field
        .as_ref()
        .and_then(|f| t.fields.get(f))
        .and_then(|v| Kind::parse(v))
    {
        return k;
    }
    match (t.voltage_kv, s.low_voltage_up_to_kv) {
        (None, _) => Kind::Earth,
        (Some(kv), Some(lv)) if kv <= lv => Kind::LowVoltage,
        _ => Kind::HighVoltage,
    }
}

/// Whether wire `t` is a cable (its section's construction field says
/// `cable`), not an overhead wire.
pub fn is_cable(t: &Trace, s: &LineStyles) -> bool {
    s.construction_field
        .as_ref()
        .and_then(|f| t.fields.get(f))
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("cable"))
}

/// The colour `c` faded to `share` of itself (the rest white).
pub fn faded(c: Color, share: f64) -> Color {
    let f = |v: u8| (f64::from(v) * share + 255.0 * (1.0 - share)).round() as u8;
    Color(f(c.0), f(c.1), f(c.2))
}

/// The pen of wire `t` drawn in `role` (none: the line drawn as it is),
/// and what it means in the legend (the line's kind or phase, its role,
/// cable): by kind when the template says so, else in its phase colour;
/// the role's colour, width and dash over it; a cable dashed; another
/// weather case's dash over a solid line.
pub fn wire_pen(tpl: &SheetTemplate, t: &Trace, role: Option<&str>) -> (Pen, String) {
    let s = &tpl.line_styles;
    let mut pen = Pen::new(Color::hex_or_black(&t.color), WIRE_WIDTH);
    let mut words = if t.phase.is_empty() {
        "wire".to_owned()
    } else {
        format!("phase {}", t.phase)
    };
    let mut dashed = false;
    if s.by_kind {
        let kind = kind_of(t, s);
        let style = s.kinds.get(kind.key());
        if let Some(style) = style {
            pen = style.pen.over(pen);
            dashed |= style.pen.dash.as_deref().is_some_and(|d| !d.is_empty());
        }
        words = style
            .and_then(|k| k.label.clone())
            .unwrap_or_else(|| kind.words().to_owned());
        if let Some(kv) = t.voltage_kv {
            words = format!("{words} {} kV", crate::template::format_number(kv, ""));
        }
    }
    if is_cable(t, s) {
        pen = pen.dashed(s.cable_dash.as_deref().unwrap_or("3 1.5"));
        dashed = true;
        words.push_str(", cable");
    }
    if let Some(r) = role {
        let spec = tpl.roles.get(r);
        if let Some(spec) = spec {
            if let Some(w) = spec.weight {
                pen.width *= w;
            }
            if let Some(w) = spec.width {
                pen.width = w.0;
            }
            if let Some(c) = spec.colour.as_deref().and_then(Color::parse) {
                pen.color = c;
            }
            if let Some(d) = &spec.dash {
                pen = pen.dashed(d);
                dashed = !d.trim().is_empty();
            }
        }
        let name = spec
            .and_then(|s| s.label.clone())
            .unwrap_or_else(|| r.to_owned());
        words = format!("{name} — {words}");
    }
    // Another weather case: its dash, unless the style dashes already.
    if !dashed && let Some(d) = &t.dash {
        pen = pen.dashed(d);
    }
    (pen, words)
}

/// The pen of the structures of a version in `role`, `pen` the line's
/// own (none: as it is).
pub fn structure_pen(tpl: &SheetTemplate, role: Option<&str>, pen: Pen) -> Pen {
    let Some(spec) = role.and_then(|r| tpl.roles.get(r)) else {
        return pen;
    };
    let mut pen = pen;
    if let Some(c) = spec
        .structure_colour
        .as_deref()
        .or(spec.colour.as_deref())
        .and_then(Color::parse)
    {
        pen.color = c;
    }
    if let Some(w) = spec.weight {
        pen.width *= w;
    }
    pen
}

/// What a legend entry shows beside its text.
#[derive(Debug, Clone, PartialEq)]
pub enum Sample {
    /// A line in this pen.
    Line(Pen),
    /// A small square (a structure).
    Square(Pen),
    /// A circle (a marker).
    Circle(Pen),
    /// Filled dots of this colour and diameter, mm (LiDAR points).
    Dots(crate::model::Color, f64),
}

/// One entry of a legend.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// What it shows.
    pub sample: Sample,
    /// What it means.
    pub text: String,
}

/// A sheet's legend, made from what it draws: each meaning once, in the
/// order first drawn.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Legend {
    /// Its entries.
    pub entries: Vec<Entry>,
}

impl Legend {
    /// Adds an entry unless its text is there already.
    pub fn add(&mut self, sample: Sample, text: impl Into<String>) {
        let text = text.into();
        if !text.is_empty() && !self.entries.iter().any(|e| e.text == text) {
            self.entries.push(Entry { sample, text });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::SheetTemplate;

    fn trace(kv: Option<f64>) -> Trace {
        Trace {
            color: "#e4572e".into(),
            phase: "L1".into(),
            voltage_kv: kv,
            ..Trace::default()
        }
    }

    #[test]
    fn wires_by_kind_and_role() {
        let t = SheetTemplate::parse(
            r##"format = "ohl-sheet-template"
version = 1
[line_styles]
by_kind = true
low_voltage_up_to_kv = 1
kind_field = "code.kind"
construction_field = "code.construction"
cable_dash = "2 1"
[line_styles.kinds.hv]
colour = "#ff0000"
width = "0.5 mm"
label = "HV line"
[line_styles.kinds.earth]
colour = "#00aa00"
[roles.existing]
weight = 0.5
label = "existing"
[roles.removal]
colour = "#ffaa00"
dash = "1 1"
"##,
            None,
            &[],
        )
        .unwrap();
        let (p, w) = wire_pen(&t, &trace(Some(220.0)), None);
        assert_eq!(
            (p.color, p.width, w.as_str()),
            (Color(255, 0, 0), 0.5, "HV line 220 kV")
        );
        let (p, w) = wire_pen(&t, &trace(Some(220.0)), Some("existing"));
        assert_eq!((p.width, w.as_str()), (0.25, "existing — HV line 220 kV"));
        let (p, _) = wire_pen(&t, &trace(Some(220.0)), Some("removal"));
        assert_eq!(
            (p.color, p.dash),
            (Color(255, 0xaa, 0), Some(vec![1.0, 1.0]))
        );
        assert_eq!(kind_of(&trace(Some(0.4)), &t.line_styles), Kind::LowVoltage);
        let (p, w) = wire_pen(&t, &trace(None), None);
        assert_eq!((p.color, w.as_str()), (Color(0, 0xaa, 0), "earth wire"));
        // A field: telecom, a cable (dashed).
        let mut tc = trace(None);
        tc.fields.insert("code.kind".into(), "telecom".into());
        tc.fields.insert("code.construction".into(), "cable".into());
        let (p, w) = wire_pen(&t, &tc, None);
        assert_eq!(
            (p.dash, w.as_str()),
            (Some(vec![2.0, 1.0]), "telecom / weak current, cable")
        );
        // The built-in template: the phase colour, other cases dashed.
        let b = SheetTemplate::builtin();
        let mut other = trace(Some(220.0));
        other.dash = Some("3 1.5".into());
        let (p, w) = wire_pen(&b, &other, None);
        assert_eq!(
            (p.color, p.width, p.dash, w.as_str()),
            (
                Color(0xe4, 0x57, 0x2e),
                WIRE_WIDTH,
                Some(vec![3.0, 1.5]),
                "phase L1"
            )
        );
    }
}
