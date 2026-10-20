// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A small XML reader (a tree of elements) and writing helpers, for the
//! XML formats (OHL XML, LandXML); [`decode`] gives a file's text in the
//! encoding it declares.
//!
//! It reads well-formed XML 1.0: elements, attributes, text, CDATA, the
//! predefined and numeric character references; comments, processing
//! instructions and a document type declaration (without an internal
//! subset) are skipped. Namespace prefixes are dropped from names
//! ([`Element::name`] is the local name). Linear in the file size.

use std::fmt::Write as _;

/// An element: its local name, attributes and children.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Element {
    /// Local name (namespace prefix dropped).
    pub name: String,
    /// Attributes in document order (local names).
    pub attributes: Vec<(String, String)>,
    /// Child elements.
    pub children: Vec<Element>,
    /// Text content (all text and CDATA directly inside, joined).
    pub text: String,
}

impl Element {
    /// The value of an attribute.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The first child element named `name`.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    /// The child elements named `name`.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// Every element below this one (depth first, this one excluded)
    /// named `name`.
    pub fn descendants<'a>(&'a self, name: &str, out: &mut Vec<&'a Element>) {
        for c in &self.children {
            if c.name == name {
                out.push(c);
            }
            c.descendants(name, out);
        }
    }
}

/// Escapes text for an attribute value or element content.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c => out.push(c),
        }
    }
    out
}

/// Writes ` name="value"` (escaped) to `out`.
pub fn attribute(out: &mut String, name: &str, value: &str) {
    let _ = write!(out, " {name}=\"{}\"", escape(value));
}

/// Decodes character references in `text`.
fn unescape(text: &str, at: usize) -> Result<String, String> {
    if !text.contains('&') {
        return Ok(text.to_owned());
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(k) = rest.find('&') {
        out.push_str(&rest[..k]);
        let end = rest[k..]
            .find(';')
            .ok_or_else(|| format!("XML: unterminated reference near byte {at}"))?;
        let name = &rest[k + 1..k + end];
        let c = match name {
            "lt" => '<',
            "gt" => '>',
            "amp" => '&',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let code = if let Some(hex) = name.strip_prefix("#x") {
                    u32::from_str_radix(hex, 16).ok()
                } else if let Some(dec) = name.strip_prefix('#') {
                    dec.parse().ok()
                } else {
                    None
                };
                code.and_then(char::from_u32)
                    .ok_or_else(|| format!("XML: unknown reference &{name}; near byte {at}"))?
            }
        };
        out.push(c);
        rest = &rest[k + end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, l)| l)
}

/// Windows-1252 bytes 0x80–0x9F (the rest is Latin-1); unassigned bytes
/// read as the replacement character.
const CP1252: [char; 32] = [
    '€', '\u{fffd}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{fffd}', 'Ž',
    '\u{fffd}', '\u{fffd}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{fffd}',
    'ž', 'Ÿ',
];

/// The text of an XML file: UTF-8 (a byte-order mark dropped), or the
/// encoding its declaration names when it is Windows-1252 or ISO-8859-1
/// / -15 (as programs on Windows write them).
pub fn decode(data: &[u8]) -> Result<std::borrow::Cow<'_, str>, String> {
    let data = data.strip_prefix(b"\xef\xbb\xbf").unwrap_or(data);
    if let Ok(text) = std::str::from_utf8(data) {
        return Ok(std::borrow::Cow::Borrowed(text));
    }
    // The declaration is ASCII: `<?xml … encoding='NAME' … ?>`.
    let head: String = data
        .iter()
        .take(200)
        .take_while(|b| **b != b'>')
        .map(|&b| char::from(b))
        .collect();
    let encoding = head
        .find("encoding")
        .and_then(|at| {
            let rest = &head[at + 8..];
            let q = rest.find(['"', '\''])?;
            let quote = rest[q..].chars().next()?;
            let value = &rest[q + 1..];
            value
                .find(quote)
                .map(|end| value[..end].to_ascii_lowercase())
        })
        .unwrap_or_default();
    match encoding.as_str() {
        "windows-1252" | "cp1252" => Ok(std::borrow::Cow::Owned(
            data.iter()
                .map(|&b| match b {
                    0x80..=0x9f => CP1252[usize::from(b - 0x80)],
                    _ => char::from(b),
                })
                .collect(),
        )),
        "iso-8859-1" | "latin1" | "latin-1" | "iso-8859-15" => Ok(std::borrow::Cow::Owned(
            data.iter().map(|&b| char::from(b)).collect(),
        )),
        "" => Err("not UTF-8 text, and no encoding declared".into()),
        other => Err(format!(
            "text in encoding {other}: not read (UTF-8, Windows-1252 and ISO-8859-1 are)"
        )),
    }
}

