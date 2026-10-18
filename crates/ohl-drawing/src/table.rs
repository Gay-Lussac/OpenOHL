// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The profile as numbers: CSV and the OHL profile XML.

use std::fmt::Write as _;

use crate::layout::station_runs;
use crate::model::escape;
use crate::{Options, RouteDrawing};

/// The options' station range, or everything.
fn range(o: &Options) -> (f64, f64) {
    o.stations
        .map_or((f64::NEG_INFINITY, f64::INFINITY), |[a, b]| {
            (a.min(b), a.max(b))
        })
}

/// Ground, clearance, structure and wire profiles as CSV:
/// `kind,label,station_m,offset_m,z_m`. Kinds: `ground` (centreline),
/// `ground_side` (label: the offset), `clearance` (ground + the options'
/// clearance), `structure` (its base), `attachment`, `wire`. Within the
/// options' station range (lines cut at its ends).
pub fn profile_csv(d: &RouteDrawing, o: &Options) -> String {
    let (a, b) = range(o);
    let mut out = String::from("kind,label,station_m,offset_m,z_m\n");
    for run in d.ground.iter().flat_map(|r| station_runs(r, a, b)) {
        for p in &run {
            let _ = writeln!(out, "ground,centreline,{:.3},0,{:.3}", p[0], p[1]);
        }
    }
    for (offset, runs) in &d.sides {
        for run in runs.iter().flat_map(|r| station_runs(r, a, b)) {
            for p in &run {
                let _ = writeln!(
                    out,
                    "ground_side,{offset:+.1},{:.3},{offset:.3},{:.3}",
                    p[0], p[1]
                );
            }
        }
    }
    if let Some(c) = o.clearance {
        for run in d.ground.iter().flat_map(|r| station_runs(r, a, b)) {
            for p in &run {
                let _ = writeln!(out, "clearance,+{c},{:.3},0,{:.3}", p[0], p[1] + c);
            }
        }
    }
    for m in d
        .structures
        .iter()
        .filter(|m| m.station >= a && m.station <= b)
    {
        let id = ohl_core::text::csv_field(&m.id);
        let _ = writeln!(
            out,
            "structure,{id},{:.3},{:.3},{:.3}",
            m.station, m.offset, m.base
        );
        for p in &m.attachments {
            let _ = writeln!(out, "attachment,{id},{:.3},{:.3},{:.3}", p[0], p[1], p[2]);
        }
    }
    for w in &d.wires {
        let label = ohl_core::text::csv_field(&w.label);
        for run in station_runs(&w.points, a, b) {
            for p in &run {
                let _ = writeln!(out, "wire,{label},{:.3},{:.3},{:.3}", p[0], p[1], p[2]);
            }
        }
    }
    out
}

