// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Country libraries (ADR-0013, phase 9): what the session knows of a
//! library file's **entries** and of the entries an object **uses** — no
//! store, no network. The country server keeps each library kind once for
//! the whole network (`ohl_store_pg::library`); a line keeps the files of
//! the release it is pinned to as its own library files, so the engine
//! reads them as it reads a project's today.
//!
//! **An entry** of a library file (TOML):
//!
//! - a file with a top-level text `id` is one entry (a structure type, an
//!   insulator assembly): that id;
//! - else every table of a list of tables whose tables all carry a text
//!   `id`, `name` or `label`, unique in the list (`[[cable]]`, `[[case]]`,
//!   `[[type]]`), is an entry: that text;
//! - else the file is one entry named by its file name without its
//!   extension.
//!
//! Its **version** is the SHA-256 of the entry's values written in one
//! canonical way (keys sorted, numbers as read): a comment or the order of
//! keys does not make a new version, a changed value does.
//!
//! **The entries an object uses** ([`uses_of`]): a section's and a
//! jumper's wire (`cables`), a structure's type (`structures`), a string's
//! and a spacer's insulator assembly (`insulators`), a span point's body
//! and a spread object's type (`objects`). The other kinds (weather and
//! load cases, rules, costs, …) are picked by the line as a whole: a line
//! holding such a file is counted as using every entry of it (the safe
//! side).
//!
//! **A pending draft** (a request for an addition, used in its requester's
//! own branch) is a library file of its own, named [`draft_name`]
//! (`request-12.ohlw`), holding the entry requested: the engine reads it as
//! any file of its kind.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use sha2::{Digest, Sha256};
use toml_edit::{DocumentMut, Item};

use crate::Kind;

/// The library kinds whose entries an object names by id: an entry's id
/// is unique across the kind's files (the engine reads them as one
/// library).
pub const REFERENCED_KINDS: [&str; 4] = ["cables", "structures", "insulators", "objects"];

/// An entry of a library file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its id (as objects name it).
    pub id: String,
    /// Its version: SHA-256 (hex) of its values, written canonically.
    pub version: String,
    /// The list of tables it is in (`cable`); `None`: the file is the
    /// entry.
    pub list: Option<String>,
}

/// The entries of library file `file` (its name, with its extension)
/// holding `text`, in their order (see the module).
pub fn entries(file: &str, text: &str) -> Result<Vec<Entry>, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| format!("{file}: not TOML: {e}"))?;
    if let Some(toml::Value::String(id)) = table.get("id") {
        return Ok(vec![Entry {
            id: id.clone(),
            version: version_of(&toml::Value::Table(table.clone())),
            list: None,
        }]);
    }
    let mut out = Vec::new();
    for (key, v) in &table {
        let Some(list) = named_list(v) else {
            continue;
        };
        for (id, t) in list {
            out.push(Entry {
                id,
                version: version_of(t),
                list: Some(key.clone()),
            });
        }
    }
    if out.is_empty() {
        out.push(Entry {
            id: stem(file).to_owned(),
            version: version_of(&toml::Value::Table(table)),
            list: None,
        });
    }
    Ok(out)
}

/// A file name without its extension.
pub fn stem(file: &str) -> &str {
    file.rsplit_once('.').map_or(file, |(s, _)| s)
}

