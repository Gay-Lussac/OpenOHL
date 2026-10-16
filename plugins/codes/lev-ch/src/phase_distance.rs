// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The distance between conductors in the span (LeV Annex 6, 2.2): the
//! values (Table 2.4.1, Diagrams 2.4.2 and 2.4.3, the altitude rule) and
//! the rules. OLS CAD measures the conductors (distances, β, sags, l_k,
//! the sags between spacers); this says what is required at each station:
//!
//! - 2.2.1: \\( z = k \sqrt{f + l_k} + C \\), C₁ within one system, C₂
//!   between systems (2.2.3: the larger z of the two conductors);
//! - 2.2.4, conductors joined by phase spacers: \\( z = k \sqrt{d_f} +
//!   C_3 \ge C_1 \\) (C₄ ≥ C₂ between systems), at least 0.90 m; d_f the
//!   largest sag between the spacers (each spacer as an attachment point:
//!   the sag of each stretch on either side of it, the biggest counts),
//!   while f stays the sag between the span's clamps;
//! - C grows by 14 % per 1000 m above 1000 m (the mean conductor height
//!   of the span).

use serde::Deserialize;
use std::cell::RefCell;
use std::fmt::Write as _;

/// The values file shipped with the plugin (LeV Table 2.4.1 and the
/// diagrams).
pub const VALUES: &str = include_str!("../phase-distance.olspd");

/// File format id of a values file.
pub const FORMAT: &str = "ols-phase-distance";
/// Newest version read.
pub const VERSION: u32 = 1;

/// Where a circuit keeps its insulation level (the plugin's Circuits
/// column, on the circuit's sections): `low` or `high`.
pub const LEVEL_FIELD: &str = "lev-ch.insulation_level";

/// The least distance with spacers (2.2.4), m.
const SPACER_MINIMUM: f64 = 0.90;

/// The distances of one nominal voltage, m: C₁ (within one system, and
/// towards earth), C₂ (between systems), C₃ and C₄ (the same for brief
/// approaches) — each on the line of the low and of the high insulation
/// level (equal where the table gives one line), with the lightning
/// impulse withstand voltage C₁ and C₂ are for, where given.
#[derive(Debug, Clone, PartialEq)]
pub struct VoltageRow {
    /// Nominal voltage, kV: this row for systems up to it.
    pub kv: f64,
    /// C₁, low and high line, m.
    pub c1: [f64; 2],
    /// The impulse withstand voltage C₁ is for, kV.
    pub c1_bil_kv: Option<[f64; 2]>,
    /// C₂, m.
    pub c2: [f64; 2],
    /// The impulse withstand voltage C₂ is for, kV.
    pub c2_bil_kv: Option<[f64; 2]>,
    /// C₃, m.
    pub c3: Option<[f64; 2]>,
    /// C₄, m.
    pub c4: Option<[f64; 2]>,
}

impl VoltageRow {
    /// The table gives two lines (two insulation levels).
    pub fn two_levels(&self) -> bool {
        let differ = |c: [f64; 2]| c[0] != c[1];
        differ(self.c1)
            || differ(self.c2)
            || self.c3.is_some_and(differ)
            || self.c4.is_some_and(differ)
            || self.c1_bil_kv.is_some_and(differ)
            || self.c2_bil_kv.is_some_and(differ)
    }
}

/// One line of a k diagram: for swing class w = d/q up to `w_max` (d in
/// mm, q in kg/m; none: no limit), k from `k0` at β = 0° to `k90` at
/// β = 90°, linearly.
#[derive(Debug, Clone, PartialEq)]
pub struct KLine {
    /// Upper limit of the class (none: the last).
    pub w_max: Option<f64>,
    /// k at β = 0°.
    pub k0: f64,
    /// k at β = 90°.
    pub k90: f64,
}

/// A k diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    /// Its name (e.g. "2.4.2 (500 N/m²)").
    pub name: String,
    /// Wind pressure it is for, Pa.
    pub wind_pressure: f64,
    /// Lines in increasing w.
    pub lines: Vec<KLine>,
}

impl Diagram {
    /// k for swing class `w` at system angle `beta_deg`.
    pub fn k(&self, w: f64, beta_deg: f64) -> f64 {
        let line = self
            .lines
            .iter()
            .find(|l| l.w_max.is_none_or(|m| w <= m))
            .or(self.lines.last())
            .expect("a diagram has lines");
        let t = (beta_deg / 90.0).clamp(0.0, 1.0);
        line.k0 + (line.k90 - line.k0) * t
    }
}

/// The insulation level of a circuit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    /// The low line.
    Low,
    /// The high line (the stricter; not set).
    #[default]
    High,
}

