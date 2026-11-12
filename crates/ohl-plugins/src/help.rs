// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A plugin's own help pages (2026-11-12): the manifest's optional
//! `[help]` table names, per thing the plugin provides, a page shipped in
//! its folder (Markdown or HTML), and an overview of the whole plugin.
//!
//! ```toml
//! [help]
//! overview = "docs/index.md"            # the plugin as a whole
//! design-code = "docs/load-cases.md"    # its weather and load cases
//!
//! [help.check]                          # per check id (its `checks` list)
//! my-check = "docs/my-check.md"
//!
//! [help.preset]                         # per rule preset name
//! "roads" = "docs/roads.html"
//!
//! [help.column]                         # per [[column]] name
//! [help.sheet-template]                 # per sheet template name
//! [help.class-scheme]                   # per class scheme name
//! ```
//!
//! The applications open an HTML page as it is; a Markdown page is
//! rendered to a plain HTML page in a cache folder ([`viewable`]) — with
//! the other Markdown pages of its folder, its links to them made links to
//! theirs and its pictures read from the plugin's folder — and that is
//! opened. Nothing is fetched from the internet.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

/// The file extensions a help page may have.
pub const PAGE_EXTENSIONS: [&str; 4] = ["md", "markdown", "html", "htm"];

/// The manifest's `[help]` table: the plugin's own help pages, paths
/// inside its folder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct HelpPages {
    /// The plugin as a whole (the Plugin manager's *Help* button).
    #[serde(default)]
    pub overview: Option<PathBuf>,
    /// A design code's weather and load cases from its parameter file
    /// (Line › Criteria › Design code).
    #[serde(default)]
    pub design_code: Option<PathBuf>,
    /// Per check id (a check plugin's checks).
    #[serde(default)]
    pub check: BTreeMap<String, PathBuf>,
    /// Per rule preset name (a design code's presets).
    #[serde(default)]
    pub preset: BTreeMap<String, PathBuf>,
    /// Per table column name (`[[column]]`).
    #[serde(default)]
    pub column: BTreeMap<String, PathBuf>,
    /// Per sheet template name.
    #[serde(default)]
    pub sheet_template: BTreeMap<String, PathBuf>,
    /// Per LiDAR class scheme name.
    #[serde(default)]
    pub class_scheme: BTreeMap<String, PathBuf>,
}

