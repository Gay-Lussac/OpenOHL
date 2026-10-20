// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The structure report: a run of the structure's cases
//! ([`ohl_structure_link::StructureRun`]) as one printable HTML page —
//! no external resources: styles inline, no scripts, no images, no
//! fonts — and the members' results as CSV.
//!
//! The page holds the structure's identity, its level and the extension
//! it was solved on, the sections and materials (and the pole), the load
//! cases, the governing usage per check with its load case, every
//! member's largest usage with the case governing it, the reactions, the
//! pole check, the notes and flags of the run and of each case, the
//! software version and the pre-alpha warning ([`WARNING`]). Member
//! results are printed from their fields as they are (their `governs`
//! and the case notes in words), so what the solver adds to them shows.

use std::fmt::Write as _;

use ohl_structure_link::{CaseResult, StructureRun};
use ohl_structure_loads::{LoadCaseRecipe, LoadTable};
use ohl_structures::StructureType;

/// The warning on every report.
pub const WARNING: &str = "Pre-alpha software: results must be verified by a qualified engineer.";

/// What a report shows besides the run.
pub struct ReportInput<'a> {
    /// The structure.
    pub structure: &'a StructureType,
    /// Its file (as shown; empty: not saved).
    pub file: &'a str,
    /// The section library, in words (where it was read).
    pub sections_from: &'a str,
    /// The load table solved.
    pub table: &'a LoadTable,
    /// The recipes of its cases.
    pub recipes: &'a [LoadCaseRecipe],
    /// The run.
    pub run: &'a StructureRun,
    /// Cases whose line values are incomplete, with why (they are not
    /// checked).
    pub incomplete: &'a [(String, String)],
    /// Software name and version, e.g. `OHL-Structure 0.1.0`.
    pub software: &'a str,
    /// The date of the report (none: not written).
    pub date: Option<&'a str>,
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            c => o.push(c),
        }
    }
    o
}

fn pct(u: Option<f64>) -> String {
    match u {
        Some(u) if u.is_finite() => format!("{:.1} %", u * 100.0),
        Some(_) => "∞".into(),
        None => "—".into(),
    }
}

fn class(u: Option<f64>) -> &'static str {
    match u {
        Some(u) if u > 1.0 => "over",
        Some(u) if u >= 0.9 => "near",
        Some(_) => "ok",
        None => "",
    }
}

fn kn(v: f64) -> String {
    format!("{:.2}", v / 1000.0)
}

const STYLE: &str = "body{font-family:-apple-system,'Segoe UI',Helvetica,Arial,sans-serif;\
font-size:10pt;color:#111;background:#fff;margin:16px auto;max-width:1000px;padding:0 16px}\
h1{font-size:16pt;margin:0 0 4px}h2{font-size:12pt;margin:18px 0 6px;border-bottom:1px solid #999}\
h3{font-size:10.5pt;margin:12px 0 4px}\
table{border-collapse:collapse;margin:4px 0 8px;width:100%}\
th,td{border:1px solid #bbb;padding:2px 6px;text-align:left;vertical-align:top}\
th{background:#eee}td.n{text-align:right;font-variant-numeric:tabular-nums;white-space:nowrap}\
.warn{border:2px solid #b00;background:#fee;color:#600;padding:6px 10px;font-weight:bold;margin:8px 0}\
.over{color:#b00;font-weight:bold}.near{color:#a60}.ok{color:#060}\
.note{color:#555}ul{margin:4px 0 8px 18px;padding:0}\
@media print{body{margin:0;max-width:none}h2{break-after:avoid}tr{break-inside:avoid}}";

