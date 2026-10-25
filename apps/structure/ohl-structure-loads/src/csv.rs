// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! CSV of the load table and of the recipes (XLSX later). Forces in N,
//! pressures in Pa, plain numbers; lists separated by `;`. The table's
//! `source` column names the part a row comes from (`recipe`, `line`,
//! `wind`, `ice`, `point`, `typed`); the recipes' CSV holds the older
//! form only (a case with line forces or loads on the structure itself is
//! written as a `.ohlslc`).

use std::fmt::Write as _;

use ohl_core::text::csv_field;

use crate::recipe::{
    Direction, LineQuantity, LoadCaseRecipe, LoadRecipe, Pick, Side, StructureLoadCases,
    check_cases, parse_share,
};
use crate::table::{At, LoadRow, LoadTable, Source};

/// Columns of the load table.
const TABLE_HEADER: &str = "case,load,at,label,transverse,longitudinal,vertical,source,\
evaluated_transverse,evaluated_longitudinal,evaluated_vertical,missing,note";

/// Columns of the load table a file may leave out (older files).
const TABLE_OPTIONAL: &str = "area";

/// Columns of the recipes.
const RECIPE_HEADER: &str = "case,description,line_case,strength,self_weight,load,what,value,\
pressure,load_line_case,factor,one_sided,side,direction,points,sets,phases,members,note";

/// The records of a CSV text (fields may be quoted, also over lines).
fn records(text: &str) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let mut record = vec![String::new()];
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let field = record.last_mut().expect("one field");
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            ('"', _) => quoted = !quoted,
            (',', false) => record.push(String::new()),
            ('\r', false) => {}
            ('\n', false) => {
                out.push(std::mem::replace(&mut record, vec![String::new()]));
            }
            (c, _) => field.push(c),
        }
    }
    out.push(record);
    out.retain(|r| !(r.len() == 1 && r[0].trim().is_empty()));
    out
}

/// A record's fields by column name.
struct Columns(Vec<String>);

impl Columns {
    fn new(header: &[String], expected: &str) -> Result<Self, String> {
        Self::with_optional(header, expected, "")
    }

    /// The columns `expected` must be there, those of `optional` may.
    fn with_optional(header: &[String], expected: &str, optional: &str) -> Result<Self, String> {
        let names: Vec<String> = header.iter().map(|h| h.trim().to_owned()).collect();
        for want in expected.split(',') {
            if !names.iter().any(|n| n == want) {
                return Err(format!("no column {want}"));
            }
        }
        let known = |n: &str| {
            expected
                .split(',')
                .chain(optional.split(',').filter(|o| !o.is_empty()))
                .any(|w| w == n)
        };
        if let Some(extra) = names.iter().find(|n| !known(n)) {
            return Err(format!("unknown column \"{extra}\""));
        }
        Ok(Self(names))
    }

    fn get<'a>(&self, record: &'a [String], name: &str) -> &'a str {
        self.0
            .iter()
            .position(|n| n == name)
            .and_then(|i| record.get(i))
            .map_or("", |s| s.trim())
    }
}

fn number(text: &str, what: &str) -> Result<f64, String> {
    text.parse()
        .map_err(|_| format!("{what}: \"{text}\" is not a number"))
}

fn opt_number(text: &str, what: &str) -> Result<Option<f64>, String> {
    if text.is_empty() {
        Ok(None)
    } else {
        number(text, what).map(Some)
    }
}