/// Parses a document into its root element.
pub fn parse(text: &str) -> Result<Element, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let bytes = text.as_bytes();
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let mut i = 0;
    let err = |m: &str, at: usize| Err(format!("XML: {m} near byte {at}"));
    while i < bytes.len() {
        if bytes[i] != b'<' {
            let end = text[i..].find('<').map_or(bytes.len(), |k| i + k);
            let chunk = &text[i..end];
            match stack.last_mut() {
                Some(e) => e.text.push_str(&unescape(chunk, i)?),
                None if chunk.trim().is_empty() => {}
                None => return err("text outside the root element", i),
            }
            i = end;
            continue;
        }
        let rest = &text[i..];
        if rest.starts_with("<?") {
            i += rest.find("?>").map_or(bytes.len(), |k| k + 2);
        } else if rest.starts_with("<!--") {
            i += rest.find("-->").map_or(bytes.len(), |k| k + 3);
        } else if let Some(cdata) = rest.strip_prefix("<![CDATA[") {
            let end = cdata
                .find("]]>")
                .ok_or_else(|| format!("XML: unterminated CDATA near byte {i}"))?;
            if let Some(e) = stack.last_mut() {
                e.text.push_str(&cdata[..end]);
            }
            i += 9 + end + 3;
        } else if rest.starts_with("<!") {
            if rest[..rest.find('>').unwrap_or(rest.len())].contains('[') {
                return err("document type declarations with an internal subset", i);
            }
            i += rest.find('>').map_or(bytes.len(), |k| k + 1);
        } else if let Some(close) = rest.strip_prefix("</") {
            let end = close
                .find('>')
                .ok_or_else(|| format!("XML: unterminated end tag near byte {i}"))?;
            let name = local(close[..end].trim());
            let e = stack
                .pop()
                .ok_or_else(|| format!("XML: </{name}> without a start tag near byte {i}"))?;
            if e.name != name {
                return err(&format!("<{}> closed by </{name}>", e.name), i);
            }
            match stack.last_mut() {
                Some(parent) => parent.children.push(e),
                None if root.is_none() => root = Some(e),
                None => return err("a second root element", i),
            }
            i += 2 + end + 1;
        } else {
            // Start tag: name, attributes, maybe self-closing.
            let mut k = i + 1;
            let name_end = text[k..]
                .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
                .map_or(bytes.len(), |n| k + n);
            let name = local(&text[k..name_end]).to_owned();
            if name.is_empty() {
                return err("empty element name", i);
            }
            k = name_end;
            let mut e = Element {
                name,
                ..Default::default()
            };
            let closed;
            loop {
                while k < bytes.len() && (bytes[k] as char).is_whitespace() {
                    k += 1;
                }
                if k >= bytes.len() {
                    return err("unterminated start tag", i);
                }
                if bytes[k] == b'>' {
                    closed = false;
                    k += 1;
                    break;
                }
                if text[k..].starts_with("/>") {
                    closed = true;
                    k += 2;
                    break;
                }
                let eq = text[k..]
                    .find('=')
                    .map(|n| k + n)
                    .ok_or_else(|| format!("XML: attribute without a value near byte {k}"))?;
                let key = local(text[k..eq].trim()).to_owned();
                let mut v = eq + 1;
                while v < bytes.len() && (bytes[v] as char).is_whitespace() {
                    v += 1;
                }
                let quote = *bytes.get(v).ok_or("XML: unterminated attribute")?;
                if quote != b'"' && quote != b'\'' {
                    return err("unquoted attribute value", v);
                }
                let close = text[v + 1..]
                    .find(quote as char)
                    .map(|n| v + 1 + n)
                    .ok_or_else(|| format!("XML: unterminated attribute value near byte {v}"))?;
                let value = unescape(&text[v + 1..close], v)?;
                // Namespace declarations are not attributes of the model.
                if key != "xmlns" && !text[k..eq].trim().starts_with("xmlns:") {
                    e.attributes.push((key, value));
                }
                k = close + 1;
            }
            i = k;
            if closed {
                match stack.last_mut() {
                    Some(parent) => parent.children.push(e),
                    None if root.is_none() => root = Some(e),
                    None => return err("a second root element", i),
                }
            } else {
                stack.push(e);
            }
        }
    }
    if let Some(e) = stack.last() {
        return Err(format!("XML: <{}> not closed", e.name));
    }
    root.ok_or_else(|| "XML: no root element".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_elements_attributes_and_text() {
        let doc = "<?xml version=\"1.0\"?>\n<!-- c --><a:root xmlns:a=\"u\" x='1 &amp; 2'>\
                   <b>t&lt;&#65;&#x42;<![CDATA[<raw>]]></b><c/></a:root>";
        let r = parse(doc).unwrap();
        assert_eq!(r.name, "root");
        assert_eq!(r.attr("x"), Some("1 & 2"));
        assert_eq!(r.attributes.len(), 1);
        assert_eq!(r.child("b").unwrap().text, "t<AB<raw>");
        assert_eq!(r.children.len(), 2);
        assert!(parse("<a><b></a>").is_err());
        assert!(parse("<a></a><b/>").is_err());
        assert!(parse("<a>").is_err());
        let mut s = String::new();
        attribute(&mut s, "n", "a\"<&>\n");
        assert_eq!(
            parse(&format!("<e{s}/>")).unwrap().attr("n"),
            Some("a\"<&>\n")
        );
    }

    #[test]
    fn files_are_read_in_their_declared_encoding() {
        let utf8 = "<?xml version='1.0'?><a n='Zürich'/>";
        assert_eq!(decode(utf8.as_bytes()).unwrap(), utf8);
        let bom = [b"\xef\xbb\xbf".as_slice(), utf8.as_bytes()].concat();
        assert_eq!(decode(&bom).unwrap(), utf8);
        // Windows-1252: ü is 0xfc, € is 0x80, ' is 0x92.
        let cp = b"<?xml version='1.0' encoding='windows-1252'?><a n='Z\xfcrich \x80 l\x92eau'/>";
        let text = decode(cp).unwrap();
        assert!(text.ends_with("<a n='Zürich € l’eau'/>"), "{text}");
        assert_eq!(parse(&text).unwrap().attr("n"), Some("Zürich € l’eau"));
        let latin = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a n='Z\xfcrich'/>";
        assert!(decode(latin).unwrap().contains("Zürich"));
        assert!(decode(b"<a n='Z\xfcrich'/>").is_err(), "no declaration");
        let other = b"<?xml version='1.0' encoding='shift_jis'?><a n='\x82\xa0'/>";
        assert!(decode(other).unwrap_err().contains("shift_jis"));
    }
}