/// The tables of a list whose tables all carry a text `id`, `name` or
/// `label`, unique in it: (that text, the table).
fn named_list(v: &toml::Value) -> Option<Vec<(String, &toml::Value)>> {
    let toml::Value::Array(items) = v else {
        return None;
    };
    if items.is_empty() {
        return None;
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for it in items {
        let t = it.as_table()?;
        let id = ["id", "name", "label"]
            .iter()
            .find_map(|k| t.get(*k).and_then(toml::Value::as_str))?;
        if !seen.insert(id.to_owned()) {
            return None;
        }
        out.push((id.to_owned(), it));
    }
    Some(out)
}

/// An entry's version: SHA-256 of its values written canonically.
pub fn version_of(v: &toml::Value) -> String {
    let mut s = String::new();
    canonical(v, &mut s);
    Sha256::digest(s.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The text of a file's hash (SHA-256, hex): what a release keeps a file
/// by.
pub fn text_hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A text as the version store keeps a blob (compressed as the session's
/// revisions keep theirs, under [`text_hash`]): a file of a release shares
/// the blob of the same text a revision holds.
pub fn blob_data(text: &str) -> Result<Vec<u8>, String> {
    zstd::encode_all(text.as_bytes(), 3).map_err(|e| e.to_string())
}

/// The text of a blob ([`blob_data`]).
pub fn blob_text(data: &[u8]) -> Result<String, String> {
    let raw = zstd::decode_all(data).map_err(|e| e.to_string())?;
    String::from_utf8(raw).map_err(|e| e.to_string())
}

fn canonical(v: &toml::Value, out: &mut String) {
    match v {
        toml::Value::String(s) => {
            out.push_str(&serde_json::to_string(s).expect("serialises"));
        }
        toml::Value::Integer(i) => out.push_str(&format!("i{i}")),
        toml::Value::Float(f) => out.push_str(&format!("f{f:?}")),
        toml::Value::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        toml::Value::Datetime(d) => out.push_str(&format!("d{d}")),
        toml::Value::Array(a) => {
            out.push('[');
            for (k, x) in a.iter().enumerate() {
                if k > 0 {
                    out.push(',');
                }
                canonical(x, out);
            }
            out.push(']');
        }
        toml::Value::Table(t) => {
            let sorted: BTreeMap<&String, &toml::Value> = t.iter().collect();
            out.push('{');
            for (k, (key, x)) in sorted.into_iter().enumerate() {
                if k > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).expect("serialises"));
                out.push(':');
                canonical(x, out);
            }
            out.push('}');
        }
    }
}

/// The library entries object `kind` with JSON `v` uses: (library kind,
/// entry id), each once, in order.
pub fn uses_of(kind: Kind, v: &Value) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = Vec::new();
    let mut add = |k: &'static str, id: Option<&str>| {
        if let Some(id) = id.filter(|s| !s.is_empty())
            && !out.iter().any(|(a, b)| *a == k && b == id)
        {
            out.push((k, id.to_owned()));
        }
    };
    let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(str::to_owned);
    match kind {
        Kind::Section => {
            add("cables", text(v, "cable").as_deref());
            for p in v
                .get("span_points")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(o) = p.get("object") {
                    add("objects", text(o, "type").as_deref());
                }
            }
            for s in v
                .get("spreads")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                add("objects", text(s, "type").as_deref());
            }
        }
        Kind::Jumper => add("cables", text(v, "cable").as_deref()),
        Kind::Structure => {
            add("structures", text(v, "type").as_deref());
            for s in v
                .get("strings")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                add("insulators", text(s, "insulator").as_deref());
            }
        }
        Kind::Spacer => add("insulators", text(v, "insulator").as_deref()),
        _ => {}
    }
    out
}

/// The file name of request `id`'s pending draft of a file named `file`
/// (its extension kept): `request-12.ohlw`.
pub fn draft_name(id: i64, file: &str) -> String {
    match file.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => format!("request-{id}.{ext}"),
        _ => format!("request-{id}"),
    }
}

