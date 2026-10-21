// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Bill of materials and cost estimate of a line (Phase 5).
//!
//! [`bill_of_materials`] counts what a line is built from:
//!
//! - **structures** by type and extension;
//! - **insulator assemblies** by id: one per attachment a section is
//!   strung through (a bundle hangs from one assembly); strings given in a
//!   pylon file or as geometry are listed by kind and length, e.g.
//!   `string suspension 3.500 m`;
//! - **wire lengths** per cable and role (conductor, earth wire, jumper):
//!   the unstressed (cut) lengths of every span at the section's stringing
//!   condition — given for a section clipped in, computed with
//!   [`ohl_line::solve::clip_lengths`] for one in the blocks — times the
//!   subconductors of the bundle; jumpers from their cut length, or chord
//!   plus the parabolic sag allowance \\( 8 d^2 / 3 c \\);
//! - **hardware** by rule ([`cost::Hardware`]): spacers every so many
//!   metres on bundles, dampers per subconductor and span;
//! - **spans** between structures (stringing labour).
//!
//! [`CostEstimate::new`] prices it with a cost file ([`cost::CostData`]);
//! items without a price are listed. Both are plain data (serde), written
//! as a text table ([`CostEstimate::text`]) or CSV ([`CostEstimate::csv`]).

pub mod cost;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use ohl_line::{InsulatorString, Line, Stringing, Structure};
use serde::Serialize;

pub use cost::CostData;

/// Structures of one type and extension.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StructureCount {
    /// Structure type id (none: a structure given by its attachments).
    pub structure_type: Option<String>,
    /// Extension (none: the type as it is).
    pub extension: Option<String>,
    /// How many.
    pub count: usize,
    /// Their ids.
    pub ids: Vec<String>,
}

/// Insulator assemblies of one kind.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InsulatorCount {
    /// Insulator assembly id, or the inline string's name.
    pub id: String,
    /// How many.
    pub count: usize,
}

/// Wire of one cable in one role.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CableLength {
    /// Cable id.
    pub cable: String,
    /// `"conductor"`, `"earth wire"`, `"wire"` (a line without electrical
    /// systems) or `"jumper"`.
    pub role: String,
    /// Total length, m (every subconductor).
    pub length_m: f64,
    /// Sections or jumpers counted.
    pub items: usize,
}

/// The cut length of one section.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SectionLength {
    /// Section id.
    pub section: String,
    /// Cable id.
    pub cable: String,
    /// Subconductors.
    pub bundle: u32,
    /// Unstressed length of each span, per wire, m.
    pub spans_m: Vec<f64>,
    /// At this temperature, °C.
    pub temperature: f64,
    /// How the lengths were found: `"given"` (clipped in), `"stringing"`
    /// (sagged to the section's condition), `"chord"` (estimate: the
    /// section could not be solved).
    pub method: String,
}

/// Hardware of one kind.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HardwareCount {
    /// What it is.
    pub item: String,
    /// How many.
    pub count: usize,
}

