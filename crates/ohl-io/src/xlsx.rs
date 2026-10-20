// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! XLSX workbooks (Office Open XML SpreadsheetML, ECMA-376): one sheet per
//! [`Table`], bold frozen header row, text as inline strings, numbers in
//! full precision. Written directly (a zip of a few XML parts); linear in
//! the number of cells.

use std::fmt::Write as _;
use std::io::Write as _;

use crate::tables::{Cell, Table};
use crate::xml::escape;
use crate::{ExportModel, Exporter, Options};

const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG_REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

/// The spreadsheet column name of index `k` (0 → `A`, 26 → `AA`).
pub fn column_name(mut k: usize) -> String {
    let mut name = Vec::new();
    loop {
        name.push(b'A' + (k % 26) as u8);
        if k < 26 {
            break;
        }
        k = k / 26 - 1;
    }
    name.reverse();
    String::from_utf8(name).expect("ASCII")
}

/// A valid, unique sheet name (≤ 31 characters, no `[]:*?/\`).
fn sheet_name(name: &str, taken: &[String]) -> String {
    let clean: String = name
        .chars()
        .map(|c| if "[]:*?/\\".contains(c) { '_' } else { c })
        .take(31)
        .collect();
    let clean = if clean.trim().is_empty() {
        "Sheet".to_owned()
    } else {
        clean
    };
    let mut candidate = clean.clone();
    let mut n = 2;
    while taken.iter().any(|t| t.eq_ignore_ascii_case(&candidate)) {
        let suffix = format!(" ({n})");
        let keep: String = clean.chars().take(31 - suffix.len()).collect();
        candidate = format!("{keep}{suffix}");
        n += 1;
    }
    candidate
}

/// Text without the characters XML 1.0 does not allow.
fn xml_text(t: &str) -> String {
    let t: String = t
        .chars()
        .filter(|&c| matches!(c, '\t' | '\n' | '\r') || c >= ' ')
        .collect();
    escape(&t).replace("&#10;", "\n").replace("&#9;", "\t")
}

fn sheet_xml(t: &Table) -> String {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"{MAIN_NS}\">\
         <sheetViews><sheetView workbookViewId=\"0\"><pane ySplit=\"1\" topLeftCell=\"A2\" \
         activePane=\"bottomLeft\" state=\"frozen\"/></sheetView></sheetViews><sheetData>"
    );
    let text_cell = |out: &mut String, r: usize, c: usize, text: &str, style: &str| {
        let _ = write!(
            out,
            "<c r=\"{}{r}\" t=\"inlineStr\"{style}><is><t xml:space=\"preserve\">{}</t></is></c>",
            column_name(c),
            xml_text(text)
        );
    };
    out.push_str("<row r=\"1\">");
    for (c, h) in t.columns.iter().enumerate() {
        text_cell(&mut out, 1, c, h, " s=\"1\"");
    }
    out.push_str("</row>");
    for (k, row) in t.rows.iter().enumerate() {
        let r = k + 2;
        let _ = write!(out, "<row r=\"{r}\">");
        for (c, cell) in row.iter().enumerate() {
            match cell {
                Cell::Text(s) => text_cell(&mut out, r, c, s, ""),
                Cell::Number(x) if x.is_finite() => {
                    let _ = write!(out, "<c r=\"{}{r}\"><v>{x}</v></c>", column_name(c));
                }
                Cell::Number(_) | Cell::Empty => {}
            }
        }
        out.push_str("</row>");
    }
    out.push_str("</sheetData></worksheet>\n");
    out
}

const STYLES: &str = "<fonts count=\"2\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font>\
<font><b/><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts>\
<fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills>\
<borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders>\
<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>\
<cellXfs count=\"2\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>\
<xf numFmtId=\"0\" fontId=\"1\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/></cellXfs>\
<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>";

/// A workbook of the tables (one sheet each), as XLSX bytes.
pub fn workbook(tables: &[Table]) -> Result<Vec<u8>, String> {
    let mut names: Vec<String> = Vec::new();
    for t in tables {
        let n = sheet_name(&t.name, &names);
        names.push(n);
    }
    let n = tables.len();
    let mut types = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
         <Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\
         <Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>",
    );
    for k in 1..=n {
        let _ = write!(
            types,
            "<Override PartName=\"/xl/worksheets/sheet{k}.xml\" \
             ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
        );
    }
    types.push_str("</Types>\n");
    let root_rels = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"{PKG_REL_NS}\">\
         <Relationship Id=\"rId1\" Type=\"{REL_NS}/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>\n"
    );
    let mut book = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <workbook xmlns=\"{MAIN_NS}\" xmlns:r=\"{REL_NS}\"><sheets>"
    );
    let mut rels = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"{PKG_REL_NS}\">"
    );
    for (k, name) in names.iter().enumerate() {
        let id = k + 1;
        let _ = write!(
            book,
            "<sheet name=\"{}\" sheetId=\"{id}\" r:id=\"rId{id}\"/>",
            escape(name)
        );
        let _ = write!(
            rels,
            "<Relationship Id=\"rId{id}\" Type=\"{REL_NS}/worksheet\" Target=\"worksheets/sheet{id}.xml\"/>"
        );
    }
    book.push_str("</sheets></workbook>\n");
    let _ = writeln!(
        rels,
        "<Relationship Id=\"rId{}\" Type=\"{REL_NS}/styles\" Target=\"styles.xml\"/></Relationships>",
        n + 1
    );
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<styleSheet xmlns=\"{MAIN_NS}\">{STYLES}</styleSheet>\n"
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut part = |name: &str, body: &str| -> Result<(), String> {
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        zip.write_all(body.as_bytes()).map_err(|e| e.to_string())
    };
    part("[Content_Types].xml", &types)?;
    part("_rels/.rels", &root_rels)?;
    part("xl/workbook.xml", &book)?;
    part("xl/_rels/workbook.xml.rels", &rels)?;
    part("xl/styles.xml", &styles)?;
    for (k, t) in tables.iter().enumerate() {
        part(&format!("xl/worksheets/sheet{}.xml", k + 1), &sheet_xml(t))?;
    }
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
}

/// The line's tables as an Excel workbook: structures, sections, spans of
/// every weather case, stationing, and the further tables given with the
/// model (structure loads, usage).
pub struct Xlsx;

impl Exporter for Xlsx {
    fn id(&self) -> &str {
        "xlsx"
    }
    fn description(&self) -> &str {
        "Tables (structures, sections, spans per case, stations, loads) as an Excel workbook"
    }
    fn extension(&self) -> &str {
        "xlsx"
    }
    fn export(&mut self, model: &ExportModel, _: &Options) -> Result<Vec<u8>, String> {
        workbook(&crate::tables::all(model))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_and_sheet_names() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(27), "AB");
        assert_eq!(column_name(701), "ZZ");
        assert_eq!(column_name(702), "AAA");
        let taken = vec!["Loads".to_owned()];
        assert_eq!(sheet_name("loads", &taken), "loads (2)");
        assert_eq!(sheet_name("a/b", &[]), "a_b");
        assert_eq!(sheet_name(&"x".repeat(40), &[]).chars().count(), 31);
    }
}
