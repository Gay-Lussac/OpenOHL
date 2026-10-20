// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A minimal XLSX writer and reader (Office Open XML SpreadsheetML,
//! ECMA-376): sheets of text and number cells, text as inline strings.
//! Written with a fixed zip time, so the same sheets give the same bytes.

use std::fmt::Write as _;
use std::io::{Read as _, Write as _};

/// A cell.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    /// Text.
    Text(String),
    /// A number.
    Number(f64),
    /// Nothing.
    Empty,
}

impl From<&str> for Cell {
    fn from(t: &str) -> Self {
        Cell::Text(t.to_owned())
    }
}

impl From<String> for Cell {
    fn from(t: String) -> Self {
        Cell::Text(t)
    }
}

impl From<f64> for Cell {
    fn from(x: f64) -> Self {
        Cell::Number(x)
    }
}

impl<T: Into<Cell>> From<Option<T>> for Cell {
    fn from(x: Option<T>) -> Self {
        x.map_or(Cell::Empty, Into::into)
    }
}

impl Cell {
    /// The cell as text (a number in Rust's shortest form).
    pub fn text(&self) -> String {
        match self {
            Cell::Text(t) => t.clone(),
            Cell::Number(x) => format!("{x}"),
            Cell::Empty => String::new(),
        }
    }
}

/// A sheet: a name and rows of cells.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sheet {
    /// Name (at most 31 characters).
    pub name: String,
    /// Rows.
    pub rows: Vec<Vec<Cell>>,
}

impl Sheet {
    /// A sheet with a header row.
    pub fn new(name: &str, header: &[&str]) -> Self {
        Self {
            name: name.to_owned(),
            rows: vec![header.iter().map(|h| Cell::from(*h)).collect()],
        }
    }

    /// The header row's texts.
    pub fn header(&self) -> Vec<String> {
        self.rows
            .first()
            .map(|r| r.iter().map(Cell::text).collect())
            .unwrap_or_default()
    }

    /// The cell in row `row` (0: the header) under header `column`.
    pub fn get(&self, row: usize, column: &str) -> Option<&Cell> {
        let c = self.header().iter().position(|h| h == column)?;
        self.rows.get(row)?.get(c)
    }
}

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

/// Text escaped for XML, without the characters XML 1.0 does not allow.
fn xml_text(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    for c in t.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if c < ' ' => {}
            c => out.push(c),
        }
    }
    out
}

