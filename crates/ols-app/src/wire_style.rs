// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! How the wires are drawn (Sections › Display options, each user's own):
//! coloured by circuit (each section's colour, else its phase's) or by
//! system (the system's colour, each phase in its own line code: the
//! first solid, the second ·–·–, the third ·––·––), and the direction of
//! each section and jumper in its circuit as arrows.

use std::collections::HashMap;

use ols_line::Line;

/// What gives the wires their colour.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Coloring {
    /// Each section's colour (a circuit's sections share one), else its
    /// phase's.
    #[default]
    Circuits,
    /// Its system's colour, its phase in a line code; earth wires grey.
    Systems,
}

/// Line codes of the phases of a system, in units (on, off, on, off…):
/// the first phase solid, the second ·–·–, the third ·––·––; then again.
const CODES: [&[f64]; 3] = [&[], &[1.0, 1.5, 5.0, 1.5], &[1.0, 1.5, 5.0, 1.5, 5.0, 1.5]];

/// A wire's colour and line code (0: solid).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// Colour.
    pub color: [u8; 3],
    /// Line code: the phase's place in its system.
    pub code: usize,
}

/// The styles of a line's sections and jumpers, and their directions.
#[derive(Debug, Clone, Default)]
pub struct Styles {
    sections: HashMap<String, Style>,
    jumpers: HashMap<String, Style>,
    /// Directions in their circuits.
    pub directions: ols_line::electrical::Directions,
}

impl Styles {
    /// Styles of `line` coloured as `coloring` says.
    pub fn new(line: &Line, coloring: Coloring) -> Self {
        let phases = match coloring {
            Coloring::Systems => line.section_phases().unwrap_or_default(),
            Coloring::Circuits => Default::default(),
        };
        let grey = crate::scene::rgb(ols_line::NEUTRAL_COLOR);
        let style = |s: &ols_line::Section| match coloring {
            Coloring::Circuits => Style {
                color: crate::scene::rgb(&line.section_color(s)),
                code: 0,
            },
            Coloring::Systems => match phases.get(&s.id).copied().flatten() {
                Some((k, p)) => Style {
                    color: crate::scene::rgb(&line.system_color(k)),
                    code: p,
                },
                None => Style {
                    color: grey,
                    code: 0,
                },
            },
        };
        let sections: HashMap<String, Style> = line
            .sections
            .iter()
            .map(|s| (s.id.clone(), style(s)))
            .collect();
        let jumpers = line
            .jumpers
            .iter()
            .filter_map(|j| {
                let mut st = sections.get(&line.jumper_section(j)?.id).copied()?;
                if coloring == Coloring::Circuits
                    && let Some(c) = &j.color
                {
                    st.color = crate::scene::rgb(c);
                }
                Some((j.id.clone(), st))
            })
            .collect();
        Self {
            sections,
            jumpers,
            directions: ols_line::electrical::directions(line),
        }
    }

    /// A section's style.
    pub fn section(&self, id: &str) -> Style {
        self.sections.get(id).copied().unwrap_or(Style {
            color: [176, 184, 192],
            code: 0,
        })
    }

    /// A jumper's style.
    pub fn jumper(&self, id: &str) -> Style {
        self.jumpers.get(id).copied().unwrap_or(Style {
            color: [200, 200, 200],
            code: 0,
        })
    }
}

/// The styles kept between frames (2D views), made again when what they
/// follow changes: the sections' circuits, phases, systems, colours and
/// points, the jumpers, the systems, the chosen circuit starts.
#[derive(Debug, Default)]
pub struct StyleCache {
    key: Option<u64>,
    styles: Styles,
}

impl StyleCache {
    /// The styles of `line` coloured as `coloring` says.
    pub fn get(&mut self, line: &Line, coloring: Coloring) -> &Styles {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        coloring.hash(&mut h);
        for s in &line.sections {
            (
                &s.id,
                &s.circuit,
                &s.phase_label,
                &s.system,
                &s.color,
                &s.points,
            )
                .hash(&mut h);
        }
        for j in &line.jumpers {
            (&j.id, &j.from, &j.to, &j.color).hash(&mut h);
        }
        for s in &line.systems {
            (
                &s.id,
                &s.color,
                s.phases.iter().map(|p| &p.label).collect::<Vec<_>>(),
            )
                .hash(&mut h);
        }
        line.circuit_starts.hash(&mut h);
        let key = h.finish();
        if self.key != Some(key) {
            self.styles = Styles::new(line, coloring);
            self.key = Some(key);
        }
        &self.styles
    }
}

