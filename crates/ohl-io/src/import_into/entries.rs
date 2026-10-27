//! Library files compared by what they hold, where that is clear
//! (owner, 2026-10-27): wire files by cable id (every project-plugin
//! import names its wire file alike), structure type files by type id.
//! The text of an entry is kept as written; only its `id` line changes
//! when it is given another id.

use std::collections::BTreeMap;

/// A `[[cable]]` block of a wire file: its id, its data (to compare) and
/// its text.
#[derive(Debug, Clone)]
pub(crate) struct Block {
    pub id: String,
    pub data: toml::Table,
    pub text: String,
}

/// A wire file split into its head (before the first `[[cable]]`) and its
/// cable blocks; none when it does not read or holds no cable.
pub(crate) fn cable_blocks(text: &str) -> Option<(String, Vec<Block>)> {
    let mut head = String::new();
    let mut blocks: Vec<String> = Vec::new();
    for line in text.split_inclusive('\n') {
        if line.trim() == "[[cable]]" {
            blocks.push(String::new());
        }
        match blocks.last_mut() {
            Some(b) => b.push_str(line),
            None => head.push_str(line),
        }
    }
    let mut out = Vec::new();
    for b in blocks {
        let mut t: toml::Table = toml::from_str(&b).ok()?;
        let mut list = match t.remove("cable")? {
            toml::Value::Array(a) => a,
            _ => return None,
        };
        let Some(toml::Value::Table(data)) = list.pop() else {
            return None;
        };
        let id = data.get("id")?.as_str()?.to_owned();
        out.push(Block { id, data, text: b });
    }
    (!out.is_empty()).then_some((head, out))
}

/// `text` (a TOML entry) with its first `id = …` line before any table
/// header (after `[[cable]]` when it opens with one) giving `id`; none
/// when there is no such line.
pub(super) fn with_id(text: &str, id: &str) -> Option<String> {
    let mut out = String::new();
    let mut done = false;
    let mut first = true;
    for line in text.split_inclusive('\n') {
        let t = line.trim_start();
        let opening = first && t.trim() == "[[cable]]";
        first = false;
        if !done && !opening && t.starts_with('[') {
            done = true;
        }
        if !done
            && let Some(rest) = t.strip_prefix("id")
            && rest.trim_start().starts_with('=')
        {
            out.push_str(&format!("id = {}", toml::Value::String(id.to_owned())));
            if line.ends_with('\n') {
                out.push('\n');
            }
            done = true;
            continue;
        }
        out.push_str(line);
    }
    done.then_some(out).filter(|o| o != text)
}

/// The cables of wire files (name, text) by id: file name and block.
pub(super) fn cables_of(files: &[(String, String)]) -> BTreeMap<String, (String, Block)> {
    let mut out = BTreeMap::new();
    for (name, text) in files {
        if let Some((_, blocks)) = cable_blocks(text) {
            for b in blocks {
                out.entry(b.id.clone()).or_insert((name.clone(), b));
            }
        }
    }
    out
}

/// The top-level fields of two entries that differ (`id` left out).
pub(super) fn differences(a: &toml::Table, b: &toml::Table) -> Vec<String> {
    let keys: std::collections::BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    keys.into_iter()
        .filter(|k| *k != "id" && a.get(*k) != b.get(*k))
        .map(|k| k.replace('_', " "))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "format = \"ohl-wire\"\nversion = 2\n\n[[cable]]\nid = \"a\"\nmass = 1.0\n\
                        [cable.electrical]\nid = \"inner\"\n\n[[cable]]\nid = \"b\"\nmass = 2.0\n";

    /// Blocks split at `[[cable]]`, their subtables with them; an id
    /// changed on its own line only.
    #[test]
    fn blocks_and_ids() {
        let (head, blocks) = cable_blocks(FILE).unwrap();
        assert!(head.starts_with("format"));
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].id, "a");
        assert!(blocks[0].data.contains_key("electrical"));
        let t = with_id(&blocks[0].text, "a-2").unwrap();
        assert!(t.contains("id = \"a-2\"") && t.contains("id = \"inner\""));
        assert_eq!(cable_blocks(&t).unwrap().1[0].id, "a-2");
        assert_eq!(
            with_id("id = \"t\"\nname = \"x\"\n[a]\nid = 1\n", "u").unwrap(),
            "id = \"u\"\nname = \"x\"\n[a]\nid = 1\n"
        );
    }
}