fn unescape(t: &str) -> String {
    t.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn sheet_xml(s: &Sheet) -> String {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"{MAIN_NS}\"><sheetData>"
    );
    for (k, row) in s.rows.iter().enumerate() {
        let r = k + 1;
        let _ = write!(out, "<row r=\"{r}\">");
        for (c, cell) in row.iter().enumerate() {
            let at = format!("{}{r}", column_name(c));
            match cell {
                Cell::Text(t) => {
                    let style = if k == 0 { " s=\"1\"" } else { "" };
                    let _ = write!(
                        out,
                        "<c r=\"{at}\" t=\"inlineStr\"{style}><is><t xml:space=\"preserve\">{}</t></is></c>",
                        xml_text(t)
                    );
                }
                Cell::Number(x) if x.is_finite() => {
                    let _ = write!(out, "<c r=\"{at}\"><v>{x}</v></c>");
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

/// The sheets as XLSX bytes, in order.
pub fn write(sheets: &[Sheet]) -> Result<Vec<u8>, String> {
    let n = sheets.len();
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
    for (k, s) in sheets.iter().enumerate() {
        if s.name.is_empty() || s.name.chars().count() > 31 {
            return Err(format!("sheet name \"{}\": 1 to 31 characters", s.name));
        }
        let id = k + 1;
        let _ = write!(
            book,
            "<sheet name=\"{}\" sheetId=\"{id}\" r:id=\"rId{id}\"/>",
            xml_text(&s.name)
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
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    let mut part = |name: &str, body: &str| -> Result<(), String> {
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        zip.write_all(body.as_bytes()).map_err(|e| e.to_string())
    };
    part("[Content_Types].xml", &types)?;
    part("_rels/.rels", &root_rels)?;
    part("xl/workbook.xml", &book)?;
    part("xl/_rels/workbook.xml.rels", &rels)?;
    part("xl/styles.xml", &styles)?;
    for (k, s) in sheets.iter().enumerate() {
        part(&format!("xl/worksheets/sheet{}.xml", k + 1), &sheet_xml(s))?;
    }
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
}

/// The value of attribute `name` in an XML start tag's text.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let tag = format!(" {tag}");
    let at = tag.find(&format!(" {name}=\""))? + name.len() + 3;
    let end = tag[at..].find('"')?;
    Some(tag[at..at + end].to_owned())
}

/// Column index of a cell reference (`C12` → 2).
fn column_index(r: &str) -> usize {
    r.chars()
        .take_while(char::is_ascii_alphabetic)
        .fold(0, |k, c| k * 26 + (c as usize - 'A' as usize + 1))
        - 1
}

/// Reads a workbook written by [`write()`] (inline strings and numbers; the
/// sheets in workbook order). Not a general XLSX reader.
pub fn read(bytes: &[u8]) -> Result<Vec<Sheet>, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut part = |name: &str| -> Result<String, String> {
        let mut f = zip.by_name(name).map_err(|e| format!("{name}: {e}"))?;
        let mut s = String::new();
        f.read_to_string(&mut s).map_err(|e| e.to_string())?;
        Ok(s)
    };
    let book = part("xl/workbook.xml")?;
    let rels = part("xl/_rels/workbook.xml.rels")?;
    let mut sheets = Vec::new();
    for tag in book.split("<sheet ").skip(1) {
        let name = unescape(&attribute(tag, "name").ok_or("sheet without name")?);
        let id = attribute(tag, "r:id").ok_or("sheet without r:id")?;
        let target = rels
            .split("<Relationship ")
            .skip(1)
            .find(|r| attribute(r, "Id").as_deref() == Some(id.as_str()))
            .and_then(|r| attribute(r, "Target"))
            .ok_or_else(|| format!("sheet {name}: no relationship {id}"))?;
        let xml = part(&format!("xl/{target}"))?;
        let mut rows = Vec::new();
        for row in xml.split("<row ").skip(1) {
            let mut cells: Vec<Cell> = Vec::new();
            for c in row.split("<c ").skip(1) {
                let head = &c[..c.find('>').unwrap_or(c.len())];
                let r = attribute(head, "r").ok_or("cell without r")?;
                let k = column_index(&r);
                let cell = if let Some(t) = c.find("<t xml:space=\"preserve\">") {
                    let from = t + "<t xml:space=\"preserve\">".len();
                    let to = c[from..].find("</t>").ok_or("open text")? + from;
                    Cell::Text(unescape(&c[from..to]))
                } else if let Some(v) = c.find("<v>") {
                    let to = c[v..].find("</v>").ok_or("open value")? + v;
                    Cell::Number(c[v + 3..to].parse().map_err(|e| format!("{r}: {e}"))?)
                } else {
                    Cell::Empty
                };
                while cells.len() < k {
                    cells.push(Cell::Empty);
                }
                cells.push(cell);
            }
            rows.push(cells);
        }
        sheets.push(Sheet { name, rows });
    }
    Ok(sheets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut s = Sheet::new("A & B", &["Name", "Value [kN]"]);
        s.rows.push(vec!["x<y".into(), 1.5.into()]);
        s.rows.push(vec![Cell::Empty, (-2.0).into()]);
        let bytes = write(std::slice::from_ref(&s)).unwrap();
        assert_eq!(bytes, write(std::slice::from_ref(&s)).unwrap());
        let back = read(&bytes).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "A & B");
        assert_eq!(back[0].header(), ["Name", "Value [kN]"]);
        assert_eq!(back[0].get(1, "Name"), Some(&Cell::from("x<y")));
        assert_eq!(back[0].get(2, "Value [kN]"), Some(&Cell::Number(-2.0)));
        assert_eq!(column_name(27), "AB");
        assert_eq!(column_index("AB3"), 27);
    }
}