/// `points` cut into the dashes of line code `code`, `unit` long each
/// unit (the same unit as the points: metres in 3D, pixels on screen);
/// solid: the polyline itself.
pub fn dashes(points: &[[f64; 3]], code: usize, unit: f64) -> Vec<Vec<[f64; 3]>> {
    let pattern = CODES[code % CODES.len()];
    if pattern.is_empty() || points.len() < 2 || unit <= 0.0 {
        return vec![points.to_vec()];
    }
    let lengths: Vec<f64> = pattern.iter().map(|u| u * unit).collect();
    let (mut out, mut current) = (Vec::new(), vec![points[0]]);
    let (mut k, mut left, mut on) = (0, lengths[0], true);
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f64>().sqrt();
        let mut at = 0.0;
        while d - at > left {
            at += left;
            let p = [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * at / d);
            if on {
                current.push(p);
                out.push(std::mem::take(&mut current));
            } else {
                current = vec![p];
            }
            on = !on;
            k = (k + 1) % lengths.len();
            left = lengths[k];
        }
        left -= d - at;
        if on {
            current.push(b);
        } else {
            current = vec![b];
        }
    }
    if on && current.len() > 1 {
        out.push(current);
    }
    out
}

/// An arrow head (two short lines) at the middle of `points` (by length),
/// pointing along them (`forward`) or back; `size` long, opened in the
/// plane of `side` (a direction across the polyline). `None`: too short.
pub fn chevron(
    points: &[[f64; 3]],
    forward: bool,
    size: f64,
    side: impl Fn([f64; 3]) -> [f64; 3],
) -> Option<[[[f64; 3]; 2]; 2]> {
    let lengths: Vec<f64> = points
        .windows(2)
        .map(|w| {
            (0..3)
                .map(|i| (w[1][i] - w[0][i]).powi(2))
                .sum::<f64>()
                .sqrt()
        })
        .collect();
    let total: f64 = lengths.iter().sum();
    if total < 1e-9 {
        return None;
    }
    let (mut run, mut k) = (0.0, 0);
    while k + 1 < lengths.len() && run + lengths[k] < total / 2.0 {
        run += lengths[k];
        k += 1;
    }
    let (a, b) = (points[k], points[k + 1]);
    let t = [0, 1, 2].map(|i| (b[i] - a[i]) / lengths[k].max(1e-12));
    let f = ((total / 2.0 - run) / lengths[k].max(1e-12)).clamp(0.0, 1.0);
    let mid = [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * f);
    let t = if forward { t } else { t.map(|v| -v) };
    let s = side(t);
    let tip = [0, 1, 2].map(|i| mid[i] + t[i] * size / 2.0);
    let leg = |sign: f64| [0, 1, 2].map(|i| mid[i] - t[i] * size / 2.0 + sign * s[i] * size / 2.0);
    Some([[leg(1.0), tip], [leg(-1.0), tip]])
}

/// Across a direction, horizontally (3D and plan); straight up for a
/// vertical one.
pub fn across(t: [f64; 3]) -> [f64; 3] {
    let n = t[0].hypot(t[1]);
    if n < 1e-9 {
        [1.0, 0.0, 0.0]
    } else {
        [-t[1] / n, t[0] / n, 0.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_codes_cut_a_wire_into_dashes() {
        let line = vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [22.0, 0.0, 0.0]];
        assert_eq!(dashes(&line, 0, 1.0), vec![line.clone()]);
        // ·–·–: 1 on, 1.5 off, 5 on, 1.5 off (9 a period).
        let d = dashes(&line, 1, 1.0);
        let len = |p: &Vec<[f64; 3]>| p.last().unwrap()[0] - p[0][0];
        // [0, 1], [2.5, 7.5], [9, 10], [11.5, 16.5], [18, 19], [20.5, 22].
        assert_eq!(d.len(), 6);
        assert!((len(&d[0]) - 1.0).abs() < 1e-9 && (len(&d[1]) - 5.0).abs() < 1e-9);
        assert!((d[1][0][0] - 2.5).abs() < 1e-9);
        // A dash across the bend keeps the corner.
        assert!(d.iter().any(|p| p.len() == 3));
        // ·––·––: two dashes after the dot.
        let d = dashes(&line, 2, 1.0);
        assert!((len(&d[2]) - 5.0).abs() < 1e-9 && (len(&d[3]) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_chevron_points_along_or_back() {
        let line = vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]];
        let [a, b] = chevron(&line, true, 2.0, across).unwrap();
        assert_eq!(a[1], [6.0, 0.0, 0.0]);
        assert_eq!((a[0], b[0]), ([4.0, 1.0, 0.0], [4.0, -1.0, 0.0]));
        let [a, _] = chevron(&line, false, 2.0, across).unwrap();
        assert_eq!(a[1], [4.0, 0.0, 0.0]);
    }
}