/// The report as one HTML page (no external resources).
pub fn report_html(input: &ReportInput) -> String {
    let t = input.structure;
    let run = input.run;
    let mut h = String::new();
    let _ = write!(
        h,
        "<!DOCTYPE html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>Structure report — {}</title><style>{STYLE}</style></head><body>\n",
        esc(&t.id)
    );
    let _ = writeln!(
        h,
        "<h1>Structure report — {} ({})</h1>",
        esc(&t.id),
        esc(&t.name)
    );
    let _ = writeln!(h, "<p class=\"warn\">{}</p>", esc(WARNING));

    // Identity.
    h.push_str("<h2>Structure</h2>\n<table>\n");
    let d = t.detail();
    let extension = match &run.extension {
        Some(e) => format!("{e} (dz {:.3} m)", run.dz),
        None => "none — as written".into(),
    };
    let mut rows: Vec<(&str, String)> = vec![
        ("Id", t.id.clone()),
        ("Name", t.name.clone()),
        ("Kind", format!("{:?}", t.kind).to_lowercase()),
        (
            "File",
            if input.file.is_empty() {
                "not saved".into()
            } else {
                input.file.to_owned()
            },
        ),
        ("Level", format!("{} — {}", d.level(), d.name())),
        ("Extension solved on", extension),
        ("Section library", input.sections_from.to_owned()),
        ("Software", input.software.to_owned()),
    ];
    if let Some(date) = input.date {
        rows.push(("Date", date.to_owned()));
    }
    for (k, v) in rows {
        let _ = writeln!(h, "<tr><th>{k}</th><td>{}</td></tr>", esc(&v));
    }
    h.push_str("</table>\n");

    // Sections and materials.
    if !run.sections.is_empty() {
        h.push_str(
            "<h2>Sections and materials</h2>\n<table><tr><th>Section</th><th>Material</th>\
             <th>Area cm²</th><th>E GPa</th><th>f<sub>y</sub> MPa</th><th>Weight kg/m</th>\
             <th>Members</th><th>Length m</th></tr>\n",
        );
        for s in &run.sections {
            let _ = writeln!(
                h,
                "<tr><td>{}</td><td>{}</td><td class=\"n\">{:.2}</td><td class=\"n\">{:.1}</td>\
                 <td class=\"n\">{}</td><td class=\"n\">{:.2}</td><td class=\"n\">{}</td>\
                 <td class=\"n\">{:.2}</td></tr>",
                esc(&s.id),
                esc(s.material.as_deref().unwrap_or("the structure's own table")),
                s.area * 1e4,
                s.elasticity / 1e9,
                s.yield_strength
                    .map_or("—".into(), |f| format!("{:.0}", f / 1e6)),
                s.weight / 9.80665,
                s.members,
                s.length
            );
        }
        h.push_str("</table>\n");
    }
    match &run.pole {
        Some(Ok(p)) => {
            let _ = writeln!(
                h,
                "<h2>Pole</h2>\n<table><tr><th>Pole</th><th>Material</th><th>Height m</th>\
                 <th>D ground mm</th><th>D top mm</th><th>Drag</th><th>M<sub>c</sub> ground kN·m</th></tr>\n\
                 <tr><td>{}</td><td>{}</td><td class=\"n\">{:.2}</td><td class=\"n\">{:.0}</td>\
                 <td class=\"n\">{:.0}</td><td class=\"n\">{:.2}</td><td class=\"n\">{}</td></tr></table>",
                esc(&p.id),
                esc(&p.material),
                p.height,
                p.ground_diameter * 1e3,
                p.top_diameter * 1e3,
                p.drag,
                kn(p.ground_capacity)
            );
        }
        Some(Err(e)) => {
            let _ = writeln!(
                h,
                "<h2>Pole</h2>\n<p class=\"over\">The pole does not read: {}</p>",
                esc(e)
            );
        }
        None => {}
    }

    // Load cases.
    h.push_str(
        "<h2>Load cases</h2>\n<table><tr><th>Case</th><th>Description</th><th>Strength</th>\
         <th>Self-weight</th><th>Rows</th><th>Without a value</th><th>Σ loads x; y; z kN</th>\
         <th>Status</th></tr>\n",
    );
    for c in &run.cases {
        let recipe = input.recipes.iter().find(|r| r.name == c.name);
        let rows = input.table.case(&c.name).count();
        let missing = input
            .table
            .case(&c.name)
            .filter(|r| r.missing.is_some())
            .count();
        let sum = match &c.fe {
            Some(Ok(r)) => format!(
                "{}; {}; {}",
                kn(r.load_sum[0]),
                kn(r.load_sum[1]),
                kn(r.load_sum[2])
            ),
            _ => "—".into(),
        };
        let mut status = Vec::new();
        if let Some((_, why)) = input.incomplete.iter().find(|(n, _)| *n == c.name) {
            status.push(format!("not checked — {why}"));
        }
        for (what, r) in [
            ("members", c.fe.as_ref().map(|r| r.as_ref().map(|_| ()))),
            ("pole", c.pole.as_ref().map(|r| r.as_ref().map(|_| ()))),
        ] {
            if let Some(Err(e)) = r {
                status.push(format!("{what}: {e}"));
            }
        }
        let _ = writeln!(
            h,
            "<tr><td>{}</td><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td>\
             <td class=\"n\">{rows}</td><td class=\"n\">{missing}</td><td class=\"n\">{sum}</td>\
             <td>{}</td></tr>",
            esc(&c.name),
            esc(recipe.map_or("", |r| r.description.as_str())),
            recipe.map_or(1.0, |r| r.strength),
            recipe.map_or(1.0, |r| r.self_weight),
            esc(&if status.is_empty() {
                "solved".to_owned()
            } else {
                status.join("; ")
            })
        );
    }
    h.push_str("</table>\n");

    // Governing per check.
    h.push_str(
        "<h2>Usage per check</h2>\n<table><tr><th>Check</th><th>Largest usage</th>\
         <th>Governing case</th><th>What governs</th></tr>\n",
    );
    for s in run.checks() {
        let _ = writeln!(
            h,
            "<tr><td>{}</td><td class=\"n {}\">{}</td><td>{}</td><td>{}</td></tr>",
            if s.check == "members" {
                "members (finite elements)"
            } else {
                "pole"
            },
            class(s.usage),
            pct(s.usage),
            esc(&s.case),
            esc(&s.governs)
        );
        for (case, e) in &s.failed {
            let _ = writeln!(
                h,
                "<tr><td></td><td class=\"over\">not solved</td><td>{}</td><td>{}</td></tr>",
                esc(case),
                esc(e)
            );
        }
    }
    h.push_str("</table>\n");
    if !input.incomplete.is_empty() {
        h.push_str(
            "<p class=\"note\">Cases marked <em>not checked</em> lack line values: their usage \
             counts only the loads there are.</p>\n",
        );
    }

    // Members: their largest usage and the case governing.
    let solved: Vec<(&str, &CaseResult)> = run
        .cases
        .iter()
        .filter_map(|c| match &c.fe {
            Some(Ok(r)) => Some((c.name.as_str(), r)),
            _ => None,
        })
        .collect();
    if let Some((_, first)) = solved.first() {
        h.push_str(
            "<h2>Members — largest usage over the cases</h2>\n<table><tr><th>Member</th>\
             <th>Usage</th><th>Governing case</th><th>Axial kN (+ tension)</th>\
             <th>What governs</th></tr>\n",
        );
        let mut rows: Vec<(String, Option<f64>, &str, f64, String)> = first
            .members
            .iter()
            .map(|m| {
                let mut best: Option<(f64, &str, f64, String)> = None;
                for (case, r) in &solved {
                    if let Some(x) = r.members.iter().find(|x| x.id == m.id)
                        && let Some(u) = x.usage
                        && best.as_ref().is_none_or(|b| u > b.0)
                    {
                        best = Some((u, case, x.axial, x.governs.clone()));
                    }
                }
                match best {
                    Some((u, case, axial, governs)) => {
                        (m.id.clone(), Some(u), case, axial, governs)
                    }
                    None => (m.id.clone(), None, "", m.axial, "no capacity".into()),
                }
            })
            .collect();
        rows.sort_by(|a, b| b.1.unwrap_or(-1.0).total_cmp(&a.1.unwrap_or(-1.0)));
        for (id, u, case, axial, governs) in rows {
            let _ = writeln!(
                h,
                "<tr><td>{}</td><td class=\"n {}\">{}</td><td>{}</td><td class=\"n\">{}</td>\
                 <td>{}</td></tr>",
                esc(&id),
                class(u),
                pct(u),
                esc(case),
                kn(axial),
                esc(&governs)
            );
        }
        h.push_str("</table>\n");

        // Reactions.
        h.push_str(
            "<h2>Reactions</h2>\n<table><tr><th>Case</th><th>Joint</th><th>F<sub>x</sub> kN</th>\
             <th>F<sub>y</sub> kN</th><th>F<sub>z</sub> kN</th><th>|M| kN·m</th></tr>\n",
        );
        for (case, r) in &solved {
            for x in &r.reactions {
                let m = x.moment.iter().map(|v| v * v).sum::<f64>().sqrt();
                let _ = writeln!(
                    h,
                    "<tr><td>{}</td><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td>\
                     <td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
                    esc(case),
                    esc(&x.joint),
                    kn(x.force[0]),
                    kn(x.force[1]),
                    kn(x.force[2]),
                    kn(m)
                );
            }
        }
        h.push_str(
            "</table>\n<p class=\"note\">Structure axes: x right of the line, y ahead, z up; \
             the force of the support on the structure.</p>\n",
        );
    }

    // Pole check.
    if run.cases.iter().any(|c| c.pole.is_some()) {
        h.push_str(
            "<h2>Pole check</h2>\n<table><tr><th>Case</th><th>Usage</th><th>At m</th>\
             <th>M kN·m</th><th>M<sub>c</sub> kN·m</th><th>Ground M kN·m</th>\
             <th>Of it wind kN·m</th><th>P-Delta ×</th><th>What governs</th></tr>\n",
        );
        for c in &run.cases {
            match &c.pole {
                Some(Ok(p)) => {
                    let _ = writeln!(
                        h,
                        "<tr><td>{}</td><td class=\"n {}\">{}</td><td class=\"n\">{:.2}</td>\
                         <td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td>\
                         <td class=\"n\">{}</td><td class=\"n\">{:.3}</td><td>{}</td></tr>",
                        esc(&c.name),
                        class(Some(p.usage)),
                        pct(Some(p.usage)),
                        p.height,
                        kn(p.moment),
                        kn(p.capacity),
                        kn(p.ground_moment),
                        kn(p.wind_moment),
                        p.amplification,
                        esc(&p.governs)
                    );
                }
                Some(Err(e)) => {
                    let _ = writeln!(
                        h,
                        "<tr><td>{}</td><td class=\"over\">not checked</td><td colspan=\"7\">{}</td></tr>",
                        esc(&c.name),
                        esc(e)
                    );
                }
                None => {}
            }
        }
        h.push_str("</table>\n");
    }

    // Notes and flags.
    let mut notes: Vec<String> = run.notes.clone();
    for (case, why) in input.incomplete {
        notes.push(format!("{case}: not checked — {why}"));
    }
    for c in &run.cases {
        if let Some(Ok(r)) = &c.fe {
            notes.extend(r.notes.iter().map(|n| format!("{}: {n}", c.name)));
            let scale = r.load_sum.iter().map(|x| x.abs()).fold(1.0, f64::max);
            if r.imbalance() > 1e-6 * scale {
                notes.push(format!(
                    "{}: out of balance {:.3} N (Σ loads + Σ reactions)",
                    c.name,
                    r.imbalance()
                ));
            }
        }
        if let Some(Ok(p)) = &c.pole {
            notes.extend(p.notes.iter().map(|n| format!("{} (pole): {n}", c.name)));
        }
    }
    h.push_str("<h2>Notes and flags</h2>\n");
    if notes.is_empty() {
        h.push_str("<p>None.</p>\n");
    } else {
        h.push_str("<ul>\n");
        for n in notes {
            let _ = writeln!(h, "<li>{}</li>", esc(&n));
        }
        h.push_str("</ul>\n");
    }
    let _ = writeln!(
        h,
        "<p class=\"warn\">{}</p>\n<p class=\"note\">{} — OpenOHL, pre-alpha.</p>\n</body></html>",
        esc(WARNING),
        esc(input.software)
    );
    h
}