/// A bill of materials.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bom {
    /// Line name.
    pub line: String,
    /// Structures by type and extension.
    pub structures: Vec<StructureCount>,
    /// Insulator assemblies.
    pub insulators: Vec<InsulatorCount>,
    /// Wire lengths by cable and role.
    pub cables: Vec<CableLength>,
    /// Cut lengths per section.
    pub sections: Vec<SectionLength>,
    /// Hardware.
    pub hardware: Vec<HardwareCount>,
    /// Spans between two structures.
    pub spans: usize,
    /// What was estimated or left out.
    pub notes: Vec<String>,
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Name of a string in the bill: its insulator assembly, else `string
/// <kind> <length>` (kind: strain for strain strings and those not
/// hanging below their top, else suspension).
fn insulator_name(s: &Structure, string: &InsulatorString) -> Option<String> {
    if let Some(id) = &string.insulator {
        return Some(id.clone());
    }
    let top = s.node(&string.top)?.position();
    let wire = s.wire(&string.wire)?.position;
    let length = distance(top, wire);
    let d = [wire[0] - top[0], wire[1] - top[1], wire[2] - top[2]];
    let kind = if string.strain || !(d[2] < 0.0 && d[0].hypot(d[1]) < -d[2]) {
        "strain"
    } else {
        "suspension"
    };
    // To the millimetre, as pylon files give them.
    let length = (length * 1000.0).round() / 1000.0;
    Some(format!("string {kind} {length:.3} m"))
}

/// Parabolic length of a slack cable over a chord `c` with sag `d`.
fn parabolic_length(c: f64, d: f64) -> f64 {
    if c > 0.0 {
        c + 8.0 * d * d / (3.0 * c)
    } else {
        0.0
    }
}

/// The bill of materials of a line whose typed structures are resolved
/// ([`Line::resolve`]); cables from `library`, hardware by `hardware`'s
/// rules.
///
/// Expected cost: one still-air solve per section in the blocks (a few
/// milliseconds each).
pub fn bill_of_materials(
    line: &Line,
    library: &ohl_cables::Library,
    hardware: &cost::Hardware,
) -> Bom {
    let mut notes = Vec::new();

    let mut structures: BTreeMap<(Option<String>, Option<String>), Vec<String>> = BTreeMap::new();
    for s in &line.structures {
        structures
            .entry((s.structure_type.clone(), s.extension.clone()))
            .or_default()
            .push(s.id.clone());
    }
    let structures = structures
        .into_iter()
        .map(|((structure_type, extension), ids)| StructureCount {
            structure_type,
            extension,
            count: ids.len(),
            ids,
        })
        .collect();

    // The strings holding the wires strung (each once, also when sections
    // share them).
    let mut used: BTreeSet<(String, usize)> = BTreeSet::new();
    for sec in &line.sections {
        for p in &sec.points {
            let wire = line
                .structure(&p.structure)
                .and_then(|s| Some((s, s.wire_at_set(p.set)?)));
            match wire {
                Some((s, w)) => {
                    for i in s.component(&w.label).strings {
                        used.insert((s.id.clone(), i));
                    }
                }
                None => notes.push(format!(
                    "{} set {}: no such wire point (strings not counted)",
                    p.structure, p.set
                )),
            }
        }
    }
    let mut insulators: BTreeMap<String, usize> = BTreeMap::new();
    for (structure, i) in &used {
        let s = line.structure(structure).expect("found above");
        if let Some(name) = insulator_name(s, &s.strings[*i]) {
            *insulators.entry(name).or_default() += 1;
        }
    }
    let insulators = insulators
        .into_iter()
        .map(|(id, count)| InsulatorCount { id, count })
        .collect();

    let roles = if line.systems.is_empty() {
        None
    } else {
        match line.section_phases() {
            Ok(p) => Some(p),
            Err(e) => {
                notes.push(format!("electrical systems: {e} (roles not told apart)"));
                None
            }
        }
    };
    let role = |id: &str| match &roles {
        None => "wire",
        Some(p) => match p.get(id) {
            Some(Some(_)) => "conductor",
            _ => "earth wire",
        },
    };

    let mut sections = Vec::new();
    let mut cables: BTreeMap<(String, String), (f64, usize)> = BTreeMap::new();
    let mut spacers = 0;
    let mut dampers = 0;
    let mut spans = BTreeSet::new();
    for s in &line.sections {
        for w in s.points.windows(2) {
            if w[0].structure != w[1].structure {
                let mut pair = [w[0].structure.clone(), w[1].structure.clone()];
                pair.sort();
                spans.insert(pair);
            }
        }
        let chords = || -> Vec<f64> {
            s.points
                .windows(2)
                .map(|w| match (line.wire_point(&w[0]), line.wire_point(&w[1])) {
                    (Some(a), Some(b)) => distance(a.position, b.position),
                    _ => 0.0,
                })
                .collect()
        };
        let (spans_m, temperature, method) = match &s.stringing {
            Stringing::UnstressedLengths {
                lengths,
                temperature,
                ..
            } => (lengths.clone(), *temperature, "given"),
            other => {
                let solved = library
                    .get(&s.cable)
                    .ok_or_else(|| format!("wire type {} not in the wire library", s.cable))
                    .and_then(|c| {
                        ohl_line::solve::clip_lengths(line, s, c).map_err(|e| e.to_string())
                    });
                match solved {
                    Ok((l, t)) => (l, t, "stringing"),
                    Err(e) => {
                        notes.push(format!(
                            "section {}: {e}; lengths estimated from the chords",
                            s.id
                        ));
                        (chords(), other.temperature(), "chord")
                    }
                }
            }
        };
        let bundle = s.bundle.max(1);
        let total: f64 = spans_m.iter().sum::<f64>() * f64::from(bundle);
        let e = cables
            .entry((s.cable.clone(), role(&s.id).to_owned()))
            .or_default();
        e.0 += total;
        e.1 += 1;
        if bundle >= 2
            && let Some(interval) = hardware.spacer_interval
        {
            let i = interval.si();
            spacers += spans_m
                .iter()
                .map(|l| ((l / i).ceil() as usize).saturating_sub(1))
                .sum::<usize>();
        }
        dampers += hardware.dampers_per_span as usize * bundle as usize * spans_m.len();
        sections.push(SectionLength {
            section: s.id.clone(),
            cable: s.cable.clone(),
            bundle,
            spans_m,
            temperature,
            method: method.into(),
        });
    }
    for j in &line.jumpers {
        let ends = (
            ohl_line::jumper::approximate_end(line, &j.from, 0),
            ohl_line::jumper::approximate_end(line, &j.to, 0),
        );
        let length = match (j.length, ends) {
            (Some(l), _) => l,
            (None, (Some(a), Some(b))) => parabolic_length(distance(a, b), j.sag.unwrap_or(0.0)),
            _ => {
                notes.push(format!("jumper {}: ends not found (not counted)", j.id));
                continue;
            }
        };
        let e = cables
            .entry((j.cable.clone(), "jumper".to_owned()))
            .or_default();
        e.0 += length * f64::from(line.jumper_bundle(j).count.max(1));
        e.1 += 1;
    }
    let cables = cables
        .into_iter()
        .map(|((cable, role), (length_m, items))| CableLength {
            cable,
            role,
            length_m,
            items,
        })
        .collect();
    let mut hw = Vec::new();
    if spacers > 0 {
        hw.push(HardwareCount {
            item: "spacer".into(),
            count: spacers,
        });
    }
    if dampers > 0 {
        hw.push(HardwareCount {
            item: "damper".into(),
            count: dampers,
        });
    }
    Bom {
        line: line.name.clone(),
        structures,
        insulators,
        cables,
        sections,
        hardware: hw,
        spans: spans.len(),
        notes,
    }
}

/// One line of the estimate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CostLine {
    /// `"structures"`, `"foundations"`, `"erection"`, `"insulators"`,
    /// `"wire"`, `"stringing"`, `"hardware"` or `"fixed"`.
    pub category: String,
    /// What it is.
    pub item: String,
    /// How much.
    pub quantity: f64,
    /// Unit of the quantity (`"pcs"`, `"m"`, `"spans"`).
    pub unit: String,
    /// Price per unit (none: not in the cost file).
    pub unit_price: Option<f64>,
    /// Quantity × unit price.
    pub amount: Option<f64>,
}