/// What a help page is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTopic<'a> {
    /// The plugin as a whole.
    Overview,
    /// Its weather and load cases (a design code).
    DesignCode,
    /// One of its checks, by id.
    Check(&'a str),
    /// One of its rule presets, by name.
    Preset(&'a str),
    /// One of its table columns, by name.
    Column(&'a str),
    /// One of its sheet templates, by name.
    SheetTemplate(&'a str),
    /// One of its class schemes, by name.
    ClassScheme(&'a str),
}

impl HelpPages {
    /// Whether no page is declared.
    pub fn is_empty(&self) -> bool {
        self.pages().is_empty()
    }

    /// The page declared for `topic` (relative to the plugin's folder).
    pub fn page(&self, topic: HelpTopic<'_>) -> Option<&Path> {
        let p = match topic {
            HelpTopic::Overview => self.overview.as_ref(),
            HelpTopic::DesignCode => self.design_code.as_ref(),
            HelpTopic::Check(id) => self.check.get(id),
            HelpTopic::Preset(n) => self.preset.get(n),
            HelpTopic::Column(n) => self.column.get(n),
            HelpTopic::SheetTemplate(n) => self.sheet_template.get(n),
            HelpTopic::ClassScheme(n) => self.class_scheme.get(n),
        };
        p.map(PathBuf::as_path)
    }

    /// Every page declared, with what it is about in words (`check
    /// my-check`), the overview first.
    pub fn pages(&self) -> Vec<(String, &Path)> {
        let mut out: Vec<(String, &Path)> = Vec::new();
        if let Some(p) = &self.overview {
            out.push(("overview".into(), p));
        }
        if let Some(p) = &self.design_code {
            out.push(("design code".into(), p));
        }
        for (what, map) in [
            ("check", &self.check),
            ("preset", &self.preset),
            ("column", &self.column),
            ("sheet template", &self.sheet_template),
            ("class scheme", &self.class_scheme),
        ] {
            out.extend(
                map.iter()
                    .map(|(k, p)| (format!("{what} {k}"), p.as_path())),
            );
        }
        out
    }

    /// Why the table is not as it should be: a page outside the plugin's
    /// folder or of another kind than [`PAGE_EXTENSIONS`].
    pub fn check(&self) -> Result<(), String> {
        for (what, p) in self.pages() {
            let inside = p.components().all(|c| matches!(c, Component::Normal(_)));
            if !inside || p.as_os_str().is_empty() {
                return Err(format!(
                    "{what} {}: a file inside the plugin's folder",
                    p.display()
                ));
            }
            if page_kind(p).is_none() {
                return Err(format!(
                    "{what} {}: a page ending in .{}",
                    p.display(),
                    PAGE_EXTENSIONS.join(", .")
                ));
            }
        }
        Ok(())
    }

    /// The pages declared that are not in the plugin's folder `dir`
    /// (`ohl plugin check` reports them).
    pub fn missing(&self, dir: &Path) -> Vec<String> {
        self.pages()
            .into_iter()
            .filter(|(_, p)| !dir.join(p).is_file())
            .map(|(what, p)| format!("{what}: {} not found", p.display()))
            .collect()
    }
}

/// The pages the plugin in `dir` declares (none when its manifest does
/// not read): read once per change of its manifest (the windows ask
/// every frame).
pub fn pages_of(dir: &Path) -> HelpPages {
    type Cache = BTreeMap<PathBuf, (Option<std::time::SystemTime>, HelpPages)>;
    static CACHE: std::sync::Mutex<Cache> = std::sync::Mutex::new(BTreeMap::new());
    let stamp = std::fs::metadata(crate::manifest_path(dir))
        .and_then(|m| m.modified())
        .ok();
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((s, pages)) = cache.get(dir)
        && *s == stamp
    {
        return pages.clone();
    }
    let pages = crate::Manifest::load(dir)
        .map(|m| m.help)
        .unwrap_or_default();
    cache.insert(dir.to_owned(), (stamp, pages.clone()));
    pages
}

/// The page the plugin in `dir` declares for `topic`, when the file is
/// there (absolute).
pub fn page_file(dir: &Path, topic: HelpTopic<'_>) -> Option<PathBuf> {
    pages_of(dir)
        .page(topic)
        .map(|p| dir.join(p))
        .filter(|p| p.is_file())
}

/// The file to open for `topic` of the plugin in `dir` ([`viewable`],
/// rendered into [`default_cache`]), or why there is none.
pub fn file_to_open(dir: &Path, topic: HelpTopic<'_>) -> Result<PathBuf, String> {
    let pages = pages_of(dir);
    let page = pages
        .page(topic)
        .ok_or_else(|| format!("{}: no help page for this", dir.display()))?;
    viewable(dir, page, &default_cache())
}

/// Whether `path` is a Markdown page (`Some(true)`), an HTML page
/// (`Some(false)`) or neither.
fn page_kind(path: &Path) -> Option<bool> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "md" | "markdown" => Some(true),
        "html" | "htm" => Some(false),
        _ => None,
    }
}

/// The folder Markdown pages are rendered into when no other is given:
/// `ohl-plugin-help` in the system's temporary folder.
pub fn default_cache() -> PathBuf {
    std::env::temp_dir().join("ohl-plugin-help")
}

/// The file to show for page `page` (relative) of the plugin in `dir`:
/// an HTML page itself; a Markdown page rendered (with the other
/// Markdown pages of its folder) into `cache`, in a folder of its own per
/// plugin folder. An error when the page is missing or cannot be
/// written.
pub fn viewable(dir: &Path, page: &Path, cache: &Path) -> Result<PathBuf, String> {
    let file = dir.join(page);
    if !file.is_file() {
        return Err(format!("help page {} not found", file.display()));
    }
    match page_kind(page) {
        Some(false) => Ok(file),
        None => Err(format!("{}: not a Markdown or HTML page", page.display())),
        Some(true) => {
            let out_dir = cache.join(cache_name(dir));
            let folder = page.parent().unwrap_or(Path::new(""));
            let src = dir.join(folder);
            let entries = std::fs::read_dir(&src).map_err(|e| format!("{}: {e}", src.display()))?;
            for e in entries.flatten() {
                let p = e.path();
                if p.is_file() && page_kind(&p) == Some(true) {
                    let rel = folder.join(e.file_name());
                    let text =
                        std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                    let html = render(&text, &dir.join(folder), &title_of(&p));
                    let target = out_dir.join(rel.with_extension("html"));
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("{}: {e}", parent.display()))?;
                    }
                    std::fs::write(&target, html)
                        .map_err(|e| format!("{}: {e}", target.display()))?;
                }
            }
            Ok(out_dir.join(page.with_extension("html")))
        }
    }
}

