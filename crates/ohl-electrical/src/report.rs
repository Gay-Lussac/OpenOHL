// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The line parameters as table rows and CSV, shared by `ohl electrical
//! parameters` and the Line parameters window.

use std::fmt::Write as _;

use crate::parameters::{CircuitParameters, LineParameters, Totals};

/// One row of the parameters table: per kilometre and in total.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// `circuit`, `stretch` or `coupling`.
    pub kind: &'static str,
    /// The circuit (for a coupling: both, `A – B`).
    pub circuit: String,
    /// The structure it starts at (a stretch), else empty.
    pub from: String,
    /// The structure it ends at (a stretch), else empty.
    pub to: String,
    /// Its sections (a stretch), else empty.
    pub sections: Vec<String>,
    /// Horizontal length, km.
    pub length_km: f64,
    /// Positive sequence R, X Ω/km and B µS/km (ideally transposed);
    /// `None` unless three phases (and for a coupling).
    pub positive: Option<[f64; 3]>,
    /// Zero sequence R, X Ω/km and B µS/km (ideally transposed; a
    /// coupling: the zero sequence mutual R, X and 0).
    pub zero: Option<[f64; 3]>,
    /// The largest coupling between the sequences as built, relative to
    /// the positive sequence impedance.
    pub unbalance: Option<f64>,
}

impl Row {
    /// Totals over its length: (R1, X1, R0, X0) Ω and (B1, B0) µS.
    pub fn totals(&self) -> Option<([f64; 4], [f64; 2])> {
        let (p, z) = (self.positive?, self.zero?);
        let l = self.length_km;
        Some((
            [p[0] * l, p[1] * l, z[0] * l, z[1] * l],
            [p[2] * l, z[2] * l],
        ))
    }
}

fn sequence_row(
    kind: &'static str,
    c: &CircuitParameters,
    totals: &Totals,
    from: &str,
    to: &str,
    sections: &[String],
) -> Row {
    let km = totals.length / 1000.0;
    let seq = c
        .abc()
        .and_then(|abc| totals.sequence(c.frequency_hz, &abc));
    let per = |x: f64| if km > 0.0 { x / km } else { 0.0 };
    Row {
        kind,
        circuit: c.key.label(),
        from: from.to_owned(),
        to: to.to_owned(),
        sections: sections.to_vec(),
        length_km: km,
        positive: seq.map(|s| {
            [
                per(s.z1_transposed.re),
                per(s.z1_transposed.im),
                per(s.b1_transposed) * 1e6,
            ]
        }),
        zero: seq.map(|s| {
            [
                per(s.z0_transposed.re),
                per(s.z0_transposed.im),
                per(s.b0_transposed) * 1e6,
            ]
        }),
        unbalance: seq.map(|s| s.unbalance),
    }
}

/// Every circuit's row followed by its stretches' rows, then the
/// couplings'. `sections`: when not empty, only the stretches holding one
/// of them (and their circuits).
pub fn rows(p: &LineParameters, sections: &[String]) -> Vec<Row> {
    let mut out = Vec::new();
    for c in &p.circuits {
        let stretches: Vec<_> = c
            .stretches
            .iter()
            .filter(|s| sections.is_empty() || s.sections.iter().any(|x| sections.contains(x)))
            .collect();
        if !sections.is_empty() && stretches.is_empty() {
            continue;
        }
        let (from, to) = (
            c.stretches.first().map(|s| s.from.as_str()).unwrap_or(""),
            c.stretches.last().map(|s| s.to.as_str()).unwrap_or(""),
        );
        out.push(sequence_row("circuit", c, &c.total, from, to, &[]));
        for s in stretches {
            out.push(sequence_row(
                "stretch",
                c,
                &s.totals,
                &s.from,
                &s.to,
                &s.sections,
            ));
        }
    }
    for k in &p.couplings {
        if !sections.is_empty() {
            continue;
        }
        let km = k.length / 1000.0;
        let z0m = k.z0m();
        out.push(Row {
            kind: "coupling",
            circuit: format!("{} – {}", k.a.label(), k.b.label()),
            from: String::new(),
            to: String::new(),
            sections: Vec::new(),
            length_km: km,
            positive: None,
            zero: Some([z0m.re / km, z0m.im / km, 0.0]),
            unbalance: None,
        });
    }
    out
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

/// The rows as CSV: per kilometre and in total, then the notes as
/// comment lines (`# …`).
pub fn csv(p: &LineParameters, sections: &[String]) -> String {
    let mut out = String::from(
        "kind,circuit,from,to,sections,length_km,r1_ohm_per_km,x1_ohm_per_km,b1_us_per_km,r0_ohm_per_km,x0_ohm_per_km,b0_us_per_km,r1_ohm,x1_ohm,b1_us,r0_ohm,x0_ohm,b0_us,unbalance\n",
    );
    let f = |x: Option<f64>| x.map(|v| format!("{v:.6}")).unwrap_or_default();
    for r in rows(p, sections) {
        let t = r.totals();
        let _ = writeln!(
            out,
            "{},{},{},{},{},{:.6},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            r.kind,
            csv_field(&r.circuit),
            csv_field(&r.from),
            csv_field(&r.to),
            csv_field(&r.sections.join(" ")),
            r.length_km,
            f(r.positive.map(|x| x[0])),
            f(r.positive.map(|x| x[1])),
            f(r.positive.map(|x| x[2])),
            f(r.zero.map(|x| x[0])),
            f(r.zero.map(|x| x[1])),
            f(r.zero.map(|x| x[2])),
            f(t.map(|x| x.0[0])),
            f(t.map(|x| x.0[1])),
            f(t.map(|x| x.1[0])),
            f(t.map(|x| x.0[2])),
            f(t.map(|x| x.0[3])),
            f(t.map(|x| x.1[1])),
            f(r.unbalance),
        );
    }
    for n in &p.notes {
        let _ = writeln!(out, "# {n}");
    }
    out
}