/// Every member's result in every case solved, as CSV:
/// `case,member,axial_n,usage,governs,slack` (axial in N, tension
/// positive; usage as a share, empty without a capacity).
pub fn members_csv(run: &StructureRun) -> String {
    let q = ohl_core::text::csv_field;
    let mut out = String::from("case,member,axial_n,usage,governs,slack\n");
    for c in &run.cases {
        let Some(Ok(r)) = &c.fe else {
            continue;
        };
        for m in &r.members {
            let _ = writeln!(
                out,
                "{},{},{:.1},{},{},{}",
                q(&c.name),
                q(&m.id),
                m.axial,
                m.usage.map_or(String::new(), |u| format!("{u:.4}")),
                q(&m.governs),
                m.slack
            );
        }
    }
    out
}

/// Today's date (UTC) as `YYYY-MM-DD`, from the system clock
/// ([`crate::ifc::iso_time`]'s date).
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    crate::ifc::iso_time(secs)[..10].to_owned()
}

/// What [`from_text`] reports on.
pub struct FromText<'a> {
    /// The structure file's text.
    pub text: &'a str,
    /// The file, as shown.
    pub file: &'a str,
    /// Its folder (a model or pole file it names).
    pub dir: Option<&'a std::path::Path>,
    /// The section library.
    pub sections: &'a ohl_sections::SectionLibrary,
    /// Where it was read, in words.
    pub sections_from: &'a str,
    /// The load table.
    pub table: &'a LoadTable,
    /// The recipes of its cases.
    pub recipes: &'a [LoadCaseRecipe],
    /// The extension to stand on (none: as written).
    pub extension: Option<&'a str>,
    /// One case only (none: every case of the table).
    pub case: Option<&'a str>,
    /// Cases whose line values are incomplete, with why.
    pub incomplete: &'a [(String, String)],
    /// Software name and version.
    pub software: &'a str,
    /// The date (none: not written).
    pub date: Option<&'a str>,
}