/// The cache folder's name for the plugin folder `dir`: its name and a
/// hash of its path (two plugins of one name apart).
fn cache_name(dir: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    dir.hash(&mut h);
    let name = dir
        .file_name()
        .map_or("plugin".into(), |n| n.to_string_lossy().into_owned());
    format!("{name}-{:016x}", h.finish())
}

fn title_of(p: &Path) -> String {
    p.file_stem()
        .map_or(String::new(), |s| s.to_string_lossy().into_owned())
}

/// `path` as a `file://` URL (spaces and the URL's own signs escaped).
pub fn file_url(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from(if s.starts_with('/') {
        "file://"
    } else {
        "file:///"
    });
    for ch in s.chars() {
        match ch {
            ' ' => out.push_str("%20"),
            '%' => out.push_str("%25"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            c => out.push(c),
        }
    }
    out
}

/// Whether `url` is relative (no scheme, not absolute, not an anchor).
fn relative(url: &str) -> bool {
    !(url.is_empty() || url.starts_with('#') || url.starts_with('/') || url.contains(':'))
}

/// A link of a rendered page: a relative link to a Markdown page made a
/// link to its rendered page (beside it); another relative link (a
/// picture, an HTML page) made the file's URL in the plugin's folder
/// `src`.
fn rewrite(url: &str, src: &Path) -> String {
    if !relative(url) {
        return url.to_owned();
    }
    let (path, anchor) = match url.split_once('#') {
        Some((p, a)) => (p, Some(a)),
        None => (url, None),
    };
    if page_kind(Path::new(path)) == Some(true) {
        let mut out = Path::new(path)
            .with_extension("html")
            .to_string_lossy()
            .into_owned();
        if let Some(a) = anchor {
            out.push('#');
            out.push_str(a);
        }
        return out;
    }
    let mut out = file_url(&src.join(path));
    if let Some(a) = anchor {
        out.push('#');
        out.push_str(a);
    }
    out
}

/// A heading's anchor as the manual's book makes it: lower case, letters
/// and digits kept, spaces made `-`.
fn slug(text: &str) -> String {
    text.trim()
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                Some(c.to_lowercase().next().unwrap_or(c))
            } else if c.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

/// The Markdown `text` (read from the folder `src`) as a whole HTML
/// page titled by its first heading, else `fallback`.
pub fn render(text: &str, src: &Path, fallback: &str) -> String {
    use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd};
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_HEADING_ATTRIBUTES;
    let mut events: Vec<Event<'_>> = Parser::new_ext(text, options)
        .map(|e| match e {
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => Event::Start(Tag::Link {
                link_type,
                dest_url: CowStr::from(rewrite(&dest_url, src)),
                title,
                id,
            }),
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => Event::Start(Tag::Image {
                link_type,
                dest_url: CowStr::from(rewrite(&dest_url, src)),
                title,
                id,
            }),
            e => e,
        })
        .collect();
    // Headings get their anchors (the links between pages use them).
    let mut title = None;
    let mut k = 0;
    while k < events.len() {
        if let Event::Start(Tag::Heading { id: None, .. }) = &events[k] {
            let mut words = String::new();
            let mut j = k + 1;
            while j < events.len() && !matches!(events[j], Event::End(TagEnd::Heading(_))) {
                if let Event::Text(t) | Event::Code(t) = &events[j] {
                    words.push_str(t);
                }
                j += 1;
            }
            if title.is_none() {
                title = Some(words.clone());
            }
            if let Event::Start(Tag::Heading { id, .. }) = &mut events[k] {
                *id = Some(CowStr::from(slug(&words)));
            }
        }
        k += 1;
    }
    let mut body = String::new();
    pulldown_cmark::html::push_html(&mut body, events.into_iter());
    let title = title.unwrap_or_else(|| fallback.to_owned());
    let escaped = title
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{escaped}</title><style>{STYLE}</style></head>\n<body><main>\n{body}</main></body></html>\n"
    )
}