/// A bill of materials priced (or listed without prices).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CostEstimate {
    /// The bill of materials.
    pub bom: Bom,
    /// Currency label (empty without a cost file).
    pub currency: String,
    /// Lines, in the order of the bill.
    pub lines: Vec<CostLine>,
    /// Sum of the priced lines.
    pub total: f64,
    /// Items without a price (with a cost file).
    pub missing: Vec<String>,
    /// Whether a cost file was used.
    pub priced: bool,
}

fn structure_name(s: &StructureCount) -> String {
    let t = s.structure_type.as_deref().unwrap_or("(no type)");
    match &s.extension {
        Some(e) => format!("{t} ext {e}"),
        None => t.to_owned(),
    }
}

impl CostEstimate {
    /// Prices `bom` with `cost` (none: quantities only).
    pub fn new(bom: Bom, cost: Option<&CostData>) -> Self {
        let mut lines = Vec::new();
        let mut missing = Vec::new();
        let mut add = |category: &str,
                       item: String,
                       quantity: f64,
                       unit: &str,
                       unit_price: Option<f64>,
                       required: bool| {
            if cost.is_some() && unit_price.is_none() && required {
                missing.push(format!("{category}: {item}"));
            }
            lines.push(CostLine {
                category: category.into(),
                item,
                quantity,
                unit: unit.into(),
                unit_price,
                amount: unit_price.map(|p| p * quantity),
            });
        };
        for s in &bom.structures {
            let n = s.count as f64;
            let name = structure_name(s);
            let ty = s.structure_type.as_deref().unwrap_or("");
            let supply = cost.and_then(|c| c.structure_supply(ty, s.extension.as_deref()));
            add("structures", name.clone(), n, "pcs", supply, true);
            if let Some(f) = cost
                .and_then(|c| c.structure(ty))
                .and_then(|p| p.foundation)
            {
                add("foundations", name.clone(), n, "pcs", Some(f), false);
            }
            let erection = cost.and_then(|c| {
                c.structure(ty)
                    .and_then(|p| p.erection)
                    .or(c.labour.per_structure)
            });
            if erection.is_some() {
                add("erection", name, n, "pcs", erection, false);
            }
        }
        for i in &bom.insulators {
            let p = cost.and_then(|c| c.insulator(&i.id));
            add("insulators", i.id.clone(), i.count as f64, "pcs", p, true);
        }
        for c in &bom.cables {
            let price = cost.and_then(|x| x.cable(&c.cable));
            let item = format!("{} ({})", c.cable, c.role);
            add(
                "wire",
                item.clone(),
                c.length_m,
                "m",
                price.map(|p| p.price_per_m),
                true,
            );
            if let Some(p) = price.and_then(|p| p.stringing_per_m) {
                add("stringing", item, c.length_m, "m", Some(p), false);
            }
        }
        for h in &bom.hardware {
            let p = cost.and_then(|c| match h.item.as_str() {
                "spacer" => c.hardware.spacer_price,
                "damper" => c.hardware.damper_price,
                _ => None,
            });
            add("hardware", h.item.clone(), h.count as f64, "pcs", p, true);
        }
        if let Some(p) = cost.and_then(|c| c.labour.per_span) {
            add(
                "stringing",
                "spans".into(),
                bom.spans as f64,
                "spans",
                Some(p),
                false,
            );
        }
        for f in cost.map(|c| c.fixed.as_slice()).unwrap_or_default() {
            add("fixed", f.item.clone(), 1.0, "pcs", Some(f.price), false);
        }
        let total = lines.iter().filter_map(|l| l.amount).sum();
        Self {
            bom,
            currency: cost.map(|c| c.currency.clone()).unwrap_or_default(),
            lines,
            total,
            missing,
            priced: cost.is_some(),
        }
    }