impl Level {
    /// From a value of [`LEVEL_FIELD`] (any case).
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

/// The values.
#[derive(Debug, Clone, PartialEq)]
pub struct Code {
    /// Name.
    pub name: String,
    /// Rows in increasing voltage.
    pub voltages: Vec<VoltageRow>,
    /// The k diagrams.
    pub diagrams: Vec<Diagram>,
    /// Altitude up to which C holds, m (none: always).
    pub altitude_from: Option<f64>,
    /// C grows by this share per 1000 m above it.
    pub altitude_per_1000: f64,
}

/// "2.64 m", "500 Pa" or a number: its value.
fn quantity(text: &str, unit: &str, what: &str) -> Result<f64, String> {
    let t = text.trim();
    let n = t.strip_suffix(unit).unwrap_or(t).trim();
    n.parse::<f64>()
        .map_err(|_| format!("{what}: \"{text}\" is not a number of {unit}"))
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Number {
    Text(String),
    Value(f64),
}

impl Number {
    fn si(&self, unit: &str, what: &str) -> Result<f64, String> {
        match self {
            Number::Value(v) => Ok(*v),
            Number::Text(t) => quantity(t, unit, what),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawVoltage {
    kv: f64,
    c1: Vec<Number>,
    #[serde(default)]
    c1_bil_kv: Option<Vec<f64>>,
    c2: Vec<Number>,
    #[serde(default)]
    c2_bil_kv: Option<Vec<f64>>,
    #[serde(default)]
    c3: Option<Vec<Number>>,
    #[serde(default)]
    c4: Option<Vec<Number>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLine {
    #[serde(default)]
    w_max: Option<f64>,
    k0: f64,
    k90: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDiagram {
    name: String,
    #[serde(default)]
    wind_pressure: Option<Number>,
    #[serde(rename = "line")]
    lines: Vec<RawLine>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawAltitude {
    #[serde(default)]
    from: Option<Number>,
    #[serde(default)]
    per_1000_m: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCode {
    format: Option<String>,
    version: Option<u32>,
    name: String,
    #[serde(rename = "voltage")]
    voltages: Vec<RawVoltage>,
    #[serde(rename = "diagram")]
    diagrams: Vec<RawDiagram>,
    #[serde(default)]
    altitude: Option<RawAltitude>,
}

impl Code {
    /// Parses and checks a values file.
    pub fn parse(text: &str) -> Result<Self, String> {
        let raw: RawCode = ols_code_common::toml::from_str(text).map_err(|e| e.to_string())?;
        match raw.format.as_deref() {
            Some(FORMAT) | None => {}
            Some(f) => return Err(format!("not a phase distance file (format \"{f}\")")),
        }
        if raw.version.is_some_and(|v| v > VERSION) {
            return Err(format!(
                "phase distance file version {} is newer than this plugin reads ({VERSION})",
                raw.version.unwrap_or_default()
            ));
        }
        if raw.voltages.is_empty() {
            return Err("give at least one [[voltage]]".into());
        }
        if raw.diagrams.is_empty() {
            return Err("give at least one [[diagram]]".into());
        }
        let pair = |v: &[Number], what: &str, kv: f64| -> Result<[f64; 2], String> {
            let w = format!("{kv} kV {what}");
            match v {
                [a] => {
                    let a = a.si("m", &w)?;
                    Ok([a, a])
                }
                [a, b] => Ok([a.si("m", &w)?, b.si("m", &w)?]),
                _ => Err(format!("{w} needs one value or two (low, high)")),
            }
        };
        let mut voltages = Vec::new();
        for r in &raw.voltages {
            let kv_pair = |v: &Option<Vec<f64>>, what: &str| -> Result<Option<[f64; 2]>, String> {
                match v.as_deref() {
                    None => Ok(None),
                    Some([a]) => Ok(Some([*a, *a])),
                    Some([a, b]) => Ok(Some([*a, *b])),
                    Some(_) => Err(format!(
                        "{} kV: {what} needs one value or two (low, high)",
                        r.kv
                    )),
                }
            };
            let row = VoltageRow {
                kv: r.kv,
                c1: pair(&r.c1, "c1", r.kv)?,
                c1_bil_kv: kv_pair(&r.c1_bil_kv, "c1_bil_kv")?,
                c2: pair(&r.c2, "c2", r.kv)?,
                c2_bil_kv: kv_pair(&r.c2_bil_kv, "c2_bil_kv")?,
                c3: r.c3.as_deref().map(|v| pair(v, "c3", r.kv)).transpose()?,
                c4: r.c4.as_deref().map(|v| pair(v, "c4", r.kv)).transpose()?,
            };
            voltages.push(row);
        }
        let mut diagrams = Vec::new();
        for d in raw.diagrams {
            if d.lines.is_empty() {
                return Err(format!("diagram {}: give its lines", d.name));
            }
            let wind_pressure = match &d.wind_pressure {
                Some(p) => p.si("Pa", &format!("diagram {} wind_pressure", d.name))?,
                None => 0.0,
            };
            diagrams.push(Diagram {
                name: d.name,
                wind_pressure,
                lines: d
                    .lines
                    .into_iter()
                    .map(|l| KLine {
                        w_max: l.w_max,
                        k0: l.k0,
                        k90: l.k90,
                    })
                    .collect(),
            });
        }
        let altitude = raw.altitude.unwrap_or_default();
        let code = Self {
            name: raw.name,
            voltages,
            diagrams,
            altitude_from: altitude
                .from
                .as_ref()
                .map(|f| f.si("m", "altitude from"))
                .transpose()?,
            altitude_per_1000: altitude.per_1000_m,
        };
        code.validate()?;
        Ok(code)
    }

    /// The values hold.
    pub fn validate(&self) -> Result<(), String> {
        for r in &self.voltages {
            if r.c1
                .iter()
                .chain(&r.c2)
                .chain(r.c3.iter().flatten())
                .chain(r.c4.iter().flatten())
                .any(|c| !(c.is_finite() && *c >= 0.0))
            {
                return Err(format!("{} kV: C must be ≥ 0", r.kv));
            }
        }
        if self.voltages.windows(2).any(|w| w[1].kv <= w[0].kv) {
            return Err("the voltage rows must be in increasing kV".into());
        }
        for d in &self.diagrams {
            if d.lines
                .iter()
                .any(|l| !(l.k0 > 0.0 && l.k90 > 0.0 && l.k0.is_finite() && l.k90.is_finite()))
            {
                return Err(format!("diagram {}: k must be > 0", d.name));
            }
            let limits: Vec<f64> = d.lines.iter().filter_map(|l| l.w_max).collect();
            if limits.windows(2).any(|w| w[1] <= w[0]) {
                return Err(format!("diagram {}: the swing classes must grow", d.name));
            }
        }
        if !(self.altitude_per_1000.is_finite() && self.altitude_per_1000 >= 0.0) {
            return Err("the altitude share must be ≥ 0".into());
        }
        Ok(())
    }

    /// The row of nominal voltage `kv`: the first up to it (above the
    /// last: the last).
    pub fn row(&self, kv: f64) -> &VoltageRow {
        self.voltages
            .iter()
            .find(|r| kv <= r.kv + 1e-9)
            .or(self.voltages.last())
            .expect("a code has voltage rows")
    }

    /// C₁ (within one system) or C₂ at `kv` and `level`, before the
    /// altitude.
    pub fn c(&self, same_system: bool, kv: f64, level: Level) -> f64 {
        let row = self.row(kv);
        let i = usize::from(level == Level::High);
        if same_system { row.c1[i] } else { row.c2[i] }
    }

    /// C₃ (within one system) or C₄, if the table gives it.
    pub fn c_brief(&self, same_system: bool, kv: f64, level: Level) -> Option<f64> {
        let row = self.row(kv);
        let i = usize::from(level == Level::High);
        if same_system { row.c3 } else { row.c4 }.map(|c| c[i])
    }

    /// The altitude factor at mean height `h`, m.
    pub fn altitude_factor(&self, h: f64) -> f64 {
        match self.altitude_from {
            Some(from) => 1.0 + self.altitude_per_1000 * ((h - from).max(0.0) / 1000.0),
            None => 1.0,
        }
    }

    /// As a values file, headed by `comment` (`#` lines).
    pub fn to_text(&self, comment: &str) -> String {
        let mut out = String::new();
        for l in comment.lines() {
            let _ = writeln!(out, "#{}{l}", if l.is_empty() { "" } else { " " });
        }
        if !comment.is_empty() {
            out.push('\n');
        }
        let q = |s: &str| ols_code_common::toml::Value::String(s.to_owned()).to_string();
        let _ = writeln!(out, "format = \"{FORMAT}\"");
        let _ = writeln!(out, "version = {VERSION}");
        let _ = writeln!(out, "name = {}", q(&self.name));
        if self.altitude_from.is_some() || self.altitude_per_1000 != 0.0 {
            out.push_str("\n[altitude]\n");
            if let Some(f) = self.altitude_from {
                let _ = writeln!(out, "from = \"{f} m\"");
            }
            let _ = writeln!(out, "per_1000_m = {}", self.altitude_per_1000);
        }
        let pair = |c: [f64; 2]| {
            if c[0] == c[1] {
                format!("[\"{} m\"]", c[0])
            } else {
                format!("[\"{} m\", \"{} m\"]", c[0], c[1])
            }
        };
        for r in &self.voltages {
            let bil = |name: &str, b: Option<[f64; 2]>| match b {
                None => String::new(),
                Some([x, y]) if x == y => format!("{name} = [{x}]\n"),
                Some([x, y]) => format!("{name} = [{x}, {y}]\n"),
            };
            let more = |name: &str, c: Option<[f64; 2]>| {
                c.map_or(String::new(), |c| format!("{name} = {}\n", pair(c)))
            };
            let _ = write!(
                out,
                "\n[[voltage]]\nkv = {}\nc1 = {}\n{}c2 = {}\n{}{}{}",
                r.kv,
                pair(r.c1),
                bil("c1_bil_kv", r.c1_bil_kv),
                pair(r.c2),
                bil("c2_bil_kv", r.c2_bil_kv),
                more("c3", r.c3),
                more("c4", r.c4),
            );
        }
        for d in &self.diagrams {
            let _ = write!(
                out,
                "\n[[diagram]]\nname = {}\nwind_pressure = \"{} Pa\"\nline = [\n",
                q(&d.name),
                d.wind_pressure
            );
            for l in &d.lines {
                let w = l.w_max.map_or(String::new(), |w| format!("w_max = {w}, "));
                let _ = writeln!(out, "    {{ {w}k0 = {}, k90 = {} }},", l.k0, l.k90);
            }
            out.push_str("]\n");
        }
        out
    }
}

/// The comment heading a values file (its first `#` lines).
fn leading_comment(text: &str) -> String {
    text.lines()
        .take_while(|l| l.starts_with('#'))
        .map(|l| {
            l.trim_start_matches('#')
                .strip_prefix(' ')
                .unwrap_or(l.trim_start_matches('#'))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

thread_local! {
    /// The values last parsed (a run evaluates every pair with the same).
    static PARSED: RefCell<Option<(String, Result<Code, String>)>> = const { RefCell::new(None) };
}

/// `text` parsed, from the cache when it was the last.
fn parsed(text: &str) -> Result<Code, String> {
    PARSED.with(|p| {
        let mut p = p.borrow_mut();
        if let Some((t, c)) = p.as_ref()
            && t == text
        {
            return c.clone();
        }
        let c = Code::parse(text);
        *p = Some((text.to_owned(), c.clone()));
        c
    })
}

fn n(v: f64, decimals: usize) -> String {
    format!("{v:.decimals$}")
}

/// A table of the values, for the check's window.
pub struct Table {
    /// Title.
    pub title: String,
    /// Note.
    pub note: String,
    /// Column titles.
    pub columns: Vec<String>,
    /// Rows.
    pub rows: Vec<Vec<String>>,
    /// Editable columns.
    pub editable: Vec<bool>,
}

/// Each voltage row's lines: (row index, level index), one or two.
fn voltage_lines(code: &Code) -> Vec<(usize, usize)> {
    code.voltages
        .iter()
        .enumerate()
        .flat_map(|(k, r)| (0..if r.two_levels() { 2 } else { 1 }).map(move |i| (k, i)))
        .collect()
}

/// The values as tables: C by voltage (Table 2.4.1), the altitude rule,
/// each diagram's k lines.
pub fn tables(text: &str) -> Result<Vec<Table>, String> {
    let code = parsed(text)?;
    let s = |x: &str| x.to_owned();
    let mut out = vec![Table {
        title: s("C by nominal voltage, m (Table 2.4.1)"),
        note: s(
            "C₁ within one electrical system and towards earth, C₂ between systems; \
                 C₃, C₄ the same for brief approaches (wind; with phase spacers, 2.2.4). \
                 Where a voltage has two lines, the low and the high insulation level, \
                 each value for its lightning impulse withstand voltage (at kV).",
        ),
        columns: [
            "up to kV", "level", "C₁ m", "at kV", "C₂ m", "at kV", "C₃ m", "C₄ m",
        ]
        .map(s)
        .to_vec(),
        rows: voltage_lines(&code)
            .into_iter()
            .map(|(k, i)| {
                let r = &code.voltages[k];
                let opt = |v: Option<[f64; 2]>, d: usize| v.map_or(s("—"), |v| n(v[i], d));
                vec![
                    if i == 0 { n(r.kv, 0) } else { String::new() },
                    s(match (r.two_levels(), i) {
                        (false, _) => "—",
                        (_, 0) => "low",
                        _ => "high",
                    }),
                    n(r.c1[i], 2),
                    opt(r.c1_bil_kv, 0),
                    n(r.c2[i], 2),
                    opt(r.c2_bil_kv, 0),
                    opt(r.c3, 2),
                    opt(r.c4, 2),
                ]
            })
            .collect(),
        editable: vec![true, false, true, true, true, true, true, true],
    }];
    out.push(Table {
        title: s("Altitude"),
        note: s(
            "C holds up to this altitude (mean conductor height of the span), then grows \
                 by the share per 1000 m.",
        ),
        columns: vec![s("from m"), s("share per 1000 m")],
        rows: vec![vec![
            code.altitude_from.map_or(s("—"), |f| n(f, 0)),
            n(code.altitude_per_1000, 2),
        ]],
        editable: vec![true, true],
    });
    for d in &code.diagrams {
        out.push(Table {
            title: format!("k, diagram {} ({:.0} Pa)", d.name, d.wind_pressure),
            note: s(
                "k linear in β between 0° (one above the other) and 90° (side by side), \
                     by the conductor's swing class w = d/q (d in mm, q in kg/m).",
            ),
            columns: vec![s("w = d/q up to"), s("k at β 0°"), s("k at β 90°")],
            rows: d
                .lines
                .iter()
                .map(|l| {
                    vec![
                        l.w_max.map_or(s("above"), |w| n(w, 1)),
                        n(l.k0, 2),
                        n(l.k90, 2),
                    ]
                })
                .collect(),
            editable: vec![true, true, true],
        });
    }
    Ok(out)
}

/// `text` with the cell (`table`, `row`, `column`) of [`tables`] set to
/// `value`: the new file's text (its heading comment kept).
pub fn edit(
    text: &str,
    table: usize,
    row: usize,
    column: usize,
    value: &str,
) -> Result<String, String> {
    let mut code = parsed(text)?;
    let number = |what: &str| {
        let v = value.trim();
        v.parse::<f64>()
            .map_err(|_| format!("{what}: \"{value}\" is not a number"))
    };
    // "—" or empty: not given.
    let none = matches!(value.trim(), "" | "—" | "-");
    match table {
        0 => {
            let (k, i) = *voltage_lines(&code)
                .get(row)
                .ok_or_else(|| format!("no voltage line {row}"))?;
            let r = &mut code.voltages[k];
            let one = !r.two_levels();
            // A one-line row: both levels take the value.
            let set = |p: &mut [f64; 2], v: f64| {
                if one {
                    *p = [v, v];
                } else {
                    p[i] = v;
                }
            };
            let set_opt = |p: &mut Option<[f64; 2]>, v: Option<f64>| match v {
                None => *p = None,
                Some(v) => {
                    let mut q = p.unwrap_or([v, v]);
                    set(&mut q, v);
                    *p = Some(q);
                }
            };
            match column {
                0 => r.kv = number("kV")?,
                2 => set(&mut r.c1, number("C₁")?),
                3 => set_opt(&mut r.c1_bil_kv, (!none).then(|| number("kV")).transpose()?),
                4 => set(&mut r.c2, number("C₂")?),
                5 => set_opt(&mut r.c2_bil_kv, (!none).then(|| number("kV")).transpose()?),
                6 => set_opt(&mut r.c3, (!none).then(|| number("C₃")).transpose()?),
                7 => set_opt(&mut r.c4, (!none).then(|| number("C₄")).transpose()?),
                _ => return Err("this column is not edited".into()),
            }
        }
        1 => match column {
            0 => code.altitude_from = (!none).then(|| number("from")).transpose()?,
            1 => code.altitude_per_1000 = number("share")?,
            _ => return Err("no such column".into()),
        },
        t => {
            let d = code
                .diagrams
                .get_mut(t - 2)
                .ok_or_else(|| format!("no table {t}"))?;
            let last = d.lines.len().saturating_sub(1);
            let l = d
                .lines
                .get_mut(row)
                .ok_or_else(|| format!("no line {row}"))?;
            match column {
                0 if row == last && none => l.w_max = None,
                0 => l.w_max = Some(number("w")?),
                1 => l.k0 = number("k")?,
                2 => l.k90 = number("k")?,
                _ => return Err("no such column".into()),
            }
        }
    }
    code.validate()?;
    Ok(code.to_text(&leading_comment(text)))
}

/// A pair as OLS CAD measured it (the plugin interface's
/// `conductor-pair`, without the wire types).
pub struct Pair<'a> {
    /// In one electrical system (or towards earth).
    pub same_system: bool,
    /// Nominal voltage of each, kV.
    pub kv: [Option<f64>; 2],
    /// Plugin columns of each.
    pub fields: [&'a [(String, String)]; 2],
    /// Mean conductor height, m.
    pub altitude: f64,
    /// Diameter of each, mm.
    pub diameter_mm: [f64; 2],
    /// Mass of each, kg/m.
    pub mass_kg_m: [f64; 2],
    /// Largest sag of each between spacers, m (spacers join them).
    pub spacer_sag: Option<[f64; 2]>,
}

/// What is required at each station: per station z, k of each
/// conductor, C used; and the rule.
pub fn evaluate(
    text: &str,
    diagram: usize,
    pair: &Pair,
    stations: &[f64],
) -> Result<(Vec<f64>, String), String> {
    let code = parsed(text)?;
    let d = code
        .diagrams
        .get(diagram)
        .ok_or_else(|| format!("no diagram {} in {}", diagram + 1, code.name))?;
    let level_of = |i: usize| {
        pair.fields[i]
            .iter()
            .find(|(k, _)| k == LEVEL_FIELD)
            .and_then(|(_, v)| Level::parse(v))
            .unwrap_or_default()
    };
    // The insulation level of the circuit of the higher voltage (equal:
    // high when either is).
    let level = match pair.kv {
        [Some(x), Some(y)] if x > y => level_of(0),
        [Some(x), Some(y)] if y > x => level_of(1),
        [Some(_), Some(_)] => {
            if level_of(0) == Level::High || level_of(1) == Level::High {
                Level::High
            } else {
                Level::Low
            }
        }
        [Some(_), None] => level_of(0),
        _ => level_of(1),
    };
    let kv = pair.kv[0].unwrap_or(0.0).max(pair.kv[1].unwrap_or(0.0));
    let factor = code.altitude_factor(pair.altitude);
    let c_still = code.c(pair.same_system, kv, level) * factor;
    let w = |i: usize| {
        if pair.mass_kg_m[i] > 0.0 {
            pair.diameter_mm[i] / pair.mass_kg_m[i]
        } else {
            0.0
        }
    };
    let (wa, wb) = (w(0), w(1));
    let mut out = Vec::with_capacity(stations.len() / 6 * 4);
    let rule = match pair.spacer_sag {
        None => {
            for s in stations.as_chunks::<6>().0 {
                let (beta, f, lk) = (s[1], [s[2], s[3]], [s[4], s[5]]);
                let (ka, kb) = (d.k(wa, beta), d.k(wb, beta));
                let z = (ka * (f[0] + lk[0]).max(0.0).sqrt())
                    .max(kb * (f[1] + lk[1]).max(0.0).sqrt())
                    + c_still;
                out.extend([z, ka, kb, c_still]);
            }
            "2.2.1".to_owned()
        }
        Some(df) => {
            let brief = code.c_brief(pair.same_system, kv, level).ok_or_else(|| {
                format!(
                    "{kv} kV: no {} in the values (phase spacers, 2.2.4)",
                    if pair.same_system { "C₃" } else { "C₄" }
                )
            })? * factor;
            for s in stations.as_chunks::<6>().0 {
                let beta = s[1];
                let (ka, kb) = (d.k(wa, beta), d.k(wb, beta));
                let z = ((ka * df[0].max(0.0).sqrt()).max(kb * df[1].max(0.0).sqrt()) + brief)
                    .max(c_still)
                    .max(SPACER_MINIMUM);
                out.extend([z, ka, kb, brief]);
            }
            "2.2.4 (phase spacers)".to_owned()
        }
    };
    Ok((out, rule))
}

/// How the check computes, in words (paragraphs).
pub const EXPLANATION: &str = "LeV Annex 6, 2.2.1, still air: the conductors of a span must be at \
least z = k·√(f + l_k) + C apart (2.2.3: the larger z of the two conductors).

k: by β (0° one above the other, 90° side by side) on the line of the conductor's swing class \
w = d/q of the diagram chosen. C: C₁ within one electrical system and towards earth (earth \
wires), C₂ between different systems, by the higher nominal voltage and, where it has two \
lines, the circuit's insulation level (low / high: the lightning impulse withstand voltage each \
value is for; not set: high), +14 % per 1000 m of mean conductor height above 1000 m.

2.2.4, conductors joined by phase spacers in the span: z = k·√d_f + C₃, at least C₁ (between \
systems + C₄, at least C₂), and at least 0.90 m — d_f the largest sag between the spacers (each \
conductor's, its stretches between its spacer clamps and the span's clamps).";

#[cfg(test)]
mod tests {
    use super::*;

    fn lev() -> Code {
        Code::parse(VALUES).unwrap()
    }

    /// The shipped LeV values: C by voltage and level, the next row up
    /// between rows, the altitude rule, k by β and swing class.
    #[test]
    fn the_lev_values() {
        let c = lev();
        assert_eq!(c.c(true, 380.0, Level::Low), 2.64);
        assert_eq!(c.c(true, 380.0, Level::High), 2.90);
        assert_eq!(c.c(false, 380.0, Level::High), 3.64);
        // 110 and 132 kV: one line each, C₁ and C₃ their own (450 / 550
        // kV), C₂ and C₄ the same (650 kV).
        assert_eq!(c.c(true, 110.0, Level::Low), 0.92);
        assert_eq!(c.c(true, 110.0, Level::High), 0.92);
        assert_eq!(c.c(true, 132.0, Level::Low), 1.12);
        assert_eq!(c.c(false, 110.0, Level::Low), 1.38);
        assert_eq!(c.c(false, 132.0, Level::High), 1.38);
        // 65 kV: the 110 kV row.
        assert_eq!(c.c(true, 65.0, Level::Low), 0.92);
        let row = |kv: f64| c.voltages.iter().find(|r| r.kv == kv).unwrap();
        assert!(row(220.0).two_levels() && !row(132.0).two_levels());
        assert_eq!(row(220.0).c1_bil_kv, Some([950.0, 1050.0]));
        assert_eq!(row(220.0).c2_bil_kv, Some([1050.0, 1175.0]));
        assert_eq!(row(380.0).c1_bil_kv, Some([1300.0, 1425.0]));
        assert_eq!(row(380.0).c2_bil_kv, Some([1550.0, 1675.0]));
        assert_eq!(c.c(false, 220.0, Level::Low), 2.29);
        assert_eq!(row(110.0).c3, Some([0.61, 0.61]));
        assert_eq!(row(132.0).c3, Some([0.76, 0.76]));
        assert_eq!(row(132.0).c4, Some([0.89, 0.89]));
        assert_eq!(row(220.0).c3, Some([1.35, 1.60]));
        assert_eq!(row(220.0).c4, Some([1.60, 1.90]));
        assert_eq!(row(380.0).c3, Some([2.09, 2.33]));
        assert_eq!(row(380.0).c4, Some([2.30, 2.86]));
        assert!(c.voltages.iter().all(|r| r.c3.is_some() && r.c4.is_some()));
        assert_eq!(row(3.0).c1_bil_kv, Some([40.0, 40.0]));
        assert!((c.altitude_factor(500.0) - 1.0).abs() < 1e-12);
        assert!((c.altitude_factor(2000.0) - 1.14).abs() < 1e-12);
        let d = &c.diagrams[0];
        assert!((d.k(10.0, 0.0) - 0.70).abs() < 1e-12);
        assert!((d.k(10.0, 90.0) - 0.60).abs() < 1e-12);
        assert!((d.k(20.0, 45.0) - 0.70).abs() < 1e-12);
        assert!((d.k(50.0, 0.0) - 0.95).abs() < 1e-12);
        assert!((c.diagrams[1].k(25.0, 0.0) - 0.85).abs() < 1e-12);
    }

    /// Written back and read again: the same values; a cell edited
    /// changes that value only, the heading comment kept; a wrong one is
    /// refused.
    #[test]
    fn written_edited_and_read_back() {
        let c = lev();
        assert_eq!(Code::parse(&c.to_text("x")).unwrap(), c);
        let t = tables(VALUES).unwrap();
        assert_eq!(t[0].rows.len(), c.voltages.len() + 2);
        // 220 kV high: the line after 220 kV low.
        let line = t[0].rows.iter().position(|r| r[0] == "220").unwrap() + 1;
        assert_eq!(t[0].rows[line][1], "high");
        let edited = edit(VALUES, 0, line, 2, "2.20").unwrap();
        assert!(edited.starts_with("# Distance between conductors"));
        let e = Code::parse(&edited).unwrap();
        let r = e.voltages.iter().find(|r| r.kv == 220.0).unwrap();
        assert_eq!(r.c1, [1.93, 2.20]);
        // A one-line row: both levels.
        let line = t[0].rows.iter().position(|r| r[0] == "110").unwrap();
        let e = Code::parse(&edit(VALUES, 0, line, 6, "0.7").unwrap()).unwrap();
        assert_eq!(
            e.voltages.iter().find(|r| r.kv == 110.0).unwrap().c3,
            Some([0.7, 0.7])
        );
        assert!(edit(VALUES, 0, line, 2, "-1").is_err());
        assert!(edit(VALUES, 0, line, 2, "abc").is_err());
        assert!(edit(VALUES, 2, 0, 1, "0.72").is_ok());
        assert!(edit(VALUES, 1, 0, 1, "0.2").is_ok());
    }

    fn station(beta: f64, f: f64, lk: f64) -> Vec<f64> {
        vec![6.0, beta, f, f, lk, lk]
    }

    fn pair<'a>(
        same: bool,
        kv: f64,
        fields: &'a [(String, String)],
        spacers: Option<[f64; 2]>,
    ) -> Pair<'a> {
        Pair {
            same_system: same,
            kv: [Some(kv), Some(kv)],
            fields: [fields, fields],
            altitude: 500.0,
            diameter_mm: [30.0, 30.0],
            mass_kg_m: [3.0, 3.0],
            spacer_sag: spacers,
        }
    }

    /// 2.2.1: 380 kV, f 12 m, l_k 4.5 m, β 0°, w 10: z = 0.70·√16.5 +
    /// 2.90 (high, not set) or 2.64 (low); between systems 3.64.
    #[test]
    fn the_distance_in_the_span() {
        let none: Vec<(String, String)> = Vec::new();
        let (v, rule) = evaluate(
            VALUES,
            0,
            &pair(true, 380.0, &none, None),
            &station(0.0, 12.0, 4.5),
        )
        .unwrap();
        assert_eq!(rule, "2.2.1");
        assert!((v[0] - (0.70 * 16.5f64.sqrt() + 2.90)).abs() < 1e-9);
        assert_eq!(&v[1..], &[0.70, 0.70, 2.90]);
        let low = vec![(LEVEL_FIELD.to_owned(), " Low ".to_owned())];
        let (v, _) = evaluate(
            VALUES,
            0,
            &pair(true, 380.0, &low, None),
            &station(0.0, 12.0, 4.5),
        )
        .unwrap();
        assert!((v[0] - (0.70 * 16.5f64.sqrt() + 2.64)).abs() < 1e-9);
        let (v, _) = evaluate(
            VALUES,
            0,
            &pair(false, 380.0, &none, None),
            &station(90.0, 12.0, 4.5),
        )
        .unwrap();
        assert!((v[0] - (0.60 * 16.5f64.sqrt() + 3.64)).abs() < 1e-9);
        // Above 1000 m: +14 % per 1000 m.
        let mut p = pair(true, 380.0, &low, None);
        p.altitude = 2000.0;
        let (v, _) = evaluate(VALUES, 0, &p, &station(0.0, 12.0, 4.5)).unwrap();
        assert!((v[3] - 2.64 * 1.14).abs() < 1e-9);
        assert!(evaluate(VALUES, 5, &p, &station(0.0, 1.0, 1.0)).is_err());
    }

    /// 2.2.4, phase spacers: z = k·√d_f + C₃ ≥ C₁, at least 0.90 m (no
    /// l_k, not the local sag); between systems C₄ ≥ C₂.
    #[test]
    fn with_phase_spacers() {
        let low = vec![(LEVEL_FIELD.to_owned(), "low".to_owned())];
        // 380 kV low: 0.70·√9 + 2.09 = 4.19 ≥ C₁ 2.64.
        let (v, rule) = evaluate(
            VALUES,
            0,
            &pair(true, 380.0, &low, Some([9.0, 4.0])),
            &station(0.0, 12.0, 4.5),
        )
        .unwrap();
        assert!(rule.starts_with("2.2.4"));
        assert!((v[0] - (0.70 * 3.0 + 2.09)).abs() < 1e-9, "{v:?}");
        assert_eq!(v[3], 2.09);
        // A small d_f: C₁ governs.
        let (v, _) = evaluate(
            VALUES,
            0,
            &pair(true, 380.0, &low, Some([0.25, 0.25])),
            &station(0.0, 12.0, 4.5),
        )
        .unwrap();
        assert!((v[0] - 2.64).abs() < 1e-9);
        // Between systems: C₄ 2.30 ≥ C₂ 3.37.
        let (v, _) = evaluate(
            VALUES,
            0,
            &pair(false, 380.0, &low, Some([9.0, 9.0])),
            &station(0.0, 12.0, 4.5),
        )
        .unwrap();
        assert!((v[0] - (0.70 * 3.0 + 2.30)).abs() < 1e-9);
        // Low voltage: at least 0.90 m.
        let (v, _) = evaluate(
            VALUES,
            0,
            &pair(true, 10.0, &low, Some([0.1, 0.1])),
            &station(0.0, 1.0, 0.0),
        )
        .unwrap();
        assert!((v[0] - 0.90).abs() < 1e-9);
    }
}