/// The request a draft file is named for (`request-12.ohlw` → 12).
pub fn draft_of(file: &str) -> Option<i64> {
    let rest = file.strip_prefix("request-")?;
    let digits = rest.split('.').next()?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// `head` (a library file's text; `None`: no such file yet) with the
/// entry `entry` of `draft` (a file holding it) laid in: a file that is
/// one entry is the draft itself; an entry of a list is appended to the
/// same list (or replaces the entry of the same id, a change). Comments
/// and layout of `head` stay.
pub fn with_entry(head: Option<&str>, draft: &str, entry: &str) -> Result<String, String> {
    let found = entries("draft", draft)?;
    let e = found
        .iter()
        .find(|e| e.id == entry)
        .ok_or_else(|| format!("the draft holds no entry {entry}"))?;
    let Some(list) = &e.list else {
        return Ok(draft.to_owned());
    };
    let Some(head) = head else {
        return Ok(draft.to_owned());
    };
    let from: DocumentMut = draft.parse().map_err(|e| format!("draft: {e}"))?;
    let mut out: DocumentMut = head.parse().map_err(|e| format!("library file: {e}"))?;
    let mut table = from
        .get(list)
        .and_then(Item::as_array_of_tables)
        .and_then(|a| {
            a.iter().find(|t| {
                ["id", "name", "label"]
                    .iter()
                    .find_map(|k| t.get(k).and_then(Item::as_str))
                    == Some(entry)
            })
        })
        .ok_or_else(|| format!("the draft holds no entry {entry}"))?
        .clone();
    // The entry and its own tables written after everything of the file
    // (a table keeps its place of the text it came from).
    let mut next = last_position(out.as_table()) + 1;
    renumber(&mut table, &mut next);
    let target = out
        .entry(list)
        .or_insert_with(|| Item::ArrayOfTables(toml_edit::ArrayOfTables::new()));
    let arr = target
        .as_array_of_tables_mut()
        .ok_or_else(|| format!("library file: {list} is not a list of tables"))?;
    // A change: the entry of the same id goes, the new one comes last.
    arr.retain(|t| {
        ["id", "name", "label"]
            .iter()
            .find_map(|k| t.get(k).and_then(Item::as_str))
            != Some(entry)
    });
    arr.push(table);
    Ok(out.to_string())
}

/// `text` (a library file) with only its entry `entry` (a file that is
/// one entry: the file itself): the other entries of its list and the
/// other lists of entries left out, the rest (its header) kept — a draft
/// holding that entry alone.
pub fn only_entry(file: &str, text: &str, entry: &str) -> Result<String, String> {
    let found = entries(file, text)?;
    let e = found
        .iter()
        .find(|e| e.id == entry)
        .ok_or_else(|| format!("{file} holds no entry {entry}"))?;
    if e.list.is_none() {
        return Ok(text.to_owned());
    }
    let lists: BTreeSet<String> = found.iter().filter_map(|e| e.list.clone()).collect();
    let mut doc: DocumentMut = text.parse().map_err(|e| format!("{file}: {e}"))?;
    for list in lists {
        let keep = e.list.as_deref() == Some(list.as_str());
        if !keep {
            doc.remove(&list);
            continue;
        }
        if let Some(arr) = doc.get_mut(&list).and_then(Item::as_array_of_tables_mut) {
            arr.retain(|t| {
                ["id", "name", "label"]
                    .iter()
                    .find_map(|k| t.get(k).and_then(Item::as_str))
                    == Some(entry)
            });
        }
    }
    Ok(doc.to_string())
}

/// The last place of a table of `t` (and its tables) in its text.
fn last_position(t: &toml_edit::Table) -> isize {
    let mut last = t.position().unwrap_or(0);
    for (_, item) in t.iter() {
        match item {
            Item::Table(sub) => last = last.max(last_position(sub)),
            Item::ArrayOfTables(a) => {
                for sub in a.iter() {
                    last = last.max(last_position(sub));
                }
            }
            _ => {}
        }
    }
    last
}

/// Places `t` and its tables at `next`, `next + 1`, … in order.
fn renumber(t: &mut toml_edit::Table, next: &mut isize) {
    t.set_position(Some(*next));
    *next += 1;
    let mut subs: Vec<(isize, String)> = t
        .iter()
        .filter_map(|(k, item)| match item {
            Item::Table(sub) => Some((sub.position().unwrap_or(0), k.to_owned())),
            Item::ArrayOfTables(a) => a
                .iter()
                .next()
                .map(|sub| (sub.position().unwrap_or(0), k.to_owned())),
            _ => None,
        })
        .collect();
    subs.sort();
    for (_, k) in subs {
        match t.get_mut(&k) {
            Some(Item::Table(sub)) => renumber(sub, next),
            Some(Item::ArrayOfTables(a)) => {
                for sub in a.iter_mut() {
                    renumber(sub, next);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CABLES: &str = r#"
format = "ohl-wire"
version = 1

# a comment
[[cable]]
id = "a"
diameter = "10 mm"
[cable.electrical]
r = 1.5

[[cable]]
id = "b"
diameter = "12 mm"
"#;

    #[test]
    fn entries_of_a_list_a_single_item_and_a_bare_file() {
        let e = entries("x.ohlw", CABLES).unwrap();
        assert_eq!(
            e.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(e[0].list.as_deref(), Some("cable"));
        let one = entries(
            "t.ohls",
            "id = \"T1\"\nname = \"T\"\n[[point]]\nid = \"p\"\n",
        )
        .unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].id, "T1");
        assert_eq!(one[0].list, None);
        let bare = entries("costs.ohlcost", "[unit]\nsteel = 1.0\n").unwrap();
        assert_eq!(bare[0].id, "costs");
        assert!(entries("x", "not = = toml").is_err());
    }

    #[test]
    fn a_version_is_the_values_not_the_layout() {
        let a = entries("x.ohlw", CABLES).unwrap();
        let reordered = CABLES.replace("# a comment\n", "").replace(
            "id = \"a\"\ndiameter = \"10 mm\"",
            "diameter = \"10 mm\"\nid = \"a\"",
        );
        let b = entries("x.ohlw", &reordered).unwrap();
        assert_eq!(a, b);
        let changed = CABLES.replace("r = 1.5", "r = 1.6");
        let c = entries("x.ohlw", &changed).unwrap();
        assert_ne!(a[0].version, c[0].version);
        assert_eq!(a[1].version, c[1].version);
    }

    #[test]
    fn uses_of_objects() {
        let s = serde_json::json!({"id": "S1", "cable": "a",
            "span_points": [{"object": {"type": "ball"}}, {"object": {"kind": "body"}}],
            "spreads": [{"type": "ball"}, {"type": "mat"}]});
        assert_eq!(
            uses_of(Kind::Section, &s),
            [
                ("cables", "a".to_owned()),
                ("objects", "ball".to_owned()),
                ("objects", "mat".to_owned())
            ]
        );
        let t = serde_json::json!({"id": "T1", "type": "type-01",
            "strings": [{"insulator": "i1"}, {"insulator": "i1"}, {}]});
        assert_eq!(
            uses_of(Kind::Structure, &t),
            [
                ("structures", "type-01".to_owned()),
                ("insulators", "i1".to_owned())
            ]
        );
        assert!(uses_of(Kind::Route, &t).is_empty());
    }

    #[test]
    fn drafts_are_named_by_their_request() {
        assert_eq!(draft_name(12, "acsr.ohlw"), "request-12.ohlw");
        assert_eq!(draft_of("request-12.ohlw"), Some(12));
        assert_eq!(draft_of("request-.ohlw"), None);
        assert_eq!(draft_of("requests.ohlw"), None);
        assert_eq!(draft_of("request-1x.ohlw"), None);
    }

    #[test]
    fn an_entry_laid_into_a_file() {
        let draft = "format = \"ohl-wire\"\n[[cable]]\nid = \"c\"\ndiameter = \"9 mm\"\n";
        let out = with_entry(Some(CABLES), draft, "c").unwrap();
        let e = entries("x.ohlw", &out).unwrap();
        assert_eq!(
            e.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        assert!(out.contains("# a comment"), "{out}");
        // A change replaces the entry of the same id.
        let change = "[[cable]]\nid = \"b\"\ndiameter = \"13 mm\"\n";
        let out = with_entry(Some(CABLES), change, "b").unwrap();
        let e = entries("x.ohlw", &out).unwrap();
        assert_eq!(e.len(), 2);
        assert!(out.contains("13 mm") && !out.contains("12 mm"), "{out}");
        // A new file: the draft itself; a single item: the draft.
        assert_eq!(with_entry(None, draft, "c").unwrap(), draft);
        let ohls = "id = \"T9\"\nname = \"T\"\n";
        assert_eq!(with_entry(Some("id = \"T8\""), ohls, "T9").unwrap(), ohls);
        assert!(with_entry(Some(CABLES), draft, "zz").is_err());
        // One entry alone, the header kept.
        let b = only_entry("x.ohlw", CABLES, "b").unwrap();
        let e = entries("x.ohlw", &b).unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0], entries("x.ohlw", CABLES).unwrap()[1]);
        assert!(b.contains("format = \"ohl-wire\""), "{b}");
        assert_eq!(only_entry("t.ohls", ohls, "T9").unwrap(), ohls);
    }
}