    /// A text table: one row per line, the total and what has no price.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "Bill of materials: {}", self.bom.line);
        let rows: Vec<[String; 6]> = self
            .lines
            .iter()
            .map(|l| {
                [
                    l.category.clone(),
                    l.item.clone(),
                    quantity(l.quantity, &l.unit),
                    l.unit.clone(),
                    l.unit_price.map_or(String::new(), |p| format!("{p:.2}")),
                    l.amount.map_or(String::new(), |a| format!("{a:.2}")),
                ]
            })
            .collect();
        let head = [
            "category",
            "item",
            "quantity",
            "unit",
            format!("unit price {}", self.currency).trim(),
            format!("amount {}", self.currency).trim(),
        ]
        .map(str::to_owned);
        let shown = if self.priced { 6 } else { 4 };
        let mut width = [0usize; 6];
        for r in std::iter::once(&head).chain(&rows) {
            for (k, c) in r.iter().enumerate() {
                width[k] = width[k].max(c.chars().count());
            }
        }
        let row = |out: &mut String, r: &[String; 6]| {
            let cells: Vec<String> = (0..shown)
                .map(|k| {
                    if k >= 2 && k != 3 {
                        format!("{:>w$}", r[k], w = width[k])
                    } else {
                        format!("{:<w$}", r[k], w = width[k])
                    }
                })
                .collect();
            let _ = writeln!(out, "  {}", cells.join("  ").trim_end());
        };
        row(&mut out, &head);
        for r in &rows {
            row(&mut out, r);
        }
        let _ = writeln!(out, "  spans between structures: {}", self.bom.spans);
        if self.priced {
            let _ = writeln!(out, "Total: {:.2} {}", self.total, self.currency);
            if !self.missing.is_empty() {
                let _ = writeln!(out, "No price in the cost file ({}):", self.missing.len());
                for m in &self.missing {
                    let _ = writeln!(out, "  {m}");
                }
            }
        }
        for n in &self.bom.notes {
            let _ = writeln!(out, "note: {n}");
        }
        out
    }

    /// CSV: `category,item,quantity,unit,unit_price,amount,currency`, a
    /// total row when priced.
    pub fn csv(&self) -> String {
        let mut out = String::from("category,item,quantity,unit,unit_price,amount,currency\n");
        let opt = |x: Option<f64>| x.map_or(String::new(), |v| format!("{v}"));
        for l in &self.lines {
            let _ = writeln!(
                out,
                "{},{},{},{},{},{},{}",
                csv_field(&l.category),
                csv_field(&l.item),
                l.quantity,
                l.unit,
                opt(l.unit_price),
                opt(l.amount),
                csv_field(&self.currency)
            );
        }
        if self.priced {
            let _ = writeln!(
                out,
                "total,,,,,{},{}",
                self.total,
                csv_field(&self.currency)
            );
        }
        out
    }
}

fn quantity(q: f64, unit: &str) -> String {
    if unit == "m" {
        format!("{q:.1}")
    } else {
        format!("{q}")
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

#[cfg(test)]
mod tests;