/// The profile as OHL profile XML (`<ohlProfile version="1">`; see the
/// user guide, Drawings): ground runs, side ground, clearance line,
/// structures with attachments, wires and markers, within the options'
/// station range.
pub fn profile_xml(d: &RouteDrawing, o: &Options) -> String {
    let (a, b) = range(o);
    let (from, to) = o.stations.map_or((0.0, d.length), |_| (a, b));
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        r#"<ohlProfile version="1" route="{}" name="{}" length="{:.3}" from="{from:.3}" to="{to:.3}" units="m">"#,
        escape(&d.route),
        escape(&d.name),
        d.length,
    );
    let _ = writeln!(
        out,
        r#"  <scales horizontal="{}" vertical="{}"/>"#,
        o.horizontal_scale, o.vertical_scale
    );
    let runs2 = |out: &mut String, runs: &[Vec<[f64; 2]>], dz: f64| {
        for run in runs.iter().flat_map(|r| station_runs(r, a, b)) {
            out.push_str("    <run>");
            for p in &run {
                let _ = write!(out, r#"<p s="{:.3}" z="{:.3}"/>"#, p[0], p[1] + dz);
            }
            out.push_str("</run>\n");
        }
    };
    out.push_str("  <ground offset=\"0\">\n");
    runs2(&mut out, &d.ground, 0.0);
    out.push_str("  </ground>\n");
    for (offset, runs) in &d.sides {
        let _ = writeln!(out, r#"  <ground offset="{offset}">"#);
        runs2(&mut out, runs, 0.0);
        out.push_str("  </ground>\n");
    }
    if let Some(c) = o.clearance {
        let _ = writeln!(out, r#"  <clearance height="{c}">"#);
        runs2(&mut out, &d.ground, c);
        out.push_str("  </clearance>\n");
    }
    for m in d
        .structures
        .iter()
        .filter(|m| m.station >= a && m.station <= b)
    {
        let _ = writeln!(
            out,
            r#"  <structure id="{}" s="{:.3}" o="{:.3}" base="{:.3}" top="{:.3}" x="{:.3}" y="{:.3}">"#,
            escape(&m.id),
            m.station,
            m.offset,
            m.base,
            m.top,
            m.xy[0],
            m.xy[1]
        );
        for p in &m.attachments {
            let _ = writeln!(
                out,
                r#"    <attachment s="{:.3}" o="{:.3}" z="{:.3}"/>"#,
                p[0], p[1], p[2]
            );
        }
        out.push_str("  </structure>\n");
    }
    for w in &d.wires {
        for run in station_runs(&w.points, a, b) {
            let _ = write!(
                out,
                r#"  <wire label="{}" color="{}"{}>"#,
                escape(&w.label),
                escape(&w.color),
                w.dash
                    .as_ref()
                    .map_or(String::new(), |d| format!(r#" dash="{}""#, escape(d)))
            );
            for p in &run {
                let _ = write!(
                    out,
                    r#"<p s="{:.3}" o="{:.3}" z="{:.3}"/>"#,
                    p[0], p[1], p[2]
                );
            }
            out.push_str("</wire>\n");
        }
    }
    for (p, label) in d.markers.iter().filter(|(p, _)| p[0] >= a && p[0] <= b) {
        let _ = writeln!(
            out,
            r#"  <marker label="{}" s="{:.3}" o="{:.3}" z="{:.3}"/>"#,
            escape(label),
            p[0],
            p[1],
            p[2]
        );
    }
    out.push_str("</ohlProfile>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::main_route;

    #[test]
    fn station_ranges_cut_the_profiles() {
        let o = Options {
            clearance: Some(7.0),
            side_offsets: vec![-10.0],
            ..Options::default()
        };
        let d = main_route(&o);
        let ranged = Options {
            stations: Some([200.0, 700.0]),
            ..o.clone()
        };
        let csv = profile_csv(&d, &ranged);
        let stations: Vec<f64> = csv
            .lines()
            .skip(1)
            .map(|l| l.rsplit(',').nth(2).unwrap().parse().unwrap())
            .collect();
        assert!(
            stations
                .iter()
                .all(|s| (200.0 - 1e-6..=700.0 + 1e-6).contains(s))
        );
        // Ground starts and ends exactly at the range.
        let ground: Vec<&str> = csv.lines().filter(|l| l.starts_with("ground,")).collect();
        assert!(ground[0].contains(",200.000,") && ground.last().unwrap().contains(",700.000,"));
        // T2 (0+310) and T3 (0+640) only.
        let structures: Vec<&str> = csv
            .lines()
            .filter(|l| l.starts_with("structure,"))
            .collect();
        assert_eq!(structures.len(), 2, "{structures:?}");
        assert!(csv.contains("clearance,+7,"));
        let all = profile_csv(&d, &o);
        assert!(all.lines().count() > csv.lines().count());

        let xml = profile_xml(&d, &ranged);
        assert!(xml.starts_with("<?xml") && xml.trim_end().ends_with("</ohlProfile>"));
        assert!(xml.contains(r#"from="200.000" to="700.000""#));
        assert_eq!(xml.matches("<structure ").count(), 2);
        assert!(xml.contains(r#"<ground offset="-10">"#));
        assert!(xml.contains(r#"<clearance height="7">"#));
        // Every element opened is closed.
        for tag in ["ground", "structure", "wire", "run", "clearance"] {
            assert_eq!(
                xml.matches(&format!("<{tag}")).count(),
                xml.matches(&format!("</{tag}>")).count(),
                "{tag}"
            );
        }
    }
}