fn list(text: &str) -> Vec<String> {
    text.split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn pick(text: &str) -> Option<Pick> {
    match text {
        "" => None,
        "all" => Some(Pick::All),
        t => Some(Pick::These(list(t))),
    }
}

fn pick_text(p: &Option<Pick>) -> String {
    match p {
        None => String::new(),
        Some(Pick::All) => "all".into(),
        Some(Pick::These(v)) => v.join(";"),
    }
}

fn opt<T: ToString>(v: Option<T>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}

impl LoadTable {
    /// The table as CSV: one row per case, load and place; forces in N.
    pub fn to_csv(&self) -> String {
        let mut out = format!("{TABLE_HEADER},{TABLE_OPTIONAL}\n");
        for r in &self.rows {
            let e = r.evaluated.map(|e| e.map(|x| x.to_string()));
            let _ = writeln!(
                out,
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                csv_field(&r.case),
                opt(r.load),
                r.at.as_str(),
                csv_field(&r.label),
                r.force[0],
                r.force[1],
                r.force[2],
                r.source.as_str(),
                e.as_ref().map_or("", |e| &e[0]),
                e.as_ref().map_or("", |e| &e[1]),
                e.as_ref().map_or("", |e| &e[2]),
                csv_field(r.missing.as_deref().unwrap_or("")),
                csv_field(&r.note),
                opt(r.area),
            );
        }
        out
    }

    /// Reads a table written by [`LoadTable::to_csv`] (or typed in a
    /// spreadsheet with its columns).
    pub fn from_csv(text: &str) -> Result<Self, String> {
        let mut recs = records(text).into_iter();
        let header = recs.next().ok_or("an empty table")?;
        let cols = Columns::with_optional(&header, TABLE_HEADER, TABLE_OPTIONAL)?;
        let mut rows = Vec::new();
        for (k, rec) in recs.enumerate() {
            let at_line = |e: String| format!("row {}: {e}", k + 2);
            let g = |name: &str| cols.get(&rec, name);
            let force = |prefix: &str| -> Result<Option<[f64; 3]>, String> {
                let v = ["transverse", "longitudinal", "vertical"]
                    .map(|c| opt_number(g(&format!("{prefix}{c}")), c));
                match v {
                    [Ok(None), Ok(None), Ok(None)] => Ok(None),
                    [a, b, c] => Ok(Some([
                        a?.unwrap_or_default(),
                        b?.unwrap_or_default(),
                        c?.unwrap_or_default(),
                    ])),
                }
            };
            let row = (|| -> Result<LoadRow, String> {
                Ok(LoadRow {
                    case: g("case").to_owned(),
                    load: opt_number(g("load"), "load")?.map(|x| x as usize),
                    at: At::parse(g("at"))
                        .ok_or_else(|| format!("at \"{}\": point, member or joint", g("at")))?,
                    label: g("label").to_owned(),
                    force: force("")?.unwrap_or_default(),
                    source: match g("source") {
                        "" => Source::Typed,
                        s => Source::parse(s).ok_or_else(|| {
                            format!(
                                "source \"{s}\": {}",
                                Source::ALL.map(Source::as_str).join(", ")
                            )
                        })?,
                    },
                    evaluated: force("evaluated_")?,
                    missing: Some(g("missing").to_owned()).filter(|m| !m.is_empty()),
                    note: g("note").to_owned(),
                    area: opt_number(g("area"), "area")?,
                })
            })()
            .map_err(at_line)?;
            if row.case.is_empty() {
                return Err(format!("row {}: no case", k + 2));
            }
            rows.push(row);
        }
        Ok(Self { rows })
    }
}

impl StructureLoadCases {
    /// The recipes as CSV: one row per load (a case without loads: one row
    /// with the load columns empty); values in N, pressures in Pa. The
    /// older form only: Err (why) when a case has line forces or loads on
    /// the structure itself (those are written as a `.ohlslc`).
    pub fn cases_to_csv(cases: &[LoadCaseRecipe]) -> Result<String, String> {
        if let Some(c) = cases.iter().find(|c| c.has_parts()) {
            return Err(format!(
                "load case \"{}\" has line forces or loads on the structure itself: the CSV of \
                 the recipes holds the older form only — export as a structure load-case file \
                 (.ohlslc)",
                c.name
            ));
        }
        let mut out = format!("{RECIPE_HEADER}\n");
        for c in cases {
            let head = format!(
                "{},{},{},{},{}",
                csv_field(&c.name),
                csv_field(&c.description),
                csv_field(c.line_case.as_deref().unwrap_or("")),
                c.strength,
                c.self_weight,
            );
            if c.loads.is_empty() {
                let _ = writeln!(out, "{head},,,,,,,,,,,,,,");
            }
            for (k, l) in c.loads.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "{head},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                    k + 1,
                    l.what.map_or("", LineQuantity::as_str),
                    opt(l.value),
                    opt(l.pressure),
                    csv_field(l.line_case.as_deref().unwrap_or("")),
                    l.factor,
                    opt(l.one_sided),
                    l.side.map_or("", Side::as_str),
                    l.direction.map_or("", Direction::as_str),
                    csv_field(&pick_text(&l.points)),
                    l.sets
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(";"),
                    csv_field(&l.phases.join(";")),
                    csv_field(&pick_text(&l.members)),
                    csv_field(&l.note),
                );
            }
        }
        Ok(out)
    }

    /// Reads recipes written by [`StructureLoadCases::cases_to_csv`]: rows
    /// of one case follow each other; the case's columns are taken from its
    /// first row. Checked as a file is.
    pub fn cases_from_csv(text: &str) -> Result<Vec<LoadCaseRecipe>, String> {
        let mut recs = records(text).into_iter();
        let header = recs.next().ok_or("an empty table")?;
        let cols = Columns::new(&header, RECIPE_HEADER)?;
        let mut cases: Vec<LoadCaseRecipe> = Vec::new();
        for (k, rec) in recs.enumerate() {
            let g = |name: &str| cols.get(&rec, name);
            let mut row = || -> Result<(), String> {
                let name = g("case");
                if name.is_empty() {
                    return Err("no case".into());
                }
                if cases.last().is_none_or(|c| c.name != name) {
                    if cases.iter().any(|c| c.name == name) {
                        return Err(format!("the rows of case \"{name}\" are not together"));
                    }
                    let mut c = LoadCaseRecipe::new(name);
                    c.description = g("description").to_owned();
                    c.line_case = Some(g("line_case").to_owned()).filter(|s| !s.is_empty());
                    c.strength = opt_number(g("strength"), "strength")?.unwrap_or(1.0);
                    c.self_weight = opt_number(g("self_weight"), "self_weight")?.unwrap_or(1.0);
                    cases.push(c);
                }
                if g("load").is_empty() {
                    return Ok(());
                }
                let load = LoadRecipe {
                    what: match g("what") {
                        "" => None,
                        w => Some(
                            LineQuantity::parse(w)
                                .ok_or_else(|| format!("what \"{w}\" unknown"))?,
                        ),
                    },
                    value: opt_number(g("value"), "value")?,
                    pressure: opt_number(g("pressure"), "pressure")?,
                    line_case: Some(g("load_line_case").to_owned()).filter(|s| !s.is_empty()),
                    factor: opt_number(g("factor"), "factor")?.unwrap_or(1.0),
                    one_sided: match g("one_sided") {
                        "" => None,
                        s => Some(parse_share(s)?),
                    },
                    side: match g("side") {
                        "" => None,
                        "ahead" => Some(Side::Ahead),
                        "back" => Some(Side::Back),
                        s => return Err(format!("side \"{s}\": ahead or back")),
                    },
                    direction: match g("direction") {
                        "" => None,
                        d => Some(Direction::try_from(d.to_owned())?),
                    },
                    points: pick(g("points")),
                    sets: list(g("sets"))
                        .iter()
                        .map(|s| s.parse().map_err(|_| format!("set \"{s}\"")))
                        .collect::<Result<_, _>>()?,
                    phases: list(g("phases")),
                    members: pick(g("members")),
                    note: g("note").to_owned(),
                };
                cases.last_mut().expect("a case").loads.push(load);
                Ok(())
            };
            row().map_err(|e| format!("row {}: {e}", k + 2))?;
        }
        check_cases(&cases)?;
        Ok(cases)
    }
}