/// A report: the page, the members' CSV and a summary in lines.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The HTML page.
    pub html: String,
    /// The members' results ([`members_csv`]).
    pub csv: String,
    /// Per check its largest usage and governing case, one line each.
    pub summary: Vec<String>,
    /// The run reported on.
    pub run: ohl_structure_link::StructureRun,
}

/// Solves the cases of a structure file (each under 200 iterations and
/// 30 s, as the application's results panel) and makes its report.
pub fn from_text(input: &FromText) -> Result<Report, String> {
    use ohl_structures::LibraryItem as _;
    let structure = StructureType::parse(std::path::Path::new("structure.ohls"), input.text)
        .map_err(|e| e.message)?;
    let cases = ohl_structure_link::cases_to_run(input.table, input.recipes, input.case);
    if cases.is_empty() {
        return Err(match input.case {
            Some(c) => format!("no case \"{c}\" in the load table"),
            None => "no case in the load table".into(),
        });
    }
    let budget = ohl_core::budget::Budget::new()
        .max_iterations(200)
        .time_limit(std::time::Duration::from_secs(30));
    let run = ohl_structure_link::analyse_structure(
        &ohl_structure_link::RunInput {
            structure: &structure,
            text: input.text,
            dir: input.dir,
            sections: input.sections,
            table: input.table,
            recipes: input.recipes,
            cases: &cases,
            extension: input.extension,
        },
        &budget,
    )?;
    let html = report_html(&ReportInput {
        structure: &structure,
        file: input.file,
        sections_from: input.sections_from,
        table: input.table,
        recipes: input.recipes,
        run: &run,
        incomplete: input.incomplete,
        software: input.software,
        date: input.date,
    });
    let summary = run
        .checks()
        .iter()
        .map(|s| {
            let mut line = format!(
                "{}: {} in case {}{}",
                s.check,
                pct(s.usage),
                if s.case.is_empty() { "—" } else { &s.case },
                if s.governs.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", s.governs)
                }
            );
            if !s.failed.is_empty() {
                let _ = write!(line, "; {} case(s) not solved", s.failed.len());
            }
            line
        })
        .collect();
    Ok(Report {
        csv: members_csv(&run),
        html,
        summary,
        run,
    })
}