/// The rendered pages' style: readable, light and dark.
const STYLE: &str = "body{margin:0;font:16px/1.55 system-ui,-apple-system,'Segoe UI',sans-serif;\
background:#fff;color:#1d1d1f}main{max-width:52rem;margin:0 auto;padding:1.5rem 1rem 4rem}\
img{max-width:100%}table{border-collapse:collapse;margin:1rem 0}td,th{border:1px solid #ccc;\
padding:.3rem .6rem;vertical-align:top}th{background:#f3f3f3}code{background:#f3f3f3;\
padding:.1rem .25rem;border-radius:3px}pre{background:#f3f3f3;padding:.75rem;overflow:auto}\
pre code{padding:0}a{color:#0b62c4}blockquote{margin:1rem 0;padding:.25rem 1rem;\
border-left:3px solid #ccc;color:#555}@media (prefers-color-scheme: dark){body{\
background:#1e1e1e;color:#e6e6e6}th,code,pre{background:#2b2b2b}td,th{border-color:#444}\
a{color:#6cb2ff}blockquote{color:#aaa;border-color:#555}}";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_is_rendered_with_links_and_anchors() {
        let src = Path::new("/plugins/my plugin/docs");
        let html = render(
            "# My check\n\nSee [the other](other.md#step-2), ![a](fig.svg) and \
             [web](https://example.org).\n\n## Step 2\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
            src,
            "x",
        );
        assert!(html.contains("<title>My check</title>"), "{html}");
        assert!(html.contains("href=\"other.html#step-2\""), "{html}");
        assert!(
            html.contains("src=\"file:///plugins/my%20plugin/docs/fig.svg\""),
            "{html}"
        );
        assert!(html.contains("href=\"https://example.org\""), "{html}");
        assert!(html.contains("<h2 id=\"step-2\">"), "{html}");
        assert!(html.contains("<table>"), "{html}");
    }

    #[test]
    fn viewable_renders_markdown_and_keeps_html() {
        let root = std::env::temp_dir().join(format!("ohl-help-test-{}", std::process::id()));
        let dir = root.join("plugin");
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        std::fs::write(dir.join("docs/a.md"), "# A\n\n[b](b.md)\n").unwrap();
        std::fs::write(dir.join("docs/b.md"), "# B\n").unwrap();
        std::fs::write(dir.join("docs/c.html"), "<p>c</p>").unwrap();
        let cache = root.join("cache");
        let a = viewable(&dir, Path::new("docs/a.md"), &cache).unwrap();
        assert!(a.starts_with(&cache) && a.ends_with("docs/a.html"));
        assert!(
            std::fs::read_to_string(&a)
                .unwrap()
                .contains("href=\"b.html\"")
        );
        assert!(
            a.with_file_name("b.html").is_file(),
            "its folder's pages too"
        );
        assert_eq!(
            viewable(&dir, Path::new("docs/c.html"), &cache).unwrap(),
            dir.join("docs/c.html")
        );
        assert!(viewable(&dir, Path::new("docs/none.md"), &cache).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pages_are_checked() {
        let mut h = HelpPages::default();
        assert!(h.is_empty() && h.check().is_ok());
        h.check.insert("c".into(), "../x.md".into());
        assert!(h.check().is_err(), "outside the folder");
        h.check.insert("c".into(), "docs/x.pdf".into());
        assert!(h.check().is_err(), "not a page");
        h.check.insert("c".into(), "docs/x.md".into());
        assert!(h.check().is_ok());
        assert_eq!(h.page(HelpTopic::Check("c")), Some(Path::new("docs/x.md")));
        assert_eq!(h.page(HelpTopic::Check("d")), None);
        assert_eq!(h.missing(Path::new("/nowhere")).len(), 1);
    }
}
